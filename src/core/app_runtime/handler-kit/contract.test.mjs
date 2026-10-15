import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createContext, runHandler, CtoxError, validateManifest, triggerInterval } from './index.mjs';
const app = extra => ({app_id:'contract',app_ver:'v1',collections:{rows:{}},handlers:{run:'handlers/run.js'},...extra});
const cas = files => {
  const hashes = {}, artifacts = {};
  for (const [name,source] of Object.entries(files)) { const hash=createHash('sha256').update(source).digest('hex'); hashes[name]=hash; artifacts[hash]=source; }
  return {hashes,artifacts};
};
test('full command envelope and actor metadata, static relative helper import, globals and UUID', async () => {
  const sources=cas({
    'handlers/run.js': "import { f } from './_lib.js'; export default async function handle(command,ctx) { return {command, n:f(), uuid:ctx.uuid(), url:new URL('/x','https://example.invalid').href,params:new URLSearchParams('x=2').get('x'),text:new TextDecoder().decode(new TextEncoder().encode('ok')),digest:Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode('a'))))}; }",
    'handlers/_lib.js': 'export const f=()=>7;',
  });
  const manifest=app({files:sources.hashes});
  const actor={id:'a',role:'writer',display_name:'Writer',is_admin:false};
  const result=await runHandler(manifest,'handlers/run.js',{id:'cmd',type:'run',payload:{p:1},record_id:'r',module:'m',client_context:{view:2}},{fixtures:{actor,artifacts:sources.artifacts}});
  assert.equal(result.n,7); assert.equal(result.command.record_id,'r'); assert.equal(result.command.module,'m');
  assert.deepEqual(result.command.actor,actor); assert.deepEqual(result.command.client_context,{view:2});
  assert.match(result.uuid,/^[a-f0-9-]{36}$/); assert.equal(result.digest.length,32); assert.equal(result.text,'ok'); assert.equal(result.params,'2');
  await assert.rejects(runHandler(app({handlers:{x:'handlers/_lib.js'},files:sources.hashes}),'handlers/_lib.js',{}, {fixtures:{artifacts:sources.artifacts}}),{code:'invalid'});
});
test('CAS generations invalidate relative-library cache, and reject import escape', async () => {
  const files={'handlers/run.js':"import { n } from './_lib.js'; export default()=>n;",'handlers/_lib.js':'export const n=1;'};
  let data=cas(files);
  assert.equal(await runHandler(app({files:data.hashes}),'handlers/run.js',{}, {fixtures:{artifacts:data.artifacts}}),1);
  files['handlers/_lib.js']='export const n=2;'; data=cas(files);
  assert.equal(await runHandler(app({files:data.hashes}),'handlers/run.js',{}, {fixtures:{artifacts:data.artifacts}}),2);
  const escaped=cas({'handlers/run.js':"import fs from 'node:fs'; export default()=>fs;"});
  await assert.rejects(runHandler(app({files:escaped.hashes}),'handlers/run.js',{}, {fixtures:{artifacts:escaped.artifacts}}),{code:'forbidden'});
});
test('transactions atomically commit, roll back errors and prohibit outer-context effects', async () => {
  let effects=0;
  const fixture={collections:{rows:[{id:'a',rev:'1',n:1}]},grants:['net.http'],http:()=>{effects++;return{};}};
  const ctx=createContext(app({capabilities_required:['net.http']}),fixture);
  await assert.rejects(ctx.transaction(async tx=>{await tx.patch('rows','a',{n:2});await tx.put('rows',{id:'b'});throw Error('rollback');}),/rollback/);
  assert.deepEqual(await ctx.read('rows'),[{id:'a',rev:'1',n:1}]);
  await assert.rejects(ctx.transaction(async tx=>{await tx.remove('rows','a');await ctx.http({});}),{code:'forbidden'});
  assert.equal(effects,0); assert.equal((await ctx.get('rows','a')).n,1);
  const count=await ctx.transaction(async tx=>{await tx.patch('rows','a',{n:3});await tx.put('rows',{id:'b'});return(await tx.read('rows')).length;});
  assert.equal(count,2); assert.equal((await ctx.get('rows','a')).n,3);
});
test('concurrent modification rejects transaction snapshot rather than losing another write', async () => {
  const fixture={collections:{rows:[{id:'a',rev:'1',n:1}]}};
  const first=createContext(app(),fixture),second=createContext(app(),fixture);
  await assert.rejects(first.transaction(async tx=>{await tx.patch('rows','a',{n:2});await second.patch('rows','a',{n:9});}),{code:'conflict'});
  assert.equal((await first.get('rows','a')).n,9);
});
test('commands return acceptance plus task_id and preserve options; cursors persist per trigger', async () => {
  let options;
  const f={trigger_id:'tick',command:(_type,_payload,opts)=>{options=opts;return{id:opts.id,status:'accepted',task_id:'task-real-ref'};}};
  const ctx=createContext(app(),f);
  assert.deepEqual(await ctx.command('run',{x:1},{id:'next',record_id:'row',module:'mod',client_context:{a:1}}),{id:'next',status:'accepted',task_id:'task-real-ref'});
  assert.equal(options.record_id,'row');assert.equal(options.module,'mod');
  assert.equal(await ctx.cursor.get(),null);await ctx.cursor.set(17);assert.equal(await createContext(app(),f).cursor.get(),17);
});
test('command_rights rejects before loading/executing handler and foreign watch is explicit', async () => {
  const sources=cas({'handlers/run.js':"throw Error('must never execute');"});
  await assert.rejects(runHandler(app({files:sources.hashes,command_rights:{'r*':{roles:['admin'],assigned_users:false}}}),'handlers/run.js',{id:'x',type:'run',payload:{}},{fixtures:{actor:{id:'a',role:'viewer'},artifacts:sources.artifacts}}),{code:'forbidden'});
  const spec=app({foreign_collections:{shared:{owner_app:'owner',ops:['read','watch']}},triggers:[{handler:'handlers/run.js',on:{collection:'shared',op:'update'}}]});
  assert.equal(validateManifest(spec),spec);
  assert.throws(()=>validateManifest({...spec,foreign_collections:{shared:{owner_app:'owner',ops:['read']}}}),{code:'invalid'});
  assert.equal(triggerInterval('PT1M'),60000);assert.equal(triggerInterval('PT2H'),7200000);
  for(const value of ['PT59S','P','PT','P-1D','PTInfinityM'])assert.throws(()=>triggerInterval(value),{code:'invalid'});
});
test('remove leaves only tombstone metadata and rejects stale resurrection; object CtoxError', async () => {
  const fixture={collections:{rows:[{id:'x',rev:'r',personal:'erase'}]}},ctx=createContext(app(),fixture);
  const result=await ctx.remove('rows','x',{base_rev:'r'});
  assert.equal((await ctx.read('rows')).length,0);
  assert.deepEqual(fixture.collections.rows,[{id:'x',rev:result.rev,deleted:true}]);
  await assert.rejects(ctx.put('rows',{id:'x',personal:'resurrect'},{base_rev:'r'}),{code:'conflict'});
  assert.equal(new CtoxError({code:'conflict',message:'changed',path:'x'}).path,'x');
});
