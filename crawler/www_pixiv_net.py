import re
import os
import shutil
import requests
import json
import time
import random
import functools
import zipfile
import logging
import tempfile
import hashlib
from urllib.parse import unquote, urlparse
from html import unescape
from datetime import datetime, timezone, timedelta
from typing import Optional, Dict, List, Any, Union, Set

# 外部ライブラリ
from playwright.sync_api import Playwright, sync_playwright, TimeoutError
from playwright_recaptcha import recaptchav2
from tqdm import tqdm
from PIL import Image
import apng  # 必要に応じて有効化

# 内部モジュール (環境に合わせてパス解決してください)
import crawler.common as cm
import crawler.convert_narou as cn
from crawler.common import safe_fromiso

# --- 定数定義 ---
VERSION = 5
JST = timezone(timedelta(hours=9))
DEFAULT_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"

# --- ユーティリティ関数 ---

def suppress_errors(default=None):
    """例外を握りつぶしてログ出力するデコレータ"""
    def decorator(func):
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            try:
                return func(*args, **kwargs)
            except Exception as e:
                logging.error(f"[{func.__name__}] 例外発生: {e}", exc_info=True)
                return default
        return wrapper
    return decorator

def format_tags(tags: List[Union[str, Dict]]) -> List[str]:
    """タグリストを整形する"""
    result = []
    seen = set()
    for t in tags:
        if not t: continue
        if isinstance(t, dict):
            t = t.get('tag', '')
        for tag in t.split('#'):
            tag = tag.strip()
            if not tag: continue
            if tag.startswith('#'):
                tag = tag[1:]
            if tag and tag not in seen:
                result.append(tag)
                seen.add(tag)
    return result

def remove_chapter_tag(text: str) -> str:
    """[chapter:...]タグを整形する"""
    def replacer(match):
        before = match.group(1)
        content = match.group(3).replace('\n', '')
        if before and before[-1] != '\n':
            return f'{before}\n{content}\n\n\n'
        return f'{before}{content}\n\n\n'
    return re.sub(r"(.*?)(\[chapter:(.*?)\])", replacer, text, flags=re.DOTALL)

def format_for_url(text: str) -> str:
    """[[jumpuri:...]]タグをHTMLリンクに変換"""
    def repl(match):
        txt = match.group(1)
        url = match.group(2)
        if url.startswith('/http://') or url.startswith('/https://'):
            url = url[1:]
        return f'<a href="{url}">{txt}</a>'
    return re.sub(r"\[\[jumpuri:(.*?) > (.*?)\]\]", repl, text, flags=re.DOTALL)

def format_ruby(text: str) -> str:
    """[[rb:...]]タグを独自形式に変換"""
    return re.sub(r"\[\[rb:(.*?)\s*>\s*(.*?)\]\]", r'[ruby:<\1>(\2)]', text)

def format_survey(survey: Dict) -> str:
    """アンケートデータを整形"""
    if not survey: return ''
    q = survey.get('question', '')
    total = survey.get('total', 0)
    res = f'アンケート　"{q}"　　票数{total}票\n\n'
    for c in survey.get('choices', []):
        res += f'　　　{c.get("text", "")}　　{c.get("count", 0)}票\n'
    return res

def _hash_ids(ids):
    joined = ",".join(sorted(map(str, ids or [])))
    return hashlib.sha256(joined.encode("utf-8")).hexdigest()

# --- PixivCrawler クラス ---

