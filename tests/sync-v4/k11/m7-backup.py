#!/usr/bin/env python3
"""SQLite API snapshots for owned synthetic profiles, never live file copies.

Per-file snapshots are NOT a cross-database transaction. Quiesce the fixture
owner first when a common application checkpoint is required. Restore always
targets a new empty prefix; this tool cannot overwrite a running installation.
"""
import argparse
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import fcntl
from contextlib import ExitStack
import time

OWNER = "01a087a0-169a-7e23-ba3d-71352256cbfb"
SUFFIXES = {".db", ".sqlite", ".sqlite3"}


def connection(path, immutable=False):
    # Immutable is allowed only under the explicit, held offline custody below.
    db = sqlite3.connect(path.as_uri() + "?mode=ro" + ("&immutable=1" if immutable else ""), uri=True, timeout=5)
    db.execute("PRAGMA query_only=ON")
    return db


def inventory(root):
    mark = json.loads((root / ".sync-v4-e2e.json").read_text())
    if mark.get("owner") != OWNER or mark.get("synthetic") is not True:
        raise ValueError("Only the owned synthetic profile is permitted")
    state = root / "runtime"
    if state.is_symlink() or not state.is_dir():
        raise ValueError("A real private runtime directory is required")
    result = []
    for directory, dirs, files in os.walk(state, followlinks=False):
        if Path(directory) == state:
            # Existing updater snapshots are recovery artifacts, not active DBs.
            dirs[:] = [name for name in dirs if name != "backups"]
        for name in dirs:
            if (Path(directory) / name).is_symlink():
                raise ValueError("State directory symlink requires explicit inventory")
        for name in files:
            path = Path(directory) / name
            if path.suffix.lower() not in SUFFIXES:
                continue
            if path.is_symlink() or not path.is_file():
                raise ValueError("SQLite source must be a regular file")
            result.append(path.relative_to(state))
    if not result:
        raise ValueError("Zero SQLite databases is not a snapshot")
    return sorted(result)


def fingerprint(path):
    # Hash only the completed, private snapshot, never the live source file.
    with closing(connection(path)) as db:
        check = [row[0] for row in db.execute("PRAGMA integrity_check")]
        if check != ["ok"]:
            raise ValueError("Snapshot integrity_check failed")
        schema = list(db.execute("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name"))
        tables = {}
        for name, sql in db.execute("SELECT name,sql FROM sqlite_master WHERE type='table' ORDER BY name"):
            quoted = '"' + name.replace('"', '""') + '"'
            rows = []
            for row in db.execute("SELECT * FROM " + quoted):
                encoded = json.dumps(row, default=lambda v: {"blob_hex": v.hex()},
                                     ensure_ascii=False, separators=(",", ":")).encode()
                rows.append(hashlib.sha256(encoded).digest())
            tables[name] = {"rows": len(rows), "sha256": hashlib.sha256(b"".join(sorted(rows))).hexdigest(),
                            "schemaSha256": hashlib.sha256((sql or "").encode()).hexdigest()}
    return {"integrity": "ok", "tables": tables,
            "schemaSha256": hashlib.sha256(json.dumps(schema, ensure_ascii=False).encode()).hexdigest()}


def copy_database(source, destination, immutable=False):
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if destination.exists():
        raise ValueError("Refusing to overwrite a database")
    deadline = time.monotonic() + 120
    def progress(status, remaining, total):
        if time.monotonic() > deadline:
            raise TimeoutError("SQLite backup exceeded 120 seconds")
    src = connection(source, immutable)
    dst = sqlite3.connect(destination)
    try:
        src.backup(dst, pages=128, progress=progress, sleep=0.05)
    finally:
        dst.close()
        src.close()
    destination.chmod(0o600)
    return fingerprint(destination)


