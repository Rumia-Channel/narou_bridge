@echo off

cd /d %~dp0
rye sync
rye run playwright install
rye run python main.py