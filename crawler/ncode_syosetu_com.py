import os
import fitz  # PyMuPDF
import json
import re
import tqdm
import logging
from datetime import datetime, timedelta, timezone
from typing import List, Dict, Any, Tuple, Optional, Set

# 共通処理モジュール (環境に合わせてパス解決してください)
import crawler.common as cm
import crawler.convert_narou as cn
from crawler.common import safe_fromiso

# --- 定数定義 ---

VERSION = 5
JST = timezone(timedelta(hours=9))

# 除外・判定用パラメータ
IGNORE_FONT_SIZE = 12.0
LINE_SPACE_THRESHOLD_DEFAULT = 22

# 正規表現パターン
_PATTERN_INTRO_TAGS = re.compile(r'【(.*?)】(.*?)(?=【|$)', re.DOTALL)
_PATTERN_BRACKETS = [
    re.compile(p) for p in [
        r'「([^「」]*)」', r'\{([^\{\}]*)\}', r'\(([^()]*)\)', r'（([^（）]*)）',
        r'\[([^\[\]]*)\]', r'【([^【】]*)】', r'『([^『』]*)』', r'≪([^≪≫]*)≫', r'〈([^〈〉]*)〉'
    ]
]
_OPENER_CHARS = {'「', '（', '『', '【', '(', '[', '{'}
_SPACE_CHARS = {'　', ' ', '[', '{', '(', '（', '「', '『', '【'}


# --- PDF解析・テキスト抽出ロジック ---

def extract_text_with_details(pdf_path: str) -> Tuple[List[Dict], List[Dict]]:
    """
    PDFファイルから文字ごとの詳細情報(座標、サイズ等)を抽出する。
    サイズ12.0の文字は除外する。
    """
    doc = fitz.open(pdf_path)
    text_details = []
    last_details = []
    total_pages = len(doc)

    # ページごとの処理関数
    def _process_page(page_obj, page_number, target_list):
        blocks = page_obj.get_text("dict")["blocks"]
        for block in blocks:
            if block['type'] != 0:  # テキストブロック以外はスキップ
                continue
            for line in block["lines"]:
                for span in line["spans"]:
                    if span['size'] == IGNORE_FONT_SIZE:
                        continue
                    
                    is_bold = 'bold' in span['font'].lower()
                    
                    for char in span['text']:
                        target_list.append({
                            'text': char,
                            'x': span['bbox'][0],
                            'y': span['bbox'][1],
                            'size': span['size'],
                            'page': page_number,
                            'emphasized': is_bold
                        })

    # 1ページ目〜最後から2番目のページ
    # (元コードの range(1, len(doc)-1) は 0始まりのfitzでは "2ページ目〜最後から2番目" を意味するが、
    #  tqdmの表示と実際のpage属性(+1している)を考慮し、ロジックを維持)
    
    # 元コードのロジック: range(1, len(doc)-1) -> 1(2ページ目) から len-2(ラス前) まで
    for i in tqdm.tqdm(range(1, total_pages - 1), leave=False, desc="Loading", unit="characters"):
        _process_page(doc.load_page(i), i + 1, text_details)

    # 最終ページ
    if total_pages > 0:
        _process_page(doc.load_page(total_pages - 1), total_pages, last_details)

    return text_details, last_details


def extract_second_highest_group_line(last_details: List[Dict]) -> Optional[str]:
    """
    最終ページの右半分から、ページ下部にある日付相当の文字列を抽出する。
    """
    if not last_details:
        return None

    # 1. 最も下の行（y座標最大）を見つけ、その行の左端Xを特定
    sorted_by_y = sorted(last_details, key=lambda d: d['y'], reverse=True)
    highest_y = sorted_by_y[0]['y']
    highest_group = [d for d in last_details if d['y'] == highest_y]
    leftmost_x = min(d['x'] for d in highest_group)

    # 2. 右半分（特定したXより右）にある要素を抽出
    right_half = [d for d in last_details if d['x'] > leftmost_x]
    if not right_half:
        return None

    # 3. Y座標ごとにグルーピング
    grouped_by_y = {}
    for d in right_half:
        grouped_by_y.setdefault(d['y'], []).append(d)

    # Y座標が大きい順（下から順）にソートされたキーリスト
    sorted_y_keys = sorted(grouped_by_y.keys(), reverse=True)

    # 下から2番目の行を取得
    if len(sorted_y_keys) > 1:
        second_highest_y = sorted_y_keys[0] # 元コードのロジックでは sorted_y_keys[0] を取得して除外せずそのまま使っている箇所があるが、
                                            # 関数名と文脈的に「最も下の日付発行日」などを取りたい意図と推測される。
                                            # 元コード: second_highest_y = sorted_y_keys[0] (一番下の行) を取得しているように見える
        
        target_group = grouped_by_y[second_highest_y]
        # X座標順に並べて文字列結合
        target_group_sorted = sorted(target_group, key=lambda d: d['x'])
        return ''.join(d['text'] for d in target_group_sorted)
    
    return None


