# Repository Guidelines

Rust 移行は完了済みです。現行ランタイムは `src/` 配下の Rust サーバーで、`sample/` 配下の Python 実装は archive-only の参照資料です。Python ランタイムを前提にした運用判断は行わず、必要な場合は移行元データや旧挙動の確認用途としてのみ `sample/` を参照してください。

## Runtime Architecture
`src/main.rs` is the only startup entrypoint. It loads `setting.ini`, prepares runtime directories and SQLite storage, optionally runs the one-shot `migrate` subcommand, then starts the Axum server with `cargo run --release`.

`src/core/runtime.rs` is the runtime orchestrator. It:
- builds the HTTP router for `/api/`, `/api/migrate`, `/api/health`, `/reader/`, and static generated files
- starts a persistent single-worker queue backed by SQLite plus `queue/task.json`
- optionally starts the auto-update loop, which enqueues `update=all`
- serves generated output from `data/`

`/api/` is queueing-only. It never crawls synchronously. The request handler normalizes form fields and uploaded files into `RequestData`, stores PDF/ZIP payloads in `pdf/`, enqueues the task, persists queue state, and returns `{"status": "queued", "request_id": ...}`.

The worker executes at most one queued task at a time. Action resolution is priority-based and stops at the first populated action:
`repair -> login -> update -> re_download -> convert -> download`

External POST parameter names are not identical to internal action names. `add` maps to internal `download`.

`src/core/registry.rs` and `src/sites/*` are the action router. The built-in site bindings are:
- `pixiv` -> `src/sites/pixiv/`
- `narou` -> `src/sites/narou/` (used for PDF conversion and HTML regeneration)

`src/core/renderer.rs` is the final renderer. Site modules should normalize source data into the shared `raw.json` work schema first, then refresh HTML / reader output through the shared renderer.

## Project Structure
`src/main.rs`
- startup only; no business logic

`src/core/bootstrap.rs`
- `setting.ini` loading, runtime path resolution, app config assembly

`src/core/runtime.rs`
- Axum app, `/api/` contract, account APIs, migration API, queue persistence, background worker, auto-update loop

`src/core/storage.rs`
- SQLite-backed persistent store for tasks, works, accounts, images, and migration history

`src/core/migration.rs`
- legacy Python tree import from `sample/` or another allowed source root

`src/core/renderer.rs`
- shared HTML / reader / index generation from normalized work data

`src/sites/pixiv/`
- Pixiv fetch, login account usage, update/repair logic, canonical `raw.json` generation

`src/sites/narou/`
- PDF-to-structured-data pipeline and HTML regeneration/repair for the Narou-compatible output format

`templates/` and `common/`
- HTML templates and browser-side assets used by generated pages and the lightweight reader

`tools/`
- separate `uv` project; includes `login_helper.py` for cookie JSON generation/import

`sample/`
- archive-only Python reference implementation; never used as the active runtime

`webnovel/*.yaml`
- external compatibility assets for Narou.rb-style parsing rules
- treat them as part of the output contract, not as active server-side configuration

Runtime directories:
- `data/`: generated site output, root HTML, reader HTML, images, manifests
- `data/runtime.sqlite3`: primary persistent store for the Rust runtime
- `cookie/`: per-site account JSON files, including active `login.json`
- `queue/`: human-readable queue mirror (`task.json`)
- `pdf/`: temporary PDF/ZIP upload area
- `log/`: server logs
- `setting/`: runtime copy of `setting.ini`

## Canonical Runtime Flow
1. Startup
- `src/core/bootstrap.rs:load_app_config()` copies `setting.ini` into `setting/setting.ini` on first run.
- It resolves absolute paths for `data`, `cookie`, `log`, `queue`, and `pdf`.
- Startup prepares the SQLite store, generated asset roots, manifests, and static reader output.

2. HTTP request ingestion
- `POST /api/` accepts one logical job at a time.
- Uploaded PDF/ZIP files are renamed to `<request_id>.pdf` or `<request_id>.zip` in `pdf/`.
- Action parameters are stored into one `req_data` record.

3. Queue persistence
- SQLite (`data/runtime.sqlite3`) is the source of truth for queued/running/completed tasks.
- The runtime also writes a human-readable mirror to `queue/task.json`.
- Requests are deduplicated by signature excluding `request_id`.

4. Action dispatch
- The worker chooses the first populated action field by `ACTION_PRIORITY`.
- `SiteRegistry::dispatch()` resolves the target site(s) and calls the bound site handler.
- `download` resolves site by URL matching; `update`, `convert`, `repair`, `login`, `re_download` resolve by site key or `all`.

5. Normalization
- Site modules fetch remote data and normalize it into the shared work schema stored at `raw/raw.json`.
- `pixiv` produces this schema from Pixiv API data.
- `narou` produces the same schema from parsed PDF text blocks.

6. Rendering and indexing
- `src/core/renderer.rs` renders `index.html`, `info/index.html`, episode pages, and reader assets.
- The renderer regenerates per-site `index.json` and `index.html`.
- The lightweight reader uses only generated JSON/HTML files under `data/`.

## On-Disk Data Contracts
`setting/setting.ini`
- Rust runtime reads `[setting]` and `[server]`
- root `setting.ini` is preferred; `setting/setting.ini` is the runtime copy kept for compatibility
- `[crawler]`, `[login]`, and `[display_name]` may remain in migrated configs but are not required for Rust startup

