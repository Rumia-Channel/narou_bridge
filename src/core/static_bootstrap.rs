use crate::core::model::AppConfig;
use anyhow::{Context, Result};
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::fs;

const ROOT_TEMPLATE: &str = include_str!("../../templates/index.html");
const READER_TEMPLATE: &str = include_str!("../../templates/reader_index.html");
const COMMON_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub async fn write_static_bootstrap(config: &AppConfig, sites: &[String]) -> Result<()> {
    ensure_static_dirs(config).await?;
    write_root_index(config, sites).await?;
    write_reader_index(config, sites).await?;
    write_manifest(config).await?;
    write_empty_site_indexes(config, sites).await?;
    mirror_common_assets(config).await?;
    Ok(())
}

async fn ensure_static_dirs(config: &AppConfig) -> Result<()> {
    fs::create_dir_all(config.data_dir_path()).await?;
    fs::create_dir_all(config.data_images_dir()).await?;
    fs::create_dir_all(config.data_reader_dir()).await?;
    fs::create_dir_all(config.data_dir_path().join("css")).await?;
    fs::create_dir_all(config.data_dir_path().join("script")).await?;
    fs::create_dir_all(config.data_dir_path().join("icon")).await?;
    Ok(())
}

async fn write_root_index(config: &AppConfig, sites: &[String]) -> Result<()> {
    let site_buttons = render_site_buttons(sites);
    let site_links = render_site_links(sites);
    let html = ROOT_TEMPLATE
        .replace("{post_url}", "/api/")
        .replace("{site_buttons}", &site_buttons)
        .replace("{site_links}", &site_links);
    write_text_if_changed(&config.data_dir_path().join("index.html"), &html).await
}

async fn write_reader_index(config: &AppConfig, sites: &[String]) -> Result<()> {
    let site_list_json = serde_json::to_string(sites)?;
    let html = READER_TEMPLATE.replace("{site_list_json}", &site_list_json);
    write_text_if_changed(&config.data_reader_dir().join("index.html"), &html).await
}

async fn write_empty_site_indexes(config: &AppConfig, sites: &[String]) -> Result<()> {
    for site in sites {
        let site_dir = config.data_dir_path().join(site);
        fs::create_dir_all(&site_dir).await?;
        let site_index_path = site_dir.join("index.html");
        if fs::try_exists(&site_index_path).await? {
            continue;
        }

        // Create empty site index page that will be overwritten when data is loaded
        let empty_index = render_empty_site_index(site);
        write_text_if_changed(&site_index_path, &empty_index).await?;
    }
    Ok(())
}

