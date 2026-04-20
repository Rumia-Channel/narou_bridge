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
use axum::http::{HeaderMap, StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum_extra::extract::Multipart;
use bytes::Bytes;
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs as stdfs;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{Mutex, Notify};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

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
        .with_state(state.clone());

    let addr: SocketAddr = state
        .config
        .bind_addr
        .parse()
        .context("invalid bind address")?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(bind_addr = %state.config.bind_addr, host_name = %state.config.host_name, "http server listening");
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
    let account_name = payload.name.unwrap_or_else(random_account_name);
    let saved_name = if account_name.ends_with(".json") {
        account_name
    } else {
        format!("{account_name}.json")
    };
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
    let file_name = if account.ends_with(".json") {
        account
    } else {
        format!("{account}.json")
    };
    let path = state.config.cookie_dir_path(&site).join(&file_name);
    match fs::remove_file(&path).await {
        Ok(_) => Json(json!({"status": "success", "message": format!("Account {file_name} deleted successfully")})).into_response(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => create_error(StatusCode::NOT_FOUND, "Account not found".to_string()),
        Err(err) => create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
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
    let name = if account.ends_with(".json") {
        account
    } else {
        format!("{account}.json")
    };
    let dir = state.config.cookie_dir_path(&site);
    let source = dir.join(&name);
    let target = dir.join("login.json");
    if !source.exists() {
        return create_error(StatusCode::NOT_FOUND, format!("Account {name} not found"));
    }
    if target.exists() {
        let backup = dir.join(format!(
            "login.json.backup.{}",
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        ));
        let _ = fs::rename(&target, &backup).await;
    }
    if let Err(err) = fs::rename(&source, &target).await {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    Json(json!({"status": "success", "message": format!("Switched to account {name}"), "site": site, "account": name})).into_response()
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
    let old_name = if old_name.ends_with(".json") {
        old_name
    } else {
        format!("{old_name}.json")
    };
    let new_name = if new_name_raw.ends_with(".json") {
        new_name_raw
    } else {
        format!("{new_name_raw}.json")
    };
    if old_name == new_name {
        return Json(json!({"status": "success", "message": "Account name unchanged"}))
            .into_response();
    }
    let dir = state.config.cookie_dir_path(&site);
    let old_path = dir.join(&old_name);
    let new_path = dir.join(&new_name);
    if !old_path.exists() {
        return create_error(StatusCode::NOT_FOUND, "Account not found".to_string());
    }
    if new_path.exists() {
        return create_error(
            StatusCode::CONFLICT,
            format!("Account name '{new_name}' already exists"),
        );
    }
    if let Err(err) = fs::rename(&old_path, &new_path).await {
        return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    Json(json!({"status": "success", "message": format!("Account renamed from {old_name} to {new_name}"), "site": site, "old_name": old_name, "new_name": new_name})).into_response()
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

    let store = state.store.lock().await;
    match store.enqueue_task(&task) {
        Ok(_) => {}
        Err(err) => {
            return create_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
        }
    }

    drop(store);
    if let Err(err) = sync_queue_state(&state).await {
        warn!(request_id = %request_id, error = %err, "failed to persist queue/task.json after enqueue");
    }
    state.worker_notify.notify_one();

    Json(json!({"status": "queued", "request_id": request_id})).into_response()
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
    let has_success = results.iter().any(|r| r.status == "success");
    let has_failed = results.iter().any(|r| r.status == "failed");
    let status = if has_success {
        TaskStatus::Succeeded
    } else if has_failed {
        TaskStatus::Failed
    } else {
        TaskStatus::Skipped
    };
    let error = if has_failed {
        Some(
            results
                .iter()
                .find(|r| r.status == "failed")
                .map(|r| r.message.clone())
                .unwrap_or_default(),
        )
    } else {
        None
    };
    let _ = store.mark_task_status(task_id, status, error, &now_string());
    Ok(())
}

fn create_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({"status": "error", "message": message}))).into_response()
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
            .map_err(|err| err.to_string());
    }

    if content_type.contains("application/x-www-form-urlencoded") {
        return serde_urlencoded::from_bytes(body)
            .map(normalize_task_input)
            .map_err(|err| err.to_string());
    }

    serde_json::from_slice(body)
        .or_else(|_| serde_urlencoded::from_bytes(body))
        .map(normalize_task_input)
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
    let request_id = input.request_id.clone().unwrap_or_else(random_request_id);

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
    fs::create_dir_all(config.pdf_dir_path())
        .await
        .map_err(|err| err.to_string())?;
    let path = config
        .pdf_dir_path()
        .join(format!("{request_id}.{extension}"));
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
        queued_zip_path(request, pdf_dir).context("queued ZIP upload is missing from pdf/")?;
    let file = stdfs::File::open(&archive_path)
        .with_context(|| format!("failed to open zip archive {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("invalid zip {}", archive_path.display()))?;
    let metadata = read_zip_metadata(&mut archive)?;

    extract_zip_payload(&mut archive, Path::new(data_dir), &metadata.site_name)?;
    import_site_records_from_tree(
        store,
        &PathBuf::from(data_dir).join(&metadata.site_name),
        &metadata.site_name,
    )?;
    merge_zip_images(store, &metadata)?;
    renderer::refresh_image_manifests(store, data_dir)?;
    renderer::render_site_from_store(store, &metadata.site_name, data_dir, host_name)?;

    Ok(format!(
        "imported {}",
        request
            .zip_name
            .clone()
            .unwrap_or_else(|| metadata.site_name.clone())
    ))
}

fn queued_zip_path(request: &RequestData, pdf_dir: &str) -> Option<PathBuf> {
    request.zip_name.as_ref()?;
    let path = PathBuf::from(pdf_dir).join(format!("{}.zip", request.request_id));
    path.exists().then_some(path)
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
    if metadata.site_name.trim().is_empty() {
        anyhow::bail!("zip data.json is missing site_name");
    }
    Ok(metadata)
}

fn extract_zip_payload<R>(
    archive: &mut zip::ZipArchive<R>,
    data_dir: &Path,
    site_name: &str,
) -> Result<()>
where
    R: Read + std::io::Seek,
{
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
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
            continue;
        };
        if root != site_name && root != "images" {
            continue;
        }

        let target = data_dir.join(&relative);
        if let Some(parent) = target.parent() {
            stdfs::create_dir_all(parent)?;
        }
        let mut output = stdfs::File::create(&target)
            .with_context(|| format!("failed to create extracted file {}", target.display()))?;
        std::io::copy(&mut entry, &mut output)
            .with_context(|| format!("failed to extract {}", target.display()))?;
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
    Ok(clean)
}

fn import_site_records_from_tree(store: &Store, site_dir: &Path, site_name: &str) -> Result<usize> {
    if !site_dir.exists() {
        return Ok(0);
    }

    let mut count = 0;
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
        store.upsert_work(&work_record_from_json(site_name, &work_key, raw_json))?;
        count += 1;
    }
    Ok(count)
}

