#!/bin/bash

USER_NAME="user"
source /home/$USER_NAME/.bashrc
cd "$(dirname "$0")"
rye sync
#rye run playwright install
rye run python main.py