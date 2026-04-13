use crate::core::model::RequestData;
use crate::sites::{narou, pixiv, SiteActionContext, SiteActionResult, SiteRegistry};

pub fn build_registry() -> SiteRegistry {
    SiteRegistry::new(vec![pixiv::site(), narou::site()])
}

pub fn default_action_order() -> [&'static str; 6] {
    [
        "repair",
        "login",
        "update",
        "re_download",
        "convert",
        "download",
    ]
}

pub fn action_from_request(request: &RequestData) -> Option<(&'static str, String)> {
    if let Some(value) = request.repair.clone() {
        return Some(("repair", value));
    }
    if let Some(value) = request.login.clone() {
        return Some(("login", value));
    }
    if let Some(value) = request.update.clone() {
        return Some(("update", value));
    }
    if let Some(value) = request.re_download.clone() {
        return Some(("re_download", value));
    }
    if let Some(value) = request.convert.clone() {
        return Some(("convert", value));
    }
    request.add.clone().map(|value| ("download", value))
}

pub fn dispatch_download(
    registry: &SiteRegistry,
    context: &SiteActionContext,
    url: &str,
    store: &mut crate::core::storage::Store,
) -> Vec<SiteActionResult> {
    registry.dispatch("download", url, context, store)
}
