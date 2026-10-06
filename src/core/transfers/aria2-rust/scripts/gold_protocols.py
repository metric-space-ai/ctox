#!/usr/bin/env python3
"""C++ vs Rust protocol E2E (HTTPS, Metalink, FTP, JSON-RPC). No secrets.

  ARIA2_CPP=/usr/bin/aria2c ARIA2_RUST=./aria2c python3 scripts/gold_protocols.py
"""
from __future__ import annotations

import hashlib
import json
import os
import socket
import ssl
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


def serve_http(body: bytes, tls: tuple[str, str] | None = None) -> ThreadingHTTPServer:
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
    if tls:
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(tls[0], tls[1])
        httpd.socket = ctx.wrap_socket(httpd.socket, server_side=True)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


def fetch(bin_path: str, dest: Path, extra: list[str], url: str | None = None) -> None:
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
    ]
    if url:
        cmd.append(url)
    subprocess.check_call(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT, timeout=40)


def start_ftp(body: bytes, name: str) -> tuple[int, threading.Event]:
    stop = threading.Event()
    lsock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    lsock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    lsock.bind(("127.0.0.1", 0))
    lsock.listen(8)
    lsock.settimeout(0.5)
    port = lsock.getsockname()[1]

    def session(c: socket.socket) -> None:
        c.sendall(b"220 gold\r\n")
        data_sock: socket.socket | None = None
        buf = b""
        while not stop.is_set():
            try:
                chunk = c.recv(4096)
            except OSError:
                break
            if not chunk:
                break
            buf += chunk
            while b"\r\n" in buf:
                line, buf = buf.split(b"\r\n", 1)
                cmd = line.decode("ascii", "replace").strip()
                u = cmd.upper()
                if u.startswith("USER"):
                    c.sendall(b"331 ok\r\n")
                elif u.startswith("PASS"):
                    c.sendall(b"230 ok\r\n")
                elif u.startswith("SYST") or u.startswith("FEAT") or u.startswith("TYPE"):
                    c.sendall(b"200 ok\r\n")
                elif u.startswith("PWD"):
                    c.sendall(b'257 "/"\r\n')
                elif u.startswith("CWD") or u.startswith("SIZE"):
                    c.sendall(b"250 ok\r\n")
                elif u.startswith("PASV"):
                    ds = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                    ds.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                    ds.bind(("127.0.0.1", 0))
                    ds.listen(1)
                    dp = ds.getsockname()[1]
                    data_sock = ds
                    c.sendall(
                        f"227 Entering Passive Mode (127,0,0,1,{dp // 256},{dp % 256})\r\n".encode()
                    )
                elif u.startswith("RETR"):
                    c.sendall(b"150 data\r\n")
                    if data_sock is None:
                        c.sendall(b"425 no pasv\r\n")
                        continue
                    data_sock.settimeout(5)
                    d, _ = data_sock.accept()
                    d.sendall(body)
                    d.close()
                    data_sock.close()
                    data_sock = None
                    c.sendall(b"226 done\r\n")
                elif u.startswith("QUIT"):
                    c.sendall(b"221 bye\r\n")
                    return
                else:
                    c.sendall(b"200 ok\r\n")

    def loop() -> None:
        while not stop.is_set():
            try:
                c, _ = lsock.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            threading.Thread(target=session, args=(c,), daemon=True).start()
        lsock.close()

    threading.Thread(target=loop, daemon=True).start()
    return port, stop


