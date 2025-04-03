import re
import os
import shutil
import requests
from urllib.parse import unquote
import json
from playwright.sync_api import Playwright, sync_playwright, expect
from playwright_recaptcha import recaptchav2
from html import unescape
from datetime import datetime, timezone, timedelta
import zipfile

#アニメーションPNG用
from PIL import Image
import apng

#ログを保存
import logging

#リキャプチャ対策
import time
import random
from tqdm import tqdm

#共通の処理
import crawler.common as cm
import crawler.convert_narou as cn

#ファイルのバージョン
mv = 4

#初期化処理
def init(cookie_path, data_path, is_login, interval):

    cookie_path = os.path.join(cookie_path, 'login.json')

    global interval_sec
    global g_count
    interval_sec = int(interval)
    g_count = 1

    logging.info(f'Login : {is_login}')

    def update_cookie(playwright: Playwright) -> None:

        pixiv_cookie, ua = cm.load_cookies_and_ua(cookie_path)
        
        #ヘッドレスモードで起動
        browser = playwright.firefox.launch(
            headless=True,
            args=[
                "-headless",  # ヘッドレスモード
                "--disable-blink-features=AutomationControlled"  # 自動化検出の回避
            ])
        context = browser.new_context(locale='en-US', viewport={"width": 1920, "height": 1080}, screen={"width": 1920, "height": 1080}, user_agent=ua)
        page = context.new_page()
        #page.set_viewport_size({'width': 1280, 'height': 1280})

        # ヘッドレスモードを隠すためのスクリプト
        page.add_init_script("""
        Object.defineProperty(navigator, 'webdriver', {
        get: () => undefined
        });
        """)

        # フィンガープリントを偽装するスクリプトを挿入
        page.add_init_script("""
        Object.defineProperty(window, 'chrome', {
        get: () => ({ runtime: {} }),
        });

        Object.defineProperty(navigator, 'plugins', {
        get: () => [1, 2, 3],  // プラグイン情報を設定
        });

        Object.defineProperty(navigator, 'languages', {
        get: () => ['en-US', 'en']
        });
        """)

        # Pixivのログインページにアクセス
        logging.info("Navigating to Pixiv...")
        cookie_list = [
                        {"name": k, "value": v, "domain": ".pixiv.net", "path": "/", "secure": True, "httpOnly": True}
                        for k, v in pixiv_cookie.items() if v  # 空でない Cookie だけ追加
                    ]
        context.add_cookies(cookie_list)
        page.goto("https://www.pixiv.net/")
        time.sleep(random.uniform(1, 5))
        page.goto("https://www.pixiv.net/novel")

        cookies = context.cookies()
        
        cm.save_cookies_and_ua(cookie_path, cookies, ua)

        # ---------------------
        context.close()
        browser.close()


    def login(playwright: Playwright) -> None:
        # ブラウザ設定

        # 通常モードからユーザーエージェントを取得
        browser = playwright.firefox.launch(headless=False)  # 通常モード（UIあり）でブラウザを起動
        context = browser.new_context()
        page = context.new_page()
        user_agent = page.evaluate('navigator.userAgent')
        browser.close()


        #ヘッドレスモードで起動
        browser = playwright.firefox.launch(
            headless=True,
            args=[
                "-headless",  # ヘッドレスモード
                "--disable-blink-features=AutomationControlled"  # 自動化検出の回避
            ])
        context = browser.new_context(locale='en-US', viewport={"width": 1920, "height": 1080}, screen={"width": 1920, "height": 1080}, user_agent=user_agent)
        page = context.new_page()
        #page.set_viewport_size({'width': 1280, 'height': 1280})
        
        # ヘッドレスモードを隠すためのスクリプト
        page.add_init_script("""
        Object.defineProperty(navigator, 'webdriver', {
        get: () => undefined
        });
        """)

        # フィンガープリントを偽装するスクリプトを挿入
        page.add_init_script("""
        Object.defineProperty(window, 'chrome', {
        get: () => ({ runtime: {} }),
        });

        Object.defineProperty(navigator, 'plugins', {
        get: () => [1, 2, 3],  // プラグイン情報を設定
        });

        Object.defineProperty(navigator, 'languages', {
        get: () => ['en-US', 'en']
        });
        """)

        # Pixivのログインページにアクセス
        logging.info("Navigating to Pixiv...")
        page.goto("https://www.pixiv.net/")
        time.sleep(random.uniform(1, 5))
        
        # ログインリンクをクリック
        logging.info("Clicking login link...")
        page.get_by_role("link", name="Login").click()

        # ユーザー情報を入力
        page.get_by_placeholder("E-mail address or pixiv ID").click()
        mail = input("メールアドレスを入力してください: ")
        page.get_by_placeholder("E-mail address or pixiv ID").fill(mail)
        page.get_by_placeholder("Password").click()
        pswd = input("パスワードを入力してください: ")
        page.get_by_placeholder("Password").fill(pswd)
        
        time.sleep(random.uniform(2, 5))
        logging.info("Attempting to log in...")
        page.get_by_role("button", name="Log In", exact=True).click()

        # リキャプチャの確認を1度だけ行う
        time.sleep(random.uniform(2, 5))
        logging.info(f"Current URL after login attempt: {page.url}")
        if "accounts.pixiv.net" in page.url and "two-factor-authentication" in page.url:  # Pixivのログインページに留まっている場合
            logging.info("reCAPTCHA detected. Solving...")
            try:
                with recaptchav2.SyncSolver(page) as solver:
                    token = solver.solve_recaptcha(wait=True)
                    logging.info(f"reCAPTCHA token obtained: {token}")
                    if not token:
                        raise ValueError("Failed to retrieve a valid reCAPTCHA token.")
                logging.info("Retrying login after solving reCAPTCHA...")
                page.get_by_role("button", name="Log In", exact=True).click()
                time.sleep(random.uniform(2, 5))
            except Exception as e:
                logging.info(f"Failed to solve reCAPTCHA: {e}")
                return

        # ログイン成功後、リダイレクト処理を待機
        def wait_for_redirects(page, is_2fa=False, timeout=60):
            start_time = time.time()
            while time.time() - start_time < timeout:
                page.wait_for_load_state("load")
                logging.info(f"Waiting for redirects... Current URL: {page.url}")
                
                # 2段階認証ページが表示された場合
                if "two-factor-authentication" in page.url and not is_2fa:
                    logging.info("Two-factor authentication page detected.")
                    return "two-factor-authentication"
                
                # ホームページに到達した場合
                if page.url in ["https://www.pixiv.net/", "https://www.pixiv.net/en/"]:
                    logging.info("Successfully redirected to Pixiv homepage.")
                    return "success"
            return "timeout"

        redirect_status = wait_for_redirects(page)
        
        if redirect_status == "timeout":
            logging.error("Redirect did not complete within the timeout period.")
            return
        
        if redirect_status == "two-factor-authentication":
            # 2段階認証処理
            logging.info("2-factor authentication required.")
            time.sleep(random.uniform(1, 5))
            page.get_by_label("Trust this browser").check()  # ブラウザを信頼する
            tfak = input("2段階認証のコードを入力してください: ")  # ユーザーにコードを入力させる
            page.get_by_placeholder("Verification code").fill(tfak)
            time.sleep(random.uniform(1, 5))
            page.get_by_role("button", name="Log In").click()  # ログインボタンをクリック

            # 2段階認証後のリダイレクトを待機（is_2fa=Trueとして呼び出し）
            logging.info("Waiting for redirect after 2-factor authentication...")
            redirect_status = wait_for_redirects(page, is_2fa=True)
            if redirect_status == "timeout":
                logging.error("Redirect after 2-factor authentication did not complete.")
                return

        # ログイン完了
        logging.info(f"Login successful. Final URL: {page.url}")

        cookies = context.cookies()
        user_agent = page.evaluate("() => navigator.userAgent")
        
        cm.save_cookies_and_ua(cookie_path, cookies, user_agent)

        # ---------------------
        context.close()
        browser.close()

    #クッキーとユーザーエージェントをグローバルで宣言
    global pixiv_cookie
    global pixiv_header

    if os.path.isfile(cookie_path):
        pixiv_cookie, ua = cm.load_cookies_and_ua(cookie_path)

    if is_login:

        # cookieの有無とログイン状態を確認
        if not os.path.isfile(cookie_path) or bool(requests.get('https://www.pixiv.net/dashboard', cookies=pixiv_cookie, headers={'User-Agent': str(ua)}).history):
            with sync_playwright() as playwright:
                login(playwright)      

        pixiv_cookie, ua = cm.load_cookies_and_ua(cookie_path)

    else:

        pixiv_cookie = {}

        ua_list = [
            "Mozilla/5.0 (Linux; Android 10; SM-G973F) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.77 Mobile Safari/537.36",  # Android
            "Mozilla/5.0 (iPhone; CPU iPhone OS 14_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.0 Mobile/15E148 Safari/604.1",  # iOS
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.114 Safari/537.36",  # MacOS
            "Mozilla/5.0 (AppleTV; U; CPU OS 13_4 like Mac OS X; en-us) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/13.0 Safari/605.1.15",  # TvOS
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",  # Windows
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.101 Safari/537.36"  # Linux
        ]
        ua = random.choice(ua_list)

    pixiv_header = {
        'User-Agent': ua,
        'Accept-Language': 'ja,en-US;q=0.9,en;q=0.8',
        'Accept-Encoding': 'gzip, deflate, br',
        'Connection': 'keep-alive',
        'Referer': 'https://www.pixiv.net/',
    }

    global img_path
    img_path = os.path.join(data_path, 'images')
    logging.debug(f'Image Path: {img_path}')

