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
