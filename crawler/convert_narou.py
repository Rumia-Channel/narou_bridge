from datetime import datetime
import os
import re

#ログを保存
import logging

#目次の書き込み
def write_index(f, data, key_data):
    """エピソード情報を基に目次を生成する関数"""
    
    chapter = None
    
    f.write(f'<div class="p-eplist">\n')

    for ep in data['episodes'].values():  # ソートせずにそのまま順番で処理
        episode_id = ep['id']
        episode_title = ep['title']
        create_date = datetime.fromisoformat(ep['createDate']).strftime("%Y/%m/%d %H:%M")
        update_date = datetime.fromisoformat(ep['updateDate']).strftime("%Y/%m/%d %H:%M")
        if not ep['chapter'] == chapter:
            chapter = ep['chapter']
            f.write(f'<div class="p-eplist__chapter-title">{chapter}</div>\n')
        f.write(f'<div class="p-eplist__sublist">\n')
        f.write(f'<a href="{a_link}/{episode_id}/{key_data}" class="p-eplist__subtitle">\n{episode_title}\n</a>\n\n')
        f.write(f'<div class="p-eplist__update">\n')
        f.write(f'{create_date}\n<span title="{update_date} 改稿">（<u>改</u>）</span>\n')
        f.write(f'</div>\n')
        f.write(f'</div>\n')

    f.write(f'</div>\n')

#画像リンクへの置き換え
def replace_images(text, key_data):
    pattern = r'\[image\]\((.*?)\)'
    return re.sub(pattern, lambda match: f'<img src="{link}{match.group(1)}{key_data}" alt="{match.group(1)}">', text)

#改ページの置き換え
def replace_newpage(text):
    pattern = r'\[newpage\]'
    return re.sub(pattern, lambda match: '[#改ページ]', text)

#ルビの置き換え
def replace_ruby(text):
    pattern = r"\[ruby:<(.*?)>\((.*?)\)\]"
    return re.sub(pattern, lambda match: f'<ruby>{match.group(1)}<rp>(</rp><rt>{match.group(2)}</rt><rp>)</rp></ruby>', text)

#テキスト形式の整形
def format_text(text, id_prefix, key_data):
    
    #画像リンクの置き換え
    text = replace_images(text, key_data)
    #青空文庫形式の改ページへ置き換え
    text = replace_newpage(text)

    #ルビの置き換え
    text = replace_ruby(text)

    # 改行で分割
    lines = text.split('\n')
    
    #画像リンクの正規表現
    img_pattern = re.compile(r'(.*?)(<img[^>]*>)(.*)')
    
    formatted_paragraphs = []
    id_counter = 1
    
    for line in lines:
        line = line.rstrip()
        if line == '':
            # 空行を <br> タグに変換
            formatted_paragraphs.append(f'<p id="{id_prefix}{id_counter}"><br></p>')
            id_counter += 1
        elif img_pattern.search(line):
            # <img> タグが含まれている行の処理
            match = img_pattern.search(line)
            before_img = match.group(1).strip()  # <img の前のテキスト
            img_tag = match.group(2).strip()  # <img タグ
            after_img = match.group(3).strip()  # <img の後のテキスト
            
            if before_img:
                # <img> タグの前にテキストがある場合
                formatted_paragraphs.append(f'<p id="{id_prefix}{id_counter}">{before_img}</p>')
                id_counter += 1
            
            # <img> タグを含む部分を追加
            formatted_paragraphs.append(f'<p id="{id_prefix}{id_counter}">{img_tag}</p>')
            id_counter += 1
            
            if after_img:
                # <img> タグの後にテキストがある場合
                formatted_paragraphs.append(f'<p id="{id_prefix}{id_counter}">{after_img}</p>')
                id_counter += 1
        else:
            formatted_paragraphs.append(f'<p id="{id_prefix}{id_counter}">{line}</p>')
            id_counter += 1
    
    # 結果を一つのテキストに結合
    result = '\n'.join(formatted_paragraphs)
    
    return result

#前書きの書き込み
def write_preface(f, ep, key_data):
    preface = ep.get('introduction')
    if preface == '':
        return
    f.write('<div class="js-novel-text p-novel__text p-novel__text--preface">\n')
    f.write(format_text(preface, 'Lp', key_data) + '\n')
    f.write('</div>\n')

