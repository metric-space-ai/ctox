import {test} from 'node:test';
import assert from 'node:assert/strict';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {protocolChannels,FakeNativePeer,offer,scopeFor} from './fake-native-peer.mjs';
import {sha256Hex} from '../values.mjs';
const tick=()=>new Promise(r=>setTimeout(r,5));
function fixture(opts={}){const pair=protocolChannels(),peer=new FakeNativePeer(pair.server,opts),transport=new WebRTCKernelTransport({wireFormat:"k2-fixture",channels:pair.client,hello:offer(opts.instanceId??'instance'),resolveScope:scopeFor,verifyPackage:async p=>p.sig==='test-signature',...opts.transport});return {peer,transport};}
test('P4 hello and exact V4 subscribe/write/result/activation/priority wire',async()=>{
 const {peer,transport}=fixture({transport:{hello:async()=>{await new Promise(resolve=>setTimeout(resolve,450));return offer("instance");}}}),pages=[],results=[],timings=[];
 try {
  await transport.connect({instanceId:'instance',userId:'alice'},{link(){},page:async p=>pages.push(p),write_result:async r=>results.push(r),timing:t=>timings.push(t)});
  const scope=await transport.scope('notes');await transport.subscribe({...scope,collection:'notes',checkpoint_seq:'9007199254740998',priority:'P0'});
  await peer.page('notes',[{id:'a',data:{id:'a'},seq:peer.head,rev:'r'}]);
  await transport.write({client_seq:'1',op_id:'op1',prev_op_id:'prior',collection:'notes',id:'a',op:'patch',body:{title:'x'},base_rev:'op:prior',base_seq:'9007199254740998',gen:1,schema_ver:1,app_id:'notes',app_ver:'1'});
  await transport.appActivation({app_id:'notes',app_ver:'1',state:'activated',at_seq:peer.head});transport.priority({app_id:'notes',collections:['notes']});
  for(let i=0;i<20&&(!results.length||!timings.length||!peer.messages.some(m=>m.type==='priority'));i++)await tick();
  assert.equal(pages[0].cursor_to,'9007199254740999');assert.equal(results[0].seq,'9007199254741000');
  assert.equal(peer.messages.find(m=>m.type==='subscribe').checkpoint.seq,'9007199254740998');assert.equal(peer.messages.find(m=>m.type==='write').prev_op_id,'prior');
  assert.ok(peer.messages.some(m=>m.type==='app_activation'));assert.ok(timings[0].push_latency_ms>=0);assert.ok(Math.abs(timings[0].clock_offset_ms-5000)<50);assert.deepEqual(peer.errors,[]);
 }finally{transport.close();peer.close();}
});
test('bad signature/progress proof fails closed without page application',async()=>{
 for(const extra of [{sig:'tampered'},{cursor_to:'9007199254741000',complete:false},{complete:true,cursor_to:'1',seq_to:'1'}]){
  const {peer,transport}=fixture();let applied=0,offline=false;
  try {
   await transport.connect({instanceId:'instance',userId:'alice'},{link:l=>offline=l.state==='offline',page:async()=>applied++,write_result(){}});
   await transport.subscribe({...await transport.scope('notes'),collection:'notes',checkpoint_seq:'0',priority:'P2'});
   await peer.page('notes',[],extra);for(let i=0;i<20&&!transport.closed;i++)await tick();
   assert.equal(applied,0);assert.equal(transport.closed,true);assert.equal(offline,true);
  }finally{transport.close();peer.close();}
 }
});
test('CAS upload pipelines four chunks and download/hash crosses frame boundaries',async()=>{
 const {peer,transport}=fixture();
 try {
  await transport.connect({instanceId:'instance',userId:'alice'},{link(){},page(){},write_result(){}});
  const bytes=new Uint8Array(280000);let seed=0x12345678;for(let i=0;i<bytes.length;i++){seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;bytes[i]=seed&255;}
  const blob=new Blob([bytes],{type:'application/octet-stream'}),ref=await sha256Hex(blob);await transport.upload(ref,blob);
  assert.deepEqual(new Uint8Array(await(await transport.blob(ref)).arrayBuffer()),bytes);assert.ok(peer.messages.filter(m=>m.type==='transfer_put').length>=4);assert.deepEqual(peer.errors,[]);
  assert.ok(peer.sent[5]>peer.messages.filter(m=>m.type==='transfer_put'||m.type==='transfer_get').length,'incompressible CAS actually crosses negotiated frame boundaries');
  peer.blobs.set(ref,new Blob(['x'.repeat(blob.size)]));await assert.rejects(transport.blob(ref),e=>e.code==='invalid');
 }finally{transport.close();peer.close();}
});
test('wrong pairing and missing verifier never open a data path',async()=>{
 const {peer,transport}=fixture();await assert.rejects(transport.connect({instanceId:'foreign',userId:'alice'},{link(){},page(){},write_result(){}}),e=>e.code==='forbidden');peer.close();
});
