#!/usr/bin/env python3
"""C++ vs Rust HTTP gold: dest SHA + wall time. No secrets.

  ARIA2_CPP=/usr/bin/aria2c ARIA2_RUST=./aria2c python3 scripts/gold_http.py

Env:
  GOLD_BYTES  payload size (default 33554432 = 32MiB)
  GOLD_SPLIT  --split / -x (default 4)
  GOLD_SOAK   rust repeats after first match (default 2)
"""
from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def serve(body: bytes) -> ThreadingHTTPServer:
    mv = memoryview(body)
    n = len(body)

    class H(BaseHTTPRequestHandler):
        def do_HEAD(self) -> None:
            self.send_response(200)
            self.send_header("Content-Length", str(n))
            self.send_header("Accept-Ranges", "bytes")
            self.end_headers()

        def do_GET(self) -> None:
            rng = self.headers.get("Range", "")
            if rng.lower().startswith("bytes="):
                spec = rng.split("=", 1)[1]
                a, _, b = spec.partition("-")
                start = int(a or 0)
                end = int(b) if b else n - 1
                end = min(end, n - 1)
                chunk = mv[start : end + 1]
                self.send_response(206)
                self.send_header("Content-Length", str(len(chunk)))
                self.send_header("Content-Range", f"bytes {start}-{end}/{n}")
                self.send_header("Accept-Ranges", "bytes")
                self.end_headers()
                self.wfile.write(chunk)
                return
            self.send_response(200)
            self.send_header("Content-Length", str(n))
            self.send_header("Accept-Ranges", "bytes")
            self.end_headers()
            self.wfile.write(mv)

        def log_message(self, *_a: object) -> None:
            return

    httpd = ThreadingHTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch(bin_path: str, dest: Path, url: str, extra: list[str]) -> float:
    dest.mkdir(parents=True, exist_ok=True)
    cmd = [
        bin_path,
        "--file-allocation=none",
        "-d",
        str(dest),
        "-o",
        "blob.bin",
        *extra,
        url,
    ]
    t0 = time.perf_counter()
    subprocess.check_call(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
    return time.perf_counter() - t0


def main() -> int:
    cpp = os.environ.get("ARIA2_CPP", "/usr/bin/aria2c")
    rust = os.environ.get("ARIA2_RUST", "")
    if not rust or not Path(cpp).exists() or not Path(rust).exists():
        print("skip: set ARIA2_CPP and ARIA2_RUST to existing binaries", file=sys.stderr)
        return 0
    size = int(os.environ.get("GOLD_BYTES", str(32 * 1024 * 1024)))
    split = int(os.environ.get("GOLD_SPLIT", "4"))
    soak = int(os.environ.get("GOLD_SOAK", "2"))
    body = bytes(range(256)) * (size // 256)
    if len(body) < size:
        body += bytes(range(size - len(body)))
    httpd = serve(body)
    port = httpd.server_address[1]
    url = f"http://127.0.0.1:{port}/blob.bin"
    flags = [f"--split={split}", f"--max-connection-per-server={split}"]
    expect = hashlib.sha256(body).hexdigest()
    with tempfile.TemporaryDirectory() as td:
        d = Path(td)
        t_cpp = fetch(cpp, d / "cpp", url, flags)
        t_rust = fetch(rust, d / "rust", url, flags)
        cpp_sha = sha256_file(d / "cpp" / "blob.bin")
        rust_sha = sha256_file(d / "rust" / "blob.bin")
        soak_ok = True
        soak_times: list[float] = []
        for i in range(max(0, soak - 1)):
            dest = d / f"soak{i}"
            t = fetch(rust, dest, url, flags)
            soak_times.append(t)
            if sha256_file(dest / "blob.bin") != expect:
                soak_ok = False
    httpd.shutdown()
    ratio = t_rust / t_cpp if t_cpp > 0 else 0.0
    print(
        "GOLD HTTP",
        f"bytes={len(body)}",
        f"split={split}",
        f"sha={expect}",
        f"cpp={t_cpp:.4f}s",
        f"rust={t_rust:.4f}s",
        f"ratio={ratio:.3f}",
        f"soak={'ok' if soak_ok else 'FAIL'}",
        f"soak_times={','.join(f'{t:.4f}' for t in soak_times) or '-'}",
    )
    if cpp_sha != expect or rust_sha != expect or not soak_ok:
        print("GOLD MISMATCH", "cpp", cpp_sha, "rust", rust_sha, "expect", expect)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
