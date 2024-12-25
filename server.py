import os
import json
import random

import time
from datetime import datetime, timedelta

import requests

import threading
import queue

from flask import Flask, request, jsonify, abort, send_from_directory, Response

#ログを保存
import logging

# 共通設定の読み込み
import util

class NoNewlineFormatter(logging.Formatter):
    """改行をスペースに置き換えるフォーマッター"""
    def format(self, record):
        message = super().format(record)
        # 改行をスペースに置き換える
        return message.replace("\n", " ").replace("\r", " ")

def setup_logging(log_path):
    """ログ設定を初期化"""
    # 共通フォーマット
    common_format = '%(asctime)s - %(levelname)s - %(message)s'
    
    # コンソールログ（改行そのまま）
    console_handler = logging.StreamHandler()
    console_handler.setFormatter(logging.Formatter(common_format))
    
    # ファイルログ（改行を除去）
    file_handler = logging.FileHandler(os.path.join(log_path, 'server.log'), encoding='utf-8')
    file_handler.setFormatter(NoNewlineFormatter(common_format))

    # ログ設定
    logging.basicConfig(
        level=logging.DEBUG,  # DEBUGレベル以上を記録
        handlers=[console_handler, file_handler]
    )

def generate_request_id():
    """JavaScriptと同じプロセスでリクエストIDを生成"""
    request_id_template = "xxxx-xxxx-4xxx-yxxx-xxxx"

    def replace_char(c):
        """ランダムな16進数（0-15）でcを置き換える"""
        r = random.randint(0, 15)  # 0から15までのランダムな値を生成
        if c == 'x':
            return hex(r)[2:]  # 'x'の場合はランダムな16進数（0-9、a-f）を返す
        elif c == 'y':
            return hex(r & 0x3 | 0x8)[2:]  # 'y'の場合は条件に合わせてランダムな16進数（8-11）を返す
        elif c == '4':
            return '4'  # '4'は固定
        return c

    # テンプレート文字列を置換してリクエストIDを生成
    return ''.join(replace_char(c) for c in request_id_template)

# サーバー起動後に自動的に更新する処理
def auto_update_task(domain, port, auto_update, auto_update_interval, use_ssl):

    #サーバーが起動しきるまで待機
    time.sleep(30)

    """auto_updateが有効な場合、指定された間隔で定期的にupdate_param=allをPOSTする"""
    while True:
        if auto_update:
            logging.info("Sending auto-update request with update_param=all")
            try:
                # SSL対応のURLを設定
                url = f"https://{domain}:443/api/" if use_ssl else f"http://{domain}:{port}/api/"
                # POSTリクエストのデータ
                payload = {
                    "update": "all",
                    "request_id": str(generate_request_id())
                }
                # リクエストを送信
                response = requests.post(url, data=payload)
                if response.status_code == 200:
                    logging.info("Auto-update request succeeded")
                else:
                    logging.warning(f"Auto-update request failed with status {response.status_code}")
            except Exception as e:
                logging.error(f"Auto-update failed: {e}")

        # 次の更新時刻を計算
        next_update_time = datetime.now() + timedelta(seconds=auto_update_interval)
        logging.info(f"Next update will be at: {next_update_time.strftime('%Y-%m-%d %H:%M:%S')}")
        
        # 指定されたインターバルでスリープ
        time.sleep(auto_update_interval)

