import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../../src/apps/business-os/kernel/kernel.mjs';
import {KernelTransport} from '../../src/apps/business-os/kernel/transport.mjs';
import {WebRTCKernelTransport} from '../../src/apps/business-os/kernel/transport-webrtc.mjs';
const deliver=async(transport,result)=>{const wire=new WebRTCKernelTransport({});wire.events=transport.events;await wire.apply({type:'write_result',...result},1);};
const until=async predicate=>{const end=Date.now()+3000;while(!predicate()){assert.ok(Date.now()<end,'dispatch deadline');await new Promise(r=>setTimeout(r,5));}};
class Capture extends KernelTransport{sent=[];async connect(_,events){this.events=events;events.link({state:'online'});}async subscribe(){}async write(entry){this.sent.push(entry);}}
const page=(seq,title)=>({collection:'notes',epoch:'1',scope_hash:'local',gen:1,cursor_to:seq,complete:true,docs:[{id:'a',data:{id:'a',title},seq,rev:'r'+seq}]});
test('merged results retain durable outbox until the canonical page, including reload and page-before-result',async()=>{
 const options={instanceId:`requires-page-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:5};let transport=new Capture(),kernel=createKernel({...options,transport});await kernel.ready;
 try{
  const handle=kernel.appApi('notes','1').put('notes',{id:'a',title:'optimistic'});await handle.local;await until(()=>transport.sent.length===1);
  const result={op_id:handle.op_id,client_seq:'1',status:'merged',requires_page:true,rev:'r5',seq:'5'};
  let resolved=false;handle.result.then(()=>resolved=true);
  await deliver(transport,result);await Promise.resolve();assert.equal(resolved,false,'result must wait for its authoritative page');
  assert.equal((await kernel.sim.outbox())[0].pending_result.seq,'5');
  await kernel.receivePage(page('5','server wins'));assert.equal((await handle.result).status,'accepted');
  assert.equal(kernel.appApi('notes','1').get('notes','a').get().title,'server wins');assert.deepEqual(await kernel.sim.outbox(),[]);
  const next=kernel.appApi('notes','1').patch('notes','a',{title:'second optimistic'});await next.local;await until(()=>transport.sent.length===2);
  await deliver(transport,{op_id:next.op_id,client_seq:'2',status:'migrated',requires_page:true,rev:'r6',seq:'6'});
  await kernel.close();transport=new Capture();kernel=createKernel({...options,transport});await kernel.ready;
  assert.equal((await kernel.sim.outbox())[0].pending_result.seq,'6');
  await kernel.receivePage(page('6','canonical after reload'));assert.equal(kernel.appApi('notes','1').get('notes','a').get().title,'canonical after reload');assert.deepEqual(await kernel.sim.outbox(),[]);
  const third=kernel.appApi('notes','1').patch('notes','a',{title:'must not overwrite canonical'});await third.local;await until(()=>transport.sent.some(e=>e.op_id===third.op_id));
  await kernel.receivePage(page('7','page arrived first'));
  await deliver(transport,{op_id:third.op_id,client_seq:'3',status:'merged',requires_page:true,rev:'r7',seq:'7'});
  assert.equal((await third.result).status,'accepted');assert.equal(kernel.appApi('notes','1').get('notes','a').get().title,'page arrived first');
 }finally{await kernel.close();}
});
