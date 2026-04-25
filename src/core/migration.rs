use crate::core::model::{
    AccountFile, AccountRecord, ImageRecord, MigrationSummary, RequestData, TaskRecord, TaskStatus,
    WorkRecord,
};
use crate::core::storage::Store;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

pub struct MigrationPlan {
    pub source_root: PathBuf,
    pub archive_root: PathBuf,
}

pub fn migrate_legacy_tree(store: &Store, plan: MigrationPlan) -> Result<MigrationSummary> {
    let started_at = now_string();
    let mut summary = MigrationSummary::default();

    if plan.source_root.exists() {
        summary.accounts += migrate_accounts(store, &plan.source_root)?;
        summary.tasks += migrate_tasks(store, &plan.source_root)?;
        summary.works += migrate_works(store, &plan.source_root)?;
        summary.images += migrate_images(store, &plan.source_root)?;
        summary.site_documents += migrate_pixiv_site_documents(store, &plan.source_root)?;
        summary.archived_files += archive_legacy_files(&plan.source_root, &plan.archive_root)?;
    }

    store.record_migration(
        plan.source_root.to_string_lossy().as_ref(),
        &started_at,
        &summary,
    )?;
    Ok(summary)
}

fn migrate_accounts(store: &Store, root: &Path) -> Result<usize> {
    let mut count = 0;
    for site_dir in ["cookie"].iter() {
        let dir = root.join(site_dir);
        if !dir.exists() {
            continue;
        }
        for site_entry in fs::read_dir(&dir)? {
            let site_entry = site_entry?;
            if !site_entry.file_type()?.is_dir() {
                continue;
            }
            let site = site_entry.file_name().to_string_lossy().to_string();
            for file in fs::read_dir(site_entry.path())? {
                let file = file?;
                if !file.file_type()?.is_file() {
                    continue;
                }
                let path = file.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("login")
                    .to_string();
                let account_json: AccountFile = load_json_or_default(&path)?;
                let updated_at = file_modified_string(&path).unwrap_or_else(now_string);
                let active = name == "login";
                let record = AccountRecord {
                    site: site.clone(),
                    name,
                    display_name: account_json.display_name.clone(),
                    account: account_json,
                    active,
                    updated_at,
                };
                store.upsert_account(&record)?;
                count += 1;
            }
        }
    }
    Ok(count)
}

fn migrate_tasks(store: &Store, root: &Path) -> Result<usize> {
    let mut count = 0;
    let queue_dir = root.join("queue");
    if !queue_dir.exists() {
        return Ok(0);
    }

    let task_json = queue_dir.join("task.json");
    if task_json.exists() {
        let state: Value = load_json_or_default(&task_json)?;
        if let Some(queue) = state.get("queue").and_then(|v| v.as_array()) {
            for item in queue.iter() {
                let req = parse_request_data(item);
                let task = TaskRecord {
                    id: 0,
                    request_id: req.request_id.clone(),
                    action: detect_action(&req),
                    param: detect_param(&req),
                    request: req,
                    status: TaskStatus::Queued,
                    error: None,
                    created_at: now_string(),
                    updated_at: now_string(),
                };
                store.enqueue_task(&task)?;
                count += 1;
            }
        }
    }

    let queue_pkl = queue_dir.join("queue.pkl");
    if queue_pkl.exists() {
        let data = fs::read(&queue_pkl)?;
        match serde_pickle::from_slice::<Vec<Value>>(&data, Default::default()) {
            Ok(values) => {
                for item in values {
                    let req = parse_request_data(&item);
                    let task = TaskRecord {
                        id: 0,
                        request_id: req.request_id.clone(),
                        action: detect_action(&req),
                        param: detect_param(&req),
                        request: req,
                        status: TaskStatus::Queued,
                        error: None,
                        created_at: now_string(),
                        updated_at: now_string(),
                    };
                    store.enqueue_task(&task)?;
                    count += 1;
                }
            }
            Err(err) => tracing::warn!(
                "legacy queue pickle parse failed: {} ({err})",
                queue_pkl.display()
            ),
        }
    }

    Ok(count)
}

