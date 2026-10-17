import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernelSession,getV3CapabilityToken,resetV3CapabilityTokenCacheForTests} from '../session.mjs';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {protocolChannels,FakeNativePeer,offer,scopeFor} from './fake-native-peer.mjs';

test('V3 token owner preserves shared acquisition, refresh, injected authority and terminal denial',async()=>{
 const prior=globalThis.fetch;delete globalThis.CTOX_BUSINESS_OS_SESSION;resetV3CapabilityTokenCacheForTests();let calls=0;
 try{
  globalThis.fetch=async(url,options)=>{calls++;assert.equal(url,'/api/business-os/auth/capability');assert.equal(options.credentials,'same-origin');return {ok:true,json:async()=>({capability_token:'fixture-'+calls,expires_at_ms:Date.now()+3600000})};};
  assert.deepEqual(await Promise.all([getV3CapabilityToken(),getV3CapabilityToken()]),['fixture-1','fixture-1']);assert.equal(calls,1);
  assert.equal(await getV3CapabilityToken(),'fixture-1');assert.equal(await getV3CapabilityToken({refresh:true}),'fixture-2');
  resetV3CapabilityTokenCacheForTests();globalThis.CTOX_BUSINESS_OS_SESSION={capability_token:'fixture-native',capability_expires_at_ms:Date.now()+3600000};
  assert.equal(await getV3CapabilityToken(),'fixture-native');assert.equal(calls,2);delete globalThis.CTOX_BUSINESS_OS_SESSION;
  resetV3CapabilityTokenCacheForTests();globalThis.fetch=async()=>{calls++;return {ok:false,status:403};};
  assert.equal(await getV3CapabilityToken(),null);assert.equal(await getV3CapabilityToken({refresh:true}),null);assert.equal(calls,3);
 }finally{globalThis.fetch=prior;delete globalThis.CTOX_BUSINESS_OS_SESSION;resetV3CapabilityTokenCacheForTests();}
});

test('V4 transport admits verified instance scopes and rejects invalid grants without V3 downgrade',async()=>{
 const prior=globalThis.fetch;globalThis.CTOX_BUSINESS_OS_SESSION={capability_token:'fixture-must-not-be-used',capability_expires_at_ms:Date.now()+3600000};let requests=0;
 globalThis.fetch=async()=>{requests++;throw Error('V4 must not acquire HTTP capability');};
 try{for(const valid of [true,false]){
  const pair=protocolChannels(),peer=new FakeNativePeer(pair.server);
  const auth=createKernelSession({instanceId:'instance',userId:'alice',hello:()=>offer('instance'),verifyScopeToken:async(token,identity)=>valid&&token==='test-scope-token'&&identity.instanceId==='instance'&&identity.userId==='alice',resolveScope:scopeFor});
  const transport=new WebRTCKernelTransport({channels:pair.client,authSession:auth,verifyPackage:async p=>p.sig==='test-signature'});
  const online=[];
  try{
   const connected=transport.connect({instanceId:'instance',userId:'alice'},{link:value=>online.push(value.state),page(){},write_result(){}});
   if(valid){await connected;assert.deepEqual(auth.tokens(),['test-scope-token']);assert.equal((await transport.scope('notes')).scope_hash,'scope:notes');assert.ok(online.includes('online'));}
   else {await assert.rejects(connected,e=>e.code==='forbidden');assert.ok(!online.includes('online'));assert.throws(()=>auth.tokens(),e=>e.code==='forbidden');}
  }finally{transport.close();peer.close();auth.close();}
  await assert.rejects(auth.hello({instanceId:'foreign',userId:'alice'}),e=>e.code==='forbidden');
 }
 assert.equal(requests,0,'denied V4 never consults legacy authority');
 }finally{globalThis.fetch=prior;delete globalThis.CTOX_BUSINESS_OS_SESSION;}
});


