pub mod narou;
pub mod pixiv;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SiteId {
    Pixiv,
    Narou,
}

#[derive(Debug, Clone)]
pub struct SiteActionContext {
    pub host_name: String,
    pub data_dir: String,
    pub cookie_dir: String,
    pub queue_dir: String,
    pub pdf_dir: String,
}

#[derive(Debug, Clone)]
pub struct SiteActionResult {
    pub site_id: SiteId,
    pub action: String,
    pub status: String,
    pub message: String,
}

pub trait Site: Send + Sync {
    fn id(&self) -> SiteId;
    fn name(&self) -> &'static str;
    fn supported_actions(&self) -> &'static [&'static str];
    fn matches_url(&self, _url: &str) -> bool { false }
    fn execute(&self, action: &str, _value: &str, _context: &SiteActionContext) -> SiteActionResult {
        SiteActionResult {
            site_id: self.id(),
            action: action.to_string(),
            status: "skipped".to_string(),
            message: "not implemented".to_string(),
        }
    }
}

pub struct SiteRegistry {
    sites: Vec<Box<dyn Site>>,
}

impl SiteRegistry {
    pub fn new(sites: Vec<Box<dyn Site>>) -> Self {
        Self { sites }
    }

    pub fn dispatch(
        &self,
        action: &str,
        value: &str,
        context: &SiteActionContext,
    ) -> Vec<SiteActionResult> {
        let mut results = Vec::new();
        for site in &self.sites {
            if action == "download" && !site.matches_url(value) {
                continue;
            }
            if action != "download" && !site.supported_actions().contains(&action) {
                continue;
            }
            results.push(site.execute(action, value, context));
        }
        results
    }
}
