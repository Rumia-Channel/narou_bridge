import os
import shutil
import json
from jsondiff import diff
import requests
from requests.exceptions import RequestException, ConnectionError, Timeout
from urllib.parse import unquote
import time
from datetime import datetime, timezone, timedelta
import hashlib
import base64

#ログを保存
import logging

# 半角文字を全角文字に変換する関数
def full_to_half(text):
    # 全角文字を半角文字に変換する変換テーブル
    fullwidth = '０１２３４５６７８９ＡＢＣＤＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺａｂｃｄｅｆｇｈｉｊｋｌｍｎｏｐｑｒｓｔｕｖｗｘｙｚ！”＃＄％＆’（）＊＋，－．／：；＜＝＞？＠［＼］＾＿｀｛｜｝～'
    halfwidth = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~'
    
    # 全角から半角への変換テーブルを作成
    translate_table = str.maketrans(fullwidth, halfwidth)
    
    # 変換を実行
    return text.translate(translate_table)

# Cookie とユーザーエージェントを返す
def load_cookies_and_ua(input_file):
    
    with open(input_file, 'r', encoding='utf-8') as f:
        data = json.load(f)  # 1 回だけファイルを読み込む
        cookies = data.get('cookies', {})
        ua = data.get('user_agent')

    # requests用にCookieを変換
    cookies_dict = {cookie['name']: cookie['value'] for cookie in cookies}
    return cookies_dict, ua

# Cookie とユーザーエージェントを保存する
def save_cookies_and_ua(output_file, cookies, ua):
    with open(output_file, 'w', encoding='utf-8') as f:
            json.dump({'cookies': cookies, 'user_agent': ua}, f, ensure_ascii=False, indent=4)

#画像ファイルのチェック
def check_image_file(img_path, file_name):
    if not os.path.exists(os.path.join(img_path, 'database.json')):
        with open(os.path.join(img_path, 'database.json'), 'w', encoding='utf-8') as f:
            json.dump({}, f, ensure_ascii=False, indent=4)

    with open(os.path.join(img_path, 'database.json'), 'r', encoding='utf-8') as f:
        database = json.load(f)

    for key, value in database.items():
        if key == file_name or key.split('.')[0] == file_name.split('.')[0]:
            if os.path.exists(os.path.join(img_path, value+f'{os.path.splitext(file_name)[1]}')):
                return value+f'{os.path.splitext(key)[1]}'

    return None

# 日本標準時(JST)を使いたいときに便利
JST = timezone(timedelta(hours=9))

def safe_fromiso(date_str, tzinfo=JST):
    """
    ISO フォーマット文字列を安全に datetime に変換します。
    - date_str が None や空文字列なら None を返します。
    - パースに失敗した場合は警告ログを出して None を返します。
    - tzinfo を指定すると astimezone(tzinfo) を行います。
    """
    if not date_str:
        logging.warning(f"safe_fromiso: empty or None date_str received")
        return None
    try:
        dt = datetime.fromisoformat(date_str)
        if tzinfo is not None:
            return dt.astimezone(tzinfo)
        return dt
    except ValueError:
        logging.warning(f"safe_fromiso: invalid ISO format: {date_str}")
        return None

#画像ファイルのハッシュをチェック
def check_image_hash(img_path, file_data, file_name):
    if not os.path.exists(os.path.join(img_path, 'database.json')):
        with open(os.path.join(img_path, 'database.json'), 'w', encoding='utf-8') as f:
            json.dump({}, f, ensure_ascii=False, indent=4)

    with open(os.path.join(img_path, 'database.json'), 'r', encoding='utf-8') as f:
        database = json.load(f)

    image_hash = base64.urlsafe_b64encode(hashlib.sha3_256(file_data).digest()).rstrip(b'=').decode('utf-8')
    for key, value in database.items():
        if value == image_hash:
            database[file_name] = str(image_hash)

            with open(os.path.join(img_path, 'database.json'), 'w', encoding='utf-8') as f:
                json.dump(database, f, ensure_ascii=False, indent=4)

            return value
    
    with open(os.path.join(img_path, 'database.json'), 'w', encoding='utf-8') as f:
        database[file_name] = str(image_hash)
        json.dump(database, f, ensure_ascii=False, indent=4)

    return image_hash

# 再帰的にキーを探す
def find_key_recursively(data, target_key):
    
    #辞書型の時
    if isinstance(data, dict):
        for key, value in data.items():
            if key == target_key:
                return value
            elif isinstance(value, (dict, list)):
                result = find_key_recursively(value, target_key)
                if result is not None:
                    return result
    
    #リスト型の時
    elif isinstance(data, list):
        for item in data:
            result = find_key_recursively(item, target_key)
            if result is not None:
                return result
    return None

