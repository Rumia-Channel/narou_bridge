import re
import os
import requests
from urllib.parse import unquote
import json
from playwright.sync_api import Playwright, sync_playwright, expect
from html import unescape
from datetime import datetime, timezone, timedelta
from jsondiff import diff
from . import convert_narou as cn

#リキャプチャ対策
import time
import random

def gen_pixiv_index(folder_path ,key_data):
    subfolders = [f for f in os.listdir(folder_path) if os.path.isdir(os.path.join(folder_path, f))]
    pairs = {}
    # 各サブフォルダの raw/raw.json を読み込む
    for folder in subfolders:
        json_path = os.path.join(folder_path, folder, 'raw', 'raw.json')
        if os.path.exists(json_path):
            with open(json_path, 'r', encoding='utf-8') as f:
                data = json.load(f)
                title = data.get('title', 'No title found')
                author = data.get('author', 'No author found')
                author_id = data.get('author_url', 'No author_id found').replace('https://www.pixiv.net/users/', '')
                type = data.get('type', 'No type found')
                pairs[folder] = {'title': title, 'author': author, 'author_id': author_id,'type': type}
        else:
            print(f"raw.json not found in {folder}")
            return
    
    pairs = dict(sorted(pairs.items(), key=lambda item: item[1]['author']))
    
    # index.html の生成
    with open(os.path.join(folder_path, 'index.html'), 'w', encoding='utf-8') as f:
        f.write('<!DOCTYPE html>\n')
        f.write('<html lang="ja">\n')
        f.write('<head>\n')
        f.write('<meta charset="UTF-8">\n')
        f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
        f.write('<title>Pixiv Index</title>\n')
        f.write('</head>\n')
        f.write('<body>\n')
        f.write(f'<a href="../{key_data}">戻る</a>\n')
        f.write('<h1>Pixiv 小説一覧</h1>\n')
        for folder, info in pairs.items():
            f.write(f'<a href="{folder}/{key_data}">({info['type']}) {info['title']}: {info['author']}</a><br>\n')
        f.write('</body>\n')
        f.write('</html>\n')
    
    with open(os.path.join(folder_path, 'index.json'), 'w', encoding='utf-8') as f:
        json.dump(pairs, f, ensure_ascii=False, indent=4)

    print('目次の生成が完了しました')

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
        full_path = os.path.join(folder_path, f's{id}')
    else:
        full_path = os.path.join(folder_path, f'n{id}')
    if not os.path.exists(full_path):
        os.makedirs(full_path)
    if not os.path.exists(f'{full_path}/raw'):
        os.makedirs(f'{full_path}/raw')
    if not os.path.exists(f'{full_path}/info'):
        os.makedirs(f'{full_path}/info')

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
        episode_path = os.path.join(folder_path, f's{id}', str(episode))
    else:
        episode_path = os.path.join(folder_path, f'n{id}')

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
            time.sleep(random.uniform(5,10))
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
    pattern = re.compile(r"(.*?)(\[chapter:(.*?)\])", re.DOTALL)
    def replacer(match):
        before_chapter = match.group(1)
        chapter_content = match.group(3).replace('\n', '')
        
        # 直前が改行でない場合のみ改行を追加
        if before_chapter and before_chapter[-1] != '\n':
            return f'{before_chapter}\n{chapter_content}\n\n\n'
        else:
            return f'{before_chapter}{chapter_content}\n\n\n'
    
    return re.sub(pattern, replacer, data)

#URLへのリンクを置き換え
def format_for_url(data):
    pattern = re.compile(r"\[\[jumpuri:(.*?) > (.*?)\]\]", re.DOTALL)
    return re.sub(pattern, lambda match: f'<a href={match.group(2)}>{match.group(1)}</a>', data)

