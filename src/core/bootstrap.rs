use crate::core::model::AppConfig;
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

const ROOT_SETTING_FILE: &str = "setting.ini";

pub fn load_app_config(repo_root: &Path) -> Result<AppConfig> {
    let setting_path = repo_root.join(ROOT_SETTING_FILE);
    if !setting_path.exists() {
        bail!("missing setting.ini at {}", setting_path.display());
    }
    let document = IniDocument::from_file(&setting_path)?;
    Ok(build_app_config(repo_root, &document))
}

fn build_app_config(repo_root: &Path, document: &IniDocument) -> AppConfig {
    let data_dir = resolve_runtime_dir(repo_root, document.get("setting", "data"), "data");
    let pdf_dir = resolve_runtime_dir(repo_root, document.get("setting", "pdf"), "pdf");
    let log_dir = resolve_runtime_dir(repo_root, document.get("setting", "log"), "log");
    let img_url = document
        .get("server", "img_url")
        .map(str::trim)
        .unwrap_or_default()
        .to_string();

    let port = parse_port(document.get("server", "port"), 8080);
    let bind_addr = derive_bind_addr(document.get("server", "domain"), port);
    let host_name = derive_host_name(
        document.get("server", "domain"),
        port,
        parse_bool(document.get("server", "use_proxy")),
        parse_port(document.get("server", "proxy_port"), 443),
        parse_bool(document.get("server", "proxy_ssl")),
    );

    AppConfig {
        data_dir: data_dir.to_string_lossy().to_string(),
        pdf_dir: pdf_dir.to_string_lossy().to_string(),
        log_dir: log_dir.to_string_lossy().to_string(),
        db_path: data_dir
            .join("runtime.sqlite3")
            .to_string_lossy()
            .to_string(),
        bind_addr,
        host_name,
        img_url,
        auto_update: parse_bool(document.get("setting", "auto_update")),
        auto_update_interval: parse_u64(document.get("setting", "auto_update_interval"), 43_200),
    }
}

fn resolve_runtime_dir(repo_root: &Path, raw_value: Option<&str>, child_name: &str) -> PathBuf {
    if let Some(value) = raw_value.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        }
    } else {
        repo_root.join(child_name)
    }
}

fn parse_bool(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .map(|value| {
            value == "1"
                || value.eq_ignore_ascii_case("true")
                || value.eq_ignore_ascii_case("yes")
                || value.eq_ignore_ascii_case("on")
        })
        .unwrap_or(false)
}

fn parse_port(value: Option<&str>, default: u16) -> u16 {
    value
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(default)
}

fn parse_u64(value: Option<&str>, default: u64) -> u64 {
    value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn derive_bind_addr(domain: Option<&str>, port: u16) -> String {
    let bind_host = domain
        .map(extract_bind_host)
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    format!("{bind_host}:{port}")
}

fn extract_bind_host(domain: &str) -> String {
    let normalized = normalize_domain_value(domain);
    if normalized.is_empty() || normalized.eq_ignore_ascii_case("localhost") {
        return "127.0.0.1".to_string();
    }

    if normalized.parse::<IpAddr>().is_ok() {
        normalized
    } else {
        "127.0.0.1".to_string()
    }
}

fn derive_host_name(
    domain: Option<&str>,
    port: u16,
    use_proxy: bool,
    proxy_port: u16,
    proxy_ssl: bool,
) -> String {
    let raw_domain = domain.map(str::trim).filter(|value| !value.is_empty());
    if let Some(explicit_url) =
        raw_domain.filter(|value| value.starts_with("http://") || value.starts_with("https://"))
    {
        return explicit_url.trim_end_matches('/').to_string();
    }

    let domain = raw_domain
        .map(normalize_domain_value)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "localhost".to_string());

    let (scheme, public_port) = if use_proxy {
        (if proxy_ssl { "https" } else { "http" }, proxy_port)
    } else {
        ("http", port)
    };

    if is_default_port(scheme, public_port) {
        format!("{scheme}://{domain}")
    } else {
        format!("{scheme}://{domain}:{public_port}")
    }
}

fn normalize_domain_value(domain: &str) -> String {
    let trimmed = domain.trim().trim_end_matches('/');
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let without_path = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);

    if let Some(host) = without_path
        .strip_prefix('[')
        .and_then(|value| value.split_once(']'))
        .map(|(host, _)| host)
    {
        return host.to_string();
    }

    if let Some((host, _)) = without_path.rsplit_once(':') {
        if !host.contains(':') {
            return host.to_string();
        }
    }

    without_path.to_string()
}