# レスポンスからjsonデータ(本文データ)を返却
def return_content_json(novelid):
    novel_data = cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/{novelid}", pixiv_cookie, pixiv_header).text
    json_data = json.loads(unescape(novel_data))
    return json_data

# レスポンスからcomicのjsonデータを返却
def return_comic_content_json(comic_id):
    response = cm.get_with_cookie(f"https://www.pixiv.net/ajax/illust/{comic_id}", pixiv_cookie, pixiv_header)
    if response.status_code == 200:
        return json.loads(unescape(response.text))
    else:
        return None

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

#ルビ形式の整形
def format_ruby(data):
    pattern = re.compile(r"\[\[rb:(.*?) > (.*?)\]\]")
    return re.sub(pattern, lambda match: f'[ruby:<{match.group(1)}>({match.group(2)})]', data)

#画像リンク形式の整形
def format_image(id, episode, novel, series, data, json_data, folder_path):
    global g_count
    #pixivimage: で始まるリンクの抽出
    links = re.findall(r"\[pixivimage:(\d+)-(\d+)\]", data)
    link_dict = {}
    #uploadedimage: で始まるリンクの抽出
    inner_links = re.findall(r"\[uploadedimage:(\d+)\]", data)

    #シリーズとその他のリンクの切り替え
    if novel:
        if series:
            episode_path = os.path.join(folder_path, f's{id}', str(episode))
            url = f"https://www.pixiv.net/novel/series/{id}/{episode}"
        else:
            episode_path = os.path.join(folder_path, f'n{id}')
            url = f"https://www.pixiv.net/novel/show.php?id={id}"
    else:
        if series:
            episode_path = os.path.join(folder_path, f'c{id}', str(episode))
        else:
            episode_path = os.path.join(folder_path, f'a{id}')


    for i in links:
        art_id = i[0]
        img_num = i[1]
        if art_id not in link_dict:
            link_dict[art_id] = []  # art_id が存在しない場合に空のリストを初期化
        link_dict[art_id].append(img_num)
    #画像リンクの形式を[リンク名](リンク先)に変更
    for art_id, img_nums in link_dict.items():

        art_data = cm.get_with_cookie(f"https://www.pixiv.net/ajax/illust/{art_id}/pages", pixiv_cookie, pixiv_header)

        if not art_data:
            pattern = re.compile(fr"\[pixivimage:({art_id})-(\d+)\]")
            matches = pattern.findall(data)
            for match in matches:
                art_id, num = match
                index = int(num) - 1
                img_name = f'pixiv_{art_id}_p{index}'
                img_file_name = cm.check_image_file(img_path, img_name) #画像ファイルの名前からデータベースを探索
                if img_file_name:
                    data = data.replace(f'[pixivimage:{art_id}-{num}]', f'[image]({img_file_name})')
                    continue

            data = pattern.sub('\n', data)
            continue

        illust_json = art_data.json()
        illust_datas = cm.find_key_recursively(illust_json, 'body')
        for index, i in tqdm(enumerate(illust_datas), desc=f"Downloading illusts from https://www.pixiv.net/artworks/{art_id}", unit="illusts", total=len(illust_datas), leave=False):
            if str(index + 1) in img_nums:
                img_url = i.get('urls').get('original')
                img_name = f'pixiv_{art_id}_p{index}{os.path.splitext(img_url)[1]}'
                img_file_name = cm.check_image_file(img_path, img_name) #画像ファイルの名前からデータベースを探索
                #データベースに乗っていた場合、ダウンロードせずにそのまま利用
                if img_file_name:
                    data = data.replace(f'[pixivimage:{art_id}-{index + 1}]', f'[image]({img_file_name})')
                    logging.info(f"Image {img_file_name} already exists.")
                    continue
                
                #BAN対策
                if g_count >= 10:
                    time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                    g_count = 1
                else:
                    time.sleep(interval_sec)
                    g_count += 1
                
                img_data = cm.get_with_cookie(img_url, pixiv_cookie, pixiv_header) #画像のダウンロード]

                if not img_data.status_code == 200:
                    data = data.replace(f'[pixivimage:{art_id}-{index + 1}]', '\n') #画像のダウンロードに失敗した場合リンクを削除

                img_file_name = cm.check_image_hash(img_path, img_data.content, img_name) #画像ファイルのハッシュ値を取得

                with open(os.path.join(img_path, f'{img_file_name}{os.path.splitext(img_url)[1]}'), 'wb') as f:
                    f.write(img_data.content)
                data = data.replace(f'[pixivimage:{art_id}-{index + 1}]', f'[image]({img_file_name}{os.path.splitext(img_url)[1]})')
    if novel:
        #小説内アップロードの画像リンクの形式を[リンク名](リンク先)に変更
        for inner_link in tqdm(inner_links, desc=f"Downloading inner illusts from {url}", unit="illusts", total=len(inner_links), leave=False):
            in_img_url = cm.find_key_recursively(json_data, inner_link).get('urls').get('original')
            in_img_name = f'pixiv_{id}_{inner_link}{os.path.splitext(in_img_url)[1]}'
            in_img_file_name = cm.check_image_file(img_path, in_img_name) #画像ファイルの名前からデータベースを探索

            if in_img_file_name:
                    data = data.replace(f'[uploadedimage:{inner_link}]', f'[image]({in_img_file_name})')
                    logging.info(f"Image {in_img_file_name} already exists.")
                    continue
            
            #BAN対策
            if g_count >= 10:
                time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                g_count = 1
            else:
                time.sleep(interval_sec)
                g_count += 1
            
            in_img_data = cm.get_with_cookie(in_img_url, pixiv_cookie, pixiv_header) #画像のダウンロード

            if not in_img_data.status_code == 200:
                data = data.replace(f'[uploadedimage:{inner_link}]', '\n') #画像のダウンロードに失敗した場合リンクを削除
                continue

            in_img_file_name = cm.check_image_hash(img_path, in_img_data.content, in_img_name) #画像ファイルのハッシュ値を取得

            with open(os.path.join(img_path, f'{in_img_file_name}{os.path.splitext(in_img_url)[1]}'), 'wb') as f:
                f.write(in_img_data.content)

            data = data.replace(f'[uploadedimage:{inner_link}]', f'[image]({in_img_file_name}{os.path.splitext(in_img_url)[1]})')

    return data

