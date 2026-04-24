use crate::core::model::{EpisodeIndexEntry, SiteIndexEntry, WorkRecord};
use crate::core::storage::Store;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

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
    let works = store.list_works(Some(site))?;
    let site_dir = PathBuf::from(data_dir).join(site);
    fs::create_dir_all(&site_dir).context("failed to create site dir")?;

    let image_assets = load_image_assets(store)?;
    let mut rendered = Vec::new();

    for work in works {
        rendered.push(render_work(&site_dir, &work, host_name, &image_assets)?);
    }

    write_site_index(&site_dir, site, &rendered, host_name)?;
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
    let work_dir = site_dir.join(&work.work_key);
    let raw_dir = work_dir.join("raw");
    let info_dir = work_dir.join("info");
    fs::create_dir_all(&raw_dir).context("failed to create raw dir")?;
    fs::create_dir_all(&info_dir).context("failed to create info dir")?;

    fs::write(
        raw_dir.join("raw.json"),
        serde_json::to_string_pretty(&work.raw_json)?,
    )?;

    let raw_json = work.raw_json.clone();
    let raw_work = parse_raw_work(&raw_json)
        .with_context(|| format!("failed to parse raw work for {}", work.work_key))?;
    let rendered =
        build_rendered_work(raw_work, work.site.clone(), work.work_key.clone(), raw_json);

    fs::write(
        work_dir.join("index.html"),
        render_work_index(
            &rendered.site,
            &rendered.work_key,
            &rendered,
            host_name,
            images,
        ),
    )?;
    fs::write(
        info_dir.join("index.html"),
        render_work_info(&rendered.site, &rendered.work_key, &rendered, host_name),
    )?;

    for (index, (episode_key, episode)) in rendered.episodes.iter().enumerate() {
        let prev = index
            .checked_sub(1)
            .and_then(|idx| rendered.episodes.get(idx).map(|(key, _)| key.as_str()));
        let next = rendered
            .episodes
            .get(index + 1)
            .map(|(key, _)| key.as_str());
        let episode_path = work_dir.join(format!("{episode_key}.html"));
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
        render_site_index(site, works, host_name),
    )?;
    Ok(())
}

