#!/usr/bin/env python3
"""Bounded isolated-image component probe; NOT product enrollment/P2P acceptance."""
import argparse
import array
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

# pc >= 5.2 always creates KVM's nonportable clock device, even when its
# CPU feature is hidden. Use a common versioned device set and userspace IRQ
# chip before the source boots; never strip a section from saved VM state.
# ref: qemu v6.2.0 hw/i386/pc_piix.c, hw/i386/kvm/clock.c
CHECKPOINT_MACHINE = "pc-i440fx-5.1"
CHECKPOINT_CPU = "qemu64-v1,kvm=off,kvmclock=off,svm=off"

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
    mode = p.add_mutually_exclusive_group()
    mode.add_argument("--checkpoint", action="store_true",
                      help="KVM component: retain coherent RAM/devices/disk and quit the source")
    mode.add_argument("--restore-from", type=Path,
                      help="TCG component: restore that stopped source; never a fresh boot")
    p.add_argument("--source-receipt-sha256")
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
    source = None
    if args.checkpoint and args.accel != "kvm":
        raise ValueError("checkpoint component source must explicitly use KVM")
    if args.restore_from:
        if args.accel != "tcg" or not args.source_receipt_sha256:
            raise ValueError("restore requires explicit TCG and the source receipt hash")
        if not args.restore_from.is_absolute():
            raise ValueError("source checkpoint directory must be absolute")
        path = args.restore_from / "receipt.json"
        meta = path.lstat()
        if (not stat.S_ISREG(meta.st_mode) or not 0 < meta.st_size <= 65536
                or meta.st_uid != os.geteuid()):
            raise ValueError("source receipt must be a bounded owned regular file")
        if digest(path) != args.source_receipt_sha256:
            raise ValueError("source receipt custody differs")
        source = json.loads(path.read_text())
        if (source.get("status") != "checkpoint_captured"
                or source.get("schema") != "ctox.guest_image_component_probe.v1"
                or source.get("owner_thread") != "01a0879f-fa04-7a72-a9be-f471c2df5471"
                or source.get("base_sha256") != args.sha256
                or source.get("acceleration") != "kvm"
                or source.get("owned_qemu_reaped") is not True
                or source.get("source_qmp_quit_exit") != 0
                or source.get("machine") != CHECKPOINT_MACHINE
                or source.get("cpu") != CHECKPOINT_CPU
                or source.get("irqchip") != "userspace"
                or source.get("vcpus") != 2):
            raise ValueError("source is not a completed, reaped compatible component checkpoint")
        for name, limit in [("root.qcow2", 9 * 1024**3), ("vm-state.bin", 5 * 1024**3)]:
            path = args.restore_from / name
            meta = path.lstat()
            bound = source.get("checkpoint_files", {}).get(name, {})
            if (not stat.S_ISREG(meta.st_mode) or not 0 < meta.st_size <= limit
                    or meta.st_uid != os.geteuid() or meta.st_mode & 0o222
                    or meta.st_size != bound.get("bytes")
                    or digest(path) != bound.get("sha256")):
                raise ValueError("source checkpoint file custody differs: " + name)
    elif args.source_receipt_sha256:
        raise ValueError("source receipt hash applies only to restore")
    os.umask(0o077)
    args.out.mkdir(mode=0o700, parents=False, exist_ok=False)
    out = args.out.resolve()
    if any(c in str(out) for c in ",\n\r"):
        raise ValueError("private QEMU socket paths cannot contain keyval delimiters")
    # Multiple guest CPUs contend on the single TCG execution thread.
    # Keep KVM's two vCPUs; explicit software emulation uses one.
    checkpoint_mode = args.checkpoint or source is not None
    vcpus = 2 if checkpoint_mode else (1 if args.accel == "tcg" else 2)
    machine = CHECKPOINT_MACHINE if checkpoint_mode else "pc"
    # The actual TCG cold boot can exceed six minutes even with both
    # mandatory boot mounts healthy. Keep finite accelerator-specific
    # lifetime limits and always reap the owned child.
    probe_timeout = 1200 if args.accel == "tcg" else 600
    boot_timeout = 900 if args.accel == "tcg" else 120
    desktop_settle = 30 if args.accel == "tcg" else 10
    # Actual restored TCG guests need about 116s for ordinary service teardown;
    # the previous 120s bound also expired after filesystems had unmounted.
    shutdown_timeout = 180 if args.accel == "tcg" else 45
    guest_id = source["guest_id"] if source else "o04-" + args.accel + "-" + str(uuid.uuid4())
    receipt = dict(schema="ctox.guest_image_component_probe.v1", scope="isolated image/endpoint only",
                   owner_thread="01a0879f-fa04-7a72-a9be-f471c2df5471",
                   guest_id=guest_id, acceleration=args.accel,
                   vcpus=vcpus,
                   base_sha256=args.sha256, started=time.time(),
                   boot_timeout_seconds=boot_timeout, desktop_settle_seconds=desktop_settle,
                   shutdown_timeout_seconds=shutdown_timeout,
                   probe_timeout_seconds=probe_timeout,
                   product_enrollment_p2p_restore_accepted=False, assertions=[])
    if checkpoint_mode:
        receipt.update(machine=machine, cpu=CHECKPOINT_CPU, irqchip="userspace",
                       checkpoint_scope="isolated VM RAM/devices/disk only; no native enrollment, protected P2P or distributed owner fence",
                       source_receipt_sha256=args.source_receipt_sha256)
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
        if source:
            with (args.restore_from / "root.qcow2").open("rb") as src, (out / "root.qcow2").open("xb") as dst:
                for part in iter(lambda: src.read(1024 * 1024), b""):
                    dst.write(part)
                dst.flush()
                os.fsync(dst.fileno())
        else:
            subprocess.run(["/usr/bin/qemu-img", "create", "-f", "qcow2", "-F", "raw", "-b",
                            str(base), str(out / "root.qcow2")], check=True, timeout=20)
        startup = out / "guest-startup.json"
        startup.write_text(json.dumps(dict(guest_id=guest_id, display=":0",
                                          xauthority="/run/ctox-desktop/Xauthority")))
        qmp_listener, guest_listener = listener("qmp.sock"), listener("guest.sock")
        command = ["/usr/bin/qemu-system-x86_64", "-machine", machine, "-accel",
                   ("kvm,kernel-irqchip=off" if checkpoint_mode else "kvm") if args.accel == "kvm" else "tcg,thread=single",
                   "-m", "4096", "-smp", str(vcpus), "-fw_cfg",
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
        if checkpoint_mode:
            command += ["-cpu", CHECKPOINT_CPU]
        if source:
            command += ["-incoming", "defer"]
        logs = (out / "qemu.stderr").open("wb")
        proc = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                stderr=logs, env={})
        signal.alarm(probe_timeout)
        receipt.update(pid=proc.pid, argv=command,
                       stop_condition=f"maximum {probe_timeout} seconds; always reap owned QEMU")
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
        def monitor(execute, arguments=None, descriptor=None):
            nonlocal counter
            counter += 1
            command = dict(execute=execute, id=counter)
            if arguments is not None:
                command["arguments"] = arguments
            wire = json.dumps(command).encode() + b"\n"
            if descriptor is None:
                qmp.sendall(wire)
            else:
                sent = qmp.sendmsg([wire], [(socket.SOL_SOCKET, socket.SCM_RIGHTS,
                                           array.array("i", [descriptor]))])
                qmp.sendall(wire[sent:])
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
        assertion("restore_process_initially_paused" if source else "fresh_guest_initially_paused", status=status)
        if source:
            with (args.restore_from / "vm-state.bin").open("rb") as state:
                monitor("getfd", dict(fdname="checkpoint"), state.fileno())
                monitor("migrate-incoming", dict(uri="fd:checkpoint"))
                deadline = time.monotonic() + 300
                while True:
                    status = monitor("query-status")
                    if status["status"] == "paused" and not status["running"]:
                        break
                    if status["status"] != "inmigrate" or time.monotonic() >= deadline:
                        raise ValueError("checkpoint restore did not reach paused state: " + str(status))
                    time.sleep(1)
            assertion("actual_checkpoint_restored_paused", status=status)
        start = time.monotonic()
        monitor("cont")
        guest.settimeout(boot_timeout)
        endpoint = request(guest, dict(kind="probe", id=1))
        session_id = endpoint.get("session_id", "")
        if endpoint.get("kind") != "endpoint" or endpoint.get("guest_id") != guest_id or str(uuid.UUID(session_id)) != session_id:
            raise ValueError("native endpoint identity/session differs")
        if source and session_id != source.get("session_id"):
            raise ValueError("restore created a different native guest session")
        receipt["session_id"] = session_id
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
        if source:
            # The live KVM-to-TCG restore answers the endpoint before the
            # resumed desktop has settled. Retain the native helper deadline;
            # allow this bounded software-emulation settle before observing.
            time.sleep(desktop_settle)
            assertion("restored_desktop_settled", seconds=desktop_settle)
        capture(2, "capture-before.png")
        # XFCE can still be painting when the X11 endpoint first answers.
        # Retain that first frame and inspect a second actual capture after a
        # bounded cold-start settling period; neither proves a usable desktop.
        time.sleep(desktop_settle)
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
            pause_seconds = 0
            for action in actions:
                if not isinstance(action, dict):
                    raise ValueError("component input action must be an object")
                if action.get("kind") == "pause":
                    seconds = action.get("seconds")
                    if type(seconds) is not int or not 1 <= seconds <= 30:
                        raise ValueError("component pause must be 1 to 30 whole seconds")
                    pause_seconds += seconds
            if pause_seconds > 60:
                raise ValueError("component input pauses exceed 60 seconds")
            for i, action in enumerate(actions, 5):
                if action.get("kind") == "pause":
                    time.sleep(action["seconds"])
                    assertion("component_input_pause", seconds=action["seconds"])
                    continue
                reply = request(guest, dict(kind="input_session", id=i, session_id=session_id, input=action))
                if reply.get("kind") != "applied":
                    raise ValueError("requested component input failed")
            capture(40, "capture-after.png")
        final = request(guest, dict(kind="probe", id=41))
        if final != dict(kind="endpoint", id=41, guest_id=guest_id, session_id=session_id):
            raise ValueError("guest session changed during component test")
        assertion("session_stable_after_capture_and_input")
        if args.checkpoint:
            monitor("stop")
            status = monitor("query-status")
            if status["running"]:
                raise ValueError("source did not quiesce")
            assertion("source_stopped_before_checkpoint", status=status)
            with (out / "vm-state.bin").open("xb") as state:
                monitor("getfd", dict(fdname="checkpoint"), state.fileno())
                monitor("migrate", dict(uri="fd:checkpoint"))
                deadline = time.monotonic() + 300
                while True:
                    migration = monitor("query-migrate")
                    if migration.get("status") == "completed":
                        break
                    if migration.get("status") in ("failed", "cancelled") or time.monotonic() >= deadline:
                        raise ValueError("source VM migration failed: " + str(migration))
                    time.sleep(1)
                state.flush()
                os.fsync(state.fileno())
            status = monitor("query-status")
            if status["running"] or status["status"] != "postmigrate":
                raise ValueError("source was runnable after checkpoint")
            assertion("source_vm_state_captured", migration=migration, status=status)
            monitor("quit")
            code = proc.wait(timeout=10)
            if code != 0:
                raise ValueError("source QMP quit failed")
            receipt["source_qmp_quit_exit"] = code
            receipt["checkpoint_files"] = {}
            for name in ["root.qcow2", "vm-state.bin"]:
                path = out / name
                with path.open("rb") as stream:
                    os.fsync(stream.fileno())
                path.chmod(0o400)
                receipt["checkpoint_files"][name] = dict(bytes=path.stat().st_size, sha256=digest(path))
            receipt["status"] = "checkpoint_captured"
            return
        monitor("system_powerdown")
        shutdown_started = time.monotonic()
        try:
            code = proc.wait(timeout=shutdown_timeout)
            assertion("guest_acpi_powerdown", exit=code)
            receipt["guest_graceful_powerdown"] = code == 0
            if code != 0:
                raise RuntimeError("owned QEMU exited nonzero after ACPI powerdown: " + str(code))
        except subprocess.TimeoutExpired:
            receipt["guest_graceful_powerdown"] = False
            raise RuntimeError("ACPI powerdown did not complete; no consistent checkpoint claimed")
        finally:
            receipt["shutdown_wait_seconds"] = time.monotonic() - shutdown_started
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
