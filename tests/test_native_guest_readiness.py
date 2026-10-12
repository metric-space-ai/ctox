# Origin: CTOX
# License: AGPL-3.0-only
import importlib.util
import json
from pathlib import Path
import socket
import struct
import tempfile
import time
import unittest
import zlib

spec = importlib.util.spec_from_file_location("readiness", Path(__file__).with_name("native_guest_readiness.py"))
readiness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(readiness)


def chunk(kind, payload):
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload))


def png():
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\0\0\0\0")) + chunk(b"IEND", b""))


class ProtocolBounds(unittest.TestCase):
    def wire(self, payload):
        left, right = socket.socketpair()
        self.addCleanup(left.close); self.addCleanup(right.close)
        left.sendall(payload)
        left.shutdown(socket.SHUT_WR)
        return right

    def test_complete_frame_and_eof(self):
        stream = self.wire(b"\0\0\0\3abc")
        self.assertEqual(readiness.frame(stream, 3, time.monotonic() + 1), b"abc")
        stream = self.wire(b"\0\0\0\4ab")
        with self.assertRaises(EOFError):
            readiness.frame(stream, 4, time.monotonic() + 1)

    def test_header_rejects_unbounded_size_before_payload(self):
        for size in (0, readiness.PNG_LIMIT + 1, 0xffffffff):
            with self.subTest(size=size), self.assertRaisesRegex(ValueError, "bound"):
                readiness.frame(self.wire(struct.pack(">I", size)), readiness.PNG_LIMIT, time.monotonic()+1)

    def test_reply_requires_kind_id_and_exact_fields(self):
        good = {"kind": "failed", "id": 3}
        bad = [dict(good, id=True), dict(good, id=4), dict(good, kind="applied"),
               dict(good, session_id="foreign"), [], {"kind": "failed"}]
        for value in [good] + bad:
            payload = json.dumps(value).encode()
            stream = self.wire(struct.pack(">I", len(payload)) + payload)
            if value is good:
                self.assertEqual(readiness.reply(stream, "failed", 3, time.monotonic()+1), good)
            else:
                with self.subTest(value=value), self.assertRaises(ValueError):
                    readiness.reply(stream, "failed", 3, time.monotonic()+1)

    def test_png_integrity_completeness_and_dimensions(self):
        valid = png()
        self.assertEqual(readiness.png_dimensions(valid), (1, 1))
        for invalid in (valid[:-1], valid + b"junk", valid[:33], valid[:10] + b"X" + valid[11:]):
            with self.subTest(length=len(invalid)), self.assertRaises(ValueError):
                readiness.png_dimensions(invalid)
        oversized = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 8193, 1, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", b"data") + chunk(b"IEND", b""))
        with self.assertRaisesRegex(ValueError, "dimensions"):
            readiness.png_dimensions(oversized)

    def test_immutable_master_aliases_and_permissions_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            base = root / "base.raw"
            base.write_bytes(b"\0" * 512)
            base.chmod(0o400)
            readiness.regular(base, immutable=True)
            alias = root / "alias.raw"
            alias.symlink_to(base)
            with self.assertRaisesRegex(ValueError, "nonsymlink"):
                readiness.regular(alias, immutable=True)
            alias.unlink(); alias.hardlink_to(base)
            with self.assertRaisesRegex(ValueError, "single-link"):
                readiness.regular(base, immutable=True)
            alias.unlink(); base.chmod(0o600)
            with self.assertRaisesRegex(ValueError, "immutable"):
                readiness.regular(base, immutable=True)

    def test_offline_profile_explicit_backing_and_no_input(self):
        args = readiness.qemu_args(Path("/usr/bin/qemu-system-x86_64"), Path("/base.raw"),
                                   Path("/overlay.qcow2"), Path("/private"), "kvm")
        self.assertEqual(args[args.index("-nic") + 1], "none")
        self.assertIn("-S", args)
        self.assertIn("pc-i440fx-5.1,kernel-irqchip=off", args)
        self.assertIn("qemu64-v1,kvm=off,kvmclock=off,svm=off", args)
        blocks = [json.loads(args[i + 1]) for i, arg in enumerate(args) if arg == "-blockdev"]
        self.assertTrue(blocks[0]["read-only"])
        self.assertEqual(blocks[1]["backing"], "guest-base")
        tcg = readiness.qemu_args(Path("/qemu"), Path("/base"), Path("/overlay"), Path("/private"), "tcg")
        self.assertIn("tcg,thread=single", tcg)
        self.assertNotIn("kvm", tcg)


if __name__ == "__main__":
    unittest.main()
