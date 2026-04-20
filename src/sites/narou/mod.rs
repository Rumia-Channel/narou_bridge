use crate::core::model::WorkRecord;
use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};
use anyhow::{Context, Result, bail};
use lopdf::Document;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub mod convert;
pub mod repair;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChapterRange {
    start_page: usize,
    end_page: usize,
    title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChapterInput {
    None,
    Label(String),
    Sections(Vec<ChapterRange>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportedEpisode {
    id: String,
    chapter: Option<String>,
    title: String,
    introduction: String,
    text: String,
    text_count: i64,
}

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
    let chapter_input = parse_chapter_input(request.chapter.as_deref())?;
    let serialization = normalize_serialization(request.novel_type.as_deref(), &chapter_input);
    let extracted_pages = extract_pdf_pages(pdf_path)?;
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
            &chapter_input,
            &extracted_pages,
            &now,
        )?,
    };

    store.upsert_work(&record)?;
    save_pdf_copy(pdf_path, pdf_dir, &work_key)?;
    renderer::render_site_from_store(store, "narou", data_dir, host_name)?;
    Ok(record)
}

fn extract_pdf_pages(pdf_path: &Path) -> Result<Vec<String>> {
    let doc = Document::load(pdf_path)
        .with_context(|| format!("failed to load pdf {}", pdf_path.display()))?;
    let pages = doc.get_pages();
    let mut page_numbers: Vec<u32> = pages.keys().cloned().collect();
    page_numbers.sort_unstable();

    let mut extracted_pages = Vec::with_capacity(page_numbers.len());
    for page_number in page_numbers {
        extracted_pages.push(doc.extract_text(&[page_number]).unwrap_or_default());
    }

    if extracted_pages.is_empty() {
        extracted_pages.push(String::new());
    }

    Ok(extracted_pages)
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
    chapter_input: &ChapterInput,
    pages: &[String],
    now: &str,
) -> Result<serde_json::Value> {
    let episodes = build_pdf_episodes(title, caption, pages, chapter_input)?;
    let total_characters = episodes
        .iter()
        .map(|episode| episode.text_count)
        .sum::<i64>();
    let mut episode_map = serde_json::Map::new();
    for (index, episode) in episodes.iter().enumerate() {
        episode_map.insert(
            (index + 1).to_string(),
            json!({
                "id": episode.id,
                "chapter": episode.chapter,
                "title": episode.title,
                "textCount": episode.text_count,
                "tags": [],
                "introduction": episode.introduction,
                "text": episode.text,
                "postscript": "",
                "createDate": now,
                "updateDate": now,
            }),
        );
    }

    Ok(json!({
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
        "total_episodes": episode_map.len(),
        "all_episodes": episode_map.len(),
        "total_characters": total_characters,
        "all_characters": total_characters,
        "type": "novel",
        "serialization": serialization,
        "tags": [],
        "all_tags": [],
        "createDate": now,
        "updateDate": now,
        "episodes": episode_map
    }))
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

fn normalize_serialization(raw: Option<&str>, chapter_input: &ChapterInput) -> String {
    let normalized =
        raw.map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| match value {
                "0" | "連載" | "連載中" => "連載中".to_string(),
                "1" | "完結" | "完結済" => "完結済".to_string(),
                "2" | "短編" => "短編".to_string(),
                other => other.to_string(),
            });

    normalized.unwrap_or_else(|| match chapter_input {
        ChapterInput::None => "短編".to_string(),
        ChapterInput::Label(_) | ChapterInput::Sections(_) => "連載中".to_string(),
    })
}

