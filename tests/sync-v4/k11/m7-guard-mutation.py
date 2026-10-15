#!/usr/bin/env python3
"""Change the actual snapshot guard, observe causal failure, restore it."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

out = Path(sys.argv[1]).resolve()
allowed = Path(os.environ["BUILD_LANE_BIN"]).resolve().parent / "artifacts"
if allowed not in out.parents or out.exists():
    raise SystemExit("New private output required")
out.mkdir(mode=0o700)
here = Path(__file__).parent
for name in ["m7-backup.py", "m7-backup-mutation.py"]:
    (out / name).write_bytes((here / name).read_bytes())
file = out / "m7-backup.py"
gold = file.read_text()
needle = 'if actual["tables"] != row["tables"] or actual["schemaSha256"] != row["schemaSha256"]:\n            raise ValueError("Snapshot row/schema fingerprint differs: " + relative.as_posix())'
assert gold.count(needle) == 1
report = {"sourceSha256": hashlib.sha256(gold.encode()).hexdigest(),
          "line": gold[:gold.index(needle)].count("\n")+1,
          "change": "snapshot row/schema mismatch guard -> if False",
          "test": "m7-backup-mutation.py detects missing committed WAL row", "pass": False}
def run(name):
    p = subprocess.run([sys.executable, str(out / "m7-backup-mutation.py"), str(out / name)],
                       capture_output=True, text=True, timeout=30)
    (out / (name+".log")).write_text(p.stdout+p.stderr)
    return p
try:
    baseline = run("baseline")
    assert baseline.returncode == 0, baseline.stderr
    file.write_text(gold.replace(needle, needle.replace(needle.splitlines()[0], "if False:")))
    mutated = run("mutated")
    assert mutated.returncode != 0 and "Deleted committed row was incorrectly accepted" in mutated.stderr, mutated.stderr
    report["baselineExit"] = baseline.returncode
    report["mutatedExit"] = mutated.returncode
    report["mutatedAssertion"] = "Deleted committed row was incorrectly accepted"
finally:
    file.write_text(gold)
restored = run("restored")
assert restored.returncode == 0, restored.stderr
report["restoredExit"] = restored.returncode
report["restoredSourceSha256"] = hashlib.sha256(file.read_bytes()).hexdigest()
report["pass"] = report["sourceSha256"] == report["restoredSourceSha256"]
(out / "mutation.json").write_text(json.dumps(report, indent=2)+"\n")
print(json.dumps(report))
