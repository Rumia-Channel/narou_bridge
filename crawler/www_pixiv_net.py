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

#ログを保存
import logging

#リキャプチャ対策
import time
import random
from tqdm import tqdm

#共通の処理
import crawler.common as cm
import crawler.convert_narou as cn

def gen_pixiv_index(folder_path ,key_data):
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
                author_id = data.get('author_url', 'No author_id found').replace('https://www.pixiv.net/users/', '')
                create_date = data.get('createDate', 'No create date found')
                update_date = data.get('updateDate', 'No update date found')
                type = data.get('type', 'No type found')
                pairs[folder] = {'title': title, 'author': author, 'author_id': author_id,'type': type, 'create_date': create_date, 'update_date': update_date}
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
        f.write('<title>Pixiv Index</title>\n')
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

        f.write('<h1>Pixiv 小説一覧</h1>\n')
        f.write('<table>\n')
        f.write('<tr><th>掲載タイプ</th><th>タイトル</th><th>作者名</th><th>掲載日時</th><th>更新日時</th></tr>\n')
        
        # 各行のデータ出力
        for folder, info in pairs.items():
            f.write(f'''<tr><td>{info["type"]}</td>
                        <td class="text"><a href="{folder}/{key_data}" class="text">{info["title"]}</a></td>
                        <td class="text"><a href="https://www.pixiv.net/users/{info["author_id"]}">{info["author"]}</a></td>
                        <td>{datetime.strptime(info["create_date"], "%Y-%m-%d %H:%M:%S%z").strftime("%Y/%m/%d %H:%M")}</td>
                        <td>{datetime.strptime(info["update_date"], "%Y-%m-%d %H:%M:%S%z").strftime("%Y/%m/%d %H:%M")}</td></tr>\n''')
        
        f.write('</table>\n')
        
        # JavaScriptによる折り返し処理
        f.write("""<script>
            // テキスト折り返し関数
            const wrapTextByLength = (text, maxLength) => {
                // 指定した長さでテキストを分割し、<br>タグで改行を追加
                return text.match(new RegExp(`.{1,${maxLength}}`, 'g')).join('<br>');
            };

            // localStorageから折り返し文字数を取得し、なければデフォルト（10文字）を使用
            let maxLength = localStorage.getItem('maxLength') || 10;

            // 数値入力欄にローカルストレージの値を表示
            document.getElementById('maxLengthInput').value = maxLength;

            // テーブル内の特定の列を対象に折り返し処理を適用
            document.querySelectorAll('table tr').forEach(row => {
                // タイトル列（2列目）と作者名列（3列目）を取得
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

            // 保存ボタンのイベント
            document.getElementById('saveButton').addEventListener('click', () => {
                const inputValue = document.getElementById('maxLengthInput').value;
                if (inputValue && inputValue > 0) {
                    // 新しい文字数をローカルストレージに保存
                    localStorage.setItem('maxLength', inputValue);

                    // 保存後、ページをリロードして変更を適用
                    location.reload();
                }
            });
        </script>""")
        
        f.write('</body>\n')
        f.write('</html>\n')

    
    with open(os.path.join(folder_path, 'index.json'), 'w', encoding='utf-8') as f:
        json.dump(pairs, f, ensure_ascii=False, indent=4)

    if no_raw:
        logging.warning(f"The folders {', '.join(no_raw)} were deleted because they do not contain 'raw.json'.\n")

    logging.info('目次の生成が完了しました')

#初期化処理
def init(cookie_path, is_login, interval):

    cookie_path = os.path.join(cookie_path, 'login.json')

    global interval_sec
    global g_count
    interval_sec = int(interval)
    g_count = 1

    logging.info(f'Login : {is_login}')

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
        if "accounts.pixiv.net" in page.url:  # Pixivのログインページに留まっている場合
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

# レスポンスからjsonデータ(本文データ)を返却
def return_content_json(novelid):
    novel_data = cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/{novelid}", pixiv_cookie, pixiv_header).text
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

#ルビ形式の整形
def format_ruby(data):
    pattern = re.compile(r"\[\[rb:(.*?) > (.*?)\]\]")
    return re.sub(pattern, lambda match: f'[ruby:<{match.group(1)}>({match.group(2)})]', data)

