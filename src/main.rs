use anyhow::{Context, Result, bail};
use narou_bridge::core::bootstrap::load_app_config;
use narou_bridge::core::registry::build_registry;
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
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("rebuild-store") {
        return rebuild_store(&args);
    }
    if args.get(1).map(String::as_str) == Some("install-store") {
        return install_store(&args);
    }

    let config = load_app_config(&root)?;
    let _guard = init_logging(&config.log_dir_path());
    fs::create_dir_all(config.data_dir_path())?;
    let store = Store::open(config.db_path_buf())?;

    if args.get(1).is_some() {
        bail!("unknown command: {}", args[1]);
    }

    let registry = build_registry();
    info!(bind_addr = %config.bind_addr, host_name = %config.host_name, "starting server");
    run(config, store, registry).await
}

fn rebuild_store(args: &[String]) -> Result<()> {
    if args.len() != 4 {
        bail!("usage: narou_bridge rebuild-store <source-db> <output-db>");
    }

    let source_db = std::path::PathBuf::from(&args[2]);
    let output_db = std::path::PathBuf::from(&args[3]);
    if !source_db.is_file() {
        bail!("source database does not exist: {}", source_db.display());
    }
    if output_db.exists() {
        bail!("output database already exists: {}", output_db.display());
    }
    if source_db == output_db {
        bail!("output database must differ from the source database");
    }
    if let Some(parent) = output_db.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let source = Store::open(&source_db)?;
    source.integrity_check()?;
    if let Err(err) = source.vacuum_into(&output_db) {
        let _ = fs::remove_file(&output_db);
        return Err(err);
    }

    let rebuilt = Store::open(&output_db)?;
    rebuilt.integrity_check()?;
    rebuilt.checkpoint_wal()?;
    println!("{}", serde_json::to_string_pretty(&rebuilt.summary()?)?);
    Ok(())
}

fn install_store(args: &[String]) -> Result<()> {
    if args.len() != 4 {
        bail!("usage: narou_bridge install-store <source-db> <destination-db>");
    }

    let source_db = std::path::PathBuf::from(&args[2]);
    let destination_db = std::path::PathBuf::from(&args[3]);
    if !source_db.is_file() {
        bail!("source database does not exist: {}", source_db.display());
    }
    if !destination_db.is_file() {
        bail!(
            "destination database does not exist: {}",
            destination_db.display()
        );
    }
    if source_db == destination_db {
        bail!("source and destination databases must differ");
    }

    let source = Store::open(&source_db)?;
    source.integrity_check()?;
    let destination = Store::open(&destination_db)?;
    destination.replace_contents_from(&source_db)?;
    destination.checkpoint_wal()?;
    println!("{}", serde_json::to_string_pretty(&destination.summary()?)?);
    Ok(())
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
