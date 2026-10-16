#!/usr/bin/env python3
"""Bounded route qualification: real native source, kernel verifier, source mutations."""
import argparse, hashlib, json, os, pathlib, re, subprocess, time
p=argparse.ArgumentParser();p.add_argument('--deadline',type=int,required=True);a=p.parse_args()
root=pathlib.Path.cwd();out=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/('p6-e55-route-'+time.strftime('%Y%m%dT%H%M%SZ',time.gmtime()));out.mkdir(parents=True)
r={'out':str(out),'source_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'runs':[],'mutations':[]}
def save(): (out/'result.json').write_text(json.dumps(r,indent=2))
def run(name,cmd):
    left=a.deadline-int(time.time())
    if left<15: raise RuntimeError('absolute deadline')
    with (out/(name+'.log')).open('w') as log:
        code=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,timeout=left,env={**os.environ,'CARGO_BUILD_JOBS':'2','RUST_TEST_THREADS':'2','CMAKE_BUILD_PARALLEL_LEVEL':'2','P6_E55_VECTOR':str(out/'native-vector.json')}).returncode
    r['runs'].append({'name':name,'exit':code,'time':time.time()});save();print('P6_E55_ROUTE',name,'exit',code,flush=True);return code
owned=['src/core/store/app_grant_route.rs','src/core/store/app_grant.rs','src/core/store/rights.rs','src/core/sync/src/authority/auth.rs','src/core/sync/src/authority/auth/app_grant_packet.rs','src/core/service/store_browser_scope_grant.rs','src/core/service/store_browser.rs','src/core/service/store_browser_ingress.rs','src/core/rxdb/src/plugins/replication_webrtc/v4_session.rs','src/core/rxdb/src/plugins/replication_webrtc/v4_scope_grant_control.rs','tests/sync-v4/p6-e55/lib.rs','tests/sync-v4/p6-e55/contract.rs']
originals={}
cmd=['cargo','test','--manifest-path','tests/sync-v4/p6-e55/Cargo.toml','--test','contract','--jobs','2','--','--test-threads=2']
try:
    if run('format',['rustfmt','--edition','2021','--config','skip_children=true',*owned]):raise RuntimeError('format')
    for f in owned:
        d=out/'formatted'/f;d.parent.mkdir(parents=True,exist_ok=True);d.write_bytes((root/f).read_bytes())
    r['input_sha256']={f:hashlib.sha256((root/f).read_bytes()).hexdigest() for f in owned+['tests/sync-v4/p6-e55/route_run.py','tests/sync-v4/p6-e55/kernel-route.mjs','tests/sync-v4/p6-e55/Cargo.toml']};save()
    if run('baseline',cmd):raise RuntimeError('baseline')
    cmd.insert(2,'--locked')
    route=root/'src/core/store/app_grant_route.rs';originals[route]=route.read_text()
    mutations=[('denied-app','let response = self.session.handle_request(request, now).await;','let response = self.session.handle_request(match request { GrantRequest::Request { app_id } if app_id == "b" => GrantRequest::Request { app_id: "a".into() }, other => other }, now).await;','unauthorized app received grant'),('duplicate','if valid.is_ok() {','if false && valid.is_ok() {','duplicate app request answered twice')]
    # rustfmt may expand the cached-response expression; duplicate mutation returns
    # a forbidden second frame, which must also be detected as a duplicate reply.
    for name,before,after,reason in mutations:
        s=originals[route]
        if s.count(before)!=1:raise RuntimeError('mutation anchor '+name)
        route.write_text(s.replace(before,after));code=run('mutation-'+name,cmd);log=(out/('mutation-'+name+'.log')).read_text();route.write_text(s)
        red=code==101 and reason in log;r['mutations'].append({'name':name,'red':red,'expected':reason});save()
        if not red:raise RuntimeError('mutation '+name)
    grant=root/'src/core/store/app_grant.rs';originals[grant]=grant.read_text()
    pattern=r'self\s*\.peer_authority\s*\.validate\(tx, &state.user, &state.role, &state.device, app\)\?;'
    changed,n=re.subn(pattern,'let _ = &self.peer_authority;',originals[grant])
    if n!=1:raise RuntimeError('app authority mutation anchor')
    grant.write_text(changed);code=run('mutation-app-authority',cmd);log=(out/'mutation-app-authority.log').read_text();grant.write_text(originals[grant])
    red=code==101 and 'unauthorized app received grant' in log;r['mutations'].append({'name':'app-authority','red':red,'expected':'unauthorized app received grant'});save()
    if not red:raise RuntimeError('app authority mutation')
    if run('restored',cmd):raise RuntimeError('restored')
    # Fetch a pinned public source snapshot only during validation. No runtime dependency.
    kernel_head='3e4c183e0f9949a33e8c9d3e3a3a54068ae1ff75'
    if run('kernel-fetch',['git','fetch','origin',kernel_head]):
        if run('kernel-fetch-branch',['git','fetch','origin','v4/kernel-e47']):raise RuntimeError('kernel fetch')
    kernel_sha=subprocess.check_output(['git','rev-parse','FETCH_HEAD'],text=True).strip();r['kernel_head']=kernel_sha;save()
    archive=out/'kernel.tar'
    with archive.open('wb') as dest:subprocess.run(['git','archive',kernel_sha,'src/apps/business-os/kernel'],stdout=dest,check=True)
    if run('kernel-extract',['tar','-xf',str(archive),'-C',str(out)]):raise RuntimeError('kernel extract')
    kernel=str(out/'src/apps/business-os/kernel');node=['node','tests/sync-v4/p6-e55/kernel-route.mjs',str(out/'native-vector.json'),kernel]
    if run('kernel-native-route',node):raise RuntimeError('kernel native route')
    code=run('mutation-kernel-key',node+['wrong-key']);log=(out/'mutation-kernel-key.log').read_text();red=code!=0 and 'foreign key accepted by kernel' in log;r['mutations'].append({'name':'kernel-key','red':red,'expected':'foreign key accepted by kernel'});save()
    if not red:raise RuntimeError('kernel mutation')
    if run('kernel-restored',node):raise RuntimeError('kernel restored')
    if run('kernel-protocol',['node','--test',kernel+'/tests/e55.test.mjs']):raise RuntimeError('kernel protocol')
    # Compile the actual foreground adapter, including the single responder hook.
    r['daemon_check']=run('daemon-check',['cargo','check','--locked','--bin','ctox','--jobs','2'])==0;save()
    r['qualified']=r['daemon_check'] and all(m['red'] for m in r['mutations'])
except Exception as e:r['error']=str(e)
finally:
    for f,s in originals.items():f.write_text(s)
    lock=root/'tests/sync-v4/p6-e55/Cargo.lock'
    if lock.exists():(out/'Cargo.lock').write_bytes(lock.read_bytes())
    r['source_restored']=all(f.read_text()==s for f,s in originals.items());r['terminal']=True;r['ended']=time.time();save()
print('P6_E55_ROUTE_RESULT',out/'result.json',flush=True)
raise SystemExit(0 if r.get('qualified') else 1)
