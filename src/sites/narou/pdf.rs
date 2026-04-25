use anyhow::{Context, Result};
use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone};
use lopdf::{Document, Object, ObjectId};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedNarouPdf {
    pub nid: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub caption: Option<String>,
    pub serialization: Option<String>,
    pub tags: Vec<String>,
    pub create_date: Option<String>,
    pub update_date: Option<String>,
    pub episodes: Vec<ParsedEpisode>,
    pub pages: Vec<String>,
    pub vertical: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedEpisode {
    pub id: String,
    pub chapter: Option<String>,
    pub title: String,
    pub introduction: String,
    pub text: String,
    pub postscript: String,
    pub create_date: Option<String>,
    pub update_date: Option<String>,
    pub text_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct TextFragment {
    page: u32,
    x: f32,
    y: f32,
    font_size: f32,
    emphasized: bool,
    text: String,
}

#[derive(Debug, Clone, PartialEq)]
struct PageLine {
    page: u32,
    x: f32,
    y: f32,
    font_size: f32,
    emphasized: bool,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct ParsedMeta {
    nid: Option<String>,
    title: Option<String>,
    author: Option<String>,
    caption: Option<String>,
    serialization: Option<String>,
    tags: Vec<String>,
    create_date: Option<String>,
    update_date: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Matrix {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn from_operands(operands: &[Object]) -> Option<Self> {
        if operands.len() < 6 {
            return None;
        }
        Some(Self {
            a: object_to_f32(&operands[0])?,
            b: object_to_f32(&operands[1])?,
            c: object_to_f32(&operands[2])?,
            d: object_to_f32(&operands[3])?,
            e: object_to_f32(&operands[4])?,
            f: object_to_f32(&operands[5])?,
        })
    }

    fn multiply(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    fn translate(self, tx: f32, ty: f32) -> Self {
        self.multiply(Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        })
    }

    fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    fn looks_vertical(self) -> bool {
        self.b.abs() > self.a.abs() || self.c.abs() > self.d.abs()
    }
}

#[derive(Debug, Default)]
struct EpisodeBuilder {
    start_page: u32,
    end_page: u32,
    title: Option<String>,
    chapter: Option<String>,
    introduction: Vec<String>,
    text: Vec<String>,
    postscript: Vec<String>,
}

impl EpisodeBuilder {
    fn new(title: Option<String>, page: u32) -> Self {
        Self {
            start_page: page,
            end_page: page,
            title,
            ..Self::default()
        }
    }

    fn has_content(&self) -> bool {
        self.title.is_some()
            || !self.introduction.is_empty()
            || !self.text.is_empty()
            || !self.postscript.is_empty()
    }

    fn append_intro(&mut self, value: &str) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            self.introduction.push(trimmed.to_string());
        }
    }

    fn append_text(&mut self, value: &str) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            self.text.push(trimmed.to_string());
        }
    }

    fn append_postscript(&mut self, value: &str) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            self.postscript.push(trimmed.to_string());
        }
    }

    fn build(self, index: usize, fallback_title: &str) -> ParsedEpisode {
        let title = self.title.unwrap_or_else(|| {
            if index == 1 {
                fallback_title.to_string()
            } else {
                format!("{fallback_title} {index}")
            }
        });
        let introduction = self.introduction.join("\n\n").trim().to_string();
        let text = self.text.join("\n\n[newpage]\n\n").trim().to_string();
        let postscript = self.postscript.join("\n\n").trim().to_string();
        let text_count = text.chars().count() as i64;
        ParsedEpisode {
            id: page_label(self.start_page as usize, self.end_page as usize),
            chapter: self.chapter,
            title,
            introduction,
            text,
            postscript,
            create_date: None,
            update_date: None,
            text_count,
        }
    }
}

