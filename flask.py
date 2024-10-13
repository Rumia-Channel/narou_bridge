import os
from flask import Flask, request, jsonify
import configparser
import random
import binascii
import hashlib
from Crypto.Cipher import AES
from Crypto.Util.Padding import unpad
import string
import json

app = Flask(__name__)

# ワーカークラス
class Worker:
    def __init__(self):
        self.worker = {}  # 辞書型で worker を初期化
        self.worker_task = {} # 辞書型で worker_task を初期化
        self.worker_id = 0  # worker ID を初期化

    def add_worker(self, data, hex_iv):
        script_folder = os.path.dirname(os.path.abspath(__file__))
        auth_file = os.path.join(script_folder, "auth.key")
        
        # 既存の auth_key を読み込む
        with open(auth_file, 'r', encoding='utf-8') as file:
            key_text = file.read().strip()
        
        # SHA3-256 でハッシュを生成し、バイナリ形式に変換
        auth_key = hashlib.sha3_256(key_text.encode('utf-8')).digest()  # バイナリ形式で取得

        # IV を16バイトに変換
        iv = binascii.unhexlify(hex_iv)

        # data をバイナリ形式に変換（ここでは Base64 形式を想定）
        try:
            # 例えば、data が Base64 でエンコードされている場合
            data_bytes = binascii.a2b_base64(data)
        except (binascii.Error, ValueError):
            raise ValueError("data must be a valid Base64 encoded string")

        try:
            # AES 複合化
            decrypted_data = AES.new(auth_key, AES.MODE_CBC, iv).decrypt(data_bytes)

            # パディングを除去し、JSON データを読み込む
            json_data = json.loads(unpad(decrypted_data, AES.block_size).decode('utf-8'))

            # 既存のデータと一致するか確認
            if json_data not in self.worker.values():
                print(f"New worker ID: {self.worker_id}")
                print(f"New worker IP: {json_data.get("ip")}")
                # worker 辞書に JSON データを追加
                self.worker[self.worker_id] = json_data  # 新しいエントリを追加
                self.worker_id += 1  # サーバー ID をインクリメント

        except ValueError as e:
            print(f"Error during decryption or JSON loading: {e}")
            return None

# サーバーの設定
class Setting:
    def __init__(self):

        site_dic = {}
        login_dic = {}

        #-------------------------------------------------------------------------------------

        config = configparser.ConfigParser()
        config.read(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'setting.ini'))
        
        # 指定されないならカレントディレクトリ
        if not data_path:
            data_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'data')

        # ないなら作れdataフォルダ
        if not os.path.exists(data_path):
            os.makedirs(data_path)

        # dataフォルダとサイト名のマトリョシカを作成
        for key in config['crawler']:
            folder_name = key
            site_dic[key] = config['crawler'][key]
            login_dic[key] = int(config['login'][key])
            folder_path = os.path.join(data_path, folder_name)
            if not os.path.exists(folder_path):
                os.makedirs(folder_path)

        if config['server']['job'] == 'server':

            # クラスタリングを行うかどうか
            if int(config['server']['clustering']) == 1:
                clustering = True
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
                else:
                    # 既存の auth_key を読み込む
                    with open(auth_file, 'r', encoding='utf-8') as file:
                        auth_key = file.read().strip()
            else:
                clustering = False

            

        #-------------------------------------------------------------------------------------

        #設定
        self.site_dic = site_dic
        self.login_dic = login_dic
        self.data_path = data_path
        self.clustering = clustering


#ワーカーのインスタンスを作成して保持
worker = Worker()

#設定のインスタンスを作成して保持
setting = Setting()

