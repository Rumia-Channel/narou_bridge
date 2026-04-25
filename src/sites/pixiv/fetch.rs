use crate::core::account::load_account_file;
use crate::core::model::AccountFile;
use crate::core::storage::Store;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, COOKIE, HeaderMap, HeaderValue, REFERER, USER_AGENT};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:130.0) Gecko/20100101 Firefox/130.0";
const SLEEP_MS: u64 = 500;

/// Build an HTTP client with optional Pixiv account cookies
pub fn build_client(store: &Store, cookie_dir: &str) -> Result<Client> {
    let account = resolve_active_pixiv_account(store, cookie_dir);
    build_client_for_account(
        account.as_ref().map(|(account, _)| account),
        account.as_ref().map(|(_, source)| source.as_str()),
    )
}

pub fn build_client_for_account(
    account: Option<&AccountFile>,
    source: Option<&str>,
) -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(DEFAULT_UA).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(REFERER, HeaderValue::from_static("https://www.pixiv.net/"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    if let Some(account) = account {
        let cookie_str = build_cookie_string(account);
        if !cookie_str.is_empty() {
            if let Some(source) = source {
                info!("Loaded Pixiv cookies from {source}");
            }
            if let Ok(val) = HeaderValue::from_str(&cookie_str) {
                headers.insert(COOKIE, val);
            }
        }
        if let Some(ua) = account.user_agent.as_deref().filter(|ua| !ua.is_empty()) {
            if let Ok(val) = HeaderValue::from_str(ua) {
                headers.insert(USER_AGENT, val);
            }
        }
    }

    Ok(Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()?)
}

#[derive(Debug, Clone)]
pub struct PixivAccountCandidate {
    pub name: String,
    pub account: AccountFile,
    pub source: String,
    pub backed_by_store: bool,
}

pub fn resolve_active_pixiv_account(
    store: &Store,
    cookie_dir: &str,
) -> Option<(AccountFile, String)> {
    if let Ok(Some(account)) = store.get_active_account("pixiv") {
        return Some((
            account.account,
            format!("sqlite accounts table (pixiv/{})", account.name),
        ));
    }

    let login_path = PathBuf::from(cookie_dir).join("pixiv").join("login.json");
    let account = load_account_file(&login_path).ok()?;
    Some((account, login_path.display().to_string()))
}

pub fn resolve_pixiv_account_candidates(
    store: &Store,
    cookie_dir: &str,
) -> Vec<PixivAccountCandidate> {
    let mut candidates = Vec::new();
    let mut seen_names = HashSet::new();

    if let Ok(accounts) = store.list_accounts("pixiv") {
        for account in accounts {
            seen_names.insert(account.name.clone());
            candidates.push(PixivAccountCandidate {
                name: account.name.clone(),
                account: account.account,
                source: format!("sqlite accounts table (pixiv/{})", account.name),
                backed_by_store: true,
            });
        }
    }
    let has_store_accounts = !candidates.is_empty();

    let site_dir = PathBuf::from(cookie_dir).join("pixiv");
    let mut file_paths = match fs::read_dir(&site_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect::<Vec<_>>(),
        Err(_) => return candidates,
    };
    file_paths.sort_by(|left, right| {
        let left_name = left
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let right_name = right
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if left_name == "login.json" {
            return std::cmp::Ordering::Less;
        }
        if right_name == "login.json" {
            return std::cmp::Ordering::Greater;
        }
        left_name.cmp(right_name)
    });

    for path in file_paths {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if has_store_accounts && file_name == "login.json" {
            continue;
        }
        let candidate_name = if file_name == "login.json" {
            "login".to_string()
        } else {
            path.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string()
        };
        if candidate_name.is_empty() || !seen_names.insert(candidate_name.clone()) {
            continue;
        }
        if let Ok(account) = load_account_file(&path) {
            candidates.push(PixivAccountCandidate {
                name: candidate_name,
                account,
                source: path.display().to_string(),
                backed_by_store: false,
            });
        }
    }

    candidates
}

fn build_cookie_string(account: &AccountFile) -> String {
    match &account.cookies {
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("; "),
        _ => String::new(),
    }
}

/// Fetch JSON body from a Pixiv API endpoint
pub fn fetch_body_json(client: &Client, url: &str) -> Result<Value> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("request failed: {url}"))?;
    if matches!(resp.status().as_u16(), 401 | 403) {
        return Err(anyhow::anyhow!(
            "pixiv authentication failed: HTTP {} for {url}",
            resp.status()
        ));
    }
    if resp.url().path().contains("login") {
        return Err(anyhow::anyhow!(
            "pixiv authentication failed: redirected to login for {url}"
        ));
    }
    let resp = resp
        .error_for_status()
        .with_context(|| format!("request returned error status: {url}"))?;
    let json: Value = resp
        .json()
        .with_context(|| format!("invalid json: {url}"))?;
    if json.get("error").and_then(Value::as_bool).unwrap_or(false) {
        let message = json
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("pixiv returned error");
        if is_authentication_failure_text(message) {
            return Err(anyhow::anyhow!(
                "pixiv authentication failed: {message}: {url}"
            ));
        }
        return Err(anyhow::anyhow!("{message}: {url}"));
    }
    Ok(json.get("body").cloned().unwrap_or_default())
}

pub fn is_authentication_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| is_authentication_failure_text(&cause.to_string()))
}

pub fn is_authentication_failure_text(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("pixiv authentication failed")
        || normalized.contains("redirected to login")
        || normalized.contains("http 401")
        || normalized.contains("http 403")
}

/// Download an image, returning raw bytes on success.
pub fn download_image(client: &Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.bytes().ok().map(|b| b.to_vec())
}

pub fn sleep() {
    std::thread::sleep(Duration::from_millis(SLEEP_MS));
}
