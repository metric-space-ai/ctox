import {test} from 'node:test';
import assert from 'node:assert/strict';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';
import {protocolChannels,FakeNativePeer,offer} from './fake-native-peer.mjs';
import {sha256Hex} from '../values.mjs';
test('E26 remote range uses aligned transfer chunks and rejects corrupt per-chunk hash',async()=>{
 const pair=protocolChannels(),peer=new FakeNativePeer(pair.server),transport=new WebRTCKernelTransport({wireFormat:'k2-fixture',channels:pair.client,hello:offer('instance'),verifyPackage:async()=>true});
 try{
  await transport.connect({instanceId:'instance',userId:'alice'},{link(){},page(){},write_result(){}});
  const bytes=Uint8Array.from({length:210000},(_,i)=>i%251),blob=new Blob([bytes]),ref=await sha256Hex(blob);peer.blobs.set(ref,blob);
  const result=await transport.blob(ref,{start:65530,end:131080});
  assert.deepEqual(new Uint8Array(await result.arrayBuffer()),bytes.slice(65530,131080));
  assert.deepEqual(peer.messages.filter(m=>m.type==='transfer_get').map(m=>[m.offset,m.limit]).sort((a,b)=>a[0]-b[0]),[[0,65536],[65536,65536],[131072,65536]]);
  const send=peer.send.bind(peer);peer.send=(lane,m)=>send(lane,m.type==='transfer_data'?{...m,sha256:'0'.repeat(64)}:m);
  await assert.rejects(transport.blob(ref,{start:65530,end:65545}),e=>e.code==='invalid'&&/chunk hash/.test(e.message));
  await assert.rejects(transport.blob(ref,{start:0,end:bytes.length+1}),e=>e.code==='invalid'&&e.path==='range');
  assert.deepEqual(peer.errors,[]);
 }finally{transport.close();peer.close();}
});
