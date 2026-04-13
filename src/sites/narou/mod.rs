use crate::core::model::WorkRecord;
use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};
use anyhow::{Context, Result};
use lopdf::Document;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub mod convert;
pub mod repair;

pub struct NarouSite;

pub fn site() -> Box<dyn Site> {
    Box::new(NarouSite)
}

impl Site for NarouSite {
    fn id(&self) -> SiteId {
        SiteId::Narou
    }

    fn name(&self) -> &'static str {
        "narou"
    }

    fn supported_actions(&self) -> &'static [&'static str] {
        &["download", "convert", "repair"]
    }

    fn execute(
        &self,
        action: &str,
        value: &str,
        context: &SiteActionContext,
        store: &mut Store,
    ) -> SiteActionResult {
        match action {
            "download" => match narou_download(value, store, &context.data_dir, &context.host_name)
            {
                Ok(message) => SiteActionResult::success(self.id(), action, message),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            "convert" | "repair" => match import_pdf_or_render(store, context) {
                Ok(message) => SiteActionResult::success(self.id(), action, message),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            _ => SiteActionResult::skipped(self.id(), action, "unsupported"),
        }
    }
}

fn narou_download(
    value: &str,
    store: &mut Store,
    data_dir: &str,
    host_name: &str,
) -> Result<String> {
    let work_key = sanitize_key(value);
    let now = chrono::Utc::now().to_rfc3339();
    let record = WorkRecord {
        site: "narou".to_string(),
        work_key: work_key.clone(),
        title: format!("Narou Import {work_key}"),
        author: "narou".to_string(),
        author_id: None,
        author_url: None,
        r#type: "novel".to_string(),
        serialization: "連載中".to_string(),
        caption: value.to_string(),
        create_date: now.clone(),
        update_date: now.clone(),
        raw_json: json!({
            "version": 0,
            "get_date": now,
            "title": format!("Narou Import {work_key}"),
            "id": work_key,
            "nid": value,
            "url": value,
            "author": "narou",
            "author_id": null,
            "author_url": null,
            "caption": value,
            "total_episodes": 1,
            "all_episodes": 1,
            "total_characters": 0,
            "all_characters": 0,
            "type": "novel",
            "serialization": "連載中",
            "tags": [],
            "all_tags": [],
            "createDate": now,
            "updateDate": now,
            "episodes": {}
        }),
    };
    store.upsert_work(&record)?;
    renderer::render_site_from_store(store, "narou", data_dir, host_name)?;
    Ok(format!("stored {work_key}"))
}

fn import_pdf_or_render(store: &mut Store, context: &SiteActionContext) -> Result<String> {
    let request = &context.request;
    let pdf_path = request
        .pdf_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.exists());

    if let Some(pdf_path) = pdf_path {
        let imported = import_pdf_file(
            store,
            &pdf_path,
            request,
            &context.data_dir,
            &context.pdf_dir,
            &context.host_name,
        )?;
        return Ok(format!("imported {}", imported.work_key));
    }

    renderer::render_site_from_store(store, "narou", &context.data_dir, &context.host_name)?;
    Ok(format!(
        "narou {} completed",
        if request.chapter.is_some() {
            "chapter"
        } else {
            "render"
        }
    ))
}

fn import_pdf_file(
    store: &mut Store,
    pdf_path: &Path,
    request: &crate::core::model::RequestData,
    data_dir: &str,
    pdf_dir: &str,
    host_name: &str,
) -> Result<WorkRecord> {
    let work_key = sanitize_key(
        pdf_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("ncode"),
    );
    let title = request.pdf_name.clone().unwrap_or_else(|| work_key.clone());
    let author_id = request.author_id.clone();
    let author_url = request.author_url.clone();
    let author = author_id
        .clone()
        .or_else(|| author_url.clone())
        .unwrap_or_else(|| "narou".to_string());
    let serialization = request.novel_type.clone().unwrap_or_else(|| {
        if request.chapter.is_some() {
            "連載中".to_string()
        } else {
            "短編".to_string()
        }
    });
    let chapter = request.chapter.clone();
    let extracted_text = extract_pdf_text(pdf_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    let record = WorkRecord {
        site: "narou".to_string(),
        work_key: work_key.clone(),
        title: title.clone(),
        author: author.clone(),
        author_id: author_id.clone(),
        author_url: author_url.clone(),
        r#type: "novel".to_string(),
        serialization: serialization.clone(),
        caption: request.pdf_name.clone().unwrap_or_default(),
        create_date: now.clone(),
        update_date: now.clone(),
        raw_json: build_raw_json(
            &work_key,
            &title,
            &author,
            &author_id,
            &author_url,
            &request.pdf_name.clone().unwrap_or_default(),
            &pdf_path.to_string_lossy(),
            &serialization,
            chapter,
            &extracted_text,
            &now,
        ),
    };

    store.upsert_work(&record)?;
    save_pdf_copy(pdf_path, pdf_dir, &work_key)?;
    renderer::render_site_from_store(store, "narou", data_dir, host_name)?;
    Ok(record)
}

fn extract_pdf_text(pdf_path: &Path) -> Result<String> {
    let doc = Document::load(pdf_path)
        .with_context(|| format!("failed to load pdf {}", pdf_path.display()))?;
    let pages = doc.get_pages();
    let page_numbers: Vec<u32> = pages.keys().cloned().collect();
    Ok(doc.extract_text(&page_numbers).unwrap_or_default())
}

fn save_pdf_copy(pdf_path: &Path, pdf_dir: &str, work_key: &str) -> Result<()> {
    let target_dir = PathBuf::from(pdf_dir);
    fs::create_dir_all(&target_dir)?;
    let target = target_dir.join(format!("{work_key}.pdf"));
    if pdf_path != target {
        fs::copy(pdf_path, target)?;
    }
    Ok(())
}

fn build_raw_json(
    work_key: &str,
    title: &str,
    author: &str,
    author_id: &Option<String>,
    author_url: &Option<String>,
    caption: &str,
    source_url: &str,
    serialization: &str,
    chapter: Option<String>,
    text: &str,
    now: &str,
) -> serde_json::Value {
    let text_count = text.chars().count() as i64;
    let episode_id = chapter.clone().unwrap_or_else(|| "1".to_string());
    json!({
        "version": 0,
        "get_date": now,
        "title": title,
        "id": work_key,
        "nid": work_key,
        "url": source_url,
        "author": author,
        "author_id": author_id,
        "author_url": author_url,
        "caption": caption,
        "total_episodes": 1,
        "all_episodes": 1,
        "total_characters": text_count,
        "all_characters": text_count,
        "type": "novel",
        "serialization": serialization,
        "tags": [],
        "all_tags": [],
        "createDate": now,
        "updateDate": now,
        "episodes": {
            "1": {
                "id": episode_id,
                "chapter": chapter,
                "title": title,
                "textCount": text_count,
                "tags": [],
                "introduction": caption,
                "text": text,
                "postscript": "",
                "createDate": now,
                "updateDate": now,
            }
        }
    })
}

fn sanitize_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '?' | '&' | '=' | ' ' | '\t' | '\n' | '\r' => '_',
            _ => ch,
        })
        .collect()
}
