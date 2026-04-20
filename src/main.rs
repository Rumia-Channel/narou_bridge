use anyhow::Result;
use narou_bridge::core::bootstrap::load_app_config;
use narou_bridge::core::migration::{MigrationPlan, migrate_legacy_tree};
use narou_bridge::core::registry::build_registry;
use narou_bridge::core::renderer;
use narou_bridge::core::runtime::run;
use narou_bridge::core::storage::Store;
use std::fs;
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<()> {
    let root = std::env::current_dir()?;
    let config = load_app_config(&root)?;
    let _guard = init_logging(&config.log_dir_path());
    let args: Vec<String> = std::env::args().collect();
    fs::create_dir_all(config.data_dir_path())?;
    let store = Store::open(config.db_path_buf())?;

    if args.get(1).map(|s| s.as_str()) == Some("migrate") {
        let source_root = args
            .get(2)
            .map(std::path::PathBuf::from)
            .or_else(|| config.legacy_root_path())
            .unwrap_or_else(|| root.join("sample"));
        let plan = MigrationPlan {
            source_root,
            archive_root: config.archive_dir_path(),
        };
        let summary = migrate_legacy_tree(&store, plan)?;
        renderer::refresh_image_manifests(&store, &config.data_dir)?;
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

fn init_logging(log_dir: &std::path::Path) -> WorkerGuard {
    let _ = std::fs::create_dir_all(log_dir);
    let file_appender = tracing_appender::rolling::daily(log_dir, "narou_bridge.log");
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
