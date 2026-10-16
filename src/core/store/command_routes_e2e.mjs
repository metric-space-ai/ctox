// E25 contract composition. In-process P5 channels + native P9 writer.
// Native DTOs are fixture inputs, not a claim of productive device/speech I/O.
import assert from 'node:assert/strict';
import {createInterface} from 'node:readline';
import {readFileSync} from 'node:fs';
import {verify,createPublicKey} from 'node:crypto';
const inventory=JSON.parse(readFileSync(new URL('../../../tests/sync_v4_e25/e25-methods.json',import.meta.url)));
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
   assert.equal(lane,1,'command must use P1');
   const response=await rpc('write',{message:m});
   await this.send('commands',response.receipt);
   await this.send('live',response.page);return;
  }
  if(m.type==='subscribe'){
   if(m.collection!=='business_commands')return;
   const page=await rpc('subscribe',{message:m});
   if(m.checkpoint){replays++;assert.equal(m.checkpoint.seq,page.seq_from);}
   if(holdNextInitial&&!m.checkpoint){holdNextInitial=false;heldInitial=page;return;}
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
const timer=setTimeout(()=>{console.error('E25 kernel component deadline');process.exit(1)},20000);
try {
 await k.ready;await transport.ready;
 while(!transport.scopes.has('business_app_versions'))await new Promise(r=>setTimeout(r,1));
 const boot=async(collection,docs)=>transport.events.page({collection,epoch:config.epoch,scope_hash:'rights:1',gen:1,cursor_to:'0',complete:true,backlog:0,docs:docs.map(doc=>({id:doc.id,seq:'0',rev:'bootstrap',body:doc}))});
 await boot('business_apps',[{id:'ctox-platform',active_ver:'platform-7'}]);
 await boot('business_app_versions',[{id:'ctox-platform@platform-7',manifest:config.manifest}]);
 const api=k.appApi('sender','99');
 const measured=[];
 for(const method of inventory.methods){
  const samples=[];
  for(let i=0;i<16;i++){
   const fixture_result=method.streaming?{state:'finished',text:'native final'}:{method:method.method,value:i};
   const start=performance.now(),h=api.command(method.method,{fixture_result});
   await h.local;assert.equal((await h.result).status,'accepted');
   await new Promise((resolve,reject)=>{let off;const timeout=setTimeout(()=>{off?.();reject(Error('E25 completion missing '+method.method))},3000);off=h.status.subscribe(s=>{if(s.status==='done'){clearTimeout(timeout);queueMicrotask(()=>{off?.();resolve()});}});});
   assert.deepEqual(h.status.get().result,fixture_result);
   samples.push(performance.now()-start);
  }
  samples.sort((a,b)=>a-b);
  const p95=samples[Math.ceil(samples.length*.95)-1];
  assert.ok(p95<=150,`${method.method} component P1 p95 ${p95}>150ms`);
  measured.push({method:method.method,p50_ms:samples[Math.ceil(samples.length*.5)-1],p95_ms:p95,p99_ms:samples.at(-1),samples_ms:samples,lane:1});
 }
 assert.deepEqual(await k.sim.outbox(),[]);assert.deepEqual(peer.errors,[]);
 // Replay the same canonical command through ordinary kernel write + signed page.
 const doc=await rpc('first_command');await k.evict('business_commands');holdNextInitial=true;
 const {result:ignoredResult,executor_fence:ignoredFence,...intent}=doc;
 const h=api.put('business_commands',{...intent,status:'queued'});
 await h.local;assert.equal((await h.result).status,'merged');
 assert.equal(api.get('business_commands',doc.id).get().status,'done');
 assert.equal((await k.sim.outbox()).length,0);
 assert.ok(replays>0,'canonical receipt must trigger a real page replay');
 if(heldInitial)await peer.send('initial',heldInitial);
 console.log(JSON.stringify({type:'measured',profile:'E25 component, fixture native DTOs, real Kernel/P5/P9 writer/signatures; not product',methods:measured,verified_pages:verified,canonical_replays:replays}));
}finally{clearTimeout(timer);await k.close();peer.close();lines.close();}
