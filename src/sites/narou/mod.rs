use crate::core::model::WorkRecord;
use crate::core::renderer;
use crate::core::storage::Store;
use crate::sites::narou::pdf::{ParsedNarouPdf, extract_narou_pdf};
use crate::sites::narou::repair::repair_narou;
use crate::sites::{Site, SiteActionContext, SiteActionResult, SiteId};
use anyhow::{Context, Result, bail};
use lopdf::Document;
use regex::Regex;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub mod convert;
pub mod pdf;
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

#[derive(Debug, PartialEq, Eq)]
enum NarouDownloadOutcome {
    Imported(String),
    Skipped(String),
}

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
            "download" => match narou_download(value, store, context) {
                Ok(NarouDownloadOutcome::Imported(message)) => {
                    SiteActionResult::success(self.id(), action, message)
                }
                Ok(NarouDownloadOutcome::Skipped(message)) => {
                    SiteActionResult::skipped(self.id(), action, message)
                }
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            "convert" => match import_pdf_or_render(store, context) {
                Ok(message) => SiteActionResult::success(self.id(), action, message),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            "repair" => match repair_narou(
                store,
                &context.data_dir,
                &context.host_name,
                &context.img_url,
                context.request.pdf_path.as_deref(),
            ) {
                Ok(()) => SiteActionResult::success(
                    self.id(),
                    action,
                    "narou repair completed".to_string(),
                ),
                Err(err) => SiteActionResult::failed(self.id(), action, err.to_string()),
            },
            _ => SiteActionResult::skipped(self.id(), action, "unsupported"),
        }
    }
}

fn narou_download(
    value: &str,
    store: &mut Store,
    context: &SiteActionContext,
) -> Result<NarouDownloadOutcome> {
    let pdf_path = context
        .request
        .pdf_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.exists());

    if let Some(pdf_path) = pdf_path {
        let imported = import_pdf_file(store, &pdf_path, &context.request, &context.pdf_dir)?;
        return Ok(NarouDownloadOutcome::Imported(format!(
            "imported {}",
            imported.work_key
        )));
    }

    let candidates = narou_lookup_candidates(value);
    if let Some(existing) = find_existing_narou_work(store, &candidates)? {
        let nid = existing
            .raw_json
            .get("nid")
            .and_then(|value| value.as_str())
            .unwrap_or(&existing.work_key);
        info!(
            site = "narou",
            work_key = %existing.work_key,
            nid = %nid,
            "narou download skipped: PDF アップロードなしには更新できません。/api/ から PDF を添付してください"
        );
        return Ok(NarouDownloadOutcome::Skipped(format!(
            "narou {nid} は既存データを保持しました。PDF アップロードなしには更新できないため、/api/ から PDF を添付してください"
        )));
    }

    let requested = candidates
        .iter()
        .find(|candidate| looks_like_narou_nid(candidate))
        .cloned()
        .unwrap_or_else(|| value.trim().to_string());
    bail!("narou {requested} は未登録です。PDF を /api/ から添付してください")
}

