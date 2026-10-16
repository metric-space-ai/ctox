import {test} from 'node:test';
import assert from 'node:assert/strict';
import {WebRTCKernelTransport} from '../../src/apps/business-os/kernel/transport-webrtc.mjs';
import {FakeNativePeer,protocolChannels,offer} from '../../src/apps/business-os/kernel/tests/fake-native-peer.mjs';

test('peer disconnection reports offline while SCTP channels have not emitted close',async()=>{
 const pair=protocolChannels(),native=new FakeNativePeer(pair.server),peer=new EventTarget(),links=[];
 peer.connectionState='connected';peer.iceConnectionState='connected';peer.close=()=>{peer.connectionState='closed';};
 const transport=new WebRTCKernelTransport({wireFormat:'k2-fixture',channels:pair.client,peer,hello:offer('instance'),verifyPackage:()=>true});
 try{
  await transport.connect({instanceId:'instance',userId:'alice'},{link:value=>links.push(value),session(){}});
  assert.equal(links.at(-1).state,'online');assert.ok(Object.values(pair.client).every(c=>c.readyState==='open'));
  peer.connectionState='disconnected';peer.dispatchEvent(new Event('connectionstatechange'));
  assert.equal(links.at(-1).state,'offline','an unusable peer must not remain online until SCTP close');
  assert.equal(transport.closed,true,'retire the failed association before resuming');
 }finally{transport.close();native.close();}
});
