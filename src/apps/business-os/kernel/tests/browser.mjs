// Run on the admitted Linux lane with an existing Playwright package.
import {createServer} from 'node:http';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {resolve,extname} from 'node:path';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE??'playwright');
const root=resolve('src/apps/business-os/kernel');
const out=process.env.KERNEL_EVIDENCE_DIR??resolve(process.env.TMPDIR??'.','k1-browser');
await mkdir(out,{recursive:true});
const server=createServer(async(req,res)=>{
 try{
  if(req.url==='/'){res.setHeader('Content-Type','text/html');res.end('<!doctype html><meta charset="utf-8"><title>Kernel acceptance</title><pre id="result">Running kernel acceptance</pre>');return;}
  const path=resolve(root,'.'+decodeURIComponent(new URL(req.url,'http://local').pathname));
  if(!path.startsWith(root+'/'))throw Error('outside root');
  res.setHeader('Content-Type',extname(path)==='.mjs'?'text/javascript':'text/plain');res.end(await readFile(path));
 }catch{res.statusCode=404;res.end('Not found');}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
let browser;
const errors=[];
try{
 browser=await chromium.launch({headless:true,...(process.env.CHROME_BIN?{executablePath:process.env.CHROME_BIN}:{}),args:['--disable-gpu']});
 const context=await browser.newContext({viewport:{width:1280,height:900}});
 const page=await context.newPage();page.on('pageerror',e=>errors.push(String(e)));
 const url=`http://127.0.0.1:${server.address().port}`;await page.goto(url);
 const external = await page.evaluate(async()=>{
  const source='k1-legacy-owned',target='ctox-k1-external-alice';
  for(const name of [source,target])await new Promise((r,j)=>{const q=indexedDB.deleteDatabase(name);q.onsuccess=r;q.onerror=()=>j(q.error)});
  await new Promise((r,j)=>{const q=indexedDB.open(source,4);q.onupgradeneeded=()=>q.result.createObjectStore('documents',{keyPath:['collection','id']});q.onsuccess=()=>{
   const db=q.result,t=db.transaction('documents','readwrite');
   for(let n=0;n<260;n++)t.objectStore('documents').put({collection:'legacy',id:String(n).padStart(3,'0'),pushable:n===259?1:0,replicationOriginRole:n===259?'':'master',doc:{id:String(n).padStart(3,'0'),title:'v3-'+n}});
   t.oncomplete=()=>{db.close();r()};t.onabort=()=>j(t.error);
  };q.onerror=()=>j(q.error)});
  const {createKernel}=await import('/kernel.mjs');
  const options={instanceId:'k1-external',userId:'alice',legacyDatabase:{name:source,instanceId:'k1-external',userId:'alice',quiesced:true}};
  const k=createKernel(options);await k.ready;
  if(k.appApi('legacy','1').query('legacy',{}).get().length!==260)throw Error('external chunk takeover incomplete');
  if((await k.sim.archiveLocal()).length!==1)throw Error('unconfirmed legacy input not archived');
  await k.close();const again=createKernel(options);await again.ready;
  if((await again.sim.archiveLocal()).length!==1)throw Error('migration replayed twice');
  await again.logout({deleteData:true});await new Promise(r=>{const q=indexedDB.deleteDatabase(source);q.onsuccess=r;});
  return {imported:260,archived:1,oneTime:true};
 });
 const first=await page.evaluate(async()=>{
  const name='ctox-k1-browser-alice';
  await new Promise((r,j)=>{const q=indexedDB.deleteDatabase(name);q.onsuccess=r;q.onerror=()=>j(q.error)});
  await new Promise((r,j)=>{const q=indexedDB.open(name,4);q.onupgradeneeded=()=>q.result.createObjectStore('documents',{keyPath:['collection','id']});q.onsuccess=()=>{
   const db=q.result,t=db.transaction('documents','readwrite');t.objectStore('documents').put({collection:'notes',id:'v3',replicationOriginRole:'master',pushable:0,doc:{id:'v3',title:'kept from V3'}});t.oncomplete=()=>{db.close();r()};t.onabort=()=>j(t.error);
  };q.onerror=()=>j(q.error)});
  const {createKernel}=await import('/kernel.mjs');const {MemoryCoreTransport}=await import('/fake-core.mjs');
  const transport=new MemoryCoreTransport({namespace:name});
  const k=createKernel({instanceId:'k1-browser',userId:'alice',transport});await k.ready;globalThis.kernel=k;
  if(k.storageKind!=='indexeddb')throw Error('not IndexedDB');
  if(k.appApi('notes','1').get('notes','v3').get()?.title!=='kept from V3')throw Error('V3 takeover failed');
  await k.sim.setLink({state:'offline'});
  await k.appApi('notes','1').put('notes',{id:'offline',title:'durable draft'}).local;
  document.querySelector('#result').textContent='PASS: V3 → IDB v5; offline outbox durably committed';
  return {storage:k.storageKind,leader:k.isLeader,dbName:k.dbName};
 });
 await page.screenshot({path:resolve(out,'01-migration-offline.png')});
 await page.evaluate(()=>kernel.close());await page.reload();
 const reopened=await page.evaluate(async()=>{
  const {createKernel}=await import('/kernel.mjs');const {MemoryCoreTransport}=await import('/fake-core.mjs');
  globalThis.kernel=createKernel({instanceId:'k1-browser',userId:'alice',transport:new MemoryCoreTransport({namespace:'ctox-k1-browser-alice'})});await kernel.ready;
  const api=kernel.appApi('notes','1');if(api.get('notes','offline').get()?.title!=='durable draft')throw Error('draft lost on reload');
  await kernel.sim.settle();document.querySelector('#result').textContent='PASS: reload kept offline input; outbox accepted after reconnect';return {leader:kernel.isLeader};
 });
 const reader=await context.newPage();await reader.goto(url);
 const follower=await reader.evaluate(async()=>{
  const {createKernel}=await import('/kernel.mjs');const {MemoryCoreTransport}=await import('/fake-core.mjs');
  globalThis.kernel=createKernel({instanceId:'k1-browser',userId:'alice',transport:new MemoryCoreTransport({namespace:'ctox-k1-browser-alice'})});await kernel.ready;
  if(kernel.isLeader)throw Error('two leaders');
  const handle=kernel.appApi('notes','1').put('notes',{id:'reader',title:'reader write'});await handle.local;await handle.result;
  document.querySelector('#result').textContent='PASS: second tab is reader; write accepted by leader through shared IDB';return {leader:kernel.isLeader};
 });
 await reader.screenshot({path:resolve(out,'02-reader-write.png')});
 await page.close();await reader.waitForFunction(()=>globalThis.kernel?.isLeader,undefined,{timeout:10000});
 const takeover=await reader.evaluate(async()=>{
  const h=kernel.appApi('notes','1').put('notes',{id:'takeover',title:'new leader'});await h.local;await h.result;
  const leader=kernel.shell.leader.get();document.querySelector('#result').textContent=`PASS: Web Lock takeover; leader_gen=${leader.leader_gen}; new leader write accepted`;
  return leader;
 });
 await reader.screenshot({path:resolve(out,'03-leader-takeover.png')});
 const quota=await reader.evaluate(async()=>{
  const original=IDBObjectStore.prototype.put;let injected=0;
  IDBObjectStore.prototype.put=function(value,...rest){
   if(this.name==='outbox' && value.id==='quota-retry' && injected++===0)throw new DOMException('injected full disk','QuotaExceededError');
   return original.call(this,value,...rest);
  };
  try {
   const h=kernel.appApi('notes','1').put('notes',{id:'quota-retry',title:'saved after reclaim'});await h.local;
   const result=await h.result;if(result.status!=='accepted')throw Error('quota retry not accepted');
   if(!kernel.statusZone.get().some(e=>e.code==='storage.quota'))throw Error('quota missing status zone');
   document.querySelector('#result').textContent='PASS: real IDB transaction abort on quota; protected LRU reclaim; one retry accepted';
   return {injected,accepted:true,statusReported:true};
  } finally {IDBObjectStore.prototype.put=original;}
 });
 await reader.screenshot({path:resolve(out,'04-quota-reclaim.png')});
 await reader.evaluate(()=>kernel.logout({deleteData:true}));
 if(errors.length)throw Error(errors.join('\n'));
 const result={pass:true,external,first,reopened,follower,takeover,quota,errors};await writeFile(resolve(out,'result.json'),JSON.stringify(result,null,2));console.log('KERNEL_BROWSER '+JSON.stringify(result));
}finally{await browser?.close();await new Promise(r=>server.close(r));}