def create_app(config, reload_time, auto_update, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, key, use_ssl, port, domain):
    setup_logging(log_path)
    logging.debug(f"サーバー起動")

    app = Flask(__name__, static_folder=data_path)  # data_path を静的ファイルのルートとして設定

    # 静的ファイルのルートを設定（data_path をルートとして）
    app.config['DATA_FOLDER'] = data_path

    # 静的ファイルのデバッグを有効にする
    app.config['SEND_FILE_MAX_AGE_DEFAULT'] = 0  # 開発時にキャッシュを無効化
    app.config['TEMPLATES_AUTO_RELOAD'] = True

    @app.before_request
    def log_request():
        """リクエストの前にパスをログに出力"""
        logging.info(f"Request URL: {request.url}")
        logging.info(f"Request Path: {request.path}")

    @app.route('/', methods=["GET"])
    def serve_root():
        """ルート / にアクセスされた場合、data_path 内の index.html を返す"""
        index_path = os.path.join(app.config['DATA_FOLDER'], "index.html")
        if os.path.exists(index_path):
            logging.info(f"Serving root index.html from: {index_path}")
            return send_from_directory(app.config['DATA_FOLDER'], "index.html")
        else:
            logging.error(f"File not found: {index_path}")
            return jsonify({"status": "error", "message": "File not found"}), 404

    @app.route('/<path:folder>', methods=["GET"])
    def serve_folder(folder):
        """指定されたフォルダ内の index.html を返す"""
        folder_path = os.path.join(app.config['DATA_FOLDER'], folder)
        index_path = os.path.join(folder_path, "index.html")

        # フォルダ内に index.html があるか確認
        if os.path.isdir(folder_path) and os.path.exists(index_path):
            logging.info(f"Serving index.html from: {index_path}")
            return send_from_directory(folder_path, "index.html")
        else:
            logging.warning(f"Folder or index.html not found for {folder}: {index_path}")
            return jsonify({"status": "error", "message": "Folder or index.html not found"}), 404

    # auto_updateスレッドを開始する部分
    if auto_update:
        update_thread = threading.Thread(target=auto_update_task, args=(domain, port, auto_update, auto_update_interval, use_ssl))
        update_thread.daemon = True
        update_thread.start()

    global request_datas
    # リクエストデータを格納する辞書
    request_datas = {}

    # スレッドセーフにするためのロック
    lock = threading.Lock()

    # リクエストを順番に処理するためのキュー
    request_queue = queue.Queue()

    util.init_import(site_dic)
    util.create_index(data_path, config, 'api')

    def process_request(req_data):
        """リクエストデータを順番に処理する関数"""
        add_param = req_data.get("add")
        update_param = req_data.get("update")
        convert_param = req_data.get("convert")
        re_download_param = req_data.get("re_download")
        request_id = req_data.get("request_id")
        key_data = ''

        # ホスト名の確定
        host_name = f"https://{domain}:443" if use_ssl else f"http://{domain}:{port}"

        try:
            # 更新処理
            if update_param:
                update_return = util.update(update_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)
                if update_return == 400:
                    return create_error_response(400, "Invalid update_param value")
                else:
                    return create_success_response("Update Complete")

            # 再ダウンロード処理
            elif re_download_param:
                re_download_return = util.re_download(re_download_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)
                if re_download_return == 400:
                    return create_error_response(400, "Invalid re_download_param value")
                else:
                    return create_success_response("Re Download Complete")

            # 変換処理
            elif convert_param:
                convert_return = util.convert(convert_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)
                if convert_return == 400:
                    return create_error_response(400, "Invalid convert_param value")
                else:
                    return create_success_response("Convert Complete")

            # ダウンロード処理
            elif add_param:
                add_return = util.download(add_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)
                if add_return == 400:
                    return create_error_response(400, "Invalid add_param value")
                else:
                    return create_success_response("Download Complete")

            # パラメータがない場合
            else:
                return create_error_response(400, "Missing parameters")

        except Exception as e:
            logging.exception("リクエストにてエラーが発生しました")
            return create_error_response(500, str(e))

    def create_error_response(status_code, message):
        """エラーレスポンスを生成"""
        logging.error(f"Error {status_code}: {message}")
        response = Response(json.dumps({"status": "error", "message": message}), mimetype="application/json")
        response.status_code = status_code
        return response

    def create_success_response(message):
        """成功レスポンスを生成"""
        logging.info(f"Success: {message}")
        response = Response(json.dumps({"status": "success", "message": message}), mimetype="application/json")
        response.status_code = 200
        return response

    def process_queue():
        """キューからリクエストを順番に取り出して処理するバックグラウンドスレッド"""
        while True:
            req_data = request_queue.get()  # キューからリクエストを取り出す
            if req_data is None:  # None が入った場合はスレッドを終了
                break
            process_request(req_data)
            request_queue.task_done()  # 処理が終わったら task_done を呼ぶ

    # キュー処理用のスレッドを開始
    queue_thread = threading.Thread(target=process_queue)
    queue_thread.daemon = True  # デーモンスレッドとして実行
    queue_thread.start()

    @app.route('/api/', methods=['POST'])
    def handle_post():
        global request_datas
        """POSTリクエストを受け取って処理を開始する"""
        logging.debug(f"POST received: {datetime.now().isoformat()}")

        # POSTデータの取得
        add_param = request.form.get("add")
        update_param = request.form.get("update")
        convert_param = request.form.get("convert")
        re_download_param = request.form.get("re_download")
        request_id = request.form.get("request_id")

        if not request_id:
            return create_error_response(400, "Missing request_id")

        with lock:

            # リクエストIDが既に存在し、再送信がreload_time以内の場合、エラーを返す
            if request_id in request_datas:
                previous_request_time = request_datas[request_id]['time']
                time_diff = (datetime.now() - previous_request_time).total_seconds()
                if time_diff <= int(reload_time):
                    return create_error_response(429, f"Request {request_id} already exists within reload time.")

            # リクエストデータを保存
            request_datas[request_id] = {
                "time": datetime.now(),
                "data": {
                    "add": add_param,
                    "update": update_param,
                    "convert": convert_param,
                    "re_download": re_download_param,
                }
            }

            # 古いリクエストIDを削除
            request_datas, queue_stop = util.cleanup_expired_requests(request_datas, expiration_time=int(reload_time))

        logging.debug(f'Current queue: {request_datas}')

        if not queue_stop:
            # キューにリクエストを追加
            req_data = {
                "add": add_param,
                "update": update_param,
                "convert": convert_param,
                "re_download": re_download_param,
                "request_id": request_id,
            }

            request_queue.put(req_data)  # リクエストをキューに追加

            return jsonify({"status": "queued", "request_id": request_id})
        else:
            return jsonify({"status": "stopped", "request_id": request_id})

    return app

# エクスポートされる関数
def http_run(config, reload_time, auto_update, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, key, use_ssl, ssl_crt, ssl_key, port, domain):

    # Flask サーバーをバックグラウンドスレッドで実行 (debug=False)
    if use_ssl:
        app = create_app(config, reload_time, auto_update, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, key, use_ssl, 443, domain)
        server_thread = threading.Thread(target=app.run, kwargs={'debug': False, 'threaded': True, 'port': port, 'ssl_context': (ssl_crt, ssl_key)})
    else:
        app = create_app(config, reload_time, auto_update, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, key, use_ssl, port, domain)
        server_thread = threading.Thread(target=app.run, kwargs={'debug': False, 'threaded': True, 'port': port})
    
    server_thread.daemon = True
    server_thread.start()

    # サーバーが動作している間、メインスレッドで待機
    while True:
        time.sleep(1)
