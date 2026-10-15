import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createLoader,artifactUrl} from '../loader.mjs';
import {MemoryCoreTransport} from '../fake-core.mjs';

test('loader follows shell activation, persists snapshot and falls back on failed shell start',async()=>{
 const calls=[],transport=new MemoryCoreTransport({namespace:`loader-${process.pid}`});
 const modules={
  1:{startShell(_k,o){calls.push(['start1',o.restored])},stopShell(){calls.push(['stop1']);return {draft:'kept'}}},
  2:{startShell(_k,o){calls.push(['start2',o.restored])},stopShell(){return {draft:'second'}}},
  3:{startShell(){throw Error('bad shell')},stopShell(){}}
 };
 const loader=await createLoader({instanceId:`loader-${process.pid}`,userId:'u',storage:'memory',multiTab:false,serviceWorker:false,transport,resolveShell:ver=>modules[ver]});
 for(const version of ['1','2','3']){
  await loader.kernel.sim.publishAppVersion('shell',version,{module:{mount(){return {unmount(){}}}}});await loader.settle();
 }
 assert.equal(loader.shellVersion,'2');assert.deepEqual(calls.find(x=>x[0]==='start2')[1],{draft:'kept'});
 assert.ok(loader.kernel.statusZone.get().some(x=>x.code==='shell.swap_failed'));
 assert.equal((await loader.kernel.sim.windowState()).find(x=>x.key==='shell').snapshot.draft,'second');
 await loader.close();
});
test('artifact URLs are versioned and reject path traversal',()=>{
 assert.equal(artifactUrl('my app','2','shared/helper.js'),'/~a/my%20app@2/shared/helper.js');
 for(const path of ['../secret','/absolute','x/../y','x?token','x\\y'])assert.throws(()=>artifactUrl('a','1',path),e=>e.code==='invalid');
});

test('module import handles a large UTF-8 entry without an argument stack overflow',async()=>{
 const {createStubKernel}=await import('../kernel.mjs');
 const k=createStubKernel({instanceId:`large-module-${process.pid}`,userId:'u',storage:'memory',multiTab:false});await k.ready;
 try {
  const code='/*'+'ä'.repeat(300000)+'*/ export function mount(root){root.loaded=true;return {unmount(){}}}';
  await k.sim.publishAppVersion('large','1',{files:{'index.js':code}});
  const root={};const mounted=await k.shell.mount(root,'large');assert.equal(root.loaded,true);await mounted.unmount();
 } finally {await k.close();}
});
test('a stalled old app snapshot and unmount cannot hold hot activation forever',async()=>{
 const {createStubKernel}=await import('../kernel.mjs');
 const k=createStubKernel({instanceId:`hung-old-${process.pid}`,userId:'u',storage:'memory',multiTab:false,remountTimeoutMs:25});await k.ready;
 try {
  await k.sim.publishAppVersion('notes','1',{module:{mount(){return {unmount:()=>new Promise(()=>{}),snapshot:()=>new Promise(()=>{})}}}});
  const m=await k.shell.mount({},'notes');
  await k.sim.publishAppVersion('notes','2',{module:{mount(){return {unmount(){}}}}});
  assert.equal(m.ver,'2');assert.ok(k.statusZone.get().some(e=>e.code==='app.snapshot_failed'));await m.unmount();
 } finally {await k.close();}
});
