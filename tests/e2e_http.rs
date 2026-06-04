use axum::body::Body;
use axum::http::{
    Request, StatusCode,
    header::{CONTENT_TYPE, ETAG},
};
use http_body_util::BodyExt;
use narou_bridge::core::model::{AppConfig, ImageRecord, WorkRecord};
use narou_bridge::core::registry::build_registry;
use narou_bridge::core::runtime::{build_app, build_runtime_state};
use narou_bridge::core::storage::Store;
use serde_json::{Value, json};
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
        img_url: String::new(),
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

async fn response_text(response: axum::response::Response) -> String {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    String::from_utf8(body.to_vec()).expect("utf-8 body")
}

fn sample_work_record() -> WorkRecord {
    let raw_json = json!({
        "version": 0,
        "get_date": "2025-01-01T00:00:00Z",
        "title": "DB-backed work",
        "id": "n123",
        "nid": "n123",
        "url": "https://example.com/n123",
        "author": "author",
        "author_id": "author-1",
        "author_url": "https://example.com/author-1",
        "caption": "caption",
        "total_episodes": 1,
        "all_episodes": 1,
        "total_characters": 10,
        "all_characters": 10,
        "type": "novel",
        "serialization": "短編",
        "tags": ["tag"],
        "all_tags": ["tag"],
        "createDate": "2025-01-01T00:00:00Z",
        "updateDate": "2025-01-02T00:00:00Z",
        "episodes": {
            "1": {
                "id": "1",
                "chapter": null,
                "title": "Episode 1",
                "textCount": 10,
                "tags": [],
                "introduction": "",
                "text": "text",
                "postscript": "",
                "createDate": "2025-01-01T00:00:00Z",
                "updateDate": "2025-01-02T00:00:00Z"
            }
        }
    });

    WorkRecord {
        site: "pixiv".to_string(),
        work_key: "n123".to_string(),
        title: "DB-backed work".to_string(),
        author: "author".to_string(),
        author_id: Some("author-1".to_string()),
        author_url: Some("https://example.com/author-1".to_string()),
        r#type: "novel".to_string(),
        serialization: "短編".to_string(),
        caption: "caption".to_string(),
        create_date: "2025-01-01T00:00:00Z".to_string(),
        update_date: "2025-01-02T00:00:00Z".to_string(),
        raw_json,
    }
}

fn sample_library_work_record(
    work_key: &str,
    title: &str,
    author: &str,
    update_date: &str,
) -> WorkRecord {
    let mut work = sample_work_record();
    work.work_key = work_key.to_string();
    work.title = title.to_string();
    work.author = author.to_string();
    work.author_id = Some(format!("{author}-id"));
    work.author_url = Some(format!("https://example.com/users/{author}"));
    work.update_date = update_date.to_string();

    let raw = work.raw_json.as_object_mut().expect("raw object");
    raw.insert("title".to_string(), json!(title));
    raw.insert("id".to_string(), json!(work_key));
    raw.insert("nid".to_string(), json!(work_key));
    raw.insert(
        "url".to_string(),
        json!(format!("https://example.com/{work_key}")),
    );
    raw.insert("author".to_string(), json!(author));
    raw.insert("author_id".to_string(), json!(format!("{author}-id")));
    raw.insert(
        "author_url".to_string(),
        json!(format!("https://example.com/users/{author}")),
    );
    raw.insert("updateDate".to_string(), json!(update_date));

    work
}

