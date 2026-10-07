#!/usr/bin/env python3
"""Exercise real framed capture replies; never retry input or broken transport."""
import ast
import hashlib
import json
import socket
import struct
import tempfile
import threading
import time
import unittest
from pathlib import Path

HELPER = Path(__file__).with_name("verify-guest.py")
TREE = ast.parse(HELPER.read_text())
TOP = [n for n in TREE.body if isinstance(n, ast.FunctionDef) and n.name in ("exact", "packet", "request")]
CAPTURE = next(n for n in ast.walk(TREE) if isinstance(n, ast.FunctionDef) and n.name == "capture")
PNG = bytes.fromhex("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
                    "0000000b49444154789c636000020000050001a5f645400000000049454e44ae426082")

class CaptureWireTests(unittest.TestCase):
    def exercise(self, replies, restored=True, expected=None, timeout=10):
        client, server = socket.socketpair()
        client.settimeout(timeout)
        server.settimeout(10)
        seen, assertions, failures = [], [], []
        namespace = dict(json=json, struct=struct, hashlib=hashlib, time=time,
                         guest=client, source={"verified": True} if restored else None,
                         session_id="same-owned-session",
                         assertion=lambda event, **fields: assertions.append((event, fields)))
        exec(compile(ast.Module(body=TOP + [CAPTURE], type_ignores=[]), str(HELPER), "exec"), namespace)
        def peer():
            try:
                for kind in replies:
                    value = json.loads(namespace["packet"](server))
                    seen.append(value)
                    if kind == "eof":
                        return
                    if kind == "timeout":
                        time.sleep(0.2)
                        return
                    reply = dict(kind="failed" if kind in ("failed", "wrong-id") else kind,
                                 id=999 if kind == "wrong-id" else value["id"])
                    if kind in ("observation", "bad-png", "bad-dimensions"):
                        reply.update(kind="observation", width=2 if kind == "bad-dimensions" else 1, height=1)
                    payload = json.dumps(reply).encode()
                    server.sendall(struct.pack(">I", len(payload)) + payload)
                    if reply["kind"] == "observation":
                        data = b"malformed" if kind == "bad-png" else PNG
                        server.sendall(struct.pack(">I", len(data)) + data)
            except BaseException as error:
                failures.append(error)
            finally:
                server.close()
        thread = threading.Thread(target=peer)
        with tempfile.TemporaryDirectory(prefix="guest-capture-wire-") as directory:
            namespace["out"] = Path(directory)
            thread.start()
            try:
                if expected:
                    with self.assertRaises(expected):
                        namespace["capture"](2, "actual.png")
                    self.assertFalse((Path(directory) / "actual.png").exists())
                else:
                    namespace["capture"](2, "actual.png")
                    self.assertEqual((Path(directory) / "actual.png").read_bytes(), PNG)
            finally:
                client.close()
                thread.join(timeout=10)
                self.assertFalse(thread.is_alive(), "wire peer must terminate")
        self.assertEqual(failures, [])
        self.assertTrue(all(v["kind"] == "observe_session" and
                            v["session_id"] == "same-owned-session" for v in seen))
        return seen, assertions

    def test_complete_failure_can_recover_with_fresh_id(self):
        seen, events = self.exercise(["failed", "observation"])
        self.assertEqual([v["id"] for v in seen], [2, 1002])
        self.assertEqual([e for e, _ in events], ["native_capture_retry", "fresh_same_session_capture"])

    def test_persistent_failure_is_bounded(self):
        seen, events = self.exercise(["failed"] * 3, expected=ValueError)
        self.assertEqual([v["id"] for v in seen], [2, 1002, 2002])
        self.assertEqual([e for e, _ in events], ["native_capture_retry"] * 2)

    def test_fresh_guest_failure_is_not_retried(self):
        seen, events = self.exercise(["failed"], restored=False, expected=ValueError)
        self.assertEqual(len(seen), 1)
        self.assertEqual(events, [])

    def test_broken_or_uncorrelated_replies_are_not_retried(self):
        for kind, error in [("wrong-id", ValueError), ("eof", EOFError),
                            ("timeout", TimeoutError), ("bad-png", ValueError),
                            ("bad-dimensions", ValueError), ("unknown", ValueError)]:
            with self.subTest(kind=kind):
                seen, events = self.exercise([kind], expected=error, timeout=0.05 if kind == "timeout" else 10)
                self.assertEqual(len(seen), 1)
                self.assertEqual(events, [])

if __name__ == "__main__":
    unittest.main()
