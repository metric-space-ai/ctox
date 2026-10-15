// Privileged setup only, in already-new PID/mount/net namespaces. Host /run is
// hidden before dropping privileges; PID namespace death retires every child.
import fs from 'node:fs';
import {spawn} from 'node:child_process';
const file=process.argv[2],c=JSON.parse(fs.readFileSync(file));
if(process.getuid()!==0||fs.statSync(file).uid!==c.uid||c.uid<=0)throw Error('Root setup of owned fixture only');
const child=spawn('python3',[new URL('./m7-manager-probe.py',import.meta.url).pathname,'--setup',file],{stdio:'inherit'});
const timer=setTimeout(()=>child.kill('SIGKILL'),10*60*1000);
process.once('SIGTERM',()=>child.kill('SIGTERM'));
child.once('exit',code=>{clearTimeout(timer);process.exit(code??1);});
