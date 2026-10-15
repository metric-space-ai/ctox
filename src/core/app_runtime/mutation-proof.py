#!/usr/bin/env python3
"""Bounded contract mutation proof. Run only inside gpu-build-run.sh.
Mutates only this crate's lane copy, restores exact bytes in finally, and rejects
compiler/import failures as mutation evidence. Never operates on a tenant.
"""
import argparse, hashlib, json, os, re, subprocess
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--out", required=True)
args = parser.parse_args()
out = Path(args.out)
out.mkdir(parents=True, exist_ok=True)
base = Path("src/core/app_runtime")
node = Path(os.environ["CARGO_TARGET_DIR"]) / "app-runtime-test-node/bin/node"
os.environ["CTOX_APP_RUNTIME_NODE_BUNDLE"] = str(node)
os.environ["CTOX_APP_RUNTIME_TEST_NODE"] = str(node)
os.environ["CTOX_APP_RUNTIME_TEST_ROOT"] = str(Path(os.environ["CARGO_TARGET_DIR"]) / "ipc-tests")
native = ["cargo", "test", "--locked", "--manifest-path", str(base/"Cargo.toml"),
          "--features", "bundled-node"]
kit = [str(node), "--test", "--test-reporter=tap"]
cases = [
    ("command-envelope", "lib.rs", r'"input":input,', '"input":input["payload"],',
     "core_dispatch_preserves_envelope_and_writes_via_the_single_writer"),
    ("transaction-atomicity", "core_host.rs",
     r'self\.write_batch\(binding, Some\(snapshot\.snapshot\.stamp\), operations\)',
     'for operation in operations { self.write_batch(binding,None,vec![operation])?; } Ok(json!([]))',
     "transaction_is_one_atomic_store_job_and_failed_second_write_rolls_back_first"),
    ("snapshot-conflict", "core_host.rs", r'Some\(snapshot\.snapshot\.stamp\)', 'None',
     "transaction_snapshot_rejects_concurrent_commit_and_tokens_cannot_cross_calls"),
    ("admission-before-code", "lib.rs", r'self\.inner\.host\.admit\(&bundle\.manifest, binding\)\?;', '/* deliberately skipped admission */',
     "command_rights_rejects_before_node_start_even_with_forged_admin_flag"),
    ("cap-wire-arguments", "core_host.rs", r'args: native_args,', 'args: json!({"payload":{}}),',
     "native_registry_routes_cap_invoke_with_policy_fence_and_audit"),
    ("publish-drain", "lib.rs", r'// Publication drains the running call before changing generations\.',
     'old.interrupt.store(true, std::sync::atomic::Ordering::SeqCst);',
     "published_cas_version_restarts_after_running_call_finishes_and_refreshes_helpers"),
    ("completion-once", "state.rs", r'if state != "pending"', 'if false',
     "completion_uses_original_cas_export_and_is_at_most_once_across_versions"),
    ("timer-due-state", "state.rs", r'if now < next', 'if false',
     "timer_trigger_minimum_is_one_minute_and_due_time_survives_service_restart"),
    ("embedded-node-integrity", "delivery.rs", r'Sha256::digest\(&current\) == Sha256::digest\(NODE\)', 'true',
     "production_uses_embedded_node_and_rejects_changed_delivered_bytes"),
    ("foreign-watch-authority", "service.rs",
     r'if release\.manifest\["collections"\]\s*\.get\(&event\.collection\)\s*\.is_none\(\)', 'if false',
     "change_stream_triggers_watch_exports_and_durable_cursor_and_skip_replay"),
    ("native-function-needs-update", "core_host.rs", r'matches\.len\(\) == 1,', 'true,',
     "unsupported_native_enqueue_is_explicit_without_effect"),
    ("kit-envelope", "handler-kit/index.mjs", r'\.\.\.input\?\.type \? clone\(input\) : \{\}', '...{}',
     "full command envelope and actor metadata, static relative helper import, globals and UUID"),
    ("kit-cas-generation", "handler-kit/index.mjs", r"const base = 'ctox-handler-kit://'\+randomUUID\(\)\+'/'", "const base = 'ctox-handler-kit://fixed/'",
     "CAS generations invalidate relative-library cache, and reject import escape"),
    ("kit-rollback", "handler-kit/index.mjs", r'collections:clone\(collections\)', 'collections:collections',
     "transactions atomically commit, roll back errors and prohibit outer-context effects"),
    ("kit-serializable", "handler-kit/index.mjs", r'if \(state\.revision !== revision\)', 'if (false)',
     "concurrent modification rejects transaction snapshot rather than losing another write"),
    ("kit-command-options", "handler-kit/index.mjs", r'fixtures\.command\(type,clone\(payload\),clone\(opts\)\)', 'fixtures.command(type,clone(payload),{})',
     "commands return acceptance plus task_id and preserve options; cursors persist per trigger"),
    ("kit-command-rights", "handler-kit/index.mjs", r'commandRights\(manifest,command,fixtures\);', '/* deliberately skipped rights */',
     "command_rights rejects before loading/executing handler and foreign watch is explicit"),
    ("kit-delete-payload", "handler-kit/index.mjs", r"next = \{ id, rev, deleted:true \}", "next = { ...current, id, rev, deleted:true }",
     "remove leaves only tombstone metadata and rejects stale resurrection; object CtoxError"),
]
# Cursor authority is checked again at the persisted state boundary.
cases.append(("cursor-not-client-metadata", "state.rs",
    r'if binding\["kind"\] != "trigger"', 'if false',
    "client_context_cannot_grant_trigger_cursor_authority"))

