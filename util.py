import os
import json
import shutil
import importlib
import configparser
import logging
import zipfile
from datetime import datetime
from typing import Dict, Any, List, Optional, Tuple

# 必要に応じてインポート (環境に合わせてパス解決してください)
import crawler.convert_narou as cn
import crawler.common as cm

# --- 定数: HTMLテンプレート ---

INDEX_HTML_TEMPLATE = """<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<script>const POST_URL = "{post_url}";</script>
<script src="script/top_page.js"></script>
<link rel="stylesheet" href="/css/common.css">
<link rel="icon" href="/icon/favicon.ico">
<link rel="manifest" href="/manifest.json">
<link rel="apple-touch-icon" sizes="180x180" href="/icon/icon_180x180.png">
<link rel="apple-touch-icon" sizes="167x167" href="/icon/icon_167x167.png">
<link rel="apple-touch-icon" sizes="152x152" href="/icon/icon_152x152.png">
<title>Index</title>
</head>
<body>
<input type="text" id="input1" placeholder="登録URL">
<button onclick="debouncedSubmit()">送信</button><br><br><br>

<button onclick="submitUpdate('all')">全て 更新</button>
<button onclick="submitConvert('all')">全て 変換</button>
<button onclick="submitReDownload('all')">全て 再ダウンロード</button>
<br>
{site_buttons}

<br><br>
<label for="pdfFile">PDFファイル選択:</label>
<input type="file" id="pdfFile" accept="application/pdf">
<br>
<label for="authorId">author_id:</label>
<input type="text" id="authorId" placeholder="author_id">
<br>
<label for="authorUrl">author_url:</label>
<input type="text" id="authorUrl" placeholder="author_url">
<br>
<label for="chapter">chapter:</label>
<input type="text" id="chapter" placeholder="exa:1-3:ああ,4-5:いい">
<br>
<label for="novelType">作品タイプ:</label>
<select id="novelType">
<option value="1">完結済</option>
<option value="0">連載中</option>
<option value="2">短編</option>
</select>
<br>
<button onclick="submitPdfData()">送信</button>

<br><br>
<label for="zipFile">ZIPファイル選択:</label>
<input type="file" id="zipFile" accept="application/zip">
<br>
<br>
<button onclick="submitZipData()">送信</button>

<br><br><br>
{site_links}
<br><br><br><br><a href="#" onclick="redirectWithParams(/reader/)">簡易リーダーへ移動</a>
</body>
</html>
"""

READER_INDEX_HTML_TEMPLATE = """<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Web小説リーダー</title>
  <link rel="stylesheet" href="/css/reader.css">
  <link rel="icon" href="/icon/favicon.ico">
  <link rel="manifest" href="/manifest.json">
  <link rel="apple-touch-icon" sizes="180x180" href="/icon/icon_180x180.png">
  <link rel="apple-touch-icon" sizes="167x167" href="/icon/icon_167x167.png">
  <link rel="apple-touch-icon" sizes="152x152" href="/icon/icon_152x152.png">
</head>
<body>
  <header>
    <h1>Web小説リーダー</h1>
    <button id="btn-clear-cache">キャッシュクリア</button>
  </header>

  <div id="progress-container">
    <div id="progress-bar"></div>
    <div id="progress-info">0.00% (0/0)</div>
  </div>

  <main id="app">
    </main>

  <script>
  const sources = {site_list_json};
  </script>
  <script src="/script/reader.js"></script>
  <script>
    document.addEventListener('DOMContentLoaded', () => {{
      const p = new URLSearchParams(location.search);
      if (p.has('site') && p.has('nid') && typeof window.adjustImages === 'function') {{
        window.adjustImages();
      }}
    }});
  </script>
</body>
</html>
"""

# --- 初期化・設定関連 ---

def init_import(site_dic):
    """各サイト用のクローラーモジュールを動的にインポート"""
    globals().update(import_modules(site_dic))

