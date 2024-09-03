import re
import os
import requests
from urllib.parse import unquote
import json
from playwright.sync_api import Playwright, sync_playwright, expect
from html import unescape
from bs4 import BeautifulSoup
from datetime import datetime

from . import convert_narou as cn

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
def return_content_json(novelid):
    novel_data = get_with_cookie(f"https://www.pixiv.net/ajax/novel/{novelid}").text
    json_data = json.loads(unescape(novel_data))
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
def make_dir(id, folder_path, series):
    if series:
        full_path = os.path.join(folder_path, f'{id}_s')
    else:
        full_path = os.path.join(folder_path, f'{id}_n')
    if not os.path.exists(full_path):
        os.makedirs(full_path)
    if not os.path.exists(f'{full_path}/raw'):
        os.makedirs(f'{full_path}/raw')
    if not os.path.exists(f'{full_path}/info'):
        os.makedirs(f'{full_path}/info')

def md_id(id, folder_path, title):
    # ディレクトリが存在しない場合は作成
    full_path = os.path.join(folder_path, str(id))
    raw_path = os.path.join(full_path, 'raw')
    os.makedirs(raw_path, exist_ok=True)
    # index.htmlの更新
    index_file(folder_path, id, title)
    return raw_path

#ルビ形式の整形
def format_ruby(data):
    pattern = re.compile(r"\[\[rb:(.*?) > (.*?)\]\]")
    return re.sub(pattern, lambda match: f'[ruby:<{match.group(1)}>({match.group(2)})]', data)

#画像リンク形式の整形
def format_image(id, episode, series, data, json_data, folder_path):
    #pixivimage: で始まるリンクの抽出
    links = re.findall(r"\[pixivimage:(\d+)-(\d+)\]", data)
    link_dict = {}
    #uploadedimage: で始まるリンクの抽出
    inner_links = re.findall(r"\[uploadedimage:(\d+)\]", data)

    #シリーズとその他のリンクの切り替え
    if series:
        episode_path = os.path.join(folder_path, f'{id}_s', str(episode))
    else:
        episode_path = os.path.join(folder_path, f'{id}_n')

    for i in links:
        art_id = i[0]
        img_num = i[1]
        if art_id not in link_dict:
            link_dict[art_id] = []  # art_id が存在しない場合に空のリストを初期化
        link_dict[art_id].append(img_num)
    #画像リンクの形式を[リンク名](リンク先)に変更
    for art_id, img_nums in link_dict.items():
        illust_json = get_with_cookie(f"https://www.pixiv.net/ajax/illust/{art_id}/pages").json()
        illust_datas = find_key_recursively(illust_json, 'body')
        for index, i in enumerate(illust_datas):
            if str(index + 1) in img_nums:
                img_url = i.get('urls').get('original')
                img_data = get_with_cookie(img_url)
                with open(os.path.join(episode_path, f'{art_id}_p{index}{os.path.splitext(img_url)[1]}'), 'wb') as f:
                    f.write(img_data.content)
                data = data.replace(f'[pixivimage:{art_id}-{index + 1}]', f'[image]({art_id}_p{index}{os.path.splitext(img_url)[1]})')
    #小説内アップロードの画像リンクの形式を[リンク名](リンク先)に変更
    for inner_link in inner_links:
        in_img_url = find_key_recursively(json_data, inner_link).get('urls').get('original')
        in_img_data = get_with_cookie(in_img_url)
        with open(os.path.join(episode_path, f'{inner_link}{os.path.splitext(in_img_url)[1]}'), 'wb') as f:
            f.write(in_img_data.content)
        data = data.replace(f'[uploadedimage:{inner_link}]', f'[image]({inner_link}{os.path.splitext(in_img_url)[1]})')

    return data

#チャプタータグの除去
def remove_chapter_tag(data):
    pattern = re.compile(r"\[chapter:(.*?)\]")
    return re.sub(pattern, lambda match: f'[newpage]{match.group(1)}\n\n\n', data)

