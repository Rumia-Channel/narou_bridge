use crate::core::model::WorkRecord;
use crate::core::storage::Store;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn render_site_from_store(
    store: &Store,
    site: &str,
    data_dir: &str,
    host_name: &str,
) -> Result<()> {
    let works = store.list_works(Some(site))?;
    let site_dir = PathBuf::from(data_dir).join(site);
    fs::create_dir_all(&site_dir).context("failed to create site dir")?;

    let mut index_entries = Vec::new();
    for work in works {
        render_work(&site_dir, &work, host_name)?;
        index_entries.push(work);
    }

    write_site_index(&site_dir, &index_entries)?;
    Ok(())
}

fn render_work(site_dir: &Path, work: &WorkRecord, host_name: &str) -> Result<()> {
    let work_dir = site_dir.join(&work.work_key);
    let raw_dir = work_dir.join("raw");
    let info_dir = work_dir.join("info");
    fs::create_dir_all(&raw_dir)?;
    fs::create_dir_all(&info_dir)?;

    let raw_json = work.raw_json.to_string();
    fs::write(raw_dir.join("raw.json"), raw_json)?;

    let index_html = format!(
        "<html><body><h1>{}</h1><p>{}</p><p><a href=\"{}/reader/?site={}&nid={}\">reader</a></p></body></html>",
        escape(&work.title),
        escape(&work.author),
        host_name.trim_end_matches('/'),
        work.site,
        work.work_key
    );
    fs::write(work_dir.join("index.html"), index_html)?;
    fs::write(
        info_dir.join("index.html"),
        format!("<html><body><h1>{}</h1></body></html>", escape(&work.title)),
    )?;
    Ok(())
}

fn write_site_index(site_dir: &Path, works: &[WorkRecord]) -> Result<()> {
    let mut entries = serde_json::Map::new();
    for work in works {
        let raw = &work.raw_json;
        let episodes_data = raw
            .get("episodes")
            .and_then(|v| v.as_object())
            .map(|episodes| {
                let mut map = serde_json::Map::new();
                for (key, episode) in episodes {
                    map.insert(
                        key.clone(),
                        serde_json::json!({
                            "title": episode.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                            "id": episode.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                            "caption": episode.get("introduction").and_then(|v| v.as_str()).unwrap_or(""),
                            "tags": episode.get("tags").cloned().unwrap_or_else(|| serde_json::json!([])),
                        }),
                    );
                }
                serde_json::Value::Object(map)
            })
            .unwrap_or_else(|| serde_json::json!({}));

        entries.insert(
            work.work_key.clone(),
            serde_json::json!({
                "title": work.title,
                "author": work.author,
                "author_id": work.author_id,
                "author_url": work.author_url,
                "type": work.r#type,
                "serialization": work.serialization,
                "tags": raw.get("tags").cloned().unwrap_or_else(|| serde_json::json!([])),
                "all_tags": raw.get("all_tags").cloned().unwrap_or_else(|| serde_json::json!([])),
                "caption": work.caption,
                "create_date": work.create_date,
                "update_date": work.update_date,
                "episodes_data": episodes_data,
            }),
        );
    }
    fs::write(
        site_dir.join("index.json"),
        serde_json::to_string_pretty(&serde_json::Value::Object(entries))?,
    )?;
    fs::write(
        site_dir.join("index.html"),
        "<html><body><h1>site index</h1></body></html>",
    )?;
    Ok(())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
