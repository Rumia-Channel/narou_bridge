use anyhow::Result;
use narou_bridge::core::model::AppConfig;
use narou_bridge::core::registry::build_registry;
use narou_bridge::core::storage::Store;
use narou_bridge::core::runtime::run;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let root = std::env::current_dir()?;
    let config = AppConfig {
        data_dir: root.join("data").to_string_lossy().to_string(),
        cookie_dir: root.join("cookie").to_string_lossy().to_string(),
        queue_dir: root.join("queue").to_string_lossy().to_string(),
        pdf_dir: root.join("pdf").to_string_lossy().to_string(),
        log_dir: root.join("log").to_string_lossy().to_string(),
        db_path: root.join("data").join("runtime.sqlite3").to_string_lossy().to_string(),
        archive_dir: root.join("archive").to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:8080".to_string(),
        host_name: "http://127.0.0.1:8080".to_string(),
        legacy_root: Some(root.join("sample").to_string_lossy().to_string()),
    };

    let store = Store::open(PathBuf::from(&config.db_path))?;
    let registry = build_registry();
    run(config, store, registry).await
}