fn work_record_from_json(site: &str, work_key: &str, raw_json: Value) -> WorkRecord {
    WorkRecord {
        site: site.to_string(),
        work_key: work_key.to_string(),
        title: raw_json
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        author: raw_json
            .get("author")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        author_id: raw_json
            .get("author_id")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        author_url: raw_json
            .get("author_url")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        r#type: raw_json
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        serialization: raw_json
            .get("serialization")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        caption: raw_json
            .get("caption")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        create_date: raw_json
            .get("createDate")
            .and_then(|value| value.as_str())
            .or_else(|| raw_json.get("create_date").and_then(|value| value.as_str()))
            .unwrap_or_default()
            .to_string(),
        update_date: raw_json
            .get("updateDate")
            .and_then(|value| value.as_str())
            .or_else(|| raw_json.get("update_date").and_then(|value| value.as_str()))
            .unwrap_or_default()
            .to_string(),
        raw_json,
    }
}

fn merge_zip_images(store: &Store, metadata: &ZipImportMetadata) -> Result<()> {
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
        store.upsert_image(&ImageRecord {
            logical_name: logical_name.clone(),
            hash: hash.clone(),
            ext,
            kind: kind.to_string(),
        })?;
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

async fn unique_name_path(dir: &Path, name: &str) -> PathBuf {
    let mut candidate = dir.join(name);
    let mut counter = 1;
    while candidate.exists() {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(name);
        let ext = Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{s}"))
            .unwrap_or_default();
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
}
