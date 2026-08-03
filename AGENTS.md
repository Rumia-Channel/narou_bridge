# Repository Guidelines

現行ランタイムは `src/` 配下の Rust / Axum サーバーです。`src/main.rs` 以外の起動経路や Python ランタイムはありません。

## Runtime Architecture

`src/main.rs`
- `setting.ini` を読み、SQLite を開き、Axum サーバーを起動する唯一の entrypoint。`setting.ini` が無ければ Git 管理外の `setting/setting.ini` から起動時に自動コピーする
- `rebuild-store <source.sqlite3> <output.sqlite3>` と `install-store <source.sqlite3> <destination.sqlite3>` の DB 保守 CLI もここから実行

`src/core/runtime.rs`
- `/api/`, `/api/tasks`, `/api/account`, `/api/library/*`, `/api/health`
- SQLite-backed SSR: `/<site>/`, `/<site>/<work>/`, `/<site>/<work>/<episode>/`
- single-worker 永続キューと optional auto-update loop

`src/core/storage.rs`
- SQLite の tasks / works / accounts / images / site_documents
- WAL、busy timeout、stale-running task recovery
- SQLite が全メタデータと作品 JSON の唯一の正本

`src/core/renderer.rs`
- DB の共通作品モデルから HTML を決定的に生成
- サイト一覧は検索、絞り込み、sort、ページングを SQL で実行して SSR
- 作品 JSON や生成 HTML をディスクへ保存しない

`src/core/static_bootstrap.rs`
- root page、reader shell、manifest、CSS / JS / icon のみを `<data>` へ配置
- 作品・サイト別 index は生成しない

`src/core/registry.rs`, `src/sites/*`
- 固定 `SiteId` registry: `pixiv`, `narou`
- 固定 action: `download`, `login`, `update`, `re_download`, `convert`, `repair`
- site module は fetch / parse / normalize と approved sidecar state に限定

## Request And Worker Flow

1. `POST /api/` が multipart または form body を `RequestData` へ正規化する。
2. PDF / ZIP payload は `<pdf>/<request_id>.<ext>` へ一時保存する。
3. SQLite の tasks へ deduplicate して enqueue する。
4. worker が最大一件を claim する。
5. action は次の優先順で最初の値だけを実行する。
   `repair -> login -> update -> re_download -> convert -> download`
6. 外部 POST parameter `add` は内部 action `download` に対応する。
7. site handler が共通作品 model を組み立て、Store へ直接 upsert する。
8. SSR と Reader API は Store から読み、ディスク上の旧 JSON を参照しない。

## Persistent Contracts

`<data>/runtime.sqlite3`
- `works`: site + work_key、一覧列、canonical work JSON
- `images`: logical_name、hash、ext、kind
- `site_documents`: Pixiv tracked-user config と illust snapshot
- `accounts`: account JSON、display name、active flag
- `tasks`: queue と実行履歴

`<data>/images/<hash>.<ext>`
- ローカル画像実体。画像メタデータは SQLite が正本。
- `setting.ini [server].img_url` があれば、renderer と cover route は外部画像サーバー URL を使う。

`<pdf>/<request_id>.pdf|zip`
- queued task の一時 upload。完了または失敗後に cleanup する。

次の legacy 保存先や URL は存在しない。
- `raw/raw.json`, `<site>/index.json`, 生成済み work HTML
- `images/database.json`, `images/cover.json`
- `queue/task.json`
- `cookie/<site>/*.json`, `login.json` mirror
- migration HTTP API、JSON tree import、archive flow

## Canonical Work Model

`WorkRecord.raw_json` は次の意味を持つ共通 schema を保持する。
- work: `version`, `get_date`, `title`, `id`, `nid`, `url`, author fields, caption, episode/character totals, `type`, `serialization`, tags, dates
- `episodes`: sequential display key ごとの id, chapter, title, textCount, tags, introduction, text, postscript, dates
- episode text markup: `[image](logical_name.ext)`, `[newpage]`, `[ruby:<漢字>(かな)]`, `[jump:3]`

schema を変更する場合は Reader API、SSR、Pixiv / Narou normalizer を同時に更新する。

## Frontend Contracts

トップページは次の external fields を送る。
- `add`, `update`, `convert`, `re_download`, `repair`
- `pdf`, `author_id`, `author_url`, `novel_type`, `chapter`
- `zip`

Reader は `/api/library/works` を site ごとに100件ずつページングし、選択時に work metadata、本文表示時に対象 episode だけを取得する。一括 library JSON、全 episode 本文の先読み、browser-side site index renderer を再導入しない。

サイト一覧の query:
- `page`, `per_page`, `search`, `type`, `serialization`, `sort`

## Implementation Rules

- SQLite を正本とし、JSON mirror / fallback / dual-write を追加しない。
- rendering は Store の normalized data からの deterministic transform に保つ。
- core が queue、path policy、HTTP、indexing、rendering を所有する。
- site module から filesystem JSON を永続化しない。
- 新規 crate は `cargo add` で追加し、`Cargo.toml` を手編集しない。
- exported symbol を変更する前に LSP references を確認する。

## Testing And Validation

サーバープロセスを agent 自身で起動してはならない。実サーバーを使う確認が必要なら、最初にユーザーへ「サーバーは起動していますか？」と質問する。未起動なら次のコマンドを案内して待つ。

```bash
cargo run --release
```

サーバー不要の最低検証:
- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo test --all-targets`
- `cargo clippy --all-targets`

behavior change は router の in-process E2E で HTTP status、cache headers、DB state を確認する。画像変更時は image row と実ファイル / external URL の整合を確認する。

## Commit Notes

日本語の commit message で behavior、DB contract、互換性への影響を明記する。小さな改行変更だけの commit は作らない。
