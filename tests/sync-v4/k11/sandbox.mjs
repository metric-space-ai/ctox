// Privileged setup only, in already-new PID/mount/net namespaces. Host /run is
// hidden before dropping privileges; PID namespace death retires every child.
import fs from 'node:fs';
import {execFileSync,spawn} from 'node:child_process';
const file=process.argv[2],c=JSON.parse(fs.readFileSync(file));
if(process.getuid()!==0||fs.statSync(file).uid!==c.uid||c.uid<=0)throw Error('Root setup of owned fixture only');
for(const n of ['net','mnt','pid'])if(fs.readlinkSync('/proc/self/ns/'+n)===c.host[n])throw Error('Private namespace missing: '+n);
execFileSync('mount',['--make-rprivate','/']);
execFileSync('mount',['-t','tmpfs','-o','mode=755,size=16m','tmpfs','/run']);
execFileSync('ip',['link','set','lo','up']);
c.proof={host:c.host,runtimeMasked:true,namespaces:Object.fromEntries(['net','mnt','pid'].map(n=>[n,fs.readlinkSync('/proc/self/ns/'+n)]))};
fs.writeFileSync(file,JSON.stringify(c,null,2));fs.chownSync(file,c.uid,c.gid);
const child=spawn('setpriv',[`--reuid=${c.uid}`,`--regid=${c.gid}`,'--init-groups','--reset-env',process.execPath,c.runner,file],{stdio:'inherit'});
const timer=setTimeout(()=>child.kill('SIGKILL'),10*60*1000);
process.once('SIGTERM',()=>child.kill('SIGTERM'));
child.once('exit',code=>{clearTimeout(timer);process.exit(code??1);});
