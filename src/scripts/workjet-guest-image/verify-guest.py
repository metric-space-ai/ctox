#!/usr/bin/env python3
"""Bounded isolated-image component probe; NOT product enrollment/P2P acceptance."""
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
import time
import uuid

def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for part in iter(lambda: f.read(1024 * 1024), b""):
            h.update(part)
    return h.hexdigest()

def exact(sock, size):
    out = bytearray()
    while len(out) < size:
        chunk = sock.recv(size - len(out))
        if not chunk:
            raise EOFError("owned guest endpoint closed")
        out.extend(chunk)
    return bytes(out)

def packet(sock, limit=65536):
    size = struct.unpack(">I", exact(sock, 4))[0]
    if not 0 < size <= limit:
        raise ValueError("untrusted guest frame length")
    return exact(sock, size)

def request(sock, value):
    data = json.dumps(value, separators=(",", ":")).encode()
    if len(data) > 65536:
        raise ValueError("request exceeds the native wire limit")
    sock.sendall(struct.pack(">I", len(data)) + data)
    reply = json.loads(packet(sock))
    if reply.get("id") != value["id"]:
        raise ValueError("uncorrelated guest response")
    return reply

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--base", type=Path, required=True)
    p.add_argument("--sha256", required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--input-json", type=Path)
    p.add_argument("--accel", choices=("kvm", "tcg"), default="kvm",
                   help="Explicit accelerator; never falls back from KVM to TCG")
    args = p.parse_args()
    base = args.base
    meta = base.lstat()
    if not base.is_absolute() or not stat.S_ISREG(meta.st_mode):
        raise ValueError("base must be an absolute regular standalone RAW file")
    if meta.st_size == 0 or meta.st_size % 512 or meta.st_mode & 0o222:
        raise ValueError("base must be sector aligned and read only")
    if digest(base) != args.sha256:
        raise ValueError("actual frozen base identity differs")
    os.umask(0o077)
    args.out.mkdir(mode=0o700, parents=False, exist_ok=False)
    out = args.out.resolve()
    if any(c in str(out) for c in ",\n\r"):
        raise ValueError("private QEMU socket paths cannot contain keyval delimiters")
    guest_id = "o04-" + args.accel + "-" + str(uuid.uuid4())
    receipt = dict(schema="ctox.guest_image_component_probe.v1", scope="isolated image/endpoint only",
                   owner_thread="01a0879f-fa04-7a72-a9be-f471c2df5471",
                   guest_id=guest_id, acceleration=args.accel,
                   base_sha256=args.sha256, started=time.time(),
                   product_enrollment_p2p_restore_accepted=False, assertions=[])
    def interrupted(number, _frame):
        raise TimeoutError("component probe interrupted or deadline reached: " + str(number))
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGALRM, interrupted)
    proc = None
    listeners = []
    streams = []
    logs = None
    def save():
        (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    def assertion(name, **detail):
        receipt["assertions"].append(dict(name=name, **detail))
        save()
    def listener(name):
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.bind(str(out / name))
        s.listen(1)
        s.settimeout(15)
        listeners.append(s)
        return s
    try:
        subprocess.run(["/usr/bin/qemu-img", "create", "-f", "qcow2", "-F", "raw", "-b",
                        str(base), str(out / "root.qcow2")], check=True, timeout=20)
        startup = out / "guest-startup.json"
        startup.write_text(json.dumps(dict(guest_id=guest_id, display=":0",
                                          xauthority="/run/ctox-desktop/Xauthority")))
        qmp_listener, guest_listener = listener("qmp.sock"), listener("guest.sock")
        command = ["/usr/bin/qemu-system-x86_64", "-machine", "pc", "-accel",
                   "kvm" if args.accel == "kvm" else "tcg,thread=single",
                   "-m", "4096", "-smp", "2", "-fw_cfg",
                   "name=opt/org.ctox/guest-startup,file=" + str(startup),
                   "-nodefaults", "-no-user-config", "-display", "none",
                   "-serial", "file:" + str(out / "boot-serial.log"),
                   "-monitor", "none", "-nic", "none", "-sandbox",
                   "on,obsolete=deny,elevateprivileges=deny,spawn=deny,resourcecontrol=deny", "-S",
                   "-blockdev", json.dumps(dict(driver="raw", **{"node-name":"guest-base", "read-only":True},
                        file=dict(driver="file", filename=str(base), **{"read-only":True, "locking":"on"}))),
                   "-blockdev", json.dumps(dict(driver="qcow2", **{"node-name":"guest-root", "backing":"guest-base"},
                        file=dict(driver="file", filename=str(out / "root.qcow2"), locking="on"))),
                   "-device", "virtio-blk-pci,drive=guest-root", "-device", "virtio-vga",
                   "-device", "virtio-serial", "-chardev",
                   "socket,id=control,path=" + str(out / "qmp.sock") + ",server=off",
                   "-mon", "chardev=control,mode=control", "-chardev",
                   "socket,id=guestctl,path=" + str(out / "guest.sock") + ",server=off",
                   "-device", "virtserialport,chardev=guestctl,name=org.ctox.guest.desktop"]
        logs = (out / "qemu.stderr").open("wb")
        proc = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                stderr=logs, env={})
        signal.alarm(600)
        receipt.update(pid=proc.pid, argv=command, stop_condition="maximum 600 seconds; always reap owned QEMU")
        save()
        accepted = []
        for l in [qmp_listener, guest_listener]:
            conn, _ = l.accept()
            streams.append(conn)
            peer_pid, peer_uid, peer_gid = struct.unpack("3i", conn.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
            if (peer_pid, peer_uid) != (proc.pid, os.geteuid()):
                raise ValueError("socket peer differs from the actual owned QEMU child")
            accepted.append(dict(pid=peer_pid, uid=peer_uid, gid=peer_gid))
        assertion("actual_owned_socket_peers", peers=accepted)
        qmp, guest = streams
        qmp.settimeout(15)
        f = qmp.makefile("rb")
        greeting = json.loads(f.readline(65536))
        if "QMP" not in greeting:
            raise ValueError("invalid QMP greeting")
        counter = 0
        def monitor(execute, arguments=None):
            nonlocal counter
            counter += 1
            command = dict(execute=execute, id=counter)
            if arguments is not None:
                command["arguments"] = arguments
            qmp.sendall(json.dumps(command).encode() + b"\n")
            for _ in range(50):
                line = f.readline(65536)
                if not line:
                    raise EOFError("owned QMP closed")
                value = json.loads(line)
                if value.get("id") == counter:
                    if "error" in value:
                        raise ValueError(value["error"])
                    return value["return"]
            raise ValueError("QMP event bound exceeded")
        monitor("qmp_capabilities")
        status = monitor("query-status")
        if status["running"]:
            raise ValueError("guest did not start paused")
        assertion("fresh_guest_initially_paused", status=status)
        start = time.monotonic()
        monitor("cont")
        guest.settimeout(120)
        endpoint = request(guest, dict(kind="probe", id=1))
        session_id = endpoint.get("session_id", "")
        if endpoint.get("kind") != "endpoint" or endpoint.get("guest_id") != guest_id or str(uuid.UUID(session_id)) != session_id:
            raise ValueError("native endpoint identity/session differs")
        assertion("actual_guest_endpoint", session_id=session_id, boot_seconds=time.monotonic()-start)
        guest.settimeout(10)
        def capture(i, name):
            reply = request(guest, dict(kind="observe_session", id=i, session_id=session_id))
            if reply.get("kind") != "observation":
                raise ValueError("real Xorg capture failed")
            png = packet(guest, 16 * 1024 * 1024)
            if len(png) < 33 or png[:8] != b"\x89PNG\r\n\x1a\n" or png[12:16] != b"IHDR":
                raise ValueError("invalid actual captured PNG")
            w, h = struct.unpack(">II", png[16:24])
            if (w,h) != (reply["width"],reply["height"]) or not (0 < w <= 4096 and 0 < h <= 4096 and w*h <= 8388608):
                raise ValueError("captured dimensions differ")
            (out / name).write_bytes(png)
            assertion("fresh_same_session_capture", file=name, width=w, height=h, sha256=hashlib.sha256(png).hexdigest())
        capture(2, "capture-before.png")
        # XFCE can still be painting when the X11 endpoint first answers.
        # Retain that first frame and inspect a second actual capture after a
        # bounded cold-start settling period; neither proves a usable desktop.
        time.sleep(10)
        capture(3, "capture-desktop.png")
        bad = request(guest, dict(kind="input_session", id=4, session_id=str(uuid.uuid4()),
                                 input=dict(kind="type", text="INVALID_SESSION_MUST_NOT_APPLY")))
        if bad.get("kind") != "failed":
            raise ValueError("stale guest session admitted input")
        assertion("wrong_guest_session_input_denied")
        if args.input_json:
            actions = json.loads(args.input_json.read_text())
            if not isinstance(actions, list) or not 0 < len(actions) <= 30:
                raise ValueError("bounded native action list required")
            for i, action in enumerate(actions, 5):
                reply = request(guest, dict(kind="input_session", id=i, session_id=session_id, input=action))
                if reply.get("kind") != "applied":
                    raise ValueError("requested component input failed")
            capture(40, "capture-after.png")
        final = request(guest, dict(kind="probe", id=41))
        if final != dict(kind="endpoint", id=41, guest_id=guest_id, session_id=session_id):
            raise ValueError("guest session changed during component test")
        assertion("session_stable_after_capture_and_input")
        monitor("system_powerdown")
        try:
            code = proc.wait(timeout=45)
            assertion("guest_acpi_powerdown", exit=code)
            receipt["guest_graceful_powerdown"] = code == 0
            if code != 0:
                raise RuntimeError("owned QEMU exited nonzero after ACPI powerdown: " + str(code))
        except subprocess.TimeoutExpired:
            receipt["guest_graceful_powerdown"] = False
            raise RuntimeError("ACPI powerdown did not complete; no consistent checkpoint claimed")
        receipt["status"] = "component_passed"
    except BaseException as e:
        receipt.update(status="failed", error=str(e))
        # Failure diagnostics have a separate short deadline; they must not
        # extend a timed-out probe while the owned QEMU waits to be reaped.
        signal.alarm(5)
        if proc and proc.poll() is None and "monitor" in locals():
            try:
                monitor("screendump", dict(filename=str(out / "boot-display.ppm")))
                receipt["diagnostic_boot_display"] = "boot-display.ppm"
            except Exception as diagnostic_error:
                receipt["diagnostic_error"] = str(diagnostic_error)
        raise
    finally:
        signal.alarm(0)
        for s in streams + listeners:
            s.close()
        if proc and proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=10)
        if logs:
            logs.close()
        receipt.update(finished=time.time(), owned_qemu_reaped=proc is None or proc.poll() is not None)
        save()
    print(json.dumps(receipt))

if __name__ == "__main__":
    main()
