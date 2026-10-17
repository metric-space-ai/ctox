// Real foreground control on an already-owned installed rehearsal profile.
// No installer, host service manager, live DB copy or production tenant access.
import fs from 'node:fs';
import {resolve,join} from 'node:path';
import {spawn,spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {verifiedBuild,owner,until} from '../../sync-v4-e2e/native.mjs';
const arg=n=>{const i=process.argv.indexOf('--'+n);if(i<0)throw Error('Missing --'+n);return process.argv[i+1];};
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Admitted Linux lane required');
const lane=resolve(process.env.BUILD_LANE_BIN,'..'),allowed=join(lane,'artifacts')+'/',root=resolve(arg('root')),out=resolve(arg('output'));
const image=verifiedBuild(arg('release'));
if(!root.startsWith(allowed)||!out.startsWith(allowed)||fs.existsSync(out))throw Error('Owned private profile and new artifact output required');
const marker=JSON.parse(fs.readFileSync(join(root,'.sync-v4-e2e.json')));
if(marker.owner!==owner||marker.synthetic!==true)throw Error('Only our synthetic rehearsal profile');
const current=fs.realpathSync(join(root,'managed/current')),binary=join(current,'bin/ctox');
const hash=x=>createHash('sha256').update(x).digest('hex');
if(hash(fs.readFileSync(binary))!==image.binarySha256)throw Error('Actual installed image differs');
fs.mkdirSync(out,{mode:0o700});
const home=join(root,'installer-home'),state=join(root,'runtime');
const env={PATH:process.env.PATH,HOME:home,TMPDIR:process.env.TMPDIR,LANG:'C.UTF-8',CTOX_ROOT:join(root,'managed/current'),CTOX_INSTALL_ROOT:join(root,'managed'),CTOX_STATE_ROOT:state,XDG_CONFIG_HOME:join(home,'.config'),XDG_CACHE_HOME:join(home,'.cache'),XDG_RUNTIME_DIR:join(out,'run'),BUILD_LANE_BIN:process.env.BUILD_LANE_BIN};
fs.mkdirSync(env.XDG_RUNTIME_DIR,{mode:0o700});
const r={schema:'ctox.m7.launch-context-control.v1',owner,startedAtMs:Date.now(),terminal:false,pass:false,source:image.revision,binarySha256:image.binarySha256,root,scope:'Existing private installed candidate, host-network foreground component only; not recovery or signed-shell acceptance',samples:[],hostServiceCommands:false,customerAccess:false};
const save=()=>fs.writeFileSync(join(out,'result.json'),JSON.stringify(r,null,2),{mode:0o600});save();let child,exit,interrupted=false;
process.once('SIGTERM',()=>{interrupted=true;if(child)try{process.kill(-child.pid,'SIGTERM');}catch{}});
const readback=phase=>{const p=spawnSync('python3',[new URL('./m7-data-readback.py',import.meta.url).pathname,'--root',root,'--output',join(out,'data-'+phase+'.json'),'--expect',arg('expect')],{env,encoding:'utf8',timeout:30000});if(p.status!==0)throw Error('Quiescent private data readback failed');return JSON.parse(fs.readFileSync(join(out,'data-'+phase+'.json')));};
try{
 const previous=JSON.parse(fs.readFileSync(resolve(root,'../upgrade/controller.json')));
 const manager=JSON.parse(fs.readFileSync(resolve(root,'../upgrade/service-manager.json')));
 if(previous.groupAbsent!==true||manager.childrenTerminal!==true||manager.privateCgroupRemoved!==true)throw Error('Prior owned namespace is not terminal');
 try{process.kill(-previous.pgid,0);throw Error('Prior namespace group still exists');}catch(e){if(e.code!=='ESRCH')throw e;}
 // SQL API row readback and the fixture helper's owner-socket check precede config.
 r.before=readback('before');save();
 const config=spawnSync('python3',[new URL('./m7-bootstrap-fixture.py',import.meta.url).pathname,'--root',root,'--output',join(out,'fixture-address.json')],{env,encoding:'utf8',timeout:10000});
 if(config.status!==0)throw Error('Owned quiescent fixture configuration failed');
 if(interrupted)throw Error('Owned foreground control interrupted');
 const log=fs.openSync(join(out,'foreground.log'),'w',0o600);child=spawn(binary,['service','--foreground','--root',env.CTOX_ROOT],{cwd:current,env,detached:true,stdio:['ignore',log,log]});fs.closeSync(log);
 exit=new Promise((res,rej)=>{child.once('exit',(code,signal)=>res({code,signal}));child.once('error',rej);});
 r.process={pid:child.pid,pgid:child.pid,purpose:'Actual private candidate foreground/HTTP control with working host DNS',stop:'finally or180s admitted command deadline'};save();
 let base;
 await until(()=>{if(child.exitCode!==null||child.signalCode!==null)throw Error('Private foreground exited');const text=fs.readFileSync(join(out,'foreground.log'),'utf8');const prefix='CTOX Business OS listening on ';const line=text.split('\n').find(x=>x.startsWith(prefix));if(line){const url=new URL(line.slice(prefix.length).split(' ')[0]);if(url.hostname!=='127.0.0.1'||!Number(url.port))throw Error('Non-private listener');base=url.origin;return true;}return false;},35000,'actual private ephemeral HTTP listener');
 r.base=base;
 for(let n=0;n<20;n++){
  if(interrupted)throw Error('Owned foreground control interrupted');
  if(Date.now()-r.startedAtMs>105000)throw Error('Private control deadline');
  const at=Date.now();let row={index:n};
  try{const response=await fetch(base+'/api/business-os/launch-context',{signal:AbortSignal.timeout(5000)});const body=await response.text();let value;try{value=JSON.parse(body);}catch{}
   row={...row,httpStatus:response.status,durationMs:Date.now()-at,hasV4:!!value&&typeof value.v4==='object'&&value.v4!==null,bytes:Buffer.byteLength(body)};
   const code=value?.error?.code??value?.code;if(typeof code==='string'&&/^[a-z0-9_.:-]{1,128}$/.test(code))row.errorCode=code;
  }catch(e){row.errorClass=e.name;row.durationMs=Date.now()-at;}
  r.samples.push(row);save();
 }
 r.http200=r.samples.filter(x=>x.httpStatus===200&&x.hasV4).length;r.httpFail=r.samples.length-r.http200;
}catch(e){r.error=e.message;}
finally{
 if(child){try{process.kill(-child.pid,'SIGTERM');}catch(e){if(e.code!=='ESRCH')throw e;}const kill=setTimeout(()=>{try{process.kill(-child.pid,'SIGKILL');}catch{}},8000);r.process.exit=await exit;clearTimeout(kill);try{process.kill(-child.pid,'SIGKILL');}catch(e){if(e.code!=='ESRCH')throw e;}
  r.process.groupAbsent=await until(()=>{try{process.kill(-child.pid,0);return false;}catch(e){if(e.code==='ESRCH')return true;throw e;}},5000,'owned group exit').then(()=>true,()=>false);
 }
 try{r.after=readback('after');}catch(e){r.readbackError=e.message;}
 r.pass=r.http200===20&&r.httpFail===0&&r.process?.groupAbsent===true&&r.after?.matchesBefore===true;r.terminal=true;r.finishedAtMs=Date.now();save();
}
console.log(JSON.stringify({pass:r.pass,http200:r.http200,httpFail:r.httpFail,error:r.error,rows:r.after?.rows,groupAbsent:r.process?.groupAbsent}));process.exitCode=r.pass?0:2;
