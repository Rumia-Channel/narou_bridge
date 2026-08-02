use crate::core::model::WorkRecord;
use crate::core::storage::{Store, WorkListFilters, WorkListSort};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

const NOVEL_PAGE_TEMPLATE: &str = include_str!("../../templates/novel_page.html");
const SITE_INDEX_TEMPLATE: &str = include_str!("../../templates/site_index.html");

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct RawWork {
    #[serde(default)]
    version: i64,
    #[serde(default, rename = "get_date")]
    get_date: String,
    #[serde(default)]
    title: String,
    #[serde(default, deserialize_with = "crate::core::de_util::de_string_or_num")]
    id: String,
    #[serde(default)]
    nid: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    author: String,
    #[serde(
        default,
        deserialize_with = "crate::core::de_util::de_opt_string_or_num"
    )]
    author_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::core::de_util::de_opt_string_or_num"
    )]
    author_url: Option<String>,
    #[serde(default)]
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
    #[serde(default)]
    serialization: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, rename = "all_tags")]
    all_tags: Vec<String>,
    #[serde(default, rename = "createDate")]
    create_date: String,
    #[serde(default, rename = "updateDate")]
    update_date: String,
    #[serde(default)]
    episodes: BTreeMap<String, RawEpisode>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct RawEpisode {
    #[serde(default, deserialize_with = "crate::core::de_util::de_string_or_num")]
    id: String,
    #[serde(
        default,
        deserialize_with = "crate::core::de_util::de_opt_string_or_num"
    )]
    chapter: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "textCount")]
    text_count: i64,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    introduction: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    postscript: String,
    #[serde(default, rename = "createDate")]
    create_date: String,
    #[serde(default, rename = "updateDate")]
    update_date: String,
}

#[derive(Debug, Clone)]
struct ImageAsset {
    hash: String,
    ext: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct RenderedWork {
    site: String,
    work_key: String,
    work: RawWork,
    episodes: Vec<(String, RawEpisode)>,
}

fn collect_narou_cover_candidates(
    value: &serde_json::Value,
    in_cover_field: bool,
    candidates: &mut Vec<String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                collect_narou_cover_candidates(
                    child,
                    in_cover_field || is_cover_field(key),
                    candidates,
                );
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_narou_cover_candidates(child, in_cover_field, candidates);
            }
        }
        serde_json::Value::String(text) => {
            if in_cover_field {
                push_unique_candidate(candidates, text.trim());
            }
            collect_image_markup_candidates(text, candidates);
        }
        _ => {}
    }
}

fn is_cover_field(key: &str) -> bool {
    matches!(
        key,
        "cover" | "cover_image" | "coverImage" | "thumbnail" | "thumbnail_image" | "thumbnailImage"
    )
}

fn collect_image_markup_candidates(text: &str, candidates: &mut Vec<String>) {
    let mut remainder = text;
    while let Some(start) = remainder.find("[image](") {
        let after_marker = &remainder[start + "[image](".len()..];
        let Some(end) = after_marker.find(')') else {
            break;
        };
        push_unique_candidate(candidates, after_marker[..end].trim());
        remainder = &after_marker[end + 1..];
    }
}

fn push_unique_candidate(candidates: &mut Vec<String>, candidate: &str) {
    if candidate.is_empty() {
        return;
    }
    if candidates.iter().any(|existing| existing == candidate) {
        return;
    }
    candidates.push(candidate.to_string());
}

pub fn validate_site_from_store(
    store: &Store,
    site: &str,
    target_work: Option<&str>,
) -> Result<()> {
    if let Some(work_key) = target_work {
        let work = store
            .get_work(site, work_key)?
            .with_context(|| format!("work {work_key} was not found for site {site}"))?;
        rendered_work_from_record(&work)?;
        return Ok(());
    }

    for work in store.list_works(Some(site))? {
        rendered_work_from_record(&work)?;
    }
    Ok(())
}

fn rendered_work_from_record(work: &WorkRecord) -> Result<RenderedWork> {
    let raw_work = parse_raw_work(&work.raw_json)
        .with_context(|| format!("failed to parse raw work for {}", work.work_key))?;
    Ok(build_rendered_work(
        raw_work,
        work.site.clone(),
        work.work_key.clone(),
    ))
}

fn build_rendered_work(raw_work: RawWork, site: String, work_key: String) -> RenderedWork {
    let episodes = sort_episodes(&raw_work.episodes);
    RenderedWork {
        site,
        work_key,
        work: raw_work,
        episodes,
    }
}

#[derive(Debug, Clone)]
struct SiteListEntry {
    title: String,
    author: String,
    author_url: Option<String>,
    work_type: String,
    serialization: String,
    all_tags: Vec<String>,
    create_date: String,
    update_date: String,
}

#[derive(Debug, Clone)]
pub struct SiteIndexRenderOptions {
    pub page: usize,
    pub per_page: usize,
    pub search: Option<String>,
    pub work_type: Option<String>,
    pub serialization: Option<String>,
    pub sort: WorkListSort,
}

