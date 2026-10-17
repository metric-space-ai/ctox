const PLATFORM_PROBES=["ctox.app_store.install", "ats.consent.check", "browser.back", "ats.consent.check", "ctox.appsec.probe", "business_os.chat.task", "business_os.context.ask", "business_os.data.modify", "ctox.workjet.new.route.v1", "ctox.plan.step.retry"];
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {testLocks} from './web-locks.mjs';
import {createStubKernel} from '../testkit.mjs';
let serial=0;
async function open(t,role='admin'){const instanceId=`e39-${process.pid}-${++serial}`;const k=createStubKernel({instanceId,coreNamespace:instanceId,actor:{role,permissions:[]},userId:'alice',locks:testLocks,storage:'memory',multiTab:false,latencyMs:0});t.after(()=>k.close());await k.ready;return k;}
const files={'index.js':'export function mount(){return {unmount(){}}}', 'handler.js':"export default async command=>({sender:command.app_id,target:'receiver'});"};
async function receiver(k,extra={},handlers={'ipc.run':'handler.js'},ver='1'){await k.sim.publishAppVersion('receiver',ver,{files,handlers,manifestExtra:{commands_in:[{type:'ipc.run',from:['sender']}],command_rights:{'ipc.run':{roles:['admin','operator']}},...extra}});}
async function sender(k,out=[{app:'receiver',type:'ipc.run'}],ver='1'){await k.sim.publishAppVersion('sender',ver,{files,manifestExtra:{commands_out:out}});return k.appApi('sender',ver);}
async function end(h){await h.result;for(let i=0;i<400&&!['done','failed'].includes(h.status.get().status);i++)await new Promise(r=>setTimeout(r,5));return h.status.get();}
const forbidden=r=>assert.deepEqual([r.status,r.code??r.reason],['rejected','forbidden']);
test('E39 cross-app routing uses sender declarations, inbound grant, handler and target user right',async t=>{
 const k=await open(t);await receiver(k);let api=await sender(k);
 const h=api.command('ipc.run',{app_id:'attacker',target_app:'other'});assert.equal((await h.result).status,'accepted');assert.deepEqual((await end(h)).result,{sender:'sender',target:'receiver'});
 forbidden(await api.command('ipc.run',{}, {target_app:'other'}).result);
 api=await sender(k,[{app:'receiver',type:'ipc.run'},{app:'other',type:'ipc.run'}],'2');
 forbidden(await api.command('ipc.run',{}).result);
 assert.equal((await end(api.command('ipc.run',{}, {target_app:'receiver'}))).status,'done');
 await receiver(k,{commands_in:[]},undefined,'2');forbidden(await api.command('ipc.run',{}, {target_app:'receiver'}).result);
 await receiver(k,{commands_in:[{type:'ipc.run',from:'*'}]},undefined,'3');assert.equal((await end(api.command('ipc.run',{}, {target_app:'receiver'}))).status,'done');
 await receiver(k,{}, {},'4');forbidden(await api.command('ipc.run',{}, {target_app:'receiver'}).result);
 await receiver(k,{},undefined,'5');
 await receiver(k,{commands_in:[]},undefined,'6');
 await k.sim.setLink({state:'offline'});
 const deniedBeforeForge=api.command('ipc.run',{}, {target_app:'receiver'});await deniedBeforeForge.local;
 const canonical=(await k.sim.inspect('business_app_versions','receiver@6')).data;
 const forge=api.put('business_app_versions',{...canonical,manifest:{...canonical.manifest,commands_in:[{type:'ipc.run',from:'*'}]}});await forge.local;
 await k.sim.setLink({state:'online'});forbidden(await deniedBeforeForge.result);await forge.result;
 await receiver(k,{},undefined,'7');

 // Authoritative check also guards a forged document, bypassing api.command target resolution.
 const id='forged-target';forbidden(await api.put('business_commands',{id,type:'ipc.run',payload:{},app_id:'sender',target_app:'other',actor_id:'alice',status:'queued'}).result);
 const unknown=await api.command('unregistered.command',{}).result;assert.equal(unknown.code,'unknown_command');
});
test('E42 every native inventory class and registry is routed to Core without an app handler',async t=>{
 const k=await open(t);const api=await sender(k,[]);
 for(const type of PLATFORM_PROBES){const h=api.command(type,{});assert.equal((await h.result).status,'accepted',type);assert.notEqual((await end(h)).error,`no_handler: ${type} hat keinen Handler in sender`,type);}
});
test('E44 canNavigate is read-only and agrees with actual navigation including declared command_type',async t=>{
 const k=await open(t);await receiver(k);const api=await sender(k);const before=k.sim.navigations.length,zone=k.status.forApp('sender').get();
 assert.equal(api.shell.canNavigate('receiver'),true);assert.equal(api.shell.canNavigate('missing'),false);
 assert.equal(api.shell.canNavigate('receiver',{command_type:'ipc.run'}),true);assert.equal(api.shell.canNavigate('receiver',{command_type:'forged'}),false);
 assert.equal(k.sim.navigations.length,before);assert.deepEqual(k.status.forApp('sender').get(),zone);
 assert.equal(api.shell.navigate('receiver',{command_type:'ipc.run'}),true);assert.equal(k.sim.navigations.length,before+1);
});