#画像リンク形式の整形
def format_image(id, episode, series, data, json_data, folder_path):
    global g_count
    #pixivimage: で始まるリンクの抽出
    links = re.findall(r"\[pixivimage:(\d+)-(\d+)\]", data)
    link_dict = {}
    #uploadedimage: で始まるリンクの抽出
    inner_links = re.findall(r"\[uploadedimage:(\d+)\]", data)

    #シリーズとその他のリンクの切り替え
    if series:
        episode_path = os.path.join(folder_path, f's{id}', str(episode))
        url = f"https://www.pixiv.net/novel/series/{id}/{episode}"
    else:
        episode_path = os.path.join(folder_path, f'n{id}')
        url = f"https://www.pixiv.net/novel/show.php?id={id}"

    for i in links:
        art_id = i[0]
        img_num = i[1]
        if art_id not in link_dict:
            link_dict[art_id] = []  # art_id が存在しない場合に空のリストを初期化
        link_dict[art_id].append(img_num)
    #画像リンクの形式を[リンク名](リンク先)に変更
    for art_id, img_nums in link_dict.items():
        illust_json = cm.get_with_cookie(f"https://www.pixiv.net/ajax/illust/{art_id}/pages", pixiv_cookie, pixiv_header).json()
        illust_datas = cm.find_key_recursively(illust_json, 'body')
        for index, i in tqdm(enumerate(illust_datas), desc=f"Downloading illusts from https://www.pixiv.net/artworks/{art_id}", unit="illusts", total=len(illust_datas), leave=False):
            time.sleep(interval_sec)
            if str(index + 1) in img_nums:
                img_url = i.get('urls').get('original')
                img_data = cm.get_with_cookie(img_url, pixiv_cookie, pixiv_header)
                with open(os.path.join(episode_path, f'{art_id}_p{index}{os.path.splitext(img_url)[1]}'), 'wb') as f:
                    f.write(img_data.content)
                data = data.replace(f'[pixivimage:{art_id}-{index + 1}]', f'[image]({art_id}_p{index}{os.path.splitext(img_url)[1]})')
    #小説内アップロードの画像リンクの形式を[リンク名](リンク先)に変更
    for inner_link in tqdm(inner_links, desc=f"Downloading inner illusts from {url}", unit="illusts", total=len(inner_links), leave=False):
        time.sleep(interval_sec)
        in_img_url = cm.find_key_recursively(json_data, inner_link).get('urls').get('original')
        in_img_data = cm.get_with_cookie(in_img_url, pixiv_cookie, pixiv_header)
        with open(os.path.join(episode_path, f'{inner_link}{os.path.splitext(in_img_url)[1]}'), 'wb') as f:
            f.write(in_img_data.content)
        data = data.replace(f'[uploadedimage:{inner_link}]', f'[image]({inner_link}{os.path.splitext(in_img_url)[1]})')

    return data

# 各話の表紙のダウンロード
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

    # 全てのURLが404だった場合
    logging.error("Failed to download cover image.")

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
            text_count = int(ep_json.get('textCount'))
            total_text += int(ep_json.get('textCount'))
        else:

            #BAN対策
            if g_count == 10:
                time.sleep(random.uniform(10,30))
                g_count = 1
            else:
                time.sleep(interval_sec)
                g_count += 1

            #エピソードの処理
            json_data = return_content_json(entry['id'])

            #表紙のダウンロード
            get_cover(json_data.get('body').get('coverUrl'), os.path.join(series_path, entry['id']))


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
            text = format_image(series_id, entry['id'], True, text, json_data, folder_path)
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

        episode[i] = {
            'id' : entry['id'],
            'chapter': None,
            'title': entry['title'],
            'textCount': text_count,
            'introduction': unquote(introduction),
            'text': text,
            'postscript': postscript,
            'createDate': createdate,
            'updateDate': updatedate
        }

    # 作成日で並び替え
    #episode = dict(sorted(episode.items(), key=lambda x: x[1]['createDate']))
    
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
        'total_characters': total_text,
        'all_characters': series_chara,
        'type': '連載中',
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
    novel_create_day = datetime.fromisoformat(novel_data.get('createDate'))
    novel_update_day = datetime.fromisoformat(novel_data.get('uploadDate'))
    logging.info(f"Novel ID: {novel_id}")
    logging.info(f"Novel Title: {novel_title}")
    logging.info(f"Novel Author: {novel_author}")
    logging.info(f"Novel Author ID: {novel_author_id}")
    logging.info(f"Novel Caption: {novel_caption}")
    logging.info(f"Novel Create Date: {novel_create_day}")
    logging.info(f"Novel Update Date: {novel_update_day}")
    cm.make_dir('n'+str(novel_id), folder_path)
    novel_path = os.path.join(folder_path, f'n{novel_id}')
    raw_path = os.path.join(novel_path, 'raw', 'raw.json')
    #挿絵リンクへの置き換え
    text = format_image(novel_id, novel_id, False, novel_text, json_data, folder_path)
    #表紙のダウンロード
    get_cover(novel_data.get('coverUrl'), novel_path)
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

    #小説データの差分を保存
    cm.save_raw_diff(raw_path, novel_path, novel)

    #生データの書き出し
    with open(raw_path, 'w', encoding='utf-8') as f:
        json.dump(novel, f, ensure_ascii=False, indent=4)

    cn.narou_gen(novel, novel_path, key_data, data_folder, host)
    print("")

