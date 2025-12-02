import os
import shutil
import json
import requests
from requests.exceptions import RequestException, ConnectionError, Timeout
from urllib.parse import unquote
import time
from datetime import datetime, timezone, timedelta
import hashlib
import base64
import re
import logging
from jsondiff import diff
from typing import Optional, Dict, Any, Union, List

# --- 定数定義 ---

# JST定義
JST = timezone(timedelta(hours=9))

# 字下げ処理用定数
_INDENT_OMIT_CHARS = " 　「『（【〔〖〘〈《｛"
_INDENT_PATTERN = re.compile(r'(^|\n)(?![' + re.escape(_INDENT_OMIT_CHARS) + '])')

# 全角半角変換用テーブル
_FULLWIDTH = '０１２３４５６７８９ＡＢＣＤＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺａｂｃｄｅｆｇｈｉｊｋｌｍｎｏｐｑｒｓｔｕｖｗｘｙｚ！”＃＄％＆’（）＊＋，－．／：；＜＝＞？＠［＼］＾＿｀｛｜｝～'
_HALFWIDTH = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~'
_TRANSLATE_TABLE = str.maketrans(_FULLWIDTH, _HALFWIDTH)

# HTMLテンプレート (index.html用)
_HTML_TEMPLATE = """<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{site_name} Index</title>
    <link rel="stylesheet" href="/css/index.css">
    <link rel="icon" href="/icon/favicon.ico">
    <link rel="manifest" href="/manifest.json">
    <link rel="apple-touch-icon" sizes="180x180" href="/icon/icon_180x180.png">
    <link rel="apple-touch-icon" sizes="167x167" href="/icon/icon_167x167.png">
    <link rel="apple-touch-icon" sizes="152x152" href="/icon/icon_152x152.png">
</head>
<body>
    <div><a href="../" class="header-link">戻る</a></div><br><br>
    <div style='display: flex; justify-content: space-between;'><h1>{site_name} Index</h1><button id='reset-localstorage-button'>ローカルストレージリセット</button></div>
    <div class="type-selector">
        <label for="typeSelect">表示タイプ: </label>
        <select id="typeSelect" onchange="filterByType()">
            <option value="all">すべて</option>
            <option value="novel">小説のみ</option>
            <option value="comic">漫画のみ</option>
        </select>
    </div>

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

    <div style="display: flex; justify-content: space-between;">
        <div class="author-filter">
            <label for="author-filter-dropdown">作者絞り込み:</label>
            <select id="author-filter-dropdown" onchange="updateAuthorFilter(this.value)">
                <option value="">全て表示</option>
                </select>
        </div>
        <button id="reset-author-filter-button">作者絞り込みリセット</button>
    </div>
    <div style="display: flex; justify-content: space-between;">
        <div class="hidden-author-filter">
            <span id="hidden-author-container"></span>
        </div>
        <button id="reset-hidden-authors-button">非表示作者リセット</button>
    </div>
    <br>
    <div class="selection-controls" style="margin-top: 1em;">
        <span id="selected-count">選択された件数: 0</span>
        <button id="copy-selected-button">選択項目のリンクをコピー</button>
    </div>
    <br>
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
                </tr>
        </thead>
        <tbody id="user-table-body">
            </tbody>
    </table>

    <div class="pagination">
        <button onclick="prevPage()">前へ</button>
        <span id="page-info">1 / 1</span>
        <button onclick="nextPage()">次へ</button>
    </div>

    <script src="/script/index.js"></script>
    <link rel="stylesheet" href="/css/common.css">
</body>
</html>
"""

# --- ユーティリティ関数 ---

def indent_paragraphs(text: str) -> str:
    """
    テキストの先頭および改行直後に、
    直後の文字が特定文字に該当しない場合に全角スペースを挿入して字下げを行います。
    """
    # (^|\n) のキャプチャを保持しつつ、その直後に全角スペースを挿入
    return _INDENT_PATTERN.sub(r"\1　", text)


def full_to_half(text: str) -> str:
    """全角文字を半角文字に変換する関数"""
    return text.translate(_TRANSLATE_TABLE)


def load_cookies_and_ua(input_file: str):
    """
    cookieファイルを読み込み、{name: value} な dict と UA を返す。
    """
    data = _load_json_safe(input_file)
    if not data:
        logging.warning(f"Failed to load cookies from {input_file}, returning empty")
        return {}, None

    ua = data.get('user_agent') or data.get('ua')
    cookies_raw = data.get('cookies', {})

    cookies_dict = {}
    if isinstance(cookies_raw, dict):
        cookies_dict = cookies_raw.copy()
    elif isinstance(cookies_raw, list):
        for c in cookies_raw:
            if isinstance(c, dict):
                name = c.get('name')
                value = c.get('value')
                if name is not None and value is not None:
                    cookies_dict[name] = value
    else:
        raise TypeError(f"Unsupported cookies format: {type(cookies_raw)}")

    return cookies_dict, ua


