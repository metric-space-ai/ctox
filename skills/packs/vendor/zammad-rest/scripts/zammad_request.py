#!/usr/bin/env python3
import argparse
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from base64 import b64encode


def build_headers(args: argparse.Namespace) -> dict[str, str]:
    headers = {"Accept": "application/json"}
    if args.content_type:
        headers["Content-Type"] = args.content_type

    token = args.token or os.environ.get("ZAMMAD_API_TOKEN")
    user = args.user or os.environ.get("ZAMMAD_USER")
    password = args.password or os.environ.get("ZAMMAD_PASSWORD")

    if token:
        headers["Authorization"] = f"Token token={token}"
        return headers
    if user and password:
        pair = f"{user}:{password}".encode("utf-8")
        headers["Authorization"] = "Basic " + b64encode(pair).decode("ascii")
        return headers

    raise SystemExit(
        "Missing auth. Set ZAMMAD_API_TOKEN or provide ZAMMAD_USER and ZAMMAD_PASSWORD."
    )


def load_body(args: argparse.Namespace) -> bytes | None:
    if args.body_file:
        with open(args.body_file, "rb") as handle:
            return handle.read()
    if args.json_body:
        return json.dumps(json.loads(args.json_body)).encode("utf-8")
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Authenticated Zammad API helper.")
    parser.add_argument("--base-url", default=os.environ.get("ZAMMAD_BASE_URL"))
    parser.add_argument("--method", default="GET")
    parser.add_argument("--path", required=True)
    parser.add_argument("--token")
    parser.add_argument("--user")
    parser.add_argument("--password")
    parser.add_argument("--content-type", default="application/json")
    parser.add_argument("--json-body")
    parser.add_argument("--body-file")
    parser.add_argument("--timeout", type=int, default=30)
    parser.add_argument("--include-status", action="store_true")
    args = parser.parse_args()

    if not args.base_url:
        raise SystemExit("Missing base URL. Use --base-url or set ZAMMAD_BASE_URL.")

    base_url = args.base_url.rstrip("/")
    path = args.path if args.path.startswith("/") else f"/{args.path}"
    url = urllib.parse.urljoin(base_url + "/", path.lstrip("/"))
    body = load_body(args)
    headers = build_headers(args)

    request = urllib.request.Request(
        url=url,
        data=body,
        headers=headers,
        method=args.method.upper(),
    )

    try:
        with urllib.request.urlopen(request, timeout=args.timeout) as response:
            payload = response.read()
            if args.include_status:
                print(f"STATUS {response.status}")
            sys.stdout.write(payload.decode("utf-8", errors="replace"))
            return 0
    except urllib.error.HTTPError as exc:
        if args.include_status:
            print(f"STATUS {exc.code}")
        sys.stdout.write(exc.read().decode("utf-8", errors="replace"))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