pub fn render_site_index_html_from_store(
    store: &Store,
    site: &str,
    mut options: SiteIndexRenderOptions,
) -> Result<String> {
    options.page = options.page.max(1);
    options.per_page = match options.per_page {
        10 | 25 | 50 | 100 => options.per_page,
        _ => 25,
    };

    let filters = WorkListFilters {
        site: Some(site),
        search: options.search.as_deref(),
        work_type: options.work_type.as_deref(),
        serialization: options.serialization.as_deref(),
        ..WorkListFilters::default()
    };
    let mut page = store.list_work_summaries(
        filters,
        options.per_page,
        (options.page - 1).saturating_mul(options.per_page),
        options.sort,
    )?;
    let total_pages = page.total.div_ceil(options.per_page).max(1);
    if options.page > total_pages {
        options.page = total_pages;
        page = store.list_work_summaries(
            filters,
            options.per_page,
            (options.page - 1).saturating_mul(options.per_page),
            options.sort,
        )?;
    }

    let entries = page
        .works
        .into_iter()
        .map(|work| {
            (
                work.work_key,
                SiteListEntry {
                    title: work.title,
                    author: work.author,
                    author_url: work.author_url,
                    work_type: work.r#type,
                    serialization: work.serialization,
                    all_tags: work.all_tags,
                    create_date: work.create_date,
                    update_date: work.update_date,
                },
            )
        })
        .collect::<Vec<_>>();
    let initial_rows = render_initial_site_rows(&entries);
    let previous_page_link =
        render_page_link(&options, options.page - 1, "前へ", options.page > 1)?;
    let next_page_link = render_page_link(
        &options,
        options.page + 1,
        "次へ",
        options.page < total_pages,
    )?;

    Ok(SITE_INDEX_TEMPLATE
        .replace("{site_name}", &escape_html(site))
        .replace(
            "{search_value}",
            &escape_html(options.search.as_deref().unwrap_or("")),
        )
        .replace(
            "{type_all_selected}",
            selected(options.work_type.as_deref(), None),
        )
        .replace(
            "{type_novel_selected}",
            selected(options.work_type.as_deref(), Some("novel")),
        )
        .replace(
            "{type_comic_selected}",
            selected(options.work_type.as_deref(), Some("comic")),
        )
        .replace(
            "{serialization_all_selected}",
            selected(options.serialization.as_deref(), None),
        )
        .replace(
            "{serialization_short_selected}",
            selected(options.serialization.as_deref(), Some("短編")),
        )
        .replace(
            "{serialization_active_selected}",
            selected(options.serialization.as_deref(), Some("連載中")),
        )
        .replace(
            "{serialization_complete_selected}",
            selected(options.serialization.as_deref(), Some("完結済")),
        )
        .replace(
            "{sort_updated_desc_selected}",
            selected_sort(options.sort, WorkListSort::UpdatedDesc),
        )
        .replace(
            "{sort_updated_asc_selected}",
            selected_sort(options.sort, WorkListSort::UpdatedAsc),
        )
        .replace(
            "{sort_title_asc_selected}",
            selected_sort(options.sort, WorkListSort::TitleAsc),
        )
        .replace(
            "{sort_title_desc_selected}",
            selected_sort(options.sort, WorkListSort::TitleDesc),
        )
        .replace(
            "{sort_author_asc_selected}",
            selected_sort(options.sort, WorkListSort::AuthorAsc),
        )
        .replace(
            "{sort_author_desc_selected}",
            selected_sort(options.sort, WorkListSort::AuthorDesc),
        )
        .replace(
            "{per_page_10_selected}",
            selected_usize(options.per_page, 10),
        )
        .replace(
            "{per_page_25_selected}",
            selected_usize(options.per_page, 25),
        )
        .replace(
            "{per_page_50_selected}",
            selected_usize(options.per_page, 50),
        )
        .replace(
            "{per_page_100_selected}",
            selected_usize(options.per_page, 100),
        )
        .replace("{initial_table_head}", &render_initial_site_head())
        .replace("{initial_table_rows}", &initial_rows)
        .replace(
            "{page_info}",
            &format!("{} / {}（全{}件）", options.page, total_pages, page.total),
        )
        .replace("{previous_page_link}", &previous_page_link)
        .replace("{next_page_link}", &next_page_link))
}

fn selected(actual: Option<&str>, expected: Option<&str>) -> &'static str {
    if actual == expected { " selected" } else { "" }
}

fn selected_sort(actual: WorkListSort, expected: WorkListSort) -> &'static str {
    if actual == expected { " selected" } else { "" }
}

fn selected_usize(actual: usize, expected: usize) -> &'static str {
    if actual == expected { " selected" } else { "" }
}

fn render_page_link(
    options: &SiteIndexRenderOptions,
    page: usize,
    label: &str,
    enabled: bool,
) -> Result<String> {
    if !enabled {
        return Ok(format!(
            r#"<span class="btn btn-outline" aria-disabled="true">{}</span>"#,
            escape_html(label)
        ));
    }
    let mut query = vec![
        ("page", page.to_string()),
        ("per_page", options.per_page.to_string()),
        ("sort", work_list_sort_query_value(options.sort).to_string()),
    ];
    if let Some(value) = options.search.as_deref() {
        query.push(("search", value.to_string()));
    }
    if let Some(value) = options.work_type.as_deref() {
        query.push(("type", value.to_string()));
    }
    if let Some(value) = options.serialization.as_deref() {
        query.push(("serialization", value.to_string()));
    }
    let href = format!("?{}", serde_urlencoded::to_string(query)?);
    Ok(format!(
        r#"<a class="btn btn-outline" href="{}">{}</a>"#,
        escape_html(&href),
        escape_html(label)
    ))
}

fn work_list_sort_query_value(sort: WorkListSort) -> &'static str {
    match sort {
        WorkListSort::UpdatedDesc => "updated_desc",
        WorkListSort::UpdatedAsc => "updated_asc",
        WorkListSort::TitleAsc => "title_asc",
        WorkListSort::TitleDesc => "title_desc",
        WorkListSort::AuthorAsc => "author_asc",
        WorkListSort::AuthorDesc => "author_desc",
    }
}

fn render_initial_site_head() -> String {
    [
        r#"<tr><th class="th-serialization" style="width:10ch">連載状況</th>"#,
        r#"<th class="th-title" style="width:50%">タイトル</th>"#,
        r#"<th class="th-author" style="width:20%">作者名</th>"#,
        r#"<th class="th-type" style="width:8ch">形式</th>"#,
        r#"<th class="th-tags" style="width:30%">タグ</th>"#,
        r#"<th class="th-create_date" style="width:14ch">掲載日時</th>"#,
        r#"<th class="th-update_date" style="width:14ch">更新日時</th></tr>"#,
    ]
    .concat()
}

