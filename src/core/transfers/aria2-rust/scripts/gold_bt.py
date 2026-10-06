#!/usr/bin/env python3
"""C++ aria2c seeds, Rust aria2c leeches, dest SHA must match. No secrets.

  ARIA2_CPP=/usr/bin/aria2c ARIA2_RUST=./aria2c python3 scripts/gold_bt.py
"""
from __future__ import annotations

import hashlib
import os
import struct
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path


def benc_bytes(b: bytes) -> bytes:
    return f"{len(b)}:".encode() + b


def benc_str(s: str) -> bytes:
    return benc_bytes(s.encode())


def benc_int(i: int) -> bytes:
    return f"i{i}e".encode()


def benc_dict(items: list[tuple[bytes, bytes]]) -> bytes:
    out = b"d"
    for k, v in sorted(items, key=lambda x: x[0]):
        out += benc_bytes(k) + v
    return out + b"e"


def make_torrent(name: str, piece_len: int, data: bytes, announce: str) -> bytes:
    pieces = b""
    for i in range(0, len(data), piece_len):
        pieces += hashlib.sha1(data[i : i + piece_len]).digest()
    info = benc_dict(
        [
            (b"length", benc_int(len(data))),
            (b"name", benc_str(name)),
            (b"piece length", benc_int(piece_len)),
            (b"pieces", benc_bytes(pieces)),
        ]
    )
    return benc_dict([(b"announce", benc_str(announce)), (b"info", info)])


def main() -> int:
    cpp = os.environ.get("ARIA2_CPP", "/usr/bin/aria2c")
    rust = os.environ.get("ARIA2_RUST", "")
    if not rust or not Path(cpp).is_file() or not Path(rust).is_file():
        print("skip: set ARIA2_CPP and ARIA2_RUST", file=sys.stderr)
        return 0
    body = bytes(i % 256 for i in range(64 * 1024))
    piece_len = 16 * 1024
    seed_port = 16881

    class Tracker(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            peers = b"\x7f\x00\x00\x01" + struct.pack("!H", seed_port)
            payload = benc_dict(
                [(b"interval", benc_int(60)), (b"peers", benc_bytes(peers))]
            )
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *_a: object) -> None:
            return

    httpd = HTTPServer(("127.0.0.1", 0), Tracker)
    trk_port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    announce = f"http://127.0.0.1:{trk_port}/announce"
    torrent = make_torrent("blob.bin", piece_len, body, announce)

    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        seed_dir = root / "seed"
        leech_dir = root / "leech"
        seed_dir.mkdir()
        leech_dir.mkdir()
        tpath = root / "gold.torrent"
        tpath.write_bytes(torrent)
        (seed_dir / "blob.bin").write_bytes(body)
        common = [
            "--console-log-level=warn",
            "--enable-dht=false",
            "--enable-dht6=false",
            "--bt-enable-lpd=false",
            "--enable-peer-exchange=false",
            "--file-allocation=none",
            f"--bt-tracker={announce}",
            f"--torrent-file={tpath}",
        ]
        seed_cmd = [
            cpp,
            *common,
            "--seed-ratio=0.0",
            f"--listen-port={seed_port}",
            "--check-integrity=true",
            f"--dir={seed_dir}",
        ]
        seed_proc = subprocess.Popen(
            seed_cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )
        try:
            time.sleep(0.6)
            leech_cmd = [
                rust,
                "--enable-dht=false",
                "--enable-dht6=false",
                "--bt-enable-lpd=false",
                "--enable-peer-exchange=false",
                "--file-allocation=none",
                "--seed-ratio=0",
                "--listen-port=16882",
                f"--bt-tracker={announce}",
                f"--torrent-file={tpath}",
                f"--dir={leech_dir}",
            ]
            subprocess.check_call(leech_cmd, timeout=30)
            got = (leech_dir / "blob.bin").read_bytes()
        finally:
            seed_proc.terminate()
            try:
                seed_proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                seed_proc.kill()
            httpd.shutdown()
    if got != body:
        print(
            "GOLD BT MISMATCH",
            hashlib.sha256(got).hexdigest(),
            hashlib.sha256(body).hexdigest(),
            "sizes",
            len(got),
            len(body),
        )
        return 1
    print(
        "GOLD BT DEST-MATCH vs C++",
        hashlib.sha256(got).hexdigest(),
        "bytes",
        len(got),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
