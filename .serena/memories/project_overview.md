Project: Narou Bridge
Purpose: queue-based webnovel ingestion and conversion service that normalizes external sources into a shared work schema, then renders Narou.rb-compatible HTML. Current runtime focus is Pixiv crawling plus PDF/ZIP ingestion for Narou-style output.
Tech stack: Python 3.12, Flask, requests, Playwright, PyMuPDF, Pillow, apng, jsondiff.
Runtime architecture:
- `main.py` is startup-only: load config, create indexes/assets, then call `server.http_run()`.
- `server.create_app()` loads crawler bindings, starts `TaskManager` single-worker queue, optionally starts `AutoUpdater`, and serves generated files from `data/`.
- `POST /api/` only enqueues work; it does not crawl synchronously.
- Worker execution order is priority-based: `repair -> login -> update -> re_download -> convert -> download`.
- `util.dispatch_action()` delegates to `crawler/site_runtime.py` which resolves site targets and calls site handlers.
- `crawler/convert_narou.py:narou_gen()` is the final HTML renderer and assumes input is already normalized into canonical `raw.json` work data.
Current site bindings:
- `pixiv` -> `crawler/www_pixiv_net.py`
- `narou` -> `crawler/ncode_syosetu_com.py` for PDF conversion and HTML regeneration/repair
Important runtime directories:
- `data/`: generated HTML/JSON/images/manifests
- `cookie/`: per-site account files including active `login.json`
- `queue/`: `queue.pkl` and `task.json`
- `pdf/`: temporary PDF/ZIP uploads
- `log/`: server logs
- `setting/`: runtime copy of `setting.ini`
Rust migration note:
- planned Rust site layout is library-style modules like `src/sites/pixiv/mod.rs` and `src/sites/narou/mod.rs`, with private submodules for large sites.
- `sample/` is the archive location for old Python implementations and reference code; archive files must not define runtime behavior.
Rust dependency note:
- when adding Rust crates during migration, use `cargo add`; do not edit `Cargo.toml` directly just to add dependencies.
Rust storage note:
- final Rust direction may replace the legacy JSON tree with SQLite or another explicit store; the current JSON/HTML/Python files should be treated as migration inputs and compatibility artifacts, not the permanent runtime contract.
- login is expected to move out of crawler modules and into a simpler cookie import flow.
Important note for refactoring: preserve runtime behavior and on-disk contracts first; Python helper names are not migration boundaries.