class PixivCrawler:
    def __init__(self, cookie_path: str, data_path: str, interval: int = 2):
        self.cookie_path = os.path.join(cookie_path, 'login.json')
        self.data_path = data_path
        self.img_path = os.path.join(data_path, 'images')
        self.interval = int(interval)
        self.request_count = 1
        
        self.cookies: Dict[str, str] = {}
        self.headers: Dict[str, str] = {}
        self.ua: str = DEFAULT_UA
        
        # 初期化時に画像フォルダを作成
        logging.debug(f'Image Path: {self.img_path}')
        cm.make_dir('images', data_path)

    # -------------------------------------------------------------------------
    # ネットワーク / 認証 / スリープ
    # -------------------------------------------------------------------------

    def _sleep(self):
        """BAN対策スリープ"""
        if self.request_count >= 10:
            wait = random.uniform(self.interval * 5, self.interval * 10)
            self.request_count = 1
        else:
            wait = self.interval
            self.request_count += 1
        time.sleep(wait)

    def login(self, force_login: bool = False):
        """ログイン処理 (Playwright + requests)"""
        if os.path.isfile(self.cookie_path):
            self.cookies, self.ua = cm.load_cookies_and_ua(self.cookie_path)
        
        need_login = force_login or not self.cookies

        # 既存クッキーでのセッションチェック
        if not need_login:
            try:
                resp = requests.get(
                    "https://www.pixiv.net/dashboard",
                    cookies=self.cookies,
                    headers={"User-Agent": self.ua},
                    timeout=10,
                    allow_redirects=False
                )
                if resp.status_code == 200:
                    need_login = False
                elif 300 <= resp.status_code < 400:
                    need_login = True
                else:
                    need_login = True
            except Exception as e:
                logging.warning(f"Login check failed: {e}")
                need_login = True

        if need_login:
            with sync_playwright() as playwright:
                self._perform_playwright_login(playwright)
            self.cookies, self.ua = cm.load_cookies_and_ua(self.cookie_path)
        
        self.headers = {
            'User-Agent': self.ua,
            'Accept-Language': 'ja,en-US;q=0.9,en;q=0.8',
            'Accept-Encoding': 'gzip, deflate, br',
            'Connection': 'keep-alive',
            'Referer': 'https://www.pixiv.net/',
        }
        logging.info(f"Login initialized.")

    def _perform_playwright_login(self, playwright: Playwright):
        """Playwrightを使用したログイン・2FA・ReCAPTCHA対応"""
        browser = playwright.firefox.launch(headless=True)
        context = browser.new_context(
            locale="en-US",
            viewport={"width": 1920, "height": 1080},
            user_agent=self.ua
        )
        page = context.new_page()
        
        # ステルス
        page.add_init_script("""
            Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
        """)

        try:
            logging.info("Navigating to Pixiv Login...")
            page.goto("https://accounts.pixiv.net/login", timeout=0)
            
            # 入力
            mail = input("メールアドレスを入力してください: ")
            pswd = input("パスワードを入力してください: ")
            page.get_by_placeholder("E-mail address or pixiv ID").fill(mail)
            page.get_by_placeholder("Password").fill(pswd)
            page.get_by_role("button", name="Log In", exact=True).click()

            # リダイレクト待機
            def wait_for_redirects(p, ignore_2fa=False, timeout=60):
                deadline = time.time() + timeout
                while time.time() < deadline:
                    try:
                        p.wait_for_load_state("load", timeout=1000)
                    except TimeoutError:
                        pass
                    
                    url = p.url
                    parsed = urlparse(url)
                    host, path = parsed.netloc, parsed.path

                    # 2FA
                    if host == "accounts.pixiv.net" and "two-factor-authentication" in path:
                        if ignore_2fa:
                            time.sleep(0.5); continue
                        return "2fa"
                    
                    # 成功
                    if host in ("www.pixiv.net", "m.pixiv.net"):
                        return "success"
                    
                    time.sleep(0.5)
                return "timeout"

            status = wait_for_redirects(page)

            # ReCAPTCHA
            if status == "timeout" and page.url.startswith("https://accounts.pixiv.net/login"):
                logging.info("Checking for reCAPTCHA")
                try:
                    with recaptchav2.SyncSolver(page) as solver:
                        token = solver.solve_recaptcha(wait=True)
                    if token:
                        page.get_by_role("button", name="Log In", exact=True).click()
                        status = wait_for_redirects(page)
                except:
                    pass

            # 2FA
            if status == "2fa":
                logging.info("Two-factor authentication required")
                code = input("２段階認証コード: ")
                page.get_by_placeholder("Verification code").fill(code)
                page.get_by_role("button", name="Log In", exact=True).click()
                status = wait_for_redirects(page, ignore_2fa=True, timeout=30)

            if status == "success":
                logging.info("Login Successful")
                cookies = context.cookies()
                cookies_dict = {c["name"]: c["value"] for c in cookies if c.get("name") and c.get("value")}
                ua = page.evaluate("() => navigator.userAgent")
                cm.save_cookies_and_ua(self.cookie_path, cookies_dict, ua)
            else:
                logging.error(f"Login failed status={status}")

        except Exception as e:
            logging.error(f"Playwright Error: {e}")
        finally:
            context.close()
            browser.close()

    def get_json(self, url: str) -> Optional[Dict]:
        """APIからJSONを取得"""
        res = cm.get_with_cookie(url, self.cookies, self.headers)
        if not res or res.status_code != 200:
            return None
        try:
            return res.json()
        except:
            return json.loads(unescape(res.text))

    # -------------------------------------------------------------------------
    # ヘルパー: 保存、更新チェック、スナップショット
    # -------------------------------------------------------------------------

    def _save_raw_file(self, path: str, data: Dict):
        cm._save_json(path, data)

    def _check_update(self, raw_path: str, new_date: datetime) -> bool:
        if not os.path.isfile(raw_path):
            return True
        
        old = cm._load_json_safe(raw_path)
        if not old:
            return True
        
        # 更新日が None でない場合に比較
        old_date = safe_fromiso(old.get('updateDate'))
        if old_date and new_date:
            return new_date != old_date
        return True

    def _snapshot_path(self, folder_path: str, user_id: str) -> str:
        return os.path.join(folder_path, "snapshots", "illust_ids", f"{user_id}.json")

    def _load_illust_snapshot(self, folder_path: str, user_id: str, fallback: List) -> List:
        path = self._snapshot_path(folder_path, user_id)
        data = cm._load_json_safe(path)
        return data if data else (fallback or [])

    def _save_illust_snapshot(self, folder_path: str, user_id: str, ids_set: Set[int]):
        path = self._snapshot_path(folder_path, user_id)
        cm._save_json(path, list(map(str, sorted(ids_set))))

    # -------------------------------------------------------------------------
    # 画像・コンテンツ処理
    # -------------------------------------------------------------------------

    @suppress_errors()
    def get_cover(self, url: str, folder_path: str):
        """表紙画像のダウンロード（リトライ機能付き）"""
        if not url: return
        
        ncode = os.path.basename(folder_path)
        ext = os.path.splitext(url)[1]
        
        # 既に存在するかチェック
        if cm.check_image_file(folder_path, f'cover{ext}'):
            return
        if cm.check_image_file(self.img_path, f'pixiv_{ncode}_cover{ext}'):
            return

        candidates = [
            url.replace('c/600x600/novel-cover-master', 'novel-cover-original').replace('_master1200', '').replace('.jpg', e)
            for e in ['.png', '.jpg', '.jpeg', '.gif']
        ] + [url]

        for cand in candidates:
            res = cm.get_with_cookie(cand, self.cookies, self.headers)
            if res and res.status_code == 200:
                f_ext = os.path.splitext(cand)[1]
                # 共通フォルダへ保存
                hash_name = cm.check_image_hash(self.img_path, res.content, f'pixiv_{ncode}_cover{f_ext}', is_cover=True)
                with open(os.path.join(self.img_path, f'{hash_name}{f_ext}'), 'wb') as f:
                    f.write(res.content)
                # 個別フォルダへ保存
                with open(os.path.join(folder_path, f'cover{f_ext}'), 'wb') as f:
                    f.write(res.content)
                return

    def _format_image_links(self, text: str, content_id: str, json_data: Dict, folder_path: str, is_series: bool = False, episode: int = 0) -> str:
        """本文中の [pixivimage] / [uploadedimage] を処理し、画像をダウンロード"""
        
        # ID修正
        text = re.sub(r'\[pixivimage:(\d+)\]', r'[pixivimage:\1-1]', text)
        
        links = re.findall(r"\[pixivimage:(\d+)-(\d+)\]", text)
        inner_links = re.findall(r"\[uploadedimage:(\d+)\]", text)
        
        # --- pixivimage 処理 ---
        link_map = {}
        for art_id, p_num in links:
            link_map.setdefault(art_id, []).append(p_num)
            
        for art_id, pages_needed in link_map.items():
            self._sleep()
            try:
                # ページ情報取得
                p_data = self.get_json(f"https://www.pixiv.net/ajax/illust/{art_id}/pages")
                if not p_data: continue
                pages = p_data.get('body', [])

                for idx, page in enumerate(pages):
                    p_num_str = str(idx + 1)
                    if p_num_str not in pages_needed: continue
                    
                    url = page['urls']['original']
                    ext = os.path.splitext(url)[1]
                    img_name = f'pixiv_{art_id}_p{idx}{ext}'

                    # 画像ダウンロード
                    saved_file = cm.check_image_file(self.img_path, img_name)
                    if not saved_file:
                        res = cm.get_with_cookie(url, self.cookies, self.headers)
                        if res and res.status_code == 200:
                            img_hash = cm.check_image_hash(self.img_path, res.content, img_name)
                            with open(os.path.join(self.img_path, f'{img_hash}{ext}'), 'wb') as f:
                                f.write(res.content)
                            saved_file = f'{img_hash}{ext}'
                    
                    if saved_file:
                        text = text.replace(f'[pixivimage:{art_id}-{p_num_str}]', f'[image]({saved_file})')

            except Exception as e:
                logging.error(f"Image DL error art={art_id}: {e}")

        # --- uploadedimage 処理 ---
        for inner_link in inner_links:
            try:
                upload_info = cm.find_key_recursively(json_data, inner_link)
                if not upload_info: continue
                
                url = upload_info['urls']['original']
                ext = os.path.splitext(url)[1]
                img_name = f'pixiv_{content_id}_{inner_link}{ext}'

                self._sleep()
                saved_file = cm.check_image_file(self.img_path, img_name)
                if not saved_file:
                    res = cm.get_with_cookie(url, self.cookies, self.headers)
                    if res and res.status_code == 200:
                        img_hash = cm.check_image_hash(self.img_path, res.content, img_name)
                        with open(os.path.join(self.img_path, f'{img_hash}{ext}'), 'wb') as f:
                            f.write(res.content)
                        saved_file = f'{img_hash}{ext}'
                
                if saved_file:
                    text = text.replace(f'[uploadedimage:{inner_link}]', f'[image]({saved_file})')

            except Exception as e:
                logging.error(f"Inner image DL error {inner_link}: {e}")
        
        return text

    def _format_novel_text(self, text: str, novel_id: str, json_data: Dict, folder_path: str, is_series: bool = False, episode_num: int = 0) -> str:
        text = text.replace('\r\n', '\n')
        text = self._format_image_links(text, novel_id, json_data, folder_path, is_series, episode_num)
        text = format_ruby(text)
        text = remove_chapter_tag(text)
        text = format_for_url(text)
        return text

    # -------------------------------------------------------------------------
    # ダウンロードロジック
    # -------------------------------------------------------------------------

    @suppress_errors()
    def download_novel(self, novel_id: str, folder_path: str, key_data: str, update: bool = False):
        """短編小説ダウンロード"""
        logging.info(f"Novel ID: {novel_id}")
        json_data = self.get_json(f"https://www.pixiv.net/ajax/novel/{novel_id}")
        if not json_data: return

        body = json_data.get('body', {})
        upload_date = safe_fromiso(body.get('uploadDate'))
        
        novel_path = os.path.join(folder_path, f'n{novel_id}')
        raw_path = os.path.join(novel_path, 'raw', 'raw.json')

        # 更新チェック
        if update and not self._check_update(raw_path, upload_date):
            logging.info(f"{body.get('title')} に更新はありません。")
            return

        cm.make_dir(f'n{novel_id}', folder_path)
        
        # 本文・表紙
        text = self._format_novel_text(body.get('content', ''), novel_id, json_data, folder_path)
        self.get_cover(body.get('coverUrl'), novel_path)

        tags = format_tags([t.get('tag', '') for t in body.get('tags', {}).get('tags', [])])
        poll = cm.find_key_recursively(body, 'pollData')
        
        novel_data = {
            'version': VERSION,
            'get_date': str(datetime.now(JST)),
            'title': body.get('title'),
            'id': novel_id,
            'nid': f'n{novel_id}',
            'url': f"https://www.pixiv.net/novel/show.php?id={novel_id}",
            'author': body.get('userName'),
            'author_id': body.get('userId'),
            'author_url': f"https://www.pixiv.net/users/{body.get('userId')}",
            'caption': body.get('description', '').replace('<br />', '\n'),
            'total_episodes': 1,
            'all_episodes': 1,
            'total_characters': body.get('characterCount'),
            'all_characters': body.get('characterCount'),
            'type': 'novel',
            'serialization': '短編',
            'tags': tags,
            'all_tags': tags,
            'createDate': str(safe_fromiso(body.get('createDate')).astimezone(JST)),
            'updateDate': str(safe_fromiso(body.get('uploadDate')).astimezone(JST)),
            'episodes': {
                1: {
                    'id': novel_id,
                    'chapter': None,
                    'title': body.get('title'),
                    'textCount': body.get('characterCount'),
                    'tags': tags,
                    'introduction': unquote(body.get('description', '').replace('<br />', '\n')),
                    'text': text,
                    'postscript': format_survey(poll) if poll else '',
                    'createDate': str(safe_fromiso(body.get('createDate')).astimezone(JST)),
                    'updateDate': str(safe_fromiso(body.get('uploadDate')).astimezone(JST))
                }
            }
        }

        cm.save_raw_diff(raw_path, novel_path, novel_data) if update else None
        self._save_raw_file(raw_path, novel_data)
        cn.narou_gen(novel_data, novel_path, key_data, self.data_path, "")
        cm.gen_site_index(folder_path, key_data, 'Pixiv')

    @suppress_errors()
    def download_series(self, series_id: str, folder_path: str, key_data: str, update: bool = False):
        """シリーズ小説ダウンロード"""
        logging.info(f"Series ID: {series_id}")
        
        s_detail = self.get_json(f"https://www.pixiv.net/ajax/novel/series/{series_id}")
        if not s_detail: return
        body = s_detail['body']
        
        s_toc = self.get_json(f"https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles")
        if not s_toc: return
        toc = s_toc['body']

        series_path = os.path.join(folder_path, f's{series_id}')
        raw_path = os.path.join(series_path, 'raw', 'raw.json')
        
        # シリーズ更新チェック
        series_update_date = safe_fromiso(body.get('updateDate'))
        if update and not self._check_update(raw_path, series_update_date):
            logging.info(f"{body.get('title')} に更新はありません。")
            return

        cm.make_dir(f's{series_id}', folder_path)
        self.get_cover(body.get('cover', {}).get('urls', {}).get('original'), series_path)

        all_tags = format_tags(list(body.get('tags', [])))
        episodes_data = {}
        total_text = 0

        # 既存エピソード読み込み
        old_eps = {}
        if update and os.path.isfile(raw_path):
            raw_data = cm._load_json_safe(raw_path)
            old_eps = raw_data.get('episodes', {}) if raw_data else {}

        for idx, entry in tqdm(enumerate(toc, 1), total=len(toc), desc=f"Downloading series {series_id}", leave=False):
            if not entry.get('available'): continue
            ep_id = entry['id']
            ep_folder = os.path.join(series_path, str(ep_id))
            os.makedirs(ep_folder, exist_ok=True)

            # エピソード更新チェック
            ep_update = False
            old_ep_data = old_eps.get(str(idx)) # idxに対応するか確認が必要だが、元コードロジックを踏襲
            if update and old_ep_data:
                old_ud = safe_fromiso(old_ep_data.get('updateDate'))
                new_ud = safe_fromiso(entry.get('updateDate'))
                if old_ud and new_ud and old_ud == new_ud:
                    ep_update = True
            
            if ep_update:
                # 古いデータをそのまま使う
                episodes_data[idx] = old_ep_data
                total_text += old_ep_data.get('textCount', 0)
                # タグ統合
                all_tags.extend(old_ep_data.get('tags', []))
            else:
                self._sleep()
                ep_json = self.get_json(f"https://www.pixiv.net/ajax/novel/{ep_id}")
                if not ep_json: continue
                ep_body = ep_json['body']

                self.get_cover(ep_body.get('coverUrl'), ep_folder)
                
                text = self._format_novel_text(ep_body.get('content', ''), ep_id, ep_json, folder_path, True, ep_id)
                tags = format_tags([t.get('tag', '') for t in ep_body.get('tags', {}).get('tags', [])])
                all_tags.extend(tags)
                poll = cm.find_key_recursively(ep_body, 'pollData')

                t_count = int(ep_body.get('characterCount', 0))
                total_text += t_count

                episodes_data[idx] = {
                    'id': ep_id,
                    'chapter': None,
                    'title': entry.get('title'),
                    'textCount': t_count,
                    'tags': tags,
                    'introduction': unquote(ep_body.get('description', '').replace('<br />', '\n')),
                    'text': text,
                    'postscript': format_survey(poll) if poll else '',
                    'createDate': str(safe_fromiso(ep_body.get('createDate')).astimezone(JST)),
                    'updateDate': str(safe_fromiso(ep_body.get('uploadDate')).astimezone(JST))
                }
                
                # 重複フォルダ削除（元コードロジック）
                dup = os.path.join(folder_path, f'n{ep_id}')
                if os.path.exists(dup): shutil.rmtree(dup)

        novel_data = {
            'version': VERSION,
            'get_date': str(datetime.now(JST)),
            'title': body.get('title'),
            'id': series_id,
            'nid': f's{series_id}',
            'url': f"https://www.pixiv.net/novel/series/{series_id}",
            'author': body.get('userName'),
            'author_id': body.get('userId'),
            'author_url': f"https://www.pixiv.net/users/{body.get('userId')}",
            'caption': body.get('caption', '').replace('<br />', '\n'),
            'total_episodes': len(episodes_data),
            'all_episodes': body.get('total'),
            'total_characters': total_text,
            'all_characters': body.get('publishedTotalCharacterCount'),
            'type': 'novel',
            'serialization': '連載中',
            'tags': format_tags(list(body.get('tags', []))),
            'all_tags': format_tags(list(set(all_tags))),
            'createDate': str(safe_fromiso(body.get('createDate')).astimezone(JST)),
            'updateDate': str(safe_fromiso(body.get('updateDate')).astimezone(JST)),
            'episodes': episodes_data
        }

        cm.save_raw_diff(raw_path, series_path, novel_data) if update else None
        self._save_raw_file(raw_path, novel_data)
        cn.narou_gen(novel_data, series_path, key_data, self.data_path, "")
        cm.gen_site_index(folder_path, key_data, 'Pixiv')

    @suppress_errors()
    def download_art(self, art_id: str, folder_path: str, key_data: str):
        """短編漫画/イラストダウンロード (APNG/Ugoira対応)"""
        logging.info(f"Art ID: {art_id}")
        
        # 消えてる可能性チェック
        a_data = self.get_json(f"https://www.pixiv.net/ajax/illust/{art_id}")
        if not a_data: return
        body = a_data['body']

        a_pages = self.get_json(f"https://www.pixiv.net/ajax/illust/{art_id}/pages")
        pages = a_pages.get('body', []) if a_pages else []

        cm.make_dir(f'a{art_id}', folder_path)
        art_path = os.path.join(folder_path, f'a{art_id}')
        raw_path = os.path.join(art_path, 'raw', 'raw.json')

        # 本文構築
        art_text = ""
        for page in pages:
            url = page['urls']['original']
            if '_ugoira' in url:
                # うごイラ処理
                anim_name = f'pixiv_{art_id}_ugoira.apng'
                if cm.check_image_file(self.img_path, anim_name):
                    art_text += f'[image]({cm.check_image_file(self.img_path, anim_name)})\n'
                    continue
                
                self._sleep()
                meta = self.get_json(f"https://www.pixiv.net/ajax/illust/{art_id}/ugoira_meta?lang=ja")
                if meta:
                    m_body = meta['body']
                    src_url = m_body['originalSrc']
                    
                    zip_res = cm.get_with_cookie(src_url, self.cookies, self.headers)
                    zip_path = os.path.join(art_path, f'{art_id}.zip')
                    with open(zip_path, 'wb') as f:
                        f.write(zip_res.content)
                    
                    # 解凍
                    temp_dir = os.path.join(art_path, 'temp')
                    os.makedirs(temp_dir, exist_ok=True)
                    with zipfile.ZipFile(zip_path) as zf:
                        zf.extractall(temp_dir)
                    os.remove(zip_path)

                    # APNG作成
                    frames = []
                    delays = []
                    for f_info in m_body['frames']:
                        frames.append(Image.open(os.path.join(temp_dir, f_info['file'])))
                        delays.append(f_info['delay'])
                    
                    # apng保存 (apngライブラリ使用)
                    temp_apng = os.path.join(temp_dir, "temp.apng")
                    if frames:
                        apng_obj = apng.APNG()
                        for i, frame_file in enumerate(m_body['frames']):
                            apng_obj.append_file(os.path.join(temp_dir, frame_file['file']), delay=frame_file['delay'])
                        apng_obj.save(temp_apng)
                    
                    with open(temp_apng, 'rb') as f:
                        img_data = f.read()
                    
                    final_name = cm.check_image_hash(self.img_path, img_data, anim_name)
                    with open(os.path.join(self.img_path, f'{final_name}.apng'), 'wb') as f:
                        f.write(img_data)
                    
                    art_text += f'[image]({final_name}.apng)\n'
                    shutil.rmtree(temp_dir)
            else:
                # 通常画像
                match = re.search(r'_p(\d+)\.', url)
                if match:
                    num = int(match.group(1)) + 1
                    art_text += f'[pixivimage:{art_id}-{num}]\n'

        # 画像DL処理
        art_text = self._format_image_links(art_text, art_id, a_data, folder_path)
        self.get_cover(body.get('urls', {}).get('original'), art_path)

        tags = format_tags([t.get('tag', '') for t in body.get('tags', {}).get('tags', [])])
        poll = cm.find_key_recursively(body, 'pollData')

        novel_data = {
            'version': VERSION,
            'get_date': str(datetime.now(JST)),
            'title': body.get('title'),
            'id': art_id,
            'nid': f'a{art_id}',
            'url': f"https://www.pixiv.net/artworks/{art_id}",
            'author': body.get('userName'),
            'author_id': body.get('userId'),
            'author_url': f"https://www.pixiv.net/users/{body.get('userId')}",
            'caption': body.get('description', '').replace('<br />', '\n'),
            'total_episodes': 1,
            'all_episodes': 1,
            'total_characters': 0,
            'all_characters': 0,
            'type': 'comic',
            'serialization': '短編',
            'tags': tags,
            'all_tags': tags,
            'createDate': str(safe_fromiso(body.get('createDate')).astimezone(JST)),
            'updateDate': str(safe_fromiso(body.get('uploadDate')).astimezone(JST)),
            'episodes': {
                1: {
                    'id': art_id,
                    'chapter': None,
                    'title': body.get('title'),
                    'textCount': 0,
                    'tags': tags,
                    'introduction': unquote(body.get('description', '').replace('<br />', '\n')),
                    'text': art_text,
                    'postscript': format_survey(poll) if poll else '',
                    'createDate': str(safe_fromiso(body.get('createDate')).astimezone(JST)),
                    'updateDate': str(safe_fromiso(body.get('uploadDate')).astimezone(JST))
                }
            }
        }
        
        # イラストは常に全データ取得なので差分保存不要
        # cm.save_raw_diff(raw_path, art_path, novel_data)
        self._save_raw_file(raw_path, novel_data)
        cn.narou_gen(novel_data, art_path, key_data, self.data_path, "")
        cm.gen_site_index(folder_path, key_data, 'Pixiv')

    @suppress_errors()
    def download_comic(self, comic_id: str, folder_path: str, key_data: str, update: bool = False):
        """漫画シリーズダウンロード"""
        logging.info(f"Comic ID: {comic_id}")

        # シリーズ情報取得 (page 1)
        resp = self.get_json(f"https://www.pixiv.net/ajax/series/{comic_id}?p=1&lang=ja")
        if not resp: return
        c_detail = cm.find_key_recursively(resp, "body")

        # リンク取得（ページング対応）
        arts = {}
        page = 1
        while True:
            self._sleep()
            p_json = self.get_json(f"https://www.pixiv.net/ajax/series/{comic_id}?p={page}&lang=ja")
            if not p_json: break
            series_data = cm.find_key_recursively(p_json, "body").get("series", [])
            if not series_data: break
            
            for item in series_data:
                arts[item["order"]] = item["workId"]
            
            # 簡易ループ継続判定 (通常はもっと多く取れるが、空でなければ次へ)
            if len(series_data) == 0:
                 break
            page += 1
        
        if not arts: return

        # メタデータ
        meta = c_detail['extraData']['meta']
        title = meta['twitter']['title']
        try:
            author = re.search(r'「[^」]*」/「(.*?)」のシリーズ', meta['title']).group(1)
        except:
            author = "Unknown"
        try:
            author_id = re.search(r'user/(\d+)/series', meta['canonical']).group(1)
        except:
            author_id = "0"

        comic_dir = os.path.join(folder_path, f'c{comic_id}')
        raw_path = os.path.join(comic_dir, 'raw', 'raw.json')
        cm.make_dir(f'c{comic_id}', folder_path)

        # シリーズ更新チェック
        series_update_date = None
        for j in c_detail.get('illustSeries', []):
            if str(j['id']) == str(comic_id):
                series_update_date = safe_fromiso(j['updateDate'])
                break
        
        if update and not self._check_update(raw_path, series_update_date):
             logging.info(f"{title} に更新はありません。")
             return

        # エピソードDL
        episodes_data = {}
        all_tags = format_tags(list(c_detail.get('tagTranslation', {}).keys()))

        old_eps = {}
        if update and os.path.isfile(raw_path):
            raw_data = cm._load_json_safe(raw_path)
            old_eps = raw_data.get('episodes', {}) if raw_data else {}

        arts = dict(sorted(arts.items()))
        
        for idx, work_id in tqdm(arts.items(), desc=f"Downloading comic {comic_id}", leave=False):
            ep_folder = os.path.join(comic_dir, str(work_id))
            os.makedirs(ep_folder, exist_ok=True)
            
            # エピソード更新チェック (Illust API)
            ep_update = False
            old_ep = old_eps.get(str(idx))
            if update and old_ep:
                self._sleep()
                det = self.get_json(f"https://www.pixiv.net/ajax/illust/{work_id}")
                if det:
                    new_ud = safe_fromiso(det['body']['uploadDate'])
                    old_ud = safe_fromiso(old_ep['updateDate'])
                    if new_ud == old_ud:
                        ep_update = True

            if ep_update:
                episodes_data[idx] = old_ep
                all_tags.extend(old_ep.get('tags', []))
            else:
                self._sleep()
                ep_data = self.get_json(f"https://www.pixiv.net/ajax/illust/{work_id}")
                if not ep_data: continue
                body = ep_data['body']
                
                # 画像リンク構築
                pages_res = self.get_json(f"https://www.pixiv.net/ajax/illust/{work_id}/pages")
                text = ""
                if pages_res:
                    for p in pages_res.get('body', []):
                        u = p['urls']['original']
                        num = int(re.search(r'_p(\d+)\.', u).group(1)) + 1
                        text += f'[pixivimage:{work_id}-{num}]\n'
                
                text = self._format_image_links(text, work_id, ep_data, folder_path, True, work_id)
                self.get_cover(body.get('urls', {}).get('original'), ep_folder)
                
                tags = format_tags([t.get('tag', '') for t in body.get('tags', {}).get('tags', [])])
                all_tags.extend(tags)
                poll = cm.find_key_recursively(body, 'pollData')

                episodes_data[idx] = {
                    'id': work_id,
                    'chapter': None,
                    'title': body.get('title'),
                    'textCount': 0,
                    'tags': tags,
                    'introduction': unquote(body.get('description', '').replace('<br />', '\n')),
                    'text': text,
                    'postscript': format_survey(poll) if poll else '',
                    'createDate': str(safe_fromiso(body.get('createDate')).astimezone(JST)),
                    'updateDate': str(safe_fromiso(body.get('uploadDate')).astimezone(JST))
                }
                
                # 重複フォルダ削除
                dup = os.path.join(folder_path, f'a{work_id}')
                if os.path.exists(dup): shutil.rmtree(dup)

        novel_data = {
            'version': VERSION,
            'get_date': str(datetime.now(JST)),
            'title': title,
            'id': comic_id,
            'nid': f'c{comic_id}',
            'url': f"https://www.pixiv.net/user/{author_id}/series/{comic_id}",
            'author': author,
            'author_id': author_id,
            'caption': meta.get('description', '').replace('<br />', '\n'),
            'total_episodes': len(episodes_data),
            'all_episodes': len(episodes_data),
            'total_characters': 0,
            'all_characters': 0,
            'type': 'comic',
            'serialization': '連載中',
            'tags': format_tags(list(c_detail.get('tagTranslation', {}).keys())),
            'all_tags': format_tags(list(set(all_tags))),
            'createDate': str(safe_fromiso(c_detail.get('illustSeries', [{}])[0].get('createDate')).astimezone(JST)),
            'updateDate': str(safe_fromiso(c_detail.get('illustSeries', [{}])[0].get('updateDate')).astimezone(JST)),
            'episodes': episodes_data
        }

        cm.save_raw_diff(raw_path, comic_dir, novel_data) if update else None
        self._save_raw_file(raw_path, novel_data)
        cn.narou_gen(novel_data, comic_dir, key_data, self.data_path, "")
        cm.gen_site_index(folder_path, key_data, 'Pixiv')

    @suppress_errors()
    def download_user(self, user_id: str, folder_path: str, key_data: str, update: bool = False):
        """ユーザー一括ダウンロード (スナップショット機能付き)"""
        logging.info(f"User ID: {user_id}")
        
        # ユーザー設定読み込み
        user_json_path = os.path.join(folder_path, "user.json")
        
        full_conf = cm._load_json_safe(user_json_path)
        user_conf = full_conf.get(user_id, {})
        
        # 初期化
        if not user_conf:
            user_conf = {"novel": "enable", "comic": "enable", "illust_ids_snapshot": []}
            # 再読み込み (atomic writeにより競合は減るが、最新状態を取得)
            full_conf = cm._load_json_safe(user_json_path)
            if not full_conf:
                 full_conf = {"version": 3}
            
            full_conf[user_id] = user_conf
            cm._save_json(user_json_path, full_conf)

        # プロフィール全取得
        all_data = self.get_json(f"https://www.pixiv.net/ajax/user/{user_id}/profile/all")
        if not all_data: return
        body = all_data['body']
        
        # エラー修正: 辞書型・リスト型両対応でIDリスト抽出
        # novelSeries
        ns_data = body.get("novelSeries")
        n_series = []
        if isinstance(ns_data, list):
            n_series = [str(x['id']) for x in ns_data]
        elif isinstance(ns_data, dict):
            n_series = list(ns_data.keys())

        # mangaSeries
        ms_data = body.get("mangaSeries")
        m_series = []
        if isinstance(ms_data, list):
            m_series = [str(x['id']) for x in ms_data]
        elif isinstance(ms_data, dict):
            m_series = list(ms_data.keys())
            
        # novels, illusts, manga (これらは通常Dictだが念の為チェック)
        novels_data = body.get("novels", {})
        novels = list(novels_data.keys()) if isinstance(novels_data, dict) else []

        illusts_data = body.get("illusts", {})
        illusts = list(illusts_data.keys()) if isinstance(illusts_data, dict) else []

        mangas_data = body.get("manga", {})
        mangas = list(mangas_data.keys()) if isinstance(mangas_data, dict) else []

        user_name = self.get_json(f"https://www.pixiv.net/ajax/user/{user_id}")['body']['name']
        logging.info(f"User Name: {user_name}")

        # シリーズ重複除外（事前チェック）
        in_n_series = []
        for sid in n_series:
            self._sleep()
            toc = self.get_json(f"https://www.pixiv.net/ajax/novel/series/{sid}/content_titles")
            if toc:
                in_n_series.extend([str(t['id']) for t in toc['body']])
        novels = [n for n in novels if n not in in_n_series]

        in_m_series = []
        for sid in m_series:
            self._sleep()
            s_p1 = self.get_json(f"https://www.pixiv.net/ajax/series/{sid}?p=1&lang=ja")
            if s_p1:
                det = cm.find_key_recursively(s_p1, "body")
                for item in det.get("series", []):
                     in_m_series.append(str(item["workId"]))
        mangas = [m for m in mangas if m not in in_m_series]

        # --- 小説処理 ---
        if user_conf.get("novel") == "enable":
            for sid in n_series:
                self._sleep()
                self.download_series(str(sid), folder_path, key_data, update)
            for nid in novels:
                self._sleep()
                self.download_novel(str(nid), folder_path, key_data, update)

        # --- 漫画/イラスト処理 (スナップショット) ---
        if user_conf.get("comic") == "enable":
            # 保留中の破損漫画の修復チェック
            if hasattr(_crawler, '_corrupt_comics_pending') and _crawler._corrupt_comics_pending:
                logging.info(f"Checking pending corrupted comics for user {user_id}...")
                for pending_item in list(_crawler._corrupt_comics_pending):
                    folder_name, corrupt_path, cid = pending_item
                    # このユーザーの漫画シリーズに該当するか確認
                    if cid in m_series:
                        logging.info(f"Found pending comic {cid} in user {user_id} series, re-downloading...")
                        try:
                            self.download_comic(cid, folder_path, key_data, update=False)
                            # 成功したら .corrupt を削除
                            if os.path.exists(corrupt_path):
                                os.remove(corrupt_path)
                                logging.info(f"Removed corrupt file: {corrupt_path}")
                            # 保留リストから削除
                            _crawler._corrupt_comics_pending.remove(pending_item)
                        except Exception as e:
                            logging.error(f"Failed to repair pending comic {cid}: {e}")
            
            for sid in m_series:
                self._sleep()
                self.download_comic(str(sid), folder_path, key_data, update)
            
            # イラスト・単発漫画
            target_ids = set(map(int, illusts + mangas))
            prev_snapshot = set(map(int, self._load_illust_snapshot(folder_path, user_id, user_conf.get("illust_ids_snapshot", []))))
            
            if update:
                new_ids = sorted(list(target_ids - prev_snapshot))
            else:
                new_ids = sorted(list(target_ids))
            
            logging.info(f"Downloading {len(new_ids)} new images/artworks...")
            
            for aid in new_ids:
                self._sleep()
                self.download_art(str(aid), folder_path, key_data)
            
            # スナップショット保存
            self._save_illust_snapshot(folder_path, user_id, target_ids)
            
            # user.json 更新 (ハッシュのみ)
            full_conf = cm._load_json_safe(user_json_path)
            if user_id in full_conf:
                full_conf[user_id]["illust_ids_snapshot_hash"] = _hash_ids(target_ids)
                cm._save_json(user_json_path, full_conf)


