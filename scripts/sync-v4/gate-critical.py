#!/usr/bin/env python3
"""Named critical contracts, executable without GitHub Actions.

Default: critical audit + store/CAP/transport/kernel contracts. --scope and
--only-file permit narrow lane runs; such runs are explicitly labelled partial.
Logs and a machine-readable result stay under the lane TMPDIR (or --evidence).
"""
import argparse
import collections
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def title_pattern(name):
    # TSV titles of parameterized JS tests preserve ${expression} placeholders.
    # Match only the variable span; fixed surrounding text remains literal.
    return ".+?".join(re.escape(part) for part in re.split(r"\$\{[^}]+\}", name))


def python_worker(file, names):
    path = ROOT / file
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    def flatten(suite):
        for entry in suite:
            if isinstance(entry, unittest.TestSuite):
                yield from flatten(entry)
            else:
                yield entry
    available = list(flatten(unittest.defaultTestLoader.loadTestsFromModule(module)))
    selected = []
    for name in names:
        matches = [test for test in available if test.id().split(".")[-1] == name]
        require(len(matches) == 1, "missing/ambiguous Python test: " + name)
        selected.extend(matches)
    require(selected, "no Python tests selected")
    result = unittest.TextTestRunner(verbosity=2).run(unittest.TestSuite(selected))
    require(result.wasSuccessful() and not result.skipped and result.testsRun == len(selected),
            "critical Python tests failed, skipped or did not execute")


