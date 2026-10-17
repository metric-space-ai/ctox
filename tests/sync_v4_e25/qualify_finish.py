#!/usr/bin/env python3
"""E30 and productive Core binding invariants; canonical lane only."""
import argparse,pathlib,subprocess,os,time,signal,json,hashlib,datetime,re,shutil
p=argparse.ArgumentParser();p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--deadline',required=True);p.add_argument('--stage-timeout',type=int,default=600);p.add_argument('--cache-baseline',action='store_true');p.add_argument('--only-mutation',action='append');a=p.parse_args();a.out.mkdir(parents=True,exist_ok=True)
deadline=datetime.datetime.fromisoformat(a.deadline.replace('Z','+00:00')).timestamp()
def interrupted(_signal,_frame): raise SystemExit(130)
signal.signal(signal.SIGTERM,interrupted)
command=['cargo','test','--locked','-p','ctox','--bin','ctox','--jobs','2']
receipt={'profile':'isolated native Core composition; not installed acceptance','pass':False,'steps':[],'mutations':[],'selected_mutations':a.only_mutation}
def save(): (a.out/'mutation.json').write_text(json.dumps(receipt,indent=2)+'\n')
baseline_binary=None
pristine_hashes={}
def run(name,filter,features=True,pristine=False):
 if pristine and baseline_binary:
  assert all(hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==digest for f,digest in pristine_hashes.items()),'restored source differs from immutable baseline'
 cmd=([str(baseline_binary),filter,'--nocapture'] if pristine and baseline_binary else command+(['--features','app-runtime-node'] if features else [])+[filter,'--','--nocapture']);start=time.monotonic()
 with (a.out/(name+'.log')).open('w') as log:
  proc=subprocess.Popen(cmd,stdout=log,stderr=subprocess.STDOUT,start_new_session=True,env={**os.environ,'RUST_TEST_THREADS':'2','CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS':'1'})
  receipt['active']={'pid':proc.pid,'pgid':proc.pid,'command':cmd,'stop_condition':'terminal, stage timeout or order deadline'};save()
  try: code=proc.wait(timeout=max(1,min(a.stage_timeout,deadline-time.time())))
  except subprocess.TimeoutExpired:
   os.killpg(proc.pid,signal.SIGTERM)
   try: proc.wait(timeout=5)
   except subprocess.TimeoutExpired: os.killpg(proc.pid,signal.SIGKILL);proc.wait()
   code=124
  except BaseException:
   if proc.poll() is None:
    os.killpg(proc.pid,signal.SIGTERM)
    try:proc.wait(timeout=5)
    except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
   raise
  finally:receipt.pop('active',None)
 receipt['steps'].append({'name':name,'exit':code,'elapsed_s':time.monotonic()-start});save()
 return code,(a.out/(name+'.log')).read_text()
