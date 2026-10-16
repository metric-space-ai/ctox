#!/usr/bin/env python3
"""Run only in the assigned GPU lane. Default exits red for any regression.

The root binary is built from the requested train before test-only wiring.
Private modules are wired only in the lane checkout and restored in finally.
R13 uses the train's actual rights authority. The legacy #827 composition is
retained only for reproducing the original review baseline.
"""
import argparse, hashlib, json, os, re, shutil, signal, subprocess, sys, time
from pathlib import Path

ROOT = Path.cwd()
HERE = ROOT / 'tests/sync-v4/p6-review-4'
TRAIN = 'a6dda5aab750a1020cd0ae0a5b9af4276b2ce8bb'
RIGHTS = "7f972f89b72aa4da7af19b5fae07d50f23c6a600"
USE_CURRENT_RIGHTS = (ROOT / "src/core/store/rights.rs").is_file()
if USE_CURRENT_RIGHTS:
    RIGHTS = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()

parser = argparse.ArgumentParser()
parser.add_argument('--train', default=TRAIN, help='immutable product train revision, including any controller fix')
parser.add_argument('--turn-bin', help='explicit existing lane coturn executable')
parser.add_argument('--turn-lib', help='userspace coturn dependency directory')
parser.add_argument('--namespace-mutation', action='store_true', help='qualify namespace lifecycle and mutate away the net namespace flag')
parser.add_argument('--only', choices=['all', 'regressions', 'r10', 'r13', 'r14', 'scenarios']
, default='all')
parser.add_argument('--deadline',type=float,help='absolute UTC Unix deadline; reserve time for owned-process cleanup')
parser.add_argument('--expect-red', action='store_true', help='qualify specifically reproduced assertion failures; default remains a red target')
args = parser.parse_args()
TRAIN = subprocess.check_output(['git','rev-parse',args.train],text=True).strip()
subprocess.run(['git','merge-base','--is-ancestor',TRAIN,'HEAD'],check=True)
subprocess.run(['git','diff','--exit-code',TRAIN,'--','src','Cargo.toml','Cargo.lock'],check=True)
assert sys.platform == 'linux' and os.environ.get('BUILD_LANE_BIN') and os.environ.get('CARGO_TARGET_DIR'), 'canonical GPU lane required'
os.environ.setdefault('TOKIO_WORKER_THREADS','2')
# Existing build-only switch: these contracts never exercise optional local
# inference engines. Avoid reusing CMake source-directory caches from the seed.
os.environ['CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS']='1'
TARGET = Path(os.environ['CARGO_TARGET_DIR'])
OUT = TARGET / ('p6-review4-' + time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()))
OUT.mkdir(parents=True)
STEPS = []
(OUT/"test-source-binding.json").write_text(json.dumps({str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in HERE.iterdir() if p.is_file()},indent=2))

RESULTS = {}
ACTIVE = None
def interrupted(signum, _frame):
    if ACTIVE is not None and ACTIVE.poll() is None:
        try: os.killpg(ACTIVE.pid,signal.SIGTERM)
        except ProcessLookupError: pass
        try: ACTIVE.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try: os.killpg(ACTIVE.pid,signal.SIGKILL)
            except ProcessLookupError: pass
            ACTIVE.wait()
    raise SystemExit(128+signum)
signal.signal(signal.SIGTERM,interrupted)
signal.signal(signal.SIGINT,interrupted)

def digest(path):
    hash = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024*1024), b''): hash.update(chunk)
    return hash.hexdigest()
def incident_summary(stdout):
    match=re.search(r'^K13 \| .* \| (.+)/K13/result\.json$',stdout,re.M)
    assert match, 'namespace contract: incident summary path absent'
    path=Path(match.group(1))/'p6-result.json'
    return path,json.loads(path.read_text())
def private_namespaces(summary):
    rows=summary.get('scenarios',[])
    return summary.get('terminal') is True and {r['id'] for r in rows}=={'K13','K21','K22'} and len(rows)==3 and all(
        r.get('namespaceVerified') is True and r.get('cleanup') is True for r in rows)
def save():
    (OUT/'results.json').write_text(json.dumps(dict(train=TRAIN, rights_composition=RIGHTS,
        source_head=subprocess.check_output(['git','rev-parse','HEAD'], text=True).strip(),
        steps=STEPS, results=RESULTS), indent=2)+'\n')