fn render_initial_site_rows(entries: &[(String, SiteListEntry)]) -> String {
    if entries.is_empty() {
        return r#"<tr><td colspan="7">該当する作品はありません。</td></tr>"#.to_string();
    }

    let mut html = String::new();
    for (work_key, entry) in entries {
        let tags = entry
            .all_tags
            .iter()
            .map(|tag| format!(r#"<span class="tag-item">{}</span>"#, escape_html(tag)))
            .collect::<String>();
        let author = entry
            .author_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .map(|url| {
                format!(
                    r#"<a href="{}">{}</a>"#,
                    escape_html(url),
                    escape_html(&entry.author)
                )
            })
            .unwrap_or_else(|| escape_html(&entry.author));
        html.push_str(&format!(
            concat!(
                "<tr>",
                r#"<td class="td-serialization" style="width:10ch">{serialization}</td>"#,
                r#"<td class="td-title" style="width:50%"><a href="./{key}/">{title}</a></td>"#,
                r#"<td class="td-author" style="width:20%">{author}</td>"#,
                r#"<td class="td-type" style="width:8ch">{work_type}</td>"#,
                r#"<td class="td-tags" style="width:30%">{tags}</td>"#,
                r#"<td class="td-create_date" style="width:14ch">{create_date}</td>"#,
                r#"<td class="td-update_date" style="width:14ch">{update_date}</td>"#,
                "</tr>"
            ),
            key = escape_html(work_key),
            serialization = escape_html(&entry.serialization),
            title = escape_html(&entry.title),
            author = author,
            work_type = if entry.work_type == "novel" {
                "小説"
            } else {
                "漫画"
            },
            tags = tags,
            create_date = escape_html(&format_site_index_date(&entry.create_date)),
            update_date = escape_html(&format_site_index_date(&entry.update_date)),
        ));
    }
    html
}

fn format_site_index_date(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.format("%Y/%m/%d %H:%M").to_string())
        .unwrap_or_else(|_| value.to_string())
}

pub fn render_work_root_html_from_store(
    store: &Store,
    site: &str,
    work_key: &str,
    host_name: &str,
    img_url: &str,
) -> Result<Option<String>> {
    let Some(work) = store.get_work(site, work_key)? else {
        return Ok(None);
    };
    let rendered = rendered_work_from_record(&work)?;
    let images = load_image_assets_for_work(store, site, work_key, &work.raw_json)?;
    Ok(Some(render_work_root_html(
        &rendered, host_name, img_url, &images,
    )))
}

pub fn render_work_info_html_from_store(
    store: &Store,
    site: &str,
    work_key: &str,
    host_name: &str,
    img_url: &str,
) -> Result<Option<String>> {
    let Some(work) = store.get_work(site, work_key)? else {
        return Ok(None);
    };
    let rendered = rendered_work_from_record(&work)?;
    let images = load_image_assets_for_work(store, site, work_key, &work.raw_json)?;
    Ok(Some(render_work_info(
        &rendered.site,
        &rendered.work_key,
        &rendered,
        host_name,
        img_url,
        &images,
    )))
}

pub fn render_episode_html_from_store(
    store: &Store,
    site: &str,
    work_key: &str,
    episode_id: &str,
    host_name: &str,
    img_url: &str,
) -> Result<Option<String>> {
    let Some(work) = store.get_work(site, work_key)? else {
        return Ok(None);
    };
    let rendered = rendered_work_from_record(&work)?;
    let Some((index, episode_key, episode)) = find_episode_for_route(&rendered, episode_id) else {
        return Ok(None);
    };
    let images = load_image_assets_for_work(store, site, work_key, &work.raw_json)?;
    let prev = index.checked_sub(1).and_then(|idx| {
        rendered
            .episodes
            .get(idx)
            .map(|(_, episode)| episode.id.as_str())
    });
    let next = rendered
        .episodes
        .get(index + 1)
        .map(|(_, episode)| episode.id.as_str());
    Ok(Some(render_episode_page(
        &rendered.site,
        &rendered.work_key,
        &rendered,
        episode_key,
        episode,
        prev,
        next,
        host_name,
        img_url,
        &images,
    )))
}

fn load_image_assets_for_work(
    store: &Store,
    site: &str,
    work_key: &str,
    raw_json: &serde_json::Value,
) -> Result<HashMap<String, ImageAsset>> {
    const IMAGE_EXTENSIONS: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "apng", "webp", "bmp", "avif", "svg",
    ];
    let mut candidates = Vec::new();
    for ext in IMAGE_EXTENSIONS {
        candidates.push(format!("{site}_{work_key}_cover.{ext}"));
    }
    collect_narou_cover_candidates(raw_json, false, &mut candidates);

    let mut images = HashMap::new();
    for logical_name in candidates {
        if split_hashed_name(&logical_name).is_some() || images.contains_key(&logical_name) {
            continue;
        }
        if let Some(image) = store.get_image(&logical_name)? {
            images.insert(
                image.logical_name,
                ImageAsset {
                    hash: image.hash,
                    ext: image.ext,
                },
            );
        }
    }
    Ok(images)
}

fn render_work_root_html(
    rendered: &RenderedWork,
    host_name: &str,
    img_url: &str,
    images: &HashMap<String, ImageAsset>,
) -> String {
    if rendered.work.serialization == "短編" {
        if let Some((episode_key, episode)) = rendered.episodes.first() {
            return render_episode_page(
                &rendered.site,
                &rendered.work_key,
                rendered,
                episode_key,
                episode,
                None,
                None,
                host_name,
                img_url,
                images,
            );
        }
    }

    render_work_index(
        &rendered.site,
        &rendered.work_key,
        rendered,
        host_name,
        img_url,
        images,
    )
}

fn find_episode_for_route<'a>(
    rendered: &'a RenderedWork,
    episode_id: &str,
) -> Option<(usize, &'a str, &'a RawEpisode)> {
    rendered
        .episodes
        .iter()
        .enumerate()
        .find(|(_, (episode_key, episode))| episode_key == episode_id || episode.id == episode_id)
        .map(|(index, (episode_key, episode))| (index, episode_key.as_str(), episode))
}

fn render_work_index(
    site: &str,
    work_key: &str,
    rendered: &RenderedWork,
    host_name: &str,
    img_url: &str,
    images: &HashMap<String, ImageAsset>,
) -> String {
    let work = &rendered.work;
    let page_title = format!("{} - {}", work.title, site);
    let nav = format!(
        r#"<a href="./info/index.html" class="header-nav-link">作品情報</a><a href="{reader_url}" class="header-nav-link">簡易リーダー</a>"#,
        reader_url = escape_html(&reader_url(host_name, site, work_key)),
    );
    let body = format!(
        r#"<p class="novel_title">{title}</p>
<div class="index_box">
{episodes}
</div>"#,
        title = escape_html(&work.title),
        episodes = render_episode_index_content(&rendered.episodes)
    );

    let canonical_url =
        format_canonical_url(host_name, &format!("/{}/{}/index.html", site, work_key));
    let extra_head = build_og_tags(
        &page_title,
        &work.caption,
        "article",
        &canonical_url,
        &cover_image_url(host_name, img_url, site, work_key, images),
    );

    render_novel_page(&page_title, &extra_head, &body, "../", &work.title, &nav)
}