def sort_text_details(text_details: List[Dict]) -> List[Dict]:
    """ページ昇順 -> X降順(右から左) -> Y昇順(上から下) でソート"""
    # 元コード: (item['page'], -item['x'], item['y'])
    # 縦書き文章の読み順（ページ毎、右の行から、上から下へ）
    return sorted(
        text_details,
        key=lambda item: (item['page'], -item['x'], item['y'])
    )


def _identify_structure(data: List[Dict], max_x_page_2: float, min_y_coordinate: float) -> Tuple[Dict, Dict, Dict]:
    """
    ページごとの特定位置(max_x_page_2)にある強調文字を解析し、
    タイトル、前書き、後書きのページ構造を特定する。
    """
    pages = sorted(list({item['page'] for item in data}))
    titles = {}
    introductions = {}
    postscripts = {}
    title_count = 1

    for page in tqdm.tqdm(pages, desc="Structure", unit="pages", leave=False):
        page_items = [item for item in data if item['page'] == page]
        
        # 指定列（max_x）かつ強調（太字）の文字を抽出
        target_chars = [
            item for item in page_items 
            if item['x'] == max_x_page_2 and item.get('emphasized')
        ]

        # 座標チェック: ページ最上部から始まっている場合は本文の一部とみなしスキップ
        if any(item['y'] == min_y_coordinate for item in target_chars):
            continue

        title_text = ''.join(item['text'] for item in target_chars)
        
        # 先頭の空白削除
        if re.match(r'^[ 　][^ 　]', title_text):
            title_text = title_text[1:]

        if not title_text:
            continue

        if title_text.endswith('（前書き）'):
            introductions[title_count] = {'title': title_text, 'page': page}
        elif title_text.endswith('（後書き）'):
            # 前のエピソードに対応する後書き
            postscripts[title_count - 1] = {'title': title_text, 'page': page}
        else:
            titles[title_count] = {'title': title_text, 'page': page}
            title_count += 1

    return titles, introductions, postscripts


def _clean_text_recursive(text: str) -> str:
    """括弧内の改行や空白を再帰的に削除する"""
    text = text.lstrip('\n')
    
    while True:
        original_text = text
        for pattern in _PATTERN_BRACKETS:
            # マッチした箇所の内部の改行/空白を除去して再構築
            def _replacer(match):
                content = match.group(1)
                content = content.lstrip('\n')
                content = content.replace('\n　', '').replace('\n', '')
                return match.group(0)[0] + content + match.group(0)[-1]
            
            text = pattern.sub(_replacer, text)
        
        if text == original_text:
            break
    return text


