#!/usr/bin/env python3
"""E30 and productive Core binding invariants; canonical lane only."""
import argparse,pathlib,subprocess,os,time,signal,json,hashlib,datetime,re
p=argparse.ArgumentParser();p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--deadline',required=True);p.add_argument('--stage-timeout',type=int,default=600);a=p.parse_args();a.out.mkdir(parents=True,exist_ok=True)
deadline=datetime.datetime.fromisoformat(a.deadline.replace('Z','+00:00')).timestamp()
def interrupted(_signal,_frame): raise SystemExit(130)
signal.signal(signal.SIGTERM,interrupted)
command=['cargo','test','--locked','-p','ctox','--bin','ctox','--jobs','2']
receipt={'profile':'isolated native Core composition; not installed acceptance','pass':False,'steps':[],'mutations':[]}
def save(): (a.out/'mutation.json').write_text(json.dumps(receipt,indent=2)+'\n')
def run(name,filter,features=True):
 cmd=command+(['--features','app-runtime-node'] if features else [])+[filter,'--','--nocapture'];start=time.monotonic()
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
 cases=[('e30-owned-file',rb'let own_read\s*=\s*own;',b'let own_read=false;','e30_native_files_require'),('e30-owner-search',rb'let own_metadata\s*=\s*doc\["owner_app"\]\s*==\s*app\s*&&\s*doc\["owner_user"\]\s*==\s*actor_id;',b'let own_metadata=false;','e30_native_files_require'),('e30-export-row',rb'let mut allowed\s*=\s*own_read;',b'let mut allowed=true;','e30_native_files_require'),('e30-hidden-reference',rb'(?<=\|projected\|)\s*holds_ref\(&projected,\s*reference\)',b'holds_ref(&doc,reference)','e30_native_files_require')]
 for name,old,new,test in cases:
  clean=path.read_bytes();candidate,count=re.subn(old,lambda _:new,clean);assert count==1,(name,'production anchor',count)
  try:
   path.write_bytes(candidate);os.utime(path,None);code,text=run(name,test)
   assert code!=0 and 'FAILED' in text and 'could not compile' not in text,(name,code,text[-5000:]);receipt['mutations'].append({'name':name,'source_sha256':hashlib.sha256(clean).hexdigest(),'production_anchor':old.decode(),'test':test,'exit':code});save()
  finally:path.write_bytes(clean);os.utime(path,None)
 code,text=run('e30-restored','e30_');assert code==0,text[-5000:]
 code,text=run('native-core-baseline','native_daemon_hotplug_roundtrip_and_change',True);assert code==0 and 'E25_NATIVE_CORE_BINDING' in text,text[-7000:]
 path=pathlib.Path('src/core/business_os/app_platform/runtime_host/feed.rs');clean=path.read_bytes();old=b'if e25_core {';assert clean.count(old)==1
 try:
  path.write_bytes(clean.replace(old,b'if false {'));os.utime(path,None);code,text=run('native-core-route-removed','native_daemon_hotplug_roundtrip_and_change',True)
  assert code!=0 and 'FAILED' in text and 'could not compile' not in text and 'E25 real Core feed failed' in text,(code,text[-7000:]);receipt['mutations'].append({'name':'productive-core-route','production_anchor':old.decode(),'source_sha256':hashlib.sha256(clean).hexdigest(),'test':'native_daemon_hotplug_roundtrip_and_change','exit':code});save()
 finally:path.write_bytes(clean);os.utime(path,None)
 code,text=run('native-core-restored','native_daemon_hotplug_roundtrip_and_change',True);assert code==0 and 'E25_NATIVE_CORE_BINDING' in text,text[-7000:]
 receipt['pass']=True
finally:save()
print(json.dumps(receipt))
