import {installFixtureApp} from './platform-fixture.mjs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createStubKernel} from '../testkit.mjs';
let next=0;
const routes=k=>{for(const type of ['notes.run','other.run'])k.sim.onCommand(type,()=>({ok:true}));};
function waitCommandDone(handle){return new Promise((resolve,reject)=>{
 let off;const timer=setTimeout(()=>{off?.();reject(Error('command completion deadline'));},3000);
 off=handle.status.subscribe(state=>{if(state.status==='done'){clearTimeout(timer);queueMicrotask(()=>off?.());resolve(state);}else if(['failed','rejected'].includes(state.status)){clearTimeout(timer);queueMicrotask(()=>off?.());reject(Error(JSON.stringify(state)));}});
});}
test('E26 idempotency: SHA UUID, one durable intent across repeat and reload',async()=>{
 const opts={instanceId:`e26-${process.pid}-${++next}`,userId:'u1',latencyMs:2,multiTab:false};
 let k=createStubKernel(opts);await k.ready;routes(k);await installFixtureApp(k,'notes',{platform:['business_commands']});
 try{
  await k.sim.setLink({state:'offline'});
  const api=k.appApi('notes','1'),h=api.command('notes.run',{original:true},{idempotency_key:'persist'});
  const repeat=api.command('notes.changed',{original:false},{idempotency_key:'persist'});
  await Promise.all([h.local,repeat.local]);
  const hash=createHash('sha256').update('notes|u1|persist').digest('hex');
  const expected=`${hash.slice(0,8)}-${hash.slice(8,12)}-8${hash.slice(13,16)}-${((parseInt(hash[16],16)&3)|8).toString(16)}${hash.slice(17,20)}-${hash.slice(20,32)}`;
  assert.equal(h.id,expected);assert.equal(repeat.id,h.id);
  let resolved=false;repeat.result.then(()=>resolved=true);await Promise.resolve();assert.equal(resolved,false,'offline result remains pending');
  const otherApp=k.appApi('other','1').command('other.run',{}, {idempotency_key:'persist'});await otherApp.local;assert.notEqual(otherApp.id,h.id);
  const otherUser=createStubKernel({...opts,userId:'u2'});await otherUser.ready;try{await otherUser.sim.setLink({state:'offline'});const u=otherUser.appApi('notes','1').command('notes.run',{}, {idempotency_key:'persist'});await u.local;assert.notEqual(u.id,h.id);}finally{await otherUser.close();}
  assert.equal((await k.sim.outbox()).filter(e=>e.id===h.id).length,1);
  assert.equal(repeat.cancel(),false,'reused handle cannot claim to cancel a new operation');
  assert.deepEqual(api.get('business_commands',h.id).get().payload,{original:true});
  for(const key of [null,7,'x'.repeat(129)])assert.throws(()=>api.command('notes.run',{}, {idempotency_key:key}),e=>e.code==='invalid'&&e.path==='idempotency_key');
  assert.notEqual(api.command('notes.run',{}).id,api.command('notes.run',{}).id);
  const id=h.id;await k.close();k=createStubKernel(opts);await k.ready;routes(k);await k.sim.setLink({state:'offline'});
  const resumed=k.appApi('notes','2').command('notes.changed',{}, {idempotency_key:'persist'});await resumed.local;
  assert.equal(resumed.id,id);assert.equal((await k.sim.outbox()).filter(e=>e.id===id).length,1);
  await k.sim.setLink({state:'online'});await k.sim.settle();assert.equal((await resumed.result).status,'accepted');await waitCommandDone(resumed);
  const before=k.appApi('notes','1').get('business_commands',id).get();assert.equal(before.status,'done');
  const completed=k.appApi('notes','1').command('notes.run',{}, {idempotency_key:'persist'});assert.equal(completed.cancel(),false);await completed.local;await completed.result;
  assert.deepEqual(k.appApi('notes','1').get('business_commands',id).get(),before);
  assert.deepEqual(completed.status.get(),resumed.status.get());
 }finally{await k.close();}
});


test('E26 byte range crosses CAS chunk boundary; end exclusive and typed bounds',async()=>{
 const k=createStubKernel({instanceId:`range-${process.pid}-${++next}`,userId:'u1',latencyMs:2,multiTab:false});await k.ready;
 try{
  const api=k.appApi('notes','1'),data=Uint8Array.from({length:140000},(_,i)=>i%251);
  const file=await api.files.put(data,{name:'large.bin'});await k.sim.settle();
  for(const cold of [false,true]){
   if(cold)await k.sim.evictBlobs();
   const result=await api.files.get({id:file.id,range:{start:65530,end:65545}});
   assert.deepEqual(result.bytes,data.slice(65530,65545));assert.equal(result.text,undefined);assert.equal(result.size,data.length);
  }
  for(const range of [{start:0,end:data.length+1},{start:-1,end:1},{start:2,end:1},{start:0.5,end:2},null])await assert.rejects(api.files.get({id:file.id,range}),e=>e.code==='invalid'&&e.path==='range');
  assert.equal((await api.files.get({id:file.id,range:{start:data.length,end:data.length}})).bytes.length,0);
  const text=await api.files.put({text:'unchanged'});assert.equal((await api.files.get({id:text.id})).text,'unchanged');
 }finally{await k.close();}
});