# --- エントリーポイント (互換性維持) ---

_crawler: Optional[PixivCrawler] = None

def init(cookie_path, data_path, is_login, interval):
    global _crawler
    _crawler = PixivCrawler(cookie_path, data_path, interval)
    if is_login:
        _crawler.login()

def download(url, folder_path, key_data, data_path, host_name):
    if not _crawler:
        logging.error("Crawler not initialized")
        return

    try:
        if "novel/show" in url:
            nid = re.search(r"id=(\d+)", url).group(1)
            _crawler.download_novel(nid, folder_path, key_data)
        elif "novel/series" in url:
            sid = re.search(r"series/(\d+)", url).group(1)
            _crawler.download_series(sid, folder_path, key_data)
        elif "users/" in url:
            uid = re.search(r"users/(\d+)", url).group(1)
            _crawler.download_user(uid, folder_path, key_data)
        elif "artworks/" in url:
            aid = re.search(r"artworks/(\d+)", url).group(1)
            _crawler.download_art(aid, folder_path, key_data)
        elif "series/" in url:
            match = re.search(r"series/(\d+)", url)
            if match:
                _crawler.download_comic(match.group(1), folder_path, key_data)
        else:
            logging.error(f"Unknown URL format: {url}")
    except Exception as e:
        logging.error(f"Download failed: {e}", exc_info=True)

