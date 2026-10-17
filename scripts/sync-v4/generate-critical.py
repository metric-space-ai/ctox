#!/usr/bin/env python3
"""Export only named KEEP-CRITICAL selectors, never private audit commentary."""
import argparse
import csv
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("audit_dir", type=Path)
parser.add_argument("--output", type=Path, default=Path(__file__).with_name("critical-tests.json"))
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
sources, selected = {}, set()
for source in sorted(args.audit_dir.glob("*.tsv")):
    sources[source.name] = hashlib.sha256(source.read_bytes()).hexdigest()
    for row in csv.DictReader(source.open(), delimiter="\t"):
        if row["verdict"] != "KEEP-CRITICAL":
            continue
        # appsec's audit explicitly uses paths relative to its standalone crate.
        file = ("src/tools/appsec-pentest/" if source.name == "appsec.tsv" else "") + row["file"]
        if not (root / file).is_file():
            raise SystemExit("critical source missing: " + file)
        selected.add((file, row["test"]))
if not sources or not selected:
    raise SystemExit("empty critical audit selection")
args.output.write_text(json.dumps({
    "schema": 1, "audit_sha256": sources,
    "tests": [{"file": file, "test": test} for file, test in sorted(selected)],
}, indent=2, ensure_ascii=False) + "\n")
