import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../kernel.mjs';
import {KernelTransport} from '../transport.mjs';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {readFile} from 'node:fs/promises';
const schema=JSON.parse(await readFile(new URL('../../../../core/rxdb/tests/fixtures/business-command-document-v4.json',import.meta.url)));
const tick=()=>new Promise(r=>setTimeout(r,5));
class Capture extends KernelTransport {
 async connect(_,events){this.events=events;events.link({state:'online',rtt_ms:1,relay:false});}
 async subscribe(s){(this.subs??=[]).push(s);}
 async write(e){(this.sent??=[]).push(e);}
}
async function setup(name){
 const transport=new Capture(),k=createKernel({instanceId:name+process.pid,userId:'alice',storage:'memory',multiTab:false,latencyMs:0,transport});await k.ready;
 const page=(collection,docs,cursor='10')=>transport.events.page({collection,epoch:'1',scope_hash:'local',gen:1,cursor_to:cursor,complete:true,backlog:0,docs});
 await page('business_apps',[{id:'ctox-platform',seq:'1',rev:'a',doc:{id:'ctox-platform',active_ver:'platform-7'}}]);
 await page('business_app_versions',[{id:'ctox-platform@platform-7',seq:'2',rev:'v',doc:{id:'ctox-platform@platform-7',manifest:{collections:{business_commands:{schema}}}}}]);
 return {k,transport,page,api:k.appApi('sender','99')};
}
test('canonical replay retains the outbox until its own row, despite advanced cursor and stale/foreign pages',async()=>{
 const {k,transport,page,api}=await setup('replay');
 try {
  const h=api.put('business_commands',{id:'c',type:'task',app_id:'sender',actor_id:'alice',created_at:'2026-10-15T00:00:00.000Z',status:'queued',payload:{n:1}});await h.local;
  for(let i=0;i<100&&!transport.sent?.length;i++)await tick();
  assert.equal(transport.sent[0].schema_ver,3);assert.equal(transport.sent[0].app_ver,'platform-7');
  await page('business_commands',[],'100');
  let settled=false;h.result.then(()=>{settled=true});
  await transport.events.write_result({op_id:h.op_id,status:'merged',requires_page:true,gen:1,seq:'20',rev:'done'});
  assert.equal((await k.sim.outbox()).length,1);assert.equal(settled,false);
  assert.ok(BigInt(transport.subs.at(-1).checkpoint_seq)<20n);
  await page('business_commands',[{id:'other',seq:'21',rev:'other',doc:{id:'other',status:'done'}}],'101');
  assert.equal((await k.sim.outbox()).length,1);
  await page('business_commands',[{id:'c',seq:'19',rev:'stale',doc:{id:'c',status:'queued'}}],'101');
  assert.equal((await k.sim.outbox()).length,1);
  await page('business_commands',[{id:'c',seq:'20',rev:'done',doc:{id:'c',type:'task',app_id:'sender',actor_id:'alice',created_at:'2026-10-15T00:00:00.000Z',status:'done',payload:{n:1},result:{ok:true}}}],'101');
  assert.equal((await h.result).status,'merged');assert.equal(api.get('business_commands','c').get().status,'done');
  assert.equal((await k.sim.outbox()).length,0);assert.equal(k.sim.checkpoint,'101');
  // Replay's old page must never replace the canonical newer terminal result.
  await page('business_commands',[{id:'c',seq:'19',rev:'stale',doc:{id:'c',status:'queued'}}],'19');
  assert.equal(api.get('business_commands','c').get().status,'done');
 }finally{await k.close();}
});
test('page-before-receipt and tombstones complete canonical writes without ack payloads',async()=>{
 const {k,transport,page,api}=await setup('page-first');
 try {
  const h=api.put('items',{id:'x',n:1});await h.local;
  await page('items',[{id:'x',seq:'30',rev:'removed',op:'delete'}],'30');
  await transport.events.write_result({op_id:h.op_id,status:'merged',requires_page:true,seq:'20',rev:'r20',gen:1,doc:{id:'x',forged:true}});
  assert.equal((await h.result).status,'merged');assert.equal(api.get('items','x').get(),null);assert.equal((await k.sim.outbox()).length,0);
  await page('items',[{id:'x',seq:'19',rev:'stale',body:{id:'x',n:0}}],'30');
  assert.equal(api.get('items','x').get(),null);
 }finally{await k.close();}
});
test('the real WebRTC ingress delivers typed merged receipts without payload authority',async()=>{
 const t=new WebRTCKernelTransport({});let receipt;t.events={write_result:r=>{receipt=r}};
 await t.apply({type:'write_result',status:'merged',requires_page:true,op_id:'op',seq:'9007199254740999',code:'conflict',reason:'stale_fence'},1);
 assert.equal(receipt.requires_page,true);assert.equal(receipt.seq,'9007199254740999');assert.equal(receipt.reason,'stale_fence');
 t.close();
});
test('dispatch sequence has no cancellation gap and reconnect reuses its durable slot',async()=>{
 const {k,transport,api}=await setup('client-order');
 try {
  transport.events.link({state:'offline'});
  const cancelled=api.put('items',{id:'cancelled'});await cancelled.local;assert.equal(cancelled.cancel(),true);
  for(let i=0;i<100&&(await k.sim.outbox()).length;i++)await tick();
  const h=api.put('items',{id:'kept'});await h.local;
  transport.events.link({state:'online'});
  for(let i=0;i<100&&!transport.sent?.length;i++)await tick();
  assert.equal(transport.sent[0].client_seq,'1');
  transport.events.link({state:'offline'});transport.events.link({state:'online'});
  for(let i=0;i<100&&transport.sent.length<2;i++)await tick();
  assert.equal(transport.sent[1].op_id,h.op_id);assert.equal(transport.sent[1].client_seq,'1');
 }finally{await k.close();}
});

test('migrated receipt needs a page from the authenticated target generation',async()=>{
 const {k,transport,api,page}=await setup('migrated-generation');
 try {
  const h=api.put('items',{id:'x',n:1});await h.local;
  await page('items',[{id:'x',seq:'10',rev:'old',body:{id:'x',n:0}}],'10');
  transport.scope=async collection=>({collection,epoch:'1',scope_hash:'local',gen:2});
  await transport.events.write_result({op_id:h.op_id,status:'migrated',requires_page:true,seq:'20',rev:'new',gen:2});
  assert.equal((await k.sim.outbox()).length,1);assert.equal(transport.subs.at(-1).gen,2);
  await assert.rejects(page('items',[{id:'x',seq:'21',rev:'wrong',body:{id:'x',n:2}}],'21'),e=>e.code==='invalid');
  await transport.events.page({collection:'items',epoch:'1',scope_hash:'local',gen:2,cursor_to:'21',complete:true,backlog:0,docs:[{id:'x',seq:'21',rev:'new',body:{id:'x',n:2}}]});
  assert.equal((await h.result).status,'migrated');assert.equal(api.get('items','x').get().n,2);assert.equal((await k.sim.outbox()).length,0);
 }finally{await k.close();}
});
