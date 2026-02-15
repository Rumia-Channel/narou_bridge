Task completion guidance:
- No explicit automated tests/lint commands defined in repo.
- After changes, validate by running the server: `uv run python main.py` (or `main.cmd` on Windows) and exercising relevant /api endpoints.
- If changes affect Pixiv crawler or queue behavior, check logs in `log/` and queue files in `queue/` for regressions.