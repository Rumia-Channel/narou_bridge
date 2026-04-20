use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub data_dir: String,
    pub cookie_dir: String,
    pub queue_dir: String,
    pub pdf_dir: String,
    pub log_dir: String,
    pub db_path: String,
    pub archive_dir: String,
    pub bind_addr: String,
    pub host_name: String,
    pub legacy_root: Option<String>,
}

impl AppConfig {
    pub fn data_dir_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.data_dir)
    }

    pub fn log_dir_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.log_dir)
    }

    pub fn db_path_buf(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.db_path)
    }

    pub fn archive_dir_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.archive_dir)
    }

    pub fn legacy_root_path(&self) -> Option<std::path::PathBuf> {
        self.legacy_root.as_deref().map(std::path::PathBuf::from)
    }

    pub fn data_images_dir(&self) -> std::path::PathBuf {
        self.data_dir_path().join("images")
    }

    pub fn data_reader_dir(&self) -> std::path::PathBuf {
        self.data_dir_path().join("reader")
    }

    pub fn cookie_dir_path(&self, site: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.cookie_dir).join(site)
    }

    pub fn pdf_dir_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.pdf_dir)
    }

    pub fn queue_task_json_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.queue_dir).join("task.json")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestData {
    pub request_id: String,
    pub pdf_path: Option<String>,
    pub pdf_name: Option<String>,
    pub zip_name: Option<String>,
    pub author_id: Option<String>,
    pub author_url: Option<String>,
    pub novel_type: Option<String>,
    pub chapter: Option<String>,
    pub repair: Option<String>,
    pub login: Option<String>,
    pub update: Option<String>,
    pub re_download: Option<String>,
    pub convert: Option<String>,
    pub add: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskState {
    pub current_task: Option<RequestData>,
    pub queue: Vec<RequestData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountFile {
    pub cookies: serde_json::Value,
    pub user_agent: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageDatabase(pub std::collections::BTreeMap<String, String>);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoverDatabase(pub std::collections::BTreeMap<String, String>);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SiteIndexEntry {
    pub title: String,
    pub author: String,
    pub author_id: Option<String>,
    pub author_url: Option<String>,
    pub r#type: String,
    pub serialization: String,
    pub tags: Vec<String>,
    pub all_tags: Vec<String>,
    pub caption: String,
    pub create_date: String,
    pub update_date: String,
    #[serde(default)]
    pub episodes_data: std::collections::BTreeMap<String, EpisodeIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EpisodeIndexEntry {
    pub title: String,
    pub id: String,
    pub caption: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Work {
    pub version: i32,
    pub get_date: String,
    pub title: String,
    pub id: String,
    pub nid: String,
    pub url: String,
    pub author: String,
    pub author_id: String,
    pub author_url: String,
    pub caption: String,
    pub total_episodes: i32,
    pub all_episodes: i32,
    pub total_characters: i32,
    pub all_characters: i32,
    pub r#type: String,
    pub serialization: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub all_tags: Vec<String>,
    pub create_date: String,
    pub update_date: String,
    #[serde(default)]
    pub episodes: std::collections::BTreeMap<String, Episode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Episode {
    pub id: String,
    pub chapter: Option<String>,
    pub title: String,
    pub text_count: i32,
    #[serde(default)]
    pub tags: Vec<String>,
    pub introduction: String,
    pub text: String,
    pub postscript: String,
    pub create_date: String,
    pub update_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ZipImportMetadata {
    pub site_name: String,
    #[serde(default)]
    pub images: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Queued
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskRecord {
    pub id: i64,
    pub request_id: String,
    pub action: String,
    pub param: String,
    pub request: RequestData,
    pub status: TaskStatus,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountRecord {
    pub site: String,
    pub name: String,
    pub display_name: Option<String>,
    pub account: AccountFile,
    pub active: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkRecord {
    pub site: String,
    pub work_key: String,
    pub title: String,
    pub author: String,
    pub author_id: Option<String>,
    pub author_url: Option<String>,
    pub r#type: String,
    pub serialization: String,
    pub caption: String,
    pub create_date: String,
    pub update_date: String,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageRecord {
    pub logical_name: String,
    pub hash: String,
    pub ext: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MigrationSummary {
    pub accounts: usize,
    pub tasks: usize,
    pub works: usize,
    pub images: usize,
    pub archived_files: usize,
}
