use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};

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
            "download" => SiteActionResult::skipped(
                self.id(),
                action,
                format!("pixiv download placeholder for {value}"),
            ),
            "update" => SiteActionResult::skipped(self.id(), action, "pixiv update placeholder"),
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
