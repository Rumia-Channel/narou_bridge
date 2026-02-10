import os
import re
import logging
from datetime import datetime
from typing import Dict, Any, Optional

# 外部モジュールの想定 (環境に合わせて適宜調整してください)
from crawler.common import safe_fromiso, _load_template

# --- 定数・テンプレート定義 ---

# 正規表現パターン
_PATTERN_IMG_TAG = re.compile(r"\[image\]\((.*?)\)")
_PATTERN_NEWPAGE_TAG = re.compile(r"\[newpage\]")
_PATTERN_RUBY_TAG = re.compile(r"\[ruby:<(.*?)>\((.*?)\)\]")
_PATTERN_HTML_IMG = re.compile(r"(.*?)(<img[^>]*>)(.*)")
_PATTERN_JUMP_TAG = re.compile(r"\[jump:(\d+)\]")

# ページカウント用パターン (narou_genで使用)
_PATTERN_PAGE_COUNT = re.compile(
    r"\[newpage\]\s*|\s*\[image\]\((.*?)\)|"
    r"\[image\]\s*\((.*?)\)\s*(?:\\n|\n)+\s*\[newpage\]|\s*"
    r"\[newpage\]\s*(?:\\n|\n)+\s*\[image\]\s*\((.*?)\)|"
    r"\[newpage\]\s*\[image\]\s*\((.*?)\)|"
    r"\[image\]\s*\((.*?)\)\s*\[newpage\]|"
    r"(?:\s*(?:\\n|\n)*\s*\[image\]\((.*?)\)\s*)+"
)


# --- ユーティリティ関数 ---


def _format_date_jp(
    iso_date_str: str, format_str: str = "%Y/%m/%d %H:%M", default_msg: str = ""
) -> str:
    """日付文字列を整形して返す"""
    dt = safe_fromiso(iso_date_str)
    if dt:
        return dt.strftime(format_str)
    return default_msg


def _save_html(path: str, title: str, body_content: str, extra_head: str = ""):
    """HTMLファイルを保存する共通関数"""
    # 外部テンプレートを使用
    template = _load_template("novel_page")
    html_content = template.format(
        title=title, extra_head=extra_head, body=body_content
    )
    # ディレクトリ作成
    os.makedirs(os.path.dirname(path), exist_ok=True)

    with open(path, "w", encoding="utf-8") as f:
        f.write(html_content)


# --- テキスト整形処理 ---


def replace_images(text: str, img_link_base: str, key_data: str) -> str:
    """[image](url) を <img> タグに置換"""
    return _PATTERN_IMG_TAG.sub(
        lambda match: (
            f'<img src="{img_link_base}{match.group(1)}{key_data}" alt="{match.group(1)}">'
        ),
        text,
    )


def replace_newpage(text: str) -> str:
    """[newpage] を [#改ページ] に置換"""
    return _PATTERN_NEWPAGE_TAG.sub("[#改ページ]", text)


def replace_ruby(text: str) -> str:
    """[ruby:文字(ルビ)] をHTMLルビタグに置換"""
    return _PATTERN_RUBY_TAG.sub(
        lambda match: (
            f"<ruby>{match.group(1)}<rp>(</rp><rt>{match.group(2)}</rt><rp>)</rp></ruby>"
        ),
        text,
    )


def format_text(text: str, id_prefix: str, key_data: str, img_link_base: str) -> str:
    """
    テキスト全体をHTML段落形式に整形する
    """
    if not text:
        return ""

    text = replace_images(text, img_link_base, key_data)
    text = replace_newpage(text)
    text = replace_ruby(text)

    lines = text.split("\n")
    formatted_paragraphs = []
    id_counter = 1

    for line in lines:
        line = line.rstrip()

        # 空行
        if line == "":
            formatted_paragraphs.append(f'<p id="{id_prefix}{id_counter}"><br></p>')
            id_counter += 1
            continue

        # <img>タグを含む行の特別処理
        match = _PATTERN_HTML_IMG.search(line)
        if match:
            before_img = match.group(1).strip()
            img_tag = match.group(2).strip()
            after_img = match.group(3).strip()

            if before_img:
                formatted_paragraphs.append(
                    f'<p id="{id_prefix}{id_counter}">{before_img}</p>'
                )
                id_counter += 1

            formatted_paragraphs.append(
                f'<p id="{id_prefix}{id_counter}">{img_tag}</p>'
            )
            id_counter += 1

            if after_img:
                formatted_paragraphs.append(
                    f'<p id="{id_prefix}{id_counter}">{after_img}</p>'
                )
                id_counter += 1
        else:
            # 通常のテキスト行
            formatted_paragraphs.append(f'<p id="{id_prefix}{id_counter}">{line}</p>')
            id_counter += 1

    return "\n".join(formatted_paragraphs)


# --- HTMLコンテンツ生成パーツ ---