test('native enrollment challenge reaches the device proof producer and cannot be substituted',async()=>{
 const nonce='D'.repeat(43);let received;
 const session=createKernelSession({instanceId:'instance',userId:'alice',hello:async(identity,challenge)=>{received=challenge;const value=offer(identity.instanceId);value.device_proof.nonce=challenge.nonce;return value;},verifyScopeToken:async()=>true});
 const challenge={nonce};const value=await session.hello({instanceId:'instance',userId:'alice'},challenge);
 assert.equal(received,challenge);assert.equal(value.device_proof.nonce,nonce);assert.throws(()=>session.tokens(),e=>e.code==='forbidden');
 const stale=createKernelSession({instanceId:'instance',userId:'alice',hello:()=>offer('instance'),verifyScopeToken:async()=>true});
 await assert.rejects(stale.hello({instanceId:'instance',userId:'alice'},challenge),e=>e.code==='forbidden');session.close();stale.close();
});

test('native 37-byte P5 frames replay after a lost E14 ACK without applying the write twice',async()=>{
 const pair=protocolChannels(),peer=new FakeNativePeer(pair.server),frames=[];
 const send=pair.client[1].send.bind(pair.client[1]);pair.client[1].send=b=>{frames.push(b.slice());send(b);};
 const clientControlSend=pair.client[0].send.bind(pair.client[0]);let resultAcks=0;
 pair.client[0].send=b=>{if(typeof b!=='string'&&b[6]===1&&b[7]===48){const ack=JSON.parse(new TextDecoder().decode(b.slice(37)));if(ack.lane===1)resultAcks++;}clientControlSend(b);};
 const serverSend=pair.server[0].send.bind(pair.server[0]);let dropped=false;
 pair.server[0].send=b=>{if(typeof b!=='string'&&b[6]===1&&b[7]===48){const ack=JSON.parse(new TextDecoder().decode(b.slice(37)));if(ack.lane===1&&!dropped){dropped=true;return;}}serverSend(b);};
 const transport=new WebRTCKernelTransport({channels:pair.client,hello:offer('instance'),resolveScope:scopeFor,verifyPackage:async()=>true});
 const until=async predicate=>{const deadline=Date.now()+2000;while(!predicate()){assert.ok(Date.now()<deadline,'protocol progress deadline');await new Promise(r=>setTimeout(r,1));}};
 let receipts=0;
 try{
  await transport.connect({instanceId:'instance',userId:'alice'},{link(){},page(){},write_result(){receipts++;}});
  await transport.write({client_seq:'1',op_id:'first',collection:'notes',id:'n',op:'put',body:{value:'fragment'.repeat(5000)},base_seq:'0',gen:1,schema_ver:1,app_ver:'1'});
  await until(()=>dropped&&receipts===1);assert.ok(frames.length>1);
  for(const b of frames){const v=new DataView(b.buffer,b.byteOffset,b.byteLength);assert.equal(b[4],1);assert.equal(new TextDecoder().decode(b.slice(7,7+b[6])),'1');assert.ok(b.length<=16384);assert.ok(v.getUint32(31)>1);assert.ok(v.getUint16(35)<=16347);}
  assert.equal(transport.reliable.retained[1].size,1,'lost ACK retains the message');
  await transport.scheduler.replay();await until(()=>transport.reliable.acked[1]===1n);
  assert.equal(transport.reliable.retained[1].size,0);assert.equal(peer.messages.filter(m=>m.type==='write').length,1);
  // The Core may replay its receipt after losing the reverse ACK as well.
  // The production browser ingress must ACK again without delivering twice.
  const before=resultAcks;
  await peer.frameNative(1,{type:'write_result',op_id:'first',status:'accepted',rev:'core-rev',seq:String(BigInt(peer.head)+1n)},'1');
  await until(()=>resultAcks>before);
  assert.equal(transport.reliable.delivered[1],1n);assert.equal(receipts,1);assert.deepEqual(peer.errors,[]);
 }finally{transport.close();peer.close();}
});
