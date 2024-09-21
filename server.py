from http.server import BaseHTTPRequestHandler, HTTPServer
import urllib.parse
import ssl as ssl_lib
import hashlib
import os
import importlib
import random
import string
from datetime import datetime

# アクセスを制限するファイルやフォルダのリスト
restricted_items = ['cookie.json', 'ua.txt', '.json', '.js', '.key']

# リクエストIDの保存用辞書
recent_request_ids = {}

def cleanup_expired_requests(requests_dict, expiration_time):
    current_time = datetime.now()
    for key in list(requests_dict):
        if (current_time - requests_dict[key]).total_seconds() > expiration_time:
            del requests_dict[key]

def http_run(site_dic, folder_path, data_path, enc_key, use_ssl, port, domain):

    for site_key, value in site_dic.items():
        module_name = 'crawler.' + value.replace('.py', '')
        module = importlib.import_module(module_name)
        globals()[site_key] = module

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
                if not update_param == 'all':
                    # 更新処理
                    for site_key, value in site_dic.items():
                        if update_param == site_key:
                            site = site_key
                            break
                    else:
                        self.send_response(400)
                        self.send_header("Content-type", "text/html")
                        self.end_headers()
                        self.wfile.write(b"Invalid update_parm value")
                        return
                    print(f'Update: {site}\n')
                    globals()[site].init(folder_path)
                    globals()[site].update(folder_path, key_data, data_path, host_name)
                else:
                    # 全更新処理
                    for site_key, value in site_dic.items():
                        print(f'Update: {site_key}\n')
                        globals()[site_key].init(folder_path)
                        globals()[site_key].update(folder_path, key_data, data_path, host_name)
                
                print("Update Complete\n")

            # ダウンロード処理
            if not add_param is None:
                # webサイトの判別
                site = None
                for site_key, value in site_dic.items():
                    if value.replace('_', '.').replace('.py', '') in add_param:
                        site = site_key
                        break
                else:
                    self.send_response(400)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Invalid add_param value")
                    return

                print(f'Web site: {site}')
                print(f'URL: {add_param}')
                globals()[site].init(folder_path)
                globals()[site].download(add_param, folder_path, key_data, data_path, host_name)

            # 古いリクエストIDのクリーンアップ
            cleanup_expired_requests(recent_request_ids, expiration_time=600)

    # 認証キーを用いるか
    if enc_key == 1:
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
