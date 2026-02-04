# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Narou Bridge** is a web-based tool that converts novels from various websites (especially Pixiv) into HTML formats compatible with Narou.rb (a Japanese web novel downloader). It includes a Flask-based HTTP server, web crawlers for multiple sites, and a task queue system for managing download/conversion operations.

Key capabilities:
- Downloads novels from Pixiv and other sites
- Converts novels to Narou.rb-compatible HTML format
- Converts PDF/ZIP files to web novel format
- Provides a web interface for managing downloads
- Automatic update scheduling for tracked novels
- Built-in simple web reader for downloaded content

## Common Commands

### Setup and Installation
```bash
# Windows
main.cmd

# Linux/Mac
./main.sh
```

Both scripts will:
1. Sync dependencies via `uv sync`
2. Install Playwright browsers with `uv run playwright install`
3. Launch the server with `python main.py`

### Manual Steps
```bash
# Sync dependencies only
uv sync

# Install Playwright browsers
uv run playwright install

# Install Playwright system dependencies (Linux only)
uv run playwright install-deps

# Run the server directly
uv run python main.py
```

### Configuration
Edit `setting/setting.ini` (or `setting.ini` before first run) to configure:
- Data storage paths (data, cookie, log, queue, pdf)
- Crawler modules for different sites
- Server settings (domain, port, proxy configuration)
- Auto-update intervals

## Architecture

### Core Components

**Entry Point (`main.py`)**
- Loads configuration via `util.load_config()`
- Creates index files
- Launches the Flask server via `server.http_run()`

**Web Server (`server.py`)**
- Flask application serving static files and API endpoints
- `TaskManager`: Thread-safe queue system using `queue.Queue` and `threading.RLock`
  - Persists queue to `queue/queue.pkl` (with backup rotation)
  - Exports readable state to `queue/task.json`
  - Handles duplicate request detection and reload throttling
  - Worker thread processes tasks in background
- `AutoUpdater`: Daemon thread that periodically triggers update requests
- API endpoint `/api/` accepts POST requests with parameters:
  - `add`: URL to download new novel
  - `update`: Site key or "all" to update existing novels
  - `convert`: Convert downloaded data to HTML
  - `re_download`: Re-download specific novel
  - `pdf`/`zip`: File uploads for conversion

**Utilities (`util.py`)**
- `load_config()`: Parses `setting.ini`, creates folder structure, copies static assets
- `create_index()`: Generates main page HTML with site-specific buttons
- `create_reader_index()`: Generates web reader interface
- Dynamic module loading: Imports site-specific crawlers based on configuration
- Task execution dispatchers: `update()`, `download()`, `convert()`, `re_download()`
- PDF/ZIP conversion utilities

**Crawlers (`crawler/` directory)**
- `common.py`: Shared utilities for all crawlers
  - JSON safe load/save with atomic writes and backup rotation
  - HTTP session management with retry logic
  - HTML generation utilities
  - Image hash-based deduplication (`database.json` in `images/` folder)
  - Text formatting helpers (indent, ruby tags, links)
- `www_pixiv_net.py`: Pixiv-specific crawler
  - Uses Playwright for browser automation (handles login, CAPTCHA)
  - Downloads novels (single works and series)
  - Fetches cover images and episode images
  - Handles AI-generated content tagging
  - Version-based update detection via tag comparison
- `convert_narou.py`: Converts crawler output to Narou.rb format
- `ncode_syosetu_com.py`: Handles Syosetu (narou) site conversion
- Configuration files in `webnovel/*.yaml`: Define regex patterns for each site

### Data Flow

1. **User Request** → `/api/` endpoint receives POST with URL or action
2. **Queue Management** → `TaskManager.enqueue_request()` adds to queue, compresses duplicates, persists to disk
3. **Worker Processing** → Background worker thread calls `_execute_task()`
4. **Site Module Execution** → Dynamically imported crawler module performs:
   - `init()`: Set up session, cookies, login if needed
   - `download()`: Fetch novel metadata and content
   - `update()`: Check for updates, download new episodes
   - `convert()`: Generate HTML files compatible with Narou.rb
5. **Data Storage** → Novels stored in `data/{site}/{novel_id}/`
   - Metadata in `data.json`
   - HTML chapters
   - Cover images
   - Local `database.json` for images

### Key Design Patterns

**Asset Management**
- Local asset checking via `check_assets()` detects missing resources and triggers re-download
- Image deduplication using MD5 hashes stored in `database.json`
- Backup rotation (up to 3 backups) for critical JSON files

**Update Detection**
- Pixiv: Compares `tags` field in metadata to detect changes
- Tag changes trigger full re-download of affected novels
- Version field tracks crawler compatibility

**Concurrency**
- Re-entrant lock (`threading.RLock`) prevents deadlocks in TaskManager
- Single worker thread processes queue sequentially
- Request deduplication prevents redundant work

## Critical Files

- `setting/setting.ini`: Main configuration (copied from `setting.ini` on first run)
- `queue/queue.pkl`: Persisted task queue (binary)
- `queue/task.json`: Human-readable task state
- `data/index.html`: Main web interface
- `data/reader/index.html`: Novel reader interface
- `data/images/database.json`: Image hash database
- `data/{site}/database.json`: Per-site novel metadata index
- `webnovel/*.yaml`: Site-specific regex patterns for parsing

## Development Notes

**Adding a New Site Crawler**
1. Create `crawler/{site_domain}.py` following the pattern in existing crawlers
2. Implement required methods: `init()`, `download()`, `update()`, `convert()`
3. Add configuration to `setting.ini` under `[crawler]` and `[login]`
4. Create `webnovel/{site_domain}.yaml` with regex patterns
5. Use `crawler.common` utilities for consistency

**Modifying Queue Behavior**
- Queue persistence happens in `_save_queue()` after every operation
- Backup rotation preserves last 3 states
- Task deduplication logic in `_compact_list()` compares all fields except `request_id`

**Server Configuration**
- Direct access: `http://{domain}:{port}`
- Behind proxy: Use `use_proxy=1`, set `proxy_port` and `proxy_ssl`
- Host name construction in `_get_host_name()` affects generated URLs

**Logging**
- Server logs to `log/server.log` (all levels) and `log/error.log` (errors only)
- NoNewlineFormatter replaces newlines with spaces for single-line logs
- Toggle with `save_log` in settings