#トップページを開いた際に
@app.route('/', methods=['GET'])
def init():
    
    
    if config['server']['job'] == 'server':

        # クラスタリングを行うかどうか
        if int(config['server']['clustering']) == 1:
            worker.clustering = True
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
            else:
                # 既存の auth_key を読み込む
                with open(auth_file, 'r', encoding='utf-8') as file:
                    auth_key = file.read().strip()

        # Indexファイルを作成
        with open(os.path.join(data_path, 'index.html'), 'w', encoding='utf-8') as f:
            f.write('<!DOCTYPE html>\n')
            f.write('<html lang="ja">\n')
            f.write('<head>\n')
            f.write('<meta charset="UTF-8">\n')
            f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
            f.write('<script>\n')

            # URLにパラメータを付与してリダイレクトする関数
            f.write('function redirectWithParams(baseURL) {\n')
            f.write('  var params = document.location.search;\n')
            f.write('  var newURL = baseURL + params;\n')
            f.write('  window.location.href = newURL;\n')
            f.write('}\n')

            # リクエストIDを生成する関数
            f.write('function generateRequestId() {\n')
            f.write('  return "xxxx-xxxx-4xxx-yxxx-xxxx".replace(/[xy]/g, function(c) {\n')
            f.write('    var r = Math.random() * 16 | 0, v = c === "x" ? r : (r & 0x3 | 0x8);\n')
            f.write('    return v.toString(16);\n')
            f.write('  });\n')
            f.write('}\n')

            # 関数を指定された遅延時間後に実行するデバウンス処理
            f.write('function debounce(func, delay) {\n')
            f.write('  var timeoutId;\n')
            f.write('  return function() {\n')
            f.write('    clearTimeout(timeoutId);\n')
            f.write('    timeoutId = setTimeout(func, delay);\n')
            f.write('  };\n')
            f.write('}\n')

            # デバウンスされたsubmit関数
            f.write('var debouncedSubmit = debounce(submit, 1000);\n')

            # input1の値をPOSTリクエストで送信する関数
            f.write('function submit() {\n')
            f.write('  var input1 = document.getElementById("input1").value;\n')
            f.write('  var requestId = generateRequestId();\n')
            f.write('  var currentUrl = new URL(window.location.href);\n')
            f.write('  var keyParam = currentUrl.searchParams.get("key");\n')
            f.write('  var url = "/post/#?add=" + encodeURIComponent(input1);\n')
            f.write('  if (keyParam) {\n')
            f.write('    url += "&key=" + encodeURIComponent(keyParam);\n')
            f.write('  }\n')
            f.write('  var xhr = new XMLHttpRequest();\n')
            f.write('  xhr.open("POST", url, true);\n')
            f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
            f.write('  xhr.onreadystatechange = function() {\n')
            f.write('    if (xhr.readyState === 4) {\n')
            f.write('      if (xhr.status === 200) {\n')
            f.write('        alert("Compleat !!");\n')
            f.write('      } else {\n')
            f.write('        document.body.innerHTML += xhr.responseText;\n')
            f.write('      }\n')
            f.write('    }\n')
            f.write('  };\n')
            f.write('  xhr.send("add=" + encodeURIComponent(input1) + "&request_id=" + requestId);\n')
            f.write('}\n')

            # updateパラメーターを持ったリクエストを送信する関数
            f.write('function submitUpdate(key) {\n')
            f.write('  var requestId = generateRequestId();\n')
            f.write('  var url = "/post/#?update=" + encodeURIComponent(key);\n')
            f.write('  var xhr = new XMLHttpRequest();\n')
            f.write('  xhr.open("POST", url, true);\n')
            f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
            f.write('  xhr.onreadystatechange = function() {\n')
            f.write('    if (xhr.readyState === 4) {\n')
            f.write('      if (xhr.status === 200) {\n')
            f.write('        alert("Compleat !!");\n')
            f.write('      } else {\n')
            f.write('        document.body.innerHTML += xhr.responseText;\n')
            f.write('      }\n')
            f.write('    }\n')
            f.write('  };\n')
            f.write('  xhr.send("update=" + encodeURIComponent(key) + "&request_id=" + requestId);\n')
            f.write('}\n')

            # convertパラメーターを持ったリクエストを送信する関数
            f.write('function submitConvert(key) {\n')
            f.write('  var requestId = generateRequestId();\n')
            f.write('  var url = "/post/#?convert=" + encodeURIComponent(key);\n')
            f.write('  var xhr = new XMLHttpRequest();\n')
            f.write('  xhr.open("POST", url, true);\n')
            f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
            f.write('  xhr.onreadystatechange = function() {\n')
            f.write('    if (xhr.readyState === 4) {\n')
            f.write('      if (xhr.status === 200) {\n')
            f.write('        alert("Compleat !!");\n')
            f.write('      } else {\n')
            f.write('        document.body.innerHTML += xhr.responseText;\n')
            f.write('      }\n')
            f.write('    }\n')
            f.write('  };\n')
            f.write('  xhr.send("convert=" + encodeURIComponent(key) + "&request_id=" + requestId);\n')
            f.write('}\n')

            # re_downloadパラメーターを持ったリクエストを送信する関数
            f.write('function submitReDownload(key) {\n')
            f.write('  var requestId = generateRequestId();\n')
            f.write('  var url = "/post/#?re_download=" + encodeURIComponent(key);\n')
            f.write('  var xhr = new XMLHttpRequest();\n')
            f.write('  xhr.open("POST", url, true);\n')
            f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
            f.write('  xhr.onreadystatechange = function() {\n')
            f.write('    if (xhr.readyState === 4) {\n')
            f.write('      if (xhr.status === 200) {\n')
            f.write('        alert("Compleat !!");\n')
            f.write('      } else {\n')
            f.write('        document.body.innerHTML += xhr.responseText;\n')
            f.write('      }\n')
            f.write('    }\n')
            f.write('  };\n')
            f.write('  xhr.send("re_download=" + encodeURIComponent(key) + "&request_id=" + requestId);\n')
            f.write('}\n')

            f.write('</script>\n')
            f.write('<title>Index</title>\n')
            f.write('</head>\n')
            f.write('<body>\n')

            # 入力フィールドと送信ボタン
            f.write('<input type="text" id="input1" placeholder="登録URL">\n')
            f.write('<button onclick="debouncedSubmit()">送信</button><br><br><br>\n')

            f.write(f'<button onclick="submitUpdate(\'all\')">全て 更新</button>\n')  # 更新ボタン追加
            f.write(f'<button onclick="submitConvert(\'all\')">全て 変換</button>\n')  # 変換ボタン追加
            f.write(f'<button onclick="submitReDownload(\'all\')">全て 再ダウンロード</button>\n<br>')  # 再ダウンロードボタン追加

            # config['crawler']のキーを使ったボタン生成
            for key in config['crawler']:
                f.write(f'<button onclick="submitUpdate(\'{key}\')">{key} 更新</button>\n')  # サイトごとの更新ボタン追加
                f.write(f'<button onclick="submitConvert(\'{key}\')">{key} 変換</button>\n')  # サイトごとの変換ボタン追加
                f.write(f'<button onclick="submitReDownload(\'{key}\')">{key} 再ダウンロード</button>\n<br>')  # サイトごとの再ダウンロードボタン追加

            f.write('<br><br><br>\n')

            # config['crawler']内のキーを使った動的リンクの生成（オプション）
            for key in config['crawler']:
                f.write(f'<a href="#" onclick="redirectWithParams(\'{key}/\')">{key}</a><br>\n')

            f.write('</body>\n')
            f.write('</html>\n')


if __name__ == '__main__':
    app.run()