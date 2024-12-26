# Narou Bridge

Narou rb が対応していないウェブサイトを, 小説家になろうに近いHTMLファイルに変換するツール.

## How to use
1. seetting.ini を設定
2. main.cmd または main.sh を実行

## How to download
1. Narou.rb で narou init したフォルダに webnovel フォルダをコピー
2. Narou.rb で利用する Aozora Epub 3 フォルダ内の chuki_tag.txt の行末に以下のテキストを追加

```
### Narou Bridge embedded custom chuki ###
ｌｉｎｋ＿ｓ	<a href="
link_s	<a href="
ｌｉｎｋ＿ｔ	">
link_t	">
ｌｉｎｋ＿ｅ	</a>
link_e	</a>
### Narou Bridge embedded custom chuki ###
```

## To Do
特定の条件でのみ画像ファイルがフォルダと認識されてしまう不具合の修正  
詳細なオリジナルスクリプト作成手順の表記

## How to setting

#### setting/setting.ini を変更

設定例(Tailscaleの443ポートからサーバーに転送する前提)
```
[setting]
data=
cookie=
log=
interval=2
reload=900
auto_update=1
auto_update_interval=43200

[crawler]
pixiv=www_pixiv_net.py

[login]
pixiv=1

[server]
domain=example.tail0exam.ts.net
port=8080
job=server
clustering=0
key=0
ssl=0
ssl_crt=
ssl_key=
use_proxy=1
proxy_port=443
proxy_ssl=1
```

中身の詳細
```
[setting]
data=小説の保存先(強制的にそのフォルダ内にdataフォルダが作成される, 空だとスクリプトが存在するフォルダ)
cookie=クッキーの保存先(強制的にそのフォルダ内にcookieフォルダが作成される, 空だとスクリプトが存在するフォルダ)
log=クッキーの保存先(強制的にそのフォルダ内にcookieフォルダが作成される, 空だとスクリプトが存在するフォルダ)
interval=小説やファイルを取得する間隔の秒数(2秒以上が負荷が少なくていいかも, int 型なので整数指定)
reload=現状無意味(デフォルトでOK)
auto_update=サーバー起動時に小説の自動アップデートをするか(0で無効, 1で有効)
auto_update_interval=小説の自動アップデートをする際の間隔(秒指定, デフォルトで12時間)

[crawler]
サイト名(内部で使う名前)=crawlerフォルダにあるファイル名(サイトのURLから'https://'を抜いて '.' を '_' に入れ替えることを推奨)

[login]
サイト名=ログインをするか否か(すべてのサイトでこの設定があるわけではない, ログインするなら1, 違うなら0)

[server]
domain=サーバーを公開するURL(localhostで内部にできる)
port=サーバーを公開するポート番号
job=現状無意味(デフォルトでOK)
clustering=現状無意味(デフォルトでOK)
key=現状無意味(デフォルトでOK)
ssl=スクリプト単体でSSLサーバーを立てるかどうか(0でSSL無し, 1でSSLあり)
ssl_crt=SSLを有効にする際のcrtファイルのパス
ssl_key=SSLを有効にする際のkeyファイルのパス
use_proxy=プロキシを利用するか否か(0で無効, 1で有効)
proxy_port=プロキシが公開するポート(実際にユーザーがアクセスするポート)
proxy_ssl=プロキシが公開する際にSSLを利用するか否か(0で無効, 1で有効)
```