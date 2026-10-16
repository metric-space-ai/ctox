import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel,createStubKernel} from '../testkit.mjs';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {protocolChannels,FakeNativePeer,offer,scopeFor} from './fake-native-peer.mjs';

const tick=()=>new Promise(r=>setTimeout(r,5));
test('real transport feeds atomic IDB-style pages with canonical per-collection scope and clock metrics',async()=>{
 let pair,peer,release,hellos=0;const addresses=[],sessionState={};
 const createPeer=async address=>{addresses.push(address);pair=protocolChannels();peer=new FakeNativePeer(pair.server,{sessionState,onSubscribe:async(sub,p)=>p.page(sub.collection,sub.collection==='notes'?[{id:'server',data:{id:'server',title:'core'},seq:p.head,rev:'r'}]:[])});
  const native=peer;return {channels:pair.client,close:()=>native.close()};
 };
 const transport=new WebRTCKernelTransport({address:'native:first',createPeer,hello:()=>++hellos===1?new Promise(r=>release=r):offer('instance'),resolveScope:scopeFor,verifyPackage:async m=>m.sig==='test-signature'});
 const k=createKernel({instanceId:'instance',userId:'alice',storage:'memory',multiTab:false,transport});
 try {
  await k.ready;assert.equal(transport.session,undefined);release(offer('instance'));await transport.ready;
  const h=k.appApi('notes','1').put('notes',{id:'client',title:'local'});await h.local;assert.equal((await h.result).status,'accepted');
  for(let n=0;n<100&&(!k.appApi('notes','1').get('notes','server').get()||!k.metrics.get().timing);n++)await tick();
  assert.equal(k.appApi('notes','1').get('notes','server').get().title,'core');assert.equal(k.sim.checkpoint,'9007199254741000');
  assert.ok(k.metrics.get().timing.samples>=1);assert.equal(k.shell.mount,undefined);assert.equal(k.sim.registerModule,undefined);
  const {openBackend}=await import('../store-idb-v5.mjs');const db=await openBackend(k.dbName,'memory');const meta=(await db.all('meta')).find(m=>m.collection==='notes');assert.equal(meta.scope_hash,'scope:notes');assert.equal(meta.cursor_to,'9007199254740999');
  const operations=db._ops.bind(db);db._ops=stores=>{const ops=operations(stores);return {...ops,put:async(store,value)=>{if(store==='meta'&&value.collection==='notes'&&value.cursor_to==='9007199254741001')throw Error('checkpoint fault');return ops.put(store,value);}};};
  try{await assert.rejects(k.receivePage({collection:'notes',scope_hash:'scope:notes',epoch:'1',gen:1,cursor_to:'9007199254741001',complete:true,docs:[{id:'rollback',data:{id:'rollback'},seq:'9007199254741001',rev:'fault'}]}),/checkpoint fault/);
   assert.equal(await db.get('docs',['notes','rollback']),undefined);assert.equal((await db.all('meta')).find(m=>m.collection==='notes').cursor_to,'9007199254740999');
  }finally{db._ops=operations;}
  pair.client[0].close();assert.equal(k.metrics.get().link.state,'offline');
  const queued=k.appApi('notes','1').put('notes',{id:'after-disconnect',title:'durable offline'});await queued.local;
  assert.ok((await k.sim.outbox()).some(entry=>entry.op_id===queued.op_id));
  await transport.reconnect('native:replacement');assert.equal((await queued.result).status,'accepted');
  assert.deepEqual(addresses,['native:first','native:replacement']);assert.ok(peer.messages.some(message=>message.type==='write'&&message.op_id===queued.op_id));
 }finally{await k.close();peer.close();}
});
test('platform collections use the platform schema rather than the calling app schema',async()=>{
 const k=createStubKernel({instanceId:'platform-schema',userId:'alice',storage:'memory',multiTab:false});
 try{await k.ready;
  await k.sim.publishAppVersion('ctox-platform','1',{collections:{shell_preferences:{schema:{type:'object',required:['pins'],properties:{pins:{type:'array'}}}}}});
  await k.sim.publishAppVersion('notes','1',{collections:{shell_preferences:{schema:{type:'object',required:['wrong']}}}});
  const api=k.appApi('notes','1');await assert.rejects(api.put('shell_preferences',{id:'alice'}).local,e=>e.code==='invalid');
  assert.equal((await api.put('shell_preferences',{id:'alice',pins:[]}).result).status,'accepted');
 }finally{await k.close();}
});