fn render_site_index(site: &str, works: &[RenderedWork], host_name: &str) -> String {
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
    let og_tags = build_og_tags(
        &format!("{site} - Narou Bridge"),
        "Web小説・漫画の管理・閲覧システム",
        "website",
        &canonical_url,
        "",
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
    let episode_count = rendered.episodes.len();
    let episodes = render_episode_summary_list(site, work_key, &rendered.episodes, images);
    let tags = render_tag_list(&work.all_tags);
    let first_episode = rendered
        .episodes
        .first()
        .map(|(key, _)| format!("./{key}.html"))
        .unwrap_or_else(|| "./info/index.html".to_string());

    let page_title = format!("{} - {}", work.title, site);
    let nav = format!(
        r#"<a href="../index.html">site index</a>
<a href="info/index.html">info</a>
<a href="{reader_url}">reader</a>
<a href="{first_episode}">first episode</a>"#,
        reader_url = escape_html(&reader_url(host_name, site, work_key)),
        first_episode = escape_html(&first_episode)
    );
    
    let body = format!(
        r#"<section class="meta-panel">
  <p class="meta">{author} · {serialization} · {episode_count} 話</p>
  <p class="meta">作者ID: {author_id}</p>
  <p class="meta">作成: {create_date} / 更新: {update_date}</p>
  <p class="summary">{caption}</p>
  {tags}
</section>
<section class="episode-list">
  <h2>Episodes</h2>
  {episodes}
</section>"#,
        author = escape_html(&work.author),
        serialization = escape_html(&work.serialization),
        episode_count = episode_count,
        author_id = escape_html(work.author_id.as_deref().unwrap_or("-")),
        create_date = escape_html(&work.create_date),
        update_date = escape_html(&work.update_date),
        caption = escape_html(&work.caption),
        tags = tags,
        episodes = episodes
    );

    let canonical_url = format_canonical_url(host_name, &format!("/{}/{}/index.html", site, work_key));
    let og_tags = build_og_tags(
        &page_title,
        &work.caption,
        "article",
        &canonical_url,
        "",
    );

    render_page_shell_with_ogp(&page_title, &work.title, &nav, &body, &og_tags)
}

fn render_work_info(
    site: &str,
    work_key: &str,
    rendered: &RenderedWork,
    host_name: &str,
) -> String {
    let work = &rendered.work;
    let raw_json = serde_json::to_string_pretty(&rendered.raw_json).unwrap_or_default();
    let first_episode = rendered
        .episodes
        .first()
        .map(|(key, _)| format!("../{key}.html"))
        .unwrap_or_else(|| "../index.html".to_string());

    let page_title = format!("{} info", work.title);
    let nav = format!(
        r#"<a href="../index.html">back</a>
<a href="../{first_episode}">first episode</a>
<a href="{reader_url}">reader</a>"#,
        first_episode = escape_html(&first_episode),
        reader_url = escape_html(&reader_url(host_name, site, work_key))
    );
    
    let body = format!(
        r#"<section class="meta-panel">
  <p class="meta">{author} · {serialization} · {work_key}</p>
  <p class="meta">version: {version} / get_date: {get_date}</p>
  <p class="meta">ID: {id} / NID: {nid}</p>
  <p class="meta">URL: {url}</p>
  <p class="meta">作者ID: {author_id}</p>
  <p class="meta">作者URL: {author_url}</p>
  <p class="meta">作品種別: {work_type} / 話数: {episode_count} / 総話数: {all_episodes}</p>
  <p class="meta">文字数: {total_characters} / 総文字数: {all_characters}</p>
  <p class="meta">作成: {create_date} / 更新: {update_date}</p>
  <p class="summary">{caption}</p>
  {tags}
</section>
<section class="raw-json">
  <h2>raw.json</h2>
  <pre>{raw_json}</pre>
</section>"#,
        author = escape_html(&work.author),
        serialization = escape_html(&work.serialization),
        work_key = escape_html(work_key),
        version = rendered.work.version,
        get_date = escape_html(&rendered.work.get_date),
        id = escape_html(&rendered.work.id),
        nid = escape_html(&rendered.work.nid),
        url = escape_html(&rendered.work.url),
        author_id = escape_html(work.author_id.as_deref().unwrap_or("-")),
        author_url = escape_html(work.author_url.as_deref().unwrap_or("-")),
        work_type = escape_html(&work.work_type),
        episode_count = rendered.work.total_episodes,
        all_episodes = rendered.work.all_episodes,
        total_characters = work.total_characters,
        all_characters = work.all_characters,
        create_date = escape_html(&work.create_date),
        update_date = escape_html(&work.update_date),
        caption = escape_html(&work.caption),
        tags = render_tag_list(&work.all_tags),
        raw_json = escape_html(&raw_json)
    );

    let canonical_url = format_canonical_url(host_name, &format!("/{}/{}/info/index.html", site, work_key));
    let og_tags = build_og_tags(
        &page_title,
        &work.caption,
        "article",
        &canonical_url,
        "",
    );

    render_page_shell_with_ogp(&page_title, &format!("{} / info", work.title), &nav, &body, &og_tags)
}

fn render_episode_page(
    site: &str,
    work_key: &str,
    rendered: &RenderedWork,
    episode_key: &str,
    episode: &RawEpisode,
    prev: Option<&str>,
    next: Option<&str>,
    host_name: &str,
    images: &HashMap<String, ImageAsset>,
) -> String {
    let page_title = format!("{} - {}", episode.title, rendered.work.title);
    let nav = render_episode_nav(site, work_key, episode_key, prev, next, host_name);
    
    let mut paragraph_id = 1usize;
    let introduction_html = render_rich_text(&episode.introduction, site, work_key, images, &mut paragraph_id);
    let text_html = render_rich_text(&episode.text, site, work_key, images, &mut paragraph_id);
    let postscript_html = render_rich_text(&episode.postscript, site, work_key, images, &mut paragraph_id);
    
    let body = format!(
        r#"<section class="meta-panel">
  <p class="meta">{author} · {serialization} · {work_key}</p>
  <p class="meta">episode {episode_key} / id: {episode_id}</p>
  <p class="meta">文字数: {text_count} / chapter: {chapter}</p>
  <p class="meta">作成: {create_date} / 更新: {update_date}</p>
  <p class="summary">{introduction}</p>
  {tags}
</section>
<article class="episode">
  <h2>{title}</h2>
  <div class="episode-block introduction">{introduction_html}</div>
  <div class="episode-block text">{text_html}</div>
  <div class="episode-block postscript">{postscript_html}</div>
</article>"#,
        author = escape_html(&rendered.work.author),
        serialization = escape_html(&rendered.work.serialization),
        work_key = escape_html(work_key),
        episode_key = escape_html(episode_key),
        episode_id = escape_html(&episode.id),
        chapter = escape_html(episode.chapter.as_deref().unwrap_or("-")),
        create_date = escape_html(&episode.create_date),
        update_date = escape_html(&episode.update_date),
        text_count = episode.text_count,
        introduction = escape_html(&episode.introduction),
        tags = render_tag_list(&episode.tags),
        title = escape_html(&episode.title),
        introduction_html = introduction_html,
        text_html = text_html,
        postscript_html = postscript_html
    );

    let canonical_url = format_canonical_url(host_name, &format!("/{}/{}/{}.html", site, work_key, episode_key));
    let og_tags = build_og_tags(
        &page_title,
        &episode.introduction,
        "article",
        &canonical_url,
        "",
    );

    render_page_shell_with_ogp(&page_title, &episode.title, &nav, &body, &og_tags)
}

fn render_episode_summary_list(
    site: &str,
    work_key: &str,
    episodes: &[(String, RawEpisode)],
    images: &HashMap<String, ImageAsset>,
) -> String {
    let mut items = String::new();
    for (episode_key, episode) in episodes {
        let mut paragraph_id = 1usize;
        let caption = render_rich_text(&episode.introduction, site, work_key, images, &mut paragraph_id);
        items.push_str(&format!(
            r#"<li>
  <a href="./{episode_key}.html">{title}</a>
  <span class="meta">[{episode_id}] {update_date}</span>
  <div class="summary">{caption}</div>
</li>"#,
            episode_key = escape_html(episode_key),
            title = escape_html(&episode.title),
            episode_id = escape_html(&episode.id),
            update_date = escape_html(&episode.update_date),
            caption = caption
        ));
    }

    format!(r#"<ol class="episode-summary">{items}</ol>"#)
}

fn render_episode_nav(
    site: &str,
    work_key: &str,
    episode_key: &str,
    prev: Option<&str>,
    next: Option<&str>,
    host_name: &str,
) -> String {
    let mut links = Vec::new();
    links.push(format!(r#"<a href="./index.html">index</a>"#));
    links.push(format!(r#"<a href="./info/index.html">info</a>"#));
    links.push(format!(
        r#"<a href="{reader_url}">reader</a>"#,
        reader_url = escape_html(&reader_url(host_name, site, work_key))
    ));
    if let Some(prev) = prev {
        links.push(format!(r#"<a href="./{prev}.html">prev</a>"#));
    }
    if let Some(next) = next {
        links.push(format!(r#"<a href="./{next}.html">next</a>"#));
    }
    links.push(format!(
        r#"<span class="episode-key">{}</span>"#,
        escape_html(episode_key)
    ));
    links.join(" ")
}

fn render_page_shell(
    title: &str,
    header_title: &str,
    nav_links: &str,
    body: &str,
    extra_head: &str,
) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
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
  {extra_head}
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
        extra_head = extra_head
    )
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
                    paragraph_id,
                ));
                current.clear();
            }
            blocks.push("<hr class=\"page-break\">".to_string());
            continue;
        }

        if !trimmed.is_empty() && current.is_empty() && is_standalone_markup(trimmed) {
            blocks.push(render_markup_line(trimmed, site, work_key, images, paragraph_id));
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
    paragraph_id: &mut usize,
) -> String {
    let mut paragraphs = Vec::new();
    for paragraph in normalize_newlines(text).split("\n\n") {
        let paragraph = paragraph.trim_end();
        if paragraph.is_empty() {
            continue;
        }
        let html = format!(
            r#"<p id="p{}">{}</p>"#,
            paragraph_id,
            render_inline_markup(paragraph, site, work_key, images)
        );
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
    paragraph_id: &mut usize,
) -> String {
    if let Some(inner) = line
        .strip_prefix("[image](")
        .and_then(|s| s.strip_suffix(')'))
    {
        return render_image(inner, images);
    }
    if let Some(inner) = line
        .strip_prefix("[ruby:<")
        .and_then(|s| s.strip_suffix(']'))
    {
        if let Some((base, reading)) = inner.split_once(">(") {
            let reading = reading.trim_end_matches(')');
            let result = format!(
                r#"<p id="p{}"><ruby><rb>{}</rb><rt>{}</rt></ruby></p>"#,
                paragraph_id,
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
            r#"<p id="p{}"><a href="./{}.html">episode {}</a></p>"#,
            paragraph_id,
            escape_html(target),
            escape_html(target)
        );
        *paragraph_id += 1;
        return result;
    }
    let result = format!(
        r#"<p id="p{}">{}</p>"#,
        paragraph_id,
        render_inline_markup(line, site, work_key, images)
    );
    *paragraph_id += 1;
    result
}

fn render_inline_markup(
    text: &str,
    site: &str,
    work_key: &str,
    images: &HashMap<String, ImageAsset>,
) -> String {
    let mut out = String::new();
    let mut remainder = text;

    while !remainder.is_empty() {
        if let Some(rest) = remainder.strip_prefix("[image](") {
            if let Some(end) = rest.find(')') {
                let logical = &rest[..end];
                out.push_str(&render_image(logical, images));
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
                        "<ruby><rb>{}</rb><rt>{}</rt></ruby>",
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
                    r#"<a href="./{target}.html">episode {target}</a>"#,
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

fn render_image(logical_name: &str, images: &HashMap<String, ImageAsset>) -> String {
    if let Some(image) = images.get(logical_name) {
        return format!(
            r#"<figure><img src="../../images/{}.{}" alt="{}"></figure>"#,
            escape_html(&image.hash),
            escape_html(&image.ext),
            escape_html(logical_name)
        );
    }

    format!("<p><code>{}</code></p>", escape_html(logical_name))
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

fn build_og_tags(
    title: &str,
    description: &str,
    og_type: &str,
    url: &str,
    image: &str,
) -> String {
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
            .to_string() + "..."
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

fn get_cover_image_url(_site: &str, _work_key: &str, images: &HashMap<String, ImageAsset>) -> String {
    // Try to find a cover image
    for (logical_name, image) in images.iter() {
        if logical_name.contains("cover") || logical_name.contains("Cover") {
            return format!("/images/{}.{}", image.hash, image.ext);
        }
    }
    String::new()
}


fn update_cover_json(store: &Store, site: &str, site_dir: &Path) -> Result<()> {
    // Generate cover.json containing cover images for the site
    let images = store.list_images()?.into_iter()
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
        serde_json::to_string_pretty(&serde_json::Value::Object(cover_map))?
    ).context("failed to write cover.json")?;
    Ok(())
}
