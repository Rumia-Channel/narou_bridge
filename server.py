from http.server import BaseHTTPRequestHandler, HTTPServer
import urllib.parse
import ssl
import hashlib
import os
import importlib


def sprit_url():
    pass

def http_run(interval, site_dic, folder_path, key, ssl, port, domain):

    for key, value in site_dic.items():
        module_name = 'crawler.'+value.replace('.py', '')
        module = importlib.import_module(module_name)
        globals()[key] = module

    class RequestHandler(BaseHTTPRequestHandler):
        def parse_query_params(self):
            query_components = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
            req_key = query_components.get("key", [None])[0]
            return req_key
        
        def check_auth(self, req_key):
                if not req_key == auth_key or auth_key is None:
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

            # インデックスページとしてfolder_pathに格納されたパスを登録
            index_path = os.path.join(folder_path, 'index.html')
            if os.path.exists(index_path):
                self.send_response(200)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                with open(index_path, 'rb') as file:
                    self.wfile.write(file.read())
            else:
                self.send_response(404)
                self.send_header("Content-type", "text/html")
                self.end_headers()
                self.wfile.write(b"Index page not found!")

        def do_POST(self):
            req_key = self.parse_query_params()
            if not self.check_auth(req_key):
                return
            

            content_length = int(self.headers['Content-Length'])
            post_data = self.rfile.read(content_length).decode('utf-8')
            post_params = urllib.parse.parse_qs(post_data)
            add_param = post_params.get("add", [None])[0]

            if add_param is not None:
                # webサイトの判別
                site = None
                for key, value in site_dic.items():
                    if value.replace('_', '.').replace('.py', '') in add_param:
                        site = key
                        break
                else:
                    self.send_response(400)
                    self.send_header("Content-type", "text/html")
                    self.end_headers()
                    self.wfile.write(b"Invalid add_param value")
                    return
                    
                print(add_param)
                globals()[site].run(add_param, folder_path)

            

    # 認証キーを用いるか
    if key == 1:

        script_folder = os.path.dirname(os.path.abspath(__file__))
        auth_file = os.path.join(script_folder, "auth.txt")

        if not os.path.exists(auth_file):
            # Generate random string
            random_string = "random_string"  # Replace with your random string generation logic

            # Hash the random string
            hashed_string = hashlib.sha3_256(random_string.encode('utf-8')).hexdigest()

            # Write hashed string to auth.txt
            with open(auth_file, 'w', encoding='utf-8') as file:
                file.write(hashed_string)

            # Assign hashed string to auth_key
            auth_key = hashed_string

            print(f"Auth key generated: {auth_key}")
            print("Please check auth.txt for the auth key")
        else:
            # Read existing auth key from auth.txt
            with open(auth_file, 'r', encoding='utf-8') as file:
                auth_key = file.read().strip()
    else:
        auth_key = None  
    
    server_address = ('', port)
    httpd = HTTPServer(server_address, RequestHandler)

    #SSL証明書の設定
    if ssl == 1:
        cert_path = f"/etc/letsencrypt/live/{domain}/fullchain.pem"
        key_path = f"/etc/letsencrypt/live/{domain}/privkey.pem"
        httpd.socket = ssl.wrap_socket(httpd.socket, certfile=cert_path, keyfile=key_path, server_side=True)

    print(f"Starting httpd server on port {port}")
    httpd.serve_forever()