fn render_work_info(
    site: &str,
    work_key: &str,
    rendered: &RenderedWork,
    host_name: &str,
    img_url: &str,
    images: &HashMap<String, ImageAsset>,
) -> String {
    let work = &rendered.work;
    let page_title = format!("{} info", work.title);
    let nav = format!(
        r#"<a href="{reader_url}" class="header-nav-link">簡易リーダー</a>"#,
        reader_url = escape_html(&reader_url(host_name, site, work_key))
    );

    let body = render_info_content(rendered, work_key);

    let canonical_url = format_canonical_url(
        host_name,
        &format!("/{}/{}/info/index.html", site, work_key),
    );
    let extra_head = build_og_tags(
        &page_title,
        &work.caption,
        "article",
        &canonical_url,
        &cover_image_url(host_name, img_url, site, work_key, images),
    );

    render_novel_page(&page_title, &extra_head, &body, "../", &work.title, &nav)
}

fn render_episode_page(
    site: &str,
    work_key: &str,
    rendered: &RenderedWork,
    _episode_key: &str,
    episode: &RawEpisode,
    _prev: Option<&str>,
    _next: Option<&str>,
    host_name: &str,
    img_url: &str,
    images: &HashMap<String, ImageAsset>,
) -> String {
    let page_title = format!("{} - {}", episode.title, rendered.work.title);
    let is_short_story = rendered.work.serialization == "短編";
    let nav = if is_short_story {
        format!(
            r#"<a href="./info/index.html" class="header-nav-link">作品情報</a><a href="{reader_url}" class="header-nav-link">簡易リーダー</a>"#,
            reader_url = escape_html(&reader_url(host_name, site, work_key))
        )
    } else {
        format!(
            r#"<a href="../info/index.html" class="header-nav-link">作品情報</a><a href="{reader_url}" class="header-nav-link">簡易リーダー</a>"#,
            reader_url = escape_html(&format!(
                "{}&eid={}",
                reader_url(host_name, site, work_key),
                escape_html_attr(&episode.id)
            ))
        )
    };
    let image_path_base = inline_image_base(img_url, is_short_story);

    let mut preface_paragraph_id = 1usize;
    let introduction_html = render_rich_text(
        &episode.introduction,
        site,
        work_key,
        images,
        image_path_base.as_ref(),
        "Lp",
        &mut preface_paragraph_id,
    );
    let mut body_paragraph_id = 1usize;
    let text_html = render_rich_text(
        &episode.text,
        site,
        work_key,
        images,
        image_path_base.as_ref(),
        "L",
        &mut body_paragraph_id,
    );
    let mut postscript_paragraph_id = 1usize;
    let postscript_html = render_rich_text(
        &episode.postscript,
        site,
        work_key,
        images,
        image_path_base.as_ref(),
        "La",
        &mut postscript_paragraph_id,
    );
    let introduction_block = if introduction_html.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="episode-block introduction">{introduction_html}</div>"#)
    };
    let postscript_block = if postscript_html.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="episode-block postscript">{postscript_html}</div>"#)
    };

    let body = format!(
        r#"{body_title}
{introduction_block}
<div class="js-novel-text p-novel__text">{text_html}</div>
{postscript_block}
<script src="/script/image.js"></script>"#,
        body_title = if is_short_story {
            format!(
                r#"<p class="novel_title">{}</p>"#,
                escape_html(&rendered.work.title)
            )
        } else {
            format!(
                r#"<p class="novel_subtitle">{}</p>"#,
                escape_html(&episode.title)
            )
        },
        introduction_block = if introduction_block.is_empty() {
            String::new()
        } else {
            introduction_block.replace(
                r#"<div class="episode-block introduction">"#,
                r#"<div class="js-novel-text p-novel__text p-novel__text--preface">"#,
            )
        },
        text_html = text_html,
        postscript_block = if postscript_block.is_empty() {
            String::new()
        } else {
            postscript_block.replace(
                r#"<div class="episode-block postscript">"#,
                r#"<div class="js-novel-text p-novel__text p-novel__text--afterword">"#,
            )
        }
    );

    let canonical_url = if is_short_story {
        format_canonical_url(host_name, &format!("/{}/{}/index.html", site, work_key))
    } else {
        format_canonical_url(
            host_name,
            &format!("/{}/{}/{}/index.html", site, work_key, episode.id),
        )
    };
    let extra_head = format!(
        "{}\n<style>\n            img {{ display: block; }}\n        </style>",
        build_og_tags(
            &page_title,
            &episode.introduction,
            "article",
            &canonical_url,
            &cover_image_url(host_name, img_url, site, work_key, images),
        )
    );

    render_novel_page(
        &page_title,
        &extra_head,
        &body,
        if is_short_story { "../" } else { "../" },
        if is_short_story {
            &rendered.work.title
        } else {
            &episode.title
        },
        &nav,
    )
}

