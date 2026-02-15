Project: Narou Bridge
Purpose: Convert novels from various websites (notably Pixiv) into Narou.rb-compatible HTML. Includes a Flask web server, task queue, crawlers, and conversion utilities. Also supports PDF/ZIP conversion.
Tech stack: Python on Windows; Flask server; requests; Playwright + playwright-recaptcha; Pillow; apng; tqdm; BeautifulSoup4.
High-level structure:
- main.py: entry point; loads config; generates indexes; launches server
- server.py: Flask app; TaskManager queue; AutoUpdater; /api endpoints
- util.py: config loading, index generation, dynamic crawler loading, task dispatch
- crawler/: site-specific crawlers (www_pixiv_net.py, ncode_syosetu_com.py, convert_narou.py, common.py)
- setting/setting.ini: main config (copied from setting.ini on first run)
- data/: outputs, images, indexes; queue/: queue.pkl and task.json
