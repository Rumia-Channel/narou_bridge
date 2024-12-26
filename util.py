import os
import json
import shutil
import importlib
import configparser
from datetime import datetime

#ログを保存
import logging

#utilで使うモジュールのインポート
def init_import(site_dic):
    globals().update(import_modules(site_dic))

#ルート用のindexファイルの作成
def create_index(data_path, config, post_path=''):

    # POST先を指定する
    post_url = '/api/' if post_path == 'api' else '#'

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

        # メッセージの表示
        f.write('function showMessage(color, message) {\n')
        f.write('  var existingMessages = document.querySelectorAll(".response-message");\n')
        f.write('  existingMessages.forEach(function(msg) {\n')
        f.write('    msg.remove();\n')
        f.write('  });\n')
        f.write('  var div = document.createElement("div");\n')
        f.write('  div.style.color = color;\n')
        f.write('  div.className = "response-message";\n')
        f.write('  div.textContent = message;\n')
        f.write('  document.body.appendChild(div);\n')
        f.write('}\n')

        # デバウンスされたsubmit関数
        f.write('var debouncedSubmit = debounce(submit, 1000);\n')

        # input1の値をPOSTリクエストで送信する関数
        f.write('function submit() {\n')
        f.write('  var input1 = document.getElementById("input1").value;\n')
        f.write('  var requestId = generateRequestId();\n')
        f.write('  var currentUrl = new URL(window.location.href);\n')
        f.write('  var keyParam = currentUrl.searchParams.get("key");\n')
        f.write(f'  var url = "{post_url}?add=" + encodeURIComponent(input1);\n')
        f.write('  if (keyParam) {\n')
        f.write('    url += "&key=" + encodeURIComponent(keyParam);\n')
        f.write('  }\n')
        f.write('  var xhr = new XMLHttpRequest();\n')
        f.write('  xhr.open("POST", url, true);\n')
        f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
        f.write('  xhr.onreadystatechange = function() {\n')
        f.write('    if (xhr.readyState === 4) {\n')
        f.write('      if (xhr.status === 200) {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var successMsg = response.message || "リクエストは正常に送信されました";\n')
        f.write('        showMessage("green", successMsg);\n')
        f.write('      } else {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var errorMsg = response.message || "エラーが発生しました";\n')
        f.write('        showMessage("red", errorMsg);\n')
        f.write('      }\n')
        f.write('    }\n')
        f.write('  };\n')
        f.write('  xhr.send("add=" + encodeURIComponent(input1) + "&request_id=" + requestId);\n')
        f.write('}\n')

        # submitUpdate関数
        f.write('function submitUpdate(key) {\n')
        f.write('  var requestId = generateRequestId();\n')
        f.write(f'  var url = "{post_url}?update=" + encodeURIComponent(key);\n')
        f.write('  var xhr = new XMLHttpRequest();\n')
        f.write('  xhr.open("POST", url, true);\n')
        f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
        f.write('  xhr.onreadystatechange = function() {\n')
        f.write('    if (xhr.readyState === 4) {\n')
        f.write('      if (xhr.status === 200) {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var successMsg = response.message || "リクエストは正常に送信されました";\n')
        f.write('        showMessage("green", successMsg);\n')
        f.write('      } else {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var errorMsg = response.message || "エラーが発生しました";\n')
        f.write('        showMessage("red", errorMsg);\n')
        f.write('      }\n')
        f.write('    }\n')
        f.write('  };\n')
        f.write('  xhr.send("update=" + encodeURIComponent(key) + "&request_id=" + requestId);\n')
        f.write('}\n')

        # submitConvert関数
        f.write('function submitConvert(key) {\n')
        f.write('  var requestId = generateRequestId();\n')
        f.write(f'  var url = "{post_url}?convert=" + encodeURIComponent(key);\n')
        f.write('  var xhr = new XMLHttpRequest();\n')
        f.write('  xhr.open("POST", url, true);\n')
        f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
        f.write('  xhr.onreadystatechange = function() {\n')
        f.write('    if (xhr.readyState === 4) {\n')
        f.write('      if (xhr.status === 200) {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var successMsg = response.message || "リクエストは正常に送信されました";\n')
        f.write('        showMessage("green", successMsg);\n')
        f.write('      } else {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var errorMsg = response.message || "エラーが発生しました";\n')
        f.write('        showMessage("red", errorMsg);\n')
        f.write('      }\n')
        f.write('    }\n')
        f.write('  };\n')
        f.write('  xhr.send("convert=" + encodeURIComponent(key) + "&request_id=" + requestId);\n')
        f.write('}\n')

        # submitReDownload関数
        f.write('function submitReDownload(key) {\n')
        f.write('  var requestId = generateRequestId();\n')
        f.write(f'  var url = "{post_url}?redownload=" + encodeURIComponent(key);\n')
        f.write('  var xhr = new XMLHttpRequest();\n')
        f.write('  xhr.open("POST", url, true);\n')
        f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
        f.write('  xhr.onreadystatechange = function() {\n')
        f.write('    if (xhr.readyState === 4) {\n')
        f.write('      if (xhr.status === 200) {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var successMsg = response.message || "リクエストは正常に送信されました";\n')
        f.write('        showMessage("green", successMsg);\n')
        f.write('      } else {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var errorMsg = response.message || "エラーが発生しました";\n')
        f.write('        showMessage("red", errorMsg);\n')
        f.write('      }\n')
        f.write('    }\n')
        f.write('  };\n')
        f.write('  xhr.send("redownload=" + encodeURIComponent(key) + "&request_id=" + requestId);\n')
        f.write('}\n')

        f.write('</script>\n')
        f.write('<title>Index</title>\n')
        f.write('</head>\n')
        f.write('<body>\n')

        # 入力フィールドと送信ボタン
        f.write('<input type="text" id="input1" placeholder="登録URL">\n')
        f.write('<button onclick="debouncedSubmit()">送信</button><br><br><br>\n')

        f.write(f'<button onclick="submitUpdate(\'all\')">全て 更新</button>\n')
        f.write(f'<button onclick="submitConvert(\'all\')">全て 変換</button>\n')
        f.write(f'<button onclick="submitReDownload(\'all\')">全て 再ダウンロード</button>\n<br>')

        # config['crawler']のキーを使ったボタン生成
        for key in config['crawler']:
            f.write(f'<button onclick="submitUpdate(\'{key}\')">{key} 更新</button>\n')
            f.write(f'<button onclick="submitConvert(\'{key}\')">{key} 変換</button>\n')
            f.write(f'<button onclick="submitReDownload(\'{key}\')">{key} 再ダウンロード</button>\n<br>')
        
        f.write('<br><br><br>\n')

        # config['crawler']内のキーを使った動的リンクの生成（オプション）
        for key in config['crawler']:
            f.write(f'<a href="#" onclick="redirectWithParams(\'{key}/\')">{key}</a><br>\n')

        f.write('</body>\n')
        f.write('</html>\n')

