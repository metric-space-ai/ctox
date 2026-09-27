#!/usr/bin/env python3
"""Disposable build cache admission and retention; no daemon or backup ownership."""
import argparse
import contextlib
import fcntl
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import time
import uuid

GIB = 1024 ** 3
DEFAULTS = {"completed_days": 7, "soft_bytes": 10 * GIB, "min_free_bytes": 20 * GIB}
MARKER = ".build-cache.json"


def policy(cache):
    path = cache / "build-cache-policy.json"
    if not path.exists():
        return DEFAULTS.copy()
    if path.is_symlink():
        raise ValueError("cache policy must not be a symlink")
    values = json.loads(path.read_text())
    if not isinstance(values, dict) or set(values) - set(DEFAULTS):
        raise ValueError("unknown build cache policy fields")
    result = dict(DEFAULTS, **values)
    if any(type(v) not in (int, float) or not math.isfinite(v) or v <= 0 for v in result.values()):
        raise ValueError("build cache policy values must be finite and positive")
    return result


@contextlib.contextmanager
def locked(cache):
    cache.mkdir(parents=True, exist_ok=True)
    root = cache / "builds"
    if root.is_symlink():
        raise ValueError("builds directory must not be a symlink")
    root.mkdir(exist_ok=True)
    fd = os.open(root / ".lease", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise ValueError("build cache busy: another build or sweep owns the lease") from exc
        yield root, fd
    finally:
        os.close(fd)


def measure(path, budget):
    """Count link inodes without following targets; reject mounts and special files."""
    total = 0
    device = path.lstat().st_dev
    pending = [path]
    seen = set()
    while pending:
        budget[0] -= 1
        if budget[0] < 0 or time.monotonic() > budget[1]:
            raise ValueError("build cache inventory budget exceeded; no cleanup performed")
        item = pending.pop()
        info = item.lstat()
        if info.st_dev != device:
            raise ValueError("mount in cache entry")
        identity = (info.st_dev, info.st_ino)
        if identity not in seen:
            total += info.st_blocks * 512
            seen.add(identity)
        if stat.S_ISLNK(info.st_mode):
            continue
        if stat.S_ISDIR(info.st_mode):
            pending.extend(item.iterdir())
        elif not stat.S_ISREG(info.st_mode):
            raise ValueError("special file in cache entry")
    return total


def producer_alive(pid):
    # PID reuse/permission uncertainty conservatively protects, never authorizes deletion.
    if type(pid) is not int or pid <= 0:
        return True
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def inventory(root, now):
    entries = []
    budget = [250000, time.monotonic() + 10]
    for path in root.iterdir():
        if path.name == ".lease":
            continue
        item = {"name": path.name, "bytes": 0, "eligible": False, "reason": "unknown"}
        try:
            if path.is_symlink():
                item.update(bytes=path.lstat().st_blocks * 512, reason="entry_symlink")
                entries.append(item)
                continue
            if path.lstat().st_dev != root.stat().st_dev or os.path.ismount(path):
                raise ValueError("mount at cache entry boundary")
            item["bytes"] = measure(path, budget)
            marker = path / MARKER
            if path.is_dir() and marker.is_file() and not marker.is_symlink():
                if marker.stat().st_size > 16384:
                    raise ValueError("oversized metadata")
                meta = json.loads(marker.read_text())
                item["reason"] = "active_or_invalid"
                done = meta.get("completed_at")
                if (meta.get("schema") == 1 and meta.get("kind") == "disposable_build"
                        and isinstance(meta.get("owner"), str) and meta["owner"].strip()
                        and meta.get("state") == "completed"
                        and type(done) in (int, float) and math.isfinite(done) and 0 < done <= now):
                    item.update(eligible=True, completed_at=done, reason="completed")
                if "producer_pid" in meta and producer_alive(meta["producer_pid"]):
                    item.update(eligible=False, reason="active_owner")
        except (OSError, ValueError, TypeError, AttributeError) as exc:
            # An incomplete size must never masquerade as an aggregate total.
            raise ValueError(f"cannot safely inventory {path.name}: {exc}") from exc
        entries.append(item)
    return entries


def sweep(root, settings, apply=False, now=None):
    now = time.time() if now is None else now
    entries = inventory(root, now)
    total = sum(e["bytes"] for e in entries)
    remaining = total
    selected = []
    for entry in sorted((e for e in entries if e["eligible"]), key=lambda e: (e["completed_at"], e["name"])):
        if now - entry["completed_at"] >= settings["completed_days"] * 86400 or remaining > settings["soft_bytes"]:
            selected.append(entry)
            remaining -= entry["bytes"]
    removed = []
    errors = []
    for entry in selected if apply else []:
        path = root / entry["name"]
        try:
            # All cooperating users share the lease; rmtree must also resist symlink swaps.
            if path.is_symlink() or not shutil.rmtree.avoids_symlink_attacks:
                raise ValueError("safe directory removal unavailable")
            shutil.rmtree(path)
            removed.append(entry["name"])
        except (OSError, ValueError) as exc:
            errors.append({"name": entry["name"], "error": str(exc)})
    actual = total - sum(e["bytes"] for e in selected if e["name"] in removed)
    return {"scope": str(root), "bytes_before": total, "bytes_after": actual,
            "projected_bytes": remaining, "soft_bytes": settings["soft_bytes"],
            "over_soft_limit": actual > settings["soft_bytes"], "entries": entries,
            "selected": [e["name"] for e in selected], "removed": removed, "errors": errors}


def admit(paths, settings):
    for path in paths:
        free = shutil.disk_usage(path).free
        if free < settings["min_free_bytes"]:
            raise ValueError(f"heavy build denied: {path} has {free} free bytes; requires {settings['min_free_bytes']}")


def save(path, value):
    # Directory is new or validated under the exclusive build lease.
    tmp = path / (MARKER + ".new")
    fd = os.open(tmp, os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "w") as out:
        json.dump(value, out)
    os.replace(tmp, path / MARKER)


def prepare_target(root, settings, link, cache_key):
    """Register new installer output, preserving every legacy target's contents."""
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,60}", cache_key):
        raise ValueError("invalid installer cache key")
    link = link.absolute()
    link = link.parent.resolve() / link.name
    anchor = link.parent
    while not anchor.exists():
        anchor = anchor.parent
    admit([root, anchor], settings)
    parent_pid = os.getppid()
    # Reject simultaneous installers targeting the same source-tree link. The shell
    # remains its owner through copying the built binaries, after each module exits.
    for candidate in root.iterdir():
        marker = candidate / MARKER
        if candidate.is_symlink() or marker.is_symlink() or not marker.is_file():
            continue
        if marker.stat().st_size > 16384:
            raise ValueError("oversized cache metadata")
        recorded = json.loads(marker.read_text())
        if recorded.get("producer_link") != str(link):
            continue
        pid = recorded.get("producer_pid")
        if pid != parent_pid and producer_alive(pid):
            raise ValueError("installer target is owned by another live producer")
        if (recorded.get("schema") == 1 and recorded.get("kind") == "disposable_build"
                and link.is_symlink() and link.resolve() == candidate / "target"
                and not (candidate / "target").is_symlink()):
            if pid == parent_pid:
                return
            completed = recorded.get("completed_at")
            if (recorded.get("state") == "completed" and type(completed) in (int, float)
                    and math.isfinite(completed) and 0 < completed <= time.time()):
                recorded.update(owner=f"installer:{parent_pid}", producer_pid=parent_pid,
                                state="active", started_at=time.time())
                save(candidate, recorded)
                return
    if link.exists() and not link.is_symlink():
        if not link.is_dir() or any(link.iterdir()):
            raise ValueError("unclassified target directory preserved; choose a fresh build path")
        link.rmdir()  # Only an empty producer placeholder, never legacy artifacts.
    entry = root / f"installer-{cache_key}-{uuid.uuid4().hex[:16]}"
    entry.mkdir(mode=0o700)
    (entry / "target").mkdir()
    save(entry, {"schema": 1, "kind": "disposable_build", "owner": f"installer:{parent_pid}",
                 "producer_pid": parent_pid, "producer_link": str(link),
                 "state": "active", "started_at": time.time()})
    link.parent.mkdir(parents=True, exist_ok=True)
    staging = link.parent / f".{link.name}.{uuid.uuid4().hex}.link"
    try:
        staging.symlink_to(entry / "target", target_is_directory=True)
        os.replace(staging, link)
    finally:
        if staging.is_symlink():
            staging.unlink()


