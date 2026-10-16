import {test} from 'node:test';
import assert from 'node:assert/strict';
import {MemoryBackend} from '../store-idb-v5.mjs';
import {evictCollection,reclaimCache} from '../cache.mjs';
test('E35 rejected file input remains durable under explicit eviction and cache pressure',async()=>{
 const db=new MemoryBackend(`e35-rejected-cache-${process.pid}`);
 await db.tx(async t=>{
  await t.put('docs',{collection:'files',id:'f',data:{id:'f',ref:'content',state:'rejected'},base:{data:null,seq:'0'},local_file_rejection:true});
  await t.put('blobs',{ref:'content',blob:new Blob(['retained']),size:8,pinned:false});
  await t.put('docs',{collection:'cold',id:'cache',data:{id:'cache',text:'rebuildable'}});
 });
 await assert.rejects(evictCollection(db,'files',async()=>{}),e=>e.code==='quota');
 const result=await reclaimCache(db,async()=>{},new Map(),new Set(),1);
 assert.deepEqual(result.collections,['cold']);assert.equal(result.within_budget,false);
 const retained=await db.get('docs',['files','f']);assert.ok(retained?.data,'rejected metadata survives cache pressure');assert.equal(retained.data.state,'rejected');assert.ok(await db.get('blobs','content'));
 // Explicit discard of the rejected document releases its protected bytes.
 await db.tx(t=>t.del('docs',['files','f']));
 const discarded=await reclaimCache(db,async()=>{},new Map(),new Set(),1);assert.deepEqual(discarded.blobs,['content']);
});
