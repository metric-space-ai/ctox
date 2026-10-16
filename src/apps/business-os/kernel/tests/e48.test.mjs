import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../testkit.mjs';
let serial=0;
const definition={schema:{type:'object',properties:{id:{type:'string'}}}};
async function open(t,options={}){const k=createStubKernel({instanceId:`e48-${process.pid}-${++serial}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:0,...options});t.after(()=>k.close());await k.ready;return k;}
test('E48 Shell news v4-api gets business_commands through automatically published platform exports',async t=>{
 const k=await open(t);
 await k.sim.publishAppVersion('news','1',{collections:{news_topics:definition},manifestExtra:{foreign_collections:{business_commands:{owner_app:'ctox-platform',ops:['read']}}}});
 let rows;assert.doesNotThrow(()=>{rows=k.appApi('news','1').query('business_commands',{});});assert.deepEqual(rows.get(),[]);
 const platform=await k.shell.artifacts.manifest('ctox-platform','1');assert.ok(platform.collections.business_commands);assert.ok(platform.exports.business_commands.includes('read'));
 await k.sim.publishAppVersion('outsider','1',{collections:{own:definition}});
 assert.throws(()=>k.appApi('outsider','1').query('business_commands',{}),e=>e.code==='forbidden');
 await k.sim.publishAppVersion('claimant','1',{collections:{business_commands:definition}});
 assert.throws(()=>k.appApi('claimant','1').query('business_commands',{}),e=>e.code==='forbidden','an app cannot claim ownership of a platform collection');
});
test('E48 navigation grants exactly one files.get for the exact ref, expires, cannot delegate and is audited',async t=>{
 const k=await open(t);for(const id of ['vault','viewer','other'])await k.sim.publishAppVersion(id,'1',{collections:{[id+'_rows']:definition}});
 const vault=k.appApi('vault','1'),viewer=k.appApi('viewer','1'),other=k.appApi('other','1');
 const a=await vault.files.put({text:'secret A'},{id:'a'});await a.done;const b=await vault.files.put({text:'secret B'},{id:'b'});await b.done;
 const deny=()=>e=>e.code==='forbidden';await assert.rejects(viewer.files.get({ref:a.ref}),deny());
 let routes=0;viewer.shell.onRoute(()=>routes++);
 await assert.rejects(other.shell.navigate('viewer',{ref:a.ref}),deny());assert.equal(routes,0);
 assert.equal(await vault.shell.navigate('viewer',{ref:a.ref}),true);assert.equal(routes,1);
 await assert.rejects(other.files.get({ref:a.ref}),deny());await assert.rejects(viewer.files.get({ref:b.ref}),deny());
 await assert.rejects(viewer.shell.navigate('other',{ref:a.ref}),deny(),'one-shot grant cannot mint a delegated grant');
 assert.equal((await viewer.files.get({ref:a.ref})).text,'secret A');await assert.rejects(viewer.files.get({ref:a.ref}),deny());
 assert.throws(()=>viewer.query('files',{}),deny(),'grant does not expose a collection');
 await vault.shell.navigate('viewer',{ref:b.ref});const realNow=Date.now; t.mock.method(Date,'now',()=>realNow()+3600001);
 await assert.rejects(viewer.files.get({ref:b.ref}),deny());t.mock.restoreAll();
 const {openBackend}=await import('../store-idb-v5.mjs');const db=await openBackend(k.dbName,'memory');const audit=(await db.all('meta')).filter(x=>x.key.startsWith('file-grant:')).map(x=>x.value);
 assert.ok(audit.some(x=>x.event==='granted'&&x.from==='vault'&&x.to==='viewer'&&x.ref===a.ref&&x.user_id==='alice'));
 assert.ok(audit.some(x=>x.event==='consumed'&&x.ref===a.ref));
});

test('E48 generated platform manifest equals the native source without handwritten grants',async()=>{const {readFile}=await import('node:fs/promises');const {PLATFORM_MANIFEST}=await import('../platform-manifest.generated.mjs');assert.deepEqual(PLATFORM_MANIFEST,JSON.parse(await readFile(new URL('../../../../core/business_os/v4/ctox-platform.manifest.json',import.meta.url),'utf8')));});

test('E48 read grants end with the session even though their audit persists',async t=>{
 const k=await open(t);for(const id of ['vault','viewer'])await k.sim.publishAppVersion(id,'1',{collections:{[id+'_rows']:definition}});
 const file=await k.appApi('vault','1').files.put({text:'session bound'},{id:'session-file'});await file.done;
 await k.appApi('vault','1').shell.navigate('viewer',{ref:file.ref});
 const opts={instanceId:k.identity.instance_id,userId:'alice',dbName:k.dbName,storage:'memory',multiTab:false,latencyMs:0};await k.close();
 const reload=createStubKernel(opts);t.after(()=>reload.close());await reload.ready;
 await assert.rejects(reload.appApi('viewer','1').files.get({ref:file.ref}),e=>e.code==='forbidden');
});