#各話の表紙のダウンロード
def get_cover(raw_small_url, folder_path):
    original_url = raw_small_url
    # URLの候補リストを作成
    url_variants = [
        raw_small_url.replace('c/600x600/novel-cover-master', 'novel-cover-original').replace('_master1200', '').replace('.jpg', ext)
        for ext in ['.png', '.jpg', '.jpeg', '.gif']
    ]
    
    #tqdmの表示崩れ対策用
    #print('')

    # 各URLを試行
    for ep_cover in url_variants:
        #print(f"Download cover image from: {ep_cover}")
        response = cm.get_with_cookie(ep_cover, pixiv_cookie, pixiv_header)
        if response is not None and response.status_code == 200:
            # ファイルを保存
            file_extension = os.path.splitext(ep_cover)[1]
            with open(os.path.join(folder_path, f'cover{file_extension}'), 'wb') as f:
                f.write(response.content)
            #print(f"Save compleat!: {ep_cover}")
            return  # 成功したら終了

    # 全てのURLが404だった場合小さいサイズの表紙を保存
    response = cm.get_with_cookie(original_url, pixiv_cookie, pixiv_header)
    if response is not None and response.status_code == 200:
        file_extension = os.path.splitext(original_url)[1]
        with open(os.path.join(folder_path, f'cover{file_extension}'), 'wb') as f:
            f.write(response.content)
        #print(f"Save compleat!: {original_url}")
        return
    
    # どのURLもダウンロードに失敗した場合
    logging.error(f"Failed to download cover image from: {original_url}")

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

#漫画シリーズからリンクを取得する
def get_comic_link(cache, id):
    c_p = 1
    arts = {}
    while True:
            for item in cache["page"]["series"]:
                work_id = item["workId"]
                order = item["order"]
                arts[order] = work_id

                if not item:
                    break

            if 1 in arts.keys():
                break
            elif not arts:
                return None
            else:
                c_p += 1
                cache = cm.find_key_recursively(json.loads(cm.get_with_cookie(f"https://www.pixiv.net/ajax/series/{id}?p={c_p}&lang=ja", pixiv_cookie, pixiv_header).text), "body")

            time.sleep(interval_sec)
    return arts