def save_cookies_and_ua(output_file: str, cookies: dict, ua: str):
    """Cookie とユーザーエージェントを保存する"""
    _save_json(output_file, {'cookies': cookies, 'user_agent': ua})


# --- ファイル操作・画像処理関連 ---

def _load_json_safe(path: str, create_backup: bool = True, max_backups: int = 3) -> Dict:
    """
    JSON読み込みヘルパー（安全版）
    
    Args:
        path: JSONファイルパス
        create_backup: 破損時にバックアップを作成するか
        max_backups: 保持する最大バックアップ数
    
    Returns:
        JSONデータ（失敗時は空辞書）
    """
    if not os.path.exists(path):
        return {}
    
    # バックアップからの復元試行
    def try_restore_from_backup():
        if not create_backup:
            return {}
        
        backup_dir = os.path.dirname(path)
        backup_name = os.path.basename(path)
        
        # .backup.N 形式のバックアップを探す
        for i in range(1, max_backups + 1):
            backup_path = os.path.join(backup_dir, f"{backup_name}.backup.{i}")
            if os.path.exists(backup_path):
                try:
                    with open(backup_path, 'r', encoding='utf-8') as f:
                        data = json.load(f)
                    logging.info(f"Restored from backup: {backup_path}")
                    # 復元成功したらメインファイルを上書き（破損データは .corrupt に保存済み）
                    shutil.copy2(backup_path, path)
                    return data
                except:
                    continue
        return {}
    
    try:
        with open(path, 'r', encoding='utf-8') as f:
            return json.load(f)
    except json.JSONDecodeError as e:
        logging.error(f"JSONDecodeError: {path} is corrupted at line {e.lineno}: {e.msg}")
        
        if create_backup:
            # 破損ファイルを .corrupt として保存
            corrupt_path = f"{path}.corrupt"
            try:
                shutil.copy2(path, corrupt_path)
                logging.info(f"Corrupted file saved to: {corrupt_path}")
            except Exception as backup_err:
                logging.error(f"Failed to save corrupted file: {backup_err}")
        
        # バックアップからの復元を試行
        restored_data = try_restore_from_backup()
        if restored_data:
            return restored_data
        
        logging.warning(f"No valid backup found for {path}, returning empty dict")
        return {}
    
    except Exception as e:
        logging.error(f"Failed to load JSON {path}: {e}")
        
        # バックアップからの復元を試行
        restored_data = try_restore_from_backup()
        if restored_data:
            return restored_data
        
        return {}


def _save_json(path: str, data: Dict, create_backup: bool = True, max_backups: int = 3):
    """
    JSON保存ヘルパー（アトミック書き込み + バックアップローテーション）
    
    Args:
        path: JSONファイルパス
        data: 保存するデータ
        create_backup: バックアップを作成するか
        max_backups: 保持する最大バックアップ数
    """
    dir_name = os.path.dirname(path)
    if dir_name and not os.path.exists(dir_name):
        os.makedirs(dir_name, exist_ok=True)
    
    # 既存ファイルのバックアップ作成
    if create_backup and os.path.exists(path):
        backup_name = os.path.basename(path)
        backup_dir = os.path.dirname(path)
        
        # バックアップをローテーション (.backup.3 -> .backup.4, .backup.2 -> .backup.3, ...)
        for i in range(max_backups, 1, -1):
            old_backup = os.path.join(backup_dir, f"{backup_name}.backup.{i-1}")
            new_backup = os.path.join(backup_dir, f"{backup_name}.backup.{i}")
            if os.path.exists(old_backup):
                try:
                    shutil.move(old_backup, new_backup)
                except Exception as e:
                    logging.warning(f"Failed to rotate backup {old_backup}: {e}")
        
        # 現在のファイルを .backup.1 として保存
        try:
            first_backup = os.path.join(backup_dir, f"{backup_name}.backup.1")
            shutil.copy2(path, first_backup)
        except Exception as e:
            logging.warning(f"Failed to create backup for {path}: {e}")
    
    # アトミック書き込み
    tmp_path = path + ".tmp"
    try:
        with open(tmp_path, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=4)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp_path, path)
    except Exception as e:
        logging.error(f"Failed to write JSON {path}: {e}")
        if os.path.exists(tmp_path):
            try:
                os.remove(tmp_path)
            except:
                pass
        raise


