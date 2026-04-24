use crate::core::renderer;
use crate::core::storage::Store;
use anyhow::{Result, bail};

/// Repair/rebuild Narou works from existing raw.json data only.
pub fn repair_narou(
    store: &Store,
    data_dir: &str,
    host_name: &str,
    pdf_path: Option<&str>,
) -> Result<()> {
    if pdf_path.is_some() {
        bail!("narou repair rebuilds from existing raw.json only; use convert to import PDFs");
    }

    renderer::repair_site_from_raw(store, "narou", data_dir, host_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_rejects_pdf_import_mode() {
        let store = Store::open_in_memory().expect("store");
        let err = repair_narou(&store, "data", "", Some("pdf\\queued.pdf"))
            .expect_err("repair should reject pdf input");
        assert!(
            err.to_string()
                .contains("rebuilds from existing raw.json only")
        );
    }
}
