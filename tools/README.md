# Tools

This directory is a separate `uv` project.

## Setup

```bash
uv sync
```

## Login helper

```bash
uv run python login_helper.py pixiv output/login.json --display-name "example"
```

Import existing cookie JSON:

```bash
uv run python login_helper.py pixiv output/login.json --cookies-file existing.json
```
