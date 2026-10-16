#!/usr/bin/env python3
"""One bounded lane job: exact production source, writer-backed contracts, mutations."""
import argparse, hashlib, json, os, pathlib, re, subprocess, time
p=argparse.ArgumentParser();p.add_argument('--deadline',type=int,required=True);a=p.parse_args()
root=pathlib.Path.cwd(); out=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/('p6-e55-'+time.strftime('%Y%m%dT%H%M%SZ',time.gmtime()));out.mkdir(parents=True)
source=root/'src/core/store/app_grant.rs'
record={'source_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'out':str(out),'started':time.time(),'runs':[]}
def save(): (out/'result.json').write_text(json.dumps(record,indent=2))
def run(name,cmd):
    remaining=a.deadline-int(time.time())
    if remaining<20: raise RuntimeError('P6 absolute deadline')
    with (out/(name+'.log')).open('w') as log:
        r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,timeout=remaining,env={**os.environ,'CARGO_BUILD_JOBS':'2','RUST_TEST_THREADS':'2','CMAKE_BUILD_PARALLEL_LEVEL':'2'})
    record['runs'].append({'name':name,'exit':r.returncode,'time':time.time()});save()
    print('P6_E55',name,'exit',r.returncode,flush=True)
    return r.returncode
# Format only the files owned by this order. The formatted inputs are retained
# for exact synchronization back to the source checkout, not silently discarded.
owned=['src/core/store/app_grant.rs','tests/sync-v4/p6-e55/lib.rs','tests/sync-v4/p6-e55/contract.rs']
if run('format',['rustfmt','--edition','2021','--config','skip_children=true',*owned])!=0: raise SystemExit(1)
for f in owned:
    dest=out/'formatted'/f;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes((root/f).read_bytes())
original=source.read_bytes(); record['production_sha256']=hashlib.sha256(original).hexdigest(); record['input_sha256']={f:hashlib.sha256((root/f).read_bytes()).hexdigest() for f in owned+['src/core/store/rights.rs','src/core/store/scope.rs','src/core/store/manifest_rules.rs','tests/sync-v4/p6-e55/run.py','tests/sync-v4/p6-e55/Cargo.toml']}; save()
cmd=['cargo','test','--manifest-path','tests/sync-v4/p6-e55/Cargo.toml','--test','contract','--jobs','2','--','--test-threads=2']
try:
    if run('baseline',cmd)!=0: raise RuntimeError('baseline failed; no mutation evidence')
    mutants=[
      ('foreign-app',r'rights\s*\.resolve_app_collection\(\s*app,\s*&state.role,\s*&state.user,\s*&rule.collection,?\s*\)','rights.resolve_collection(&state.role, &state.user, &rule.collection)','foreign app collection entered grant'),
      ('expired',r'\|\| now >= claims.exp','|| false /* MUTATION: expiry omitted */','expired grant remained readable'),
      ('revoked',r'state\s*\.issued\s*\.remove\(app\)\s*\.map\(\|_\|\s*revoke_frame\(app\)\)','state.issued.get(app).map(|_| revoke_frame(app))','revoked app remained readable'),
    ]
    record['mutations']=[]
    for name,before,after,reason in mutants:
        text=original.decode()
        mutated, changed = re.subn(before, lambda _: after, text)
        if changed!=1: raise RuntimeError('mutation anchor missing: '+name)
        source.write_text(mutated)
        code=run('mutation-'+name,cmd)
        log=(out/('mutation-'+name+'.log')).read_text()
        proven=code==101 and reason in log and 'test result: FAILED' in log
        record['mutations'].append({'name':name,'red':proven,'expected':reason});save()
        source.write_bytes(original)
        if not proven: raise RuntimeError('mutation not proven: '+name)
    record['restored_baseline']=run('restored',cmd)==0
    record['qualified']=record['restored_baseline'] and all(x['red'] for x in record['mutations'])
except Exception as e:
    record['error']=str(e)
finally:
    source.write_bytes(original)
    record['source_restored']=source.read_bytes()==original
    record['terminal']=True;record['ended']=time.time();save()
print('P6_E55_RESULT',str(out/'result.json'),flush=True)
raise SystemExit(0 if record.get('qualified') and record.get('source_restored') else 1)
