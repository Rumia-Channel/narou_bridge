use axum::body::Body;
use axum::http::{Request, StatusCode, header::CONTENT_TYPE};
use http_body_util::BodyExt;
use narou_bridge::core::model::AppConfig;
use narou_bridge::core::registry::build_registry;
use narou_bridge::core::runtime::{build_app, build_runtime_state};
use narou_bridge::core::storage::Store;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

fn test_root(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let path = std::env::current_dir()
        .expect("current dir")
        .join("target")
        .join("test-output")
        .join(format!("{name}-{unique}"));
    std::fs::create_dir_all(&path).expect("create test root");
    path
}

fn test_config(root: &Path) -> AppConfig {
    let legacy_root = root.join("legacy-empty");
    std::fs::create_dir_all(&legacy_root).expect("create legacy root");
    AppConfig {
        data_dir: root.join("data").to_string_lossy().to_string(),
        cookie_dir: root.join("cookie").to_string_lossy().to_string(),
        queue_dir: root.join("queue").to_string_lossy().to_string(),
        pdf_dir: root.join("pdf").to_string_lossy().to_string(),
        log_dir: root.join("log").to_string_lossy().to_string(),
        db_path: root.join("narou_bridge.db").to_string_lossy().to_string(),
        archive_dir: root.join("archive").to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        host_name: "http://127.0.0.1:0".to_string(),
        auto_update: false,
        auto_update_interval: 0,
        legacy_root: Some(legacy_root.to_string_lossy().to_string()),
    }
}

async fn test_app(root: &Path) -> axum::Router {
    let config = test_config(root);
    let store = Store::open(config.db_path_buf()).expect("open store");
    let state = build_runtime_state(config, store, build_registry())
        .await
        .expect("build runtime state");
    build_app(state)
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&body).expect("json body")
}

#[tokio::test]
async fn key_http_endpoints_respond_in_process() {
    let root = test_root("e2e-http");
    let app = test_app(&root).await;

    let queue_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/")
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "add=https%3A%2F%2Fwww.pixiv.net%2Fnovel%2Fshow.php%3Fid%3D12345",
                ))
                .expect("queue request"),
        )
        .await
        .expect("queue response");
    assert_eq!(queue_response.status(), StatusCode::OK);
    let queue_json = response_json(queue_response).await;
    assert_eq!(
        queue_json.get("status").and_then(Value::as_str),
        Some("queued")
    );
    assert!(
        queue_json
            .get("request_id")
            .and_then(Value::as_str)
            .is_some_and(|request_id| !request_id.is_empty())
    );

    let migrate_query = serde_urlencoded::to_string([(
        "source_root",
        root.join("legacy-empty").to_string_lossy().to_string(),
    )])
    .expect("migrate query");
    let migrate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/migrate?{migrate_query}"))
                .body(Body::empty())
                .expect("migrate request"),
        )
        .await
        .expect("migrate response");
    assert_eq!(migrate_response.status(), StatusCode::OK);
    let migrate_json = response_json(migrate_response).await;
    assert_eq!(
        migrate_json.get("status").and_then(Value::as_str),
        Some("success")
    );
    assert!(migrate_json.get("summary").is_some());

    let reader_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/reader/")
                .body(Body::empty())
                .expect("reader request"),
        )
        .await
        .expect("reader response");
    assert_eq!(reader_response.status(), StatusCode::OK);

    let account_response = app
        .oneshot(
            Request::builder()
                .uri("/api/account?site=pixiv")
                .body(Body::empty())
                .expect("account request"),
        )
        .await
        .expect("account response");
    assert_eq!(account_response.status(), StatusCode::OK);
    let account_json = response_json(account_response).await;
    assert_eq!(
        account_json.get("site").and_then(Value::as_str),
        Some("pixiv")
    );
    assert!(
        account_json
            .get("accounts")
            .and_then(Value::as_array)
            .is_some()
    );
}
