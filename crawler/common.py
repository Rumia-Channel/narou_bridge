import os
import shutil
import json
from jsondiff import diff
import requests
from requests.exceptions import RequestException, ConnectionError, Timeout
from urllib.parse import unquote
import time
from datetime import datetime
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
                        <style>

                            table {
                                table-layout: fixed;
                                width: 100%;
                            }

                            .hidden-column {
                                display: none;
                            }

                            th, td {
                                border: 1px solid #ddd;
                                padding: 8px;
                                text-align: left;
                                cursor: default; /* ソートできる列を示唆 */
                            }

                            th {
                                background-color: #f4f4f4;
                                cursor: pointer; /* ソートできる列を示唆 */
                            }

                            /* ページネーション */
                            .pagination {
                                margin-top: 10px;
                                text-align: center;
                            }
                            .pagination button {
                                padding: 5px 10px;
                                margin: 5px;
                                cursor: pointer;
                            }
                            .row-selector, .column-selector, .type-selector {
                                margin-bottom: 10px;
                            }
                
                            .tag-item {
                                display: inline-block;
                                padding: 0.2em 0.5em;
                                border: 1px solid #ccc;
                                border-radius: 3px;
                                margin-right: 0.3em;
                                margin-bottom: 0.3em;
                                background-color: #f8f8f8;
                            }
                
                            .hidden-author-tag {
                                display: inline-block;
                                padding: 0.2em 0.5em;
                                border: 1px solid #ccc;
                                border-radius: 3px;
                                margin-right: 0.3em;
                                margin-bottom: 0.3em;
                                background-color: #ffecec;
                                cursor: pointer;
                            }
                
                            .row-checkbox {
                                width: calc(100% - 1ch);  /* セルの横幅から 1ch を引いたサイズ */
                                margin: 0.5ch;            /* 各方向に 0.5ch の隙間 */
                                aspect-ratio: 1 / 1;      /* 正方形を維持 */
                                box-sizing: border-box;
                            }
                
                            /* 追加: スピナーのスタイル */
                            .spinner {
                                border: 8px solid #f3f3f3;
                                border-top: 8px solid #3498db;
                                border-radius: 50%;
                                width: 60px;
                                height: 60px;
                                animation: spin 1s linear infinite;
                                margin: auto;
                            }
                            @keyframes spin {
                                0% { transform: rotate(0deg); }
                                100% { transform: rotate(360deg); }
                            }

                            /* 追加: ローディング用オーバーレイ */
                            #loading-overlay {
                                position: fixed;
                                top: 0;
                                left: 0;
                                width: 100%;
                                height: 100%;
                                background: rgba(255, 255, 255, 0.8);
                                display: flex;
                                align-items: center;
                                justify-content: center;
                                z-index: 1000;
                            }

                        </style>
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

                        <script>
                            // 列の並び順は固定
                            let columns = [
                                'serialization',
                                'title',
                                'author',
                                'type',
                                'tags',
                                'create_date',
                                'update_date'
                            ];

                            let tableData = {};  // テーブルのデータ
                            let currentPage = 1; // 現在のページ
                            let rowsPerPage = 10; // 初期行数（デフォルトは10）
                            let hiddenCols = [];  // 幅を0pxにするカラムを管理

                            let filteredAuthors = []; // 絞り込む作者名
                            let hiddenAuthors = [];   // 非表示にする作者名
                            let typeFilter = "all";   // 形式フィルター

                            let includedTags = [];   // 含むタグ
                            let excludedTags = [];   // 除外するタグ
                            let includeOperator = "AND";  // 含むフィルターの演算子（AND/OR）
                            let excludeOperator = "AND";  // 非表示フィルターの演算子（AND/OR）

                            let selectedRows = new Set(); // 各行のキー（ID）を保持する

                            // 固定幅を持つ列（ch 単位の数値）
                            const fixedWidthMapping = {
                                'serialization': 6,
                                'type': 3,
                                'create_date': 14,
                                'update_date': 14
                            };
                            // 可変幅の列の重み
                            const variableWeightMapping = {
                                'title': 50,
                                'author': 20,
                                'tags': 30
                            };

                            // ソート情報
                            let sortInfo = {
                                column: null,
                                ascending: true
                            };

                            // index.jsonからデータを読み込み
                            async function fetchData() {
                                const loadingOverlay = document.getElementById("loading-overlay");
                                loadingOverlay.style.display = "flex"; // ローディング開始時に表示
                                try {
                                    const response = await fetch('index.json');  // 同一階層にある index.json を読み込み
                                    const data = await response.json();
                                    tableData = data;  // JSONデータを tableData にセット
                                    loadSettings();  // 設定をローカルストレージから読み込み
                                    renderTagFilters();
                                    updateAuthorDropdownOptions();
                                    updateAuthorDropdownValue();
                                    renderHiddenAuthors();
                                    renderTableHeaders();
                                    renderTable();
                                    updatePagination();
                                } catch (error) {
                                    console.error('JSONデータの読み込みに失敗しました: ', error);
                                } finally {
                                    loadingOverlay.style.display = "none"; // 終了時に非表示
                                }
                            }

                            // 初期設定をローカルストレージから取得
                            function loadSettings() {
                                const savedSettings = JSON.parse(localStorage.getItem('tableSettings')) || {};
                                rowsPerPage = (typeof savedSettings.rowsPerPage === 'number')
                                ? savedSettings.rowsPerPage
                                : 10;
                                hiddenCols = savedSettings.hiddenCols || [];
                                currentPage = savedSettings.currentPage || 1;
                                typeFilter = savedSettings.typeFilter || "all";
                                filteredAuthors = savedSettings.filteredAuthors || [];
                                hiddenAuthors = savedSettings.hiddenAuthors || [];
                                sortInfo = savedSettings.sortInfo || { column: null, ascending: true };
                
                                // 追加した部分
                                includedTags = savedSettings.includedTags || [];
                                excludedTags = savedSettings.excludedTags || [];
                                includeOperator = savedSettings.includeOperator || "AND";
                                excludeOperator = savedSettings.excludeOperator || "AND";
                                // 追加部分：各フィルターの折り畳み状態（true: 折り畳み済み）
                                isIncludeTagsCollapsed = savedSettings.isIncludeTagsCollapsed || false;
                                isExcludeTagsCollapsed = savedSettings.isExcludeTagsCollapsed || false;
                                isHiddenAuthorsCollapsed = savedSettings.isHiddenAuthorsCollapsed || false;
                            }

                            // 設定をローカルストレージに保存
                            function saveSettings() {
                                const settings = {
                                    rowsPerPage,
                                    hiddenCols,
                                    currentPage,
                                    filteredAuthors,
                                    typeFilter,
                                    hiddenAuthors,
                                    sortInfo,
                                    // 追加した部分
                                    includedTags,
                                    excludedTags,
                                    includeOperator,
                                    excludeOperator,
                                    // 追加部分：折り畳み状態を保存
                                    isIncludeTagsCollapsed,
                                    isExcludeTagsCollapsed,
                                    isHiddenAuthorsCollapsed
                                };
                                localStorage.setItem('tableSettings', JSON.stringify(settings));
                            }

                            // UIを現在の設定に反映
                            function applySettingsToUI() {
                                document.getElementById("rowsPerPageSelect").value = rowsPerPage;
                                document.getElementById("typeSelect").value = typeFilter;
                                columns.forEach(col => {
                                    const checkbox = document.getElementById('show-' + col);
                                    if (!checkbox) return;
                                    checkbox.checked = !hiddenCols.includes(col);
                                });
                            }

                            function updateAuthorDropdownOptions() {
                                const dropdown = document.getElementById('author-filter-dropdown');
                                if (!dropdown) return;
                                // 既存のデフォルトオプションを残して、それ以降の option を一旦削除
                                while (dropdown.options.length > 1) {
                                    dropdown.remove(1);
                                }
                                // authorMap を { id: { author, updateTime } } の形式で生成（最新の update_date を採用）
                                const authorMap = {};
                                Object.entries(tableData).forEach(([key, item]) => {
                                    const id = item.author_id || item.author;
                                    const updateDateStr = item.update_date || "0";
                                    const updateTime = (new Date(updateDateStr)).getTime();
                                    if (!authorMap[id]) {
                                        authorMap[id] = { author: item.author, updateTime: updateTime };
                                    } else {
                                        if (updateTime > authorMap[id].updateTime) {
                                            authorMap[id] = { author: item.author, updateTime: updateTime };
                                        }
                                    }
                                });
                                // オプションを作者名の昇順で追加
                                const sortedIds = Object.keys(authorMap).sort((a, b) => {
                                    return authorMap[a].author.localeCompare(authorMap[b].author);
                                });
                                sortedIds.forEach(id => {
                                    const opt = document.createElement('option');
                                    opt.value = id;
                                    opt.textContent = authorMap[id].author;
                                    dropdown.appendChild(opt);
                                });
                            }
                
                            function updateAuthorDropdownValue() {
                                const dropdown = document.getElementById('author-filter-dropdown');
                                if (!dropdown) return;
                                if (filteredAuthors.length > 0) {
                                    dropdown.value = filteredAuthors[0];
                                } else {
                                    dropdown.value = "";
                                }
                            }

                            // 例: 作者の絞り込み・非表示処理を判定する関数
                            function handleAuthorFiltering(author, authorId, event) {
                                event.preventDefault();
                                if (confirm('この作者で絞り込みますか？  キャンセルを押すと非表示リストに追加されます。')) {
                                    // OKの場合：絞り込みリストに追加
                                    const dropdown = document.getElementById('author-filter-dropdown');
                                    if (dropdown) {
                                        dropdown.value = authorId;
                                    }
                                    updateAuthorFilter(authorId);
                                } else {
                                    // キャンセルの場合：非表示リストに追加
                                    if (!hiddenAuthors.includes(author)) {
                                        hiddenAuthors.push(author);
                                        renderHiddenAuthors();
                                    }
                                }
                                // 絞り込み更新時はページ番号をリセット
                                renderTable();
                                saveSettings();
                            }

                            // 既存の関数定義部付近に追加
                            function renderHiddenAuthors() {
                                const container = document.getElementById("hidden-author-container");
                                if (!container) return;
                                container.innerHTML = "";
                                
                                const header = document.createElement("div");
                                header.style.cursor = "pointer";
                                header.style.fontWeight = "bold";
                                header.textContent = "非表示作者" + (isHiddenAuthorsCollapsed ? " [+]" : " [-]");
                                header.addEventListener("click", () => {
                                    isHiddenAuthorsCollapsed = !isHiddenAuthorsCollapsed;
                                    saveSettings();
                                    renderHiddenAuthors();
                                });
                                container.appendChild(header);
                                
                                if (!isHiddenAuthorsCollapsed && hiddenAuthors.length > 0) {
                                    hiddenAuthors.forEach(author => {
                                        const span = document.createElement("span");
                                        span.textContent = author;
                                        span.classList.add("hidden-author-tag");  // CSSスタイル適用済み
                                        span.addEventListener("click", () => {
                                            // クリックで非表示リストから除去
                                            hiddenAuthors = hiddenAuthors.filter(a => a !== author);
                                            renderHiddenAuthors();
                                            renderTable();
                                            saveSettings();
                                        });
                                        container.appendChild(span);
                                        container.appendChild(document.createTextNode(" "));
                                    });
                                }
                            }

                            function formatDateTime(datetimeStr) {
                                const date = new Date(datetimeStr);
                                if (isNaN(date)) {
                                    return datetimeStr;
                                }
                                const year = date.getFullYear();
                                const month = ('0' + (date.getMonth() + 1)).slice(-2);
                                const day = ('0' + date.getDate()).slice(-2);
                                const hour = ('0' + date.getHours()).slice(-2);
                                const minute = ('0' + date.getMinutes()).slice(-2);
                                return `${year}/${month}/${day} ${hour}:${minute}`;
                            }

                            // 既存の createTagCheckboxes の代わりに追加する新関数
                            function renderTagFilters() {
                                const includeContainer = document.getElementById("include-tags");
                                const excludeContainer = document.getElementById("exclude-tags");
                                
                                // 含むフィルターのヘッダー作成
                                includeContainer.innerHTML = "";
                                const includeHeader = document.createElement("div");
                                includeHeader.style.cursor = "pointer";
                                includeHeader.style.fontWeight = "bold";
                                includeHeader.textContent = "含むタグ" + (isIncludeTagsCollapsed ? " [+]" : " [-]");
                                includeHeader.addEventListener("click", () => {
                                    isIncludeTagsCollapsed = !isIncludeTagsCollapsed;
                                    saveSettings();
                                    renderTagFilters();
                                });
                                includeContainer.appendChild(includeHeader);
                                
                                if (!isIncludeTagsCollapsed && includedTags.length > 0) {
                                    // 演算子ドロップダウンの追加
                                    const includeOpSelect = document.createElement("select");
                                    includeOpSelect.addEventListener("change", () => {
                                        includeOperator = includeOpSelect.value;
                                        saveSettings();
                                        renderTable();
                                    });
                                    ["AND", "OR"].forEach(op => {
                                        const opt = document.createElement("option");
                                        opt.value = op;
                                        opt.textContent = op;
                                        includeOpSelect.appendChild(opt);
                                    });
                                    includeOpSelect.value = includeOperator;
                                    includeContainer.appendChild(document.createTextNode(" 条件: "));
                                    includeContainer.appendChild(includeOpSelect);
                                    includeContainer.appendChild(document.createElement("br"));
                                    
                                    // 含むタグのチェックボックス
                                    includedTags.forEach(tag => {
                                        const checkbox = document.createElement("input");
                                        checkbox.type = "checkbox";
                                        checkbox.checked = true;
                                        checkbox.value = tag;
                                        checkbox.addEventListener("change", () => {
                                            if (!checkbox.checked) {
                                                includedTags = includedTags.filter(t => t !== tag);
                                                renderTagFilters();
                                                renderTable();
                                                saveSettings();
                                            }
                                        });
                                        includeContainer.appendChild(checkbox);
                                        includeContainer.appendChild(document.createTextNode(" " + tag));
                                        includeContainer.appendChild(document.createElement("br"));
                                    });
                                }
                                
                                // 非表示タグ用フィルターのヘッダー作成
                                excludeContainer.innerHTML = "";
                                const excludeHeader = document.createElement("div");
                                excludeHeader.style.cursor = "pointer";
                                excludeHeader.style.fontWeight = "bold";
                                excludeHeader.textContent = "含まないタグ" + (isExcludeTagsCollapsed ? " [+]" : " [-]");
                                excludeHeader.addEventListener("click", () => {
                                    isExcludeTagsCollapsed = !isExcludeTagsCollapsed;
                                    saveSettings();
                                    renderTagFilters();
                                });
                                excludeContainer.appendChild(excludeHeader);
                                
                                if (!isExcludeTagsCollapsed && excludedTags.length > 0) {
                                    const excludeOpSelect = document.createElement("select");
                                    excludeOpSelect.addEventListener("change", () => {
                                        excludeOperator = excludeOpSelect.value;
                                        saveSettings();
                                        renderTable();
                                    });
                                    ["AND", "OR"].forEach(op => {
                                        const opt = document.createElement("option");
                                        opt.value = op;
                                        opt.textContent = op;
                                        excludeOpSelect.appendChild(opt);
                                    });
                                    excludeOpSelect.value = excludeOperator;
                                    excludeContainer.appendChild(document.createTextNode(" 条件: "));
                                    excludeContainer.appendChild(excludeOpSelect);
                                    excludeContainer.appendChild(document.createElement("br"));
                                    
                                    excludedTags.forEach(tag => {
                                        const checkbox = document.createElement("input");
                                        checkbox.type = "checkbox";
                                        checkbox.checked = true;
                                        checkbox.value = tag;
                                        checkbox.addEventListener("change", () => {
                                            if (!checkbox.checked) {
                                                excludedTags = excludedTags.filter(t => t !== tag);
                                                renderTagFilters();
                                                renderTable();
                                                saveSettings();
                                            }
                                        });
                                        excludeContainer.appendChild(checkbox);
                                        excludeContainer.appendChild(document.createTextNode(" " + tag));
                                        excludeContainer.appendChild(document.createElement("br"));
                                    });
                                }
                            }
                
                            // テーブルヘッダーを動的に表示する
                            function renderTableHeaders() {
                                const tableHead = document.getElementById("table-head");
                                tableHead.innerHTML = "";
                                const headerRow = document.createElement("tr");
                                
                                // 左端にチェックボックス用ヘッダー（固定）
                                const selectTh = document.createElement("th");
                                selectTh.style.width = "3ch";
                                selectTh.textContent = "";
                                headerRow.appendChild(selectTh);
                                
                                // visibleColumns の取得（非表示でない列）
                                const visibleColumns = columns.filter(column => !hiddenCols.includes(column));
                                
                                // 固定列の合計幅 (数値、単位 ch)
                                let totalFixedCh = 0;
                                visibleColumns.forEach(column => {
                                    if(fixedWidthMapping[column]){
                                        totalFixedCh += fixedWidthMapping[column];
                                    }
                                });
                                
                                // 可変列の集合と合計重み
                                const visibleVariableColumns = visibleColumns.filter(column => !fixedWidthMapping[column]);
                                let totalVariableWeight = 0;
                                visibleVariableColumns.forEach(column => {
                                    totalVariableWeight += variableWeightMapping[column] || 0;
                                });
                                
                                // 各列ごとの幅設定
                                columns.forEach(column => {
                                    const th = document.createElement("th");
                                    th.textContent = columnLabel(column);
                                    th.addEventListener('click', () => {
                                        sortByColumn(column);
                                    });
                                    th.classList.add(`th-${column}`);
                                    
                                    if(hiddenCols.includes(column)) {
                                        th.classList.add("hidden-column");
                                    } else {
                                        // 固定列：そのまま ch 単位指定
                                        if(fixedWidthMapping[column]){
                                            th.style.width = fixedWidthMapping[column] + "ch";
                                        } else {
                                            // 可変列：100% から固定列の幅分を差し引いた余剰スペースを重みで分配
                                            // 例："calc((100% - 20ch) * (weight/totalVariableWeight))"
                                            let weight = variableWeightMapping[column] || 0;
                                            th.style.width = "calc((100% - " + totalFixedCh + "ch) * " + (weight / totalVariableWeight).toFixed(2) + ")";
                                        }
                                    }
                                    headerRow.appendChild(th);
                                });
                                tableHead.appendChild(headerRow);
                            }

                            // 表示用にカラム名を返す
                            function columnLabel(column) {
                                switch(column) {
                                    case 'serialization': return '連載状況';
                                    case 'title': return 'タイトル';
                                    case 'author': return '作者名';
                                    case 'type': return '形式';
                                    case 'tags': return 'タグ';
                                    case 'create_date': return '掲載日時';
                                    case 'update_date': return '更新日時';
                                    default: return column.charAt(0).toUpperCase() + column.slice(1);
                                }
                            }

                            // ソート処理
                            function sortByColumn(column) {
                                if (sortInfo.column === column) {
                                    sortInfo.ascending = !sortInfo.ascending;
                                } else {
                                    sortInfo.column = column;
                                    sortInfo.ascending = true;
                                }
                                currentPage = 1;
                                renderTable();
                                updatePagination();
                                saveSettings();
                            }

                            // タグクリック時のフィルター処理
                            function tagFilterClick(tag) {
                                if (confirm(`「${tag}」を含むフィルターに追加しますか？ キャンセルを押すと非表示フィルターに追加されます。`)) {
                                    if (!includedTags.includes(tag)) {
                                        includedTags.push(tag);
                                    }
                                    excludedTags = excludedTags.filter(t => t !== tag);
                                } else {
                                    if (!excludedTags.includes(tag)) {
                                        excludedTags.push(tag);
                                    }
                                    includedTags = includedTags.filter(t => t !== tag);
                                }
                                currentPage = 1;
                                renderTagFilters();  // 追加
                                renderTable();
                                updatePagination();
                                saveSettings();
                            }

                            function updateAuthorFilter(authorId) {
                                if (authorId === "") {
                                    filteredAuthors = [];
                                } else {
                                    filteredAuthors = [authorId];
                                }
                                currentPage = 1;
                                updatePagination();
                                renderTable();
                                saveSettings();
                            }

                            // テーブル本体の描画
                            function renderTable() {
                                const tableBody = document.getElementById("user-table-body");
                                tableBody.innerHTML = "";
                                let dataEntries = Object.entries(tableData);
                                // 形式フィルター
                                if (typeFilter !== "all") {
                                    dataEntries = dataEntries.filter(([_, item]) => item.type === typeFilter);
                                }

                                // 作者フィルター / 非表示
                                dataEntries = dataEntries.filter(([_, item]) => {
                                    const itemAuthorId = item.author_id || item.author;
                                    return !hiddenAuthors.includes(item.author) &&
                                        (filteredAuthors.length === 0 || filteredAuthors.includes(itemAuthorId));
                                });

                                // タグによるフィルター（含むタグ、除外タグの条件）
                                dataEntries = dataEntries.filter(([_, item]) => {
                                    if (item.all_tags) {
                                        let includeMatch = true;
                                        if (includedTags.length > 0) {
                                            if (includeOperator === "AND") {
                                                includeMatch = includedTags.every(tag => item.all_tags.includes(tag));
                                            } else {
                                                includeMatch = includedTags.some(tag => item.all_tags.includes(tag));
                                            }
                                        }
                                        let excludeMatch = true;
                                        if (excludedTags.length > 0) {
                                            if (excludeOperator === "OR") {
                                                // OR: いずれかのタグが含まれていたら除外
                                                excludeMatch = !excludedTags.some(tag => item.all_tags.includes(tag));
                                            } else {
                                                // AND: 全ての除外タグが含まれていたら除外
                                                excludeMatch = !excludedTags.every(tag => item.all_tags.includes(tag));
                                            }
                                        }
                                        if (!includeMatch || !excludeMatch) return false;
                                    }
                                    return true;
                                });
                                // ソート
                                if (sortInfo.column) {
                                    dataEntries.sort(([_, a], [__, b]) => {
                                        let valA = a[sortInfo.column] ?? "";
                                        let valB = b[sortInfo.column] ?? "";
                                        if (!isNaN(valA) && !isNaN(valB)) {
                                            valA = parseFloat(valA);
                                            valB = parseFloat(valB);
                                        }
                                        if (valA < valB) return -1;
                                        if (valA > valB) return 1;
                                        return 0;
                                    });
                                    if (!sortInfo.ascending) {
                                        dataEntries.reverse();
                                    }
                                }
                                // ページネーション用スライス (rowsPerPageが0なら全件)
                                let start = (currentPage - 1) * rowsPerPage;
                                let end = rowsPerPage === 0 ? dataEntries.length : start + rowsPerPage;
                                let paginatedData = dataEntries.slice(start, end);
                                paginatedData.forEach(([key, item]) => {
                                    const row = document.createElement("tr");
                                    
                                    // チェックボックスセル（固定）
                                    const selectTd = document.createElement("td");
                                    selectTd.style.width = "3ch";
                                    const checkbox = document.createElement("input");
                                    checkbox.type = "checkbox";
                                    checkbox.classList.add("row-checkbox");
                                    checkbox.setAttribute("data-key", key);
                                    checkbox.checked = selectedRows.has(key);
                                    checkbox.addEventListener("change", function() {
                                        const rowKey = this.getAttribute("data-key");
                                        if (this.checked) {
                                            selectedRows.add(rowKey);
                                        } else {
                                            selectedRows.delete(rowKey);
                                        }
                                        updateSelectedCount();
                                    });
                                    selectTd.appendChild(checkbox);
                                    row.appendChild(selectTd);
                                    
                                    // visibleColumns の取得（非表示でない列）
                                    const visibleColumns = columns.filter(column => !hiddenCols.includes(column));
                                    
                                    // 固定列の合計幅 (数値、単位 ch)
                                    let totalFixedCh = 0;
                                    visibleColumns.forEach(column => {
                                        if(fixedWidthMapping[column]){
                                            totalFixedCh += fixedWidthMapping[column];
                                        }
                                    });
                                    // 可変列の集合と合計重み
                                    const visibleVariableColumns = visibleColumns.filter(column => !fixedWidthMapping[column]);
                                    let totalVariableWeight = 0;
                                    visibleVariableColumns.forEach(column => {
                                        totalVariableWeight += variableWeightMapping[column] || 0;
                                    });
                                    
                                    columns.forEach(column => {
                                        const td = document.createElement("td");
                                        if (hiddenCols.includes(column)) {
                                            td.classList.add("hidden-column");
                                            td.textContent = "";
                                        } else {
                                            if(fixedWidthMapping[column]){
                                                td.style.width = fixedWidthMapping[column] + "ch";
                                            } else {
                                                let weight = variableWeightMapping[column] || 0;
                                                td.style.width = "calc((100% - " + totalFixedCh + "ch) * " + (weight / totalVariableWeight).toFixed(2) + ")";
                                            }
                                            td.classList.add(`td-${column}`);
                                            // 各列の内容生成処理
                                            if (column === 'serialization') {
                                                td.textContent = item.serialization ?? "";
                                            } else if (column === 'title') {
                                                const titleLink = document.createElement("a");
                                                titleLink.href = `./${key}/`;
                                                titleLink.textContent = item.title;
                                                td.appendChild(titleLink);
                                            } else if (column === 'author') {
                                                const authorLink = document.createElement("a");
                                                authorLink.href = item.author_url;
                                                authorLink.target = "_blank";
                                                authorLink.textContent = item.author;
                                                // Ctrl キー押下時はカスタム処理でメニュー表示
                                                authorLink.addEventListener("click", function(event) {
                                                    if (event.ctrlKey) {
                                                        event.preventDefault(); // 新規タブで開くのをキャンセル
                                                        // Ctrl+クリック時に handleAuthorFiltering を呼び出す
                                                        handleAuthorFiltering(item.author, item.author_id || item.author, event);
                                                    }
                                                });
                                                td.appendChild(authorLink);
                                            } else if (column === 'type') {
                                                td.textContent = item.type === 'novel' ? '小説' : '漫画';
                                            } else if (column === 'tags') {
                                                td.innerHTML = "";
                                                if (Array.isArray(item.all_tags)) {
                                                    item.all_tags.forEach(t => {
                                                        const tagSpan = document.createElement('span');
                                                        tagSpan.textContent = t;
                                                        tagSpan.classList.add("tag-item");
                                                        tagSpan.style.cursor = 'pointer';
                                                        tagSpan.addEventListener('click', () => { tagFilterClick(t); });
                                                        td.appendChild(tagSpan);
                                                    });
                                                }
                                            } else if (column === 'create_date' || column === 'update_date') {
                                                td.textContent = formatDateTime(item[column] ?? "");
                                            } else {
                                                td.textContent = item[column] ?? "";
                                            }
                                        }
                                        row.appendChild(td);
                                    });
                                    tableBody.appendChild(row);
                                });
                            }

                            // ページ情報を更新
                            function updatePagination() {
                                const pageInfo = document.getElementById("page-info");
                                const totalDataCount = Object.entries(tableData).filter(([_, item]) => {
                                    if (typeFilter !== 'all' && item.type !== typeFilter) return false;
                                    if (hiddenAuthors.includes(item.author)) return false;
                                    const itemAuthorId = item.author_id || item.author;
                                    if (filteredAuthors.length > 0 && !filteredAuthors.includes(itemAuthorId)) return false;
                                    if (item.all_tags) {
                                        const hasAllIncluded = includedTags.every(tag => item.all_tags.includes(tag));
                                        const hasNoneExcluded = excludedTags.every(tag => !item.all_tags.includes(tag));
                                        if (!hasAllIncluded || !hasNoneExcluded) return false;
                                    }
                                    return true;
                                }).length;
                                let totalPages = Math.ceil(totalDataCount / (rowsPerPage === 0 ? totalDataCount : rowsPerPage));
                                if (totalPages < 1) totalPages = 1;
                                pageInfo.textContent = `${currentPage} / ${totalPages}`;
                            }

                            function nextPage() {
                                const totalDataCount = Object.entries(tableData).filter(([_, item]) => {
                                    if (typeFilter !== 'all' && item.type !== typeFilter) return false;
                                    if (hiddenAuthors.includes(item.author)) return false;
                                    const itemAuthorId = item.author_id || item.author;
                                    if (filteredAuthors.length > 0 && !filteredAuthors.includes(itemAuthorId)) return false;
                                    if (item.all_tags) {
                                        const hasAllIncluded = includedTags.every(tag => item.all_tags.includes(tag));
                                        const hasNoneExcluded = excludedTags.every(tag => !item.all_tags.includes(tag));
                                        if (!hasAllIncluded || !hasNoneExcluded) return false;
                                    }
                                    return true;
                                }).length;
                                
                                let totalPages = Math.ceil(totalDataCount / (rowsPerPage === 0 ? totalDataCount : rowsPerPage));
                                if (totalPages < 1) totalPages = 1;
                                
                                if (currentPage < totalPages) {
                                    currentPage++;
                                    renderTable();
                                    updatePagination();
                                    saveSettings();
                                }
                            }

                            function prevPage() {
                                if (currentPage > 1) {
                                    currentPage--;
                                    renderTable();
                                    updatePagination();
                                    saveSettings();
                                }
                            }

                            // 行数変更
                            function updateRowsPerPage() {
                                const select = document.getElementById("rowsPerPageSelect");
                                rowsPerPage = parseInt(select.value, 10);
                                currentPage = 1;
                                renderTable();
                                updatePagination();
                                saveSettings();
                            }

                            // カラムの表示切替（幅0化）
                            function toggleColumn(column) {
                                const checkbox = document.getElementById('show-' + column);
                                if (!checkbox.checked) {
                                    if (!hiddenCols.includes(column)) {
                                        hiddenCols.push(column);
                                    }
                                } else {
                                    const idx = hiddenCols.indexOf(column);
                                    if (idx !== -1) {
                                        hiddenCols.splice(idx, 1);
                                    }
                                }
                                renderTable();
                                renderTableHeaders();
                                updatePagination();
                                saveSettings();
                            }

                            // 形式フィルター
                            function filterByType() {
                                const select = document.getElementById("typeSelect");
                                typeFilter = select.value;
                                currentPage = 1;
                                renderTable();
                                updatePagination();
                                saveSettings();
                            }

                            function updateSelectedCount() {
                                document.getElementById("selected-count").textContent = "選択された件数: " + selectedRows.size;
                            }
                
                            // コピー結果をポップアップ表示する関数
                            function showCopyPopup(titles) {
                                const modalOverlay = document.createElement("div");
                                modalOverlay.style.position = "fixed";
                                modalOverlay.style.top = "0";
                                modalOverlay.style.left = "0";
                                modalOverlay.style.width = "100%";
                                modalOverlay.style.height = "100%";
                                modalOverlay.style.backgroundColor = "rgba(0, 0, 0, 0.5)";
                                modalOverlay.style.display = "flex";
                                modalOverlay.style.alignItems = "center";
                                modalOverlay.style.justifyContent = "center";
                                
                                const modalBox = document.createElement("div");
                                modalBox.style.backgroundColor = "#fff";
                                modalBox.style.padding = "1em";
                                modalBox.style.borderRadius = "5px";
                                modalBox.style.maxWidth = "80%";
                                modalBox.style.maxHeight = "80%";
                                modalBox.style.overflowY = "auto";
                                modalBox.innerHTML = "<strong>リンク先をコピーしました</strong><br><br>" + titles.join("<br>") +
                                    "<br><br><button id='close-copy-popup'>閉じる</button>";
                                
                                modalOverlay.appendChild(modalBox);
                                document.body.appendChild(modalOverlay);
                                
                                document.getElementById("close-copy-popup").addEventListener("click", function() {
                                    document.body.removeChild(modalOverlay);
                                });
                            }

                            // 既存の copySelected 関数の修正例（前後数行含む）
                            function copySelected() {
                                const checkboxes = document.querySelectorAll('.row-checkbox');
                                let links = [];
                                let titles = [];
                                checkboxes.forEach(cb => {
                                    if (cb.checked) {
                                        const row = cb.closest("tr");
                                        if (row) {
                                            const titleCell = row.querySelector(".td-title");
                                            if (titleCell) {
                                                const a = titleCell.querySelector("a");
                                                if (a) {
                                                    links.push(a.href);
                                                    titles.push(a.textContent);
                                                }
                                            }
                                        }
                                    }
                                });
                                const newline = String.fromCharCode(10);
                                const copyText = links.join(newline);
                                navigator.clipboard.writeText(copyText).then(() => {
                                    showCopyPopup(titles);
                                }).catch(err => {
                                    alert("コピーに失敗しました: " + err);
                                });
                            }

                            document.addEventListener("DOMContentLoaded", function() {

                                document.getElementById("copy-selected-button").addEventListener("click", copySelected);

                                document.getElementById("reset-localstorage-button").addEventListener("click", function(){
                                    if (confirm("ローカルストレージをリセットしますか？")) {
                                        localStorage.clear();
                                        location.reload();
                                    }
                                });
                                
                                // 非表示作者リセット
                                document.getElementById("reset-hidden-authors-button").addEventListener("click", function(){
                                    if (confirm("非表示作者をリセットしますか？")) {
                                        hiddenAuthors = [];  // グローバル変数のリセット
                                        saveSettings();        // 変更をローカルストレージに書き込み
                                        renderHiddenAuthors(); // 非表示作者エリアを更新
                                        renderTable();         // テーブルの再描画
                                    }
                                });
                                
                                // 含むタグリセット
                                document.getElementById("reset-include-tags-button").addEventListener("click", function(){
                                    if (confirm("含むタグをリセットしますか？")) {
                                        includedTags = [];
                                        saveSettings();
                                        renderTagFilters();
                                        renderTable();
                                    }
                                });
                                
                                // 含まないタグリセット
                                document.getElementById("reset-exclude-tags-button").addEventListener("click", function(){
                                    if (confirm("含まないタグをリセットしますか？")) {
                                        excludedTags = [];
                                        saveSettings();
                                        renderTagFilters();
                                        renderTable();
                                    }
                                });
                
                                // 作者絞り込みリセットボタン
                                document.getElementById("reset-author-filter-button").addEventListener("click", function(){
                                    if (confirm("作者絞り込みをリセットしますか？")) {
                                        filteredAuthors = [];  // グローバル変数をリセット
                                        const dropdown = document.getElementById("author-filter-dropdown");
                                        if (dropdown) { dropdown.value = ""; }
                                        saveSettings();        // ローカルストレージに書き込み
                                        renderTable();         // テーブル更新
                                    }
                                });
                            });

                            document.addEventListener('keydown', function(event) {
                
                                if (event.key === "Escape") {
                                    event.preventDefault();
                                    // すべての選択状態をクリア
                                    selectedRows.clear();
                                    // 全チェックボックスのチェックを外す（renderTableで反映されるよう再描画）
                                    renderTable();
                                    updateSelectedCount();
                                }

                                if ((event.ctrlKey || event.shiftKey) && event.key.toLowerCase() === 'a') {
                                    event.preventDefault(); // テキスト選択などのデフォルト動作をキャンセル
                                    
                                    if (event.shiftKey && !event.ctrlKey) {
                                        // Shift+A：フィルター適用後のすべての行のキー（現在の全ページ以外も）を選択
                                        
                                        // 以下、tableData から現在のフィルター条件に合致する行キーを収集（renderTable で用いている条件と同様）
                                        const filteredKeys = Object.entries(tableData)
                                            .filter(([key, item]) => {
                                                // 形式フィルター
                                                if (typeFilter !== "all" && item.type !== typeFilter) return false;
                                                // 作者非表示フィルター
                                                if (hiddenAuthors.includes(item.author)) return false;
                                                // 作者絞り込みフィルター（filteredAuthors は作者id 等を保持）
                                                const itemAuthorId = item.author_id || item.author;
                                                if (filteredAuthors.length > 0 && !filteredAuthors.includes(itemAuthorId)) return false;
                                                
                                                // タグフィルター（省略：必要な処理を追加）
                                                if (item.all_tags) {
                                                    let includeMatch = true;
                                                    if (includedTags.length > 0) {
                                                        if (includeOperator === "AND") {
                                                            includeMatch = includedTags.every(tag => item.all_tags.includes(tag));
                                                        } else {
                                                            includeMatch = includedTags.some(tag => item.all_tags.includes(tag));
                                                        }
                                                    }
                                                    let excludeMatch = true;
                                                    if (excludedTags.length > 0) {
                                                        if (excludeOperator === "OR") {
                                                            excludeMatch = !excludedTags.some(tag => item.all_tags.includes(tag));
                                                        } else {
                                                            excludeMatch = !excludedTags.every(tag => item.all_tags.includes(tag));
                                                        }
                                                    }
                                                    if (!includeMatch || !excludeMatch) return false;
                                                }
                                                return true;
                                            })
                                            .map(([key, item]) => key);
                                        filteredKeys.forEach(key => selectedRows.add(key));
                                    } else if (event.ctrlKey && !event.shiftKey) {
                                        // Ctrl+A：現在表示されている行のみ選択
                                        const visibleCheckboxes = document.querySelectorAll('#user-table-body .row-checkbox');
                                        visibleCheckboxes.forEach(cb => {
                                            const rowKey = cb.getAttribute("data-key");
                                            if (!cb.checked) {
                                                cb.checked = true;
                                                selectedRows.add(rowKey);
                                            }
                                        });
                                    }
                                    updateSelectedCount();
                                    // 全体再描画で、現在のページのチェック状態に反映させる
                                    renderTable();
                                }
                            });

                            fetchData().then(applySettingsToUI);
                        </script>
                    </body>
                    </html>
                    """)

    
    with open(os.path.join(folder_path, 'index.json'), 'w', encoding='utf-8') as f:
        json.dump(pairs, f, ensure_ascii=False, indent=4)

    if no_raw:
        logging.warning(f"The folders {', '.join(no_raw)} were deleted because they do not contain 'raw.json'.")

    logging.info('目次の生成が完了しました')