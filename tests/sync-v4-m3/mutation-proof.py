#!/usr/bin/env python3
"""Bounded contract mutations; run only inside the admitted Linux lane."""
import hashlib
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[2]
out = pathlib.Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
browser = "src/apps/business-os/rxdb/src/webrtc-native.mjs"
handler = "src/core/rxdb/src/plugins/replication_webrtc/connection_handler_rs.rs"
router = "src/core/rxdb/src/plugins/replication_webrtc/index_mod.rs"
tests = "tests/sync-v4-m3/"
def node(name, file="hello-wiring.test.mjs"):
    return ["node", "--test", "--test-concurrency=2", "--test-name-pattern="+name, tests+file]
def cargo(name, fixture=False):
    args = ["cargo", "test", "--locked", "--manifest-path",
            tests+"native/Cargo.toml" if fixture else "src/core/rxdb/Cargo.toml"]
    return args + ([] if fixture else ["--lib"]) + [name, "--", "--test-threads=2"]

cases = [
 ("native-admission", handler, ".is_some_and(|router| !router.is_live(peer))",
  ".is_some_and(|_| false)", cargo("v4_router_never_admits"), "v4_router_never_admits"),
 ("native-deadline", router, "if config.handshake_timeout.is_zero()", "if false",
  cargo("v4_router_requires_a_bounded_deadline"), "v4_router_requires_a_bounded_deadline"),
 ("physical-open", browser, "      if (this.options.v4Hello) {",
  "      this.events.emit('peer-open', {peerId: connection.remotePeerId});\n      if (this.options.v4Hello) {",
  node("physical open"), "physical open"),
 ("incompatible-api", browser, "connection.v4Session.receive(JSON.stringify(frame));",
  "connection.v4Session.receive(JSON.stringify({...frame, app_api_ver:'4.1'}));",
  node("incompatible API"), "incompatible API"),
 ("paired-authority", browser, "if (verified !== true)", "if (false)",
  node("paired-server verification"), "paired-server verification"),
 ("generation-fence", browser, "        if (!current()) return;\n        if (hello?.device_proof?.nonce",
  "        if (hello?.device_proof?.nonce", node("retired asynchronous signer"), "retired asynchronous signer"),
 ("no-legacy-fallback", browser,
  "if (this.options.v4Hello && (['hello', 'hello_ok', 'hello_reject'].includes(payload?.type) || !connection.v4Live))",
  "if (false && (['hello', 'hello_ok', 'hello_reject'].includes(payload?.type) || !connection.v4Live))",
  node("configured V4 never falls back"), "configured V4 never falls back"),
 ("auxiliary-admission", browser, "    if (this.options.v4Hello && !connection.v4Live) {",
  "    if (false) {", node("auxiliary traffic"), "auxiliary traffic"),
 ("loss-and-duplicates", tests+"counters.mjs",
  "expected.size === actual.size && [...expected].every(id => actual.has(id)) && delivered.length === actual.size",
  "true", node("SCTP acceptance|duplicate delivery", "counter-contract.test.mjs"), "SCTP acceptance"),
 ("explicit-failure", tests+"counters.mjs", "explicitSendFailures: failed.length",
  "explicitSendFailures: 0", node("explicitly failed", "counter-contract.test.mjs"), "explicitly failed"),
 ("transport-parity", tests+"native/Cargo.lock", 'name = "webrtc"\nversion = "0.20.0-alpha.1"',
  'name = "webrtc"\nversion = "0.0.0"',
  node("identical native transport", "transport-parity.test.mjs"), "identical native transport"),
 ("device-key", tests+"native/src/main.rs", "if public != expected.public_key().as_ref() {", "if false {",
  cargo("m3_fixture_authenticates_only_registered_signed_devices", True),
  "m3_fixture_authenticates_only_registered_signed_devices"),
]
proof = []
for name, relative, old, new, command, expected in cases:
    path = root / relative
    original = path.read_bytes()
    text = original.decode()
    if text.count(old) != 1:
        raise RuntimeError(name+": mutation anchor must be unique")
    line = text[:text.index(old)].count("\n")+1
    try:
        path.write_text(text.replace(old,new,1))
        result = subprocess.run(command, cwd=root, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=150)
        log = result.stdout.decode(errors="replace")
        (out/(name+".log")).write_text(log)
        rejected = result.returncode != 0 and expected in log and ("FAILED" in log or "✖" in log or "not ok" in log)
        proof.append(dict(name=name,file=relative,line=line,removed=old,inserted=new,
                          command=command,exit=result.returncode,rejected=rejected,
                          original_sha256=hashlib.sha256(original).hexdigest()))
        (out/"mutation-proof.json").write_text(json.dumps(proof,indent=2))
        print(json.dumps({"mutation":name,"file":relative,"line":line,"rejected":rejected}),flush=True)
        if not rejected:
            raise RuntimeError(name+": did not produce the required named test failure")
    finally:
        path.write_bytes(original)
        assert path.read_bytes() == original

# Verify restored source, with bounded workers. Never ship a mutant.
gold = [
 ["node","--test","--test-concurrency=2", *map(str,sorted((root/tests).glob("*.test.mjs")))],
 cargo("v4_router"),
 cargo("m3_fixture_authenticates_only_registered_signed_devices",True),
 ["rustfmt","--check","--edition","2021","--config","skip_children=true",
  router,handler,tests+"native/src/main.rs"],
]
for index,command in enumerate(gold):
    result=subprocess.run(command,cwd=root,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=180)
    (out/("restored-"+str(index)+".log")).write_bytes(result.stdout)
    if result.returncode:
        raise RuntimeError("restored source validation failed: "+str(command))
print("restored source contracts and formatting passed",flush=True)
