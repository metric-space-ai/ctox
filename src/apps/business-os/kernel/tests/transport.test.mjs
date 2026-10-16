import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../kernel.mjs';
import {KernelTransport} from '../transport.mjs';
import {testLocks} from './web-locks.mjs';
const tick=()=>new Promise(r=>setTimeout(r,10));

class CapturingTransport extends KernelTransport {
 async connect(identity,events){this.identity=identity;this.events=events;events.link({state:'online',rtt_ms:1,relay:false});}
 async subscribe(sub){(this.subs??=[]).push(sub);}
 async write(entry){(this.sent??=[]).push(entry);return undefined;}
}
test('queries remain local; transport gets resumable subscriptions and chained writes',async()=>{
 const transport=new CapturingTransport(),k=createKernel({instanceId:`captured-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,transport});await k.ready;
 const api=k.appApi('notes','1');let emissions=0;const off=api.query('notes',{}).subscribe(()=>emissions++);
 const first=api.put('notes',{id:'a',n:1});await first.local;
 const second=api.patch('notes','a',{n:2});await second.local;
 for(let i=0;i<20 && !transport.sent?.length;i++)await tick();
 assert.equal(transport.sent.length,1);assert.equal(transport.sent[0].op_id,first.op_id);assert.equal(transport.sent[0].base_seq,'0');assert.equal(transport.sent[0].schema_ver,1);
 assert.ok(Object.isFrozen(api.query('notes',{}).get()));
 await transport.events.write_result({op_id:first.op_id,status:'accepted',seq:'1',rev:'r1'});await first.result;
 for(let i=0;i<20 && transport.sent.length<2;i++)await tick();
 assert.equal(transport.sent[1].prev_op_id,first.op_id);
 await transport.events.write_result({op_id:second.op_id,status:'accepted',seq:'2',rev:'r2'});await second.result;
 assert.equal(api.get('notes','a').get().n,2);assert.ok(emissions>=2);off();await k.close();
});
test('reader recovers a dropped result event from durable meta.local_seq',async()=>{
 const transport=new CapturingTransport();const opts={instanceId:`lost-result-${process.pid}`,userId:'alice',storage:'memory',locks:testLocks};
 const leader=createKernel({...opts,transport});await leader.ready;
 const reader=createKernel({...opts,transport:new CapturingTransport()});await reader.ready;
 reader.sim.dropBroadcasts(true);
 const h=reader.appApi('notes','1').put('notes',{id:'a'});await h.local;
 for(let i=0;i<300 && !transport.sent?.length;i++)await tick();
 // A dropped outbox hint is repaired on leader's periodic local scan, not network polling.
 assert.equal(transport.sent?.length,1,"the original write must be sent without a wake-up write");
 assert.equal(transport.sent[0].op_id,h.op_id);
 await transport.events.write_result({op_id:h.op_id,status:'accepted',seq:'1',rev:'r'});
 await reader.sim.reconcile();assert.equal((await h.result).status,'accepted');
 await leader.close();await reader.close();
});

test('backlogs sum across scopes and readers reconcile missed network state durably',async()=>{
 const transport=new CapturingTransport(),opts={instanceId:`metrics-${process.pid}`,userId:'alice',storage:'memory',locks:testLocks};
 const leader=createKernel({...opts,transport});await leader.ready;
 const reader=createKernel({...opts,transport:new CapturingTransport()});await reader.ready;
 try {
  reader.sim.dropBroadcasts(true);
  for(const [collection,backlog] of [['notes',7],['mail',3]])await transport.events.page({epoch:'1',scope_hash:'local',gen:1,collection,cursor_to:'1',complete:false,backlog,docs:[]});
  transport.events.link({state:'offline',rtt_ms:80,relay:true});
  for(let i=0;i<10 && reader.shell.metrics.get().backlog!==10;i++){await tick();await reader.sim.reconcile();}
  assert.equal(leader.shell.metrics.get().backlog,10);assert.equal(reader.shell.metrics.get().backlog,10);
  assert.equal(reader.shell.metrics.get().link.state,'offline');assert.equal(reader.shell.metrics.get().link.relay,true);
 } finally {await reader.close();await leader.close();}
});

test('reader upload completion survives missed broadcasts and file fetch uses the leader',async()=>{
 const {MemoryCoreTransport}=await import('../fake-core.mjs');
 const namespace=`files-tabs-${process.pid}`,opts={instanceId:namespace,userId:'alice',storage:'memory',locks:testLocks,latencyMs:5};
 const leaderTransport=new MemoryCoreTransport({namespace,latencyMs:5}),readerTransport=new MemoryCoreTransport({namespace,latencyMs:5});
 readerTransport.blob=()=>{throw Error('reader must not use a connection')};
 const leader=createKernel({...opts,transport:leaderTransport});await leader.ready;
 const reader=createKernel({...opts,transport:readerTransport});await reader.ready;
 try {
  reader.sim.dropBroadcasts(true);
  const file=reader.appApi('notes','1').files.put(new Blob(['reader durable input']),{name:'draft.txt'});await file.local;
  await leader.sim.settle();await reader.sim.reconcile();
  assert.equal((await file.result).status,'accepted');
  const ref=await file.ref;assert.equal((await reader.appApi('notes','1').files.get(ref)).size,20);
  reader.sim.dropBroadcasts(false);await leader.sim.evictBlobs();
  assert.equal(await (await reader.appApi('notes','1').files.get(ref)).text(),'reader durable input');
 } finally {await reader.close();await leader.close();}
});
test('reader reopening an evicted collection requests normal P1 catch-up from the leader',async()=>{
 const {MemoryCoreTransport}=await import('../fake-core.mjs');
 const namespace=`evicted-tabs-${process.pid}`,opts={instanceId:namespace,userId:'alice',storage:'memory',locks:testLocks,latencyMs:5};
 const leader=createKernel({...opts,transport:new MemoryCoreTransport({namespace,latencyMs:5})});await leader.ready;
 const reader=createKernel({...opts,transport:new MemoryCoreTransport({namespace,latencyMs:5})});await reader.ready;
 try {
  await leader.sim.push('notes',{id:'a',title:'normal stream'});await reader.sim.reconcile();
  await leader.evict('notes');await reader.sim.reconcile();
  const q=reader.appApi('notes','1').query('notes',{});
  for(let i=0;i<20 && !q.get().length;i++){await tick();await reader.sim.reconcile();}
  assert.equal(q.get()[0].title,'normal stream');
 } finally {await reader.close();await leader.close();}
});
