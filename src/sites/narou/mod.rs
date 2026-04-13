use crate::core::model::WorkRecord;
use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};
use anyhow::Result;

pub mod convert;
pub mod repair;

pub struct NarouSite;

pub fn site() -> Box<dyn Site> {
    Box::new(NarouSite)
}

impl Site for NarouSite {
    fn id(&self) -> SiteId {
        SiteId::Narou
    }

    fn name(&self) -> &'static str {
        "narou"
    }

    fn supported_actions(&self) -> &'static [&'static str] {
        &["convert", "repair"]
    }

    fn execute(
        &self,
        action: &str,
        value: &str,
        context: &SiteActionContext,
        store: &mut Store,
    ) -> SiteActionResult {
        match action {
            "download" => match narou_download(value, store, &context.data_dir, &context.host_name)
            {
                Ok(message) => SiteActionResult::success(self.id(), action, message),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            "convert" | "repair" => match renderer::render_site_from_store(
                store,
                "narou",
                &context.data_dir,
                &context.host_name,
            ) {
                Ok(_) => SiteActionResult::success(
                    self.id(),
                    action,
                    format!("narou {action} completed"),
                ),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            _ => SiteActionResult::skipped(self.id(), action, "unsupported"),
        }
    }
}

fn narou_download(
    value: &str,
    store: &mut Store,
    data_dir: &str,
    host_name: &str,
) -> Result<String> {
    let work_key = value.replace(['/', ':', '?', '&', '='], "_");
    let record = WorkRecord {
        site: "narou".to_string(),
        work_key: work_key.clone(),
        title: format!("Narou Import {work_key}"),
        author: "narou".to_string(),
        author_id: None,
        author_url: None,
        r#type: "novel".to_string(),
        serialization: "連載中".to_string(),
        caption: value.to_string(),
        create_date: chrono::Utc::now().to_rfc3339(),
        update_date: chrono::Utc::now().to_rfc3339(),
        raw_json: serde_json::json!({"source": value, "imported": true}),
    };
    store.upsert_work(&record)?;
    renderer::render_site_from_store(store, "narou", data_dir, host_name)?;
    Ok(format!("stored {work_key}"))
}