fn import_pdf_or_render(store: &mut Store, context: &SiteActionContext) -> Result<String> {
    let request = &context.request;
    let pdf_path = request
        .pdf_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.exists());

    if let Some(pdf_path) = pdf_path {
        let imported = import_pdf_file(store, &pdf_path, request, &context.pdf_dir)?;
        return Ok(format!("imported {}", imported.work_key));
    }

    renderer::validate_site_from_store(store, "narou", None)?;
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
    pdf_dir: &str,
) -> Result<WorkRecord> {
    let author_id = request.author_id.clone();
    let author_url = request.author_url.clone();
    let chapter_input = parse_chapter_input(request.chapter.as_deref())?;
    let serialization = normalize_serialization(request.novel_type.as_deref(), &chapter_input);
    let now = chrono::Utc::now().to_rfc3339();
    let parsed_pdf = match extract_narou_pdf(pdf_path) {
        Ok(parsed_pdf) => Some(parsed_pdf),
        Err(err) => {
            warn!(
                path = %pdf_path.display(),
                error = %err,
                "narou PDF structured extraction failed; falling back to page-text import"
            );
            None
        }
    };
    let extracted_pages = parsed_pdf
        .as_ref()
        .map(|parsed| parsed.pages.clone())
        .unwrap_or_else(|| extract_pdf_pages(pdf_path).unwrap_or_default());
    let work_key = parsed_pdf
        .as_ref()
        .and_then(|parsed| parsed.nid.clone())
        .filter(|nid| looks_like_narou_nid(nid))
        .unwrap_or_else(|| derive_pdf_work_key(request, pdf_path, &extracted_pages));
    let title = parsed_pdf
        .as_ref()
        .and_then(|parsed| parsed.title.clone())
        .or_else(|| request.pdf_name.clone())
        .unwrap_or_else(|| work_key.clone());
    let author = parsed_pdf
        .as_ref()
        .and_then(|parsed| parsed.author.clone())
        .or_else(|| author_id.clone())
        .or_else(|| author_url.clone())
        .unwrap_or_else(|| "narou".to_string());
    let caption = parsed_pdf
        .as_ref()
        .and_then(|parsed| parsed.caption.clone())
        .unwrap_or_else(|| request.pdf_name.clone().unwrap_or_default());
    let record = WorkRecord {
        site: "narou".to_string(),
        work_key: work_key.clone(),
        title: title.clone(),
        author: author.clone(),
        author_id: author_id.clone(),
        author_url: author_url.clone(),
        r#type: "novel".to_string(),
        serialization: parsed_pdf
            .as_ref()
            .and_then(|parsed| parsed.serialization.clone())
            .unwrap_or_else(|| serialization.clone()),
        caption: caption.clone(),
        create_date: now.clone(),
        update_date: now.clone(),
        raw_json: match parsed_pdf.as_ref() {
            Some(parsed) => build_raw_json_from_extracted(
                &work_key,
                parsed,
                &title,
                &author,
                &author_id,
                &author_url,
                &caption,
                &serialization,
                &now,
            )?,
            None => build_raw_json(
                &work_key,
                &title,
                &author,
                &author_id,
                &author_url,
                &caption,
                &pdf_path.to_string_lossy(),
                &serialization,
                &chapter_input,
                &extracted_pages,
                &now,
            )?,
        },
    };

    store.upsert_work(&record)?;
    save_pdf_copy(pdf_path, pdf_dir, &work_key)?;
    Ok(record)
}

fn build_raw_json_from_extracted(
    work_key: &str,
    parsed: &ParsedNarouPdf,
    fallback_title: &str,
    fallback_author: &str,
    author_id: &Option<String>,
    author_url: &Option<String>,
    fallback_caption: &str,
    fallback_serialization: &str,
    now: &str,
) -> Result<serde_json::Value> {
    let nid = parsed
        .nid
        .clone()
        .filter(|nid| !nid.trim().is_empty())
        .unwrap_or_else(|| work_key.to_string());
    let title = parsed
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| fallback_title.to_string());
    let author = parsed
        .author
        .clone()
        .filter(|author| !author.trim().is_empty())
        .unwrap_or_else(|| fallback_author.to_string());
    let caption = parsed
        .caption
        .clone()
        .filter(|caption| !caption.trim().is_empty())
        .unwrap_or_else(|| fallback_caption.to_string());
    let serialization = parsed
        .serialization
        .clone()
        .filter(|serialization| !serialization.trim().is_empty())
        .unwrap_or_else(|| fallback_serialization.to_string());
    let create_date = parsed
        .create_date
        .clone()
        .filter(|date| !date.trim().is_empty())
        .unwrap_or_else(|| now.to_string());
    let update_date = parsed
        .update_date
        .clone()
        .filter(|date| !date.trim().is_empty())
        .unwrap_or_else(|| now.to_string());
    let mut episode_map = serde_json::Map::new();
    let mut total_characters = 0i64;

    for (index, episode) in parsed.episodes.iter().enumerate() {
        total_characters += episode.text_count;
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
                "postscript": episode.postscript,
                "createDate": episode.create_date.as_deref().unwrap_or(&create_date),
                "updateDate": episode.update_date.as_deref().unwrap_or(&update_date),
            }),
        );
    }

    if episode_map.is_empty() {
        bail!("narou PDF extraction produced no episodes");
    }

    let tags = parsed.tags.clone();
    let all_tags = tags.clone();
    Ok(json!({
        "version": 0,
        "get_date": now,
        "title": title,
        "id": nid,
        "nid": nid,
        "url": format!("https://ncode.syosetu.com/{}/", parsed.nid.as_deref().unwrap_or(work_key)),
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
        "tags": tags,
        "all_tags": all_tags,
        "createDate": create_date,
        "updateDate": update_date,
        "episodes": episode_map
    }))
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

