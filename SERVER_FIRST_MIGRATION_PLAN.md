# サーバー主体ランタイム移行計画

## 最終系

- SQLite を作品・話・タグ・アカウント・ジョブ・画像メタデータの唯一の正本にする。
- Axum が DB から HTML / JSON / reader 用レスポンスを動的生成する。
- `raw.json`、`index.json`、生成 HTML は永続正本ではなく、移行入力または手動 export の互換物にする。
- 画像実体は DB BLOB 化せず、既存どおり `data/images/<hash>.<ext>` のファイルとして保持する。
- DB の画像テーブルは logical name、hash、拡張子、用途などのメタデータだけを持つ。

## 移行段階

### 1. DB-backed read path を完成させる

- `/images/database.json`、`/images/cover.json`、`/<site>/index.json`、`/<site>/<work>/raw/raw.json` は DB から返す。
- 作品 HTML、作品情報 HTML、話 HTML、サイト index HTML も DB から動的生成する。
- 既存の静的ファイル配信は、動的ルートで扱えない asset と互換ファイル用に残す。

### 2. renderer を純粋生成と export に分ける

- HTML / JSON 文字列を作る関数を副作用なしの API として維持する。
- ファイルへ書く `render_site_from_store()` は互換 export 用に格下げする。
- site module は DB へ normalized record を保存し、通常操作では HTML/JSON を書かない。

### 3. site index / reader をページング API 化する

- 巨大な `index.json` 全件取得を前提にしない。
- 作品一覧は `limit` / `offset` / sort / tag / author / type / search で DB query する。
- reader は必要な作品・話だけを DB API から取得する。

### 4. DB schema を raw snapshot 依存から正規化へ寄せる

- `works` と `raw_json` の併存から始める。
- `episodes`、`tags`、`work_tags`、`images`、`work_images` を正規化する。
- `raw_json` は旧形式 export / migration audit 用の snapshot として扱う。

### 5. 互換ファイル生成を optional export にする

- `convert` / `repair` は DB 更新を主目的にする。
- HTML/JSON ファイル生成は明示的な export action または管理 API に分離する。
- `data/<site>/...` は通常運用では不要な出力先にする。

## 現在の実装スライス

- DB からサイト index HTML、作品 HTML、作品情報 HTML、話 HTML を返す Axum ルートを追加する。
- 画像ファイル配信は既存の `ServeDir` / `/images/...` に任せる。
- 既存の JSON 互換ルートと静的 export は残す。