def _generate_index_content(data: Dict, key_data: str, a_link_base: str) -> str:
    """目次(index_box)の中身を生成"""
    lines = []
    lines.append('<div class="p-eplist">')

    chapter = None
    # エピソード順序は元の辞書の順序に依存
    for ep in data["episodes"].values():
        episode_id = ep["id"]
        episode_title = ep["title"]

        create_date = _format_date_jp(ep.get("createDate"), default_msg="作成日不明")
        update_date = _format_date_jp(ep.get("updateDate"), default_msg="更新日不明")

        # 章が変わった場合のヘッダー
        if ep.get("chapter") != chapter:
            chapter = ep.get("chapter")
            if chapter:
                lines.append(f'<div class="p-eplist__chapter-title">{chapter}</div>')

        lines.append('<div class="p-eplist__sublist">')
        lines.append(
            f'<a href="{a_link_base}/{episode_id}/{key_data}" class="p-eplist__subtitle">\n{episode_title}\n</a>'
        )
        lines.append('<div class="p-eplist__update">')
        lines.append(
            f'{create_date}\n<span title="{update_date} 改稿">（<u>改</u>）</span>'
        )
        lines.append("</div>")  # p-eplist__update
        lines.append("</div>")  # p-eplist__sublist

    lines.append("</div>")  # p-eplist
    return "\n".join(lines)


def _generate_info_content(data: Dict, key_data: str, site_name: str, nid: str) -> str:
    """作品情報ページの中身を生成"""

    # 共通ヘッダーリンク
    links = [
        f'<a href="../{key_data}" class="header-link">戻る</a>',
        f'<a href="/reader/?site={site_name}&nid={nid}" class="header-link">簡易リーダーで読む</a>',
    ]

    # 作品タイトル・種別
    content_parts = list(links)
    content_parts.append(
        f'<h1><a href="{data.get("url")}" target="_blank">{data.get("title")}</a></h1>'
    )

    serialization = data.get("serialization")
    if serialization == "短編":
        content_parts.append(f'<div><span id="noveltype">短編</span></div>')
    else:
        content_parts.append(
            f'<div><span id="noveltype">{serialization}</span> 全{data.get("total_episodes")}エピソード</div>'
        )

    # テーブル情報
    all_tags_str = " ".join(data.get("all_tags", [])).strip()
    if not all_tags_str:
        all_tags_str = "\n<span>キーワードが設定されていません</span>\n"

    formatted_create = _format_date_jp(data.get("createDate"), "%Y年 %m月%d日 %H時%M分")
    formatted_update = _format_date_jp(data.get("updateDate"), "%Y年 %m月%d日 %H時%M分")

    date_label = (
        "最終更新日"
        if serialization == "短編"
        else ("最新掲載日" if serialization == "連載中" else "最終掲載日")
    )

    table_html = f"""
    <table>
    <tr><th class="ex">あらすじ</th><td class="ex">{data.get("caption")}</td></tr>
    <tr><th>作者名</th><td><a href="{data.get("author_url")}" target="_blank">{data.get("author")}</a></td></tr>
    <tr><th>キーワード</th><td>{all_tags_str}</td></tr>
    <tr><th>掲載日</th><td>{formatted_create}</td></tr>
    <tr><th>{date_label}</th><td>{formatted_update}</td></tr>
    <tr><th>文字数</th><td>{int(data.get("total_characters", 0)):,}文字</td></tr>
    </table>
    """
    content_parts.append(table_html)
    return "\n".join(content_parts)


# --- メイン処理 ---


