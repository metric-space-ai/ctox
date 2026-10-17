import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../kernel.mjs';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {protocolChannels,FakeNativePeer,offer,scopeFor} from './fake-native-peer.mjs';
test('E47 device identity comes from authenticated device_pairing_id, not user or tab',async t=>{
 const pair=protocolChannels(),peer=new FakeNativePeer(pair.server,{instanceId:'device-instance'});t.after(()=>peer.close());
 const hello=offer('device-instance');hello.device_proof.device_id='paired-browser-42';
 const transport=new WebRTCKernelTransport({channels:pair.client,hello,resolveScope:scopeFor,verifyPackage:async()=>true});
 const kernel=createKernel({instanceId:'device-instance',userId:'alice',storage:'memory',multiTab:false,transport});t.after(()=>kernel.close());
 await kernel.ready;await transport.ready;assert.equal(kernel.identity.device_id,'paired-browser-42');assert.equal(kernel.shell.identity.device_id,'paired-browser-42');
});
