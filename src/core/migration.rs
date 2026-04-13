use crate::core::model::{
    AccountFile, AccountRecord, ImageRecord, MigrationSummary, RequestData, TaskRecord, TaskStatus,
    WorkRecord,
};
use crate::core::storage::Store;
use anyhow::{Context, Result};
use serde_json::Value;
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
        if let Ok(values) = serde_pickle::from_slice::<Vec<Value>>(&data, Default::default()) {
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
    }

    Ok(count)
}

fn migrate_works(store: &Store, root: &Path) -> Result<usize> {
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
                        create_date: alt
                            .get("createDate")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        update_date: alt
                            .get("updateDate")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
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
                create_date: raw_json
                    .get("createDate")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                update_date: raw_json
                    .get("updateDate")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
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

    for ext in ["jpg", "jpeg", "png", "gif", "apng", "webp"] {
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

fn load_json_or_default<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

fn file_modified_string(path: &Path) -> Option<String> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Utc> = modified.into();
    Some(dt.to_rfc3339())
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339()
}
