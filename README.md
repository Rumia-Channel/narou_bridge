# Narou Bridge

Rust 版 `narou_bridge` は、Pixiv や PDF 由来の作品データを小説家になろう互換の `raw.json` / HTML / reader 出力へ正規化するサーバーです。現行ランタイムは Rust 実装で、旧 Python 実装は `sample/` に reference として保存されています。

## 起動

1. リポジトリ直下の `setting.ini` を編集します。  
   初回起動時に `setting\setting.ini` が自動生成されます。
2. サーバーを起動します。

```bash
cargo run --release
```

3. ヘルスチェックを確認します。

```bash
curl http://127.0.0.1:8080/api/health
```

## `setting.ini` の場所と最低限の設定

- 優先される設定ファイル: リポジトリ直下の `setting.ini`
- 互換用ランタイムコピー: `setting\setting.ini`
- Rust ランタイムが読む主なセクション: `[setting]`, `[server]`
- 旧 Python 版向けの `[crawler]`, `[login]`, `[display_name]` は移行資料として残しても構いませんが、Rust サーバーの起動には必須ではありません。

最小例:

```ini
[setting]
data=
cookie=
queue=
pdf=
log=
auto_update=0
auto_update_interval=43200

[server]
domain=127.0.0.1
port=8080
use_proxy=0
proxy_port=443
proxy_ssl=0
```

- 空欄のパスはリポジトリ基準で解決され、`data\`, `cookie\`, `queue\`, `pdf\`, `log\` が作られます。
- 永続ストアは `data\runtime.sqlite3` に作成されます。

## Cookie ログイン情報の用意

`tools\login_helper.py` は別 `uv` プロジェクトです。ここで cookie JSON を作成し、`cookie\<site>\login.json` に配置します。

```bash
cd tools
uv sync
uv run python login_helper.py pixiv output/login.json --display-name "main account"
```

生成した JSON をランタイム配置へコピーします。

```bash
copy output\login.json ..\cookie\pixiv\login.json
```

- `pixiv` の部分は対象サイト ID に置き換えてください。
- 既存 cookie を流用する場合は `--cookies-file existing.json` が使えます。

## 旧 Python 版データの取り込み

`POST /api/migrate` で旧 Python 版の `data\`, `cookie\`, `queue\` を含むディレクトリを取り込めます。`source_root` には **旧 Python 版のルート** を指定してください（`data\` 単体ではなく、その親ディレクトリ）。

例:

```bash
curl -X POST "http://127.0.0.1:8080/api/migrate?source_root=C:\Users\me\Desktop\narou_bridge_py"
```

- `source_root` を省略すると既定で `sample\` を参照します。
- 取り込み後、旧ファイル群は `archive\` へ退避されます。

## 主要エンドポイント

| Endpoint | Method | 用途 |
| --- | --- | --- |
| `/api/` | POST | ダウンロード / 更新 / repair / convert / PDF / ZIP 取り込みをキュー投入 |
| `/api/migrate` | POST | 旧 Python 版ツリーの移行 |
| `/api/health` | GET | ヘルスチェック |
| `/<site>/index.html` | GET | サイト別の生成済み一覧ページ |
| `/reader/` | GET | 軽量 reader UI |

補足:

- `/reader/?site=<site>&nid=<work>` で作品 reader を直接開けます。
- `/api/` は同期クロールせず、常にキューへ積んでバックグラウンド worker が順次処理します。

## `/api/` の代表例

URL 追加:

```bash
curl -X POST -F "add=https://www.pixiv.net/novel/show.php?id=123456" http://127.0.0.1:8080/api/
```

全件更新:

```bash
curl -X POST -F "update=all" http://127.0.0.1:8080/api/
```

## Legacy Python reference

- `sample/` は旧 Python 版 `narou_bridge` の参照実装です。
- Rust ランタイムから直接呼ばれません。
- 旧データ仕様、移行前挙動、差分確認のためだけに保持しています。
