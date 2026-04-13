use anyhow::Result;
use narou_bridge::core::migration::{migrate_legacy_tree, MigrationPlan};
use narou_bridge::core::model::AppConfig;
use narou_bridge::core::registry::build_registry;
use narou_bridge::core::runtime::run;
use narou_bridge::core::storage::Store;
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::util::SubscriberInitExt;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = init_logging();
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
        info!(
            accounts = summary.accounts,
            tasks = summary.tasks,
            works = summary.works,
            images = summary.images,
            archived = summary.archived_files,
            "migration completed"
        );
        return Ok(());
    }

    let registry = build_registry();
    info!(bind_addr = %config.bind_addr, host_name = %config.host_name, "starting server");
    run(config, store, registry).await
}

fn init_logging() -> WorkerGuard {
    let log_dir = std::env::current_dir()
        .map(|root| root.join("log"))
        .unwrap_or_else(|_| PathBuf::from("log"));
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "narou_bridge.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_span_events(FmtSpan::NONE)
        .compact();
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(false)
        .compact();

    let _ = tracing_subscriber::registry()
        .with(LevelFilter::INFO)
        .with(console_layer)
        .with(file_layer)
        .try_init();

    guard
}
