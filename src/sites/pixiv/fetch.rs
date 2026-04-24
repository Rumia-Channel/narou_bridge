use crate::core::model::AccountFile;
use crate::core::storage::Store;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, COOKIE, HeaderMap, HeaderValue, REFERER, USER_AGENT};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:130.0) Gecko/20100101 Firefox/130.0";
const SLEEP_MS: u64 = 500;

/// Build an HTTP client with optional Pixiv account cookies
pub fn build_client(store: &Store, cookie_dir: &str) -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(DEFAULT_UA).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(REFERER, HeaderValue::from_static("https://www.pixiv.net/"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    if let Some((account, source)) = resolve_active_pixiv_account(store, cookie_dir) {
        let account_value = serde_json::to_value(&account).unwrap_or(Value::Null);
        let cookie_str = build_cookie_string(&account_value);
        if !cookie_str.is_empty() {
            info!("Loaded Pixiv cookies from {source}");
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
    let content = std::fs::read_to_string(&login_path).ok()?;
    let account = serde_json::from_str::<AccountFile>(&content).ok()?;
    Some((account, login_path.display().to_string()))
}

fn build_cookie_string(account: &Value) -> String {
    match account.get("cookies") {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("; "),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| {
                let name = item.get("name").and_then(Value::as_str)?;
                let value = item.get("value").and_then(Value::as_str)?;
                Some(format!("{name}={value}"))
            })
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
        .with_context(|| format!("request failed: {url}"))?
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
        return Err(anyhow::anyhow!("{message}: {url}"));
    }
    Ok(json.get("body").cloned().unwrap_or_default())
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
