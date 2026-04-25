use narou_bridge::core::model::WorkRecord;
use narou_bridge::core::renderer::render_site_from_store;
use narou_bridge::core::storage::Store;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn unique_root(name: &str) -> PathBuf {
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

fn synthetic_raw(work_key: &str, episode_count: usize) -> serde_json::Value {
    let mut episodes = serde_json::Map::new();
    for n in 1..=episode_count {
        episodes.insert(
            n.to_string(),
            json!({
                "id": format!("ep{n}"),
                "chapter": null,
                "title": format!("第{n}話"),
                "textCount": 1234,
                "tags": [],
                "introduction": "",
                "text": "本文サンプルテキスト。\n[newpage]\n続き。",
                "postscript": "",
                "createDate": "2025-01-01T00:00:00+09:00",
                "updateDate": "2025-01-02T00:00:00+09:00",
            }),
        );
    }
    json!({
        "version": 0,
        "title": format!("Synthetic {work_key}"),
        "id": work_key,
        "nid": work_key,
        "url": format!("https://example.com/{work_key}"),
        "author": "テスト作者",
        "author_id": "u1",
        "author_url": "https://example.com/u1",
        "caption": "計測用合成データ",
        "total_episodes": episode_count,
        "all_episodes": episode_count,
        "total_characters": episode_count as i64 * 1234,
        "all_characters": episode_count as i64 * 1234,
        "type": "novel",
        "serialization": "短編",
        "tags": ["テスト"],
        "all_tags": ["テスト"],
        "createDate": "2025-01-01T00:00:00+09:00",
        "updateDate": "2025-01-02T00:00:00+09:00",
        "episodes": episodes,
    })
}

/// Light-weight perf smoke: build N synthetic works, render the site, print elapsed.
/// Acts as a baseline anchor — fails only if catastrophically slow (>30s for 100 works).
#[test]
fn perf_baseline_render_site() {
    let root = unique_root("perf-baseline");
    let db_path = root.join("perf.db");
    let data_dir = root.join("data");
    std::fs::create_dir_all(&data_dir).expect("create data dir");

    let store = Store::open(&db_path).expect("open store");

    const N_WORKS: usize = 100;
    const N_EPISODES_PER_WORK: usize = 5;

    let insert_start = Instant::now();
    for i in 0..N_WORKS {
        let key = format!("p{i:05}");
        let raw = synthetic_raw(&key, N_EPISODES_PER_WORK);
        let record = WorkRecord {
            site: "pixiv".to_string(),
            work_key: key.clone(),
            title: format!("Synthetic {key}"),
            author: "テスト作者".to_string(),
            author_id: Some("u1".to_string()),
            author_url: Some("https://example.com/u1".to_string()),
            r#type: "novel".to_string(),
            serialization: "短編".to_string(),
            caption: "計測用".to_string(),
            create_date: "2025-01-01T00:00:00+09:00".to_string(),
            update_date: "2025-01-02T00:00:00+09:00".to_string(),
            raw_json: raw,
        };
        store.upsert_work(&record).expect("upsert");
    }
    let insert_elapsed = insert_start.elapsed();

    let render_start = Instant::now();
    render_site_from_store(
        &store,
        "pixiv",
        data_dir.to_str().unwrap(),
        "http://127.0.0.1:0",
        "",
        None,
    )
    .expect("render site");
    let render_elapsed = render_start.elapsed();

    eprintln!(
        "[perf-baseline] works={N_WORKS} eps_per_work={N_EPISODES_PER_WORK} \
         insert={insert_elapsed:?} render={render_elapsed:?}"
    );

    assert!(
        render_elapsed.as_secs() < 30,
        "render too slow: {render_elapsed:?}"
    );
    let index_html = data_dir.join("pixiv").join("index.html");
    assert!(index_html.exists(), "site index html not generated");
}
