import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../../src/apps/business-os/kernel/kernel.mjs';
import {KernelTransport} from '../../src/apps/business-os/kernel/transport.mjs';
import {openBackend} from '../../src/apps/business-os/kernel/store-idb-v5.mjs';

test('stale leader sends nothing after a generation change before dispatch',async()=>{
 let sent=0;
 class Transport extends KernelTransport {
  async connect(identity,events){this.events=events;events.link({state:'offline',rtt_ms:0,relay:false});}
  async subscribe(){}
  async write(){sent++;}
 }
 const transport=new Transport(),kernel=createKernel({instanceId:`fenced-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:20,transport});
 await kernel.ready;
 try {
  const handle=kernel.appApi('notes','1').put('notes',{id:'must-stay-local'});await handle.local;
  transport.events.link({state:'online',rtt_ms:0,relay:false});
  const db=await openBackend(kernel.dbName,'memory');
  // Finish the independent link transaction before changing the generation:
  // its guard must not accidentally kill the dispatch mutation for us.
  assert.equal((await db.get('meta','network')).value.link.state,'online');
  await kernel.sim.bumpLeaderGen();

  // Observe the fencing outcome, not an arbitrary delay before the assertion.
  const deadline=Date.now()+3000;
  while(kernel.isLeader){assert.ok(Date.now()<deadline,"stale leader must step down");await new Promise(resolve=>setTimeout(resolve,5));}
  assert.equal(sent,0,'the old generation must have no transport effect');
  assert.equal((await kernel.sim.outbox())[0].sent_gen,undefined,'dispatch marker must roll back');
 } finally {await kernel.close();}
});