class Gate:
    def __init__(self, evidence):
        self.evidence = evidence
        self.results = []
        self.metadata = {}
        evidence.mkdir(parents=True, exist_ok=True)

    def run(self, label, command):
        print("RUN " + label, flush=True)
        log = self.evidence / (label + ".log")
        with log.open("w") as output:
            process = subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
        text = log.read_text(errors="replace")
        require(process.returncode == 0, f"{label}: exit {process.returncode}; evidence {log}\n{text[-6000:]}")
        return text

    def passed(self, label):
        self.results.append(label)
        print("PASS " + label, flush=True)

    def cargo_target(self, file):
        path = ROOT / file
        require(path.is_file(), "critical source missing: " + file)
        directory = path.parent
        while not (directory / "Cargo.toml").is_file():
            require(directory != ROOT, "no Cargo manifest for " + file)
            directory = directory.parent
        manifest = directory / "Cargo.toml"
        if manifest not in self.metadata:
            result = subprocess.run(["cargo", "metadata", "--locked", "--no-deps", "--format-version=1",
                                     "--manifest-path", str(manifest)], cwd=ROOT, capture_output=True, text=True)
            require(result.returncode == 0, "cargo metadata failed: " + result.stderr[-6000:])
            self.metadata[manifest] = next(p for p in json.loads(result.stdout)["packages"]
                                          if Path(p["manifest_path"]) == manifest)
        package = self.metadata[manifest]
        candidates = []
        for target in package["targets"]:
            kind = next((k for k in target["kind"] if k in ("lib", "bin", "test", "proc-macro")), None)
            if kind is None:
                continue
            source = Path(target["src_path"])
            if path == source or source.parent in path.parents:
                candidates.append((path == source, len(source.parent.parts), kind in ("lib", "proc-macro"), kind, target["name"]))
        require(candidates, "no cargo test target for " + file)
        chosen = max(candidates)
        kind, name = chosen[-2:]
        flag = ("--lib",) if kind in ("lib", "proc-macro") else ("--" + kind, name)
        return (str(manifest.relative_to(ROOT)), package["name"], flag)

    def rust(self, label, target, selectors=None, prefix=None):
        manifest, package, flag = target
        command = ["cargo", "test", "--locked", "--jobs", "2", "--manifest-path", manifest,
                   "-p", package, *flag]
        listing = self.run(label + "-list", command + ["--", "--list", "--format=terse"])
        available = re.findall(r"^(.+): test$", listing, re.M)
        if selectors is None:
            selected = [n for n in available if prefix is None or prefix in n]
        else:
            selected = []
            for selector in selectors:
                matches = [n for n in available if n == selector or n.endswith("::" + selector)]
                require(len(matches) == 1, f"{label}: missing/ambiguous named test {selector}")
                selected.extend(matches)
        require(selected, label + ": zero selected tests")
        selected = sorted(set(selected))
        output = self.run(label, command + ["--", "--exact", "--include-ignored", "--test-threads=2", *selected])
        passed = set(re.findall(r"^test (.+) \.\.\. ok$", output, re.M))
        require(set(selected) <= passed, label + ": named test did not pass (missing/ignored): " +
                ", ".join(sorted(set(selected) - passed)))
        require(re.search(r"^test result: ok\. [1-9][0-9]* passed; 0 failed;", output, re.M),
                label + ": no executed passing tests")
        self.passed(label)

    def node(self, label, files, selectors=None):
        command = ["node", "--test", "--test-concurrency=2", "--test-reporter=" + str(HERE / "gate-critical-reporter.mjs")]
        patterns = None if selectors is None else [title_pattern(name) for name in selectors]
        if patterns is not None:
            require(patterns, label + ": empty Node selection")
            command += ["--test-name-pattern=(?:^| )(?:(?:" + ")|(?:".join(patterns) + "))$"]
        output = self.run(label, command + files)
        passed = [json.loads(line) for line in re.findall(r"^# v4_case (.+)$", output, re.M)]
        require(passed, label + ": no executed passing Node cases (file wrappers are not cases)")
        for name, pattern in zip(selectors or [], patterns or []):
            require(any(re.fullmatch(pattern, item["name"]) for item in passed),
                    label + ": named Node case did not execute: " + name)
        self.passed(label)

    def contracts(self):
        for label, manifest, package, prefix in [
            ("store", "Cargo.toml", "ctox-store", None),
            ("cap", "src/core/app_capabilities/contract/Cargo.toml", "ctox-app-capabilities-contract", None),
            ("rtc-patch", "src/core/rxdb/patches/rtc/Cargo.toml", "rtc", "p4_"),
            ("webrtc-patch", "src/core/rxdb/patches/webrtc/Cargo.toml", "webrtc", "p4_"),
            ("rxdb-send", "src/core/rxdb/Cargo.toml", "ctox-rxdb", "p4_"),
        ]:
            self.rust(label, (manifest, package, ("--lib",)), prefix=prefix)
        self.node("kernel", [str(p.relative_to(ROOT)) for p in sorted((ROOT / "src/apps/business-os/kernel/tests").glob("*.test.mjs"))])

    def critical(self, rows):
        rust, other = collections.defaultdict(set), collections.defaultdict(set)
        for row in rows:
            file, name = row["file"], row["test"]
            require((ROOT / file).is_file(), "critical source missing: " + file)
            if file.endswith(".rs"):
                rust[self.cargo_target(file)].add(name)
            else:
                other[file].add(name)
        for index, (target, names) in enumerate(sorted(rust.items())):
            self.rust("critical-rust-" + str(index), target, sorted(names))
        for index, (file, names) in enumerate(sorted(other.items())):
            label = "critical-script-" + str(index)
            if names == {"(script)"}:
                # Explicit audit process contracts are unfiltered, unlike named
                # node:test cases. A script's nonzero exit fails the gate.
                require(file.endswith((".js", ".mjs", ".sh")), "unsupported script: " + file)
                self.run(label, (["bash"] if file.endswith(".sh") else ["node"]) + [file])
                self.passed(label)
            elif file.endswith((".js", ".mjs")):
                self.node(label, [file], sorted(names))
            elif file.endswith(".py"):
                self.run(label, [sys.executable, str(__file__), "--python-worker", file, json.dumps(sorted(names))])
                self.passed(label)
            else:
                raise RuntimeError("unsupported critical runner: " + file)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--python-worker":
        python_worker(sys.argv[2], json.loads(sys.argv[3]))
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scope", choices=("all", "critical", "contracts"), default="all")
    parser.add_argument("--only-file", help="exact audit source path; marks the run partial")
    parser.add_argument("--inventory", type=Path, default=HERE / "critical-tests.json")
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--plan", action="store_true", help="validate/export selection without executing builds")
    args = parser.parse_args()
    inventory = json.loads(args.inventory.read_text())
    require(inventory["schema"] == 1 and inventory["tests"], "empty/invalid critical inventory")
    rows = inventory["tests"]
    require(not (args.only_file and args.scope == "contracts"), "--only-file requires critical scope")
    if args.only_file:
        rows = [row for row in rows if row["file"] == args.only_file]
    require(rows, "zero critical tests selected")
    for row in rows:
        require((ROOT / row["file"]).is_file() and row["test"].strip(), "missing source/name: " + str(row))
    if args.plan:
        print(json.dumps({"scope": args.scope, "partial": bool(args.only_file) or args.scope != "all", "tests": rows}, ensure_ascii=False, indent=2))
        return
    os.environ["CARGO_TERM_COLOR"] = "never"
    os.environ["RUST_TEST_THREADS"] = "2"
    evidence = args.evidence or Path(tempfile.mkdtemp(prefix="v4-critical-"))
    gate = Gate(evidence)
    status, error = "failed", None
    try:
        if args.scope in ("all", "contracts"):
            gate.contracts()
        if args.scope in ("all", "critical"):
            gate.critical(rows)
        status = "passed"
    except (RuntimeError, OSError, ValueError) as exception:
        error = str(exception)
        raise
    finally:
        (evidence / "result.json").write_text(json.dumps({
            "status": status, "scope": args.scope, "partial": bool(args.only_file) or args.scope != "all",
            "completed": gate.results, "error": error,
        }, indent=2) + "\n")
        print("EVIDENCE " + str(evidence), flush=True)


if __name__ == "__main__":
    main()