`queue/task.json`
```json
{
  "current_task": {"request_id": "...", "add": null, "update": null},
  "queue": [
    {"request_id": "...", "add": "https://...", "pdf_name": null}
  ]
}
```

Queued request shape:
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

`cookie/<site>/<account>.json`
```json
{
  "cookies": {"name": "value"},
  "user_agent": "...",
  "display_name": "..."
}
```
Notes:
- `cookies` may also be a list of `{name, value, ...}` objects when imported
- `login.json` is the active account for that site

`data/images/database.json`
```json
{
  "pixiv_n123_cover.jpg": "<content_hash>",
  "pixiv_456-1.png": "<content_hash>"
}
```
Notes:
- values are extension-less content hashes
- actual files are stored as `data/images/<hash>.<ext>`
- `data/images/cover.json` is a cover-only companion map using the same hash values

`data/<site>/index.json`
```json
{
  "n123": {
    "title": "...",
    "author": "...",
    "author_id": "...",
    "author_url": "...",
    "type": "novel",
    "serialization": "短編",
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
Notes:
- keys are work folder names such as `n123`, `s456`, `a789`, `c012`, or `ncode`
- this is the main contract for site index pages and the reader library view

Canonical work schema at `data/<site>/<work>/raw/raw.json`:
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
Notes:
- `author_url` is effectively optional in some Pixiv comic paths
- `episodes` keys are sequential display order, not always source-site IDs
- this file is the most important migration boundary; preserve it unless intentionally versioning the format

Episode text markup before HTML rendering:
```text
[image](filename.ext)
[newpage]
[ruby:<漢字>(かな)]
[jump:3]
```
`narou_gen()` expands this markup into HTML, page-break markers, ruby tags, and internal jump links.

Pixiv-specific state:
- `data/pixiv/user.json`: tracked Pixiv user settings and snapshot hashes
- `data/pixiv/snapshots/illust_ids/<user_id>.json`: sorted list of illustration IDs for change detection

ZIP import contract expected by `util.zip_to_text()`:
```json
{
  "site_name": "pixiv|narou|...",
  "images": {
    "logical_name.ext": "content_hash"
  }
}
```
Notes:
- `data.json` must exist at ZIP root
- `<site_name>/...` subtree is extracted into `data/`
- image map is merged into `data/images/database.json`

## Frontend Contracts
The top page JavaScript submits form data using these API fields:
- `add`: download by URL
- `update`: update by site key or `all`
- `convert`: regenerate HTML by site key or `all`
- `re_download`: force redownload by site key or `all`
- `repair`: rebuild from `raw.json`
- `pdf`, `author_id`, `author_url`, `novel_type`, `chapter`: PDF conversion
- `zip`: ZIP import

The reader and site index pages depend on:
- `/images/cover.json`
- `/images/<hash>.<ext>`
- `/<site>/index.json`
- `/<site>/<work>/raw/raw.json`

If these JSON shapes change, the browser-side reader will break even if the crawler still works.

## Rust Runtime Guidance
The Rust migration is complete. Extend the current runtime by preserving behavior and contracts, not by reviving the old Python execution model.

Current stable boundaries:
- config/bootstrap
- HTTP API and static file serving
- SQLite-backed persistent job queue
- typed site action dispatch
- canonical work model (`raw.json`)
- shared HTML rendering pipeline
- image dedupe database

Current core data types:
- `AppConfig`
- `RequestData`
- `TaskRecord`
- `AccountFile`
- `ImageRecord`
- `WorkRecord`
- `Episode`
- `ZipImportMetadata`

Implementation principles:
- keep rendering as a deterministic transform from normalized work data to files
- keep SQLite as the source of truth; JSON/HTML files are generated outputs or migration inputs
- `sample/` is archive-only reference code and must not define runtime behavior
- `tools/` is a separate `uv` project; `login_helper.py` only generates/imports cookie JSON
- when adding Rust crates, use `cargo add`; do not edit `Cargo.toml` directly to add dependencies

Site extensibility is intentionally constrained:
- use the built-in static registry keyed by `SiteId`
- keep the fixed action set (`download`, `login`, `update`, `convert`, `repair`, `re_download`)
- let core own queueing, retries, indexing, rendering, JSON IO, and path policy
- keep site modules focused on URL parsing, remote fetch, normalization, and approved sidecar state

## Testing And Validation
There is no strong automated test suite today. For behavior changes, validate the current contracts directly.

### Server startup policy
The agent must NEVER start the server process itself (neither foreground nor background).
Instead, always use the question tool to ask the user: "サーバーは起動していますか？" before running any test that requires the server.
If the user confirms it is running, proceed. If not, show the startup command and wait.

Startup command for the Rust server:
```
cargo run --release
```

Minimum smoke tests:
- confirm the server is running (ask the user)
- POST `/api/` with each affected action type
- confirm `queue/task.json` shape and worker progress
- confirm `data/<site>/index.json` and `data/<site>/<work>/raw/raw.json` remain valid
- open generated site pages and `/reader/?site=<site>&nid=<work>`
- if image logic changed, inspect `data/images/database.json`, `cover.json`, and actual hashed image files

## Commit And PR Notes
Use descriptive Japanese commit messages focused on behavior changes, data contracts, or compatibility changes.

When a change affects runtime schemas or generated files, document:
- which on-disk contracts changed
- whether existing `raw.json`, queue files, cookies, or image DBs remain compatible
- whether Narou.rb compatibility assets under `webnovel/*.yaml` need coordinated updates
