use crate::core::renderer;
use crate::core::storage::Store;
use anyhow::Result;

/// Repair/rebuild Pixiv works from existing raw.json data
pub fn repair_pixiv(
    store: &Store,
    data_dir: &str,
    host_name: &str,
) -> Result<()> {
    renderer::render_site_from_store(store, "pixiv", data_dir, host_name)
}
