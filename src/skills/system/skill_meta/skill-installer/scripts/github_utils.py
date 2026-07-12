#!/usr/bin/env python3
"""Shared GitHub helpers for skill install scripts."""

from __future__ import annotations

import http.client
import os
import urllib.parse


def github_request(url: str, user_agent: str) -> bytes:
    require_github_https_url(url)
    headers = {"User-Agent": user_agent}
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        headers["Authorization"] = f"token {token}"
    status, body = github_http_get(url, headers)
    if not (200 <= status < 300):
        raise RuntimeError(f"GitHub API request failed with HTTP {status}")
    return body


def require_github_https_url(url: str) -> None:
    parsed = urllib.parse.urlparse(str(url or ""))
    if parsed.scheme != "https" or parsed.netloc.lower() != "api.github.com":
        raise ValueError(f"Refusing non-GitHub HTTPS API URL: {url!r}")


def github_http_get(url: str, headers: dict[str, str]) -> tuple[int, bytes]:
    require_github_https_url(url)
    parsed = urllib.parse.urlparse(str(url))
    path = parsed.path or "/"
    if parsed.query:
        path = f"{path}?{parsed.query}"
    conn = http.client.HTTPSConnection(parsed.hostname, parsed.port, timeout=30)
    try:
        conn.request("GET", path, headers=headers)
        response = conn.getresponse()
        return response.status, response.read()
    finally:
        conn.close()


def github_api_contents_url(repo: str, path: str, ref: str) -> str:
    return f"https://api.github.com/repos/{repo}/contents/{path}?ref={ref}"
