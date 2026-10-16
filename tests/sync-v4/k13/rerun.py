#!/usr/bin/env python3
"""Lane-only K rerun; builds the composed source and preserves measured/not-run criteria."""
import argparse,hashlib,json,os,re,signal,subprocess,time,zipfile
from pathlib import Path
P=argparse.ArgumentParser();P.add_argument('--revision',required=True);P.add_argument('--deadline',type=float,required=True);P.add_argument('--prepare-only',action='store_true');a=P.parse_args()
ROOT=Path(__file__).resolve().parents[3];HERE=Path(__file__).resolve().parent
assert os.environ.get('BUILD_LANE_BIN') and os.environ.get('CARGO_TARGET_DIR'),'canonical lane required'
OUT=Path(os.environ['CARGO_TARGET_DIR'])/('p6-k-rerun-'+time.strftime('%Y%m%dT%H%M%SZ',time.gmtime()));OUT.mkdir()
ENV=dict(os.environ,CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2',TOKIO_WORKER_THREADS='2',CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS='1',CMAKE_BUILD_PARALLEL_LEVEL='2')
ACTIVE=None;steps=[];results={'expected_revision':a.revision,'steps':steps};artifacts=[]
def hash_file(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  for data in iter(lambda:f.read(1024*1024),b''):h.update(data)
 return h.hexdigest()
def save(): (OUT/'results.json').write_text(json.dumps(results,indent=2)+'\n')
def stop(signum,frame):
 if ACTIVE is not None:
  try:os.killpg(ACTIVE.pid,signal.SIGTERM)
  except ProcessLookupError:pass
 raise InterruptedError('order interrupted')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
def run(name,args,cap):
 global ACTIVE
 remaining=min(cap,a.deadline-time.time()-20)
 if remaining<=0:raise TimeoutError('order deadline')
 start=time.monotonic()
 with (OUT/(name+'.log')).open('w') as f:
  ACTIVE=subprocess.Popen(list(map(str,args)),cwd=ROOT,env=ENV,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
  (OUT/(name+'-process.json')).write_text(json.dumps({'owner':'01a14204-d613','pid':ACTIVE.pid,'pgid':ACTIVE.pid,'purpose':name,'output':str(OUT),'stop':remaining}))
  try:code=ACTIVE.wait(timeout=remaining)
  except subprocess.TimeoutExpired:
   os.killpg(ACTIVE.pid,signal.SIGTERM)
   try:ACTIVE.wait(timeout=10)
   except subprocess.TimeoutExpired:os.killpg(ACTIVE.pid,signal.SIGKILL);ACTIVE.wait()
   code=124
  finally:ACTIVE=None
 steps.append({'name':name,'exit':code,'seconds':round(time.monotonic()-start,3)});save();print('P6_K_STEP',name,code,flush=True)
 return code,(OUT/(name+'.log')).read_text()
def summary(text):
 m=re.search(r'^K13 \| .* \| (.+)/K13/result\.json$',text,re.M)
 if not m:raise RuntimeError('scenario summary path absent')
 path=Path(m[1]);artifacts.append(path);return path,json.loads((path/'p6-result.json').read_text())
def private(s):return len(s.get('scenarios',[]))==3 and all(r.get('namespaceVerified') and r.get('cleanup') for r in s['scenarios'])
try:
 paths=['src/core/service/store_startup.rs','src/core/service/store_native.rs','src/core/business_os/policy.rs','src/core/business_os/app_platform/runtime_host.rs']
 paths += ['src/apps/business-os/kernel/boot.mjs','src/apps/business-os/kernel/loader.mjs','src/apps/business-os/kernel/transport-webrtc.mjs','src/core/service/store_http.rs','src/core/business_os/store.rs',str(HERE.relative_to(ROOT)/'enrolled-fixture.rs')]
 paths += [str(p.relative_to(ROOT)) for d in ['k13','k21','k22'] for p in (ROOT/'tests/sync-v4'/d).glob('*.mjs')]
 (OUT/'source-binding.json').write_text(json.dumps({'expected_revision':a.revision,'remote_git_metadata':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'hashes':{p:hash_file(ROOT/p) for p in paths}},indent=2)+'\n')
 for file in HERE.glob('*.mjs'):
  if run('syntax-'+file.stem,['node','--check',file],30)[0]:raise RuntimeError('runner syntax failed')
 if run('node-prepare',['bash','src/core/app_runtime/verify.sh','prepare'],150)[0]:raise RuntimeError('pinned Node bundle unavailable')
 ENV['CTOX_APP_RUNTIME_NODE_BUNDLE']=str(Path(os.environ['CARGO_TARGET_DIR'])/'app-runtime-test-node/bin/node')
 if run('daemon-build',['cargo','build','--locked','--jobs','2','--bin','ctox','--features','app-runtime-node'],1800)[0]:raise RuntimeError('build unverified')
 fixture_source=ROOT/'src/core/business_os/app_platform/runtime_host/native_proof.rs'
 fixture_original=fixture_source.read_bytes()
 try:
  fixture_source.write_bytes(fixture_original+('\n#[path = '+json.dumps(str(HERE/'enrolled-fixture.rs'))+']\nmod p6_k_fixture;\n').encode())
  fcode,flog=run('fixture-build',['cargo','test','--locked','--jobs','2','--bin','ctox','--features','app-runtime-node','--no-run','--message-format=json'],1200)
  if fcode:raise RuntimeError('native fixture build unverified')
  bins=[j['executable'] for line in flog.splitlines() if line.startswith('{') for j in [json.loads(line)] if j.get('reason')=='compiler-artifact' and j.get('target',{}).get('name')=='ctox' and j.get('profile',{}).get('test') and j.get('executable')]
  if len(bins)!=1:raise RuntimeError('native fixture executable ambiguous')
  fixture_binary=Path(bins[0])
 finally:fixture_source.write_bytes(fixture_original)
 binary=Path(os.environ['CARGO_TARGET_DIR'])/'debug/ctox'
 receipt=OUT/'build.json';receipt.write_text(json.dumps({'pass':True,'terminal':True,'revision':a.revision,'source':str(ROOT),'binary':str(binary),'binarySha256':hash_file(binary),'fixtureBinary':str(fixture_binary),'fixtureBinarySha256':hash_file(fixture_binary)},indent=2))
 args=['node',HERE/'run.mjs','--build',receipt,'--expected-revision',a.revision]
 if a.prepare_only:args+=['--prepare-only','true']
 code,text=run('scenarios',args,2400)
 path,s=summary(text);results['scenarios']=s['scenarios'];results['artifact_directory']=str(path)
 results['namespace_contract']=private(s);results['acceptance']=s['pass'];save()
 for r in s['scenarios']:print('P6_K_RESULT',r['id'],r['status'],r.get('reason'),flush=True)
 mutant=HERE/'.p6-namespace-mutant.mjs';original=(HERE/'run.mjs').read_text();old="'--map-root-user','--net','--mount'";assert original.count(old)==1
 try:
  mutant.write_text(original.replace(old,"'--map-root-user','--mount'"))
  mcode,mtext=run('namespace-mutation',['node',mutant,'--build',receipt,'--expected-revision',a.revision],90)
  mp,ms=summary(mtext)
  rejected=mcode!=0 and not private(ms) and all(not r['daemonExecuted'] and 'Private mapped user/net/mount/PID namespaces required' in (mp/(r['id']+'.log')).read_text() for r in ms['scenarios'])
  results['namespace_mutation']={'source':'tests/sync-v4/k13/run.mjs','line':next(i for i,l in enumerate(original.splitlines(),1) if old in l),'mutation':'remove --net','positive':private(s),'mutant_rejected_before_daemon':rejected}
 finally:mutant.unlink(missing_ok=True)
except Exception as e:
 results['runner_error']=str(e);print('P6_K_ERROR',str(e),flush=True)
finally:
 save()
 # Keep only evidence, never tenant databases, credentials, private configs or executables.
 with zipfile.ZipFile(OUT/'runtime-evidence.zip','w',zipfile.ZIP_DEFLATED) as z:
  for i,d in enumerate(artifacts):
   for p in d.rglob('*'):
    rel=p.relative_to(d)
    if not p.is_file() or p.is_symlink() or any(part.startswith('tenant-') for part in rel.parts) or p.name.endswith('.private.json'):continue
    if p.suffix not in ['.json','.log','.txt']:continue
    z.write(p,Path('positive' if i==0 else 'namespace-mutant')/rel)
 print('P6_K_EVIDENCE',OUT,flush=True)
raise SystemExit(0 if results.get('acceptance') else 1)