def recover_global_image_db(folder_path: str, corrupt_db_path: str):
    """
    グローバル画像DBの復元
    全作品フォルダから database.json と cover.json を収集してマージ
    """
    logging.info("Recovering global image database from all work folders...")
    
    merged_db = {}
    recovered_count = 0
    
    # 全作品フォルダを探索
    for item in os.listdir(folder_path):
        item_path = os.path.join(folder_path, item)
        if not os.path.isdir(item_path):
            continue
        
        # images フォルダ自体はスキップ
        if item == 'images':
            continue
        
        # 各作品フォルダ内の database.json と cover.json を収集
        for json_file in ['database.json', 'cover.json']:
            json_path = os.path.join(item_path, json_file)
            if os.path.exists(json_path):
                try:
                    data = cm._load_json_safe(json_path)
                    if data:
                        merged_db.update(data)
                        recovered_count += len(data)
                except Exception as e:
                    logging.warning(f"Failed to load {json_path}: {e}")
    
    if merged_db:
        # 復元したデータを保存
        db_dir = os.path.dirname(corrupt_db_path)
        db_filename = os.path.basename(corrupt_db_path).replace('.corrupt', '')
        recovered_path = os.path.join(db_dir, db_filename)
        
        cm._save_json(recovered_path, merged_db)
        logging.info(f"Recovered {len(merged_db)} unique entries ({recovered_count} total) to {recovered_path}")
        
        # .corrupt ファイルを削除
        if os.path.exists(corrupt_db_path):
            os.remove(corrupt_db_path)
            logging.info(f"Removed {corrupt_db_path} after recovery")
    else:
        logging.warning("No data found to recover global image database")

