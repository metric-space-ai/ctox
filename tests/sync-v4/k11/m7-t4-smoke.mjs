// A real foreground HTTP + Chromium boot; launch-context503 is explicitly red.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
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
const criterion=(id,pass,details)=>r.criteria.push({id,status:pass?'pass':'fail',...details});
const save=()=>fs.writeFileSync(join(out,'smoke.json'),JSON.stringify(r,null,2),{mode:0o600});
try{
 const readyAt=Date.now();await until(async()=>{try{await fetch(base,{method:'HEAD',signal:AbortSignal.timeout(1000)});return true;}catch{return false;}},8000,'private HTTP socket ready');r.socketReadyMs=Date.now()-readyAt;
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
 save();
 const pw=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE_PATH);
 browser=await pw.chromium.launch({headless:true,chromiumSandbox:false,timeout:10000});
 const context=await browser.newContext(),page=await context.newPage();
 page.on('console',message=>{if(message.type()==='error'){const text=message.text();r.console.push({type:'error',sha256:digest(text),v4BootError:text.includes('[ctox v4 boot]')});}});
 page.on('pageerror',error=>r.pageErrors.push({name:error.name,sha256:digest(error.message)}));
 page.on('requestfailed',request=>r.requestsFailed.push({path:new URL(request.url()).pathname}));
 const at=Date.now();let frame=false,navigationStatus=null,errorClass;
 try{const response=await page.goto(base,{waitUntil:'domcontentloaded',timeout:10000});navigationStatus=response?.status();await page.locator('[data-v4-frame]').waitFor({state:'attached',timeout:25000});frame=true;}catch(error){errorClass=error.name;}
 const visible=frame?await page.locator('[data-v4-frame]').first().evaluate(el=>({shellVersion:el.dataset.shellVer,visible:!!el.getBoundingClientRect().width})):null;
 criterion('browser-boot',frame&&navigationStatus===200&&!r.console.some(x=>x.v4BootError),{durationMs:Date.now()-at,navigationStatus,frame,visible,errorClass});
 r.visualEvidence='whitelisted frame/visibility metadata only; no session or grant-bearing screenshot';await context.close();
 r.pass=r.criteria.every(x=>x.status==='pass');
}catch(error){r.errorClass=error.name;r.errorSha256=digest(String(error.message));}
finally{if(browser)await browser.close();r.terminal=true;r.durationMs=Date.now()-r.startedAtMs;save();}
console.log(JSON.stringify({pass:r.pass,criteria:r.criteria.map(x=>({id:x.id,status:x.status,httpStatus:x.httpStatus,hasV4:x.hasV4})),durationMs:r.durationMs}));process.exitCode=r.pass?0:2;
