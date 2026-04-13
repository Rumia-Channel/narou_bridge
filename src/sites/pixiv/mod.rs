use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};

pub mod download;
pub mod fetch;
pub mod update;
pub mod convert;
pub mod repair;

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
        &["download", "login", "update", "convert", "repair"]
    }

    fn matches_url(&self, url: &str) -> bool {
        url.contains("pixiv.net")
    }

    fn execute(&self, action: &str, _value: &str, _context: &SiteActionContext) -> SiteActionResult {
        SiteActionResult {
            site_id: self.id(),
            action: action.to_string(),
            status: "success".to_string(),
            message: "pixiv stub".to_string(),
        }
    }
}