# クッキーを使ってGETリクエストを送信
def get_with_cookie(url, cookie, header, retries=5, delay=5):
    response = None  # responseを初期化
    for i in range(retries):
        try:
            response = requests.get(url, cookies=cookie, headers=header, timeout=10)
            response.raise_for_status()  # HTTPエラーをキャッチ
            return response
        except (ConnectionError, Timeout) as e:
            logging.error(f"\nError: {e}. Retrying in {delay * (2 ** i)} seconds...")
        except RequestException as e:
            # 404エラーを特別扱い
            if response is not None and response.status_code == 404:
                logging.error("\n404 Error: Resource not found.")
                return response
            else:
                logging.error(f"\nError: {e}. Retrying in {delay * (2 ** i)} seconds...")
        
        if i < retries - 1:
            time.sleep(delay * (2 ** i))  # 指数バックオフ
        else:
            logging.error("\nThe retry limit has been reached. No response received.")
            return response
        
# キーをすべて文字列に変換する関数
def convert_keys_to_str(d):
    if isinstance(d, dict):
        return {str(k): convert_keys_to_str(v) for k, v in d.items()}
    elif isinstance(d, list):
        return [convert_keys_to_str(i) for i in d]
    else:
        return d

#小説データに差分があるなら保存   
def save_raw_diff(raw_path, novel_path, novel):
    if os.path.exists(raw_path):
        with open(raw_path, 'r', encoding='utf-8') as f:
            old_json = json.load(f)
        old_json = json.loads(json.dumps(old_json))
        new_json = json.loads(json.dumps(novel))
        diff_json = convert_keys_to_str(diff(new_json,old_json))
        if len(diff_json) == 1 and 'get_date' in diff_json:
            pass
        else:
            with open(os.path.join(novel_path, 'raw', f'diff_{str(old_json["get_date"]).replace(":", "-").replace(" ", "_")}.json'), 'w', encoding='utf-8') as f:
                json.dump(diff_json, f, ensure_ascii=False, indent=4)

#ベースフォルダ作成
def make_dir(id, folder_path):

    full_path = os.path.join(folder_path, f'{id}')
    
    if not os.path.exists(full_path):
        os.makedirs(full_path)
    if not os.path.exists(f'{full_path}/raw'):
        os.makedirs(f'{full_path}/raw')
    if not os.path.exists(f'{full_path}/info'):
        os.makedirs(f'{full_path}/info')

