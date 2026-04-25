# sample/

このディレクトリは Python 版 narou_bridge の参照実装で、Rust ランタイムからは一切呼ばれません。新機能の追加先ではありません。データ仕様や旧挙動の確認用です。

主な参照先:

- `server.py`: 旧 Flask サーバーと API/queue 実装
- `util.py`: 旧設定読み込み、静的ファイル生成、アクション振り分け
- `crawler/`: 旧サイト別クローラー群
  - `crawler/common.py`
  - `crawler/site_runtime.py`
  - `crawler/www_pixiv_net.py`
  - `crawler/ncode_syosetu_com.py`
  - `crawler/convert_narou.py`

Rust 実装の変更点を検討する際は、ここを「移行前の挙動確認用アーカイブ」としてのみ参照してください。
