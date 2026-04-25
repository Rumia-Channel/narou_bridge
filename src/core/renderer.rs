use crate::core::model::{EpisodeIndexEntry, SiteIndexEntry, WorkRecord};
use crate::core::storage::Store;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

const NOVEL_PAGE_TEMPLATE: &str = include_str!("../../templates/novel_page.html");

#[derive(Debug, Clone, Deserialize)]
struct RawWork {
    #[serde(default)]
    version: i64,
    #[serde(default, rename = "get_date")]
    get_date: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    nid: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    author_id: Option<String>,
    #[serde(default)]
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
struct RawEpisode {
    #[serde(default)]
    id: String,
    #[serde(default)]
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
struct RenderedWork {
    site: String,
    work_key: String,
    raw_json: serde_json::Value,
    work: RawWork,
    index_entry: SiteIndexEntry,
    episodes: Vec<(String, RawEpisode)>,
}

pub fn refresh_image_manifests(store: &Store, data_dir: &str) -> Result<()> {
    let image_dir = PathBuf::from(data_dir).join("images");
    fs::create_dir_all(&image_dir).context("failed to create image dir")?;

    let mut database = serde_json::Map::new();
    let mut cover = serde_json::Map::new();

    for image in store.list_images()? {
        database.insert(
            image.logical_name.clone(),
            serde_json::Value::String(image.hash.clone()),
        );
        if image.kind == "cover" {
            cover.insert(image.logical_name, serde_json::Value::String(image.hash));
        }
    }

    fs::write(
        image_dir.join("database.json"),
        serde_json::to_string_pretty(&serde_json::Value::Object(database))?,
    )?;
    fs::write(
        image_dir.join("cover.json"),
        serde_json::to_string_pretty(&serde_json::Value::Object(cover))?,
    )?;
    Ok(())
}

pub fn render_site_from_store(
    store: &Store,
    site: &str,
    data_dir: &str,
    host_name: &str,
) -> Result<()> {
    refresh_image_manifests(store, data_dir)?;
    let works = store.list_works(Some(site))?;
    let site_dir = PathBuf::from(data_dir).join(site);
    fs::create_dir_all(&site_dir).context("failed to create site dir")?;

    let image_assets = load_image_assets(store)?;
    let mut rendered = Vec::new();

    for work in works {
        rendered.push(render_work(&site_dir, &work, host_name, &image_assets)?);
    }

    write_site_index(&site_dir, site, &rendered, host_name, &image_assets)?;
    update_cover_json(store, site, &site_dir)?;
    Ok(())
}

pub fn repair_site_from_raw(
    store: &Store,
    site: &str,
    data_dir: &str,
    host_name: &str,
) -> Result<()> {
    refresh_image_manifests(store, data_dir)?;
    let site_dir = PathBuf::from(data_dir).join(site);
    if !site_dir.exists() {
        bail!(
            "repair requires existing raw.json data under {}",
            site_dir.display()
        );
    }

    let image_assets = load_image_assets(store)?;
    let mut rendered = Vec::new();

    for entry in
        fs::read_dir(&site_dir).with_context(|| format!("failed to read {}", site_dir.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let work_key = entry.file_name().to_string_lossy().to_string();
        let raw_path = entry.path().join("raw").join("raw.json");
        if !raw_path.exists() {
            continue;
        }

        let raw_json: serde_json::Value = serde_json::from_reader(
            fs::File::open(&raw_path)
                .with_context(|| format!("failed to open {}", raw_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", raw_path.display()))?;

        rendered.push(render_work_from_raw(
            &site_dir,
            site,
            &work_key,
            raw_json,
            host_name,
            &image_assets,
            false,
        )?);
    }

    if rendered.is_empty() {
        bail!(
            "repair requires at least one existing raw.json under {}",
            site_dir.display()
        );
    }

    write_site_index(&site_dir, site, &rendered, host_name, &image_assets)?;
    update_cover_json(store, site, &site_dir)?;
    Ok(())
}

fn load_image_assets(store: &Store) -> Result<HashMap<String, ImageAsset>> {
    let mut images = HashMap::new();
    for image in store.list_images()? {
        images.insert(
            image.logical_name,
            ImageAsset {
                hash: image.hash,
                ext: image.ext,
            },
        );
    }
    Ok(images)
}

fn render_work(
    site_dir: &Path,
    work: &WorkRecord,
    host_name: &str,
    images: &HashMap<String, ImageAsset>,
) -> Result<RenderedWork> {
    render_work_from_raw(
        site_dir,
        &work.site,
        &work.work_key,
        work.raw_json.clone(),
        host_name,
        images,
        true,
    )
}

fn render_work_from_raw(
    site_dir: &Path,
    site: &str,
    work_key: &str,
    raw_json: serde_json::Value,
    host_name: &str,
    images: &HashMap<String, ImageAsset>,
    write_raw_json: bool,
) -> Result<RenderedWork> {
    let work_dir = site_dir.join(work_key);
    let raw_dir = work_dir.join("raw");
    let info_dir = work_dir.join("info");
    fs::create_dir_all(&raw_dir).context("failed to create raw dir")?;
    fs::create_dir_all(&info_dir).context("failed to create info dir")?;

    if write_raw_json {
        fs::write(
            raw_dir.join("raw.json"),
            serde_json::to_string_pretty(&raw_json)?,
        )?;
    }

    let raw_work = parse_raw_work(&raw_json)
        .with_context(|| format!("failed to parse raw work for {work_key}"))?;
    let rendered = build_rendered_work(raw_work, site.to_string(), work_key.to_string(), raw_json);

    let work_root_html = if rendered.work.serialization == "短編" {
        if let Some((episode_key, episode)) = rendered.episodes.first() {
            render_episode_page(
                &rendered.site,
                &rendered.work_key,
                &rendered,
                episode_key,
                episode,
                None,
                None,
                host_name,
                images,
            )
        } else {
            render_work_index(
                &rendered.site,
                &rendered.work_key,
                &rendered,
                host_name,
                images,
            )
        }
    } else {
        render_work_index(
            &rendered.site,
            &rendered.work_key,
            &rendered,
            host_name,
            images,
        )
    };
    fs::write(work_dir.join("index.html"), work_root_html)?;
    fs::write(
        info_dir.join("index.html"),
        render_work_info(
            &rendered.site,
            &rendered.work_key,
            &rendered,
            host_name,
            images,
        ),
    )?;

    for (index, (episode_key, episode)) in rendered.episodes.iter().enumerate() {
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
        let episode_path = work_dir.join(&episode.id).join("index.html");
        if let Some(parent) = episode_path.parent() {
            fs::create_dir_all(parent).context("failed to create episode dir")?;
        }
        fs::write(
            episode_path,
            render_episode_page(
                &rendered.site,
                &rendered.work_key,
                &rendered,
                episode_key,
                episode,
                prev,
                next,
                host_name,
                images,
            ),
        )?;
    }

    Ok(rendered)
}

fn build_rendered_work(
    raw_work: RawWork,
    site: String,
    work_key: String,
    raw_json: serde_json::Value,
) -> RenderedWork {
    let episodes = sort_episodes(&raw_work.episodes);
    let index_entry = SiteIndexEntry {
        title: raw_work.title.clone(),
        author: raw_work.author.clone(),
        author_id: raw_work.author_id.clone(),
        author_url: raw_work.author_url.clone(),
        r#type: raw_work.work_type.clone(),
        serialization: raw_work.serialization.clone(),
        tags: raw_work.tags.clone(),
        all_tags: raw_work.all_tags.clone(),
        caption: raw_work.caption.clone(),
        create_date: raw_work.create_date.clone(),
        update_date: raw_work.update_date.clone(),
        episodes_data: episodes
            .iter()
            .map(|(key, episode)| {
                (
                    key.clone(),
                    EpisodeIndexEntry {
                        title: episode.title.clone(),
                        id: episode.id.clone(),
                        caption: episode.introduction.clone(),
                        tags: episode.tags.clone(),
                    },
                )
            })
            .collect(),
    };

    RenderedWork {
        site,
        work_key,
        raw_json,
        work: raw_work,
        index_entry,
        episodes,
    }
}

fn write_site_index(
    site_dir: &Path,
    site: &str,
    works: &[RenderedWork],
    host_name: &str,
    images: &HashMap<String, ImageAsset>,
) -> Result<()> {
    let index_entries: BTreeMap<String, SiteIndexEntry> = works
        .iter()
        .map(|rendered| (rendered.work_key.clone(), rendered.index_entry.clone()))
        .collect();

    fs::write(
        site_dir.join("index.json"),
        serde_json::to_string_pretty(&index_entries)?,
    )?;
    fs::write(
        site_dir.join("index.html"),
        render_site_index(site, works, host_name, images),
    )?;
    Ok(())
}

fn render_site_index(
    site: &str,
    works: &[RenderedWork],
    host_name: &str,
    images: &HashMap<String, ImageAsset>,
) -> String {
    let mut cards = String::new();
    for rendered in works {
        let work = &rendered.work;
        let episode_count = rendered.episodes.len();
        let tag_list = render_tag_list(&work.tags);
        cards.push_str(&format!(
            r#"<article class="card">
  <h2><a href="./{work_key}/index.html">{title}</a></h2>
  <p class="meta">{author} · {serialization} · {episode_count} 話</p>
  <p class="meta">更新: {update_date}</p>
  <p class="summary">{caption}</p>
  {tag_list}
  <p class="links">
    <a href="./{work_key}/index.html">作品ページ</a>
    <a href="./{work_key}/info/index.html">詳細</a>
    <a href="./{work_key}/raw/raw.json">raw.json</a>
    <a href="{reader_url}">reader</a>
  </p>
</article>"#,
            work_key = escape_html(&rendered.work_key),
            title = escape_html(&work.title),
            author = escape_html(&work.author),
            serialization = escape_html(&work.serialization),
            episode_count = episode_count,
            update_date = escape_html(&work.update_date),
            caption = escape_html(&work.caption),
            tag_list = tag_list,
            reader_url = escape_html(&reader_url(host_name, site, &rendered.work_key)),
        ));
    }

    let canonical_url = format_canonical_url(host_name, &format!("/{}/", site));
    let og_image = works
        .iter()
        .find_map(|rendered| {
            let path = get_cover_image_url(site, &rendered.work_key, images);
            if path.is_empty() { None } else { Some(path) }
        })
        .unwrap_or_default();
    let og_tags = build_og_tags(
        &format!("{site} - Narou Bridge"),
        "Web小説・漫画の管理・閲覧システム",
        "website",
        &canonical_url,
        &format_optional_url(host_name, &og_image),
    );

    format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{site} index</title>
  {og_tags}
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #f6f7fb; color: #1f2933; }}
    header {{ padding: 24px 20px; background: #111827; color: #fff; }}
    main {{ max-width: 1100px; margin: 0 auto; padding: 20px; }}
    .card {{ background: #fff; border-radius: 14px; padding: 16px 18px; margin-bottom: 16px; box-shadow: 0 1px 4px rgba(0,0,0,.08); }}
    .card h2 {{ margin: 0 0 8px; font-size: 1.05rem; }}
    .card a {{ color: #2563eb; text-decoration: none; }}
    .meta {{ margin: 0 0 8px; color: #6b7280; font-size: .92rem; }}
    .summary {{ margin: 0 0 10px; line-height: 1.7; white-space: pre-wrap; }}
    .tags {{ display: flex; flex-wrap: wrap; gap: 6px; margin: 0 0 10px; }}
    .tag {{ background: #e5eefc; color: #1d4ed8; border-radius: 999px; padding: 2px 10px; font-size: .82rem; }}
    .links {{ display: flex; flex-wrap: wrap; gap: 12px; margin: 0; }}
  </style>
</head>
<body>
  <header>
    <h1>{site}</h1>
    <p>{count} works · <a href="/">home</a></p>
  </header>
  <main>
    {cards}
  </main>
</body>
</html>"#,
        site = escape_html(site),
        count = works.len(),
        cards = cards
    )
}

fn render_work_index(
    site: &str,
    work_key: &str,
    rendered: &RenderedWork,
    host_name: &str,
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
        &format_optional_url(host_name, &get_cover_image_url(site, work_key, images)),
    );

    render_novel_page(&page_title, &extra_head, &body, "../", &work.title, &nav)
}

fn render_work_info(
    site: &str,
    work_key: &str,
    rendered: &RenderedWork,
    host_name: &str,
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
        &format_optional_url(host_name, &get_cover_image_url(site, work_key, images)),
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
    let image_path_base = if is_short_story {
        "../../images"
    } else {
        "../../../images"
    };

    let mut preface_paragraph_id = 1usize;
    let introduction_html = render_rich_text(
        &episode.introduction,
        site,
        work_key,
        images,
        image_path_base,
        "Lp",
        &mut preface_paragraph_id,
    );
    let mut body_paragraph_id = 1usize;
    let text_html = render_rich_text(
        &episode.text,
        site,
        work_key,
        images,
        image_path_base,
        "L",
        &mut body_paragraph_id,
    );
    let mut postscript_paragraph_id = 1usize;
    let postscript_html = render_rich_text(
        &episode.postscript,
        site,
        work_key,
        images,
        image_path_base,
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
            &format_optional_url(host_name, &get_cover_image_url(site, work_key, images)),
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

fn render_tag_list(tags: &[String]) -> String {
    if tags.is_empty() {
        return String::new();
    }
    let tags = tags
        .iter()
        .map(|tag| format!(r#"<span class="tag">{}</span>"#, escape_html(tag)))
        .collect::<Vec<_>>()
        .join("");
    format!(r#"<div class="tags">{tags}</div>"#)
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

    // Fallback: pixiv (and migrated data) embed already-hashed filenames such as
    // "abc1234567.jpg" directly into episode markup. These are not registered in
    // the logical-name map, so emit the image tag if the name matches a stored
    // image asset by hash + extension.
    if let Some((stem, ext)) = split_hashed_name(logical_name) {
        let matches_known_asset = images
            .values()
            .any(|asset| asset.hash == stem && asset.ext == ext);
        if matches_known_asset {
            return format!(
                r#"<img src="{}/{}" alt="">"#,
                escape_html(image_path_base),
                escape_html(logical_name)
            );
        }
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
    if !stem.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    if !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((stem, ext))
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

fn render_page_shell_with_ogp(
    title: &str,
    header_title: &str,
    nav_links: &str,
    body: &str,
    og_tags: &str,
) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  {og_tags}
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #f6f7fb; color: #1f2933; line-height: 1.75; }}
    header {{ position: sticky; top: 0; background: #111827; color: #fff; padding: 16px 20px; box-shadow: 0 1px 4px rgba(0,0,0,.12); }}
    header h1 {{ margin: 0 0 8px; font-size: 1.1rem; }}
    header nav {{ display: flex; flex-wrap: wrap; gap: 10px; font-size: .92rem; }}
    header a {{ color: #dbeafe; text-decoration: none; }}
    main {{ max-width: 920px; margin: 0 auto; padding: 20px; }}
    .meta-panel, .episode, .episode-list, .raw-json {{ background: #fff; border-radius: 14px; padding: 16px 18px; margin-bottom: 16px; box-shadow: 0 1px 4px rgba(0,0,0,.08); }}
    .meta {{ margin: 0 0 8px; color: #6b7280; font-size: .92rem; }}
    .summary {{ margin: 0 0 10px; white-space: pre-wrap; }}
    .episode-block {{ margin-top: 14px; }}
    .episode-block > :first-child {{ margin-top: 0; }}
    .episode-text, .episode-block.text {{ white-space: pre-wrap; }}
    pre {{ white-space: pre-wrap; overflow-x: auto; }}
    .tags {{ display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }}
    .tag {{ background: #e5eefc; color: #1d4ed8; border-radius: 999px; padding: 2px 10px; font-size: .82rem; }}
    .episode-summary {{ margin: 0; padding-left: 1.4rem; }}
    .episode-summary li {{ margin: 0 0 12px; }}
    .episode-summary .summary {{ color: #4b5563; }}
    img {{ max-width: 100%; height: auto; }}
    figure {{ margin: 0; }}
    hr.page-break {{ border: 0; border-top: 1px solid #d1d5db; margin: 1.5rem 0; }}
    ruby rt {{ font-size: .7em; }}
  </style>
</head>
<body>
  <header>
    <h1>{header_title}</h1>
    <nav>{nav_links}</nav>
  </header>
  <main>{body}</main>
</body>
</html>"#,
        title = escape_html(title),
        header_title = escape_html(header_title),
        nav_links = nav_links,
        body = body,
        og_tags = og_tags
    )
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

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        text.chars()
            .take(max_len)
            .collect::<String>()
            .trim_end()
            .to_string()
            + "..."
    }
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

fn get_cover_image_url(site: &str, work_key: &str, images: &HashMap<String, ImageAsset>) -> String {
    let preferred_prefix = format!("{site}_{work_key}");
    for (logical_name, image) in images.iter() {
        if logical_name.contains(&preferred_prefix)
            && (logical_name.contains("cover") || logical_name.contains("Cover"))
        {
            return format!("/images/{}.{}", image.hash, image.ext);
        }
    }
    for (logical_name, image) in images.iter() {
        if logical_name.contains("cover") || logical_name.contains("Cover") {
            return format!("/images/{}.{}", image.hash, image.ext);
        }
    }
    String::new()
}

fn update_cover_json(store: &Store, site: &str, site_dir: &Path) -> Result<()> {
    // Generate cover.json containing cover images for the site
    let images = store
        .list_images()?
        .into_iter()
        .filter(|img| img.logical_name.contains(site) || img.kind == "cover")
        .collect::<Vec<_>>();

    let mut cover_map = serde_json::Map::new();
    for img in images {
        if img.kind == "cover" {
            cover_map.insert(img.logical_name, serde_json::Value::String(img.hash));
        }
    }

    let cover_dir = site_dir.join("images");
    fs::create_dir_all(&cover_dir).context("failed to create images dir")?;
    fs::write(
        cover_dir.join("cover.json"),
        serde_json::to_string_pretty(&serde_json::Value::Object(cover_map))?,
    )
    .context("failed to write cover.json")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ImageRecord;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test-output")
            .join(format!("{name}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

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
    fn render_site_refreshes_root_cover_manifest() {
        let root = test_dir("renderer-cover-manifest");
        let data_dir = root.join("data");
        let images_dir = data_dir.join("images");
        fs::create_dir_all(&images_dir).unwrap();
        fs::write(
            images_dir.join("cover.json"),
            "{\n  \"stale\": \"value\"\n}",
        )
        .unwrap();

        let store = Store::open_in_memory().expect("store");
        store
            .upsert_image(&ImageRecord {
                logical_name: "pixiv_n123_cover.jpg".to_string(),
                hash: "abc123def4567890".to_string(),
                ext: "jpg".to_string(),
                kind: "cover".to_string(),
            })
            .expect("upsert image");
        store
            .upsert_work(&WorkRecord {
                site: "pixiv".to_string(),
                work_key: "n123".to_string(),
                title: "Example Title".to_string(),
                author: "Example Author".to_string(),
                author_id: None,
                author_url: None,
                r#type: "novel".to_string(),
                serialization: "短編".to_string(),
                caption: "Example Caption".to_string(),
                create_date: "2025-01-01T00:00:00Z".to_string(),
                update_date: "2025-01-01T00:00:00Z".to_string(),
                raw_json: sample_raw_json(),
            })
            .expect("upsert work");

        render_site_from_store(&store, "pixiv", data_dir.to_str().unwrap(), "")
            .expect("render site");

        let cover: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(images_dir.join("cover.json")).expect("read cover manifest"),
        )
        .expect("parse cover manifest");
        assert_eq!(
            cover,
            json!({
                "pixiv_n123_cover.jpg": "abc123def4567890"
            })
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repair_site_prefers_existing_raw_json_without_rewriting_it() {
        let root = test_dir("renderer-repair-from-raw");
        let data_dir = root.join("data");
        let raw_dir = data_dir.join("pixiv").join("n123").join("raw");
        fs::create_dir_all(&raw_dir).unwrap();

        let mut disk_raw = sample_raw_json();
        disk_raw["title"] = json!("Disk Title");
        disk_raw["caption"] = json!("Disk Caption");
        let disk_raw_text = serde_json::to_string(&disk_raw).unwrap();
        fs::write(raw_dir.join("raw.json"), &disk_raw_text).unwrap();

        let store = Store::open_in_memory().expect("store");
        store
            .upsert_work(&WorkRecord {
                site: "pixiv".to_string(),
                work_key: "n123".to_string(),
                title: "Database Title".to_string(),
                author: "Database Author".to_string(),
                author_id: None,
                author_url: None,
                r#type: "novel".to_string(),
                serialization: "短編".to_string(),
                caption: "Database Caption".to_string(),
                create_date: "2025-01-01T00:00:00Z".to_string(),
                update_date: "2025-01-01T00:00:00Z".to_string(),
                raw_json: {
                    let mut raw = sample_raw_json();
                    raw["title"] = json!("Database Title");
                    raw["caption"] = json!("Database Caption");
                    raw
                },
            })
            .expect("upsert work");

        repair_site_from_raw(&store, "pixiv", data_dir.to_str().unwrap(), "")
            .expect("repair site from raw");

        let repaired_raw = fs::read_to_string(raw_dir.join("raw.json")).expect("read repaired raw");
        assert_eq!(repaired_raw, disk_raw_text);

        let index: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(data_dir.join("pixiv").join("index.json")).expect("read index"),
        )
        .expect("parse index");
        assert_eq!(index["n123"]["title"].as_str(), Some("Disk Title"));
        assert_eq!(index["n123"]["caption"].as_str(), Some("Disk Caption"));

        fs::remove_dir_all(root).unwrap();
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
    fn render_image_resolves_hashed_filename() {
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
    fn render_image_falls_back_for_unknown_name() {
        let images: HashMap<String, ImageAsset> = HashMap::new();
        let html = render_image("not-an-image", &images, "../../images");
        assert_eq!(html, "<p><code>not-an-image</code></p>");
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
            raw,
        );

        let html = render_work_info("narou", "n123", &rendered, "", &HashMap::new());

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
            raw,
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
    fn render_episode_page_does_not_duplicate_introduction_summary() {
        let mut raw = sample_raw_json();
        raw["episodes"]["1"]["introduction"] = json!("導入文");
        let rendered = build_rendered_work(
            parse_raw_work(&raw).expect("parse raw"),
            "narou".to_string(),
            "n123".to_string(),
            raw,
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
            raw,
        );

        let html = render_work_index("narou", "n123", &rendered, "", &HashMap::new());

        assert!(html.contains(r#"<div class="p-eplist__chapter-title">第一章</div>"#));
        assert!(html.contains(r#"<div class="p-eplist__chapter-title">第二章</div>"#));
    }
}
