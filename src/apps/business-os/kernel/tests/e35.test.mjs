import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../kernel.mjs';
const schema={schema:{type:'object',properties:{id:{type:'string'}}}};
let serial=0;
async function open(t,opts={}) {
 const options={instanceId:`e35-${process.pid}-${++serial}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:0,...opts};
 const k=createStubKernel(options);t.after(()=>k.close());await k.ready;return {k,options};
}
test('E35 foreign query requires declaration AND current owner read export, including retained handles',async t=>{
 const {k}=await open(t);
 await k.sim.publishAppVersion('owner','1',{collections:{shared_rows:schema},manifestExtra:{exports:{shared_rows:['read']}}});
 await k.sim.publishAppVersion('reader','1',{collections:{own_rows:schema},manifestExtra:{foreign_collections:{shared_rows:{owner_app:'owner',ops:['read']}}}});
 await k.sim.publishAppVersion('undeclared','1',{collections:{private_rows:schema}});
 await k.sim.publishAppVersion('writeonly','1',{manifestExtra:{foreign_collections:{shared_rows:{owner_app:'owner',ops:['write']}}}});
 await k.sim.push('shared_rows',[{id:'secret',value:42}]);
 const reader=k.appApi('reader','1'),live=reader.query('shared_rows',{});
 assert.equal(live.get()[0].value,42);assert.equal(reader.get('shared_rows','secret').get().value,42);
 for(const app of ['undeclared','writeonly','shell']) {
  assert.throws(()=>k.appApi(app,'1').query('shared_rows',{}),e=>e.code==='forbidden');
  assert.throws(()=>k.appApi(app,'1').get('shared_rows','secret'),e=>e.code==='forbidden');
 }
 await k.sim.publishAppVersion('owner','2',{collections:{shared_rows:schema}});
 assert.throws(()=>reader.query('shared_rows',{}),e=>e.code==='forbidden');
 assert.throws(()=>live.get(),e=>e.code==='forbidden');
});
test('E35 desktop range validates against verified CAS size, never row size_bytes',async t=>{
 const {k}=await open(t);
 await k.sim.publishAppVersion('desktop','1',{collections:{desktop_files:schema},manifestExtra:{exports:{desktop_files:['read']}}});
 await k.sim.publishAppVersion('reader','1',{manifestExtra:{foreign_collections:{desktop_files:{owner_app:'desktop',ops:['read']}}}});
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
 const {k}=await open(t),api=k.appApi('notes','1');
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
 const {k,options}=await open(t);await k.sim.setLink({state:'offline'});
 const api=k.appApi('notes','1'),handle=await api.files.put({text:'retained'},{id:'rejected-file',name:'receipt.txt'});
 assert.equal(typeof handle.done.then,'function');assert.equal(api.get('files',handle.id).get().name,'receipt.txt');
 k.sim.rejectNext({code:'forbidden',reason:'Native policy denies this upload'});
 const rejected=assert.rejects(handle.done,e=>e.code==='forbidden'&&e.reason==='Native policy denies this upload');
 await k.sim.setLink({state:'online'});await rejected;await k.sim.settle();
 const doc=api.get('files',handle.id).get();assert.equal(doc.state,'rejected');assert.equal(doc.ref,handle.ref);assert.equal(doc.name,'receipt.txt');
 assert.equal((await api.files.get({id:handle.id})).text,'retained');
 await k.close();const reload=createStubKernel(options);t.after(()=>reload.close());await reload.ready;
 assert.equal(reload.appApi('notes','1').get('files',handle.id).get().state,'rejected');
 // The original Blob handle exposes the same done contract.
 await reload.sim.setLink({state:'offline'});const blob=reload.appApi('notes','1').files.put(new Blob(['blob']),{id:'blob-reject'});await blob.local;
 reload.sim.rejectNext({code:'quota',reason:'Quota denied'});const blobRejected=assert.rejects(blob.done,e=>e.code==='quota'&&e.reason==='Quota denied');
 await reload.sim.setLink({state:'online'});await blobRejected;
});
