#!/usr/bin/env python3
"""Install/check a pinned, operator-owned Linux Node runtime for the Pi sidecar."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import signal
import selectors
import time
import subprocess
import tarfile
import tempfile

VERSION = "22.23.2"
ARCHIVES = {
    "x86_64": ("x64", "d60acfe00a2932254bb0ad20e01b0d74397a0875595de719654b214f4b03f307"),
    "aarch64": ("arm64", "fff4078c5def658577f92c88db7db3bc0072924bfb93fe52c1e744a54e94abb8"),
}
MAX_ARCHIVE = 100 * 1024 * 1024
MAX_EXPANDED = 500 * 1024 * 1024


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unpack(archive, destination, name):
    """Validate the entire member graph before extracting into a fresh directory."""
    with tarfile.open(archive, "r:xz") as stream:
        members = stream.getmembers()
        require(len(members) <= 20000, "too many archive entries")
        require(sum(m.size for m in members) <= MAX_EXPANDED, "expanded archive too large")
        paths, links = set(), set()
        for member in members:
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and ".." not in path.parts and
                    path.parts and path.parts[0] == name, "archive path escapes version directory")
            require(path not in paths, "duplicate archive path")
            paths.add(path)
            require(member.isdir() or member.isfile() or member.issym(), "unsupported archive entry")
            if member.issym():
                links.add(path)
                target = PurePosixPath(member.linkname)
                require(not target.is_absolute(), "absolute archive symlink")
                parts = list(path.parent.parts)
                for part in target.parts:
                    if part == "..":
                        require(len(parts) > 1, "archive symlink escapes version directory")
                        parts.pop()
                    elif part != ".":
                        parts.append(part)
        for path in paths:
            require(not any(parent in links for parent in path.parents), "archive writes through symlink")
        for member in members:
            target = destination / member.name
            target.parent.mkdir(parents=True, exist_ok=True)
            if member.isdir():
                target.mkdir(exist_ok=True)
            elif member.issym():
                target.symlink_to(member.linkname)
            else:
                with stream.extractfile(member) as source, target.open("xb") as output:
                    shutil.copyfileobj(source, output)
                target.chmod(0o755 if member.mode & 0o111 else 0o644)


def bounded(node, arguments, scratch, capture=False, diagnostics=None):
    # No inherited secrets, NODE_OPTIONS, preload hooks or user package paths.
    env = {"PATH": str(node.parent) + os.pathsep + os.defpath,
           "HOME": str(scratch), "TMPDIR": str(scratch), "LANG": "C"}
    diagnostics = diagnostics or scratch
    fd, log_name = tempfile.mkstemp(prefix="node-preflight-", suffix=".stderr", dir=diagnostics)
    log_path = Path(log_name)
    output = bytearray()
    retained, observed, timed_out = 0, 0, False
    with os.fdopen(fd, "wb") as log, subprocess.Popen(
            [str(node), *arguments], cwd=scratch, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
            stderr=subprocess.PIPE, start_new_session=True) as child:
        with selectors.DefaultSelector() as selected:
            selected.register(child.stderr, selectors.EVENT_READ, "stderr")
            if capture:
                selected.register(child.stdout, selectors.EVENT_READ, "stdout")
            deadline = time.monotonic() + 15
            while selected.get_map():
                if time.monotonic() >= deadline:
                    timed_out = True
                    break
                for key, _ in selected.select(min(0.1, max(0, deadline - time.monotonic()))):
                    block = os.read(key.fileobj.fileno(), 8192)
                    if not block:
                        selected.unregister(key.fileobj)
                    elif key.data == "stderr":
                        observed += len(block)
                        kept = block[:max(0, 65536 - retained)]
                        log.write(kept)
                        retained += len(kept)
                    else:
                        output.extend(block[:max(0, 4097 - len(output))])
            try:
                if timed_out:
                    raise subprocess.TimeoutExpired(str(node), 15)
                child.wait(timeout=max(0.001, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                timed_out = True
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait()
    if timed_out or child.returncode != 0 or len(output) > 4096:
        receipt = {"phase": "version" if arguments == ["--version"] else "library_import",
                   "exit_code": child.returncode, "timed_out": timed_out,
                   "stderr_path": str(log_path), "stderr_sha256": digest(log_path),
                   "stderr_retained_bytes": retained, "stderr_observed_bytes": observed,
                   "stderr_truncated": observed > retained, "stdout_limit_exceeded": len(output) > 4096}
        receipt_path = log_path.with_suffix(".json")
        with open(receipt_path, "x", opener=lambda p, f: os.open(p, f, 0o600)) as record:
            json.dump(receipt, record)
        raise ValueError("Node preflight failed; private diagnostic receipt: " + str(receipt_path))
    log_path.unlink()
    return bytes(output)


def preflight(node, bundle, expected_bundle, scratch, diagnostics=None):
    require(node.is_file() and not node.is_symlink(), "Node executable missing or symlinked")
    require(digest(bundle) == expected_bundle, "sidecar bundle checksum mismatch")
    version = bounded(node, ["--version"], scratch, capture=True, diagnostics=diagnostics).decode().strip()
    require(version == "v" + VERSION, "unexpected Node version")
    # Passing the bundle as argv[1] directly would activate its daemon guard.
    # Capture that path then clear argv[1] before the library import.
    code = ("import {pathToFileURL} from 'node:url';"
            "const url=pathToFileURL(process.argv[1]);process.argv[1]=undefined;"
            "await import(url.href);")
    bounded(node, ["--input-type=module", "-e", code, str(bundle)], scratch, diagnostics=diagnostics)
    require(digest(bundle) == expected_bundle, "sidecar bundle changed during preflight")
    return version


def provision(root, archive, bundle, bundle_sha, machine):
    arch, archive_sha = ARCHIVES[machine]
    name = "node-v" + VERSION + "-linux-" + arch
    target = root / name
    if target.exists() or target.is_symlink():
        require(target.is_dir() and not target.is_symlink(), "runtime target is not a real directory")
        receipt = json.loads((target / "ctox-runtime-receipt.json").read_text())
        require(receipt.get("archive_sha256") == archive_sha and
                receipt.get("version") == VERSION and receipt.get("architecture") == arch,
                "runtime receipt does not match pin")
        node = target / "bin/node"
        require(digest(node) == receipt.get("node_sha256"), "installed Node checksum mismatch")
        with tempfile.TemporaryDirectory(prefix=".node-check-", dir=root) as scratch:
            preflight(node, bundle, bundle_sha, Path(scratch), root)
        return node
    require(archive is not None, "runtime missing; supply the pinned --archive to install")
    require(archive.is_file() and archive.stat().st_size <= MAX_ARCHIVE, "archive missing or too large")
    # Copy before hashing to avoid verifying one file and extracting another.
    with tempfile.TemporaryDirectory(prefix=".node-install-", dir=root) as scratch:
        staging = Path(scratch)
        copy = staging / "archive.tar.xz"
        with archive.open("rb") as source, copy.open("xb") as output:
            remaining = MAX_ARCHIVE + 1
            while remaining:
                block = source.read(min(1024 * 1024, remaining))
                if not block:
                    break
                output.write(block)
                remaining -= len(block)
        require(copy.stat().st_size <= MAX_ARCHIVE and digest(copy) == archive_sha,
                "Node archive checksum mismatch")
        unpack(copy, staging, name)
        tree = staging / name
        node = tree / "bin/node"
        preflight(node, bundle, bundle_sha, staging, root)
        receipt = {"version": VERSION, "architecture": arch, "archive_sha256": archive_sha,
                   "node_sha256": digest(node), "checked_bundle_sha256": bundle_sha}
        (tree / "ctox-runtime-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        tree.rename(target)
    return target / "bin/node"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install-root", type=Path, required=True)
    parser.add_argument("--archive", type=Path, help="previously downloaded official pinned .tar.xz")
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--bundle-sha256", required=True)
    parser.add_argument("--print-path", action="store_true", help="emit only the checked bin directory")
    args = parser.parse_args()
    require(platform.system() == "Linux" and platform.machine() in ARCHIVES,
            "supported platforms: Linux x86_64 and aarch64")
    require(re.fullmatch(r"[a-f0-9]{64}", args.bundle_sha256), "invalid bundle SHA256")
    root = args.install_root.expanduser().resolve()
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    require(root.stat().st_uid == os.getuid() and root.stat().st_mode & 0o022 == 0,
            "install root must be owned by this user and not group/world writable")
    # The lock is retained as a normal file; flock releases on process exit.
    with (root / ".install.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        node = provision(root, args.archive, args.bundle.resolve(strict=True),
                         args.bundle_sha256, platform.machine())
    if args.print_path:
        print(node.parent)
    else:
        print(json.dumps({"ok": True, "node": str(node), "version": VERSION,
                          "node_sha256": digest(node), "bundle_sha256": args.bundle_sha256,
                          "acceptance": "inert_library_import_only"}))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, tarfile.TarError) as error:
        raise SystemExit("Node runtime rejected: " + str(error)) from None
