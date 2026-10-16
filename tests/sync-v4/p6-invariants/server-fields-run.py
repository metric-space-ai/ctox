#!/usr/bin/env python3
"""Detached lane job: real kernel outbox -> native P8 + committed policy -> kernel journal."""
import argparse,hashlib,json,os,shutil,signal,subprocess,time
from pathlib import Path
P=argparse.ArgumentParser();P.add_argument('--deadline',type=float,required=True);P.add_argument('--reuse-native');P.add_argument('--outbound-only',action='store_true');a=P.parse_args()
ROOT=Path(__file__).resolve().parents[3];HERE=Path(__file__).resolve().parent
assert os.environ.get('BUILD_LANE_BIN') and os.environ.get('CARGO_TARGET_DIR'),'canonical lane required'
OUT=Path(os.environ['CARGO_TARGET_DIR'])/('p6-fields-'+time.strftime('%Y%m%dT%H%M%SZ',time.gmtime()));OUT.mkdir()
ENV=dict(os.environ,CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2',TOKIO_WORKER_THREADS='2',CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS='1',CMAKE_BUILD_PARALLEL_LEVEL='2')
originals={};record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'steps':[],'mutations':[],'terminal':False};active=None
paths=['src/core/store/rights.rs','src/core/store/manifest_rules.rs','src/core/store/write_contract.rs','src/core/store/subscription.rs','src/apps/business-os/kernel/kernel.mjs','src/apps/business-os/kernel/values.mjs']+[str(p.relative_to(ROOT)) for p in HERE.glob('server-fields*')]
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
record['sources']={p:sha(ROOT/p) for p in paths}
def save():(OUT/'results.json').write_text(json.dumps(record,indent=2)+'\n')
def stop(sig,frame):raise InterruptedError('order stopped')
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
def run(label,args,cap=1200,env=None):
 global active
 remaining=min(cap,a.deadline-time.time()-15)
 if remaining<=0:raise TimeoutError('absolute order deadline')
 with (OUT/(label+'.log')).open('w') as log:
  start=time.monotonic();active=subprocess.Popen(list(map(str,args)),cwd=ROOT,env=env or ENV,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
  (OUT/'process.json').write_text(json.dumps({'owner':'01a14204-d613','pid':active.pid,'pgid':active.pid,'purpose':label,'output':str(OUT),'stopAt':a.deadline}))
  try:code=active.wait(timeout=remaining)
  except subprocess.TimeoutExpired:code=124
  finally:
   if active.poll() is None:
    os.killpg(active.pid,signal.SIGTERM)
    try:active.wait(timeout=5)
    except subprocess.TimeoutExpired:os.killpg(active.pid,signal.SIGKILL);active.wait()
   active=None
 record['steps'].append({'name':label,'exit':code,'seconds':round(time.monotonic()-start,3)});save();print('P6_FIELDS_STEP',label,code,flush=True)
 return code,(OUT/(label+'.log')).read_text()
def build(label):
 code,log=run(label,['cargo','test','--locked','--jobs','2','--bin','ctox','--features','app-runtime-node','--no-run','--message-format=json'])
 if code:raise RuntimeError('build unverified: '+label)
 bins=[j['executable'] for line in log.splitlines() if line.startswith('{') for j in [json.loads(line)] if j.get('reason')=='compiler-artifact' and j.get('target',{}).get('name')=='ctox' and j.get('profile',{}).get('test') and j.get('executable')]
 assert len(bins)==1,'native executable ambiguous'
 return Path(bins[0])
def case(label,mode,binary,marker=None):
 d=OUT/label;d.mkdir();env=dict(ENV,P6_FIELDS_DIR=str(d),P6_FIELDS_BINARY=str(binary))
 code,log=run(label,[node,HERE/'server-fields.mjs',mode],70,env)
 native=(d/(mode+'-native.log'));evidence=log+(native.read_text() if native.exists() else '')
 if marker is not None:
  proof=code!=0 and code!=124 and marker in evidence
  record['mutations'].append({'name':label,'expected_marker':marker,'rejected_for_expected_reason':proof});save()
  assert proof,'mutation oracle unverified: '+label
 return code
save();print('P6_FIELDS_EVIDENCE',OUT,flush=True)
try:
 assert run('node-prepare',['bash','src/core/app_runtime/verify.sh','prepare'],180)[0]==0
 node=Path(os.environ['CARGO_TARGET_DIR'])/'app-runtime-test-node/bin/node'
 ENV['CTOX_APP_RUNTIME_NODE_BUNDLE']=str(node)
 assert run('syntax',[node,'--check',HERE/'server-fields.mjs'],30)[0]==0
 parent=ROOT/'src/core/store/rights.rs';originals[parent]=parent.read_bytes()
 parent.write_bytes(originals[parent]+('\n#[cfg(test)]\n#[path = '+json.dumps(str(HERE/'server-fields.rs'))+']\nmod p6_fields;\n').encode())
 if a.reuse_native:
  producer=Path(a.reuse_native);previous=json.loads(producer.read_text())
  assert previous.get('terminal') and previous.get('binary'),'nonterminal producer'
  assert all(record['sources'].get(p)==h for p,h in previous['sources'].items() if p.endswith('.rs')),'native input changed'
  assert subprocess.run(['git','diff','--quiet',previous['head'],record['head'],'--','*.rs','Cargo.toml','Cargo.lock'],cwd=ROOT).returncode==0,'compiled Rust graph changed'
  baseline=Path(previous['binary']['path']);assert sha(baseline)==previous['binary']['sha256'],'producer binary changed'
  record['native_build_reuse']={'producer':str(producer),'compiled_head':previous['head'],'runtime_head':record['head'],'binary_sha256':sha(baseline)};save()
 else:baseline=build('baseline-build')
 retained=OUT/'native-baseline';shutil.copy2(baseline,retained)
 record['binary']={'path':str(retained),'sha256':sha(retained)};save()
 if not a.outbound_only:
  assert case('baseline-patch','patch',retained)==0,'peer patch contract failed'
  assert case('baseline-guard','guard',retained)==0,'explicit server guard contract failed'
  record['stale_intent_exit']=case('baseline-stale','stale',retained);save()
  kernel=ROOT/'src/apps/business-os/kernel/kernel.mjs';before=kernel.read_bytes()
  old=b'collection: ns, id, op, body: clean, base_rev:';assert before.count(old)==1
  try:
   kernel.write_bytes(before.replace(old,b'collection: ns, id, op, body: st.data, base_rev:'))
   record.setdefault('mutation_sources',[]).append({'name':'kernel-whole-document','path':str(kernel.relative_to(ROOT)),'line':before[:before.index(old)].count(b'\n')+1});save()
   case('mutation-kernel-whole-document','patch',retained,'kernel patch carried stale server-owned fields')
  finally:kernel.write_bytes(before)
  for label,path,old,new,mode,marker in [
   ('native-guard','src/core/store/manifest_rules.rs','return Err("command_only_field".into());','return Ok(());','guard','server-owned mutation lost specific rejection'),
   ('native-merge','src/core/store/write_contract.rs','if client == base {\n            return server.cloned();','if client == base {\n            return client.cloned();','patch','peer-only patch was rejected after concurrent native update'),
   ('native-journal','src/core/store/subscription.rs','    tx.execute("INSERT INTO _ops(op_id,participant,collection,id,gen,request_json,result_json,result_rev,created_at_ms,expires_at_ms)','    if result.reason.as_deref() == Some("command_only_field") { return Ok(()); }\n    tx.execute("INSERT INTO _ops(op_id,participant,collection,id,gen,request_json,result_json,result_rev,created_at_ms,expires_at_ms)','guard','server-owned rejection missing from _ops')]:
   p=ROOT/path;original=p.read_bytes();text=original.decode();assert text.count(old)==1,label
   try:
    p.write_text(text.replace(old,new));record['mutation_sources'].append({'name':label,'path':path,'line':text[:text.index(old)].count('\n')+1});save()
    mutant=build('build-'+label);case('mutation-'+label,mode,mutant,marker)
   finally:p.write_bytes(original)
  assert case('restored-patch','patch',retained)==0
  assert case('restored-guard','guard',retained)==0
  record['pass']=record['stale_intent_exit']==0
 code,log=run('outbound-real-guards',[retained,'p6_fields_outbound_','--nocapture','--test-threads=1'],70)
 names=['p6_fields_outbound_manifest_guard_is_loadable','p6_fields_outbound_contact_native_status_is_guarded']
 assert all(name in log for name in names),'incident guard targets not executed'
 record['outbound_findings']={'manifest_guard_load':{'failure_proven':'checked-in Outbound native field guard is not loadable' in log},'contact_native_status':{'failure_proven':'declared contact native field-status was writable' in log}}
 if code:assert code==101 and any(item['failure_proven'] for item in record['outbound_findings'].values()),'incident guard failure unclassified'
 record['outbound_guard_exit']=code;record['pass']=record.get('pass',True) and code==0;save()
except Exception as error:
 record['error']=str(error);print('P6_FIELDS_ERROR',str(error),flush=True)
finally:
 for path,original in originals.items():path.write_bytes(original)
 record['restored']={p:sha(ROOT/p)==h for p,h in record['sources'].items()}
 record['terminal']=True;save();print('P6_FIELDS_EVIDENCE',OUT,flush=True)
raise SystemExit(0 if record.get('pass') else 1)
