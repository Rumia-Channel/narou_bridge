#!/usr/bin/env python3
"""Minimal cookie helper for legacy login migration.

This script does not automate browsers. It imports existing cookie JSON or writes
the expected account file shape so the Rust runtime can import it.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("site", help="site key, e.g. pixiv")
    parser.add_argument("output", help="output account json path")
    parser.add_argument("--display-name", default="", help="display name to store")
    parser.add_argument("--user-agent", default="", help="user agent to store")
    parser.add_argument(
        "--cookies-file",
        default="",
        help="path to a JSON file containing cookies or a full account file",
    )
    parser.add_argument(
        "--cookies-json",
        default="",
        help="raw JSON string containing cookies or a full account file",
    )
    args = parser.parse_args()

    payload = {
        "cookies": {},
        "user_agent": args.user_agent,
        "display_name": args.display_name,
    }

    if args.cookies_file:
        payload.update(load_cookie_source(Path(args.cookies_file)))
    elif args.cookies_json:
        payload.update(load_cookie_source(args.cookies_json))

    payload["user_agent"] = args.user_agent or payload.get("user_agent", "")
    payload["display_name"] = args.display_name or payload.get("display_name", "")

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(f"wrote {output} for {args.site}")
    return 0


def load_cookie_source(source: Path | str) -> dict:
    if isinstance(source, Path):
        data = json.loads(source.read_text(encoding="utf-8"))
    else:
        data = json.loads(source)

    if isinstance(data, dict) and "cookies" in data:
        cookies = data.get("cookies", {})
        if isinstance(cookies, list):
            cookies = normalize_cookie_list(cookies)
        return {
            "cookies": cookies,
            "user_agent": data.get("user_agent") or data.get("ua") or "",
            "display_name": data.get("display_name") or "",
        }

    if isinstance(data, list):
        return {
            "cookies": normalize_cookie_list(data),
            "user_agent": "",
            "display_name": "",
        }

    if isinstance(data, dict):
        return {
            "cookies": data,
            "user_agent": "",
            "display_name": "",
        }

    raise TypeError(f"unsupported cookie source: {type(data)!r}")


def normalize_cookie_list(items: list) -> dict:
    cookies = {}
    for item in items:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        value = item.get("value")
        if name is not None and value is not None:
            cookies[str(name)] = value
    return cookies


if __name__ == "__main__":
    raise SystemExit(main())
