import re
import os
import requests
import json
from playwright.sync_api import Playwright, sync_playwright, expect
from html import unescape
from bs4 import BeautifulSoup
from datetime import datetime
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
        'Connection': 'keep-alive',
        'Referer': 'https://www.pixiv.net/',
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

# レスポンスからjsonデータ(本文データ)を返却
def return_content_json(response):
    soup = BeautifulSoup(str(response.text), 'html.parser')
    meta_tag = soup.find_all('meta', {'id': 'meta-preload-data'})
    # meta_tagがリストである場合、最初の要素を取得
    if isinstance(meta_tag, list):
        meta_tag = meta_tag[0]

    # content属性の値を取得
    content_value = meta_tag['content']

    # HTMLエンティティをデコード
    decoded_content = unescape(content_value)
    json_data = json.loads(decoded_content)
    return json_data

#アンケートの整形
def format_survey(survey):
    # アンケートの質問と総票数を取得
    question = survey['question']
    total_votes = survey['total']
    
    # 結果の初期化
    result = f'アンケート　"{question}"　　票数{total_votes}票\n\n'
    
    # 各選択肢とその票数を追加
    for choice in survey['choices']:
        result += f'　　　{choice["text"]}　　{choice["count"]}票\n'
    
    return result

#ベースフォルダ作成
def make_dir(id):
    if not os.path.exists(id):
        os.makedirs(id)
    if not os.path.exists(f'{id}/raw'):
        os.makedirs(f'{id}/raw')
    if not os.path.exists(f'{id}/images'):
        os.makedirs(f'{id}/images')

def md_id(id, folder_path, title):
    # ディレクトリが存在しない場合は作成
    full_path = os.path.join(folder_path, id)
    raw_path = os.path.join(full_path, 'raw')
    os.makedirs(raw_path, exist_ok=True)
    # index.htmlの更新
    index_file(folder_path, id, title)
    return raw_path

#画像フォーマットの整形
def format_image(data):
    pass

#シリーズのダウンロードに関する処理
def dl_series(series_id, folder_path):
    # seriesNavDataの内部にあるseriesIdを取得
    print(f"Series ID: {series_id}")
    s_detail = find_key_recursively(json.loads(get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}").text), "body")
    s_contents = get_with_cookie(f"https://www.pixiv.net/ajax/novel/series_content/{series_id}")
    series_title = s_detail.get('title')
    series_authour = s_detail.get('userName')
    series_authour_id = s_detail.get('userId')
    series_episodes = s_detail.get('total')
    series_chara = s_detail.get('publishedTotalCharacterCount')
    series_caption_data = find_key_recursively(s_detail, 'caption')
    series_create_day = datetime.fromisoformat(s_detail.get('createDate'))
    series_update_day = datetime.fromisoformat(s_detail.get('updateDate'))
    if series_caption_data:
        series_caption = series_caption_data
    else:
        series_caption = ''
    print(f"Series Title: {series_title}")
    print(f"Series Authour: {series_authour}")
    print(f"Series Authour ID: {series_authour_id}")
    print(f"Series Caption: {series_caption}")
    print(f"Series Total Episodes: {series_episodes}")
    print(f"Series Total Characters: {series_chara}")
    print(f"Series Create Date: {series_create_day}")
    print(f"Series Update Date: {series_update_day}")
    make_dir(series_id)
    con_json_data = json.loads(s_contents.text)
    novel_datas = find_key_recursively(con_json_data, 'novel')
    episode = {}
    total_text = 0
    for i, entry in enumerate(reversed(novel_datas), 1):
        json_data = return_content_json(get_with_cookie(f"https://www.pixiv.net/novel/show.php?id={entry['id']}"))
        introduction = find_key_recursively(json_data, entry['id']).get('description').replace('<br />', '\n')
        postscript = find_key_recursively(json_data, entry['id']).get('pollData')
        text = find_key_recursively(json_data, entry['id']).get('content').replace('\r\n', '\n')
        if postscript:
            postscript = format_survey(postscript)
            #print(postscript)
        else:
            postscript = ''
        if not introduction:
            introduction = ''
        #挿絵リンクへの置き換え

        episode[i] = {
            'id' : entry['id'],
            'title': entry['title'],
            'introduction': introduction,
            'text': text,
            'postscript': postscript,
            'createDate': datetime.strptime(entry['createDate'], "%Y-%m-%dT%H:%M:%S%z").strftime("%Y/%m/%d %H:%M"),
            'updateDate': datetime.strptime(entry['updateDate'], "%Y-%m-%dT%H:%M:%S%z").strftime("%Y/%m/%d %H:%M")
        }
        total_text += int(entry['textCount'])
    
    novel = {
        'title': series_title,
        'authour': series_authour,
        'authour_url': f"https://www.pixiv.net/users/{series_authour_id}",
        'caption': series_caption,
        'total_episodes': len(episode),
        'all_episodes': series_episodes,
        'total_characters': total_text,
        'all_characters': series_chara,
        'type': '連載中',
        'createDate': series_create_day.strftime("%Y年 %m月%d日 %H時%M分"),
        'updateDate': series_update_day.strftime("%Y年 %m月%d日 %H時%M分"),
        'episodes': episode
    }

    with open('test_b.json', 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)


#ダウンロード処理
def download(url, folder_path):
    
    response = get_with_cookie(url)

    if response.status_code == 404:
        print("404 Not Found")
        print("Incorrect URL, Deleted, Private, or My Pics Only.")
        return
    print(f'Response Status Code: {response.status_code}')
    if "https://www.pixiv.net/novel/show.php?id=" in url:
        # JSONとして解析
        json_data = return_content_json(response)
        series_nav_data = find_key_recursively(json_data, "seriesNavData")
        if series_nav_data:
            series_id = series_nav_data.get("seriesId")
            dl_series(series_id, folder_path)
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
        series_id = re.search(r"series/(\d+)", url).group(1)
        dl_series(series_id, folder_path)
    else:
        json_data = get_with_cookie(url).json()
        urls_data = find_key_recursively(json_data, 'body')
        with open('test_c.json', 'w', encoding='utf-8') as f:
            json.dump(urls_data, f, ensure_ascii=False, indent=4)
        i_n = 1
        for i in urls_data:
            image_url = find_key_recursively(i, 'urls').get('original')
            image = get_with_cookie(image_url)
            with open(os.path.join(folder_path, f'{i_n}.png'), 'wb') as f:
                f.write(image.content)
            i_n += 1