fn derive_pdf_work_key(
    request: &crate::core::model::RequestData,
    pdf_path: &Path,
    pages: &[String],
) -> String {
    let author_identity = normalized_identity_component(
        request
            .author_id
            .as_deref()
            .or(request.author_url.as_deref()),
    );
    let title_identity =
        normalized_identity_component(pdf_identity_title(request, pdf_path).as_deref());
    let chapter_identity = if author_identity.is_some() && title_identity.is_some() {
        None
    } else {
        normalized_identity_component(request.chapter.as_deref())
    };

    let mut basis_parts = Vec::new();
    if let Some(author) = author_identity.as_deref() {
        basis_parts.push(format!("author:{author}"));
    }
    if let Some(title) = title_identity.as_deref() {
        basis_parts.push(format!("title:{title}"));
    }
    if let Some(chapter) = chapter_identity.as_deref() {
        basis_parts.push(format!("chapter:{chapter}"));
    }
    if basis_parts.len() < 2 {
        basis_parts.push(format!("content:{}", pdf_content_fingerprint(pages)));
    }

    let slug = build_work_key_slug(
        author_identity.as_deref(),
        title_identity.as_deref(),
        chapter_identity.as_deref(),
    );
    let digest = short_hash(&basis_parts.join("\n"));
    format!("ncode_pdf_{slug}_{digest}")
}

fn pdf_identity_title(
    request: &crate::core::model::RequestData,
    pdf_path: &Path,
) -> Option<String> {
    request
        .pdf_name
        .as_deref()
        .and_then(file_stem_string)
        .or_else(|| {
            pdf_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
}

fn file_stem_string(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    Path::new(trimmed)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .or_else(|| Some(trimmed.to_string()))
}

fn normalized_identity_component(raw: Option<&str>) -> Option<String> {
    raw.map(|value| {
        value
            .trim()
            .to_lowercase()
            .chars()
            .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
            .collect::<String>()
    })
    .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
    .filter(|value| !value.is_empty())
}

fn build_work_key_slug(author: Option<&str>, title: Option<&str>, chapter: Option<&str>) -> String {
    let mut slug = String::new();
    for part in [author, title, chapter].into_iter().flatten() {
        if !slug.is_empty() {
            slug.push('_');
        }
        slug.push_str(part);
        if slug.chars().count() >= 48 {
            break;
        }
    }

    let slug = sanitize_key(&slug);
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "import".to_string()
    } else {
        slug.chars().take(48).collect()
    }
}

fn pdf_content_fingerprint(pages: &[String]) -> String {
    let mut hasher = Sha256::new();
    for page in pages {
        hasher.update(page.as_bytes());
        hasher.update([0]);
    }
    hex_prefix(hasher.finalize())
}

fn short_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex_prefix(hasher.finalize())
}

fn hex_prefix(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

fn find_existing_narou_work(store: &Store, candidates: &[String]) -> Result<Option<WorkRecord>> {
    let works = store.list_works(Some("narou"))?;
    Ok(works.into_iter().find(|work| {
        let nid = work
            .raw_json
            .get("nid")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_lowercase());
        candidates.iter().any(|candidate| {
            candidate == &work.work_key.to_lowercase() || nid.as_deref() == Some(candidate.as_str())
        })
    }))
}

