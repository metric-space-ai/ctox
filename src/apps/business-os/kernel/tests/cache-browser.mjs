// Real Chromium/IDB: upgrade accounting, reopen, rollback, pressure and pinned input.
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createRequire} from 'node:module';
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE??'playwright');
const root=resolve('src/apps/business-os/kernel');
const server=createServer(async(req,res)=>{try{
 if(req.url==='/'){res.setHeader('Content-Type','text/html');res.end('<title>Cache accounting contract</title>');return;}
 const file=resolve(root,'.'+new URL(req.url,'http://local').pathname);if(!file.startsWith(root+'/'))throw Error('path');
 res.setHeader('Content-Type','text/javascript');res.end(await readFile(file));
}catch{res.statusCode=404;res.end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));let browser;
try{
 browser=await chromium.launch({headless:true});const page=await browser.newPage();await page.goto(`http://127.0.0.1:${server.address().port}`);
 const result=await page.evaluate(async()=>{
  const {IdbBackend,STORES}=await import('/store-idb-v5.mjs');
  const {CACHE_USAGE_KEY,cacheBytes}=await import('/cache-accounting.mjs');const {evictCollection,reclaimCache}=await import('/cache.mjs');
  const name='cache-before-accounting';
  const rows=Array.from({length:100},(_,n)=>({collection:'cold',id:String(n).padStart(3,'0'),data:{id:String(n),value:'ä'.repeat(1000)}}));
  const pinned={collection:'pending',id:'p',data:{value:'my local input'},legacy_unconfirmed:true};
  await new Promise((resolve,reject)=>{const q=indexedDB.open(name,5);q.onupgradeneeded=()=>{for(const [s,keyPath] of Object.entries(STORES)){const store=q.result.createObjectStore(s,{keyPath});if(s==='outbox')store.createIndex('by_doc',['collection','id']);}};
   q.onerror=()=>reject(q.error);q.onsuccess=()=>{const db=q.result,t=db.transaction(['docs','blobs','outbox'],'readwrite');for(const row of [...rows,pinned])t.objectStore('docs').put(row);t.objectStore('blobs').put({ref:'blob',size:50,pinned:true});t.oncomplete=()=>{db.close();resolve();};t.onabort=()=>reject(t.error);};});
  let db=await IdbBackend.open(name);const expected=[...rows,pinned].reduce((n,r)=>n+cacheBytes(r),50);
  if((await db.get('meta',CACHE_USAGE_KEY)).value!==expected)throw Error('existing V5 accounting mismatch');
  await db.close();db=await IdbBackend.open(name);if((await db.get('meta',CACHE_USAGE_KEY)).value!==expected)throw Error('reopen counted twice');
  try{await db.tx(async t=>{await t.del('docs',['cold','000']);throw Error('rollback');});}catch(e){if(e.message!=='rollback')throw e;}
  if((await db.get('meta',CACHE_USAGE_KEY)).value!==expected)throw Error('aborted accounting leaked');
  let turns=0;const timer=setInterval(()=>turns++,0);const result=await reclaimCache(db,async()=>{},new Map([['cold',0]]),new Set(),1);clearInterval(timer);
  if(!turns||result.records.length!==100||result.within_budget)throw Error('bounded IDB eviction/pinned budget contract');
  if((await db.get('docs',['pending','p'])).data.value!=='my local input')throw Error('input discarded');
  if((await db.get('meta',CACHE_USAGE_KEY)).value!==cacheBytes(pinned)+50)throw Error('post-eviction accounting mismatch');
  await db.close();return {existingV5:true,reopen:true,rollback:true,yieldedTurns:turns,retainedInput:true,cacheRows:result.records.length};
 });
 console.log('CACHE_IDB '+JSON.stringify(result));
}finally{await browser?.close();await new Promise(r=>server.close(r));}
