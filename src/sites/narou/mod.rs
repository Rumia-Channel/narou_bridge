use crate::core::renderer;
use crate::core::storage::Store;
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

    fn execute(
        &self,
        action: &str,
        _value: &str,
        context: &SiteActionContext,
        store: &mut Store,
    ) -> SiteActionResult {
        match renderer::render_site_from_store(
            store,
            "narou",
            &context.data_dir,
            &context.host_name,
        ) {
            Ok(_) => {
                SiteActionResult::success(self.id(), action, format!("narou {action} completed"))
            }
            Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
        }
    }
}
