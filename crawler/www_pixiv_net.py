import re
import os
import requests
import json
from playwright.sync_api import Playwright, sync_playwright, expect
#リキャプチャ対策
import time
import random

def load_cookies_from_json(input_file):
    with open(input_file, 'r', encoding='utf-8') as f:
        cookies = json.load(f)
    
    # requests用にCookieを変換
    cookies_dict = {cookie['name']: cookie['value'] for cookie in cookies}
    return cookies_dict

def init(folder_path):

    #クッキーとユーザーエージェントはグローバルで宣言
    global cookie_path
    global ua_path

    cookie_path = os.path.join(folder_path, 'cookie.json')
    ua_path = os.path.join(folder_path, 'ua.txt')

    def login(playwright: Playwright) -> None:
        browser = playwright.chromium.launch(headless=True)
        context = browser.new_context()
        page = context.new_page()
        page.goto("https://www.pixiv.net/")
        time.sleep(random.randint(1,5))
        page.get_by_role("link", name="ログイン").click()
        mail = input("メールアドレスを入力してください: ")
        page.get_by_placeholder("メールアドレスまたはpixiv ID").fill(mail)
        pswd = input("パスワードを入力してください: ")
        page.get_by_placeholder("パスワード").fill(pswd)
        time.sleep(random.randint(1,5))
        page.get_by_role("button", name="ログイン", exact=True).click()
        # リダイレクトが完全に終わるまで待つ
        def wait_for_redirects(page):
            while True:
                page.wait_for_load_state("load")
                if page.url.startswith("https://accounts.pixiv.net/login/two-factor-authentication?") or page.url == "https://www.pixiv.net/":
                    break
        wait_for_redirects(page)
        print(page.url)
        if "two-factor-authentication" in page.url:
            time.sleep(random.randint(1,5))
            page.get_by_label("このブラウザを信頼する").check()
            tfak = input("2段階認証のコードを入力してください: ")
            page.get_by_placeholder("確認コード").fill(tfak)
            time.sleep(random.randint(1,5))
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


def download(url, folder_path):
    cookie = load_cookies_from_json(cookie_path)
    ua = open(ua_path, 'r', encoding='utf-8').read()
    header = {
        'User-Agent': ua
    }

    response = requests.get(url, cookies=cookie, headers=header)

    if response.status_code == 404:
        print("404 Not Found")
        print("Incorrect URL, Deleted, Private, or My Pics Only.")
        return