def gen_site_index(folder_path ,key_data, site_name):
    subfolders = [f for f in os.listdir(folder_path) if os.path.isdir(os.path.join(folder_path, f))]
    pairs = {}
    no_raw = []
    # 各サブフォルダの raw/raw.json を読み込む
    for folder in subfolders:
        json_path = os.path.join(folder_path, folder, 'raw', 'raw.json')
        if os.path.exists(json_path):
            with open(json_path, 'r', encoding='utf-8') as f:
                data = json.load(f)
                title = data.get('title', 'No title found')
                author = data.get('author', 'No author found')
                author_id = data.get('author_id', 'No author_id found')
                author_url = data.get('author_url', 'No author_url found')
                tags = data.get('tags', 'No tags found')
                all_tags = data.get('all_tags', 'No all_tags found')
                caption = unquote(data.get('caption', 'No caption found'))
                create_date = data.get('createDate', 'No create date found')
                update_date = data.get('updateDate', 'No update date found')
                type = data.get('type', 'No type found')
                serialization = data.get('serialization', 'No serialization found')
                episodes_data = {}
                for key, value in data.get('episodes').items():
                    episodes_data[key] = {}
                    episodes_data[key]['title'] = value.get('title', 'No title found')
                    episodes_data[key]['id'] = value.get('id', 'No id found')
                    episodes_data[key]['caption'] = unquote(value.get('caption', 'No caption found'))
                    episodes_data[key]['tags'] = value.get('tags', 'No tags found')


                pairs[folder] = {'title': title, 'author': author, 'author_id': author_id, 'author_url' : author_url, 'type': type, 'serialization': serialization, 'tags': tags, 'all_tags': all_tags, 'caption': caption, 'episodes_data': episodes_data, 'create_date': create_date, 'update_date': update_date}
        else:
            #print(f"raw.json not found in {folder}")
            #return
            shutil.rmtree(os.path.join(folder_path, folder))
            no_raw.append(folder)
            continue
    
    pairs = dict(sorted(pairs.items(), key=lambda item: item[1]['author']))

    # index.html の生成
    with open(os.path.join(folder_path, 'index.html'), 'w', encoding='utf-8') as f:
        f.write("""
                    <!DOCTYPE html>
                    <html lang="ja">
                    <head>
                        <meta charset="UTF-8">
                        <meta name="viewport" content="width=device-width, initial-scale=1.0">
                        <title>""")
        f.write(f"{site_name} Index</title>\n")
        f.write("""
                        <!-- 画面固有スタイル -->
                        <link rel="stylesheet" href="/css/index.css">
                        <!-- favicon -->
                        <link rel="icon" href="/icon/favicon.ico">
                        <!-- PWA対応 -->
                        <link rel="manifest" href="/manifest.json">
                        <link rel="apple-touch-icon" sizes="180x180" href="/icon/icon_180x180.png">
                        <link rel="apple-touch-icon" sizes="167x167" href="/icon/icon_167x167.png">
                        <link rel="apple-touch-icon" sizes="152x152" href="/icon/icon_152x152.png">
                    </head>
                    <body>
                        <div><a href="../">戻る</a></div><br><br>""")

        f.write(f"    <div style='display: flex; justify-content: space-between;'><h1>{site_name} Index</h1><button id='reset-localstorage-button'>ローカルストレージリセット</button></div>")
        f.write("""
                        <!-- テーブルの表示タイプを選ぶドロップダウン -->
                        <div class="type-selector">
                            <label for="typeSelect">表示タイプ: </label>
                            <select id="typeSelect" onchange="filterByType()">
                                <option value="all">すべて</option>
                                <option value="novel">小説のみ</option>
                                <option value="comic">漫画のみ</option>
                            </select>
                        </div>

                        <!-- 行数変更のドロップダウン -->
                        <div class="row-selector">
                            <label for="rowsPerPageSelect">1ページあたりの行数: </label>
                            <select id="rowsPerPageSelect" onchange="updateRowsPerPage()">
                                <option value="10">10</option>
                                <option value="20">20</option>
                                <option value="30">30</option>
                                <option value="50">50</option>
                                <option value="100">100</option>
                                <option value="150">150</option>
                                <option value="200">200</option>
                                <option value="0">すべて</option>
                            </select>
                        </div>
                
                            <!-- 既存のフィルターUI付近の変更箇所 -->
                            <div style="display: flex; justify-content: space-between;">
                                <div class="author-filter">
                                    <label for="author-filter-dropdown">作者絞り込み:</label>
                                    <select id="author-filter-dropdown" onchange="updateAuthorFilter(this.value)">
                                        <option value="">全て表示</option>
                                        <!-- ここに JS で作者一覧のオプションが追加される -->
                                    </select>
                                </div>
                                <button id="reset-author-filter-button">作者絞り込みリセット</button>
                            </div>
                        <div style="display: flex; justify-content: space-between;">
                            <!-- 既存のフィルターUI付近（例えば、作者絞り込みの下） -->
                            <div class="hidden-author-filter">
                                <span id="hidden-author-container"></span>
                            </div>
                            <button id="reset-hidden-authors-button">非表示作者リセット</button>
                        </div>
                        <br>
                        <!-- 選択された行数表示と、選択項目のリンクをコピーするボタン -->
                        <div class="selection-controls" style="margin-top: 1em;">
                            <span id="selected-count">選択された件数: 0</span>
                            <button id="copy-selected-button">選択項目のリンクをコピー</button>
                        </div>
                        <br>
                        <!-- カラム選択のチェックボックス -->
                        <div>
                            <label>表示項目の選択</label>
                            <div class="column-selector">
                                <label><input type="checkbox" id="show-serialization" checked onclick="toggleColumn('serialization')"> 連載状況</label>
                                <label><input type="checkbox" id="show-title" checked onclick="toggleColumn('title')"> タイトル</label>
                                <label><input type="checkbox" id="show-author" checked onclick="toggleColumn('author')"> 作者名</label>
                                <label><input type="checkbox" id="show-type" checked onclick="toggleColumn('type')"> 形式</label>
                                <label><input type="checkbox" id="show-tags" checked onclick="toggleColumn('tags')"> タグ</label>
                                <label><input type="checkbox" id="show-create_date" checked onclick="toggleColumn('create_date')"> 掲載日時</label>
                                <label><input type="checkbox" id="show-update_date" checked onclick="toggleColumn('update_date')"> 更新日時</label>
                            </div>
                        </div>
                        <br>
                
                        <div class="tag-filter">
                            <div style="display: flex; justify-content: space-between;">
                                <div>
                                    <div id="include-tags"></div>
                                </div>
                                <button id="reset-include-tags-button">含むタグリセット</button>
                            </div>
                            <div style="display: flex; justify-content: space-between;">
                                <div>
                                    <div id="exclude-tags"></div>
                                </div>
                                <button id="reset-exclude-tags-button">含まないタグリセット</button>
                            </div>
                        </div>
                
                        <br>
                        <br>
                
                        <div id="loading-overlay">
                            <div class="spinner"></div>
                        </div>

                        <table>
                            <thead id="table-head">
                                <tr>
                                    <!-- JavaScript でカラムを動的に追加 -->
                                </tr>
                            </thead>
                            <tbody id="user-table-body">
                                <!-- JavaScript でデータを挿入 -->
                            </tbody>
                        </table>

                        <!-- ページネーション用のボタン -->
                        <div class="pagination">
                            <button onclick="prevPage()">前へ</button>
                            <span id="page-info">1 / 1</span>
                            <button onclick="nextPage()">次へ</button>
                        </div>

                        <script src="/script/index.js"></script>
                        <!-- 共通スタイル -->
                        <link rel="stylesheet" href="/css/common.css">
                    </body>
                    </html>
                    """)

    
    with open(os.path.join(folder_path, 'index.json'), 'w', encoding='utf-8') as f:
        json.dump(pairs, f, ensure_ascii=False, indent=4)

    if no_raw:
        logging.warning(f"The folders {', '.join(no_raw)} were deleted because they do not contain 'raw.json'.")

    logging.info('目次の生成が完了しました')