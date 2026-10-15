import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../kernel.mjs';
import {MemoryBackend} from '../store-idb-v5.mjs';
import {evictCollection} from '../cache.mjs';
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
