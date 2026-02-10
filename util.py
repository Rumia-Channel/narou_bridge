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

# グローバル設定
_global_img_url = ""

# --- テンプレート読み込み ---


def _load_template(template_name: str) -> str:
    """templatesディレクトリからHTMLテンプレートを読み込む"""
    template_path = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "templates", f"{template_name}.html"
    )
    try:
        with open(template_path, "r", encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        logging.error(f"Template file not found: {template_path}")
        raise
    except Exception as e:
        logging.error(f"Failed to load template {template_name}: {e}")
        raise


# --- 初期化・設定関連 ---


def set_img_url(img_url: str):
    """画像URL設定をグローバルに設定"""
    global _global_img_url
    _global_img_url = img_url
    # common モジュールにも設定を伝播
    cm.set_img_url(img_url)


def get_img_url() -> str:
    """画像URL設定を取得"""
    return _global_img_url


def init_import(site_dic):
    """各サイト用のクローラーモジュールを動的にインポート"""
    globals().update(import_modules(site_dic))


def import_modules(site_dic):
    modules = {}
    for site_key, value in site_dic.items():
        module_name = "crawler." + value.replace(".py", "")
        modules[site_key] = importlib.import_module(module_name)
    return modules


def load_config():
    """設定ファイル(setting.ini)を読み込み、初期フォルダ構成を作成"""
    if not os.path.isdir("setting"):
        os.makedirs("setting")
        shutil.copy("setting.ini", "setting/setting.ini")

    config = configparser.ConfigParser()
    config.read("setting/setting.ini", encoding="utf-8")

    # パス設定のヘルパー
    def get_path(section, key, default_name):
        path = config[section].get(key)
        if not path:
            path = os.path.join(
                os.path.dirname(os.path.abspath(__file__)), default_name
            )
        return path

    data_path = get_path("setting", "data", "data")
    cookie_folder = get_path("setting", "cookie", "cookie")
    log_path = get_path("setting", "log", "log")
    queue_path = get_path("setting", "queue", "queue")
    pdf_path = get_path("setting", "pdf", "pdf")

    # 必須フォルダ作成
    for p in [data_path, cookie_folder, log_path, queue_path, pdf_path]:
        os.makedirs(p, exist_ok=True)

    os.makedirs(os.path.join(data_path, "images"), exist_ok=True)
    os.makedirs(os.path.join(data_path, "reader"), exist_ok=True)

    # サイトごとの設定読み込み
    site_dic = {}
    login_dic = {}
    folder_path = {}
    cookie_path = {}
    site_list = []

    for key in config["crawler"]:
        site_list.append(key)
        site_dic[key] = config["crawler"][key]
        login_dic[key] = int(config["login"][key])

        folder_path[key] = os.path.join(data_path, key)
        cookie_path[key] = os.path.join(cookie_folder, key)

        os.makedirs(folder_path[key], exist_ok=True)
        os.makedirs(cookie_path[key], exist_ok=True)

    # インデックスとマニフェスト作成
    create_reader_index(data_path, site_list)
    create_manifest(data_path)

    # 静的リソースのコピー (css, script, icon, default_cover)
    _copy_static_resources(data_path)

    # サイトインデックスHTMLの再生成 (テンプレート更新を反映)
    display_names = (
        dict(config["display_name"]) if config.has_section("display_name") else {}
    )
    _regenerate_site_index_html(folder_path, display_names)

    print("Initialize successfully!")

    return (
        config,
        int(config["setting"]["reload"]),
        int(config["setting"]["auto_update"]),
        int(config["setting"]["save_log"]),
        int(config["setting"]["interval"]),
        int(config["setting"]["auto_update_interval"]),
        site_dic,
        login_dic,
        folder_path,
        data_path,
        cookie_path,
        log_path,
        queue_path,
        pdf_path,
        int(config["server"]["port"]),
        config["server"]["domain"],
        int(config["server"]["use_proxy"]),
        int(config["server"]["proxy_port"]),
        int(config["server"]["proxy_ssl"]),
        config["server"].get("img_url", ""),
    )


def _copy_static_resources(data_path):
    """commonフォルダから静的リソースをコピー"""
    cwd = os.getcwd()
    resources = ["css", "script", "icon"]

    for res in resources:
        src = os.path.join(cwd, "common", res)
        dst = os.path.join(data_path, res)
        if os.path.exists(src):
            if os.path.exists(dst):
                shutil.rmtree(dst)
            shutil.copytree(src, dst)

    # default_cover.png
    cover_src = os.path.join(cwd, "common", "default_cover.png")
    cover_dst = os.path.join(data_path, "images", "default_cover.png")
    if os.path.exists(cover_src):
        if os.path.exists(cover_dst):
            os.remove(cover_dst)
        shutil.copy2(cover_src, cover_dst)


def _regenerate_site_index_html(
    folder_path: Dict[str, str], display_names: Dict[str, str]
):
    """各サイトフォルダの index.html をテンプレートから再生成する (HTMLのみ、index.jsonは変更しない)"""
    template = _load_template("site_index")
    for site_key, path in folder_path.items():
        site_name = display_names.get(site_key, site_key)
        html_path = os.path.join(path, "index.html")
        try:
            html_content = template.format(site_name=site_name)
            with open(html_path, "w", encoding="utf-8") as f:
                f.write(html_content)
            logging.info(f"Regenerated site index HTML: {html_path}")
        except Exception as e:
            logging.error(f"Failed to regenerate site index HTML for {site_key}: {e}")


# --- ファイル生成関連 ---


def create_index(data_path, config, post_path=""):
    """ルート用index.htmlを作成"""
    post_url = "/api/" if post_path == "api" else "#"

    # サイト別ボタン生成（新デザイン対応）
    buttons_html = []
    for key in config["crawler"]:
        if key == "narou":
            buttons_html.append(f"""
            <div style="margin-bottom: 1rem; padding: 1rem; background: var(--bg-tertiary); border-radius: var(--radius-md);">
              <h3 style="font-size: 1rem; margin-bottom: 0.75rem; color: var(--text-primary);">{key}</h3>
              <div class="btn-group">
                <button disabled class="btn btn-sm btn-secondary">更新</button>
                <button onclick="submitConvert('{key}')" class="btn btn-sm btn-success">変換</button>
                <button disabled class="btn btn-sm btn-secondary">再DL</button>
              </div>
            </div>""")
        else:
            buttons_html.append(f"""
            <div style="margin-bottom: 1rem; padding: 1rem; background: var(--bg-tertiary); border-radius: var(--radius-md);">
              <h3 style="font-size: 1rem; margin-bottom: 0.75rem; color: var(--text-primary);">{key}</h3>
              <div class="btn-group">
                <button onclick="submitUpdate('{key}')" class="btn btn-sm btn-primary">更新</button>
                <button onclick="submitConvert('{key}')" class="btn btn-sm btn-success">変換</button>
                <button onclick="submitReDownload('{key}')" class="btn btn-sm btn-danger">再DL</button>
              </div>
            </div>""")

    # サイトリンク生成（新デザイン対応 - カード形式）
    links_html = []
    for key in config["crawler"]:
        links_html.append(f"""
          <a href="#" onclick="redirectWithParams('{key}/')" class="site-card">
            <span class="site-name">{key}</span>
            <svg class="site-arrow" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="9 18 15 12 9 6"/>
            </svg>
          </a>""")

    template = _load_template("index")
    html_content = template.format(
        post_url=post_url,
        site_buttons="\n".join(buttons_html),
        site_links="\n".join(links_html),
    )

    with open(os.path.join(data_path, "index.html"), "w", encoding="utf-8") as f:
        f.write(html_content)


def create_reader_index(data_path, site_list):
    """リーダー用index.htmlを作成"""
    # JSON文字列として埋め込む
    site_list_json = json.dumps(site_list, ensure_ascii=False)
    template = _load_template("reader_index")
    html_content = template.format(site_list_json=site_list_json)

    os.makedirs(os.path.join(data_path, "reader"), exist_ok=True)
    with open(
        os.path.join(data_path, "reader", "index.html"), "w", encoding="utf-8"
    ) as f:
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
            "purpose": "any maskable" if size == 512 else "any",
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
        "icons": icons,
    }

    manifest_path = os.path.join(data_path, "manifest.json")
    # manifest.json はバックアップ不要（静的設定ファイル）
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest_data, f, ensure_ascii=False, indent=2)