fn write_legacy_data_root(data_root: &Path) {
    std::fs::create_dir_all(data_root.join("pixiv").join("n123").join("raw"))
        .expect("create work raw dir");
    std::fs::create_dir_all(data_root.join("images")).expect("create images dir");
    std::fs::create_dir_all(data_root.join("pixiv").join("snapshots").join("illust_ids"))
        .expect("create snapshots dir");

    let raw = serde_json::to_vec_pretty(&sample_work_record().raw_json).expect("raw json");
    std::fs::write(
        data_root
            .join("pixiv")
            .join("n123")
            .join("raw")
            .join("raw.json"),
        raw,
    )
    .expect("write raw json");
    std::fs::write(
        data_root.join("images").join("database.json"),
        br#"{"pixiv_n123_cover.jpg":"deadbeefcafebabe"}"#,
    )
    .expect("write database json");
    std::fs::write(
        data_root.join("images").join("cover.json"),
        br#"{"pixiv_n123_cover.jpg":"deadbeefcafebabe"}"#,
    )
    .expect("write cover json");
    std::fs::write(
        data_root.join("images").join("deadbeefcafebabe.jpg"),
        b"jpg",
    )
    .expect("write image file");
    std::fs::write(
        data_root.join("pixiv").join("user.json"),
        br#"{"version":3,"12345":{"novel":"enable","comic":"enable","illust_ids_snapshot_hash":"abc"}}"#,
    )
    .expect("write user json");
    std::fs::write(
        data_root
            .join("pixiv")
            .join("snapshots")
            .join("illust_ids")
            .join("12345.json"),
        br#"["10","20"]"#,
    )
    .expect("write snapshot json");
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
    assert!(root.join("queue").join("task.json").exists());

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

#[tokio::test]
async fn db_backed_json_routes_respond_without_persisted_json_files() {
    let root = test_root("e2e-http-db-json");
    let config = test_config(&root);
    let seed_store = Store::open(config.db_path_buf()).expect("open seed store");
    seed_store
        .upsert_work(&sample_work_record())
        .expect("upsert work");
    seed_store
        .upsert_image(&ImageRecord {
            logical_name: "pixiv_n123_cover.jpg".to_string(),
            hash: "deadbeefcafebabe".to_string(),
            ext: "jpg".to_string(),
            kind: "cover".to_string(),
        })
        .expect("upsert image");

    let state = build_runtime_state(
        config.clone(),
        Store::open(config.db_path_buf()).expect("reopen store"),
        build_registry(),
    )
    .await
    .expect("build runtime state");
    let app = build_app(state);

    assert!(!root.join("data").join("pixiv").join("index.json").exists());
    assert!(!root.join("data").join("images").join("cover.json").exists());
    assert!(
        !root
            .join("data")
            .join("images")
            .join("database.json")
            .exists()
    );

    let index_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pixiv/index.json")
                .body(Body::empty())
                .expect("index request"),
        )
        .await
        .expect("index response");
    assert_eq!(index_response.status(), StatusCode::OK);
    assert!(index_response.headers().contains_key(ETAG));
    let index_json = response_json(index_response).await;
    assert_eq!(index_json["n123"]["title"].as_str(), Some("DB-backed work"));

    let cover_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/images/cover.json")
                .body(Body::empty())
                .expect("cover request"),
        )
        .await
        .expect("cover response");
    assert_eq!(cover_response.status(), StatusCode::OK);
    assert!(cover_response.headers().contains_key(ETAG));
    let cover_json = response_json(cover_response).await;
    assert_eq!(
        cover_json["pixiv_n123_cover.jpg"].as_str(),
        Some("deadbeefcafebabe")
    );

    let database_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/images/database.json")
                .body(Body::empty())
                .expect("database request"),
        )
        .await
        .expect("database response");
    assert_eq!(database_response.status(), StatusCode::OK);
    let database_json = response_json(database_response).await;
    assert_eq!(
        database_json["pixiv_n123_cover.jpg"].as_str(),
        Some("deadbeefcafebabe")
    );

    let raw_response = app
        .oneshot(
            Request::builder()
                .uri("/pixiv/n123/raw/raw.json")
                .body(Body::empty())
                .expect("raw request"),
        )
        .await
        .expect("raw response");
    assert_eq!(raw_response.status(), StatusCode::OK);
    assert!(raw_response.headers().contains_key(ETAG));
    let raw_json = response_json(raw_response).await;
    assert_eq!(raw_json["title"].as_str(), Some("DB-backed work"));
    assert!(
        !root
            .join("data")
            .join("pixiv")
            .join("n123")
            .join("raw")
            .join("raw.json")
            .exists()
    );
}

#[tokio::test]
async fn library_works_api_pages_db_summaries_without_raw_json() {
    let root = test_root("e2e-http-library-works");
    let config = test_config(&root);
    let seed_store = Store::open(config.db_path_buf()).expect("open seed store");
    seed_store
        .upsert_work(&sample_library_work_record(
            "n101",
            "Alpha Library",
            "Zeta",
            "2025-01-01T00:00:00Z",
        ))
        .expect("upsert alpha work");
    seed_store
        .upsert_work(&sample_library_work_record(
            "n102",
            "Beta Library",
            "Alpha",
            "2025-01-02T00:00:00Z",
        ))
        .expect("upsert beta work");
    seed_store
        .upsert_work(&sample_library_work_record(
            "n103",
            "Gamma Other",
            "Beta",
            "2025-01-03T00:00:00Z",
        ))
        .expect("upsert gamma work");

    let state = build_runtime_state(
        config.clone(),
        Store::open(config.db_path_buf()).expect("reopen store"),
        build_registry(),
    )
    .await
    .expect("runtime state");
    let app = build_app(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/library/works?site=pixiv&search=Library&sort=title_asc&limit=1&offset=1")
                .body(Body::empty())
                .expect("library works request"),
        )
        .await
        .expect("library works response");
    assert_eq!(response.status(), StatusCode::OK);
    let parsed = response_json(response).await;

    assert_eq!(parsed["site"].as_str(), Some("pixiv"));
    assert_eq!(parsed["total"].as_u64(), Some(2));
    assert_eq!(parsed["limit"].as_u64(), Some(1));
    assert_eq!(parsed["offset"].as_u64(), Some(1));
    let works = parsed["works"].as_array().expect("works array");
    assert_eq!(works.len(), 1);
    assert_eq!(works[0]["work_key"].as_str(), Some("n102"));
    assert_eq!(works[0]["title"].as_str(), Some("Beta Library"));
    assert!(works[0].get("raw_json").is_none());

    let filtered_response = app
        .oneshot(
            Request::builder()
                .uri("/api/library/works?site=pixiv&author_id=Alpha-id&type=novel&serialization=%E7%9F%AD%E7%B7%A8&limit=10")
                .body(Body::empty())
                .expect("filtered library works request"),
        )
        .await
        .expect("filtered library works response");
    assert_eq!(filtered_response.status(), StatusCode::OK);
    let filtered = response_json(filtered_response).await;
    assert_eq!(filtered["total"].as_u64(), Some(1));
    let filtered_works = filtered["works"].as_array().expect("filtered works array");
    assert_eq!(filtered_works[0]["work_key"].as_str(), Some("n102"));
}

