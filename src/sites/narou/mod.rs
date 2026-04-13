use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};

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

    fn execute(&self, action: &str, _value: &str, _context: &SiteActionContext) -> SiteActionResult {
        SiteActionResult {
            site_id: self.id(),
            action: action.to_string(),
            status: "success".to_string(),
            message: "narou stub".to_string(),
        }
    }
}
