Task completion guidance:
- No strong automated test suite exists; validate behavior through runtime contracts.
- Minimum smoke test after meaningful behavior changes:
  1. run `uv run python main.py`
  2. exercise affected `/api/` action types
  3. verify `queue/task.json` shape and worker progress
  4. verify `data/<site>/index.json` and `data/<site>/<work>/raw/raw.json`
  5. open generated site pages and `/reader/?site=<site>&nid=<work>`
  6. if image handling changed, inspect `data/images/database.json`, `cover.json`, and hashed image files
- For Rust migration work, explicitly note whether on-disk compatibility changed for queue files, cookies, image DBs, and `raw.json`.