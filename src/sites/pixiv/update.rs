use crate::core::auto_updater::AccountRotation;
use crate::core::storage::Store;
use anyhow::Result;
use chrono::Utc;
use reqwest::blocking::Client;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;

use super::download::download_user;
use super::fetch::{
    PixivAccountCandidate, build_client, build_client_for_account, is_authentication_error,
    is_authentication_failure_text, resolve_pixiv_account_candidates,
};

/// Update all tracked Pixiv users' works
pub fn pixiv_update(store: &mut Store, data_dir: &str) -> Result<String> {
    let img_path = PathBuf::from(data_dir).join("images");
    fs::create_dir_all(&img_path)?;

    let tracked_users = super::tracked_user_ids(store)?;
    if tracked_users.is_empty() {
        return Ok("pixiv update skipped; no tracked users in stored settings".to_string());
    }

    let candidates = resolve_pixiv_account_candidates(store);
    if candidates.is_empty() {
        return run_pixiv_update_with_client(
            store,
            &build_client(store)?,
            tracked_users,
            &img_path,
        );
    }

    let active_name = candidates.first().map(|candidate| candidate.name.as_str());
    let mut rotation = AccountRotation::new(
        active_name,
        candidates.iter().map(|candidate| candidate.name.clone()),
    );
    let mut last_auth_error = None;

    while let Some(candidate_name) = rotation.current().map(ToOwned::to_owned) {
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.name == candidate_name)
            .expect("candidate exists for current rotation entry");
        let client = build_client_for_account(Some(&candidate.account), Some(&candidate.source))?;

        match run_pixiv_update_with_client(store, &client, tracked_users.clone(), &img_path) {
            Ok(message) => {
                persist_active_candidate(store, candidate)?;
                return Ok(message);
            }
            Err(err) if is_authentication_error(&err) => {
                warn!(
                    account = candidate.name,
                    source = candidate.source,
                    error = %err,
                    "pixiv update failed with authentication error; trying next account"
                );
                last_auth_error = Some(err);
                if rotation.mark_failed().is_none() {
                    break;
                }
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_auth_error
        .unwrap_or_else(|| anyhow::anyhow!("pixiv update failed with every configured account")))
}

fn run_pixiv_update_with_client(
    store: &mut Store,
    client: &Client,
    tracked_users: Vec<String>,
    img_path: &Path,
) -> Result<String> {
    let mut updated_users = 0usize;
    let mut downloaded_works = 0usize;
    let mut failures = Vec::new();

    for user_id in tracked_users {
        match download_user(client, &user_id, img_path, store, true) {
            Ok(summary) => {
                if summary
                    .failures
                    .iter()
                    .any(|failure| is_authentication_failure_text(failure))
                {
                    return Err(anyhow::anyhow!(
                        "pixiv authentication failed: {}",
                        summary.failures.join(", ")
                    ));
                }
                updated_users += 1;
                downloaded_works += summary.downloaded_works;
                if !summary.failures.is_empty() {
                    failures.push(format!(
                        "user {}: {}",
                        summary.user_id,
                        summary.failures.join(", ")
                    ));
                }
            }
            Err(err) if is_authentication_error(&err) => return Err(err),
            Err(err) => failures.push(format!("user {user_id}: {err}")),
        }
    }

    if !failures.is_empty() {
        return Err(anyhow::anyhow!(
            "pixiv update refreshed {updated_users} tracked users and stored {downloaded_works} works, but some updates failed: {}",
            failures.join("; ")
        ));
    }

    Ok(format!(
        "pixiv update refreshed {updated_users} tracked users and stored {downloaded_works} works"
    ))
}

fn persist_active_candidate(store: &Store, candidate: &PixivAccountCandidate) -> Result<()> {
    let updated_at = Utc::now().to_rfc3339();
    let _ = store.set_active_account("pixiv", &candidate.name, &updated_at)?;
    Ok(())
}