test('E39 target command right is authoritative for the initiating user',async t=>{
 for(const role of ['viewer','operator']){const k=await open(t,role);await receiver(k);const api=await sender(k);const h=api.command('ipc.run',{});if(role==='viewer')forbidden(await h.result);else assert.equal((await end(h)).status,'done');}
});

 test('E39 native rights precedence and app-write fallback cannot be overridden by sender rules',async t=>{
  const k=await open(t,'operator');await receiver(k,{command_rights:{'ipc.*':{roles:['operator']},'ipc.run*':{roles:[]}}});const api=await sender(k);
  forbidden(await api.command('ipc.run',{}).result);
  await receiver(k,{command_rights:{'ipc.*':{roles:[]},'ipc.run':{roles:['operator']}}},undefined,'2');assert.equal((await end(api.command('ipc.run',{}))).status,'done');
  const schema={schema:{type:'object',properties:{id:{type:'string'}}}};
  await k.sim.publishAppVersion('receiver','3',{files,handlers:{'ipc.run':'handler.js'},collections:{target_rows:{...schema,rights:{write:'role:operator'}}},manifestExtra:{commands_in:[{type:'ipc.run',from:['sender']}]}});
  assert.equal((await end(api.command('ipc.run',{}))).status,'done');
  await k.sim.publishAppVersion('receiver','4',{files,handlers:{'ipc.run':'handler.js'},collections:{target_rows:{...schema,rights:{write:'role:admin'}}},manifestExtra:{commands_in:[{type:'ipc.run',from:['sender']}]}});
  forbidden(await api.command('ipc.run',{}).result);
 });

 test('E39 explicit own target survives the wire document when outgoing targets are ambiguous',async t=>{
  const k=await open(t);await receiver(k);
  await k.sim.publishAppVersion('sender','1',{files:{...files,'own.js':"export default async c=>({sender:c.app_id,target:'sender'});"},handlers:{'ipc.run':'own.js'},manifestExtra:{commands_out:[{app:'sender',type:'ipc.run'},{app:'receiver',type:'ipc.run'}]}});
  const h=k.appApi('sender','1').command('ipc.run',{}, {target_app:'sender'});
  assert.equal((await h.result).status,'accepted');assert.deepEqual((await end(h)).result,{sender:'sender',target:'sender'});
 });

 test('E39 routing authorization preserves existing idempotent commands across target/declaration changes',async t=>{
  const k=await open(t);await receiver(k);let api=await sender(k);
  const first=api.command('ipc.run',{}, {idempotency_key:'once'});assert.equal((await end(first)).status,'done');
  const seq=(await k.sim.inspect('business_commands',first.id)).base.seq;
  let replay;assert.doesNotThrow(()=>{replay=api.command('ipc.run',{}, {idempotency_key:'once',target_app:'other'});});
  assert.equal(replay.id,first.id);assert.equal((await replay.result).status,'accepted');assert.deepEqual(replay.status.get().result,{sender:'sender',target:'receiver'});
  api=await sender(k,[{app:'receiver',type:'ipc.run'},{app:'other',type:'ipc.run'}],'2');
  let changed;assert.doesNotThrow(()=>{changed=api.command('ipc.run',{}, {idempotency_key:'once'});});
  assert.equal(changed.id,first.id);assert.equal((await changed.result).status,'accepted');assert.equal((await k.sim.inspect('business_commands',first.id)).base.seq,seq);
 });