def snapshot(root, output, immutable=False):
    paths = inventory(root)
    if output.exists() or output == root or root in output.parents:
        raise ValueError("A new snapshot outside the source profile is required")
    output.mkdir(parents=True, mode=0o700)
    guards = ExitStack()
    if immutable:
        try:
            # Same stable inode locks as the real store; retained until backup ends.
            locks = [root/'runtime/store/store-daemon.lock'] + [root/'runtime'/Path(str(p)+'.ctox-store.lock') for p in paths]
            for path in locks:
                path.parent.mkdir(parents=True, exist_ok=True)
                file = guards.enter_context(path.open('a+'))
                fcntl.flock(file, fcntl.LOCK_EX | fcntl.LOCK_NB)
            files = [str(root/'runtime'/p)+s for p in paths for s in ('','-wal','-shm') if Path(str(root/'runtime'/p)+s).exists()]
            probe = subprocess.run(['lsof','-nP','-t','--',*files], capture_output=True, text=True, timeout=10)
            proof = dict(atMs=int(time.time()*1000), command=['lsof','-nP','-t','--',*files], exit=probe.returncode,
                         stdout=probe.stdout, stderr=probe.stderr, ownerLocksHeld=True)
            (output/'quiescence.json').write_text(json.dumps(proof, indent=2))
            if probe.returncode != 1 or probe.stdout.strip() or probe.stderr.strip():
                raise ValueError('Immutable snapshot refused: DB holders or unverifiable lsof')
        except BaseException:
            guards.close()
            raise
    result = {"schema": "ctox.m7.sqlite-snapshot.v1", "owner": OWNER, "synthetic": True,
              "consistency": "per_database; caller must quiesce for a shared checkpoint",
              "excludedDirectories": ["runtime/backups (existing recovery copies, preserved)"],
              "sourceRoot": str(root), "startedAtMs": int(time.time()*1000), "databases": [], "complete": False}
    try:
        for relative in paths:
            started = time.monotonic()
            dest = output / "databases" / relative
            proof = copy_database(root / "runtime" / relative, dest, immutable)
            result["databases"].append({"relative": relative.as_posix(), "bytes": dest.stat().st_size,
                                        "durationMs": round((time.monotonic()-started)*1000), **proof})
        if inventory(root) != paths:
            raise ValueError("SQLite inventory changed during snapshot")
        result["complete"] = True
    finally:
        guards.close()
        result["finishedAtMs"] = int(time.time()*1000)
        (output / "snapshot.json").write_text(json.dumps(result, indent=2)+"\n")
    return result


def verify(output):
    manifest = json.loads((output / "snapshot.json").read_text())
    if manifest.get("owner") != OWNER or not manifest.get("synthetic") or not manifest.get("complete") or not manifest.get("databases"):
        raise ValueError("Incomplete owned snapshot")
    expected = set()
    for row in manifest["databases"]:
        relative = Path(row["relative"])
        if relative.is_absolute() or ".." in relative.parts or relative.as_posix() in expected:
            raise ValueError("Snapshot inventory path invalid/duplicate")
        expected.add(relative.as_posix())
        path = output / "databases" / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError("Snapshot file missing or symlinked")
        actual = fingerprint(path)
        if actual["tables"] != row["tables"] or actual["schemaSha256"] != row["schemaSha256"]:
            raise ValueError("Snapshot row/schema fingerprint differs: " + relative.as_posix())
    actual = {p.relative_to(output / "databases").as_posix() for p in (output / "databases").rglob("*") if p.is_file()}
    sidecars = {name + suffix for name in expected for suffix in ["-wal", "-shm", "-journal"]}
    if not expected <= actual or actual - expected - sidecars:
        raise ValueError("Snapshot database inventory differs")
    return manifest


def restore(output, destination):
    manifest = verify(output)
    if destination.exists():
        raise ValueError("Restore requires a new empty prefix, never a live tenant")
    destination.mkdir(parents=True, mode=0o700)
    for row in manifest["databases"]:
        relative = Path(row["relative"])
        actual = copy_database(output / "databases" / relative, destination / relative)
        if actual["tables"] != row["tables"] or actual["schemaSha256"] != row["schemaSha256"]:
            raise ValueError("Restored row/schema fingerprint differs")
    return {"restored": True, "databases": len(manifest["databases"]), "destination": str(destination)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["snapshot", "verify", "restore"])
    parser.add_argument("--root", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--destination", type=Path)
    parser.add_argument("--immutable-after-quiesce", action="store_true")
    args = parser.parse_args()
    if not os.environ.get("BUILD_LANE_BIN") or os.uname().sysname != "Linux":
        raise SystemExit("Canonical Linux lane required")
    output = args.output.resolve()
    allowed = Path(os.environ["BUILD_LANE_BIN"]).resolve().parent / "artifacts"
    for value in [output, args.root.resolve() if args.root else None, args.destination.resolve() if args.destination else None]:
        if value is not None and allowed not in value.parents:
            raise SystemExit("Only private lane artifact paths are permitted")
    if args.operation == "snapshot":
        result = snapshot(args.root.resolve(), output, args.immutable_after_quiesce)
    elif args.operation == "verify":
        result = {"verified": bool(verify(output))}
    else:
        result = restore(output, args.destination.resolve())
    print(json.dumps(result))