def narou_gen(
    data: Dict,
    nove_path: str,
    key_data: str,
    data_folder: str,
    host_name: str,
    img_url: str = "",
):
    """
    小説データを基に、目次、作品情報、各話ページを生成する

    Args:
        img_url: 画像のベースURL。設定されている場合はこちらを優先。空の場合は host_name/images/ を使用
    """

    # 1. パスとリンクの計算
    try:
        rel_path = os.path.relpath(nove_path, data_folder)
        # Windowsパス対策
        rel_path_web = rel_path.replace("\\", "/")
        site_name, nid = rel_path.split(os.sep)[:2]
    except ValueError:
        # data_folder が nove_path の親でない場合などの安全策
        logging.error(f"Path Error: {nove_path} is not under {data_folder}")
        return

    # 画像リンクベースの決定
    if img_url:
        # img_url が設定されている場合はそれを使用（末尾にスラッシュがなければ追加）
        img_link_base = img_url if img_url.endswith("/") else img_url + "/"
    else:
        # 従来通り host_name/images/ を使用
        img_link_base = f"{host_name}/images/"
    a_link_base = f"/{rel_path_web}"

    # 2. テキスト内のページ分割と [jump] リンクの解決 (ep['text']を更新)
    page_counter = 2

    for ep in data["episodes"].values():
        digits = max(4, len(str(page_counter)))
        v_page = [page_counter]

        # ページ数カウントロジック
        for match in _PATTERN_PAGE_COUNT.finditer(ep["text"]):
            matched_str = match.group(0)

            is_newpage = "[newpage]" in matched_str
            is_image = "[image]" in matched_str

            # ロジックの再現
            if matched_str.strip() == "[newpage]":
                page_counter += 1
                v_page.append(page_counter)

            elif matched_str.startswith("[image]"):
                page_counter += 2

            elif match.group(1):  # [image]...[newpage] pattern inside complex regex?
                # NOTE: 元の正規表現のgroupインデックスに依存。
                # group(1)は [image] \n [newpage] の最初のimageの中身を指す想定
                page_counter += 2
                if is_newpage:
                    v_page.append(page_counter)

            elif match.group(3):  # [newpage]...[image]
                page_counter += 1
                if is_newpage:
                    v_page.append(page_counter)
                page_counter += 1  # 画像分

            elif match.group(4):  # 連続image
                images_count = len(match.group(4).strip().split())
                page_counter += images_count + 1

        # [jump:x] の置換処理
        def jump_replacer(match):
            jump_value = int(match.group(1))
            if 0 <= jump_value - 1 < len(v_page):
                # リンク先ページ番号
                target_page = str(v_page[jump_value - 1]).zfill(digits)
                return f"[#link_s]{target_page}.xhtml[#link_t]{jump_value}ページ目へ移動[#link_e]"
            return match.group(0)  # 範囲外なら置換しない

        ep["text"] = _PATTERN_JUMP_TAG.sub(jump_replacer, ep["text"])
        page_counter += 1

    # 3. index.html (目次) の生成
    if data.get("serialization") != "短編":
        index_body_parts = [
            f'<a href="../{key_data}" class="header-link">戻る</a>',
            f'<a href="./info/{key_data}" class="header-link">作品情報</a>',
            f'<a href="/reader/?site={site_name}&nid={nid}" class="header-link">簡易リーダーで読む</a>',
            f'<p class="novel_title">{data.get("title")}</p>',
            '<div class="index_box">',
            _generate_index_content(data, key_data, a_link_base),
            "</div>",
        ]
        _save_html(
            os.path.join(nove_path, "index.html"),
            title="Index Pixiv",
            body_content="\n".join(index_body_parts),
        )

    # 4. info/index.html (作品情報) の生成
    info_body = _generate_info_content(data, key_data, site_name, nid)
    _save_html(
        os.path.join(nove_path, "info", "index.html"),
        title="Index Pixiv",
        body_content=info_body,
    )

    # 5. 各エピソードページの生成
    is_short_story = data.get("serialization") == "短編"

    for ep in data["episodes"].values():
        if is_short_story:
            ep_path = os.path.join(nove_path, "index.html")
            # 短編用のナビゲーション
            nav_links = [
                f'<a href="../{key_data}" class="header-link">戻る</a>',
                f'<a href="./info/{key_data}" class="header-link">作品情報</a>',
                f'<a href="/reader/?site={site_name}&nid={nid}" class="header-link">簡易リーダーで読む</a>',
                f'<p class="novel_title">{data.get("title")}</p>',
            ]
        else:
            ep_path = os.path.join(nove_path, f"{ep['id']}", "index.html")
            # 連載用のナビゲーション
            nav_links = [
                f'<a href="../{key_data}" class="header-link">戻る</a>',
                f'<a href="../info/{key_data}" class="header-link">作品情報</a>',
                f'<a href="/reader/?site={site_name}&nid={nid}&eid={ep["id"]}" class="header-link">簡易リーダーで読む</a>',
                f'<p class="novel_subtitle">{ep["title"]}</p>',
            ]

        # 本文組み立て
        ep_body_parts = nav_links

        # 前書き
        if ep.get("introduction"):
            ep_body_parts.append(
                '<div class="js-novel-text p-novel__text p-novel__text--preface">'
            )
            ep_body_parts.append(
                format_text(ep["introduction"], "Lp", key_data, img_link_base)
            )
            ep_body_parts.append("</div>")

        # 本文
        ep_body_parts.append('<div class="js-novel-text p-novel__text">')
        ep_body_parts.append(
            format_text(ep.get("text", ""), "L", key_data, img_link_base)
        )
        ep_body_parts.append("</div>")

        # 後書き
        if ep.get("postscript"):
            ep_body_parts.append(
                '<div class="js-novel-text p-novel__text p-novel__text--afterword">'
            )
            ep_body_parts.append(
                format_text(ep["postscript"], "La", key_data, img_link_base)
            )
            ep_body_parts.append("</div>")

        ep_body_parts.append('<script src="/script/image.js"></script>')

        # 画像表示用の追加CSS
        extra_css = """<style>
            img { display: block; }
        </style>"""

        _save_html(
            ep_path,
            title=ep["title"],
            body_content="\n".join(ep_body_parts),
            extra_head=extra_css,
        )

    logging.info(f"{data.get('title')}の変換が完了しました。")
