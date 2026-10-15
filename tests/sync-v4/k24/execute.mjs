// Adapter only: delivered run.mjs/app files remain byte-identical to ORIGIN.json.
import fs from 'node:fs';
import {resolve,join,dirname} from 'node:path';
import {spawn,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const o={};for(let i=2;i<process.argv.length;i+=2)o[process.argv[i].slice(2)]=process.argv[i+1];
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const target=o.target??'bind';
if(target!=='bind'){
 if(!target.startsWith('url='))throw Error('Expected bind or url=');
 const u=new URL(target.slice(4));
 if(!['127.0.0.1','[::1]','localhost'].includes(u.hostname)||o['isolated-url']!=='yes')throw Error('URL execution requires an isolated loopback fixture; WELSCH requires controller coordination');
}
const here=dirname(new URL(import.meta.url).pathname),origin=JSON.parse(fs.readFileSync(join(here,'ORIGIN.json')));
const hash=b=>createHash('sha256').update(b).digest('hex');
for(const f of origin.files)if(hash(fs.readFileSync(join(here,f.path)))!==f.sha256)throw Error('Delivered K24 changed: '+f.path);
const out=resolve(o.output),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts');
if(!out.startsWith(allowed+'/'))throw Error('Private lane output required');
fs.mkdirSync(out,{recursive:false,mode:0o700});
const result={id:'K24',target,originCommit:origin.commit,startedAt:new Date().toISOString(),status:'not_run',pass:false,acceptanceComplete:false,criteria:null};
const save=()=>fs.writeFileSync(join(out,'result.json'),JSON.stringify(result,null,2));save();
try{
 if(!o.source)throw Error('Missing --source verified Shell bind snapshot');
 const source=resolve(o.source),manifest=JSON.parse(fs.readFileSync(join(source,'snapshot.json')));
 if(manifest.shellRevision!==origin.commit)throw Error('Shell snapshot differs from delivered K24 origin');
 for(const f of manifest.files){const p=resolve(source,f.path);if(!p.startsWith(source+'/')||!fs.lstatSync(p).isFile()||hash(fs.readFileSync(p))!==f.sha256)throw Error('Snapshot hash mismatch: '+f.path);}
 const runtime=join(out,'snapshot');fs.cpSync(source,runtime,{recursive:true});
 const k24=join(runtime,'src/apps/business-os/v4/k24');fs.mkdirSync(k24,{recursive:true});
 for(const f of origin.files){const p=join(k24,f.path);fs.mkdirSync(dirname(p),{recursive:true});fs.copyFileSync(join(here,f.path),p);}
 // Upstream fingerprints git before/after. This custody commit describes the
 // imported fixture tree; it is never represented as the product source SHA.
 const git=a=>execFileSync('git',a,{cwd:runtime,stdio:'pipe'});
 git(['init','--quiet']);git(['add','.']);git(['-c','user.name=CTOX synthetic fixture','-c','user.email=fixture@invalid','commit','--quiet','-m','Imported exact Shell/K-1 snapshot; see snapshot.json']);
 result.snapshot={shellRevision:manifest.shellRevision,kernelRevision:manifest.kernelRevision,manifestSha256:hash(fs.readFileSync(join(source,'snapshot.json'))),custodyCommit:git(['rev-parse','HEAD']).toString().trim(),nativePeer:target!=='bind'};
 const requireShim=join(out,'playwright.cjs');
 fs.writeFileSync(requireShim,`const real=require(${JSON.stringify(o.playwright)});module.exports={...real,chromium:{...real.chromium,launch:o=>real.chromium.launch({...o,executablePath:${JSON.stringify(o.chrome??'/opt/google/chrome/chrome')}})}};\n`);
 const log=fs.openSync(join(out,'tool.log'),'w',0o600);
 const child=spawn(process.execPath,[join(k24,'run.mjs'),'--target',target],{cwd:runtime,env:{...process.env,PLAYWRIGHT_MODULE:requireShim,K24_GUARD_BOS:join(runtime,'src/apps/business-os')},detached:true,stdio:['ignore',log,log]});fs.closeSync(log);
 result.process={owner:'01a087a0-169a',pid:child.pid,pgid:child.pid,stop:'terminal or 12 minutes'};save();
 const kill=s=>{try{process.kill(-child.pid,s);}catch(e){if(e.code!=='ESRCH')throw e;}};
 let escalation;const timer=setTimeout(()=>{kill('SIGTERM');escalation=setTimeout(()=>kill('SIGKILL'),5000);},12*60*1000);
 const cancel=()=>kill('SIGTERM');process.once('SIGTERM',cancel);
 const exit=await new Promise((r,j)=>{child.once('exit',(code,signal)=>r({code,signal}));child.once('error',j);});
 clearTimeout(timer);clearTimeout(escalation);process.removeListener('SIGTERM',cancel);kill('SIGKILL');
 result.exit=exit;
 const file=join(k24,'out/results.json');
 if(fs.existsSync(file)){
  const raw=JSON.parse(fs.readFileSync(file));result.criteria=raw.criteria;result.subs=raw.subs;result.measured=raw.measures;result.fatal=raw.fatal??null;
  result.status=raw.fatal?'failed':'measured';result.pass=!raw.fatal&&exit.code===0&&Object.values(raw.criteria).every(c=>c.status==='PASS');
  result.acceptanceComplete=target!=='bind'&&result.pass;
  result.reason=target==='bind'?'Bind uses MemoryCoreTransport; native criteria remain OPEN/FAIL exactly as upstream reports':null;
 }else result.reason='Delivered tool terminated before producing results';
 try{process.kill(-child.pid,0);result.cleanup='owned group still present';result.pass=false;}catch(e){if(e.code!=='ESRCH')throw e;result.cleanup='owned groups absent';}
}catch(e){result.reason=e.message;}
finally{result.finishedAt=new Date().toISOString();result.terminal=true;save();}
console.log(JSON.stringify({output:out,status:result.status,criteria:result.criteria,reason:result.reason}));
process.exitCode=result.pass&&result.acceptanceComplete?0:result.status==='not_run'?2:1;
