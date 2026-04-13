use narou_bridge::core::model::{AppConfig, RequestData};
use narou_bridge::core::registry::{action_from_request, build_registry};
use narou_bridge::sites::SiteActionContext;

fn main() {
    let _config = AppConfig {
        data_dir: "data".to_string(),
        cookie_dir: "cookie".to_string(),
        queue_dir: "queue".to_string(),
        pdf_dir: "pdf".to_string(),
        log_dir: "log".to_string(),
        host_name: "http://127.0.0.1:8080".to_string(),
    };

    let registry = build_registry();
    let request = RequestData {
        add: Some("https://www.pixiv.net/novel/show.php?id=1".to_string()),
        ..RequestData::default()
    };

    if let Some((action, value)) = action_from_request(&request) {
        let context = SiteActionContext {
            host_name: "http://127.0.0.1:8080".to_string(),
            data_dir: "data".to_string(),
            cookie_dir: "cookie".to_string(),
            queue_dir: "queue".to_string(),
            pdf_dir: "pdf".to_string(),
        };
        let _ = registry.dispatch(action, &value, &context);
    }

    println!("narou_bridge rust skeleton initialized");
}
