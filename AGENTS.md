# Repository Guidelines

## Runtime Architecture
`main.py` is the only startup entrypoint. It calls `util.load_config()`, creates runtime directories, regenerates static assets and index files under `data/`, then starts the Flask server through `server.http_run()`.

`server.create_app()` is the runtime orchestrator. It:
- loads crawler modules via `util.init_import()`
- starts `TaskManager`, a persistent single-worker queue
- optionally starts `AutoUpdater`, which posts `update=all` back into `/api/`
- serves everything under `data/` as generated/static output

`/api/` is queueing-only. It never crawls synchronously. The request handler normalizes form fields and uploaded files into a `req_data` object, stores PDF/ZIP payloads in `pdf/`, enqueues the task, persists queue state, and returns `{"status": "queued", "request_id": ...}`.

The worker thread executes at most one queued task at a time. Action resolution is priority-based and stops at the first populated action:
`repair -> login -> update -> re_download -> convert -> download`

External POST parameter names are not identical to internal action names. `add` maps to internal `download`.

`util.dispatch_action()` and `crawler/site_runtime.py` are the action router. The current site bindings are:
- `pixiv` -> `crawler/www_pixiv_net.py`
- `narou` -> `crawler/ncode_syosetu_com.py` (used for PDF conversion and HTML regeneration)

`crawler/convert_narou.py` is the final renderer. Site modules should normalize source data into the shared `raw.json` work schema first, then call `narou_gen()` to emit HTML pages.

## Project Structure
`main.py`
- startup only; no business logic

`server.py`
- Flask app, `/api/` contract, account APIs, queue persistence, background worker, auto-update loop

`util.py`
- config loading, directory bootstrap, static asset copy, root/reader index generation, action dispatch, PDF/ZIP entrypoints

`crawler/site_runtime.py`
- site registry, bound action context, adapter layer between generic actions and site-specific implementations

`crawler/common.py`
- shared filesystem helpers, safe JSON IO, image database handling, site index generation, date/text helpers

`crawler/www_pixiv_net.py`
- Pixiv crawler, login flow, update/repair logic, canonical `raw.json` generation, user tracking snapshots

`crawler/ncode_syosetu_com.py`
- PDF-to-structured-data pipeline and HTML regeneration/repair for the Narou-compatible output format

`crawler/convert_narou.py`
- pure-ish HTML generator from normalized work data

`templates/` and `common/`
- HTML templates and browser-side assets used by generated pages and the lightweight reader

`webnovel/*.yaml`
- external compatibility assets for Narou.rb-style parsing rules
- current Python runtime does not read these YAML files directly
- treat them as part of the output contract, not as active server-side configuration

Runtime directories:
- `data/`: generated site output, root HTML, reader HTML, images, manifests
- `cookie/`: per-site account JSON files, including active `login.json`
- `queue/`: persistent queue state (`queue.pkl`, `task.json`)
- `pdf/`: temporary PDF/ZIP upload area
- `log/`: server logs
- `setting/`: runtime copy of `setting.ini`

## Canonical Runtime Flow
1. Startup
- `util.load_config()` copies `setting.ini` into `setting/setting.ini` on first run.
- It resolves absolute paths for `data`, `cookie`, `log`, `queue`, and `pdf`.
- It creates per-site folders, `data/images`, `data/reader`, root manifest, reader index, and copied static assets.

2. HTTP request ingestion
- `POST /api/` accepts one logical job at a time.
- Uploaded PDF/ZIP files are renamed to `<request_id>.pdf` or `<request_id>.zip` in `pdf/`.
- Action parameters are stored into one `req_data` record.

3. Queue persistence
- `TaskManager` writes a pickle snapshot to `queue/queue.pkl` for restart recovery.
- It also writes a human-readable mirror to `queue/task.json`.
- Requests are deduplicated by signature excluding `request_id`.

4. Action dispatch
- The worker chooses the first populated action field by `ACTION_PRIORITY`.
- `util.dispatch_action()` resolves the target site(s) and calls the bound site handler.
- `download` resolves site by URL matching; `update`, `convert`, `repair`, `login`, `re_download` resolve by site key or `all`.

5. Normalization
- Site modules fetch remote data and normalize it into the shared work schema stored at `raw/raw.json`.
- `pixiv` produces this schema from Pixiv API data.
- `narou` produces the same schema from parsed PDF text blocks.

6. Rendering and indexing
- `crawler/convert_narou.py:narou_gen()` renders `index.html`, `info/index.html`, and episode pages.
- `crawler/common.py:gen_site_index()` regenerates per-site `index.json` and `index.html`.
- The lightweight reader uses only generated JSON/HTML files under `data/`.

## On-Disk Data Contracts
`setting/setting.ini`
- sections: `[setting]`, `[crawler]`, `[login]`, `[display_name]`, `[server]`
- `[crawler]` maps external site key to crawler module file name
- `[login]` is per-site login enable flag

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

