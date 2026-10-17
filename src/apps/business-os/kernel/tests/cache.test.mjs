import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from './legacy-kernel.mjs';
import {MemoryBackend} from '../store-idb-v5.mjs';
import {evictCollection,reclaimCache} from '../cache.mjs';
import {checkpointKey} from '../transport.mjs';

test('eviction invalidates the cursor and preserves unrelated durable stores',async()=>{
 const db=new MemoryBackend('cache');const scope={collection:'notes',scope_hash:'x',epoch:'1',gen:1};
 await db.tx(async t=>{
  await t.put('docs',{collection:'notes',id:'a',data:{id:'a'},base:{seq:'8'}});
  for(let n=0;n<65;n++)await t.put('docs',{collection:'notes',id:'extra-'+n,data:{id:'extra-'+n}});
  await t.put('meta',{key:checkpointKey(scope),...scope,cursor_to:'8',complete:true});
  await t.put('archive_local',{ref:'rejected',body:'keep'});await t.put('outbox',{op_id:'elsewhere',collection:'other'});
 });
 const rows=await evictCollection(db,'notes',async()=>{});
 assert.equal(rows[0].cache_evicted,true);assert.equal((await db.get('meta',checkpointKey(scope))).complete,false);
 assert.equal((await db.get('meta',checkpointKey(scope))).evicted_at_seq,'8','multi-slice eviction must preserve the original checkpoint');
 assert.equal((await db.get('meta',checkpointKey(scope))).cursor_to,'0');assert.equal((await db.all('archive_local')).length,1);assert.equal((await db.all('outbox')).length,1);
 const vacant={...scope,collection:'empty'};await db.tx(t=>t.put('meta',{key:checkpointKey(vacant),...vacant,cursor_to:'9',complete:true}));
 assert.deepEqual(await evictCollection(db,'empty',async()=>{}),[]);
 assert.equal((await db.get('meta',checkpointKey(vacant))).cursor_to,'0','empty collection eviction must invalidate its checkpoint');
 assert.equal((await db.get('meta',checkpointKey(vacant))).evicted_at_seq,'9');
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


test('incremental cache tick stays below 5 ms for 5000 documents without eviction',async()=>{
 const db=new MemoryBackend('cache-tick'),payload='x'.repeat(8192);
 await db.tx(async t=>{for(let n=0;n<5000;n++)await t.put('docs',{collection:'large',id:String(n),data:{id:String(n),payload},base:{data:{payload}}});});
 const elapsed=[];
 for(let n=0;n<12;n++){
  const start=performance.now(),result=await reclaimCache(db,async()=>{},new Map(),new Set(),2_000_000_000);
  elapsed.push(performance.now()-start);assert.equal(result.reclaimed,false);assert.equal(result.within_budget,true);
 }
 console.log('CACHE_TICK_MS '+JSON.stringify({documents:5000,max:Math.max(...elapsed),samples:elapsed}));
 assert.ok(Math.max(...elapsed)<=5,`cache tick exceeded 5 ms: ${Math.max(...elapsed)}`);
});

test('incremental bytes and pin counts survive overwrite delete and transaction rollback',async()=>{
 const {CACHE_USAGE_KEY,cacheBytes}=await import('../cache-accounting.mjs');const db=new MemoryBackend('cache-accounting');
 const row={collection:'notes',id:'a',data:{text:'ä'},legacy_unconfirmed:true};
 await db.tx(t=>t.put('docs',row));assert.equal((await db.get('meta',CACHE_USAGE_KEY)).value,cacheBytes(row));
 await assert.rejects(db.tx(async t=>{await t.put('docs',{...row,data:{text:'bad'.repeat(200)}});await t.put('blobs',{ref:'b',size:99});throw Error('rollback');}));
 assert.equal((await db.get('meta',CACHE_USAGE_KEY)).value,cacheBytes(row));
 const next={...row,legacy_unconfirmed:false,data:{text:'new'}};await db.tx(t=>t.put('docs',next));
 await db.tx(t=>t.put('blobs',{ref:'b',size:100}));assert.equal((await db.get('meta',CACHE_USAGE_KEY)).value,cacheBytes(next)+100);
 await db.tx(async t=>{await t.del('docs',['notes','a']);await t.del('blobs','b');});
 assert.equal((await db.get('meta',CACHE_USAGE_KEY)).value,0);
 const upload={op_id:'u',collection:'files',id:'blob',op:'upload'};
 await db.tx(async t=>{await t.put('outbox',upload);await t.put('outbox',{...upload,dispatched_at:1});});
 assert.equal(await db.read(t=>t.hasUpload('blob')),true);
 await assert.rejects(db.tx(async t=>{await t.del('outbox','u');throw Error('rollback');}));
 assert.equal(await db.read(t=>t.hasOutboxForCollection('files')),true);
 await db.tx(t=>t.del('outbox','u'));assert.equal(await db.read(t=>t.hasUpload('blob')),false);
 assert.equal(await db.read(t=>t.hasOutboxForCollection('files')),false);
});

test('cache eviction yields between bounded slices and retains input arriving between them',async()=>{
 const db=new MemoryBackend('cache-slices');await db.tx(async t=>{for(let n=0;n<100;n++)await t.put('docs',{collection:'notes',id:String(n),data:{id:String(n)}});});
 let inserted=false;const pending=new Promise(resolve=>setTimeout(()=>{db.tx(async t=>{await t.put('outbox',{op_id:'arrived',collection:'notes',id:'99'});await t.put('docs',{collection:'notes',id:'99',data:{id:'99',input:'keep'},pending:true});}).then(()=>{inserted=true;resolve();});},0));
 const records=await evictCollection(db,'notes',async()=>{});await pending;
 assert.equal(inserted,true);assert.ok(records.length>0&&records.length<100,'eviction must yield before all documents are removed');
 assert.equal((await db.get('docs',['notes','99'])).data.input,'keep');assert.equal((await db.all('outbox')).length,1);
});


test('pressure guards never clone the pending outbox to choose evictable cache',async()=>{
 const db=new MemoryBackend('cache-no-outbox-clone');await db.tx(async t=>{
  await t.put('docs',{collection:'cold',id:'a',data:{value:'rebuildable'}});
  await t.put('docs',{collection:'pending',id:'a',data:{value:'local input'}});
  await t.put('outbox',{op_id:'pending',collection:'pending',id:'a',body:{value:'local input'}});
  await t.put('blobs',{ref:'uploading',size:1000});await t.put('outbox',{op_id:'upload',collection:'files',id:'uploading',op:'upload'});
 });
 const ops=db._ops.bind(db);db._ops=(...args)=>{const t=ops(...args),all=t.all;return {...t,all:s=>{assert.notEqual(s,'outbox','cache pressure cloned the pending outbox');assert.notEqual(s,'docs','cache pressure cloned all documents');assert.notEqual(s,'meta','cache pressure cloned all metadata');return all(s);}};};
 const result=await reclaimCache(db,async()=>{},new Map(),new Set(),1);
 assert.deepEqual(result.collections,['cold']);assert.equal(result.within_budget,false);
 assert.equal((await db.get('docs',['pending','a'])).data.value,'local input');assert.equal((await db.get('blobs','uploading')).size,1000);
});