def recover_from_corrupt_json(corrupt_path: str) -> dict:
    """
    破損したJSONファイルから可能な限りデータを抽出
    
    戦略:
    1. 部分的に読めるJSONを抽出（複数のJSON objectが含まれる場合）
    2. 正規表現で key-value ペアを抽出
    3. バックアップファイルから抽出
    """
    recovered_data = {}
    
    try:
        with open(corrupt_path, 'r', encoding='utf-8', errors='ignore') as f:
            content = f.read()
        
        logging.info(f"Attempting aggressive recovery from: {corrupt_path}")
        
        # 戦略1: 部分的なJSON objectを探す
        import re
        # { ... } の塊を探す
        json_objects = re.findall(r'\{[^{}]*\}', content)
        for obj_str in json_objects:
            try:
                obj = json.loads(obj_str)
                if isinstance(obj, dict):
                    recovered_data.update(obj)
                    logging.debug(f"Recovered object: {len(obj)} entries")
            except:
                continue
        
        # 戦略2: key-value ペアを正規表現で抽出
        # "key": "value" 形式
        string_pairs = re.findall(r'"([^"]+)":\s*"([^"]*)"', content)
        for key, value in string_pairs:
            if key not in recovered_data and value:
                recovered_data[key] = value
        
        # "key": number 形式
        number_pairs = re.findall(r'"([^"]+)":\s*(\d+(?:\.\d+)?)', content)
        for key, value in number_pairs:
            if key not in recovered_data:
                try:
                    recovered_data[key] = int(value) if '.' not in value else float(value)
                except:
                    recovered_data[key] = value
        
        if recovered_data:
            logging.info(f"Recovered {len(recovered_data)} entries from corrupt file")
        else:
            logging.warning(f"No recoverable data found in {corrupt_path}")
        
    except Exception as e:
        logging.error(f"Failed to recover from {corrupt_path}: {e}")
    
    # 戦略3: バックアップファイルから抽出
    backup_base = corrupt_path.replace('.corrupt', '')
    for i in range(1, 4):
        backup_path = f"{backup_base}.backup.{i}"
        if os.path.exists(backup_path):
            try:
                backup_data = cm._load_json_safe(backup_path)
                if backup_data:
                    # バックアップデータを優先的にマージ
                    for key, value in backup_data.items():
                        if key not in recovered_data:
                            recovered_data[key] = value
                    logging.info(f"Merged {len(backup_data)} entries from backup: {backup_path}")
            except:
                continue
    
    return recovered_data