sources = {}
for _, file, *_ in cases:
    path = base/file
    sources[path] = path.read_bytes()
sources[base/"state.rs"] = (base/"state.rs").read_bytes()
report = {"source_hashes": {str(p):hashlib.sha256(b).hexdigest() for p,b in sources.items()},
          "proofs":[], "restored":False}

def run(command, label):
    completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                               text=True, timeout=180)
    (out/(label+".log")).write_text(completed.stdout)
    return completed.returncode, completed.stdout

def native_command(name):
    return native + [name, "--", "--test-threads=2"]

def kit_command(name):
    return kit + ["--test-name-pattern="+re.escape(name), str(base/"handler-kit/contract.test.mjs")]

try:
    code, output = run(native_command("core_tests"),"baseline-native")
    if code: raise RuntimeError("baseline native contract is red")
    code, output = run(kit + [str(base/"handler-kit/contract.test.mjs")],"baseline-kit")
    if code: raise RuntimeError("baseline kit contract is red")
    for label, file, pattern, replacement, test in cases:
        path = base/file
        original = sources[path].decode()
        matches = list(re.finditer(pattern,original))
        expected = 2 if label=="kit-serializable" else 1
        if len(matches)!=expected: raise RuntimeError("mutation anchor mismatch: "+label)
        record={"invariant":label,"file":str(path),
                "lines":[original.count("\n",0,m.start())+1 for m in matches],
                "changed":[m.group() for m in matches],"replacement":replacement,"test":test}
        try:
            path.write_text(re.sub(pattern,lambda _:replacement,original))
            cmd = kit_command(test) if file.startswith("handler-kit/") else native_command(test)
            code, output = run(cmd,label)
            reached = test in output and ("not ok" in output if file.startswith("handler-kit/") else "FAILED" in output)
            detected = code!=0 and reached and "could not compile" not in output and "SyntaxError" not in output
            record.update({"mutant_exit":code,"reached_test_failure":reached,"detected":detected,"log":label+".log"})
            report["proofs"].append(record)
            if not detected: raise RuntimeError("mutation survived or invalid mutation: "+label)
            print("MUTATION_PROOF "+label+" detected",flush=True)
        finally:
            for source,content in sources.items(): source.write_bytes(content)
    code, output = run(native_command("core_tests"),"restored-native")
    if code: raise RuntimeError("restored native contract is red")
    code, output = run(kit + [str(base/"handler-kit/contract.test.mjs")],"restored-kit")
    if code: raise RuntimeError("restored kit contract is red")
    report["restored"] = all(p.read_bytes()==b for p,b in sources.items())
    if not report["restored"]: raise RuntimeError("source restoration mismatch")
finally:
    for source,content in sources.items(): source.write_bytes(content)
    (out/"proof.json").write_text(json.dumps(report,indent=2)+"\n")