#[tokio::test]
async fn build_runtime_state_bootstraps_empty_db_from_external_data_dir() {
    let root = test_root("e2e-http-external-data");
    let external_data_root = root.join("external-data");
    write_legacy_data_root(&external_data_root);

    let mut config = test_config(&root);
    config.data_dir = external_data_root.to_string_lossy().to_string();
    config.db_path = external_data_root
        .join("runtime.sqlite3")
        .to_string_lossy()
        .to_string();

    let state = build_runtime_state(
        config.clone(),
        Store::open(config.db_path_buf()).expect("open store"),
        build_registry(),
    )
    .await
    .expect("build runtime state");

    {
        let store = state.store.lock().await;
        assert_eq!(
            store
                .get_work("pixiv", "n123")
                .expect("get work")
                .map(|work| work.title),
            Some("DB-backed work".to_string())
        );
        assert_eq!(
            store
                .get_image("pixiv_n123_cover.jpg")
                .expect("get image")
                .map(|image| image.hash),
            Some("deadbeefcafebabe".to_string())
        );
        assert_eq!(
            store
                .list_site_document_records("pixiv", Some("tracked_users/"))
                .expect("list tracked users")
                .len(),
            2
        );
    }

    let app = build_app(state);
    let index_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pixiv/index.json")
                .body(Body::empty())
                .expect("index request"),
        )
        .await
        .expect("index response");
    assert_eq!(index_response.status(), StatusCode::OK);
    let index_json = response_json(index_response).await;
    assert_eq!(index_json["n123"]["title"].as_str(), Some("DB-backed work"));

    let raw_response = app
        .oneshot(
            Request::builder()
                .uri("/pixiv/n123/raw/raw.json")
                .body(Body::empty())
                .expect("raw request"),
        )
        .await
        .expect("raw response");
    assert_eq!(raw_response.status(), StatusCode::OK);
    let raw_json = response_json(raw_response).await;
    assert_eq!(raw_json["title"].as_str(), Some("DB-backed work"));
}

#[tokio::test]
async fn db_backed_html_routes_respond_without_persisted_work_html() {
    let root = test_root("e2e-http-db-html");
    let config = test_config(&root);
    let seed_store = Store::open(config.db_path_buf()).expect("open seed store");
    seed_store
        .upsert_work(&sample_work_record())
        .expect("upsert work");
    seed_store
        .upsert_image(&ImageRecord {
            logical_name: "pixiv_n123_cover.jpg".to_string(),
            hash: "deadbeefcafebabe".to_string(),
            ext: "jpg".to_string(),
            kind: "cover".to_string(),
        })
        .expect("upsert image");

    let state = build_runtime_state(
        config.clone(),
        Store::open(config.db_path_buf()).expect("reopen store"),
        build_registry(),
    )
    .await
    .expect("build runtime state");
    let app = build_app(state);

    assert!(
        !root
            .join("data")
            .join("pixiv")
            .join("n123")
            .join("index.html")
            .exists()
    );

    let site_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pixiv/index.html")
                .body(Body::empty())
                .expect("site html request"),
        )
        .await
        .expect("site html response");
    assert_eq!(site_response.status(), StatusCode::OK);
    assert_eq!(
        site_response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/html; charset=utf-8")
    );
    let site_html = response_text(site_response).await;
    assert!(site_html.contains("pixiv Index"));

    let work_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pixiv/n123/index.html")
                .body(Body::empty())
                .expect("work html request"),
        )
        .await
        .expect("work html response");
    assert_eq!(work_response.status(), StatusCode::OK);
    let work_html = response_text(work_response).await;
    assert!(work_html.contains("DB-backed work"));
    assert!(work_html.contains("text"));

    let info_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pixiv/n123/info/index.html")
                .body(Body::empty())
                .expect("info html request"),
        )
        .await
        .expect("info html response");
    assert_eq!(info_response.status(), StatusCode::OK);
    let info_html = response_text(info_response).await;
    assert!(info_html.contains("caption"));

    let episode_response = app
        .oneshot(
            Request::builder()
                .uri("/pixiv/n123/1/index.html")
                .body(Body::empty())
                .expect("episode html request"),
        )
        .await
        .expect("episode html response");
    assert_eq!(episode_response.status(), StatusCode::OK);
    let episode_html = response_text(episode_response).await;
    assert!(episode_html.contains("Episode 1"));
    assert!(episode_html.contains("text"));
}
