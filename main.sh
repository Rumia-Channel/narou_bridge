#!/usr/bin/env bash
set -euxo pipefail

echo "[main] uv 環境同期"
uv sync

echo "[main] Playwright 依存ライブラリインストール開始: $(date)"
uv run playwright install-deps
echo "[main] install-deps 完了: $(date)"

echo "[main] Playwright ブラウザバイナリインストール開始"
uv run playwright install
echo "[main] install 完了"

echo "[main] アプリケーション起動"
# Python のバッファリング無効化(-u)かつプロセスを置き換え
exec uv run python -u main.py