#!/usr/bin/env python3
# Origin: CTOX
# License: AGPL-3.0-only
"""Bounded, offline readiness measurement of an installed Linux guest image.

This component check has no Business OS caller, controller or quorum authority.
It never accepts a checkpoint, sends input or claims production goal19 passed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import stat
import struct
import subprocess
import tempfile
import time
import uuid
import zlib

CONTROL_LIMIT = 65536
PNG_LIMIT = 16 * 1024 * 1024
PROFILE = "ctox.pc-i440fx-5.1.qemu64-v1.v1"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def metadata(path):
    s = path.lstat()
    return {"dev": s.st_dev, "ino": s.st_ino, "uid": s.st_uid,
            "mode": stat.S_IMODE(s.st_mode), "nlink": s.st_nlink,
            "bytes": s.st_size, "mtime_ns": str(s.st_mtime_ns),
            "ctime_ns": str(s.st_ctime_ns)}


def regular(path, immutable=False):
    require(path.is_absolute() and path.resolve(strict=True) == path,
            "path must be absolute, canonical and nonsymlink")
    s = path.lstat()
    require(stat.S_ISREG(s.st_mode) and s.st_uid in (0, os.geteuid()),
            "file must be regular and owned by root or the native user")
    require(s.st_mode & 0o022 == 0, "file permits foreign writes")
    if immutable:
        require(s.st_mode & 0o222 == 0 and s.st_nlink == 1,
                "base must be immutable and single-link")
        require(0 < s.st_size <= 16 * 1024**3 and s.st_size % 512 == 0,
                "base RAW size is invalid")
    else:
        require(os.access(path, os.X_OK), "program is not executable")


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def remaining(deadline):
    value = deadline - time.monotonic()
    if value <= 0:
        raise TimeoutError("guest readiness deadline exceeded")
    return value


def exact(stream, count, deadline):
    data = bytearray()
    while len(data) < count:
        stream.settimeout(remaining(deadline))
        chunk = stream.recv(count - len(data))
        if not chunk:
            raise EOFError("guest closed an incomplete frame")
        data.extend(chunk)
    return bytes(data)


def frame(stream, limit, deadline):
    size = struct.unpack(">I", exact(stream, 4, deadline))[0]
    require(0 < size <= limit, "untrusted guest frame exceeds its bound")
    return exact(stream, size, deadline)


def send(stream, value, deadline):
    payload = json.dumps(value, separators=(",", ":")).encode()
    require(0 < len(payload) <= CONTROL_LIMIT, "control frame exceeds its bound")
    stream.settimeout(remaining(deadline))
    stream.sendall(struct.pack(">I", len(payload)) + payload)


def reply(stream, expected, request_id, deadline):
    value = json.loads(frame(stream, CONTROL_LIMIT, deadline))
    require(isinstance(value, dict) and value.get("kind") == expected
            and type(value.get("id")) is int and value["id"] == request_id,
            "guest reply kind/request correlation mismatch")
    fields = {"endpoint": {"kind", "id", "guest_id", "session_id"},
              "observation": {"kind", "id", "width", "height"},
              "failed": {"kind", "id"}}
    require(set(value) == fields[expected], "unexpected guest reply fields")
    return value


def png_dimensions(data):
    require(0 < len(data) <= PNG_LIMIT and data[:8] == b"\x89PNG\r\n\x1a\n",
            "invalid PNG signature or size")
    offset, dimensions, idat = 8, None, False
    while offset < len(data):
        require(offset + 12 <= len(data), "truncated PNG chunk")
        size = struct.unpack(">I", data[offset:offset+4])[0]
        end = offset + 12 + size
        require(end <= len(data), "truncated PNG payload")
        kind = data[offset+4:offset+8]
        payload = data[offset+8:end-4]
        crc = struct.unpack(">I", data[end-4:end])[0]
        require(zlib.crc32(kind + payload) == crc, "PNG chunk CRC mismatch")
        if offset == 8:
            require(kind == b"IHDR" and size == 13, "PNG must begin with IHDR")
            dimensions = struct.unpack(">II", payload[:8])
            require(all(0 < n <= 8192 for n in dimensions), "PNG dimensions invalid")
        elif kind == b"IHDR":
            raise ValueError("duplicate PNG IHDR")
        idat |= kind == b"IDAT" and size > 0
        if kind == b"IEND":
            require(size == 0 and end == len(data) and idat, "PNG ending invalid")
            return dimensions
        offset = end
    raise ValueError("PNG has no IEND")


class Qmp:
    def __init__(self, stream, deadline):
        self.stream, self.deadline, self.sequence = stream, deadline, 0
        require("QMP" in self.read(), "missing actual QMP greeting")
        self.call("qmp_capabilities")

    def read(self):
        value = bytearray()
        while not value.endswith(b"\n"):
            require(len(value) < CONTROL_LIMIT, "QMP reply oversized")
            value.extend(exact(self.stream, 1, self.deadline))
        return json.loads(value)

    def call(self, command):
        self.sequence += 1
        request = {"execute": command, "id": self.sequence}
        self.stream.settimeout(remaining(self.deadline))
        self.stream.sendall(json.dumps(request).encode() + b"\n")
        for _ in range(128):
            response = self.read()
            if "event" in response:
                continue
            require(response.get("id") == self.sequence and "return" in response,
                    "QMP command failed or response is uncorrelated")
            return response["return"]
        raise ValueError("QMP event bound exceeded")


def accept_owned(listener, process, deadline):
    listener.settimeout(remaining(deadline))
    stream, _ = listener.accept()
    pid, uid, _ = struct.unpack("3i", stream.getsockopt(
        socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("3i")))
    if pid != process.pid or uid != os.geteuid():
        stream.close()
        raise ValueError("socket peer is not the exact owned QEMU child")
    return stream


def qemu_args(program, base, overlay, root, acceleration):
    # Same pinned device profile and explicit backing nodes as qemu.rs.
    args = [str(program), "-machine", "pc-i440fx-5.1" +
            (",kernel-irqchip=off" if acceleration == "kvm" else ""),
            "-cpu", "qemu64-v1,kvm=off,kvmclock=off,svm=off",
            "-accel", "kvm" if acceleration == "kvm" else "tcg,thread=single",
            "-m", "768", "-smp", "2", "-fw_cfg",
            "name=opt/org.ctox/guest-startup,file=" + str(root / "startup.json"),
            "-nodefaults", "-no-user-config", "-display", "none", "-serial", "none",
            "-monitor", "none", "-nic", "none", "-sandbox",
            "on,obsolete=deny,elevateprivileges=deny,spawn=deny,resourcecontrol=deny", "-S"]
    nodes = [{"driver": "raw", "node-name": "guest-base", "read-only": True,
              "file": {"driver": "file", "filename": str(base), "read-only": True,
                       "locking": "on"}},
             {"driver": "qcow2", "node-name": "guest-root", "backing": "guest-base",
              "file": {"driver": "file", "filename": str(overlay), "locking": "on"}}]
    for node in nodes:
        args.extend(["-blockdev", json.dumps(node)])
    args.extend(["-device", "virtio-blk-pci,drive=guest-root", "-device", "virtio-vga",
                 "-device", "virtio-serial", "-chardev",
                 "socket,id=control,path=" + str(root / "q.sock") + ",server=off",
                 "-mon", "chardev=control,mode=control", "-chardev",
                 "socket,id=guestctl,path=" + str(root / "g.sock") + ",server=off",
                 "-device", "virtserialport,chardev=guestctl,name=org.ctox.guest.desktop"])
    return args


def run(args):
    require(os.name == "posix" and hasattr(socket, "SO_PEERCRED"), "Linux required")
    require(len(args.base_sha256) == 64 and all(c in "0123456789abcdef"
            for c in args.base_sha256), "full expected base SHA256 required")
    regular(args.base, immutable=True)
    regular(args.program)
    image_program = args.program.with_name("qemu-img")
    regular(image_program)
    for directory in (args.work_parent, args.output):
        require(directory.is_absolute() and directory.resolve(strict=True) == directory,
                "directory must be canonical and nonsymlink")
        s = directory.lstat()
        require(stat.S_ISDIR(s.st_mode) and s.st_uid == os.geteuid()
                and stat.S_IMODE(s.st_mode) == 0o700, "directory must be native-owned0700")
    require(not any(args.output.iterdir()), "output must be a fresh empty directory")
    receipt = {"goal": 19, "component": "installed_guest_image_readiness",
               "pass": False, "goal19_pass": False, "production_authority": False,
               "restore_measured": False, "revocation_measured": False,
               "two_host_measured": False, "profile": PROFILE,
               "base": str(args.base), "expected_base_sha256": args.base_sha256,
               "embedded_native_source": args.embedded_native_source,
               "owner": args.owner, "steps": [], "qemu_pid": None}
    before = metadata(args.base)
    started = time.monotonic()
    process, monitor = None, None
    deadline = started + args.timeout
    old_term = signal.signal(signal.SIGTERM,
                             lambda *_: (_ for _ in ()).throw(InterruptedError("terminated")))
    try:
        require(digest(args.base) == args.base_sha256, "full base SHA256 mismatch")
        receipt["steps"].append("full immutable RAW SHA256 verified")
        with tempfile.TemporaryDirectory(prefix="g19-", dir=args.work_parent) as temp:
            root = Path(temp)
            require(len(os.fsencode(root / "q.sock")) < 104
                    and not any(c in str(root) for c in ",\n\r"), "unsafe socket path")
            overlay = root / "root.qcow2"
            subprocess.run([str(image_program), "create", "-f", "qcow2", "-u", "-F", "raw",
                            "-b", "ctox-native-base", str(overlay), str(before["bytes"])],
                           check=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=min(10, remaining(deadline)), env={})
            overlay.chmod(0o600)
            guest_id = "readiness-" + str(uuid.uuid4())
            startup = root / "startup.json"
            with startup.open("x") as out:
                startup.chmod(0o600)
                json.dump({"guest_id": guest_id, "display": ":0",
                           "xauthority": "/run/ctox-desktop/Xauthority"}, out)
            with socket.socket(socket.AF_UNIX) as ql, socket.socket(socket.AF_UNIX) as gl:
                ql.bind(str(root / "q.sock")); gl.bind(str(root / "g.sock"))
                ql.listen(1); gl.listen(1)
                with (args.output / "qemu.log").open("xb") as log:
                    process = subprocess.Popen(qemu_args(args.program, args.base, overlay, root,
                                                args.acceleration), stdin=subprocess.DEVNULL,
                                               stdout=log, stderr=log, env={}, close_fds=True)
                    receipt["qemu_pid"] = process.pid
                    receipt["process_group"] = os.getpgid(process.pid)
                    receipt["stop_condition"] = "deadline or first error; exact owned child quit/terminate/kill and wait"
                    # Record owner/PID before any potentially long accept/boot.
                    (args.output / "process-owner.json").write_text(json.dumps(receipt, indent=2))
                    try:
                        with accept_owned(ql, process, deadline) as qs, accept_owned(gl, process, deadline) as gs:
                            monitor = Qmp(qs, deadline)
                            require(monitor.call("query-status")["running"] is False,
                                    "QEMU did not start paused")
                            monitor.call("cont")
                            send(gs, {"kind": "probe", "id": 1}, deadline)
                            endpoint = reply(gs, "endpoint", 1, deadline)
                            require(endpoint["guest_id"] == guest_id, "guest assignment mismatch")
                            session = endpoint["session_id"]
                            require(isinstance(session, str) and str(uuid.UUID(session)) == session,
                                    "guest service session is not canonical UUID")
                            receipt["ready_seconds"] = round(time.monotonic() - started, 3)
                            receipt["service_session_id"] = session
                            receipt["guest_id"] = guest_id
                            send(gs, {"kind": "observe_session", "id": 2, "session_id": session}, deadline)
                            observation = reply(gs, "observation", 2, deadline)
                            png = frame(gs, PNG_LIMIT, deadline)
                            dimensions = png_dimensions(png)
                            require(all(type(observation[k]) is int for k in ("width", "height"))
                                    and dimensions == (observation["width"], observation["height"]),
                                    "PNG and advertised dimensions differ")
                            (args.output / "guest.png").write_bytes(png)
                            receipt["frame"] = {"bytes": len(png), "sha256": hashlib.sha256(png).hexdigest(),
                                                "width": dimensions[0], "height": dimensions[1]}
                            stale = str(uuid.uuid4())
                            send(gs, {"kind": "observe_session", "id": 3, "session_id": stale}, deadline)
                            reply(gs, "failed", 3, deadline)
                            send(gs, {"kind": "probe", "id": 4}, deadline)
                            require(reply(gs, "endpoint", 4, deadline) == dict(endpoint, id=4),
                                    "guest service restarted or changed identity")
                            receipt["stale_service_session_rejected"] = True
                            receipt["steps"].extend(["actual guest assignment/session probe",
                                "session-bound real X11 PNG", "foreign service-session observation rejected",
                                "same guest/session retained after rejection"])
                            monitor.call("quit")
                        require(process.wait(timeout=10) == 0, "owned QEMU did not exit cleanly")
                        receipt["clean_qemu_exit"] = True
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            try:
                                process.wait(timeout=5)
                            except subprocess.TimeoutExpired:
                                process.kill(); process.wait(timeout=5)
                        receipt["qemu_reaped"] = process.poll() is not None
                        receipt["qemu_exit"] = process.returncode
            receipt["temporary_runtime_removed"] = True
        receipt["base_metadata_unchanged"] = metadata(args.base) == before
        require(receipt["base_metadata_unchanged"], "immutable base metadata changed")
        receipt["pass"] = True
    except Exception as error:
        receipt["error"] = type(error).__name__ + ": " + str(error)
    finally:
        signal.signal(signal.SIGTERM, old_term)
        receipt["elapsed_seconds"] = round(time.monotonic() - started, 3)
        receipt["evidence_level"] = "Installed guest image/QEMU/virtio/X11 component only; no native account/controller/quorum or restore/two-host proof"
        (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt), flush=True)
    return 0 if receipt["pass"] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--base-sha256", required=True)
    parser.add_argument("--embedded-native-source", required=True)
    parser.add_argument("--owner", required=True)
    parser.add_argument("--work-parent", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--program", type=Path, default=Path("/usr/bin/qemu-system-x86_64"))
    parser.add_argument("--acceleration", choices=("kvm", "tcg"), required=True)
    parser.add_argument("--timeout", type=int, choices=range(30, 301), default=180)
    return run(parser.parse_args())


if __name__ == "__main__":
    raise SystemExit(main())
