import configparser
import os
import server

def initialize():
    site_dic = {}
    # 設定の読み込み
    config = configparser.ConfigParser()
    config.read('setting.ini')

    # Get the path from the data key
    data_path = config['setting']['data']

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
        folder_path = os.path.join(data_path, folder_name)
        if not os.path.exists(folder_path):
            os.makedirs(folder_path)
        
    # Indexファイルを作成
    with open(os.path.join(data_path, 'index.html'), 'w', encoding='utf-8') as f:
        f.write('<!DOCTYPE html>\n')
        f.write('<html lang="ja">\n')
        f.write('<head>\n')
        f.write('<meta charset="UTF-8">\n')
        f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
        f.write('<script>function redirectWithParams(baseURL) {var params = document.location.search; var newURL = baseURL + params; window.location.href = newURL;}</script>\n')
        f.write('<script>function generateRequestId() {return "xxxx-xxxx-4xxx-yxxx-xxxx".replace(/[xy]/g, function(c) {var r = Math.random() * 16 | 0, v = c === "x" ? r : (r & 0x3 | 0x8); return v.toString(16);});} function debounce(func, delay) {var timeoutId; return function() {clearTimeout(timeoutId); timeoutId = setTimeout(func, delay);};} var debouncedSubmit = debounce(submit, 1000); function submit() {var input1 = document.getElementById("input1").value; var requestId = generateRequestId(); var currentUrl = new URL(window.location.href); var keyParam = currentUrl.searchParams.get("key"); var url = "#?add=" + encodeURIComponent(input1); if (keyParam) {url += "&key=" + encodeURIComponent(keyParam);} var xhr = new XMLHttpRequest(); xhr.open("POST", url, true); xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded"); xhr.onreadystatechange = function() {if (xhr.readyState === 4 && xhr.status === 200) {}}; xhr.send("add=" + encodeURIComponent(input1) + "&request_id=" + requestId);}</script>\n')
        f.write('<title>Index</title>\n')
        f.write('</head>\n')
        f.write('<body>\n')
        f.write('<input type="text" id="input1" placeholder="登録URL">\n')
        f.write('<button onclick="debouncedSubmit()">送信</button>\n')
        f.write('<br><br><br>\n')
        for key in config['crawler']:
            f.write(f'<a href="#" onclick="redirectWithParams(\'{key}/\')">{key}</a><br>\n')
        f.write('</body>\n')
        f.write('</html>\n')





    print("Initialize successfully!")
    return int(config['setting']['reload']), int(config['setting']['interval']), site_dic, folder_path, data_path, int(config['server']['key']), int(config['server']['ssl']), int(config['server']['port']), config['ssl']['domain']

if __name__ == '__main__':
    reload_time, interval, site_dic, folder_path, data_path, key, use_ssl, port, domain = initialize()

    server.http_run(interval, site_dic, folder_path, data_path, key, use_ssl, port, domain)