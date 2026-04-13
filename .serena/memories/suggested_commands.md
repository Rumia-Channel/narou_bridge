Common commands:
- Setup and run (Windows): `main.cmd`
- Setup and run (Linux/Mac): `./main.sh`
- Sync dependencies: `uv sync`
- Install Playwright browsers: `uv run playwright install`
- Install Linux Playwright deps: `uv run playwright install-deps`
- Run Python server: `uv run python main.py`
- Run Rust server: `cargo run --release`

IMPORTANT: The agent must NEVER start the server process itself. Always use the question tool to ask "サーバーは起動していますか？" before any test that requires a running server. If the user says no, show the startup command and wait.
Useful validation targets:
- `POST /api/` with `add`, `update`, `convert`, `re_download`, `repair`
- `POST /api/` with PDF fields: `pdf`, `author_id`, `author_url`, `novel_type`, `chapter`
- `POST /api/` with `zip`
Inspect generated/runtime state after changes:
- `queue/task.json`
- `data/<site>/index.json`
- `data/<site>/<work>/raw/raw.json`
- `data/images/database.json`
- `data/images/cover.json`
- `/reader/?site=<site>&nid=<work>` in browser
Configuration:
- edit `setting/setting.ini` (or `setting.ini` before first run)