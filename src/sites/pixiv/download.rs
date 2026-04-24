use crate::core::model::{ImageRecord, WorkRecord};
use crate::core::storage::Store;
use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::{fs, path::PathBuf};
use tracing::{info, warn};

use super::fetch::{fetch_body_json, download_image, sleep};
use super::{
    add_ai_tag_if_needed, build_work, check_image_file, dedup_tags, download_cover,
    extract_comic_series_entries, extract_flat_tags, extract_object_keys, extract_profile_ids,
    extract_tag_names_from_translation, extract_tag_names_nested, find_image_by_logical_prefix,
    find_key_recursively, format_image_links, format_ruby, format_survey, format_tags,
    hash_ids, html_breaks_to_newlines, int_field, last_numeric_segment, load_illust_snapshot,
    load_legacy_illust_snapshot, now_string, persist_work_record, regex_capture, remove_chapter_tag,
    save_image_hashed, save_illust_snapshot, sort_episodes_by_create_date, str_field,
    update_cover_json, url_decode, url_ext, value_to_string, format_jumpuri, format_jump_url,
    PIXIV_SITE_DOCUMENT_SCOPE, VERSION, ensure_tracked_user, action_enabled,
};

const PIXIV_SITE_DOCUMENT_SCOPE_STR: &str = PIXIV_SITE_DOCUMENT_SCOPE;

/// Download a single novel work
pub fn download_novel(
    client: &Client,
    novel_id: &str,
    folder_path: &Path,
    img_path: &Path,
) -> Result<WorkRecord> {
    let body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/novel/{novel_id}"),
    )?;

    let work_key = format!("n{novel_id}");
    let novel_path = folder_path.join(&work_key);
    fs::create_dir_all(novel_path.join("raw"))?;

    // Text
    let raw_text = body
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let text = format_novel_text(&raw_text, novel_id, &body, client, img_path);

    // Cover
    download_cover(
        client,
        body.get("coverUrl").and_then(Value::as_str),
        img_path,
        &work_key,
    );

    // Tags
    let mut tags = extract_tag_names_nested(&body, &["tags", "tags"]);
    add_ai_tag_if_needed(body.get("aiType").and_then(Value::as_i64), &mut tags);

    // Poll
    let postscript = format_survey(find_key_recursively(&body, "pollData"));

    let caption = html_breaks_to_newlines(
        body.get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let title = str_field(&body, "title", novel_id);
    let author = str_field(&body, "userName", "pixiv");
    let author_id = body
        .get("userId")
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let author_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/users/{uid}"));
    let create_date = str_field(&body, "createDate", "");
    let upload_date = body
        .get("uploadDate")
        .and_then(Value::as_str)
        .unwrap_or(create_date);
    let char_count = int_field(&body, &["characterCount", "textLength", "wordCount"]);

    let episodes = {
        let mut map = Map::new();
        map.insert(
            "1".to_string(),
            json!({
                "id": novel_id,
                "chapter": null,
                "title": title,
                "textCount": char_count,
                "tags": tags,
                "introduction": url_decode(&caption),
                "text": text,
                "postscript": postscript,
                "createDate": create_date,
                "updateDate": upload_date,
            }),
        );
        map
    };

    Ok(build_work(
        &work_key,
        novel_id,
        title,
        author,
        author_id,
        author_url,
        &caption,
        "novel",
        "短編",
        &tags,
        &tags,
        create_date,
        upload_date,
        char_count,
        char_count,
        1,
        1,
        &episodes,
        &format!("https://www.pixiv.net/novel/show.php?id={novel_id}"),
    ))
}

