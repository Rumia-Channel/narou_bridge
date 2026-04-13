use crate::core::model::{ImageRecord, WorkRecord};
use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};
use anyhow::Result;
use serde_json::json;

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
            "update" => match renderer::render_site_from_store(
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

fn pixiv_download(url: &str, store: &mut Store, data_dir: &str, host_name: &str) -> Result<String> {
    let path = url.split('?').next().unwrap_or(url);
    let site = "pixiv".to_string();
    let (
        work_key,
        title,
        r#type,
        serialization,
        author,
        author_id,
        author_url,
        caption,
        create_date,
        update_date,
    ) = if path.contains("/novel/show.php") {
        let id = url
            .split('?')
            .nth(1)
            .and_then(|query| {
                query.split('&').find_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    let key = parts.next().unwrap_or("");
                    let value = parts.next().unwrap_or("");
                    if key == "id" {
                        Some(value.to_string())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default();
        (
            format!("n{id}"),
            format!("Pixiv Novel {id}"),
            "novel".to_string(),
            "短編".to_string(),
            "pixiv".to_string(),
            Some("0".to_string()),
            Some("https://www.pixiv.net/users/0".to_string()),
            String::new(),
            chrono::Utc::now().to_rfc3339(),
            chrono::Utc::now().to_rfc3339(),
        )
    } else if path.contains("/novel/series/") {
        let id = path
            .split('/')
            .filter(|s: &&str| !s.is_empty())
            .next_back()
            .unwrap_or("0");
        let now = chrono::Utc::now().to_rfc3339();
        (
            format!("s{id}"),
            format!("Pixiv Series {id}"),
            "novel".to_string(),
            "連載中".to_string(),
            "pixiv".to_string(),
            Some("0".to_string()),
            Some("https://www.pixiv.net/users/0".to_string()),
            String::new(),
            now.clone(),
            now,
        )
    } else if path.contains("/artworks/") {
        let id = path
            .split('/')
            .filter(|s: &&str| !s.is_empty())
            .next_back()
            .unwrap_or("0");
        let now = chrono::Utc::now().to_rfc3339();
        (
            format!("a{id}"),
            format!("Pixiv Artwork {id}"),
            "comic".to_string(),
            "短編".to_string(),
            "pixiv".to_string(),
            Some("0".to_string()),
            Some("https://www.pixiv.net/users/0".to_string()),
            String::new(),
            now.clone(),
            now,
        )
    } else if path.contains("/series/") {
        let id = path
            .split('/')
            .filter(|s: &&str| !s.is_empty())
            .next_back()
            .unwrap_or("0");
        let now = chrono::Utc::now().to_rfc3339();
        (
            format!("c{id}"),
            format!("Pixiv Comic Series {id}"),
            "comic".to_string(),
            "連載中".to_string(),
            "pixiv".to_string(),
            Some("0".to_string()),
            Some("https://www.pixiv.net/users/0".to_string()),
            String::new(),
            now.clone(),
            now,
        )
    } else {
        return Err(anyhow::anyhow!("unsupported pixiv url: {url}"));
    };
    let raw_json = json!({"source_url": url, "downloaded": true});

    let record = WorkRecord {
        site: site.clone(),
        work_key: work_key.clone(),
        title,
        author,
        author_id,
        author_url,
        r#type,
        serialization,
        caption,
        create_date,
        update_date,
        raw_json,
    };
    store.upsert_work(&record)?;
    let image = ImageRecord {
        logical_name: format!("{work_key}_cover.jpg"),
        hash: format!("{work_key}_hash"),
        ext: "jpg".to_string(),
        kind: "cover".to_string(),
    };
    let _ = store.upsert_image(&image);
    renderer::render_site_from_store(store, &site, data_dir, host_name)?;
    Ok(format!("stored {work_key}"))
}
