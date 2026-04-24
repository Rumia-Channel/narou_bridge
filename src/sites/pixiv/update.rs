use crate::core::renderer;
use crate::core::storage::Store;
use anyhow::Result;
use reqwest::blocking::Client;
use std::path::Path;
use std::{fs, path::PathBuf};

use super::download::download_user;
use super::fetch::build_client;

/// Update all tracked Pixiv users' works
pub fn pixiv_update(
    store: &mut Store,
    data_dir: &str,
    cookie_dir: &str,
    host_name: &str,
) -> Result<String> {
    let img_path = PathBuf::from(data_dir).join("images");
    fs::create_dir_all(&img_path)?;
    let folder_path = PathBuf::from(data_dir).join("pixiv");
    fs::create_dir_all(&folder_path)?;

    let tracked_users = super::tracked_user_ids(store, &folder_path)?;
    if tracked_users.is_empty() {
        renderer::render_site_from_store(store, "pixiv", data_dir, host_name)?;
        return Ok(
            "pixiv update rendered existing works; no tracked users in user.json".to_string(),
        );
    }

    let client = build_client(store, cookie_dir)?;
    let mut updated_users = 0usize;
    let mut downloaded_works = 0usize;
    let mut failures = Vec::new();

    for user_id in tracked_users {
        match download_user(&client, &user_id, &folder_path, &img_path, store, true) {
            Ok(summary) => {
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
            Err(err) => failures.push(format!("user {user_id}: {err}")),
        }
    }

    renderer::render_site_from_store(store, "pixiv", data_dir, host_name)?;

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
