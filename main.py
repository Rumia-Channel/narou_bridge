import os
import server

#共通設定の読み込み
import common as cm

def create_index(data_path, config):
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
        f.write('  var url = "#?add=" + encodeURIComponent(input1);\n')
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
        f.write('  var url = "#?update=" + encodeURIComponent(key);\n')
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
        f.write('  var url = "#?convert=" + encodeURIComponent(key);\n')
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
        f.write('  var url = "#?re_download=" + encodeURIComponent(key);\n')
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
    config, reload_time, interval, site_dic, login_dic, folder_path, data_path, cookie_path, key, use_ssl, port, domain = cm.load_config()

    # Indexファイルを作成
    create_index(data_path, config)

    server.http_run(interval, site_dic, login_dic, folder_path, data_path, cookie_path,  key, use_ssl, port, domain)