import {createServer} from 'node:http';
import {readFile,mkdir,writeFile,mkdtemp,rm} from 'node:fs/promises';
import {resolve,extname,sep} from 'node:path';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),{chromium}=require(process.env.PLAYWRIGHT_MODULE);
const root=resolve('src/apps/business-os'),out=resolve(process.env.CARGO_TARGET_DIR,'evidence/e26/browser');await mkdir(out,{recursive:true});
const server=createServer(async(req,res)=>{try{
 if(req.url==='/'){res.setHeader('Content-Type','text/html');res.end('<!doctype html><title>E26 kernel</title><pre>Running</pre>');return;}
 const file=resolve(root,'.'+new URL(req.url,'http://local').pathname);if(!file.startsWith(root+sep))throw Error('path');res.setHeader('Content-Type',extname(file)==='.mjs'?'text/javascript':'application/json');res.end(await readFile(file));
}catch{res.statusCode=404;res.end();}});await new Promise(r=>server.listen(0,'127.0.0.1',r));
let browser,browserTmp;const errors=[];
const watchdog=setTimeout(()=>{errors.push('E26 browser deadline');void browser?.close();},45000);
try{
 browserTmp=await mkdtemp('/mnt/nvme1/build-lane/tmp/e26-');
 browser=await chromium.launch({headless:true,env:{...process.env,TMPDIR:browserTmp},executablePath:process.env.CHROME_BIN??'/usr/bin/google-chrome'});
 const context=await browser.newContext(),page=await context.newPage();page.on('pageerror',e=>errors.push(String(e)));
 const url='http://127.0.0.1:'+server.address().port;await page.goto(url);
 const second=await context.newPage();second.on('pageerror',e=>errors.push(String(e)));await second.goto(url);
 for(const tab of [page,second])await tab.evaluate(async()=>{
  const {createStubKernel}=await import('/kernel/testkit.mjs');globalThis.e26=createStubKernel({instanceId:'e26-idb',userId:'u1',latencyMs:2});await e26.ready;await e26.sim.setLink({state:'offline'});
 });
 const race=await Promise.all([page,second].map(tab=>tab.evaluate(async()=>{
  const a=e26.appApi('notes','1'),h=a.command('notes.run',{n:1},{idempotency_key:'reload'});await h.local;
  return {id:h.id,storage:e26.storageKind,db:(await indexedDB.databases()).find(d=>d.name===e26.dbName)};
 })));
 const before=race[0];if(race[1].id!==before.id)throw Error('cross-tab ID mismatch');
 await second.evaluate(async(id)=>{await e26.sim.reconcile();if((await e26.sim.outbox()).filter(e=>e.id===id).length!==1)throw Error('cross-tab duplicate');await e26.close();},before.id);await second.close();
 await page.evaluate(async()=>{await e26.close();});
 await page.reload();
 const after=await page.evaluate(async()=>{
  const {createStubKernel}=await import('/kernel/testkit.mjs');const k=createStubKernel({instanceId:'e26-idb',userId:'u1',latencyMs:2});await k.ready;await k.sim.setLink({state:'offline'});
  try{
   const a=k.appApi('notes','2'),h=a.command('notes.other',{n:9},{idempotency_key:'reload'});await h.local;
   if(a.get('business_commands',h.id).get().payload.n!==1||(await k.sim.outbox()).filter(e=>e.id===h.id).length>1)throw Error('reload replaced command');
   await k.sim.setLink({state:'online'});await k.sim.settle();
   const data=Uint8Array.from({length:140000},(_,i)=>i%251),f=await a.files.put(data);const r=await a.files.get({id:f.id,range:{start:65530,end:65545}});
   if(r.bytes.length!==15||r.bytes.some((v,i)=>v!==data[65530+i]))throw Error('byte range');
   document.querySelector('pre').textContent='PASS: E26 / real IndexedDB / cross-tab single command / reload / end-exclusive byte range';
   return {id:h.id,storage:k.storageKind,db:(await indexedDB.databases()).find(d=>d.name===k.dbName),bytes:r.bytes.length};
  }finally{await k.logout({deleteData:true});}
 });
 if(before.id!==after.id||before.storage!=='indexeddb'||after.storage!=='indexeddb'||before.db?.version!==5||after.db?.version!==5||errors.length)throw Error(JSON.stringify({before,after,errors}));
 await page.screenshot({path:resolve(out,'e26-idb.png')});await context.close();await writeFile(resolve(out,'result.json'),JSON.stringify({pass:true,before,after,scope:'real browser IDB; explicit in-memory core'},null,2));console.log('E26_BROWSER '+JSON.stringify(after));
}catch(error){await writeFile(resolve(out,'result.json'),JSON.stringify({pass:false,errors,error:String(error.stack??error)},null,2));throw error;}finally{clearTimeout(watchdog);await browser?.close();await new Promise(r=>server.close(r));if(browserTmp)await rm(browserTmp,{recursive:true,force:true});}