fn is_default_port(scheme: &str, port: u16) -> bool {
    matches!((scheme, port), ("http", 80) | ("https", 443))
}

#[derive(Debug, Default)]
struct IniDocument {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl IniDocument {
    fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::parse(&content)
    }

    fn parse(content: &str) -> Result<Self> {
        let mut document = Self::default();
        let mut current_section = String::new();

        for (line_index, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                document
                    .sections
                    .entry(current_section.clone())
                    .or_default();
                continue;
            }

            let (key, value) = line.split_once('=').ok_or_else(|| {
                anyhow::anyhow!("invalid setting.ini line {}: {}", line_index + 1, raw_line)
            })?;
            document
                .sections
                .entry(current_section.clone())
                .or_default()
                .insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }

        Ok(document)
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_ascii_lowercase())
            .and_then(|entries| entries.get(&key.to_ascii_lowercase()))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IniDocument, build_app_config, derive_bind_addr, derive_host_name, parse_u64,
        resolve_runtime_dir,
    };
    use std::path::Path;

    #[test]
    fn runtime_dirs_default_to_repo_root_children() {
        let root = Path::new(r"C:\repo");
        let dir = resolve_runtime_dir(root, Some(""), "data");
        assert_eq!(dir, root.join("data"));
    }

    #[test]
    fn runtime_dirs_keep_existing_contract_folder() {
        let root = Path::new(r"C:\repo");
        let dir = resolve_runtime_dir(root, Some(r"D:\library\data"), "data");
        assert_eq!(dir, Path::new(r"D:\library\data"));
    }

    #[test]
    fn host_name_uses_proxy_public_settings() {
        let host_name = derive_host_name(Some("example.tailnet.ts.net"), 8080, true, 443, true);
        assert_eq!(host_name, "https://example.tailnet.ts.net");
    }

    #[test]
    fn bind_addr_falls_back_to_localhost_for_domain_names() {
        let bind_addr = derive_bind_addr(Some("example.tailnet.ts.net"), 8080);
        assert_eq!(bind_addr, "127.0.0.1:8080");
    }

    #[test]
    fn parse_u64_uses_default_for_invalid_values() {
        assert_eq!(parse_u64(Some("600"), 43_200), 600);
        assert_eq!(parse_u64(Some("invalid"), 43_200), 43_200);
        assert_eq!(parse_u64(None, 43_200), 43_200);
    }

    #[test]
    fn build_config_reads_runtime_sections() {
        let root = Path::new(r"C:\repo");
        let document = IniDocument::parse(
            r#"
[setting]
data=
auto_update=1
auto_update_interval=600

[server]
domain=localhost
port=9000
use_proxy=0
"#,
        )
        .expect("ini parsing should succeed");

        let config = build_app_config(root, &document);
        assert_eq!(config.data_dir, root.join("data").to_string_lossy());
        assert_eq!(config.bind_addr, "127.0.0.1:9000");
        assert_eq!(config.host_name, "http://localhost:9000");
        assert_eq!(config.img_url, "");
        assert!(config.auto_update);
        assert_eq!(config.auto_update_interval, 600);
    }

    #[test]
    fn build_config_preserves_absolute_external_data_dir() {
        let root = Path::new(r"C:\repo");
        let document = IniDocument::parse(
            r#"
[setting]
data=C:\Users\user\Documents\Webnovel\narou_bridge
pdf=C:\Users\user\Documents\Webnovel\pdf
log=C:\Users\user\Documents\Webnovel\log

[server]
domain=localhost
port=8080
"#,
        )
        .expect("ini parsing should succeed");

        let config = build_app_config(root, &document);
        assert_eq!(
            config.data_dir,
            Path::new(r"C:\Users\user\Documents\Webnovel\narou_bridge").to_string_lossy()
        );
        assert_eq!(
            config.db_path,
            Path::new(r"C:\Users\user\Documents\Webnovel\narou_bridge")
                .join("runtime.sqlite3")
                .to_string_lossy()
        );
    }

    #[test]
    fn build_config_preserves_img_url_override() {
        let root = Path::new(r"C:\repo");
        let document = IniDocument::parse(
            r#"
[server]
domain=example.invalid
port=8080
img_url=https://cdn.example.invalid/novels/
"#,
        )
        .expect("ini parsing should succeed");

        let config = build_app_config(root, &document);
        assert_eq!(config.host_name, "http://example.invalid:8080");
        assert_eq!(config.img_url, "https://cdn.example.invalid/novels/");
    }
}
