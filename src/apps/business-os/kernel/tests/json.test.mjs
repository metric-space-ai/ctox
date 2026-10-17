import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from './legacy-kernel.mjs';
import {cloneValue,mergePatch} from '../values.mjs';
import {normalizeJson} from '../app-api.mjs';

test('JSON rejects cycles and classes with invalid path; __proto__ remains data',()=>{
 const cycle={id:'x'};cycle.self=cycle;assert.throws(()=>normalizeJson(cycle,''),e=>e.code==='invalid'&&e.path==='self');
 assert.throws(()=>normalizeJson(new Map(),''),e=>e.code==='invalid');
 const input=JSON.parse('{"id":"x","__proto__":{"polluted":true}}');
 const clean=normalizeJson(input,'');assert.equal(Object.getPrototypeOf(clean),Object.prototype);assert.equal(Object.hasOwn(clean,'__proto__'),true);assert.equal({}.polluted,undefined);
 const patched=mergePatch({},input);assert.equal(Object.getPrototypeOf(patched),Object.prototype);assert.equal(Object.hasOwn(patched,'__proto__'),true);
});
test('structured-clone-compatible snapshots preserve cycles and typed data',()=>{
 const snap={draft:'kept',map:new Map([['x',1]])};snap.self=snap;
 const copy=cloneValue(snap);assert.equal(copy.self,copy);assert.equal(copy.map.get('x'),1);
});
test('invalid cyclic write never reaches the outbox and result never rejects',async()=>{
 const k=createStubKernel({instanceId:`cyclic-${process.pid}`,userId:'u',storage:'memory',multiTab:false});await k.ready;
 const doc={id:'x'};doc.self=doc;const h=k.appApi('notes','1').put('notes',doc);
 await assert.rejects(h.local,e=>e.code==='invalid');assert.equal((await h.result).reason,'invalid');assert.equal((await k.sim.outbox()).length,0);await k.close();
});

test('snapshot accounting includes binary content and cycles rather than JSON-only size',async()=>{
 const {snapshotBytes}=await import('../snapshot.mjs');
 const cycle={data:new Uint8Array(1024*1024)};cycle.self=cycle;
 assert.ok(snapshotBytes(cycle)>1024*1024);
 assert.ok(snapshotBytes({text:'ä'.repeat(600000)})>1024*1024);
});

test('cyclic merge patch rejects as invalid without corrupting saved data',async()=>{
 const k=createStubKernel({instanceId:`patch-cycle-${process.pid}`,userId:'u',storage:'memory',multiTab:false});await k.ready;
 try {
  const api=k.appApi('notes','1');await api.put('notes',{id:'a',title:'keep'}).result;
  const patch={title:'no'};patch.self=patch;const h=api.patch('notes','a',patch);
  await assert.rejects(h.local,e=>e.code==='invalid' && e.path==='self');assert.equal((await h.result).reason,'invalid');
  assert.equal(api.get('notes','a').get().title,'keep');assert.equal((await k.sim.outbox()).length,0);
 } finally {await k.close();}
});
