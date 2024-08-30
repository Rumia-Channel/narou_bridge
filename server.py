from http.server import BaseHTTPRequestHandler, HTTPServer
import urllib.parse
import ssl as ssl_lib
import hashlib
import os
import importlib
import random
import string
import datetime

# アクセスを制限するファイルやフォルダのリスト
restricted_items = ['cookie.json', '.json', '.js', '.key']

# リクエストIDの保存用辞書
recent_request_ids = {}

def cleanup_expired_requests(requests_dict, expiration_time):
    current_time = datetime.datetime.now()
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

            # リクエストされたパスを取得
            requested_path = urllib.parse.urlparse(self.path).path.strip('/')
            file_path = os.path.join(data_path, requested_path)

            # リクエストされたパスが制限されたパスに含まれているかチェック
            for restricted_item in restricted_items:
                if requested_path.startswith(restricted_item) or requested_path.endswith(restricted_item):
                    self.send_response(403)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Access to this file or folder is restricted!")
                    return

            # リクエストされたパスが空の場合は data_path をディレクトリとして扱う
            if requested_path == '':
                file_path = os.path.join(data_path, 'index.html')
            elif os.path.isdir(file_path):
                file_path = os.path.join(file_path, 'index.html')

            if os.path.exists(file_path):
                self.send_response(200)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                with open(file_path, 'rb') as file:
                    self.wfile.write(file.read())
            else:
                self.send_response(404)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                self.wfile.write(b"File not found!")

        def do_POST(self):
            print(f'POST received: {datetime.datetime.now()}')
            req_key = self.parse_query_params()
            if not self.check_auth(req_key):
                return

            content_length = int(self.headers['Content-Length'])
            post_data = self.rfile.read(content_length).decode('utf-8')
            post_params = urllib.parse.parse_qs(post_data)
            add_param = post_params.get("add", [None])[0]
            request_id = post_params.get("request_id", [None])[0]

            if request_id is None:
                self.send_response(400)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                self.wfile.write(b"Invalid add_param or request_id value")
                return

            # リクエストIDを表示
            print(f"Request ID: {request_id}")

            # 重複チェック: すでに同じリクエストIDがあるか確認
            if request_id in recent_request_ids:
                self.send_response(429)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                self.wfile.write(b"Duplicate request detected")
                return

            # リクエストIDを記録
            recent_request_ids[request_id] = datetime.datetime.now()

            #ダウンロード処理
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
                globals()[site].download(add_param, folder_path)

            # 古いリクエストIDのクリーンアップ
            cleanup_expired_requests(recent_request_ids, expiration_time=600)

    # 認証キーを用いるか
    if enc_key == 1:
        script_folder = os.path.dirname(os.path.abspath(__file__))
        auth_file = os.path.join(script_folder, "auth.key")

        if not os.path.exists(auth_file):
            # Generate random string
            random_string = ''.join(random.choices(string.ascii_letters + string.digits + string.punctuation, k=random.randint(64, 512)))

            # Hash the random string
            hashed_string = hashlib.sha3_256(random_string.encode('utf-8')).hexdigest()

            # Write hashed string to auth.key
            with open(auth_file, 'w', encoding='utf-8') as file:
                file.write(hashed_string)

            # Assign hashed string to auth_key
            auth_key = hashed_string

            print(f"Auth key generated: {auth_key}")
            print("Please check auth.key for the auth key")
        else:
            # Read existing auth key from auth.key
            with open(auth_file, 'r', encoding='utf-8') as file:
                auth_key = file.read().strip()
    else:
        auth_key = None  
    
    server_address = ('', port)
    httpd = HTTPServer(server_address, RequestHandler)

    #SSL証明書の設定
    if use_ssl == 1:
        cert_path = f"/etc/letsencrypt/live/{domain}/fullchain.pem"
        key_path = f"/etc/letsencrypt/live/{domain}/privkey.pem"
        httpd.socket = ssl_lib.wrap_socket(httpd.socket, certfile=cert_path, keyfile=key_path, server_side=True)

    print(f"Starting httpd server on port {port}")
    httpd.serve_forever()
