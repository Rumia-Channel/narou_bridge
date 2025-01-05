import os
import fitz  # PyMuPDF
import json
import re
import tqdm
from datetime import datetime, timedelta, timezone

#ログを保存
import logging

#共通の処理
import crawler.common as cm
import crawler.convert_narou as cn

def extract_second_highest_group_line(last_details):
    """
    最終ページの右半分から、ページ内で最もy座標が大きい文字群の次にy座標が大きい文字群を抽出
    :param last_details: 最終ページの文字情報リスト
    :return: 下から2番目にy座標が大きい文字群の横書き文字列
    """
    # y座標が最も大きい文字群を抽出（最もy座標が大きい＝ページ下部に近い）
    sorted_by_y = sorted(last_details, key=lambda d: d['y'], reverse=True)  # y座標が大きいほど下に
    highest_y = sorted_by_y[0]['y']  # 最もy座標が大きい
    highest_group = [detail for detail in last_details if detail['y'] == highest_y]

    # 最もy座標が大きい文字群の中で最も左側の文字のx座標を取得
    leftmost_x = min(detail['x'] for detail in highest_group)

    # 右半分は、このx座標より大きいx座標を持つ文字群
    right_half = [detail for detail in last_details if detail['x'] > leftmost_x]

    # y座標を降順に並べる（y座標が大きいほど下に）
    right_half_sorted = sorted(right_half, key=lambda d: d['y'], reverse=True)

    if len(right_half_sorted) < 2:
        return None  # 要素が不足している場合はNoneを返す

    # y座標でグルーピング（同じy座標を持つ文字を1つのグループに）
    grouped_by_y = {}
    for detail in right_half_sorted:
        if detail['y'] not in grouped_by_y:
            grouped_by_y[detail['y']] = []
        grouped_by_y[detail['y']].append(detail)

    # y座標を降順にソート
    sorted_y_keys = sorted(grouped_by_y.keys(), reverse=True)

    # 最もy座標が大きい文字群を除外して、2番目に高い文字群を選択
    if len(sorted_y_keys) > 1:
        second_highest_y = sorted_y_keys[0]  # 2番目に高いy座標を取得
        second_highest_group = grouped_by_y[second_highest_y]

        # 同じy座標を持つ文字をx座標順にソート
        second_highest_group_sorted = sorted(second_highest_group, key=lambda d: d['x'])

        # 下から2番目のy座標に対応する文字列を作成（x座標が小さい順に結合）
        second_highest_line = ''.join([detail['text'] for detail in second_highest_group_sorted])

        return second_highest_line
    return None  # 要素が不足している場合

def extract_text_with_details(pdf_path):
    """
    PDFファイルから各文字のx座標、y座標、文字サイズ、ページ番号、テキスト内容を抽出する関数
    文字サイズが12.0の要素は除外し、太文字を強調文字として抽出する
    :param pdf_path: PDFファイルのパス
    :return: 各文字の詳細情報を含むリスト
    """
    doc = fitz.open(pdf_path)
    text_details = []
    last_details = []

    # 各ページを処理
    for page_num in tqdm.tqdm(range(1, len(doc)-1), leave=False, desc="Loading", unit="characters"):  # 1ページ目と最終ページは除外
        page = doc.load_page(page_num)
        blocks = page.get_text("dict")["blocks"]
        
        # 各ブロック内のテキストを取得
        for block in blocks:
            if block['type'] == 0:  # テキストブロック
                for line in block["lines"]:
                    for span in line["spans"]:
                        if span['size'] == 12.0:  # サイズが12.0の文字を除外
                            continue

                        # 太文字の判定（太字のみ強調文字とする）
                        is_emphasized = 'bold' in span['font'].lower()

                        for char in span['text']:
                            text_details.append({
                                'text': char,
                                'x': span['bbox'][0],  # 左端のx座標
                                'y': span['bbox'][1],  # 下端のy座標
                                'size': span['size'],  # 文字のサイズ
                                'page': page_num + 1,  # ページ番号（0始まりのため+1）
                                'emphasized': is_emphasized  # 太文字かどうか
                            })
    
    last_page = doc.load_page(len(doc)-1)
    last_blocks = last_page.get_text("dict")["blocks"]
    for last_block in last_blocks:
        if last_block['type'] == 0:  # テキストブロック
            for last_line in last_block["lines"]:
                for last_span in last_line["spans"]:
                    for last_char in last_span['text']:
                        last_details.append({
                            'text': last_char,
                            'x': last_span['bbox'][0],  # 左端のx座標
                            'y': last_span['bbox'][1],  # 下端のy座標
                            'size': last_span['size'],  # 文字のサイズ
                            'page': len(doc),  # ページ番号（0始まりのため+1）
                            'emphasized': 'bold' in last_span['font'].lower()  # 太文字かどうか
                        })

    return text_details, last_details