def import_modules(site_dic):
    modules = {}
    for site_key, value in site_dic.items():
        module_name = 'crawler.' + value.replace('.py', '')
        modules[site_key] = importlib.import_module(module_name)
    return modules

def load_config():
    """設定ファイル(setting.ini)を読み込み、初期フォルダ構成を作成"""
    if not os.path.isdir('setting'):
        os.makedirs('setting')
        shutil.copy('setting.ini', 'setting/setting.ini')

    config = configparser.ConfigParser()
    config.read('setting/setting.ini', encoding='utf-8')

    # パス設定のヘルパー
    def get_path(section, key, default_name):
        path = config[section].get(key)
        if not path:
            path = os.path.join(os.path.dirname(os.path.abspath(__file__)), default_name)
        return path

    data_path = get_path('setting', 'data', 'data')
    cookie_folder = get_path('setting', 'cookie', 'cookie')
    log_path = get_path('setting', 'log', 'log')
    queue_path = get_path('setting', 'queue', 'queue')
    pdf_path = get_path('setting', 'pdf', 'pdf')

    # 必須フォルダ作成
    for p in [data_path, cookie_folder, log_path, queue_path, pdf_path]:
        os.makedirs(p, exist_ok=True)
    
    os.makedirs(os.path.join(data_path, 'images'), exist_ok=True)
    os.makedirs(os.path.join(data_path, 'reader'), exist_ok=True)

    # サイトごとの設定読み込み
    site_dic = {}
    login_dic = {}
    folder_path = {}
    cookie_path = {}
    site_list = []

    for key in config['crawler']:
        site_list.append(key)
        site_dic[key] = config['crawler'][key]
        login_dic[key] = int(config['login'][key])
        
        folder_path[key] = os.path.join(data_path, key)
        cookie_path[key] = os.path.join(cookie_folder, key)
        
        os.makedirs(folder_path[key], exist_ok=True)
        os.makedirs(cookie_path[key], exist_ok=True)

    # インデックスとマニフェスト作成
    create_reader_index(data_path, site_list)
    create_manifest(data_path)

    # 静的リソースのコピー (css, script, icon, default_cover)
    _copy_static_resources(data_path)

    print("Initialize successfully!")
    
    return (
        config, 
        int(config['setting']['reload']), 
        int(config['setting']['auto_update']), 
        int(config['setting']['save_log']), 
        int(config['setting']['interval']), 
        int(config['setting']['auto_update_interval']), 
        site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, 
        int(config['server']['port']), config['server']['domain'], 
        int(config['server']['use_proxy']), int(config['server']['proxy_port']), int(config['server']['proxy_ssl'])
    )

def _copy_static_resources(data_path):
    """commonフォルダから静的リソースをコピー"""
    cwd = os.getcwd()
    resources = ['css', 'script', 'icon']
    
    for res in resources:
        src = os.path.join(cwd, 'common', res)
        dst = os.path.join(data_path, res)
        if os.path.exists(src):
            if os.path.exists(dst):
                shutil.rmtree(dst)
            shutil.copytree(src, dst)

    # default_cover.png
    cover_src = os.path.join(cwd, 'common', 'default_cover.png')
    cover_dst = os.path.join(data_path, 'images', 'default_cover.png')
    if os.path.exists(cover_src):
        if os.path.exists(cover_dst):
            os.remove(cover_dst)
        shutil.copy2(cover_src, cover_dst)

# --- ファイル生成関連 ---

