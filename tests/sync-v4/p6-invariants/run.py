#!/usr/bin/env python3
"""Lane-only contract runner. Adds cfg(test) modules and always restores product files.

Run through gpu-build-run.sh; all diagnostics/artifacts stay in the assigned target.
No production fixes, substituted authority/policy, or persistent source edits.
"""
import argparse
import hashlib
import json
import os
import resource
from pathlib import Path
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
TARGET = Path(os.environ['CARGO_TARGET_DIR'])
OUT = TARGET / 'p6-invariants'
OUT.mkdir(parents=True, exist_ok=True)
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
ENV = dict(os.environ, RUST_TEST_THREADS='2', TOKIO_WORKER_THREADS='2', CARGO_BUILD_JOBS='2')
RECORDS = []
ORIGINALS = {}
ACTIVE = None

def save():
    (OUT / 'results.json').write_text(json.dumps(RECORDS, indent=2) + '\n')

def run(name, args, timeout=1200, expected=0):
    global ACTIVE
    start = time.monotonic()
    with (OUT / (name + '.log')).open('w') as log:
        ACTIVE = subprocess.Popen(args, cwd=ROOT, env=ENV, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = ACTIVE.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(ACTIVE.pid, signal.SIGTERM)
            try:
                ACTIVE.wait(timeout=8)
            except subprocess.TimeoutExpired:
                os.killpg(ACTIVE.pid, signal.SIGKILL)
                ACTIVE.wait()
            code = 124
        finally:
            ACTIVE = None
    output=(OUT / (name + '.log')).read_text()
    if '--nocapture' in args and 'running 1 test' not in output:
        code=125
    red_markers={
        'p6_noncanonical_input_cannot_reach_instance_signing':'raw noncanonical bytes reached the instance signing primitive',
        'p6_core_only_handler_has_native_origin_and_rejects_ui':'native core-origin event denied by E23.1 core-only rights',

    }
    if expected==101 and name in red_markers and red_markers[name] not in output:
        code=125
    record = dict(name=name, command=args, exit=code, expected=expected, elapsed_s=round(time.monotonic()-start,3))
    RECORDS.append(record)
    save()
    print('P6_RESULT', name, code, flush=True)
    if code != expected:
        print((OUT / (name + '.log')).read_text()[-7000:], flush=True)
    return code

def inject(path, child, inside_tests=False):
    file = ROOT / path
    before = file.read_bytes()
    ORIGINALS[file] = before
    addition = '\n#[cfg(test)]\n#[path = ' + json.dumps(str(HERE / child)) + ']\nmod p6_invariants;\n'
    text = before.decode()
    if inside_tests:
        pos = text.rfind('\n}')
        assert pos > 0
        text = text[:pos] + addition + text[pos:]
    else:
        text += addition
    file.write_text(text)

def binary_from_log(name, target_name):
    candidates=[]
    for line in (OUT / (name+'.log')).read_text().splitlines():
        try: data=json.loads(line)
        except ValueError: continue
        if data.get('reason')=='compiler-artifact' and data.get('target',{}).get('name')==target_name and data.get('profile',{}).get('test') and data.get('executable'):
            candidates.append(data['executable'])
    if not candidates:
        raise RuntimeError('test executable missing: '+name)
    return candidates[-1]

def stopped(signum, frame):
    if ACTIVE is not None:
        os.killpg(ACTIVE.pid, signal.SIGTERM)
    raise InterruptedError('bounded runner interrupted')

signal.signal(signal.SIGTERM, stopped)
signal.signal(signal.SIGINT, stopped)
parser=argparse.ArgumentParser()
parser.add_argument('--mutations', action='store_true')
options=parser.parse_args()
source_paths = [
    'src/core/service/store_startup.rs', 'src/core/service/store_native.rs', 'src/core/secrets.rs',
    'src/core/sync/src/authority/auth.rs', 'src/core/business_os/app_platform/runtime_host/codec.rs',
    'src/core/business_os/app_platform/runtime_host/authority.rs', 'src/core/app_runtime/lib.rs',
    'src/apps/business-os/kernel/transport-webrtc.mjs', 'src/apps/business-os/kernel/kernel.mjs',
    *[str(p.relative_to(ROOT)) for p in HERE.glob('*') if p.is_file()],
]
(OUT/'source-binding.json').write_text(json.dumps({
    'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
    'sources':{p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in source_paths}
},indent=2)+'\n')
try:
    node_candidates=[TARGET/'app-runtime-test-node/bin/node',Path('/mnt/nvme1/build-lane/target/v4-v4-app-runtime-bind/app-runtime-test-node/bin/node')]
    node=next((p for p in node_candidates if p.is_file()),None)
    if node is None:
        raise RuntimeError('lane-owned pinned Node bundle unavailable')
    ENV['CTOX_APP_RUNTIME_NODE_BUNDLE']=str(node)
    ENV['CTOX_APP_RUNTIME_TEST_NODE']=str(node)
    ENV['CTOX_APP_RUNTIME_TEST_ROOT']=str(OUT/'node-scratch')
    Path(ENV['CTOX_APP_RUNTIME_TEST_ROOT']).mkdir(exist_ok=True)
    (OUT/'node-binding.json').write_text(json.dumps({'path':str(node),'sha256':hashlib.sha256(node.read_bytes()).hexdigest()},indent=2)+'\n')
    js=[str(node),'--test',str(HERE/'document-signature.mjs')]
    run('document-signature',js,60)
    if options.mutations:
        transport=ROOT/'src/apps/business-os/kernel/transport-webrtc.mjs'
        baseline=transport.read_bytes()
        old=b"if(!await this.options.verifyPackage(message,this.session))throw new CtoxError('forbidden','Page signature invalid');"
        assert baseline.count(old)==1
        try:
            transport.write_bytes(baseline.replace(old,b"if(false)throw new CtoxError('forbidden','Page signature invalid');"))
            run('mutation-document-signature-red',js,60,1)
        finally:
            transport.write_bytes(baseline)
        run('restored-document-signature',js,60)
    inject('src/core/service/store_startup.rs','identity.rs')
    inject('src/core/business_os/app_platform/runtime_host/runtime_tests.rs','codec.rs')
    inject('src/core/business_os/app_platform/runtime_host/native_proof.rs','origin.rs')
    inject('src/core/app_runtime/lib.rs','resources.rs',inside_tests=True)
    appcargo=['cargo','test','--locked','--jobs','2','--manifest-path','src/core/app_runtime/Cargo.toml','--lib','--no-run','--message-format=json']
    if run('process-build',appcargo):
        raise RuntimeError('process test build unverified')
    appbinary=binary_from_log('process-build','ctox_app_runtime')
    for test in ['p6_process_cpu_budget_kills_only_the_app_and_allows_replacement','p6_process_result_budget_rejects_oversized_message']:
        run(test,[appbinary,test,'--nocapture','--test-threads=1'],60)
    run('p6_process_old_space_heap_limit_is_enforced',[appbinary,'p6_process_old_space_heap_limit_is_enforced','--nocapture','--test-threads=1'],60)
    cargo=['cargo','test','--locked','--jobs','2','-p','ctox','--bin','ctox','--features','app-runtime-node','--no-run','--message-format=json']
    if run('root-build',cargo):
        raise RuntimeError('root test build unverified')
    binary=binary_from_log('root-build','ctox')
    for test, expected, timeout in [
        ('p6_identity_concurrent_creators_restart_and_corrupt_store',0,120),
        ('p6_identity_two_process_first_start_has_one_stored_key',0,120),
        ('p6_signing_errors_do_not_echo_input_key_material',0,30),
        ('p6_signing_identity_cannot_implement_debug',0,30),
        ('p6_noncanonical_input_cannot_reach_instance_signing',101,30),
        ('p6_revocation_in_writer_transaction_prevents_effect',0,30),
        ('p6_manifest_isolates_foreign_collection_read_and_write',0,30),
        ('p6_invalid_release_signature_never_admits_document_effect',0,30),
        ('p6_blob_bytes_require_digest_size_and_private_cas',0,30),
        ('p6_core_only_handler_has_native_origin_and_rejects_ui',101,180),
        ('foreground_retains_real_native_authority_after_sixty_seconds_idle',0,180),
    ]:
        run(test,[binary,test,'--nocapture','--test-threads=1'],timeout,expected)

    if options.mutations:
        process=ROOT/'src/core/app_runtime/lib.rs'
        process_baseline=process.read_text()
        old='--max-old-space-size=256'
        assert process_baseline.count(old)==1
        process.write_text(process_baseline.replace(old,'--max-old-space-size=512'))
        try:
            if run('mutation-heap-build',appcargo)==0:
                mutated=binary_from_log('mutation-heap-build','ctox_app_runtime')
                run('mutation-heap-red',[mutated,'p6_process_old_space_heap_limit_is_enforced','--nocapture','--test-threads=1'],60,101)
        finally:
            process.write_text(process_baseline)
        if run('restored-heap-build',appcargo)==0:
            restored=binary_from_log('restored-heap-build','ctox_app_runtime')
            run('restored-heap',[restored,'p6_process_old_space_heap_limit_is_enforced','--nocapture','--test-threads=1'],60)
        codec=ROOT/'src/core/business_os/app_platform/runtime_host/codec.rs'
        baseline=codec.read_text()
        old='if binding["fence"]["rights_ver"].as_u64() != Some(r.rights_ver) {'
        assert baseline.count(old)==1
        codec.write_text(baseline.replace(old,'if false && binding["fence"]["rights_ver"].as_u64() != Some(r.rights_ver) {'))
        try:
            if run('mutation-policy-build',cargo)==0:
                mutated=binary_from_log('mutation-policy-build','ctox')
                run('mutation-policy-red',[mutated,'p6_revocation_in_writer_transaction_prevents_effect','--nocapture','--test-threads=1'],30,101)
        finally:
            codec.write_text(baseline)
        old='|| record["signature"]\n                != super::super::commands::sign_manifest(self.signer.as_ref(), &bytes)?'
        assert baseline.count(old)==1
        codec.write_text(baseline.replace(old,'|| false'))
        try:
            if run('mutation-signature-build',cargo)==0:
                mutated=binary_from_log('mutation-signature-build','ctox')
                run('mutation-signature-red',[mutated,'p6_invalid_release_signature_never_admits_document_effect','--nocapture','--test-threads=1'],30,101)
        finally:
            codec.write_text(baseline)
        if run('restored-build',cargo)==0:
            restored=binary_from_log('restored-build','ctox')
            for test in ['p6_revocation_in_writer_transaction_prevents_effect','p6_invalid_release_signature_never_admits_document_effect']:
                run('restored-'+test,[restored,test,'--nocapture','--test-threads=1'],30)
    # A red target is preserved as red, even when it is the expected review result.
    bad=[r for r in RECORDS if r['exit']!=r['expected']]
    raise SystemExit(1 if bad else 0)
finally:
    for path, original in ORIGINALS.items():
        path.write_bytes(original)
    (OUT/'restoration.json').write_text(json.dumps({str(p.relative_to(ROOT)):p.read_bytes()==data for p,data in ORIGINALS.items()},indent=2)+'\n')