def sort_text_details(text_details):
    """
    指定された条件（ページ番号、x座標（降順）、y座標（降順））でソート
    :param text_details: 抽出されたテキスト情報リスト
    :return: ソート済みのリスト
    """
    return sorted(
        text_details,
        key=lambda item: (item['page'], -item['x'], item['y'])  # ソート条件
    )

def save_text_details_to_json(pdf_path):
    """
    PDFから抽出したテキスト情報をJSONファイルに保存する関数
    :param pdf_path: PDFファイルのパス
    :param json_file_path: 保存先のJSONファイルのパス
    """
    text_details, last_details = extract_text_with_details(pdf_path)
    sorted_details = sort_text_details(text_details)  # ソート

    # 最終ページ右半分から下から2番目にy座標が大きい文字列を抽出
    second_highest_line_text = extract_second_highest_group_line(last_details)

    logging.info(f"PDF発行日: {second_highest_line_text}")

    # JSON形式で保存
    return sorted_details, second_highest_line_text

def extract_details_from_introduction(introduction):
    # 正規表現で【タグ】を基準に文字列を分割
    unique_tag = ['【小説タイトル】', '【Ｎコード】', '【作者名】', '【あらすじ】']
    
    # 【小説タイトル】から【Ｎコード】までの間の処理
    novel_title_match = re.search(r'【小説タイトル】(.*?)【Ｎコード】', introduction, re.DOTALL)
    if novel_title_match:
        novel_title = novel_title_match.group(1).strip('\n　')
    else:
        novel_title = ""

    # 【Ｎコード】から【作者名】までの間の処理
    n_code_match = re.search(r'【Ｎコード】(.*?)【作者名】', introduction, re.DOTALL)
    if n_code_match:
        n_code = n_code_match.group(1).strip('\n　')
    else:
        n_code = ""

    # 【作者名】から【あらすじ】までの間の処理
    author_match = re.search(r'【作者名】(.*?)【あらすじ】', introduction, re.DOTALL)
    if author_match:
        author = author_match.group(1).strip('\n　')
    else:
        author = ""

    # 【あらすじ】から文字列の終わりまでの処理
    introduction_match = re.search(r'【あらすじ】(.*)', introduction, re.DOTALL)
    if introduction_match:
        n_introduction = introduction_match.group(1).strip('\n　')
    else:
        n_introduction = ""
    
    return novel_title, n_code, author, n_introduction

def remove_unwanted_spaces_recursive(text):
    # ネストに対応する正規表現パターンを定義
    patterns = [
        r'「([^「」]*)」', r'\{([^\{\}]*)\}', r'\(([^()]*)\)', r'（([^（）]*)）',
        r'\[([^\[\]]*)\]', r'【([^【】]*)】', r'『([^『』]*)』', r'≪([^≪≫]*)≫', r'〈([^〈〉]*)〉'
    ]

    text = re.sub(r'^\n', '', text)  # 先頭の \n を削除

    # 各パターンを繰り返し処理
    changed = True
    while changed:
        changed = False
        for pattern in patterns:
            # 括弧内を処理して再構築
            def replacer(match):
                content = match.group(1)
                content = re.sub(r'^\n', '', content)  # 先頭の \n を削除
                cleaned_content = re.sub(r'\n　', '', content)  # \n全角スペースを削除
                cleaned_content = re.sub(r'\n', '', cleaned_content)
                return match.group(0)[0] + cleaned_content + match.group(0)[-1]

            # テキストを置換処理
            new_text = re.sub(pattern, replacer, text)
            if new_text != text:
                changed = True
                text = new_text
    return text