/// Download a novel series (連載小説)
pub fn download_series(
    client: &Client,
    series_id: &str,
    folder_path: &Path,
    img_path: &Path,
) -> Result<WorkRecord> {
    let body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/novel/series/{series_id}"),
    )?;

    let work_key = format!("s{series_id}");
    let series_path = folder_path.join(&work_key);
    fs::create_dir_all(series_path.join("raw"))?;

    // TOC (content_titles)
    let toc_body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles"),
    )?;
    let toc = toc_body.as_array().cloned().unwrap_or_default();

    // Cover
    let cover_url = body
        .get("cover")
        .and_then(|v| v.get("urls"))
        .and_then(|v| v.get("original"))
        .and_then(Value::as_str);
    download_cover(client, cover_url, img_path, &work_key);

    // Series-level tags
    let mut series_tags = extract_flat_tags(&body, "tags");
    add_ai_tag_if_needed(body.get("aiType").and_then(Value::as_i64), &mut series_tags);

    let mut all_tags: Vec<String> = series_tags.clone();
    let mut episodes = Map::new();
    let mut total_text: i64 = 0;

    // Fetch each episode
    for (idx, entry) in toc.iter().enumerate() {
        let available = entry
            .get("available")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if !available {
            continue;
        }

        let ep_id = entry.get("id").map(value_to_string).unwrap_or_default();
        if ep_id.is_empty() {
            continue;
        }

        let ep_title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        info!(
            "  Series {series_id}: fetching episode {}/{} id={ep_id} title={ep_title}",
            idx + 1,
            toc.len()
        );
        sleep();

        let ep_json = match fetch_body_json(client, &format!("https://www.pixiv.net/ajax/novel/{ep_id}")) {
            Ok(j) => j,
            Err(e) => {
                warn!("  Failed to fetch episode {ep_id}: {e}");
                continue;
            }
        };

        // Per-episode cover
        download_cover(
            client,
            ep_json.get("coverUrl").and_then(Value::as_str),
            img_path,
            &format!("pixiv_s{series_id}_{ep_id}"),
        );

        // Text
        let raw_text = ep_json.get("content").and_then(Value::as_str).unwrap_or("");
        let text = format_novel_text(raw_text, &ep_id, &ep_json, client, img_path);

        // Tags
        let mut ep_tags = extract_tag_names_nested(&ep_json, &["tags", "tags"]);
        add_ai_tag_if_needed(ep_json.get("aiType").and_then(Value::as_i64), &mut ep_tags);
        all_tags.extend(ep_tags.iter().cloned());

        // Poll
        let postscript = format_survey(find_key_recursively(&ep_json, "pollData"));

        let t_count = ep_json
            .get("characterCount")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        total_text += t_count;

        let ep_create = str_field(&ep_json, "createDate", "");
        let ep_update = ep_json
            .get("uploadDate")
            .and_then(Value::as_str)
            .unwrap_or(ep_create);
        let ep_caption = html_breaks_to_newlines(
            ep_json
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(""),
        );

        let ep_key = (idx + 1).to_string();
        episodes.insert(
            ep_key,
            json!({
                "id": ep_id,
                "chapter": null,
                "title": ep_title,
                "textCount": t_count,
                "tags": ep_tags,
                "introduction": url_decode(&ep_caption),
                "text": text,
                "postscript": postscript,
                "createDate": ep_create,
                "updateDate": ep_update,
            }),
        );
    }

    // Sort episodes by createDate, re-key
    let episodes = sort_episodes_by_create_date(episodes);

    // Deduplicate all_tags
    dedup_tags(&mut all_tags);

    let title = str_field(&body, "title", series_id);
    let author = str_field(&body, "userName", "pixiv");
    let author_id = body
        .get("userId")
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let author_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/users/{uid}"));
    let caption =
        html_breaks_to_newlines(body.get("caption").and_then(Value::as_str).unwrap_or(""));
    let create_date = str_field(&body, "createDate", "");
    let update_date = body
        .get("updateDate")
        .and_then(Value::as_str)
        .unwrap_or(create_date);
    let all_episodes = body
        .get("total")
        .and_then(Value::as_i64)
        .unwrap_or(episodes.len() as i64);
    let all_characters = body
        .get("publishedTotalCharacterCount")
        .and_then(Value::as_i64)
        .unwrap_or(total_text);
    let serialization = if body
        .get("isConcluded")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "完結済"
    } else {
        "連載中"
    };

    Ok(build_work(
        &work_key,
        series_id,
        title,
        author,
        author_id,
        author_url,
        &caption,
        "novel",
        serialization,
        &series_tags,
        &all_tags,
        create_date,
        update_date,
        total_text,
        all_characters,
        episodes.len() as i64,
        all_episodes,
        &episodes,
        &format!("https://www.pixiv.net/novel/series/{series_id}"),
    ))
}

