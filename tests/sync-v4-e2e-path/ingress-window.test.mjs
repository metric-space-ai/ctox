import {test} from 'node:test';
import assert from 'node:assert/strict';
import {NativeWire} from '../../src/apps/business-os/kernel/transport-webrtc.mjs';
import {protocolChannels} from '../../src/apps/business-os/kernel/tests/fake-native-peer.mjs';

test('legal live lane burst survives a slow consumer and remains bounded',async()=>{
 const pair=protocolChannels(),errors=[];let release,effects=0;
 const held=new Promise(resolve=>{release=resolve;});
 const wire=new NativeWire(pair.server,{apply:async()=>{await held;effects++;},onError:error=>errors.push(error)});
 try{
  const pending=[];
  for(let i=1;i<=600;i++){
   // Independent native raw CTX4 frame; this case tests ingress admission,
   // not agreement between two copies of the client's encoder.
   const payload=new TextEncoder().encode(JSON.stringify({type:'live',padding:'x'.repeat(1200)})),id=String(i),frame=new Uint8Array(37+payload.length),view=new DataView(frame.buffer);
   frame.set([67,84,88,52,3,0,id.length]);frame.set(new TextEncoder().encode(id),7);view.setUint32(31,1);view.setUint16(35,payload.length);frame.set(payload,37);
   pending.push(wire.push(3,frame));
  }
  const drained=Promise.all(pending);release();await drained;
  assert.equal(effects,600);assert.deepEqual(errors,[]);assert.equal(wire.incoming[3],0);
  await assert.rejects(wire.push(3,new Uint8Array(5*1024*1024)),error=>error.code==='quota','lane admission must stay bounded');
 }finally{release();wire.close();}
});
