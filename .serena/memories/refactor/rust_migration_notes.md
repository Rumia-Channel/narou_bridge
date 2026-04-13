Rust refactor guidance for Narou Bridge:
- Preserve behavior and contracts first, not Python method names or helper decomposition.
- Treat these as stable boundaries:
  1. config/bootstrap (`setting.ini` -> runtime dirs/assets)
  2. HTTP API and static file serving
  3. persistent job queue
  4. site action dispatch
  5. canonical work model (`raw.json`)
  6. HTML rendering pipeline
  7. image dedupe database
- Model-first Rust types to define early:
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
- Important behavioral requirements:
  - `/api/` is queueing-only; do not make site crawling synchronous.
  - queue is single-worker and priority-based.
  - `add` externally means internal `download`.
  - site dispatch is based on site key for maintenance actions and URL matching for downloads.
  - renderer is a deterministic transform from normalized work data to files.
  - browser reader depends on `index.json`, `raw.json`, `/images/cover.json`, and hashed image files.
- Migration strategy notes:
  - if compatibility matters, support reading current `queue.pkl`, `queue/task.json`, `raw.json`, cookies, and image DB files before replacing formats.
  - if compatibility does not matter, replace pickle persistence early with versioned JSON or another explicit format.
  - avoid one-to-one helper porting; port state transitions and file contracts.
  - isolate side effects behind interfaces: filesystem, HTTP client, login/browser automation, queue store, clock.
- Current Python complexity is driven by over-open site extensibility:
  - runtime mapping from `[crawler]` config to arbitrary module import paths
  - dual extension model: module-level action functions plus `create_site()`/`BaseSite`
  - module-global singleton state like `_crawler`
  - broad untyped action context (`folder_path`, `data_path`, `cookie_path`, `key_data`, `host_name`, `interval`)
  - site modules can indirectly re-enter the queue by POSTing back to `/api/`
  - shared operations like `convert` and `repair` are reimplemented in site modules
- Rust should intentionally narrow extensibility even if user-provided site additions become harder.
- Keep only this outer shape for familiarity:
  - built-in site modules/files may still expose `download`, `login`, `update`, `convert`, `repair`
  - `convert` and `repair` should usually be thin wrappers delegating to shared core logic
- Do not preserve these Python-era freedoms:
  - runtime loading of arbitrary user crawler modules from config
  - arbitrary action names beyond a fixed action enum
  - site-owned queue callbacks, retry scheduling, or local HTTP self-calls
  - site-owned final HTML rendering or custom directory layouts
- Preferred Rust architecture:
  - built-in static registry keyed by `SiteId`
  - fixed `Action` enum and typed `SiteActionContext`
  - core owns queueing, retries, indexing, rendering, JSON IO, and path policy
  - site modules own only URL parsing, optional login, remote fetch, source normalization, and approved sidecar state
  - per-work refetch/update identity should be explicit structured metadata, not reconstructed from folder names or ad hoc URL parsing
  - `key_data` and `host_name` should move into renderer/app core where possible
- Configuration direction:
  - stop using `[crawler]` as `site key -> module path`
  - prefer fixed built-in site IDs with enable/disable or display-name settings only
  - adding a new site in Rust should require code changes and rebuild, not dropping in a runtime script
- Dependency management rule:
  - when a new Rust crate is needed, add it with `cargo add`
  - do not edit `Cargo.toml` directly just to add dependencies
- Special-case state to account for:
  - Pixiv user tracking in `data/pixiv/user.json`
  - Pixiv snapshots in `data/pixiv/snapshots/illust_ids/<user_id>.json`
  - JSON backup and corruption handling (`.backup.N`, `.corrupt`) used widely in Python implementation.
- `webnovel/*.yaml` are not consumed by the current Python runtime. They are compatibility/output assets, not active server config.
- `sample/` is the archive location for old Python implementations and examples; archive code must not define runtime behavior.
- Rust target direction:
  - primary storage should be SQLite or another explicit database, not the legacy JSON tree
  - the current HTML frontend can be dropped; ship a minimal API-first runtime instead
  - legacy JSON / pickle / HTML files are migration inputs, not the long-term runtime contract
  - login should move out of crawler modules; provide a simple cookie import helper instead of browser automation in the main crawler path
  - preserve current directory layout only where it matters for migration/import
  - if compatibility matters, build a one-shot migration tool before replacing formats
  - if compatibility does not matter, move fully to SQLite/explicit storage early and keep JSON only as import/export surface
  - the simple login helper lives under `tools/` and should only generate/import cookie JSON; do not reintroduce browser automation into the main runtime