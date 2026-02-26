# Repository Guidelines

## Project Structure & Module Organization
`main.py` is the entry point: it loads config and starts the Flask server.  
`server.py` contains API routes, task queue handling, and auto-update logic.  
`util.py` handles config parsing, index generation, and task dispatch helpers.  
`crawler/` contains site-specific crawlers (`www_pixiv_net.py`, `ncode_syosetu_com.py`) plus shared crawler utilities in `crawler/common.py`.  
`webnovel/*.yaml` stores per-site parsing rules.  
`templates/` and `common/` hold HTML templates and static assets (CSS/JS/icons).  
Runtime folders (`data/`, `cookie/`, `log/`, `queue/`, `pdf/`, `setting/`) are generated locally and should stay untracked.

## Build, Test, and Development Commands
- `main.cmd` (Windows) / `./main.sh` (Linux/macOS): sync deps, install Playwright, start app.
- `uv sync`: install/update Python dependencies from `pyproject.toml` and `uv.lock`.
- `uv run playwright install`: install browser binaries.
- `uv run playwright install-deps`: Linux system deps for Playwright.
- `uv run python main.py`: run the server directly for development.
- `uv run python -m unittest discover -s tests`: run unit tests when `tests/` exists.

## Coding Style & Naming Conventions
Use Python 3.12, 4-space indentation, and `snake_case` for functions/variables/modules.  
Use `PascalCase` for classes.  
Follow existing crawler naming: domain-based module names like `www_pixiv_net.py`.  
Keep config keys consistent across `[crawler]`, `[login]`, and `webnovel/*.yaml`.  
No formatter/linter is enforced in-repo, so match existing style and keep comments concise.

## Testing Guidelines
Automated tests are currently limited; prioritize targeted unit tests for new logic (parsers, queue handling, URL formatting).  
Name tests `test_<module>.py` under `tests/`.  
For crawler/server changes, add a manual smoke test note in PRs (API call used, expected output path, and observed result).

## Commit & Pull Request Guidelines
Git history uses descriptive Japanese commit messages focused on behavior changes (example: `format_jump_url関数を追加し...`) rather than strict prefixes.  
Keep commits scoped to one logical change.  
PRs should include:
- What changed and why
- Affected files/modules
- Config impact (`setting.ini`, crawler mappings, YAML rules)
- Test evidence (command output or manual verification steps)
- Screenshots when changing `templates/` or `common/css` UI behavior

## Security & Configuration Tips
Never commit real cookies, secrets, or personal host settings from `setting.ini`.  
Use placeholder values in examples and verify generated runtime data stays ignored by `.gitignore`.
