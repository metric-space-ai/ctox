#!/usr/bin/env python3
"""C++ vs Rust JSON-RPC live behavior + dest SHA. No secrets.

  ARIA2_CPP=/usr/bin/aria2c ARIA2_RUST=./aria2c python3 scripts/gold_rpc.py
"""
from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def rpc(port: int, method: str, params: list[object] | None = None) -> object:
    req = json.dumps(
        {"jsonrpc": "2.0", "id": "1", "method": method, "params": params or []}
    ).encode()
    r = urllib.request.Request(
        f"http://127.0.0.1:{port}/jsonrpc",
        data=req,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(r, timeout=15) as resp:
        body = json.loads(resp.read().decode())
    if "error" in body:
        raise RuntimeError(f"{method}: {body['error']}")
    return body["result"]


def wait_status(port: int, gid: str, want: str, timeout: float = 25.0) -> dict:
    deadline = time.time() + timeout
    last = {}
    while time.time() < deadline:
        last = rpc(port, "aria2.tellStatus", [gid])
        if isinstance(last, dict) and last.get("status") == want:
            return last
        if isinstance(last, dict) and last.get("status") == "error":
            raise RuntimeError(last.get("errorMessage", "error"))
        time.sleep(0.05)
    raise TimeoutError(f"{gid} not {want}: {last}")


def run_daemon(bin_path: str, port: int, directory: Path) -> subprocess.Popen:
    directory.mkdir(parents=True, exist_ok=True)
    lf = (directory / "daemon.log").open("wb")
    p = subprocess.Popen(
        [
            bin_path,
            "--enable-rpc",
            "--rpc-listen-all=true",
            "--rpc-allow-origin-all",
            f"--rpc-listen-port={port}",
            f"--dir={directory}",
            "--file-allocation=none",
            "--console-log-level=error",
        ],
        stdout=lf,
        stderr=subprocess.STDOUT,
    )
    for _ in range(60):
        try:
            rpc(port, "aria2.getVersion")
            return p
        except (urllib.error.URLError, TimeoutError, OSError):
            time.sleep(0.1)
    p.terminate()
    raise RuntimeError(f"rpc {port} did not start")


def gated_http(body: bytes, started: threading.Event, release: threading.Event) -> ThreadingHTTPServer:
    n = len(body)
    mv = memoryview(body)

    class H(BaseHTTPRequestHandler):
        def do_HEAD(self) -> None:
            self.send_response(200)
            self.send_header("Content-Length", str(n))
            self.send_header("Accept-Ranges", "bytes")
            self.end_headers()

        def do_GET(self) -> None:
            started.set()
            release.wait(30)
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


def exercise(port: int, url: str, started: threading.Event, release: threading.Event) -> str:
    ver = rpc(port, "aria2.getVersion")
    if not isinstance(ver, dict) or "version" not in ver:
        raise RuntimeError("getVersion")
    methods = rpc(port, "system.listMethods")
    if not isinstance(methods, list) or "aria2.addUri" not in methods:
        raise RuntimeError("listMethods")
    gid = str(rpc(port, "aria2.addUri", [[url], {"out": "blob.bin"}]))
    if not started.wait(10):
        raise TimeoutError("http not hit")
    rpc(port, "aria2.pause", [gid])
    st = wait_status(port, gid, "paused")
    files = rpc(port, "aria2.getFiles", [gid])
    uris = rpc(port, "aria2.getUris", [gid])
    active = rpc(port, "aria2.tellActive")
    waiting = rpc(port, "aria2.tellWaiting", [0, 10])
    gstat = rpc(port, "aria2.getGlobalStat")
    opt = rpc(port, "aria2.getOption", [gid])
    rpc(port, "aria2.changeOption", [gid, {"max-download-limit": "0"}])
    if not isinstance(files, list) or not isinstance(uris, list):
        raise RuntimeError("getFiles/getUris")
    if not isinstance(gstat, dict) or not isinstance(opt, dict):
        raise RuntimeError("global/option")
    _ = active, waiting, st
    rpc(port, "aria2.unpause", [gid])
    release.set()
    wait_status(port, gid, "complete")
    rpc(port, "aria2.purgeDownloadResult")
    return gid


def main() -> int:
    cpp = os.environ.get("ARIA2_CPP", "/usr/bin/aria2c")
    rust = os.environ.get("ARIA2_RUST", "")
    if not rust or not Path(cpp).is_file() or not Path(rust).is_file():
        print("skip: set ARIA2_CPP and ARIA2_RUST", file=sys.stderr)
        return 0
    body = bytes(range(256)) * 256
    expect = hashlib.sha256(body).hexdigest()
    failed: list[str] = []
    ok: list[str] = []
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        started = threading.Event()
        release = threading.Event()
        httpd = gated_http(body, started, release)
        url = f"http://127.0.0.1:{httpd.server_address[1]}/blob.bin"
        cpp_p = rust_p = None
        try:
            cpp_p = run_daemon(cpp, 16810, root / "rpc-cpp")
            exercise(16810, url, started, release)
            if sha256_file(root / "rpc-cpp" / "blob.bin") == expect:
                ok.append("C++ pause/unpause/tell/getFiles")
            else:
                failed.append("C++ sha")
        except Exception as e:
            failed.append(f"C++ {e}")
        started.clear()
        release = threading.Event()
        httpd.shutdown()
        started2 = threading.Event()
        release2 = threading.Event()
        httpd2 = gated_http(body, started2, release2)
        url2 = f"http://127.0.0.1:{httpd2.server_address[1]}/blob.bin"
        try:
            rust_p = run_daemon(rust, 16811, root / "rpc-rust")
            exercise(16811, url2, started2, release2)
            if sha256_file(root / "rpc-rust" / "blob.bin") == expect:
                ok.append("Rust pause/unpause/tell/getFiles")
            else:
                failed.append("Rust sha")
        except Exception as e:
            failed.append(f"Rust {e}")
        finally:
            for p in (cpp_p, rust_p):
                if p is not None:
                    p.terminate()
                    try:
                        p.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        p.kill()
            httpd2.shutdown()
    print("GOLD RPC ok=", ",".join(ok) or "-", "fail=", ",".join(failed) or "-", "sha", expect)
    return 0 if not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
