use crate::core::renderer;
use crate::core::storage::Store;
use anyhow::Result;

/// Validate Pixiv works; HTML is rendered on request.
pub fn convert_pixiv(
    store: &Store,
    _data_dir: &str,
    _host_name: &str,
    _img_url: &str,
) -> Result<()> {
    renderer::validate_site_from_store(store, "pixiv", None)
}
