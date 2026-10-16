import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createKernel} from '../kernel.mjs';
import {KernelTransport} from '../transport.mjs';
import {observeCommand} from '../command-observer.mjs';
import {testLocks} from './web-locks.mjs';
let serial=0;
const until=async fn=>{const end=Date.now()+2500;while(!await fn()){assert.ok(Date.now()<end,'kernel dispatch deadline');await new Promise(r=>setTimeout(r,2));}};
class Capture extends KernelTransport {
 constructor(online=true){super();this.online=online;this.sent=[];this.subs=[];}
 async connect(_,events){this.events=events;events.link({state:this.online?'online':'offline',rtt_ms:1,relay:false});}
 async subscribe(scope){this.subs.push(scope);}
 async write(entry){this.sent.push(entry);}
}
async function setup(t,transport=new Capture(),instanceId='s9-'+process.pid+'-'+(++serial)){
 const options={instanceId,userId:'alice',storage:'memory',multiTab:false,locks:testLocks,latencyMs:0,transport};
 const kernel=createKernel(options);await kernel.ready;t.after(()=>kernel.close());
 await until(()=>transport.events);
 const manifest={collections:{},foreign_collections:{business_commands:{owner_app:'ctox-platform',ops:['read']},ctox_queue_tasks:{owner_app:'ctox-platform',ops:['read']}}};
 await transport.events.page({collection:'business_app_versions',epoch:'1',scope_hash:'local',gen:1,cursor_to:'1',head_seq:'1',complete:true,docs:[{id:'probe@1',seq:'1',rev:'fixture-app-1',data:{id:'probe@1',app_id:'probe',app_ver:'1',manifest}}]});
 const api=kernel.appApi('probe','1');
 const receipt=async(entry,status='accepted',extra={})=>transport.events.write_result({op_id:entry.op_id,status,seq:'10',rev:'native-10',gen:1,...extra});
 const page=async(doc,seq='11')=>{const {_rev,_pending,...canonical}=doc;return transport.events.page({collection:'business_commands',epoch:'1',scope_hash:'local',gen:1,cursor_to:seq,head_seq:seq,complete:true,docs:[{id:doc.id,data:canonical,seq,rev:'native-'+seq}]});};
 return {kernel,api,transport,options,receipt,page};
}

test('s9 projection: authoritative terminal command needs no queue projection; failed and replayed terminal cannot become success',async t=>{
 const {kernel,api,transport,receipt,page}=await setup(t);
 for(const outcome of ['done','failed']){
  const h=api.command('probe.control',{outcome}),o=observeCommand(h,{timeoutMs:2000});await o.local;await until(()=>transport.sent.some(e=>e.id===h.id));
  const e=transport.sent.find(e=>e.id===h.id),doc=api.get('business_commands',h.id).get();
  await receipt(e);assert.equal((await o.accepted).status,'accepted');
  assert.equal(h.status.get().status,'queued','write acceptance is not handler completion');
  await page({...doc,status:outcome,...(outcome==='done'?{result:{ok:true,task_id:''}}:{error:'native_failed'})},outcome==='done'?'11':'13');
  if(outcome==='done')assert.equal((await o.done).status,'done');else await assert.rejects(o.done,e=>e.code==='unavailable'&&e.reason==='failed'&&e.detail.command_id===h.id&&e.detail.error==='native_failed');
  await page({...doc,status:'running'},outcome==='done'?'12':'14');
  assert.equal(h.status.get().status,outcome,'newer sequence cannot regress terminal status');
  assert.equal(api.query('ctox_queue_tasks',{}).get().length,0,'no synthetic queue task is required');
 }
 assert.ok(kernel.status.forApp('kernel').get().some(e=>e.code==='command.invalid_transition'));
 assert.ok(!transport.subs.some(s=>s.collection==='desktop_file_chunks'),'a control command does not wake file upload dependencies');
});

test('s9 atomic: local commit survives missing ack, deadline and reload with the same operation and no redispatch',async t=>{
 const a=await setup(t);const h=a.api.command('probe.once',{saved:true}),observer=observeCommand(h,{timeoutMs:40});await observer.local;
 await until(()=>a.transport.sent.length===1);const original=a.transport.sent[0];
 await assert.rejects(observer.done,e=>e.code==='unavailable');
 assert.equal((await a.kernel.sim.outbox()).length,1);assert.equal(a.api.get('business_commands',h.id).get().payload.saved,true);
 assert.equal(a.transport.sent.length,1,'deadline must never submit a new intent');
 await a.kernel.close();const b=await setup(t,new Capture(),a.options.instanceId);await until(()=>b.transport.sent.length===1);
 assert.equal(b.transport.sent[0].id,h.id);assert.equal(b.transport.sent[0].op_id,original.op_id);assert.equal(b.transport.sent[0].client_seq,original.client_seq);
 await b.receipt(b.transport.sent[0]);assert.equal((await b.kernel.sim.outbox()).length,0);
 const rejected=b.api.command('probe.denied',{});await rejected.local;await until(()=>b.transport.sent.length===2);
 await b.receipt(b.transport.sent[1],'rejected',{reason:'revoked',seq:undefined,rev:undefined});
 assert.equal((await rejected.result).status,'rejected');assert.ok((await b.kernel.sim.archiveLocal()).some(e=>e.id===rejected.id));
 assert.equal(b.transport.sent.length,2,'terminal refusal is not silently retried');
});

