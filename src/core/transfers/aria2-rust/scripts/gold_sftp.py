#!/usr/bin/env python3
"""C++ vs Rust SFTP dest SHA. Needs a password-auth sshd on GOLD_SFTP_PORT.

  GOLD_SFTP_USER GOLD_SFTP_PASS ARIA2_CPP ARIA2_RUST python3 scripts/gold_sftp.py

Does not print the password. No secrets in git.
"""
from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tempfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch(bin_path: str, dest: Path, extra: list[str], url: str) -> None:
    dest.mkdir(parents=True, exist_ok=True)
    cmd = [
        bin_path,
        "--file-allocation=none",
        "--console-log-level=error",
        "-d",
        str(dest),
        "-o",
        "blob.bin",
        *extra,
        url,
    ]
    subprocess.check_call(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT, timeout=40)


def main() -> int:
    cpp = os.environ.get("ARIA2_CPP", "/usr/bin/aria2c")
    rust = os.environ.get("ARIA2_RUST", "")
    user = os.environ.get("GOLD_SFTP_USER", "")
    passwd = os.environ.get("GOLD_SFTP_PASS", "")
    port = os.environ.get("GOLD_SFTP_PORT", "2222")
    src = os.environ.get("GOLD_SFTP_SRC", "")
    if not rust or not Path(cpp).is_file() or not Path(rust).is_file():
        print("skip: set ARIA2_CPP and ARIA2_RUST", file=sys.stderr)
        return 0
    if not user or not passwd or not src or not Path(src).is_file():
        print("skip: set GOLD_SFTP_USER/PASS/SRC", file=sys.stderr)
        return 0
    body = Path(src).read_bytes()
    expect = hashlib.sha256(body).hexdigest()
    url = f"sftp://127.0.0.1:{port}{src if src.startswith('/') else '/' + src}"
    extra = [
        f"--ftp-user={user}",
        f"--ftp-passwd={passwd}",
        "--split=1",
    ]
    with tempfile.TemporaryDirectory() as td:
        d = Path(td)
        fetch(cpp, d / "cpp", extra, url)
        fetch(rust, d / "rust", extra, url)
        cpp_sha = sha256_file(d / "cpp" / "blob.bin")
        rust_sha = sha256_file(d / "rust" / "blob.bin")
    print("GOLD SFTP", f"bytes={len(body)}", f"sha={expect}", f"cpp={cpp_sha}", f"rust={rust_sha}")
    if cpp_sha != expect or rust_sha != expect:
        print("GOLD MISMATCH")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
