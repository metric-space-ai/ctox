import assert from 'node:assert/strict';
import http from 'node:http';
import {readFileSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const testDir=dirname(fileURLToPath(import.meta.url));
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE_PATH ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH,'index.mjs')).href : '../../node_modules/playwright/index.mjs');
const bundle=readFileSync(resolve(testDir,'../dist/ctox-rxdb-js.mjs'));
const server=http.createServer((request,response)=>{
  response.writeHead(200,{'content-type':request.url==='/bundle.mjs'?'text/javascript':'text/html'});
  response.end(request.url==='/bundle.mjs'?bundle:'<!doctype html><title>projection cache guard</title>');
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
let browser;
try {
  browser=await chromium.launch({headless:true});
  const context=await browser.newContext();
  const pages=await Promise.all([context.newPage(),context.newPage()]);
  const origin=`http://127.0.0.1:${server.address().port}`;
  await Promise.all(pages.map(page=>page.goto(origin)));
  await Promise.all(pages.map(page=>page.evaluate(async()=>{
    const {createIndexedDbMetaBackend,QueryMetaStorage}=await import('/bundle.mjs');
    globalThis.backend=createIndexedDbMetaBackend({databaseName:'projection-persistence'});
    globalThis.sidecar=new QueryMetaStorage(backend,{databaseName:'projection-persistence'});
  })));
  await pages[0].evaluate(async()=>{
    await sidecar.putProjectedQueryRows('persisted',[{id:'one',payload:{weitere_kampagnen:['x','y']}}]);
    await sidecar.upsertQueryWindow({collection:'leads',queryFingerprint:'fingerprint',offset:0,limit:200,documentIds:['one'],complete:true,permissionDigest:'authorized-role',projection:['id','payload.weitere_kampagnen'],projectionKey:'persisted'});
    if(backend.name!=='indexeddb'||backend.projectedCacheName!=='indexeddb')throw new Error('persistence cannot pass through a memory fallback');
    await backend.close();
  });
  const reopened=await pages[1].evaluate(async()=>({
    documents:await sidecar.getProjectedQueryRows('persisted'),
    window:await sidecar.getQueryWindow(['leads','fingerprint',0,200]),
    databases:await indexedDB.databases(),
  }));
  assert.deepEqual(reopened.documents,[{id:'one',payload:{weitere_kampagnen:['x','y']}}]);
  assert.equal(reopened.window.projectionKey,'persisted');
  assert.equal(reopened.window.permissionDigest,'authorized-role');
  assert.deepEqual(reopened.databases.map(db=>[db.name,db.version]).sort(),[['projection-persistence',2],['projection-persistence_query_projection_v1',1]]);

  // Concurrent writers on two tabs must share one transactional byte budget.
  await Promise.all(pages.map((page,tab)=>page.evaluate(async({tab})=>{
    for(let index=tab;index<70;index+=2) await sidecar.putProjectedQueryRows('window-'+index,[{id:String(index),title:'x'.repeat(500_000)}]);
  },{tab})));
  const budget=await pages[1].evaluate(async()=>{
    const db=await new Promise((resolve,reject)=>{const request=indexedDB.open('projection-persistence_query_projection_v1',1);request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);});
    const result=await new Promise((resolve,reject)=>{const tx=db.transaction(['rows','metadata'],'readonly');let budget,keys;tx.objectStore('metadata').get('@budget').onsuccess=event=>budget=event.target.result;tx.objectStore('rows').getAllKeys().onsuccess=event=>keys=event.target.result;tx.oncomplete=()=>resolve({budget,keys});tx.onerror=()=>reject(tx.error);});
    db.close(); return result;
  });
  assert.ok(budget.budget.bytes<=16*1024*1024);
  assert.ok(budget.budget.count<=64);
  assert.equal(budget.keys.length,budget.budget.count);
  assert.equal(await pages[0].evaluate(()=>sidecar.getProjectedQueryRows('persisted')),null,'byte budget must retire the oldest payload');
  await pages[0].reload();
  const afterReload=await pages[0].evaluate(async()=>{
    const {createIndexedDbMetaBackend}=await import('/bundle.mjs');
    const backend=createIndexedDbMetaBackend({databaseName:'projection-persistence'});
    const documents=await backend.getProjectedQueryRows('window-69');
    const name=backend.projectedCacheName;
    await backend.clear(); await backend.close();
    return {documents,name};
  });
  assert.equal(afterReload.name,'indexeddb');
  assert.equal(afterReload.documents[0].title.length,500_000);
  assert.equal(await pages[1].evaluate(()=>sidecar.getProjectedQueryRows('window-69')),null);
  await pages[1].evaluate(()=>backend.close());
  await context.close();
  console.log('query projection browser PASS: real IndexedDB reopen, two-tab bounded transactions, isolated payloads and clear');
} finally {
  await browser?.close();
  await new Promise(resolve=>server.close(resolve));
}
