// A real foreground HTTP + Chromium boot; launch-context503 is explicitly red.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {createConnection} from 'node:net';
import {createRequire} from 'node:module';
import {until,owner} from '../../sync-v4-e2e/native.mjs';
import {requireSandbox} from './upgrade.mjs';
const arg=n=>{const i=process.argv.indexOf('--'+n);if(i<0)throw Error('Missing --'+n);return process.argv[i+1];};
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN||!process.env.PLAYWRIGHT_MODULE_PATH)throw Error('Canonical lane Playwright wrapper required');
const base=arg('base'),out=resolve(arg('output')),expected=resolve(arg('expected-root'));
const allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts');
const sandbox=JSON.parse(fs.readFileSync(arg('proof')));requireSandbox(sandbox.proof);
const marker=JSON.parse(fs.readFileSync(join(sandbox.root,'.sync-v4-e2e.json')));
if(marker.owner!==owner||marker.synthetic!==true||new URL(base).origin!=='http://127.0.0.1:8765')throw Error('Only the private namespace fixture loopback may be tested');
if(!out.startsWith(allowed+'/')||!expected.startsWith(allowed+'/'))throw Error('Private artifact paths required');
const digest=x=>createHash('sha256').update(x).digest('hex');
const r={startedAtMs:Date.now(),terminal:false,pass:false,criteria:[],console:[],pageErrors:[],requestsFailed:[]};let browser;
const pendingSignaling=process.env.M7_EXPECT_SIGNALING_DEPLOY==='1';
const criterion=(id,pass,details)=>{
 const blocked=pendingSignaling&&!pass&&((id.startsWith('launch-context')&&(details.httpStatus===503||details.http503===20))||(['browser-boot','platform-command-roundtrip'].includes(id)&&r.launchSeries?.every(x=>x.samples.every(s=>s.httpStatus===503))));
 r.criteria.push({id,status:blocked?'not_run':pass?'pass':'fail',...(blocked?{reason:'signaling-deploy',observed:'blocked by expected HTTP503; never a passing launch claim'}:{}),...details});
};
const save=()=>fs.writeFileSync(join(out,'smoke.json'),JSON.stringify(r,null,2),{mode:0o600});
try{
 // Readiness observes the TCP listener, not an asset request that can spend
 // its entire deadline validating a newly activated signed slot. HTTP timings
 // and their unchanged thresholds are measured separately below.
 const readyAt=Date.now();r.phase='socket';r.socketBudgetMs=30000;
 await until(()=>new Promise(resolve=>{const socket=createConnection({host:'127.0.0.1',port:8765});let done=false;const finish=ok=>{if(done)return;done=true;socket.destroy();resolve(ok);};socket.setTimeout(1000,()=>finish(false));socket.once('connect',()=>finish(true));socket.once('error',()=>finish(false));}),30000,'private HTTP socket ready');r.socketReadyMs=Date.now()-readyAt;r.phase='launch-context';
 // Both series are retained; the fixed warm interval cannot hide cold 503s.
 r.launchSeries=[];
 for(const phase of ['cold','warm']){
  if(phase==='warm')await new Promise(resolve=>setTimeout(resolve,5000));
  const samples=[];
  for(let index=0;index<20;index++){
   const startedAtMs=Date.now();let row={index,startedAtMs,sinceServiceStartMs:startedAtMs-Number(process.env.M7_SERVICE_START_AT)};
   try{const response=await fetch(base+'/api/business-os/launch-context',{signal:AbortSignal.timeout(5000)});const body=await response.text();let value;try{value=JSON.parse(body);}catch{}
    row={...row,httpStatus:response.status,durationMs:Date.now()-startedAtMs,hasV4:!!value&&typeof value.v4==='object'&&value.v4!==null,bytes:Buffer.byteLength(body)};
   }catch(error){row.errorClass=error.name;row.durationMs=Date.now()-startedAtMs;}
   samples.push(row);
  }
  const good=samples.filter(x=>x.httpStatus===200&&x.hasV4).length;r.launchSeries.push({phase,samples,http200:good,httpFail:20-good});
  criterion('launch-context-'+phase,good===20,{calls:20,http200:good,httpFail:20-good,http503:samples.filter(x=>x.httpStatus===503).length,warmDelayMs:phase==='warm'?5000:0});save();
 }
 r.phase='http-assets';
 for(const [id,path] of [['entry','/'],['service-worker','/sw.js'],['kernel','/kernel/boot.mjs'],['launch-context','/api/business-os/launch-context']]){
  const at=Date.now();
  try{
   const response=await fetch(base+path,{signal:AbortSignal.timeout(5000)}),body=Buffer.from(await response.arrayBuffer());
   const row={path,httpStatus:response.status,durationMs:Date.now()-at,bytes:body.length};
   let pass=response.status===200;
   if(id==='entry'){row.sha256=digest(body);row.expectedSha256=digest(fs.readFileSync(expected));row.v4Entry=body.includes(Buffer.from('/business-os/v4/boot.js'));pass&&=row.sha256===row.expectedSha256&&row.v4Entry;}
   if(id==='launch-context'){
    let value;try{value=JSON.parse(body);}catch{}row.hasV4=!!value&&Object.hasOwn(value,'v4')&&value.v4!==null&&typeof value.v4==='object';pass&&=row.hasV4;
    // Never persist the launch context: it contains session/signaling grants.
    row.topLevelKeys=value&&typeof value==='object'?Object.keys(value).sort():[];
    const code=value?.code??value?.error?.code??value?.error;if(typeof code==='string'&&/^[a-z0-9_.:-]{1,128}$/.test(code))row.errorCode=code;
   }
   criterion(id,pass,row);
  }catch(error){criterion(id,false,{path,durationMs:Date.now()-at,errorClass:error.name});}
 }
 r.phase='root-p99';r.mountSeries=[];
 for(let index=0;index<20;index++){
  const at=Date.now();const response=await fetch(base+'/',{signal:AbortSignal.timeout(5000)});await response.arrayBuffer();
  r.mountSeries.push({index,httpStatus:response.status,durationMs:Date.now()-at});
 }
 const sorted=r.mountSeries.map(x=>x.durationMs).sort((a,b)=>a-b);r.mountP99Ms=sorted[Math.ceil(.99*sorted.length)-1];
 criterion('entry-p99',r.mountSeries.every(x=>x.httpStatus===200)&&r.mountP99Ms<500,{calls:20,p99Ms:r.mountP99Ms,thresholdMs:500});
 save();
 r.phase='browser';const pw=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE_PATH);
 browser=await pw.chromium.launch({headless:true,chromiumSandbox:false,timeout:10000});
 const context=await browser.newContext();await context.addInitScript(()=>{globalThis.__CTOX_V4_BOOT_MANUAL__=true;});const page=await context.newPage();
 page.on('console',message=>{if(message.type()==='error'){const text=message.text();r.console.push({type:'error',sha256:digest(text),v4BootError:text.includes('[ctox v4 boot]')});}});
 page.on('pageerror',error=>r.pageErrors.push({name:error.name,sha256:digest(error.message)}));
 page.on('requestfailed',request=>r.requestsFailed.push({path:new URL(request.url()).pathname}));
 const at=Date.now();let frame=false,navigationStatus=null,errorClass;
 try{const response=await page.goto(base,{waitUntil:'domcontentloaded',timeout:10000});navigationStatus=response?.status();await page.evaluate(async()=>{const {boot}=await import('/business-os/v4/boot.js');globalThis.__m7Boot=await Promise.race([boot(),new Promise((_,reject)=>setTimeout(()=>reject(Error('private boot deadline')),25000))]);});await page.locator('[data-v4-frame]').waitFor({state:'attached',timeout:25000});frame=true;}catch(error){errorClass=error.name;}
 const visible=frame?await page.locator('[data-v4-frame]').first().evaluate(el=>({shellVersion:el.dataset.shellVer,visible:!!el.getBoundingClientRect().width})):null;
 criterion('browser-boot',frame&&navigationStatus===200&&!r.console.some(x=>x.v4BootError),{durationMs:Date.now()-at,navigationStatus,frame,visible,errorClass});
 if(frame){
  const command=await page.evaluate(async()=>{
   const begin=performance.now(),h=globalThis.__m7Boot.kernel.appApi('ctox-platform','1').command('ctox.workjet.device.v1',{action:'binding.list'});
   await h.local;const localMs=performance.now()-begin;await h.result;const acceptedMs=performance.now()-begin;
   await new Promise((resolve,reject)=>{let off=()=>{};const timer=setTimeout(()=>{off();reject(Error('native command completion deadline'));},15000);off=h.status.subscribe(s=>{if(['done','failed','rejected','cancelled'].includes(s.status)){clearTimeout(timer);queueMicrotask(()=>{off();resolve();});}});});
   const status=h.status.get();return {commandId:h.id,type:'ctox.workjet.device.v1',terminalStatus:status.status,localMs,acceptedMs,terminalMs:performance.now()-begin};
  }).catch(error=>({terminalStatus:'failed',errorClass:error.name}));
  criterion('platform-command-roundtrip',command.terminalStatus==='done',command);
 }else criterion('platform-command-roundtrip',false,{observed:'Browser did not boot; no command was submitted',submitted:false});
 r.visualEvidence='whitelisted frame/visibility metadata only; no session or grant-bearing screenshot';await context.close();
 r.pass=r.criteria.every(x=>x.status==='pass');
}catch(error){r.errorClass=error.name;r.errorSha256=digest(String(error.message));criterion('runner-completed',false,{phase:r.phase||'setup',errorClass:error.name});}
finally{if(browser)await browser.close();r.terminal=true;r.durationMs=Date.now()-r.startedAtMs;save();}
console.log(JSON.stringify({pass:r.pass,criteria:r.criteria.map(x=>({id:x.id,status:x.status,httpStatus:x.httpStatus,hasV4:x.hasV4})),durationMs:r.durationMs}));process.exitCode=r.pass?0:2;
