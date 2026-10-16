import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../testkit.mjs';
let serial=0;
async function open(t){const k=createStubKernel({instanceId:`e51-${process.pid}-${++serial}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:0});t.after(()=>k.close());await k.ready;return k;}
test('E51 preferences bind app and user even for admin, persist, and never expose foreign rows through raw reads',async t=>{
 const k=await open(t),a=k.appApi('a','1'),b=k.appApi('b','1');
 assert.equal(a.prefs.get('same'),null);const first=a.prefs.set('same',{own:'A'});await first.local;await first.result;
 await b.prefs.set('same',{own:'B'}).result;assert.deepEqual(a.prefs.get('same'),{own:'A'});assert.deepEqual(b.prefs.get('same'),{own:'B'});
 const clone=a.prefs.get('same');clone.own='forged';assert.deepEqual(a.prefs.get('same'),{own:'A'});
 const rows=await k.sim.coreRows('app_user_preferences');assert.ok(rows.some(r=>r.id==='a/alice/same'&&r.app_id==='a'&&r.user_id==='alice'));
 await k.sim.publishAppVersion('ctox-platform','test-e51',{collections:{app_user_preferences:{schema:{type:'object',properties:{id:{type:'string'}}}}},manifestExtra:{exports:{app_user_preferences:['read','write']}}});
 await k.sim.publishAppVersion('a','1',{manifestExtra:{foreign_collections:{app_user_preferences:{owner_app:'ctox-platform',ops:['read','write']}}}});
 assert.throws(()=>a.query('app_user_preferences',{}),e=>e.code==='forbidden');assert.throws(()=>a.get('app_user_preferences','b/alice/same'),e=>e.code==='forbidden');
 await assert.rejects(a.put('app_user_preferences',{id:'b/alice/same',app_id:'b',user_id:'alice',key:'same',value:'stolen'}).local,e=>e.code==='forbidden');
 const options={instanceId:k.identity.instance_id,userId:'alice',dbName:k.dbName,storage:'memory',multiTab:false,latencyMs:0};await k.close();const reload=createStubKernel(options);t.after(()=>reload.close());await reload.ready;
 assert.deepEqual(reload.appApi('a','1').prefs.get('same'),{own:'A'});
});
test('E51 preferences quota is atomic with local acknowledgement; replacement and delete release capacity',async t=>{
 const k=await open(t),a=k.appApi('a','1');await k.sim.setLink({state:'offline'});
 for(let i=0;i<64;i++)await a.prefs.set('key'+i,i).local;
 await assert.rejects(a.prefs.set('overflow',true).local,e=>e.code==='quota');assert.equal(a.prefs.get('overflow'),null);
 await a.prefs.set('key0','updated').local;await a.prefs.delete('key1').local;await a.prefs.set('replacement',null).local;
 assert.equal(a.prefs.get('key0'),'updated');assert.equal(a.prefs.get('key1'),null);
 assert.throws(()=>a.prefs.set('big','é'.repeat(8192)),e=>e.code==='quota');
 for(const key of ['','a/b','UPPER','x'.repeat(65)])assert.throws(()=>a.prefs.get(key),e=>e.code==='invalid');
 await k.sim.setLink({state:'online'});await k.sim.settle();assert.equal(a.prefs.get('key0'),'updated');
});
