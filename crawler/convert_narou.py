from datetime import datetime
import os
import re

#目次の書き込み
def write_index(f, data, key_data):
    """エピソード情報を基に目次を生成する関数"""
    
    for ep in data['episodes'].values():  # ソートせずにそのまま順番で処理
        episode_id = ep['id']
        episode_title = ep['title']
        create_date = datetime.strptime(ep['createDate'], "%Y-%m-%dT%H:%M:%S%z").strftime("%Y/%m/%d %H:%M")
        update_date = datetime.strptime(ep['updateDate'], "%Y-%m-%dT%H:%M:%S%z").strftime("%Y/%m/%d %H:%M")
        
        f.write(f'<dl class="novel_sublist2 customlayout1">\n')
        f.write(f'<dd class="subtitle">\n')
        f.write(f'<a href="./{episode_id}/{key_data}">{episode_title}</a>\n')
        f.write(f'</dd>\n')
        f.write(f'<dt class="long_update">\n')
        f.write(f'{create_date}<span title="{update_date} 改稿">（<u>改</u>）</span>\n')
        f.write(f'</dt>\n')
        f.write(f'</dl>\n')

#画像リンクへの置き換え
def replace_images(text, key_data):
    pattern = r'\[image\]\((.*?)\)'
    return re.sub(pattern, lambda match: f'<img src="./{match.group(1)}{key_data}" alt="{match.group(1)}">', text)

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
    #text = replace_newpage(text)

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
    f.write('<div id="novel_p" class="novel_view">\n')
    f.write(format_text(preface, 'Lp', key_data) + '\n')
    f.write('</div>\n')

#本文の書き込み
def write_main_text(f, ep, key_data):
    text = ep.get('text')
    f.write('<div id="novel_honbun" class="novel_view">\n')
    f.write(format_text(text, 'L', key_data) + '\n')
    f.write('</div>\n')

#あとがきの書き込み
def write_postscript(f, ep, key_data):
    postscript = ep.get('postscript')
    if postscript == '':
        return
    f.write('<div id="novel_a" class="novel_view">\n')
    f.write(format_text(postscript, 'La', key_data) + '\n')
    f.write('</div>\n')

#小説家になろう形式で生成
def narou_gen(data, nove_path, key_data):
    if data.get("type") == "短編":
        return

    page_counter = 2  # 最初のページ番号
    for ep in data['episodes'].values():
        # エピソードの開始ページリンクを記録
        digits = max(4, len(str(page_counter)))
        
        # [newpage] でページを分割
        pages = ep['text'].split('[newpage]')
        page_links = []

        for page in pages:
            # 各ページのリンクを保存（ページ番号をカウントしてリンクを作成）
            page_link = f'{str(page_counter).zfill(digits)}.xhtml'
            page_links.append(page_link)
            page_counter += 1
        
        # 各ページ内の [jump:X] を対応するリンクに置き換え
        formatted_text = []
        for page in pages:
            for jump in range(1, len(page_links) + 1):
                jump_marker = f'[jump:{jump}]'
                if jump_marker in page:
                    jump_link = f'<a href="{page_links[jump - 1]}">{jump}ページ目へ移動</a>'
                    page = page.replace(jump_marker, jump_link)
            formatted_text.append(page)

        # フォーマット済みテキストをエピソードに保存
        ep['text'] = '[newpage]'.join(formatted_text)

    #目次ファイルの生成
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
        f.write(f'<h1><a href="{data.get('url')}">{data.get("title")}</a></h1>\n')
        f.write(f'<div><span id="noveltype">{data.get("type")}</span> 全{data.get("total_episodes")}部</div>\n')
        f.write('<table>\n')
        f.write(f'<tr><th class="ex">あらすじ</th><td class="ex">{data.get("caption")}</td></tr>\n')
        f.write('<tr>\n')
        f.write('<th>作者名</th>\n')
        f.write(f'<td><a href="{data.get("authour_url")}">{data.get("authour")}</a></td>\n')
        f.write('</tr>\n')
        f.write('<tr>\n')
        f.write('<th>掲載日</th>\n')
        f.write(f'<td>{datetime.fromisoformat(data.get("createDate")).strftime("%Y年 %m月%d日 %H時%M分")}</td>\n')
        f.write('</tr>\n')
        f.write('<tr>\n')
        f.write('<th>最終話掲載日</th>\n')
        f.write(f'<td>{datetime.fromisoformat(data.get("updateDate")).strftime("%Y年 %m月%d日 %H時%M分")}</td>\n')
        f.write('</tr>\n')
        f.write('<tr>\n')
        f.write('<th>文字数</th>\n')
        f.write(f'<td>{data.get("total_characters")}文字</td>\n')
        f.write('</tr>\n')
        f.write('</table>\n')
        f.write('</body>\n')
        f.write('</html>\n')

    #エピソードファイルの生成
    for ep in data['episodes'].values():
        ep_path = os.path.join(nove_path, f'{ep["id"]}', 'index.html')
        with open(ep_path, 'w', encoding='utf-8') as f:
            f.write('<!DOCTYPE html>\n')
            f.write('<html lang="ja">\n')
            f.write('<head>\n')
            f.write('<meta charset="UTF-8">\n')
            f.write('<meta name="viewport" content="width=device-width, initial-scale=1.0">\n')
            f.write('<title>Index Pixiv</title>\n')
            f.write('</head>\n')
            f.write('<body>\n')
            f.write(f'<a href="../{key_data}">戻る</a>\n')
            write_preface(f, ep, key_data)  # 前書き生成
            write_main_text(f, ep, key_data) # 本文生成
            write_postscript(f, ep, key_data) # あとがき生成
            f.write('</body>\n')
            f.write('</html>\n')

    #完了
    print(f'{data.get("title")}の変換が完了しました。 終了時刻: {datetime.now().astimezone().strftime('%Y-%m-%d %H:%M:%S%z')}')