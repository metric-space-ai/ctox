import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../kernel.mjs';
import {openBackend} from '../store-idb-v5.mjs';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {protocolChannels,FakeNativePeer,offer,scopeFor} from './fake-native-peer.mjs';
const until=async pred=>{for(let i=0;i<500;i++){if(await pred())return;await new Promise(r=>setTimeout(r,4));}assert.fail('push invariant deadline');};
test('E47 demand-only push resumes without a pull checkpoint, across association replacement and kernel reload',async t=>{
 const native={ops:new Map(),messages:[],drop:false};let peer,pair;
 class DemandPeer extends FakeNativePeer {
  async apply(message,lane){
   if(message.type!=='write')return super.apply(message,lane);
   native.messages.push(message);
   let receipt=native.ops.get(message.op_id);
   if(!receipt){receipt={type:'write_result',op_id:message.op_id,client_seq:message.client_seq,status:'accepted',seq:String(100+native.ops.size),rev:'native-'+message.op_id,doc:message.body};native.ops.set(message.op_id,receipt);}
   if(native.drop){native.drop=false;return;}await this.send('commands',receipt);
  }
 }
 const connect=()=>{const sessionState={};return new WebRTCKernelTransport({hello:()=>offer('push-no-pull'),createPeer:async()=>{pair=protocolChannels();peer=new DemandPeer(pair.server,{instanceId:'push-no-pull',sessionState});return {channels:pair.client,close:()=>peer.close()};},resolveScope:scopeFor,verifyPackage:async()=>true});};
 const options={instanceId:'push-no-pull',userId:'alice',dbName:'push-no-pull-'+process.pid,storage:'memory',multiTab:false,latencyMs:0};
 let transport=connect(),kernel=createKernel({...options,transport});t.after(()=>kernel.close());await kernel.ready;await transport.ready;
 const api=kernel.appApi('leads','1'),a=api.put('leads',{id:'already-acked',value:'retained native'});await a.local;await until(()=>native.ops.has(a.op_id));await a.result;
 native.drop=true;const b=api.put('leads',{id:'in-flight',value:'retry same op'});await b.local;await until(()=>native.ops.has(b.op_id));
 const original=(await kernel.sim.outbox()).find(e=>e.op_id===b.op_id);assert.ok(original.client_seq);
 const db=await openBackend(kernel.dbName,'memory');assert.equal((await db.all('meta')).some(row=>row.collection==='leads'&&'cursor_to' in row),false,'peer never sent a pull page');
 await transport.reconnect();await until(async()=>!(await kernel.sim.outbox()).some(e=>e.op_id===b.op_id));
 assert.equal(native.messages.filter(m=>m.op_id===a.op_id).length,1,'confirmed progress cannot reset solely because pull has no cursor');
 const sends=native.messages.filter(m=>m.op_id===b.op_id);assert.equal(sends.length,2);assert.equal(sends[1].client_seq,original.client_seq);assert.equal(native.ops.size,2,'resend applies exactly once');
 await kernel.close();transport=connect();kernel=createKernel({...options,transport});await kernel.ready;await transport.ready;
 const c=kernel.appApi('leads','1').put('leads',{id:'after-reload',value:'next'});await c.local;await c.result;
 assert.ok(BigInt(native.messages.find(m=>m.op_id===c.op_id).client_seq)>BigInt(original.client_seq));assert.equal(native.ops.size,3);assert.equal(native.messages.filter(m=>m.op_id===a.op_id).length,1);
});

test('E47 demand-only context changes fully re-offer the durable outbox under fresh scopes',async t=>{
 // V4 has no historical push cursor: the complete unconfirmed outbox is the
 // resend set. A new scope never borrows another collection/store/role cursor.
 // The peer retains accepted op IDs; authorization of a replay belongs to Core.
 for(const changed of ['storage-generation','collection','schema-generation','local-store','role-digest']){
  const native={ops:new Map(),first:[],resumed:[],subs:[]};let phase=0,peer;
  const instanceId='push-context-'+changed+'-'+process.pid,dbName=instanceId+'-db';
  class ContextPeer extends FakeNativePeer {
   async apply(message,lane){
    if(message.type==='subscribe')native.subs.push({phase,...message});
    if(message.type!=='write')return super.apply(message,lane);
    (phase?native.resumed:native.first).push(message);
    let receipt=native.ops.get(message.op_id);
    if(!receipt){receipt={type:'write_result',op_id:message.op_id,client_seq:message.client_seq,status:'accepted',seq:String(100+native.ops.size),rev:'accepted-'+message.op_id,doc:message.body};native.ops.set(message.op_id,receipt);}
    if(phase)await this.send('commands',receipt); // drop first write results, never a pull page
   }
  }
  function connect(){return new WebRTCKernelTransport({hello:()=>offer(instanceId),createPeer:async()=>{const pair=protocolChannels();peer=new ContextPeer(pair.server,{instanceId,epoch:phase&&changed==='storage-generation'?'2':'1'});return {channels:pair.client,close:()=>peer.close()};},resolveScope:collection=>({...scopeFor(collection),...(phase&&changed==='schema-generation'?{gen:2}:{}),scope_hash:`scope:${collection}:${phase&&changed==='role-digest'?'role-2':'role-1'}`}),verifyPackage:async()=>true});}
  let transport=connect();const options={instanceId,userId:'alice',dbName,storage:'memory',multiTab:false,latencyMs:0};
  let kernel=createKernel({...options,transport});await kernel.ready;await transport.ready;
  try{
   const api=kernel.appApi('writer','1');for(const id of ['a','b'])await api.put('leads',{id,value:changed}).local;
   await until(()=>native.first.length===2);
   transport.events.link({state:'offline',rtt_ms:0,relay:false});
   if(changed==='collection')await api.put('other_leads',{id:'new-collection',value:changed}).local;
   const pending=await kernel.sim.outbox();assert.equal(pending.length,changed==='collection'?3:2);
   await kernel.close();phase=1;
   let resumedDb=dbName;
   if(changed==='local-store'){
    resumedDb+='-replacement';const source=await openBackend(dbName,'memory'),replacement=await openBackend(resumedDb,'memory');
    for(const store of ['docs','meta','outbox']){const rows=await source.all(store);await replacement.tx(async tx=>{for(const row of rows)await tx.put(store,row);});}
   }
   transport=connect();kernel=createKernel({...options,dbName:resumedDb,transport});await kernel.ready;await transport.ready;
   await until(async()=>!(await kernel.sim.outbox()).length);
   assert.deepEqual(native.resumed.map(m=>m.op_id).sort(),pending.map(m=>m.op_id).sort(),changed+': every pending intent re-offered');
   for(const original of pending.filter(e=>e.client_seq!==null))assert.equal(native.resumed.find(m=>m.op_id===original.op_id).client_seq,original.client_seq,'client ordering survives '+changed);
   const sub=native.subs.find(s=>s.phase===1&&s.collection==='leads');assert.ok(sub);assert.equal(sub.checkpoint,null,'no invented pull checkpoint');
   assert.equal(String(transport.session.epoch),changed==='storage-generation'?'2':'1');
   assert.equal(sub.gen,changed==='schema-generation'?2:1);assert.ok(sub.scope_hash.endsWith(changed==='role-digest'?'role-2':'role-1'));
   assert.equal(native.ops.size,pending.length,'resend cannot become a new intent');
  }finally{await kernel.close();peer?.close();}
 }
});