#初期設定の読み込み
def load_config():
    site_dic = {}
    login_dic = {}

    folder_path = {}
    cookie_path = {}

    if not os.path.isdir('setting'):
        os.makedirs('setting')
        # config.ini を config フォルダ内にコピー
        shutil.copy('setting.ini', 'setting/setting.ini')

    # 設定の読み込み
    config = configparser.ConfigParser()
    config.read('setting/setting.ini')

    # Get the path from the data key
    data_path = config['setting']['data']

    # 指定されないならカレントディレクトリ
    if not data_path:
        data_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'data')

    #Cookieの保存先を指定
    cookie_folder = config['setting']['cookie']

    # 指定されないならカレントディレクトリ
    if not cookie_folder:
        cookie_folder = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'cookie')

    log_path = config['setting']['log']

    # 指定されないならカレントディレクトリ
    if not log_path:
        log_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'log')

    # ないなら作れdataフォルダ
    if not os.path.exists(data_path):
        os.makedirs(data_path)

    # dataフォルダcookieフォルダとサイト名のマトリョシカを作成
    for key in config['crawler']:
        folder_name = key
        site_dic[key] = config['crawler'][key]
        login_dic[key] = int(config['login'][key])
        folder_path[key] = os.path.join(data_path, folder_name)
        cookie_path[key] = os.path.join(cookie_folder, folder_name)
        if not os.path.exists(folder_path[key]):
            os.makedirs(folder_path[key])
        if not os.path.exists(cookie_path[key]):
            os.makedirs(cookie_path[key])

    # ログフォルダがないなら作成
    if not os.path.exists(log_path):
        os.makedirs(log_path)

    print("Initialize successfully!")
    return config, int(config['setting']['reload']), int(config['setting']['auto_update']), int(config['setting']['save_log']), int(config['setting']['interval']), int(config['setting']['auto_update_interval']), site_dic, login_dic, folder_path, data_path, cookie_path, log_path, int(config['server']['key']), int(config['server']['ssl']), str(config['server']['ssl_crt']), str(config['server']['ssl_key']), int(config['server']['port']), config['server']['domain'], int(config['server']['use_proxy']), int(config['server']['proxy_port']), int(config['server']['proxy_ssl'])

