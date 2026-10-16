// Chromium proof of the SAME command observer and contract consumed by the host harness.
import {createServer} from 'node:http';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {resolve,sep,extname} from 'node:path';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),{chromium}=require(process.env.PLAYWRIGHT_MODULE);
const root=resolve('src/apps/business-os'),out=resolve(process.env.KERNEL_EVIDENCE_DIR,'s9-browser');await mkdir(out,{recursive:true});
const server=createServer(async(req,res)=>{try{
 if(req.url==='/'){res.setHeader('Content-Type','text/html');res.end('<!doctype html><title>V4 kernel delivery contract</title><pre>running</pre>');return;}
 const file=resolve(root,'.'+new URL(req.url,'http://local').pathname);if(!file.startsWith(root+sep))throw Error('path');
 res.setHeader('Content-Type',extname(file)==='.mjs'||extname(file)==='.js'?'text/javascript':'application/json');res.end(await readFile(file));
}catch{res.statusCode=404;res.end();}});await new Promise(r=>server.listen(0,'127.0.0.1',r));
let browser;const errors=[];
try{
 browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_BIN??'/usr/bin/google-chrome'});
 const context=await browser.newContext(),page=await context.newPage();page.on('pageerror',e=>errors.push(String(e)));
 await page.goto('http://127.0.0.1:'+server.address().port);
 const result=await page.evaluate(async()=>{
  const {createStubKernel}=await import('/kernel/kernel.mjs');const {exerciseKernelCommands}=await import('/kernel/testkit/command-contracts.mjs');
  const kernel=createStubKernel({instanceId:'s9-browser',userId:'fixture',latencyMs:2,multiTab:false});await kernel.ready;
  try{
   kernel.sim.onCommand('probe.ok',async()=>({ok:true}));
   const measurement=await exerciseKernelCommands(kernel,{type:'probe.ok',samples:3,timeoutMs:2000});
   if(measurement.storage!=='indexeddb')throw Error('real IndexedDB required');
   const query=kernel.appApi('probe','1').query('business_commands',{});if(query.get().length!==3)throw Error('terminal records missing');
   document.querySelector('pre').textContent='PASS: V4 kernel / real IndexedDB / durable commands / measured stages';
   return measurement;
  }finally{await kernel.logout({deleteData:true});}
 });
 if(errors.length)throw Error(errors.join('\n'));
 await page.screenshot({path:resolve(out,'delivery-timing.png')});await context.close();
 await writeFile(resolve(out,'result.json'),JSON.stringify({pass:true,scope:'local V4 kernel and real IDB; simulated core handler, no native/tenant acceptance',...result},null,2));
 console.log('S9_BROWSER '+JSON.stringify({pass:true,storage:result.storage,samples:result.samples.map(s=>({id:s.id,stagesMs:s.stagesMs}))}));
}finally{await browser?.close();await new Promise(r=>server.close(r));}
