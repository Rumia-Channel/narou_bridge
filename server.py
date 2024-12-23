from flask import Flask, request, jsonify, abort, send_from_directory, Response
from datetime import datetime
import threading
import util
import time
import os
import json

def create_app(config, reload_time, interval, site_dic, login_dic, folder_path, data_path, cookie_path, key, use_ssl, port, domain):
    app = Flask(__name__, static_folder=data_path)  # data_path を静的ファイルのルートとして設定

    # 静的ファイルのルートを設定（data_path をルートとして）
    app.config['DATA_FOLDER'] = data_path

    # 静的ファイルのデバッグを有効にする
    app.config['SEND_FILE_MAX_AGE_DEFAULT'] = 0  # 開発時にキャッシュを無効化
    app.config['TEMPLATES_AUTO_RELOAD'] = True

    @app.before_request
    def log_request():
        """リクエストの前にパスをログに出力"""
        print(f"Request URL: {request.url}")
        print(f"Request Path: {request.path}")

    @app.route('/', methods=["GET"])
    def serve_root():
        """ルート / にアクセスされた場合、data_path 内の index.html を返す"""
        index_path = os.path.join(app.config['DATA_FOLDER'], "index.html")
        if os.path.exists(index_path):
            print(f"Serving root index.html from: {index_path}")
            return send_from_directory(app.config['DATA_FOLDER'], "index.html")
        else:
            print(f"File not found: {index_path}")
            return jsonify({"status": "error", "message": "File not found"}), 404

    @app.route('/<path:folder>', methods=["GET"])
    def serve_folder(folder):
        """指定されたフォルダ内の index.html を返す"""
        folder_path = os.path.join(app.config['DATA_FOLDER'], folder)
        index_path = os.path.join(folder_path, "index.html")
        
        # フォルダ内に index.html があるか確認
        if os.path.isdir(folder_path) and os.path.exists(index_path):
            print(f"Serving index.html from: {index_path}")
            return send_from_directory(folder_path, "index.html")
        else:
            print(f"Folder or index.html not found for {folder}: {index_path}")
            return jsonify({"status": "error", "message": "Folder or index.html not found"}), 404

    # リクエストを格納するリスト
    request_queue = []

    # スレッドセーフにするためのロック
    lock = threading.Lock()

    # recent_request_ids をスレッドセーフに管理
    recent_request_ids = {}

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

            # パラメータがない場合
            else:
                return create_error_response(400, "Missing parameters")

        except Exception as e:
            return create_error_response(500, str(e))

    def create_error_response(status_code, message):
        """エラーレスポンスを生成"""
        response = Response(json.dumps({"status": "error", "message": message}), mimetype="application/json")
        response.status_code = status_code
        return response

    def create_success_response(message):
        """成功レスポンスを生成"""
        response = Response(json.dumps({"status": "success", "message": message}), mimetype="application/json")
        response.status_code = 200
        return response

    @app.route('/api/', methods=['POST'])
    def handle_post():
        """POSTリクエストを受け取って処理を開始する"""
        print(f"POST received: {datetime.now().isoformat()}")

        # POSTデータの取得
        add_param = request.form.get("add")
        update_param = request.form.get("update")
        convert_param = request.form.get("convert")
        re_download_param = request.form.get("re_download")
        request_id = request.form.get("request_id")

        if not request_id:
            return create_error_response(400, "Missing request_id")


        with lock:
            # 古いリクエストIDを削除
            util.cleanup_expired_requests(recent_request_ids, expiration_time=int(reload_time))

            print(f'リクエストID: {recent_request_ids}')

            if request_id in recent_request_ids:
                return create_error_response(429, "Duplicate request detected")

            recent_request_ids[request_id] = datetime.now()

        # キューにリクエストを追加
        req_data = {
            "add": add_param,
            "update": update_param,
            "convert": convert_param,
            "re_download": re_download_param,
            "request_id": request_id,
        }

        threading.Thread(target=process_request, args=(req_data,)).start()

        return jsonify({"status": "queued", "request_id": request_id})

    return app

# エクスポートされる関数
def http_run(config, reload_time, interval, site_dic, login_dic, folder_path, data_path, cookie_path, key, use_ssl, port, domain):
    app = create_app(config, reload_time, interval, site_dic, login_dic, folder_path, data_path, cookie_path, key, use_ssl, port, domain)
    
    # Flask サーバーをバックグラウンドスレッドで実行 (debug=False)
    server_thread = threading.Thread(target=app.run, kwargs={'debug': False, 'threaded': True, 'port': port})
    server_thread.daemon = True
    server_thread.start()

    # サーバーが動作している間、メインスレッドで待機
    while True:
        time.sleep(1)

if __name__ == "__main__":
    import argparse

    # コマンドライン引数を定義
    parser = argparse.ArgumentParser(description="Start the Flask API server.")
    parser.add_argument("--config", required=True, help="Path to the configuration file.")
    parser.add_argument("--reload_time", type=int, required=True, help="Reload interval time in seconds.")
    parser.add_argument("--interval", type=int, required=True, help="Request interval time in seconds.")
    parser.add_argument("--site_dic", required=True, help="Site dictionary.")
    parser.add_argument("--login_dic", required=True, help="Login dictionary.")
    parser.add_argument("--folder_path", required=True, help="Folder path.")
    parser.add_argument("--data_path", required=True, help="Data path.")
    parser.add_argument("--cookie_path", required=True, help="Cookie path.")
    parser.add_argument("--key", required=True, help="Encryption key.")
    parser.add_argument("--use_ssl", action="store_true", help="Use SSL.")
    parser.add_argument("--port", type=int, required=True, help="Port number.")
    parser.add_argument("--domain", required=True, help="Domain name.")

    args = parser.parse_args()

    # 設定をアプリに渡す
    http_run(
        config=args.config,
        reload_time=args.reload_time,
        interval=args.interval,
        site_dic=args.site_dic,
        login_dic=args.login_dic,
        folder_path=args.folder_path,
        data_path=args.data_path,
        cookie_path=args.cookie_path,
        key=args.key,
        use_ssl=args.use_ssl,
        port=args.port,
        domain=args.domain
    )