fn parse_chapter_input(raw: Option<&str>) -> Result<ChapterInput> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(ChapterInput::None);
    };

    if !raw.contains(':') {
        return Ok(ChapterInput::Label(raw.to_string()));
    }

    let mut sections = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let (range, title) = part
            .split_once(':')
            .with_context(|| format!("invalid chapter segment: {part}"))?;
        let title = title.trim();
        if title.is_empty() {
            bail!("chapter title is empty in segment: {part}");
        }

        let (start_page, end_page) = parse_page_range(range.trim())?;
        sections.push(ChapterRange {
            start_page,
            end_page,
            title: title.to_string(),
        });
    }

    if sections.is_empty() {
        return Ok(ChapterInput::None);
    }

    Ok(ChapterInput::Sections(sections))
}

fn parse_page_range(raw: &str) -> Result<(usize, usize)> {
    let normalized = raw.trim().replace(['〜', '~'], "-");
    let mut parts = normalized.splitn(2, '-').map(str::trim);
    let start = parts
        .next()
        .filter(|value| !value.is_empty())
        .with_context(|| format!("missing chapter range start: {raw}"))?
        .parse::<usize>()
        .with_context(|| format!("invalid chapter range start: {raw}"))?;
    let end = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("invalid chapter range end: {raw}"))
        })
        .transpose()?
        .unwrap_or(start);

    if start == 0 || end == 0 || start > end {
        bail!("invalid chapter range: {raw}");
    }

    Ok((start, end))
}

fn build_pdf_episodes(
    work_title: &str,
    caption: &str,
    pages: &[String],
    chapter_input: &ChapterInput,
) -> Result<Vec<ImportedEpisode>> {
    let normalized_pages: Vec<String> = if pages.is_empty() {
        vec![String::new()]
    } else {
        pages.iter().map(|page| normalize_page_text(page)).collect()
    };

    match chapter_input {
        ChapterInput::None => Ok(vec![build_episode_segment(
            work_title,
            caption,
            &normalized_pages,
            1,
            normalized_pages.len(),
            None,
            1,
            1,
        )]),
        ChapterInput::Label(label) => Ok(vec![build_episode_segment(
            work_title,
            caption,
            &normalized_pages,
            1,
            normalized_pages.len(),
            Some(label),
            1,
            1,
        )]),
        ChapterInput::Sections(sections) => {
            build_chapter_sections(work_title, caption, &normalized_pages, sections)
        }
    }
}

fn build_chapter_sections(
    work_title: &str,
    caption: &str,
    pages: &[String],
    sections: &[ChapterRange],
) -> Result<Vec<ImportedEpisode>> {
    let total_pages = pages.len();
    let mut ordered_sections = sections.to_vec();
    ordered_sections
        .sort_by_key(|section| (section.start_page, section.end_page, section.title.clone()));

    let mut last_end = 0usize;
    for section in &ordered_sections {
        if section.start_page > total_pages || section.end_page > total_pages {
            bail!(
                "chapter range {}-{} exceeds PDF page count {}",
                section.start_page,
                section.end_page,
                total_pages
            );
        }
        if section.start_page <= last_end {
            bail!("chapter ranges overlap around page {}", section.start_page);
        }
        last_end = section.end_page;
    }

    let mut segments = Vec::new();
    let mut current_page = 1usize;
    for section in &ordered_sections {
        if current_page < section.start_page {
            segments.push((current_page, section.start_page - 1, None));
        }
        segments.push((
            section.start_page,
            section.end_page,
            Some(section.title.as_str()),
        ));
        current_page = section.end_page + 1;
    }
    if current_page <= total_pages {
        segments.push((current_page, total_pages, None));
    }
    let total_segments = segments.len().max(1);

    Ok(segments
        .into_iter()
        .enumerate()
        .map(|(index, (start_page, end_page, chapter))| {
            build_episode_segment(
                work_title,
                caption,
                pages,
                start_page,
                end_page,
                chapter,
                index + 1,
                total_segments,
            )
        })
        .collect())
}