def run(root, lease, settings, owner, name, cwd, command, timeout, installer=False):
    if name and (not owner or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,100}", name)):
        raise ValueError("named build cache requires an owner and a simple entry name")
    entry = root / name if name else None
    # Never adopt an existing unknown directory or reuse a completed directory silently.
    if entry is not None and entry.exists():
        raise ValueError("cache entry already exists; choose a new build name (existing data preserved)")
    report = sweep(root, settings, apply=True)
    print(json.dumps(report), file=sys.stderr, flush=True)
    if report["errors"]:
        raise ValueError("cache cleanup failed; build admission stopped")
    admit([root, cwd], settings)
    meta = None
    env = os.environ.copy()
    if entry is not None:
        entry.mkdir(mode=0o700)
        meta = {"schema": 1, "kind": "disposable_build", "owner": owner,
                "state": "active", "started_at": time.time()}
        save(entry, meta)
        # Existing standard tool variables, confined to this child, not CTOX runtime toggles.
        env["CARGO_TARGET_DIR"] = str(entry / "target")
        env["TMPDIR"] = str(entry / "tmp")
        (entry / "tmp").mkdir()
    producer_entries = []
    if installer:
        for candidate in root.iterdir():
            marker = candidate / MARKER
            if candidate.is_symlink() or marker.is_symlink() or not marker.is_file():
                continue
            recorded = json.loads(marker.read_text())
            if recorded.get("producer_pid") == os.getppid() and recorded.get("kind") == "disposable_build":
                recorded.update(state="active")
                save(candidate, recorded)
                producer_entries.append((candidate, recorded))
    child = subprocess.Popen(command, cwd=cwd, env=env, pass_fds=(lease,), start_new_session=True)
    try:
        result = child.wait(timeout=timeout)
    except BaseException:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait()
        # Interrupted builds remain active/unknown until an operator resolves them.
        raise
    # A command returning while its process group is alive has not completed its cache use.
    try:
        os.killpg(child.pid, 0)
    except ProcessLookupError:
        pass
    else:
        raise ValueError("build command left descendants; cache remains active and protected")
    if meta is not None:
        meta.update(state="completed", completed_at=time.time(), exit_code=result)
        save(entry, meta)
    for candidate, recorded in producer_entries:
        recorded.update(state="completed", completed_at=time.time(), exit_code=result)
        save(candidate, recorded)
    print(json.dumps(sweep(root, settings)), file=sys.stderr, flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-root", required=True, type=Path)
    sub = parser.add_subparsers(dest="action", required=True)
    prepare = sub.add_parser("prepare")
    prepare.add_argument("--link-path", required=True, type=Path)
    prepare.add_argument("--cache-key", required=True)
    cleanup = sub.add_parser("sweep")
    cleanup.add_argument("--apply", action="store_true")
    build = sub.add_parser("run")
    build.add_argument("--installer", action="store_true")
    build.add_argument("--owner")
    build.add_argument("--entry")
    build.add_argument("--cwd", type=Path, default=Path.cwd())
    build.add_argument("--timeout-seconds", type=float, default=3600)
    build.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    cache = args.cache_root.resolve()
    settings = policy(cache)
    with locked(cache) as (root, lease):
        if args.action == "prepare":
            prepare_target(root, settings, args.link_path, args.cache_key)
            return 0
        if args.action == "sweep":
            report = sweep(root, settings, args.apply)
            print(json.dumps(report))
            return int(bool(report["errors"]))
        command = args.command[1:] if args.command[:1] == ["--"] else args.command
        if not command or not math.isfinite(args.timeout_seconds) or args.timeout_seconds <= 0:
            raise ValueError("command and positive finite timeout are required")
        return run(root, lease, settings, args.owner, args.entry, args.cwd.resolve(), command, args.timeout_seconds, args.installer)


def stop_requested(signum, frame):
    raise InterruptedError(f"build interrupted by signal {signum}")


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, stop_requested)
    try:
        sys.exit(main())
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"build cache: {error}", file=sys.stderr)
        sys.exit(1)
