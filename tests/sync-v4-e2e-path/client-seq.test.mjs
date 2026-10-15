import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../../src/apps/business-os/kernel/kernel.mjs';
import {KernelTransport} from '../../src/apps/business-os/kernel/transport.mjs';
import {WebRTCKernelTransport} from '../../src/apps/business-os/kernel/transport-webrtc.mjs';

const until = async predicate => { const end=Date.now()+3000; while(!predicate()){assert.ok(Date.now()<end,'dispatch deadline');await new Promise(r=>setTimeout(r,5));} };
class Capture extends KernelTransport {
 constructor(online=false){super();this.online=online;this.sent=[];}
 async connect(identity,events){this.events=events;if(this.online)events.link({state:'online'});}
 async subscribe(){}
 async write(entry){this.sent.push(entry);}
}
test('one durable ordering survives reload and leader generation; cancelled unsent input leaves no gap',async()=>{
 const options={instanceId:`client-seq-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:5};
 const firstTransport=new Capture();let kernel=createKernel({...options,transport:firstTransport});await kernel.ready;
 try {
  const api=kernel.appApi('notes','1');
  const first=api.put('notes',{id:'a',value:1});await first.local;
  const second=api.patch('notes','a',{value:2});await second.local;
  const command=api.command('bench.read',{id:'a'});await command.local;
  const commandOp=(await kernel.sim.outbox()).find(e=>e.id===command.id).op_id;
  const cancelled=api.put('notes',{id:'cancelled'});await cancelled.local;
  assert.equal(cancelled.cancel(),true);assert.equal((await cancelled.result).reason,'cancelled');
  firstTransport.events.link({state:'online'});await until(()=>firstTransport.sent.length===2);
  const before=(await kernel.sim.outbox()).sort((a,b)=>Number(BigInt(a.client_seq)-BigInt(b.client_seq)));
  assert.deepEqual(before.map(e=>[e.op_id,e.client_seq]),[[first.op_id,'1'],[second.op_id,'2'],[commandOp,'3']]);
  assert.equal(second.cancel(),false,'reserved sequence cannot be deleted and leave a core gap');
  const oldGen=kernel.leader.get().leader_gen;await kernel.close();
  const nextTransport=new Capture(true);kernel=createKernel({...options,transport:nextTransport});await kernel.ready;
  await until(()=>nextTransport.sent.length===2);
  assert.ok(kernel.leader.get().leader_gen>oldGen);
  assert.deepEqual(nextTransport.sent.map(e=>[e.op_id,e.client_seq]).sort(),firstTransport.sent.map(e=>[e.op_id,e.client_seq]).sort(),'reload must resend stable op_id/client_seq');
  const result=e=>({op_id:e.op_id,status:'accepted',rev:`r${e.client_seq}`,seq:e.client_seq});
  await nextTransport.events.write_result(result(before[0]));await until(()=>nextTransport.sent.length===3);
  assert.equal(nextTransport.sent.find(e=>e.op_id===second.op_id).client_seq,'2');
  await nextTransport.events.write_result(result(before[1]));await nextTransport.events.write_result(result(before[2]));
  const later=kernel.appApi('notes','1').put('notes',{id:'after-reload'});await later.local;
  await until(()=>nextTransport.sent.some(e=>e.op_id===later.op_id));
  assert.equal(nextTransport.sent.find(e=>e.op_id===later.op_id).client_seq,'4','high-water mark survives an empty outbox');
  // Capture the actual adapter boundary: the scheduler does not manufacture
  // fields, so removing client_seq from the native write envelope must fail.
  const wire=[],adapter=new WebRTCKernelTransport({});adapter.ready.catch(()=>{});adapter.ready=Promise.resolve();adapter.scheduler={async send(lane,message){wire.push({lane,message});},close(){}};
  try {
   await adapter.write({...nextTransport.sent.at(-1),client_seq:'9007199254740993'});
   assert.equal(wire[0].lane,'commands');assert.equal(wire[0].message.client_seq,'9007199254740993');
   await assert.rejects(adapter.write({...nextTransport.sent.at(-1),client_seq:1}),e=>e.code==='invalid');
  } finally {adapter.close();}
  // Keep seq 4 pending: later independent documents may fill, never exceed,
  // the core's 64-position gap window while an earlier result is missing.
  for(let i=0;i<100;i++)await kernel.appApi('notes','1').put('notes',{id:`window-${i}`}).local;
  await until(()=>nextTransport.sent.length>=67);await new Promise(r=>setTimeout(r,15));
  assert.equal(nextTransport.sent.length,67,'three retired writes plus at most 64 outstanding positions');
  assert.equal(nextTransport.sent.at(-1).client_seq,'67');
 } finally {await kernel.close();}
});
