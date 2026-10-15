import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../kernel.mjs';
import {MemoryBackend} from '../store-idb-v5.mjs';
import {evictCollection,reclaimCache} from '../cache.mjs';
import {checkpointKey} from '../transport.mjs';

test('eviction invalidates the cursor and preserves unrelated durable stores',async()=>{
 const db=new MemoryBackend('cache');const scope={collection:'notes',scope_hash:'x',epoch:'1',gen:1};
 await db.tx(async t=>{
  await t.put('docs',{collection:'notes',id:'a',data:{id:'a'},base:{seq:'8'}});
  await t.put('meta',{key:checkpointKey(scope),...scope,cursor_to:'8',complete:true});
  await t.put('archive_local',{ref:'rejected',body:'keep'});await t.put('outbox',{op_id:'elsewhere',collection:'other'});
 });
 const rows=await evictCollection(db,'notes',async()=>{});
 assert.equal(rows[0].cache_evicted,true);assert.equal((await db.get('meta',checkpointKey(scope))).complete,false);
 assert.equal((await db.get('meta',checkpointKey(scope))).cursor_to,'0');assert.equal((await db.all('archive_local')).length,1);assert.equal((await db.all('outbox')).length,1);
});
test('eviction with pending writes rolls back without deleting either document or cursor',async()=>{
 const db=new MemoryBackend('pinned');
 await db.tx(async t=>{await t.put('docs',{collection:'notes',id:'a',data:{id:'a'}});await t.put('outbox',{op_id:'p',collection:'notes',id:'a'});});
 await assert.rejects(evictCollection(db,'notes',async()=>{}),e=>e.code==='quota');
 assert.equal((await db.get('docs',['notes','a'])).data.id,'a');
});
test('compatible schema defaults are supplied on read without rewriting documents',async()=>{
 const k=createStubKernel({instanceId:`defaults-${process.pid}`,userId:'u',storage:'memory',multiTab:false});await k.ready;
 await k.sim.publishAppVersion('notes','1',{module:{mount(){return{unmount(){}}}},collections:{notes:{schema:{type:'object',properties:{done:{type:'boolean',default:false}}}}}});
 await k.sim.push('notes',[{id:'a'}]);
 assert.equal(k.appApi('notes','1').get('notes','a').get().done,false);assert.equal((await k.sim.inspect('notes','a')).data.done,undefined);await k.close();
});

test('eviction actually releases base data and normal page replay repopulates it',async()=>{
 const k=createStubKernel({instanceId:`cache-replay-${process.pid}`,userId:'u',storage:'memory',multiTab:false});await k.ready;
 try {
  await k.sim.push('notes',{id:'a',title:'kept in core'});await k.evict('notes');
  const row=await k.sim.inspect('notes','a');assert.equal(row.data,null);assert.equal(row.base.data,null);
  const q=k.appApi('notes','1').query('notes',{});await k.sim.settle();assert.equal(q.get()[0].title,'kept in core');
 } finally {await k.close();}
});
test('quota LRU reclaim protects pending input, release code and archive',async()=>{
 const db=new MemoryBackend('quota-reclaim');await db.tx(async t=>{
  for(const collection of ['hot','cold','pinned','legacy','business_apps'])await t.put('docs',{collection,id:'a',data:{id:'a',large:'x'.repeat(1000)},base:{data:{large:'x'.repeat(1000)}},legacy_unconfirmed:collection==='legacy'});
  await t.put('outbox',{op_id:'p',collection:'pinned',id:'a'});
  await t.put('archive_local',{ref:'rejected',body:'keep'});await t.put('artifacts',{hash:'code',content:'keep'});
 });
 const result=await reclaimCache(db,async()=>{},new Map([['cold',1],['hot',2]]),new Set(['business_apps']));
 assert.equal(result.collection,'cold');assert.equal((await db.get('docs',['cold','a'])).base.data,null);
 for(const collection of ['hot','pinned','legacy','business_apps'])assert.ok((await db.get('docs',[collection,'a'])).data);
 assert.equal((await db.all('outbox')).length,1);assert.equal((await db.all('archive_local')).length,1);assert.equal((await db.all('artifacts')).length,1);
});
test('IDB quota handling retries once after reclaim and exposes typed failure',async()=>{
 const {IdbBackend}=await import('../store-idb-v5.mjs');
 const db=Object.create(IdbBackend.prototype);let tries=0,reclaims=0;
 db.transaction=async()=>{tries++;throw new DOMException('full','QuotaExceededError')};
 db.recoverQuota=async()=>{reclaims++;return true};
 await assert.rejects(db.tx(()=>{}),e=>e.code==='quota');assert.equal(tries,2);assert.equal(reclaims,1);
 db.recoverQuota=async()=>false;tries=0;await assert.rejects(db.tx(()=>{}),e=>e.code==='quota');assert.equal(tries,1);
 db.recoverQuota=async()=>true;tries=0;db.transaction=async()=>{if(++tries===1)throw new DOMException('full','QuotaExceededError');return 'durable'};
 assert.equal(await db.tx(()=>{}),'durable');assert.equal(tries,2);
});

test('byte budget reclaims oldest cache collections while preserving protected input',async()=>{
 const db=new MemoryBackend('budget');await db.tx(async t=>{
  for(const collection of ['cold','hot','pending'])await t.put('docs',{collection,id:'a',data:{id:'a',text:'ä'.repeat(1000)}});
  await t.put('outbox',{op_id:'p',collection:'pending',id:'a'});
  await t.put('blobs',{ref:'pinned',pinned:true,size:100});
 });
 const result=await reclaimCache(db,async()=>{},new Map([['cold',1],['hot',2]]),new Set(),2500);
 assert.deepEqual(result.collections,['cold','hot']);assert.equal(result.within_budget,true);
 assert.ok((await db.get('docs',['pending','a'])).data);assert.equal((await db.get('blobs','pinned')).pinned,true);
 const full=await reclaimCache(db,async()=>{},new Map(),new Set(),1);assert.equal(full.within_budget,false);assert.equal(full.reclaimed,false);assert.equal((await db.all('outbox')).length,1);
});
