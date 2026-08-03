# Narou Bridge

Pixiv と PDF / ZIP 入力を共通作品モデルへ正規化し、SQLite から HTML と Reader API を動的に返す Rust / Axum サーバーです。

## 起動

設定は Git 管理外の `setting/setting.ini` を正本とします。起動時にリポジトリ直下の `setting.ini` が存在しない場合、`setting/setting.ini` があれば自動コピーして読み込みます。どちらも無い場合は起動エラーになります。

新規導入時は、`setting.ini.example` を `setting.ini` にコピーして編集するか、`setting/setting.ini` を用意してください。`setting.ini` と `setting/` は `.gitignore` で管理対象外です。

```bash
cargo run --release
```

ヘルスチェック:

```bash
curl http://127.0.0.1:8080/api/health
```

設定項目の例は `setting.ini.example` を参照してください。

```ini
[setting]
data=C:\Users\me\Documents\Webnovel\narou_bridge
log=
pdf=
auto_update=0
auto_update_interval=43200

[server]
domain=127.0.0.1
port=8080
use_proxy=0
proxy_port=443
proxy_ssl=0
img_url=https://images.example.com
```

- `data` が空の場合はリポジトリ直下の `data` を使います。
- 永続データの正本は `<data>/runtime.sqlite3` です。
- `log` と `pdf` が空の場合はリポジトリ直下の同名ディレクトリを使います。
- `img_url` を指定すると、カバーと本文画像の URL は外部画像サーバーを指します。未指定時は `<data>/images/<hash>.<ext>` を配信します。
- 画像レスポンスは immutable、カバー解決と静的アセットは CDN 向け `Cache-Control` を返します。

## データ契約

SQLite が次のデータの唯一の正本です。

- `works`: 作品メタデータと共通作品 JSON
- `images`: 論理画像名、content hash、拡張子、cover/image 種別
- `site_documents`: Pixiv 追跡ユーザーとスナップショット
- `accounts`: Cookie アカウントと active 状態
- `tasks`: 永続キューと実行履歴

`raw/raw.json`、`index.json`、`images/database.json`、`images/cover.json`、`queue/task.json`、`cookie/<site>/*.json` はランタイム保存先として使用しません。これらの旧 URL も公開しません。作品ページ、作品情報、エピソード、サイト一覧は SQLite からリクエスト時に SSR します。

画像バイナリだけは SQLite BLOB にせず、ローカルまたは外部画像サーバーに content hash 名で置きます。

## アカウント登録

`tools/login_helper.py` で生成したアカウント JSON は、トップページのアカウント管理からアップロードします。アップロード後は SQLite に保存され、JSON ファイルの配置や mirror は行いません。

```bash
cd tools
uv sync
uv run python login_helper.py pixiv output/account.json --display-name "main account"
```

既存 Cookie を入力に使う場合は `--cookies-file existing.json` を指定できます。

## 主要エンドポイント

| Endpoint | Method | 用途 |
| --- | --- | --- |
| `/api/` | POST | download / update / repair / convert / PDF / ZIP を永続キューへ投入 |
| `/api/tasks` | GET | タスク状態 |
| `/api/library/works` | GET | 検索・絞り込み・sort・offset/limit 対応の作品一覧 API |
| `/api/library/works/<site>/<work>` | GET | 本文を含まない Reader 用作品・目次 metadata |
| `/api/library/works/<site>/<work>/episodes/<episode>` | GET | 選択したエピソード本文 |
| `/<site>/` | GET | 検索・sort・ページング対応の SQLite-backed SSR 一覧 |
| `/<site>/<work>/` | GET | 作品目次 SSR |
| `/<site>/<work>/<episode>/` | GET | エピソード SSR |
| `/covers/<site>/<work>` | GET | DB の画像メタデータから cover URL を解決 |
| `/reader/` | GET | 軽量 Reader UI |
| `/api/account` | GET / POST / DELETE | DB アカウント管理 |
| `/api/health` | GET | ヘルスチェック |

`/api/` はクロールを同期実行しません。SQLite へ投入後、single-worker が一件ずつ処理します。

## DB 保守 CLI

DB の再パックと検証:

```bash
cargo run --release -- rebuild-store "<source.sqlite3>" "<new.sqlite3>"
```

検証済み DB の内容を既存 DB へトランザクション単位で導入:

```bash
cargo run --release -- install-store "<source.sqlite3>" "<destination.sqlite3>"
```

どちらも SQLite DB 同士だけを扱います。旧 JSON tree を探索・移行するコマンドや HTTP migration API はありません。