try:
 code,text=run('e30-baseline','e30_');assert code==0 and 'e30_native_files_require_exported_row_or_visible_actual_reference' in text and 'e30_real_native_provider_rechecks_rights_before_any_frame' in text,text[-5000:]
 path=pathlib.Path('src/core/business_os/rxdb_peer_v4_files.rs')
 if a.cache_baseline:
  digest=hashlib.sha256(path.read_bytes()).hexdigest();assert 'E30_SOURCE_SHA256 '+digest in text,'missing rebuilt E30 production marker'
  found=re.findall(r'Running unittests .*? \(([^)]+)\)',text);assert len(found)==1,found
  baseline_binary=a.out.resolve()/'immutable-baseline';shutil.copy2(found[0],baseline_binary)
  pristine_hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [path,pathlib.Path('src/core/business_os/app_platform/runtime_host/feed.rs'),pathlib.Path('src/core/business_os/rxdb_peer_v4_files_tests.rs'),pathlib.Path('tests/sync_v4_e25/native_binding.rs')]}
  hasher=hashlib.sha256()
  with baseline_binary.open('rb') as binary:
   for block in iter(lambda:binary.read(1024*1024),b''):hasher.update(block)
  receipt['immutable_baseline']={'binary_sha256':hasher.hexdigest(),'sources':pristine_hashes};save()
 dispatch_code,dispatch_text=run('native-command-bus','e25_native_command_bus_preserves_user_and_skips_legacy_intake',True,pristine=True);receipt['native_dispatch_pass']=dispatch_code==0;save()
 core_code,core_text=run('native-core-baseline','native_daemon_hotplug_roundtrip_and_change',True,pristine=True)
 receipt['native_core_baseline_pass']=core_code==0 and 'E25_NATIVE_CORE_BINDING' in core_text and 'E25_CORE_FEED_SHA256 '+hashlib.sha256(pathlib.Path('src/core/business_os/app_platform/runtime_host/feed.rs').read_bytes()).hexdigest() in core_text;save()
 cases=[('e30-owner-search',rb'let own_metadata\s*=\s*doc\["owner_app"\]\s*==\s*app\s*&&\s*doc\["owner_user"\]\s*==\s*actor_id;',b'let own_metadata=false;','e30_native_files_require'),('e30-owned-file',rb'let own_read\s*=\s*own;',b'let own_read=false;','e30_native_files_require'),('e30-export-row',rb'let mut allowed\s*=\s*own_read;',b'let mut allowed=true;','e30_native_files_require'),('e30-hidden-reference',rb'(?<=\|projected\|)\s*holds_ref\(&projected,\s*reference\)',b'holds_ref(&doc,reference)','e30_native_files_require')]
 if a.only_mutation:
  assert set(a.only_mutation)<=set(c[0] for c in cases),'unknown production mutation'
  cases=[c for c in cases if c[0] in a.only_mutation]
 for name,old,new,test in cases:
  clean=path.read_bytes();candidate,count=re.subn(old,lambda _:new,clean);assert count==1,(name,'production anchor',count)
  try:
   path.write_bytes(candidate);os.utime(path,None);code,text=run(name,test)
   assert 'E30_SOURCE_SHA256 '+hashlib.sha256(candidate).hexdigest() in text,(name,'mutant binary did not print changed production source marker');assert code!=0 and 'FAILED' in text and 'could not compile' not in text,(name,code,text[-5000:]);receipt['mutations'].append({'name':name,'source_sha256':hashlib.sha256(clean).hexdigest(),'production_anchor':old.decode(),'test':test,'exit':code});save()
  finally:path.write_bytes(clean);os.utime(path,None)
 code,text=run('e30-restored','e30_',pristine=True);assert code==0,text[-5000:]
 assert receipt['native_core_baseline_pass'],'native Core baseline failed; E30 delta proof retained separately'
 path=pathlib.Path('src/core/business_os/app_platform/runtime_host/feed.rs');clean=path.read_bytes();old=b'if e25_core {';assert clean.count(old)==1
 try:
  path.write_bytes(clean.replace(old,b'if false {'));os.utime(path,None);code,text=run('native-core-route-removed','native_daemon_hotplug_roundtrip_and_change',True)
  assert 'E25_CORE_FEED_SHA256 '+hashlib.sha256(clean.replace(old,b'if false {')).hexdigest() in text,'Core mutant binary did not print changed feed source marker'
  assert code!=0 and 'FAILED' in text and 'could not compile' not in text and 'E25 real Core feed failed' in text,(code,text[-7000:]);receipt['mutations'].append({'name':'productive-core-route','production_anchor':old.decode(),'source_sha256':hashlib.sha256(clean).hexdigest(),'test':'native_daemon_hotplug_roundtrip_and_change','exit':code});save()
 finally:path.write_bytes(clean);os.utime(path,None)
 code,text=run('native-core-restored','native_daemon_hotplug_roundtrip_and_change',True,pristine=True);assert code==0 and 'E25_NATIVE_CORE_BINDING' in text and 'E25_CORE_FEED_SHA256 '+hashlib.sha256(pathlib.Path('src/core/business_os/app_platform/runtime_host/feed.rs').read_bytes()).hexdigest() in text,text[-7000:]
 receipt['pass']=receipt.get('native_dispatch_pass',False)
finally:save()
print(json.dumps(receipt))
raise SystemExit(0 if receipt['pass'] else 1)
