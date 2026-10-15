#!/usr/bin/env python3
"""Lane-only contract and mutation proof. No tenant, profile or fake model."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

root = Path(__file__).resolve().parents[3]
os.chdir(root)
target = Path(os.environ["CARGO_TARGET_DIR"])
proof = target / "rt2-proof"
proof.mkdir(exist_ok=True)
runtime = Path("src/core/app_runtime")
env = os.environ.copy()
env["RUST_TEST_THREADS"] = "2"
node = target / "app-runtime-test-node/bin/node"
records = []

def run(label, args, expected=0, timeout=900):
    started = time.time()
    with (proof / (label + ".log")).open("w") as log:
        process = subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT, env=env, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                code = process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait()
            raise RuntimeError(f"{label}: owned process group timed out ({code})")
    output = (proof / (label + ".log")).read_text()
    records.append({"label": label, "exit": code, "elapsed_seconds": time.time()-started})
    (proof / "proof.json").write_text(json.dumps(records, indent=2))
    print(f"{label}: exit {code}", flush=True)
    if (expected == 0 and code != 0) or (expected != 0 and code == 0):
        print(output[-12000:], flush=True)
        raise RuntimeError(f"{label}: unexpected exit")
    if expected != 0:
        assert "test result: FAILED" in output or re.search(r"# fail [1-9]", output), output[-5000:]
        assert "could not compile" not in output, "compile errors are not mutation evidence"
    return output

def named(label, suffix, root_test=False, expected=0):
    args = ["cargo", "test", "--locked"]
    if root_test:
        args += ["-p", "ctox", "--bin", "ctox", "--features", "app-runtime-node"]
    else:
        args += ["--manifest-path", str(runtime / "Cargo.toml"), "--features", "bundled-node"]
    listing = run(label+"-selection", args+[suffix,"--","--list"])
    names = re.findall(r"^(.+): test$", listing, re.M)
    names = [n for n in names if n.endswith(suffix)]
    assert len(names) == 1, f"missing or ambiguous test selector: {suffix}"
    return run(label, args+[names[0],"--","--exact","--test-threads=2"], expected=expected)

def mutate(label, path, before, after, suffix, root_test=False):
    path = Path(path)
    original = path.read_bytes()
    source = original.decode()
    assert source.count(before) == 1, (path, before, source.count(before))
    line = source[:source.index(before)].count("\n")+1
    try:
        path.write_text(source.replace(before, after))
        named(label+"-red", suffix, root_test, expected=1)
        records.append({"mutation": label, "path": str(path), "line": line, "before": before, "after": after})
    finally:
        path.write_bytes(original)
    named(label+"-restored", suffix, root_test)

def lock_packages(path):
    # Lane Python may predate 3.11/tomllib. Cargo's generated package identity
    # fields are single-line quoted strings; never parse dependency arrays.
    packages = set()
    for block in re.split(r"(?m)^\[\[package\]\]\s*$", path.read_text())[1:]:
        fields = {}
        for field in ("name", "version", "source"):
            match = re.search(r'^' + field + r' = ("[^"\n]*")$', block, re.M)
            if match:
                fields[field] = json.loads(match[1])
        assert "name" in fields and "version" in fields, "invalid Cargo.lock package identity"
        packages.add((fields["name"], fields["version"], fields.get("source")))
    assert packages, "empty Cargo.lock package selection"
    return packages

run("node-prepare", ["bash", str(runtime / "verify.sh"), "prepare"])
env["CTOX_APP_RUNTIME_NODE_BUNDLE"] = str(node)
env["CTOX_APP_RUNTIME_TEST_NODE"] = str(node)
env["CTOX_APP_RUNTIME_TEST_ROOT"] = str(target / "ipc-tests")
old_runtime = lock_packages(runtime / "Cargo.lock")
run("runtime-lock-check", ["cargo", "check", "--manifest-path", str(runtime / "Cargo.toml"), "--features", "bundled-node"])
new_runtime = lock_packages(runtime / "Cargo.lock")
assert old_runtime <= new_runtime, "runtime lock changed existing package versions; review required"
(proof / "runtime-lock-added.json").write_text(json.dumps(sorted(new_runtime-old_runtime,key=str),indent=2))
run("component", ["bash", str(runtime / "verify.sh"), "bundle"])

mutate("active-command-version", runtime / "service.rs",
       '.is_some_and(|release| release.version == version)',
       '.is_some_and(|_| true)',
       "completion_uses_original_cas_export_and_is_at_most_once_across_versions")
mutate("sender-export", runtime / "ipc.rs",
       'sender["commands_out"]', 'serde_json::json!([{"app":to,"type":kind}])',
       "core_foreign_command_requires_current_bilateral_exports_and_user_policy_before_writer_effect")
mutate("shell-pin", runtime / "worker.mjs",
       'if(shellVersion!==shared.version){modules.clear();shellVersion=shared.version;}',
       'if(false){modules.clear();shellVersion=shared.version;}',
       "active_shell_shared_cas_is_pinned_and_module_generation_relinks")
mutate("callback-once", runtime / "state.rs", 'if state != "pending" {', 'if false {',
       "command_callback_is_owned_by_source_and_cancelled_delivers_once_with_workspace_refs")
mutate("publish-hook", runtime / "registration.rs",
       'daemon.published(app,ver).await.map(|_|Value::Null)',
       'daemon.removed(app).await.map(|_|Value::Null)',
       "registered_core_ingress_hotplugs_late_cas_app_and_drains_before_writer_shutdown")

old = lock_packages(Path("Cargo.lock"))
run("core-prep", ["ctox-prep.sh"])
run("core-check", ["cargo","check","-p","ctox","--features","app-runtime-node"])
new = lock_packages(Path("Cargo.lock"))
assert old <= new, "foreign package removals/version changes need review"
(proof / "lock-added.json").write_text(json.dumps(sorted(new-old,key=str),indent=2))
named("native-bridge", "native_registry_bridge_preserves_core_fence_and_executes_real_cap2", True)
mutate("native-effect-fence", runtime / "native_adapter.rs",
       '        let ctx = convert(ctx).map_err(|_| native::CtoxError::uncertain())?;\n        contract::CapabilityHost::with_fence',
       '        return effect();\n        let ctx = convert(ctx).map_err(|_| native::CtoxError::uncertain())?;\n        contract::CapabilityHost::with_fence',
       "native_registry_bridge_preserves_core_fence_and_executes_real_cap2", True)
run("final-locked-check", ["cargo","check","--locked","-p","ctox","--features","app-runtime-node"])
run("final-diff-check", ["git","diff","--check"])
sources = {str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in runtime.rglob("*") if p.is_file() and "target" not in p.parts and not any(part.startswith(".") for part in p.parts)}
(proof / "source-hashes.json").write_text(json.dumps(sources,indent=2))
(proof / "proof.json").write_text(json.dumps(records,indent=2))
print("rt2 contract proof complete; real daemon acceptance still requires CUTOVER registration",flush=True)