fn narou_lookup_candidates(value: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return candidates;
    }

    candidates.push(trimmed.to_lowercase());
    candidates.push(sanitize_key(trimmed).to_lowercase());
    if let Some(nid) = extract_narou_nid(trimmed) {
        candidates.push(nid);
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn extract_narou_nid(value: &str) -> Option<String> {
    let regex = Regex::new(r"(?i)(n[0-9]+[a-z0-9]+)").ok()?;
    regex
        .captures(value.trim())
        .and_then(|captures| captures.get(1))
        .map(|capture| capture.as_str().to_lowercase())
}

fn looks_like_narou_nid(value: &str) -> bool {
    extract_narou_nid(value)
        .as_deref()
        .map(|nid| nid == value.trim().to_lowercase())
        .unwrap_or(false)
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
    use crate::sites::SiteActionContext;
    use lopdf::content::{Content, Operation};
    use lopdf::{Object, Stream, dictionary};
    use std::fs;

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

    #[test]
    fn derive_pdf_work_key_ignores_request_id_paths_when_author_and_title_exist() {
        let request = crate::core::model::RequestData {
            request_id: "req-1".to_string(),
            pdf_name: Some("作品タイトル.pdf".to_string()),
            author_id: Some("author-1".to_string()),
            author_url: Some("https://example.com/authors/1".to_string()),
            chapter: Some("1-3:前編,4-5:後編".to_string()),
            ..Default::default()
        };
        let pages = vec!["1ページ".to_string(), "2ページ".to_string()];

        let first = derive_pdf_work_key(&request, Path::new("C:\\queued\\req-1.pdf"), &pages);
        let second = derive_pdf_work_key(&request, Path::new("C:\\queued\\req-2.pdf"), &pages);

        assert_eq!(first, second);
    }

    #[test]
    fn derive_pdf_work_key_falls_back_to_content_when_metadata_is_too_sparse() {
        let request = crate::core::model::RequestData {
            request_id: "req-1".to_string(),
            pdf_name: Some("sample.pdf".to_string()),
            ..Default::default()
        };

        let first = derive_pdf_work_key(
            &request,
            Path::new("C:\\queued\\req-1.pdf"),
            &["同じ本文".to_string()],
        );
        let second = derive_pdf_work_key(
            &request,
            Path::new("C:\\queued\\req-2.pdf"),
            &["同じ本文".to_string()],
        );
        let third = derive_pdf_work_key(
            &request,
            Path::new("C:\\queued\\req-3.pdf"),
            &["違う本文".to_string()],
        );

        assert_eq!(first, second);
        assert_ne!(first, third);
    }

    #[test]
    fn narou_download_returns_error_when_no_work_exists() {
        let mut store = Store::open_in_memory().unwrap();
        let context = test_context(
            crate::core::model::RequestData::default(),
            "C:\\narou-missing",
        );

        let err = narou_download("n1234ab", &mut store, &context).unwrap_err();
        assert!(
            err.to_string()
                .contains("PDF を /api/ から添付してください")
        );
    }

    #[test]
    fn narou_download_preserves_existing_raw_json() {
        let mut store = Store::open_in_memory().unwrap();
        let existing = sample_work_record("n1234ab", "n1234ab");
        let expected_raw = existing.raw_json.clone();
        store.upsert_work(&existing).unwrap();
        let context = test_context(
            crate::core::model::RequestData::default(),
            "C:\\narou-existing",
        );

        let outcome = narou_download("n1234ab", &mut store, &context).unwrap();
        assert!(matches!(outcome, NarouDownloadOutcome::Skipped(_)));

        let stored = store.list_works(Some("narou")).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].raw_json, expected_raw);
    }

    #[test]
    fn narou_download_matches_existing_nid_without_overwrite() {
        let mut store = Store::open_in_memory().unwrap();
        let existing = sample_work_record("ncode_pdf_demo", "n1234ab");
        store.upsert_work(&existing).unwrap();
        let context = test_context(crate::core::model::RequestData::default(), "C:\\narou-nid");

        let outcome =
            narou_download("https://ncode.syosetu.com/n1234ab/", &mut store, &context).unwrap();
        assert!(matches!(outcome, NarouDownloadOutcome::Skipped(_)));
        let stored = store.list_works(Some("narou")).unwrap();
        assert_eq!(stored[0].raw_json["episodes"]["1"]["text"], "既存本文");
    }

    #[test]
    fn narou_download_imports_pdf_when_path_is_present() {
        let root = std::env::temp_dir().join(format!(
            "narou-download-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let data_dir = root.join("data");
        let pdf_dir = root.join("pdf");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&pdf_dir).unwrap();
        let pdf_path = root.join("sample.pdf");
        create_sample_pdf(&pdf_path).unwrap();

        let mut store = Store::open_in_memory().unwrap();
        let request = crate::core::model::RequestData {
            request_id: "req-pdf".to_string(),
            pdf_path: Some(pdf_path.to_string_lossy().to_string()),
            pdf_name: Some("sample.pdf".to_string()),
            ..Default::default()
        };
        let context = SiteActionContext {
            host_name: String::new(),
            img_url: String::new(),
            data_dir: data_dir.to_string_lossy().to_string(),
            pdf_dir: pdf_dir.to_string_lossy().to_string(),
            request,
        };

        let outcome = narou_download("n1234ab", &mut store, &context).unwrap();
        assert!(matches!(outcome, NarouDownloadOutcome::Imported(_)));

        let works = store.list_works(Some("narou")).unwrap();
        assert_eq!(works.len(), 1);
        assert_eq!(works[0].work_key, "n1234ab");
        assert_eq!(works[0].raw_json["title"], "Sample Title");
        assert_eq!(works[0].raw_json["author"], "Alice");
        assert_eq!(works[0].raw_json["caption"], "Summary line");
        assert_eq!(
            works[0].raw_json["episodes"]
                .as_object()
                .map(|episodes| episodes.len()),
            Some(1)
        );
        assert!(
            works[0].raw_json["episodes"]["1"]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("Body text")
        );

        fs::remove_dir_all(&root).unwrap();
    }

    fn sample_work_record(work_key: &str, nid: &str) -> WorkRecord {
        WorkRecord {
            site: "narou".to_string(),
            work_key: work_key.to_string(),
            title: "既存作品".to_string(),
            author: "作者".to_string(),
            author_id: None,
            author_url: None,
            r#type: "novel".to_string(),
            serialization: "連載中".to_string(),
            caption: "既存キャプション".to_string(),
            create_date: "2026-01-01T00:00:00+09:00".to_string(),
            update_date: "2026-01-01T00:00:00+09:00".to_string(),
            raw_json: json!({
                "version": 0,
                "get_date": "2026-01-01T00:00:00+09:00",
                "title": "既存作品",
                "id": nid,
                "nid": nid,
                "url": format!("https://ncode.syosetu.com/{nid}/"),
                "author": "作者",
                "author_id": null,
                "author_url": null,
                "caption": "既存キャプション",
                "total_episodes": 1,
                "all_episodes": 1,
                "total_characters": 4,
                "all_characters": 4,
                "type": "novel",
                "serialization": "連載中",
                "tags": [],
                "all_tags": [],
                "createDate": "2026-01-01T00:00:00+09:00",
                "updateDate": "2026-01-01T00:00:00+09:00",
                "episodes": {
                    "1": {
                        "id": "page-1",
                        "chapter": null,
                        "title": "既存作品",
                        "textCount": 4,
                        "tags": [],
                        "introduction": "",
                        "text": "既存本文",
                        "postscript": "",
                        "createDate": "2026-01-01T00:00:00+09:00",
                        "updateDate": "2026-01-01T00:00:00+09:00"
                    }
                }
            }),
        }
    }

    fn test_context(request: crate::core::model::RequestData, root: &str) -> SiteActionContext {
        SiteActionContext {
            host_name: String::new(),
            img_url: String::new(),
            data_dir: format!("{root}\\data"),
            pdf_dir: format!("{root}\\pdf"),
            request,
        }
    }

    fn create_sample_pdf(path: &Path) -> Result<()> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });

        let page_one = add_test_page(
            &mut doc,
            pages_id,
            resources_id,
            &[
                (24, 72, 760, "Sample Title"),
                (12, 72, 720, "ncode: n1234ab"),
                (12, 72, 700, "author: Alice"),
                (12, 72, 680, "caption: Summary line"),
            ],
        )?;
        let page_two = add_test_page(
            &mut doc,
            pages_id,
            resources_id,
            &[(28, 200, 500, "Episode 1")],
        )?;
        let page_three = add_test_page(
            &mut doc,
            pages_id,
            resources_id,
            &[(14, 72, 720, "Body text for episode one.")],
        )?;

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_one.into(), page_two.into(), page_three.into()],
            "Count" => 3,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc.compress();
        doc.save(path)?;
        Ok(())
    }

    fn add_test_page(
        doc: &mut Document,
        pages_id: lopdf::ObjectId,
        resources_id: lopdf::ObjectId,
        lines: &[(i64, i64, i64, &str)],
    ) -> Result<lopdf::ObjectId> {
        let mut operations = Vec::new();
        for (size, x, y, text) in lines {
            operations.push(Operation::new("BT", vec![]));
            operations.push(Operation::new("Tf", vec!["F1".into(), (*size).into()]));
            operations.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
            operations.push(Operation::new("Tj", vec![Object::string_literal(*text)]));
            operations.push(Operation::new("ET", vec![]));
        }
        let content = Content { operations };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
        Ok(doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }))
    }
}
