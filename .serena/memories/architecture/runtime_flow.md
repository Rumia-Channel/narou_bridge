Canonical runtime flow for Narou Bridge:
1. Startup
- `util.load_config()` copies `setting.ini` to `setting/setting.ini` on first run.
- Resolves absolute paths for runtime folders and creates them.
- Creates `data/images`, `data/reader`, root index, reader index, manifest, static asset copies, and per-site folders.
2. HTTP ingestion
- `POST /api/` accepts one logical request.
- Saves uploaded PDF/ZIP to `pdf/<request_id>.pdf|zip`.
- Builds one `req_data` record containing request metadata and all action fields.
3. Queue persistence
- `TaskManager.enqueue_request()` deduplicates by request signature excluding `request_id`.
- Queue is persisted to `queue/queue.pkl` and mirrored to `queue/task.json`.
4. Worker dispatch
- Single background worker pops one item at a time.
- Chooses the first non-empty action field in `ACTION_PRIORITY`.
- For action jobs, dispatches through `SiteRegistry`; for PDF/ZIP jobs, calls `util.pdf_to_text()` or `util.zip_to_text()`.
5. Normalization
- Site modules fetch/parse source data and normalize into shared work JSON at `data/<site>/<work>/raw/raw.json`.
6. Rendering and indexing
- `narou_gen()` writes work HTML files.
- `gen_site_index()` regenerates `data/<site>/index.json` and `index.html`.
- Reader UI consumes generated JSON/HTML only; it does not call crawlers directly.
Auto-update flow:
- `AutoUpdater` periodically POSTs `update=all` back into local `/api/`, so updates also flow through the same queue and persistence path.
Routing note:
- external form parameter `add` maps to internal action `download`.
- `download` resolves target site by URL matching; non-download actions resolve by site key or `all`.