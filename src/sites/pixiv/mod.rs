use crate::core::model::{ImageRecord, WorkRecord};
use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};
use anyhow::Result;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

pub mod convert;
pub mod download;
pub mod fetch;
pub mod repair;
pub mod update;

pub struct PixivSite;

pub fn site() -> Box<dyn Site> {
    Box::new(PixivSite)
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
        url.contains("pixiv.net")
    }

    fn execute(
        &self,
        action: &str,
        value: &str,
        context: &SiteActionContext,
        store: &mut Store,
    ) -> SiteActionResult {
        match action {
            "download" => match pixiv_download(value, store, &context.data_dir, &context.host_name)
            {
                Ok(message) => SiteActionResult::success(self.id(), action, message),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            "update" | "convert" | "repair" => {
                match renderer::render_site_from_store(
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
                }
            }
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

fn pixiv_download(url: &str, store: &mut Store, data_dir: &str, host_name: &str) -> Result<String> {
    let client = Client::builder().user_agent("narou_bridge/0.1").build()?;
    let path = url.split('?').next().unwrap_or(url);
    let record = if path.contains("/novel/show.php") {
        let id = query_value(url, "id").unwrap_or_default();
        novel_record(&client, &id, url, "n")?
    } else if path.contains("/novel/series/") {
        let id = last_path_segment(path);
        series_record(&client, &id, url, "s")?
    } else if path.contains("/artworks/") {
        let id = last_path_segment(path);
        artwork_record(&client, &id, url, "a")?
    } else if path.contains("/series/") {
        let id = last_path_segment(path);
        comic_series_record(&client, &id, url, "c")?
    } else {
        return Err(anyhow::anyhow!("unsupported pixiv url: {url}"));
    };

    store.upsert_work(&record)?;
    store.upsert_image(&ImageRecord {
        logical_name: format!("{}_cover.jpg", record.work_key),
        hash: format!("{}_hash", record.work_key),
        ext: "jpg".to_string(),
        kind: "cover".to_string(),
    })?;

    save_pixiv_cover_stub(data_dir, &record.work_key)?;
    renderer::render_site_from_store(store, "pixiv", data_dir, host_name)?;
    Ok(format!("stored {}", record.work_key))
}

fn novel_record(client: &Client, id: &str, url: &str, prefix: &str) -> Result<WorkRecord> {
    let body = fetch_json(client, &format!("https://www.pixiv.net/ajax/novel/{id}"))?;
    let body = body.get("body").cloned().unwrap_or_default();
    let tags = extract_tags(&body);
    Ok(work_record(
        prefix,
        id,
        url,
        body.get("title").and_then(|v| v.as_str()).unwrap_or(id),
        body.get("userName")
            .and_then(|v| v.as_str())
            .unwrap_or("pixiv"),
        body.get("userId")
            .and_then(|v| v.as_i64())
            .map(|v| v.to_string()),
        body.get("userId")
            .and_then(|v| v.as_i64())
            .map(|v| format!("https://www.pixiv.net/users/{v}")),
        "novel",
        "短編",
        body.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .replace("<br />", "\n"),
        body.get("createDate")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        body.get("uploadDate")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        body.get("characterCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        tags,
        json!({
            "source_url": url,
            "kind": "novel",
            "fetched": true,
        }),
    ))
}

fn series_record(client: &Client, id: &str, url: &str, prefix: &str) -> Result<WorkRecord> {
    let body = fetch_json(
        client,
        &format!("https://www.pixiv.net/ajax/novel/series/{id}"),
    )?;
    let body = body.get("body").cloned().unwrap_or_default();
    let tags = extract_series_tags(&body);
    Ok(work_record(
        prefix,
        id,
        url,
        body.get("title").and_then(|v| v.as_str()).unwrap_or(id),
        body.get("userName")
            .and_then(|v| v.as_str())
            .unwrap_or("pixiv"),
        body.get("userId")
            .and_then(|v| v.as_i64())
            .map(|v| v.to_string()),
        body.get("userId")
            .and_then(|v| v.as_i64())
            .map(|v| format!("https://www.pixiv.net/users/{v}")),
        "novel",
        "連載中",
        body.get("caption")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .replace("<br />", "\n"),
        body.get("createDate")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        body.get("updateDate")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        body.get("publishedTotalCharacterCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        tags,
        json!({
            "source_url": url,
            "kind": "series",
            "fetched": true,
        }),
    ))
}

fn artwork_record(client: &Client, id: &str, url: &str, prefix: &str) -> Result<WorkRecord> {
    let body = fetch_json(client, &format!("https://www.pixiv.net/ajax/illust/{id}"))?;
    let body = body.get("body").cloned().unwrap_or_default();
    let tags = extract_tags(&body);
    Ok(work_record(
        prefix,
        id,
        url,
        body.get("title").and_then(|v| v.as_str()).unwrap_or(id),
        body.get("userName")
            .and_then(|v| v.as_str())
            .unwrap_or("pixiv"),
        body.get("userId")
            .and_then(|v| v.as_i64())
            .map(|v| v.to_string()),
        body.get("userId")
            .and_then(|v| v.as_i64())
            .map(|v| format!("https://www.pixiv.net/users/{v}")),
        "comic",
        "短編",
        body.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .replace("<br />", "\n"),
        body.get("createDate")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        body.get("uploadDate")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        0,
        tags,
        json!({
            "source_url": url,
            "kind": "artwork",
            "fetched": true,
        }),
    ))
}

fn comic_series_record(client: &Client, id: &str, url: &str, prefix: &str) -> Result<WorkRecord> {
    let body = fetch_json(
        client,
        &format!("https://www.pixiv.net/ajax/series/{id}?p=1&lang=ja"),
    )?;
    let body = body.get("body").cloned().unwrap_or_default();
    let tags = extract_series_tags(&body);
    Ok(work_record(
        prefix,
        id,
        url,
        body.get("title").and_then(|v| v.as_str()).unwrap_or(id),
        body.get("userName")
            .and_then(|v| v.as_str())
            .unwrap_or("pixiv"),
        body.get("userId")
            .and_then(|v| v.as_i64())
            .map(|v| v.to_string()),
        body.get("userId")
            .and_then(|v| v.as_i64())
            .map(|v| format!("https://www.pixiv.net/users/{v}")),
        "comic",
        "連載中",
        body.get("caption")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .replace("<br />", "\n"),
        body.get("createDate")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        body.get("updateDate")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        body.get("publishedTotalCharacterCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        tags,
        json!({
            "source_url": url,
            "kind": "comic_series",
            "fetched": true,
        }),
    ))
}

fn work_record(
    prefix: &str,
    id: &str,
    url: &str,
    title: &str,
    author: &str,
    author_id: Option<String>,
    author_url: Option<String>,
    r#type: &str,
    serialization: &str,
    caption: String,
    create_date: &str,
    update_date: &str,
    text_count: i64,
    tags: Vec<String>,
    raw_json: Value,
) -> WorkRecord {
    WorkRecord {
        site: "pixiv".to_string(),
        work_key: format!("{prefix}{id}"),
        title: title.to_string(),
        author: author.to_string(),
        author_id: author_id.clone(),
        author_url: author_url.clone(),
        r#type: r#type.to_string(),
        serialization: serialization.to_string(),
        caption,
        create_date: create_date.to_string(),
        update_date: update_date.to_string(),
        raw_json: json!({
            "version": 0,
            "get_date": chrono::Utc::now().to_rfc3339(),
            "title": title,
            "id": id,
            "nid": format!("{prefix}{id}"),
            "url": url,
            "author": author,
            "author_id": author_id,
            "author_url": author_url,
            "caption": raw_json.get("caption").and_then(|v| v.as_str()).unwrap_or(""),
            "total_episodes": 1,
            "all_episodes": 1,
            "total_characters": text_count,
            "all_characters": text_count,
            "type": r#type,
            "serialization": serialization,
            "tags": tags.clone(),
            "all_tags": tags.clone(),
            "createDate": create_date,
            "updateDate": update_date,
            "episodes": {
                "1": {
                    "id": id,
                    "chapter": null,
                    "title": title,
                    "textCount": text_count,
                    "tags": tags,
                    "introduction": raw_json.get("caption").and_then(|v| v.as_str()).unwrap_or(""),
                    "text": raw_json.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                    "postscript": "",
                    "createDate": create_date,
                    "updateDate": update_date,
                }
            }
        }),
    }
}

fn fetch_json(client: &Client, url: &str) -> Result<Value> {
    let resp = client.get(url).send()?.error_for_status()?;
    Ok(resp.json()?)
}

fn extract_tags(body: &Value) -> Vec<String> {
    body.get("tags")
        .and_then(|v| v.get("tags"))
        .and_then(|v| v.as_array())
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| {
                    tag.get("tag")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_series_tags(body: &Value) -> Vec<String> {
    body.get("tags")
        .and_then(|v| v.as_array())
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| tag.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
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

fn last_path_segment(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or("0")
        .to_string()
}

fn save_pixiv_cover_stub(data_dir: &str, work_key: &str) -> Result<()> {
    let image_dir = PathBuf::from(data_dir).join("images");
    fs::create_dir_all(&image_dir)?;
    fs::write(image_dir.join(format!("{work_key}_cover.jpg")), b"stub")?;
    Ok(())
}
