use anyhow::Result;
use narou_bridge::core::migration::{migrate_legacy_tree, MigrationPlan};
use narou_bridge::core::model::AppConfig;
use narou_bridge::core::registry::build_registry;
use narou_bridge::core::runtime::run;
use narou_bridge::core::storage::Store;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
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

    if args.get(1).map(|s| s.as_str()) == Some("migrate") {
        let source_root = args.get(2).cloned().unwrap_or_else(|| root.join("sample").to_string_lossy().to_string());
        let plan = MigrationPlan {
            source_root: PathBuf::from(source_root),
            archive_root: root.join("archive"),
        };
        let summary = migrate_legacy_tree(&store, plan)?;
        println!("migrated: accounts={}, tasks={}, works={}, images={}, archived={}", summary.accounts, summary.tasks, summary.works, summary.images, summary.archived_files);
        return Ok(());
    }

    let registry = build_registry();
    run(config, store, registry).await
}