def run(name, command, timeout=1200, **kwargs):
    global ACTIVE
    if args.deadline is not None:
        remaining=int(args.deadline-time.time()-20)
        if remaining<1: raise SystemExit('order deadline: remaining targets build unverified')
        timeout=min(timeout,remaining)
    at=time.time()
    child=subprocess.Popen([str(x) for x in command], stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,text=True,start_new_session=True,**kwargs)
    ACTIVE=child
    (OUT/(name+'-process.json')).write_text(json.dumps(dict(owner='01a14204-d613',
        pid=child.pid,pgid=child.pid,purpose=name,output=str(OUT),stop=f'terminal or {timeout}s'),indent=2))
    try:
        stdout,_=child.communicate(timeout=timeout)
        code=child.returncode
    except subprocess.TimeoutExpired:
        os.killpg(child.pid,signal.SIGTERM)
        try: stdout,_=child.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid,signal.SIGKILL)
            stdout,_=child.communicate()
        code=124
    p=subprocess.CompletedProcess(command,code,stdout)
    ACTIVE=None
    p.pid=child.pid
    (OUT/(name+'.log')).write_text(p.stdout)
    STEPS.append(dict(name=name, command=[str(x) for x in command], exit=p.returncode, seconds=time.time()-at))
    save(); print(name, 'exit', p.returncode, flush=True)
    if p.returncode: print(p.stdout[-10000:], flush=True)
    return p

def test_binary(name, command):
    p=run(name, command+['--no-run','--message-format=json'], 1800)
    assert p.returncode == 0, 'build unverified: '+name
    bins=[]
    for line in p.stdout.splitlines():
        try: row=json.loads(line)
        except ValueError: continue
        if row.get('reason')=='compiler-artifact' and row.get('profile',{}).get('test') and row.get('executable'):
            bins.append(row['executable'])
    assert len(bins)==1, bins
    return bins[0]

def wire(path, source, name):
    original=path.read_bytes()
    path.write_bytes(original+f'\n#[cfg(test)]\n#[path = {json.dumps(str(source))}]\nmod {name};\n'.encode())
    return original

# Reflink only immutable-looking cargo outputs into this order's own new cache.
# No build shares a writable target. Failure to reflink is not a copy fallback.
if not (TARGET/'debug/deps').exists():
    seed=Path('/mnt/sata8t/lane/target/v4-train')
    if seed.exists() and seed.resolve()!=TARGET.resolve():
        p=run('cache-reflink', ['cp','-a','--reflink=always',str(seed/'debug'),str(TARGET/'debug')],180)
        assert p.returncode==0, 'cache seed failed; no system-volume fallback'

run('diff-check',['git','diff','--check'])
for path in sorted((ROOT/'tests/sync-v4/k13').glob('*.mjs')):
    assert run('syntax-'+path.stem,['node','--check',path]).returncode==0
assert run('existing-oracles',['node','--test',ROOT/'tests/sync-v4/k13/oracles.test.mjs']).returncode==0

if args.only in ('all','scenarios'):
    boundary=OUT/'namespace-negative.json'
    boundary.write_text(json.dumps(dict(uid=os.getuid(),gid=os.getgid(),
        hostNamespaces={kind:os.readlink('/proc/self/ns/'+kind) for kind in ['user','net','mnt','pid']})))
    negative=run('namespace-host-entry-rejected',['node',ROOT/'tests/sync-v4/k13/netns.mjs',boundary],10)
    assert negative.returncode!=0 and 'Private mapped user/net/mount/PID namespaces required' in negative.stdout