def process_text_details(file_path, gen_date, author_id, author_url, novel_type, chapter):
    """
    指定されたJSONファイルからデータを処理し、ページ2の最大x座標を基準に各ページごとに条件に基づいて結果を返します。
    """
    tit = 1

    # チャプターを処理して範囲と値を取り出す
    ranges = []
    if chapter:
        for part in chapter.split(","):
            range_part, value = part.split(":")
            start, end = map(int, range_part.split("-"))
            ranges.append((start, end, value))
    
    # JSONデータの読み込み
    data = file_path

    # ページ2のデータを抽出
    page_2_data = [item for item in data if item['page'] == 2]
    if not page_2_data:
        raise ValueError("Page 2 data not found in the JSON file.")

    # ページ2のx座標が最大の値を取得
    max_x_page_2 = max(item['x'] for item in page_2_data)

    # すべてのページのy座標を取得し、最小値を特定
    all_y_coordinates = [item['y'] for item in data]
    min_y_coordinate = min(all_y_coordinates)

    # ページごとのデータを格納
    pages = set(item['page'] for item in data)
    titles = {}
    introductions = {}
    postscripts = {}
    results = {}

    # ページごとのデータ処理
    for page in tqdm.tqdm(pages, desc="Processing", unit="pages", leave=False):
        page_data = [item for item in data if item['page'] == page]
        matching_x_items = [item for item in page_data if item['x'] == max_x_page_2 and item.get('emphasized', False)]

        # ページ2の最大x座標と最小y座標が一致する場合はスキップ
        if any(item['y'] == min_y_coordinate for item in matching_x_items):
            continue

        # タイトルの結合
        title = ''.join(item['text'] for item in matching_x_items)

        if title.endswith('（前書き）'):
            introductions[tit] = {'title': title, 'page': page}
            continue
        elif title.endswith('（後書き）'):
            postscripts[tit-1] = {'title': title, 'page': page}
            continue

        if title:
            titles[tit] = {'title': title, 'page': page}
            tit += 1

    title_pages = []

    # titles のデータに基づいて title_pages を構築
    for key, value in tqdm.tqdm(titles.items(), desc="Processing", unit="titles", leave=False):
        if key in introductions:
            title_pages.append(introductions[key]['page'])
        title_pages.append(int(value['page']))
        if key in postscripts:
            title_pages.append(int(postscripts[key]['page']))

    # 初期化
    previous_x = previous_y = previous_page = 0
    previous_txt = ""
    full_texts = []

    for i in tqdm.tqdm(range(len(title_pages)), desc="Processing", unit="episodes", leave=False):
        texts = [item for item in data if item['x'] != max_x_page_2 or not item.get('emphasized', False)]
        txt_size = sorted({item['size'] for item in texts if 'size' in item}, reverse=True)
        line_space = txt_size[0] + txt_size[1] + 1 if len(txt_size) == 2 else 22

        # size == txt_size[0] に基づき texts をフィルタリング
        filtered_texts = [item for item in texts if item.get('size') == txt_size[0]]

        # xが最も大きい値を取得
        txt_x_max = max(filtered_texts, key=lambda item: item['x'])['x']
        txt_y_min = min(filtered_texts, key=lambda item: item['y'])['y']

        # 重複する y 値を除去
        unique_y_values = sorted({item['y'] for item in filtered_texts}, reverse=True)

        # txt_y_max_2: y の二番目に大きい値を取得
        txt_y_max_2 = unique_y_values[1] if len(unique_y_values) > 1 else None

        for item in tqdm.tqdm(filtered_texts, leave=False, desc="Processing", unit="texts"):
            if item.get('page') < title_pages[i]:
                continue

            if i != len(title_pages)-1 and int(item.get('page')) == title_pages[i+1]:
                break

            if int(item.get('page')) != previous_page:
                if item.get('x') != previous_x:
                    if previous_y == txt_y_max_2:
                        previous_txt += item['text']
                    elif any(char in item['text'] for char in ['　', ' ', '[', '{', '(', '（', '「', '『', '【']) and item.get('y') == txt_y_min:
                        if item['text'] == ' ':
                            previous_txt += '　'
                        previous_txt += '\n' + item['text']
                    elif item.get('y') == txt_y_min:
                        previous_txt += '\n' + item['text']
                    else:
                        previous_txt += item['text']
                else:
                    previous_txt += '\n' * int(abs(previous_x - item['x']) // line_space)
                    previous_txt += item['text']
            else:
                if item.get('x') != txt_x_max:
                    if previous_y == txt_y_max_2:
                        previous_txt += item['text']
                    elif any(char in item['text'] for char in ['　', ' ', '[', '{', '(', '（', '「', '『', '【']) and item.get('y') == txt_y_min:
                        if item['text'] == ' ':
                            previous_txt += '　'
                        previous_txt += '\n' + item['text']
                    elif item.get('y') == txt_y_min:
                        previous_txt += '\n' + item['text']
                    else:
                        previous_txt += item['text']
                else:
                    previous_txt += '\n' * int(abs(txt_x_max - item['x']) // line_space)
                    previous_txt += item['text']

            previous_page = int(item.get('page'))
            previous_x = item.get('x')
            previous_y = item.get('y')

        if i == 0:
            full_texts.append(remove_unwanted_spaces_recursive(previous_txt))
        else:
            for n in tqdm.tqdm(range(i), leave=False, desc="Merging", unit="episodes"):
                previous_txt = remove_unwanted_spaces_recursive(previous_txt).replace(full_texts[n], '')
            full_texts.append(remove_unwanted_spaces_recursive(previous_txt))
    
    novel_title, n_code, author, introduction = extract_details_from_introduction(full_texts[0])

    ncode = cm.full_to_half(n_code).lower()

    # 現在の日付をUTC+9の形式で取得
    update_date = str(datetime.now().astimezone(timezone(timedelta(hours=9))))
    gen_date = str(datetime.strptime(gen_date, "%Y年%m月%d日%H時%M分発行").replace(tzinfo=timezone(timedelta(hours=9))))

    results['get_date'] = update_date
    results['title'] = novel_title
    results['id'] = ncode
    results['url'] = "https://ncode.syosetu.com/" + ncode
    results['author'] = author
    results['author_id'] = author_id
    results['author_url'] = author_url
    results['caption'] = introduction
    results['total_episodes'] = ""
    results['all_episodes'] = ""
    results['total_characters'] = ""
    results['all_characters'] = ""
    if novel_type == 0:
        results['type'] = "連載中"
    elif novel_type == 1:
        results['type'] = "完結済"
    elif novel_type == 2:
        results['type'] = "短編"
    results['createDate'] = gen_date
    results['updateDate'] = update_date

    line_txt = 1
    all_str = 0
    all_episodes = 0
    results['episodes'] = {}
    for key, value in tqdm.tqdm(titles.items()):
        _key = int(key) -1
        if int(key) == 1:
            continue
        
        # episodes に _key が存在しない場合は初期化
        if not results['episodes'].get(_key):
            results['episodes'][_key] = {}

        results['episodes'][_key]['id'] = _key
        
        found = False  # この番号が範囲内かどうかを記録するフラグ
        if ranges:
            for start, end, value_ch in ranges:
                if start <= _key <= end:
                    results['episodes'][_key]['chapter'] = value_ch
                    found = True
                    break
        if not found:
            results['episodes'][_key]['chapter'] = ""

        results['episodes'][_key]['title'] = ""
        results['episodes'][_key]['textCount'] = ""
        if key in introductions:
            results['episodes'][_key]['introduction'] = full_texts[line_txt]
            line_txt += 1
        else:
            results['episodes'][_key]['introduction'] = ""
        results['episodes'][_key]['title'] = value['title']
        results['episodes'][_key]['text'] = full_texts[line_txt]
        results['episodes'][_key]['textCount'] = int(len(full_texts[line_txt].replace('\n', '')))
        all_str += len(full_texts[line_txt].replace('\n', ''))
        line_txt += 1
        if key in postscripts:
            results['episodes'][_key]['postscript'] = full_texts[line_txt]
            line_txt += 1
        else:
            results['episodes'][_key]['postscript'] = ""

        results['episodes'][_key]['createDate'] = gen_date
        results['episodes'][_key]['updateDate'] = update_date

        all_episodes = _key

    results['total_episodes'] = int(all_episodes)
    results['all_episodes'] = int(all_episodes)
    results['total_characters'] = int(all_str)
    results['all_characters'] = int(all_str)

    with open('results.json', 'w', encoding='utf-8') as file:
        json.dump(results, file, ensure_ascii=False, indent=4)

    return results, ncode


def gen_from_pdf(pdf_path, pdf_name, author_id, author_url, novel_type, chapter, folder_path, key_data, data_path, host_name):

    pdf_file = os.path.join(pdf_path, pdf_name)

    json_data, last_details = save_text_details_to_json(pdf_file)

    results, ncode = process_text_details(json_data, last_details, author_id, author_url, int(novel_type), chapter)

    cm.make_dir(ncode, folder_path)

    raw_path = os.path.join(folder_path, ncode, 'raw', 'raw.json')

    # フォルダの作成
    if not int(novel_type) == 2:
        i = 1

        for key, value in results['episodes'].items():
            os.makedirs(os.path.join(folder_path, ncode, str(i)), exist_ok=True)
            i += 1

    cm.save_raw_diff(raw_path, os.path.join(folder_path, ncode), results)

    with open(raw_path, 'w', encoding='utf-8') as file:
        json.dump(results, file, ensure_ascii=False, indent=4)

    cn.narou_gen(results, os.path.join(folder_path, ncode), key_data, data_path, host_name)

    cm.gen_site_index(folder_path, key_data, '小説家になろう')

    os.remove(pdf_file)
    
    logging.info(f"Generated {ncode} from PDF file")

def init(cookie_path, is_login, interval):
    pass

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

    cm.gen_site_index(folder_path, key_data, '小説家になろう')

## 使用例
#if __name__ == "__main__":
#
#    # pdf_path = "N0411FU.pdf"
#    pdf_path = "N7145BL.pdf"
#    json_data , last_details = save_text_details_to_json(pdf_path)
#
#    introductions, titles, postscripts, results = process_text_details(json_data, last_details)
#    
#    # 出力例: titles, introductions, postscripts を表示
#    for key, value in titles.items():
#        if key in introductions:
#            logging.info(f"{key}: page:{introductions[key]['page']} {introductions[key]['title']}")
#        logging.info(f"{key}: page:{value['page']} {value['title']}")
#    
#    for key, value in postscripts.items():
#        logging.info(f"{key}: page:{value['page']} {value['title']}")