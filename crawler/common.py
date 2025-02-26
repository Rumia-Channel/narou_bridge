import os
import shutil
import json
from jsondiff import diff
import requests
from requests.exceptions import RequestException, ConnectionError, Timeout
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
def get_with_cookie(url, cookie, header, retries=5, delay=1):
    for i in range(retries):
        try:
            response = requests.get(url, cookies=cookie, headers=header, timeout=10)
            response.raise_for_status()  # HTTPエラーをキャッチ
            return response
        except (ConnectionError, Timeout) as e:
            logging.error(f"\nError: {e}. Retrying in {delay * (2 ** i)} seconds...")
        except RequestException as e:
            # 404エラーを特別扱い
            if response.status_code == 404:
                logging.error("\n404 Error: Resource not found.")
                return None  # 404エラーの場合はリトライしない
            else:
                logging.error(f"\nError: {e}. Retrying in {delay * (2 ** i)} seconds...")
        
        if i < retries - 1:
            time.sleep(delay * (2 ** i))  # 指数バックオフ
        else:
            logging.error("\nThe retry limit has been reached. No response received.。")
            return None  # リトライ限界に達した場合
        
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
                create_date = data.get('createDate', 'No create date found')
                update_date = data.get('updateDate', 'No update date found')
                type = data.get('type', 'No type found')
                serialization = data.get('serialization', 'No serialization found')
                pairs[folder] = {'title': title, 'author': author, 'author_id': author_id, 'author_url' : author_url, 'type': type, 'serialization': serialization, 'create_date': create_date, 'update_date': update_date}
        else:
            #print(f"raw.json not found in {folder}")
            #return
            shutil.rmtree(os.path.join(folder_path, folder))
            no_raw.append(folder)
            continue
    
    pairs = dict(sorted(pairs.items(), key=lambda item: item[1]['author']))

    # index.html の生成
    with open(os.path.join(folder_path, 'index.html'), 'w', encoding='utf-8') as f:
        f.write('<!DOCTYPE html>\n')
        f.write('<html lang="ja">\n')
        f.write('<head>\n')
        f.write('<meta charset="UTF-8">\n')
        f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
        f.write(f'<title>{site_name} Index</title>\n')
        # CSS追加
        f.write('<style>\n')
        f.write('table { width: 100%; border-collapse: collapse; }\n')
        f.write('th, td { border: 1px solid #ccc; padding: 8px; text-align: left; overflow-wrap: break-word; word-wrap: break-word; }\n')
        f.write('</style>\n')
        f.write('</head>\n')
        f.write('<body>\n')

        # 戻るリンク
        f.write(f'<a href="../{key_data}">戻る</a>\n')

        # 右寄せで数値入力欄とボタン
        f.write('''<div style="text-align: right; margin-top: 10px;">
            折り返し文字数 <input type="number" id="maxLengthInput" value="10" min="1" style="width: 60px;" />
            <button id="saveButton">保存</button>
        </div>\n''')

        # 非表示にするユーザーID入力欄とボタン
        f.write('''<div style="text-align: right; margin-top: 20px;">
            <label for="hideUserId">非表示にするユーザーID:</label>
            <input type="text" id="hideUserId" placeholder="ユーザーIDを入力" style="width: 200px;" />
            <button id="hideUserButton">非表示にする</button>
        </div>\n''')

        # 非表示から戻すユーザーID入力欄とボタン
        f.write('''<div style="text-align: right; margin-top: 20px;">
            <label for="restoreUserId">戻すユーザーID:</label>
            <input type="text" id="restoreUserId" placeholder="ユーザーIDを入力" style="width: 200px;" />
            <button id="restoreUserButton">戻す</button>
        </div>\n''')

        # 非表示ユーザーリスト（IDと著者名、戻すボタンを含む）
        f.write('''<div style="margin-top: 20px;">
            <button id="toggleHiddenUsers">非表示ユーザーを表示</button>
            <ul id="hiddenUsersList" style="display: none;"></ul>
        </div>\n''')

        f.write(f'<h1>{site_name} 小説一覧</h1>\n')
        f.write('<table>\n')
        f.write('<tr><th>掲載タイプ</th><th>タイトル</th><th>作者名</th><th>掲載日時</th><th>更新日時</th></tr>\n')

        # 各行のデータ出力（author_idをclassとして扱う）
        for folder, info in pairs.items():
            author_id = info["author_id"]
            f.write(f'''<tr class="author-{author_id}"><td>{info["serialization"]}</td>
                        <td class="text"><a href="{folder}/{key_data}" class="text">{info["title"]}</a></td>
                        <td class="text"><a href="{info["author_url"]}" target="_blank">{info["author"]}</a>　<button class="copyButton" data-author-id="{author_id}">IDのコピー</button></td>
                        <td>{datetime.fromisoformat(info["create_date"]).strftime("%Y/%m/%d %H:%M")}</td>
                        <td>{datetime.fromisoformat(info["update_date"]).strftime("%Y/%m/%d %H:%M")}</td></tr>\n''')

        f.write('</table>\n')

        # JavaScriptによる処理
        f.write("""<script>
            // テキスト折り返し関数
            const wrapTextByLength = (text, maxLength) => {
                return text.match(new RegExp(`.{1,${maxLength}}`, 'g')).join('<br>');
            };

            // localStorageから折り返し文字数を取得し、なければデフォルト（10文字）を使用
            let maxLength = localStorage.getItem('maxLength') || 10;

            // 数値入力欄にローカルストレージの値を表示
            document.getElementById('maxLengthInput').value = maxLength;

            // ページロード時に非表示ユーザーを確認して非表示にする
            document.addEventListener('DOMContentLoaded', function() {
                // localStorage から非表示ユーザーを取得
                let hiddenUsers = JSON.parse(localStorage.getItem('hiddenUsers')) || [];

                // 非表示ユーザーをリストに追加
                let hiddenUsersList = document.getElementById('hiddenUsersList');
                hiddenUsers.forEach(function(userId) {
                    // 非表示にするユーザーIDがあれば、行を非表示にする
                    var rows = document.querySelectorAll('.author-' + userId);
                    rows.forEach(function(row) {
                        row.style.display = 'none';
                    });

                    // 非表示ユーザーリストに表示
                    var listItem = document.createElement('li');
                    var userName = rows[0].cells[2].querySelector('a').textContent;  // 著者名を取得
                    listItem.innerHTML = `${userId} - ${userName} <button class="restoreButton" data-author-id="${userId}">戻す</button>`;
                    hiddenUsersList.appendChild(listItem);
                });

                // 非表示ユーザーリストが空でない場合、ボタンを更新
                if (hiddenUsers.length > 0) {
                    document.getElementById('toggleHiddenUsers').style.display = 'block';
                }
            });

            // テーブル内の特定の列を対象に折り返し処理を適用
            document.querySelectorAll('table tr').forEach(row => {
                const titleCell = row.cells[1];
                const authorCell = row.cells[2];

                if (titleCell) {
                    // タイトルセル内のリンク部分を保持しつつ、テキストを折り返し
                    const titleLink = titleCell.querySelector('a');
                    const titleText = titleLink ? titleLink.textContent : titleCell.textContent;

                    if (titleLink) {
                        titleLink.innerHTML = wrapTextByLength(titleText, maxLength);
                    } else {
                        titleCell.innerHTML = wrapTextByLength(titleText, maxLength);
                    }
                }

                if (authorCell) {
                    // 作者名セル内のリンク部分を保持しつつ、テキストを折り返し
                    const authorLink = authorCell.querySelector('a');
                    const authorText = authorLink ? authorLink.textContent : authorCell.textContent;
                    
                    if (authorLink) {
                        authorLink.innerHTML = wrapTextByLength(authorText, maxLength);
                    } else {
                        authorCell.innerHTML = wrapTextByLength(authorText, maxLength);
                    }
                }
            });

            // 戻すボタンのイベントリスナー
            hiddenUsersList.addEventListener('click', function(event) {
                if (event.target.classList.contains('restoreButton')) {
                    const userId = event.target.getAttribute('data-author-id');
                    let hiddenUsers = JSON.parse(localStorage.getItem('hiddenUsers')) || [];

                    // 非表示ユーザーリストから該当ユーザーIDを削除
                    hiddenUsers = hiddenUsers.filter(id => id !== userId);
                    localStorage.setItem('hiddenUsers', JSON.stringify(hiddenUsers));

                    // 非表示リストから該当リストアイテムを削除
                    event.target.parentElement.remove();

                    // 非表示にしていたユーザーの行を再表示
                    var rows = document.querySelectorAll('.author-' + userId);
                    rows.forEach(function(row) {
                        row.style.display = '';
                    });
                }
            });

            // 保存ボタンのイベント
            document.getElementById('saveButton').addEventListener('click', () => {
                const inputValue = document.getElementById('maxLengthInput').value;
                if (inputValue && inputValue > 0) {
                    localStorage.setItem('maxLength', inputValue);
                    location.reload();
                }
            });

            // 非表示にするユーザーIDを追加
            document.getElementById('hideUserButton').addEventListener('click', function() {
                var userId = document.getElementById('hideUserId').value;
                if (userId) {
                    let hiddenUsers = JSON.parse(localStorage.getItem('hiddenUsers')) || [];
                    if (!hiddenUsers.includes(userId)) {
                        hiddenUsers.push(userId);
                        localStorage.setItem('hiddenUsers', JSON.stringify(hiddenUsers));

                        // 該当のユーザー行を非表示
                        var rows = document.querySelectorAll('.author-' + userId);
                        rows.forEach(function(row) {
                            row.style.display = 'none';
                        });

                        // 非表示ユーザーリストに追加
                        var hiddenUsersList = document.getElementById('hiddenUsersList');
                        var userName = rows[0].cells[2].querySelector('a').textContent;  // 著者名を取得
                        var listItem = document.createElement('li');
                        listItem.innerHTML = `${userId} - ${userName} <button class="restoreButton" data-author-id="${userId}">戻す</button>`;
                        hiddenUsersList.appendChild(listItem);

                        // フォームリセット
                        document.getElementById('hideUserId').value = '';
                    }
                }
            });

            // 非表示から戻すユーザーIDを追加
            document.getElementById('restoreUserButton').addEventListener('click', function() {
                var userId = document.getElementById('restoreUserId').value;
                if (userId) {
                    let hiddenUsers = JSON.parse(localStorage.getItem('hiddenUsers')) || [];
                    if (hiddenUsers.includes(userId)) {
                        hiddenUsers = hiddenUsers.filter(id => id !== userId);
                        localStorage.setItem('hiddenUsers', JSON.stringify(hiddenUsers));

                        // 非表示ユーザーリストから削除
                        var hiddenUsersList = document.getElementById('hiddenUsersList');
                        const listItem = hiddenUsersList.querySelector(`li button[data-author-id="${userId}"]`).parentNode;
                        hiddenUsersList.removeChild(listItem);

                        // 非表示にしていた行を再表示
                        var rows = document.querySelectorAll('.author-' + userId);
                        rows.forEach(function(row) {
                            row.style.display = '';
                        });

                        // 非表示ユーザーリストから削除
                        var hiddenUsersList = document.getElementById('hiddenUsersList');
                        var listItems = hiddenUsersList.getElementsByTagName('li');
                        for (let item of listItems) {
                            if (item.textContent.includes(userId)) {
                                hiddenUsersList.removeChild(item);
                                break;
                            }
                        }

                        // フォームリセット
                        document.getElementById('restoreUserId').value = '';
                    }
                }
            });

            // 非表示ユーザーを表示/非表示切り替え
            document.getElementById('toggleHiddenUsers').addEventListener('click', function() {
                const hiddenUsersList = document.getElementById('hiddenUsersList');
                if (hiddenUsersList.style.display === 'none') {
                    hiddenUsersList.style.display = 'block';
                    this.textContent = '非表示ユーザーを隠す';
                } else {
                    hiddenUsersList.style.display = 'none';
                    this.textContent = '非表示ユーザーを表示';
                }
            });
                
            // コピー機能
            document.querySelectorAll('.copyButton').forEach(function(button) {
                button.addEventListener('click', function() {
                    var authorId = button.getAttribute('data-author-id'); // ユーザーIDを取得
                    if (authorId) {
                        // クリップボードにコピー
                        navigator.clipboard.writeText(authorId).then(function() {
                            alert('ユーザーIDをコピーしました: ' + authorId);
                        }).catch(function(err) {
                            console.error('コピー失敗:', err);
                        });
                    }
                });
            });
        </script>\n""")

        f.write('</body>\n')
        f.write('</html>\n')

    
    with open(os.path.join(folder_path, 'index.json'), 'w', encoding='utf-8') as f:
        json.dump(pairs, f, ensure_ascii=False, indent=4)

    if no_raw:
        logging.warning(f"The folders {', '.join(no_raw)} were deleted because they do not contain 'raw.json'.")

    logging.info('目次の生成が完了しました')