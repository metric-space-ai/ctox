#!/usr/bin/env python3
"""Reassess retained native samples without rerunning or relabeling their runtime."""
import datetime, hashlib, importlib.util, json, pathlib, shutil, subprocess, sys, tempfile
import evaluate
HERE=pathlib.Path(__file__).resolve().parent
def qualify(out):
    out=pathlib.Path(out).resolve()
    profiles={r:json.loads((out/f'profile-{r}.json').read_text()) for r in (20,150,600) if (out/f'profile-{r}.json').exists()}
    provenance=json.loads((out/'provenance.json').read_text())
    raw_hashes={str(r):hashlib.sha256((out/f'profile-{r}.json').read_bytes()).hexdigest() for r in profiles}
    result=evaluate.evaluate(profiles)
    result.update(goal='M8',revisions=provenance,hosts=[provenance['host']],
        steps=['real native SCTP component profile','assessment of retained immutable native samples'],
        criterion='all native profile thresholds plus CUTOVER-3 productive bindings',artifacts=[str(out)],
        assessment={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'evaluate_sha256':hashlib.sha256((HERE/'evaluate.py').read_bytes()).hexdigest(),
            'qualify_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
            'raw_profile_sha256':raw_hashes})
    # The runtime's revision stays intact even when a newer assessor is used.
    lock=out/'source/tests/sync_v4_m8/Cargo.lock'
    if lock.exists():result['assessment']['resolved_lock_sha256']=hashlib.sha256(lock.read_bytes()).hexdigest()
    locations={
        'command_p99_ms':('command_limit=rtt+100 if command_override is None else command_override','command_limit=0.0'),
        'terminal_echo_p95_ms':('terminal_limit=100','terminal_limit=0.0'),
        'live_push_p95_ms':('live_p95_limit=150','live_p95_limit=0.0'),
        'live_push_p99_ms':('live_p99_limit=500','live_p99_limit=0.0')}
    candidates=[c for c in result['criteria'] if c['criterion'] in locations and c['pass']]
    chosen=candidates[0]['criterion'] if candidates else 'command_p99_ms'
    needle,replacement=locations[chosen]
    path=HERE/'evaluate.py';original=path.read_text()
    assert original.count(needle)==1
    proof_root=pathlib.Path(tempfile.mkdtemp(prefix='threshold-proof-',dir=out))
    mutant=proof_root/'evaluate-mutant.py'
    mutant.write_text(original.replace(needle,replacement+' # mutation: lower one real latency threshold'))
    spec=importlib.util.spec_from_file_location('m8_mutant',mutant)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    mutated=module.evaluate(profiles)
    changed=[c for c in result['criteria'] if c['criterion']==chosen and c['pass']]
    red=all(not c['pass'] for c in mutated['criteria'] if c['criterion']==chosen)
    exits=[]
    for criterion in changed:
        rtt=criterion['profile_rtt_ms']
        for label,script in [('baseline',path),('mutant',mutant)]:
            check=proof_root/f'{label}-{rtt}';check.mkdir()
            for r in profiles:shutil.copy2(out/f'profile-{r}.json',check/f'profile-{r}.json')
            with (check/'evaluation.log').open('w') as log:
                code=subprocess.run([sys.executable,str(script),str(check),'--assert-criterion',chosen,'--rtt',str(rtt)],stdout=log,stderr=subprocess.STDOUT,timeout=30).returncode
            exits.append(dict(variant=label,rtt_ms=rtt,exit=code,log=str(check/'evaluation.log')))
    proof={'criterion':chosen,'file':'scripts/sync-v4/m8/evaluate.py',
        'line':next(i for i,line in enumerate(original.splitlines(),1) if needle in line),
        'mutation':needle+' -> '+replacement,'source_sha256':hashlib.sha256(original.encode()).hexdigest(),
        'raw_profile_sha256':raw_hashes,'baseline_green_profiles':[c['profile_rtt_ms'] for c in changed],
        'mutant_criteria_red':red,'process_exits':exits,'baseline_retained':True,
        'pass':bool(exits) and red and all(e['exit']==(0 if e['variant']=='baseline' else 1) for e in exits)}
    (out/'threshold-mutation.json').write_text(json.dumps(proof,indent=2)+'\n')
    result['threshold_mutation']=proof
    result['artifacts'].append(str(proof_root))
    evaluate.write(out,result)
    print('M8_COMPONENT_RESULT '+str(out/'m8.json'),flush=True)
    print((out/'m8.md').read_text(),flush=True)
    return 0 if result['component_profile_pass'] and proof['pass'] else 1
if __name__=='__main__':
    if len(sys.argv)!=2:raise SystemExit('usage: qualify.py retained-native-output')
    raise SystemExit(qualify(sys.argv[1]))