def check_image_file(img_path: str, file_name: str) -> Optional[str]:
    """
    database.json を参照し、同名のファイルが存在するかチェックする。
    """
    db_path = os.path.join(img_path, 'database.json')
    
    # DB初期化
    if not os.path.exists(db_path):
        _save_json(db_path, {})

    database = _load_json_safe(db_path)
    base_name = file_name.split('.')[0]
    base_url = f'https://{read_domain_settings()}/images/'
    
    for key, value in database.items():
        if key == file_name or key.split('.')[0] == base_name:
            # 拡張子を元のキーから取得して構築
            target_ext = os.path.splitext(key)[1]
            check_path = os.path.join(img_path, value + target_ext)
            check_url = base_url + value + target_ext
            
            # ローカルチェック
            if os.path.exists(check_path):
                return value + target_ext
            
            # リモートチェック (通信エラー時はNone扱いにして再DLへ)
            try:
                if requests.head(check_url, timeout=5).status_code == 200:
                    return value + target_ext
            except requests.RequestException:
                pass 

    return None

def get_root_path() -> str:
    """プロジェクトのルートパスを取得"""
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def read_domain_settings() -> str:
    # setting.ini からドメイン設定を読み込む
    import configparser
    config = configparser.ConfigParser()
    ini_path = os.path.join(get_root_path(), 'setting', 'setting.ini')
    
    config.read(ini_path, encoding='utf-8')
    # [server] セクションの domain キーを取得
    return config.get('server', 'domain', fallback='localhost')

def check_image_hash(img_path: str, file_data: bytes, file_name: str, is_cover: bool = False) -> str:
    """
    画像データのハッシュを計算し、重複があれば既存のハッシュを、なければ新規ハッシュを返す。
    is_cover=True の場合は cover.json も同期する。
    """
    db_path = os.path.join(img_path, 'database.json')
    cover_path = os.path.join(img_path, 'cover.json')

    # メインDB読み込み（なければ作成）
    if not os.path.exists(db_path):
        _save_json(db_path, {})
    database = _load_json_safe(db_path)

    # カバー用DB読み込み
    cover_db = {}
    if is_cover:
        if not os.path.exists(cover_path):
            _save_json(cover_path, {})
        cover_db = _load_json_safe(cover_path)
        
        # すでにcover.jsonにある場合
        if file_name in cover_db:
            image_hash = cover_db[file_name]
            # database.json と同期
            if database.get(file_name) != image_hash:
                database[file_name] = image_hash
                _save_json(db_path, database)
            return image_hash

    # ハッシュ計算
    image_hash = base64.urlsafe_b64encode(
        hashlib.sha3_256(file_data).digest()
    ).rstrip(b'=').decode('utf-8')

    # database.json 内でハッシュ重複チェック
    # (値が同じ＝画像が同じなので、既存のハッシュを採用して別名保存扱いにする)
    existing_hash = None
    for val in database.values():
        if val == image_hash:
            existing_hash = val
            break
    
    final_hash = existing_hash if existing_hash else image_hash
    
    # DB更新
    database[file_name] = final_hash
    _save_json(db_path, database)

    if is_cover:
        cover_db[file_name] = final_hash
        _save_json(cover_path, cover_db)

    return final_hash


def safe_fromiso(date_str: Optional[str], tzinfo=JST) -> Optional[datetime]:
    """ISO フォーマット文字列を安全に datetime に変換"""
    if not date_str:
        logging.warning("safe_fromiso: empty or None date_str received")
        return None
    try:
        dt = datetime.fromisoformat(date_str)
        if tzinfo is not None:
            return dt.astimezone(tzinfo)
        return dt
    except ValueError:
        logging.warning(f"safe_fromiso: invalid ISO format: {date_str}")
        return None


# --- その他ユーティリティ ---

def find_key_recursively(data: Union[Dict, List], target_key: str) -> Any:
    """再帰的にキーを探す"""
    if isinstance(data, dict):
        for key, value in data.items():
            if key == target_key:
                return value
            if isinstance(value, (dict, list)):
                result = find_key_recursively(value, target_key)
                if result is not None:
                    return result
    elif isinstance(data, list):
        for item in data:
            result = find_key_recursively(item, target_key)
            if result is not None:
                return result
    return None


def get_with_cookie(url: str, cookie: dict, header: dict, retries: int = 5, delay: int = 5) -> Optional[requests.Response]:
    """クッキーを使ってGETリクエストを送信 (リトライ機能付き)"""
    response = None
    for i in range(retries):
        try:
            response = requests.get(url, cookies=cookie, headers=header, timeout=10)
            response.raise_for_status()
            return response
        except (ConnectionError, Timeout) as e:
            logging.error(f"\nError: {e}. Retrying in {delay * (2 ** i)} seconds...")
        except RequestException as e:
            # 404エラーの場合は即時終了
            if response is not None and response.status_code == 404:
                logging.error("\n404 Error: Resource not found.")
                return response
            else:
                logging.error(f"\nError: {e}. Retrying in {delay * (2 ** i)} seconds...")
        
        if i < retries - 1:
            time.sleep(delay * (2 ** i))
    
    logging.error("\nThe retry limit has been reached. No response received.")
    return response


