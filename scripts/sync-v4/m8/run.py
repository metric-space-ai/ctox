#!/usr/bin/env python3
"""Bounded, isolated native profile runner. Run through the canonical Linux lane."""
import argparse, datetime, hashlib, json, os, pathlib, shutil, subprocess, sys, tarfile, time
HERE=pathlib.Path(__file__).resolve()
REPO=HERE.parents[3]
def execute(args,log,timeout=1200):
    with log.open('w') as output:
        p=subprocess.Popen(args,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
        try:p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            import signal
            os.killpg(p.pid,signal.SIGTERM)
            try:p.wait(timeout=10)
            except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
            return 124
        return p.returncode
def main():
    a=argparse.ArgumentParser();a.add_argument('--out',type=pathlib.Path,required=True)
    a.add_argument('--component-ref',default='HEAD',help='P9 source revision; HEAD after integration contains P9')
    a.add_argument('--profile-seconds',type=int,default=360);a.add_argument('--check-only',action='store_true')
    a.add_argument('--deadline-utc',type=datetime.datetime.fromisoformat,help='optional total measurement deadline, timezone required')
    args=a.parse_args();out=args.out.resolve()
    if args.deadline_utc is not None and args.deadline_utc.tzinfo is None: a.error('--deadline-utc requires timezone')
    def remaining():
        return (args.deadline_utc-datetime.datetime.now(datetime.timezone.utc)).total_seconds() if args.deadline_utc else 1000000
    out.mkdir(parents=True,exist_ok=False)
    source=REPO;component=args.component_ref
    if component=='HEAD': component=subprocess.check_output(['git','-C',str(REPO),'rev-parse','HEAD'],text=True).strip()
    if component!='HEAD':
        # Build input only, not a runtime GitHub dependency. Never modify the caller clone.
        cache=out/'component.git'
        subprocess.run(['git','init','--bare',str(cache)],check=True,stdout=subprocess.DEVNULL)
        existing=subprocess.run(['git','-C',str(REPO),'cat-file','-e',component+'^{commit}'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0
        if existing:
            fetch=['git','--git-dir',str(cache),'fetch','--depth=1',str(REPO),component]
        else:
            subprocess.run(['git','--git-dir',str(cache),'remote','add','origin','https://github.com/metric-space-ai/ctox.git'],check=True)
            subprocess.run(['git','--git-dir',str(cache),'config','remote.origin.promisor','true'],check=True)
            subprocess.run(['git','--git-dir',str(cache),'config','remote.origin.partialclonefilter','blob:none'],check=True)
            fetch=['git','--git-dir',str(cache),'fetch','--depth=1','--filter=blob:none','origin',component]
        subprocess.run(fetch,check=True,timeout=180,stdout=subprocess.DEVNULL)
        component=subprocess.check_output(['git','--git-dir',str(cache),'rev-parse','FETCH_HEAD'],text=True).strip()
        source=out/'source';source.mkdir()
        archive=out/'component-source.tar'
        with archive.open('wb') as f:
            subprocess.run(['git','--git-dir',str(cache),'archive',component,'src/core/store','src/core/rxdb','tests/sync-v4-m2'],stdout=f,check=True,timeout=180)
        with tarfile.open(archive) as t:t.extractall(source,filter='data')
        shutil.copytree(REPO/'tests/sync_v4_m8',source/'tests/sync_v4_m8')
    else:
        component=subprocess.check_output(['git','-C',str(REPO),'rev-parse','HEAD'],text=True).strip()
    # Test-only access seam, exactly as the in-module lane4 test has. Preserve
    # pristine source and hash the sole appended declaration for auditability.
    scheduler=source/'src/core/rxdb/src/plugins/replication_webrtc/scheduler.rs'
    pristine=scheduler.read_bytes()
    bridge=source/'tests/sync_v4_m8/src/scheduler_bridge.rs'
    declaration='\n#[path = '+json.dumps(str(bridge))+']\npub mod m8_bridge;\n'
    assert source!=REPO, 'never attach test seams to the caller clone'
    scheduler.write_bytes(pristine+declaration.encode())
    composition={'pristine_scheduler_sha256':hashlib.sha256(pristine).hexdigest(),'appended_declaration':declaration,'compiled_scheduler_sha256':hashlib.sha256(scheduler.read_bytes()).hexdigest(),'bridge_sha256':hashlib.sha256(bridge.read_bytes()).hexdigest()}
    manifest=source/'tests/sync_v4_m8/Cargo.toml'
    harness_paths=[REPO/'scripts/sync-v4/m8-native.sh']
    harness_paths += [p for directory in (REPO/'tests/sync_v4_m8',HERE.parent) for p in directory.rglob('*') if p.is_file() and p.suffix in ('.rs','.toml','.py','.md','.sh','.lock')]
    inputs={str(p.relative_to(REPO)):hashlib.sha256(p.read_bytes()).hexdigest() for p in harness_paths}
    provenance={'profile':'component profile, pre-cutover','component_source':component,'harness_source':subprocess.check_output(['git','-C',str(REPO),'rev-parse','HEAD'],text=True).strip(),
        'harness_files':inputs,'test_composition':composition,'host':os.uname().nodename,'started':datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'command_fixture':'OrderedWriter, store fence, projections, commit-driven signed Subscription; fixture authority/executor, no AppRuntimeIngress',
        'clock':'one native process monotonic Instant; no cross-host synchronization claim',
        'requested_profile_seconds':args.profile_seconds,'deadline_utc':args.deadline_utc.isoformat() if args.deadline_utc else None,'effective_profile_seconds':{}}
    (out/'provenance.json').write_text(json.dumps(provenance,indent=2))
    # Capability probe is confined to a new unprivileged user/network namespace.
    os.environ['M8_PARENT_NETNS']=os.readlink('/proc/self/ns/net')
    netns=execute(['unshare','--user','--map-root-user','--net','bash',str(HERE.parent/'netem.sh'),'--probe'],out/'netns.log',10) if all(shutil.which(c) for c in ('unshare','ip','tc')) else 127
    provenance['netns_probe_exit']=netns
    provenance['network_impairment']='unprivileged-netns-tc' if netns==0 else 'userspace UDP shim (no shared interface changes); seeded 1% packet loss below SCTP'
    turn=shutil.which('turnserver')
    provenance['turn']={'status':'TURN not measured','binary_present':bool(turn),'reason':'no local coturn executable' if not turn else 'current component source lacks verified native relay selection; no direct path relabeled TURN'}
    (out/'provenance.json').write_text(json.dumps(provenance,indent=2))
    # The canonical mirror can preserve old mtimes: force the actual compiled copy.
    rust_sources=[manifest.parent/'src'/name for name in ('main.rs','commands.rs','network.rs','scheduler_bridge.rs')]
    for path in rust_sources: os.utime(path,None)
    expected_marker='M8_NATIVE_SOURCE_SHA256 '+hashlib.sha256(b''.join(p.read_bytes() for p in rust_sources)).hexdigest()
    provenance['expected_source_marker']=expected_marker
    provenance['forced_rebuild_sources']=[str(p) for p in rust_sources]
    build=execute(['cargo','build','--locked','--manifest-path',str(manifest),'--jobs','2'],out/'build.log',max(1,min(1800,int(remaining()-180))))
    if build:
        print((out/'build.log').read_text()[-18000:])
        sys.path.insert(0,str(HERE.parent));import evaluate
        failure=evaluate.evaluate({});failure.update(goal='M8',revisions=provenance,hosts=[os.uname().nodename],steps=[{'step':'native harness build','exit':build}],criterion='native profile could not start',artifacts=[str(out/'build.log')])
        evaluate.write(out,failure)
        return build
    print('M8_NATIVE_BUILD_GREEN',flush=True)
    binary=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'debug/ctox-sync-v4-m8'
    actual_marker=subprocess.check_output([str(binary),'--source-marker'],text=True,timeout=10).strip()
    (out/'source-marker.txt').write_text(actual_marker+'\n')
    print(actual_marker,flush=True)
    if actual_marker!=expected_marker: raise RuntimeError('stale native binary: source marker mismatch')
    provenance['verified_source_marker']=actual_marker
    provenance['binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
    (out/'provenance.json').write_text(json.dumps(provenance,indent=2))
    # Evaluator contract: absent samples fail; mutation changes real evaluation threshold.
    sys.path.insert(0,str(HERE.parent))
    import evaluate
    assert evaluate.evaluate({})['component_profile_pass'] is False
    if args.check_only:
        print('M8_BUILD_AND_FAIL_CLOSED_EVALUATOR_GREEN');return 0
    for index,rtt in enumerate((20,150,600)):
        if remaining()<=60:
            provenance["effective_profile_seconds"][str(rtt)]=0
            (out/f"profile-{rtt}.json").write_text(json.dumps({"rtt_ms":rtt,"completed":False,"errors":["measurement deadline exhausted before native start"]}))
            continue
        seconds=max(1,min(args.profile_seconds,int((remaining()-60)/(3-index))-60))
        provenance['effective_profile_seconds'][str(rtt)]=seconds
        (out/'provenance.json').write_text(json.dumps(provenance,indent=2))
        command=[str(binary),str(out),str(seconds),str(rtt)]
        if netns==0:command=['unshare','--user','--map-root-user','--net','bash',str(HERE.parent/'netem.sh'),*command]
        code=execute(command,out/f'native-{rtt}.log',seconds+60)
        p=out/f'profile-{rtt}.json'
        if not p.exists():p.write_text(json.dumps({'rtt_ms':rtt,'completed':False,'errors':[f'native process exit {code}; see native-{rtt}.log']}))
        if netns==0 and (out/f'netem-{rtt}.json').exists() and code==0:
            native=json.loads(p.read_text());qdiscs=json.loads((out/f'netem-{rtt}.json').read_text())
            q=next(q for q in qdiscs if q['kind']=='netem')
            native['impairment'].update(mode='unprivileged-netns-tc',udp_offered=q['packets']+q['drops'],udp_dropped=q['drops'],udp_sent=q['packets'],udp_send_errors=0,active_after_handshake=False,rate_bytes_per_s_per_direction=None,aggregate_rate_bytes_per_s=8000000,rate_scope="namespace lo shared by all directions",seed=None,tc_statistics=q)
            p.write_text(json.dumps(native,indent=2))
        print(f'M8_MEASUREMENT rtt={rtt} exit={code}',flush=True)
        profiles={r:json.loads((out/f'profile-{r}.json').read_text()) for r in (20,150,600) if (out/f'profile-{r}.json').exists()}
        result=evaluate.evaluate(profiles);result.update(goal='M8',revisions=provenance,hosts=[os.uname().nodename],steps=['real native SCTP component profile'],artifacts=[str(out)],criterion='all native profile thresholds plus CUTOVER-3 product binding')
        evaluate.write(out,result)
    provenance['finished']=datetime.datetime.now(datetime.timezone.utc).isoformat()
    (out/'provenance.json').write_text(json.dumps(provenance,indent=2))
    from qualify import qualify
    return qualify(out)
if __name__=='__main__':sys.exit(main())
