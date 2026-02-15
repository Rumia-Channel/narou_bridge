import os
import json
import shutil
import random
import time
import threading
import queue
import pickle
import mimetypes
import logging
from datetime import datetime, timedelta
from typing import Optional, Dict, Any, List

import requests
from flask import Flask, request, jsonify, Response, redirect, url_for

# 共通設定の読み込み (外部モジュール)
import util

# MIMEタイプの設定
mimetypes.add_type("application/javascript", ".js")
mimetypes.add_type("text/css", ".css")
mimetypes.add_type("text/html", ".html")

# --- ロギング設定 ---


class NoNewlineFormatter(logging.Formatter):
    """改行をスペースに置き換えるフォーマッター"""

    def format(self, record):
        message = super().format(record)
        return message.replace("\n", " ").replace("\r", " ")


def setup_logging(log_path: str, save_log: bool):
    """ログ設定を初期化"""
    common_format = "%(asctime)s - %(levelname)s - %(message)s"
    handlers = []

    # コンソールログ
    console_handler = logging.StreamHandler()
    console_handler.setLevel(logging.DEBUG)
    console_handler.setFormatter(logging.Formatter(common_format))
    handlers.append(console_handler)

    if save_log:
        # 全レベルログ
        server_handler = logging.FileHandler(
            os.path.join(log_path, "server.log"), encoding="utf-8"
        )
        server_handler.setLevel(logging.DEBUG)
        server_handler.setFormatter(NoNewlineFormatter(common_format))
        handlers.append(server_handler)

        # エラーログ
        error_handler = logging.FileHandler(
            os.path.join(log_path, "error.log"), encoding="utf-8"
        )
        error_handler.setLevel(logging.ERROR)
        error_handler.setFormatter(NoNewlineFormatter(common_format))
        handlers.append(error_handler)

    # force=True: 既存のハンドラを上書きしてカスタムハンドラを確実に適用
    logging.basicConfig(level=logging.DEBUG, handlers=handlers, force=True)

    # requests/urllib3 のログレベルを上げて tqdm の進捗バーを見やすくする
    logging.getLogger("urllib3").setLevel(logging.WARNING)
    logging.getLogger("requests").setLevel(logging.WARNING)


# --- ユーティリティ関数 ---


def generate_request_id() -> str:
    """JavaScriptと同じプロセスでリクエストIDを生成"""
    template = "xxxx-xxxx-4xxx-yxxx-xxxx"

    def replace_char(c):
        r = random.randint(0, 15)
        if c == "x":
            return hex(r)[2:]
        if c == "y":
            return hex(r & 0x3 | 0x8)[2:]
        if c == "4":
            return "4"
        return c

    return "".join(replace_char(c) for c in template)


def secure_path(base_folder: str, requested_path: str) -> str:
    """ディレクトリトラバーサル対策を行ったパスを返す"""
    abs_base = os.path.abspath(base_folder)
    abs_req = os.path.abspath(requested_path)
    if not abs_req.startswith(abs_base):
        raise ValueError(f"Access denied: {requested_path}")
    return abs_req


def create_response(status_code: int, status: str, message: str) -> Response:
    """JSONレスポンス生成ヘルパー"""
    if status == "error":
        logging.error(f"Error {status_code}: {message}")
    else:
        logging.info(f"Success: {message}")

    response = jsonify({"status": status, "message": message})
    response.status_code = status_code
    return response


# --- タスク管理クラス ---