fn render_empty_site_index(site: &str) -> String {
    let site_escaped = escape_html(site);
    format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{site} index</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #f6f7fb; color: #1f2933; }}
    header {{ padding: 24px 20px; background: #111827; color: #fff; }}
    main {{ max-width: 1100px; margin: 0 auto; padding: 20px; }}
    .empty-message {{ background: #fff; border-radius: 14px; padding: 32px; text-align: center; box-shadow: 0 1px 4px rgba(0,0,0,.08); }}
    .empty-message h2 {{ margin-top: 0; color: #6b7280; }}
    .empty-message p {{ color: #9ca3af; }}
    .empty-message a {{ color: #2563eb; text-decoration: none; }}
  </style>
</head>
<body>
  <header>
    <h1>{site}</h1>
    <p>0 works · <a href="/">home</a></p>
  </header>
  <main>
    <div class="empty-message">
      <h2>作品がまだありません</h2>
      <p>このサイトに作品をダウンロードまたはインポートすると、ここに表示されます。</p>
      <p><a href="/">トップページに戻る</a></p>
    </div>
  </main>
</body>
</html>"#,
        site = site_escaped
    )
}

async fn write_manifest(config: &AppConfig) -> Result<()> {
    let manifest = json!({
        "name": "Narou Bridge",
        "short_name": "Narou Bridge",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "background_color": "#ffffff",
        "theme_color": "#2563eb",
        "icons": [
            {"src": "/icon/icon_192x192.png", "sizes": "192x192", "type": "image/png"},
            {"src": "/icon/icon_512x512.png", "sizes": "512x512", "type": "image/png"}
        ]
    });
    write_text_if_changed(
        &config.data_dir_path().join("manifest.json"),
        &serde_json::to_string_pretty(&manifest)?,
    )
    .await
}

async fn mirror_common_assets(config: &AppConfig) -> Result<()> {
    mirror_tree(
        &PathBuf::from(COMMON_DIR).join("common").join("css"),
        &config.data_dir_path().join("css"),
    )
    .await?;
    mirror_tree(
        &PathBuf::from(COMMON_DIR).join("common").join("script"),
        &config.data_dir_path().join("script"),
    )
    .await?;
    mirror_tree(
        &PathBuf::from(COMMON_DIR).join("common").join("icon"),
        &config.data_dir_path().join("icon"),
    )
    .await?;
    mirror_file(
        &PathBuf::from(COMMON_DIR)
            .join("common")
            .join("default_cover.png"),
        &config.data_images_dir().join("default_cover.png"),
    )
    .await?;
    Ok(())
}

async fn mirror_tree(source_dir: &Path, target_dir: &Path) -> Result<()> {
    let mut stack = vec![(source_dir.to_path_buf(), target_dir.to_path_buf())];
    while let Some((current_source, current_target)) = stack.pop() {
        let mut entries = fs::read_dir(&current_source)
            .await
            .with_context(|| format!("failed to read {}", current_source.display()))?;
        while let Some(entry) = entries.next_entry().await? {
            let source_path = entry.path();
            let target_path = current_target.join(entry.file_name());
            if entry.file_type().await?.is_dir() {
                fs::create_dir_all(&target_path).await?;
                stack.push((source_path, target_path));
            } else {
                mirror_file(&source_path, &target_path).await?;
            }
        }
    }
    Ok(())
}

async fn mirror_file(source_path: &Path, target_path: &Path) -> Result<()> {
    let bytes = fs::read(source_path)
        .await
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    write_bytes_if_changed(target_path, &bytes).await
}

async fn write_text_if_changed(path: &Path, content: &str) -> Result<()> {
    write_bytes_if_changed(path, content.as_bytes()).await
}

async fn write_bytes_if_changed(path: &Path, content: &[u8]) -> Result<()> {
    if let Ok(existing) = fs::read(path).await {
        if existing == content {
            return Ok(());
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, content).await?;
    Ok(())
}

fn render_site_buttons(sites: &[String]) -> String {
    if sites.is_empty() {
        return r#"<div class="card" style="margin: 0;"><div class="card-body">サイトが登録されていません。</div></div>"#
            .to_string();
    }

    sites
        .iter()
        .map(|site| {
            let site = escape_html(site);
            format!(
                r#"<div class="card" style="margin: 0 0 0.75rem 0;">
  <div class="card-body">
    <div style="display:flex;justify-content:space-between;align-items:center;gap:1rem;flex-wrap:wrap;">
      <a class="site-card" href="/{site}/index.html" style="flex:1;min-width:220px;">
        <span class="site-name">{site}</span>
        <span class="site-arrow">→</span>
      </a>
      <div class="btn-group" style="margin:0;">
        <button onclick="submitUpdate('{site}')" class="btn btn-primary">更新</button>
        <button onclick="submitConvert('{site}')" class="btn btn-success">変換</button>
        <button onclick="submitReDownload('{site}')" class="btn btn-danger">再DL</button>
        <button onclick="submitRepair('{site}')" class="btn btn-warning">修復</button>
      </div>
    </div>
  </div>
</div>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_site_links(sites: &[String]) -> String {
    if sites.is_empty() {
        return r#"<div class="site-card"><span class="site-name">サイトがありません</span></div>"#
            .to_string();
    }

    sites
        .iter()
        .map(|site| {
            let site = escape_html(site);
            format!(
                r#"<a class="site-card" href="/{site}/index.html">
  <span class="site-name">{site}</span>
  <span class="site-arrow">→</span>
</a>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::AppConfig;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn root_bootstrap_mentions_sites() {
        let html = ROOT_TEMPLATE
            .replace("{post_url}", "/api/")
            .replace(
                "{site_buttons}",
                &render_site_buttons(&["pixiv".to_string()]),
            )
            .replace("{site_links}", &render_site_links(&["pixiv".to_string()]));
        assert!(html.contains("/pixiv/index.html"));
        assert!(html.contains("submitUpdate('pixiv')"));
    }

    #[test]
    fn reader_bootstrap_embeds_site_list_json() {
        let html = READER_TEMPLATE.replace(
            "{site_list_json}",
            &serde_json::to_string(&vec!["pixiv".to_string(), "narou".to_string()]).unwrap(),
        );
        assert!(html.contains(r#"const sources = ["pixiv","narou"];"#));
    }

    #[test]
    fn empty_site_index_is_valid_html() {
        let html = render_empty_site_index("pixiv");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>pixiv index</title>"));
        assert!(html.contains("作品がまだありません"));
        assert!(html.contains("href=\"/\""));
    }

    #[tokio::test]
    async fn static_bootstrap_preserves_existing_site_index_html() {
        let test_dir = unique_test_dir("preserve-existing-site-index");
        let config = test_config(&test_dir);
        let site_index_path = config.data_dir_path().join("pixiv").join("index.html");
        let existing_html = "<html><body>existing site index</body></html>";

        fs::create_dir_all(site_index_path.parent().unwrap())
            .await
            .unwrap();
        fs::write(&site_index_path, existing_html).await.unwrap();

        write_static_bootstrap(&config, &["pixiv".to_string()])
            .await
            .unwrap();

        let actual = fs::read_to_string(&site_index_path).await.unwrap();
        assert_eq!(actual, existing_html);

        std::fs::remove_dir_all(test_dir).unwrap();
    }

    #[tokio::test]
    async fn static_bootstrap_creates_missing_site_index_html() {
        let test_dir = unique_test_dir("create-missing-site-index");
        let config = test_config(&test_dir);
        let site_index_path = config.data_dir_path().join("narou").join("index.html");

        write_static_bootstrap(&config, &["narou".to_string()])
            .await
            .unwrap();

        let actual = fs::read_to_string(&site_index_path).await.unwrap();
        assert!(actual.contains("<title>narou index</title>"));
        assert!(actual.contains("作品がまだありません"));

        std::fs::remove_dir_all(test_dir).unwrap();
    }

    fn test_config(root: &std::path::Path) -> AppConfig {
        AppConfig {
            data_dir: root.join("data").to_string_lossy().into_owned(),
            cookie_dir: root.join("cookie").to_string_lossy().into_owned(),
            queue_dir: root.join("queue").to_string_lossy().into_owned(),
            pdf_dir: root.join("pdf").to_string_lossy().into_owned(),
            log_dir: root.join("log").to_string_lossy().into_owned(),
            db_path: root
                .join("data")
                .join("runtime.sqlite3")
                .to_string_lossy()
                .into_owned(),
            archive_dir: root.join("archive").to_string_lossy().into_owned(),
            bind_addr: "127.0.0.1:8080".to_string(),
            host_name: "http://localhost:8080".to_string(),
            auto_update: false,
            auto_update_interval: 0,
            legacy_root: None,
        }
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test-artifacts")
            .join(format!("{name}-{unique}"))
    }
}
