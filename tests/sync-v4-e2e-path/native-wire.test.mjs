import {test} from 'node:test';
import assert from 'node:assert/strict';
import {NativeWire,ReliableClientSession} from '../../src/apps/business-os/kernel/transport-webrtc.mjs';
import {protocolChannels} from '../../src/apps/business-os/kernel/tests/fake-native-peer.mjs';
test('E14 replay keeps per-lane ID and cumulative ACK; delivered duplicates have no second effect',async()=>{
 const pair=protocolChannels(),state=new ReliableClientSession('same-session'),remote=new ReliableClientSession('same-session');state.attach(remote.hello());remote.attach(state.hello());let effects=0,drop=true,captured;
 const errors=[],sender=new NativeWire(pair.client,{state,apply(){},onError:e=>errors.push(e)}),receiver=new NativeWire(pair.server,{state:remote,apply:async()=>{effects++;},onError:e=>errors.push(e)});
 for(const [lane,channel]of Object.entries(pair.client))channel.addEventListener('message',e=>sender.push(Number(lane),e.data).catch(e=>errors.push(e)));
 for(const [lane,channel]of Object.entries(pair.server))channel.addEventListener('message',e=>{if(Number(lane)===1)captured=e.data;if(!drop)receiver.push(Number(lane),e.data).catch(e=>errors.push(e));});
 try{
  await sender.send('commands',{type:'write',op_id:'one'});assert.equal(state.retained[1].size,1);drop=false;
  await sender.replay();await new Promise(r=>setTimeout(r,20));assert.equal(effects,1);assert.equal(state.retained[1].size,0);
  await receiver.push(1,captured);assert.equal(effects,1,'delivered replay must not apply twice');
  await sender.send('commands',{type:'write',op_id:'two'});await new Promise(r=>setTimeout(r,20));assert.equal(effects,2);assert.equal(state.sent[1],2n);
  assert.throws(()=>state.ack(1,'3'));assert.deepEqual(errors,[]);
 }finally{sender.close();receiver.close();}
});
