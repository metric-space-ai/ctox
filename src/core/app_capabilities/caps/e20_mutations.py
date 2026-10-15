#!/usr/bin/env python3
"""E20 sandbox mutation proofs: switch one confinement off, the matching escape
test must turn red; the source is restored byte-identically afterwards.
Run only through the GPU build lane, from the repository root:
  python3 src/core/app_capabilities/caps/e20_mutations.py [label,label,...]"""
import datetime, hashlib, json, os, pathlib, re, subprocess, sys

ROOT = pathlib.Path.cwd()
CAPS = ROOT / "src/core/app_capabilities/caps"
OUT = pathlib.Path(os.environ.get("CARGO_TARGET_DIR", "/tmp")) / (
    "e20-proofs-" + datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ"))
OUT.mkdir(parents=True, exist_ok=False)
E = re.escape
T = "app_capabilities::cap3::tests::sandbox_tests::app_capabilities_e20_"
PROOFS = [
    # label, file, pattern, replacement, test, expected panic fragment
    ("read-host-root", "sandbox.rs",
     r'const SYSTEM_TREES: &\[&str\] = &\[[^\]]*\];', 'const SYSTEM_TREES: &[&str] = &["/"];',
     "read_outside_workspace_contract", "E20 read escape"),
    ("read-ctox-root", "sandbox.rs",
     E("let mut out = Vec::new();"), "let mut out = vec![plan.root.canonicalize().unwrap()];",
     "read_outside_workspace_contract", "E20 read escape"),
    ("write-root-bind", "sandbox.rs",
     E('args.extend(["--bind".into(), path(plan.workspace), path(plan.workspace)]);'),
     'args.extend(["--bind".into(), path(plan.root), path(plan.root)]);',
     "write_outside_workspace_contract", "E20 write escape"),
    ("network-shared", "sandbox.rs",
     E('"--unshare-all",\n        "--die-with-parent",'), '"--unshare-all",\n        "--share-net",\n        "--die-with-parent",',
     "network_without_approval_contract", "E20 network escape"),
    ("nproc-off", "sandbox.rs",
     E('("nproc", limits.processes),'), "",
     "fork_bomb_contract", "E20 process limit escaped"),
    ("memory-off", "sandbox.rs",
     E('("data", limits.memory_bytes),'), "",
     "memory_limit_contract", "E20 memory limit escaped"),
    ("cpu-off", "sandbox.rs",
     E('("cpu", limits.cpu_seconds),'), "",
     "memory_limit_contract", "CPU limit must end the run"),
    ("unconfined-fallback", "process.rs",
     E("let mut command = super::sandbox::command(&plan)?;"),
     "let mut command = super::sandbox::command(&plan).unwrap_or_else(|_| { let mut c = std::process::Command::new(&entry.program); c.args(&plan.args); c });",
     "unavailable_never_unconfined_contract", "capability must reject"),
    ("untrusted-shell", "sandbox.rs",
     E(".is_some_and(|name| !SHELLS.contains(&name)),"), ".is_some_and(|_| true),",
     "untrusted_code_contract", "capability must reject"),
]
only = set(sys.argv[1].split(",")) if len(sys.argv) > 1 and sys.argv[1] else None
files = sorted({CAPS / p[1] for p in PROOFS})
original = {path: path.read_bytes() for path in files}
digest = lambda: {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
before = digest()
print("E20_PROOF_DIR=" + str(OUT), flush=True)

def cargo(label, test):
    log = OUT / (label + ".log")
    select = ["--exact", T + test] if test else ["app_capabilities_e20_", "app_capabilities_cap3_process"]
    args = ["cargo", "test", "--locked", "-p", "ctox", "--bin", "ctox", "--"] + select
    with log.open("w") as stream:
        code = subprocess.run(args, stdout=stream, stderr=subprocess.STDOUT, timeout=2400).returncode
    return code, log.read_text(errors="replace")

results = []
try:
    for label, name, pattern, replacement, test, fragment in PROOFS:
        if only and label not in only:
            continue
        path = CAPS / name
        source = original[path].decode()
        mutated, count = re.subn(pattern, lambda _m: replacement, source)
        if count != 1:
            results.append({"label": label, "status": "PATTERN_MISSING", "count": count})
            print("PATTERN_MISSING", label, count, flush=True)
            continue
        path.write_text(mutated)
        try:
            code, log = cargo(label, test)
        finally:
            path.write_bytes(original[path])
        ran = "1 passed" in log or "1 failed" in log
        red = code != 0 and "1 failed" in log and fragment in log
        status = "RED_AS_EXPECTED" if red else ("NOT_RUN" if not ran else "STAYED_GREEN_OR_WRONG_FAILURE")
        results.append({"label": label, "test": test, "exit": code, "status": status, "fragment": fragment})
        print(label, status, "exit=" + str(code), flush=True)
finally:
    for path, data in original.items():
        path.write_bytes(data)
after = digest()
restored = before == after
code, log = cargo("restored-green", "")
green = code == 0 and " 0 failed" in log
summary = {"restored_byte_identical": restored, "restored_green": green, "source_sha256": after, "results": results}
(OUT / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2), flush=True)
ok = restored and green and results and all(r["status"] == "RED_AS_EXPECTED" for r in results)
sys.exit(0 if ok else 1)
