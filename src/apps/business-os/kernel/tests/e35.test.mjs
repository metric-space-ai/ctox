import {testLocks} from './web-locks.mjs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../testkit.mjs';
const schema={schema:{type:'object',properties:{id:{type:'string'}}}};
let serial=0;
async function open(t,opts={}) {
 const options={instanceId:`e35-${process.pid}-${++serial}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:0,...opts};
 const k=createStubKernel(options);t.after(()=>k.close());await k.ready;return {k,options};
}
test('E35 foreign query requires declaration AND current owner read export, including retained handles',async t=>{
 const {k}=await open(t);
 await k.sim.publishAppVersion('owner','1',{collections:{shared_rows:schema},manifestExtra:{exports:{shared_rows:['read','write']}}});
 await k.sim.publishAppVersion('reader','1',{collections:{own_rows:schema},manifestExtra:{foreign_collections:{shared_rows:{owner_app:'owner',ops:['read']}}}});
 await k.sim.publishAppVersion('undeclared','1',{collections:{private_rows:schema}});
 await k.sim.publishAppVersion('writeonly','1',{manifestExtra:{foreign_collections:{shared_rows:{owner_app:'owner',ops:['write']}}}});
 await k.sim.push('shared_rows',[{id:'secret',value:42}]);
 const reader=k.appApi('reader','1'),live=reader.query('shared_rows',{});
 assert.throws(()=>reader.query('not_declared_yet',{}),e=>e.code==='forbidden','no permissive window before owner metadata arrives');
 const seen=[];const off=live.subscribe(rows=>seen.push(rows));t.after(off);
 assert.equal(live.get()[0].value,42);assert.equal(reader.get('shared_rows','secret').get().value,42);
 for(const app of ['undeclared','writeonly','shell']) {
  assert.throws(()=>k.appApi(app,'1').query('shared_rows',{}),e=>e.code==='forbidden');
  assert.throws(()=>k.appApi(app,'1').get('shared_rows','secret'),e=>e.code==='forbidden');
 }
 await k.sim.publishAppVersion('owner','2',{collections:{shared_rows:schema}});
 assert.throws(()=>reader.query('shared_rows',{}),e=>e.code==='forbidden');
 assert.throws(()=>live.get(),e=>e.code==='forbidden');
 assert.deepEqual(seen.at(-1),[],'revocation clears subscribed rows without an uncaught refresh error');
 await k.sim.setLink({state:'offline'});
 const spy=k.appApi('undeclared','1'),catalog=(await k.sim.inspect('business_app_versions','owner@2')).data;
 const forged=spy.put('business_app_versions',{...catalog,manifest:{...catalog.manifest,exports:{shared_rows:['read']}}});await forged.local;
 assert.throws(()=>reader.query('shared_rows',{}),e=>e.code==='forbidden','optimistic owner export cannot mint a grant');
});
test('E35 desktop range validates against verified CAS size, never row size_bytes',async t=>{
 const {k}=await open(t);
 await k.sim.publishAppVersion('desktop','1',{collections:{desktop_files:schema},manifestExtra:{exports:{desktop_files:['read']}}});
 await k.sim.publishAppVersion('reader','1',{manifestExtra:{foreign_collections:{desktop_files:{owner_app:'desktop',ops:['read']}}}});
 await k.sim.publishAppVersion('vault','1',{collections:{vault_refs:schema}});
 const file=await k.appApi('vault','1').files.put({text:'actual bytes'},{id:'cas-file'});await file.done;
 const api=k.appApi('reader','1');
 for (const size_bytes of [9999,1]) {
  await k.sim.push('desktop_files',[{id:'desktop-id',content_ref:file.ref,size_bytes}]);
  const result=await api.files.get({id:'desktop-id',range:{start:2,end:8}});
  assert.equal(result.size,12);assert.equal(new TextDecoder().decode(result.bytes),'tual b');
  await assert.rejects(api.files.get({id:'desktop-id',range:{start:0,end:13}}),e=>e.code==='invalid'&&e.path==='range');
 }
 await k.sim.evictBlobs();
 const cold=await api.files.get({id:'desktop-id',range:{start:11,end:12}});assert.equal(cold.size,12);assert.deepEqual([...cold.bytes],[115]);
});
test('E35 patch expectation is checked by the writer after local ack, not against optimistic state',async t=>{
 const {k}=await open(t);await k.sim.publishAppVersion('notes','1',{collections:{notes:schema}});const api=k.appApi('notes','1');
 const first=api.put('notes',{id:'n',value:0,kept:true});await first.local;const initial=await first.result;
 await k.sim.setLink({state:'offline'});
 const a=api.patch('notes','n',{value:1},{expected_revision:initial.rev});
 const b=api.patch('notes','n',{value:2},{expected_revision:initial.rev});
 await Promise.all([a.local,b.local]);
 const entries=await k.sim.outbox();assert.equal(entries.find(e=>e.op_id===b.op_id).expected_revision,initial.rev);
 await k.sim.setLink({state:'online'});
 assert.notEqual((await a.result).status,'rejected');const rejected=await b.result;
 assert.equal(rejected.status,'rejected');assert.equal(rejected.code,'revision_conflict');
 assert.equal(api.get('notes','n').get().value,1);
 const merge=api.patch('notes','n',{extra:true});await merge.local;assert.notEqual((await merge.result).status,'rejected');
 assert.equal(api.get('notes','n').get().extra,true);
 const invalid=api.patch('notes','n',{value:3},{expected_revision:5});await assert.rejects(invalid.local,e=>e.code==='invalid'&&e.path==='expected_revision');
});
test('E35 rejected file keeps metadata and done rejects with code/reason, also after reload',async t=>{
 const {k,options}=await open(t);
 await k.sim.publishAppVersion('ctox-platform','test-e35',{collections:{files:schema},manifestExtra:{exports:{files:['read']}}});
 await k.sim.publishAppVersion('notes','1',{manifestExtra:{foreign_collections:{files:{owner_app:'ctox-platform',ops:['read']}}}});
 await k.sim.setLink({state:'offline'});
 const api=k.appApi('notes','1'),handle=await api.files.put({text:'retained'},{id:'rejected-file',name:'receipt.txt'});
 assert.equal(typeof handle.done.then,'function');assert.deepEqual(structuredClone(handle),{id:handle.id,ref:handle.ref},'done does not break serializable file references');assert.equal(api.get('files',handle.id).get().name,'receipt.txt');
 k.sim.rejectNext({code:'forbidden',reason:'Native policy denies this upload'});
 const rejected=assert.rejects(handle.done,e=>e.code==='forbidden'&&e.reason==='Native policy denies this upload');
 await k.sim.setLink({state:'online'});await rejected;await k.sim.settle();
 const doc=api.get('files',handle.id).get();assert.ok(doc,'rejected file document must remain');assert.equal(doc.state,'rejected');assert.equal(doc.ref,handle.ref);assert.equal(doc.name,'receipt.txt');
 assert.equal((await api.files.get({id:handle.id})).text,'retained');
 await k.close();const reload=createStubKernel(options);t.after(()=>reload.close());await reload.ready;
 assert.equal(reload.appApi('notes','1').get('files',handle.id).get().state,'rejected');
 // The original Blob handle exposes the same done contract.
 await reload.sim.setLink({state:'offline'});const blob=reload.appApi('notes','1').files.put(new Blob(['blob']),{id:'blob-reject'});await blob.local;
 reload.sim.rejectNext({code:'quota',reason:'Quota denied'});const blobRejected=assert.rejects(blob.done,e=>e.code==='quota'&&e.reason==='Quota denied');
 await reload.sim.setLink({state:'online'});await blobRejected;
 const again=reload.appApi('notes','1');const original=await again.files.put({text:'original'},{id:'replace-file'});await original.done;
 await reload.sim.setLink({state:'offline'});const replacement=await again.files.put({text:'replacement'},{id:'replace-file'});
 reload.sim.rejectNext({code:'forbidden',reason:'Replacement denied'});const replacementRejected=assert.rejects(replacement.done,e=>e.code==='forbidden');
 await reload.sim.setLink({state:'online'});await replacementRejected;
 assert.equal(again.get('files','replace-file').get().state,'rejected');
 assert.equal((await again.files.get({id:'replace-file'})).text,'replacement','the real denied replacement remains readable from local bytes');
});

test('E35 explicit CAS rejection completes the file handle and preserves metadata despite accepted write receipt',async t=>{
 const {k}=await open(t);await k.sim.publishAppVersion('ctox-platform','test-e35',{collections:{files:schema},manifestExtra:{exports:{files:['read']}}});
 await k.sim.publishAppVersion('notes','1',{manifestExtra:{foreign_collections:{files:{owner_app:'ctox-platform',ops:['read']}}}});
 await k.sim.setLink({state:'offline'});const api=k.appApi('notes','1'),file=await api.files.put({text:'CAS rejected bytes'},{id:'cas-rejected'});
 k.sim.rejectNextUpload({code:'quota',reason:'CAS quota denied'});
 let timer;const deadline=new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('upload completion deadline')),2000);});t.after(()=>clearTimeout(timer));
 const rejected=assert.rejects(Promise.race([file.done,deadline]),e=>e.code==='quota'&&e.reason==='CAS quota denied');
 await k.sim.setLink({state:'online'});await rejected;clearTimeout(timer);await k.sim.settle();
 const row=await k.sim.inspect('files',file.id);assert.ok(row.base.data,'metadata write was accepted independently');assert.equal(row.data.state,'rejected');
 assert.equal((await k.sim.outbox()).filter(e=>e.op==='upload').length,0,'terminal rejection is not retried forever');
 assert.equal((await api.files.get({id:file.id})).text,'CAS rejected bytes');
 const retry=await api.files.put({text:'CAS rejected bytes'},{id:'cas-retry'});await retry.done;
 assert.notEqual(api.get('files',retry.id).get().state,'rejected','an old same-ref rejection cannot poison a new upload');
});

test('E35 file completion resumes when a reader takes over after lost broadcasts',async t=>{
 const options={instanceId:`e35-handoff-${process.pid}-${++serial}`,userId:'alice',storage:'memory',locks:testLocks,latencyMs:0};
 const leader=createStubKernel(options);await leader.ready;t.after(()=>leader.close());
 await leader.sim.publishAppVersion('ctox-platform','test-e35',{collections:{files:schema},manifestExtra:{exports:{files:['read']}}});
 await leader.sim.publishAppVersion('notes','1',{manifestExtra:{foreign_collections:{files:{owner_app:'ctox-platform',ops:['read']}}}});
 const reader=createStubKernel(options);await reader.ready;t.after(()=>reader.close());reader.sim.dropBroadcasts(true);
 const file=reader.appApi('notes','1').files.put(new Blob(['durable takeover bytes']),{id:'handoff-file'});await file.local;
 await leader.sim.settle();
 // No explicit reconcile: the new leader must recover durable terminal receipts
 // while taking over, before the ordinary 2-second reconciliation tick.
 let timer;const deadline=new Promise(resolve=>{timer=setTimeout(()=>resolve(null),1000);});t.after(()=>clearTimeout(timer));
 await leader.close();const result=await Promise.race([file.done,deadline]);clearTimeout(timer);assert.ok(result,'takeover must recover both durable receipts');
 assert.equal(result.status,'accepted');assert.equal((await reader.appApi('notes','1').files.get({id:'handoff-file'})).text,'durable takeover bytes');
});
