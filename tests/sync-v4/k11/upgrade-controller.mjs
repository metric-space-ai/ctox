import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawn,execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {verifiedUpgrade} from './upgrade.mjs';
const [oldReceipt,newReceipt,root,directory,roundtrip,resume,dressConfig]=process.argv.slice(2);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical lane required');
verifiedUpgrade(oldReceipt,newReceipt,root);
const output=resolve(directory),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts');
if(!output.startsWith(allowed+'/'))throw Error('Private artifact output required');
fs.mkdirSync(output,{recursive:false,mode:0o700});
const here=fileURLToPath(new URL('.',import.meta.url));
if(resume&&resume!=='resume-rolled-back')throw Error('Unknown recovery mode');
const privileged=process.env.M7_NAMESPACE_MODE==='privileged';
const config={owner:'01a087a0-169a',oldReceipt,newReceipt,root:resolve(root),output,roundtrip:roundtrip==='yes',resumeRolledBack:resume==='resume-rolled-back',dressSmoke:dressConfig?JSON.parse(fs.readFileSync(dressConfig)):null,tmpdir:process.env.TMPDIR,path:process.env.PATH,laneBin:process.env.BUILD_LANE_BIN,node:process.execPath,uid:process.getuid(),gid:process.getgid(),mappedUser:!privileged,traceRollback:process.env.M7_TRACE_ROLLBACK==='1',runner:join(here,'upgrade-runner.mjs'),host:Object.fromEntries(['user','net','mnt','pid','cgroup'].map(n=>[n,fs.readlinkSync('/proc/self/ns/'+n)]))};
const file=join(output,'sandbox.json');fs.writeFileSync(file,JSON.stringify(config,null,2),{mode:0o600});
const ns=['--kill-child=SIGKILL','--net','--mount','--pid','--fork','--mount-proc',process.execPath,join(here,'sandbox.mjs'),file];
if(config.dressSmoke)execFileSync('mkfifo',[join(output,'network-ack')]);
const child=privileged?spawn('sudo',['-n','unshare',...ns],{stdio:'inherit',detached:true}):spawn('unshare',['--user','--map-root-user',...ns],{stdio:'inherit',detached:true});
const row={owner:'01a087a0-169a',pid:child.pid,pgid:child.pid,purpose:'supported installer in private PID/mount/network namespaces',stop:'terminal or 10 min',startedAt:new Date().toISOString()};
const save=()=>fs.writeFileSync(join(output,'controller.json'),JSON.stringify(row,null,2));save();
const kill=s=>{try{process.kill(-child.pid,s);}catch(e){if(e.code!=='ESRCH')throw e;}};
const timer=setTimeout(()=>kill('SIGKILL'),10*60*1000);process.once('SIGTERM',()=>kill('SIGKILL'));
let network;
try {
 if(config.dressSmoke){
  const request=join(output,'network-request.json');
  await new Promise((resolve,reject)=>{
   const done=()=>{if(fs.existsSync(request)){clearTimeout(timer);watch.close();resolve();}};
   const watch=fs.watch(output,done),timer=setTimeout(()=>{watch.close();reject(Error('Private network namespace readiness deadline'));},20000);done();
  });
  if(fs.readlinkSync('/proc/'+child.pid+'/ns/net')===config.host.net)throw Error('Refuse slirp on host namespace');
  const log=fs.openSync(join(output,'slirp.log'),'w',0o600);
  network=spawn('slirp4netns',['--configure','--disable-host-loopback','--userns-path=/proc/'+child.pid+'/ns/user','--ready-fd=3',String(child.pid),'tap0'],{stdio:['ignore',log,log,'pipe'],detached:true});fs.closeSync(log);
  row.network={pid:network.pid,pgid:network.pid,purpose:'rootless private namespace HTTPS for official signed release stage',hostLoopbackDisabled:true,stop:'namespace exit or controller10min deadline'};save();
  await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('slirp readiness deadline')),15000);network.stdio[3].once('data',()=>{clearTimeout(timer);resolve();});network.once('error',reject);network.once('exit',code=>{clearTimeout(timer);reject(Error('slirp exited '+code));});});
  const ack=fs.openSync(join(output,'network-ack'),fs.constants.O_WRONLY|fs.constants.O_NONBLOCK);try{fs.writeSync(ack,'1');}finally{fs.closeSync(ack);}
 }
 row.exit=(child.exitCode!==null||child.signalCode!==null)?{code:child.exitCode,signal:child.signalCode}:await new Promise((r,j)=>{child.once('exit',(code,signal)=>r({code,signal}));child.once('error',j);});
}catch(error){row.error=error.message;row.exit={code:1};}
finally{
 clearTimeout(timer);kill('SIGKILL');
 if(network){try{process.kill(-network.pid,'SIGTERM');}catch(e){if(e.code!=='ESRCH')throw e;}await new Promise(r=>{if(network.exitCode!==null||network.signalCode!==null){r();return;}const t=setTimeout(()=>{try{process.kill(-network.pid,'SIGKILL');}catch{}},5000);network.once('exit',()=>{clearTimeout(t);r();});});row.network.terminal=true;}
 row.finishedAt=new Date().toISOString();row.groupAbsent=false;
 for(let i=0;i<100;i++){try{process.kill(-child.pid,0);}catch(e){if(e.code==='ESRCH'){row.groupAbsent=true;break;}throw e;}await new Promise(r=>setTimeout(r,50));}
 save();
}
if(!row.groupAbsent)throw Error('Installer sandbox group remains');process.exitCode=row.exit.code??1;