class TaskManager:
    def __init__(self, config: Dict[str, Any]):
        self.config = config
        self.queue_path = config["queue_path"]
        self.job_file_path = os.path.join(self.queue_path, "queue.pkl")
        self.task_json_path = os.path.join(self.queue_path, "task.json")

        self.queue = queue.Queue()

        # 【修正箇所】再入可能ロック(RLock)を使用することでデッドロックを回避
        self.lock = threading.RLock()

        self.request_datas = {}  # リクエスト履歴管理
        self.current_task = None

        # キューの復元とワーカーの開始
        self._load_queue()
        self.worker_thread = threading.Thread(target=self._worker_loop, daemon=True)
        self.worker_thread.start()

    def _save_queue(self):
        """キューの状態をファイルに保存"""
        with self.lock:
            # Pickle保存 (互換性維持のため) - バックアップ付き
            try:
                # バックアップローテーション
                if os.path.exists(self.job_file_path):
                    for i in range(3, 1, -1):
                        old_backup = f"{self.job_file_path}.backup.{i - 1}"
                        new_backup = f"{self.job_file_path}.backup.{i}"
                        if os.path.exists(old_backup):
                            shutil.move(old_backup, new_backup)
                    shutil.copy2(self.job_file_path, f"{self.job_file_path}.backup.1")

                # アトミック書き込み
                tmp_path = self.job_file_path + ".tmp"
                with open(tmp_path, "wb") as f:
                    pickle.dump(list(self.queue.queue), f)
                os.replace(tmp_path, self.job_file_path)
            except Exception as e:
                logging.error(f"Failed to save queue pickle: {e}")

            # JSON保存 (可視化用) - task.json はバックアップ不要
            try:
                data = {
                    "current_task": self.current_task,
                    "queue": list(self.queue.queue),
                }
                with open(self.task_json_path, "w", encoding="utf-8") as jf:
                    json.dump(data, jf, ensure_ascii=False, indent=4)
                logging.info(f"Queue saved to {self.job_file_path}")
            except Exception as e:
                logging.error(f"Failed to save task.json: {e}")

    def _load_queue(self):
        """起動時にキューを復元（バックアップからの復元機能付き）"""
        if not os.path.exists(self.job_file_path):
            logging.info("No job file found. Starting empty.")
            return

        with self.lock:
            jobs = None
            # メインファイル読み込み試行
            try:
                with open(self.job_file_path, "rb") as f:
                    jobs = pickle.load(f)
                logging.info(f"Queue loaded from {self.job_file_path}")
            except Exception as e:
                logging.error(f"Failed to load queue from main file: {e}")

                # バックアップからの復元試行
                for i in range(1, 4):
                    backup_path = f"{self.job_file_path}.backup.{i}"
                    if os.path.exists(backup_path):
                        try:
                            with open(backup_path, "rb") as f:
                                jobs = pickle.load(f)
                            logging.info(f"Queue restored from backup: {backup_path}")
                            # 復元成功したらメインファイルを上書き
                            shutil.copy2(backup_path, self.job_file_path)
                            break
                        except Exception as backup_err:
                            logging.error(
                                f"Failed to restore from {backup_path}: {backup_err}"
                            )
                            continue

            if jobs:
                compacted_jobs = self._compact_list(jobs)
                for job in compacted_jobs:
                    self.queue.put(job)
                logging.info(f"Queue restored with {len(compacted_jobs)} items.")
            else:
                logging.warning(
                    "All queue restore attempts failed. Starting with empty queue."
                )

    def _compact_list(self, requests_list: List[Dict]) -> List[Dict]:
        """リスト内の重複リクエストをまとめる"""
        if not requests_list:
            return []

        compacted = []
        seen = {}

        for req in requests_list:
            if req is None:
                compacted.append(None)
                continue

            # request_id以外をキーにして重複判定
            key = tuple(sorted((k, v) for k, v in req.items() if k != "request_id"))
            if key not in seen:
                seen[key] = [req]
            else:
                seen[key].append(req)

        # 先頭のリクエストを採用
        for group in seen.values():
            compacted.append(group[0])

        return compacted

    def enqueue_request(self, req_data: Dict[str, Any]) -> str:
        """リクエストをキューに追加"""
        req_id = req_data.get("request_id")

        with self.lock:
            # リロード連打対策
            reload_time = int(self.config["reload_time"])
            if req_id in self.request_datas:
                prev_time = self.request_datas[req_id]["time"]
                if (datetime.now() - prev_time).total_seconds() <= reload_time:
                    raise ValueError(f"Request {req_id} ignored (reload time).")

            # 履歴保存
            self.request_datas[req_id] = {"time": datetime.now(), "data": req_data}
            self.request_datas, _ = util.cleanup_expired_requests(
                self.request_datas, expiration_time=reload_time
            )

            # キューに追加
            self.queue.put(req_data)

            # キュー圧縮（一度リストにして圧縮して戻す）
            all_items = list(self.queue.queue)
            with self.queue.mutex:
                self.queue.queue.clear()

            compacted = self._compact_list(all_items)
            for item in compacted:
                self.queue.put(item)

            self._save_queue()

        return req_id

    def _worker_loop(self):
        """バックグラウンドでタスクを処理するループ"""
        while True:
            req_data = self.queue.get()
            if req_data is None:
                break

            with self.lock:
                self.current_task = req_data
                self._save_queue()

            try:
                self._execute_task(req_data)
            except Exception as e:
                logging.exception(f"Task execution failed: {e}")

            with self.lock:
                self.current_task = None
                self._save_queue()

            self.queue.task_done()

    def _execute_task(self, req_data: Dict[str, Any]):
        """個々のタスク実行ロジック"""
        c = self.config
        host_name = self._get_host_name()

        # パラメータと実行関数のマッピング
        # (util.ACTION_PRIORITY の順に優先度が高い)
        for action_name in util.ACTION_PRIORITY:
            param_key = util._resolve_param_key(action_name)
            val = req_data.get(param_key)
            if val:
                ret = util.dispatch_action(
                    action_name,
                    val,
                    c["site_dic"],
                    c["login_dic"],
                    c["folder_path"],
                    c["data_path"],
                    c["cookie_path"],
                    "",
                    c["interval"],
                    host_name,
                )
                if ret == 400:
                    logging.error(f"Task {param_key} failed with 400")
                else:
                    logging.info(f"Task {param_key} completed")
                return

        # PDF/ZIP処理
        if req_data.get("pdf_name"):
            ret = util.pdf_to_text(
                req_data["pdf_path"],
                req_data["pdf_name"],
                req_data["author_id"],
                req_data["author_url"],
                req_data["novel_type"],
                req_data["chapter"],
                c["folder_path"],
                c["data_path"],
                "",
                host_name,
            )
            logging.info(f"PDF Task result: {ret}")
            return

        if req_data.get("zip_name"):
            ret = util.zip_to_text(
                req_data["pdf_path"], req_data["zip_name"], c["data_path"], host_name
            )
            logging.info(f"ZIP Task result: {ret}")
            return

        logging.warning("No valid action found in request data")

    def _get_host_name(self):
        c = self.config
        if c["use_proxy"]:
            protocol = "https" if c["proxy_ssl"] else "http"
            return f"{protocol}://{c['domain']}:{c['proxy_port']}"
        return f"http://{c['domain']}:{c['port']}"