def create_index(data_path, config, post_path=''):
    """ルート用index.htmlを作成"""
    post_url = '/api/' if post_path == 'api' else '#'
    
    # ボタン生成ロジック
    buttons_html = []
    links_html = []
    
    for key in config['crawler']:
        # リンク
        links_html.append(f'<a href="#" onclick="redirectWithParams(\'{key}/\')">{key}</a><br>')
        
        # ボタン
        if key == 'narou':
            buttons_html.append(f'<button disabled>{key} 更新</button>')
            buttons_html.append(f'<button onclick="submitConvert(\'{key}\')">{key} 変換</button>')
            buttons_html.append(f'<button disabled>{key} 再ダウンロード</button><br>')
        else:
            buttons_html.append(f'<button onclick="submitUpdate(\'{key}\')">{key} 更新</button>')
            buttons_html.append(f'<button onclick="submitConvert(\'{key}\')">{key} 変換</button>')
            buttons_html.append(f'<button onclick="submitReDownload(\'{key}\')">{key} 再ダウンロード</button><br>')

    html_content = INDEX_HTML_TEMPLATE.format(
        post_url=post_url,
        site_buttons='\n'.join(buttons_html),
        site_links='\n'.join(links_html)
    )

    with open(os.path.join(data_path, 'index.html'), 'w', encoding='utf-8') as f:
        f.write(html_content)

def create_reader_index(data_path, site_list):
    """リーダー用index.htmlを作成"""
    # JSON文字列として埋め込む
    site_list_json = json.dumps(site_list, ensure_ascii=False)
    html_content = READER_INDEX_HTML_TEMPLATE.format(site_list_json=site_list_json)
    
    os.makedirs(os.path.join(data_path, 'reader'), exist_ok=True)
    with open(os.path.join(data_path, 'reader', 'index.html'), 'w', encoding='utf-8') as f:
        f.write(html_content)

def create_manifest(data_path):
    """PWA用 manifest.json を作成"""
    icons = []
    sizes = [512, 384, 192, 180, 152, 144, 128, 96, 72, 48]
    for size in sizes:
        icon_entry = {
            "src": f"/icon/icon_{size}x{size}.png",
            "sizes": f"{size}x{size}",
            "type": "image/png",
            "purpose": "any maskable" if size == 512 else "any"
        }
        icons.append(icon_entry)

    manifest_data = {
        "name": "Narou Bridge",
        "short_name": "NarouBridge",
        "description": "A web application for Narou Bridge.",
        "start_url": "/",
        "display": "standalone",
        "background_color": "#ffffff",
        "theme_color": "#ffffff",
        "orientation": "portrait",
        "icons": icons
    }

    manifest_path = os.path.join(data_path, 'manifest.json')
    # manifest.json はバックアップ不要（静的設定ファイル）
    with open(manifest_path, 'w', encoding='utf-8') as f:
        json.dump(manifest_data, f, ensure_ascii=False, indent=2)

# --- クローラー実行ロジック ---

