#!/usr/bin/env python3
"""Materialize exact committed runtime assets into an exclusive isolated bundle.

Run through the shared admission gate. This does not activate a shell slot,
install a binary, start a service, or read/copy runtime state and credentials.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile

PREFIX = "src/apps/business-os"
MANIFEST = "src/transfer-runtime-manifest.json"
REQUIRED = {f"{PREFIX}/index.html", f"{PREFIX}/shared/rxdb-runtime.js",
            f"{PREFIX}/rxdb/dist/ctox-rxdb-js.mjs"}


def git(repository, *arguments):
    return subprocess.check_output(["git", "-C", str(repository), *arguments])


def inventory(repository, revision):
    if not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", revision):
        raise ValueError("revision must be the candidate's full immutable commit ID")
    commit = git(repository, "rev-parse", "--verify", revision + "^{commit}").decode().strip()
    if commit != revision:
        raise ValueError("revision does not identify the requested commit")
    entries = []
    for row in git(repository, "ls-tree", "-r", "-z", "-l", revision, "--", PREFIX).split(b"\0"):
        if not row:
            continue
        metadata, raw_path = row.split(b"\t", 1)
        mode, kind, oid, size = metadata.decode("ascii").split()
        path = raw_path.decode("utf-8")
        parts = PurePosixPath(path).parts
        if not path.startswith(PREFIX + "/") or ".." in parts:
            raise ValueError("runtime asset escaped the source prefix")
        if kind != "blob" or mode not in ("100644", "100755"):
            raise ValueError("runtime assets must be regular tracked files: " + path)
        entries.append({"path": path, "git_blob": oid, "mode": mode, "size": int(size)})
    if not REQUIRED.issubset({entry["path"] for entry in entries}):
        raise ValueError("candidate commit lacks required Business OS runtime assets")
    return sorted(entries, key=lambda entry: entry["path"])


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def materialize(repository, revision, bundle_root):
    repository = Path(repository).resolve(strict=True)
    bundle_root = Path(bundle_root)
    if bundle_root.is_symlink():
        raise ValueError("bundle root must not be a symbolic link")
    bundle_root = bundle_root.resolve(strict=True)
    source_root = Path(git(repository, "rev-parse", "--show-toplevel").decode().strip()).resolve()
    common_git = Path(git(repository, "rev-parse", "--path-format=absolute", "--git-common-dir").decode().strip()).resolve()
    if bundle_root.is_relative_to(source_root) or bundle_root.is_relative_to(common_git):
        raise ValueError("bundle root must be outside the source checkout and Git metadata")
    if not bundle_root.is_dir() or os.path.lexists(bundle_root / "src"):
        raise ValueError("exclusive bundle root must exist and have no src entry")
    entries = inventory(repository, revision)
    total = sum(entry["size"] for entry in entries)
    if shutil.disk_usage(bundle_root).free < 20 * 1024**3 + total:
        raise ValueError("runtime bundle requires 20 GiB free after materialization")
    lock_path = bundle_root / ".transfer-runtime.lock"
    descriptor = os.open(lock_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.close(descriptor)
    try:
        with tempfile.TemporaryDirectory(prefix=".transfer-runtime-", dir=bundle_root) as temporary:
            stage = Path(temporary)
            process = subprocess.Popen(["git", "-C", str(repository), "cat-file", "--batch"],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE)
            try:
                for entry in entries:
                    process.stdin.write((entry["git_blob"] + "\n").encode("ascii"))
                    process.stdin.flush()
                    expected = f"{entry['git_blob']} blob {entry['size']}\n".encode("ascii")
                    if process.stdout.readline() != expected:
                        raise ValueError("Git object header disagrees with committed inventory")
                    destination = stage / entry["path"]
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    digest = hashlib.sha256()
                    object_digest = hashlib.sha1() if len(entry["git_blob"]) == 40 else hashlib.sha256()
                    object_digest.update(f"blob {entry['size']}\0".encode("ascii"))
                    remaining = entry["size"]
                    with destination.open("xb") as output:
                        while remaining:
                            block = process.stdout.read(min(remaining, 1024 * 1024))
                            if not block:
                                raise ValueError("truncated committed runtime object")
                            output.write(block)
                            digest.update(block)
                            object_digest.update(block)
                            remaining -= len(block)
                        output.flush()
                        os.fsync(output.fileno())
                    os.chmod(destination, int(entry["mode"], 8) & 0o777)
                    if process.stdout.read(1) != b"\n":
                        raise ValueError("invalid Git object framing")
                    if object_digest.hexdigest() != entry["git_blob"]:
                        raise ValueError("runtime object bytes disagree with committed Git object ID")
                    entry["sha256"] = digest.hexdigest()
                process.stdin.close()
                if process.wait(timeout=10) != 0:
                    raise ValueError("Git object export failed")
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                for stream in (process.stdin, process.stdout, process.stderr):
                    stream.close()
            manifest = {"schema": "ctox.transfer.runtime.v1", "source_commit": revision,
                        "source_tree": git(repository, "rev-parse", revision + ":" + PREFIX).decode().strip(),
                        "file_count": len(entries), "total_bytes": total, "files": entries}
            path = stage / MANIFEST
            path.write_text(json.dumps(manifest, indent=2) + "\n")
            with path.open("rb") as stream:
                os.fsync(stream.fileno())
            for directory, _, _ in os.walk(stage / "src", topdown=False):
                sync_directory(directory)
            # The operator must retain exclusive ownership of this bundle root.
            if os.path.lexists(bundle_root / "src"):
                raise ValueError("bundle src appeared during materialization")
            os.rename(stage / "src", bundle_root / "src")
            sync_directory(bundle_root)
            return {"source_commit": revision, "source_tree": manifest["source_tree"],
                    "file_count": len(entries), "total_bytes": total,
                    "manifest": str(bundle_root / MANIFEST),
                    "manifest_sha256": hashlib.sha256((bundle_root / MANIFEST).read_bytes()).hexdigest()}
    finally:
        lock_path.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--revision", required=True, help="full commit ID from the identified binary manifest")
    parser.add_argument("--bundle-root", required=True, type=Path, help="existing exclusive isolated root, with no src entry")
    arguments = parser.parse_args()
    print(json.dumps(materialize(arguments.repository, arguments.revision, arguments.bundle_root)))


if __name__ == "__main__":
    main()