fn build_episode_segment(
    work_title: &str,
    caption: &str,
    pages: &[String],
    start_page: usize,
    end_page: usize,
    chapter: Option<&str>,
    index: usize,
    total_segments: usize,
) -> ImportedEpisode {
    let text = join_page_text(&pages[start_page - 1..end_page]);
    let chapter = chapter.map(str::to_string);
    let title = match chapter.as_deref() {
        Some(chapter_title) => chapter_title.to_string(),
        None if total_segments == 1 => work_title.to_string(),
        None => format!("{work_title} {index}"),
    };
    let introduction = if total_segments == 1 {
        caption.to_string()
    } else {
        page_label(start_page, end_page)
    };

    ImportedEpisode {
        id: page_label(start_page, end_page),
        chapter,
        title,
        introduction,
        text_count: text.chars().count() as i64,
        text,
    }
}

fn join_page_text(pages: &[String]) -> String {
    pages
        .iter()
        .map(|page| page.trim())
        .collect::<Vec<_>>()
        .join("\n\n[newpage]\n\n")
        .trim()
        .to_string()
}

fn normalize_page_text(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

fn page_label(start_page: usize, end_page: usize) -> String {
    if start_page == end_page {
        format!("page-{start_page}")
    } else {
        format!("pages-{start_page}-{end_page}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_serialization_accepts_legacy_values() {
        assert_eq!(
            normalize_serialization(Some("0"), &ChapterInput::None),
            "連載中"
        );
        assert_eq!(
            normalize_serialization(Some("1"), &ChapterInput::None),
            "完結済"
        );
        assert_eq!(
            normalize_serialization(Some("2"), &ChapterInput::None),
            "短編"
        );
        assert_eq!(
            normalize_serialization(Some("完結済"), &ChapterInput::None),
            "完結済"
        );
    }

    #[test]
    fn parse_chapter_input_supports_legacy_page_ranges() {
        assert_eq!(
            parse_chapter_input(Some("1-3:タイトルA, 4-5:タイトルB")).unwrap(),
            ChapterInput::Sections(vec![
                ChapterRange {
                    start_page: 1,
                    end_page: 3,
                    title: "タイトルA".to_string(),
                },
                ChapterRange {
                    start_page: 4,
                    end_page: 5,
                    title: "タイトルB".to_string(),
                }
            ])
        );
    }

    #[test]
    fn parse_chapter_input_preserves_plain_labels() {
        assert_eq!(
            parse_chapter_input(Some("序章")).unwrap(),
            ChapterInput::Label("序章".to_string())
        );
    }

    #[test]
    fn build_pdf_episodes_splits_pages_by_chapter_ranges() {
        let episodes = build_pdf_episodes(
            "作品タイトル",
            "PDF説明",
            &[
                "1ページ".to_string(),
                "2ページ".to_string(),
                "3ページ".to_string(),
                "4ページ".to_string(),
                "5ページ".to_string(),
            ],
            &ChapterInput::Sections(vec![
                ChapterRange {
                    start_page: 1,
                    end_page: 3,
                    title: "前編".to_string(),
                },
                ChapterRange {
                    start_page: 4,
                    end_page: 5,
                    title: "後編".to_string(),
                },
            ]),
        )
        .unwrap();

        assert_eq!(episodes.len(), 2);
        assert_eq!(episodes[0].id, "pages-1-3");
        assert_eq!(episodes[0].chapter.as_deref(), Some("前編"));
        assert_eq!(episodes[0].title, "前編");
        assert!(episodes[0].text.contains("[newpage]"));
        assert_eq!(episodes[1].id, "pages-4-5");
        assert_eq!(episodes[1].chapter.as_deref(), Some("後編"));
        assert_eq!(episodes[1].title, "後編");
    }

    #[test]
    fn build_pdf_episodes_rejects_out_of_range_sections() {
        let err = build_pdf_episodes(
            "作品タイトル",
            "",
            &["1ページ".to_string(), "2ページ".to_string()],
            &ChapterInput::Sections(vec![ChapterRange {
                start_page: 2,
                end_page: 3,
                title: "終章".to_string(),
            }]),
        )
        .unwrap_err();

        assert!(err.to_string().contains("exceeds PDF page count"));
    }
}
