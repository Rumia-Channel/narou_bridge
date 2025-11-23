@echo off

cd /d %~dp0
uv sync
uv run playwright install
uv run python main.py