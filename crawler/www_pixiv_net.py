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
from crawler.common import safe_fromiso, safe_date_str
import util

# --- 定数定義 ---
VERSION = 5
JST = timezone(timedelta(hours=9))
DEFAULT_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"

# --- ユーティリティ関数 ---


def add_ai_tag_if_needed(ai_type: Optional[int], tags: List[str]) -> List[str]:
    """aiType が 2 のときだけ 'AI生成' タグを 1 個だけ追加する"""
    if ai_type == 2 and "AI生成" not in tags:
        tags.append("AI生成")
    return tags


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
        if not t:
            continue
        if isinstance(t, dict):
            t = t.get("tag", "")
        for tag in t.split("#"):
            tag = tag.strip()
            if not tag:
                continue
            if tag.startswith("#"):
                tag = tag[1:]
            if tag and tag not in seen:
                result.append(tag)
                seen.add(tag)
    return result


def remove_chapter_tag(text: str) -> str:
    """[chapter:...]タグを整形する"""

    def replacer(match):
        before = match.group(1)
        content = match.group(3).replace("\n", "")
        if before and before[-1] != "\n":
            return f"{before}\n{content}\n\n\n"
        return f"{before}{content}\n\n\n"

    return re.sub(r"(.*?)(\[chapter:(.*?)\])", replacer, text, flags=re.DOTALL)


def format_for_url(text: str) -> str:
    """[[jumpuri:...]]タグをHTMLリンクに変換"""

    def repl(match):
        txt = match.group(1)
        url = match.group(2)
        if url.startswith("/http://") or url.startswith("/https://"):
            url = url[1:]
        return f'<a href="{url}">{txt}</a>'

    return re.sub(r"\[\[jumpuri:(.*?) > (.*?)\]\]", repl, text, flags=re.DOTALL)


def format_ruby(text: str) -> str:
    """[[rb:...]]タグを独自形式に変換"""
    return re.sub(r"\[\[rb:(.*?)\s*>\s*(.*?)\]\]", r"[ruby:<\1>(\2)]", text)


def format_survey(survey: Dict) -> str:
    """アンケートデータを整形"""
    if not survey:
        return ""
    q = survey.get("question", "")
    total = survey.get("total", 0)
    res = f'アンケート　"{q}"　　票数{total}票\n\n'
    for c in survey.get("choices", []):
        res += f"　　　{c.get('text', '')}　　{c.get('count', 0)}票\n"
    return res


def _hash_ids(ids):
    joined = ",".join(sorted(map(str, ids or [])))
    return hashlib.sha256(joined.encode("utf-8")).hexdigest()


# --- PixivCrawler クラス ---