if args.only in ('all','scenarios'):
    p=run('train-daemon-build',['cargo','build','--locked','--bin','ctox','--jobs','2'],1800)
    if p.returncode==0:
        binary=TARGET/'debug/ctox'
        receipt=OUT/'train-build.json'
        receipt.write_text(json.dumps(dict(pass_=True, **{'pass':True}, terminal=True,
            revision=TRAIN,binary=str(binary),binarySha256=digest(binary),source=str(ROOT)),indent=2))
        p=run('train-incidents',['node',ROOT/'tests/sync-v4/k13/run.mjs',
            '--build',receipt,'--expected-revision',TRAIN]+
            (['--turn-bin',args.turn_bin] if args.turn_bin else [])+
            (['--turn-lib',args.turn_lib] if args.turn_lib else []),2700)
        # The default runner creates a new private artifact output; locate it
        # from the printed evidence path rather than guessing a producer pass.
        RESULTS['scenarios']=dict(exit=p.returncode,log=str(OUT/'train-incidents.log'))
        if args.namespace_mutation:
            summary_path,summary=incident_summary(p.stdout)
            assert private_namespaces(summary), 'namespace contract RED: expected private namespaces or cleanup absent'
            RESULTS['native_entry']=dict(exit=0 if all(row.get('daemonExecuted') is True and row.get('entryNamespaceBound') is True for row in summary['scenarios']) else 1,
                evidence=str(summary_path))
            launcher=ROOT/'tests/sync-v4/k13/run.mjs'
            original=launcher.read_text()
            before="'--map-root-user','--net','--mount'"
            assert original.count(before)==1, 'namespace mutation must select exactly the real launcher flag'
            mutant=launcher.with_name('.p6-netns-mutant.mjs')
            assert not mutant.exists(), 'never overwrite another mutation run'
            mutant.write_text(original.replace(before,"'--map-root-user','--mount'"))
            try:
                mutation=run('namespace-mutation-no-net',['node',mutant,'--build',receipt,'--expected-revision',TRAIN]+
                    (['--turn-bin',args.turn_bin] if args.turn_bin else [])+
                    (['--turn-lib',args.turn_lib] if args.turn_lib else []),90)
                mutant_path,mutated=incident_summary(mutation.stdout)
                assert mutation.returncode!=0 and not private_namespaces(mutated), 'namespace mutation survived'
                for row in mutated['scenarios']:
                    assert row.get('daemonExecuted') is False
                    log=Path(row['evidence']).parent.parent/(row['id']+'.log')
                    assert 'Private mapped user/net/mount/PID namespaces required' in log.read_text()
                RESULTS['namespace_contract']=dict(exit=0,positive=str(summary_path),mutant=str(mutant_path),
                    source=str(launcher.relative_to(ROOT)),line=next(i for i,l in enumerate(original.splitlines(),1) if before in l),
                    mutation='remove --net from the real namespace launcher; private_namespaces contract turns RED before setup',
                    source_sha256=digest(launcher),mutant_sha256=digest(mutant))
                save()
            finally:
                mutant.unlink()
    else: RESULTS['scenarios']=dict(status='build unverified',exit=p.returncode)
    save()