pub(crate) fn extract_narou_pdf(pdf_path: &Path) -> Result<ParsedNarouPdf> {
    let doc = Document::load(pdf_path)
        .with_context(|| format!("failed to load pdf {}", pdf_path.display()))?;
    let pages = doc.get_pages();
    let mut page_numbers: Vec<u32> = pages.keys().copied().collect();
    page_numbers.sort_unstable();

    let mut all_fragments = Vec::new();
    let mut page_lines = BTreeMap::new();
    let mut page_texts = Vec::new();
    let mut first_page_vertical_votes = (0usize, 0usize);

    for page_number in &page_numbers {
        let page_id = pages[page_number];
        let page_height = page_height(&doc, page_id).unwrap_or(842.0);
        let mut page_fragments = extract_page_fragments(&doc, *page_number, page_id, page_height)?;
        for fragment in &page_fragments {
            if *page_number == 1 {
                if fragment.font_size > 0.0 {
                    if fragment.emphasized {
                        first_page_vertical_votes.0 += 1;
                    } else {
                        first_page_vertical_votes.1 += 1;
                    }
                }
            }
        }
        all_fragments.append(&mut page_fragments);
    }

    let vertical = detect_vertical_document(&doc, pages.get(&1).copied(), &all_fragments)
        || first_page_vertical_votes.0 > first_page_vertical_votes.1;

    for page_number in &page_numbers {
        let fragments: Vec<TextFragment> = all_fragments
            .iter()
            .filter(|fragment| fragment.page == *page_number)
            .cloned()
            .collect();
        let lines = build_page_lines(*page_number, &fragments, vertical);
        page_texts.push(lines_to_page_text(&lines));
        page_lines.insert(*page_number, lines);
    }

    let meta_page_limit = page_numbers.len().min(2) as u32;
    let metadata = parse_metadata(&page_lines, meta_page_limit);
    let fallback_title = metadata
        .title
        .clone()
        .or_else(|| {
            pdf_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Narou PDF".to_string());

    let mut episodes = extract_episodes(&page_lines, meta_page_limit, &fallback_title);
    if episodes.is_empty() {
        episodes = vec![fallback_episode(
            &fallback_title,
            metadata.caption.as_deref().unwrap_or_default(),
            &page_texts,
            meta_page_limit,
        )];
    }

    Ok(ParsedNarouPdf {
        nid: metadata.nid,
        title: metadata.title.or(Some(fallback_title)),
        author: metadata.author,
        caption: metadata.caption,
        serialization: metadata.serialization,
        tags: metadata.tags,
        create_date: metadata.create_date,
        update_date: metadata.update_date,
        episodes,
        pages: page_texts,
        vertical,
    })
}

fn extract_page_fragments(
    doc: &Document,
    page_number: u32,
    page_id: ObjectId,
    page_height: f32,
) -> Result<Vec<TextFragment>> {
    let fonts = doc.get_page_fonts(page_id)?;
    let encodings = fonts
        .iter()
        .filter_map(|(name, font)| {
            font.get_font_encoding(doc)
                .ok()
                .map(|encoding| (name.clone(), encoding))
        })
        .collect::<BTreeMap<_, _>>();
    let content = doc.get_and_decode_page_content(page_id)?;
    let mut fragments = Vec::new();
    let mut graphics_stack = vec![Matrix::identity()];
    let mut ctm = Matrix::identity();
    let mut text_matrix = Matrix::identity();
    let mut line_matrix = Matrix::identity();
    let mut leading = 0.0f32;
    let mut current_font: Option<Vec<u8>> = None;
    let mut current_font_size = 12.0f32;
    let mut current_vertical_font = false;

    for operation in &content.operations {
        match operation.operator.as_str() {
            "q" => {
                graphics_stack.push(ctm);
            }
            "Q" => {
                if let Some(previous) = graphics_stack.pop() {
                    ctm = previous;
                }
            }
            "cm" => {
                if let Some(matrix) = Matrix::from_operands(&operation.operands) {
                    ctm = ctm.multiply(matrix);
                    if let Some(last) = graphics_stack.last_mut() {
                        *last = ctm;
                    }
                }
            }
            "BT" => {
                text_matrix = Matrix::identity();
                line_matrix = Matrix::identity();
            }
            "Tf" => {
                current_font = operation
                    .operands
                    .first()
                    .and_then(object_name_bytes)
                    .map(|name| name.to_vec());
                current_font_size = operation
                    .operands
                    .get(1)
                    .and_then(object_to_f32)
                    .unwrap_or(current_font_size);
                current_vertical_font = current_font
                    .as_ref()
                    .and_then(|name| fonts.get(name))
                    .map(|font| font_is_vertical(font, doc))
                    .unwrap_or(false);
            }
            "TL" => {
                leading = operation
                    .operands
                    .first()
                    .and_then(object_to_f32)
                    .unwrap_or(leading);
            }
            "Tm" => {
                if let Some(matrix) = Matrix::from_operands(&operation.operands) {
                    text_matrix = matrix;
                    line_matrix = matrix;
                }
            }
            "Td" => {
                let tx = operation
                    .operands
                    .first()
                    .and_then(object_to_f32)
                    .unwrap_or(0.0);
                let ty = operation
                    .operands
                    .get(1)
                    .and_then(object_to_f32)
                    .unwrap_or(0.0);
                line_matrix = line_matrix.translate(tx, ty);
                text_matrix = line_matrix;
            }
            "TD" => {
                let tx = operation
                    .operands
                    .first()
                    .and_then(object_to_f32)
                    .unwrap_or(0.0);
                let ty = operation
                    .operands
                    .get(1)
                    .and_then(object_to_f32)
                    .unwrap_or(0.0);
                leading = -ty;
                line_matrix = line_matrix.translate(tx, ty);
                text_matrix = line_matrix;
            }
            "T*" => {
                line_matrix = line_matrix.translate(0.0, -leading);
                text_matrix = line_matrix;
            }
            "'" => {
                line_matrix = line_matrix.translate(0.0, -leading);
                text_matrix = line_matrix;
                if let Some(text) = decode_text_operand(
                    doc,
                    current_font.as_ref().and_then(|name| encodings.get(name)),
                    operation.operands.first(),
                ) {
                    emit_text_fragments(
                        &mut fragments,
                        page_number,
                        page_height,
                        ctm,
                        text_matrix,
                        current_font.as_ref(),
                        current_font_size,
                        current_vertical_font,
                        &text,
                        0.0,
                    );
                }
            }
            "\"" => {
                line_matrix = line_matrix.translate(0.0, -leading);
                text_matrix = line_matrix;
                if let Some(text) = decode_text_operand(
                    doc,
                    current_font.as_ref().and_then(|name| encodings.get(name)),
                    operation.operands.get(2),
                ) {
                    emit_text_fragments(
                        &mut fragments,
                        page_number,
                        page_height,
                        ctm,
                        text_matrix,
                        current_font.as_ref(),
                        current_font_size,
                        current_vertical_font,
                        &text,
                        0.0,
                    );
                }
            }
            "Tj" => {
                if let Some(text) = decode_text_operand(
                    doc,
                    current_font.as_ref().and_then(|name| encodings.get(name)),
                    operation.operands.first(),
                ) {
                    emit_text_fragments(
                        &mut fragments,
                        page_number,
                        page_height,
                        ctm,
                        text_matrix,
                        current_font.as_ref(),
                        current_font_size,
                        current_vertical_font,
                        &text,
                        0.0,
                    );
                }
            }
            "TJ" => {
                let mut offset = 0.0f32;
                if let Some(Object::Array(items)) = operation.operands.first() {
                    for item in items {
                        if let Some(text) = decode_text_operand(
                            doc,
                            current_font.as_ref().and_then(|name| encodings.get(name)),
                            Some(item),
                        ) {
                            emit_text_fragments(
                                &mut fragments,
                                page_number,
                                page_height,
                                ctm,
                                text_matrix,
                                current_font.as_ref(),
                                current_font_size,
                                current_vertical_font,
                                &text,
                                offset,
                            );
                            offset += approximate_advance(
                                &text,
                                current_font_size,
                                current_vertical_font,
                            );
                        } else if let Some(adjust) = object_to_f32(item) {
                            offset += (-adjust / 1000.0) * current_font_size;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(fragments)
}

fn build_page_lines(page_number: u32, fragments: &[TextFragment], vertical: bool) -> Vec<PageLine> {
    if fragments.is_empty() {
        return Vec::new();
    }

    let mut items = fragments.to_vec();
    if vertical {
        items
            .sort_by(|left, right| compare_f32(right.x, left.x).then(compare_f32(left.y, right.y)));
    } else {
        items
            .sort_by(|left, right| compare_f32(left.y, right.y).then(compare_f32(left.x, right.x)));
    }

    let mut buckets: Vec<Vec<TextFragment>> = Vec::new();
    for fragment in items {
        let tolerance = (fragment.font_size * 0.7).max(6.0);
        if let Some(bucket) = buckets.iter_mut().find(|bucket| {
            let pivot = &bucket[0];
            if vertical {
                (pivot.x - fragment.x).abs() <= tolerance
            } else {
                (pivot.y - fragment.y).abs() <= tolerance
            }
        }) {
            bucket.push(fragment);
        } else {
            buckets.push(vec![fragment]);
        }
    }

    let mut lines = buckets
        .into_iter()
        .map(|mut bucket| {
            if vertical {
                bucket.sort_by(|left, right| compare_f32(left.y, right.y));
            } else {
                bucket.sort_by(|left, right| compare_f32(left.x, right.x));
            }
            let text = bucket
                .iter()
                .map(|fragment| fragment.text.as_str())
                .collect::<String>();
            let font_size = bucket
                .iter()
                .map(|fragment| fragment.font_size)
                .fold(0.0f32, f32::max);
            let emphasized = bucket.iter().any(|fragment| fragment.emphasized);
            let x = bucket
                .first()
                .map(|fragment| fragment.x)
                .unwrap_or_default();
            let y = bucket
                .first()
                .map(|fragment| fragment.y)
                .unwrap_or_default();
            PageLine {
                page: page_number,
                x,
                y,
                font_size,
                emphasized,
                text: text.trim().to_string(),
            }
        })
        .filter(|line| !line.text.is_empty())
        .collect::<Vec<_>>();

    if vertical {
        lines
            .sort_by(|left, right| compare_f32(right.x, left.x).then(compare_f32(left.y, right.y)));
    } else {
        lines
            .sort_by(|left, right| compare_f32(left.y, right.y).then(compare_f32(left.x, right.x)));
    }

    lines
}

fn lines_to_page_text(lines: &[PageLine]) -> String {
    lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

fn parse_metadata(page_lines: &BTreeMap<u32, Vec<PageLine>>, meta_page_limit: u32) -> ParsedMeta {
    let meta_lines = (1..=meta_page_limit)
        .flat_map(|page| page_lines.get(&page).cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let texts = meta_lines
        .iter()
        .map(|line| normalize_ascii_width(&line.text))
        .collect::<Vec<_>>();

    let caption = extract_line_value(&texts, &["【あらすじ】", "あらすじ", "summary", "caption"])
        .or_else(|| {
            extract_block_value(&texts, &["【あらすじ】", "あらすじ", "summary", "caption"])
        });
    let title = extract_line_value(&texts, &["【小説タイトル】", "小説タイトル", "title"])
        .or_else(|| {
            meta_lines
                .iter()
                .filter(|line| line.page == 1)
                .filter(|line| !looks_like_metadata_label(&normalize_ascii_width(&line.text)))
                .max_by(|left, right| compare_f32(left.font_size, right.font_size))
                .map(|line| line.text.trim().to_string())
        })
        .or_else(|| {
            meta_lines
                .iter()
                .filter(|line| !looks_like_metadata_label(&normalize_ascii_width(&line.text)))
                .max_by(|left, right| compare_f32(left.font_size, right.font_size))
                .map(|line| line.text.trim().to_string())
        });
    let nid = extract_nid(&texts);
    let author = extract_line_value(&texts, &["【作者名】", "作者", "author"]);
    let tags = extract_line_value(&texts, &["キーワード", "keyword", "keywords"])
        .map(|value| split_tags(&value))
        .unwrap_or_default();
    let serialization = texts.iter().find_map(|line| {
        if line.contains("完結済") {
            Some("完結済".to_string())
        } else if line.contains("連載中") {
            Some("連載中".to_string())
        } else {
            None
        }
    });
    let create_date = extract_line_value(
        &texts,
        &["初回掲載日", "掲載日", "公開日", "posted", "published"],
    )
    .and_then(|value| normalize_date_string(&value));
    let update_date = extract_line_value(&texts, &["最終掲載日", "更新日", "updated", "update"])
        .and_then(|value| normalize_date_string(&value))
        .or_else(|| extract_issue_date_text(&meta_lines));

    ParsedMeta {
        nid,
        title,
        author,
        caption,
        serialization,
        tags,
        create_date,
        update_date,
    }
}

fn extract_episodes(
    page_lines: &BTreeMap<u32, Vec<PageLine>>,
    meta_page_limit: u32,
    fallback_title: &str,
) -> Vec<ParsedEpisode> {
    let body_pages = page_lines
        .iter()
        .filter(|(page, _)| **page > meta_page_limit)
        .map(|(page, lines)| (*page, lines.clone()))
        .collect::<Vec<_>>();
    let target_pages = if body_pages.is_empty() {
        page_lines
            .iter()
            .map(|(page, lines)| (*page, lines.clone()))
            .collect::<Vec<_>>()
    } else {
        body_pages
    };
    if target_pages.is_empty() {
        return Vec::new();
    }

    let body_font_size = target_pages
        .iter()
        .flat_map(|(_, lines)| lines.iter().map(|line| line.font_size))
        .filter(|size| *size > 0.0)
        .fold(
            14.0f32,
            |acc, value| if acc == 14.0 { value } else { acc.min(value) },
        );
    let title_threshold = (body_font_size * 1.35).max(body_font_size + 2.0);

    let mut episodes = Vec::new();
    let mut current: Option<EpisodeBuilder> = None;
    let mut pending_chapter: Option<String> = None;

    for (page_number, lines) in target_pages {
        let page_text = lines_to_page_text(&lines);
        if page_text.trim().is_empty() || page_text.contains("目次") {
            continue;
        }
        let title_page_line = detect_title_page(&lines, title_threshold);
        if let Some(title_line) = title_page_line {
            let title = title_line.text.trim().to_string();
            if looks_like_episode_title(&title) {
                if let Some(builder) = current.take().filter(EpisodeBuilder::has_content) {
                    episodes.push(builder);
                }
                let mut builder = EpisodeBuilder::new(Some(title), page_number);
                builder.chapter = pending_chapter.take();
                current = Some(builder);
                continue;
            }

            if !looks_like_metadata_label(&normalize_ascii_width(&title)) {
                pending_chapter = Some(title);
                continue;
            }
        }

        let builder = current.get_or_insert_with(|| EpisodeBuilder::new(None, page_number));
        if builder.chapter.is_none() {
            builder.chapter = pending_chapter.take();
        }
        builder.end_page = page_number;
        let (introduction, body, postscript) = split_page_sections(&page_text);
        if !introduction.is_empty() && builder.text.is_empty() {
            builder.append_intro(&introduction);
        } else if !introduction.is_empty() {
            builder.append_text(&introduction);
        }
        builder.append_text(&body);
        builder.append_postscript(&postscript);
    }

    if let Some(builder) = current.filter(EpisodeBuilder::has_content) {
        episodes.push(builder);
    }

    if episodes.is_empty() {
        return Vec::new();
    }

    episodes
        .into_iter()
        .enumerate()
        .map(|(index, builder)| builder.build(index + 1, fallback_title))
        .collect()
}

fn fallback_episode(
    title: &str,
    caption: &str,
    page_texts: &[String],
    meta_page_limit: u32,
) -> ParsedEpisode {
    let content_pages = page_texts
        .iter()
        .enumerate()
        .filter(|(index, _)| (*index as u32) + 1 > meta_page_limit)
        .map(|(_, page)| page.trim())
        .filter(|page| !page.is_empty())
        .collect::<Vec<_>>();
    let text = if content_pages.is_empty() {
        page_texts
            .iter()
            .map(|page| page.trim())
            .filter(|page| !page.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n[newpage]\n\n")
    } else {
        content_pages.join("\n\n[newpage]\n\n")
    };
    ParsedEpisode {
        id: page_label(1, page_texts.len().max(1)),
        chapter: None,
        title: title.to_string(),
        introduction: caption.to_string(),
        text_count: text.chars().count() as i64,
        text,
        postscript: String::new(),
        create_date: None,
        update_date: None,
    }
}

fn detect_vertical_document(
    doc: &Document,
    first_page_id: Option<ObjectId>,
    fragments: &[TextFragment],
) -> bool {
    if let Some(page_id) = first_page_id {
        if let Ok(fonts) = doc.get_page_fonts(page_id) {
            if fonts.values().any(|font| font_is_vertical(font, doc)) {
                return true;
            }
        }
    }

    let sample = fragments.iter().take(32).collect::<Vec<_>>();
    if sample.is_empty() {
        return false;
    }
    let unique_x = sample
        .iter()
        .map(|fragment| (fragment.x * 10.0).round() as i32)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let unique_y = sample
        .iter()
        .map(|fragment| (fragment.y * 10.0).round() as i32)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    unique_x < unique_y
}

fn font_is_vertical(font: &lopdf::Dictionary, doc: &Document) -> bool {
    if let Ok(encoding_name) = font.get(b"Encoding").and_then(Object::as_name) {
        if encoding_name.ends_with(b"-V") {
            return true;
        }
    }

    if let Ok(descendant_fonts) = font.get(b"DescendantFonts").and_then(Object::as_array) {
        if let Some(first) = descendant_fonts.first() {
            if let Ok(reference) = first.as_reference() {
                if let Ok(descendant) = doc.get_dictionary(reference) {
                    if descendant
                        .get(b"WMode")
                        .and_then(Object::as_i64)
                        .map(|mode| mode == 1)
                        .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn extract_line_value(lines: &[String], labels: &[&str]) -> Option<String> {
    for (index, line) in lines.iter().enumerate() {
        let normalized = normalize_ascii_width(line);
        for label in labels {
            if let Some(value) = strip_label(&normalized, label) {
                if !value.is_empty() {
                    return Some(value.to_string());
                }
                if let Some(next_line) = lines.get(index + 1) {
                    let trimmed = next_line.trim();
                    if !trimmed.is_empty() && !looks_like_metadata_label(trimmed) {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }
    None
}

fn extract_block_value(lines: &[String], labels: &[&str]) -> Option<String> {
    for (index, line) in lines.iter().enumerate() {
        let normalized = normalize_ascii_width(line);
        for label in labels {
            if let Some(value) = strip_label(&normalized, label) {
                let mut parts = Vec::new();
                if !value.is_empty() {
                    parts.push(value.to_string());
                }
                for next_line in lines.iter().skip(index + 1) {
                    let trimmed = next_line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if looks_like_metadata_label(trimmed) {
                        break;
                    }
                    parts.push(trimmed.to_string());
                }
                if !parts.is_empty() {
                    return Some(parts.join("\n").trim().to_string());
                }
            }
        }
    }
    None
}

fn extract_nid(lines: &[String]) -> Option<String> {
    let regex = Regex::new(r"(?i)\b(?:ncode|nコード)\s*[:：]?\s*(n[0-9]+[a-z0-9]+)\b").ok()?;
    for line in lines {
        let normalized = normalize_ascii_width(line);
        if let Some(captures) = regex.captures(&normalized) {
            return captures
                .get(1)
                .map(|capture| capture.as_str().to_lowercase());
        }
    }
    None
}

fn extract_issue_date_text(lines: &[PageLine]) -> Option<String> {
    let regex =
        Regex::new(r"(\d{4})[/-年](\d{1,2})[/-月](\d{1,2})(?:日)?(?:\s+(\d{1,2}):(\d{2}))?")
            .ok()?;
    for line in lines.iter().rev() {
        let normalized = normalize_ascii_width(&line.text);
        if let Some(captures) = regex.captures(&normalized) {
            let year = captures.get(1)?.as_str().parse::<i32>().ok()?;
            let month = captures.get(2)?.as_str().parse::<u32>().ok()?;
            let day = captures.get(3)?.as_str().parse::<u32>().ok()?;
            let hour = captures
                .get(4)
                .and_then(|value| value.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let minute = captures
                .get(5)
                .and_then(|value| value.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let date = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, 0)?;
            let offset = FixedOffset::east_opt(9 * 3600)?;
            return offset
                .from_local_datetime(&date)
                .single()
                .map(|value| value.to_rfc3339());
        }
    }
    None
}

fn normalize_date_string(raw: &str) -> Option<String> {
    let text = normalize_ascii_width(raw);
    let regex = Regex::new(r"(\d{4})[/-年](\d{1,2})[/-月](\d{1,2})(?:日)?").ok()?;
    let captures = regex.captures(&text)?;
    let year = captures.get(1)?.as_str().parse::<i32>().ok()?;
    let month = captures.get(2)?.as_str().parse::<u32>().ok()?;
    let day = captures.get(3)?.as_str().parse::<u32>().ok()?;
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let offset = FixedOffset::east_opt(9 * 3600)?;
    let local = offset
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()?;
    Some(local.to_rfc3339())
}

fn split_tags(raw: &str) -> Vec<String> {
    raw.split([' ', '　', ',', '、'])
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

fn split_page_sections(page_text: &str) -> (String, String, String) {
    let normalized = page_text.replace("\r\n", "\n");
    let mut body = normalized.trim().to_string();
    let mut introduction = String::new();
    let mut postscript = String::new();

    for marker in ["【まえがき】", "[frontmatter]"] {
        if let Some(index) = body.find(marker) {
            let prefix = body[..index].trim().to_string();
            let suffix = body[index + marker.len()..].trim().to_string();
            if let Some(post_index) = suffix.find("【あとがき】") {
                introduction = suffix[..post_index].trim().to_string();
                postscript = suffix[post_index + "【あとがき】".len()..]
                    .trim()
                    .to_string();
                body = prefix;
                return (introduction, body, postscript);
            }
            introduction = suffix;
            body = prefix;
            break;
        }
    }

    for marker in ["【あとがき】", "[postscript]"] {
        if let Some(index) = body.find(marker) {
            postscript = body[index + marker.len()..].trim().to_string();
            body = body[..index].trim().to_string();
            break;
        }
    }

    (introduction, body, postscript)
}

fn detect_title_page(lines: &[PageLine], title_threshold: f32) -> Option<PageLine> {
    let non_empty = lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    if non_empty.is_empty() || non_empty.len() > 2 {
        return None;
    }
    let first = non_empty.first()?.clone();
    if first.font_size >= title_threshold || first.emphasized {
        return Some(first);
    }
    None
}

fn looks_like_episode_title(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let normalized = normalize_ascii_width(trimmed);
    let title_regexes = [
        Regex::new(r"^第.+話$").ok(),
        Regex::new(r"(?i)^episode\s+\d+").ok(),
        Regex::new(r"(?i)^chapter\s+\d+").ok(),
    ];
    title_regexes
        .iter()
        .flatten()
        .any(|regex| regex.is_match(&normalized))
}

fn looks_like_metadata_label(text: &str) -> bool {
    let normalized = normalize_ascii_width(text);
    [
        "作品情報",
        "小説タイトル",
        "ncode",
        "nコード",
        "作者",
        "author",
        "あらすじ",
        "summary",
        "caption",
        "キーワード",
        "keyword",
        "掲載日",
        "公開日",
        "更新日",
        "最終掲載日",
        "posted",
        "published",
    ]
    .iter()
    .any(|label| {
        normalized
            .to_ascii_lowercase()
            .contains(&label.to_ascii_lowercase())
    })
}

fn strip_label<'a>(line: &'a str, label: &str) -> Option<&'a str> {
    let normalized_label = normalize_ascii_width(label);
    let trimmed_line = line.trim_start();
    let lower_line = trimmed_line.to_ascii_lowercase();
    let lower_label = normalized_label.to_ascii_lowercase();
    if lower_line == lower_label {
        return Some("");
    }
    if lower_line.starts_with(&lower_label) {
        return Some(
            trimmed_line[normalized_label.len()..].trim_start_matches([':', '：', ' ', '　']),
        );
    }
    None
}

fn normalize_ascii_width(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\u{3000}' => ' ',
            '０'..='９' => char::from_u32(ch as u32 - '０' as u32 + '0' as u32).unwrap_or(ch),
            'Ａ'..='Ｚ' => char::from_u32(ch as u32 - 'Ａ' as u32 + 'A' as u32).unwrap_or(ch),
            'ａ'..='ｚ' => char::from_u32(ch as u32 - 'ａ' as u32 + 'a' as u32).unwrap_or(ch),
            '：' => ':',
            '／' => '/',
            '－' => '-',
            _ => ch,
        })
        .collect::<String>()
}

fn decode_text_operand(
    _doc: &Document,
    encoding: Option<&lopdf::Encoding<'_>>,
    operand: Option<&Object>,
) -> Option<String> {
    let encoding = encoding?;
    let operand = operand?;
    match operand {
        Object::String(bytes, _) => Document::decode_text(encoding, bytes).ok(),
        _ => None,
    }
}

fn emit_text_fragments(
    fragments: &mut Vec<TextFragment>,
    page_number: u32,
    page_height: f32,
    ctm: Matrix,
    text_matrix: Matrix,
    current_font: Option<&Vec<u8>>,
    current_font_size: f32,
    current_vertical_font: bool,
    text: &str,
    offset: f32,
) {
    let cleaned = text.replace('\0', "");
    if cleaned.trim().is_empty() {
        return;
    }

    let (base_x, base_y) = ctm.apply(text_matrix.e, text_matrix.f);
    let top_y = page_height - base_y;
    let emphasized = current_font
        .and_then(|name| std::str::from_utf8(name).ok())
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("bold") || lower.contains("gothic")
        })
        .unwrap_or(false);
    let looks_vertical = current_vertical_font || text_matrix.looks_vertical();

    for (index, ch) in cleaned.chars().enumerate() {
        let advance = if looks_vertical {
            index as f32 * current_font_size + offset
        } else {
            index as f32 * current_font_size * 0.6 + offset
        };
        fragments.push(TextFragment {
            page: page_number,
            x: round_coord(if looks_vertical {
                base_x
            } else {
                base_x + advance
            }),
            y: round_coord(if looks_vertical {
                top_y + advance
            } else {
                top_y
            }),
            font_size: round_coord(current_font_size),
            emphasized,
            text: ch.to_string(),
        });
    }
}

fn approximate_advance(text: &str, font_size: f32, vertical: bool) -> f32 {
    let width = if vertical { 1.0 } else { 0.6 };
    text.chars().count() as f32 * font_size * width
}

fn object_name_bytes(object: &Object) -> Option<&[u8]> {
    object.as_name().ok()
}

fn object_to_f32(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

fn page_height(doc: &Document, page_id: ObjectId) -> Option<f32> {
    let page = doc.get_dictionary(page_id).ok()?;
    let media_box = page.get(b"MediaBox").and_then(Object::as_array).ok()?;
    let top = media_box.get(3).and_then(object_to_f32)?;
    let bottom = media_box.get(1).and_then(object_to_f32).unwrap_or(0.0);
    Some(top - bottom)
}

fn page_label(start_page: usize, end_page: usize) -> String {
    if start_page == end_page {
        format!("page-{start_page}")
    } else {
        format!("pages-{start_page}-{end_page}")
    }
}

fn round_coord(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

fn compare_f32(left: f32, right: f32) -> Ordering {
    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_metadata_extracts_core_fields() {
        let lines = BTreeMap::from([(
            1,
            vec![
                PageLine {
                    page: 1,
                    x: 100.0,
                    y: 100.0,
                    font_size: 24.0,
                    emphasized: true,
                    text: "Sample Title".to_string(),
                },
                PageLine {
                    page: 1,
                    x: 90.0,
                    y: 140.0,
                    font_size: 12.0,
                    emphasized: false,
                    text: "ncode: n1234ab".to_string(),
                },
                PageLine {
                    page: 1,
                    x: 90.0,
                    y: 160.0,
                    font_size: 12.0,
                    emphasized: false,
                    text: "author: Alice".to_string(),
                },
                PageLine {
                    page: 1,
                    x: 90.0,
                    y: 180.0,
                    font_size: 12.0,
                    emphasized: false,
                    text: "caption: Summary line".to_string(),
                },
            ],
        )]);

        let meta = parse_metadata(&lines, 1);
        assert_eq!(meta.nid.as_deref(), Some("n1234ab"));
        assert_eq!(meta.title.as_deref(), Some("Sample Title"));
        assert_eq!(meta.author.as_deref(), Some("Alice"));
        assert_eq!(meta.caption.as_deref(), Some("Summary line"));
    }

    #[test]
    fn looks_like_episode_title_matches_common_markers() {
        assert!(looks_like_episode_title("第1話"));
        assert!(looks_like_episode_title("Episode 12"));
        assert!(!looks_like_episode_title("プロローグ"));
    }

    #[test]
    fn fallback_episode_uses_non_meta_pages() {
        let episode = fallback_episode(
            "Sample",
            "Caption",
            &[
                "meta page".to_string(),
                "body page 1".to_string(),
                "body page 2".to_string(),
            ],
            1,
        );

        assert_eq!(episode.introduction, "Caption");
        assert!(episode.text.contains("body page 1"));
        assert!(episode.text.contains("[newpage]"));
    }

    #[test]
    fn extract_episodes_splits_title_and_marker_pages() {
        let pages = BTreeMap::from([
            (
                1,
                vec![PageLine {
                    page: 1,
                    x: 10.0,
                    y: 10.0,
                    font_size: 24.0,
                    emphasized: true,
                    text: "Meta".to_string(),
                }],
            ),
            (
                2,
                vec![PageLine {
                    page: 2,
                    x: 10.0,
                    y: 10.0,
                    font_size: 28.0,
                    emphasized: true,
                    text: "Episode 1".to_string(),
                }],
            ),
            (
                3,
                vec![PageLine {
                    page: 3,
                    x: 10.0,
                    y: 10.0,
                    font_size: 14.0,
                    emphasized: false,
                    text: "【まえがき】Intro本文【あとがき】Post".to_string(),
                }],
            ),
        ]);

        let episodes = extract_episodes(&pages, 1, "Sample");
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].title, "Episode 1");
        assert_eq!(episodes[0].introduction, "Intro本文");
        assert_eq!(episodes[0].postscript, "Post");
    }
}
