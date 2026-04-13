pub mod narou;
pub mod pixiv;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::storage::Store;

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
    pub archive_dir: String,
}

#[derive(Debug, Clone)]
pub struct SiteActionResult {
    pub site_id: SiteId,
    pub action: String,
    pub status: String,
    pub message: String,
}

impl SiteActionResult {
    pub fn success(site_id: SiteId, action: &str, message: impl Into<String>) -> Self {
        Self {
            site_id,
            action: action.to_string(),
            status: "success".to_string(),
            message: message.into(),
        }
    }

    pub fn skipped(site_id: SiteId, action: &str, message: impl Into<String>) -> Self {
        Self {
            site_id,
            action: action.to_string(),
            status: "skipped".to_string(),
            message: message.into(),
        }
    }

    pub fn failed(site_id: SiteId, action: &str, message: impl Into<String>) -> Self {
        Self {
            site_id,
            action: action.to_string(),
            status: "failed".to_string(),
            message: message.into(),
        }
    }
}

pub trait Site: Send + Sync {
    fn id(&self) -> SiteId;
    fn name(&self) -> &'static str;
    fn supported_actions(&self) -> &'static [&'static str];
    fn matches_url(&self, _url: &str) -> bool {
        false
    }
    fn execute(
        &self,
        action: &str,
        _value: &str,
        _context: &SiteActionContext,
        _store: &mut Store,
    ) -> SiteActionResult {
        SiteActionResult {
            site_id: self.id(),
            action: action.to_string(),
            status: "skipped".to_string(),
            message: "not implemented".to_string(),
        }
    }
}

pub struct SiteRegistry {
    sites: BTreeMap<String, Box<dyn Site>>,
}

impl SiteRegistry {
    pub fn new(sites: Vec<Box<dyn Site>>) -> Self {
        let mut map = BTreeMap::new();
        for site in sites {
            map.insert(site.name().to_string(), site);
        }
        Self { sites: map }
    }

    pub fn site_names(&self) -> Vec<String> {
        self.sites.keys().cloned().collect()
    }

    pub fn dispatch(
        &self,
        action: &str,
        value: &str,
        context: &SiteActionContext,
        store: &mut Store,
    ) -> Vec<SiteActionResult> {
        let mut results = Vec::new();
        for site in self.resolve_targets(action, value) {
            results.push(match site.execute(action, value, context, store) {
                result => result,
            });
        }
        results
    }

    pub fn resolve_targets(&self, action: &str, value: &str) -> Vec<&dyn Site> {
        if value == "all" {
            return self.sites.values().map(|site| site.as_ref()).collect();
        }

        if action != "download" && value.contains(':') {
            let site_key = value.split_once(':').map(|(key, _)| key).unwrap_or(value);
            return self
                .sites
                .get(site_key)
                .map(|site| vec![site.as_ref()])
                .unwrap_or_default();
        }

        if let Some(site) = self.sites.get(value) {
            return vec![site.as_ref()];
        }

        if action == "download" {
            for site in self.sites.values() {
                if site.matches_url(value) {
                    return vec![site.as_ref()];
                }
            }
        }

        Vec::new()
    }
}