# clawler　フォルダ内のモジュールをインポート
def import_modules(site_dic):
    modules = {}
    for site_key, value in site_dic.items():
        module_name = 'crawler.' + value.replace('.py', '')
        modules[site_key] = importlib.import_module(module_name)
    return modules

#小説の更新処理
def update(update_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):

    if not update_param == 'all':
        # 更新処理
        for site_key, value in site_dic.items():
            if update_param == site_key:
                site = site_key
                if int(login_dic[site])==0|1:
                    is_login = int(login_dic[site])
                else:
                    return 400
                
                break
        else:
            return 400
        
        logging.info(f'Update: {site}')
        globals()[site].init(cookie_path[site], is_login, interval)
        globals()[site].update(folder_path[site], key_data, data_path, host_name)

    else:
        # 全更新処理
        for site_key, value in site_dic.items():
            site = site_key
            if int(login_dic[site])==0|1:
                is_login = int(login_dic[site])
            else:
                return 400
            
            logging.info(f'Update: {site}')
            globals()[site].init(cookie_path[site], is_login, interval)
            globals()[site].update(folder_path[site], key_data, data_path, host_name)

#小説の再ダウンロード処理
def re_download(re_download_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):

    if not re_download_param == 'all':
        # 更新処理
        for site_key, value in site_dic.items():
            if value.replace('_', '.').replace('.py', '') in re_download_param:
                if re_download_param == site_key:
                    site = site_key
                    if int(login_dic[site])==0|1:
                        is_login = int(login_dic[site])
                    else:
                        return 400
                    
                    break
            else:
                return 400
        else:
            return 400

        logging.info(f'Re Download: {site}')
        globals()[site].init(cookie_path[site], is_login, interval)
        globals()[site].re_download(folder_path[site], key_data, data_path, host_name)

    else:
        # 全更新処理
        for site_key, value in site_dic.items():
            site = site_key
            if int(login_dic[site])==0|1:
                is_login = int(login_dic[site])
            else:
                return 400
            
            logging.info(f'Re Download: {site}')
            globals()[site].init(cookie_path[site], is_login, interval)
            globals()[site].re_download(folder_path[site], key_data, data_path, host_name)

#小説の変換処理
def convert(convert_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    
    if not convert_param == 'all':
        # 変換処理
        for site_key, value in site_dic.items():
            if convert_param == site_key:
                site = site_key
                if int(login_dic[site])==0|1:
                    is_login = int(login_dic[site])
                else:
                    return 400

                break
        else:
            return 400

        logging.info(f'Convert: {site}')
        globals()[site].init(cookie_path[site], is_login, interval)
        globals()[site].convert(folder_path[site], key_data, data_path, host_name)

    else:
        # 全変換処理
        for site_key, value in site_dic.items():
            site = site_key
            if int(login_dic[site])==0|1:
                is_login = int(login_dic[site])
            else:
                return 400

            logging.info(f'Convert: {site}')
            globals()[site].init(cookie_path[site], is_login, interval)
            globals()[site].convert(folder_path[site], key_data, data_path, host_name)

#小説のダウンロード処理
def download(add_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    # webサイトの判別
    for site_key, value in site_dic.items():
        if value.replace('_', '.').replace('.py', '') in add_param:
            site = site_key
            if int(login_dic[site])==0|1:
                is_login = int(login_dic[site])
            else:
                return 400
            break
    else:
        if int(login_dic[site])==0|1:
            is_login = int(login_dic[site])
        else:
            return 400

    logging.info(f'Web site: {site}')
    logging.info(f'URL: {add_param}')
    globals()[site].init(cookie_path[site], is_login, interval)
    globals()[site].download(add_param, folder_path[site], key_data, data_path, host_name)

#リクエストIDの削除
def cleanup_expired_requests(requests_dict, expiration_time):
    """
    指定した有効期限を超えたリクエストIDを削除し、リクエストID以外が同じ内容の重複リクエストを削除する。
    """
    current_time = datetime.now()
    processed_signatures = set()  # 処理済みのリクエスト内容を記録する集合

    for key in list(requests_dict):
        request_time = requests_dict[key]["time"]
        request_data = requests_dict[key]["data"]

        # リクエストの有効期限を確認
        if (current_time - request_time).total_seconds() > expiration_time:
            del requests_dict[key]
            continue

        # リクエストの内容を識別するためのシグネチャを生成（リクエストIDを除く）
        signature = json.dumps({k: v for k, v in request_data.items() if k != "request_id"}, sort_keys=True)

        # シグネチャが既に存在する場合、このリクエストを削除
        if signature in processed_signatures:
            del requests_dict[key]
            queue_stop = True
        else:
            # 新しいシグネチャを記録
            processed_signatures.add(signature)
            queue_stop = False

    return requests_dict, queue_stop