// One admission attempt, bounded to the order's 40 minutes. Never cancel an
// admitted job or a foreign process. No retry/successor admission is created.
import {spawn,execFileSync} from 'node:child_process';
import {homedir} from 'node:os';
import {join} from 'node:path';
const argv=process.argv.slice(2);if(argv.shift()!=='--')throw Error('Expected -- canonical lane arguments');
const helper=argv.shift();if(helper!==join(homedir(),'.codex/bin/gpu-build-run.sh'))throw Error('Only the canonical lane helper is permitted');
const child=spawn(helper,argv,{stdio:['inherit','pipe','pipe'],detached:true});
let admitted=false,remote=null,partial='',expired=false;
const receive=(bytes,stream)=>{
 stream.write(bytes);partial+=bytes.toString();const lines=partial.split('\n');partial=lines.pop();
 for(const line of lines){
  const m=line.match(/started (v4-e2e-harness-[A-Za-z0-9]+) on (gpu[134]) pid (\d+); remote log (\/[^ ]+)$/);
  if(m)remote={run:m[1],host:m[2],pid:Number(m[3]),log:m[4]};
  if(line.startsWith('== host ')){admitted=true;clearTimeout(timer);}
 }
};
child.stdout.on('data',b=>receive(b,process.stdout));child.stderr.on('data',b=>receive(b,process.stderr));
const stopQueued=()=>{
 if(admitted)return;
 expired=true;
 if(!remote){console.error('Admission expired before remote identity was received; inspect canonical receipt, no blind remote kill');}
 else{
  const host={gpu3:'ts-gpu3',gpu4:'metricspace@100.120.16.93',gpu1:'ts-gpu1'}[remote.host];
  const code=`import os,pathlib,signal,json,time\npid=${remote.pid}\nlog=pathlib.Path(${JSON.stringify(remote.log)})\nrun=${JSON.stringify(remote.run)}\nbase=log.parent.parent\nmeta=(base/'run'/(run+'.meta')).read_text()\nassert '01a087a0-169a' in meta, 'owner mismatch'\ncmd=pathlib.Path('/proc/'+str(pid)+'/cmdline').read_bytes()\nassert str(base/'run'/(run+'.sh')).encode() in cmd, 'runner identity mismatch'\nassert os.getpgid(pid)==pid, 'runner group mismatch'\nos.kill(pid,signal.SIGSTOP)\ncancelled=False\ntry:\n text=log.read_text() if log.exists() else ''\n assert '== host ' not in text, 'already admitted; refusing cancellation'\n fds=list(pathlib.Path('/proc/'+str(pid)+'/fd').iterdir())\n assert not any('/slot-' in os.readlink(f) for f in fds), 'slot fd present; refusing cancellation'\n os.killpg(pid,signal.SIGTERM);cancelled=True\nfinally:\n os.kill(pid,signal.SIGCONT)\nprint(json.dumps({'run':run,'admissionExpired':cancelled,'queuedOwnerGroupTerminated':pid if cancelled else None,'noSuccessor':True}))\n`;

  try{execFileSync('ssh',['-o','BatchMode=yes','-o','ConnectTimeout=10',host,'python3 -'],{input:code,stdio:['pipe','inherit','inherit'],timeout:15000});}
  catch(e){expired=false;admitted=true;console.error('Admission cancellation was refused/failed; canonical job retained:',JSON.stringify(remote));return;}
 }
 try{process.kill(-child.pid,'SIGTERM');}catch(e){if(e.code!=='ESRCH')throw e;}
};
const timer=setTimeout(stopQueued,40*60*1000);
process.once('SIGTERM',stopQueued);
child.once('error',e=>{clearTimeout(timer);throw e;});
child.once('exit',(code,signal)=>{clearTimeout(timer);process.exitCode=expired?75:code??(signal?143:1);});
