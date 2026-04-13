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
- Special-case state to account for:
  - Pixiv user tracking in `data/pixiv/user.json`
  - Pixiv snapshots in `data/pixiv/snapshots/illust_ids/<user_id>.json`
  - JSON backup and corruption handling (`.backup.N`, `.corrupt`) used widely in Python implementation.
- `webnovel/*.yaml` are not consumed by the current Python runtime. They are compatibility/output assets, not active server config.