## Rust Refactor Guidance
Refactor toward Rust by preserving behavior and contracts first, not Python method names.

Rust target direction:
- primary storage should be SQLite or another explicit database, not the legacy JSON tree
- the current HTML frontend can be dropped; ship a minimal API-first runtime instead
- legacy JSON / pickle / HTML files are migration inputs, not the long-term runtime contract
- login should move out of crawler modules; provide a simple cookie import helper instead of browser automation in the main crawler path
- `sample/` is archive-only reference code and must not define runtime behavior
- the simple login helper lives under `tools/` and should only generate/import cookie JSON; do not reintroduce browser automation into the main runtime
- `tools/` is a separate `uv` project with its own `pyproject.toml` and `uv.lock`

Target stable boundaries:
- config/bootstrap
- HTTP API and static file serving
- persistent job queue
- typed site action dispatch
- canonical work model (`raw.json`)
- HTML rendering pipeline
- image dedupe database

Recommended Rust-first data types:
- `AppConfig`
- `RequestData`
- `TaskState`
- `AccountFile`
- `ImageDatabase`
- `CoverDatabase`
- `SiteIndexEntry`
- `Work`
- `Episode`
- `ZipImportMetadata`

Refactor principles:
- do not port Python helpers one-for-one; port state transitions and file contracts
- keep rendering as a deterministic transform from normalized work data to files
- isolate side effects behind interfaces: HTTP client, browser login, filesystem, queue store, clock
- preserve current directory layout only where it matters for migration/import
- if compatibility with existing installations matters, build a one-shot migration tool for legacy data before replacing formats
- if compatibility does not matter, move fully to SQLite/explicit storage early and keep JSON only as import/export surface
- when adding Rust crates, use `cargo add`; do not edit `Cargo.toml` directly to add dependencies

### Constraining Site Extensibility
The current Python implementation became complex because site integration is too open-ended. Complexity drivers to avoid carrying into Rust:
- `setting.ini` can map site keys to arbitrary crawler module names at runtime
- there are two extension mechanisms at once: module-level action functions and `create_site()` / `BaseSite` objects
- site modules hold hidden singleton state such as module-global `_crawler`
- site modules receive many loosely typed context arguments (`folder_path`, `data_path`, `cookie_path`, `key_data`, `host_name`, `interval`)
- site modules can re-enter the system indirectly by POSTing back to `/api/`
- `convert` / `repair` are mostly common operations but are reimplemented per site

For Rust, intentionally narrow the extension contract even if it reduces user extensibility.

Keep this outer shape for familiarity:
- each built-in site module/file may still expose `download`, `login`, `update`, `convert`, `repair`
- `convert` and `repair` should usually be thin wrappers that delegate to shared core logic

Do not preserve these Python-era freedoms:
- runtime loading of arbitrary user-provided site modules from config
- arbitrary new action names beyond the fixed action enum
- site modules directly owning queue behavior, retry scheduling, or local HTTP callbacks
- site modules generating final HTML outside the shared renderer
- site modules choosing their own directory layout or primary on-disk contracts

Preferred Rust shape:
- built-in static registry keyed by `SiteId`, not dynamic import strings
- fixed `Action` enum and typed `SiteActionContext`
- core owns queueing, retries, indexing, rendering, JSON IO, and path policy
- site modules own only URL parsing, optional login, remote fetch, source-specific normalization, and approved sidecar state
- per-work refetch/update identity should be persisted as structured metadata, not reconstructed from folder names or ad hoc URL parsing
- `key_data` and `host_name` should be absorbed by renderer/app core where possible, not passed through every site action

Rust site layout suggestion:
- organize each built-in site as a library-style module tree such as `src/sites/pixiv/mod.rs` and `src/sites/narou/mod.rs`
- split large sites into submodules (`fetch.rs`, `download.rs`, `update.rs`, `convert.rs`, `repair.rs`) once that improves clarity
- keep each site's public surface small and explicit; internal helpers should stay private to the site module tree
- prefer shared core modules for common behavior instead of letting site modules grow into mini-frameworks

Archive note:
- treat `sample/` as the archive location for old Python implementations and examples
- legacy Python files moved there are reference-only and must not define Rust runtime behavior
- do not expand Rust behavior based on archived Python code unless it matches the current on-disk contracts

Configuration direction for Rust:
- stop using `[crawler]` as `site key -> python module path`
- prefer fixed built-in site IDs with enable/disable or display-name settings only
- adding a new site should require changing Rust code and rebuilding, not dropping in a new runtime script

Practical consequence:
- preserve the shared `raw.json` boundary and shared renderer
- narrow site customization to normalization/fetch logic instead of letting each site redefine the whole runtime behavior

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