#シリーズのダウンロードに関する処理
def dl_series(series_id, folder_path, key_data):
    # seriesNavDataの内部にあるseriesIdを取得
    print(f"Series ID: {series_id}")
    s_detail = find_key_recursively(json.loads(get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}").text), "body")
    s_toc = get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles")
    series_title = s_detail.get('title')
    series_author = s_detail.get('userName')
    series_author_id = s_detail.get('userId')
    series_episodes = s_detail.get('total')
    series_chara = s_detail.get('publishedTotalCharacterCount')
    series_caption_data = find_key_recursively(s_detail, 'caption')
    series_create_day = datetime.fromisoformat(s_detail.get('createDate'))
    series_update_day = datetime.fromisoformat(s_detail.get('updateDate'))
    if series_caption_data:
        series_caption = series_caption_data.replace('<br />', '\n').replace('jump.php?', '')
    else:
        series_caption = ''
    print(f"Series Title: {series_title}")
    print(f"Series Author: {series_author}")
    print(f"Series Author ID: {series_author_id}")
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
    for i, entry in enumerate(novel_toc, 1):
        if not entry['available']:
            continue
        time.sleep(random.uniform(5,10))
        json_data = return_content_json(entry['id'])
        introduction = find_key_recursively(json_data, 'body').get('description').replace('<br />', '\n').replace('jump.php?', '')
        postscript = find_key_recursively(json_data, 'body').get('pollData')
        text = find_key_recursively(json_data, 'body').get('content').replace('\r\n', '\n')
        if postscript:
            postscript = format_survey(postscript)
        else:
            postscript = ''
        if not introduction:
            introduction = ''
        
        #エピソードごとのフォルダの作成
        os.makedirs(os.path.join(folder_path, f's{series_id}', entry['id']), exist_ok=True)
        #挿絵リンクへの置き換え
        text = format_image(series_id, entry['id'], True, text, json_data, folder_path)
        #ルビの置き換え
        text = format_ruby(text)
        #チャプタータグの除去
        text = remove_chapter_tag(text)
        #URLへのリンクを置き換え
        text = format_for_url(text)
        episode[i] = {
            'id' : entry['id'],
            'title': entry['title'],
            'introduction': unquote(introduction),
            'text': text,
            'postscript': postscript,
            'createDate': str(datetime.fromisoformat(json_data.get('body').get('createDate')).astimezone(timezone(timedelta(hours=9)))),
            'updateDate': str(datetime.fromisoformat(json_data.get('body').get('uploadDate')).astimezone(timezone(timedelta(hours=9))))
        }
        total_text += int(find_key_recursively(json_data, 'body').get('characterCount'))

    # 作成日で並び替え
    episode = dict(sorted(episode.items(), key=lambda x: x[1]['createDate']))
    
    total_charactors = str(f'{total_text:,}')
    all_charactors = str(f'{series_chara:,}')
    novel = {
        'get_date': str(datetime.now().astimezone(timezone(timedelta(hours=9))).strftime('%Y-%m-%d %H:%M:%S%z')),
        'title': series_title,
        'id': series_id,
        'url': f"https://www.pixiv.net/novel/series/{series_id}",
        'author': series_author,
        'author_url': f"https://www.pixiv.net/users/{series_author_id}",
        'caption': series_caption,
        'total_episodes': len(episode),
        'all_episodes': series_episodes,
        'total_characters': total_charactors,
        'all_characters': all_charactors,
        'type': '連載中',
        'createDate': str(series_create_day.astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(series_update_day.astimezone(timezone(timedelta(hours=9)))),
        'episodes': episode
    }

    #生データがすでにあるなら差分を保管
    if os.path.exists(os.path.join(folder_path, f's{series_id}', 'raw', 'raw.json')):
        with open(os.path.join(folder_path, f's{series_id}', 'raw', 'raw.json'), 'r', encoding='utf-8') as f:
            old_json = json.load(f)
        old_json = json.loads(json.dumps(old_json))
        new_json = json.loads(json.dumps(novel))
        diff_json = convert_keys_to_str(diff(new_json,old_json))
        if len(diff_json) == 1 and 'get_date' in diff_json:
            pass
        else:
            with open(os.path.join(folder_path, f's{series_id}', 'raw', f'diff_{str(novel["updateDate"]).replace(':', '-').replace(' ', '_')}.json'), 'w', encoding='utf-8') as f:
                json.dump(diff_json, f, ensure_ascii=False, indent=4)
        

    #生データの書き出し
    with open(os.path.join(folder_path, f's{series_id}', 'raw', 'raw.json'), 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)

    cn.narou_gen(novel, os.path.join(folder_path, f's{series_id}'), key_data, data_folder, host)
    print("")

# キーをすべて文字列に変換する関数
def convert_keys_to_str(d):
    if isinstance(d, dict):
        return {str(k): convert_keys_to_str(v) for k, v in d.items()}
    elif isinstance(d, list):
        return [convert_keys_to_str(i) for i in d]
    else:
        return d

#短編のダウンロードに関する処理
def dl_novel(json_data, novel_id, folder_path, key_data):
    novel_data = json_data.get('body')
    novel_title = novel_data.get('title')
    novel_author = novel_data.get('userName')
    novel_author_id = novel_data.get('userId')
    novel_caption_data = novel_data.get('description')
    if novel_caption_data:
        novel_caption = novel_caption_data.replace('<br />', '\n').replace('jump.php?', '')
    else:
        novel_caption = ''
    novel_text = novel_data.get('content').replace('\r\n', '\n')
    novel_postscript = find_key_recursively(novel_data, 'pollData')
    if novel_postscript:
        novel_postscript = format_survey(novel_postscript)
    else:
        novel_postscript = ''
    novel_create_day = datetime.fromisoformat(novel_data.get('createDate'))
    novel_update_day = datetime.fromisoformat(novel_data.get('uploadDate'))
    print(f"Novel ID: {novel_id}")
    print(f"Novel Title: {novel_title}")
    print(f"Novel Author: {novel_author}")
    print(f"Novel Author ID: {novel_author_id}")
    print(f"Novel Caption: {novel_caption}")
    print(f"Novel Create Date: {novel_create_day}")
    print(f"Novel Update Date: {novel_update_day}")
    make_dir(novel_id, folder_path, False)
    #挿絵リンクへの置き換え
    text = format_image(novel_id, novel_id, False, novel_text, json_data, folder_path)
    #ルビの置き換え
    text = format_ruby(text)
    #チャプタータグの除去
    text = remove_chapter_tag(text)
    #URLへのリンクを置き換え
    text = format_for_url(text)
    episode = {}
    episode[1] = {
        'id' : novel_id,
        'title': novel_title,
        'introduction': unquote(novel_caption),
        'text': text,
        'postscript': novel_postscript,
        'createDate': str(datetime.fromisoformat(novel_data.get('createDate')).astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(datetime.fromisoformat(novel_data.get('uploadDate')).astimezone(timezone(timedelta(hours=9))))
    }

    novel = {
        'get_date': str(datetime.now().astimezone(timezone(timedelta(hours=9))).strftime('%Y-%m-%d %H:%M:%S%z')),
        'title': novel_title,
        'id': novel_id,
        'url': f"https://www.pixiv.net/novel/show.php?id={novel_id}",
        'author': novel_author,
        'author_url': f"https://www.pixiv.net/users/{novel_author_id}",
        'caption': novel_caption,
        'total_episodes': 1,
        'all_episodes': 1,
        'total_characters': novel_data.get('characterCount'),
        'all_characters': novel_data.get('characterCount'),
        'type': '短編',
        'createDate': str(novel_create_day.astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(novel_update_day.astimezone(timezone(timedelta(hours=9)))),
        'episodes': episode
    }

    #生データがすでにあるなら差分を保管
    if os.path.exists(os.path.join(folder_path, f'n{novel_id}', 'raw', 'raw.json')):
        with open(os.path.join(folder_path, f'n{novel_id}', 'raw', 'raw.json'), 'r', encoding='utf-8') as f:
            old_json = json.load(f)
        old_json = json.loads(json.dumps(old_json))
        new_json = json.loads(json.dumps(novel))
        diff_json = convert_keys_to_str(diff(new_json,old_json))
        if len(diff_json) == 1 and 'get_date' in diff_json:
            pass
        else:
            with open(os.path.join(folder_path, f'n{novel_id}', 'raw', f'diff_{str(old_json["get_date"]).replace(':', '-').replace(' ', '_')}.json'), 'w', encoding='utf-8') as f:
                json.dump(diff_json, f, ensure_ascii=False, indent=4)

    #生データの書き出し
    with open(os.path.join(folder_path, f'n{novel_id}', 'raw', 'raw.json'), 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)

    cn.narou_gen(novel, os.path.join(folder_path, f'n{novel_id}'), key_data, data_folder, host)
    print("")

#ユーザーページからのダウンロード
def dl_user(user_id, folder_path, key_data, update):
    count = 1
    print(f'User ID: {user_id}')
    user_data = get_with_cookie(f"https://www.pixiv.net/ajax/user/{user_id}/profile/all").json()
    user_name = get_with_cookie(f"https://www.pixiv.net/ajax/user/{user_id}").json().get('body').get('name')
    user_all_novels = user_data.get('body').get('novels')
    user_all_novel_series = user_data.get('body').get('novelSeries')
    user_novel_series = []
    in_novel_series = []
    user_novels = []
    print(f'User Name: {user_name}')
    #シリーズIDの取得
    for ns in user_all_novel_series:
        user_novel_series.append(ns.get('id'))
    #小説IDの取得
    user_novels = list(user_all_novels.keys())
    #シリーズとの重複を除去
    for i in user_novel_series:
        time.sleep(random.uniform(5,10))
        for nid in get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{i}/content_titles").json().get('body'):
            in_novel_series.append(nid.get('id'))
    user_novels = [n for n in user_novels if n not in in_novel_series]
    print(f'User Novels: {len(user_novels)}')
    print(f'User Novel Series: {len(user_novel_series)}')

    print("\nSeries Download Start\n")
    for series_id in user_novel_series:
        if update:
            raw_path = os.path.join(folder_path, f's{series_id}', 'raw', 'raw.json')
            if os.path.isfile(raw_path):
                series_update_date = datetime.fromisoformat(get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}").json().get('body').get('updateDate'))
                with open (raw_path, 'r', encoding='utf-8') as f:
                    old_series_json = json.load(f)
                series_old_update_date = datetime.fromisoformat(old_series_json.get('updateDate'))
                if series_update_date == series_old_update_date:
                    print(f'{old_series_json['title']} に更新はありません。\n')
                    if count == 10:
                        time.sleep(random.uniform(30,60))
                        count = 1
                    else:
                        time.sleep(random.uniform(5,10))
                        count += 1
                    continue
        dl_series(series_id, folder_path, key_data)
        time.sleep(random.uniform(5,10))

    print("\nNovel Download Start\n")
    for novel_id in user_novels:
        if update:
            raw_path = os.path.join(folder_path, f'n{novel_id}', 'raw', 'raw.json')
            if os.path.isfile(raw_path):
                novel_update_date = datetime.fromisoformat(return_content_json(novel_id).get('body').get('uploadDate'))
                with open (raw_path, 'r', encoding='utf-8') as f:
                    old_novel_json = json.load(f)
                novel_old_update_date = datetime.fromisoformat(old_novel_json.get('updateDate'))
                if novel_update_date == novel_old_update_date:
                    print(f'{old_novel_json['title']} に更新はありません。\n')
                    if count == 10:
                        time.sleep(random.uniform(30,60))
                        count = 1
                    else:
                        time.sleep(random.uniform(5,10))
                        count += 1
                    continue
        dl_novel(return_content_json(novel_id), novel_id, folder_path, key_data)
        time.sleep(random.uniform(5,10))

    user_json = os.path.join(folder_path, 'user.json')

    # user.jsonファイルが存在するかどうか確認
    if os.path.exists(user_json):
        # 既存のデータを読み込む
        with open(user_json, 'r', encoding='utf-8') as f:
            data = json.load(f)
    else:
        # ファイルがない場合、空のデータを初期化
        data = {}

    # user_id が JSON のキーとして存在するか確認
    if user_id not in data:
        # user_id が存在しなければ "enable" を値として追加
        data[user_id] = 'enable'

    # 更新されたデータを再び user.json に書き込む
    with open(user_json, 'w', encoding='utf-8') as f:
        json.dump(data, f, ensure_ascii=False, indent=4)

#ダウンロード処理
def download(url, folder_path, key_data, data_path, host_name):

    #引き渡し用変数
    global data_folder
    global host
    data_folder = data_path
    host = host_name

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
            dl_novel(json_data, novel_id, folder_path, key_data) #ダウンロード処理
    elif "https://www.pixiv.net/novel/series/" in url:
        series_id = re.search(r"series/(\d+)", url).group(1)
        dl_series(series_id, folder_path, key_data)
    elif "https://www.pixiv.net/users/" in url:
        user_id = re.search(r"users/(\d+)", url).group(1)
        dl_user(user_id, folder_path, key_data, False)
    else:
        print(f'Error: "{url}" is not a valid URL')
        return
    
    #仕上げ処理(indexファイルの更新)
    gen_pixiv_index(folder_path, key_data)

    print("\nDownload Complete\n")

#更新処理
def update(folder_path, key_data, data_path, host_name):

    #引き渡し用変数
    global data_folder
    global host
    data_folder = data_path
    host = host_name

    index_json = os.path.join(folder_path, 'index.json')
    with open(index_json, 'r', encoding='utf-8') as f:
        index_json = json.load(f)

    user_ids = []

    if os.path.isfile(os.path.join(folder_path, 'user.json')):
        with open(os.path.join(folder_path, 'user.json'), 'r', encoding='utf-8') as uf:
            user_json = json.load(uf)
        for user_id, status in user_json.items():
            if status == 'enable':
                if get_with_cookie(f'https://www.pixiv.net/users/{user_id}/novels').status_code == 404:
                    print("404 Not Found")
                    print("Incorrect URL, Deleted, Private, or My Pics Only.")
                    return
                dl_user(user_id, folder_path, key_data, True)
                user_ids.append(user_id)
                time.sleep(random.uniform(5,10))

    for folder_name, index_data in index_json.items():
        if index_data.get("author_id") in user_ids:
            continue
        if index_data.get("type") == "短編":
            novel_id = folder_name.replace('n', '')
            if get_with_cookie(f'https://www.pixiv.net/novel/show.php?id={novel_id}').status_code == 404:
                print("404 Not Found")
                print("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue
            json_data = return_content_json(novel_id)
            with open(os.path.join(folder_path, folder_name, 'raw', 'raw.json'), 'r', encoding='utf-8') as onf:
                old_novel_json = json.load(onf)
            if datetime.fromisoformat(json_data.get('body').get('uploadDate')) != datetime.fromisoformat(old_novel_json.get('updateDate')):
                dl_novel(json_data, novel_id, folder_path, key_data)
            else:
                print(f'{index_data.get("title")} に更新はありません。\n')

        elif index_data.get("type") == "連載中" or index_data.get("type") == "完結":
            series_id = folder_name.replace('s', '')
            if get_with_cookie(f'https://www.pixiv.net/novel/series/{series_id}').status_code == 404:
                print("404 Not Found")
                print("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue
            s_detail = find_key_recursively(json.loads(get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}").text), "body")
            with open(os.path.join(folder_path, folder_name, 'raw', 'raw.json'), 'r', encoding='utf-8') as osf:
                old_series_json = json.load(osf)

            if datetime.fromisoformat(s_detail.get('updateDate')) != datetime.fromisoformat(old_series_json.get('updateDate')):
                dl_series(series_id, folder_path, key_data)
            else:
                print(f'{index_data.get("title")} に更新はありません。\n')
        time.sleep(random.uniform(5,10))
    
    gen_pixiv_index(folder_path, key_data)

#変換処理
def convert(folder_path, key_data, data_path, host_name):

    #引き渡し用変数
    data_folder = data_path
    host = host_name

    folder_names = [name for name in os.listdir(folder_path) if os.path.isdir(os.path.join(folder_path, name))]

    for i in folder_names:
        cn.narou_gen(json.load(open(os.path.join(folder_path, i, 'raw', 'raw.json'), 'r', encoding='utf-8')), os.path.join(folder_path, i), key_data, data_folder, host)

    gen_pixiv_index(folder_path, key_data)