def _execute_site_action(action_name, param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    """各サイトのアクション（update, convert, re_download, download）を共通処理"""
    
    target_sites = []

    # パラメータ解析: 'all' または 特定のサイトキー または URLの一部
    if param == 'all':
        target_sites = [k for k in site_dic.keys() if k != 'narou'] # narouは一括処理対象外とする仕様
    elif param in site_dic:
        if param == 'narou' and action_name != 'convert': # narouはconvert以外許可しない
             logging.debug(f'Skipping site: {param} for {action_name}')
             return 400
        target_sites = [param]
    else:
        # URLの一部からサイト判定 (download用)
        for site_key, value in site_dic.items():
            # ファイル名からモジュール判定ロジック (元コード準拠)
            module_sig = value.replace('_', '.').replace('.py', '')
            if module_sig in param:
                if site_key == 'narou':
                    logging.debug('Skipping site: narou')
                    return 400
                target_sites = [site_key]
                break
        else:
            return 400 # 該当サイトなし

    # 実行ループ
    for site in target_sites:
        # ログイン設定チェック
        login_val = int(login_dic[site])
        if login_val not in [0, 1]:
            return 400
        
        logging.info(f'{action_name.capitalize()}: {site}')
        module = globals()[site]
        
        # 共通初期化
        module.init(cookie_path[site], data_path, login_val, interval)
        
        # アクション実行
        if action_name == 'update':
            module.update(folder_path[site], key_data, data_path, host_name)
        elif action_name == 're_download':
            module.re_download(folder_path[site], key_data, data_path, host_name)
        elif action_name == 'convert':
            module.convert(folder_path[site], key_data, data_path, host_name)
        elif action_name == 'download':
            # downloadだけ引数が異なる (param=URL)
            module.download(param, folder_path[site], key_data, data_path, host_name)

    return 200

def update(update_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    return _execute_site_action('update', update_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

def re_download(re_download_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    return _execute_site_action('re_download', re_download_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

def convert(convert_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    return _execute_site_action('convert', convert_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

def download(add_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    return _execute_site_action('download', add_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

# --- ファイル変換処理 (PDF/ZIP) ---

def pdf_to_text(pdf_path, pdf_name, author_id, author_url, novel_type, chapter, folder_path, data_path, key_data, host_name):
    site = 'narou' # PDF変換はnarouモジュールの機能を使用
    logging.info(f'Generate HTML from PDF: {site}')
    # globals()[site] -> crawler.narou
    globals()[site].gen_from_pdf(pdf_path, pdf_name, author_id, author_url, novel_type, chapter, folder_path[site], key_data, data_path, host_name)

def zip_to_text(pdf_path, zip_name, data_path, host_name):
    """ZIPファイルからデータを展開・統合"""
    zip_file_path = os.path.join(pdf_path, zip_name)
    
    try:
        with zipfile.ZipFile(zip_file_path, 'r') as zip_ref:
            # data.json の読み込み
            if 'data.json' not in zip_ref.namelist():
                logging.error('data.json not found in the zip file.')
                return
            
            with zip_ref.open('data.json') as f:
                data_json_content = f.read().decode('utf-8')
            
            json_data = json.loads(data_json_content)
            sitename = json_data.get("site_name")

            # 画像データベースの更新
            if json_data.get("images"):
                images_dir = os.path.join(data_path, "images")
                db_json_path = os.path.join(images_dir, "database.json")
                os.makedirs(images_dir, exist_ok=True)

                db_data = cm._load_json_safe(db_json_path)

                # マージ
                db_data.update(json_data["images"])

                cm._save_json(db_json_path, db_data)

                # 画像ファイルの展開
                images_dir_in_zip = f"{sitename}/images/"
                for member in zip_ref.namelist():
                    if member.startswith(images_dir_in_zip) and not member.endswith('/'):
                        rel_path = os.path.relpath(member, images_dir_in_zip)
                        dest_path = os.path.join(images_dir, rel_path)
                        os.makedirs(os.path.dirname(dest_path), exist_ok=True)
                        with zip_ref.open(member) as src, open(dest_path, "wb") as dst:
                            shutil.copyfileobj(src, dst)

            # サイトデータの展開
            if sitename:
                target_dir = os.path.join(data_path, sitename)
                os.makedirs(target_dir, exist_ok=True)
                for member in zip_ref.namelist():
                    if member.startswith(f"{sitename}/"):
                        zip_ref.extract(member, data_path)

        # ZIP削除
        os.remove(zip_file_path)

    except Exception as e:
        logging.error(f"ZIP processing failed: {e}")

# --- その他ユーティリティ ---

def cleanup_expired_requests(requests_dict, expiration_time):
    """有効期限切れリクエストと重複リクエストの削除"""
    current_time = datetime.now()
    processed_signatures = set()
    queue_stop = False # 元コードのロジックを維持するための変数（実際にはシグネチャ重複時のフラグとして使われている模様）

    # 辞書を変更しながらイテレートしないようリスト化
    for key in list(requests_dict.keys()):
        item = requests_dict[key]
        request_time = item["time"]
        request_data = item["data"]

        # 期限切れ削除
        if (current_time - request_time).total_seconds() > expiration_time:
            del requests_dict[key]
            continue

        # シグネチャによる重複チェック
        # request_id 以外をシグネチャとする
        sig_data = {k: v for k, v in request_data.items() if k != "request_id"}
        signature = json.dumps(sig_data, sort_keys=True)

        if signature in processed_signatures:
            del requests_dict[key]
            queue_stop = True # 重複があったことを通知
        else:
            processed_signatures.add(signature)

    return requests_dict, queue_stop