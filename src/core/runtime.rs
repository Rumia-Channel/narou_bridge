use crate::core::migration::{MigrationPlan, migrate_legacy_tree};
use crate::core::model::{
    AccountFile, AccountRecord, AppConfig, ImageRecord, RequestData, TaskRecord, TaskStatus,
    WorkRecord, ZipImportMetadata,
};
use crate::core::renderer;
use crate::core::static_bootstrap::write_static_bootstrap;
use crate::core::storage::Store;
use crate::sites::SiteRegistry;
use anyhow::{Context, Result};
use axum::Router;
use axum::extract::{FromRequest, Query, Request, State};
use axum::http::header;
use axum::http::{HeaderMap, StatusCode, header::CONTENT_TYPE};
use axum::middleware::map_response;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum_extra::extract::Multipart;
use bytes::Bytes;
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::fs as stdfs;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::sync::{Mutex, Notify};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

const AUTO_UPDATE_STARTUP_DELAY: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct RuntimeState {
    pub config: AppConfig,
    pub store: Arc<Mutex<Store>>,
    pub registry: Arc<SiteRegistry>,
    pub worker_notify: Arc<Notify>,
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
struct WorksQuery {
    site: Option<String>,
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

#[derive(Debug)]
struct PendingUpload {
    original_name: Option<String>,
    bytes: Bytes,
}

pub async fn run(config: AppConfig, store: Store, registry: SiteRegistry) -> Result<()> {
    let state = RuntimeState {
        config: config.clone(),
        store: Arc::new(Mutex::new(store)),
        registry: Arc::new(registry),
        worker_notify: Arc::new(Notify::new()),
    };

    ensure_runtime_dirs(&state.config).await?;
    write_static_bootstrap(&state.config, &state.registry.site_names()).await?;
    recover_queued_runtime_state(&state).await?;
    sync_queue_state(&state).await?;
    tokio::spawn(worker_loop(state.clone()));
    if state.config.auto_update {
        tokio::spawn(auto_update_loop(state.clone()));
    }

    let data_dir = state.config.data_dir_path();
    let static_files = ServeDir::new(&data_dir).append_index_html_on_directories(true);
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/tasks", get(list_tasks))
        .route("/api/works", get(list_works_query))
        .route(
            "/api/account",
            get(list_accounts_query)
                .post(upload_account_query)
                .delete(delete_account_query),
        )
        .route("/api/account/switch", post(switch_account_query))
        .route("/api/account/rename", post(rename_account_query))
        .route("/api/", post(handle_post))
        .route("/api/migrate", get(migrate))
        .route_service("/", ServeFile::new(data_dir.join("index.html")))
        .route_service(
            "/reader",
            ServeFile::new(data_dir.join("reader").join("index.html")),
        )
        .route_service(
            "/reader/",
            ServeFile::new(data_dir.join("reader").join("index.html")),
        )
        .fallback_service(static_files)
        .layer(map_response(ensure_html_utf8_charset))
        .with_state(state.clone());

    let addr: SocketAddr = state
        .config
        .bind_addr
        .parse()
        .context("invalid bind address")?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(
        bind_addr = %state.config.bind_addr,
        host_name = %state.config.host_name,
        data_dir = %state.config.data_dir,
        "http server listening"
    );
    axum::serve(listener, app).await.context("server failed")?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn list_tasks(State(state): State<RuntimeState>) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_tasks() {
        Ok(tasks) => Json(json!({"tasks": tasks})).into_response(),
        Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn list_works_query(
    State(state): State<RuntimeState>,
    Query(query): Query<WorksQuery>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_works(query.site.as_deref()) {
        Ok(works) => Json(json!({"site": query.site, "works": works})).into_response(),
        Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn list_accounts_query(
    State(state): State<RuntimeState>,
    Query(query): Query<AccountQuery>,
) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let store = state.store.lock().await;
    match store.list_accounts(&site) {
        Ok(accounts) => Json(json!({"site": site, "accounts": accounts})).into_response(),
        Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn upload_account_query(
    State(state): State<RuntimeState>,
    Query(query): Query<AccountQuery>,
    Json(payload): Json<AccountInput>,
) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let Some(file_text) = payload.file else {
        return create_error(StatusCode::BAD_REQUEST, "file is required".to_string());
    };
    let account_json: AccountFile = serde_json::from_str(&file_text).unwrap_or_default();
    let preferred_name = payload
        .name
        .as_deref()
        .and_then(normalize_account_name)
        .unwrap_or_else(random_account_name);
    let updated_at = now_string();
    let (saved_name, accounts) = {
        let store = state.store.lock().await;
        let saved_name = match choose_available_account_name(&store, &site, &preferred_name) {
            Ok(name) => name,
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        let should_activate = preferred_name == "login"
            || match store.get_active_account(&site) {
                Ok(active) => active.is_none(),
                Err(err) => {
                    return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
                }
            };
        let record = AccountRecord {
            site: site.clone(),
            name: saved_name.clone(),
            display_name: account_json.display_name.clone(),
            account: account_json,
            active: should_activate,
            updated_at: updated_at.clone(),
        };
        if let Err(err) = store.upsert_account(&record) {
            return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
        }
        if should_activate
            && let Err(err) = store.set_active_account(&site, &saved_name, &updated_at)
        {
            return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
        }
        let accounts = match store.list_accounts(&site) {
            Ok(accounts) => accounts,
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        (saved_name, accounts)
    };
    if let Err(err) = rewrite_cookie_site_mirror(&state.config, &site, &accounts) {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    let file_name = account_file_name(&saved_name);
    Json(json!({"status": "success", "message": format!("Account {file_name} uploaded successfully"), "site": site, "account": file_name})).into_response()
}

async fn delete_account_query(
    State(state): State<RuntimeState>,
    Query(query): Query<AccountQuery>,
) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let Some(account) = query.account else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let Some(account_name) = normalize_account_name(&account) else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let accounts = {
        let store = state.store.lock().await;
        match store.get_account(&site, &account_name) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return create_error(StatusCode::NOT_FOUND, "Account not found".to_string());
            }
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
        if let Err(err) = store.delete_account(&site, &account_name) {
            return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
        }
        match store.list_accounts(&site) {
            Ok(accounts) => accounts,
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    };
    if let Err(err) = rewrite_cookie_site_mirror(&state.config, &site, &accounts) {
        create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    } else {
        let file_name = account_file_name(&account_name);
        Json(json!({"status": "success", "message": format!("Account {file_name} deleted successfully")})).into_response()
    }
}

async fn switch_account_query(
    State(state): State<RuntimeState>,
    Query(query): Query<AccountQuery>,
    Json(payload): Json<AccountSwitchPayload>,
) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let Some(account) = payload.account else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let Some(account_name) = normalize_account_name(&account) else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let updated_at = now_string();
    let (saved_name, accounts) = {
        let store = state.store.lock().await;
        let account = match store.set_active_account(&site, &account_name, &updated_at) {
            Ok(Some(account)) => account,
            Ok(None) => {
                return create_error(
                    StatusCode::NOT_FOUND,
                    format!("Account {} not found", account_file_name(&account_name)),
                );
            }
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        let accounts = match store.list_accounts(&site) {
            Ok(accounts) => accounts,
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        (account.name, accounts)
    };
    if let Err(err) = rewrite_cookie_site_mirror(&state.config, &site, &accounts) {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    let file_name = account_file_name(&saved_name);
    Json(json!({"status": "success", "message": format!("Switched to account {file_name}"), "site": site, "account": file_name})).into_response()
}

async fn rename_account_query(
    State(state): State<RuntimeState>,
    Query(query): Query<AccountQuery>,
    Json(payload): Json<AccountRenamePayload>,
) -> impl IntoResponse {
    let Some(site) = query.site else {
        return create_error(StatusCode::BAD_REQUEST, "site is required".to_string());
    };
    let Some(old_name) = query.account else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let Some(new_name_raw) = payload.new_name else {
        return create_error(StatusCode::BAD_REQUEST, "new_name is required".to_string());
    };
    let Some(old_name) = normalize_account_name(&old_name) else {
        return create_error(StatusCode::BAD_REQUEST, "account is required".to_string());
    };
    let Some(new_name) = normalize_account_name(&new_name_raw) else {
        return create_error(StatusCode::BAD_REQUEST, "new_name is required".to_string());
    };
    if old_name == new_name {
        let store = state.store.lock().await;
        return match store.get_account(&site, &old_name) {
            Ok(Some(_)) => Json(json!({"status": "success", "message": "Account name unchanged"}))
                .into_response(),
            Ok(None) => create_error(StatusCode::NOT_FOUND, "Account not found".to_string()),
            Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
    }
    let updated_at = now_string();
    let accounts = {
        let store = state.store.lock().await;
        match store.get_account(&site, &old_name) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return create_error(StatusCode::NOT_FOUND, "Account not found".to_string());
            }
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
        match store.get_account(&site, &new_name) {
            Ok(Some(_)) => {
                return create_error(
                    StatusCode::CONFLICT,
                    format!(
                        "Account name '{}' already exists",
                        account_file_name(&new_name)
                    ),
                );
            }
            Ok(None) => {}
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
        if let Err(err) = store.rename_account(&site, &old_name, &new_name, &updated_at) {
            return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
        }
        match store.list_accounts(&site) {
            Ok(accounts) => accounts,
            Err(err) => return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        }
    };
    if let Err(err) = rewrite_cookie_site_mirror(&state.config, &site, &accounts) {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    let old_file_name = account_file_name(&old_name);
    let new_file_name = account_file_name(&new_name);
    Json(json!({"status": "success", "message": format!("Account renamed from {old_file_name} to {new_file_name}"), "site": site, "old_name": old_file_name, "new_name": new_file_name})).into_response()
}

async fn handle_post(State(state): State<RuntimeState>, request: Request) -> impl IntoResponse {
    let payload = match parse_task_input(&state, request).await {
        Ok(payload) => payload,
        Err(message) => return create_error(StatusCode::BAD_REQUEST, message),
    };

    let request_id = payload.request_id.clone().unwrap_or_else(random_request_id);
    let req_data = request_data_from_input(payload, request_id.clone());

    let (action, param) = match resolve_request_action(&req_data) {
        Some(v) => v,
        None => {
            return create_error(
                StatusCode::BAD_REQUEST,
                "No valid action found in request data".to_string(),
            );
        }
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

    match enqueue_task_record(&state, task, false).await {
        Ok(EnqueueTaskOutcome::Enqueued) => {}
        Ok(EnqueueTaskOutcome::SkippedDuplicate) => unreachable!("HTTP enqueue does not dedupe"),
        Err(err) => {
            return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
        }
    }

    Json(json!({"status": "queued", "request_id": request_id})).into_response()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnqueueTaskOutcome {
    Enqueued,
    SkippedDuplicate,
}

async fn enqueue_task_record(
    state: &RuntimeState,
    task: TaskRecord,
    dedupe_incomplete: bool,
) -> Result<EnqueueTaskOutcome> {
    let request_id = task.request_id.clone();
    let action = task.action.clone();
    let param = task.param.clone();

    let outcome = {
        let store = state.store.lock().await;
        if dedupe_incomplete && store.has_incomplete_task(&action, &param)? {
            EnqueueTaskOutcome::SkippedDuplicate
        } else {
            store.enqueue_task(&task)?;
            EnqueueTaskOutcome::Enqueued
        }
    };

    if matches!(outcome, EnqueueTaskOutcome::Enqueued) {
        if let Err(err) = sync_queue_state(state).await {
            warn!(request_id = %request_id, error = %err, "failed to persist queue/task.json after enqueue");
        }
        state.worker_notify.notify_one();
    }

    Ok(outcome)
}

fn auto_update_task() -> TaskRecord {
    let request_id = random_request_id();
    let now = now_string();
    TaskRecord {
        id: 0,
        request_id: request_id.clone(),
        action: "update".to_string(),
        param: "all".to_string(),
        request: RequestData {
            request_id,
            update: Some("all".to_string()),
            ..RequestData::default()
        },
        status: TaskStatus::Queued,
        error: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

async fn enqueue_auto_update_task(state: &RuntimeState) -> Result<EnqueueTaskOutcome> {
    enqueue_task_record(state, auto_update_task(), true).await
}

async fn auto_update_loop(state: RuntimeState) {
    info!(
        startup_delay_seconds = AUTO_UPDATE_STARTUP_DELAY.as_secs(),
        interval_seconds = state.config.auto_update_interval,
        "auto-update loop started"
    );
    tokio::time::sleep(AUTO_UPDATE_STARTUP_DELAY).await;

    let interval = Duration::from_secs(state.config.auto_update_interval.max(1));
    loop {
        match enqueue_auto_update_task(&state).await {
            Ok(EnqueueTaskOutcome::Enqueued) => {
                info!("enqueued automatic update=all task");
            }
            Ok(EnqueueTaskOutcome::SkippedDuplicate) => {
                info!(
                    "skipped automatic update because equivalent work is already queued or running"
                );
            }
            Err(err) => {
                error!(error = %err, "failed to enqueue automatic update task");
            }
        }

        let next_run = chrono::Utc::now()
            + chrono::TimeDelta::from_std(interval)
                .unwrap_or_else(|_| chrono::TimeDelta::seconds(0));
        info!(
            next_run = %next_run.to_rfc3339(),
            interval_seconds = interval.as_secs(),
            "next auto-update scheduled"
        );
        tokio::time::sleep(interval).await;
    }
}

async fn migrate(
    State(state): State<RuntimeState>,
    Query(query): Query<MigrationQuery>,
) -> impl IntoResponse {
    let source_root = query.source_root.unwrap_or_else(|| {
        state
            .config
            .legacy_root
            .clone()
            .unwrap_or_else(|| "sample".to_string())
    });
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
    if req.pdf_path.is_some() {
        return Some(("convert".to_string(), "narou".to_string()));
    }
    if let Some(v) = req.repair.clone() {
        return Some(("repair".to_string(), v));
    }
    if let Some(v) = req.login.clone() {
        return Some(("login".to_string(), v));
    }
    if let Some(v) = req.update.clone() {
        return Some(("update".to_string(), v));
    }
    if let Some(v) = req.re_download.clone() {
        return Some(("re_download".to_string(), v));
    }
    if let Some(v) = req.convert.clone() {
        return Some(("convert".to_string(), v));
    }
    if req.zip_name.is_some() {
        return Some(("convert".to_string(), "zip".to_string()));
    }
    if let Some(v) = req.add.clone() {
        return Some(("download".to_string(), v));
    }
    None
}

async fn execute_queued_task(state: &RuntimeState, task_id: i64, task: &TaskRecord) -> Result<()> {
    let mut store = Store::open(state.config.db_path_buf())?;

    if should_import_zip_request(&task.request) {
        import_uploaded_zip(
            &store,
            &task.request,
            &state.config.data_dir,
            &state.config.pdf_dir,
            &state.config.host_name,
        )?;
        let _ = store.mark_task_status(task_id, TaskStatus::Succeeded, None, &now_string());
        return Ok(());
    }

    let context = crate::sites::SiteActionContext {
        host_name: state.config.host_name.clone(),
        data_dir: state.config.data_dir.clone(),
        cookie_dir: state.config.cookie_dir.clone(),
        queue_dir: state.config.queue_dir.clone(),
        pdf_dir: state.config.pdf_dir.clone(),
        archive_dir: state.config.archive_dir.clone(),
        request: task.request.clone(),
    };

    let results = state
        .registry
        .dispatch(&task.action, &task.param, &context, &mut store);
    let (status, error) = summarize_site_results(&results);
    let _ = store.mark_task_status(task_id, status, error, &now_string());
    Ok(())
}

fn summarize_site_results(
    results: &[crate::sites::SiteActionResult],
) -> (TaskStatus, Option<String>) {
    let failed: Vec<_> = results
        .iter()
        .filter(|result| result.status == "failed")
        .collect();
    let succeeded: Vec<_> = results
        .iter()
        .filter(|result| result.status == "success")
        .collect();
    let skipped: Vec<_> = results
        .iter()
        .filter(|result| result.status == "skipped")
        .collect();

    let status = if !failed.is_empty() {
        TaskStatus::Failed
    } else if !succeeded.is_empty() {
        TaskStatus::Succeeded
    } else {
        TaskStatus::Skipped
    };

    let error = if failed.is_empty() {
        None
    } else {
        let mut sections = vec![format!("failed sites: {}", summarize_result_group(&failed))];
        if !succeeded.is_empty() {
            sections.push(format!(
                "succeeded sites: {}",
                summarize_result_group(&succeeded)
            ));
        }
        if !skipped.is_empty() {
            sections.push(format!(
                "skipped sites: {}",
                summarize_result_group(&skipped)
            ));
        }
        Some(sections.join(" | "))
    };

    (status, error)
}

fn summarize_result_group(results: &[&crate::sites::SiteActionResult]) -> String {
    results
        .iter()
        .map(|result| {
            format!(
                "{} ({}) {}",
                site_id_label(result.site_id),
                result.action,
                result.message
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn site_id_label(site_id: crate::sites::SiteId) -> &'static str {
    match site_id {
        crate::sites::SiteId::Pixiv => "pixiv",
        crate::sites::SiteId::Narou => "narou",
    }
}

fn create_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({"status": "error", "message": message}))).into_response()
}

async fn ensure_html_utf8_charset(mut response: Response) -> Response {
    let should_update = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let normalized = value.to_ascii_lowercase();
            normalized.starts_with("text/html") && !normalized.contains("charset=")
        })
        .unwrap_or(false);

    if should_update {
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/html; charset=utf-8"),
        );
    }

    response
}

async fn parse_task_input(state: &RuntimeState, request: Request) -> Result<TaskInput, String> {
    let (parts, body) = request.into_parts();
    let content_type = parts
        .headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if content_type.contains("multipart/form-data") {
        let request = Request::from_parts(parts, body);
        let multipart = Multipart::from_request(request, state)
            .await
            .map_err(|err| err.to_string())?;
        return parse_multipart_task_input(&state.config, multipart).await;
    }

    let collected = body
        .collect()
        .await
        .map_err(|err| err.to_string())?
        .to_bytes();
    parse_task_input_bytes(&parts.headers, &collected)
}

fn parse_task_input_bytes(headers: &HeaderMap, body: &Bytes) -> Result<TaskInput, String> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if content_type.contains("application/json") || content_type.ends_with("+json") {
        return serde_json::from_slice(body)
            .map(normalize_task_input)
            .map(discard_client_request_id)
            .map_err(|err| err.to_string());
    }

    if content_type.contains("application/x-www-form-urlencoded") {
        return serde_urlencoded::from_bytes(body)
            .map(normalize_task_input)
            .map(discard_client_request_id)
            .map_err(|err| err.to_string());
    }

    serde_json::from_slice(body)
        .or_else(|_| serde_urlencoded::from_bytes(body))
        .map(normalize_task_input)
        .map(discard_client_request_id)
        .map_err(|err| err.to_string())
}

async fn parse_multipart_task_input(
    config: &AppConfig,
    mut multipart: Multipart,
) -> Result<TaskInput, String> {
    let mut input = TaskInput::default();
    let mut pdf_upload = None;
    let mut zip_upload = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| err.to_string())?
    {
        let field_name = field.name().map(str::to_string).unwrap_or_default();
        let file_name = field.file_name().map(str::to_string);

        match field_name.as_str() {
            "pdf" => {
                let bytes = field.bytes().await.map_err(|err| err.to_string())?;
                if !bytes.is_empty() {
                    if input.pdf_name.is_none() {
                        input.pdf_name = file_name.clone();
                    }
                    pdf_upload = Some(PendingUpload {
                        original_name: file_name,
                        bytes,
                    });
                }
            }
            "zip" => {
                let bytes = field.bytes().await.map_err(|err| err.to_string())?;
                if !bytes.is_empty() {
                    if input.zip_name.is_none() {
                        input.zip_name = file_name.clone();
                    }
                    zip_upload = Some(PendingUpload {
                        original_name: file_name,
                        bytes,
                    });
                }
            }
            _ => {
                let text = field.text().await.map_err(|err| err.to_string())?;
                apply_task_input_field(&mut input, &field_name, text);
            }
        }
    }

    input = normalize_task_input(input);
    input = discard_client_request_id(input);
    let request_id = random_request_id();

    if let Some(upload) = pdf_upload {
        let path = persist_uploaded_file(config, &request_id, "pdf", upload.bytes).await?;
        input.pdf_path = Some(path_to_string(&path));
        if input.pdf_name.is_none() {
            input.pdf_name = upload.original_name;
        }
    }

    if let Some(upload) = zip_upload {
        let _ = persist_uploaded_file(config, &request_id, "zip", upload.bytes).await?;
        if input.zip_name.is_none() {
            input.zip_name = upload.original_name;
        }
    }

    input.request_id = Some(request_id);
    Ok(input)
}

fn request_data_from_input(input: TaskInput, request_id: String) -> RequestData {
    RequestData {
        request_id,
        pdf_path: input.pdf_path,
        pdf_name: input.pdf_name,
        zip_name: input.zip_name,
        author_id: input.author_id,
        author_url: input.author_url,
        novel_type: input.novel_type,
        chapter: input.chapter,
        repair: input.repair,
        login: input.login,
        update: input.update,
        re_download: input.re_download,
        convert: input.convert,
        add: input.add,
    }
}

fn normalize_task_input(input: TaskInput) -> TaskInput {
    TaskInput {
        request_id: normalize_optional_string(input.request_id),
        pdf_path: normalize_optional_string(input.pdf_path),
        pdf_name: normalize_optional_string(input.pdf_name),
        zip_name: normalize_optional_string(input.zip_name),
        author_id: normalize_optional_string(input.author_id),
        author_url: normalize_optional_string(input.author_url),
        novel_type: normalize_optional_string(input.novel_type),
        chapter: normalize_optional_string(input.chapter),
        repair: normalize_optional_string(input.repair),
        login: normalize_optional_string(input.login),
        update: normalize_optional_string(input.update),
        re_download: normalize_optional_string(input.re_download),
        convert: normalize_optional_string(input.convert),
        add: normalize_optional_string(input.add),
    }
}

fn discard_client_request_id(mut input: TaskInput) -> TaskInput {
    input.request_id = None;
    input
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn apply_task_input_field(input: &mut TaskInput, name: &str, value: String) {
    match name {
        "request_id" => input.request_id = Some(value),
        "pdf_path" => input.pdf_path = Some(value),
        "pdf_name" => input.pdf_name = Some(value),
        "zip_name" => input.zip_name = Some(value),
        "author_id" => input.author_id = Some(value),
        "author_url" => input.author_url = Some(value),
        "novel_type" => input.novel_type = Some(value),
        "chapter" => input.chapter = Some(value),
        "repair" => input.repair = Some(value),
        "login" => input.login = Some(value),
        "update" => input.update = Some(value),
        "re_download" => input.re_download = Some(value),
        "convert" => input.convert = Some(value),
        "add" => input.add = Some(value),
        _ => {}
    }
}

async fn persist_uploaded_file(
    config: &AppConfig,
    request_id: &str,
    extension: &str,
    bytes: Bytes,
) -> Result<PathBuf, String> {
    let pdf_dir = config.pdf_dir_path();
    fs::create_dir_all(&pdf_dir)
        .await
        .map_err(|err| err.to_string())?;
    let path =
        queued_upload_path(&pdf_dir, request_id, extension).map_err(|err| err.to_string())?;
    fs::write(&path, bytes)
        .await
        .map_err(|err| err.to_string())?;
    Ok(path)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn should_import_zip_request(request: &RequestData) -> bool {
    request.zip_name.is_some()
        && request.pdf_path.is_none()
        && request.repair.is_none()
        && request.login.is_none()
        && request.update.is_none()
        && request.re_download.is_none()
        && request.convert.is_none()
        && request.add.is_none()
}

fn import_uploaded_zip(
    store: &Store,
    request: &RequestData,
    data_dir: &str,
    pdf_dir: &str,
    host_name: &str,
) -> Result<String> {
    let archive_path =
        queued_zip_path(request, pdf_dir)?.context("queued ZIP upload is missing from pdf/")?;
    let file = stdfs::File::open(&archive_path)
        .with_context(|| format!("failed to open zip archive {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("invalid zip {}", archive_path.display()))?;
    let metadata = read_zip_metadata(&mut archive)?;

    let payload_entries =
        collect_zip_payload_entries(&mut archive, Path::new(data_dir), &metadata.site_name)?;
    extract_zip_payload(&mut archive, &payload_entries)?;
    let imported = import_site_records_from_tree(
        store,
        &PathBuf::from(data_dir).join(&metadata.site_name),
        &metadata.site_name,
    )?;
    merge_zip_images(store, &metadata)?;
    renderer::refresh_image_manifests(store, data_dir)?;
    renderer::render_site_from_store(store, &metadata.site_name, data_dir, host_name)?;

    Ok(format!(
        "imported {} ({imported} works)",
        request
            .zip_name
            .clone()
            .unwrap_or_else(|| metadata.site_name.clone())
    ))
}

fn queued_zip_path(request: &RequestData, pdf_dir: &str) -> Result<Option<PathBuf>> {
    if request.zip_name.is_none() {
        return Ok(None);
    }

    let path = queued_upload_path(Path::new(pdf_dir), &request.request_id, "zip")?;
    Ok(path.exists().then_some(path))
}

fn read_zip_metadata<R>(archive: &mut zip::ZipArchive<R>) -> Result<ZipImportMetadata>
where
    R: Read + std::io::Seek,
{
    let mut metadata = String::new();
    archive
        .by_name("data.json")
        .context("zip archive is missing data.json")?
        .read_to_string(&mut metadata)
        .context("failed to read zip data.json")?;
    let metadata: ZipImportMetadata =
        serde_json::from_str(&metadata).context("failed to parse zip data.json")?;
    normalize_zip_metadata(metadata)
}

fn collect_zip_payload_entries<R>(
    archive: &mut zip::ZipArchive<R>,
    data_dir: &Path,
    site_name: &str,
) -> Result<Vec<ZipPayloadEntry>>
where
    R: Read + std::io::Seek,
{
    let mut entries = Vec::new();
    let mut seen_targets = HashSet::new();
    let mut found_site_payload = false;

    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }

        let name = entry.name().to_string();
        if name == "data.json" {
            continue;
        }

        let relative = sanitized_archive_path(&name)?;
        let Some(root) = relative
            .components()
            .next()
            .and_then(|component| component.as_os_str().to_str())
        else {
            anyhow::bail!("zip archive contains an empty entry path");
        };
        if root != site_name && root != "images" {
            anyhow::bail!("zip archive contains unexpected top-level entry: {name}");
        }
        if root == site_name {
            found_site_payload = true;
        }

        let target = data_dir.join(&relative);
        let target_key = relative.to_string_lossy().to_string();
        if !seen_targets.insert(target_key.clone()) {
            anyhow::bail!("zip archive contains duplicate entry path: {target_key}");
        }

        entries.push(ZipPayloadEntry { index, target });
    }

    if !found_site_payload {
        anyhow::bail!("zip archive is missing the {site_name}/ payload");
    }

    Ok(entries)
}

fn extract_zip_payload<R>(
    archive: &mut zip::ZipArchive<R>,
    entries: &[ZipPayloadEntry],
) -> Result<()>
where
    R: Read + std::io::Seek,
{
    for entry in entries {
        let mut zip_entry = archive.by_index(entry.index)?;
        if let Some(parent) = entry.target.parent() {
            stdfs::create_dir_all(parent)?;
        }
        let mut output = stdfs::File::create(&entry.target).with_context(|| {
            format!("failed to create extracted file {}", entry.target.display())
        })?;
        std::io::copy(&mut zip_entry, &mut output)
            .with_context(|| format!("failed to extract {}", entry.target.display()))?;
    }
    Ok(())
}

fn sanitized_archive_path(name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => clean.push(part),
            std::path::Component::CurDir => {}
            _ => anyhow::bail!("unsupported zip entry path: {name}"),
        }
    }
    if clean.as_os_str().is_empty() {
        anyhow::bail!("unsupported zip entry path: {name}");
    }
    Ok(clean)
}

#[derive(Debug)]
struct ZipPayloadEntry {
    index: usize,
    target: PathBuf,
}

fn normalize_zip_metadata(mut metadata: ZipImportMetadata) -> Result<ZipImportMetadata> {
    metadata.site_name = metadata.site_name.trim().to_string();
    if metadata.site_name.is_empty() {
        anyhow::bail!("zip data.json is missing site_name");
    }
    if !is_simple_path_segment(&metadata.site_name) {
        anyhow::bail!(
            "zip data.json has an invalid site_name: {}",
            metadata.site_name
        );
    }

    let mut images = BTreeMap::new();
    for (logical_name, hash) in metadata.images {
        let logical_name = logical_name.trim();
        let hash = hash.trim();
        validate_zip_image_entry(logical_name, hash)?;
        images.insert(logical_name.to_string(), hash.to_string());
    }
    metadata.images = images;
    Ok(metadata)
}

fn validate_zip_image_entry(logical_name: &str, hash: &str) -> Result<()> {
    if logical_name.is_empty() {
        anyhow::bail!("zip data.json contains an empty image logical name");
    }
    if hash.is_empty() {
        anyhow::bail!("zip data.json contains an empty image hash for {logical_name}");
    }
    if !is_simple_path_segment(logical_name) {
        anyhow::bail!("zip data.json contains an invalid image logical name: {logical_name}");
    }
    if !hash
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!("zip data.json contains an invalid image hash for {logical_name}");
    }
    Ok(())
}

fn is_simple_path_segment(value: &str) -> bool {
    if value.trim().is_empty() {
        return false;
    }

    let mut components = Path::new(value).components();
    matches!(components.next(), Some(std::path::Component::Normal(part)) if components.next().is_none() && part == value)
}

fn queued_upload_path(base_dir: &Path, request_id: &str, extension: &str) -> Result<PathBuf> {
    if !is_simple_path_segment(request_id) {
        anyhow::bail!("invalid request_id for queued upload path: {request_id}");
    }
    if extension.is_empty() || !extension.chars().all(|c| c.is_ascii_alphanumeric()) {
        anyhow::bail!("invalid upload extension: {extension}");
    }

    Ok(base_dir.join(format!("{request_id}.{extension}")))
}

fn import_site_records_from_tree(store: &Store, site_dir: &Path, site_name: &str) -> Result<usize> {
    if !site_dir.exists() {
        anyhow::bail!("zip archive is missing the {site_name}/ payload");
    }

    let mut records = Vec::new();
    for entry in stdfs::read_dir(site_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let work_key = entry.file_name().to_string_lossy().to_string();
        let raw_path = entry.path().join("raw").join("raw.json");
        let alt_path = entry.path().join("data.json");
        let payload_path = if raw_path.exists() {
            raw_path
        } else if alt_path.exists() {
            alt_path
        } else {
            continue;
        };

        let raw_json: Value = serde_json::from_reader(
            stdfs::File::open(&payload_path)
                .with_context(|| format!("failed to open {}", payload_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", payload_path.display()))?;
        let record = work_record_from_json(site_name, &work_key, raw_json)?;
        records.push(record);
    }

    if records.is_empty() {
        anyhow::bail!("zip archive does not contain any work payloads in {site_name}/");
    }

    let count = records.len();
    for record in records {
        store.upsert_work(&record)?;
    }
    Ok(count)
}

fn work_record_from_json(site: &str, work_key: &str, raw_json: Value) -> Result<WorkRecord> {
    let payload: ZipWorkPayload =
        serde_json::from_value(raw_json.clone()).context("invalid zip work payload")?;
    if payload.episodes.is_empty() {
        anyhow::bail!("zip work payload {work_key} is missing episodes");
    }

    Ok(WorkRecord {
        site: site.to_string(),
        work_key: work_key.to_string(),
        title: payload.title,
        author: payload.author,
        author_id: payload.author_id,
        author_url: payload.author_url,
        r#type: payload.work_type,
        serialization: payload.serialization,
        caption: payload.caption,
        create_date: payload.create_date,
        update_date: payload.update_date,
        raw_json,
    })
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ZipWorkPayload {
    #[serde(default)]
    version: i64,
    #[serde(default, rename = "get_date")]
    get_date: String,
    title: String,
    id: String,
    nid: String,
    url: String,
    author: String,
    #[serde(default)]
    author_id: Option<String>,
    #[serde(default)]
    author_url: Option<String>,
    caption: String,
    #[serde(default, rename = "total_episodes")]
    total_episodes: i64,
    #[serde(default, rename = "all_episodes")]
    all_episodes: i64,
    #[serde(default, rename = "total_characters")]
    total_characters: i64,
    #[serde(default, rename = "all_characters")]
    all_characters: i64,
    #[serde(default, rename = "type")]
    work_type: String,
    serialization: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, rename = "all_tags")]
    all_tags: Vec<String>,
    #[serde(default, rename = "createDate", alias = "create_date")]
    create_date: String,
    #[serde(default, rename = "updateDate", alias = "update_date")]
    update_date: String,
    #[serde(default)]
    episodes: BTreeMap<String, ZipEpisodePayload>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ZipEpisodePayload {
    id: String,
    #[serde(default)]
    chapter: Option<String>,
    title: String,
    #[serde(rename = "textCount")]
    text_count: i64,
    #[serde(default)]
    tags: Vec<String>,
    introduction: String,
    text: String,
    postscript: String,
    #[serde(default, rename = "createDate", alias = "create_date")]
    create_date: String,
    #[serde(default, rename = "updateDate", alias = "update_date")]
    update_date: String,
}

fn merge_zip_images(store: &Store, metadata: &ZipImportMetadata) -> Result<()> {
    let mut images = Vec::with_capacity(metadata.images.len());
    for (logical_name, hash) in &metadata.images {
        let ext = Path::new(logical_name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let kind = if logical_name.to_ascii_lowercase().contains("cover") {
            "cover"
        } else {
            "image"
        };
        images.push(ImageRecord {
            logical_name: logical_name.clone(),
            hash: hash.clone(),
            ext,
            kind: kind.to_string(),
        });
    }
    for image in images {
        store.upsert_image(&image)?;
    }
    Ok(())
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

fn choose_available_account_name(
    store: &Store,
    site: &str,
    preferred_name: &str,
) -> Result<String> {
    let mut candidate = preferred_name.to_string();
    let mut counter = 1;
    while store.get_account(site, &candidate)?.is_some() {
        candidate = format!("{preferred_name}_{counter}");
        counter += 1;
    }
    Ok(candidate)
}

fn normalize_account_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let file_name = Path::new(trimmed)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(trimmed)
        .trim();
    if file_name.is_empty() {
        return None;
    }
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name)
        .trim();
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

fn account_file_name(name: &str) -> String {
    format!("{name}.json")
}

fn rewrite_cookie_site_mirror(
    config: &AppConfig,
    site: &str,
    accounts: &[AccountRecord],
) -> Result<()> {
    let dir = config.cookie_dir_path(site);
    stdfs::create_dir_all(&dir)?;
    for entry in stdfs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
        {
            stdfs::remove_file(entry.path())?;
        }
    }

    let mut active_json = None;
    for account in accounts {
        let json = serde_json::to_string_pretty(&account.account)?;
        stdfs::write(dir.join(account_file_name(&account.name)), &json)?;
        if account.active {
            active_json = Some(json);
        }
    }
    if let Some(json) = active_json {
        stdfs::write(dir.join("login.json"), json)?;
    }
    Ok(())
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

async fn recover_queued_runtime_state(state: &RuntimeState) -> Result<()> {
    let recovered = {
        let store = state.store.lock().await;
        store.requeue_running_tasks(&now_string())?
    };
    if recovered > 0 {
        warn!(
            count = recovered,
            "re-queued tasks left in running state from a previous process"
        );
    }
    Ok(())
}

async fn sync_queue_state(state: &RuntimeState) -> Result<()> {
    let task_state = {
        let store = state.store.lock().await;
        store.task_state()?
    };
    let payload = serde_json::to_vec_pretty(&task_state)?;
    fs::write(state.config.queue_task_json_path(), payload)
        .await
        .context("failed to write queue/task.json")?;
    Ok(())
}

async fn worker_loop(state: RuntimeState) {
    loop {
        let task = match claim_next_task(&state).await {
            Ok(task) => task,
            Err(err) => {
                error!(error = %err, "failed to claim queued task");
                state.worker_notify.notified().await;
                continue;
            }
        };

        let Some(task) = task else {
            state.worker_notify.notified().await;
            continue;
        };

        if let Err(err) = sync_queue_state(&state).await {
            error!(task_id = task.id, request_id = %task.request_id, error = %err, "failed to persist queue/task.json for running task");
        }

        if let Err(err) = execute_queued_task(&state, task.id, &task).await {
            error!(task_id = task.id, request_id = %task.request_id, error = %err, "queued task failed with runtime error");
            let store = state.store.lock().await;
            if let Err(mark_err) = store.mark_task_status(
                task.id,
                TaskStatus::Failed,
                Some(err.to_string()),
                &now_string(),
            ) {
                error!(task_id = task.id, request_id = %task.request_id, error = %mark_err, "failed to persist failed task status");
            }
        }

        if let Err(err) = sync_queue_state(&state).await {
            error!(task_id = task.id, request_id = %task.request_id, error = %err, "failed to persist queue/task.json after task completion");
        }
    }
}

async fn claim_next_task(state: &RuntimeState) -> Result<Option<TaskRecord>> {
    let store = state.store.lock().await;
    store.claim_next_queued_task(&now_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::{Cursor, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test-output")
            .join(format!("{name}-{unique}"));
        stdfs::create_dir_all(&path).unwrap();
        path
    }

    fn sample_account(site: &str, name: &str, active: bool) -> AccountRecord {
        AccountRecord {
            site: site.to_string(),
            name: name.to_string(),
            display_name: Some(format!("{name} display")),
            account: AccountFile {
                cookies: json!({"session": name}),
                user_agent: Some(format!("{name}-ua")),
                display_name: Some(format!("{name} display")),
            },
            active,
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    fn zip_archive(entries: &[(&str, &str)]) -> zip::ZipArchive<Cursor<Vec<u8>>> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for (name, contents) in entries {
            writer.start_file(name, options).expect("start file");
            writer.write_all(contents.as_bytes()).expect("write file");
        }
        let cursor = writer.finish().expect("finish zip");
        zip::ZipArchive::new(cursor).expect("open zip")
    }

    fn request_with_actions() -> RequestData {
        RequestData {
            request_id: "req-1".to_string(),
            repair: Some("repair-site".to_string()),
            login: Some("login-site".to_string()),
            update: Some("update-site".to_string()),
            re_download: Some("re-download-site".to_string()),
            convert: Some("convert-site".to_string()),
            add: Some("https://example.com/work".to_string()),
            ..RequestData::default()
        }
    }

    #[test]
    fn resolve_request_action_uses_expected_priority() {
        let req = request_with_actions();
        let action = resolve_request_action(&req).expect("action");
        assert_eq!(action, ("repair".to_string(), "repair-site".to_string()));
    }

    #[test]
    fn resolve_request_action_prefers_pdf_conversion() {
        let req = RequestData {
            request_id: "req-1".to_string(),
            pdf_path: Some("input.pdf".to_string()),
            repair: Some("repair-site".to_string()),
            ..RequestData::default()
        };
        let action = resolve_request_action(&req).expect("action");
        assert_eq!(action, ("convert".to_string(), "narou".to_string()));
    }

    #[test]
    fn resolve_request_action_accepts_zip_only_upload() {
        let req = RequestData {
            request_id: "req-zip".to_string(),
            zip_name: Some("legacy.zip".to_string()),
            ..RequestData::default()
        };
        let action = resolve_request_action(&req).expect("action");
        assert_eq!(action, ("convert".to_string(), "zip".to_string()));
    }

    #[test]
    fn normalize_task_input_trims_and_drops_empty_values() {
        let input = TaskInput {
            request_id: Some("  req-1  ".to_string()),
            pdf_name: Some("  sample.pdf  ".to_string()),
            zip_name: Some("   ".to_string()),
            add: Some("\nhttps://example.com/work\t".to_string()),
            ..TaskInput::default()
        };
        let normalized = normalize_task_input(input);
        assert_eq!(normalized.request_id.as_deref(), Some("req-1"));
        assert_eq!(normalized.pdf_name.as_deref(), Some("sample.pdf"));
        assert_eq!(normalized.zip_name, None);
        assert_eq!(normalized.add.as_deref(), Some("https://example.com/work"));
    }

    #[test]
    fn parse_task_input_bytes_discards_client_supplied_request_id() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        let body = Bytes::from_static(br#"{"request_id":"../escape","update":"narou"}"#);

        let input = parse_task_input_bytes(&headers, &body).expect("request should parse");

        assert_eq!(input.request_id, None);
        assert_eq!(input.update.as_deref(), Some("narou"));
    }

    #[test]
    fn should_import_zip_request_only_for_zip_only_payloads() {
        let zip_only = RequestData {
            request_id: "req-zip".to_string(),
            zip_name: Some("legacy.zip".to_string()),
            ..RequestData::default()
        };
        assert!(should_import_zip_request(&zip_only));

        let with_action = RequestData {
            add: Some("https://example.com/work".to_string()),
            ..zip_only.clone()
        };
        assert!(!should_import_zip_request(&with_action));
    }

    #[tokio::test]
    async fn persist_uploaded_file_rejects_invalid_request_id() {
        let root = test_dir("runtime-invalid-request-id");
        let config = AppConfig {
            pdf_dir: root.to_string_lossy().to_string(),
            ..AppConfig::default()
        };

        let err = persist_uploaded_file(&config, "../escape", "zip", Bytes::from_static(b"zip"))
            .await
            .expect_err("path traversal request_id should fail");

        assert!(err.contains("invalid request_id"));
        stdfs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn queued_zip_path_rejects_invalid_request_id() {
        let request = RequestData {
            request_id: "../escape".to_string(),
            zip_name: Some("upload.zip".to_string()),
            ..RequestData::default()
        };

        let err = queued_zip_path(&request, "pdf").expect_err("invalid request_id should fail");
        assert!(err.to_string().contains("invalid request_id"));
    }

    #[test]
    fn sanitized_archive_path_rejects_parent_segments() {
        assert!(sanitized_archive_path("../outside.txt").is_err());
        assert_eq!(
            sanitized_archive_path("narou\\work\\raw\\raw.json")
                .expect("path")
                .to_string_lossy(),
            "narou\\work\\raw\\raw.json"
        );
    }

    #[test]
    fn normalize_zip_metadata_trims_and_validates() {
        let mut images = BTreeMap::new();
        images.insert(" cover.png ".to_string(), " abc123 ".to_string());
        let metadata = ZipImportMetadata {
            site_name: " narou ".to_string(),
            images,
        };
        let normalized = normalize_zip_metadata(metadata).expect("metadata");
        assert_eq!(normalized.site_name, "narou");
        assert_eq!(
            normalized.images.get("cover.png").map(String::as_str),
            Some("abc123")
        );
    }

    #[test]
    fn normalize_zip_metadata_rejects_path_traversal() {
        let metadata = ZipImportMetadata {
            site_name: "../narou".to_string(),
            images: BTreeMap::new(),
        };
        assert!(normalize_zip_metadata(metadata).is_err());
    }

    #[test]
    fn collect_zip_payload_entries_rejects_unexpected_roots() {
        let mut archive = zip_archive(&[
            ("data.json", r#"{"site_name":"narou","images":{}}"#),
            ("evil/readme.txt", "nope"),
            ("narou/work/raw/raw.json", "{}"),
        ]);
        let err = collect_zip_payload_entries(&mut archive, Path::new("data"), "narou")
            .expect_err("unexpected root should fail");
        assert!(err.to_string().contains("unexpected top-level entry"));
    }

    #[test]
    fn rewrite_cookie_site_mirror_tracks_db_accounts_and_active_login() {
        let root = test_dir("runtime-account-mirror");
        let cookie_root = root.join("cookie");
        let site_dir = cookie_root.join("pixiv");
        stdfs::create_dir_all(&site_dir).unwrap();
        stdfs::write(site_dir.join("stale.json"), "{}").unwrap();

        let config = AppConfig {
            cookie_dir: cookie_root.to_string_lossy().to_string(),
            ..AppConfig::default()
        };
        let accounts = vec![
            sample_account("pixiv", "alpha", false),
            sample_account("pixiv", "beta", true),
        ];

        rewrite_cookie_site_mirror(&config, "pixiv", &accounts).expect("rewrite mirrors");

        assert!(!site_dir.join("stale.json").exists());
        assert!(site_dir.join("alpha.json").exists());
        assert!(site_dir.join("beta.json").exists());
        let active_named = stdfs::read_to_string(site_dir.join("beta.json")).unwrap();
        let active_login = stdfs::read_to_string(site_dir.join("login.json")).unwrap();
        assert_eq!(active_named, active_login);

        stdfs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn auto_update_task_targets_update_all() {
        let task = auto_update_task();
        assert_eq!(task.action, "update");
        assert_eq!(task.param, "all");
        assert_eq!(task.request.request_id, task.request_id);
        assert_eq!(task.request.update.as_deref(), Some("all"));
    }

    #[test]
    fn summarize_site_results_marks_partial_failures_as_failed() {
        let results = vec![
            crate::sites::SiteActionResult::success(
                crate::sites::SiteId::Narou,
                "update",
                "narou update completed",
            ),
            crate::sites::SiteActionResult::failed(
                crate::sites::SiteId::Pixiv,
                "update",
                "pixiv login expired",
            ),
        ];

        let (status, error) = summarize_site_results(&results);

        assert_eq!(status, TaskStatus::Failed);
        let error = error.expect("partial failure should keep an error summary");
        assert!(error.contains("failed sites: pixiv (update) pixiv login expired"));
        assert!(error.contains("succeeded sites: narou (update) narou update completed"));
    }

    #[test]
    fn summarize_site_results_marks_all_successes_as_succeeded() {
        let results = vec![
            crate::sites::SiteActionResult::success(
                crate::sites::SiteId::Narou,
                "convert",
                "narou convert completed",
            ),
            crate::sites::SiteActionResult::success(
                crate::sites::SiteId::Pixiv,
                "convert",
                "pixiv convert completed",
            ),
        ];

        let (status, error) = summarize_site_results(&results);

        assert_eq!(status, TaskStatus::Succeeded);
        assert_eq!(error, None);
    }

    #[test]
    fn collect_zip_payload_entries_requires_site_payload() {
        let mut archive = zip_archive(&[
            ("data.json", r#"{"site_name":"narou","images":{}}"#),
            ("images/cover.png", "img"),
        ]);
        let err = collect_zip_payload_entries(&mut archive, Path::new("data"), "narou")
            .expect_err("missing site payload should fail");
        assert!(err.to_string().contains("missing the narou/ payload"));
    }

    #[test]
    fn work_record_from_json_requires_episodes() {
        let raw_json = json!({
            "version": 0,
            "get_date": "2026-01-01",
            "title": "Sample",
            "id": "1",
            "nid": "n1",
            "url": "https://example.com",
            "author": "Author",
            "caption": "Caption",
            "total_episodes": 1,
            "all_episodes": 1,
            "total_characters": 1,
            "all_characters": 1,
            "type": "novel",
            "serialization": "短編",
            "tags": [],
            "all_tags": [],
            "createDate": "2026-01-01",
            "updateDate": "2026-01-01",
            "episodes": {}
        });
        let err = work_record_from_json("narou", "work", raw_json)
            .expect_err("missing episodes should fail");
        assert!(err.to_string().contains("missing episodes"));
    }

    #[tokio::test]
    async fn ensure_html_utf8_charset_updates_html_without_charset() {
        let mut response = Response::new(axum::body::Body::empty());
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/html"),
        );

        let response = ensure_html_utf8_charset(response).await;

        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/html; charset=utf-8")
        );
    }

    #[tokio::test]
    async fn ensure_html_utf8_charset_leaves_json_content_type_unchanged() {
        let mut response = Response::new(axum::body::Body::empty());
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let response = ensure_html_utf8_charset(response).await;

        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    }
}
