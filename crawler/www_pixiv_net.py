import re
from playwright.sync_api import Playwright, sync_playwright, expect

def run(url, folder_path):

    def login(playwright: Playwright) -> None:
        browser = playwright.chromium.launch(headless=False)
        context = browser.new_context()
        page = context.new_page()
        page.goto("https://www.pixiv.net/")
        mail=input("メールアドレスを入力してください")
        pswd=input("パスワードを入力してください")
        page.get_by_role("link", name="ログイン").click()
        page.get_by_placeholder("メールアドレスまたはpixiv ID").fill(mail)
        page.get_by_placeholder("パスワード").fill(pswd)
        page.get_by_role("button", name="ログイン", exact=True).click()
        page.wait_for_load_state("load")
        if "two-factor-authentication" in page.url:
            tfak=input("2段階認証のコードを入力してください")
            page.get_by_placeholder("確認コード").fill(tfak)
            page.get_by_label("このブラウザを信頼する").check()
            page.get_by_role("button", name="ログイン").click()
        page.close()

        # ---------------------
        context.close()
        browser.close()


    with sync_playwright() as playwright:
        login(playwright)