def update(folder_path, key_data, data_path, host_name):
    if not _crawler:
        logging.error("Crawler not initialized")
        return

    # 1. 最優先: .corrupt ファイルの探索と再ダウンロード
    corrupt_targets = []
    corrupt_comics_pending = []  # author_id 不明の漫画シリーズ
    
    # クローラーインスタンスに保留リストを初期化（既存のものがあれば保持）
    if not hasattr(_crawler, '_corrupt_comics_pending'):
        _crawler._corrupt_comics_pending = []
    
    for root, dirs, files in os.walk(folder_path):
        for file in files:
            if file.endswith('.corrupt'):
                # raw.json.corrupt, database.json.corrupt, cover.json.corrupt を検出
                if file == 'raw.json.corrupt':
                    # raw.json.corrupt は raw フォルダ内にあるため、親の親フォルダが作品フォルダ
                    # 例: /pixiv/n18922281/raw/raw.json.corrupt
                    # root = /pixiv/n18922281/raw なので、os.path.dirname(root) で /pixiv/n18922281 を取得
                    work_folder = os.path.dirname(root)
                    folder_name = os.path.basename(work_folder)
                    corrupt_path = os.path.join(root, file)
                    corrupt_targets.append((folder_name, corrupt_path))
                    logging.warning(f"Found corrupted file: {corrupt_path}, work folder: {folder_name}")
                    
                elif file in ('database.json.corrupt', 'cover.json.corrupt'):
                    # database.json.corrupt, cover.json.corrupt は作品フォルダ直下または images フォルダ内
                    # 例: /pixiv/a135990959/database.json.corrupt
                    # または /pixiv/images/database.json.corrupt
                    folder_name = os.path.basename(root)
                    corrupt_path = os.path.join(root, file)
                    
                    # images フォルダ内の場合は特別処理
                    if folder_name == 'images':
                        logging.warning(f"Found corrupted global image DB: {corrupt_path}")
                        # グローバル画像DBの修復は別途処理
                        # 全作品フォルダから画像情報を収集して再構築
                        try:
                            recover_global_image_db(folder_path, corrupt_path)
                        except Exception as e:
                            logging.error(f"Failed to recover global image DB: {e}")
                        continue
                    
                    # 作品フォルダ内の database.json/cover.json の破損からデータを拾い上げ
                    try:
                        recovered_data = recover_from_corrupt_json(corrupt_path)
                        if recovered_data:
                            # 復元したデータを保存
                            recovered_path = os.path.join(root, file.replace('.corrupt', ''))
                            
                            # 既存のデータとマージ
                            existing_data = {}
                            if os.path.exists(recovered_path):
                                existing_data = cm._load_json_safe(recovered_path)
                            
                            # 既存データを優先、破損ファイルから拾ったデータで補完
                            for key, value in recovered_data.items():
                                if key not in existing_data:
                                    existing_data[key] = value
                            
                            cm._save_json(recovered_path, existing_data)
                            logging.info(f"Saved recovered data ({len(recovered_data)} entries) to {recovered_path}")
                            
                            # .corrupt ファイルを削除
                            if os.path.exists(corrupt_path):
                                os.remove(corrupt_path)
                                logging.info(f"Removed {file} after data recovery")
                            continue
                        else:
                            logging.warning(f"No data could be recovered from {corrupt_path}")
                    except Exception as e:
                        logging.error(f"Failed to recover data from {corrupt_path}: {e}")
                    
                    # データ回収が不可能な場合、同じフォルダに複数の .corrupt がある場合は重複登録を避ける
                    if not any(fn == folder_name for fn, _ in corrupt_targets):
                        corrupt_targets.append((folder_name, corrupt_path))
                        logging.warning(f"Found corrupted {file}: {corrupt_path}, work folder: {folder_name}")
                    else:
                        # raw.json.corrupt も見つかっている場合は database/cover.json.corrupt だけ削除
                        # （再ダウンロード時に再生成されるため）
                        if os.path.exists(corrupt_path):
                            os.remove(corrupt_path)
                            logging.info(f"Removed {file} (will be regenerated): {corrupt_path}")
    
    # .corrupt ファイルがある作品を再ダウンロード
    if corrupt_targets:
        logging.info(f"Found {len(corrupt_targets)} corrupted files, re-downloading...")
        for folder_name, corrupt_path in corrupt_targets:
            try:
                logging.info(f"Re-downloading corrupted: {folder_name}")
                
                # フォルダ名から作品タイプと IDを推定
                if folder_name.startswith('n'):
                    nid = folder_name.lstrip('n')
                    _crawler.download_novel(nid, folder_path, key_data, update=False)
                elif folder_name.startswith('s'):
                    sid = folder_name.lstrip('s')
                    _crawler.download_series(sid, folder_path, key_data, update=False)
                elif folder_name.startswith('a'):
                    aid = folder_name.lstrip('a')
                    _crawler.download_art(aid, folder_path, key_data)
                elif folder_name.startswith('c'):
                    cid = folder_name.lstrip('c')
                    
                    # 漫画シリーズの author_id をパターンマッチングで探索
                    author_id = None
                    
                    # 1. index.json から author_id を取得試行
                    index_path = os.path.join(folder_path, 'index.json')
                    if os.path.exists(index_path):
                        index_data = cm._load_json_safe(index_path)
                        if index_data and folder_name in index_data:
                            author_id = index_data[folder_name].get('author_id')
                            logging.info(f"Found author_id from index.json: {author_id}")
                    
                    # 2. 同一フォルダ内の他のファイルから author_id を探索
                    if not author_id:
                        parent_dir = os.path.dirname(corrupt_path)
                        for fname in os.listdir(parent_dir):
                            fpath = os.path.join(parent_dir, fname)
                            if fname.endswith('.json') and os.path.isfile(fpath):
                                try:
                                    data = cm._load_json_safe(fpath)
                                    if data and 'author_id' in data:
                                        author_id = data['author_id']
                                        logging.info(f"Found author_id from {fname}: {author_id}")
                                        break
                                except:
                                    continue
                    
                    # 3. raw フォルダ内のバックアップファイルから探索
                    if not author_id:
                        raw_dir = os.path.join(os.path.dirname(corrupt_path), 'raw')
                        if os.path.exists(raw_dir):
                            for fname in os.listdir(raw_dir):
                                if fname.startswith('raw.json.backup.'):
                                    backup_path = os.path.join(raw_dir, fname)
                                    try:
                                        data = cm._load_json_safe(backup_path)
                                        if data and 'author_id' in data:
                                            author_id = data['author_id']
                                            logging.info(f"Found author_id from backup {fname}: {author_id}")
                                            break
                                    except:
                                        continue
                    
                    if author_id:
                        # author_id が判明した場合は即座に再ダウンロード
                        _crawler.download_comic(cid, folder_path, key_data, update=False)
                    else:
                        # author_id が不明な場合は保留リストに追加
                        logging.warning(f"Comic {cid} author_id unknown, adding to pending list")
                        corrupt_comics_pending.append((folder_name, corrupt_path, cid))
                        continue  # .corrupt 削除をスキップ
                
                # 再ダウンロード成功後、.corrupt ファイルを削除
                if os.path.exists(corrupt_path):
                    os.remove(corrupt_path)
                    logging.info(f"Removed corrupt file: {corrupt_path}")
                    
            except Exception as e:
                logging.error(f"Failed to re-download corrupted {folder_name}: {e}", exc_info=True)
    
    # 保留中の漫画をクローラーインスタンスに追加
    if corrupt_comics_pending:
        _crawler._corrupt_comics_pending.extend(corrupt_comics_pending)
        logging.info(f"Added {len(corrupt_comics_pending)} pending comics to repair queue")
    
    # 2. 通常の更新処理
    # index.json から更新
    index_path = os.path.join(folder_path, 'index.json')
    if not os.path.exists(index_path): return
    
    index_data = cm._load_json_safe(index_path)
    if not index_data: return
    
    # .corrupt が見つかったフォルダ名のセット（高速検索用）
    corrupt_folder_names = {folder_name for folder_name, _ in corrupt_targets}
    
    # user.json からユーザー更新
    user_json_path = os.path.join(folder_path, 'user.json')
    users = cm._load_json_safe(user_json_path)
    if users:
        for uid in users:
            if uid == "version": continue
            _crawler.download_user(uid, folder_path, key_data, update=True)
    
    # 個別作品更新
    for folder, meta in index_data.items():
        # .corrupt で既に処理済みのフォルダはスキップ
        if folder in corrupt_folder_names:
            logging.debug(f"Skipping {folder} (already re-downloaded due to corruption)")
            continue
            
        try:
            if meta['type'] == 'novel':
                if meta['serialization'] == '短編':
                    nid = folder.lstrip('n')
                    _crawler.download_novel(nid, folder_path, key_data, update=True)
                else:
                    sid = folder.lstrip('s')
                    _crawler.download_series(sid, folder_path, key_data, update=True)
            elif meta['type'] == 'comic':
                if meta['serialization'] == '短編':
                    aid = folder.lstrip('a')
                    _crawler.download_art(aid, folder_path, key_data) 
                else:
                    cid = folder.lstrip('c')
                    _crawler.download_comic(cid, folder_path, key_data, update=True)
        except Exception as e:
            logging.error(f"Update failed for {folder}: {e}")
            # エラー発生時は破損の可能性が高いため再ダウンロードをリクエスト
            request_re_download(folder, host_name)

