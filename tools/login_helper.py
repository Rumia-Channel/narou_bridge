#!/usr/bin/env python3
"""Minimal cookie helper for legacy login migration.

This script does not automate browsers. It writes a cookie JSON file that matches
the expected account file shape so the Rust runtime can import it.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) < 4:
        print("usage: login_helper.py <site> <output.json> <display_name> [user_agent]")
        return 1

    site = sys.argv[1]
    output = Path(sys.argv[2])
    display_name = sys.argv[3]
    user_agent = sys.argv[4] if len(sys.argv) > 4 else ""

    payload = {
        "site": site,
        "cookies": {},
        "user_agent": user_agent,
        "display_name": display_name,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(f"wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
