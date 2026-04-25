use crate::core::model::{ImageRecord, WorkRecord};
use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};
use anyhow::{Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub mod convert;
pub mod download;
pub mod fetch;
pub mod repair;
pub mod update;

use convert::convert_pixiv;
use download::{download_art, download_comic, download_novel, download_series, download_user};
use fetch::{build_client, download_image, fetch_body_json, sleep};
use repair::repair_pixiv;
use update::pixiv_update;

const VERSION: i64 = 0;
const USER_CONFIG_VERSION: i64 = 5;
const ENABLED_FLAG: &str = "enable";
pub const PIXIV_SITE_DOCUMENT_SCOPE: &str = "pixiv";
const TRACKED_USER_KEY_PREFIX: &str = "tracked_users/";
const TRACKED_USER_CONFIG_SUFFIX: &str = "/config";
const TRACKED_USER_ILLUST_IDS_SUFFIX: &str = "/illust_ids";

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
pub struct UserDownloadSummary {
    pub user_id: String,
    pub user_name: Option<String>,
    pub downloaded_works: usize,
    pub tracked_artworks: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PixivTrackedUserEntry {
    #[serde(default = "enabled_flag_string")]
    pub novel: String,
    #[serde(default = "enabled_flag_string")]
    pub comic: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub illust_ids_snapshot_hash: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
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
pub struct PixivIllustSnapshotDocument {
    #[serde(default)]
    pub illust_ids: BTreeSet<String>,
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
            "convert" => match convert_pixiv(store, &context.data_dir, &context.host_name) {
                Ok(_) => SiteActionResult::success(
                    self.id(),
                    action,
                    "pixiv convert completed".to_string(),
                ),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            "repair" => match repair_pixiv(store, &context.data_dir, &context.host_name) {
                Ok(_) => SiteActionResult::success(
                    self.id(),
                    action,
                    "pixiv repair completed".to_string(),
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

// ---------------------------------------------------------------------------
// Work record persistence
// ---------------------------------------------------------------------------

pub fn persist_work_record(store: &Store, record: &WorkRecord, img_path: &Path) -> Result<()> {
    store.upsert_work(record)?;
    persist_body_image_records(store, &record.raw_json, img_path)?;

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

fn persist_body_image_records(store: &Store, raw_json: &Value, img_path: &Path) -> Result<()> {
    let mut markup_images = BTreeSet::new();
    collect_markup_image_names(raw_json, &mut markup_images);
    if markup_images.is_empty() {
        return Ok(());
    }

    let legacy_database = load_image_database(img_path);
    let mut image_records = BTreeMap::new();

    for logical_name in markup_images {
        let Some((hash, ext)) = parse_markup_image_name(&logical_name) else {
            continue;
        };

        image_records
            .entry(logical_name.clone())
            .or_insert_with(|| ImageRecord {
                logical_name: logical_name.clone(),
                hash: hash.clone(),
                ext: ext.clone(),
                kind: "image".to_string(),
            });

        for (legacy_logical_name, legacy_hash) in &legacy_database {
            if legacy_hash == &hash && logical_name_ext(legacy_logical_name) == Some(ext.as_str()) {
                image_records
                    .entry(legacy_logical_name.clone())
                    .or_insert_with(|| ImageRecord {
                        logical_name: legacy_logical_name.clone(),
                        hash: hash.clone(),
                        ext: ext.clone(),
                        kind: "image".to_string(),
                    });
            }
        }
    }

    for image in image_records.values() {
        store.upsert_image(image)?;
    }

    Ok(())
}

fn collect_markup_image_names(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(text) => {
            for logical_name in extract_markup_image_names(text) {
                out.insert(logical_name);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_markup_image_names(item, out);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_markup_image_names(item, out);
            }
        }
        _ => {}
    }
}

fn extract_markup_image_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut remainder = text;

    while let Some(start) = remainder.find("[image](") {
        let rest = &remainder[start + "[image](".len()..];
        let Some(end) = rest.find(')') else {
            break;
        };
        let logical_name = rest[..end].trim();
        if !logical_name.is_empty() {
            names.push(logical_name.to_string());
        }
        remainder = &rest[end + 1..];
    }

    names
}

fn parse_markup_image_name(logical_name: &str) -> Option<(String, String)> {
    let path = Path::new(logical_name);
    let ext = path.extension()?.to_str()?.to_string();
    let hash = path.file_stem()?.to_str()?.to_string();
    if hash.len() != 16 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some((hash, ext))
}

fn load_image_database(img_path: &Path) -> BTreeMap<String, String> {
    let db_path = img_path.join("database.json");
    let content = match fs::read_to_string(&db_path) {
        Ok(content) => content,
        Err(_) => return BTreeMap::new(),
    };
    let value: Value = serde_json::from_str(&content).unwrap_or_else(|_| json!({}));
    value
        .as_object()
        .map(|map| {
            map.iter()
                .filter_map(|(logical_name, hash)| {
                    Some((logical_name.clone(), hash.as_str()?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn logical_name_ext(logical_name: &str) -> Option<&str> {
    Path::new(logical_name).extension()?.to_str()
}

// ---------------------------------------------------------------------------
// User config paths
// ---------------------------------------------------------------------------

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

pub fn tracked_user_snapshot_key(user_id: &str) -> String {
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

pub fn ensure_tracked_user(
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

pub fn update_tracked_user<F>(
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

pub fn tracked_user_ids(store: &Store, folder_path: &Path) -> Result<Vec<String>> {
    let mut ids = list_tracked_user_entries(store, folder_path)?
        .into_iter()
        .map(|(key, _)| key)
        .collect::<Vec<_>>();
    ids.sort();
    Ok(ids)
}

pub fn action_enabled(entry: &PixivTrackedUserEntry, key: &str) -> bool {
    match key {
        "novel" => entry.novel == ENABLED_FLAG,
        "comic" => entry.comic == ENABLED_FLAG,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Snapshot helpers
// ---------------------------------------------------------------------------

pub fn load_legacy_illust_snapshot(folder_path: &Path, user_id: &str) -> Result<BTreeSet<String>> {
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

pub fn load_illust_snapshot(
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

pub fn save_illust_snapshot(
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

pub fn hash_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> String {
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

// ---------------------------------------------------------------------------
// JSON serialization helpers
// ---------------------------------------------------------------------------

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

pub fn now_string() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ---------------------------------------------------------------------------
// Tag extraction helpers
// ---------------------------------------------------------------------------

pub fn extract_profile_ids(value: Option<&Value>) -> Vec<String> {
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

pub fn extract_object_keys(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn extract_tag_names_nested(body: &Value, path: &[&str]) -> Vec<String> {
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

pub fn extract_flat_tags(body: &Value, key: &str) -> Vec<String> {
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

pub fn extract_tag_names_from_translation(tag_data: Option<&Value>) -> Vec<String> {
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

pub fn format_tags(tags: Vec<String>) -> Vec<String> {
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

pub fn add_ai_tag_if_needed(ai_type: Option<i64>, tags: &mut Vec<String>) {
    if ai_type == Some(2) && !tags.contains(&"AI生成".to_string()) {
        tags.push("AI生成".to_string());
    }
}

pub fn dedup_tags(tags: &mut Vec<String>) {
    let mut seen = HashSet::new();
    tags.retain(|t| seen.insert(t.clone()));
}

// ---------------------------------------------------------------------------
// Comic series helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComicSeriesEntriesPage {
    pub entries: Vec<Value>,
    pub has_more: bool,
}

pub fn extract_comic_series_entries(body: &Value, page: usize) -> ComicSeriesEntriesPage {
    let preferred_sources = [
        body.get("page").and_then(|v| v.get("series")),
        body.get("page").and_then(|v| v.get("contentOrder")),
        body.get("series"),
        body.get("contentOrder"),
        body.get("page").and_then(|v| v.get("seriesContents")),
        body.get("seriesContents"),
    ];

    for source in preferred_sources {
        if let Some(entries) = source
            .and_then(Value::as_array)
            .filter(|entries| !entries.is_empty())
        {
            return ComicSeriesEntriesPage {
                entries: entries.clone(),
                has_more: true,
            };
        }
    }

    if let Some(illusts) = body
        .get("thumbnails")
        .and_then(|v| v.get("illust"))
        .and_then(Value::as_array)
        .filter(|illusts| !illusts.is_empty())
    {
        let offset = page.saturating_sub(1) * illusts.len();
        return ComicSeriesEntriesPage {
            entries: illusts
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    json!({
                        "order": item.get("seriesOrder")
                            .or_else(|| item.get("order"))
                            .and_then(Value::as_i64)
                            .unwrap_or((offset + i + 1) as i64),
                        "workId": item.get("id").map(value_to_string).unwrap_or_default(),
                    })
                })
                .collect(),
            has_more: false,
        };
    }

    ComicSeriesEntriesPage {
        entries: Vec::new(),
        has_more: false,
    }
}

// ---------------------------------------------------------------------------
// Recursive key finder
// ---------------------------------------------------------------------------

pub fn find_key_recursively(value: &Value, key: &str) -> Option<Value> {
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

pub fn sort_episodes_by_create_date(episodes: Map<String, Value>) -> Map<String, Value> {
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
// Work record builder
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn build_work(
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
// Text formatting
// ---------------------------------------------------------------------------

pub fn format_ruby(text: &str) -> String {
    let re = Regex::new(r"\[\[rb:(.*?)\s*>\s*(.*?)\]\]").unwrap();
    re.replace_all(text, "[ruby:<$1>($2)]").to_string()
}

pub fn remove_chapter_tag(text: &str) -> String {
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

pub fn format_jumpuri(text: &str) -> String {
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

pub fn format_jump_url(text: &str) -> String {
    let re = Regex::new(r#""/jump\.php\?([^"]+)""#).unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let decoded = url_decode(&caps[1]);
        format!("\"{decoded}\"")
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Image link processing
// ---------------------------------------------------------------------------

pub fn format_image_links(
    text: &str,
    content_id: &str,
    json_data: &Value,
    client: &reqwest::blocking::Client,
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
// Cover image download
// ---------------------------------------------------------------------------

pub fn download_cover(
    client: &reqwest::blocking::Client,
    url: Option<&str>,
    img_path: &Path,
    ncode: &str,
) {
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

pub fn check_image_file(img_path: &Path, logical_name: &str) -> Option<String> {
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

pub fn save_image_hashed(img_path: &Path, data: &[u8], logical_name: &str) -> String {
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

pub fn update_cover_json(img_path: &Path, logical_name: &str, hash: &str) {
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

pub fn find_image_by_logical_prefix(img_path: &Path, prefix: &str) -> Option<String> {
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
// Survey/poll formatting
// ---------------------------------------------------------------------------

pub fn format_survey(survey: Option<Value>) -> String {
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
// Generic helpers
// ---------------------------------------------------------------------------

pub fn html_breaks_to_newlines(value: &str) -> String {
    value
        .replace("<br />", "\n")
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
}

pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(v) => v.to_string(),
        _ => String::new(),
    }
}

pub fn str_field<'a>(body: &'a Value, key: &str, default: &'a str) -> &'a str {
    body.get(key).and_then(Value::as_str).unwrap_or(default)
}

pub fn int_field(body: &Value, keys: &[&str]) -> i64 {
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

pub fn url_ext(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_else(|| ".jpg".to_string())
}

pub fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut idx = 0;

    while idx < bytes.len() {
        if bytes[idx] == b'%' && idx + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[idx + 1..idx + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    decoded.push(byte);
                    idx += 3;
                    continue;
                }
            }
        }

        decoded.push(bytes[idx]);
        idx += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

pub fn regex_capture(pattern: &str, text: &str) -> Option<String> {
    let re = Regex::new(pattern).ok()?;
    re.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::AccountFile;
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
            fetch::resolve_active_pixiv_account(&store, &cookie_root.to_string_lossy()).unwrap();
        assert_eq!(
            account.cookies.get("session").and_then(Value::as_str),
            Some("db")
        );
        assert!(source.contains("sqlite accounts table"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn url_decode_preserves_existing_unicode_text() {
        assert_eq!(url_decode("先生×先生"), "先生×先生");
    }

    #[test]
    fn url_decode_decodes_percent_encoded_utf8() {
        assert_eq!(
            url_decode("%E5%85%88%E7%94%9F%C3%97%E5%85%88%E7%94%9F"),
            "先生×先生"
        );
    }

    #[test]
    fn persist_work_record_keeps_pixiv_body_images_renderable() {
        let dir = test_dir("pixiv-body-images");
        fs::write(
            dir.join("database.json"),
            r#"{
  "pixiv_123_p0.png": "0123456789abcdef"
}"#,
        )
        .unwrap();

        let store = Store::open_in_memory().unwrap();
        let record = WorkRecord {
            site: "pixiv".to_string(),
            work_key: "a123".to_string(),
            title: "title".to_string(),
            author: "author".to_string(),
            author_id: None,
            author_url: None,
            r#type: "comic".to_string(),
            serialization: "短編".to_string(),
            caption: String::new(),
            create_date: "2025-01-01T00:00:00Z".to_string(),
            update_date: "2025-01-01T00:00:00Z".to_string(),
            raw_json: json!({
                "episodes": {
                    "1": {
                        "text": "[image](0123456789abcdef.png)"
                    }
                }
            }),
        };

        persist_work_record(&store, &record, &dir).unwrap();

        let images = store.list_images().unwrap();
        assert!(images.iter().any(|image| {
            image.logical_name == "0123456789abcdef.png"
                && image.hash == "0123456789abcdef"
                && image.ext == "png"
                && image.kind == "image"
        }));
        assert!(images.iter().any(|image| {
            image.logical_name == "pixiv_123_p0.png"
                && image.hash == "0123456789abcdef"
                && image.ext == "png"
                && image.kind == "image"
        }));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn extract_comic_series_entries_stops_after_thumbnail_fallback_page() {
        let body = json!({
            "thumbnails": {
                "illust": [
                    {"id": "100", "seriesOrder": 1},
                    {"id": "200", "seriesOrder": 2}
                ]
            }
        });

        let page = extract_comic_series_entries(&body, 3);
        assert_eq!(page.entries.len(), 2);
        assert!(!page.has_more);
        assert_eq!(
            page.entries[0].get("workId").and_then(Value::as_str),
            Some("100")
        );
    }
}
