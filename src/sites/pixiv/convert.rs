use crate::core::renderer;
use crate::core::storage::Store;
use anyhow::Result;

/// Render/regenerate Pixiv works from stored data
pub fn convert_pixiv(store: &Store, data_dir: &str, host_name: &str, img_url: &str) -> Result<()> {
    renderer::render_site_from_store(store, "pixiv", data_dir, host_name, img_url, None)
}