class PixivCrawler:
    def __init__(self, cookie_path: str, data_path: str, interval: int = 2):
        self.cookie_path = os.path.join(cookie_path, "login.json")
        self.data_path = data_path
        self.img_path = os.path.join(data_path, "images")
        self.interval = int(interval)
        self.request_count = 1

        self.cookies: Dict[str, str] = {}
        self.headers: Dict[str, str] = {}
        self.ua: str = DEFAULT_UA

        # 初期化時に画像フォルダを作成
        logging.debug(f"Image Path: {self.img_path}")
        cm.make_dir("images", data_path)

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

        # 既存クッキーでのセッションチェック（より厳密に）
        if not need_login:
            try:
                # 1. ダッシュボードチェック
                resp = requests.get(
                    "https://www.pixiv.net/dashboard",
                    cookies=self.cookies,
                    headers={"User-Agent": self.ua},
                    timeout=10,
                    allow_redirects=False,
                )
                if resp.status_code != 200:
                    need_login = True
                    logging.info("Session check failed: dashboard returned non-200")
                else:
                    # 2. AJAX APIエンドポイントチェック（より厳密）
                    ajax_resp = requests.get(
                        "https://www.pixiv.net/ajax/user/extra",
                        cookies=self.cookies,
                        headers={"User-Agent": self.ua, "Referer": "https://www.pixiv.net/"},
                        timeout=10,
                    )
                    if ajax_resp.status_code != 200:
                        need_login = True
                        logging.info(f"Session check failed: ajax returned {ajax_resp.status_code}")
                    else:
                        try:
                            ajax_data = ajax_resp.json()
                            if ajax_data.get("error"):
                                need_login = True
                                logging.info("Session check failed: ajax returned error")
                        except:
                            need_login = True
                            logging.info("Session check failed: ajax response parse error")
            except Exception as e:
                logging.warning(f"Login check failed: {e}")
                need_login = True

        if need_login:
            with sync_playwright() as playwright:
                self._perform_playwright_login(playwright)
            self.cookies, self.ua = cm.load_cookies_and_ua(self.cookie_path)

        self.headers = {
            "User-Agent": self.ua,
            "Accept-Language": "ja,en-US;q=0.9,en;q=0.8",
            "Accept-Encoding": "gzip, deflate, br",
            "Connection": "keep-alive",
            "Referer": "https://www.pixiv.net/",
        }
        logging.info(f"Login initialized.")

    def login_new_account(self, account_name: str, display_name: str = None):
        """新規アカウントとしてログイン（強制ログイン、指定ファイル名で保存）
        
        Args:
            account_name: アカウント名（{account_name}.json として保存）
            display_name: アカウント表示名（省略可）
        """
        cookie_path = os.path.join(os.path.dirname(self.cookie_path), f"{account_name}.json")
        with sync_playwright() as playwright:
            self._perform_playwright_login(playwright, cookie_path, display_name)
        logging.info(f"New account '{account_name}' login completed.")

    def _perform_playwright_login(self, playwright: Playwright, cookie_path: str = None, display_name: str = None):
        """Playwrightを使用したログイン・2FA・ReCAPTCHA対応
        
        Args:
            playwright: Playwright インスタンス
            cookie_path: クッキー保存先パス（None の場合は self.cookie_path を使用）
            display_name: アカウント表示名（省略可）
        """
        save_path = cookie_path if cookie_path else self.cookie_path
        
        browser = playwright.firefox.launch(headless=False)
        context = browser.new_context(
            locale="en-US", viewport={"width": 1920, "height": 1080}, user_agent=self.ua
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
                    if (
                        host == "accounts.pixiv.net"
                        and "two-factor-authentication" in path
                    ):
                        if ignore_2fa:
                            time.sleep(0.5)
                            continue
                        return "2fa"

                    # 成功
                    if host in ("www.pixiv.net", "m.pixiv.net"):
                        return "success"

                    time.sleep(0.5)
                return "timeout"

            status = wait_for_redirects(page)

            # ReCAPTCHA
            if status == "timeout" and page.url.startswith(
                "https://accounts.pixiv.net/login"
            ):
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
                cookies_dict = {
                    c["name"]: c["value"]
                    for c in cookies
                    if c.get("name") and c.get("value")
                }
                ua = page.evaluate("() => navigator.userAgent")
                cm.save_cookies_and_ua(save_path, cookies_dict, ua, display_name)
            else:
                logging.error(f"Login failed status={status}")

        except Exception as e:
            logging.error(f"Playwright Error: {e}")
        finally:
            context.close()
            browser.close()

    def _request(self, url: str) -> Optional[requests.Response]:
        """GETリクエスト（Cookieなし優先、失敗時はCookieありで再試行）"""
        # Pixiv固有のヘッダー
        pixiv_headers = {
            "Accept": "application/json",
            "Accept-Language": "ja,en-US;q=0.9,en;q=0.8",
            "Accept-Encoding": "gzip, deflate, br",
            "Referer": "https://www.pixiv.net/",
            "Connection": "keep-alive",
            "Sec-Fetch-Dest": "empty",
            "Sec-Fetch-Mode": "cors",
            "Sec-Fetch-Site": "same-origin",
            "X-Requested-With": "XMLHttpRequest",
        }

        # self.headersとマージ
        request_headers = self.headers.copy() if self.headers else {}
        request_headers.update(pixiv_headers)

        res = cm.get_with_cookie(url, {}, request_headers, log_404_as_error=False)
        if res and res.status_code == 200:
            logging.debug(f"[no login] {url}")
            return res

        if res and res.status_code == 404:
            logging.debug(f"[no login][404] {url}")
        elif res is not None:
            logging.debug(f"[no login][{res.status_code}] {url}")

        logging.debug(f"[with login] {url}")
        res_login = cm.get_with_cookie(url, self.cookies, request_headers)
        if res_login and res_login.status_code == 200:
            logging.debug(f"[with login][200] {url}")
            return res_login

        if res_login is not None:
            logging.debug(f"[with login][{res_login.status_code}] {url}")
            return res_login

        # ログイン側も取得失敗のときは、元のレスポンスを返しておく
        return res

    def get_json(self, url: str) -> Optional[Dict]:
        """APIからJSONを取得（Cookieなし優先、失敗時はCookieありで再試行）"""
        res = self._request(url)
        if not res or res.status_code != 200:
            return None
        try:
            data = res.json()
            if not data.get("error", False):
                return data
        except:
            pass
        res = cm.get_with_cookie(url, self.cookies, self.headers)
        if not res or res.status_code != 200:
            return None
        try:
            return res.json()
        except:
            try:
                return json.loads(unescape(res.text))
            except:
                return None

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
        old_date = safe_fromiso(old.get("updateDate"))
        if old_date and new_date:
            return new_date != old_date
        return True

    def _is_assets_missing(self, raw_data: Dict, ncode: str) -> bool:
        """
        raw.json のデータに含まれる画像（表紙・本文挿絵）が
        ローカル（DBおよびファイルシステム）に揃っているか確認する。
        戻り値: True=欠損あり（再DL必要）, False=完全（DL不要）
        """
        # 1. 表紙チェック (DB登録 & 実体存在)
        # 拡張子の候補
        cover_ok = False
        for ext in [".jpg", ".jpeg", ".png", ".gif"]:
            # check_image_file は database.json を確認し、かつファイルが存在すればそのパス(ファイル名)を返す
            if cm.check_image_file(
                self.img_path, f"pixiv_{ncode}_cover{ext}", is_cover_check=True
            ):
                cover_ok = True
                break

        if not cover_ok:
            logging.info(f"Asset missing (Cover): {ncode}")
            return True

        # 2. 本文画像チェック (DB登録 & 実体存在)
        # 全エピソードのテキストを結合して検索
        all_text = ""
        if "episodes" in raw_data:
            for ep in raw_data["episodes"].values():
                all_text += ep.get("text", "") + "\n"

        # [image](filename) 形式を抽出
        images = re.findall(r"\[image\]\((.*?)\)", all_text)
        for img_file in images:
            # check_image_file は database.json への登録 & ファイル存在を両方チェック
            # 本文画像チェックでは _cover. を含むエントリを除外
            if not cm.check_image_file(self.img_path, img_file, is_cover_check=False):
                logging.info(
                    f"Asset missing (Image): {img_file} in {ncode} (not in DB or file missing)"
                )
                return True

        return False

    def _snapshot_path(self, folder_path: str, user_id: str) -> str:
        return os.path.join(folder_path, "snapshots", "illust_ids", f"{user_id}.json")

    def _load_illust_snapshot(
        self, folder_path: str, user_id: str, fallback: List
    ) -> List:
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
        if not url:
            return

        ncode = os.path.basename(folder_path)
        ext = os.path.splitext(url)[1]

        # 既に存在するかチェック
        if cm.check_image_file(self.img_path, f"pixiv_{ncode}_cover{ext}"):
            return

        candidates = [
            url.replace("c/600x600/novel-cover-master", "novel-cover-original")
            .replace("_master1200", "")
            .replace(".jpg", e)
            for e in [".png", ".jpg", ".jpeg", ".gif"]
        ] + [url]

        for cand in candidates:
            res = self._request(cand)
            if res and res.status_code == 200:
                f_ext = os.path.splitext(cand)[1]
                # グローバル images フォルダへ保存
                hash_name = cm.check_image_hash(
                    self.img_path,
                    res.content,
                    f"pixiv_{ncode}_cover{f_ext}",
                    is_cover=True,
                )
                with open(
                    os.path.join(self.img_path, f"{hash_name}{f_ext}"), "wb"
                ) as f:
                    f.write(res.content)
                return

    def _format_image_links(
        self,
        text: str,
        content_id: str,
        json_data: Dict,
        folder_path: str,
        is_series: bool = False,
        episode: int = 0,
    ) -> str:
        """本文中の [pixivimage] / [uploadedimage] を処理し、画像をダウンロード"""

        # ID修正
        text = re.sub(r"\[pixivimage:(\d+)\]", r"[pixivimage:\1-1]", text)

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
                p_data = self.get_json(
                    f"https://www.pixiv.net/ajax/illust/{art_id}/pages"
                )
                if not p_data:
                    continue
                pages = p_data.get("body", [])

                for idx, page in enumerate(pages):
                    p_num_str = str(idx + 1)
                    if p_num_str not in pages_needed:
                        continue

                    url = page["urls"]["original"]
                    ext = os.path.splitext(url)[1]
                    img_name = f"pixiv_{art_id}_p{idx}{ext}"

                    # 画像ダウンロード
                    saved_file = cm.check_image_file(self.img_path, img_name)
                    if not saved_file:
                        res = self._request(url)
                        if res and res.status_code == 200:
                            img_hash = cm.check_image_hash(
                                self.img_path, res.content, img_name
                            )
                            with open(
                                os.path.join(self.img_path, f"{img_hash}{ext}"), "wb"
                            ) as f:
                                f.write(res.content)
                            saved_file = f"{img_hash}{ext}"

                    if saved_file:
                        text = text.replace(
                            f"[pixivimage:{art_id}-{p_num_str}]",
                            f"[image]({saved_file})",
                        )

            except Exception as e:
                logging.error(f"Image DL error art={art_id}: {e}")

        # --- uploadedimage 処理 ---
        for inner_link in inner_links:
            try:
                upload_info = cm.find_key_recursively(json_data, inner_link)
                if not upload_info:
                    continue

                url = upload_info["urls"]["original"]
                ext = os.path.splitext(url)[1]
                img_name = f"pixiv_{content_id}_{inner_link}{ext}"

                self._sleep()
                saved_file = cm.check_image_file(self.img_path, img_name)
                if not saved_file:
                    res = self._request(url)
                    if res and res.status_code == 200:
                        img_hash = cm.check_image_hash(
                            self.img_path, res.content, img_name
                        )
                        with open(
                            os.path.join(self.img_path, f"{img_hash}{ext}"), "wb"
                        ) as f:
                            f.write(res.content)
                        saved_file = f"{img_hash}{ext}"

                if saved_file:
                    text = text.replace(
                        f"[uploadedimage:{inner_link}]", f"[image]({saved_file})"
                    )

            except Exception as e:
                logging.error(f"Inner image DL error {inner_link}: {e}")

        return text

    def _format_novel_text(
        self,
        text: str,
        novel_id: str,
        json_data: Dict,
        folder_path: str,
        is_series: bool = False,
        episode_num: int = 0,
    ) -> str:
        text = text.replace("\r\n", "\n")
        text = self._format_image_links(
            text, novel_id, json_data, folder_path, is_series, episode_num
        )
        text = format_ruby(text)
        text = remove_chapter_tag(text)
        text = format_for_url(text)
        return text

    # -------------------------------------------------------------------------
    # ダウンロードロジック
    # -------------------------------------------------------------------------

    @suppress_errors()
    def download_novel(
        self, novel_id: str, folder_path: str, key_data: str, update: bool = False
    ):
        """短編小説ダウンロード (タグ常時更新対応版)"""
        logging.info(f"Novel ID: {novel_id}")
        json_data = self.get_json(f"https://www.pixiv.net/ajax/novel/{novel_id}")
        if not json_data:
            return

        body = json_data.get("body", {})
        upload_date = safe_fromiso(body.get("uploadDate"))

        novel_path = os.path.join(folder_path, f"n{novel_id}")
        raw_path = os.path.join(novel_path, "raw", "raw.json")

        # --- タグ情報の生成 (常に最新を使うためここで作成) ---
        tags = format_tags(
            [t.get("tag", "") for t in body.get("tags", {}).get("tags", [])]
        )
        tags = add_ai_tag_if_needed(body.get("aiType"), tags)

        # --- 更新チェックとタグのみ更新処理 ---
        if update and os.path.isfile(raw_path):
            # 既存データをロード
            old_data = cm._load_json_safe(raw_path)
            if old_data:
                old_date = safe_fromiso(old_data.get("updateDate"))
                # 日付が一致する場合 (本文更新なし)
                if old_date and upload_date and old_date == upload_date:
                    # 資産チェック
                    if not self._is_assets_missing(old_data, f"n{novel_id}"):
                        # 資産が完全な場合のみ、タグチェックや更新なしチェックを行う
                        if old_data.get("tags") != tags:
                            logging.info(
                                f"{body.get('title')} : 本文更新なし、タグのみ更新します。"
                            )
                            old_data["tags"] = tags
                            old_data["all_tags"] = tags  # 短編は tags=all_tags
                            self._save_raw_file(raw_path, old_data)
                            return
                        else:
                            logging.info(f"{body.get('title')} : 更新はありません。")
                            return
                    else:
                        # 資産が欠損している場合は再ダウンロード処理へ進む
                        logging.info(
                            f"{body.get('title')} : ローカルリソース欠損を検出、再取得を実行します。"
                        )
                        # returnしないことで、この後の通常ダウンロード処理が実行される

        cm.make_dir(f"n{novel_id}", folder_path)

        # 本文・表紙
        text = self._format_novel_text(
            body.get("content", ""), novel_id, json_data, folder_path
        )
        self.get_cover(body.get("coverUrl"), novel_path)

        tags = format_tags(
            [t.get("tag", "") for t in body.get("tags", {}).get("tags", [])]
        )
        tags = add_ai_tag_if_needed(body.get("aiType"), tags)  # ← 追加
        poll = cm.find_key_recursively(body, "pollData")

        novel_data = {
            "version": VERSION,
            "get_date": str(datetime.now(JST)),
            "title": body.get("title"),
            "id": novel_id,
            "nid": f"n{novel_id}",
            "url": f"https://www.pixiv.net/novel/show.php?id={novel_id}",
            "author": body.get("userName"),
            "author_id": body.get("userId"),
            "author_url": f"https://www.pixiv.net/users/{body.get('userId')}",
            "caption": body.get("description", "").replace("<br />", "\n"),
            "total_episodes": 1,
            "all_episodes": 1,
            "total_characters": body.get("characterCount"),
            "all_characters": body.get("characterCount"),
            "type": "novel",
            "serialization": "短編",
            "tags": tags,
            "all_tags": tags,
            "createDate": safe_date_str(body.get("createDate")),
            "updateDate": safe_date_str(body.get("uploadDate")),
            "episodes": {
                1: {
                    "id": novel_id,
                    "chapter": None,
                    "title": body.get("title"),
                    "textCount": body.get("characterCount"),
                    "tags": tags,
                    "introduction": unquote(
                        body.get("description", "").replace("<br />", "\n")
                    ),
                    "text": text,
                    "postscript": format_survey(poll) if poll else "",
                    "createDate": safe_date_str(body.get("createDate")),
                    "updateDate": safe_date_str(body.get("uploadDate")),
                }
            },
        }

        cm.save_raw_diff(raw_path, novel_path, novel_data) if update else None
        self._save_raw_file(raw_path, novel_data)
        cn.narou_gen(
            novel_data, novel_path, key_data, self.data_path, "", util.get_img_url()
        )
        cm.gen_site_index(folder_path, key_data, "Pixiv")

    @suppress_errors()
    def download_series(
        self, series_id: str, folder_path: str, key_data: str, update: bool = False
    ):
        """シリーズ小説ダウンロード (タグ常時更新対応版)"""
        logging.info(f"Series ID: {series_id}")

        s_detail = self.get_json(f"https://www.pixiv.net/ajax/novel/series/{series_id}")
        if not s_detail:
            return
        body = s_detail["body"]

        series_path = os.path.join(folder_path, f"s{series_id}")
        raw_path = os.path.join(series_path, "raw", "raw.json")

        # --- タグ情報の生成 ---
        series_tags = format_tags(list(body.get("tags", [])))
        series_tags = add_ai_tag_if_needed(body.get("aiType"), series_tags)

        series_update_date = safe_fromiso(body.get("updateDate"))

        # --- 更新チェックとタグのみ更新処理 ---
        if update and os.path.isfile(raw_path):
            old_data = cm._load_json_safe(raw_path)
            if old_data:
                old_date = safe_fromiso(old_data.get("updateDate"))

                # 日付が一致する場合
                if old_date and series_update_date and old_date == series_update_date:
                    # 資産チェック
                    if not self._is_assets_missing(old_data, f"s{series_id}"):
                        # 資産が完全な場合のみ、タグチェックや更新なしチェックを行う
                        if old_data.get("tags", []) != series_tags:
                            logging.info(
                                f"{body.get('title')} : シリーズ更新なし、タグのみ更新します。"
                            )

                            old_data["tags"] = series_tags

                            # all_tags にも新しいシリーズタグを反映させる (重複排除してマージ)
                            # ※各話のタグは再取得しないため、既存のall_tagsに新しいシリーズタグを追加する形にする
                            current_all = set(old_data.get("all_tags", []))
                            current_all.update(series_tags)
                            old_data["all_tags"] = format_tags(list(current_all))

                            self._save_raw_file(raw_path, old_data)
                            return
                        else:
                            logging.info(f"{body.get('title')} : 更新はありません。")
                            return
                    else:
                        # 資産が欠損している場合は再ダウンロード処理へ進む
                        logging.info(
                            f"{body.get('title')} : ローカルリソース欠損を検出、再取得を実行します。"
                        )
                        # returnしないことで、この後の通常ダウンロード処理が実行される

        # --- 以下、通常ダウンロード処理 ---
        s_toc = self.get_json(
            f"https://www.pixiv.net/ajax/novel/series/{series_id}/content_titles"
        )
        if not s_toc:
            return
        toc = s_toc["body"]

        cm.make_dir(f"s{series_id}", folder_path)
        self.get_cover(
            body.get("cover", {}).get("urls", {}).get("original"), series_path
        )

        # シリーズ本体のタグ（AI生成含む）を決定
        series_tags = format_tags(list(body.get("tags", [])))
        series_tags = add_ai_tag_if_needed(body.get("aiType"), series_tags)

        # all_tags はシリーズタグからスタートし、各話のタグを追加していく
        all_tags = series_tags.copy()
        episodes_data = {}
        total_text = 0

        # 既存エピソード読み込み
        old_eps = {}
        if update and os.path.isfile(raw_path):
            # load済みかもしれないが、念のため
            raw_data = cm._load_json_safe(raw_path)
            old_eps = raw_data.get("episodes", {}) if raw_data else {}

        for idx, entry in tqdm(
            enumerate(toc, 1),
            total=len(toc),
            desc=f"Downloading series {series_id}",
            leave=False,
        ):
            if not entry.get("available"):
                continue

            ep_id = entry["id"]
            ep_folder = os.path.join(series_path, str(ep_id))
            os.makedirs(ep_folder, exist_ok=True)

            # エピソード更新チェック
            ep_update = False
            old_ep_data = old_eps.get(str(idx))
            if update and old_ep_data:
                old_ud = safe_fromiso(old_ep_data.get("updateDate"))
                new_ud = safe_fromiso(entry.get("updateDate"))
                if old_ud and new_ud and old_ud == new_ud:
                    ep_update = True

            if ep_update:
                # 古いデータをそのまま使う
                episodes_data[idx] = old_ep_data
                total_text += old_ep_data.get("textCount", 0)
                # タグ統合（旧データ側に既に AI生成 が入っていればそのまま反映）
                all_tags.extend(old_ep_data.get("tags", []))
            else:
                self._sleep()
                ep_json = self.get_json(f"https://www.pixiv.net/ajax/novel/{ep_id}")
                if not ep_json:
                    continue
                ep_body = ep_json["body"]

                self.get_cover(ep_body.get("coverUrl"), ep_folder)

                text = self._format_novel_text(
                    ep_body.get("content", ""), ep_id, ep_json, folder_path, True, ep_id
                )
                tags = format_tags(
                    [t.get("tag", "") for t in ep_body.get("tags", {}).get("tags", [])]
                )
                tags = add_ai_tag_if_needed(ep_body.get("aiType"), tags)
                all_tags.extend(tags)
                poll = cm.find_key_recursively(ep_body, "pollData")

                t_count = int(ep_body.get("characterCount", 0))
                total_text += t_count

                episodes_data[idx] = {
                    "id": ep_id,
                    "chapter": None,
                    "title": entry.get("title"),
                    "textCount": t_count,
                    "tags": tags,
                    "introduction": unquote(
                        ep_body.get("description", "").replace("<br />", "\n")
                    ),
                    "text": text,
                    "postscript": format_survey(poll) if poll else "",
                    "createDate": safe_date_str(ep_body.get("createDate")),
                    "updateDate": safe_date_str(ep_body.get("uploadDate")),
                }

                # 重複フォルダ削除（元コードロジック）
                dup = os.path.join(folder_path, f"n{ep_id}")
                if os.path.exists(dup):
                    shutil.rmtree(dup)

        # エピソードを createDate 昇順でソートしてキーを振り直す
        sorted_eps = sorted(
            episodes_data.values(),
            key=lambda ep: ep.get("createDate") or "",
        )
        episodes_data = {i: ep for i, ep in enumerate(sorted_eps, 1)}

        novel_data = {
            "version": VERSION,
            "get_date": str(datetime.now(JST)),
            "title": body.get("title"),
            "id": series_id,
            "nid": f"s{series_id}",
            "url": f"https://www.pixiv.net/novel/series/{series_id}",
            "author": body.get("userName"),
            "author_id": body.get("userId"),
            "author_url": f"https://www.pixiv.net/users/{body.get('userId')}",
            "caption": body.get("caption", "").replace("<br />", "\n"),
            "total_episodes": len(episodes_data),
            "all_episodes": body.get("total"),
            "total_characters": total_text,
            "all_characters": body.get("publishedTotalCharacterCount"),
            "type": "novel",
            "serialization": "連載中",
            # ← ここで series_tags をそのまま使うのが大事
            "tags": series_tags,
            # all_tags は重複除去した上で format_tags に通す
            "all_tags": format_tags(list(set(all_tags))),
            "createDate": safe_date_str(body.get("createDate")),
            "updateDate": safe_date_str(body.get("updateDate")),
            "episodes": episodes_data,
        }

        if update:
            cm.save_raw_diff(raw_path, series_path, novel_data)
        self._save_raw_file(raw_path, novel_data)
        cn.narou_gen(
            novel_data, series_path, key_data, self.data_path, "", util.get_img_url()
        )
        cm.gen_site_index(folder_path, key_data, "Pixiv")

    @suppress_errors()
    def download_art(
        self, art_id: str, folder_path: str, key_data: str, update: bool = False
    ):
        """短編漫画/イラストダウンロード (APNG/Ugoira対応)"""
        logging.info(f"Art ID: {art_id}")

        # 消えてる可能性チェック
        a_data = self.get_json(f"https://www.pixiv.net/ajax/illust/{art_id}")
        if not a_data:
            return
        body = a_data["body"]

        # --- 更新チェックとタグのみ更新処理 ---
        art_path = os.path.join(folder_path, f"a{art_id}")
        raw_path = os.path.join(art_path, "raw", "raw.json")

        # タグ情報の生成
        tags = format_tags(
            [t.get("tag", "") for t in body.get("tags", {}).get("tags", [])]
        )
        tags = add_ai_tag_if_needed(body.get("aiType"), tags)

        new_update_date = safe_fromiso(body.get("uploadDate"))

        if update and os.path.isfile(raw_path):
            old_data = cm._load_json_safe(raw_path)
            if old_data:
                old_date = safe_fromiso(old_data.get("updateDate"))

                # 日付が一致する場合
                if old_date and new_update_date and old_date == new_update_date:
                    # 資産チェック
                    if not self._is_assets_missing(old_data, f"a{art_id}"):
                        # 資産が完全な場合のみ、タグチェックや更新なしチェックを行う
                        if old_data.get("tags") != tags:
                            logging.info(
                                f"{body.get('title')} : 更新なし、タグのみ更新します。"
                            )
                            old_data["tags"] = tags
                            old_data["all_tags"] = tags
                            self._save_raw_file(raw_path, old_data)
                            return
                        else:
                            logging.info(f"{body.get('title')} : 更新はありません。")
                            return
                    else:
                        # 資産が欠損している場合は再ダウンロード処理へ進む
                        logging.info(
                            f"{body.get('title')} : ローカルリソース欠損を検出、再取得を実行します。"
                        )
                        # returnしないことで、この後の通常ダウンロード処理が実行される

        a_pages = self.get_json(f"https://www.pixiv.net/ajax/illust/{art_id}/pages")
        pages = a_pages.get("body", []) if a_pages else []

        cm.make_dir(f"a{art_id}", folder_path)

        # 本文構築
        art_text = ""
        for page in pages:
            url = page["urls"]["original"]
            if "_ugoira" in url:
                # うごイラ処理
                anim_name = f"pixiv_{art_id}_ugoira.apng"
                if cm.check_image_file(self.img_path, anim_name):
                    art_text += (
                        f"[image]({cm.check_image_file(self.img_path, anim_name)})\n"
                    )
                    continue

                self._sleep()
                meta = self.get_json(
                    f"https://www.pixiv.net/ajax/illust/{art_id}/ugoira_meta?lang=ja"
                )
                if meta:
                    m_body = meta["body"]
                    src_url = m_body["originalSrc"]

                    zip_res = self._request(src_url)
                    zip_path = os.path.join(art_path, f"{art_id}.zip")
                    with open(zip_path, "wb") as f:
                        f.write(zip_res.content)

                    # 解凍
                    temp_dir = os.path.join(art_path, "temp")
                    os.makedirs(temp_dir, exist_ok=True)
                    with zipfile.ZipFile(zip_path) as zf:
                        zf.extractall(temp_dir)
                    os.remove(zip_path)

                    # APNG作成
                    frames = []
                    delays = []
                    opened_images = []  # 開いた画像を追跡
                    try:
                        for f_info in m_body["frames"]:
                            img = Image.open(os.path.join(temp_dir, f_info["file"]))
                            frames.append(img)
                            opened_images.append(img)  # 追跡リストに追加
                            delays.append(f_info["delay"])

                        # apng保存 (apngライブラリ使用)
                        temp_apng = os.path.join(temp_dir, "temp.apng")
                        if frames:
                            apng_obj = apng.APNG()
                            for i, frame_file in enumerate(m_body["frames"]):
                                apng_obj.append_file(
                                    os.path.join(temp_dir, frame_file["file"]),
                                    delay=frame_file["delay"],
                                )
                            apng_obj.save(temp_apng)

                        with open(temp_apng, "rb") as f:
                            img_data = f.read()

                        final_name = cm.check_image_hash(self.img_path, img_data, anim_name)
                        with open(
                            os.path.join(self.img_path, f"{final_name}.apng"), "wb"
                        ) as f:
                            f.write(img_data)

                        art_text += f"[image]({final_name}.apng)\n"
                    finally:
                        # すべての画像を明示的にクローズ
                        for img in opened_images:
                            try:
                                img.close()
                            except:
                                pass
                        
                        # ガベージコレクションを強制実行してファイルハンドルを解放
                        import gc
                        gc.collect()
                    
                    # temp_dirを削除
                    try:
                        shutil.rmtree(temp_dir)
                    except PermissionError:
                        # ファイルがロックされている場合は少し待ってから再試行
                        import time
                        time.sleep(0.5)
                        shutil.rmtree(temp_dir)
            else:
                # 通常画像
                match = re.search(r"_p(\d+)\.", url)
                if match:
                    num = int(match.group(1)) + 1
                    art_text += f"[pixivimage:{art_id}-{num}]\n"

        # 画像DL処理
        art_text = self._format_image_links(art_text, art_id, a_data, folder_path)
        self.get_cover(body.get("urls", {}).get("original"), art_path)

        tags = format_tags(
            [t.get("tag", "") for t in body.get("tags", {}).get("tags", [])]
        )
        tags = add_ai_tag_if_needed(body.get("aiType"), tags)  # ← 追加
        poll = cm.find_key_recursively(body, "pollData")

        novel_data = {
            "version": VERSION,
            "get_date": str(datetime.now(JST)),
            "title": body.get("title"),
            "id": art_id,
            "nid": f"a{art_id}",
            "url": f"https://www.pixiv.net/artworks/{art_id}",
            "author": body.get("userName"),
            "author_id": body.get("userId"),
            "author_url": f"https://www.pixiv.net/users/{body.get('userId')}",
            "caption": body.get("description", "").replace("<br />", "\n"),
            "total_episodes": 1,
            "all_episodes": 1,
            "total_characters": 0,
            "all_characters": 0,
            "type": "comic",
            "serialization": "短編",
            "tags": tags,
            "all_tags": tags,
            "createDate": safe_date_str(body.get("createDate")),
            "updateDate": safe_date_str(body.get("uploadDate")),
            "episodes": {
                1: {
                    "id": art_id,
                    "chapter": None,
                    "title": body.get("title"),
                    "textCount": 0,
                    "tags": tags,
                    "introduction": unquote(
                        body.get("description", "").replace("<br />", "\n")
                    ),
                    "text": art_text,
                    "postscript": format_survey(poll) if poll else "",
                    "createDate": safe_date_str(body.get("createDate")),
                    "updateDate": safe_date_str(body.get("uploadDate")),
                }
            },
        }

        # イラストは常に全データ取得なので差分保存不要
        # cm.save_raw_diff(raw_path, art_path, novel_data)
        self._save_raw_file(raw_path, novel_data)
        cn.narou_gen(
            novel_data, art_path, key_data, self.data_path, "", util.get_img_url()
        )
        cm.gen_site_index(folder_path, key_data, "Pixiv")

    @suppress_errors()
    def download_comic(
        self, comic_id: str, folder_path: str, key_data: str, update: bool = False
    ):
        """漫画シリーズダウンロード (タグ常時更新対応版)"""
        logging.info(f"Comic ID: {comic_id}")

        # シリーズ情報取得 (page 1)
        resp = self.get_json(
            f"https://www.pixiv.net/ajax/series/{comic_id}?p=1&lang=ja"
        )
        if not resp:
            return
        c_detail = cm.find_key_recursively(resp, "body")

        # --- タグ情報の生成 ---
        new_tags = format_tags(list(c_detail.get("tagTranslation", {}).keys()))
        # 漫画シリーズ自体には aiType が直接取れないことが多いが、取れる場合はここで処理
        # ここではとりあえずタグリストのみ取得

        comic_dir = os.path.join(folder_path, f"c{comic_id}")
        raw_path = os.path.join(comic_dir, "raw", "raw.json")
        cm.make_dir(f"c{comic_id}", folder_path)

        # シリーズ更新チェック
        series_update_date = None
        for j in c_detail.get("illustSeries", []):
            if str(j["id"]) == str(comic_id):
                series_update_date = safe_fromiso(j["updateDate"])
                break

        # --- 更新チェックとタグのみ更新処理 ---
        if update and os.path.isfile(raw_path):
            old_data = cm._load_json_safe(raw_path)
            if old_data:
                old_date = safe_fromiso(old_data.get("updateDate"))

                if old_date and series_update_date and old_date == series_update_date:
                    # 資産チェック
                    if not self._is_assets_missing(old_data, f"c{comic_id}"):
                        # 資産が完全な場合のみ、タグチェックや更新なしチェックを行う
                        if old_data.get("tags", []) != new_tags:
                            # メタデータ
                            meta = c_detail["extraData"]["meta"]
                            title = meta["twitter"]["title"]
                            logging.info(
                                f"{title} : シリーズ更新なし、タグのみ更新します。"
                            )

                            old_data["tags"] = new_tags

                            # all_tags の更新 (既存 + 新規タグ)
                            current_all = set(old_data.get("all_tags", []))
                            current_all.update(new_tags)
                            old_data["all_tags"] = format_tags(list(current_all))

                            self._save_raw_file(raw_path, old_data)
                            return
                        else:
                            # メタデータ取得前なのでタイトルが出せないがログ出力
                            logging.info(f"Comic {comic_id} : 更新はありません。")
                            return
                    else:
                        # 資産が欠損している場合は再ダウンロード処理へ進む
                        logging.info(
                            f"{c_detail['extraData']['meta']['twitter']['title']} : ローカルリソース欠損を検出、再取得を実行します。"
                        )
                        # returnしないことで、この後の通常ダウンロード処理が実行される

        # --- 以下、通常ダウンロード処理 ---
        # リンク取得（ページング対応）
        arts = {}
        page = 1
        while True:
            self._sleep()
            p_json = self.get_json(
                f"https://www.pixiv.net/ajax/series/{comic_id}?p={page}&lang=ja"
            )
            if not p_json:
                break
            series_data = cm.find_key_recursively(p_json, "body").get("series", [])
            if not series_data:
                break

            for item in series_data:
                arts[item["order"]] = item["workId"]

            if len(series_data) == 0:
                break
            page += 1

        if not arts:
            return

        # メタデータ
        meta = c_detail["extraData"]["meta"]
        title = meta["twitter"]["title"]
        try:
            author = re.search(r"「[^」]*」/「(.*?)」のシリーズ", meta["title"]).group(
                1
            )
        except:
            author = "Unknown"
        try:
            author_id = re.search(r"user/(\d+)/series", meta["canonical"]).group(1)
        except:
            author_id = "0"

        # エピソードDL
        episodes_data = {}
        all_tags = new_tags.copy()  # 初期値はシリーズタグ

        old_eps = {}
        if update and os.path.isfile(raw_path):
            raw_data = cm._load_json_safe(raw_path)
            old_eps = raw_data.get("episodes", {}) if raw_data else {}

        arts = dict(sorted(arts.items()))

        for idx, work_id in tqdm(
            arts.items(), desc=f"Downloading comic {comic_id}", leave=False
        ):
            ep_folder = os.path.join(comic_dir, str(work_id))
            os.makedirs(ep_folder, exist_ok=True)

            # エピソード更新チェック (Illust API)
            ep_update = False
            old_ep = old_eps.get(str(idx))
            if update and old_ep:
                self._sleep()
                det = self.get_json(f"https://www.pixiv.net/ajax/illust/{work_id}")
                if det:
                    new_ud = safe_fromiso(det["body"]["uploadDate"])
                    old_ud = safe_fromiso(old_ep["updateDate"])
                    if new_ud == old_ud:
                        ep_update = True

            if ep_update:
                episodes_data[idx] = old_ep
                all_tags.extend(old_ep.get("tags", []))
            else:
                self._sleep()
                ep_data = self.get_json(f"https://www.pixiv.net/ajax/illust/{work_id}")
                if not ep_data:
                    continue
                body = ep_data["body"]

                # 画像リンク構築
                pages_res = self.get_json(
                    f"https://www.pixiv.net/ajax/illust/{work_id}/pages"
                )
                text = ""
                if pages_res:
                    for p in pages_res.get("body", []):
                        u = p["urls"]["original"]
                        num = int(re.search(r"_p(\d+)\.", u).group(1)) + 1
                        text += f"[pixivimage:{work_id}-{num}]\n"

                text = self._format_image_links(
                    text, work_id, ep_data, folder_path, True, work_id
                )
                self.get_cover(body.get("urls", {}).get("original"), ep_folder)

                tags = format_tags(
                    [t.get("tag", "") for t in body.get("tags", {}).get("tags", [])]
                )
                tags = add_ai_tag_if_needed(body.get("aiType"), tags)
                all_tags.extend(tags)
                poll = cm.find_key_recursively(body, "pollData")

                episodes_data[idx] = {
                    "id": work_id,
                    "chapter": None,
                    "title": body.get("title"),
                    "textCount": 0,
                    "tags": tags,
                    "introduction": unquote(
                        body.get("description", "").replace("<br />", "\n")
                    ),
                    "text": text,
                    "postscript": format_survey(poll) if poll else "",
                    "createDate": safe_date_str(body.get("createDate")),
                    "updateDate": safe_date_str(body.get("uploadDate")),
                }

                # 重複フォルダ削除
                dup = os.path.join(folder_path, f"a{work_id}")
                if os.path.exists(dup):
                    shutil.rmtree(dup)

        # エピソードを createDate 昇順でソートしてキーを振り直す
        sorted_eps = sorted(
            episodes_data.values(),
            key=lambda ep: ep.get("createDate") or "",
        )
        episodes_data = {i: ep for i, ep in enumerate(sorted_eps, 1)}

        novel_data = {
            "version": VERSION,
            "get_date": str(datetime.now(JST)),
            "title": title,
            "id": comic_id,
            "nid": f"c{comic_id}",
            "url": f"https://www.pixiv.net/user/{author_id}/series/{comic_id}",
            "author": author,
            "author_id": author_id,
            "caption": meta.get("description", "").replace("<br />", "\n"),
            "total_episodes": len(episodes_data),
            "all_episodes": len(episodes_data),
            "total_characters": 0,
            "all_characters": 0,
            "type": "comic",
            "serialization": "連載中",
            "tags": new_tags,
            "all_tags": format_tags(list(set(all_tags))),
            "createDate": safe_date_str(
                c_detail.get("illustSeries", [{}])[0].get("createDate")
            ),
            "updateDate": safe_date_str(
                c_detail.get("illustSeries", [{}])[0].get("updateDate")
            ),
            "episodes": episodes_data,
        }

        cm.save_raw_diff(raw_path, comic_dir, novel_data) if update else None
        self._save_raw_file(raw_path, novel_data)
        cn.narou_gen(
            novel_data, comic_dir, key_data, self.data_path, "", util.get_img_url()
        )
        cm.gen_site_index(folder_path, key_data, "Pixiv")

    @suppress_errors()
    def download_user(
        self, user_id: str, folder_path: str, key_data: str, update: bool = False
    ):
        """ユーザー一括ダウンロード (スナップショット機能付き)"""
        logging.info(f"User ID: {user_id}")

        # ユーザー設定読み込み
        user_json_path = os.path.join(folder_path, "user.json")

        full_conf = cm._load_json_safe(user_json_path)
        user_conf = full_conf.get(user_id, {})

        # 初期化
        if not user_conf:
            user_conf = {
                "novel": "enable",
                "comic": "enable",
                "illust_ids_snapshot": [],
            }
            # 再読み込み (atomic writeにより競合は減るが、最新状態を取得)
            full_conf = cm._load_json_safe(user_json_path) or {"version": 3}
            user_conf = full_conf.get(user_id, {})

            if not user_conf:
                user_conf = {
                    "novel": "enable",
                    "comic": "enable",
                    "illust_ids_snapshot": [],
                }
                full_conf[user_id] = user_conf
                cm._save_json(user_json_path, full_conf)

            full_conf[user_id] = user_conf
            cm._save_json(user_json_path, full_conf)

        # プロフィール全取得
        all_data = self.get_json(
            f"https://www.pixiv.net/ajax/user/{user_id}/profile/all"
        )
        if not all_data:
            return
        body = all_data["body"]

        # エラー修正: 辞書型・リスト型両対応でIDリスト抽出
        # novelSeries
        ns_data = body.get("novelSeries")
        n_series = []
        if isinstance(ns_data, list):
            n_series = [str(x["id"]) for x in ns_data]
        elif isinstance(ns_data, dict):
            n_series = list(ns_data.keys())

        # mangaSeries
        ms_data = body.get("mangaSeries")
        m_series = []
        if isinstance(ms_data, list):
            m_series = [str(x["id"]) for x in ms_data]
        elif isinstance(ms_data, dict):
            m_series = list(ms_data.keys())

        # novels, illusts, manga (これらは通常Dictだが念の為チェック)
        novels_data = body.get("novels", {})
        novels = list(novels_data.keys()) if isinstance(novels_data, dict) else []

        illusts_data = body.get("illusts", {})
        illusts = list(illusts_data.keys()) if isinstance(illusts_data, dict) else []

        mangas_data = body.get("manga", {})
        mangas = list(mangas_data.keys()) if isinstance(mangas_data, dict) else []

        user_name = self.get_json(f"https://www.pixiv.net/ajax/user/{user_id}")["body"][
            "name"
        ]
        logging.info(f"User Name: {user_name}")

        # シリーズ重複除外（事前チェック）
        in_n_series = []
        for sid in n_series:
            self._sleep()
            toc = self.get_json(
                f"https://www.pixiv.net/ajax/novel/series/{sid}/content_titles"
            )
            if toc:
                in_n_series.extend([str(t["id"]) for t in toc["body"]])
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
            if (
                hasattr(_crawler, "_corrupt_comics_pending")
                and _crawler._corrupt_comics_pending
            ):
                logging.info(f"Checking pending corrupted comics for user {user_id}...")
                for pending_item in list(_crawler._corrupt_comics_pending):
                    folder_name, corrupt_path, cid = pending_item
                    # このユーザーの漫画シリーズに該当するか確認
                    if cid in m_series:
                        logging.info(
                            f"Found pending comic {cid} in user {user_id} series, re-downloading..."
                        )
                        try:
                            self.download_comic(
                                cid, folder_path, key_data, update=False
                            )
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

            # スナップショット読み込み（旧規格からの移行処理を含む）
            snapshot_file = self._snapshot_path(folder_path, user_id)
            old_snapshot_array = user_conf.get("illust_ids_snapshot", [])

            # snapshotsファイルが存在しない場合、旧規格配列から移行
            if not os.path.exists(snapshot_file) and old_snapshot_array:
                # ハッシュで検証
                old_hash = user_conf.get("illust_ids_snapshot_hash", "")
                current_hash = _hash_ids(old_snapshot_array)
                if old_hash == current_hash:
                    # ハッシュが一致：snapshotsファイルに移行
                    os.makedirs(os.path.dirname(snapshot_file), exist_ok=True)
                    cm._save_json(snapshot_file, old_snapshot_array)
                    logging.info(
                        f"Migrated illust_ids_snapshot array to {snapshot_file}"
                    )
                    # user.jsonから旧規格配列を削除
                    full_conf = cm._load_json_safe(user_json_path)
                    if user_id in full_conf:
                        full_conf[user_id].pop("illust_ids_snapshot", None)
                        cm._save_json(user_json_path, full_conf)

            prev_snapshot = set(
                map(
                    int,
                    self._load_illust_snapshot(
                        folder_path, user_id, old_snapshot_array
                    ),
                )
            )

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
                # 旧規格配列が残っていれば削除（念のため）
                full_conf[user_id].pop("illust_ids_snapshot", None)
                cm._save_json(user_json_path, full_conf)


# --- エントリーポイント (互換性維持) ---

_crawler: Optional[PixivCrawler] = None


def init(cookie_path, data_path, is_login, interval):
    global _crawler
    _crawler = PixivCrawler(cookie_path, data_path, interval)
    if is_login:
        _crawler.login()


def login(cookie_path: str, data_path: str, interval: int, account_name: str = None, display_name: str = None):
    """外部からログイン処理を実行するための公開関数
    
    Args:
        cookie_path: クッキー保存ディレクトリパス
        data_path: データ保存ディレクトリパス
        interval: リクエスト間隔
        account_name: アカウント名（None の場合は login.json、指定時は {account_name}.json）
        display_name: アカウント表示名（省略可）
    """
    global _crawler
    _crawler = PixivCrawler(cookie_path, data_path, interval)
    
    if account_name:
        # 新規アカウントとしてログイン（強制ログイン、指定ファイル名で保存）
        _crawler.login_new_account(account_name, display_name)
    else:
        # 既存の login.json でログイン（既存クッキーが有効ならスキップ）
        _crawler.login(force_login=True)
    
    logging.info("Login completed successfully.")


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


def recover_user_json(folder_path: str, corrupt_user_json_path: str):
    """
    user.jsonの復元
    1. バックアップから復元を試みる（_load_json_safeで自動実行済み）
    2. 破損ファイルから回収可能なデータを抽出
    3. raw.jsonからユーザーIDとイラストIDを収集してハッシュを再計算
    4. データをマージして復元
    """
    logging.warning(f"user.json corrupted: {corrupt_user_json_path}")
    logging.info(
        "Attempting to recover user.json from corrupt file and raw.json files..."
    )

    user_json_path = corrupt_user_json_path.replace(".corrupt", "")
    recovered = {"version": VERSION}

    # Step 1: 破損ファイルからデータを抽出
    try:
        with open(corrupt_user_json_path, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()

        # ユーザーIDブロックを正規表現で抽出: "12345678": { ... }
        user_pattern = r'"(\d+)":\s*\{([^}]+)\}'
        for match in re.finditer(user_pattern, content):
            user_id = match.group(1)
            user_block = match.group(2)
            user_data = {}

            # novel/comic の設定を抽出
            novel_match = re.search(r'"novel":\s*"(enable|disable)"', user_block)
            if novel_match:
                user_data["novel"] = novel_match.group(1)

            comic_match = re.search(r'"comic":\s*"(enable|disable)"', user_block)
            if comic_match:
                user_data["comic"] = comic_match.group(1)

            # illust_ids_snapshot_hash を抽出（あれば）
            hash_match = re.search(
                r'"illust_ids_snapshot_hash":\s*"([a-f0-9]{64})"', user_block
            )
            if hash_match:
                user_data["illust_ids_snapshot_hash"] = hash_match.group(1)

            # 旧規格の illust_ids_snapshot は無視（除去）

            if user_data:
                recovered[user_id] = user_data
                logging.info(f"Recovered user {user_id}: {user_data}")

        # versionも抽出
        version_match = re.search(r'"version":\s*(\d+)', content)
        if version_match:
            recovered["version"] = int(version_match.group(1))

    except Exception as e:
        logging.error(f"Failed to extract from corrupt file: {e}")

    # Step 2: raw.jsonからユーザーIDとイラストIDを収集
    user_illusts = {}  # {user_id: set(illust_ids)}

    logging.info("Scanning raw.json files to collect user IDs and illust IDs...")
    raw_file_count = 0

    for root, dirs, files in os.walk(folder_path):
        if "raw.json" in files:
            raw_path = os.path.join(root, "raw.json")
            raw_file_count += 1
            try:
                data = cm._load_json_safe(raw_path)
                if not data:
                    continue

                # ユーザーIDを取得
                user_id = None
                if "userId" in data:
                    user_id = str(data["userId"])
                elif "user_id" in data:
                    user_id = str(data["user_id"])
                elif "author_id" in data:
                    user_id = str(data["author_id"])

                if not user_id:
                    logging.debug(f"No userId/author_id in {raw_path}")
                    continue

                # イラストIDを収集（小説は除外）
                work_type = data.get("type", "")
                if work_type == "novel":
                    # 小説の場合はイラストIDを収集しない
                    if user_id not in user_illusts:
                        user_illusts[user_id] = set()
                    continue

                illust_ids = set()

                # シリーズの場合（漫画シリーズなど）
                if "series" in data:
                    for episode in data.get("series", []):
                        if isinstance(episode, dict):
                            for illust in episode.get("illusts", []):
                                if isinstance(illust, dict) and "id" in illust:
                                    illust_ids.add(str(illust["id"]))

                # 単体イラスト/漫画（typeがcomicの場合のみ）
                if work_type == "comic" and "id" in data:
                    illust_ids.add(str(data["id"]))

                # illusts配列
                if "illusts" in data and isinstance(data["illusts"], list):
                    for illust in data["illusts"]:
                        if isinstance(illust, dict) and "id" in illust:
                            illust_ids.add(str(illust["id"]))

                if user_id not in user_illusts:
                    user_illusts[user_id] = set()
                user_illusts[user_id].update(illust_ids)

            except Exception as e:
                logging.warning(f"Failed to process {raw_path}: {e}")

    logging.info(
        f"Scanned {raw_file_count} raw.json files, found {len(user_illusts)} unique users"
    )

    # Step 3: データをマージ
    for user_id, illust_ids in user_illusts.items():
        if user_id not in recovered:
            # 新規ユーザー（デフォルト設定）
            recovered[user_id] = {"novel": "enable", "comic": "enable"}
            logging.info(f"Added new user from raw.json: {user_id}")

        # ハッシュを計算（既存の_hash_ids関数を使用）
        # raw.jsonから計算したハッシュを最優先（破損ファイルの値より優先）
        if illust_ids:
            snapshot_hash = _hash_ids(illust_ids)
            recovered[user_id]["illust_ids_snapshot_hash"] = snapshot_hash
            logging.info(f"Updated hash for user {user_id}: {snapshot_hash[:16]}...")
        else:
            # イラストが無い場合は空文字列を設定（小説専業作家など）
            recovered[user_id]["illust_ids_snapshot_hash"] = ""

        # 旧規格の配列を保持（snapshotsへの移行は更新時に行う）
        # illust_ids_snapshotは削除しない

    # Step 4: 復元データを保存
    cm._save_json(user_json_path, recovered)
    logging.info(f"✓ user.json recovered with {len(recovered) - 1} users")

    # .corruptファイルを削除
    if os.path.exists(corrupt_user_json_path):
        os.remove(corrupt_user_json_path)
        logging.info(f"Removed {corrupt_user_json_path}")


def recover_global_image_db(folder_path: str, corrupt_db_path: str):
    """
    グローバル画像DBの復元
    破損したdatabase.json/cover.jsonはバックアップから復元する
    """
    logging.warning(f"Global image DB corrupted: {corrupt_db_path}")
    logging.info(
        "Attempting to restore from backup files (.backup.1, .backup.2, .backup.3)"
    )

    # バックアップからの復元は _load_json_safe で既に試行済み
    # ここでは.corruptファイルを削除するのみ
    if os.path.exists(corrupt_db_path):
        os.remove(corrupt_db_path)
        logging.info(f"Removed {corrupt_db_path}")

    # 復元できなかった場合は空のDBとして再作成される


def recover_from_corrupt_json(corrupt_path: str) -> dict:
    """
    破損したJSONファイルから可能な限りデータを抽出

    戦略:
    1. 部分的に読めるJSONを抽出（複数のJSON objectが含まれる場合）
    2. 正規表現で key-value ペアを抽出
    3. バックアップファイルから抽出
    """
    recovered_data = {}

    # ファイルサイズチェック（空ファイルの場合は処理不要）
    if os.path.exists(corrupt_path) and os.path.getsize(corrupt_path) == 0:
        logging.info(
            f"Corrupt file is empty (0 bytes), skipping recovery: {corrupt_path}"
        )
        return {}

    try:
        with open(corrupt_path, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()

        # 空または空白のみの場合
        if not content.strip():
            logging.info(
                f"Corrupt file contains only whitespace, skipping recovery: {corrupt_path}"
            )
            return {}

        logging.info(f"Attempting aggressive recovery from: {corrupt_path}")

        # 戦略1: 部分的なJSON objectを探す
        import re

        # { ... } の塊を探す
        json_objects = re.findall(r"\{[^{}]*\}", content)
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
                    recovered_data[key] = (
                        int(value) if "." not in value else float(value)
                    )
                except:
                    recovered_data[key] = value

        if recovered_data:
            logging.info(f"Recovered {len(recovered_data)} entries from corrupt file")
        else:
            logging.warning(f"No recoverable data found in {corrupt_path}")

    except Exception as e:
        logging.error(f"Failed to recover from {corrupt_path}: {e}")

    # 戦略3: バックアップファイルから抽出
    backup_base = corrupt_path.replace(".corrupt", "")
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
                    logging.info(
                        f"Merged {len(backup_data)} entries from backup: {backup_path}"
                    )
            except:
                continue

    return recovered_data


def update(folder_path, key_data, data_path, host_name):
    if not _crawler:
        logging.error("Crawler not initialized")
        return

    # 通常の更新処理（修復はrepair()に分離済み）
    logging.info("")
    logging.info("=" * 60)
    logging.info("Starting normal update process...")
    logging.info("=" * 60)

    # index.json から全作品リストを取得
    index_path = os.path.join(folder_path, "index.json")
    if not os.path.exists(index_path):
        return

    index_data = cm._load_json_safe(index_path)
    if not index_data:
        return

    # user.json からユーザー更新
    user_json_path = os.path.join(folder_path, "user.json")
    users = cm._load_json_safe(user_json_path)
    if users:
        for uid in users:
            if uid == "version":
                continue
            _crawler.download_user(uid, folder_path, key_data, update=True)

    # 個別作品更新
    for folder, meta in index_data.items():
        try:
            if meta["type"] == "novel":
                if meta["serialization"] == "短編":
                    nid = folder.lstrip("n")
                    _crawler.download_novel(nid, folder_path, key_data, update=True)
                else:
                    sid = folder.lstrip("s")
                    _crawler.download_series(sid, folder_path, key_data, update=True)
            elif meta["type"] == "comic":
                if meta["serialization"] == "短編":
                    aid = folder.lstrip("a")
                    _crawler.download_art(aid, folder_path, key_data, update=True)
                else:
                    cid = folder.lstrip("c")
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
        if c == "x":
            return hex(r)[2:]
        if c == "y":
            return hex(r & 0x3 | 0x8)[2:]
        if c == "4":
            return "4"
        return c

    return "".join(replace_char(c) for c in template)


def request_re_download(target_id: str, host_name: str):
    """破損データの再ダウンロードをリクエスト（URL形式に変換）"""
    try:
        # フォルダ名からPixiv URLを成形
        pixiv_url = None
        if target_id.startswith("n"):
            # 短編小説: https://www.pixiv.net/novel/show.php?id=12345678
            nid = target_id.lstrip("n")
            pixiv_url = f"https://www.pixiv.net/novel/show.php?id={nid}"
        elif target_id.startswith("s"):
            # 連載小説: https://www.pixiv.net/novel/series/12345678
            sid = target_id.lstrip("s")
            pixiv_url = f"https://www.pixiv.net/novel/series/{sid}"
        elif target_id.startswith("a"):
            # イラスト: https://www.pixiv.net/artworks/12345678
            aid = target_id.lstrip("a")
            pixiv_url = f"https://www.pixiv.net/artworks/{aid}"
        elif target_id.startswith("c"):
            # 漫画シリーズ: https://www.pixiv.net/user/123456/series/789012 (ユーザーIDが必要)
            # ※ comic は author_id が必要なため、ここでは対応不可
            logging.warning(
                f"Comic series {target_id} cannot be auto-repaired via URL (author_id required)"
            )
            return
        else:
            logging.error(f"Unknown target_id format: {target_id}")
            return

        url = f"{host_name}/api/"

        payload = {
            "re_download": pixiv_url,  # URL形式で送信
            "request_id": generate_request_id(),
        }
        # タイムアウトを短めに設定して、サーバーが詰まらないようにする
        requests.post(url, data=payload, timeout=1)
        logging.info(
            f"Requested auto-repair (re-download) for corrupted data: {target_id} -> {pixiv_url}"
        )
    except Exception as e:
        logging.error(f"Failed to request auto-repair for {target_id}: {e}")


def convert(folder_path, key_data, data_path, host_name):
    """ローカルデータの再変換"""

    # 1. 最優先: .corrupt ファイルの探索
    corrupt_found = []
    for root, dirs, files in os.walk(folder_path):
        for file in files:
            if file.endswith(".corrupt"):
                if file == "raw.json.corrupt":
                    # root = /pixiv/n12345678/raw
                    work_folder = os.path.dirname(root)  # /pixiv/n12345678
                    folder_name = os.path.basename(work_folder)  # n12345678
                    corrupt_path = os.path.join(root, file)
                    corrupt_found.append(folder_name)
                    logging.warning(
                        f"Convert: Found corrupted file in {folder_name}, "
                        "skipping and requesting re-download"
                    )
                    request_re_download(folder_name, host_name)

    # 2. 通常の変換処理
    folders = [
        f
        for f in os.listdir(folder_path)
        if os.path.isdir(os.path.join(folder_path, f))
    ]
    for folder in folders:
        # .corrupt が見つかったフォルダはスキップ
        if folder in corrupt_found:
            continue

        raw_path = os.path.join(folder_path, folder, "raw", "raw.json")
        if os.path.exists(raw_path):
            try:
                data = cm._load_json_safe(raw_path)
                if not data:
                    logging.warning(
                        f"Corrupted raw.json found in {folder}. Requesting re-download."
                    )
                    request_re_download(folder, host_name)
                    continue

                # タグ整形再適用
                if "tags" in data:
                    data["tags"] = format_tags(data["tags"])
                if "all_tags" in data:
                    data["all_tags"] = format_tags(data["all_tags"])

                cm._save_json(raw_path, data)

                cn.narou_gen(
                    data,
                    os.path.join(folder_path, folder),
                    key_data,
                    data_path,
                    host_name,
                    util.get_img_url(),
                )
            except Exception as e:
                logging.error(f"Convert failed for {folder}: {e}")
    cm.gen_site_index(folder_path, key_data, "Pixiv")


def repair(folder_path, key_data, data_path, host_name):
    """データ修復: 一次ファイルを削除し、全てのraw.jsonからデータを強制的に再構築する"""
    if not _crawler:
        logging.error("Crawler not initialized")
        return

    logging.info("=" * 60)
    logging.info("Data Repair: Starting full data repair...")
    logging.info("=" * 60)

    # 保護対象のフォルダ・ファイル名
    protected_dirs = {"raw", "images", "snapshots"}
    protected_files = {"user.json", "index.json", "index.html"}

    # システムフォルダ（作品フォルダではないもの）
    system_folders = {"images", "snapshots"}

    work_folders = [
        f
        for f in os.listdir(folder_path)
        if os.path.isdir(os.path.join(folder_path, f)) and f not in system_folders
    ]

    # 1. .corrupt ファイルの削除
    logging.info("Phase 1: Removing .corrupt files...")
    corrupt_count = 0
    for root, dirs, files in os.walk(folder_path):
        for file in files:
            if file.endswith(".corrupt"):
                corrupt_path = os.path.join(root, file)
                try:
                    os.remove(corrupt_path)
                    corrupt_count += 1
                    logging.info(f"Removed corrupt file: {corrupt_path}")
                except Exception as e:
                    logging.error(f"Failed to remove corrupt file {corrupt_path}: {e}")
    logging.info(f"Removed {corrupt_count} .corrupt files.")

    # 2. 各作品フォルダの一次ファイルを削除
    logging.info("")
    logging.info("Phase 2: Deleting intermediate files...")
    deleted_count = 0
    for folder in tqdm(work_folders, desc="Deleting intermediate files", unit="work"):
        work_path = os.path.join(folder_path, folder)
        raw_path = os.path.join(work_path, "raw", "raw.json")

        # raw.json が存在しないフォルダはスキップ（gen_site_indexで後で削除される）
        if not os.path.exists(raw_path):
            continue

        # 作品フォルダ内の一次ファイル（raw/ 以外）を削除
        for item in os.listdir(work_path):
            item_path = os.path.join(work_path, item)

            # raw/ フォルダは保護
            if item == "raw":
                continue

            try:
                if os.path.isdir(item_path):
                    # info/ や エピソードID フォルダを削除
                    shutil.rmtree(item_path)
                    deleted_count += 1
                elif os.path.isfile(item_path):
                    # index.html 等のファイルを削除
                    os.remove(item_path)
                    deleted_count += 1
            except Exception as e:
                logging.error(f"Failed to delete {item_path}: {e}")

    # サイトの index.json も削除（再生成されるため）
    site_index_json = os.path.join(folder_path, "index.json")
    if os.path.exists(site_index_json):
        try:
            os.remove(site_index_json)
            deleted_count += 1
            logging.info(f"Removed site index.json: {site_index_json}")
        except Exception as e:
            logging.error(f"Failed to remove site index.json: {e}")

    logging.info(f"Deleted {deleted_count} intermediate files/directories.")

    # 3. 全てのraw.jsonからデータを再構築
    logging.info("")
    logging.info("Phase 3: Rebuilding all data from raw.json...")
    rebuild_count = 0
    error_count = 0

    for folder in tqdm(work_folders, desc="Rebuilding from raw.json", unit="work"):
        raw_path = os.path.join(folder_path, folder, "raw", "raw.json")
        if not os.path.exists(raw_path):
            continue

        try:
            data = cm._load_json_safe(raw_path)
            if not data:
                logging.warning(f"Failed to load raw.json for {folder}, skipping")
                error_count += 1
                continue

            # タグ整形再適用
            if "tags" in data:
                data["tags"] = format_tags(data["tags"])
            if "all_tags" in data:
                data["all_tags"] = format_tags(data["all_tags"])

            cm._save_json(raw_path, data)

            # HTML再生成
            cn.narou_gen(
                data,
                os.path.join(folder_path, folder),
                key_data,
                data_path,
                host_name,
                util.get_img_url(),
            )
            rebuild_count += 1

        except Exception as e:
            logging.error(f"Rebuild failed for {folder}: {e}", exc_info=True)
            error_count += 1

    # 4. サイトインデックスの再生成
    logging.info("")
    logging.info("Phase 4: Regenerating site index...")
    cm.gen_site_index(folder_path, key_data, "Pixiv")

    # 総括
    logging.info("")
    logging.info("=" * 60)
    logging.info(f"Data Repair Summary:")
    logging.info(f"  - Corrupt files removed: {corrupt_count}")
    logging.info(f"  - Intermediate files deleted: {deleted_count}")
    logging.info(f"  - Works rebuilt: {rebuild_count}")
    logging.info(f"  - Errors: {error_count}")
    logging.info("=" * 60)