/// Download an artwork (イラスト/漫画)
pub fn download_art(
    client: &Client,
    art_id: &str,
    folder_path: &Path,
    img_path: &Path,
) -> Result<WorkRecord> {
    let body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/illust/{art_id}"),
    )?;

    let work_key = format!("a{art_id}");
    let art_path = folder_path.join(&work_key);
    fs::create_dir_all(art_path.join("raw"))?;

    // Pages
    let pages_body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/illust/{art_id}/pages"),
    )?;
    let pages = pages_body.as_array().cloned().unwrap_or_default();

    // Build text from pages
    let mut art_text = String::new();
    for page in &pages {
        let url = page
            .get("urls")
            .and_then(|v| v.get("original"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if url.is_empty() {
            continue;
        }
        // Extract page number from _p0, _p1, etc.
        if let Some(caps) = regex_capture(r"_p(\d+)\.", url) {
            let num: i32 = caps.parse().unwrap_or(0) + 1;
            art_text.push_str(&format!("[pixivimage:{art_id}-{num}]\n"));
        }
    }

    // Download images referenced in text
    art_text = format_image_links(&art_text, art_id, &body, client, img_path);

    // Cover
    let cover_url = body
        .get("urls")
        .and_then(|v| v.get("original"))
        .and_then(Value::as_str);
    download_cover(client, cover_url, img_path, &work_key);

    // Tags
    let mut tags = extract_tag_names_nested(&body, &["tags", "tags"]);
    add_ai_tag_if_needed(body.get("aiType").and_then(Value::as_i64), &mut tags);

    // Poll
    let postscript = format_survey(find_key_recursively(&body, "pollData"));

    let caption = html_breaks_to_newlines(
        body.get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let title = str_field(&body, "title", art_id);
    let author = str_field(&body, "userName", "pixiv");
    let author_id = body
        .get("userId")
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let author_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/users/{uid}"));
    let create_date = str_field(&body, "createDate", "");
    let upload_date = body
        .get("uploadDate")
        .and_then(Value::as_str)
        .unwrap_or(create_date);

    let episodes = {
        let mut map = Map::new();
        map.insert(
            "1".to_string(),
            json!({
                "id": art_id,
                "chapter": null,
                "title": title,
                "textCount": 0,
                "tags": tags,
                "introduction": url_decode(&caption),
                "text": art_text,
                "postscript": postscript,
                "createDate": create_date,
                "updateDate": upload_date,
            }),
        );
        map
    };

    Ok(build_work(
        &work_key,
        art_id,
        title,
        author,
        author_id,
        author_url,
        &caption,
        "comic",
        "短編",
        &tags,
        &tags,
        create_date,
        upload_date,
        0,
        0,
        1,
        1,
        &episodes,
        &format!("https://www.pixiv.net/artworks/{art_id}"),
    ))
}

/// Download a comic series (漫画シリーズ)
pub fn download_comic(
    client: &Client,
    comic_id: &str,
    folder_path: &Path,
    img_path: &Path,
) -> Result<WorkRecord> {
    let work_key = format!("c{comic_id}");
    let comic_dir = folder_path.join(&work_key);
    fs::create_dir_all(comic_dir.join("raw"))?;

    // Collect all artwork IDs across pages
    let mut arts: BTreeMap<i64, String> = BTreeMap::new();
    let mut page = 1;
    let mut series_info: Value = Value::Null;
    let mut meta_body: Value = Value::Null;
    let mut user_info: Value = Value::Null;
    let mut first_tags: Vec<String> = Vec::new();

    loop {
        sleep();
        let page_url = format!("https://www.pixiv.net/ajax/series/{comic_id}?p={page}&lang=ja");
        let resp = match fetch_body_json(client, &page_url) {
            Ok(b) => b,
            Err(e) => {
                if page == 1 {
                    return Err(e);
                }
                break;
            }
        };

        if page == 1 {
            // Extract series metadata from first page
            series_info = resp
                .get("illustSeries")
                .and_then(Value::as_array)
                .and_then(|arr| {
                    arr.iter()
                        .find(|s| s.get("id").map(value_to_string).as_deref() == Some(comic_id))
                })
                .cloned()
                .or_else(|| {
                    resp.get("illustSeries")
                        .and_then(Value::as_array)
                        .and_then(|arr| arr.first())
                        .cloned()
                })
                .unwrap_or_default();

            user_info = resp
                .get("users")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .cloned()
                .unwrap_or_default();

            meta_body = resp.clone();

            // Tags from tagTranslation or thumbnails
            first_tags = extract_tag_names_from_translation(resp.get("tagTranslation"));
            if first_tags.is_empty() {
                if let Some(thumb) = resp
                    .get("thumbnails")
                    .and_then(|v| v.get("illust"))
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                {
                    first_tags = thumb
                        .get("tags")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                }
            }
        }

        // Extract series entries from this page
        let series_data = extract_comic_series_entries(&resp);
        if series_data.is_empty() {
            break;
        }
        for item in &series_data {
            let order = item.get("order").and_then(Value::as_i64).unwrap_or(0);
            let work_id = item
                .get("workId")
                .or_else(|| item.get("id"))
                .map(value_to_string)
                .unwrap_or_default();
            if !work_id.is_empty() {
                arts.insert(order, work_id);
            }
        }

        page += 1;
    }

    if arts.is_empty() {
        return Err(anyhow!("no artworks found in comic series {comic_id}"));
    }

    // Series cover
    download_cover(
        client,
        series_info.get("url").and_then(Value::as_str),
        img_path,
        &work_key,
    );

    // Author from meta or user
    let title = series_info
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(comic_id);
    let author = user_info
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Unknown");
    let author_id = user_info
        .get("userId")
        .map(value_to_string)
        .filter(|s| !s.is_empty());
    let author_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/users/{uid}"));

    let mut episodes = Map::new();
    let mut all_tags: Vec<String> = first_tags.clone();

    // Fetch each artwork
    for (order, illust_id) in &arts {
        info!("  Comic {comic_id}: fetching artwork order={order} id={illust_id}");
        sleep();

        let ep_data = match fetch_body_json(
            client,
            &format!("https://www.pixiv.net/ajax/illust/{illust_id}"),
        ) {
            Ok(j) => j,
            Err(e) => {
                warn!("  Failed to fetch illust {illust_id}: {e}");
                continue;
            }
        };

        // Pages
        let pages_res = fetch_body_json(
            client,
            &format!("https://www.pixiv.net/ajax/illust/{illust_id}/pages"),
        );
        let mut text = String::new();
        if let Ok(pages_body) = &pages_res {
            if let Some(pages) = pages_body.as_array() {
                for p in pages {
                    let u = p
                        .get("urls")
                        .and_then(|v| v.get("original"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if let Some(caps) = regex_capture(r"_p(\d+)\.", u) {
                        let num: i32 = caps.parse().unwrap_or(0) + 1;
                        text.push_str(&format!("[pixivimage:{illust_id}-{num}]\n"));
                    }
                }
            }
        }

        // Download images
        text = format_image_links(&text, illust_id, &ep_data, client, img_path);

        // Episode cover
        let ep_cover_url = ep_data
            .get("urls")
            .and_then(|v| v.get("original"))
            .and_then(Value::as_str);
        download_cover(
            client,
            ep_cover_url,
            img_path,
            &format!("pixiv_c{comic_id}_{illust_id}"),
        );

        // Tags
        let mut ep_tags = extract_tag_names_nested(&ep_data, &["tags", "tags"]);
        add_ai_tag_if_needed(ep_data.get("aiType").and_then(Value::as_i64), &mut ep_tags);
        all_tags.extend(ep_tags.iter().cloned());

        // Poll
        let postscript = format_survey(find_key_recursively(&ep_data, "pollData"));

        let ep_caption = html_breaks_to_newlines(
            ep_data
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(""),
        );
        let ep_create = str_field(&ep_data, "createDate", "");
        let ep_update = ep_data
            .get("uploadDate")
            .and_then(Value::as_str)
            .unwrap_or(ep_create);

        episodes.insert(
            order.to_string(),
            json!({
                "id": illust_id,
                "chapter": null,
                "title": str_field(&ep_data, "title", ""),
                "textCount": 0,
                "tags": ep_tags,
                "introduction": url_decode(&ep_caption),
                "text": text,
                "postscript": postscript,
                "createDate": ep_create,
                "updateDate": ep_update,
            }),
        );
    }

    // Sort episodes by createDate, re-key
    let episodes = sort_episodes_by_create_date(episodes);
    dedup_tags(&mut all_tags);

    let caption = html_breaks_to_newlines(
        meta_body
            .get("extraData")
            .and_then(|v| v.get("meta"))
            .and_then(|v| v.get("description"))
            .and_then(Value::as_str)
            .or_else(|| series_info.get("caption").and_then(Value::as_str))
            .unwrap_or(""),
    );
    let create_date = series_info
        .get("createDate")
        .and_then(Value::as_str)
        .unwrap_or("");
    let update_date = series_info
        .get("updateDate")
        .and_then(Value::as_str)
        .unwrap_or(create_date);

    let canon_url = author_id
        .as_ref()
        .map(|uid| format!("https://www.pixiv.net/user/{uid}/series/{comic_id}"))
        .unwrap_or_else(|| format!("https://www.pixiv.net/user/0/series/{comic_id}"));

    Ok(build_work(
        &work_key,
        comic_id,
        title,
        author,
        author_id,
        author_url,
        &caption,
        "comic",
        "連載中",
        &first_tags,
        &all_tags,
        create_date,
        update_date,
        0,
        0,
        episodes.len() as i64,
        episodes.len() as i64,
        &episodes,
        &canon_url,
    ))
}

/// Download a Pixiv user's works (novels and artworks/comics)
pub fn download_user(
    client: &Client,
    user_id: &str,
    folder_path: &Path,
    img_path: &Path,
    store: &mut Store,
    update: bool,
) -> Result<super::UserDownloadSummary> {
    let user_conf = ensure_tracked_user(store, folder_path, user_id)?;
    let profile_all = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/user/{user_id}/profile/all"),
    )?;
    let user_body = fetch_body_json(
        client,
        &format!("https://www.pixiv.net/ajax/user/{user_id}"),
    )
    .ok();

    let user_name = user_body
        .as_ref()
        .and_then(|body| body.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string);

    let novel_series = extract_profile_ids(profile_all.get("novelSeries"));
    let comic_series = extract_profile_ids(profile_all.get("mangaSeries"));
    let mut novels = extract_object_keys(profile_all.get("novels"));
    let illusts = extract_object_keys(profile_all.get("illusts"));
    let mangas = extract_object_keys(profile_all.get("manga"));

    let mut series_novel_ids = HashSet::new();
    for series_id in &novel_series {
        match fetch_body_json(
            client,
            &format!("https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles"),
        ) {
            Ok(body) => {
                if let Some(entries) = body.as_array() {
                    for entry in entries {
                        if let Some(id) = entry
                            .get("id")
                            .map(value_to_string)
                            .filter(|id| !id.is_empty())
                        {
                            series_novel_ids.insert(id);
                        }
                    }
                }
            }
            Err(err) => warn!("Failed to inspect Pixiv novel series {series_id}: {err}"),
        }
    }
    novels.retain(|novel_id| !series_novel_ids.contains(novel_id));

    let mut series_art_ids = HashSet::new();
    for series_id in &comic_series {
        match fetch_comic_series_art_ids(client, series_id) {
            Ok(ids) => {
                for id in ids {
                    series_art_ids.insert(id);
                }
            }
            Err(err) => warn!("Failed to inspect Pixiv comic series {series_id}: {err}"),
        }
    }

    let mut summary = super::UserDownloadSummary {
        user_id: user_id.to_string(),
        user_name,
        ..super::UserDownloadSummary::default()
    };

    if action_enabled(&user_conf, "novel") {
        for series_id in &novel_series {
            match download_series(client, series_id, folder_path, img_path) {
                Ok(record) => {
                    persist_work_record(store, &record, img_path)?;
                    summary.downloaded_works += 1;
                }
                Err(err) => summary
                    .failures
                    .push(format!("novel series {series_id}: {err}")),
            }
        }
        for novel_id in &novels {
            match download_novel(client, novel_id, folder_path, img_path) {
                Ok(record) => {
                    persist_work_record(store, &record, img_path)?;
                    summary.downloaded_works += 1;
                }
                Err(err) => summary.failures.push(format!("novel {novel_id}: {err}")),
            }
        }
    }

    if action_enabled(&user_conf, "comic") {
        for series_id in &comic_series {
            match download_comic(client, series_id, folder_path, img_path) {
                Ok(record) => {
                    persist_work_record(store, &record, img_path)?;
                    summary.downloaded_works += 1;
                }
                Err(err) => summary
                    .failures
                    .push(format!("comic series {series_id}: {err}")),
            }
        }

        let mut target_art_ids = BTreeSet::new();
        for art_id in illusts.iter().chain(mangas.iter()) {
            if !series_art_ids.contains(art_id) {
                target_art_ids.insert(art_id.clone());
            }
        }

        let previous_snapshot = load_illust_snapshot(store, folder_path, user_id)?;
        let mut new_art_ids = Vec::new();
        for art_id in &target_art_ids {
            if !update || !previous_snapshot.contains(art_id) {
                new_art_ids.push(art_id.clone());
            }
        }

        for art_id in &new_art_ids {
            match download_art(client, art_id, folder_path, img_path) {
                Ok(record) => {
                    persist_work_record(store, &record, img_path)?;
                    summary.downloaded_works += 1;
                }
                Err(err) => summary.failures.push(format!("artwork {art_id}: {err}")),
            }
        }

        let hash = save_illust_snapshot(store, folder_path, user_id, &target_art_ids)?;
        summary.tracked_artworks = target_art_ids.len();
        super::update_tracked_user(store, folder_path, user_id, |entry| {
            entry.illust_ids_snapshot_hash = Some(hash.clone());
            if let Some(user_name) = summary.user_name.clone() {
                entry.name = Some(user_name);
            }
        })?;
    } else if summary.user_name.is_some() {
        super::update_tracked_user(store, folder_path, user_id, |entry| {
            if let Some(user_name) = summary.user_name.clone() {
                entry.name = Some(user_name);
            }
        })?;
    }

    Ok(summary)
}

fn fetch_comic_series_art_ids(client: &Client, comic_id: &str) -> Result<Vec<String>> {
    let mut page = 1;
    let mut ids = HashSet::new();
    loop {
        sleep();
        let body = fetch_body_json(
            client,
            &format!("https://www.pixiv.net/ajax/series/{comic_id}?p={page}&lang=ja"),
        )?;
        let entries = extract_comic_series_entries(&body);
        if entries.is_empty() {
            break;
        }
        for entry in entries {
            if let Some(id) = entry
                .get("workId")
                .or_else(|| entry.get("id"))
                .map(value_to_string)
                .filter(|id| !id.is_empty())
            {
                ids.insert(id);
            }
        }
        page += 1;
    }

    let mut sorted = ids.into_iter().collect::<Vec<_>>();
    sorted.sort();
    Ok(sorted)
}

/// Format novel text with image and ruby markup
fn format_novel_text(
    text: &str,
    novel_id: &str,
    json_data: &Value,
    client: &Client,
    img_path: &Path,
) -> String {
    let mut text = text.replace("\r\n", "\n");
    text = format_image_links(&text, novel_id, json_data, client, img_path);
    text = format_ruby(&text);
    text = remove_chapter_tag(&text);
    text = format_jumpuri(&text);
    text = format_jump_url(&text);
    text
}
