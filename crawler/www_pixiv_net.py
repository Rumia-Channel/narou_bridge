import re
import os
import requests
import json
from playwright.sync_api import Playwright, sync_playwright, expect
from html import unescape
from bs4 import BeautifulSoup
import gzip
import zlib
from io import BytesIO
#リキャプチャ対策
import time
import random

#indexファイルの処理
def index_file(folder_path, id, title):
    index_path = os.path.join(folder_path, 'index.html')
    if not os.path.exists(index_path):
        with open(index_path, 'w', encoding='utf-8') as f:
            f.write('<!DOCTYPE html>\n')
            f.write('<html lang="ja">\n')
            f.write('<head>\n')
            f.write('<meta charset="UTF-8">\n')
            f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
            f.write('<title>Index Pixiv</title>\n')
            f.write('<script>function redirectWithParams(baseURL) {var params = document.location.search; var newURL = baseURL + params; window.location.href = newURL;}</script>\n')
            f.write('</head>\n')
            f.write('<body>\n')
            f.write('<a href="#" onclick="redirectWithParams(\'../\')">戻る</a>\n')
            f.write('<div id="novel">\n')
            f.write('</div>\n')
            f.write('</body>\n')
            f.write('</html>\n')
    
    # BeautifulSoupを使ってHTMLを解析
    with open(index_path, 'r', encoding='utf-8') as f:
        soup = BeautifulSoup(f, 'html.parser')
    
    novel_div = soup.find('div', id='novel')
    
    # すでに同じidを持つ<a>タグが存在するか確認
    existing_link = novel_div.find('a', id=id)
    
    if not existing_link:
        # 新しいデータを追加
        new_link = soup.new_tag('a', id=id, href="#", onclick=f"redirectWithParams('{id}/')")
        new_link.string = title
        novel_div.append(new_link)
        novel_div.append(soup.new_tag('br'))
    
    # 変更をファイルに書き出し
    with open(index_path, 'w', encoding='utf-8') as f:
        f.write(str(soup))

# クッキーを読みこみ
def load_cookies_from_json(input_file):
    with open(input_file, 'r', encoding='utf-8') as f:
        cookies = json.load(f)

    # requests用にCookieを変換
    cookies_dict = {cookie['name']: cookie['value'] for cookie in cookies}
    return cookies_dict

#初期化処理
def init(folder_path):

    cookie_path = os.path.join(folder_path, 'cookie.json')
    ua_path = os.path.join(folder_path, 'ua.txt')

    def login(playwright: Playwright) -> None:
        browser = playwright.chromium.launch(headless=True)
        context = browser.new_context()
        page = context.new_page()
        page.set_viewport_size({'width': 1280, 'height': 1280})
        page.goto("https://www.pixiv.net/")
        time.sleep(random.uniform(1,5))
        page.get_by_role("link", name="ログイン").click()
        mail = input("メールアドレスを入力してください: ")
        page.get_by_placeholder("メールアドレスまたはpixiv ID").fill(mail)
        pswd = input("パスワードを入力してください: ")
        page.get_by_placeholder("パスワード").fill(pswd)
        time.sleep(random.uniform(2,5))
        page.get_by_role("button", name="ログイン", exact=True).click()
        # リダイレクトが完全に終わるまで待つ
        def wait_for_redirects(page):
            while True:
                page.wait_for_load_state("load")
                if page.url.startswith("https://accounts.pixiv.net/login/two-factor-authentication?") or page.url == "https://www.pixiv.net/":    
                    break
        wait_for_redirects(page)
        if "two-factor-authentication" in page.url:
            time.sleep(random.uniform(1,5))
            page.get_by_label("このブラウザを信頼する").check()
            tfak = input("2段階認証のコードを入力してください: ")
            page.get_by_placeholder("確認コード").fill(tfak)
            time.sleep(random.uniform(1,5))
            page.get_by_role("button", name="ログイン").click()
            def wait_for_redirects_2fa(page):
                while True:
                    page.wait_for_load_state("load")
                    if page.url == "https://www.pixiv.net/":
                        break
            wait_for_redirects_2fa(page)

        cookies = context.cookies()
        user_agent = page.evaluate("() => navigator.userAgent")
        with open(cookie_path, 'w', encoding='utf-8') as f:
            json.dump(cookies, f)
        with open(ua_path, 'w', encoding='utf-8') as f:
            f.write(user_agent)
        page.close()

        # ---------------------
        context.close()
        browser.close()


    # cookieの有無とログイン状態を確認
    if not os.path.isfile(cookie_path) or not os.path.isfile(ua_path) or bool(requests.get('https://www.pixiv.net/dashboard', cookies=load_cookies_from_json(cookie_path), headers={'User-Agent': open(ua_path, 'r', encoding='utf-8').read()}).history):
        with sync_playwright() as playwright:
            login(playwright)

    #クッキーとユーザーエージェントをグローバルで宣言(ユニーク_cookie などの形式)
    global pixiv_cookie
    global pixiv_header

    pixiv_cookie = load_cookies_from_json(cookie_path)
    ua = open(ua_path, 'r', encoding='utf-8').read()
    pixiv_header = {
        'User-Agent': ua,
        'Accept-Language': 'ja,en-US;q=0.9,en;q=0.8',
        'Accept-Encoding': 'gzip, deflate, br',
        'Connection': 'keep-alive'
    }