# --- 自動更新クラス ---


class AutoUpdater(threading.Thread):
    def __init__(self, config):
        super().__init__(daemon=True)
        self.config = config

    def run(self):
        time.sleep(30)  # サーバー起動待ち
        c = self.config
        url = f"http://127.0.0.1:{c['port']}/api/"
        interval = c["auto_update_interval"]

        while True:
            if c["auto_update"]:
                logging.info("Sending auto-update request")
                try:
                    payload = {"update": "all", "request_id": generate_request_id()}
                    requests.post(url, data=payload)
                except Exception as e:
                    logging.error(f"Auto-update failed: {e}")

            next_run = datetime.now() + timedelta(seconds=interval)
            logging.info(f"Next auto-update: {next_run}")
            time.sleep(interval)


# --- Flask アプリケーション ---


def create_app(config: Dict[str, Any]):
    setup_logging(config["log_path"], config["save_log"])
    logging.debug("サーバー起動")

    # グローバル設定
    img_url = config.get("img_url", "")
    util.set_img_url(img_url)
    if img_url:
        logging.info(f"Image URL configured: {img_url}")

    # util モジュール初期化
    if "site_dic" in config:
        try:
            util.init_import(config["site_dic"])
            logging.info("Crawler modules loaded successfully.")
        except Exception as e:
            logging.error(f"Failed to load crawler modules: {e}")

    app = Flask(__name__)
    app.config["DATA_FOLDER"] = config["data_path"]
    app.config["SEND_FILE_MAX_AGE_DEFAULT"] = 0
    app.url_map.strict_slashes = True

    # タスクマネージャー初期化
    task_manager = TaskManager(config)

    # 自動更新スレッド開始
    if config["auto_update"]:
        AutoUpdater(config).start()

    # --- ルーティング ---

    @app.before_request
    def log_request():
        logging.info(f"Request: {request.method} {request.url}")

    # アカウント管理エンドポイント（/<path:path>より前に定義する必要がある）
    @app.route("/api/account/<site>", methods=["GET"])
    def list_accounts(site):
        """サイトのアカウント一覧を取得"""
        # cookie_pathは辞書形式なので、サイト名で取得
        cookie_paths = config.get("cookie_path", {})
        if isinstance(cookie_paths, dict) and site in cookie_paths:
            cookie_dir = cookie_paths[site]
        else:
            # フォールバック: デフォルトパスを使用
            cookie_dir = os.path.join("cookie", site)

        accounts = []

        if os.path.exists(cookie_dir):
            for filename in os.listdir(cookie_dir):
                if filename.endswith(".json") and not filename.endswith(".backup.1"):
                    account_name = filename[:-5]  # .jsonを除去
                    file_path = os.path.join(cookie_dir, filename)
                    try:
                        with open(file_path, "r", encoding="utf-8") as f:
                            data = json.load(f)
                            # 最終更新日時を取得
                            mtime = os.path.getmtime(file_path)
                            accounts.append(
                                {
                                    "name": account_name,
                                    "file": filename,
                                    "display_name": data.get("display_name"),
                                    "updated": datetime.fromtimestamp(
                                        mtime
                                    ).isoformat(),
                                    "has_cookies": bool(data.get("cookies")),
                                    "has_ua": bool(
                                        data.get("user_agent") or data.get("ua")
                                    ),
                                }
                            )
                    except:
                        accounts.append(
                            {
                                "name": account_name,
                                "file": filename,
                                "error": "Failed to read",
                            }
                        )

        return jsonify({"site": site, "accounts": accounts})

    @app.route("/api/account", methods=["GET"])
    def list_accounts_query():
        """サイトのアカウント一覧を取得（クエリ版）"""
        site = request.args.get("site")
        if not site:
            return create_response(400, "error", "Site is required")
        return list_accounts(site)

    @app.route("/api/account/<site>", methods=["POST"])
    def upload_account(site):
        """アカウントJSONファイルをアップロード"""
        if "file" not in request.files:
            return create_response(400, "error", "No file provided")

        file = request.files["file"]
        if file.filename == "":
            return create_response(400, "error", "No file selected")

        if not file.filename.endswith(".json"):
            return create_response(400, "error", "File must be a JSON file")

        # アカウント名を取得（空の場合はランダム生成）
        account_name = request.form.get("name", "").strip()
        if not account_name:
            # ランダムな5文字の英数字を生成
            import random
            import string

            account_name = "".join(
                random.choices(string.ascii_lowercase + string.digits, k=5)
            )

        if not account_name.endswith(".json"):
            account_name += ".json"

        # 保存先ディレクトリを作成
        cookie_paths = config.get("cookie_path", {})
        if isinstance(cookie_paths, dict) and site in cookie_paths:
            cookie_dir = cookie_paths[site]
        else:
            cookie_dir = os.path.join("cookie", site)
        os.makedirs(cookie_dir, exist_ok=True)

        # ファイル名の重複チェック
        file_path = os.path.join(cookie_dir, account_name)
        base_name = account_name[:-5]  # .jsonを除去
        counter = 1
        while os.path.exists(file_path):
            # 重複する場合は連番を付加
            new_name = f"{base_name}_{counter}.json"
            file_path = os.path.join(cookie_dir, new_name)
            counter += 1

        try:
            # JSONとして検証
            content = file.read()
            data = json.loads(content)

            # 必須フィールドをチェック
            if "cookies" not in data:
                return create_response(
                    400, "error", "Invalid JSON: missing 'cookies' field"
                )

            # ファイルを保存
            with open(file_path, "wb") as f:
                f.write(content)

            saved_name = os.path.basename(file_path)
            logging.info(f"Account uploaded: {site}/{saved_name}")
            return jsonify(
                {
                    "status": "success",
                    "message": f"Account {saved_name} uploaded successfully",
                    "site": site,
                    "account": saved_name,
                }
            )
        except json.JSONDecodeError as e:
            return create_response(400, "error", f"Invalid JSON: {str(e)}")
        except Exception as e:
            logging.exception("Account upload error")
            return create_response(500, "error", str(e))

    @app.route("/api/account", methods=["POST"])
    def upload_account_query():
        """アカウントJSONファイルをアップロード（クエリ版）"""
        site = request.args.get("site")
        if not site:
            return create_response(400, "error", "Site is required")
        return upload_account(site)

    @app.route("/api/account/<site>/<account_name>", methods=["DELETE"])
    def delete_account(site, account_name):
        """アカウントを削除"""
        if not account_name.endswith(".json"):
            account_name += ".json"

        cookie_paths = config.get("cookie_path", {})
        if isinstance(cookie_paths, dict) and site in cookie_paths:
            cookie_dir = cookie_paths[site]
        else:
            cookie_dir = os.path.join("cookie", site)
        file_path = os.path.join(cookie_dir, account_name)

        try:
            if os.path.exists(file_path):
                os.remove(file_path)
                logging.info(f"Account deleted: {site}/{account_name}")
                return jsonify(
                    {
                        "status": "success",
                        "message": f"Account {account_name} deleted successfully",
                    }
                )
            else:
                return create_response(404, "error", "Account not found")
        except Exception as e:
            logging.exception("Account deletion error")
            return create_response(500, "error", str(e))

    @app.route("/api/account", methods=["DELETE"])
    def delete_account_query():
        """アカウントを削除（クエリ版）"""
        site = request.args.get("site")
        account_name = request.args.get("account")
        if not site or not account_name:
            return create_response(400, "error", "Site and account are required")
        return delete_account(site, account_name)

    @app.route("/api/account/<site>/switch", methods=["POST"])
    def switch_account(site):
        """アカウントを切り替え（login.jsonを置き換え）"""
        account_name = (
            request.json.get("account")
            if request.is_json
            else request.form.get("account")
        )

        if not account_name:
            return create_response(400, "error", "Account name is required")

        if not account_name.endswith(".json"):
            account_name += ".json"

        cookie_paths = config.get("cookie_path", {})
        if isinstance(cookie_paths, dict) and site in cookie_paths:
            cookie_dir = cookie_paths[site]
        else:
            cookie_dir = os.path.join("cookie", site)
        source_path = os.path.join(cookie_dir, account_name)
        target_path = os.path.join(cookie_dir, "login.json")

        try:
            if not os.path.exists(source_path):
                return create_response(
                    404, "error", f"Account {account_name} not found"
                )

            # 現在のlogin.jsonがある場合は、元のアカウント名に戻す（リネーム）
            if os.path.exists(target_path):
                # 現在のlogin.jsonの内容を読み込んで、元のアカウント名を特定
                try:
                    with open(target_path, "r", encoding="utf-8") as f:
                        current_data = json.load(f)
                    # 元のアカウント名を推定（ファイル名から）
                    # バックアップとしてランダム名を生成
                    import random
                    import string

                    backup_name = (
                        "".join(
                            random.choices(string.ascii_lowercase + string.digits, k=5)
                        )
                        + ".json"
                    )
                    backup_path = os.path.join(cookie_dir, backup_name)
                    # 現在のlogin.jsonをバックアップ名で保存
                    shutil.move(target_path, backup_path)
                except:
                    # 読み込み失敗時はタイムスタンプ付きでバックアップ
                    backup_path = (
                        target_path
                        + ".backup."
                        + datetime.now().strftime("%Y%m%d%H%M%S")
                    )
                    shutil.move(target_path, backup_path)

            # 選択したアカウントをlogin.jsonにリネーム
            shutil.move(source_path, target_path)

            logging.info(f"Account switched: {site} -> {account_name}")
            return jsonify(
                {
                    "status": "success",
                    "message": f"Switched to account {account_name}",
                    "site": site,
                    "account": account_name,
                }
            )
        except Exception as e:
            logging.exception("Account switch error")
            return create_response(500, "error", str(e))

    @app.route("/api/account/switch", methods=["POST"])
    def switch_account_query():
        """アカウントを切り替え（クエリ版）"""
        site = request.args.get("site")
        if not site:
            return create_response(400, "error", "Site is required")
        return switch_account(site)

    @app.route("/api/account/<site>/<account_name>/rename", methods=["POST"])
    def rename_account(site, account_name):
        """アカウント名を変更"""
        new_name = (
            request.json.get("new_name")
            if request.is_json
            else request.form.get("new_name")
        )

        if not new_name:
            return create_response(400, "error", "New account name is required")

        if not account_name.endswith(".json"):
            account_name += ".json"

        if not new_name.endswith(".json"):
            new_name += ".json"

        # 同じ名前の場合は何もしない
        if account_name == new_name:
            return jsonify({"status": "success", "message": "Account name unchanged"})

        cookie_paths = config.get("cookie_path", {})
        if isinstance(cookie_paths, dict) and site in cookie_paths:
            cookie_dir = cookie_paths[site]
        else:
            cookie_dir = os.path.join("cookie", site)

        old_path = os.path.join(cookie_dir, account_name)
        new_path = os.path.join(cookie_dir, new_name)

        # 重複チェック
        if os.path.exists(new_path):
            return create_response(
                409, "error", f"Account name '{new_name}' already exists"
            )

        try:
            if not os.path.exists(old_path):
                return create_response(404, "error", "Account not found")

            # ファイルをリネーム
            shutil.move(old_path, new_path)

            logging.info(f"Account renamed: {site}/{account_name} -> {new_name}")
            return jsonify(
                {
                    "status": "success",
                    "message": f"Account renamed from {account_name} to {new_name}",
                    "site": site,
                    "old_name": account_name,
                    "new_name": new_name,
                }
            )
        except Exception as e:
            logging.exception("Account rename error")
            return create_response(500, "error", str(e))

    @app.route("/api/account/rename", methods=["POST"])
    def rename_account_query():
        """アカウント名を変更（クエリ版）"""
        site = request.args.get("site")
        account_name = request.args.get("account")
        if not site or not account_name:
            return create_response(400, "error", "Site and account are required")
        return rename_account(site, account_name)

    @app.route("/", methods=["GET"])
    def serve_root():
        return handle_request("index.html")

    @app.route("/<path:path>", methods=["GET"])
    def handle_request(path):
        data_folder = app.config["DATA_FOLDER"]
        full_path = os.path.join(data_folder, path)

        try:
            abs_path = secure_path(data_folder, full_path)
        except ValueError as e:
            return create_response(403, "error", str(e))

        if os.path.isdir(abs_path):
            if not path.endswith("/") and path != "index.html":
                return redirect(url_for("handle_request", path=f"{path}/"))

            index_file = os.path.join(abs_path, "index.html")
            if os.path.exists(index_file):
                return _stream_file(index_file)
            else:
                return _stream_folder_contents(abs_path, data_folder)

        elif os.path.isfile(abs_path):
            return _stream_file(abs_path)

        return create_response(404, "error", "Not found")

    @app.route("/api/", methods=["POST"])
    def handle_post():
        request_id = request.values.get("request_id")
        if not request_id:
            request_id = generate_request_id()
            logging.info(f"Request ID automatically generated: {request_id}")

        # ファイル保存処理
        pdf_file = request.files.get("pdf")
        zip_file = request.files.get("zip")
        pdf_path = config["pdf_path"]

        pdf_file_name = None
        zip_file_name = None

        author_id = request.values.get("author_id")
        author_url = request.values.get("author_url")

        if pdf_file:
            if not author_id or not author_url:
                return create_response(400, "error", "Missing PDF metadata")
            pdf_file_name = f"{request_id}.pdf"
            pdf_file.save(os.path.join(pdf_path, pdf_file_name))

        if zip_file:
            zip_file_name = f"{request_id}.zip"
            zip_file.save(os.path.join(pdf_path, zip_file_name))

        # キュー登録用データ構築
        req_data = {
            "request_id": request_id,
            "pdf_path": pdf_path,
            "pdf_name": pdf_file_name,
            "zip_name": zip_file_name,
            "author_id": author_id,
            "author_url": author_url,
            "novel_type": request.values.get("novel_type"),
            "chapter": request.values.get("chapter"),
        }
        # アクションパラメータを動的に収集
        for action_name in util.ACTION_PRIORITY:
            param_key = util._resolve_param_key(action_name)
            req_data[param_key] = request.values.get(param_key)

        logging.debug(f"Queueing Task: {req_data}")

        try:
            task_manager.enqueue_request(req_data)
            return jsonify({"status": "queued", "request_id": request_id})
        except ValueError as e:
            return create_response(429, "error", str(e))
        except Exception as e:
            logging.exception("API Error")
            return create_response(500, "error", str(e))

    def _stream_file(file_path):
        def generate():
            with open(file_path, "rb") as f:
                while chunk := f.read(8192):
                    yield chunk

        mime_type, _ = mimetypes.guess_type(file_path)
        mime_type = mime_type or "application/octet-stream"
        file_size = os.path.getsize(file_path)

        return Response(
            generate(),
            content_type=mime_type,
            headers={"Content-Length": str(file_size)},
        )

    def _stream_folder_contents(folder_path, base_folder):
        def generate():
            for root, dirs, files in os.walk(folder_path):
                for name in sorted(files):
                    yield f"File: {os.path.relpath(os.path.join(root, name), base_folder)}\n"
                for name in sorted(dirs):
                    yield f"Directory: {os.path.relpath(os.path.join(root, name), base_folder)}\n"

        return Response(generate(), content_type="text/plain")

    # インデックス作成 (初回のみ)
    if "site_dic" in config:
        config["crawler"] = config["site_dic"]
    util.create_index(config["data_path"], config, "api")

    return app


def http_run(**kwargs):
    """外部から呼び出されるエントリポイント"""
    config = kwargs

    if "site_dic" in config:
        config["crawler"] = config["site_dic"]

    app = create_app(config)
    port = int(config.get("port", 8080))

    server_thread = threading.Thread(
        target=app.run,
        kwargs={"host": "0.0.0.0", "debug": False, "threaded": True, "port": port},
    )
    server_thread.daemon = True
    server_thread.start()

    while True:
        time.sleep(1)
