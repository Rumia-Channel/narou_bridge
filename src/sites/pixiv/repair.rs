use crate::core::renderer;
use crate::core::storage::Store;
use anyhow::Result;

/// Validate Pixiv works before they are rendered on request.
pub fn repair_pixiv(
    store: &Store,
    _data_dir: &str,
    _host_name: &str,
    _img_url: &str,
) -> Result<()> {
    renderer::validate_site_from_store(store, "pixiv", None)
}
