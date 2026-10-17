#!/usr/bin/env python3
"""One admitted candidate rehearsal, reusing the verified V3 artifact."""
import argparse,subprocess,os,json,time,signal,datetime
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--revision',required=True);p.add_argument('--output',required=True);p.add_argument('--old-receipt',required=True);p.add_argument('--deadline',required=True);p.add_argument('--signed-slot-tag');p.add_argument('--slot-source-sha');p.add_argument('--slot-sha256');p.add_argument('--expect-signaling-deploy',action='store_true');p.add_argument('--main-release',action='store_true');p.add_argument('--extra-recovery-roundtrip',action='store_true');a=p.parse_args()
lane=Path(os.environ['BUILD_LANE_BIN']).parent;out=Path(a.output).resolve();assert lane/'artifacts' in out.parents
out.mkdir(mode=0o700);runner=Path.cwd();candidate=out/'source';deadline=datetime.datetime.fromisoformat(a.deadline).timestamp()
r={'schema':'ctox.m7.candidate-rehearsal.v1','owner':'01a087a0-169a','pid':os.getpid(),'pgid':os.getpgrp(),'candidate':a.revision,'runnerHead':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'startedAtMs':int(time.time()*1000),'deadline':a.deadline,'terminal':False,'pass':False,'steps':[]}
env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',CARGO_BUILD_JOBS='2',CMAKE_BUILD_PARALLEL_LEVEL='2',RUST_TEST_THREADS='2',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_DEV_OPT_LEVEL='0',CARGO_PROFILE_DEV_CODEGEN_UNITS='256',CARGO_INCREMENTAL='0',CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS='1',M7_SLIRP_DIR=str(out/'network-tools'))
def save():(out/'measurement.json').write_text(json.dumps(r,indent=2)+'\n')
def stop(sig,_):raise RuntimeError('owned producer signal '+str(sig))
signal.signal(signal.SIGTERM,stop)
def run(name,cmd,cap=60,cwd=runner):
 left=int(deadline-time.time());assert left>20,'absolute producer stop'
 row={'name':name,'command':[str(x) for x in cmd],'startedAtMs':int(time.time()*1000),'log':str(out/(name+'.log'))};r['steps'].append(row);save()
 with (out/(name+'.log')).open('w') as log:
  child=subprocess.Popen(cmd,cwd=cwd,env=env,stdout=log,stderr=log,start_new_session=True);row.update(pid=child.pid,pgid=child.pid);save()
  try:row['exit']=child.wait(timeout=min(cap,left))
  except subprocess.TimeoutExpired:row.update(exit=124,timeout=True)
  finally:
   try:os.killpg(child.pid,signal.SIGTERM)
   except ProcessLookupError:pass
   try:child.wait(timeout=8)
   except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
 row['durationMs']=int(time.time()*1000)-row['startedAtMs'];save()
 with (out/'measurements.md').open('a') as f:f.write('| '+name+' | exit '+str(row['exit'])+' | '+str(row['durationMs'])+' ms | '+row['log']+' |\n')
 print('MEASURE',name,row['exit'],row['durationMs'],flush=True);return row['exit']