# --- クローラー実行ロジック ---

# アクション実行の優先順位 (先頭が最優先)
# 新しいアクションを追加する場合はここに名前を追加し、
# 各クローラーモジュールに同名の関数を実装するだけでよい。
ACTION_PRIORITY = ["repair", "update", "re_download", "convert", "download"]

# POSTパラメータ名 → 内部アクション名のマッピング
# (パラメータ名とアクション名が異なる場合のみ記載)
_PARAM_TO_ACTION = {"add": "download"}


def _resolve_action_name(param_key):
    """POSTパラメータ名を内部アクション名に変換"""
    return _PARAM_TO_ACTION.get(param_key, param_key)


def _resolve_param_key(action_name):
    """内部アクション名をPOSTパラメータ名に変換"""
    for k, v in _PARAM_TO_ACTION.items():
        if v == action_name:
            return k
    return action_name


def dispatch_action(
    action_name,
    param,
    site_dic,
    login_dic,
    folder_path,
    data_path,
    cookie_path,
    key_data,
    interval,
    host_name,
):
    """各サイトのアクションを共通ディスパッチで実行する。

    クローラーモジュールに action_name と同名の関数があれば呼び出す。
    モジュールに ALLOWED_ACTIONS 属性があれば、そのリストに含まれる
    アクションのみ許可する（未定義なら全アクション許可）。
    """

    target_sites = []

    # パラメータ解析: 'all' / サイトキー / URLの一部
    if param == "all":
        target_sites = list(site_dic.keys())
    elif param in site_dic:
        target_sites = [param]
    else:
        # URLの一部からサイト判定 (download用)
        for site_key, value in site_dic.items():
            module_sig = value.replace("_", ".").replace(".py", "")
            if module_sig in param:
                target_sites = [site_key]
                break
        else:
            return 400  # 該当サイトなし

    # 実行ループ
    for site in target_sites:
        module = globals()[site]

        # モジュールの ALLOWED_ACTIONS でフィルタリング
        allowed = getattr(module, "ALLOWED_ACTIONS", None)
        if allowed is not None and action_name not in allowed:
            logging.debug(
                f"Skipping site: {site} for {action_name} (not in ALLOWED_ACTIONS)"
            )
            continue

        # モジュールに該当関数が存在するかチェック
        func = getattr(module, action_name, None)
        if func is None:
            logging.debug(f"Skipping site: {site} — no '{action_name}' function")
            continue

        # ログイン設定チェック
        login_val = int(login_dic[site])
        if login_val not in [0, 1]:
            return 400

        logging.info(f"{action_name.capitalize()}: {site}")

        # 共通初期化
        module.init(cookie_path[site], data_path, login_val, interval)

        # アクション実行 (downloadだけ引数が異なる: URLが先頭に来る)
        if action_name == "download":
            func(param, folder_path[site], key_data, data_path, host_name)
        else:
            func(folder_path[site], key_data, data_path, host_name)

    return 200


