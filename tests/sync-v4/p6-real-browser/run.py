#!/usr/bin/env python3
"""One admitted job: native invariants/mutations, kernel and real browser path."""
import argparse,hashlib,json,os,signal,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--deadline',type=float,required=True);a=p.parse_args()
root=Path.cwd();out=Path(os.environ['CARGO_TARGET_DIR'])/('p6-real-browser-'+time.strftime('%Y%m%dT%H%M%SZ',time.gmtime()));out.mkdir()
env=dict(os.environ,CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2',CMAKE_BUILD_PARALLEL_LEVEL='2',CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS='1')
r={'owner':'01a14204-d613','out':str(out),'head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'steps':[],'mutations':[]};active=None
rust=['src/core/store/app_grant_reader.rs','src/core/store/app_grant.rs','src/core/store/app_grant_route.rs','src/core/store/subscription.rs','src/core/service/store_browser_scope_grant.rs','src/core/service/store_browser_ingress.rs','src/core/service/store_browser_write.rs','src/core/service/store_browser_grant_stream.rs','src/core/rxdb/src/plugins/replication_webrtc/v4_session.rs','tests/sync-v4/p6-e55/contract.rs']
def save(): (out/'result.json').write_text(json.dumps(r,indent=2))
def stop(signum,frame):
 if active:
  try:os.killpg(active.pid,signal.SIGTERM)
  except ProcessLookupError:pass
 raise InterruptedError('bounded order stopped')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
def run(name,cmd,cap=7200):
 global active
 left=min(cap,a.deadline-time.time()-25)
 if left<10:raise TimeoutError('absolute deadline')
 started=time.time()
 with (out/(name+'.log')).open('w') as log:
  active=subprocess.Popen(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
  (out/(name+'-process.json')).write_text(json.dumps({'owner':r['owner'],'pid':active.pid,'pgid':active.pid,'purpose':name,'output':str(out),'stop_after_seconds':left}))
  try:code=active.wait(timeout=left)
  except subprocess.TimeoutExpired:
   os.killpg(active.pid,signal.SIGTERM)
   try:active.wait(timeout=10)
   except subprocess.TimeoutExpired:os.killpg(active.pid,signal.SIGKILL);active.wait()
   code=124
  finally:active=None
 r['steps'].append({'name':name,'exit':code,'seconds':time.time()-started});save();print('P6_REAL_STEP',name,code,flush=True);return code
originals={}
try:
 if run('format',['rustfmt','--edition','2021','--config','skip_children=true',*rust],60):raise RuntimeError('format')
 for f in rust:
  dest=out/'formatted'/f;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes((root/f).read_bytes())
 r['source_sha256']={f:hashlib.sha256((root/f).read_bytes()).hexdigest() for f in rust};save()
 contract=['cargo','test','--locked','--manifest-path','tests/sync-v4/p6-e55/Cargo.toml','--test','contract','--jobs','2','--','--test-threads=2']
 if run('native-contracts',contract,1500):raise RuntimeError('native contract baseline')
 grant=root/'src/core/store/app_grant.rs';originals[grant]=grant.read_text()
 mutations=[('shell-preferences','let include_preferences = true;','let include_preferences = false;','shell boot preferences grant missing'),('foreign-scope','g.claims.scope_hash == h','true || g.claims.scope_hash == h','foreign scope admitted'),('retained-revoke','state.issued.remove(app).map','state.issued.get(app).map','revoked subscription survived')]
 for name,before,after,reason in mutations:
  original=originals[grant];assert original.count(before)==1,(name,'anchor')
  grant.write_text(original.replace(before,after))
  try:code=run('mutation-'+name,contract,180)
  finally:grant.write_text(original)
  log=(out/('mutation-'+name+'.log')).read_text();red=code==101 and reason in log
  r['mutations'].append({'name':name,'file':str(grant.relative_to(root)),'line':next(i for i,l in enumerate(original.splitlines(),1) if before in l),'red':red,'reason':reason});save()
  if not red:raise RuntimeError('mutation not proven: '+name)
 if run('native-restored',contract,180):raise RuntimeError('restored contracts')
 if run('kernel-protocol',['node','--test','src/apps/business-os/kernel/tests/e55.test.mjs','src/apps/business-os/kernel/tests/boot.test.mjs'],120):raise RuntimeError('kernel protocol')
 # Existing runner builds real foreground and native enrollment/control fixture;
 # it supplies no fake core, data HTTP route, browser adapter or injected role.
 env['P6_BUNDLED_CHROMIUM']=subprocess.check_output(['node','-e','process.stdout.write(require(process.env.PLAYWRIGHT_MODULE_PATH).chromium.executablePath())'],env=env,text=True)
 r['chromium']=env['P6_BUNDLED_CHROMIUM'];r['npx_available']=subprocess.call(['bash','-c','command -v npx >/dev/null'],env=env)==0;save()
 r['browser_exit']=run('real-browser',['python3','tests/sync-v4/k13/rerun.py','--revision',r['head'],'--deadline',str(a.deadline-30)],4200)
 r['qualified']=r['browser_exit']==0
except Exception as e:r['error']=str(e)
finally:
 for f,original in originals.items():f.write_text(original)
 r['source_restored']=all(f.read_text()==original for f,original in originals.items());r['terminal']=True;r['ended']=time.time();save()
 print('P6_REAL_RESULT',out/'result.json',flush=True)
raise SystemExit(0 if r.get('qualified') else 1)