def generate_request_id() -> str:
    """リクエストIDを生成 (server.pyと同等)"""
    template = "xxxx-xxxx-4xxx-yxxx-xxxx"
    def replace_char(c):
        r = random.randint(0, 15)
        if c == 'x': return hex(r)[2:]
        if c == 'y': return hex(r & 0x3 | 0x8)[2:]
        if c == '4': return '4'
        return c
    return ''.join(replace_char(c) for c in template)

def request_re_download(target_id: str, host_name: str):
    """破損データの再ダウンロードをリクエスト（URL形式に変換）"""
    try:
        # フォルダ名からPixiv URLを成形
        pixiv_url = None
        if target_id.startswith('n'):
            # 短編小説: https://www.pixiv.net/novel/show.php?id=12345678
            nid = target_id.lstrip('n')
            pixiv_url = f"https://www.pixiv.net/novel/show.php?id={nid}"
        elif target_id.startswith('s'):
            # 連載小説: https://www.pixiv.net/novel/series/12345678
            sid = target_id.lstrip('s')
            pixiv_url = f"https://www.pixiv.net/novel/series/{sid}"
        elif target_id.startswith('a'):
            # イラスト: https://www.pixiv.net/artworks/12345678
            aid = target_id.lstrip('a')
            pixiv_url = f"https://www.pixiv.net/artworks/{aid}"
        elif target_id.startswith('c'):
            # 漫画シリーズ: https://www.pixiv.net/user/123456/series/789012 (ユーザーIDが必要)
            # ※ comic は author_id が必要なため、ここでは対応不可
            logging.warning(f"Comic series {target_id} cannot be auto-repaired via URL (author_id required)")
            return
        else:
            logging.error(f"Unknown target_id format: {target_id}")
            return
        
        url = f"{host_name}/api/"
        
        payload = {
            "re_download": pixiv_url,  # URL形式で送信
            "request_id": generate_request_id()
        }
        # タイムアウトを短めに設定して、サーバーが詰まらないようにする
        requests.post(url, data=payload, timeout=1)
        logging.info(f"Requested auto-repair (re-download) for corrupted data: {target_id} -> {pixiv_url}")
    except Exception as e:
        logging.error(f"Failed to request auto-repair for {target_id}: {e}")