# 再帰的にキーを探す
def find_key_recursively(data, target_key):
    if isinstance(data, dict):
        for key, value in data.items():
            if key == target_key:
                return value
            elif isinstance(value, (dict, list)):
                result = find_key_recursively(value, target_key)
                if result is not None:
                    return result
    elif isinstance(data, list):
        for item in data:
            result = find_key_recursively(item, target_key)
            if result is not None:
                return result
    return None

# クッキーを使ってGETリクエストを送信
def get_with_cookie(url):
    response = requests.get(url, cookies=pixiv_cookie, headers=pixiv_header)
    return response

def md_id(id, folder_path, title):
    # ディレクトリが存在しない場合は作成
    full_path = os.path.join(folder_path, id)
    raw_path = os.path.join(full_path, 'raw')
    os.makedirs(raw_path, exist_ok=True)
    # index.htmlの更新
    index_file(folder_path, id, title)
    return raw_path

def dl_series(series_nav_data, folder_path):
    # seriesNavDataの内部にあるseriesIdを取得
    print(series_nav_data)
    series_id = series_nav_data.get("seriesId")
    print(f"Series ID: {series_id}")
    series_title = series_nav_data.get("title")
    print(f"Series Title: {series_title}")
    response = get_with_cookie(f"https://www.pixiv.net/novel/series/{series_id}")
    with open('test.html', 'w', encoding='utf-8') as f:
            f.write(response.text)
    print(f"Response Status Code: {response.status_code}")
    print(f"Response Content: {response.text}")


#ダウンロード処理
def download(url, folder_path):
    
    response = get_with_cookie(url)

    if response.status_code == 404:
        print("404 Not Found")
        print("Incorrect URL, Deleted, Private, or My Pics Only.")
        return
    
    if "https://www.pixiv.net/novel/show.php?id=" in url:
        soup = BeautifulSoup(str(response.text), 'html.parser')
        meta_tag = soup.find_all('meta', {'id': 'meta-preload-data'})
        # meta_tagがリストである場合、最初の要素を取得
        if isinstance(meta_tag, list):
            meta_tag = meta_tag[0]

        # content属性の値を取得
        content_value = meta_tag['content']

        # HTMLエンティティをデコード
        decoded_content = unescape(content_value)
        # JSONとして解析
        json_data = json.loads(decoded_content)
        series_nav_data = find_key_recursively(json_data, "seriesNavData")
        if series_nav_data:
            dl_series(series_nav_data, folder_path)
        else:
            novel_id = re.search(r"id=(\d+)", url).group(1)
            print(f"Novel ID: {novel_id}")
            novel_title = find_key_recursively(json_data, f"{novel_id}").get("title")
            print(f"Novel Title: {novel_title}")

            # ディレクトリが存在しない場合は作成
            raw_path = md_id(novel_id, folder_path, novel_title)
            
            # 生データの書き出し
            with open(os.path.join(raw_path, f'{novel_id}.json'), 'w', encoding='utf-8') as f:
                json.dump(json_data, f, ensure_ascii=False, indent=4)
    elif "https://www.pixiv.net/novel/series/" in url:
        pass