if args.only in ('all','regressions','r10','r14'):

    # Wire private test modules in a disposable source copy. Supplied foreign
    # production sources remain untouched even in the lane checkout.
    # Reuse this order's first source-copy location so local path dependencies
    # keep their package identities on a corrective rerun.
    copies=sorted(TARGET.glob('p6-review4-*/root-test-source'))
    overlay=copies[0] if copies else OUT/'root-test-source'
    assert overlay.resolve().is_relative_to(TARGET.resolve()), 'source copy escaped the order target'
    if overlay.exists(): shutil.rmtree(overlay)
    shutil.copytree(ROOT,overlay,symlinks=True,ignore=shutil.ignore_patterns('.git','target','node_modules','.greppy'))
    if not USE_CURRENT_RIGHTS:
        # Train7's bare policy has E0308 (36 vs 37). This is the existing #827
        # composition, not a product patch. Target caller/adapter bytes stay
        # exactly train7; the policy dependency is recorded independently.
        relative_files=['src/core/business_os/policy.rs']+['src/core/store/'+name+'.rs' for name in
            ['rights','scope','manifest_rules','rights_matrix_tests','rights_tests','scope_tests']]+['src/core/rxdb/src/plugins/replication_webrtc/scope_filter.rs']
        for relative in relative_files:
            dest=overlay/relative;dest.parent.mkdir(parents=True,exist_ok=True)
            dest.write_bytes(subprocess.check_output(['git','show',RIGHTS+':'+relative]))
        RESULTS['root_policy_composition']=dict(revision=RIGHTS,target_callers=TRAIN);save()
    sources={overlay/'src/core/service/store_startup.rs':(HERE/'r10.rs','p6_review4'),
             overlay/'src/core/store/lcm_adapter.rs':(HERE/'r14.rs','p6_review4')}
    originals={}
    try:
        for path,(source,name) in sources.items(): originals[path]=wire(path,source,name)
        exe=test_binary('train-tests-build',['cargo','test','--manifest-path',overlay/'Cargo.toml','--locked','--bin','ctox','--jobs','2'])

        if args.only in ('all','regressions','r10'):

            p=run('r10',[exe,'service::store_startup::p6_review4::r10_failed_start_must_retain_custody_until_engine_drains','--exact','--nocapture','--test-threads=1'],60)
            effect=re.search(r'R10_EFFECT_STARTED pid=(\d+)',p.stdout)
            assert effect and 'R10_DRAIN_FAILED' in p.stdout, 'R10 failed-drain path did not execute'
            reproduced=p.returncode!=0 and ('R10: maintenance acquired custody' in p.stdout or 'R10: Foreground start returned' in p.stdout)
            failstop=False
            # The alternative allowed by the contract is a confirmed WHOLE
            # daemon/effect fail-stop. A random panic/segfault is not accepted.
            if not reproduced and p.returncode in (1,-9,-15,-6,134) and 'test result: FAILED' not in p.stdout:
                deadline=time.monotonic()+2
                while time.monotonic()<deadline:
                    try: os.killpg(p.pid,0)
                    except ProcessLookupError:
                        failstop=not Path('/proc',effect.group(1)).exists()
                        break
                    time.sleep(.05)
            # Reap/stop any retained effect after independently observing it.
            if Path('/proc',effect.group(1)).exists():
                try:
                    if os.getpgid(int(effect.group(1)))==p.pid:
                        os.killpg(p.pid,signal.SIGKILL)
                except ProcessLookupError: pass
            RESULTS['r10']=dict(exit=0 if failstop else p.returncode,
                reproduced=reproduced,whole_group_failstop=failstop)
            save()
        if args.only in ('all','regressions','r14'):

            assert shutil.which('strace'), 'R14 build unverified: strace missing'
            p=run('r14-probe',['strace','-ff','-s','4096','-e','trace=%file,write','-o',OUT/'r14-strace',exe,
                'context::lcm::owner_adapter::p6_review4::r14_attach_syscall_probe','--exact','--nocapture','--test-threads=1'],60)
            assert p.returncode==0 and 'R14_PATH ' in p.stdout, 'R14 probe did not execute'
            match=re.search(r'R14_PATH (.+)',p.stdout); path=match.group(1).strip()
            violations=[];control_violations=[];polls=0;control_polls=0
            for trace in OUT.glob('r14-strace.*'):
                active=False;control_active=False
                for number,line in enumerate(trace.read_text().splitlines(),1):
                    if 'R14_POLL_BEGIN tid=' in line: active=True;polls+=1
                    elif 'R14_POLL_END tid=' in line: active=False
                    elif 'R14_CONTROL_POLL_BEGIN tid=' in line: control_active=True;control_polls+=1
                    elif 'R14_CONTROL_POLL_END tid=' in line: control_active=False
                    elif (active or control_active) and ('p6-r14-owned.sqlite3' in line or str(Path(path).parent) in line) and re.match(r'(statx|newfstatat|readlink|readlinkat|lstat|stat|access)\(',line):
                        (violations if active else control_violations).append(dict(trace=trace.name,line=number,syscall=line))
            assert polls>0, 'R14 syscall observer did not see async polls'
            assert control_polls>0 and not control_violations, 'R14 blocking-pool positive control failed'
            RESULTS['r14']=dict(exit=1 if violations else 0,reproduced=bool(violations),polls=polls,violations=violations)
            save(); print('R14 async filesystem violations',len(violations),flush=True)
    finally:
        for path,original in originals.items():path.write_bytes(original)
        RESULTS['train_sources_restored']=all(path.read_bytes()==data for path,data in originals.items());save()