test('s9 timing: measured intervals and command IDs stay exact; missing stages and payload never leak into diagnostics',async t=>{
 const {api,transport,receipt,page}=await setup(t);let tick=10;const observed=[];
 const handles=[api.command('probe.a',{secret:'never-in-diagnostics'}),api.command('probe.b',{})];
 const observers=handles.map(h=>observeCommand(h,{timeoutMs:2000,now:()=>++tick,onTiming:sample=>{observed.push(sample);throw Error('diagnostic observer');}}));
 await Promise.all(observers.map(o=>o.local));await until(()=>transport.sent.length===2);
 for(let i=0;i<handles.length;i++){
  const h=handles[i],o=observers[i];assert.equal(Object.hasOwn(o.snapshot().stagesMs,'native_acceptance'),false);
  await receipt(transport.sent.find(e=>e.id===h.id));await o.accepted;
  await page({...api.get('business_commands',h.id).get(),status:'done',result:{ok:true}},String(20+i));assert.equal((await o.done).status,'done');
  const sample=o.snapshot();assert.equal(sample.commandId,h.id);assert.ok(sample.stagesMs.local_persistence>=0);assert.ok(sample.stagesMs.native_acceptance>=0);assert.ok(sample.stagesMs.total>=0);
  assert.equal(Object.hasOwn(sample.stagesMs,'native_processing'),false,'no unmeasured Core processing interval');
 }
 assert.equal(new Set(observed.map(s=>s.commandId)).size,2);assert.ok(!JSON.stringify(observed).includes('never-in-diagnostics'));
});

test('s9 health: green link without progress reports a typed bounded failure and retains intent; flowing commands do not alarm',async t=>{
 const {kernel,api,transport,receipt,page}=await setup(t);const reports=[];
 const h=api.command('probe.suppressed',{}),o=observeCommand(h,{timeoutMs:40,report:r=>reports.push(r)});await o.local;
 await assert.rejects(o.done,e=>e.code==='unavailable');assert.equal(kernel.shell.metrics.get().link.state,'online');
 assert.equal(reports.length,1);assert.equal(reports[0].code,'command.no_progress');assert.equal(reports[0].ref,h.id);
 assert.equal((await kernel.sim.outbox()).length,1);assert.equal(h.status.get().status,'queued');assert.equal(transport.sent.length,1);
 await receipt(transport.sent[0]);await page({...api.get('business_commands',h.id).get(),status:'done',result:{ok:true}});
 const resumed=observeCommand(h,{timeoutMs:1000,report:r=>reports.push(r)});assert.equal((await resumed.done).status,'done');assert.equal(reports.length,1,'existing terminal result needs no repair or new command');
 const healthy=api.command('probe.flow',{}),good=observeCommand(healthy,{timeoutMs:1000,report:r=>reports.push(r)});await good.local;await until(()=>transport.sent.length===2);
 await receipt(transport.sent[1]);await page({...api.get('business_commands',healthy.id).get(),status:'done',result:{ok:true}},'12');await good.done;assert.equal(reports.length,1);
});


test('s9 projection: canonical progress survives a lost write receipt without a queue or changed intent', {timeout:4000},async t=>{
 const {kernel,api,transport,page}=await setup(t);const h=api.command('probe.lost-ack',{n:1});
 const observer=observeCommand(h,{timeoutMs:1000});await observer.local;await until(()=>transport.sent.length===1);
 const original=api.get('business_commands',h.id).get();
 await page({...original,status:'done',result:{ok:true}});
 assert.equal((await observer.done).status,'done','canonical terminal page is sufficient even while its write_result is missing');
 assert.equal((await kernel.sim.outbox()).length,1,'retain the exact op for later result/replay');assert.equal(transport.sent.length,1);
 assert.equal(Object.hasOwn(observer.snapshot().stagesMs,'native_acceptance'),false,'missing ACK has no manufactured timing');
 await page({...original,status:'done',payload:{n:2},result:{ok:false}},'12');
 assert.deepEqual(api.get('business_commands',h.id).get().payload,{n:1},'different intent cannot borrow a terminal receipt');
 observer.close();
});