fn render_episode_index_content(episodes: &[(String, RawEpisode)]) -> String {
    let mut items = Vec::new();
    let mut current_chapter: Option<&str> = None;

    items.push(r#"<div class="p-eplist">"#.to_string());
    for (_, episode) in episodes {
        let chapter = episode.chapter.as_deref();
        if chapter != current_chapter {
            current_chapter = chapter;
            if let Some(chapter) = chapter.filter(|chapter| !chapter.is_empty()) {
                items.push(format!(
                    r#"<div class="p-eplist__chapter-title">{}</div>"#,
                    escape_html(chapter)
                ));
            }
        }

        let create_date = format_date_jp_or_default(&episode.create_date, "%Y/%m/%d %H:%M");
        let update_date = format_date_jp_or_default(&episode.update_date, "%Y/%m/%d %H:%M");
        items.push(format!(
            r#"<div class="p-eplist__sublist">
<a href="./{episode_id}/index.html" class="p-eplist__subtitle">
{title}
</a>
<div class="p-eplist__update">
{create_date}
<span title="{update_date} 改稿">（<u>改</u>）</span>
</div>
</div>"#,
            episode_id = escape_html(&episode.id),
            title = escape_html(&episode.title),
            create_date = escape_html(&create_date),
            update_date = escape_html(&update_date),
        ));
    }
    items.push("</div>".to_string());

    items.join("\n")
}

fn render_novel_page(
    title: &str,
    extra_head: &str,
    body_content: &str,
    back_url: &str,
    header_title: &str,
    nav_links: &str,
) -> String {
    NOVEL_PAGE_TEMPLATE
        .replace("{title}", &escape_html(title))
        .replace("{extra_head}", extra_head)
        .replace("{body}", body_content)
        .replace("{back_url}", &escape_html(back_url))
        .replace("{header_title}", &escape_html(header_title))
        .replace("{nav_links}", nav_links)
}

fn render_rich_text(
    text: &str,
    site: &str,
    work_key: &str,
    images: &HashMap<String, ImageAsset>,
    image_path_base: &str,
    id_prefix: &str,
    paragraph_id: &mut usize,
) -> String {
    let mut blocks = Vec::new();
    let mut current = Vec::new();

    for line in normalize_newlines(text).lines() {
        let trimmed = line.trim();
        if trimmed == "[newpage]" {
            if !current.is_empty() {
                blocks.push(render_text_block(
                    &current.join("\n"),
                    site,
                    work_key,
                    images,
                    image_path_base,
                    id_prefix,
                    paragraph_id,
                ));
                current.clear();
            }
            blocks.push("<hr class=\"page-break\">".to_string());
            continue;
        }

        if !trimmed.is_empty() && current.is_empty() && is_standalone_markup(trimmed) {
            blocks.push(render_markup_line(
                trimmed,
                site,
                work_key,
                images,
                image_path_base,
                id_prefix,
                paragraph_id,
            ));
            continue;
        }

        current.push(line.to_string());
    }

    if !current.is_empty() {
        blocks.push(render_text_block(
            &current.join("\n"),
            site,
            work_key,
            images,
            image_path_base,
            id_prefix,
            paragraph_id,
        ));
    }

    blocks.join("\n")
}

fn render_text_block(
    text: &str,
    site: &str,
    work_key: &str,
    images: &HashMap<String, ImageAsset>,
    image_path_base: &str,
    id_prefix: &str,
    paragraph_id: &mut usize,
) -> String {
    let mut paragraphs = Vec::new();
    for line in normalize_newlines(text).split('\n') {
        let line = line.trim_end();
        let html = if line.is_empty() {
            format!(r#"<p id="{id_prefix}{paragraph_id}"><br/></p>"#)
        } else {
            format!(
                r#"<p id="{id_prefix}{paragraph_id}">{}</p>"#,
                render_inline_markup(line, site, work_key, images, image_path_base)
            )
        };
        paragraphs.push(html);
        *paragraph_id += 1;
    }
    paragraphs.join("\n")
}

fn render_markup_line(
    line: &str,
    site: &str,
    work_key: &str,
    images: &HashMap<String, ImageAsset>,
    image_path_base: &str,
    id_prefix: &str,
    paragraph_id: &mut usize,
) -> String {
    if let Some(inner) = line
        .strip_prefix("[image](")
        .and_then(|s| s.strip_suffix(')'))
    {
        let result = format!(
            r#"<p id="{id_prefix}{paragraph_id}">{}</p>"#,
            render_image(inner, images, image_path_base)
        );
        *paragraph_id += 1;
        return result;
    }
    if let Some(inner) = line
        .strip_prefix("[ruby:<")
        .and_then(|s| s.strip_suffix(']'))
    {
        if let Some((base, reading)) = inner.split_once(">(") {
            let reading = reading.trim_end_matches(')');
            let result = format!(
                r#"<p id="{id_prefix}{paragraph_id}"><ruby>{}<rp>(</rp><rt>{}</rt><rp>)</rp></ruby></p>"#,
                escape_html(base),
                escape_html(reading)
            );
            *paragraph_id += 1;
            return result;
        }
    }
    if let Some(target) = line
        .strip_prefix("[jump:")
        .and_then(|s| s.strip_suffix(']'))
    {
        let result = format!(
            "<p id=\"{id_prefix}{paragraph_id}\"><a href=\"#L{target}\">{target}</a></p>",
            target = escape_html(target)
        );
        *paragraph_id += 1;
        return result;
    }
    let result = format!(
        r#"<p id="{id_prefix}{paragraph_id}">{}</p>"#,
        render_inline_markup(line, site, work_key, images, image_path_base)
    );
    *paragraph_id += 1;
    result
}

fn render_inline_markup(
    text: &str,
    site: &str,
    work_key: &str,
    images: &HashMap<String, ImageAsset>,
    image_path_base: &str,
) -> String {
    let mut out = String::new();
    let mut remainder = text;

    while !remainder.is_empty() {
        if let Some(rest) = remainder.strip_prefix("[image](") {
            if let Some(end) = rest.find(')') {
                let logical = &rest[..end];
                out.push_str(&render_image(logical, images, image_path_base));
                remainder = &rest[end + 1..];
                continue;
            }
        }

        if let Some(rest) = remainder.strip_prefix("[ruby:<") {
            if let Some(base_end) = rest.find(">(") {
                let base = &rest[..base_end];
                let rest = &rest[base_end + 2..];
                if let Some(reading_end) = rest.find(")]") {
                    let reading = &rest[..reading_end];
                    out.push_str(&format!(
                        "<ruby>{}<rp>(</rp><rt>{}</rt><rp>)</rp></ruby>",
                        escape_html(base),
                        escape_html(reading)
                    ));
                    remainder = &rest[reading_end + 2..];
                    continue;
                }
            }
        }

        if let Some(rest) = remainder.strip_prefix("[jump:") {
            if let Some(end) = rest.find(']') {
                let target = &rest[..end];
                out.push_str(&format!(
                    "<a href=\"#L{target}\">{target}</a>",
                    target = escape_html(target)
                ));
                remainder = &rest[end + 1..];
                continue;
            }
        }

        let ch = remainder.chars().next().unwrap();
        out.push_str(&escape_html(&ch.to_string()));
        remainder = &remainder[ch.len_utf8()..];
    }

    // The site/work arguments are kept in the signature so the renderer can evolve
    // to site-specific handling without changing the call sites again.
    let _ = (site, work_key);
    out
}

fn render_image(
    logical_name: &str,
    images: &HashMap<String, ImageAsset>,
    image_path_base: &str,
) -> String {
    if let Some(image) = images.get(logical_name) {
        return format!(
            r#"<img src="{}/{}.{}" alt="{}">"#,
            escape_html(image_path_base),
            escape_html(&image.hash),
            escape_html(&image.ext),
            escape_html(logical_name)
        );
    }

    // Canonical work data may already contain a content-addressed filename.
    // Accept only the supported hash formats and image extensions.
    if split_hashed_name(logical_name).is_some() {
        return format!(
            r#"<img src="{}/{}" alt="">"#,
            escape_html(image_path_base),
            escape_html(logical_name)
        );
    }

    format!("<p><code>{}</code></p>", escape_html(logical_name))
}

fn split_hashed_name(name: &str) -> Option<(&str, &str)> {
    let dot = name.rfind('.')?;
    let stem = &name[..dot];
    let ext = &name[dot + 1..];
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    if !looks_like_image_hash(stem) {
        return None;
    }
    if !matches!(
        ext.to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "apng" | "webp" | "bmp" | "avif" | "svg"
    ) {
        return None;
    }
    Some((stem, ext))
}

fn looks_like_image_hash(hash: &str) -> bool {
    looks_like_sha3_base64url(hash) || looks_like_legacy_hex_hash(hash)
}

fn looks_like_sha3_base64url(hash: &str) -> bool {
    hash.len() == 43
        && hash
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn looks_like_legacy_hex_hash(hash: &str) -> bool {
    hash.len() >= 10 && hash.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn parse_raw_work(value: &serde_json::Value) -> Result<RawWork> {
    serde_json::from_value(value.clone()).context("invalid raw.json structure")
}

fn sort_episodes(episodes: &BTreeMap<String, RawEpisode>) -> Vec<(String, RawEpisode)> {
    let mut items: Vec<_> = episodes
        .iter()
        .map(|(key, episode)| (key.clone(), episode.clone()))
        .collect();
    items.sort_by(|a, b| episode_sort_key(&a.0).cmp(&episode_sort_key(&b.0)));
    items
}

fn episode_sort_key(value: &str) -> (i64, String) {
    (value.parse::<i64>().unwrap_or(i64::MAX), value.to_string())
}

fn reader_url(host_name: &str, site: &str, work_key: &str) -> String {
    let base = host_name.trim_end_matches('/');
    if base.is_empty() {
        format!("/reader/?site={site}&nid={work_key}")
    } else {
        format!("{base}/reader/?site={site}&nid={work_key}")
    }
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn format_date_jp(iso_date: &str, format: &str) -> String {
    DateTime::parse_from_rfc3339(iso_date)
        .map(|date| date.with_timezone(&Utc).format(format).to_string())
        .unwrap_or_default()
}

fn format_date_jp_or_default(iso_date: &str, format: &str) -> String {
    let formatted = format_date_jp(iso_date, format);
    if formatted.is_empty() {
        iso_date.to_string()
    } else {
        formatted
    }
}

fn render_info_content(rendered: &RenderedWork, _work_key: &str) -> String {
    let work = &rendered.work;
    let author_html =
        if let Some(author_url) = work.author_url.as_deref().filter(|url| !url.is_empty()) {
            format!(
                r#"<a href="{author_url}" target="_blank">{author}</a>"#,
                author_url = escape_html(author_url),
                author = escape_html(&work.author)
            )
        } else {
            escape_html(&work.author)
        };
    let all_tags = if work.all_tags.is_empty() {
        "\n<span>キーワードが設定されていません</span>\n".to_string()
    } else {
        work.all_tags
            .iter()
            .map(|tag| escape_html(tag))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let novel_type = if work.serialization == "短編" {
        r#"<div><span id="noveltype">短編</span></div>"#.to_string()
    } else {
        format!(
            r#"<div><span id="noveltype">{}</span> 全{}エピソード</div>"#,
            escape_html(&work.serialization),
            rendered.episodes.len()
        )
    };
    let date_label = if work.serialization == "短編" {
        "最終更新日"
    } else if work.serialization == "連載中" {
        "最新掲載日"
    } else {
        "最終掲載日"
    };

    format!(
        r#"<h1><a href="{url}" target="_blank">{title}</a></h1>
{novel_type}
<table>
<tr><th class="ex">あらすじ</th><td class="ex">{caption}</td></tr>
<tr><th>作者名</th><td>{author}</td></tr>
<tr><th>キーワード</th><td>{all_tags}</td></tr>
<tr><th>掲載日</th><td>{create_date}</td></tr>
<tr><th>{date_label}</th><td>{update_date}</td></tr>
<tr><th>文字数</th><td>{total_characters}文字</td></tr>
</table>"#,
        url = escape_html(&work.url),
        title = escape_html(&work.title),
        novel_type = novel_type,
        caption = escape_html(&work.caption),
        author = author_html,
        all_tags = all_tags,
        create_date = escape_html(&format_date_jp(&work.create_date, "%Y年 %m月%d日 %H時%M分")),
        date_label = date_label,
        update_date = escape_html(&format_date_jp(&work.update_date, "%Y年 %m月%d日 %H時%M分")),
        total_characters = work.total_characters,
    )
}

fn is_standalone_markup(line: &str) -> bool {
    line.starts_with("[image](") || line.starts_with("[ruby:<") || line.starts_with("[jump:")
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn build_og_tags(title: &str, description: &str, og_type: &str, url: &str, image: &str) -> String {
    let mut tags = String::new();

    if !title.is_empty() {
        tags.push_str(&format!(
            "  <meta property=\"og:title\" content=\"{}\">\n",
            escape_html_attr(title)
        ));
    }

    if !description.is_empty() {
        tags.push_str(&format!(
            "  <meta property=\"og:description\" content=\"{}\">\n",
            escape_html_attr(description)
        ));
        tags.push_str(&format!(
            "  <meta name=\"description\" content=\"{}\">\n",
            escape_html_attr(description)
        ));
    }

    if !og_type.is_empty() {
        tags.push_str(&format!(
            "  <meta property=\"og:type\" content=\"{}\">\n",
            escape_html_attr(og_type)
        ));
    }

    if !url.is_empty() {
        tags.push_str(&format!(
            "  <link rel=\"canonical\" href=\"{}\">\n",
            escape_html_attr(url)
        ));
        tags.push_str(&format!(
            "  <meta property=\"og:url\" content=\"{}\">\n",
            escape_html_attr(url)
        ));
    }

    if !image.is_empty() {
        tags.push_str(&format!(
            "  <meta property=\"og:image\" content=\"{}\">\n",
            escape_html_attr(image)
        ));
    }

    tags.trim_end().to_string()
}

fn escape_html_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_canonical_url(host_name: &str, path: &str) -> String {
    let base = host_name.trim_end_matches('/');
    if base.is_empty() {
        path.to_string()
    } else {
        format!("{}{}", base, path)
    }
}

fn format_optional_url(host_name: &str, path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format_canonical_url(host_name, path)
    }
}

fn inline_image_base<'a>(img_url: &'a str, is_short_story: bool) -> std::borrow::Cow<'a, str> {
    let img_url = img_url.trim();
    if img_url.is_empty() {
        if is_short_story {
            std::borrow::Cow::Borrowed("../../images")
        } else {
            std::borrow::Cow::Borrowed("../../../images")
        }
    } else {
        std::borrow::Cow::Owned(img_url.trim_end_matches('/').to_string())
    }
}

fn cover_image_url(
    host_name: &str,
    img_url: &str,
    site: &str,
    work_key: &str,
    images: &HashMap<String, ImageAsset>,
) -> String {
    let Some(file_name) = get_cover_image_file_name(site, work_key, images) else {
        return String::new();
    };

    let img_url = img_url.trim();
    if img_url.is_empty() {
        format_optional_url(host_name, &format!("/images/{file_name}"))
    } else {
        format!("{}/{}", img_url.trim_end_matches('/'), file_name)
    }
}

fn get_cover_image_file_name(
    site: &str,
    work_key: &str,
    images: &HashMap<String, ImageAsset>,
) -> Option<String> {
    const COVER_EXTENSIONS: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "apng", "webp", "bmp", "avif", "svg",
    ];
    for ext in COVER_EXTENSIONS {
        let logical_name = format!("{site}_{work_key}_cover.{ext}");
        if let Some(image) = images.get(&logical_name) {
            return Some(format!("{}.{}", image.hash, image.ext));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_raw_json() -> serde_json::Value {
        json!({
            "version": 0,
            "get_date": "2025-01-01T00:00:00Z",
            "title": "Example Title",
            "id": "n123",
            "nid": "n123",
            "url": "https://example.invalid/n123",
            "author": "Example Author",
            "author_id": null,
            "author_url": null,
            "caption": "Example Caption",
            "total_episodes": 1,
            "all_episodes": 1,
            "total_characters": 3,
            "all_characters": 3,
            "type": "novel",
            "serialization": "短編",
            "tags": [],
            "all_tags": [],
            "createDate": "2025-01-01T00:00:00Z",
            "updateDate": "2025-01-01T00:00:00Z",
            "episodes": {
                "1": {
                    "id": "1",
                    "chapter": null,
                    "title": "Episode 1",
                    "textCount": 3,
                    "tags": [],
                    "introduction": "",
                    "text": "abc",
                    "postscript": "",
                    "createDate": "2025-01-01T00:00:00Z",
                    "updateDate": "2025-01-01T00:00:00Z"
                }
            }
        })
    }

    #[test]
    fn render_image_uses_logical_name_lookup() {
        let mut images = HashMap::new();
        images.insert(
            "pixiv_n123_cover.jpg".to_string(),
            ImageAsset {
                hash: "abc1234567".to_string(),
                ext: "jpg".to_string(),
            },
        );

        let html = render_image("pixiv_n123_cover.jpg", &images, "../../images");
        assert_eq!(
            html,
            r#"<img src="../../images/abc1234567.jpg" alt="pixiv_n123_cover.jpg">"#
        );
    }

    #[test]
    fn render_image_resolves_legacy_hex_hashed_filename() {
        let mut images = HashMap::new();
        images.insert(
            "pixiv_n123_cover.jpg".to_string(),
            ImageAsset {
                hash: "abc1234567".to_string(),
                ext: "jpg".to_string(),
            },
        );

        let html = render_image("abc1234567.jpg", &images, "../../images");
        assert_eq!(html, r#"<img src="../../images/abc1234567.jpg" alt="">"#);
    }

    #[test]
    fn render_image_resolves_base64url_style_hashed_filename() {
        let hashed_name = "0123456789012345678901234567890123456789012";
        let mut images = HashMap::new();
        images.insert(
            "pixiv_n123_page1.jpg".to_string(),
            ImageAsset {
                hash: hashed_name.to_string(),
                ext: "jpg".to_string(),
            },
        );

        let html = render_image(&format!("{hashed_name}.jpg"), &images, "../../images");
        assert_eq!(
            html,
            format!(r#"<img src="../../images/{hashed_name}.jpg" alt="">"#)
        );
    }

    #[test]
    fn render_image_preserves_valid_unregistered_content_hash() {
        let images = HashMap::new();
        let hashed_name = "F2sqzifMxxNCurmu7wAvjp69qcDNJn7JOCuvA-23xwk.png";

        let html = render_image(hashed_name, &images, "https://images.example");

        assert_eq!(
            html,
            format!(r#"<img src="https://images.example/{hashed_name}" alt="">"#)
        );
    }

    #[test]
    fn render_image_rejects_non_image_hash_extension() {
        let images = HashMap::new();
        let hashed_name = "F2sqzifMxxNCurmu7wAvjp69qcDNJn7JOCuvA-23xwk.html";

        let html = render_image(hashed_name, &images, "https://images.example");

        assert_eq!(html, format!("<p><code>{hashed_name}</code></p>"));
    }

    #[test]
    fn render_image_falls_back_for_unknown_name() {
        let images: HashMap<String, ImageAsset> = HashMap::new();
        let html = render_image("not-an-image", &images, "../../images");
        assert_eq!(html, "<p><code>not-an-image</code></p>");
    }

    #[test]
    fn cover_lookup_does_not_borrow_another_works_cover() {
        let mut images = HashMap::new();
        images.insert(
            "pixiv_n999_cover.jpg".to_string(),
            ImageAsset {
                hash: "abc1234567".to_string(),
                ext: "jpg".to_string(),
            },
        );

        assert_eq!(get_cover_image_file_name("pixiv", "n123", &images), None);
    }

    #[test]
    fn render_inline_markup_uses_canonical_ruby_markup() {
        let html = render_inline_markup(
            "[ruby:<漢字>(かな)]",
            "narou",
            "n123",
            &HashMap::new(),
            "../../images",
        );
        assert_eq!(html, "<ruby>漢字<rp>(</rp><rt>かな</rt><rp>)</rp></ruby>");
    }

    #[test]
    fn render_work_info_uses_structured_table_without_raw_dump() {
        let raw = sample_raw_json();
        let rendered = build_rendered_work(
            parse_raw_work(&raw).expect("parse raw"),
            "narou".to_string(),
            "n123".to_string(),
        );

        let html = render_work_info("narou", "n123", &rendered, "", "", &HashMap::new());

        assert!(html.contains("<table>"));
        assert!(html.contains("あらすじ"));
        assert!(!html.contains("<pre>"));
        assert!(!html.contains("version:"));
    }

    #[test]
    fn render_work_index_includes_canonical_and_og_image() {
        let raw = sample_raw_json();
        let rendered = build_rendered_work(
            parse_raw_work(&raw).expect("parse raw"),
            "narou".to_string(),
            "n123".to_string(),
        );
        let mut images = HashMap::new();
        images.insert(
            "narou_n123_cover.jpg".to_string(),
            ImageAsset {
                hash: "coverhash".to_string(),
                ext: "jpg".to_string(),
            },
        );

        let html = render_work_index(
            "narou",
            "n123",
            &rendered,
            "https://example.invalid",
            "",
            &images,
        );

        assert!(html.contains(
            r#"<link rel="canonical" href="https://example.invalid/narou/n123/index.html">"#
        ));
        assert!(html.contains(
            r#"<meta property="og:image" content="https://example.invalid/images/coverhash.jpg">"#
        ));
    }

    #[test]
    fn render_work_index_uses_img_url_override_for_og_image() {
        let raw = sample_raw_json();
        let rendered = build_rendered_work(
            parse_raw_work(&raw).expect("parse raw"),
            "narou".to_string(),
            "n123".to_string(),
        );
        let mut images = HashMap::new();
        images.insert(
            "narou_n123_cover.jpg".to_string(),
            ImageAsset {
                hash: "coverhash".to_string(),
                ext: "jpg".to_string(),
            },
        );

        let html = render_work_index(
            "narou",
            "n123",
            &rendered,
            "https://example.invalid",
            "https://cdn.example.invalid/novels/",
            &images,
        );

        assert!(html.contains(
            r#"<meta property="og:image" content="https://cdn.example.invalid/novels/coverhash.jpg">"#
        ));
    }

    #[test]
    fn render_episode_page_does_not_duplicate_introduction_summary() {
        let mut raw = sample_raw_json();
        raw["episodes"]["1"]["introduction"] = json!("導入文");
        let rendered = build_rendered_work(
            parse_raw_work(&raw).expect("parse raw"),
            "narou".to_string(),
            "n123".to_string(),
        );
        let (_, episode) = rendered.episodes.first().expect("episode");

        let html = render_episode_page(
            "narou",
            "n123",
            &rendered,
            "1",
            episode,
            None,
            None,
            "",
            "",
            &HashMap::new(),
        );

        assert!(!html.contains(r#"<p class="summary">導入文</p>"#));
        assert!(
            html.contains(
                r#"<div class="js-novel-text p-novel__text p-novel__text--preface"><p id="Lp1">導入文</p></div>"#
            )
        );
    }

    #[test]
    fn render_work_index_groups_episodes_by_chapter_title() {
        let mut raw = sample_raw_json();
        raw["serialization"] = json!("連載中");
        raw["total_episodes"] = json!(2);
        raw["all_episodes"] = json!(2);
        raw["episodes"]["1"]["chapter"] = json!("第一章");
        raw["episodes"]["2"] = json!({
            "id": "2",
            "chapter": "第二章",
            "title": "Episode 2",
            "textCount": 3,
            "tags": [],
            "introduction": "",
            "text": "def",
            "postscript": "",
            "createDate": "2025-01-02T00:00:00Z",
            "updateDate": "2025-01-02T00:00:00Z"
        });
        let rendered = build_rendered_work(
            parse_raw_work(&raw).expect("parse raw"),
            "narou".to_string(),
            "n123".to_string(),
        );

        let html = render_work_index("narou", "n123", &rendered, "", "", &HashMap::new());

        assert!(html.contains(r#"<div class="p-eplist__chapter-title">第一章</div>"#));
        assert!(html.contains(r#"<div class="p-eplist__chapter-title">第二章</div>"#));
    }
    #[test]
    fn site_index_ssr_pages_sqlite_results_without_embedded_library_json() {
        let store = Store::open_in_memory().expect("store");
        for index in 1..=12 {
            let title = format!("Work {index:02}");
            let mut raw_json = sample_raw_json();
            raw_json["title"] = json!(title);
            store
                .upsert_work(&WorkRecord {
                    site: "pixiv".to_string(),
                    work_key: format!("n{index:02}"),
                    title,
                    author: "Author".to_string(),
                    author_id: Some("author-1".to_string()),
                    author_url: Some("https://example.com/author-1".to_string()),
                    r#type: "novel".to_string(),
                    serialization: "連載中".to_string(),
                    caption: String::new(),
                    create_date: "2025-01-01T00:00:00Z".to_string(),
                    update_date: format!("2025-01-{index:02}T00:00:00Z"),
                    raw_json,
                })
                .expect("upsert work");
        }

        let html = render_site_index_html_from_store(
            &store,
            "pixiv",
            SiteIndexRenderOptions {
                page: 2,
                per_page: 10,
                search: None,
                work_type: Some("novel".to_string()),
                serialization: None,
                sort: WorkListSort::TitleAsc,
            },
        )
        .expect("render site index");

        assert!(html.contains("Work 11"));
        assert!(html.contains("Work 12"));
        assert!(!html.contains("Work 01"));
        assert!(html.contains("2 / 2（全12件）"));
        assert!(html.contains("page=1"));
        assert!(!html.contains("site-index-data"));
    }
}
