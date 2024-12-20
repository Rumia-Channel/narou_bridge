from http.server import BaseHTTPRequestHandler, HTTPServer
import urllib.parse
import ssl as ssl_lib
import hashlib
import os
import random
import string
from datetime import datetime

#共通設定の読み込み
import util

# アクセスを制限するファイルやフォルダのリスト
restricted_items = ['login.json', '.js', '.key']

# リクエストIDの保存用辞書
recent_request_ids = {}

def http_run(interval, site_dic, login_dic, folder_path, data_path, cookie_path, enc_key, use_ssl, port, domain):

    globals().update(util.import_modules(site_dic))

    class RequestHandler(BaseHTTPRequestHandler):
        def parse_query_params(self):
            query_components = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
            req_key = query_components.get("key", [None])[0]
            return req_key
        
        def check_auth(self, req_key):
            if enc_key == 1 and (req_key != auth_key or auth_key is None):
                self.send_response(403)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                self.wfile.write(b"Access denied!")
                return False
            return True
        
        util.init_import(site_dic)
        
        def do_GET(self):
            req_key = self.parse_query_params()

            if not self.check_auth(req_key):
                return

            print(self.path)

            # リクエストされたパスを取得
            parsed_url = urllib.parse.urlparse(self.path)
            requested_path = parsed_url.path.strip('/')
            if requested_path == '':
                requested_path = 'index.html'

            # デバッグ用: リクエストパスを出力
            print(f"Requested path: {requested_path}")

            # ファイルパスの生成
            file_path = os.path.join(data_path, requested_path)
            print(f"File path: {file_path}")

            # 制限されたパスのチェック
            for restricted_item in restricted_items:
                if restricted_item in requested_path:
                    self.send_response(403)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Access to this file or folder is restricted!")
                    return

            # ディレクトリの場合は index.html をデフォルトで設定
            if os.path.isdir(file_path):
                file_path = os.path.join(file_path, 'index.html')
                print(f"Updated file path for directory: {file_path}")

            # ファイルが存在するか確認
            if os.path.exists(file_path):
                # ファイルの MIME タイプを判定
                mime_type = self.guess_mime_type(file_path)
                self.send_response(200)
                self.send_header("Content-type", mime_type)
                self.end_headers()
                with open(file_path, 'rb') as file:
                    self.wfile.write(file.read())
            else:
                self.send_response(404)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                self.wfile.write(b"File not found!")

        def guess_mime_type(self, file_path):
            # 拡張子に基づいて MIME タイプを判定
            ext_to_mime = {
                '.html': 'text/html',
                '.htm': 'text/html',
                '.txt': 'text/plain',
                '.css': 'text/css',
                '.js': 'application/javascript',
                '.json': 'application/json',
                '.png': 'image/png',
                '.jpg': 'image/jpeg',
                '.jpeg': 'image/jpeg',
                '.gif': 'image/gif',
                '.pdf': 'application/pdf',
                # その他の拡張子と MIME タイプを追加
            }

            _, ext = os.path.splitext(file_path)
            return ext_to_mime.get(ext, 'application/octet-stream')  # デフォルトはバイナリ

        def do_POST(self):
            print(f'POST received: {datetime.now().astimezone().strftime("%Y-%m-%d %H:%M:%S%z")}')
            req_key = self.parse_query_params()
            if not self.check_auth(req_key):
                return

            content_length = int(self.headers['Content-Length'])
            post_data = self.rfile.read(content_length).decode('utf-8')
            post_params = urllib.parse.parse_qs(post_data)
            add_param = post_params.get("add", [None])[0]
            update_param = post_params.get("update", [None])[0]
            convert_param = post_params.get("convert", [None])[0]
            re_download_param = post_params.get("re_download", [None])[0]
            request_id = post_params.get("request_id", [None])[0]

            if request_id is None:
                self.send_response(400)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                self.wfile.write(b"Invalid add_param or request_id value")
                return

            # リクエストIDを表示
            print(f"Request ID: {request_id}")

            # 重複チェック
            if request_id in recent_request_ids:
                self.send_response(429)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                self.wfile.write(b"Duplicate request detected")
                return

            # リクエストIDを記録
            recent_request_ids[request_id] = datetime.now()

            #ホスト名の確定
            if use_ssl == 1:
                host_name = f'https://{domain}:{port}'
            else:
                host_name = f'http://{domain}:{port}'

            # 更新処理
            if not update_param is None:

                update_return = util.update(update_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

                if update_return == 400:
                    self.send_response(400)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Invalid update_param value")
                    print("\nInvalid update_param value\n")
                    return
                else:
                    self.send_response(200)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Update Complete")
                    print("\nUpdate Complete\n")
                    return

            # 再ダウンロード処理
            elif not re_download_param is None:
                
                re_download_return = util.re_download(re_download_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

                if re_download_return == 400:
                    self.send_response(400)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Invalid re_download_param value")
                    print("\nInvalid re_download_param value\n")
                    return
                else:
                    self.send_response(200)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Update Complete")
                    print("\nRe Download Complete\n")
                    return

            # 変換処理
            elif not convert_param is None:
                
                convert_return = util.convert(convert_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

                if convert_return == 400:
                    self.send_response(400)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Invalid convert_param value")
                    print("\nInvalid convert_param value\n")
                    return
                else:
                    self.send_response(200)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Convert Complete")
                    print("\nConvert Complete\n")
                    return

            # ダウンロード処理
            elif not add_param is None:
                add_return = util.download(add_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

                if add_return == 400:
                    self.send_response(400)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Invalid add_param value")
                    print("\nInvalid add_param value\n")
                    return
                else:
                    self.send_response(200)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Download Complete")
                    print("\nDownload Complete\n")
                    return

            # 古いリクエストIDのクリーンアップ
            recent_request_ids = util.cleanup_expired_requests(recent_request_ids, expiration_time=600)

    # 認証キーを用いるか
    if enc_key:
        script_folder = os.path.dirname(os.path.abspath(__file__))
        auth_file = os.path.join(script_folder, "auth.key")

        if not os.path.exists(auth_file):
            # ランダムな文字列を生成
            random_string = ''.join(random.choices(string.ascii_letters + string.digits + string.punctuation, k=random.randint(64, 512)))
            # 生成した文字列をハッシュ化
            hashed_string = hashlib.sha3_256(random_string.encode('utf-8')).hexdigest()
            # auth.key に書き込む
            with open(auth_file, 'w', encoding='utf-8') as file:
                file.write(hashed_string)

            auth_key = hashed_string

            print(f"Auth key generated: {auth_key}")
            print("Please check auth.key for the auth key")
        else:
            # 既存の auth_key を読み込む
            with open(auth_file, 'r', encoding='utf-8') as file:
                auth_key = file.read().strip()
    else:
        auth_key = None

    global key_data
    if auth_key is not None:
        key_data = '?key=' + auth_key
    else:
        key_data = ''
    
    server_address = ('', port)
    httpd = HTTPServer(server_address, RequestHandler)

    # SSL証明書の設定
    if use_ssl == 1:
        cert_path = f"/etc/letsencrypt/live/{domain}/fullchain.pem"
        key_path = f"/etc/letsencrypt/live/{domain}/privkey.pem"
        httpd.socket = ssl_lib.wrap_socket(httpd.socket, certfile=cert_path, keyfile=key_path, server_side=True)

    print(f"Starting httpd server on port {port}")
    httpd.serve_forever()
