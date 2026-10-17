import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../testkit.mjs';
let serial=0;
const schema={schema:{type:'object',properties:{id:{type:'string'}}}};
async function open(t){const k=createStubKernel({instanceId:`e47-${process.pid}-${++serial}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:0});t.after(()=>k.close());await k.ready;return k;}
test('E47 foreign reads expose confirmed revision for CAS and keep optimistic pending separate',async t=>{
 const k=await open(t);
 await k.sim.publishAppVersion('owner','1',{collections:{notes:schema},manifestExtra:{exports:{notes:['read','write']}}});
 await k.sim.publishAppVersion('reader','1',{manifestExtra:{foreign_collections:{notes:{owner_app:'owner',ops:['read','write']}}}});
 await k.sim.push('notes',[{id:'n',value:1}]);const api=k.appApi('reader','1'),before=api.get('notes','n').get();
 assert.equal(typeof before._rev,'string');assert.equal(before._pending,false);assert.ok(Object.isFrozen(before));
 assert.equal(api.query('notes',{}).get()[0]._rev,before._rev);
 await k.sim.setLink({state:'offline'});const edit=api.patch('notes','n',{value:2},{expected_revision:before._rev});await edit.local;
 const pending=api.get('notes','n').get();assert.equal(pending._rev,before._rev);assert.equal(pending._pending,true);
 const local=api.put('notes',{id:'new',value:0});await local.local;assert.equal(api.get('notes','new').get()._rev,null);assert.equal(api.get('notes','new').get()._pending,true);
 await k.sim.setLink({state:'online'});assert.notEqual((await edit.result).status,'rejected');await local.result;await k.sim.settle();
 const confirmed=api.get('notes','n').get();assert.notEqual(confirmed._rev,before._rev);assert.equal(confirmed._pending,false);
 await k.sim.push('notes',[{id:'n',value:3}]);const stale=api.patch('notes','n',{value:4},{expected_revision:confirmed._rev});await stale.local;assert.equal((await stale.result).code,'revision_conflict');
 const row=await k.sim.inspect('notes','n');assert.equal('_rev' in row.base.data,false);assert.equal('_pending' in row.base.data,false);
});
test('E47 underscore write fields reject before local commit',async t=>{
 const k=await open(t);await k.sim.publishAppVersion('notes','1',{collections:{notes:schema}});const api=k.appApi('notes','1');
 for(const field of ['_deleted','_custom','__proto__']){
  await assert.rejects(api.put('notes',{id:'bad',_rev:'ignored',_pending:true,[field]:'forged'}).local,e=>e.code==='invalid'&&e.path===field);
  await assert.rejects(api.patch('notes','bad',{_rev:null,_pending:false,[field]:null}).local,e=>e.code==='invalid'&&e.path===field);
 }
 assert.equal(api.get('notes','bad').get(),null);assert.deepEqual(await k.sim.outbox(),[]);
});

test('E47.1 spread writes drop only read metadata and CAS uses the explicit option',async t=>{
 const k=await open(t);await k.sim.publishAppVersion('notes','1',{collections:{notes:schema}});const api=k.appApi('notes','1');
 const first=api.put('notes',{id:'spread',value:1,_rev:()=>{},_pending:undefined});await assert.doesNotReject(first.local);assert.equal((await first.result).status,'accepted');await k.sim.settle();
 const seen=api.get('notes','spread').get();assert.equal(typeof seen._rev,'string');
 const put=api.put('notes',{...seen,value:2});await put.local;assert.equal((await put.result).status,'accepted');await k.sim.settle();
 const patch=api.patch('notes','spread',{_rev:'stale-read-token',_pending:true,value:3});await patch.local;assert.equal((await patch.result).status,'accepted');await k.sim.settle();
 const conflict=api.patch('notes','spread',{value:4,_rev:seen._rev,_pending:seen._pending},{expected_revision:seen._rev});await conflict.local;assert.equal((await conflict.result).code,'revision_conflict');
 const row=await k.sim.inspect('notes','spread');assert.equal(row.base.data.value,3);assert.equal('_rev' in row.base.data,false);assert.equal('_pending' in row.base.data,false);
});

test('E47.1 fake core strips read metadata independently at its wire boundary',async t=>{
 const {MemoryCoreTransport}=await import('../testkit.mjs');const {createKernel}=await import('../index.mjs');
 const transport=new MemoryCoreTransport({namespace:`e471-wire-${process.pid}`,latencyMs:0});const original=transport.write.bind(transport);
 transport.write=entry=>original(entry.collection==='notes'?{...entry,body:{...entry.body,_rev:'ignored',_pending:true}}:entry);
 const k=createKernel({instanceId:'e471-wire',userId:'alice',storage:'memory',multiTab:false,transport});t.after(()=>k.close());await k.ready;
 await k.sim.publishAppVersion('notes','1',{collections:{notes:schema}});const h=k.appApi('notes','1').put('notes',{id:'wire',value:1});await h.local;assert.equal((await h.result).status,'accepted');await k.sim.settle();
 const row=await k.sim.inspect('notes','wire');assert.equal('_rev' in row.base.data,false);assert.equal('_pending' in row.base.data,false);
});
