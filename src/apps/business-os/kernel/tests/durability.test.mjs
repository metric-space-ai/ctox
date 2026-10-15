import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel,createStubKernel,CtoxError} from '../kernel.mjs';
import {MemoryBackend,STORES} from '../store-idb-v5.mjs';
import {assertLeader,acquireGeneration} from '../leader.mjs';
import {decimalSeq} from '../transport.mjs';
import {convertV3Record} from '../migrate-v3.mjs';

let n=0;
const options=()=>({instanceId:`durability-${process.pid}-${++n}`,userId:'alice',storage:'memory',multiTab:false});
test('memory transactions roll back all stores on failure',async()=>{
 const db=new MemoryBackend('rollback');
 await assert.rejects(db.tx(async t=>{await t.put('meta',{key:'x',value:1});await t.put('outbox',{op_id:'op',body:'input'});throw Error('abort')}));
 assert.equal(await db.get('meta','x'),undefined);assert.deepEqual(await db.all('outbox'),[]);
});
test('leader generation checked in the effect transaction rejects stale writes atomically',async()=>{
 const db=new MemoryBackend('fence');const old=await acquireGeneration(db);await acquireGeneration(db);
 await assert.rejects(db.tx(async t=>{await t.put('meta',{key:'effect',value:true});await assertLeader(t,true,old)}));
 assert.equal(await db.get('meta','effect'),undefined);
});
test('store has exactly the seven client stores; fake core is not persisted in IDB',()=>{
 assert.deepEqual(Object.keys(STORES).sort(),['archive_local','artifacts','blobs','docs','meta','outbox','window_state']);
});
test('V3 confirmed data and unconfirmed local inputs retain distinct bases',()=>{
 const confirmed=convertV3Record({collection:'notes',replicationOriginRole:'master',doc:{id:'a',title:'core',_rev:'r'}},1);
 assert.equal(confirmed.base.data.title,'core');assert.equal(confirmed.seq,'0');
 const pending=convertV3Record({collection:'notes',doc:{id:'a',title:'local'},base:{id:'a',title:'core'}},2);
 assert.equal(pending.data.title,'local');assert.equal(pending.base.data.title,'core');assert.equal(pending.legacy_unconfirmed,true);
 assert.throws(()=>convertV3Record({collection:'notes',doc:{}},1),CtoxError);
});
test('identity collisions cannot share a database or transport',async()=>{
 const opts=options(),one=createKernel({...opts,dbName:'identity-collision'});await one.ready;await one.close();
 const two=createKernel({...opts,userId:'eve',dbName:'identity-collision'});
 await assert.rejects(two.ready,e=>e.code==='forbidden');await two.close();
 assert.throws(()=>createKernel({}),e=>e.code==='invalid');
});
test('no transport means local durable outbox, never simulated acceptance',async()=>{
 const k=createKernel(options());await k.ready;const h=k.appApi('notes','1').put('notes',{id:'a'});await h.local;
 assert.equal(k.appApi('notes','1').get('notes','a').get().id,'a');
 assert.equal((await k.sim.outbox()).length,1);assert.equal(h.cancel(),true);
 assert.equal((await h.result).reason,'cancelled');await k.close();
});
test('single page transaction preserves large decimal seq, ignores stale records, advances empty cursor',async()=>{
 const k=createKernel(options());await k.ready;
 const page={collection:'notes',epoch:'1',scope_hash:'local',gen:1,cursor_to:'9007199254740999',complete:true,docs:[{id:'a',data:{id:'a',title:'new'},seq:'9007199254740999',rev:'new'}]};
 await k.receivePage(page);
 await k.receivePage({...page,cursor_to:'9007199254740998',docs:[{id:'a',data:{id:'a',title:'old'},seq:'9007199254740998',rev:'old'}]});
 assert.equal(k.appApi('notes','1').get('notes','a').get().title,'new');
 await assert.rejects(k.receivePage({...page,cursor_to:'1'}));
 assert.equal(k.appApi('notes','1').get('notes','a').get().title,'new');
 await k.receivePage({...page,docs:[],cursor_to:'9007199254741000'});
 assert.equal(k.sim.checkpoint,'9007199254741000');
 assert.throws(()=>decimalSeq(9007199254740999),CtoxError);await k.close();
});
test('logout prevents reads and keeps identity outboxes separate',async()=>{
 const k=createStubKernel(options());await k.ready;await k.sim.setLink({state:'offline'});
 const api=k.appApi('notes','1');await api.put('notes',{id:'draft'}).local;await k.logout();
 assert.throws(()=>api.query('notes',{}).get(),e=>e.code==='unavailable');
 const other=createKernel({...options(),userId:'bob'});await other.ready;assert.deepEqual(other.appApi('notes','1').query('notes',{}).get(),[]);await other.close();
});

test('external V3 takeover refuses an unverified or live source before opening it',async()=>{
 const {takeoverV3Database}=await import('../migrate-v3.mjs');
 await assert.rejects(takeoverV3Database({}, {name:'foreign',instanceId:'i',userId:'eve',quiesced:true},{instanceId:'i',userId:'alice'}),e=>e.code==='forbidden');
 await assert.rejects(takeoverV3Database({}, {name:'busy',instanceId:'i',userId:'alice'},{instanceId:'i',userId:'alice'}),e=>e.code==='forbidden');
});

test('page generation mismatch is rejected before changing local data',async()=>{
 const k=createKernel(options());await k.ready;
 try {
  await assert.rejects(k.receivePage({collection:'notes',gen:2,epoch:'1',scope_hash:'local',cursor_to:'1',complete:true,docs:[{id:'a',data:{id:'a'},seq:'1',rev:'r'}]}),e=>e.code==='invalid');
  assert.equal(k.appApi('notes','1').get('notes','a').get(),null);assert.equal(k.sim.checkpoint,'0');
 } finally {await k.close();}
});

test('logout releases Web Lock and callbacks even if app cleanup never returns',async()=>{
 const {testLocks}=await import('./web-locks.mjs');
 const opts={instanceId:`logout-hung-${process.pid}`,userId:'alice',storage:'memory',locks:testLocks,remountTimeoutMs:25,latencyMs:5};
 const leader=createStubKernel(opts);await leader.ready;
 const reader=createStubKernel(opts);await reader.ready;
 try {
  await leader.sim.publishAppVersion('notes','1',{module:{mount(){return {unmount:()=>new Promise(()=>{})}}}});
  await leader.shell.mount({},'notes');
  await leader.logout();assert.throws(()=>leader.appApi('notes','1').query('notes',{}).get(),e=>e.code==='unavailable');
  for(let i=0;i<20 && !reader.isLeader;i++)await new Promise(r=>setTimeout(r,5));
  assert.equal(reader.isLeader,true);
 } finally {await leader.close();await reader.close();}
});
test('closing before storage startup rejects readiness instead of starting timers',async()=>{
 const k=createKernel(options());const ready=assert.rejects(k.ready,e=>e.code==='unavailable');await k.close();await ready;
 assert.equal(k.isLeader,false);
});
