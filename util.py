import os
import importlib
import configparser
from datetime import datetime

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
        f.write('        var successMsg = response.message || "成功しました";\n')
        f.write('        document.body.innerHTML += "<div style=\'color:green;\'>" + successMsg + "</div>";\n')
        f.write('      } else {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var errorMsg = response.message || "エラーが発生しました";\n')
        f.write('        document.body.innerHTML += "<div style=\'color:red;\'>" + errorMsg + "</div>";\n')
        f.write('      }\n')
        f.write('    }\n')
        f.write('  };\n')
        f.write('  xhr.send("add=" + encodeURIComponent(input1) + "&request_id=" + requestId);\n')
        f.write('}\n')

        # updateパラメーターを持ったリクエストを送信する関数
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
        f.write('        var successMsg = response.message || "成功しました";\n')
        f.write('        document.body.innerHTML += "<div style=\'color:green;\'>" + successMsg + "</div>";\n')
        f.write('      } else {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var errorMsg = response.message || "エラーが発生しました";\n')
        f.write('        document.body.innerHTML += "<div style=\'color:red;\'>" + errorMsg + "</div>";\n')
        f.write('      }\n')
        f.write('    }\n')
        f.write('  };\n')
        f.write('  xhr.send("update=" + encodeURIComponent(key) + "&request_id=" + requestId);\n')
        f.write('}\n')

        # convertパラメーターを持ったリクエストを送信する関数
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
        f.write('        var successMsg = response.message || "成功しました";\n')
        f.write('        document.body.innerHTML += "<div style=\'color:green;\'>" + successMsg + "</div>";\n')
        f.write('      } else {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var errorMsg = response.message || "エラーが発生しました";\n')
        f.write('        document.body.innerHTML += "<div style=\'color:red;\'>" + errorMsg + "</div>";\n')
        f.write('      }\n')
        f.write('    }\n')
        f.write('  };\n')
        f.write('  xhr.send("convert=" + encodeURIComponent(key) + "&request_id=" + requestId);\n')
        f.write('}\n')

        # re_downloadパラメーターを持ったリクエストを送信する関数
        f.write('function submitReDownload(key) {\n')
        f.write('  var requestId = generateRequestId();\n')
        f.write(f'  var url = "{post_url}?re_download=" + encodeURIComponent(key);\n')
        f.write('  var xhr = new XMLHttpRequest();\n')
        f.write('  xhr.open("POST", url, true);\n')
        f.write('  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");\n')
        f.write('  xhr.onreadystatechange = function() {\n')
        f.write('    if (xhr.readyState === 4) {\n')
        f.write('      if (xhr.status === 200) {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var successMsg = response.message || "成功しました";\n')
        f.write('        document.body.innerHTML += "<div style=\'color:green;\'>" + successMsg + "</div>";\n')
        f.write('      } else {\n')
        f.write('        var response = JSON.parse(xhr.responseText);\n')
        f.write('        var errorMsg = response.message || "エラーが発生しました";\n')
        f.write('        document.body.innerHTML += "<div style=\'color:red;\'>" + errorMsg + "</div>";\n')
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

#初期設定の読み込み
def load_config():
    site_dic = {}
    login_dic = {}

    folder_path = {}
    cookie_path = {}

    # 設定の読み込み
    config = configparser.ConfigParser()
    config.read('setting.ini')

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

    print("Initialize successfully!")
    return config, int(config['setting']['reload']), int(config['setting']['interval']), site_dic, login_dic, folder_path, data_path, cookie_path, int(config['server']['key']), int(config['server']['ssl']), int(config['server']['port']), config['server']['domain']

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
        
        print(f'Update: {site}\n')
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
            
            print(f'Update: {site}\n')
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

        print(f'Re Download: {site}\n')
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
            
            print(f'Re Download: {site}\n')
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

        print(f'Convert: {site}\n')
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

            print(f'Convert: {site}\n')
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

    print(f'Web site: {site}')
    print(f'URL: {add_param}')
    globals()[site].init(cookie_path[site], is_login, interval)
    globals()[site].download(add_param, folder_path[site], key_data, data_path, host_name)

#リクエストIDの削除
def cleanup_expired_requests(requests_dict, expiration_time):
    current_time = datetime.now()
    for key in list(requests_dict):
        request_time = requests_dict[key]["time"]
        if (current_time - request_time).total_seconds() > expiration_time:
            del requests_dict[key]
    
    return requests_dict