try:
 for file in ['m7-candidate-rehearsal.py','m7-network-helper.py','m7-manager-probe.py','upgrade-controller.mjs','m7-t4-smoke.mjs','m7-slot-assets.py','m7-command-readback.py','m7-release.py','m7-signed-dress.mjs','m7-signed-step.mjs','m7-extra-recovery.mjs','upgrade.mjs']:
  f=runner/'tests/sync-v4/k11'/file;cmd=['node','--check',str(f)] if f.suffix=='.mjs' else ['python3','-c',"from pathlib import Path;compile(Path(__import__('sys').argv[1]).read_text(),__import__('sys').argv[1],'exec')",str(f)];assert run('syntax-'+file,cmd)==0
 if a.signed_slot_tag:assert run('official-slot-archive-checksum',['python3',str(runner/'tests/sync-v4/k11/m7-slot-assets.py'),'--tag',a.signed_slot_tag,'--sha256',a.slot_sha256,'--output',str(out/'slot-assets')],240)==0,'official signed slot archive checksum mismatch'
 old=json.loads(Path(a.old_receipt).read_text());assert old['revision']=='4794af03db8cbf28babee0369af9c416f755019d' and old['terminal'] and old['pass'] and Path(old['binary']).is_file()
 r['baselineCustody']={'receipt':a.old_receipt,'revision':old['revision'],'binarySha256':old['binarySha256'],'reused':True};save()
 packages=out/'packages';packages.mkdir();assert run('private-slirp-download',['apt-get','download','slirp4netns'],120,packages)==0
 tools=out/'network-tools';tools.mkdir()
 for package in packages.glob('*.deb'):assert run('private-slirp-extract',['dpkg-deb','-x',str(package),str(tools)])==0
 assert run('private-slirp-version',[str(tools/'usr/bin/slirp4netns'),'--version'])==0
 assert run('candidate-worktree',['git','worktree','add','--detach',str(candidate),a.revision])==0
 assert run('candidate-pi-sidecar',['bash','-lc','ctox-prep.sh'],300,candidate)==0
 r['sourceTree']=subprocess.check_output(['git','rev-parse','HEAD:src'],cwd=candidate,text=True).strip();save()
 assert run('candidate-native-dev-package',['python3',str(runner/'tests/sync-v4/k11/m7-release.py'),'--profile','dev','--output',str(out/'release')],3300,candidate)==0,'native candidate artifact not verified'
 artifact=json.loads((out/'release/release.json').read_text());assert artifact['revision']==a.revision and artifact['pass'] and artifact['terminal']
 extra=([] if a.main_release else ['--candidate-with-signed-slot'])+['--signed-slot-tag',a.signed_slot_tag,'--slot-source-sha',a.slot_source_sha,'--slot-sha256',a.slot_sha256] if a.signed_slot_tag else ['--candidate-dry-run']
 if a.expect_signaling_deploy:extra.append('--expect-signaling-deploy')
 if a.extra_recovery_roundtrip:extra.append('--extra-recovery-roundtrip')
 code=run('private-one-command',[str(lane/'tools/with-playwright.sh'),'node','tests/sync-v4/k11/m7-signed-dress.mjs','--old-receipt',a.old_receipt,'--daemon-release',str(out/'release/release.json'),'--main-sha',a.revision,'--output',str(out/'p'),*extra],1050 if a.extra_recovery_roundtrip else 850)
 if (out/'p/result.json').exists():r['dress']=json.loads((out/'p/result.json').read_text())
 r['recoveryPass']=code==0 and r.get('dress',{}).get('preparationPass') is True
 if a.extra_recovery_roundtrip and r['recoveryPass']:
  verify=['node','tests/sync-v4/k11/m7-extra-recovery.mjs','--record',str(out/'p/rehearsal/upgrade/installer.json')]
  assert run('extra-recovery-contract',verify)==0
  assert run('extra-recovery-missing-mutation',verify+['--remove-second'])==1,'Missing second recovery must turn the guard red'
  assert run('extra-recovery-restored-control',verify)==0
 r['pass']=r['recoveryPass'] and (not a.signed_slot_tag or all(r.get('dress',{}).get(phase,{}).get('signedSlot',{}).get('status')=='pass' for phase in ['afterUpdate','afterForward']))
 r['scope']='Private lifecycle and official signed slot stage/activate; final signaling acceptance may remain not_run'
 r['signedSlot']={'requestedTag':a.signed_slot_tag,'status':'measured-in-dress' if a.signed_slot_tag else 'not_run','owner':'Shell v4 after main merge','command':'CTOX_ROOT=<private-managed/current> ctox business-os shell-update stage --version '+str(a.signed_slot_tag or '').removeprefix('business-os-shell-v')+'; CTOX_ROOT=<private-managed/current> ctox business-os shell-update activate'}
 if not r['pass']:r['error']='private rehearsal failed; actual receipts retained'
except BaseException as e:r['error']=str(e)
finally:r.update(terminal=True,finishedAtMs=int(time.time()*1000));save();print(json.dumps({k:v for k,v in r.items() if k not in ['steps','dress']}),flush=True)
raise SystemExit(0 if r['pass'] else 2)
