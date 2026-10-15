// This helper requires root only to configure a brand-new network namespace.
import fs from 'node:fs';
import { execFileSync, spawn } from 'node:child_process';
const path = process.argv[2], c = JSON.parse(fs.readFileSync(path));
if (process.getuid() !== 0 || fs.readlinkSync('/proc/self/ns/net') === c.hostNetworkNamespace) throw Error('Distinct owned network namespace required');
if (!Number.isInteger(c.uid) || c.uid <= 0 || fs.statSync(path).uid !== c.uid) throw Error('Non-root owner and matching private config required');
const ownedNamespace = fs.readlinkSync('/proc/self/ns/net');
const sweep = () => {
  // A detached daemon group must not escape cleanup if Node itself crashes.
  for (const name of fs.readdirSync('/proc').filter(n => /^[0-9]+$/.test(n))) {
    const pid = Number(name); if (pid === process.pid) continue;
    try { if (fs.readlinkSync('/proc/' + name + '/ns/net') === ownedNamespace) process.kill(pid, 'SIGKILL'); } catch (e) { if (!['ENOENT','ESRCH'].includes(e.code)) throw e; }
  }
};
const run = (bin, args) => execFileSync(bin, args, { stdio: 'inherit' });
run('ip',['link','set','lo','up']); run('ip',['link','add','bench0','type','dummy']);
run('ip',['addr','add','192.0.2.1/32','dev','bench0']); run('ip',['link','set','bench0','up']);
run('ip',['route','add','default','dev','bench0']);
run('tc',['qdisc','add','dev','lo','root','handle','1:','prio','bands','3']);
run('tc',['qdisc','add','dev','lo','parent','1:3','handle','30:','netem','delay',`${c.rttMs/2}ms`,'loss',`${c.lossPercent}%`,'limit','10000']);
for (const direction of ['dport','sport']) run('tc',['filter','add','dev','lo','parent','1:','protocol','ip','prio','1','u32','match','ip','protocol','17','0xff','match','ip',direction,String(c.turnPort),'0xffff','flowid','1:3']);
c.netem = { isolated: true, hostQdiscChanged: false, ownerPid: process.pid, namespace: fs.readlinkSync('/proc/self/ns/net'), rttMs: c.rttMs, lossPercent: c.lossPercent, shaped: 'browser/TURN UDP leg' };
fs.writeFileSync(path, JSON.stringify(c,null,2)); fs.chownSync(path,c.uid,c.gid);
const saveNetem = name => {
  const file = c.output + '/' + name;
  fs.writeFileSync(file, execFileSync('tc',['-s','qdisc','show','dev','lo']), { mode: 0o600 });
  fs.chownSync(file,c.uid,c.gid);
};
saveNetem('netem-start.txt');
const child = spawn('setpriv',[`--reuid=${c.uid}`,`--regid=${c.gid}`,'--init-groups','--reset-env',process.execPath,c.runner,path],{stdio:'inherit',detached:true});
const kill = signal => { try { process.kill(-child.pid,signal); } catch (e) { if(e.code!=='ESRCH') throw e; } };
const timer = setTimeout(() => { kill('SIGTERM'); setTimeout(()=>kill('SIGKILL'),5000).unref(); },(c.durationSeconds+20)*1000);
process.once('SIGTERM',()=>kill('SIGTERM'));
child.once('exit',(code,signal)=>{clearTimeout(timer);kill('SIGKILL');sweep();saveNetem('netem-final.txt');process.exitCode=code??(signal?143:1);});
