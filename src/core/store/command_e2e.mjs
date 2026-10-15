// Cross-language component harness: real Kernel/P5 scheduler/framing, native
// Store/OrderedWriter/Subscription on the other stdio end. No mocked lifecycle.
import assert from 'node:assert/strict';
import {createInterface} from 'node:readline';
import {verify,createPublicKey} from 'node:crypto';
const root=process.env.KERNEL_URL;
const {createKernel}=await import(root+'/kernel.mjs');
const {WebRTCKernelTransport}=await import(root+'/transport-webrtc.mjs');
const {protocolChannels,FakeNativePeer,offer}=await import(root+'/tests/fake-native-peer.mjs');
const config=JSON.parse(process.env.COMMAND_CONFIG);
const pending=new Map();let request=0;
const lines=createInterface({input:process.stdin});
lines.on('line',line=>{const m=JSON.parse(line),p=pending.get(m.request);if(p){pending.delete(m.request);m.error?p.reject(Error(m.error)):p.resolve(m.value);}});
const rpc=(type,data={})=>new Promise((resolve,reject)=>{const id=++request;pending.set(id,{resolve,reject});console.log(JSON.stringify({request:id,type,...data}));});
const canonical=v=>Array.isArray(v)?'['+v.map(canonical).join(',')+']':v&&typeof v==='object'?'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+canonical(v[k])).join(',')+'}':JSON.stringify(v);
const publicKey=createPublicKey({key:Buffer.from('302a300506032b6570032100d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a','hex'),format:'der',type:'spki'});
let verified=0,replays=0,holdNextInitial=false,heldInitial=null;
class NativeBridge extends FakeNativePeer {
 async apply(m,lane){
  if(m.type==='write'){
   assert.equal(lane,1);
   const response=await rpc('write',{message:m});
   await this.send('commands',response.receipt);
   await this.send('live',response.page);
   return;
  }
  if(m.type==='subscribe'){
   assert.equal(lane,0);
   if(m.collection!=='business_commands')return;
   // Keep the first optimistic command queued until native processing finishes.
   const page=await rpc('subscribe',{message:m});
   if(m.checkpoint){replays++;assert.equal(m.checkpoint.seq,page.seq_from);}
   if(holdNextInitial && !m.checkpoint){holdNextInitial=false;heldInitial=page;return;}
   await this.send('initial',page);return;
  }
  return super.apply(m,lane);
 }
}
const pair=protocolChannels(),peer=new NativeBridge(pair.server,{instanceId:'isolated-core',epoch:config.epoch});
const transport=new WebRTCKernelTransport({channels:pair.client,hello:offer('isolated-core'),resolveScope:c=>({scope_hash:'rights:1',gen:1,participant:'alice'}),verifyPackage:async packet=>{
 const {sig,...unsigned}=packet;
 const valid=packet.participant==='alice'&&packet.instance_id==='isolated-core'&&packet.key_id==='rfc8032-public-test-key'&&verify(null,Buffer.concat([Buffer.from('ctox.sync.v4.stream\0'),Buffer.from(canonical(unsigned))]),publicKey,Buffer.from(sig));
 if(valid)verified++;return valid;
}});
const k=createKernel({instanceId:'isolated-core',userId:'alice',storage:'memory',multiTab:false,latencyMs:0,transport});
const deadline=setTimeout(()=>{console.error('native command component deadline');process.exitCode=1;setTimeout(()=>process.exit(1),1000);void k.close().finally(()=>{peer.close();lines.close();process.exit(1);});},15000);
try {
 await k.ready;
 // Bootstrap metadata itself is tested separately by the platform importer;
 // no document/business result is synthesized on this side.
 await transport.ready;while(!transport.scopes.has('business_app_versions'))await new Promise(r=>setTimeout(r,1));
 const boot=async(collection,docs)=>transport.events.page({collection,epoch:config.epoch,scope_hash:'rights:1',gen:1,cursor_to:'0',complete:true,backlog:0,docs:docs.map(doc=>({id:doc.id,seq:'0',rev:'bootstrap',body:doc}))});
 await boot('business_apps',[{id:'ctox-platform',active_ver:'platform-7'}]);
 await boot('business_app_versions',[{id:'ctox-platform@platform-7',manifest:{collections:{business_commands:{schema:config.schema}}}}]);
 const api=k.appApi('sender','99');
 const samples=[];
 for(let i=0;i<128;i++){
  const start=performance.now(),h=api.command('fixture.task',{value:i});await h.local;
  const receipt=await h.result;
  assert.equal(receipt.status,'accepted');
  await new Promise((resolve,reject)=>{let off;const timer=setTimeout(()=>{off?.();reject(Error('terminal page missing'))},3000);off=h.status.subscribe(s=>{if(s.status==='done'){clearTimeout(timer);queueMicrotask(()=>{off?.();resolve();});}});});
  assert.equal(h.status.get().result.value,i);
  samples.push(performance.now()-start);
 }
 // New op, same command_id, no base: no mutation; receipt's seq is behind
 // the live checkpoint. The core returns merged/requires_page and the old
 // canonical row must be replayed through the normal signed read path.
 const original=(await k.sim.outbox());assert.equal(original.length,0);
 const doc=await rpc('first_command');
 await k.evict('business_commands');holdNextInitial=true;
 const {result:ignoredResult,executor_fence:ignoredFence,...intent}=doc;
 const h=api.put('business_commands',{...intent,status:'queued'});
 await h.local;const repeat=await h.result;
 assert.equal(repeat.status,'merged');assert.equal(api.get('business_commands',doc.id).get().status,'done');
 assert.equal((await k.sim.outbox()).length,0);assert.ok(replays>0);assert.ok(verified>0);assert.deepEqual(peer.errors,[]);
 if(heldInitial)await peer.send('initial',heldInitial);
 samples.sort((a,b)=>a-b);
 console.log(JSON.stringify({type:'measured',profile:'cross-language component, memory client/WAL core, in-process channels; not installed/TURN',p50_ms:samples[Math.ceil(samples.length*.5)-1],p95_ms:samples[Math.ceil(samples.length*.95)-1],p99_ms:samples.at(-1),verified_pages:verified,canonical_replays:replays}));
}finally{clearTimeout(deadline);await k.close();peer.close();lines.close();}
