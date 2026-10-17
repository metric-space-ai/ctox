import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../kernel.mjs';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {publishNativePlatformCatalog} from './native-platform-catalog.mjs';
import {protocolChannels,FakeNativePeer,offer,scopeFor} from './fake-native-peer.mjs';
test('E35 native wire carries expected_revision only when supplied',async()=>{
 const pair=protocolChannels(),peer=new FakeNativePeer(pair.server),transport=new WebRTCKernelTransport({channels:pair.client,hello:offer('instance'),verifyPackage:async()=>true});
 try {
  await transport.connect({instanceId:'instance',userId:'alice'},{link(){},page(){},write_result(){}});
  const base={op_id:'expectation-op',client_seq:'1',collection:'notes',id:'n',base_seq:'0',base_rev:null,gen:1,schema_ver:1,app_ver:'1',op:'patch',body:{value:1}};
  await transport.write({...base,expected_revision:'1-native'});await new Promise((resolve,reject)=>{const deadline=Date.now()+2000;const tick=()=>peer.messages.some(m=>m.type==='write')?resolve():Date.now()>deadline?reject(Error('write delivery deadline')):setTimeout(tick,2);tick();});
  assert.equal(peer.messages.find(m=>m.type==='write').expected_revision,'1-native');
  await transport.write({...base,op_id:'no-expectation-op',client_seq:'2'});
  await new Promise((resolve,reject)=>{const deadline=Date.now()+2000;const tick=()=>peer.messages.filter(m=>m.type==='write').length===2?resolve():Date.now()>deadline?reject(Error('second write delivery deadline')):setTimeout(tick,2);tick();});
  assert.equal(Object.hasOwn(peer.messages.filter(m=>m.type==='write')[1],'expected_revision'),false);
  assert.deepEqual(peer.errors,[]);
 } finally {transport.close();peer.close();}
});

test('E35 native CAS rejection after metadata acceptance preserves code, reason and rejected document',async()=>{
 for(const form of ['status','error']) {
  const instanceId=`cas-wire-${process.pid}-${form}`,pair=protocolChannels(),peer=new FakeNativePeer(pair.server,{instanceId});
  const transport=new WebRTCKernelTransport({channels:pair.client,hello:offer(instanceId),resolveScope:scopeFor,verifyPackage:async()=>true});
  const apply=peer.apply.bind(peer);let held;
  peer.apply=async(m,lane)=>{if(m.type==='transfer_put'){peer.messages.push(m);held=m;return;}return apply(m,lane);};
  const k=createKernel({instanceId,userId:'alice',storage:'memory',multiTab:false,latencyMs:0,transport});let timer;
  try {
   await k.ready;await transport.ready;
   const subscribedBy=Date.now()+2000;
   while(!peer.messages.some(m=>m.type==='subscribe'&&m.collection==='business_app_versions')){assert.ok(Date.now()<subscribedBy,'native subscriptions precede fixture pages');await new Promise(r=>setTimeout(r,2));}
   await publishNativePlatformCatalog((collection,docs)=>peer.page(collection,docs));
   while(!(await k.sim.inspect('business_app_versions','ctox-platform@platform-native'))?.data){assert.ok(Date.now()<subscribedBy,'native catalog applied before file metadata');await new Promise(r=>setTimeout(r,2));}
   await peer.page('business_apps',[{id:'notes',seq:peer.head,rev:'a',data:{id:'notes',active_ver:'1'}}]);
   await peer.page('business_app_versions',[{id:'notes@1',seq:peer.head,rev:'v',data:{id:'notes@1',app_id:'notes',manifest:{collections:{notes:{schema:{type:'object',properties:{id:{type:'string'}}}}}}}}]);
   const file=await k.appApi('notes','1').files.put({text:'native CAS bytes'},{id:'wire-file'});
   const deadline=Date.now()+2000;
   while(!held || !(await k.sim.inspect('files',file.id))?.base?.data){assert.ok(Date.now()<deadline,'metadata receipt before CAS rejection');await new Promise(r=>setTimeout(r,2));}
   const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('native file completion deadline')),2000);});
   const denied=assert.rejects(Promise.race([file.done,timeout]),e=>e.code==='quota'&&e.reason==='Native CAS quota denied');
   await peer.send('transfer',{type:'transfer_result',request_id:held.request_id,ref:held.ref,...(form==='status'?{status:'rejected',code:'quota',reason:'Native CAS quota denied'}:{error:{code:'quota',reason:'Native CAS quota denied'}})});
   await denied;clearTimeout(timer);
   const row=await k.sim.inspect('files',file.id);assert.ok(row.base.data,'accepted metadata receipt remains canonical');assert.equal(row.data.state,'rejected');
   assert.equal((await k.sim.outbox()).filter(e=>e.op==='upload').length,0);assert.deepEqual(peer.errors,[]);
  } finally {clearTimeout(timer);await k.close();peer.close();}
 }
});
