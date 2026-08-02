use crate::core::renderer;
use crate::core::storage::Store;
use anyhow::{Result, bail};

/// Validate Narou works before they are rendered on request.
pub fn repair_narou(
    store: &Store,
    _data_dir: &str,
    _host_name: &str,
    _img_url: &str,
    pdf_path: Option<&str>,
) -> Result<()> {
    if pdf_path.is_some() {
        bail!("narou repair validates stored works only; use convert to import PDFs");
    }

    renderer::validate_site_from_store(store, "narou", None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_rejects_pdf_import_mode() {
        let store = Store::open_in_memory().expect("store");
        let err = repair_narou(&store, "data", "", "", Some("pdf\\queued.pdf"))
            .expect_err("repair should reject pdf input");
        assert!(err.to_string().contains("validates stored works only"));
    }
}
