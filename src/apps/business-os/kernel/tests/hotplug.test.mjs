import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../testkit.mjs';
const definition={schema:{type:'object',properties:{id:{type:'string'},value:{type:'number'}}}};
const tick=()=>new Promise(r=>setTimeout(r,5));
async function until(pred){for(let i=0;i<200;i++){if(pred())return;await tick();}assert.fail('open query did not follow the published owner');}
test('HOTPLUG declared read waits for owner then follows its live generation and revokes missing export',async t=>{
 const k=createStubKernel({instanceId:'hotplug-'+process.pid,userId:'alice',storage:'memory',multiTab:false,latencyMs:0});t.after(()=>k.close());await k.ready;
 await k.sim.publishAppVersion('reader','1',{manifestExtra:{foreign_collections:{reports:{owner_app:'owner',ops:['read']}}}});
 const api=k.appApi('reader','1');let live,one;assert.doesNotThrow(()=>{live=api.query('reports',{});one=api.get('reports','r');});
 let rows,doc;const off=live.subscribe(v=>rows=v),offOne=one.subscribe(v=>doc=v);t.after(off);t.after(offOne);assert.deepEqual(rows,[]);assert.equal(doc,null);
 const publish=(ver,gen,exports)=>k.sim.publishAppVersion('owner',ver,{collections:{reports:definition},manifestExtra:{generations:{reports:gen},exports}});
 await publish('1',2,{reports:['read']});const a=k.appApi('owner','1').put('reports',{id:'r',value:1});await a.local;await a.result;await until(()=>rows?.[0]?.value===1&&doc?.value===1);
 await publish('2',3,{reports:['read']});const b=k.appApi('owner','2').put('reports',{id:'r',value:2});await b.local;await b.result;await until(()=>rows?.[0]?.value===2&&doc?.value===2);
 await k.sim.push('reports@g2',[{id:'old',value:999}]);await k.sim.settle();assert.deepEqual(rows.map(r=>r.id),['r']);
 await publish('3',3,{});await until(()=>rows?.length===0&&doc===null);assert.throws(()=>live.get(),e=>e.code==='forbidden');assert.throws(()=>one.get(),e=>e.code==='forbidden');
});
