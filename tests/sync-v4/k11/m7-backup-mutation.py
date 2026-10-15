#!/usr/bin/env python3
"""Causal proof: loss of a committed WAL-only row must fail snapshot custody."""
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import sys
import time

spec = importlib.util.spec_from_file_location("backup", Path(__file__).with_name("m7-backup.py"))
backup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backup)
out = Path(sys.argv[1]).resolve()
allowed = Path(os.environ["BUILD_LANE_BIN"]).resolve().parent / "artifacts"
if allowed not in out.parents or out.exists():
    raise SystemExit("New private artifact output required")
out.mkdir(mode=0o700)
root = out / "profile"
(root / "runtime/nested").mkdir(parents=True)
(root / ".sync-v4-e2e.json").write_text(json.dumps({"owner": backup.OWNER, "synthetic": True}))
live = sqlite3.connect(root / "runtime/nested/journal.sqlite3")
live.execute("PRAGMA journal_mode=WAL")
live.execute("CREATE TABLE probe(id TEXT PRIMARY KEY, value TEXT)")
live.execute("INSERT INTO probe VALUES('committed-in-wal','survives')")
live.commit()
start = time.monotonic()
try:
    manifest = backup.snapshot(root, out / "snapshot")
    good = backup.verify(out / "snapshot")
    path = out / "snapshot/databases/nested/journal.sqlite3"
    with sqlite3.connect(path) as db:
        db.execute("DELETE FROM probe WHERE id='committed-in-wal'")
    try:
        backup.verify(out / "snapshot")
    except ValueError as error:
        mutated = str(error)
    else:
        raise AssertionError("Deleted committed row was incorrectly accepted")
    # Restore the original snapshot via the actual SQLite API, not a file copy.
    backup.snapshot(root, out / "restored-snapshot")
    backup.verify(out / "restored-snapshot")
    restored = backup.restore(out / "restored-snapshot", out / "restore-validation")
    with backup.connection(out / "restore-validation/nested/journal.sqlite3") as db:
        assert db.execute("SELECT value FROM probe WHERE id='committed-in-wal'").fetchone() == ("survives",)
    report = {"pass": True, "baseline": "green; WAL-only committed row present", "mutation": mutated,
              "mutationSql": "DELETE FROM probe WHERE id='committed-in-wal'", "restored": restored,
              "durationMs": round((time.monotonic()-start)*1000), "sourceLiveDuringSnapshot": True,
              "crossDatabaseAtomicity": False}
    (out / "mutation.json").write_text(json.dumps(report, indent=2)+"\n")
    print(json.dumps(report))
finally:
    live.close()
