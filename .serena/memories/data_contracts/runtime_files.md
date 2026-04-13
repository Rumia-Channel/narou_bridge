Important runtime/on-disk data contracts:

`setting/setting.ini`
- sections: `[setting]`, `[crawler]`, `[login]`, `[display_name]`, `[server]`
- `[crawler]`: site key -> crawler module name
- `[login]`: site key -> login enabled flag

Queued request object (used in memory, queue JSON mirror, and worker):
```json
{
  "request_id": "xxxx-xxxx-4xxx-yxxx-xxxx",
  "pdf_path": "...",
  "pdf_name": null,
  "zip_name": null,
  "author_id": null,
  "author_url": null,
  "novel_type": null,
  "chapter": null,
  "repair": null,
  "login": null,
  "update": null,
  "re_download": null,
  "convert": null,
  "add": null
}
```

`queue/task.json`
```json
{
  "current_task": {... or null},
  "queue": [{...}, {...}]
}
```

Account file: `cookie/<site>/<account>.json`
```json
{
  "cookies": {"name": "value"},
  "user_agent": "...",
  "display_name": "..."
}
```
- `cookies` may also be imported as a list of cookie objects.
- active account is `cookie/<site>/login.json`.

Image database: `data/images/database.json`
```json
{
  "logical_name.ext": "content_hash"
}
```
- actual files live at `data/images/<content_hash>.<ext>`.
- `data/images/cover.json` is a cover-only mapping with the same hash values.

Canonical work schema: `data/<site>/<work>/raw/raw.json`
```json
{
  "version": 0,
  "get_date": "...",
  "title": "...",
  "id": "...",
  "nid": "n123|s123|a123|c123|ncode",
  "url": "...",
  "author": "...",
  "author_id": "...",
  "author_url": "...",
  "caption": "...",
  "total_episodes": 1,
  "all_episodes": 1,
  "total_characters": 12345,
  "all_characters": 12345,
  "type": "novel|comic",
  "serialization": "短編|連載中|完結済",
  "tags": ["..."],
  "all_tags": ["..."],
  "createDate": "...",
  "updateDate": "...",
  "episodes": {
    "1": {
      "id": "episode id",
      "chapter": null,
      "title": "...",
      "textCount": 1234,
      "tags": ["..."],
      "introduction": "...",
      "text": "...",
      "postscript": "...",
      "createDate": "...",
      "updateDate": "..."
    }
  }
}
```
- this is the main migration boundary.
- `episodes` keys are display order, not guaranteed to equal source IDs.

Site index: `data/<site>/index.json`
```json
{
  "work_folder": {
    "title": "...",
    "author": "...",
    "author_id": "...",
    "author_url": "...",
    "type": "novel|comic",
    "serialization": "短編|連載中|完結済",
    "tags": ["..."],
    "all_tags": ["..."],
    "caption": "...",
    "create_date": "...",
    "update_date": "...",
    "episodes_data": {
      "1": {"title": "...", "id": "...", "caption": "...", "tags": ["..."]}
    }
  }
}
```
- reader library and site index UIs depend on this shape.

Episode text markup before rendering:
- `[image](filename.ext)`
- `[newpage]`
- `[ruby:<漢字>(かな)]`
- `[jump:3]`

ZIP import contract expected by `util.zip_to_text()`:
```json
{
  "site_name": "pixiv|narou|...",
  "images": {
    "logical_name.ext": "content_hash"
  }
}
```
- `data.json` must exist at ZIP root.
- `<site_name>/...` subtree is extracted under `data/`.
- image map merges into global image DB.