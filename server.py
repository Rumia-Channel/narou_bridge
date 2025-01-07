import os
import json
import random

import time
from datetime import datetime, timedelta

import requests

import threading
import queue
import pickle

from flask import Flask, request, jsonify, Response, redirect, url_for, send_file

import mimetypes

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

def setup_logging(log_path, save_log):
    """ログ設定を初期化"""
    # 共通フォーマット
    common_format = '%(asctime)s - %(levelname)s - %(message)s'
    
    # コンソールログ（改行そのまま）
    console_handler = logging.StreamHandler()
    console_handler.setFormatter(logging.Formatter(common_format))
    
    # 初期化するハンドラのリスト
    handlers = [console_handler]
    
    if save_log:
        # ファイルログ（改行を除去）
        file_handler = logging.FileHandler(os.path.join(log_path, 'server.log'), encoding='utf-8')
        file_handler.setFormatter(NoNewlineFormatter(common_format))
        handlers.append(file_handler)
    
    # ログ設定
    logging.basicConfig(
        level=logging.DEBUG,  # DEBUGレベル以上を記録
        handlers=handlers
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
def auto_update_task(domain, port, auto_update, auto_update_interval, use_ssl, use_proxy, proxy_port, proxy_ssl):

    #サーバーが起動しきるまで待機
    time.sleep(30)

    """auto_updateが有効な場合、指定された間隔で定期的にupdate_param=allをPOSTする"""
    while True:
        if auto_update:
            logging.info("Sending auto-update request with update_param=all")
            try:
                # SSL対応のURLを設定
                url = f"https://127.0.0.1:{port}/api/" if use_ssl else f"http://127.0.0.1:{port}/api/"

               
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

def create_app(config, reload_time, auto_update, save_log, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, key, use_ssl, port, domain, use_proxy, proxy_port, proxy_ssl):
    setup_logging(log_path, save_log)
    logging.debug(f"サーバー起動")

    app = Flask(__name__)
    app.config['DATA_FOLDER'] = data_path
    app.config['SEND_FILE_MAX_AGE_DEFAULT'] = 0  # キャッシュ無効化（開発用）
    app.config['TEMPLATES_AUTO_RELOAD'] = True
    app.url_map.strict_slashes = True  # スラッシュの有無に関わらず対応

    # キューの保存先ファイルパス
    JOB_FILE_PATH = os.path.join(queue_path, "queue.pkl")

    # セキュリティ: ファイルパスを正規化し、安全性を確保
    def secure_path(requested_path):
        abs_data_folder = os.path.abspath(app.config['DATA_FOLDER'])
        abs_requested_path = os.path.abspath(requested_path)

        if not abs_requested_path.startswith(abs_data_folder):
            raise ValueError(f"Access to this path is outside of the allowed directory: {requested_path}")
        return abs_requested_path

    @app.before_request
    def log_request():
        """リクエストのログ出力"""
        logging.info(f"Request URL: {request.url}")
        logging.info(f"Request Path: {request.path}")

    @app.route('/', methods=["GET"])
    def serve_root():
        """ルート (/) にアクセスされた場合、index.html を返す"""
        index_path = os.path.join(app.config['DATA_FOLDER'], "index.html")
        try:
            secure_path(index_path)
            if os.path.exists(index_path):
                logging.info(f"Serving root index.html from: {index_path}")
                return stream_file(index_path)  # HTMLファイルも通常のファイルと同じ方法で返す
            else:
                logging.error(f"File not found: {index_path}")
                return jsonify({"status": "error", "message": "File not found"}), 404
        except ValueError as e:
            logging.error(f"Unauthorized access attempt: {e}")
            return jsonify({"status": "error", "message": "Access denied"}), 403

    @app.route('/<path:path>', methods=["GET"])
    def handle_request(path):
        """指定されたパスがフォルダかファイルかを確認し、適切に処理"""
        file_path = os.path.join(app.config['DATA_FOLDER'], path)
        folder_path = os.path.join(app.config['DATA_FOLDER'], path)

        try:
            # セキュアなパスか確認
            secure_path(file_path)
        except ValueError as e:
            logging.error(f"Unauthorized access attempt: {e}")
            return jsonify({"status": "error", "message": "Access denied"}), 403

        if os.path.isfile(file_path):
            logging.info(f"Serving file from: {file_path}")
            return stream_file(file_path)  # ファイルはすべて stream_file で返す

        elif os.path.isdir(folder_path):
            if not path.endswith('/'):
                # フォルダの場合、末尾に / を追加してリダイレクト
                return redirect(url_for('handle_request', path=f'{path}/'))

            index_file = os.path.join(folder_path, "index.html")
            if os.path.exists(index_file):
                logging.info(f"Serving index.html from folder: {index_file}")
                return stream_file(index_file)  # index.htmlも通常のファイルとして扱う
            else:
                logging.warning(f"Folder or index.html not found: {folder_path}")
                return stream_folder_contents(folder_path)

        else:
            logging.error(f"Not found: {file_path}")
            return jsonify({"status": "error", "message": "Not found"}), 404

    def stream_file(file_path):
        """ファイルをストリームで送信"""
        def generate():
            try:
                with open(file_path, "rb") as f:
                    while chunk := f.read(8192):  # 8KB チャンクで送信
                        yield chunk
            except Exception as e:
                logging.error(f"Error while streaming file {file_path}: {e}")

        # MIME タイプをファイル拡張子に基づいて判別
        mime_type, _ = mimetypes.guess_type(file_path)
        
        # MIME タイプが判別できない場合、デフォルトで application/octet-stream を使用
        if not mime_type:
            mime_type = 'application/octet-stream'

        # ファイルサイズを取得し Content-Length ヘッダーに設定
        try:
            file_size = os.path.getsize(file_path)
        except Exception as e:
            logging.error(f"Could not determine file size for {file_path}: {e}")
            file_size = None  # サイズが取得できない場合

        headers = {}
        if file_size is not None:
            headers['Content-Length'] = str(file_size)

        return Response(generate(), content_type=mime_type, headers=headers)

    def stream_folder_contents(folder_path):
        """フォルダ内のコンテンツをストリームで送信"""
        def generate():
            try:
                for root, dirs, files in os.walk(folder_path):
                    for name in sorted(files):
                        yield f"File: {os.path.relpath(os.path.join(root, name), folder_path)}\n"
                    for name in sorted(dirs):
                        yield f"Directory: {os.path.relpath(os.path.join(root, name), folder_path)}\n"
            except Exception as e:
                logging.error(f"Error while streaming folder contents {folder_path}: {e}")
        
        return Response(generate(), content_type='text/plain')

    # auto_updateスレッドを開始する部分
    if auto_update:
        update_thread = threading.Thread(target=auto_update_task, args=(domain, port, auto_update, auto_update_interval, use_ssl, use_proxy, proxy_port, proxy_ssl))
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

    def save_queue_to_file(queue, lock):
        """キューをファイルに保存する"""
        with lock:  # ロックを取得してスレッドセーフに
            with open(JOB_FILE_PATH, "wb") as f:
                pickle.dump(list(queue.queue), f)  # キューの内容をリストとして保存
            logging.info(f"Queue saved to {JOB_FILE_PATH}")

    def load_queue_from_file(queue, lock):
        """ファイルからキューを復元する"""
        if not os.path.exists(JOB_FILE_PATH):
            logging.info(f"No job file found at {JOB_FILE_PATH}. Starting with an empty queue.")
            return
        
        with lock:  # ロックを取得してスレッドセーフに
            with open(JOB_FILE_PATH, "rb") as f:
                try:
                    jobs = pickle.load(f)  # ファイルからリストを読み込む
                    for job in jobs:
                        queue.put(job)  # キューに復元
                    logging.info(f"Queue restored from {JOB_FILE_PATH} with {len(jobs)} items.")
                except Exception as e:
                    logging.error(f"Failed to load queue from file: {e}")

    def process_request(req_data):
        """リクエストデータを順番に処理する関数"""
        add_param = req_data.get("add")
        update_param = req_data.get("update")
        convert_param = req_data.get("convert")
        re_download_param = req_data.get("re_download")
        request_id = req_data.get("request_id")
        pdf_path = req_data.get("pdf_path")
        pdf_name = req_data.get("pdf_name")
        author_id = req_data.get("author_id")
        author_url = req_data.get("author_url")
        novel_type = req_data.get("novel_type")
        chapter = req_data.get("chapter")
        key_data = ''

        # ホスト名の確定
        if use_proxy:
            host_name = f"https://{domain}:{proxy_port}" if proxy_ssl else f"http://{domain}:{proxy_port}"
        else:
            host_name = f"https://{domain}:{port}" if use_ssl else f"http://{domain}:{port}"

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
                
            elif pdf_name:
                add_return = util.pdf_to_text(pdf_path, pdf_name, author_id, author_url, novel_type, chapter, folder_path, data_path, key_data, host_name)
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

            # リクエストを処理
            process_request(req_data)

            # 処理後にキューを保存
            save_queue_to_file(request_queue, lock)

            request_queue.task_done()  # 処理が終わったら task_done を呼ぶ

    # アプリ起動時にキューを復元
    load_queue_from_file(request_queue, lock)

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
        pdf_file = request.files.get('pdf')
        author_id = request.form.get('author_id')
        author_url = request.form.get('author_url')
        novel_type = request.form.get("novel_type")
        chapter = request.form.get("chapter")
        request_id = request.form.get("request_id")

        if not request_id:
            return create_error_response(400, "Missing request_id")
        
        if pdf_file:
            if not author_id or not author_url:
                return create_error_response(400 ,"PDFファイル、author_id、または author_url が不足しています")
            # PDFの保存例
            pdf_file_name = str(request_id) + '.pdf'
            pdf_file.save(os.path.join(pdf_path, pdf_file_name))
        else:
            pdf_file_name = None

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
                    "pdf_path": pdf_path,
                    "pdf_name": pdf_file_name,
                    "author_id": author_id,
                    "author_url": author_url,
                    "novel_type": novel_type,
                    "chapter": chapter,
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
                "pdf_path": pdf_path,
                "pdf_name": pdf_file_name,
                "author_id": author_id,
                "author_url": author_url,
                "novel_type": novel_type,
                "chapter": chapter,
                "request_id": request_id,
            }

            request_queue.put(req_data)  # リクエストをキューに追加

            # キューを保存
            save_queue_to_file(request_queue, lock)

            return jsonify({"status": "queued", "request_id": request_id})
        else:
            return jsonify({"status": "stopped", "request_id": request_id})

    return app

# エクスポートされる関数
def http_run(config, reload_time, auto_update, save_log, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, key, use_ssl, ssl_crt, ssl_key, port, domain, use_proxy, proxy_port, proxy_ssl):

    # Flask サーバーをバックグラウンドスレッドで実行 (debug=False)
    if use_ssl:
        app = create_app(config, reload_time, auto_update, save_log, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, key, use_ssl, port, domain, use_proxy, proxy_port, proxy_ssl)
        server_thread = threading.Thread(target=app.run, kwargs={'debug': False, 'threaded': True, 'port': port, 'ssl_context': (ssl_crt, ssl_key)})
    else:
        app = create_app(config, reload_time, auto_update, save_log, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, key, use_ssl, port, domain, use_proxy, proxy_port, proxy_ssl)
        server_thread = threading.Thread(target=app.run, kwargs={'debug': False, 'threaded': True, 'port': port})
    
    server_thread.daemon = True
    server_thread.start()

    # サーバーが動作している間、メインスレッドで待機
    while True:
        time.sleep(1)