def convert(folder_path, key_data, data_path, host_name):
    """ローカルデータの再変換"""
    
    # 1. 最優先: .corrupt ファイルの探索
    corrupt_found = []
    for root, dirs, files in os.walk(folder_path):
        for file in files:
            if file.endswith('.corrupt'):
                if file == 'raw.json.corrupt':
                    folder_name = os.path.basename(root)
                    corrupt_path = os.path.join(root, file)
                    corrupt_found.append(folder_name)
                    logging.warning(f"Convert: Found corrupted file in {folder_name}, skipping and requesting re-download")
                    request_re_download(folder_name, host_name)
    
    # 2. 通常の変換処理
    folders = [f for f in os.listdir(folder_path) if os.path.isdir(os.path.join(folder_path, f))]
    for folder in folders:
        # .corrupt が見つかったフォルダはスキップ
        if folder in corrupt_found:
            continue
            
        raw_path = os.path.join(folder_path, folder, 'raw', 'raw.json')
        if os.path.exists(raw_path):
            try:
                data = cm._load_json_safe(raw_path)
                if not data:
                    logging.warning(f"Corrupted raw.json found in {folder}. Requesting re-download.")
                    request_re_download(folder, host_name)
                    continue

                # タグ整形再適用
                if 'tags' in data:
                    data['tags'] = format_tags(data['tags'])
                if 'all_tags' in data:
                    data['all_tags'] = format_tags(data['all_tags'])
                
                cm._save_json(raw_path, data)
                
                cn.narou_gen(data, os.path.join(folder_path, folder), key_data, data_path, host_name)
            except Exception as e:
                logging.error(f"Convert failed for {folder}: {e}")
    
    cm.gen_site_index(folder_path, key_data, 'Pixiv')