def convert_keys_to_str(d: Any) -> Any:
    """辞書のキーをすべて文字列に変換する"""
    if isinstance(d, dict):
        return {str(k): convert_keys_to_str(v) for k, v in d.items()}
    elif isinstance(d, list):
        return [convert_keys_to_str(i) for i in d]
    else:
        return d


def save_raw_diff(raw_path: str, novel_path: str, novel: dict):
    """小説データに差分があるなら保存"""
    old_json = _load_json_safe(raw_path)
    if not old_json:
        return
    
    # jsondiff用に一度ダンプしてロードし直す (型を揃えるため)
    old_obj = json.loads(json.dumps(old_json))
    new_obj = json.loads(json.dumps(novel))
    
    diff_json = convert_keys_to_str(diff(new_obj, old_obj))
    
    # get_date のみの差分なら無視
    if len(diff_json) == 1 and 'get_date' in diff_json:
        return
        
    if diff_json:
        # ファイル名に使えない文字を置換
        date_str = str(old_json.get("get_date", "unknown")).replace(":", "-").replace(" ", "_")
        diff_filename = f'diff_{date_str}.json'
        
        # rawフォルダ配下に保存 (novel_path直下のrawフォルダを想定)
        save_dir = os.path.join(novel_path, 'raw')
        if not os.path.exists(save_dir):
            os.makedirs(save_dir, exist_ok=True)

        diff_path = os.path.join(save_dir, diff_filename)
        _save_json(diff_path, diff_json)


def make_dir(id_str, folder_path):
    """ベースフォルダとサブフォルダ(raw, info)を作成"""
    full_path = os.path.join(folder_path, str(id_str))
    os.makedirs(os.path.join(full_path, 'raw'), exist_ok=True)
    os.makedirs(os.path.join(full_path, 'info'), exist_ok=True)


def gen_site_index(folder_path: str, key_data, site_name: str):
    """サイトのインデックス(index.html, index.json)を生成"""
    subfolders = [f for f in os.listdir(folder_path) if os.path.isdir(os.path.join(folder_path, f))]
    pairs = {}
    no_raw = []

    for folder in subfolders:
        json_path = os.path.join(folder_path, folder, 'raw', 'raw.json')
        if not os.path.exists(json_path):
            # raw.jsonが無いフォルダは削除対象
            shutil.rmtree(os.path.join(folder_path, folder))
            no_raw.append(folder)
            continue

        data = _load_json_safe(json_path)
        if not data:
            logging.error(f"Failed to load {json_path}, skipping folder {folder}")
            continue

        # エピソードデータの整形
        episodes_data = {}
        raw_episodes = data.get('episodes', {})
        if isinstance(raw_episodes, dict):
            for key, value in raw_episodes.items():
                episodes_data[key] = {
                    'title': value.get('title', 'No title found'),
                    'id': value.get('id', 'No id found'),
                    'caption': unquote(value.get('caption', 'No caption found')),
                    'tags': value.get('tags', 'No tags found')
                }

        # データの抽出・構築
        pairs[folder] = {
            'title': data.get('title', 'No title found'),
            'author': data.get('author', 'No author found'),
            'author_id': data.get('author_id', 'No author_id found'),
            'author_url': data.get('author_url', 'No author_url found'),
            'type': data.get('type', 'No type found'),
            'serialization': data.get('serialization', 'No serialization found'),
            'tags': data.get('tags', 'No tags found'),
            'all_tags': data.get('all_tags', 'No all_tags found'),
            'caption': unquote(data.get('caption', 'No caption found')),
            'create_date': data.get('createDate', 'No create date found'),
            'update_date': data.get('updateDate', 'No update date found'),
            'episodes_data': episodes_data
        }

    # 作者名でソート
    pairs = dict(sorted(pairs.items(), key=lambda item: item[1]['author']))

    # index.html の生成 (テンプレートを使用)
    html_content = _HTML_TEMPLATE.format(site_name=site_name)
    with open(os.path.join(folder_path, 'index.html'), 'w', encoding='utf-8') as f:
        f.write(html_content)

    # index.json の生成
    index_json_path = os.path.join(folder_path, 'index.json')
    _save_json(index_json_path, pairs)

    if no_raw:
        logging.warning(f"The folders {', '.join(no_raw)} were deleted because they do not contain 'raw.json'.")

    logging.info('目次の生成が完了しました')