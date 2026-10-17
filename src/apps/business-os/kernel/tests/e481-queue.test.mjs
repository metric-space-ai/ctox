import {test} from 'node:test';
import assert from 'node:assert/strict';
import {WebRTCKernelTransport,NativeWire} from '../transport-webrtc.mjs';
import {SendScheduler} from '../scheduler.mjs';
import {decodeFrame} from '../framing.mjs';
const flush=async()=>{for(let i=0;i<30;i++)await Promise.resolve();};
test('THESEN timers start at SCTP handoff; local queue time never replaces a connection',async t=>{
 const timers=[];t.mock.method(globalThis,'setTimeout',(fn,ms)=>{const timer={fn,ms};timers.push(timer);return timer;});t.mock.method(globalThis,'clearTimeout',()=>{});
 for(const kind of ['transfer','grant']){
  const transport=new WebRTCKernelTransport({transferTimeoutMs:10,grantTimeoutMs:10,authSession:{acceptAppGrant:async()=>{},tokens:()=>[],close(){}}});transport.readyState.resolve();
  let send;transport.scheduler={send:()=>new Promise(r=>send=r),close(){}};transport.events={status(){},link(){}};
  const before=timers.length;const work=kind==='transfer'?transport.request('transfer_get',{ref:'x'}):transport.grantApp('notes');const rejected=assert.rejects(work,e=>e.code===(kind==='transfer'?'unavailable':'forbidden'));
  await flush();assert.equal(timers.length,before,'local queue must not start a response timer');assert.equal(transport.closed,false);
  send();await flush();assert.equal(timers.length,before+1);timers.at(-1).fn();await rejected;assert.equal(transport.closed,false,'request deadline must not replace the connection');transport.close();
 }
});
class Channel extends EventTarget{readyState='open';bufferedAmount=65536;frames=[];send(frame){this.frames.push(frame);}}
test('THESEN commands preempt an already queued bulk frame in both schedulers',async()=>{
 for(const native of [false,true]){
  const channel=new Channel(),errors=[];
  const scheduler=native?new NativeWire({0:channel},{apply:async()=>{},onError:e=>errors.push(e)}):new SendScheduler({0:channel},{codec:'identity'});
  const bulk=scheduler.send('transfer',{type:'bulk',value:'x'.repeat(100)});await flush();
  const command=scheduler.send('commands',{type:'write',id:'command'});await flush();assert.equal(channel.frames.length,0);
  const settled=Promise.all([bulk,command]);settled.catch(()=>{});
  try{
   channel.bufferedAmount=0;channel.dispatchEvent(new Event('bufferedamountlow'));await flush();
   assert.ok(channel.frames.length>0,'the drained association must send');
   const lane=native?channel.frames[0][4]:(await decodeFrame(channel.frames[0])).lane;assert.equal(lane,1,'commands cannot sit behind bulk');await settled;assert.deepEqual(errors,[]);
  }finally{scheduler.close();}
 }
});
