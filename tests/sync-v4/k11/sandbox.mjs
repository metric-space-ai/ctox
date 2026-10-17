// Privileged setup only, in already-new PID/mount/net namespaces. Host /run is
// hidden before dropping privileges; PID namespace death retires every child.
import fs from 'node:fs';
import {spawn} from 'node:child_process';
const file=process.argv[2],c=JSON.parse(fs.readFileSync(file));
if(process.getuid()!==0||c.uid<=0)throw Error('Root setup of owned fixture only');
if(c.mappedUser){
 const mapped=fs.readFileSync('/proc/self/uid_map','utf8').trim().split(/\s+/).map(Number);
 if(mapped.length!==3||mapped[0]!==0||mapped[1]!==c.uid||mapped[2]!==1||fs.statSync(file).uid!==0||fs.readlinkSync('/proc/self/ns/user')===c.host.user)throw Error('Exactly one own host UID must map to namespace root');
 c.hostUid=c.uid;c.hostGid=c.gid;c.uid=0;c.gid=0;fs.writeFileSync(file,JSON.stringify(c));
}else if(fs.statSync(file).uid!==c.uid)throw Error('Owned fixture config required');
const child=spawn('python3',[new URL('./m7-manager-probe.py',import.meta.url).pathname,'--setup',file],{stdio:'inherit'});
const timer=setTimeout(()=>child.kill('SIGKILL'),10*60*1000);
process.once('SIGTERM',()=>child.kill('SIGTERM'));
child.once('exit',code=>{clearTimeout(timer);process.exit(code??1);});
