from flask import Flask, request, jsonify, abort
from datetime import datetime
import threading
import util

app = Flask(__name__)

# リクエストを格納するリスト
request_queue = []

# スレッドセーフにするためのロック
lock = threading.Lock()

# recent_request_ids をスレッドセーフに管理
recent_request_ids = {}

# 設定のロード
config, reload_time, interval, site_dic, login_dic, folder_path, data_path, cookie_path, key, use_ssl, port, domain = util.load_config()

util.init_import(site_dic)

util.create_index(data_path, config, 'api')

def process_request(req_data):
    """リクエストデータを順番に処理する関数"""
    # ここでリクエストの処理を行う
    add_param = req_data.get("add")
    update_param = req_data.get("update")
    convert_param = req_data.get("convert")
    re_download_param = req_data.get("re_download")
    request_id = req_data.get("request_id")
    key_data = key

    #ホスト名の確定
    if use_ssl == 1:
        host_name = f'https://{domain}:{port}'
    else:
        host_name = f'http://{domain}:{port}'

    # 実際の処理を行う (例: データを処理するなど)
    print(f'Processing request_id: {request_id}, add: {add_param}, update: {update_param}')

    # 更新処理
    if not update_param is None:

        update_return = util.update(update_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

        if update_return == 400:
            abort(400, description="Invalid update_param value")
            print("\nInvalid update_param value\n")
            return
        else:
            abort(200, description="Update Complete")
            print("\nUpdate Complete\n")
            return

    # 再ダウンロード処理
    elif not re_download_param is None:
        
        re_download_return = util.re_download(re_download_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

        if re_download_return == 400:
            abort(400, description="Invalid re_download_param value")
            print("\nInvalid re_download_param value\n")
            return
        else:
            abort(200, description="Re Download Complete")
            print("\nRe Download Complete\n")
            return

    # 変換処理
    elif not convert_param is None:
        
        convert_return = util.convert(convert_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

        if convert_return == 400:
            abort(400, description="Invalid convert_param value")
            print("\nInvalid convert_param value\n")
            return
        else:
            abort(200, description="Convert Complete")
            print("\nConvert Complete\n")
            return

    # ダウンロード処理
    elif not add_param is None:
        add_return = util.download(add_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name)

        if add_return == 400:
            abort(400, description="Invalid add_param value")
            print("\nInvalid add_param value\n")
            return
        else:
            abort(200, description="Download Complete")
            print("\nDownload Complete\n")
            return
        



@app.route('/api/', methods=['POST'])
def do_post():
    """POST リクエストを受け取って処理キューに追加する"""
    # 現在時刻を表示
    print(f'POST received: {datetime.now().astimezone().strftime("%Y-%m-%d %H:%M:%S%z")}')

    # POST データ（フォームデータ）を受け取る
    add_param = request.form.get("add", None)
    update_param = request.form.get("update", None)
    convert_param = request.form.get("convert", None)
    re_download_param = request.form.get("re_download", None)
    request_id = request.form.get("request_id", None)

    # request_id が None の場合、エラーレスポンスを返す
    if request_id is None:
        abort(400, description="Invalid request_id value")

    # request_id が重複している場合、エラーレスポンスを返す
    with lock:
        # 古いリクエストIDをクリーンアップ
        recent_request_ids = util.cleanup_expired_requests(recent_request_ids, expiration_time=600)

        if request_id in recent_request_ids:
            abort(429, description="Duplicate request detected")

        # リクエストIDを記録
        recent_request_ids[request_id] = datetime.now()

    # リクエストデータをキューに追加
    req_data = {
        "add": add_param,
        "update": update_param,
        "convert": convert_param,
        "re_download": re_download_param,
        "request_id": request_id
    }

    # キューにリクエストを追加し、別スレッドで処理を行う
    with lock:
        request_queue.append(req_data)

    # 新しいリクエストを別スレッドで処理
    threading.Thread(target=process_request, args=(req_data,)).start()

    # 処理中のステータスを返す（必要に応じて変更）
    return jsonify({"status": "queued", "request_id": request_id})


if __name__ == "__main__":
    # Flask アプリケーションをマルチスレッドモードで実行
    app.run(debug=True, threaded=True)