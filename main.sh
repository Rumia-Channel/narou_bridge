#!/bin/bash

cd "$(dirname "$0")"
rye sync
rye run playwright install-deps
rye run playwright install
rye run python main.py