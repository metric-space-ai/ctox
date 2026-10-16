import {test} from 'node:test';
import assert from 'node:assert/strict';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {protocolChannels,FakeNativePeer,offer} from './fake-native-peer.mjs';
test('E35 native wire carries expected_revision only when supplied',async()=>{
 const pair=protocolChannels(),peer=new FakeNativePeer(pair.server),transport=new WebRTCKernelTransport({channels:pair.client,hello:offer('instance'),verifyPackage:async()=>true});
 try {
  await transport.connect({instanceId:'instance',userId:'alice'},{link(){},page(){},write_result(){}});
  const base={op_id:'expectation-op',client_seq:'1',collection:'notes',id:'n',base_seq:'0',base_rev:null,gen:1,schema_ver:1,app_ver:'1',op:'patch',body:{value:1}};
  await transport.write({...base,expected_revision:'1-native'});await new Promise((resolve,reject)=>{const deadline=Date.now()+2000;const tick=()=>peer.messages.some(m=>m.type==='write')?resolve():Date.now()>deadline?reject(Error('write delivery deadline')):setTimeout(tick,2);tick();});
  assert.equal(peer.messages.find(m=>m.type==='write').expected_revision,'1-native');
 } finally {transport.close();peer.close();}
});
