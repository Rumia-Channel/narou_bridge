use crate::core::migration::{migrate_legacy_tree, MigrationPlan};
use crate::core::model::{AccountFile, AccountRecord, AppConfig, RequestData, TaskRecord, TaskStatus};
use crate::core::storage::Store;
use crate::sites::SiteRegistry;
use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RuntimeState {
    pub config: AppConfig,
    pub store: Arc<Mutex<Store>>,
    pub registry: Arc<SiteRegistry>,
}

#[derive(Debug, Deserialize)]
struct AccountQuery {
    site: Option<String>,
    account: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountSwitchPayload {
    account: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountRenamePayload {
    new_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct MigrationQuery {
    source_root: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TaskInput {
    request_id: Option<String>,
    pdf_path: Option<String>,
    pdf_name: Option<String>,
    zip_name: Option<String>,
    author_id: Option<String>,
    author_url: Option<String>,
    novel_type: Option<String>,
    chapter: Option<String>,
    repair: Option<String>,
    login: Option<String>,
    update: Option<String>,
    re_download: Option<String>,
    convert: Option<String>,
    add: Option<String>,
}

pub async fn run(config: AppConfig, store: Store, registry: SiteRegistry) -> Result<()> {
    let state = RuntimeState {
        config: config.clone(),
        store: Arc::new(Mutex::new(store)),
        registry: Arc::new(registry),
    };

    ensure_runtime_dirs(&state.config).await?;
    write_minimal_frontend(&state.config).await?;

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/tasks", get(list_tasks))
        .route("/api/account", get(list_accounts_query).post(upload_account_query).delete(delete_account_query))
        .route("/api/account/switch", post(switch_account_query))
        .route("/api/account/rename", post(rename_account_query))
        .route("/api/", post(handle_post))
        .route("/api/migrate", get(migrate))
        .route("/", get(root))
        .with_state(state.clone());

    let addr: SocketAddr = state.config.bind_addr.parse().context("invalid bind address")?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await.context("server failed")?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn root() -> Html<&'static str> {
    Html(r#"<html><body><h1>Narou Bridge API</h1><p>frontend removed</p></body></html>"#)
}

async fn list_tasks(State(state): State<RuntimeState>) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_tasks() {
        Ok(tasks) => Json(json!({"tasks": tasks})).into_response(),
        Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn list_accounts_query(State(state): State<RuntimeState>, Query(query): Query<AccountQuery>) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let store = state.store.lock().await;
    match store.list_accounts(&site) {
        Ok(accounts) => Json(json!({"site": site, "accounts": accounts})).into_response(),
        Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn upload_account_query(State(state): State<RuntimeState>, Query(query): Query<AccountQuery>, Json(payload): Json<AccountInput>) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let Some(file_text) = payload.file else {
        return create_error(StatusCode::BAD_REQUEST, "file is required".to_string());
    };
    let account_name = payload.name.unwrap_or_else(random_account_name);
    let saved_name = if account_name.ends_with(".json") { account_name } else { format!("{account_name}.json") };
    let account_json: AccountFile = serde_json::from_str(&file_text).unwrap_or_default();
    let dir = state.config.cookie_dir_path(&site);
    if let Err(err) = fs::create_dir_all(&dir).await {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    let path = unique_name_path(&dir, &saved_name).await;
    if let Err(err) = fs::write(&path, file_text.as_bytes()).await {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    let record = AccountRecord {
        site: site.clone(),
        name: saved_name.trim_end_matches(".json").to_string(),
        display_name: account_json.display_name.clone(),
        account: account_json,
        active: saved_name == "login.json",
        updated_at: now_string(),
    };
    let store = state.store.lock().await;
    if let Err(err) = store.upsert_account(&record) {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    Json(json!({"status": "success", "message": format!("Account {saved_name} uploaded successfully"), "site": site, "account": saved_name})).into_response()
}

async fn delete_account_query(State(state): State<RuntimeState>, Query(query): Query<AccountQuery>) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let Some(account) = query.account else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let file_name = if account.ends_with(".json") { account } else { format!("{account}.json") };
    let path = state.config.cookie_dir_path(&site).join(&file_name);
    match fs::remove_file(&path).await {
        Ok(_) => Json(json!({"status": "success", "message": format!("Account {file_name} deleted successfully")})).into_response(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => create_error(StatusCode::NOT_FOUND, "Account not found".to_string()),
        Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn switch_account_query(State(state): State<RuntimeState>, Query(query): Query<AccountQuery>, Json(payload): Json<AccountSwitchPayload>) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let Some(account) = payload.account else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let name = if account.ends_with(".json") { account } else { format!("{account}.json") };
    let dir = state.config.cookie_dir_path(&site);
    let source = dir.join(&name);
    let target = dir.join("login.json");
    if !source.exists() {
        return create_error(StatusCode::NOT_FOUND, format!("Account {name} not found"));
    }
    if target.exists() {
        let backup = dir.join(format!("login.json.backup.{}", chrono::Utc::now().format("%Y%m%d%H%M%S")));
        let _ = fs::rename(&target, &backup).await;
    }
    if let Err(err) = fs::rename(&source, &target).await {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    Json(json!({"status": "success", "message": format!("Switched to account {name}"), "site": site, "account": name})).into_response()
}

async fn rename_account_query(State(state): State<RuntimeState>, Query(query): Query<AccountQuery>, Json(payload): Json<AccountRenamePayload>) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let Some(old_name) = query.account else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let Some(new_name_raw) = payload.new_name else {
        return create_error(StatusCode::BAD_REQUEST, "new_name is required".to_string());
    };
    let old_name = if old_name.ends_with(".json") { old_name } else { format!("{old_name}.json") };
    let new_name = if new_name_raw.ends_with(".json") { new_name_raw } else { format!("{new_name_raw}.json") };
    if old_name == new_name {
        return Json(json!({"status": "success", "message": "Account name unchanged"})).into_response();
    }
    let dir = state.config.cookie_dir_path(&site);
    let old_path = dir.join(&old_name);
    let new_path = dir.join(&new_name);
    if !old_path.exists() {
        return create_error(StatusCode::NOT_FOUND, "Account not found".to_string());
    }
    if new_path.exists() {
        return create_error(StatusCode::CONFLICT, format!("Account name '{new_name}' already exists"));
    }
    if let Err(err) = fs::rename(&old_path, &new_path).await {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    Json(json!({"status": "success", "message": format!("Account renamed from {old_name} to {new_name}"), "site": site, "old_name": old_name, "new_name": new_name})).into_response()
}

async fn handle_post(State(state): State<RuntimeState>, Json(payload): Json<TaskInput>) -> impl IntoResponse {
    let request_id = payload.request_id.unwrap_or_else(random_request_id);
    let req_data = RequestData {
        request_id: request_id.clone(),
        pdf_path: payload.pdf_path,
        pdf_name: payload.pdf_name,
        zip_name: payload.zip_name,
        author_id: payload.author_id,
        author_url: payload.author_url,
        novel_type: payload.novel_type,
        chapter: payload.chapter,
        repair: payload.repair,
        login: payload.login,
        update: payload.update,
        re_download: payload.re_download,
        convert: payload.convert,
        add: payload.add,
    };

    let (action, param) = match resolve_request_action(&req_data) {
        Some(v) => v,
        None => return create_error(StatusCode::BAD_REQUEST, "No valid action found in request data".to_string()),
    };

    let task = TaskRecord {
        id: 0,
        request_id: request_id.clone(),
        action,
        param,
        request: req_data,
        status: TaskStatus::Queued,
        error: None,
        created_at: now_string(),
        updated_at: now_string(),
    };

    let store = state.store.lock().await;
    let task_id = match store.enqueue_task(&task) {
        Ok(id) => id,
        Err(err) => {
            return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
        }
    };

    drop(store);
    if let Err(err) = execute_queued_task(&state, task_id, &task).await {
        let store = state.store.lock().await;
        let _ = store.mark_task_status(task_id, TaskStatus::Failed, Some(err.to_string()), &now_string());
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }

    Json(json!({"status": "queued", "request_id": request_id})).into_response()
}

async fn migrate(State(state): State<RuntimeState>, Query(query): Query<MigrationQuery>) -> impl IntoResponse {
    let source_root = query.source_root.unwrap_or_else(|| state.config.legacy_root.clone().unwrap_or_else(|| "sample".to_string()));
    let plan = MigrationPlan {
        source_root: PathBuf::from(&source_root),
        archive_root: PathBuf::from(&state.config.archive_dir),
    };
    let store = state.store.lock().await;
    match migrate_legacy_tree(&store, plan) {
        Ok(summary) => Json(json!({"status": "success", "summary": summary})).into_response(),
        Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct AccountInput {
    file: Option<String>,
    name: Option<String>,
}

fn resolve_request_action(req: &RequestData) -> Option<(String, String)> {
    if let Some(v) = req.repair.clone() { return Some(("repair".to_string(), v)); }
    if let Some(v) = req.login.clone() { return Some(("login".to_string(), v)); }
    if let Some(v) = req.update.clone() { return Some(("update".to_string(), v)); }
    if let Some(v) = req.re_download.clone() { return Some(("re_download".to_string(), v)); }
    if let Some(v) = req.convert.clone() { return Some(("convert".to_string(), v)); }
    if let Some(v) = req.add.clone() { return Some(("download".to_string(), v)); }
    None
}

async fn execute_queued_task(state: &RuntimeState, task_id: i64, task: &TaskRecord) -> Result<()> {
    let mut store = state.store.lock().await;
    let context = crate::sites::SiteActionContext {
        host_name: state.config.host_name.clone(),
        data_dir: state.config.data_dir.clone(),
        cookie_dir: state.config.cookie_dir.clone(),
        queue_dir: state.config.queue_dir.clone(),
        pdf_dir: state.config.pdf_dir.clone(),
        archive_dir: state.config.archive_dir.clone(),
    };

    let results = state.registry.dispatch(&task.action, &task.param, &context, &mut store);
    let has_success = results.iter().any(|r| r.status == "success");
    let has_failed = results.iter().any(|r| r.status == "failed");
    let status = if has_success { TaskStatus::Succeeded } else if has_failed { TaskStatus::Failed } else { TaskStatus::Skipped };
    let error = if has_failed { Some(results.iter().find(|r| r.status == "failed").map(|r| r.message.clone()).unwrap_or_default()) } else { None };
    let _ = store.mark_task_status(task_id, status, error, &now_string());
    Ok(())
}

fn create_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({"status": "error", "message": message}))).into_response()
}

async fn ensure_runtime_dirs(config: &AppConfig) -> Result<()> {
    fs::create_dir_all(&config.data_dir).await?;
    fs::create_dir_all(&config.cookie_dir).await?;
    fs::create_dir_all(&config.queue_dir).await?;
    fs::create_dir_all(&config.pdf_dir).await?;
    fs::create_dir_all(&config.log_dir).await?;
    fs::create_dir_all(config.data_images_dir()).await?;
    fs::create_dir_all(config.data_reader_dir()).await?;
    fs::create_dir_all(&config.archive_dir).await?;
    Ok(())
}

async fn write_minimal_frontend(config: &AppConfig) -> Result<()> {
    let index = config.data_dir_path().join("index.html");
    if !index.exists() {
        fs::write(&index, b"<html><body><h1>Narou Bridge API</h1></body></html>").await?;
    }
    let reader = config.data_reader_dir().join("index.html");
    if !reader.exists() {
        fs::write(&reader, b"<html><body><h1>Reader removed</h1></body></html>").await?;
    }
    let manifest = config.data_dir_path().join("manifest.json");
    if !manifest.exists() {
        fs::write(&manifest, b"{} ").await?;
    }
    Ok(())
}

async fn unique_name_path(dir: &Path, name: &str) -> PathBuf {
    let mut candidate = dir.join(name);
    let mut counter = 1;
    while candidate.exists() {
        let stem = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name);
        let ext = Path::new(name).extension().and_then(|s| s.to_str()).map(|s| format!(".{s}")).unwrap_or_default();
        candidate = dir.join(format!("{stem}_{counter}{ext}"));
        counter += 1;
    }
    candidate
}

fn random_account_name() -> String {
    uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(5)
        .collect::<String>()
        .to_lowercase()
}

fn random_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339()
}