fn migrate_works(store: &Store, root: &Path) -> Result<usize> {
    let root = root.join("data");
    if !root.exists() {
        tracing::warn!("legacy works directory not found: {}", root.display());
        return Ok(0);
    }
    let mut count = 0;
    for site_dir in fs::read_dir(root)? {
        let site_dir = site_dir?;
        if !site_dir.file_type()?.is_dir() {
            continue;
        }
        let site_name = site_dir.file_name().to_string_lossy().to_string();
        if site_name == "cookie"
            || site_name == "queue"
            || site_name == "pdf"
            || site_name == "log"
            || site_name == "setting"
            || site_name == "target"
            || site_name == "archive"
        {
            continue;
        }
        for work_dir in fs::read_dir(site_dir.path())? {
            let work_dir = work_dir?;
            if !work_dir.file_type()?.is_dir() {
                continue;
            }
            let raw_path = work_dir.path().join("raw").join("raw.json");
            if !raw_path.exists() {
                let alt_json = work_dir.path().join("data.json");
                if alt_json.exists() {
                    let alt: Value = load_json_or_default(&alt_json)?;
                    let record = WorkRecord {
                        site: site_name.clone(),
                        work_key: work_dir.file_name().to_string_lossy().to_string(),
                        title: alt
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        author: alt
                            .get("author")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        author_id: alt
                            .get("author_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        author_url: alt
                            .get("author_url")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        r#type: alt
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        serialization: alt
                            .get("serialization")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        caption: alt
                            .get("caption")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        create_date: pick_first_str(&alt, &["createDate", "create_date"]),
                        update_date: pick_first_str(&alt, &["updateDate", "update_date"]),
                        raw_json: alt,
                    };
                    store.upsert_work(&record)?;
                    count += 1;
                }
                continue;
            }
            let raw_json: Value = load_json_or_default(&raw_path)?;
            let record = WorkRecord {
                site: site_name.clone(),
                work_key: work_dir.file_name().to_string_lossy().to_string(),
                title: raw_json
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                author: raw_json
                    .get("author")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                author_id: raw_json
                    .get("author_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                author_url: raw_json
                    .get("author_url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                r#type: raw_json
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                serialization: raw_json
                    .get("serialization")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                caption: raw_json
                    .get("caption")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                create_date: pick_first_str(&raw_json, &["createDate", "create_date"]),
                update_date: pick_first_str(&raw_json, &["updateDate", "update_date"]),
                raw_json,
            };
            store.upsert_work(&record)?;
            count += 1;
        }
    }
    Ok(count)
}

fn migrate_images(store: &Store, root: &Path) -> Result<usize> {
    let mut count = 0;
    let image_dir = root.join("data").join("images");
    let db_path = image_dir.join("database.json");
    if db_path.exists() {
        let db_json: Value = load_json_or_default(&db_path)?;
        if let Some(map) = db_json.as_object() {
            for (logical_name, hash_value) in map {
                let ext = Path::new(logical_name)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                let record = ImageRecord {
                    logical_name: logical_name.clone(),
                    hash: hash_value.as_str().unwrap_or_default().to_string(),
                    ext,
                    kind: if logical_name.contains("cover") {
                        "cover".to_string()
                    } else {
                        "image".to_string()
                    },
                };
                store.upsert_image(&record)?;
                count += 1;
            }
        }
    }

    let cover_path = image_dir.join("cover.json");
    if cover_path.exists() {
        let cover_json: Value = load_json_or_default(&cover_path)?;
        if let Some(map) = cover_json.as_object() {
            for (logical_name, hash_value) in map {
                let ext = Path::new(logical_name)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                let record = ImageRecord {
                    logical_name: logical_name.clone(),
                    hash: hash_value.as_str().unwrap_or_default().to_string(),
                    ext,
                    kind: "cover".to_string(),
                };
                store.upsert_image(&record)?;
                count += 1;
            }
        }
    }

    for ext in supported_image_extensions() {
        let dir = image_dir.join(ext);
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let logical_name = name.clone();
            let record = ImageRecord {
                logical_name,
                hash: name.trim_end_matches(&format!(".{ext}")).to_string(),
                ext: ext.to_string(),
                kind: "image".to_string(),
            };
            store.upsert_image(&record)?;
            count += 1;
        }
    }

    if image_dir.exists() {
        for entry in fs::read_dir(&image_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                continue;
            };
            if !supported_image_extensions().contains(&ext) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let record = ImageRecord {
                logical_name: name.clone(),
                hash: path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_string(),
                ext: ext.to_string(),
                kind: "image".to_string(),
            };
            store.upsert_image(&record)?;
            count += 1;
        }
    }
    Ok(count)
}

fn migrate_pixiv_site_documents(store: &Store, root: &Path) -> Result<usize> {
    let Some(pixiv_dir) = legacy_pixiv_dir(root) else {
        return Ok(0);
    };

    let mut count = 0;
    let user_config = pixiv_dir.join("user.json");
    if user_config.exists() {
        let document: Value = load_json_or_default(&user_config)?;
        let updated_at = file_modified_string(&user_config).unwrap_or_else(now_string);
        if let Some(entries) = document.as_object() {
            for (user_id, value) in entries {
                if user_id == "version" {
                    continue;
                }
                store.upsert_site_document(
                    "pixiv",
                    &format!("tracked_users/{user_id}/config"),
                    &normalize_pixiv_tracked_user_entry(value),
                    &updated_at,
                )?;
                count += 1;
            }
        }
    }

    let snapshot_dir = pixiv_dir.join("snapshots").join("illust_ids");
    if snapshot_dir.exists() {
        for entry in fs::read_dir(&snapshot_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Some(user_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let snapshot: Value = load_json_or_default(&path)?;
            let updated_at = file_modified_string(&path).unwrap_or_else(now_string);
            store.upsert_site_document(
                "pixiv",
                &format!("tracked_users/{user_id}/illust_ids"),
                &json!({ "illust_ids": normalize_pixiv_illust_ids(&snapshot) }),
                &updated_at,
            )?;
            count += 1;
        }
    }

    Ok(count)
}

fn archive_legacy_files(root: &Path, archive_root: &Path) -> Result<usize> {
    if !archive_root.exists() {
        fs::create_dir_all(archive_root)?;
    }
    let mut count = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let src = entry.path();
        let name = entry.file_name();
        let dst = archive_root.join(name);
        if src.is_file() {
            fs::copy(&src, &dst).context("failed to archive file")?;
            count += 1;
        } else if src.is_dir() {
            let dir_name = src.file_name().and_then(|s| s.to_str()).unwrap_or_default();
            if dir_name == "sample"
                || dir_name == "webnovel"
                || dir_name == "common"
                || dir_name == "templates"
            {
                copy_dir_recursive(&src, &dst)?;
                count += 1;
            }
        }
    }
    Ok(count)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn parse_request_data(value: &Value) -> RequestData {
    RequestData {
        request_id: value
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        pdf_path: value
            .get("pdf_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        pdf_name: value
            .get("pdf_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        zip_name: value
            .get("zip_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        author_id: value
            .get("author_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        author_url: value
            .get("author_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        novel_type: value
            .get("novel_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        chapter: value
            .get("chapter")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        repair: value
            .get("repair")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        login: value
            .get("login")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        update: value
            .get("update")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        re_download: value
            .get("re_download")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        convert: value
            .get("convert")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        add: value
            .get("add")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

fn detect_action(req: &RequestData) -> String {
    if req.repair.is_some() {
        "repair".to_string()
    } else if req.login.is_some() {
        "login".to_string()
    } else if req.update.is_some() {
        "update".to_string()
    } else if req.re_download.is_some() {
        "re_download".to_string()
    } else if req.convert.is_some() {
        "convert".to_string()
    } else {
        "download".to_string()
    }
}

fn detect_param(req: &RequestData) -> String {
    req.repair
        .clone()
        .or(req.login.clone())
        .or(req.update.clone())
        .or(req.re_download.clone())
        .or(req.convert.clone())
        .or(req.add.clone())
        .unwrap_or_default()
}

fn legacy_pixiv_dir(root: &Path) -> Option<PathBuf> {
    let data_path = root.join("data").join("pixiv");
    if data_path.exists() {
        return Some(data_path);
    }

    let direct_path = root.join("pixiv");
    if direct_path.exists() {
        return Some(direct_path);
    }

    None
}

fn normalize_pixiv_tracked_user_entry(value: &Value) -> Value {
    let mut entry = value.as_object().cloned().unwrap_or_default();
    entry
        .entry("novel".to_string())
        .or_insert_with(|| Value::String("enable".to_string()));
    entry
        .entry("comic".to_string())
        .or_insert_with(|| Value::String("enable".to_string()));
    Value::Object(entry)
}

fn normalize_pixiv_illust_ids(value: &Value) -> Vec<String> {
    let mut ids = value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    Value::String(value) => Some(value.clone()),
                    Value::Number(value) => Some(value.to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    ids
}

fn supported_image_extensions() -> &'static [&'static str] {
    &[
        "jpg", "jpeg", "png", "gif", "apng", "webp", "bmp", "avif", "svg",
    ]
}

fn pick_first_str(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn load_json_or_default<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!("legacy json parse failed: {} ({err})", path.display());
            T::default()
        }
    })
}

fn file_modified_string(path: &Path) -> Option<String> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Utc> = modified.into();
    Some(dt.to_rfc3339())
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339()
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
    fn migrate_works_reads_canonical_data_directory() {
        let root = test_dir("migration-works-data-dir");
        let raw_dir = root.join("data").join("pixiv").join("n123").join("raw");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(
            raw_dir.join("raw.json"),
            serde_json::to_string_pretty(&json!({
                "title": "Example Work",
                "author": "Example Author",
                "type": "novel",
                "serialization": "連載中",
                "caption": "caption",
                "createDate": "2025-01-01T00:00:00Z",
                "updateDate": "2025-01-02T00:00:00Z"
            }))
            .unwrap(),
        )
        .unwrap();

        let store = Store::open_in_memory().expect("store");
        let migrated = migrate_works(&store, &root).expect("migrate works");

        assert_eq!(migrated, 1);
        let works = store.list_works(Some("pixiv")).expect("list works");
        assert_eq!(works.len(), 1);
        assert_eq!(works[0].work_key, "n123");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrate_images_imports_extension_directories_and_flat_files() {
        let root = test_dir("migration-images-layouts");
        let image_dir = root.join("data").join("images");
        fs::create_dir_all(image_dir.join("png")).unwrap();
        fs::write(image_dir.join("png").join("abc.png"), b"png").unwrap();
        fs::write(image_dir.join("def.webp"), b"webp").unwrap();

        let store = Store::open_in_memory().expect("store");
        let migrated = migrate_images(&store, &root).expect("migrate images");

        assert_eq!(migrated, 2);
        let images = store.list_images().expect("list images");
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].logical_name, "abc.png");
        assert_eq!(images[0].hash, "abc");
        assert_eq!(images[1].logical_name, "def.webp");
        assert_eq!(images[1].hash, "def");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrate_works_falls_back_to_snake_case_dates() {
        let root = test_dir("migration-works-snake-case-dates");
        let raw_dir = root.join("data").join("narou").join("n123").join("raw");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(
            raw_dir.join("raw.json"),
            serde_json::to_string_pretty(&json!({
                "title": "Narou Work",
                "author": "Author",
                "type": "novel",
                "serialization": "完結済",
                "caption": "caption",
                "create_date": "2025-03-01T00:00:00Z",
                "update_date": "2025-03-02T00:00:00Z"
            }))
            .unwrap(),
        )
        .unwrap();

        let store = Store::open_in_memory().expect("store");
        let migrated = migrate_works(&store, &root).expect("migrate works");

        assert_eq!(migrated, 1);
        let works = store.list_works(Some("narou")).expect("list works");
        assert_eq!(works.len(), 1);
        assert_eq!(works[0].create_date, "2025-03-01T00:00:00Z");
        assert_eq!(works[0].update_date, "2025-03-02T00:00:00Z");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrate_pixiv_site_documents_imports_user_config_and_snapshot() {
        let root = test_dir("migration-pixiv-site-docs");
        let pixiv_dir = root.join("data").join("pixiv");
        fs::create_dir_all(pixiv_dir.join("snapshots").join("illust_ids")).unwrap();
        fs::write(
            pixiv_dir.join("user.json"),
            serde_json::to_string_pretty(&json!({
                "version": 5,
                "12345": {
                    "novel": "disable",
                    "name": "Example User"
                }
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            pixiv_dir
                .join("snapshots")
                .join("illust_ids")
                .join("12345.json"),
            serde_json::to_string_pretty(&json!(["30", 10, "20"])).unwrap(),
        )
        .unwrap();

        let store = Store::open_in_memory().expect("store");
        let summary = migrate_legacy_tree(
            &store,
            MigrationPlan {
                source_root: root.clone(),
                archive_root: root.join("archive"),
            },
        )
        .expect("migrate");

        assert_eq!(summary.site_documents, 2);

        let config = store
            .get_site_document_record("pixiv", "tracked_users/12345/config")
            .expect("load config")
            .expect("config exists");
        let snapshot = store
            .get_site_document_record("pixiv", "tracked_users/12345/illust_ids")
            .expect("load snapshot")
            .expect("snapshot exists");

        let config_object = config.document.as_object().expect("config object");
        assert_eq!(
            config_object.get("novel").and_then(Value::as_str),
            Some("disable")
        );
        assert_eq!(
            config_object.get("comic").and_then(Value::as_str),
            Some("enable")
        );
        assert_eq!(
            snapshot
                .document
                .get("illust_ids")
                .and_then(Value::as_array)
                .map(|items| { items.iter().filter_map(Value::as_str).collect::<Vec<_>>() }),
            Some(vec!["10", "20", "30"])
        );

        fs::remove_dir_all(root).unwrap();
    }
}