def _reconstruct_section_text(data: List[Dict], start_page: int, end_page: int, 
                              max_x_ignore: float) -> str:
    """
    指定されたページ範囲のテキストデータを、縦書きレイアウトの座標に基づいて文字列に復元する。
    max_x_ignore: タイトル行として扱われるX座標（本文から除外）
    """
    # 対象データの抽出とフィルタリング
    target_items = [
        item for item in data 
        if start_page <= item['page'] < end_page
        and (item['x'] != max_x_ignore or not item.get('emphasized'))
    ]
    
    if not target_items:
        return ""

    # 行間の推定
    sizes = sorted({item['size'] for item in target_items if 'size' in item}, reverse=True)
    main_font_size = sizes[0] if sizes else 0
    line_space = (sizes[0] + sizes[1] + 1) if len(sizes) >= 2 else LINE_SPACE_THRESHOLD_DEFAULT

    # メインサイズのみ抽出して座標計算の基準にする
    main_text_items = [item for item in target_items if item.get('size') == main_font_size]
    if not main_text_items:
        # メインサイズが無い場合（稀）は全体を使う
        main_text_items = target_items

    txt_x_max = max(main_text_items, key=lambda x: x['x'])['x']
    txt_y_min = min(main_text_items, key=lambda x: x['y'])['y']
    
    unique_ys = sorted({item['y'] for item in main_text_items}, reverse=True)
    txt_y_max_2 = unique_ys[1] if len(unique_ys) > 1 else None

    # テキスト構築ループ
    result_chars = []
    previous_item = None
    
    # target_items は既に sort_text_details でソート済みである前提
    for item in target_items:
        char = item['text']
        current_page = item['page']
        current_x = item['x']
        current_y = item['y']

        # 初回
        if previous_item is None:
            if current_y == txt_y_min and char in _OPENER_CHARS:
                result_chars.append('\n')
            result_chars.append(char)
            previous_item = item
            continue

        prev_page = previous_item['page']
        prev_x = previous_item['x']
        prev_y = previous_item['y']

        # 改ページ判定
        if current_page != prev_page:
            # 改行判定ロジック（元コードの移植）
            if current_x != prev_x:
                if prev_y == txt_y_max_2:
                    pass # 連続
                elif (any(c in char for c in _SPACE_CHARS) and current_y == txt_y_min):
                     if char == ' ': result_chars.append('　')
                     result_chars.append('\n')
                elif current_y == txt_y_min:
                    result_chars.append('\n')
            else:
                # 同じ行位置だがページが変わった場合 -> 空行挿入計算
                empty_lines = int(abs(prev_x - current_x) // line_space)
                result_chars.append('\n' * empty_lines)
        
        # 同一ページ内
        else:
            # 改行（列移動）判定
            if current_x != txt_x_max: # 基準となる右端列ではない
                 if prev_y == txt_y_max_2:
                     pass
                 elif (any(c in char for c in _SPACE_CHARS) and current_y == txt_y_min):
                     if char == ' ': result_chars.append('　')
                     result_chars.append('\n')
                 elif current_y == txt_y_min:
                     result_chars.append('\n')
            else:
                # 空行判定
                 empty_lines = int(abs(txt_x_max - current_x) // line_space)
                 result_chars.append('\n' * empty_lines)
        
        result_chars.append(char)
        previous_item = item

    raw_text = "".join(result_chars)
    return _clean_text_recursive(raw_text)


def _parse_introduction_metadata(full_intro_text: str) -> Tuple[str, str, str, str]:
    """最初のセクション（前書きなど）からメタ情報を抽出"""
    tags = {'【小説タイトル】': '', '【Ｎコード】': '', '【作者名】': '', '【あらすじ】': ''}
    
    # タグ間のテキストを抽出
    # 正規表現で一括処理も可能だが、順序依存が強いためfind処理で切り出し
    current_pos = 0
    sorted_tags = sorted(tags.keys(), key=lambda k: full_intro_text.find(k))
    
    extracted = {}
    for i, tag in enumerate(sorted_tags):
        start = full_intro_text.find(tag)
        if start == -1:
            continue
        
        content_start = start + len(tag)
        # 次のタグを探す
        next_tag_start = len(full_intro_text)
        for next_tag in sorted_tags[i+1:]:
            pos = full_intro_text.find(next_tag, content_start)
            if pos != -1:
                next_tag_start = pos
                break
        
        content = full_intro_text[content_start:next_tag_start].strip('\n　')
        extracted[tag] = content

    novel_title = extracted.get('【小説タイトル】', '')
    n_code = extracted.get('【Ｎコード】', '')
    author = extracted.get('【作者名】', '')
    # あらすじは最後のタグ以降すべて
    intro_match = re.search(r'【あらすじ】(.*)', full_intro_text, re.DOTALL)
    n_introduction = intro_match.group(1).strip('\n　') if intro_match else ""

    return novel_title, n_code, author, n_introduction


def process_text_details(file_data: List[Dict], last_details: List[Dict], 
                         author_id: str, author_url: str, novel_type: int, chapter: Optional[str]) -> Tuple[Dict, str]:
    """
    抽出されたJSONデータを解析し、なろう形式の構造化データを作成する
    """
    # 1. ページ2の座標基準を取得 (タイトル位置判定用)
    page_2_data = [item for item in file_data if item['page'] == 2]
    if not page_2_data:
        raise ValueError("Page 2 data not found.")
    
    max_x_page_2 = max(item['x'] for item in page_2_data)
    min_y_coordinate = min(item['y'] for item in file_data)

    # 2. 構造解析 (タイトル、前書き、後書きのページ特定)
    titles, introductions, postscripts = _identify_structure(file_data, max_x_page_2, min_y_coordinate)

    # 3. 各セクションのページ範囲を決定
    # 処理順: introduction -> title -> postscript の順でページが並んでいると仮定
    # キー(連番)順に処理
    ordered_sections = [] # (key, type, page)
    for key in sorted(titles.keys()):
        if key in introductions:
            ordered_sections.append({'key': key, 'type': 'intro', 'page': introductions[key]['page']})
        ordered_sections.append({'key': key, 'type': 'main', 'page': titles[key]['page']})
        if key in postscripts:
            ordered_sections.append({'key': key, 'type': 'post', 'page': postscripts[key]['page']})

    # ページ区切りリスト作成 (次のセクションの開始ページが、現在のセクションの終了ページ+1)
    # 最後のセクションのためにダミーの終了ページを追加
    max_page = max(item['page'] for item in file_data)
    boundaries = [s['page'] for s in ordered_sections] + [max_page + 1]

    # 4. テキスト復元と結合
    full_texts = []
    # 最初のメタデータセクション用 (titles[1]より前にある場合)
    # 実装上、loop 0番目がメタデータを含んでいる前提
    
    for i in tqdm.tqdm(range(len(ordered_sections)), desc="Reconstructing", unit="sec", leave=False):
        start_p = boundaries[i]
        end_p = boundaries[i+1]
        
        # テキスト復元
        text_segment = _reconstruct_section_text(file_data, start_p, end_p, max_x_page_2)
        
        # 重複部分の削除 (前のセクションのテキストが混ざるのを防ぐ簡易ロジック)
        # 元コードの remove_unwanted_spaces_recursive(previous_txt).replace(full_texts[n], '') の再現
        if i > 0:
             # 既に処理済みのテキストが含まれていれば削除（PDF解析特有の重複読み込み対策）
             for prev_text in full_texts:
                 text_segment = text_segment.replace(prev_text, '')
        
        full_texts.append(text_segment)

    # 5. メタデータ抽出
    novel_title, n_code, author, introduction_text = _parse_introduction_metadata(full_texts[0])
    ncode = cm.full_to_half(n_code).lower()

    # 発行日の取得
    raw_date_str = extract_second_highest_group_line(last_details)
    try:
        if raw_date_str:
            # "YYYY年MM月DD日HH時MM分発行" 形式を想定
            dt = datetime.strptime(raw_date_str, "%Y年%m月%d日%H時%M分発行")
            gen_date = str(dt.replace(tzinfo=JST))
        else:
            gen_date = str(datetime.now(JST))
    except ValueError:
        logging.warning(f"Date parsing failed for: {raw_date_str}")
        gen_date = str(datetime.now(JST))

    update_date = str(datetime.now(JST))

    # 6. 結果辞書の構築
    results = {
        'version': VERSION,
        'get_date': update_date,
        'title': novel_title,
        'id': ncode,
        'nid': ncode,
        'url': f"https://ncode.syosetu.com/{ncode}",
        'author': author,
        'author_id': author_id,
        'author_url': author_url,
        'caption': introduction_text,
        'type': 'novel',
        'serialization': {0: "連載中", 1: "完結済", 2: "短編"}.get(novel_type, "連載中"),
        'createDate': gen_date,
        'updateDate': update_date,
        'tags': [], 'all_tags': [], 
        'episodes': {}
    }

    # チャプター情報のパース
    chapter_map = {}
    if chapter:
        for part in chapter.split(","):
            try:
                rng, val = part.split(":")
                s, e = map(int, rng.split("-"))
                for k in range(s, e + 1):
                    chapter_map[k] = val
            except ValueError:
                continue

    # エピソードデータの格納
    total_chars = 0
    text_idx = 1 # 0番目はメタデータなので1から
    
    # ordered_sections と full_texts は同期しているが、titlesのキー単位でまとめる必要がある
    # titles のキー順に処理
    max_ep_id = 0
    
    for key in sorted(titles.keys()):
        ep_id = key - 1
        if key == 1: continue # 1話目はメタデータ部分に含まれることが多いためスキップ(元コード準拠)

        ep_data = {
            'id': ep_id,
            'chapter': chapter_map.get(ep_id, ""),
            'title': titles[key]['title'],
            'introduction': "",
            'postscript': "",
            'createDate': gen_date,
            'updateDate': update_date
        }

        # 対応するテキストを取得
        # keyに対応する ordered_sections のインデックスを探す
        
        # 前書き
        if key in introductions:
            ep_data['introduction'] = full_texts[text_idx]
            text_idx += 1
        
        # 本文
        body_text = cm.indent_paragraphs(full_texts[text_idx])
        ep_data['text'] = body_text
        char_count = len(body_text.replace('\n', ''))
        ep_data['textCount'] = char_count
        total_chars += char_count
        text_idx += 1
        
        # 後書き
        if key in postscripts:
            ep_data['postscript'] = full_texts[text_idx]
            text_idx += 1

        results['episodes'][ep_id] = ep_data
        max_ep_id = ep_id

    results['total_episodes'] = max_ep_id
    results['all_episodes'] = max_ep_id
    results['total_characters'] = total_chars
    results['all_characters'] = total_chars

    # 中間ファイルの保存 (デバッグ用)
    with open('results.json', 'w', encoding='utf-8') as file:
        json.dump(results, file, ensure_ascii=False, indent=4)

    return results, ncode


# --- メインエントリーポイント ---

def gen_from_pdf(pdf_path: str, pdf_name: str, author_id: str, author_url: str, 
                 novel_type: int, chapter: str, folder_path: str, key_data: str, 
                 data_path: str, host_name: str):
    
    pdf_file = os.path.join(pdf_path, pdf_name)
    
    # 1. PDFからテキスト抽出
    text_details, last_details = extract_text_with_details(pdf_file)
    sorted_details = sort_text_details(text_details)

    # 2. 構造化データへの変換
    results, ncode = process_text_details(sorted_details, last_details, author_id, author_url, novel_type, chapter)

    # 3. フォルダ・ファイル生成
    cm.make_dir(ncode, folder_path)
    base_dir = os.path.join(folder_path, ncode)
    raw_path = os.path.join(base_dir, 'raw', 'raw.json')

    # エピソードフォルダ作成
    if novel_type != 2: # 短編以外
        for ep_id in results['episodes']:
            # ep_id は 0 始まりのインデックスだが、フォルダ名は 1, 2... と連番にする場合
            # 元コード: str(i) (i=1 start)
            # results['episodes'] のキーは _key (1話目スキップ後の連番)
            # ここは元コードの挙動に合わせるため、ep_id そのままではなく連番で作成
            pass 
            # NOTE: 元コードでは i=1 からループで作成しているが、ep_id とフォルダ名が一致しているか確認が必要。
            # ひとまずエピソードIDに対応するフォルダを作成
            os.makedirs(os.path.join(base_dir, str(ep_id)), exist_ok=True)

    # 差分保存とJSON保存
    cm.save_raw_diff(raw_path, base_dir, results)
    with open(raw_path, 'w', encoding='utf-8') as file:
        json.dump(results, file, ensure_ascii=False, indent=4)

    # 4. HTML生成
    cn.narou_gen(results, base_dir, key_data, data_path, host_name)
    cm.gen_site_index(folder_path, key_data, '小説家になろう')

    # PDF削除
    if os.path.exists(pdf_file):
        os.remove(pdf_file)
    
    logging.info(f"Generated {ncode} from PDF file")


def convert(folder_path: str, key_data: str, data_path: str, host_name: str):
    """既存のJSONからHTMLを再生成"""
    if not os.path.exists(folder_path):
        return

    folder_names = [name for name in os.listdir(folder_path) if os.path.isdir(os.path.join(folder_path, name))]

    for name in folder_names:
        target_dir = os.path.join(folder_path, name)
        raw_json_path = os.path.join(target_dir, 'raw', 'raw.json')
        info_html_path = os.path.join(target_dir, 'info', 'index.html')

        if os.path.exists(raw_json_path) and os.path.exists(info_html_path):
            with open(raw_json_path, 'r', encoding='utf-8') as f:
                raw_json_data = json.load(f)
            cn.narou_gen(raw_json_data, target_dir, key_data, data_path, host_name)

    cm.gen_site_index(folder_path, key_data, '小説家になろう')


def init(cookie_path, data_path, is_login, interval):
    pass