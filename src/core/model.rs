use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub data_dir: String,
    pub cookie_dir: String,
    pub queue_dir: String,
    pub pdf_dir: String,
    pub log_dir: String,
    pub host_name: String,
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