if args.only in ('all','regressions','r13'):

    harness=OUT/'r13-harness';(harness/'src').mkdir(parents=True)
    inputs=['src/core/business_os/policy.rs']+['src/core/store/'+name+'.rs' for name in
        ['rights','scope','manifest_rules','rights_matrix_tests','rights_tests','scope_tests']]+['src/core/rxdb/src/plugins/replication_webrtc/scope_filter.rs']
    bindings={}
    for relative in inputs:
        dest=harness/relative;dest.parent.mkdir(parents=True,exist_ok=True)
        dest.write_bytes((ROOT/relative).read_bytes() if USE_CURRENT_RIGHTS else subprocess.check_output(["git","show",RIGHTS+":"+relative]))

        bindings[relative]=digest(dest)
    coding=harness/'coding';coding.mkdir()
    for name in ['draft_turn.rs','draft_turn_tests.rs']:shutil.copyfile(ROOT/'src/core/coding_agents'/name,coding/name)
    with (coding/'draft_turn_tests.rs').open('a') as stream:
        stream.write(f'\n#[path = {json.dumps(str(HERE/"r13.rs"))}]\nmod p6_review4;\n')
    toml=f'''[package]
name="ctox-p6-review4-r13"
version="0.1.0"
edition="2021"
[workspace]
[dependencies]
ctox-store={{path="{ROOT}/src/core/store"}}
rxdb={{package="ctox-rxdb",path="{ROOT}/src/core/rxdb"}}
base64="=0.22.1"
ring="=0.17.14"
rusqlite={{version="=0.32.1",features=["bundled","hooks"]}}
serde={{version="=1.0.228",features=["derive"]}}
serde_json={{version="=1.0.151",features=["preserve_order"]}}
sha2="=0.10.9"
similar="=2.7.0"
uuid={{version="=1.23.0",features=["v4"]}}
tempfile="=3.27.0"
anyhow="=1.0.102"
zeroize="=1.9.1"
tokio={{version="1",features=["macros","rt-multi-thread","time","sync"]}}
futures-util="0.3"
async-trait="0.1"
[patch.crates-io]
rtc-ice={{path="{ROOT}/src/core/rxdb/patches/rtc-ice"}}
rtc={{path="{ROOT}/src/core/rxdb/patches/rtc"}}
webrtc={{path="{ROOT}/src/core/rxdb/patches/webrtc"}}
'''
    (harness/'Cargo.toml').write_text(toml);shutil.copyfile(ROOT/'Cargo.lock',harness/'Cargo.lock')
    (harness/'src/lib.rs').write_text('''#![allow(dead_code)]
pub mod crew {pub const PUBLIC_MEMBER_FIELDS: &[&str]=&["id","name"];}
pub mod secrets { pub fn with_current_secret_value<T>(_:&std::path::Path,_:&str,_:&str,
 _:impl FnOnce(&[u8])->anyhow::Result<T>)->anyhow::Result<T>{anyhow::bail!("compile-only provider; no credential access")} }
pub mod business_os {pub mod session {
pub struct BusinessOsSession;
pub fn session_user_id(_:&BusinessOsSession)->Option<&str>{None}
pub fn session_role(_:&BusinessOsSession)->&'static str{"user"}}
'''+f'#[path="{harness}/src/core/business_os/policy.rs"] pub mod policy;\n}}\n'+
        'pub use business_os::policy::sync_rights as rights;\n'+
        f'#[path="{coding}/draft_turn.rs"] mod draft_turn;\n')
    (OUT/'r13-source-binding.json').write_text(json.dumps(dict(policy_revision=RIGHTS,policy_files=bindings,
        draft_source_revision=TRAIN,draft_sha256=digest(coding/'draft_turn.rs'),
        limitations='session/credential/crew signatures only; real policy, writer and CodingPanel; scripted model reply, not sidecar E2E'),indent=2))
    exe=test_binary('r13-build',['cargo','test','--manifest-path',harness/'Cargo.toml','--lib','--jobs','2'])
    p=run('r13',[exe,'draft_turn::tests::p6_review4::r13_inflight_reply_cannot_patch_draft_after_committed_rights_revocation','--exact','--nocapture','--test-threads=1'],60)
    RESULTS['r13']=dict(exit=p.returncode,reproduced=p.returncode!=0 and 'R13: draft changed after' in p.stdout);save()

required=['r10','r13','r14'] if args.only in ('all','regressions') else [args.only]
if args.only=='all' and not args.expect_red: required.append('scenarios')
if args.expect_red and args.only=='scenarios': raise SystemExit('--expect-red qualifies functional regression counterexamples only')

if args.expect_red:
    qualified=all(RESULTS.get(name,{}).get('reproduced') is True for name in required if name!='scenarios')
    assert qualified, 'expected functional counterexample not reproduced; inspect individual logs'
    print('P6_REVIEW4_FUNCTIONAL_RED_TARGETS_QUALIFIED',OUT,flush=True)
    sys.exit(0)
print('P6_REVIEW4_RESULTS',OUT,flush=True)
sys.exit(1 if any(RESULTS.get(name,{}).get('exit',1)!=0 for name in required) else 0)