#ユーザーページからのダウンロード
def dl_user(user_id, folder_path, key_data, update):
    global g_count
    logging.info(f'User ID: {user_id}')
    user_data = cm.get_with_cookie(f"https://www.pixiv.net/ajax/user/{user_id}/profile/all", pixiv_cookie, pixiv_header).json()
    user_name = cm.get_with_cookie(f"https://www.pixiv.net/ajax/user/{user_id}", pixiv_cookie, pixiv_header).json().get('body').get('name')
    user_all_novels = user_data.get('body').get('novels')
    user_all_novel_series = user_data.get('body').get('novelSeries')
    user_novel_series = []
    in_novel_series = []
    user_novels = []
    logging.info(f'User Name: {user_name}')
    #ユーザーの小説と小説シリーズがない場合
    if not user_all_novels and not user_all_novel_series:
        logging.error("No novels or novel series found.\n")
        return
    #シリーズIDの取得
    for ns in user_all_novel_series:
        user_novel_series.append(ns.get('id'))
    #小説IDの取得
    user_novels = list(user_all_novels.keys())
    #シリーズとの重複を除去
    for i in user_novel_series:
        time.sleep(interval_sec)
        for nid in cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{i}/content_titles", pixiv_cookie, pixiv_header).json().get('body'):
            in_novel_series.append(nid.get('id'))
    user_novels = [n for n in user_novels if n not in in_novel_series]
    logging.info(f'User Novels: {len(user_novels)}')
    logging.info(f'User Novel Series: {len(user_novel_series)}')

    logging.info("\nSeries Download Start\n")
    for series_id in user_novel_series:
        if update:
            raw_path = os.path.join(folder_path, f's{series_id}', 'raw', 'raw.json')
            if os.path.isfile(raw_path):
                series_update_date = datetime.fromisoformat(cm.get_with_cookie(f"https://www.pixiv.net/ajax/novel/series/{series_id}", pixiv_cookie, pixiv_header).json().get('body').get('updateDate'))
                with open (raw_path, 'r', encoding='utf-8') as f:
                    old_series_json = json.load(f)
                series_old_update_date = datetime.fromisoformat(old_series_json.get('updateDate'))
                if series_update_date == series_old_update_date:
                    logging.info(f"{old_series_json['title']} に更新はありません。\n")
                    if g_count == 10:
                        time.sleep(random.uniform(10,30))
                        g_count = 1
                    else:
                        time.sleep(interval_sec)
                        g_count += 1
                    continue
                dl_series(series_id, folder_path, key_data, True)
        else:
            if g_count == 10:
                time.sleep(random.uniform(10,30))
                g_count = 1
            else:
                time.sleep(interval_sec)
                g_count += 1
            dl_series(series_id, folder_path, key_data, False)

    logging.info("\nNovel Download Start\n")
    for novel_id in user_novels:
        if update:
            raw_path = os.path.join(folder_path, f'n{novel_id}', 'raw', 'raw.json')
            if os.path.isfile(raw_path):
                novel_update_date = datetime.fromisoformat(return_content_json(novel_id).get('body').get('uploadDate'))
                with open (raw_path, 'r', encoding='utf-8') as f:
                    old_novel_json = json.load(f)
                novel_old_update_date = datetime.fromisoformat(old_novel_json.get('updateDate'))
                if novel_update_date == novel_old_update_date:
                    logging.info(f"{old_novel_json['title']} に更新はありません。\n")
                    if g_count == 10:
                        time.sleep(random.uniform(10,30))
                        g_count = 1
                    else:
                        time.sleep(interval_sec)
                        g_count += 1
                    continue
        else:
            if g_count == 10:
                time.sleep(random.uniform(10,30))
                g_count = 1
            else:
                time.sleep(interval_sec)
                g_count += 1
        dl_novel(return_content_json(novel_id), novel_id, folder_path, key_data)

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
    else:
        logging.error(f'Error: "{url}" is not a valid URL')
        return
    
    #仕上げ処理(indexファイルの更新)
    gen_pixiv_index(folder_path, key_data)

    logging.info("\nDownload Complete\n")

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
            if status == 'enable':
                if cm.get_with_cookie(f'https://www.pixiv.net/users/{user_id}/novels', pixiv_cookie, pixiv_header).status_code == 404:
                    logging.error("404 Not Found")
                    logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                    return
                dl_user(user_id, folder_path, key_data, True)
                time.sleep(interval_sec)
            user_ids.append(user_id)
                

    for folder_name, index_data in index_json.items():
        if index_data.get("author_id") in user_ids:
            continue
        if index_data.get("type") == "短編":
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
                logging.info(f'{index_data.get("title")} に更新はありません。\n')

        elif index_data.get("type") == "連載中" or index_data.get("type") == "完結":
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
                logging.info(f'{index_data.get("title")} に更新はありません。\n')
        if g_count == 10:
            time.sleep(random.uniform(10,30))
            g_count = 1
        else:
            time.sleep(interval_sec)
            g_count += 1
    
    gen_pixiv_index(folder_path, key_data)

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
            if status == 'enable':
                if cm.get_with_cookie(f'https://www.pixiv.net/users/{user_id}/novels', pixiv_cookie, pixiv_header).status_code == 404:
                    logging.error("404 Not Found")
                    logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                    return
                dl_user(user_id, folder_path, key_data, False)
                time.sleep(interval_sec)
            user_ids.append(user_id)
                

    for folder_name, index_data in index_json.items():
        if index_data.get("author_id") in user_ids:
            continue
        if index_data.get("type") == "短編":
            novel_id = folder_name.replace('n', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/novel/show.php?id={novel_id}', pixiv_cookie, pixiv_header).status_code == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue

            json_data = return_content_json(novel_id)
            dl_novel(json_data, novel_id, folder_path, key_data)


        elif index_data.get("type") == "連載中" or index_data.get("type") == "完結":
            series_id = folder_name.replace('s', '')
            if cm.get_with_cookie(f'https://www.pixiv.net/novel/series/{series_id}', pixiv_cookie, pixiv_header).status_code == 404:
                logging.error("404 Not Found")
                logging.error("Incorrect URL, Deleted, Private, or My Pics Only.")
                continue

            dl_series(series_id, folder_path, key_data, False)

        if g_count == 10:
            time.sleep(random.uniform(10,30))
            g_count = 1
        else:
            time.sleep(interval_sec)
            g_count += 1
    
    gen_pixiv_index(folder_path, key_data)

#変換処理
def convert(folder_path, key_data, data_path, host_name):

    #引き渡し用変数
    data_folder = data_path
    host = host_name

    folder_names = [name for name in os.listdir(folder_path) if os.path.isdir(os.path.join(folder_path, name))]

    for i in folder_names:
        if os.path.exists(os.path.join(folder_path, i, 'raw', 'raw.json')) and os.path.exists(os.path.join(folder_path, i, 'info', 'index.html')):
            with open(os.path.join(folder_path, i, 'raw', 'raw.json'), 'r', encoding='utf-8') as f:
                raw_json_data = json.load(f)
            cn.narou_gen(raw_json_data, os.path.join(folder_path, i), key_data, data_folder, host)

    gen_pixiv_index(folder_path, key_data)