def rpc(port: int, method: str, params: list[object]) -> object:
    req = json.dumps(
        {"jsonrpc": "2.0", "id": "1", "method": method, "params": params}
    ).encode()
    r = urllib.request.Request(
        f"http://127.0.0.1:{port}/jsonrpc",
        data=req,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(r, timeout=10) as resp:
        body = json.loads(resp.read().decode())
    if "error" in body:
        raise RuntimeError(body["error"])
    return body["result"]


def wait_rpc_complete(port: int, gid: str, timeout: float = 20.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        st = rpc(port, "aria2.tellStatus", [gid])
        if isinstance(st, dict) and st.get("status") == "complete":
            return
        if isinstance(st, dict) and st.get("status") == "error":
            raise RuntimeError(st.get("errorMessage", "rpc error"))
        time.sleep(0.05)
    raise TimeoutError(gid)


def run_daemon(bin_path: str, port: int, directory: Path) -> subprocess.Popen:
    directory.mkdir(parents=True, exist_ok=True)
    log = directory / "daemon.log"
    lf = log.open("wb")
    p = subprocess.Popen(
        [
            bin_path,
            "--enable-rpc",
            "--rpc-listen-all=true",
            "--rpc-allow-origin-all",
            f"--rpc-listen-port={str(port)}",
            f"--dir={directory}",
            "--file-allocation=none",
            "--console-log-level=error",
        ],
        stdout=lf,
        stderr=subprocess.STDOUT,
    )
    for _ in range(50):
        try:
            rpc(port, "aria2.getVersion", [])
            return p
        except (urllib.error.URLError, TimeoutError, OSError):
            time.sleep(0.1)
    p.terminate()
    raise RuntimeError(f"rpc {port} did not start")


def main() -> int:
    cpp = os.environ.get("ARIA2_CPP", "/usr/bin/aria2c")
    rust = os.environ.get("ARIA2_RUST", "")
    if not rust or not Path(cpp).is_file() or not Path(rust).is_file():
        print("skip: set ARIA2_CPP and ARIA2_RUST", file=sys.stderr)
        return 0
    body = bytes(range(256)) * 256  # 64KiB
    expect = hashlib.sha256(body).hexdigest()
    failed: list[str] = []
    ok: list[str] = []

    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        httpd = serve_http(body)
        http_url = f"http://127.0.0.1:{httpd.server_address[1]}/blob.bin"

        # HTTPS
        cert = root / "c.pem"
        key = root / "k.pem"
        subprocess.check_call(
            [
                "openssl",
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-keyout",
                str(key),
                "-out",
                str(cert),
                "-days",
                "1",
                "-nodes",
                "-subj",
                "/CN=127.0.0.1",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        httpsd = serve_http(body, (str(cert), str(key)))
        https_url = f"https://127.0.0.1:{httpsd.server_address[1]}/blob.bin"
        try:
            fetch(
                cpp,
                root / "https-cpp",
                ["--check-certificate=false", "--split=1"],
                https_url,
            )
            fetch(
                rust,
                root / "https-rust",
                ["--check-certificate=false", "--split=1"],
                https_url,
            )
            if (
                sha256_file(root / "https-cpp" / "blob.bin") == expect
                and sha256_file(root / "https-rust" / "blob.bin") == expect
            ):
                ok.append("HTTPS")
            else:
                failed.append("HTTPS sha")
        except Exception as e:
            failed.append(f"HTTPS {e}")

        # Metalink
        sha1 = hashlib.sha1(body).hexdigest()
        meta = root / "gold.meta4"
        meta.write_text(
            f"""<?xml version="1.0" encoding="UTF-8"?>
<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="blob.bin">
    <size>{len(body)}</size>
    <hash type="sha-1">{sha1}</hash>
    <url>{http_url}</url>
  </file>
</metalink>
""",
            encoding="utf-8",
        )
        try:
            fetch(cpp, root / "ml-cpp", ["--metalink-file=" + str(meta)])
            fetch(rust, root / "ml-rust", ["--metalink-file=" + str(meta)])
            if (
                sha256_file(root / "ml-cpp" / "blob.bin") == expect
                and sha256_file(root / "ml-rust" / "blob.bin") == expect
            ):
                ok.append("Metalink")
            else:
                failed.append("Metalink sha")
        except Exception as e:
            failed.append(f"Metalink {e}")

        # FTP PASV
        ftp_port, ftp_stop = start_ftp(body, "blob.bin")
        ftp_url = f"ftp://gold:gold@127.0.0.1:{ftp_port}/blob.bin"
        try:
            fetch(cpp, root / "ftp-cpp", ["--ftp-pasv=true"], ftp_url)
            fetch(rust, root / "ftp-rust", ["--ftp-pasv=true"], ftp_url)
            if (
                sha256_file(root / "ftp-cpp" / "blob.bin") == expect
                and sha256_file(root / "ftp-rust" / "blob.bin") == expect
            ):
                ok.append("FTP")
            else:
                failed.append("FTP sha")
        except Exception as e:
            failed.append(f"FTP {e}")
        ftp_stop.set()

        # JSON-RPC addUri
        cpp_rpc = rust_rpc = None
        try:
            cpp_rpc = run_daemon(cpp, 16800, root / "rpc-cpp")
            rust_rpc = run_daemon(rust, 16801, root / "rpc-rust")
            gid_c = rpc(16800, "aria2.addUri", [[http_url], {"out": "blob.bin"}])
            gid_r = rpc(16801, "aria2.addUri", [[http_url], {"out": "blob.bin"}])
            wait_rpc_complete(16800, str(gid_c))
            wait_rpc_complete(16801, str(gid_r))
            if (
                sha256_file(root / "rpc-cpp" / "blob.bin") == expect
                and sha256_file(root / "rpc-rust" / "blob.bin") == expect
            ):
                ok.append("JSON-RPC addUri")
            else:
                failed.append("JSON-RPC sha")
        except Exception as e:
            failed.append(f"JSON-RPC {e}")
        finally:
            for p in (cpp_rpc, rust_rpc):
                if p is not None:
                    p.terminate()
                    try:
                        p.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        p.kill()

        httpd.shutdown()
        httpsd.shutdown()

    print("GOLD PROTOCOLS ok=", ",".join(ok) or "-", "fail=", ",".join(failed) or "-")
    print("sha", expect, "bytes", len(body))
    return 0 if not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
