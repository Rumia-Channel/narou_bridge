#!/usr/bin/env bash
set -euxo pipefail

echo "[main] Rye 環境同期"
rye sync

echo "[main] Playwright 依存ライブラリインストール開始: $(date)"
rye run playwright install-deps
echo "[main] install-deps 完了: $(date)"

echo "[main] Playwright ブラウザバイナリインストール開始"
rye run playwright install
echo "[main] install 完了"

echo "[main] アプリケーション起動"
# Python のバッファリング無効化(-u)かつプロセスを置き換え
exec rye run python -u main.py