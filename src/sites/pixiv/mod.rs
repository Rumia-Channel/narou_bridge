use crate::core::model::{AccountFile, ImageRecord, WorkRecord};
use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};
use anyhow::{Context, Result, anyhow};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, COOKIE, HeaderMap, HeaderValue, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::{fs, thread, time::Duration};
use tracing::{info, warn};

pub mod convert;
pub mod download;
pub mod fetch;
pub mod repair;
pub mod update;

const VERSION: i64 = 0;
const USER_CONFIG_VERSION: i64 = 5;
const ENABLED_FLAG: &str = "enable";
const PIXIV_SITE_DOCUMENT_SCOPE: &str = "pixiv";
const TRACKED_USER_KEY_PREFIX: &str = "tracked_users/";
const TRACKED_USER_CONFIG_SUFFIX: &str = "/config";
const TRACKED_USER_ILLUST_IDS_SUFFIX: &str = "/illust_ids";
const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:130.0) Gecko/20100101 Firefox/130.0";
const SLEEP_MS: u64 = 500;

// ---------------------------------------------------------------------------
// Site trait implementation
// ---------------------------------------------------------------------------

pub struct PixivSite;

pub fn site() -> Box<dyn Site> {
    Box::new(PixivSite)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PixivUrlTarget {
    Novel(String),
    Series(String),
    User(String),
    Art(String),
    Comic(String),
}

#[derive(Debug, Default)]
struct UserDownloadSummary {
    user_id: String,
    user_name: Option<String>,
    downloaded_works: usize,
    tracked_artworks: usize,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PixivTrackedUserEntry {
    #[serde(default = "enabled_flag_string")]
    novel: String,
    #[serde(default = "enabled_flag_string")]
    comic: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    illust_ids_snapshot_hash: Option<String>,
    #[serde(flatten, default)]
    extra: BTreeMap<String, Value>,
}

impl Default for PixivTrackedUserEntry {
    fn default() -> Self {
        Self {
            novel: enabled_flag_string(),
            comic: enabled_flag_string(),
            name: None,
            illust_ids_snapshot_hash: None,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct PixivIllustSnapshotDocument {
    #[serde(default)]
    illust_ids: BTreeSet<String>,
}

impl Site for PixivSite {
    fn id(&self) -> SiteId {
        SiteId::Pixiv
    }

    fn name(&self) -> &'static str {
        "pixiv"
    }

    fn supported_actions(&self) -> &'static [&'static str] {
        &[
            "download",
            "login",
            "update",
            "convert",
            "repair",
            "re_download",
        ]
    }

    fn matches_url(&self, url: &str) -> bool {
        parse_pixiv_url(url).is_some()
    }

    fn execute(
        &self,
        action: &str,
        value: &str,
        context: &SiteActionContext,
        store: &mut Store,
    ) -> SiteActionResult {
        match action {
            "download" => {
                match pixiv_download(
                    value,
                    store,
                    &context.data_dir,
                    &context.cookie_dir,
                    &context.host_name,
                ) {
                    Ok(message) => SiteActionResult::success(self.id(), action, message),
                    Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
                }
            }
            "update" => match pixiv_update(
                store,
                &context.data_dir,
                &context.cookie_dir,
                &context.host_name,
            ) {
                Ok(message) => SiteActionResult::success(self.id(), action, message),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            "convert" | "repair" => match renderer::render_site_from_store(
                store,
                "pixiv",
                &context.data_dir,
                &context.host_name,
            ) {
                Ok(_) => SiteActionResult::success(
                    self.id(),
                    action,
                    format!("pixiv {action} completed"),
                ),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            "login" => {
                SiteActionResult::skipped(self.id(), action, "login moved to separate helper")
            }
            "re_download" => {
                SiteActionResult::skipped(self.id(), action, "pixiv redownload placeholder")
            }
            _ => SiteActionResult::skipped(self.id(), action, "unsupported"),
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level download dispatcher
// ---------------------------------------------------------------------------

fn pixiv_download(
    url: &str,
    store: &mut Store,
    data_dir: &str,
    cookie_dir: &str,
    host_name: &str,
) -> Result<String> {
    let img_path = PathBuf::from(data_dir).join("images");
    fs::create_dir_all(&img_path)?;
    let folder_path = PathBuf::from(data_dir).join("pixiv");
    fs::create_dir_all(&folder_path)?;

    let client = build_client(store, cookie_dir)?;
    let target = parse_pixiv_url(url).ok_or_else(|| anyhow!("unsupported pixiv url: {url}"))?;

    let (message, deferred_error) = match target {
        PixivUrlTarget::Novel(id) => {
            info!("Pixiv download: novel id={id}");
            let record = download_novel(&client, &id, &folder_path, &img_path)?;
            persist_work_record(store, &record, &img_path)?;
            (format!("stored {}", record.work_key), None)
        }
        PixivUrlTarget::Series(id) => {
            info!("Pixiv download: novel series id={id}");
            let record = download_series(&client, &id, &folder_path, &img_path)?;
            persist_work_record(store, &record, &img_path)?;
            (format!("stored {}", record.work_key), None)
        }
        PixivUrlTarget::Art(id) => {
            info!("Pixiv download: artwork id={id}");
            let record = download_art(&client, &id, &folder_path, &img_path)?;
            persist_work_record(store, &record, &img_path)?;
            (format!("stored {}", record.work_key), None)
        }
        PixivUrlTarget::Comic(id) => {
            info!("Pixiv download: comic series id={id}");
            let record = download_comic(&client, &id, &folder_path, &img_path)?;
            persist_work_record(store, &record, &img_path)?;
            (format!("stored {}", record.work_key), None)
        }
        PixivUrlTarget::User(user_id) => {
            info!("Pixiv download: user id={user_id}");
            let summary = download_user(&client, &user_id, &folder_path, &img_path, store, false)?;
            let user_name = summary.user_name.as_deref().unwrap_or(&summary.user_id);
            let deferred_error = (!summary.failures.is_empty()).then(|| {
                anyhow!(
                    "tracked pixiv user {user_name} ({}) with {} stored works, but some downloads failed: {}",
                    summary.user_id,
                    summary.downloaded_works,
                    summary.failures.join("; ")
                )
            });
            (
                format!(
                    "tracked pixiv user {} ({}) and stored {} works",
                    user_name, summary.user_id, summary.downloaded_works
                ),
                deferred_error,
            )
        }
    };

    renderer::render_site_from_store(store, "pixiv", data_dir, host_name)?;
    if let Some(err) = deferred_error {
        return Err(err);
    }
    Ok(message)
}

fn parse_pixiv_url(url: &str) -> Option<PixivUrlTarget> {
    let path = url.split('?').next().unwrap_or(url);
    if path.contains("/novel/show.php") {
        return query_value(url, "id").map(PixivUrlTarget::Novel);
    }
    if path.contains("/novel/series/") {
        return last_numeric_segment(path).map(PixivUrlTarget::Series);
    }
    if path.contains("/artworks/") {
        return last_numeric_segment(path).map(PixivUrlTarget::Art);
    }
    if path.contains("/user/") && path.contains("/series/") {
        return last_numeric_segment(path).map(PixivUrlTarget::Comic);
    }
    if let Some(user_id) = user_id_from_path(path, "users") {
        return Some(PixivUrlTarget::User(user_id));
    }
    if let Some(user_id) = user_id_from_path(path, "user") {
        return Some(PixivUrlTarget::User(user_id));
    }
    None
}

fn user_id_from_path(path: &str, marker: &str) -> Option<String> {
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    segments
        .windows(2)
        .find(|window| window[0] == marker && window[1].chars().all(|ch| ch.is_ascii_digit()))
        .map(|window| window[1].to_string())
}

fn pixiv_update(
    store: &mut Store,
    data_dir: &str,
    cookie_dir: &str,
    host_name: &str,
) -> Result<String> {
    let img_path = PathBuf::from(data_dir).join("images");
    fs::create_dir_all(&img_path)?;
    let folder_path = PathBuf::from(data_dir).join("pixiv");
    fs::create_dir_all(&folder_path)?;

    let tracked_users = tracked_user_ids(store, &folder_path)?;
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
        return Err(anyhow!(
            "pixiv update refreshed {updated_users} tracked users and stored {downloaded_works} works, but some updates failed: {}",
            failures.join("; ")
        ));
    }

    Ok(format!(
        "pixiv update refreshed {updated_users} tracked users and stored {downloaded_works} works"
    ))
}

fn download_user(
    client: &Client,
    user_id: &str,
    folder_path: &Path,
    img_path: &Path,
    store: &mut Store,
    update: bool,
) -> Result<UserDownloadSummary> {
    let user_conf = ensure_tracked_user(store, folder_path, user_id)?;
    let profile_all = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/user/{user_id}/profile/all"),
    )?;
    let user_body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/user/{user_id}"),
    )
    .ok();

    let user_name = user_body
        .as_ref()
        .and_then(|body| body.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string);

    let novel_series = extract_profile_ids(profile_all.get("novelSeries"));
    let comic_series = extract_profile_ids(profile_all.get("mangaSeries"));
    let mut novels = extract_object_keys(profile_all.get("novels"));
    let illusts = extract_object_keys(profile_all.get("illusts"));
    let mangas = extract_object_keys(profile_all.get("manga"));

    let mut series_novel_ids = HashSet::new();
    for series_id in &novel_series {
        match fetch_body_json(
            client,
            &format!("https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles"),
        ) {
            Ok(body) => {
                if let Some(entries) = body.as_array() {
                    for entry in entries {
                        if let Some(id) = entry
                            .get("id")
                            .map(value_to_string)
                            .filter(|id| !id.is_empty())
                        {
                            series_novel_ids.insert(id);
                        }
                    }
                }
            }
            Err(err) => warn!("Failed to inspect Pixiv novel series {series_id}: {err}"),
        }
    }
    novels.retain(|novel_id| !series_novel_ids.contains(novel_id));

    let mut series_art_ids = HashSet::new();
    for series_id in &comic_series {
        match fetch_comic_series_art_ids(client, series_id) {
            Ok(ids) => {
                for id in ids {
                    series_art_ids.insert(id);
                }
            }
            Err(err) => warn!("Failed to inspect Pixiv comic series {series_id}: {err}"),
        }
    }

    let mut summary = UserDownloadSummary {
        user_id: user_id.to_string(),
        user_name,
        ..UserDownloadSummary::default()
    };

    if action_enabled(&user_conf, "novel") {
        for series_id in &novel_series {
            match download_series(client, series_id, folder_path, img_path) {
                Ok(record) => {
                    persist_work_record(store, &record, img_path)?;
                    summary.downloaded_works += 1;
                }
                Err(err) => summary
                    .failures
                    .push(format!("novel series {series_id}: {err}")),
            }
        }
        for novel_id in &novels {
            match download_novel(client, novel_id, folder_path, img_path) {
                Ok(record) => {
                    persist_work_record(store, &record, img_path)?;
                    summary.downloaded_works += 1;
                }
                Err(err) => summary.failures.push(format!("novel {novel_id}: {err}")),
            }
        }
    }

    if action_enabled(&user_conf, "comic") {
        for series_id in &comic_series {
            match download_comic(client, series_id, folder_path, img_path) {
                Ok(record) => {
                    persist_work_record(store, &record, img_path)?;
                    summary.downloaded_works += 1;
                }
                Err(err) => summary
                    .failures
                    .push(format!("comic series {series_id}: {err}")),
            }
        }

        let mut target_art_ids = BTreeSet::new();
        for art_id in illusts.iter().chain(mangas.iter()) {
            if !series_art_ids.contains(art_id) {
                target_art_ids.insert(art_id.clone());
            }
        }

        let previous_snapshot = load_illust_snapshot(store, folder_path, user_id)?;
        let mut new_art_ids = Vec::new();
        for art_id in &target_art_ids {
            if !update || !previous_snapshot.contains(art_id) {
                new_art_ids.push(art_id.clone());
            }
        }

        for art_id in &new_art_ids {
            match download_art(client, art_id, folder_path, img_path) {
                Ok(record) => {
                    persist_work_record(store, &record, img_path)?;
                    summary.downloaded_works += 1;
                }
                Err(err) => summary.failures.push(format!("artwork {art_id}: {err}")),
            }
        }

        let hash = save_illust_snapshot(store, folder_path, user_id, &target_art_ids)?;
        summary.tracked_artworks = target_art_ids.len();
        update_tracked_user(store, folder_path, user_id, |entry| {
            entry.illust_ids_snapshot_hash = Some(hash.clone());
            if let Some(user_name) = summary.user_name.clone() {
                entry.name = Some(user_name);
            }
        })?;
    } else if summary.user_name.is_some() {
        update_tracked_user(store, folder_path, user_id, |entry| {
            if let Some(user_name) = summary.user_name.clone() {
                entry.name = Some(user_name);
            }
        })?;
    }

    Ok(summary)
}

fn persist_work_record(store: &Store, record: &WorkRecord, img_path: &Path) -> Result<()> {
    store.upsert_work(record)?;

    let cover_logical = format!("pixiv_{}_cover", record.work_key);
    if let Some(hash) = find_image_by_logical_prefix(img_path, &cover_logical) {
        let ext = hash.rsplit('.').next().unwrap_or("jpg").to_string();
        store.upsert_image(&ImageRecord {
            logical_name: format!("{cover_logical}.{ext}"),
            hash: hash.trim_end_matches(&format!(".{ext}")).to_string(),
            ext,
            kind: "cover".to_string(),
        })?;
    }

    Ok(())
}

fn user_config_path(folder_path: &Path) -> PathBuf {
    folder_path.join("user.json")
}

fn snapshot_path(folder_path: &Path, user_id: &str) -> PathBuf {
    folder_path
        .join("snapshots")
        .join("illust_ids")
        .join(format!("{user_id}.json"))
}

fn enabled_flag_string() -> String {
    ENABLED_FLAG.to_string()
}

fn tracked_user_config_key(user_id: &str) -> String {
    format!("{TRACKED_USER_KEY_PREFIX}{user_id}{TRACKED_USER_CONFIG_SUFFIX}")
}

fn tracked_user_snapshot_key(user_id: &str) -> String {
    format!("{TRACKED_USER_KEY_PREFIX}{user_id}{TRACKED_USER_ILLUST_IDS_SUFFIX}")
}

fn tracked_user_id_from_config_key(key: &str) -> Option<&str> {
    key.strip_prefix(TRACKED_USER_KEY_PREFIX)?
        .strip_suffix(TRACKED_USER_CONFIG_SUFFIX)
}

fn load_legacy_user_config_document(folder_path: &Path) -> Result<Map<String, Value>> {
    let path = user_config_path(folder_path);
    let mut document = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str::<Value>(&content)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    } else {
        Map::new()
    };
    document.insert("version".to_string(), json!(USER_CONFIG_VERSION));
    Ok(document)
}

fn save_user_config_document(folder_path: &Path, document: &Map<String, Value>) -> Result<()> {
    let path = user_config_path(folder_path);
    save_json_pretty(&path, &Value::Object(document.clone()))
}

fn tracked_user_entry_from_value(value: Value) -> PixivTrackedUserEntry {
    serde_json::from_value(value).unwrap_or_default()
}

fn list_tracked_user_entries(
    store: &Store,
    folder_path: &Path,
) -> Result<Vec<(String, PixivTrackedUserEntry)>> {
    let mut entries = store
        .list_site_document_records(PIXIV_SITE_DOCUMENT_SCOPE, Some(TRACKED_USER_KEY_PREFIX))?
        .into_iter()
        .filter_map(|record| {
            tracked_user_id_from_config_key(&record.key).map(|user_id| {
                (
                    user_id.to_string(),
                    tracked_user_entry_from_value(record.document),
                )
            })
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        import_legacy_tracked_user_documents(store, folder_path)?;
        entries = store
            .list_site_document_records(PIXIV_SITE_DOCUMENT_SCOPE, Some(TRACKED_USER_KEY_PREFIX))?
            .into_iter()
            .filter_map(|record| {
                tracked_user_id_from_config_key(&record.key).map(|user_id| {
                    (
                        user_id.to_string(),
                        tracked_user_entry_from_value(record.document),
                    )
                })
            })
            .collect::<Vec<_>>();
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries)
}

fn sync_tracked_user_config_mirror(store: &Store, folder_path: &Path) -> Result<()> {
    let mut document = Map::new();
    document.insert("version".to_string(), json!(USER_CONFIG_VERSION));
    for (user_id, entry) in list_tracked_user_entries(store, folder_path)? {
        document.insert(user_id, serde_json::to_value(entry)?);
    }
    save_user_config_document(folder_path, &document)
}

fn import_legacy_tracked_user_documents(store: &Store, folder_path: &Path) -> Result<usize> {
    let path = user_config_path(folder_path);
    if !path.exists() {
        return Ok(0);
    }

    let document = load_legacy_user_config_document(folder_path)?;
    let updated_at = file_modified_string(&path).unwrap_or_else(now_string);
    let mut imported = 0;
    for (user_id, value) in document {
        if user_id == "version" {
            continue;
        }
        let entry = tracked_user_entry_from_value(value);
        store.upsert_site_document(
            PIXIV_SITE_DOCUMENT_SCOPE,
            &tracked_user_config_key(&user_id),
            &entry,
            &updated_at,
        )?;
        imported += 1;
    }
    Ok(imported)
}

fn ensure_tracked_user(
    store: &Store,
    folder_path: &Path,
    user_id: &str,
) -> Result<PixivTrackedUserEntry> {
    let key = tracked_user_config_key(user_id);
    if let Some(record) = store.get_site_document_record(PIXIV_SITE_DOCUMENT_SCOPE, &key)? {
        return Ok(tracked_user_entry_from_value(record.document));
    }

    import_legacy_tracked_user_documents(store, folder_path)?;
    if let Some(record) = store.get_site_document_record(PIXIV_SITE_DOCUMENT_SCOPE, &key)? {
        return Ok(tracked_user_entry_from_value(record.document));
    }

    let entry = PixivTrackedUserEntry::default();
    store.upsert_site_document(PIXIV_SITE_DOCUMENT_SCOPE, &key, &entry, &now_string())?;
    sync_tracked_user_config_mirror(store, folder_path)?;
    Ok(entry)
}

fn update_tracked_user<F>(
    store: &Store,
    folder_path: &Path,
    user_id: &str,
    mut updater: F,
) -> Result<()>
where
    F: FnMut(&mut PixivTrackedUserEntry),
{
    let mut entry = ensure_tracked_user(store, folder_path, user_id)?;
    updater(&mut entry);
    store.upsert_site_document(
        PIXIV_SITE_DOCUMENT_SCOPE,
        &tracked_user_config_key(user_id),
        &entry,
        &now_string(),
    )?;
    sync_tracked_user_config_mirror(store, folder_path)
}

fn tracked_user_ids(store: &Store, folder_path: &Path) -> Result<Vec<String>> {
    let mut ids = list_tracked_user_entries(store, folder_path)?
        .into_iter()
        .map(|(key, _)| key)
        .collect::<Vec<_>>();
    ids.sort();
    Ok(ids)
}

fn action_enabled(entry: &PixivTrackedUserEntry, key: &str) -> bool {
    match key {
        "novel" => entry.novel == ENABLED_FLAG,
        "comic" => entry.comic == ENABLED_FLAG,
        _ => false,
    }
}

fn extract_profile_ids(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Object(map)) => map.keys().cloned().collect(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("id").map(value_to_string))
            .filter(|id| !id.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn extract_object_keys(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

fn fetch_comic_series_art_ids(client: &Client, comic_id: &str) -> Result<Vec<String>> {
    let mut page = 1;
    let mut ids = HashSet::new();
    loop {
        sleep();
        let body = fetch_body_json(
            client,
            &format!("https://www.pixiv.net/ajax/series/{comic_id}?p={page}&lang=ja"),
        )?;
        let entries = extract_comic_series_entries(&body);
        if entries.is_empty() {
            break;
        }
        for entry in entries {
            if let Some(id) = entry
                .get("workId")
                .or_else(|| entry.get("id"))
                .map(value_to_string)
                .filter(|id| !id.is_empty())
            {
                ids.insert(id);
            }
        }
        page += 1;
    }

    let mut sorted = ids.into_iter().collect::<Vec<_>>();
    sorted.sort();
    Ok(sorted)
}

fn load_legacy_illust_snapshot(folder_path: &Path, user_id: &str) -> Result<BTreeSet<String>> {
    let path = snapshot_path(folder_path, user_id);
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let content = fs::read_to_string(&path)?;
    let value = serde_json::from_str::<Value>(&content).unwrap_or(Value::Null);
    let snapshot = value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    Value::String(value) => Some(value.clone()),
                    Value::Number(value) => Some(value.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(snapshot)
}

fn load_illust_snapshot(
    store: &Store,
    folder_path: &Path,
    user_id: &str,
) -> Result<BTreeSet<String>> {
    if let Some(snapshot) = store.get_site_document::<PixivIllustSnapshotDocument>(
        PIXIV_SITE_DOCUMENT_SCOPE,
        &tracked_user_snapshot_key(user_id),
    )? {
        return Ok(snapshot.illust_ids);
    }

    let legacy_snapshot = load_legacy_illust_snapshot(folder_path, user_id)?;
    if legacy_snapshot.is_empty() {
        return Ok(legacy_snapshot);
    }

    let updated_at =
        file_modified_string(&snapshot_path(folder_path, user_id)).unwrap_or_else(now_string);
    store.upsert_site_document(
        PIXIV_SITE_DOCUMENT_SCOPE,
        &tracked_user_snapshot_key(user_id),
        &PixivIllustSnapshotDocument {
            illust_ids: legacy_snapshot.clone(),
        },
        &updated_at,
    )?;
    Ok(legacy_snapshot)
}

fn save_illust_snapshot(
    store: &Store,
    folder_path: &Path,
    user_id: &str,
    ids: &BTreeSet<String>,
) -> Result<String> {
    let path = snapshot_path(folder_path, user_id);
    let values = ids.iter().cloned().collect::<Vec<_>>();
    store.upsert_site_document(
        PIXIV_SITE_DOCUMENT_SCOPE,
        &tracked_user_snapshot_key(user_id),
        &PixivIllustSnapshotDocument {
            illust_ids: ids.clone(),
        },
        &now_string(),
    )?;
    save_json_pretty(&path, &json!(values))?;
    Ok(hash_ids(values.iter().map(String::as_str)))
}

fn hash_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> String {
    let mut sorted = ids.into_iter().map(ToString::to_string).collect::<Vec<_>>();
    sorted.sort();
    let joined = sorted.join(",");
    let mut hasher = Sha256::new();
    hasher.update(joined.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn save_json_pretty(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

fn file_modified_string(path: &Path) -> Option<String> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Utc> = modified.into();
    Some(dt.to_rfc3339())
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ---------------------------------------------------------------------------
// Client construction with optional cookie loading
// ---------------------------------------------------------------------------

fn build_client(store: &Store, cookie_dir: &str) -> Result<Client> {
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

fn resolve_active_pixiv_account(store: &Store, cookie_dir: &str) -> Option<(AccountFile, String)> {
    if let Ok(Some(account)) = store.get_active_account("pixiv") {
        return Some((
            account.account,
            format!("sqlite accounts table (pixiv/{})", account.name),
        ));
    }

    let login_path = PathBuf::from(cookie_dir).join("pixiv").join("login.json");
    let content = fs::read_to_string(&login_path).ok()?;
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

// ---------------------------------------------------------------------------
// download_novel  — 短編小説
// ---------------------------------------------------------------------------

fn download_novel(
    client: &Client,
    novel_id: &str,
    folder_path: &Path,
    img_path: &Path,
) -> Result<WorkRecord> {
    let body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/novel/{novel_id}"),
    )?;

    let work_key = format!("n{novel_id}");
    let novel_path = folder_path.join(&work_key);
    fs::create_dir_all(novel_path.join("raw"))?;

    // Text
    let raw_text = body
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let text = format_novel_text(&raw_text, novel_id, &body, client, img_path);

    // Cover
    download_cover(
        client,
        body.get("coverUrl").and_then(Value::as_str),
        img_path,
        &work_key,
    );

    // Tags
    let mut tags = extract_tag_names_nested(&body, &["tags", "tags"]);
    add_ai_tag_if_needed(body.get("aiType").and_then(Value::as_i64), &mut tags);

    // Poll
    let postscript = format_survey(find_key_recursively(&body, "pollData"));

    let caption = html_breaks_to_newlines(
        body.get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let title = str_field(&body, "title", novel_id);
    let author = str_field(&body, "userName", "pixiv");
    let author_id = body
        .get("userId")
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let author_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/users/{uid}"));
    let create_date = str_field(&body, "createDate", "");
    let upload_date = body
        .get("uploadDate")
        .and_then(Value::as_str)
        .unwrap_or(create_date);
    let char_count = int_field(&body, &["characterCount", "textLength", "wordCount"]);

    let episodes = {
        let mut map = Map::new();
        map.insert(
            "1".to_string(),
            json!({
                "id": novel_id,
                "chapter": null,
                "title": title,
                "textCount": char_count,
                "tags": tags,
                "introduction": url_decode(&caption),
                "text": text,
                "postscript": postscript,
                "createDate": create_date,
                "updateDate": upload_date,
            }),
        );
        map
    };

    Ok(build_work(
        &work_key,
        novel_id,
        title,
        author,
        author_id,
        author_url,
        &caption,
        "novel",
        "短編",
        &tags,
        &tags,
        create_date,
        upload_date,
        char_count,
        char_count,
        1,
        1,
        &episodes,
        &format!("https://www.pixiv.net/novel/show.php?id={novel_id}"),
    ))
}

// ---------------------------------------------------------------------------
// download_series  — シリーズ小説
// ---------------------------------------------------------------------------

fn download_series(
    client: &Client,
    series_id: &str,
    folder_path: &Path,
    img_path: &Path,
) -> Result<WorkRecord> {
    let body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/novel/series/{series_id}"),
    )?;

    let work_key = format!("s{series_id}");
    let series_path = folder_path.join(&work_key);
    fs::create_dir_all(series_path.join("raw"))?;

    // TOC (content_titles)
    let toc_body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles"),
    )?;
    let toc = toc_body.as_array().cloned().unwrap_or_default();

    // Cover
    let cover_url = body
        .get("cover")
        .and_then(|v| v.get("urls"))
        .and_then(|v| v.get("original"))
        .and_then(Value::as_str);
    download_cover(client, cover_url, img_path, &work_key);

    // Series-level tags
    let mut series_tags = extract_flat_tags(&body, "tags");
    add_ai_tag_if_needed(body.get("aiType").and_then(Value::as_i64), &mut series_tags);

    let mut all_tags: Vec<String> = series_tags.clone();
    let mut episodes = Map::new();
    let mut total_text: i64 = 0;

    // Fetch each episode
    for (idx, entry) in toc.iter().enumerate() {
        let available = entry
            .get("available")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if !available {
            continue;
        }

        let ep_id = entry.get("id").map(value_to_string).unwrap_or_default();
        if ep_id.is_empty() {
            continue;
        }

        let ep_title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        info!(
            "  Series {series_id}: fetching episode {}/{} id={ep_id} title={ep_title}",
            idx + 1,
            toc.len()
        );
        sleep();

        let ep_json =
            match fetch_body_json(client, &format!("https://www.pixiv.net/ajax/novel/{ep_id}")) {
                Ok(j) => j,
                Err(e) => {
                    warn!("  Failed to fetch episode {ep_id}: {e}");
                    continue;
                }
            };

        // Per-episode cover
        download_cover(
            client,
            ep_json.get("coverUrl").and_then(Value::as_str),
            img_path,
            &format!("pixiv_s{series_id}_{ep_id}"),
        );

        // Text
        let raw_text = ep_json.get("content").and_then(Value::as_str).unwrap_or("");
        let text = format_novel_text(raw_text, &ep_id, &ep_json, client, img_path);

        // Tags
        let mut ep_tags = extract_tag_names_nested(&ep_json, &["tags", "tags"]);
        add_ai_tag_if_needed(ep_json.get("aiType").and_then(Value::as_i64), &mut ep_tags);
        all_tags.extend(ep_tags.iter().cloned());

        // Poll
        let postscript = format_survey(find_key_recursively(&ep_json, "pollData"));

        let t_count = ep_json
            .get("characterCount")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        total_text += t_count;

        let ep_create = str_field(&ep_json, "createDate", "");
        let ep_update = ep_json
            .get("uploadDate")
            .and_then(Value::as_str)
            .unwrap_or(ep_create);
        let ep_caption = html_breaks_to_newlines(
            ep_json
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(""),
        );

        let ep_key = (idx + 1).to_string();
        episodes.insert(
            ep_key,
            json!({
                "id": ep_id,
                "chapter": null,
                "title": ep_title,
                "textCount": t_count,
                "tags": ep_tags,
                "introduction": url_decode(&ep_caption),
                "text": text,
                "postscript": postscript,
                "createDate": ep_create,
                "updateDate": ep_update,
            }),
        );
    }

    // Sort episodes by createDate, re-key
    let episodes = sort_episodes_by_create_date(episodes);

    // Deduplicate all_tags
    dedup_tags(&mut all_tags);

    let title = str_field(&body, "title", series_id);
    let author = str_field(&body, "userName", "pixiv");
    let author_id = body
        .get("userId")
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let author_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/users/{uid}"));
    let caption =
        html_breaks_to_newlines(body.get("caption").and_then(Value::as_str).unwrap_or(""));
    let create_date = str_field(&body, "createDate", "");
    let update_date = body
        .get("updateDate")
        .and_then(Value::as_str)
        .unwrap_or(create_date);
    let all_episodes = body
        .get("total")
        .and_then(Value::as_i64)
        .unwrap_or(episodes.len() as i64);
    let all_characters = body
        .get("publishedTotalCharacterCount")
        .and_then(Value::as_i64)
        .unwrap_or(total_text);
    let serialization = if body
        .get("isConcluded")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "完結済"
    } else {
        "連載中"
    };

    Ok(build_work(
        &work_key,
        series_id,
        title,
        author,
        author_id,
        author_url,
        &caption,
        "novel",
        serialization,
        &series_tags,
        &all_tags,
        create_date,
        update_date,
        total_text,
        all_characters,
        episodes.len() as i64,
        all_episodes,
        &episodes,
        &format!("https://www.pixiv.net/novel/series/{series_id}"),
    ))
}

// ---------------------------------------------------------------------------
// download_art  — 短編漫画/イラスト
// ---------------------------------------------------------------------------

fn download_art(
    client: &Client,
    art_id: &str,
    folder_path: &Path,
    img_path: &Path,
) -> Result<WorkRecord> {
    let body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/illust/{art_id}"),
    )?;

    let work_key = format!("a{art_id}");
    let art_path = folder_path.join(&work_key);
    fs::create_dir_all(art_path.join("raw"))?;

    // Pages
    let pages_body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/illust/{art_id}/pages"),
    )?;
    let pages = pages_body.as_array().cloned().unwrap_or_default();

    // Build text from pages
    let mut art_text = String::new();
    for page in &pages {
        let url = page
            .get("urls")
            .and_then(|v| v.get("original"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if url.is_empty() {
            continue;
        }
        // Extract page number from _p0, _p1, etc.
        if let Some(caps) = regex_capture(r"_p(\d+)\.", url) {
            let num: i32 = caps.parse().unwrap_or(0) + 1;
            art_text.push_str(&format!("[pixivimage:{art_id}-{num}]\n"));
        }
    }

    // Download images referenced in text
    art_text = format_image_links(&art_text, art_id, &body, client, img_path);

    // Cover
    let cover_url = body
        .get("urls")
        .and_then(|v| v.get("original"))
        .and_then(Value::as_str);
    download_cover(client, cover_url, img_path, &work_key);

    // Tags
    let mut tags = extract_tag_names_nested(&body, &["tags", "tags"]);
    add_ai_tag_if_needed(body.get("aiType").and_then(Value::as_i64), &mut tags);

    // Poll
    let postscript = format_survey(find_key_recursively(&body, "pollData"));

    let caption = html_breaks_to_newlines(
        body.get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let title = str_field(&body, "title", art_id);
    let author = str_field(&body, "userName", "pixiv");
    let author_id = body
        .get("userId")
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let author_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/users/{uid}"));
    let create_date = str_field(&body, "createDate", "");
    let upload_date = body
        .get("uploadDate")
        .and_then(Value::as_str)
        .unwrap_or(create_date);

    let episodes = {
        let mut map = Map::new();
        map.insert(
            "1".to_string(),
            json!({
                "id": art_id,
                "chapter": null,
                "title": title,
                "textCount": 0,
                "tags": tags,
                "introduction": url_decode(&caption),
                "text": art_text,
                "postscript": postscript,
                "createDate": create_date,
                "updateDate": upload_date,
            }),
        );
        map
    };

    Ok(build_work(
        &work_key,
        art_id,
        title,
        author,
        author_id,
        author_url,
        &caption,
        "comic",
        "短編",
        &tags,
        &tags,
        create_date,
        upload_date,
        0,
        0,
        1,
        1,
        &episodes,
        &format!("https://www.pixiv.net/artworks/{art_id}"),
    ))
}

// ---------------------------------------------------------------------------
// download_comic  — 漫画シリーズ
// ---------------------------------------------------------------------------

fn download_comic(
    client: &Client,
    comic_id: &str,
    folder_path: &Path,
    img_path: &Path,
) -> Result<WorkRecord> {
    let work_key = format!("c{comic_id}");
    let comic_dir = folder_path.join(&work_key);
    fs::create_dir_all(comic_dir.join("raw"))?;

    // Collect all artwork IDs across pages
    let mut arts: BTreeMap<i64, String> = BTreeMap::new();
    let mut page = 1;
    let mut series_info: Value = Value::Null;
    let mut meta_body: Value = Value::Null;
    let mut user_info: Value = Value::Null;
    let mut first_tags: Vec<String> = Vec::new();

    loop {
        sleep();
        let page_url = format!("https://www.pixiv.net/ajax/series/{comic_id}?p={page}&lang=ja");
        let resp = match fetch_body_json(client, &page_url) {
            Ok(b) => b,
            Err(e) => {
                if page == 1 {
                    return Err(e);
                }
                break;
            }
        };

        if page == 1 {
            // Extract series metadata from first page
            series_info = resp
                .get("illustSeries")
                .and_then(Value::as_array)
                .and_then(|arr| {
                    arr.iter()
                        .find(|s| s.get("id").map(value_to_string).as_deref() == Some(comic_id))
                })
                .cloned()
                .or_else(|| {
                    resp.get("illustSeries")
                        .and_then(Value::as_array)
                        .and_then(|arr| arr.first())
                        .cloned()
                })
                .unwrap_or_default();

            user_info = resp
                .get("users")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .cloned()
                .unwrap_or_default();

            meta_body = resp.clone();

            // Tags from tagTranslation or thumbnails
            first_tags = extract_tag_names_from_translation(resp.get("tagTranslation"));
            if first_tags.is_empty() {
                if let Some(thumb) = resp
                    .get("thumbnails")
                    .and_then(|v| v.get("illust"))
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                {
                    first_tags = thumb
                        .get("tags")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                }
            }
        }

        // Extract series entries from this page
        let series_data = extract_comic_series_entries(&resp);
        if series_data.is_empty() {
            break;
        }
        for item in &series_data {
            let order = item.get("order").and_then(Value::as_i64).unwrap_or(0);
            let work_id = item
                .get("workId")
                .or_else(|| item.get("id"))
                .map(value_to_string)
                .unwrap_or_default();
            if !work_id.is_empty() {
                arts.insert(order, work_id);
            }
        }

        page += 1;
    }

    if arts.is_empty() {
        return Err(anyhow!("no artworks found in comic series {comic_id}"));
    }

    // Series cover
    download_cover(
        client,
        series_info.get("url").and_then(Value::as_str),
        img_path,
        &work_key,
    );

    // Author from meta or user
    let title = series_info
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(comic_id);
    let author = user_info
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Unknown");
    let author_id = user_info
        .get("userId")
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let author_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/users/{uid}"));

    let mut episodes = Map::new();
    let mut all_tags: Vec<String> = first_tags.clone();

    // Fetch each artwork
    for (order, illust_id) in &arts {
        info!("  Comic {comic_id}: fetching artwork order={order} id={illust_id}");
        sleep();

        let ep_data = match fetch_body_json(
            client,
            &format!("https://www.pixiv.net/ajax/illust/{illust_id}"),
        ) {
            Ok(j) => j,
            Err(e) => {
                warn!("  Failed to fetch illust {illust_id}: {e}");
                continue;
            }
        };

        // Pages
        let pages_res = fetch_body_json(
            client,
            &format!("https://www.pixiv.net/ajax/illust/{illust_id}/pages"),
        );
        let mut text = String::new();
        if let Ok(pages_body) = &pages_res {
            if let Some(pages) = pages_body.as_array() {
                for p in pages {
                    let u = p
                        .get("urls")
                        .and_then(|v| v.get("original"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if let Some(caps) = regex_capture(r"_p(\d+)\.", u) {
                        let num: i32 = caps.parse().unwrap_or(0) + 1;
                        text.push_str(&format!("[pixivimage:{illust_id}-{num}]\n"));
                    }
                }
            }
        }

        // Download images
        text = format_image_links(&text, illust_id, &ep_data, client, img_path);

        // Episode cover
        let ep_cover_url = ep_data
            .get("urls")
            .and_then(|v| v.get("original"))
            .and_then(Value::as_str);
        download_cover(
            client,
            ep_cover_url,
            img_path,
            &format!("pixiv_c{comic_id}_{illust_id}"),
        );

        // Tags
        let mut ep_tags = extract_tag_names_nested(&ep_data, &["tags", "tags"]);
        add_ai_tag_if_needed(ep_data.get("aiType").and_then(Value::as_i64), &mut ep_tags);
        all_tags.extend(ep_tags.iter().cloned());

        // Poll
        let postscript = format_survey(find_key_recursively(&ep_data, "pollData"));

        let ep_caption = html_breaks_to_newlines(
            ep_data
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(""),
        );
        let ep_create = str_field(&ep_data, "createDate", "");
        let ep_update = ep_data
            .get("uploadDate")
            .and_then(Value::as_str)
            .unwrap_or(ep_create);

        episodes.insert(
            order.to_string(),
            json!({
                "id": illust_id,
                "chapter": null,
                "title": str_field(&ep_data, "title", ""),
                "textCount": 0,
                "tags": ep_tags,
                "introduction": url_decode(&ep_caption),
                "text": text,
                "postscript": postscript,
                "createDate": ep_create,
                "updateDate": ep_update,
            }),
        );
    }

    // Sort episodes by createDate, re-key
    let episodes = sort_episodes_by_create_date(episodes);
    dedup_tags(&mut all_tags);

    let caption = html_breaks_to_newlines(
        meta_body
            .get("extraData")
            .and_then(|v| v.get("meta"))
            .and_then(|v| v.get("description"))
            .and_then(Value::as_str)
            .or_else(|| series_info.get("caption").and_then(Value::as_str))
            .unwrap_or(""),
    );
    let create_date = series_info
        .get("createDate")
        .and_then(Value::as_str)
        .unwrap_or("");
    let update_date = series_info
        .get("updateDate")
        .and_then(Value::as_str)
        .unwrap_or(create_date);

    let canon_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/user/{uid}/series/{comic_id}"))
        .unwrap_or_else(|| format!("https://www.pixiv.net/user/0/series/{comic_id}"));

    Ok(build_work(
        &work_key,
        comic_id,
        title,
        author,
        author_id,
        author_url,
        &caption,
        "comic",
        "連載中",
        &first_tags,
        &all_tags,
        create_date,
        update_date,
        0,
        0,
        episodes.len() as i64,
        episodes.len() as i64,
        &episodes,
        &canon_url,
    ))
}

// ---------------------------------------------------------------------------
// Text formatting (Python _format_novel_text equivalent)
// ---------------------------------------------------------------------------

fn format_novel_text(
    text: &str,
    novel_id: &str,
    json_data: &Value,
    client: &Client,
    img_path: &Path,
) -> String {
    let mut text = text.replace("\r\n", "\n");
    text = format_image_links(&text, novel_id, json_data, client, img_path);
    text = format_ruby(&text);
    text = remove_chapter_tag(&text);
    text = format_jumpuri(&text);
    text = format_jump_url(&text);
    text
}

/// [[rb:漢字 > かな]] → [ruby:<漢字>(かな)]
fn format_ruby(text: &str) -> String {
    let re = Regex::new(r"\[\[rb:(.*?)\s*>\s*(.*?)\]\]").unwrap();
    re.replace_all(text, "[ruby:<$1>($2)]").to_string()
}

/// [chapter:...] → content with newlines
fn remove_chapter_tag(text: &str) -> String {
    let re = Regex::new(r"(?s)(.*?)(\[chapter:(.*?)\])").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let before = &caps[1];
        let content = caps[3].replace('\n', "");
        if !before.is_empty() && !before.ends_with('\n') {
            format!("{before}\n{content}\n\n\n")
        } else {
            format!("{before}{content}\n\n\n")
        }
    })
    .to_string()
}

/// [[jumpuri:text > url]] → <a href="url">text</a>
fn format_jumpuri(text: &str) -> String {
    let re = Regex::new(r"(?s)\[\[jumpuri:(.*?) > (.*?)\]\]").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let txt = &caps[1];
        let mut url = caps[2].to_string();
        if url.starts_with("/http://") || url.starts_with("/https://") {
            url = url[1..].to_string();
        }
        format!("<a href=\"{url}\">{txt}</a>")
    })
    .to_string()
}

/// "/jump.php?..." → decoded URL
fn format_jump_url(text: &str) -> String {
    let re = Regex::new(r#""/jump\.php\?([^"]+)""#).unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let decoded = url_decode(&caps[1]);
        format!("\"{decoded}\"")
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Image link processing (Python _format_image_links equivalent)
// ---------------------------------------------------------------------------

fn format_image_links(
    text: &str,
    content_id: &str,
    json_data: &Value,
    client: &Client,
    img_path: &Path,
) -> String {
    let mut text = text.to_string();

    // Fix bare [pixivimage:123] to [pixivimage:123-1]
    let re_bare = Regex::new(r"\[pixivimage:(\d+)\]").unwrap();
    text = re_bare.replace_all(&text, "[pixivimage:$1-1]").to_string();

    // Collect all [pixivimage:art_id-page_num]
    let re_pixiv = Regex::new(r"\[pixivimage:(\d+)-(\d+)\]").unwrap();
    let mut link_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for caps in re_pixiv.captures_iter(&text) {
        let art_id = caps[1].to_string();
        let p_num = caps[2].to_string();
        link_map.entry(art_id).or_default().push(p_num);
    }

    // Download pixivimage references
    for (art_id, pages_needed) in &link_map {
        sleep();
        let pages_url = format!("https://www.pixiv.net/ajax/illust/{art_id}/pages");
        let pages_body = match fetch_body_json(client, &pages_url) {
            Ok(b) => b,
            Err(e) => {
                warn!("Image pages fetch error art={art_id}: {e}");
                continue;
            }
        };
        let pages = pages_body.as_array().cloned().unwrap_or_default();

        for (idx, page) in pages.iter().enumerate() {
            let p_num_str = (idx + 1).to_string();
            if !pages_needed.contains(&p_num_str) {
                continue;
            }

            let url = page
                .get("urls")
                .and_then(|v| v.get("original"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if url.is_empty() {
                continue;
            }

            let ext = url_ext(url);
            let img_name = format!("pixiv_{art_id}_p{idx}{ext}");

            // Check if already downloaded
            let saved = check_image_file(img_path, &img_name);
            let saved = if saved.is_none() {
                // Download
                match download_image(client, url) {
                    Some(data) => {
                        let hash = save_image_hashed(img_path, &data, &img_name);
                        Some(format!("{hash}{ext}"))
                    }
                    None => None,
                }
            } else {
                saved
            };

            if let Some(saved_file) = saved {
                text = text.replace(
                    &format!("[pixivimage:{art_id}-{p_num_str}]"),
                    &format!("[image]({saved_file})"),
                );
            }
        }
    }

    // Collect all [uploadedimage:id]
    let re_uploaded = Regex::new(r"\[uploadedimage:(\d+)\]").unwrap();
    let inner_links: Vec<String> = re_uploaded
        .captures_iter(&text)
        .map(|caps| caps[1].to_string())
        .collect();

    for inner_id in &inner_links {
        let upload_info = find_key_recursively(json_data, inner_id);
        let url = upload_info
            .as_ref()
            .and_then(|v| v.get("urls"))
            .and_then(|v| v.get("original"))
            .and_then(Value::as_str);

        if let Some(url) = url {
            let ext = url_ext(url);
            let img_name = format!("pixiv_{content_id}_{inner_id}{ext}");

            sleep();
            let saved = check_image_file(img_path, &img_name);
            let saved = if saved.is_none() {
                match download_image(client, url) {
                    Some(data) => {
                        let hash = save_image_hashed(img_path, &data, &img_name);
                        Some(format!("{hash}{ext}"))
                    }
                    None => None,
                }
            } else {
                saved
            };

            if let Some(saved_file) = saved {
                text = text.replace(
                    &format!("[uploadedimage:{inner_id}]"),
                    &format!("[image]({saved_file})"),
                );
            }
        }
    }

    text
}

// ---------------------------------------------------------------------------
// Cover image download (Python get_cover equivalent)
// ---------------------------------------------------------------------------

fn download_cover(client: &Client, url: Option<&str>, img_path: &Path, ncode: &str) {
    let url = match url {
        Some(u) if !u.is_empty() => u,
        _ => return,
    };

    let ext = url_ext(url);
    let logical_name = format!("pixiv_{ncode}_cover{ext}");

    // Already downloaded?
    if check_image_file(img_path, &logical_name).is_some() {
        return;
    }

    // Build candidate URLs (try original resolution variants)
    let base = url
        .replace("c/600x600/novel-cover-master", "novel-cover-original")
        .replace("_master1200", "");
    let candidates: Vec<String> = [".png", ".jpg", ".jpeg", ".gif"]
        .iter()
        .map(|e| {
            let mut c = base.clone();
            // Replace existing extension
            if let Some(dot_pos) = c.rfind('.') {
                c.truncate(dot_pos);
            }
            format!("{c}{e}")
        })
        .chain(std::iter::once(url.to_string()))
        .collect();

    for cand in &candidates {
        match download_image(client, cand) {
            Some(data) => {
                let cand_ext = url_ext(cand);
                let cover_name = format!("pixiv_{ncode}_cover{cand_ext}");
                let hash = save_image_hashed(img_path, &data, &cover_name);
                // Also write to cover.json
                update_cover_json(img_path, &cover_name, &hash);
                info!("  Cover saved: {hash}{cand_ext}");
                return;
            }
            None => continue,
        }
    }

    warn!("  Failed to download cover for {ncode}");
}

// ---------------------------------------------------------------------------
// Image filesystem helpers
// ---------------------------------------------------------------------------

/// Check if an image with this logical name was already saved.
/// Returns the hashed filename if found.
fn check_image_file(img_path: &Path, logical_name: &str) -> Option<String> {
    let db_path = img_path.join("database.json");
    if !db_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&db_path).ok()?;
    let db: Value = serde_json::from_str(&content).ok()?;
    let hash = db.get(logical_name)?.as_str()?;
    let ext = logical_name.rsplit('.').next().unwrap_or("jpg");
    let full = format!("{hash}.{ext}");
    if img_path.join(&full).exists() {
        Some(full)
    } else {
        None
    }
}

/// Download an image, returning raw bytes on success.
fn download_image(client: &Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.bytes().ok().map(|b| b.to_vec())
}

/// Save image with content hash, update database.json.
/// Returns the hash (without extension).
fn save_image_hashed(img_path: &Path, data: &[u8], logical_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let hash = result
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let hash_short = &hash[..16]; // Use first 16 hex chars

    let ext = logical_name.rsplit('.').next().unwrap_or("jpg");
    let file_name = format!("{hash_short}.{ext}");
    let file_path = img_path.join(&file_name);

    if let Err(e) = fs::write(&file_path, data) {
        warn!("Failed to save image {}: {e}", file_path.display());
    }

    // Update database.json
    let db_path = img_path.join("database.json");
    let mut db: Value = if db_path.exists() {
        fs::read_to_string(&db_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };
    if let Some(map) = db.as_object_mut() {
        map.insert(logical_name.to_string(), json!(hash_short));
    }
    let _ = fs::write(
        &db_path,
        serde_json::to_string_pretty(&db).unwrap_or_default(),
    );

    hash_short.to_string()
}

/// Update cover.json with cover mapping
fn update_cover_json(img_path: &Path, logical_name: &str, hash: &str) {
    let cover_path = img_path.join("cover.json");
    let mut cover: Value = if cover_path.exists() {
        fs::read_to_string(&cover_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };
    if let Some(map) = cover.as_object_mut() {
        map.insert(logical_name.to_string(), json!(hash));
    }
    let _ = fs::write(
        &cover_path,
        serde_json::to_string_pretty(&cover).unwrap_or_default(),
    );
}

/// Find an image by logical name prefix in database.json
fn find_image_by_logical_prefix(img_path: &Path, prefix: &str) -> Option<String> {
    let db_path = img_path.join("database.json");
    if !db_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&db_path).ok()?;
    let db: Value = serde_json::from_str(&content).ok()?;
    let map = db.as_object()?;
    for (key, hash) in map {
        if key.starts_with(prefix) {
            let ext = key.rsplit('.').next().unwrap_or("jpg");
            return Some(format!("{}.{ext}", hash.as_str().unwrap_or("")));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tag helpers
// ---------------------------------------------------------------------------

fn extract_tag_names_nested(body: &Value, path: &[&str]) -> Vec<String> {
    let mut current = body;
    for key in path {
        current = match current.get(*key) {
            Some(value) => value,
            None => return Vec::new(),
        };
    }
    let raw: Vec<String> = current
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|tag| {
                    tag.get("tag")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    format_tags(raw)
}

fn extract_flat_tags(body: &Value, key: &str) -> Vec<String> {
    body.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .map(format_tags)
        .unwrap_or_default()
}

fn extract_tag_names_from_translation(tag_data: Option<&Value>) -> Vec<String> {
    match tag_data {
        Some(Value::Object(map)) => format_tags(map.keys().cloned().collect()),
        Some(Value::Array(arr)) => format_tags(
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect(),
        ),
        _ => Vec::new(),
    }
}

/// Deduplicate and clean tags (Python format_tags equivalent)
fn format_tags(tags: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for t in tags {
        if t.is_empty() {
            continue;
        }
        for part in t.split('#') {
            let tag = part.trim().trim_start_matches('#').to_string();
            if !tag.is_empty() && seen.insert(tag.clone()) {
                result.push(tag);
            }
        }
    }
    result
}

fn add_ai_tag_if_needed(ai_type: Option<i64>, tags: &mut Vec<String>) {
    if ai_type == Some(2) && !tags.contains(&"AI生成".to_string()) {
        tags.push("AI生成".to_string());
    }
}

fn dedup_tags(tags: &mut Vec<String>) {
    let mut seen = HashSet::new();
    tags.retain(|t| seen.insert(t.clone()));
}

// ---------------------------------------------------------------------------
// Survey/poll formatting
// ---------------------------------------------------------------------------

fn format_survey(survey: Option<Value>) -> String {
    let survey = match survey {
        Some(v) if v.is_object() => v,
        _ => return String::new(),
    };
    let q = survey.get("question").and_then(Value::as_str).unwrap_or("");
    let total = survey.get("total").and_then(Value::as_i64).unwrap_or(0);
    let mut res = format!("アンケート　\"{q}\"　　票数{total}票\n\n");
    if let Some(choices) = survey.get("choices").and_then(Value::as_array) {
        for c in choices {
            let text = c.get("text").and_then(Value::as_str).unwrap_or("");
            let count = c.get("count").and_then(Value::as_i64).unwrap_or(0);
            res.push_str(&format!("　　　{text}　　{count}票\n"));
        }
    }
    res
}

// ---------------------------------------------------------------------------
// Comic series helpers
// ---------------------------------------------------------------------------

fn extract_comic_series_entries(body: &Value) -> Vec<Value> {
    // Try page.series first (newer API)
    if let Some(series) = body
        .get("page")
        .and_then(|v| v.get("series"))
        .and_then(Value::as_array)
    {
        return series.clone();
    }
    // Try page.seriesContents
    if let Some(contents) = body
        .get("page")
        .and_then(|v| v.get("seriesContents"))
        .and_then(Value::as_array)
    {
        return contents.clone();
    }
    // Try thumbnails.illust and construct order/workId
    if let Some(illusts) = body
        .get("thumbnails")
        .and_then(|v| v.get("illust"))
        .and_then(Value::as_array)
    {
        return illusts
            .iter()
            .enumerate()
            .map(|(i, item)| {
                json!({
                    "order": item.get("seriesOrder")
                        .or_else(|| item.get("order"))
                        .and_then(Value::as_i64)
                        .unwrap_or((i + 1) as i64),
                    "workId": item.get("id").map(value_to_string).unwrap_or_default(),
                })
            })
            .collect();
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// Recursive key finder (Python cm.find_key_recursively equivalent)
// ---------------------------------------------------------------------------

fn find_key_recursively(value: &Value, key: &str) -> Option<Value> {
    match value {
        Value::Object(map) => {
            if let Some(v) = map.get(key) {
                return Some(v.clone());
            }
            for v in map.values() {
                if let Some(found) = find_key_recursively(v, key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for v in arr {
                if let Some(found) = find_key_recursively(v, key) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Episode sorting
// ---------------------------------------------------------------------------

fn sort_episodes_by_create_date(episodes: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<Value> = episodes.into_values().collect();
    entries.sort_by(|a, b| {
        let da = a.get("createDate").and_then(Value::as_str).unwrap_or("");
        let db = b.get("createDate").and_then(Value::as_str).unwrap_or("");
        da.cmp(db)
    });
    let mut sorted = Map::new();
    for (i, ep) in entries.into_iter().enumerate() {
        sorted.insert((i + 1).to_string(), ep);
    }
    sorted
}

// ---------------------------------------------------------------------------
// Work record builder (multi-episode aware)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn build_work(
    work_key: &str,
    id: &str,
    title: &str,
    author: &str,
    author_id: Option<String>,
    author_url: Option<String>,
    caption: &str,
    kind: &str,
    serialization: &str,
    tags: &[String],
    all_tags: &[String],
    create_date: &str,
    update_date: &str,
    total_characters: i64,
    all_characters: i64,
    total_episodes: i64,
    all_episodes: i64,
    episodes: &Map<String, Value>,
    url: &str,
) -> WorkRecord {
    WorkRecord {
        site: "pixiv".to_string(),
        work_key: work_key.to_string(),
        title: title.to_string(),
        author: author.to_string(),
        author_id: author_id.clone(),
        author_url: author_url.clone(),
        r#type: kind.to_string(),
        serialization: serialization.to_string(),
        caption: caption.to_string(),
        create_date: create_date.to_string(),
        update_date: update_date.to_string(),
        raw_json: json!({
            "version": VERSION,
            "get_date": chrono::Utc::now().to_rfc3339(),
            "title": title,
            "id": id,
            "nid": work_key,
            "url": url,
            "author": author,
            "author_id": author_id,
            "author_url": author_url,
            "caption": caption,
            "total_episodes": total_episodes,
            "all_episodes": all_episodes,
            "total_characters": total_characters,
            "all_characters": all_characters,
            "type": kind,
            "serialization": serialization,
            "tags": tags,
            "all_tags": all_tags,
            "createDate": create_date,
            "updateDate": update_date,
            "episodes": Value::Object(episodes.clone()),
        }),
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn fetch_body_json(client: &Client, url: &str) -> Result<Value> {
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
        return Err(anyhow!("{message}: {url}"));
    }
    Ok(json.get("body").cloned().unwrap_or_default())
}

fn sleep() {
    thread::sleep(Duration::from_millis(SLEEP_MS));
}

// ---------------------------------------------------------------------------
// Generic helpers
// ---------------------------------------------------------------------------

fn html_breaks_to_newlines(value: &str) -> String {
    value
        .replace("<br />", "\n")
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(v) => v.to_string(),
        _ => String::new(),
    }
}

fn str_field<'a>(body: &'a Value, key: &str, default: &'a str) -> &'a str {
    body.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn int_field(body: &Value, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|key| body.get(*key).and_then(Value::as_i64))
        .unwrap_or(0)
}

fn query_value(url: &str, key: &str) -> Option<String> {
    url.split('?').nth(1).and_then(|query| {
        query.split('&').find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let k = parts.next().unwrap_or("");
            let v = parts.next().unwrap_or("");
            (k == key).then(|| v.to_string())
        })
    })
}

fn last_numeric_segment(path: &str) -> Option<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .rev()
        .find(|segment| segment.chars().all(|ch| ch.is_ascii_digit()))
        .map(ToString::to_string)
}

fn url_ext(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_else(|| ".jpg".to_string())
}

fn url_decode(s: &str) -> String {
    // Simple percent-decode
    let mut result = String::new();
    let mut chars = s.bytes().peekable();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(b'0');
            let h2 = chars.next().unwrap_or(b'0');
            let hex = format!("{}{}", h1 as char, h2 as char);
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push(h1 as char);
                result.push(h2 as char);
            }
        } else {
            result.push(b as char);
        }
    }
    result
}

fn regex_capture(pattern: &str, text: &str) -> Option<String> {
    let re = Regex::new(pattern).ok()?;
    re.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test-output")
            .join(format!("{name}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn parse_pixiv_urls_support_user_forms() {
        assert_eq!(
            parse_pixiv_url("https://www.pixiv.net/users/12345"),
            Some(PixivUrlTarget::User("12345".to_string()))
        );
        assert_eq!(
            parse_pixiv_url("https://www.pixiv.net/user/12345"),
            Some(PixivUrlTarget::User("12345".to_string()))
        );
        assert_eq!(
            parse_pixiv_url("https://www.pixiv.net/user/12345/series/9876"),
            Some(PixivUrlTarget::Comic("9876".to_string()))
        );
        assert_eq!(
            parse_pixiv_url("https://www.pixiv.net/novel/show.php?id=100"),
            Some(PixivUrlTarget::Novel("100".to_string()))
        );
    }

    #[test]
    fn ensure_tracked_user_creates_compat_entry() {
        let dir = test_dir("pixiv-user-config");
        let store = Store::open_in_memory().unwrap();
        let entry = ensure_tracked_user(&store, &dir, "12345").unwrap();

        assert_eq!(entry.novel, "enable");
        assert_eq!(entry.comic, "enable");

        let stored: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("user.json")).unwrap()).unwrap();
        assert_eq!(
            stored.get("version").and_then(Value::as_i64),
            Some(USER_CONFIG_VERSION)
        );
        assert_eq!(
            stored
                .get("12345")
                .and_then(|value| value.get("novel"))
                .and_then(Value::as_str),
            Some("enable")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ensure_tracked_user_preserves_existing_flags_and_snapshot_helpers_are_stable() {
        let dir = test_dir("pixiv-user-snapshot");
        let store = Store::open_in_memory().unwrap();
        fs::write(
            dir.join("user.json"),
            r#"{
  "version": 1,
  "12345": {
    "novel": "disable"
  }
}"#,
        )
        .unwrap();

        let entry = ensure_tracked_user(&store, &dir, "12345").unwrap();
        assert_eq!(entry.novel, "disable");
        assert_eq!(entry.comic, "enable");

        let ids = ["30".to_string(), "10".to_string(), "20".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let hash = save_illust_snapshot(&store, &dir, "12345", &ids).unwrap();
        let loaded = load_illust_snapshot(&store, &dir, "12345").unwrap();
        assert_eq!(loaded, ids);
        assert_eq!(hash, hash_ids(["20", "10", "30"]));

        let tracked = tracked_user_ids(&store, &dir).unwrap();
        assert_eq!(tracked, vec!["12345".to_string()]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolve_active_pixiv_account_prefers_sqlite_before_login_json() {
        let root = test_dir("pixiv-active-account");
        let cookie_root = root.join("cookie");
        let pixiv_cookie_dir = cookie_root.join("pixiv");
        fs::create_dir_all(&pixiv_cookie_dir).unwrap();
        fs::write(
            pixiv_cookie_dir.join("login.json"),
            serde_json::to_string_pretty(&AccountFile {
                cookies: json!({"session": "file"}),
                user_agent: Some("file-ua".to_string()),
                display_name: Some("file".to_string()),
            })
            .unwrap(),
        )
        .unwrap();

        let store = Store::open_in_memory().unwrap();
        store
            .upsert_account(&crate::core::model::AccountRecord {
                site: "pixiv".to_string(),
                name: "db-primary".to_string(),
                display_name: Some("db".to_string()),
                account: AccountFile {
                    cookies: json!({"session": "db"}),
                    user_agent: Some("db-ua".to_string()),
                    display_name: Some("db".to_string()),
                },
                active: true,
                updated_at: "2025-01-01T00:00:00Z".to_string(),
            })
            .unwrap();

        let (account, source) =
            resolve_active_pixiv_account(&store, &cookie_root.to_string_lossy()).unwrap();
        assert_eq!(
            account.cookies.get("session").and_then(Value::as_str),
            Some("db")
        );
        assert!(source.contains("sqlite accounts table"));

        fs::remove_dir_all(root).unwrap();
    }
}
