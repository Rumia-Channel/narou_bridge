import os
import json
from jsondiff import diff
import requests
from requests.exceptions import RequestException, ConnectionError, Timeout
import time

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
            print(f"\nError: {e}. Retrying in {delay * (2 ** i)} seconds...")
        except RequestException as e:
            # 404エラーを特別扱い
            if response.status_code == 404:
                print("\n404 Error: Resource not found.")
                return None  # 404エラーの場合はリトライしない
            else:
                print(f"\nError: {e}. Retrying in {delay * (2 ** i)} seconds...")
        
        if i < retries - 1:
            time.sleep(delay * (2 ** i))  # 指数バックオフ
        else:
            print("\nThe retry limit has been reached. No response received.。")
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