# --- ファイル変換処理 (PDF/ZIP) ---


def pdf_to_text(
    pdf_path,
    pdf_name,
    author_id,
    author_url,
    novel_type,
    chapter,
    folder_path,
    data_path,
    key_data,
    host_name,
):
    site = "narou"  # PDF変換はnarouモジュールの機能を使用
    logging.info(f"Generate HTML from PDF: {site}")
    # globals()[site] -> crawler.narou
    globals()[site].gen_from_pdf(
        pdf_path,
        pdf_name,
        author_id,
        author_url,
        novel_type,
        chapter,
        folder_path[site],
        key_data,
        data_path,
        host_name,
    )


def zip_to_text(pdf_path, zip_name, data_path, host_name):
    """ZIPファイルからデータを展開・統合"""
    zip_file_path = os.path.join(pdf_path, zip_name)

    try:
        with zipfile.ZipFile(zip_file_path, "r") as zip_ref:
            # data.json の読み込み
            if "data.json" not in zip_ref.namelist():
                logging.error("data.json not found in the zip file.")
                return

            with zip_ref.open("data.json") as f:
                data_json_content = f.read().decode("utf-8")

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
                    if member.startswith(images_dir_in_zip) and not member.endswith(
                        "/"
                    ):
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
    queue_stop = False  # 元コードのロジックを維持するための変数（実際にはシグネチャ重複時のフラグとして使われている模様）

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
            queue_stop = True  # 重複があったことを通知
        else:
            processed_signatures.add(signature)

    return requests_dict, queue_stop