#シリーズのダウンロードに関する処理
def dl_series(series_id, folder_path, key_data):
    # seriesNavDataの内部にあるseriesIdを取得
    print(f"Series ID: {series_id}")
    s_detail = find_key_recursively(json.loads(get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}").text), "body")
    s_toc = get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles")
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
    make_dir(series_id, folder_path, True)
    toc_json_data = json.loads(s_toc.text)
    novel_toc = toc_json_data.get('body')
    episode = {}
    total_text = 0
    for i, entry in enumerate(reversed(novel_toc), 1):
        if not entry['available']:
            continue
        json_data = return_content_json(entry['id'])
        introduction = find_key_recursively(json_data, 'body').get('description').replace('<br />', '\n').replace('jump.php?', '')
        postscript = find_key_recursively(json_data, 'body').get('pollData')
        text = find_key_recursively(json_data, 'body').get('content').replace('\r\n', '\n')
        if postscript:
            postscript = format_survey(postscript)
            #print(postscript)
        else:
            postscript = ''
        if not introduction:
            introduction = ''
        
        #エピソードごとのフォルダの作成
        os.makedirs(os.path.join(folder_path, f'{series_id}_s', entry['id']), exist_ok=True)
        #挿絵リンクへの置き換え
        text = format_image(series_id, entry['id'], True, text, json_data, folder_path)
        #ルビの置き換え
        text = format_ruby(text)
        #チャプタータグの除去
        text = remove_chapter_tag(text)
        episode[i] = {
            'id' : entry['id'],
            'title': entry['title'],
            'introduction': unquote(introduction),
            'text': text,
            'postscript': postscript,
            'createDate': datetime.strptime(json_data.get('body').get('createDate'), "%Y-%m-%dT%H:%M:%S%z").strftime("%Y/%m/%d %H:%M"),
            'updateDate': datetime.strptime(json_data.get('body').get('uploadDate'), "%Y-%m-%dT%H:%M:%S%z").strftime("%Y/%m/%d %H:%M")
        }
        total_text += int(find_key_recursively(json_data, 'body').get('characterCount'))

    # 作成日で並び替え
    episode = dict(sorted(episode.items(), key=lambda x: x[1]['createDate']))
    
    total_charactors = str(f'{total_text:,}')
    all_charactors = str(f'{series_chara:,}')
    novel = {
        'title': series_title,
        'id': series_id,
        'url': f"https://www.pixiv.net/novel/series/{series_id}",
        'authour': series_authour,
        'authour_url': f"https://www.pixiv.net/users/{series_authour_id}",
        'caption': series_caption,
        'total_episodes': len(episode),
        'all_episodes': series_episodes,
        'total_characters': total_charactors,
        'all_characters': all_charactors,
        'type': '連載中',
        'createDate': series_create_day.strftime("%Y年 %m月%d日 %H時%M分"),
        'updateDate': series_update_day.strftime("%Y年 %m月%d日 %H時%M分"),
        'episodes': episode
    }

    with open(os.path.join(folder_path, f'{series_id}_s', 'raw', 'raw.json'), 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)

    cn.narou_gen(novel, os.path.join(folder_path, f'{series_id}_s'), key_data)

#短編のダウンロードに関する処理
def dl_novel(json_data, novel_id, folder_path, key_data):
    novel_data = find_key_recursively(json_data, novel_id)
    novel_title = novel_data.get('title')
    novel_authour = novel_data.get('userName')
    novel_authour_id = novel_data.get('userId')
    novel_caption = find_key_recursively(novel_data, 'caption').replace('<br />', '\n').replace('jump.php?', '')
    if novel_caption:
        novel_caption = novel_caption
    else:
        novel_caption = ''
    novel_text = novel_data.get('content').replace('\r\n', '\n')
    novel_postscript = find_key_recursively(novel_data, 'pollData')
    if novel_postscript:
        novel_postscript = format_survey(novel_postscript)
    else:
        novel_postscript = ''
    novel_create_day = datetime.fromisoformat(novel_data.get('createDate'))
    novel_update_day = datetime.fromisoformat(novel_data.get('updateDate'))
    print(f"Novel ID: {novel_id}")
    print(f"Novel Title: {novel_title}")
    print(f"Novel Authour: {novel_authour}")
    print(f"Novel Authour ID: {novel_authour_id}")
    print(f"Novel Caption: {novel_caption}")
    print(f"Novel Create Date: {novel_create_day}")
    print(f"Novel Update Date: {novel_update_day}")
    make_dir(novel_id, folder_path, False)
    text = format_image(novel_id, novel_id, False, novel_text, json_data, folder_path)
    episode = {}
    episode[1] = {
        'id' : novel_id,
        'title': novel_title,
        'introduction': unquote(novel_caption),
        'text': text,
        'postscript': novel_postscript,
        'createDate': datetime.strptime(novel_create_day, "%Y-%m-%dT%H:%M:%S%z").strftime("%Y/%m/%d %H:%M"),
        'updateDate': datetime.strptime(novel_update_day, "%Y-%m-%dT%H:%M:%S%z").strftime("%Y/%m/%d %H:%M")
    }


#ダウンロード処理
def download(url, folder_path, key_data):
    
    response = get_with_cookie(url)

    if response.status_code == 404:
        print("404 Not Found")
        print("Incorrect URL, Deleted, Private, or My Pics Only.")
        return
    print(f'Response Status Code: {response.status_code}')
    if "https://www.pixiv.net/novel/show.php?id=" in url:
        # JSONとして解析
        novel_id = re.search(r"id=(\d+)", url).group(1)
        json_data = return_content_json(novel_id)
        series_nav_data = find_key_recursively(json_data, "seriesNavData")
        if series_nav_data:
            series_id = series_nav_data.get("seriesId")
            dl_series(series_id, folder_path, key_data)
        else:
            #dl_novel(json_data, novel_id, folder_path, key_data) #ダウンロード処理
            print(f"Novel ID: {novel_id}")
            novel_title = find_key_recursively(json_data, "body").get("title")
            print(f"Novel Title: {novel_title}")

            # ディレクトリが存在しない場合は作成
            raw_path = md_id(novel_id, folder_path, novel_title)
            
            # 生データの書き出し
            with open(os.path.join(raw_path, f'{novel_id}.json'), 'w', encoding='utf-8') as f:
                json.dump(json_data, f, ensure_ascii=False, indent=4)
    elif "https://www.pixiv.net/novel/series/" in url:
        series_id = re.search(r"series/(\d+)", url).group(1)
        dl_series(series_id, folder_path, key_data)
    else:
        print(f'Error: "{url}" is not a valid URL')
        return