use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

pub fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create parent directory for {}", path.display()))?;

    let mut temp = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file for {}", path.display()))?;
    temp.write_all(contents.as_ref())
        .with_context(|| format!("failed to write temp file for {}", path.display()))?;
    temp.flush()
        .with_context(|| format!("failed to flush temp file for {}", path.display()))?;
    temp.persist(path)
        .map_err(|err| err.error)
        .with_context(|| format!("failed to persist temp file to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::atomic_write;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_round_trips_and_replaces_existing_content() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("task.json");

        atomic_write(&path, br#"{"status":"queued"}"#).expect("initial write");
        assert_eq!(
            fs::read_to_string(&path).expect("read initial"),
            r#"{"status":"queued"}"#
        );

        atomic_write(&path, br#"{"status":"done"}"#).expect("replace write");
        assert_eq!(
            fs::read_to_string(&path).expect("read replacement"),
            r#"{"status":"done"}"#
        );
    }
}