#本文の書き込み
def write_main_text(f, ep, key_data):
    text = ep.get('text')
    f.write('<div class="js-novel-text p-novel__text">\n')
    f.write(format_text(text, 'L', key_data) + '\n')
    f.write('</div>\n')

#あとがきの書き込み
def write_postscript(f, ep, key_data):
    postscript = ep.get('postscript')
    if postscript == '':
        return
    f.write('<div class="js-novel-text p-novel__text p-novel__text--afterword">\n')
    f.write(format_text(postscript, 'La', key_data) + '\n')
    f.write('</div>\n')

#小説家になろう形式で生成
def narou_gen(data, nove_path, key_data, data_folder, host_name):

    page_counter = 2  # 最初のページ番号を2に設定
    pattern = re.compile(
        r'\[newpage\]\s*|\s*\[image\]\((.*?)\)|'  # [newpage] または [image](...)（スペースを許可）
        r'\[image\]\s*\((.*?)\)\s*(?:\\n|\n)+\s*\[newpage\]|\s*'  # [image] の後に改行（文字列または実際の改行）と [newpage]
        r'\[newpage\]\s*(?:\\n|\n)+\s*\[image\]\s*\((.*?)\)|'  # [newpage] の後に改行と [image]
        r'\[newpage\]\s*\[image\]\s*\((.*?)\)|'  # [newpage] の後に [image]（スペースを許可）
        r'\[image\]\s*\((.*?)\)\s*\[newpage\]|'  # [image] の後に [newpage]（スペースを許可
        r'(?:\s*(?:\\n|\n)*\s*\[image\]\((.*?)\)\s*)+'  # [image](...) の後に [image](...)（スペースを許可）
    )
    jump_pattern = re.compile(r'\[jump:(\d+)\]')   # '[jump(x)]' の形式のパターン

    for ep in data['episodes'].values():
        digits = max(4, len(str(page_counter)))

        # 仮想ページの設定
        v_page = []
        v_page.append(page_counter)

        logging.debug(v_page[0]) # デバッグ用

        # ページカウンターの更新ループ
        for match in pattern.finditer(ep['text']):

            if match.group(0).strip() == '[newpage]':
                # [newpage] タグのみの場合
                page_counter += 1
                v_page.append(page_counter)  # 仮想ページにページ番号を追加
                logging.debug(f'newpage: {page_counter}')  # デバッグ用
            elif match.group(0).startswith('[image]'):
                # [image](...) タグがマッチしている場合
                page_counter += 2
                logging.debug(f'image: {page_counter}')  # デバッグ用
            elif match.group(1):  # [image] の後に改行（文字列または実際の改行）と [newpage] の場合
                page_counter += 2
                if 'newpage' in match.group(0):
                    v_page.append(page_counter)  # 仮想ページにページ番号を追加
                logging.debug(f'image after newline and newpage: {page_counter}')  # デバッグ用
            elif match.group(3):  # [newpage] の後に [image] の場合
                page_counter += 1
                if 'newpage' in match.group(0):
                    v_page.append(page_counter)  # 仮想ページにページ番号を追加
                page_counter += 1 # 画像のページ
                logging.debug(f'newpage after image: {page_counter}')  # デバッグ用
            elif match.group(4):  # (?:\s*(?:\\n|\n)*\s*\[image\]\((.*?)\)\s*)+
                # 連続する [image](...) タグの場合
                images_count = len(match.group(4).strip().split())
                page_counter += images_count + 1  # 1ページと画像数分のページを追加

        # ジャンプ処理ループ
        for match in jump_pattern.finditer(ep['text']):
            jump_value = int(match.group(1))  # 'jump(x)'のxを整数として取得
            if 0 <= jump_value - 1 < len(v_page):
                ep['text'] = ep['text'].replace(match.group(), f'[#link_s]{str(v_page[jump_value - 1]).zfill(digits)}.xhtml[#link_t]{jump_value}ページ目へ移動[#link_e]')

        page_counter += 1  # 次のエピソードのページ番号を設定
       

    global link
    global a_link

    #目次ファイルの生成
    if not data.get("type") == "短編":
        link = host_name + nove_path.replace(data_folder, '').replace('\\', '/')
        a_link = nove_path.replace(data_folder, '').replace('\\', '/')
        index_path = os.path.join(nove_path, 'index.html')
        with open(index_path, 'w', encoding='utf-8') as f:
            f.write('<!DOCTYPE html>\n')
            f.write('<html lang="ja">\n')
            f.write('<head>\n')
            f.write('<meta charset="UTF-8">\n')
            f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
            f.write('<title>Index Pixiv</title>\n')
            f.write('</head>\n')
            f.write('<body>\n')
            f.write(f'<a href="../{key_data}">戻る</a>\n')
            f.write(f'<a href="./info/{key_data}">作品情報</a>\n')
            f.write(f'<p class="novel_title">{data.get("title")}</p>\n')
            f.write('<div class="index_box">\n')
            write_index(f, data, key_data)  # 目次生成
            f.write('</div>\n')
            f.write('</body>\n')
            f.write('</html>\n')
    
    #インフォメーションファイルの生成
    info_path = os.path.join(nove_path, 'info', 'index.html')
    with open(info_path, 'w', encoding='utf-8') as f:
        f.write('<!DOCTYPE html>\n')
        f.write('<html lang="ja">\n')
        f.write('<head>\n')
        f.write('<meta charset="UTF-8">\n')
        f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
        f.write('<title>Index Pixiv</title>\n')
        f.write('</head>\n')
        f.write('<body>\n')
        f.write(f'<a href="../{key_data}">戻る</a>\n')
        f.write(f'<h1><a href="{data.get("url")}">{data.get("title")}</a></h1>\n')
        if data.get("type") == "短編":
            f.write(f'<div><span id="noveltype">短編</span></div>\n')
        else:
            f.write(f'<div><span id="noveltype">{data.get("type")}</span> 全{data.get("total_episodes")}エピソード</div>\n')
        f.write('<table>\n')
        f.write(f'<tr><th class="ex">あらすじ</th><td class="ex">{data.get("caption")}</td></tr>\n')
        f.write('<tr>\n')
        f.write('<th>作者名</th>\n')
        f.write(f'<td><a href="{data.get("author_url")}">{data.get("author")}</a></td>\n')
        f.write('</tr>\n')
        f.write('<tr>\n')
        f.write('<th>掲載日</th>\n')
        f.write(f'<td>{datetime.fromisoformat(data.get("createDate")).strftime("%Y年 %m月%d日 %H時%M分")}</td>\n')
        f.write('</tr>\n')
        f.write('<tr>\n')
        if data.get("type") == "短編":
            f.write('<th>最終更新日</th>\n')
        elif data.get("type") == "連載中":
            f.write('<th>最新掲載日</th>\n')
        elif data.get("type") == "完結済":
            f.write('<th>最終掲載日</th>\n')
        f.write(f'<td>{datetime.fromisoformat(data.get("updateDate")).strftime("%Y年 %m月%d日 %H時%M分")}</td>\n')
        f.write('</tr>\n')
        f.write('<tr>\n')
        f.write('<th>文字数</th>\n')
        f.write(f'<td>{int(data.get("total_characters")):,}文字</td>\n')
        f.write('</tr>\n')
        f.write('</table>\n')
        f.write('</body>\n')
        f.write('</html>\n')

    #エピソードファイルの生成
    for ep in data['episodes'].values():
        if data.get("type") == "短編":
            ep_path = os.path.join(nove_path, 'index.html')
        else:
            ep_path = os.path.join(nove_path, f'{ep["id"]}', 'index.html')
        
        link = host_name + ep_path.replace('index.html', '').replace(data_folder, '').replace('\\', '/')
        with open(ep_path, 'w', encoding='utf-8') as f:
            f.write('<!DOCTYPE html>\n')
            f.write('<html lang="ja">\n')
            f.write('<head>\n')
            f.write('<meta charset="UTF-8">\n')
            f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
            f.write(f'<title>{ep["title"]}</title>\n')
            f.write('</head>\n')
            f.write('<body>\n')
            f.write(f'<a href="../{key_data}">戻る</a>\n')
            if data.get("type") == "短編":
                f.write(f'<a href="./info/{key_data}">作品情報</a>\n')
                f.write(f'<p class="novel_title">{data.get("title")}</p>\n')
            else:
                f.write(f'<a href="../info/{key_data}">作品情報</a>\n')
                f.write(f'<p class="novel_subtitle">{ep["title"]}</p>\n')
            write_preface(f, ep, key_data)  # 前書き生成
            write_main_text(f, ep, key_data) # 本文生成
            write_postscript(f, ep, key_data) # あとがき生成
            f.write('</body>\n')
            f.write('</html>\n')
    #完了
    logging.info(f'{data.get("title")}の変換が完了しました。')