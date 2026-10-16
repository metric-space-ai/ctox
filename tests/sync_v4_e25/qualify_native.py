#!/usr/bin/env python3
"""Narrow E13 native regression proofs. Execute only through the build lane."""
import argparse,hashlib,json,os,pathlib,subprocess
p=argparse.ArgumentParser();p.add_argument('--out',type=pathlib.Path,required=True);a=p.parse_args();a.out.mkdir(parents=True,exist_ok=True)
command=['cargo','test','--locked','--manifest-path','src/core/rxdb/Cargo.toml','--lib']
receipt={'profile':'native component, not installed product acceptance','mutations':[],'pass':False}
def run(name,filter):
 with (a.out/(name+'.log')).open('w') as log:
  result=subprocess.run(command+[filter,'--','--nocapture'],stdout=log,stderr=subprocess.STDOUT,env={**os.environ,'RUST_TEST_THREADS':'2'},timeout=180)
 return result.returncode,(a.out/(name+'.log')).read_text()
def save(): (a.out/'mutation.json').write_text(json.dumps(receipt,indent=2)+'\n')
try:
 code,text=run('baseline','e25_');assert code==0,text[-4000:]
 receipt['baseline_exit']=code;save()
 cases=[
 ('terminal-event-cursor','v4_native_sessions.rs',b'let mut value = self.feed.current(self.after)?;',b'let mut value = self.feed.current(self.after)?; if terminal(&value) { value = self.feed.current(0)?; }','e25_speech_session_controls_and_native_events_keep_one_opening_identity'),
 ('control-cannot-reset-partial','v4_native_sessions.rs',b'if terminal(&canonical) {',b'if true {','e25_speech_session_controls_and_native_events_keep_one_opening_identity'),
 ('cas-chunk-hash','v4_session.rs',b'Sha256::digest(&data)',b'Sha256::digest(b"wrong chunk")','e25_native_cold_range_sends_complete_hashed_chunks_and_fences_revocation'),
 ]
 for name,file,old,new,test in cases:
  path=pathlib.Path('src/core/rxdb/src/plugins/replication_webrtc')/file;clean=path.read_bytes();assert clean.count(old)==1
  try:
   path.write_bytes(clean.replace(old,new));os.utime(path,None);code,text=run(name,test)
   assert code!=0 and 'FAILED' in text and 'could not compile' not in text,(name,code,text[-4000:])
   receipt['mutations'].append({'invariant':name,'test':test,'production_anchor':old.decode(),'mutant_exit':code,'source_sha256':hashlib.sha256(clean).hexdigest()});save()
  finally:path.write_bytes(clean);os.utime(path,None)
 code,text=run('restored','e25_');assert code==0,text[-4000:]
 receipt.update(pass_=True,restored_exit=code);receipt['pass']=receipt.pop('pass_');save()
finally:save()
print(json.dumps(receipt))
