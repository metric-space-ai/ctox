// Test-owned namespace init. Host authority is never elevated.
import fs from 'node:fs';
import {dirname,join} from 'node:path';
import {execFileSync,spawn} from 'node:child_process';
const path=process.argv[2],c=JSON.parse(fs.readFileSync(path));
const ns=kind=>fs.readlinkSync('/proc/self/ns/'+kind);
const mapping=kind=>fs.readFileSync('/proc/self/'+kind+'_map','utf8').trim().split(/\s+/).map(Number);
const uid=mapping('uid'),gid=mapping('gid');
if(process.getuid()!==0 || process.pid!==1 || uid.length!==3 || uid[0]!==0 || uid[1]!==c.uid || uid[2]!==1
  || gid.length!==3 || gid[0]!==0 || gid[1]!==c.gid || gid[2]!==1
  || ns('user')===c.hostNamespaces.user || ns('net')===c.hostNamespaces.net
  || ns('mnt')===c.hostNamespaces.mnt || ns('pid')===c.hostNamespaces.pid)
  throw Error('Private mapped user/net/mount/PID namespaces required');
const proof={owner:c.owner,uidMap:uid,gidMap:gid,pid:process.pid,namespaces:Object.fromEntries(['user','net','mnt','pid'].map(k=>[k,ns(k)])),
  hostNamespaces:c.hostNamespaces,hostUid:c.uid,hostGid:c.gid,hostNetworkChanged:false,privilegedHostEntry:false};
const run=(cmd,args)=>execFileSync(cmd,args,{stdio:['ignore','pipe','pipe'],encoding:'utf8'});
run('mount',['-t','tmpfs','-o','mode=1777,size=256m','p6-private-tmp','/tmp']);
run('ip',['link','set','lo','up']);
run('ip',['link','add','bench0','type','dummy']);
run('ip',['addr','add','192.0.2.1/32','dev','bench0']);
run('ip',['link','set','bench0','up']);
run('ip',['route','add','default','dev','bench0']);
run('tc',['qdisc','add','dev','lo','root','handle','1:','prio','bands','3']);
run('tc',['qdisc','add','dev','lo','parent','1:3','handle','30:','netem','delay',String(c.rttMs/2)+'ms','loss',String(c.lossPercent)+'%','limit','10000']);
for(const direction of ['dport','sport'])run('tc',['filter','add','dev','lo','protocol','ip','parent','1:','prio','1','u32','match','ip','protocol','17','0xff','match','ip',direction,String(c.turnPort),'0xffff','flowid','1:3']);
proof.addresses=JSON.parse(run('ip',['-j','addr']));
proof.routes=JSON.parse(run('ip',['-j','route']));
proof.qdisc=run('tc',['-s','qdisc','show','dev','lo']);
proof.shaping='netem inside unprivileged mapped netns; browser/TURN UDP leg';
proof.processLimit={insideSandbox:true,soft:4096,hard:4096};
c.netem={isolated:true,namespace:ns('net'),rttMs:c.rttMs,lossPercent:c.lossPercent,hostQdiscChanged:false};
fs.writeFileSync(path,JSON.stringify(c,null,2),{mode:0o600});
fs.writeFileSync(join(dirname(path),'p6-namespace.json'),JSON.stringify(proof,null,2));
const child=spawn('prlimit',['--nproc=4096:4096','--',process.execPath,c.runner,path],{stdio:'inherit'});
let force;
const timer=setTimeout(()=>{child.kill('SIGTERM');force=setTimeout(()=>child.kill('SIGKILL'),5000);},(c.durationSeconds+10)*1000);
process.once('SIGTERM',()=>child.kill('SIGTERM'));
const exit=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',(code,signal)=>resolve({code,signal}));});
clearTimeout(timer);clearTimeout(force);
proof.runnerExit=exit;proof.finishedAt=new Date().toISOString();
proof.qdiscFinal=run('tc',['-s','qdisc','show','dev','lo']);
fs.writeFileSync(join(dirname(path),'p6-namespace.json'),JSON.stringify(proof,null,2));
// Exiting PID 1 also terminates any descendant that escaped its process group.
process.exitCode=exit.code??1;
