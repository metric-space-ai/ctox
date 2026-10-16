import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../testkit.mjs';
let serial=0;
const schema={schema:{type:'object',properties:{id:{type:'string'}}}};
async function open(t){const k=createStubKernel({instanceId:`journal-${process.pid}-${++serial}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:0});t.after(()=>k.close());await k.ready;await k.sim.publishAppVersion('notes','1',{collections:{notes:schema}});return k;}
test('no-silent-loss rejected patch journals the complete local acknowledgement and surfaces its reference after reload',async t=>{
 const k=await open(t),api=k.appApi('notes','1');await api.put('notes',{id:'n',value:'old',retained:'complete original'}).result;
 const revision=api.get('notes','n').get()._rev;await k.sim.setLink({state:'offline'});
 const edit=api.patch('notes','n',{value:'my acknowledged edit'},{expected_revision:revision});await edit.local;
 await k.sim.push('notes',[{id:'n',value:'server winner',retained:'new server field'}]);
 await k.sim.setLink({state:'online'});const result=await edit.result;
 assert.equal(result.status,'rejected');assert.equal(result.code,'revision_conflict');
 const archive=(await k.sim.archiveLocal()).find(x=>x.op_id===edit.op_id);assert.ok(archive,'lost input must be journaled');
 assert.deepEqual(archive.local_version.data,{id:'n',value:'my acknowledged edit',retained:'complete original'});
 assert.equal(archive.canonical_version.data.value,'server winner');assert.equal(result.local_archived_ref,archive.ref);
 assert.ok(k.status.forApp('notes').get().some(x=>x.ref===archive.ref));
 const options={instanceId:k.identity.instance_id,userId:'alice',dbName:k.dbName,storage:'memory',multiTab:false,latencyMs:0};await k.close();const reload=createStubKernel(options);t.after(()=>reload.close());await reload.ready;
 assert.ok((await reload.sim.archiveLocal()).some(x=>x.ref===archive.ref&&x.local_version.data.value==='my acknowledged edit'));
});
test('no-silent-loss canonical migration keeps dropped local fields and emits a surfaced journal reference',async t=>{
 const k=await open(t),api=k.appApi('notes','1');k.sim.migrateNext({dropFields:['value']});
 const edit=api.put('notes',{id:'m',value:'keep this input',retained:true});await edit.local;const result=await edit.result;
 assert.equal(result.status,'migrated');const archive=(await k.sim.archiveLocal()).find(x=>x.op_id===edit.op_id);assert.ok(archive,'canonical winner cannot discard local data silently');
 assert.equal(archive.local_version.data.value,'keep this input');assert.equal(archive.canonical_version.data.value,undefined);
 assert.equal(result.local_archived_ref,archive.ref);assert.ok(k.status.forApp('notes').get().some(x=>x.ref===archive.ref));
});