#シリーズのダウンロードに関する処理
def dl_series(series_id, folder_path, key_data, update):
    global g_count
    # seriesNavDataの内部にあるseriesIdを取得
    logging.info(f"Series ID: {series_id}")
    s_detail = cm.find_key_recursively(json.loads(cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}", pixiv_cookie, pixiv_header).text), "body")
    s_toc = cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles", pixiv_cookie, pixiv_header)
    s_toc_u = cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/series_content/{series_id}", pixiv_cookie, pixiv_header)
    series_title = s_detail.get('title')
    series_author = s_detail.get('userName')
    series_author_id = s_detail.get('userId')
    series_episodes = s_detail.get('total')
    series_chara = s_detail.get('publishedTotalCharacterCount')
    series_tags = list(s_detail.get('tags'))
    series_caption_data = cm.find_key_recursively(s_detail, 'caption')
    series_create_day = datetime.fromisoformat(s_detail.get('createDate'))
    series_update_day = datetime.fromisoformat(s_detail.get('updateDate'))
    if series_caption_data:
        series_caption = series_caption_data.replace('<br />', '\n').replace('jump.php?', '')
    else:
        series_caption = ''
    logging.info(f"Series Title: {series_title}")
    logging.info(f"Series Author: {series_author}")
    logging.info(f"Series Author ID: {series_author_id}")
    logging.info(f"Series Caption: {series_caption}")
    logging.info(f"Series Tags: {series_tags}")
    logging.info(f"Series Total Episodes: {series_episodes}")
    logging.info(f"Series Total Characters: {series_chara}")
    logging.info(f"Series Create Date: {series_create_day}")
    logging.info(f"Series Update Date: {series_update_day}")
    cm.make_dir('s'+str(series_id), folder_path)
    toc_json_data = json.loads(s_toc.text)
    toc_u_json_data = json.loads(s_toc_u.text)
    novel_toc = toc_json_data.get('body')
    novel_toc_u = toc_u_json_data.get('body').get('thumbnails')
    episode = {}
    all_tags = series_tags
    total_text = 0
    series_path = os.path.join(folder_path, f's{series_id}')
    raw_path = os.path.join(series_path, 'raw', 'raw.json')
    if update:
        if os.path.isfile(raw_path):
            with open (raw_path, 'r', encoding='utf-8') as f:
                old_episode_update_dates = json.load(f).get('episodes')
                is_update = True
        else:
            is_update = False

    #表紙のダウンロード
    if not update:
        series_cover = s_detail.get('cover').get('urls').get('original')
        series_cover_data = cm.get_with_cookie(series_cover, pixiv_cookie, pixiv_header)
        with open(os.path.join(series_path, f'cover{os.path.splitext(series_cover)[1]}'), 'wb') as f:
            f.write(series_cover_data.content)

    for i, entry in tqdm(enumerate(novel_toc, 1), desc=f"Downloading episodes", unit="episodes", total=len(novel_toc), leave=False):
        if not entry['available']:
            continue

        tags = []

        ep_update = update

        #エピソードごとのフォルダ作成
        os.makedirs(os.path.join(series_path, entry['id']), exist_ok=True)

        #呼び出された処理が更新処理ですでにフォルダが存在する場合
        if update and is_update:
            for item in old_episode_update_dates.values():
                if item['id'] == entry['id']:
                    ep_json = item
                    for nu in novel_toc_u['novel']:
                        if nu.get('id') == entry['id']:
                            update_date = datetime.fromisoformat(nu.get('updateDate'))
                            if update_date == datetime.fromisoformat(item['updateDate']):
                                ep_update = True
                                break
                            else:
                                ep_update = False
                                break
        else:
            ep_update = False

        if ep_update:
            introduction = ep_json.get('introduction')
            postscript = ep_json.get('postscript')
            text = ep_json.get('text')
            createdate = ep_json.get('createDate')
            updatedate = ep_json.get('updateDate')
            tags = list(ep_json.get('tags', []))
            all_tags = list(dict.fromkeys(all_tags + ep_json.get('tags')))
            text_count = int(ep_json.get('textCount'))
            total_text += int(ep_json.get('textCount'))
        else:

            #BAN対策
            if g_count >= 10:
                time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                g_count = 1
            else:
                time.sleep(interval_sec)
                g_count += 1

            #エピソードの処理
            json_data = return_content_json(entry['id'])

            #表紙のダウンロード
            if not os.path.isfile(os.path.join(series_path, f'cover{os.path.splitext(json_data.get("body").get("coverUrl"))[1]}')):
                get_cover(json_data.get('body').get('coverUrl'), os.path.join(series_path, entry['id']))
            else:
                logging.info(f"Cover image already exists: {json_data.get('body').get('coverUrl')}")

            tags = [tag.get('tag', '') for tag in json_data.get('body').get('tags', {}).get('tags', [])]

            all_tags = list(dict.fromkeys(all_tags + tags))

            introduction = cm.find_key_recursively(json_data, 'body').get('description').replace('<br />', '\n').replace('jump.php?', '')
            postscript = cm.find_key_recursively(json_data, 'body').get('pollData')
            text = cm.find_key_recursively(json_data, 'body').get('content').replace('\r\n', '\n')
            if postscript:
                postscript = format_survey(postscript)
            else:
                postscript = ''
            if not introduction:
                introduction = ''
        
            #エピソードごとのフォルダの作成
            os.makedirs(os.path.join(series_path, entry['id']), exist_ok=True)
            #挿絵リンクへの置き換え
            text = format_image(series_id, entry['id'], True, True, text, json_data, folder_path)
            #ルビの置き換え
            text = format_ruby(text)
            #チャプタータグの除去
            text = remove_chapter_tag(text)
            #URLへのリンクを置き換え
            text = format_for_url(text)
            #作成日
            createdate = str(datetime.fromisoformat(json_data.get('body').get('createDate')).astimezone(timezone(timedelta(hours=9))))
            #更新日
            updatedate = str(datetime.fromisoformat(json_data.get('body').get('uploadDate')).astimezone(timezone(timedelta(hours=9))))

            text_count = int(cm.find_key_recursively(json_data, 'body').get('characterCount'))

            total_text += text_count

        #重複した小説の除去
        if os.path.exists(os.path.join(folder_path, f'n{entry["id"]}')):
            shutil.rmtree(os.path.join(folder_path, f'n{entry["id"]}'))
            logging.info(f"Remove duplicate folder: n{entry['id']}")

        episode[i] = {
            'id' : entry['id'],
            'chapter': None,
            'title': entry['title'],
            'textCount': text_count,
            'tags': tags,
            'introduction': unquote(introduction),
            'text': text,
            'postscript': postscript,
            'createDate': createdate,
            'updateDate': updatedate
        }

    # 作成日で並び替え
    sorted_episode = dict(sorted(episode.items(), key=lambda x: x[1]['createDate']))

    # インデックスを再設定
    episode = {i + 1: entry for i, (key, entry) in enumerate(sorted_episode.items())}
    
    novel = {
        'version': mv,
        'get_date': str(datetime.now().astimezone(timezone(timedelta(hours=9)))),
        'title': series_title,
        'id': series_id,
        'url': f"https://www.pixiv.net/novel/series/{series_id}",
        'author': series_author,
        'author_id': series_author_id,
        'author_url': f"https://www.pixiv.net/users/{series_author_id}",
        'caption': series_caption,
        'total_episodes': len(episode),
        'all_episodes': series_episodes,
        'total_characters': total_text,
        'all_characters': series_chara,
        'type': 'novel',
        'serialization': '連載中',
        'tags': series_tags,
        'all_tags': all_tags,
        'createDate': str(series_create_day.astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(series_update_day.astimezone(timezone(timedelta(hours=9)))),
        'episodes': episode
    }

    #小説データの差分を保存
    cm.save_raw_diff(raw_path, series_path, novel)
        

    #生データの書き出し
    with open(raw_path, 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)

    cn.narou_gen(novel, os.path.join(series_path), key_data, data_folder, host)
    print("")
    #仕上げ処理(indexファイルの更新)
    cm.gen_site_index(folder_path, key_data, 'Pixiv')

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
    novel_postscript = cm.find_key_recursively(novel_data, 'pollData')
    if novel_postscript:
        novel_postscript = format_survey(novel_postscript)
    else:
        novel_postscript = ''
    novel_tags = [tag.get('tag', '') for tag in novel_data.get('tags', {}).get('tags', [])]
    novel_create_day = datetime.fromisoformat(novel_data.get('createDate'))
    novel_update_day = datetime.fromisoformat(novel_data.get('uploadDate'))
    logging.info(f"Novel ID: {novel_id}")
    logging.info(f"Novel Title: {novel_title}")
    logging.info(f"Novel Author: {novel_author}")
    logging.info(f"Novel Author ID: {novel_author_id}")
    logging.info(f"Novel Caption: {novel_caption}")
    logging.info(f"Novel Tags: {novel_tags}")
    logging.info(f"Novel Create Date: {novel_create_day}")
    logging.info(f"Novel Update Date: {novel_update_day}")
    cm.make_dir('n'+str(novel_id), folder_path)
    novel_path = os.path.join(folder_path, f'n{novel_id}')
    raw_path = os.path.join(novel_path, 'raw', 'raw.json')
    #挿絵リンクへの置き換え
    text = format_image(novel_id, novel_id, True, False, novel_text, json_data, folder_path)
    #表紙のダウンロード
    if not os.path.isfile(os.path.join(novel_path, f'cover{os.path.splitext(novel_data.get("coverUrl"))[1]}')):
        get_cover(novel_data.get('coverUrl'), novel_path)
    else:
        logging.info(f"Cover image already exists: {novel_data.get('coverUrl')}")
    #ルビの置き換え
    text = format_ruby(text)
    #チャプタータグの除去
    text = remove_chapter_tag(text)
    #URLへのリンクを置き換え
    text = format_for_url(text)
    episode = {}
    episode[1] = {
        'id' : novel_id,
        'chapter': None,
        'title': novel_title,
        'textCount': novel_data.get('characterCount'),
        'tags': novel_tags,
        'introduction': unquote(novel_caption),
        'text': text,
        'postscript': novel_postscript,
        'createDate': str(datetime.fromisoformat(novel_data.get('createDate')).astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(datetime.fromisoformat(novel_data.get('uploadDate')).astimezone(timezone(timedelta(hours=9))))
    }

    novel = {
        'version': mv,
        'get_date': str(datetime.now().astimezone(timezone(timedelta(hours=9))).strftime('%Y-%m-%d %H:%M:%S%z')),
        'title': novel_title,
        'id': novel_id,
        'url': f"https://www.pixiv.net/novel/show.php?id={novel_id}",
        'author': novel_author,
        'author_id': novel_author_id,
        'author_url': f"https://www.pixiv.net/users/{novel_author_id}",
        'caption': novel_caption,
        'total_episodes': 1,
        'all_episodes': 1,
        'total_characters': novel_data.get('characterCount'),
        'all_characters': novel_data.get('characterCount'),
        'type': 'novel',
        'serialization': '短編',
        'tags': novel_tags,
        'all_tags': novel_tags,
        'createDate': str(novel_create_day.astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(novel_update_day.astimezone(timezone(timedelta(hours=9)))),
        'episodes': episode
    }

    #小説データの差分を保存
    cm.save_raw_diff(raw_path, novel_path, novel)

    #生データの書き出し
    with open(raw_path, 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)

    cn.narou_gen(novel, novel_path, key_data, data_folder, host)
    print("")
    #仕上げ処理(indexファイルの更新)
    cm.gen_site_index(folder_path, key_data, 'Pixiv')

#漫画のダウンロードに関する処理
def dl_art(art_id, folder_path, key_data):
    #漫画のデータ取得
    logging.info(f"art ID: {art_id}")
    #消えてる可能性を考慮
    a_data = return_comic_content_json(art_id)
    if not a_data:
        logging.error(f"Art ID: {art_id} is not available.")
        return
    a_detail = a_data.get('body')
    a_toc = cm.get_with_cookie(f"https://www.pixiv.net/ajax/illust/{art_id}/pages", pixiv_cookie, pixiv_header)
    art_title = a_detail.get('title')
    art_author = a_detail.get('userName')
    art_author_id = a_detail.get('userId')
    art_caption_data = a_detail.get('description')
    if art_caption_data:
        art_caption = art_caption_data.replace('<br />', '\n').replace('jump.php?', '')
    else:
        art_caption = ''
    art_postscript = cm.find_key_recursively(a_detail, 'pollData')
    if art_postscript:
        art_postscript = format_survey(art_postscript)
    else:
        art_postscript = ''
    art_create_day = datetime.fromisoformat(a_detail.get('createDate'))
    art_update_day = datetime.fromisoformat(a_detail.get('uploadDate'))
    art_text = ''
    all_art = a_toc.json().get('body')
    for i in all_art:
        url = i.get('urls', {}).get('original')  # 安全にキーを取得
        match = re.search(r'_p(\d+)\.', url)  # _p数字. のパターンを探す
        
        if match:
            img_num = match.group(1)
            art_text += f'[pixivimage:{art_id}-{int(img_num) + 1}]\n'
        else:
            if '_ugoira' in url:

                anim_img_name = f'pixiv_{art_id}_ugoira.apng'
                anim_img_file_name = cm.check_image_file(img_path, anim_img_name) #画像ファイルの名前からデータベースを探索

                if anim_img_file_name:
                    art_text += f'[image]({anim_img_file_name})\n'
                    logging.info(f"Image {anim_img_file_name} already exists.")
                    continue

                cm.make_dir('a'+str(art_id), folder_path)
                time.sleep(interval_sec)
                ugoira_index = cm.get_with_cookie(f"https://www.pixiv.net/ajax/illust/{art_id}/ugoira_meta?lang=ja", pixiv_cookie, pixiv_header).json().get('body')
                time.sleep(interval_sec)
                src = cm.get_with_cookie(ugoira_index.get('originalSrc'), pixiv_cookie, pixiv_header)
                with open(os.path.join(folder_path, f'a{art_id}', f'{art_id}.zip'), 'wb') as f:
                    f.write(src.content)
                
                temp_path = os.path.join(folder_path, f'a{art_id}', 'temp')
                os.makedirs(temp_path, exist_ok=True)
                with zipfile.ZipFile(os.path.join(folder_path, f'a{art_id}', f'{art_id}.zip')) as zf:
                    zf.extractall(temp_path)

                os.remove(os.path.join(folder_path, f'a{art_id}', f'{art_id}.zip'))

                anim_files = [frame.get("file") for frame in ugoira_index.get('frames')]
                delays = [frame.get('delay') for frame in ugoira_index.get('frames')]  # delay（ミリ秒）を取得
                # PillowでAPNGを作成
                frames = [Image.open(os.path.join(temp_path, str(img))) for img in anim_files]
                frames[0].save(os.path.join(temp_path, "temp.apng"), save_all=True, append_images=frames[1:], loop=0, duration=delays)

                with open(os.path.join(temp_path, "temp.apng"), 'rb') as f:
                    anim_img_data = f.read()

                anim_img_file_name = cm.check_image_hash(img_path, anim_img_data, anim_img_name) #画像ファイルのハッシュ値を取得

                with open(os.path.join(img_path, f'{anim_img_file_name}.apng'), 'wb') as f:
                    f.write(anim_img_data)

                art_text += f'[image]({anim_img_file_name}.apng)\n'

                shutil.rmtree(temp_path)
        
    art_tags = [tag.get('tag', '') for tag in a_detail.get('tags', {}).get('tags', [])]
    logging.info(f"Art Title: {art_title}")
    logging.info(f"Art Author: {art_author}")
    logging.info(f"Art Author ID: {art_author_id}")
    logging.info(f"Art Caption: {art_caption}")
    logging.info(f"Art Tags: {art_tags}")
    logging.info(f"Art Create Date: {art_create_day}")
    logging.info(f"Art Update Date: {art_update_day}")
    cm.make_dir('a'+str(art_id), folder_path)
    art_path = os.path.join(folder_path, f'a{art_id}')
    raw_path = os.path.join(art_path, 'raw', 'raw.json')
    #挿絵リンクへの置き換え
    art_text = format_image(art_id, art_id, False, False, art_text, a_toc.json(), folder_path)
    #表紙のダウンロード
    if not os.path.isfile(os.path.join(art_path, f'cover{os.path.splitext(a_detail.get('urls').get('original'))[1]}')):
        get_cover(a_detail.get('urls').get('original'), art_path)
    else:
        logging.info(f"Cover image already exists: {a_detail.get('urls').get('original')}")
    episode = {}
    episode[1] = {
        'id' : art_id,
        'chapter': None,
        'title': art_title,
        'textCount': 0,
        'tags': art_tags,
        'introduction': unquote(art_caption),
        'text': art_text,
        'postscript': art_postscript,
        'createDate': str(datetime.fromisoformat(a_detail.get('createDate')).astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(datetime.fromisoformat(a_detail.get('uploadDate')).astimezone(timezone(timedelta(hours=9))))
    }

    novel = {
        'version': mv,
        'get_date': str(datetime.now().astimezone(timezone(timedelta(hours=9))).strftime('%Y-%m-%d %H:%M:%S%z')),
        'title': art_title,
        'id': art_id,
        'url': f"https://www.pixiv.net/artworks/{art_id}",
        'author': art_author,
        'author_id': art_author_id,
        'author_url': f"https://www.pixiv.net/users/{art_author_id}",
        'caption': art_caption,
        'total_episodes': 1,
        'all_episodes': 1,
        'total_characters': 0,
        'all_characters': 0,
        'type': 'comic',
        'serialization': '短編',
        'tags': art_tags,
        'all_tags': art_tags,
        'createDate': str(art_create_day.astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(art_update_day.astimezone(timezone(timedelta(hours=9)))),
        'episodes': episode
    }

    #小説データの差分を保存
    cm.save_raw_diff(raw_path, art_path, novel)

    #生データの書き出し
    with open(raw_path, 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)
    
    cn.narou_gen(novel, art_path, key_data, data_folder, host)
    print("")
    #仕上げ処理(indexファイルの更新)
    cm.gen_site_index(folder_path, key_data, 'Pixiv')

#漫画シリーズのダウンロードに関する処理
def dl_comic(comic_id, folder_path, key_data, update):
    global g_count
    arts = {}
    # comicのデータ取得
    logging.info(f"Comic ID: {comic_id}")
    c_detail = cm.find_key_recursively(json.loads(cm.get_with_cookie(f"https://www.pixiv.net/ajax/series/{comic_id}?p=1&lang=ja", pixiv_cookie, pixiv_header).text), "body")
    
    #イラストリンクの取得
    cache = c_detail
    arts = get_comic_link(cache, comic_id)
    if not arts:
        logging.error(f"Comic ID: {comic_id} is not available.")
        return

    c_title = c_detail['extraData']['meta']['twitter']['title']
    c_author = c_author = re.search(r'「[^」]*」/「(.*?)」のシリーズ \[pixiv\]', c_detail['extraData']['meta']['title']).group(1)
    c_author_id = re.search(r'user/(\d+)/series', c_detail['extraData']['meta']['canonical']).group(1)
    c_caption_data = c_detail['extraData']['meta']['description']
    if c_caption_data:
        c_caption = c_caption_data.replace('<br />', '\n').replace('jump.php?', '')
    else:
        c_caption = ''

    comic_tag = list(c_detail.get('tagTranslation', {}).keys())
    all_tags = comic_tag

    for j in c_detail['illustSeries']:
        if j['id'] == comic_id:
            c_create_day = datetime.fromisoformat(j['createDate'])
            c_update_day = datetime.fromisoformat(j['updateDate'])
            break

        else:
            c_create_day = str(datetime.now().astimezone(timezone(timedelta(hours=9))).strftime('%Y-%m-%d %H:%M:%S%z'))
            c_update_day = str(datetime.now().astimezone(timezone(timedelta(hours=9))).strftime('%Y-%m-%d %H:%M:%S%z'))

    logging.info(f"Comic Title: {c_title}")
    logging.info(f"Comic Author: {c_author}")
    logging.info(f"Comic Author ID: {c_author_id}")
    logging.info(f"Comic Caption: {c_caption}")
    logging.info(f"Comic Tags: {comic_tag}")
    logging.info(f"Comic Create Date: {c_create_day}")
    logging.info(f"Comic Update Date: {c_update_day}")
    cm.make_dir('c'+str(comic_id), folder_path)
    comic_path = os.path.join(folder_path, f'c{comic_id}')
    raw_path = os.path.join(comic_path, 'raw', 'raw.json')
    total_text = 0
    if update:
        if os.path.isfile(raw_path):
            with open (raw_path, 'r', encoding='utf-8') as f:
                old_episode_update_dates = json.load(f).get('episodes')
                is_update = True
        else:
            is_update = False
    
    episode = {}
    #画像順にソート
    arts = dict(sorted(arts.items()))

    for i, work_id in tqdm(arts.items(), desc=f"Downloading episodes", unit="episodes", total=len(arts), leave=False):
        
        os.makedirs(os.path.join(comic_path, work_id), exist_ok=True)

        ep_update = update

        #呼び出された処理が更新処理ですでにフォルダが存在する場合
        if update and is_update:
            for item in old_episode_update_dates.values():
                if item['id'] == work_id:
                    ep_json = item
                    for cu in arts.values():
                        if cu == work_id:
                            update_date = datetime.fromisoformat(str(cm.find_key_recursively(json.loads(cm.get_with_cookie(f"https://www.pixiv.net/ajax/illust/{cu}", pixiv_cookie, pixiv_header).text).get('uploadDate'), "body")))
                            if update_date == datetime.fromisoformat(item['updateDate']):
                                ep_update = True
                                break
                            else:
                                ep_update = False
                                break
        else:
            ep_update = False

        if ep_update:
            introduction = ep_json.get('introduction')
            postscript = ep_json.get('postscript')
            text = ep_json.get('text')
            createdate = ep_json.get('createDate')
            updatedate = ep_json.get('updateDate')
            tags = list(ep_json.get('tags', []))
            all_tags = list(dict.fromkeys(all_tags + ep_json.get('tags')))
            text_count = 0
            total_text = 0
        else:
            
            #BAN対策
            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
            g_count = 1

            #エピソードの処理
            episode_data = return_comic_content_json(work_id)
            if not episode_data:
                logging.error(f"Failed to download episode {work_id}")
                continue
            json_data = episode_data.get('body')
            ep_toc = cm.get_with_cookie(f"https://www.pixiv.net/ajax/illust/{work_id}/pages", pixiv_cookie, pixiv_header)

            ep_text = ''
            ep_art = ep_toc.json().get('body')
            for k in ep_art:
                url = k.get('urls').get('original')
                img_num = re.search(r'_p(\d+)\.', url).group(1)
                ep_text += f'[pixivimage:{work_id}-{int(img_num) + 1}]\n'

            #表紙のダウンロード
            if not os.path.isfile(os.path.join(comic_path, work_id, f'cover{os.path.splitext(json_data.get("urls").get("original"))[1]}')):
                get_cover(json_data.get('urls').get('original'), os.path.join(comic_path, work_id))
            else:
                logging.info(f"Cover image already exists: {json_data.get('urls').get('original')}")

            ep_tags = [tag.get('tag', '') for tag in json_data.get('tags', {}).get('tags', [])]

            all_tags = list(dict.fromkeys(all_tags + ep_tags))

            ep_caption_data = json_data.get('description')
            if ep_caption_data:
                ep_caption = ep_caption_data.replace('<br />', '\n').replace('jump.php?', '')
            else:
                ep_caption = ''
            ep_postscript_data = cm.find_key_recursively(json_data, 'pollData')
            if ep_postscript_data:
                ep_postscript = format_survey(ep_postscript_data)
            else:
                ep_postscript = ''
        
            #エピソードごとのフォルダの作成
            os.makedirs(os.path.join(comic_path, work_id), exist_ok=True)
            #挿絵リンクへの置き換え
            ep_text = format_image(comic_id, work_id, False, True, ep_text, json_data, folder_path)
            #作成日
            createdate = str(datetime.fromisoformat(json_data.get('createDate')).astimezone(timezone(timedelta(hours=9))))
            #更新日
            updatedate = str(datetime.fromisoformat(json_data.get('uploadDate')).astimezone(timezone(timedelta(hours=9))))

            text_count = 0

            total_text = 0

        #重複した漫画の除去
        if os.path.exists(os.path.join(folder_path, f'a{work_id}')):
            shutil.rmtree(os.path.join(folder_path, f'a{work_id}'))
            logging.info(f"Remove duplicated art folder: a{work_id}")

        episode[i] = {
            'id' : work_id,
            'chapter': None,
            'title': json_data.get('title'),
            'textCount': text_count,
            'tags': ep_tags,
            'introduction': unquote(ep_caption),
            'text': ep_text,
            'postscript': ep_postscript,
            'createDate': createdate,
            'updateDate': updatedate
        }

    # 作成日で並び替え
    sorted_episode = dict(sorted(episode.items(), key=lambda x: x[1]['createDate']))

    # インデックスを再設定
    episode = {i + 1: entry for i, (key, entry) in enumerate(sorted_episode.items())}

    novel = {
        'version': mv,
        'get_date': str(datetime.now().astimezone(timezone(timedelta(hours=9)))),
        'title': c_title,
        'id': comic_id,
        'url': f"https://www.pixiv.net/user/{c_author_id}/series/{comic_id}",
        'author': c_author,
        'author_id': c_author_id,
        'author_url': f"https://www.pixiv.net/users/{c_author_id}",
        'caption': c_caption,
        'total_episodes': len(episode),
        'all_episodes': len(episode),
        'total_characters': 0,
        'all_characters': 0,
        'type': 'comic',
        'serialization': '連載中',
        'tags': comic_tag,
        'all_tags': all_tags,
        'createDate': str(c_create_day.astimezone(timezone(timedelta(hours=9)))),
        'updateDate': str(c_update_day.astimezone(timezone(timedelta(hours=9)))),
        'episodes': episode
    }

    #小説データの差分を保存
    cm.save_raw_diff(raw_path, comic_path, novel)

    #生データの書き出し
    with open(raw_path, 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)
    
    cn.narou_gen(novel, comic_path, key_data, data_folder, host)
    print("")
    #仕上げ処理(indexファイルの更新)
    cm.gen_site_index(folder_path, key_data, 'Pixiv')

#ユーザーページからのダウンロード
def dl_user(user_id, folder_path, key_data, update):
    global g_count
    logging.info(f'User ID: {user_id}')
    user_data = cm.get_with_cookie(f"https://www.pixiv.net/ajax/user/{user_id}/profile/all", pixiv_cookie, pixiv_header).json()
    user_name = cm.get_with_cookie(f"https://www.pixiv.net/ajax/user/{user_id}", pixiv_cookie, pixiv_header).json().get('body').get('name')
    user_all_novels = user_data.get('body').get('novels')
    user_all_illusts = user_data.get('body').get('illusts')
    user_all_mangas = user_data.get('body').get('manga')
    user_all_novel_series = user_data.get('body').get('novelSeries')
    user_all_manga_series = user_data.get('body').get('mangaSeries')
    user_novel_series = []
    user_manga_series = []
    in_novel_series = []
    in_manga_series = []
    user_novels = []
    user_mangas = []
    user_arts = []
    logging.info(f'User Name: {user_name}')

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
        data["version"] = 3
        data[user_id] = {}
        data[user_id]['novel'] = "enable"
        data[user_id]['comic'] = "enable"

    # 更新されたデータを再び user.json に書き込む
    with open(user_json, 'w', encoding='utf-8') as f:
        json.dump(data, f, ensure_ascii=False, indent=4)

    #小説シリーズIDの取得
    for ns in user_all_novel_series:
        user_novel_series.append(ns.get('id'))

    #漫画シリーズIDの取得
    for ms in user_all_manga_series:
        user_manga_series.append(ms.get('id'))

    #小説IDの取得
    if user_all_novels:
        user_novels = list(user_all_novels.keys())

    #漫画IDの取得
    if user_all_mangas:
        user_mangas = list(user_all_mangas.keys())

    #イラストIDの取得
    if user_all_illusts:
        user_arts = list(user_all_illusts.keys())

    #小説シリーズとの重複を除去
    for i in user_novel_series:
        time.sleep(interval_sec)
        for nid in cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{i}/content_titles", pixiv_cookie, pixiv_header).json().get('body'):
            in_novel_series.append(nid.get('id'))
            if os.path.exists(os.path.join(folder_path, f'n{nid.get('id')}')):
                shutil.rmtree(os.path.join(folder_path, f'n{nid.get('id')}'))
                logging.info(f"Remove duplicated novel folder: n{nid.get('id')}")

        if g_count >= 10:
            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
            g_count = 1
        else:
            time.sleep(interval_sec)
            g_count += 1

    #漫画シリーズとの重複を除去
    for i in user_manga_series:
        arts = {}
        #イラストリンクの取得
        cache = cm.find_key_recursively(json.loads(cm.get_with_cookie(f"https://www.pixiv.net/ajax/series/{i}?p=1&lang=ja", pixiv_cookie, pixiv_header).text), "body")
        arts = get_comic_link(cache, i)
        if not arts:
            logging.error(f"Comic ID: {i} is not available.")
            continue
        arts = dict(sorted(arts.items()))

        for j, art_id in arts.items():
            in_manga_series.append(art_id)
            if os.path.exists(os.path.join(folder_path, f'a{art_id}')):
                shutil.rmtree(os.path.join(folder_path, f'a{art_id}'))
                logging.info(f"Remove duplicated art folder: a{art_id}")
        
        if g_count >= 10:
            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
            g_count = 1
        else:
            time.sleep(interval_sec)
            g_count += 1
        
    user_mangas = user_mangas + user_arts

    if user_all_novels:
        user_novels = [n for n in user_novels if n not in in_novel_series]
    if user_all_mangas:
        user_mangas = [m for m in user_mangas if m not in in_manga_series]
    logging.info(f'User Novels: {len(user_novels)}')
    logging.info(f'User Novel Series: {len(user_novel_series)}')
    logging.info(f'User Mangas: {len(user_mangas)}')
    logging.info(f'User Manga Series: {len(user_manga_series)}')

    #小説のダウンロード
    if data[user_id]['novel'] == "enable":
        logging.info("Novel Series Download Start")
        for series_id in user_novel_series:
            if update:
                raw_path = os.path.join(folder_path, f's{series_id}', 'raw', 'raw.json')
                if os.path.isfile(raw_path):
                    series_update_date = datetime.fromisoformat(cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}", pixiv_cookie, pixiv_header).json().get('body').get('updateDate'))
                    with open (raw_path, 'r', encoding='utf-8') as f:
                        old_series_json = json.load(f)
                    series_old_update_date = datetime.fromisoformat(old_series_json.get('updateDate'))
                    if series_update_date == series_old_update_date:
                        logging.info(f"{old_series_json['title']} に更新はありません。")
                        if g_count >= 10:
                            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                            g_count = 1
                        else:
                            time.sleep(interval_sec)
                            g_count += 1
                        continue

                    if g_count >= 10:
                        time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                        g_count = 1
                    else:
                        time.sleep(interval_sec)
                        g_count += 1

                    dl_series(series_id, folder_path, key_data, True)
            else:
                if g_count >= 10:
                    time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                    g_count = 1
                else:
                    time.sleep(interval_sec)
                    g_count += 1
                dl_series(series_id, folder_path, key_data, False)

        logging.info("Novel Download Start")
        for novel_id in user_novels:
            if update:
                raw_path = os.path.join(folder_path, f'n{novel_id}', 'raw', 'raw.json')
                if os.path.isfile(raw_path):
                    novel_update_date = datetime.fromisoformat(return_content_json(novel_id).get('body').get('uploadDate'))
                    with open (raw_path, 'r', encoding='utf-8') as f:
                        old_novel_json = json.load(f)
                    novel_old_update_date = datetime.fromisoformat(old_novel_json.get('updateDate'))
                    if novel_update_date == novel_old_update_date:
                        logging.info(f"{old_novel_json['title']} に更新はありません。")
                        if g_count >= 10:
                            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                            g_count = 1
                        else:
                            time.sleep(interval_sec)
                            g_count += 1
                        continue
            else:
                if g_count >= 10:
                    time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                    g_count = 1
                else:
                    time.sleep(interval_sec)
                    g_count += 1
            dl_novel(return_content_json(novel_id), novel_id, folder_path, key_data)
    else:
        logging.info("Novel and Novel Series Download Skipped")

    #漫画のダウンロード
    if data[user_id]['comic'] == "enable":
        logging.info("Comic Series Download Start")
        for comic_id in user_manga_series:
            if update:
                raw_path = os.path.join(folder_path, f'c{comic_id}', 'raw', 'raw.json')
                if os.path.isfile(raw_path):
                    comic_detail = cm.find_key_recursively(json.loads(cm.get_with_cookie(f"https://www.pixiv.net/ajax/series/{comic_id}?p=1&lang=ja", pixiv_cookie, pixiv_header).text), "body")
                    for j in comic_detail['illustSeries']:
                        if j['id'] == comic_id:
                            comic_update_date = datetime.fromisoformat(j['updateDate'])
                            break
                        else:
                            comic_update_date = str(datetime.now().astimezone(timezone(timedelta(hours=9))).strftime('%Y-%m-%d %H:%M:%S%z'))
                    with open (raw_path, 'r', encoding='utf-8') as f:
                        old_comic_json = json.load(f)
                    comic_old_update_date = datetime.fromisoformat(old_comic_json.get('updateDate'))
                    if str(comic_update_date) == str(comic_old_update_date):
                        logging.info(f"{old_comic_json['title']} に更新はありません。")
                        if g_count >= 10:
                            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                            g_count = 1
                        else:
                            time.sleep(interval_sec)
                            g_count += 1
                        continue

                    if g_count >= 10:
                        time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                        g_count = 1
                    else:
                        time.sleep(interval_sec)
                        g_count += 1

                    dl_comic(comic_id, folder_path, key_data, True)
            else:
                time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                g_count = 1
                dl_comic(comic_id, folder_path, key_data, False)

        logging.info("Comic Download Start")
        for art_id in user_mangas:
            if update:
                raw_path = os.path.join(folder_path, f'a{art_id}', 'raw', 'raw.json')
                if os.path.isfile(raw_path):
                    art_detail = return_comic_content_json(art_id)
                    if not art_detail:
                        logging.error(f"Failed to download episode {art_id}")
                        continue
                    art_update_date = datetime.fromisoformat(art_detail.get('body').get('uploadDate'))
                    with open (raw_path, 'r', encoding='utf-8') as f:
                        old_art_json = json.load(f)
                    art_old_update_date = datetime.fromisoformat(old_art_json.get('updateDate'))
                    if art_update_date == art_old_update_date:
                        logging.info(f"{old_art_json['title']} に更新はありません。")
                        if g_count >= 10:
                            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                            g_count = 1
                        else:
                            time.sleep(interval_sec)
                            g_count += 1
                        continue
                else:
                    if g_count >= 10:
                        time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                        g_count = 1
                    else:
                        time.sleep(interval_sec)
                        g_count += 1

            else:
                if g_count >= 10:
                    time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                    g_count = 1
                else:
                    time.sleep(interval_sec)
                    g_count += 1
            
            dl_art(art_id, folder_path, key_data)
    
    else:
        logging.info("Comic and Comic Series Download Skipped")

#ダウンロード処理
def download(url, folder_path, key_data, data_path, host_name):

    #引き渡し用変数
    global data_folder
    global host
    global g_count
    data_folder = data_path
    host = host_name

    response = cm.get_with_cookie(url, pixiv_cookie, pixiv_header)

    if response.status_code == 404:
        logging.error("404 Not Found")
        logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
        return
    logging.info(f'Response Status Code: {response.status_code}')
    if "https://www.pixiv.net/novel/show.php?id=" in url:
        # JSONとして解析
        novel_id = re.search(r"id=(\d+)", url).group(1)
        json_data = return_content_json(novel_id)
        series_nav_data = cm.find_key_recursively(json_data, "seriesNavData")
        if series_nav_data:
            series_id = series_nav_data.get("seriesId")
            dl_series(series_id, folder_path, key_data, False)
        else:
            dl_novel(json_data, novel_id, folder_path, key_data) #ダウンロード処理
    elif "https://www.pixiv.net/novel/series/" in url:
        series_id = re.search(r"series/(\d+)", url).group(1)
        dl_series(series_id, folder_path, key_data, False)
    elif "https://www.pixiv.net/users/" in url:
        user_id = re.search(r"users/(\d+)", url).group(1)
        dl_user(user_id, folder_path, key_data, False)
    elif "https://www.pixiv.net/artworks/" in url: # イラストと漫画の分岐
        art_id = re.search(r"artworks/(\d+)", url).group(1)
        dl_art(art_id, folder_path, key_data)
    elif re.search(r"https://www\.pixiv\.net/user/(\d+)/series/(\d+)", url): # 漫画シリーズの分岐
        comic_id = re.search(r"user/(\d+)/series/(\d+)", url).group(2)
        dl_comic(comic_id, folder_path, key_data, False)
    else:
        logging.error(f'Error: "{url}" is not a valid URL')
        return

    logging.info("Download Complete")

#更新処理
def update(folder_path, key_data, data_path, host_name):

    #引き渡し用変数
    global data_folder
    global host
    global g_count
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
            if user_id == "version":
                continue

            if g_count >= 10:
                time.sleep(random.uniform(interval_sec*5,interval_sec*10))
                g_count = 1
            else:
                time.sleep(interval_sec)
                g_count += 1

            flag = False
            if not flag and status['novel'] == 'enable':
                if cm.get_with_cookie(f'https://www.pixiv.net/users/{user_id}/novels', pixiv_cookie, pixiv_header).status_code == 404:
                    logging.error("404 Not Found")
                    logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                    return
                dl_user(user_id, folder_path, key_data, True)
                time.sleep(interval_sec)
                flag = True
            if not flag and status["comic"] == 'enable':
                manga = cm.get_with_cookie(f'https://www.pixiv.net/users/{user_id}/manga', pixiv_cookie, pixiv_header)
                illust = cm.get_with_cookie(f'https://www.pixiv.net/users/{user_id}/illustrations', pixiv_cookie, pixiv_header)
                if manga.status_code == 404 and illust.status_code == 404:
                    logging.error("404 Not Found")
                    logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                    return
                dl_user(user_id, folder_path, key_data, True)
                time.sleep(interval_sec)
                flag = True
            user_ids.append(user_id)
                

    for folder_name, index_data in index_json.items():
        if index_data.get("author_id") in user_ids:
            continue
        if index_data.get("serialization") == "短編" and index_data.get("type") == "novel":
            novel_id = folder_name.replace('n', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/novel/show.php?id={novel_id}', pixiv_cookie, pixiv_header).status_code == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue
            json_data = return_content_json(novel_id)
            with open(os.path.join(folder_path, folder_name, 'raw', 'raw.json'), 'r', encoding='utf-8') as onf:
                old_novel_json = json.load(onf)
            if datetime.fromisoformat(json_data.get('body').get('uploadDate')) != datetime.fromisoformat(old_novel_json.get('updateDate')):
                dl_novel(json_data, novel_id, folder_path, key_data)
            else:
                logging.info(f'{index_data.get("title")} に更新はありません。')

        elif index_data.get("serialization") in ["連載中", "完結"] and index_data.get("type") == "novel":
            series_id = folder_name.replace('s', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/novel/series/{series_id}', pixiv_cookie, pixiv_header).status_code == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue
            s_detail = cm.find_key_recursively(json.loads(cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}", pixiv_cookie, pixiv_header).text), "body")
            with open(os.path.join(folder_path, folder_name, 'raw', 'raw.json'), 'r', encoding='utf-8') as osf:
                old_series_json = json.load(osf)

            if datetime.fromisoformat(s_detail.get('updateDate')) != datetime.fromisoformat(old_series_json.get('updateDate')):
                dl_series(series_id, folder_path, key_data, True)
            else:
                logging.info(f'{index_data.get("title")} に更新はありません。')

        elif index_data.get("serialization") == "短編" and index_data.get("type") == "comic":
            art_id = folder_name.replace('a', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/ajax/illust/{art_id}', pixiv_cookie, pixiv_header).status_code == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue
            json_data = return_comic_content_json(art_id)
            if not json_data:
                logging.error(f"Failed to download episode {art_id}")
                continue
            with open(os.path.join(folder_path, folder_name, 'raw', 'raw.json'), 'r', encoding='utf-8') as onf:
                old_novel_json = json.load(onf)
            if datetime.fromisoformat(json_data.get('body').get('uploadDate')) != datetime.fromisoformat(old_novel_json.get('updateDate')):
                dl_novel(json_data, art_id, folder_path, key_data)
            else:
                logging.info(f'{index_data.get("title")} に更新はありません。')

        elif index_data.get("serialization") in ["連載中", "完結"] and index_data.get("type") == "comic":
            comic_id = folder_name.replace('c', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/user/{index_data.get("author_id")}/series/{comic_id}', pixiv_cookie, pixiv_header) == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue
            c_detail = cm.find_key_recursively(json.loads(cm.get_with_cookie(f"https://www.pixiv.net/ajax/series/{comic_id}?p=1&lang=ja", pixiv_cookie, pixiv_header).text), "body")
            with open(os.path.join(folder_path, folder_name, 'raw', 'raw.json'), 'r', encoding='utf-8') as osf:
                old_series_json = json.load(osf)

            for j in c_detail['illustSeries']:
                if j['id'] == comic_id:
                    c_update_day = datetime.fromisoformat(j['updateDate'])
                    break
                else:
                    c_update_day = str(datetime.now().astimezone(timezone(timedelta(hours=9))).strftime('%Y-%m-%d %H:%M:%S%z'))

            if str(c_update_day) != str(datetime.fromisoformat(old_series_json.get('updateDate'))):
                dl_comic(comic_id, folder_path, key_data, True)
            else:
                logging.info(f'{index_data.get("title")} に更新はありません。')


        if g_count >= 10:
            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
            g_count = 1
        else:
            time.sleep(interval_sec)
            g_count += 1
    
    cm.gen_site_index(folder_path, key_data, 'Pixiv')

#再ダウンロード処理
def re_download(folder_path, key_data, data_path, host_name):
    #引き渡し用変数
    global data_folder
    global host
    global g_count
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
            if user_id == "version":
                continue
            flag = False
            if not flag and status['novel'] == 'enable':
                if cm.get_with_cookie(f'https://www.pixiv.net/users/{user_id}/novels', pixiv_cookie, pixiv_header).status_code == 404:
                    logging.error("404 Not Found")
                    logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                    return
                dl_user(user_id, folder_path, key_data, False)
                time.sleep(interval_sec)
                flag = True
            if not flag and status["comic"] == 'enable':
                manga = cm.get_with_cookie(f'https://www.pixiv.net/users/{user_id}/manga', pixiv_cookie, pixiv_header)
                illust = cm.get_with_cookie(f'https://www.pixiv.net/users/{user_id}/illustrations', pixiv_cookie, pixiv_header)
                if manga.status_code == 404 and illust.status_code == 404:
                    logging.error("404 Not Found")
                    logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                    return
                dl_user(user_id, folder_path, key_data, False)
                time.sleep(interval_sec)
                flag = True
            user_ids.append(user_id)
                

    for folder_name, index_data in index_json.items():
        if index_data.get("author_id") in user_ids:
            continue
        if index_data.get("serialization") == "短編" and index_data.get("type") == "novel":
            novel_id = folder_name.replace('n', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/novel/show.php?id={novel_id}', pixiv_cookie, pixiv_header).status_code == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue

            json_data = return_content_json(novel_id)
            dl_novel(json_data, novel_id, folder_path, key_data)


        elif index_data.get("serialization") in ["連載中", "完結"] and index_data.get("type") == "novel":
            series_id = folder_name.replace('s', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/novel/series/{series_id}', pixiv_cookie, pixiv_header).status_code == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue

            dl_series(series_id, folder_path, key_data, False)

        elif index_data.get("serialization") == "短編" and index_data.get("type") == "comic":
            art_id = folder_name.replace('a', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/ajax/illust/{art_id}', pixiv_cookie, pixiv_header).status_code == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue

            dl_art(art_id, folder_path, key_data)

        elif index_data.get("serialization") in ["連載中", "完結"] and index_data.get("type") == "comic":
            comic_id = folder_name.replace('c', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/user/{index_data.get("author_id")}/series/{comic_id}', pixiv_cookie, pixiv_header) == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue

            dl_comic(comic_id, folder_path, key_data, False)

        if g_count >= 10:
            time.sleep(random.uniform(interval_sec*5,interval_sec*10))
            g_count = 1
        else:
            time.sleep(interval_sec)
            g_count += 1
    
    cm.gen_site_index(folder_path, key_data, 'Pixiv')

#変換処理
def convert(folder_path, key_data, data_path, host_name):

    #引き渡し用変数
    data_folder = data_path
    host = host_name

    folder_names = [name for name in os.listdir(folder_path) if os.path.isdir(os.path.join(folder_path, name))]

    for q in folder_names:
        if os.path.exists(os.path.join(folder_path, q, 'raw', 'raw.json')) and os.path.exists(os.path.join(folder_path, q, 'info', 'index.html')):
            with open(os.path.join(folder_path, q, 'raw', 'raw.json'), 'r', encoding='utf-8') as f:
                raw_json_data = json.load(f)
            cn.narou_gen(raw_json_data, os.path.join(folder_path, q), key_data, data_folder, host)

    cm.gen_site_index(folder_path, key_data, 'Pixiv')