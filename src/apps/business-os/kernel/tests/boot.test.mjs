import {test} from 'node:test';
import assert from 'node:assert/strict';
import {generateKeyPairSync,sign} from 'node:crypto';
import {bootV4,BootV4Error,parseV4LaunchContext,fetchV4LaunchContext,canonicalJson,STREAM_DOMAIN,SCOPE_DOMAIN} from '../boot.mjs';
import {decodeHelloFrame,negotiateHello} from '../vendor/hello.mjs';
import {protocolChannels,FakeNativePeer,offer} from './fake-native-peer.mjs';
import {WebRTCKernelTransport} from '../transport-webrtc.mjs';

const tick=()=>new Promise(r=>setTimeout(r,5));
const COLLECTIONS=['business_apps','business_app_versions','business_commands','files','shell_preferences','app_drafts','notes'];
const b64url=bytes=>Buffer.from(bytes).toString('base64url');
function instanceKey(){const {privateKey,publicKey}=generateKeyPairSync('ed25519');return {privateKey,publicKey:publicKey.export({format:'jwk'}).x};}
const KEY=instanceKey(),FOREIGN=instanceKey();
function scopeToken(collection,{key=KEY,instance='instance',device='test-device',exp=Math.floor(Date.now()/1000)+600}={}){
 const claims={instance_id:instance,scope_hash:`scope:${collection}`,rights_ver:1,device_id:device,exp};
 return b64url(Buffer.from(canonicalJson({claims,signature:b64url(sign(null,Buffer.from(SCOPE_DOMAIN+canonicalJson(claims)),key.privateKey))})));
}
function context({instanceId='instance',userId='alice',nonce='B',capabilities=[]}={}){
 const hello=offer(instanceId);hello.capabilities=capabilities;hello.scopes_wanted=[...COLLECTIONS];hello.device_proof.nonce=nonce.repeat(43);
 return {session:{v2:'untouched'},config:null,designTemplates:[],v4:{instanceId,userId,actor:{id:userId,role:'owner',display_name:'Alice'},
  signaling:{urls:['wss://signal.example/ws'],room:'ctox-v4:instance',peerId:'browser-1',nativePeerId:'native-1',token:'signal-token',iceServers:[{urls:'turn:turn.example:3478',username:'u',credential:'c'}]},
  hello:{offer:hello},packageKeys:[{keyId:'instance-key-1',alg:'Ed25519',publicKey:KEY.publicKey}]}};
}
function fetchFake(bodies){
 const calls=[];const queue=[...bodies];
 const impl=async(url,init)=>{calls.push({url,init});const body=queue.length>1?queue.shift():queue[0];if(body instanceof Error)throw body;if(typeof body==='number')return {ok:false,status:body,json:async()=>({})};return {ok:true,status:200,json:async()=>typeof body==='string'?JSON.parse(body):structuredClone(body)};};
 return {impl,calls};
}
/** FakeNativePeer with real instance-key signatures for hello_ok scope tokens and pages. */
class SignedNativePeer extends FakeNativePeer {
 constructor(channels,opts={}){super(channels,{instanceId:'instance',...opts});this.reject=opts.reject;this.grant=opts.grant??[];this.tokenFor=opts.tokenFor??(c=>scopeToken(c));this.key=opts.key??KEY;}
 async receive(data){
  if(typeof data==='string'&&JSON.parse(data).type!=='ack'){
   // #820/E14: the client hello carries session_id/resume_from next to the P4 frame.
   const m=JSON.parse(data),{session_id,resume_from,...p4}=m;this.native=session_id!==undefined;if(this.native)this.sessionId=session_id;
   const hello=decodeHelloFrame(JSON.stringify(p4));this.messages.push(m);
   if(this.reject){this.channels[0].send(JSON.stringify({type:'hello_reject',reason:'authentication_failed',required:'fresh device proof'}));return;}
   const selected=negotiateHello({instance_id:this.instanceId,bootstrap_min:1,bootstrap_max:1,app_api_ver:{min:'4.0',max:'4.0'},capabilities:this.grant,required_capabilities:[],transport:hello.transport},hello);
   this.channels[0].send(JSON.stringify({type:'hello_ok',protocol_ver:4,instance_id:this.instanceId,...selected,epoch:this.epoch,head_seq:this.head,scope_tokens:hello.scopes_wanted.map(c=>this.tokenFor(c)),rights_ver:'1',core_time_ms:Date.now(),...(this.native?{session_id:this.sessionId,resume_from:this.received.map(String)}:{})}));return;
  }
  return super.receive(data);
 }
 page(collection,docs=[],extra={}){
  const m={type:'page',instance_id:this.instanceId,key_id:'instance-key-1',participant:'alice',collection,scope_hash:`scope:${collection}`,gen:1,epoch:'1',package_id:`pkg-${Math.random()}`,seq_from:'0',seq_to:this.head,cursor_to:this.head,head_seq:this.head,complete:true,backlog:0,core_sent_at_ms:Date.now(),docs,...extra};
  const sig=extra.sig??[...sign(null,Buffer.from(STREAM_DOMAIN+canonicalJson(m)),this.key.privateKey)];
  return this.send('initial',{...m,sig});
 }
}
function harness(peerOpts={}){
 const peers=[],peerCalls=[];
 const createPeer=async(address,identity)=>{peerCalls.push({address,identity});const pair=protocolChannels(),peer=new SignedNativePeer(pair.server,{...peerOpts,onSubscribe:async(sub,p)=>{p.subs=(p.subs??0)+1;if(peerOpts.pages?.[sub.collection])await p.page(sub.collection,peerOpts.pages[sub.collection].map(doc=>({id:doc.id,data:doc,seq:p.head,rev:'r'})));if(sub.collection==='notes')await p.page('notes',[{id:peerOpts.docId??'server',data:{id:peerOpts.docId??'server',title:'from core'},seq:p.head,rev:'r'}],peerOpts.pageExtra);}});peers.push(peer);return {channels:pair.client,close:()=>peer.close()};};
 return {peers,peerCalls,createPeer};
}
const loaderOptions={storage:'memory',multiTab:false,serviceWorker:false,latencyMs:0};
const shell=()=>({startShell(){},stopShell(){}});

test('launch-context shape is validated with typed reasons',async()=>{
 const ok=parseV4LaunchContext(context());
 assert.equal(ok.instanceId,'instance');assert.equal(ok.signaling.nativePeerId,'native-1');assert.equal(ok.hello.offer.instance_id,'instance');
 assert.equal(Object.isFrozen(ok),true);
 const reason=r=>e=>e instanceof BootV4Error&&e.reason===r;
 for(const payload of [null,{},{session:{},config:{},designTemplates:[]},{v4:null}])assert.throws(()=>parseV4LaunchContext(payload),reason('launch_context_unavailable'));
 const broken=[
  c=>{c.v4='x'},c=>{delete c.v4.instanceId},c=>{c.v4.userId=''},c=>{c.v4.actor.id='mallory'},c=>{delete c.v4.actor.role},
  c=>{c.v4.signaling.urls=['https://signal.example']},c=>{c.v4.signaling.urls=[]},c=>{delete c.v4.signaling.token},c=>{c.v4.signaling.nativePeerId='browser-1'},
  c=>{c.v4.signaling.iceServers=[{url:'stun:x'}]},c=>{delete c.v4.hello},c=>{c.v4.hello.offer.instance_id='foreign'},c=>{c.v4.hello.offer.device_proof.signature='short'},
  c=>{c.v4.packageKeys=[{keyId:'k',alg:'RSA',publicKey:KEY.publicKey}]},c=>{c.v4.packageKeys=[{keyId:'k',alg:'Ed25519',publicKey:'x'}]},c=>{c.v4.packageKeys=[c.v4.packageKeys[0],c.v4.packageKeys[0]]},
 ];
 for(const mutate of broken){const c=context();mutate(c);assert.throws(()=>parseV4LaunchContext(c),e=>e instanceof BootV4Error&&e.reason==='launch_context_invalid'&&e.code==='invalid',mutate.toString());}
 for(const keys of [undefined,null,[]]){const c=context();c.v4.packageKeys=keys;assert.throws(()=>parseV4LaunchContext(c),e=>e.reason==='package_verifier_missing'&&e.code==='forbidden');}
});

test('launch-context fetch is same-origin control plane with typed transport failures',async()=>{
 const good=fetchFake([context()]);await fetchV4LaunchContext('/api/business-os/launch-context',good.impl);
 assert.equal(good.calls[0].url,'/api/business-os/launch-context');assert.equal(good.calls[0].init.credentials,'same-origin');assert.equal(good.calls[0].init.method,'GET');
 for(const [body,reason,code] of [[503,'launch_context_unavailable','unavailable'],[401,'launch_context_unavailable','unavailable'],[new TypeError('network down'),'launch_context_unavailable','unavailable'],['not json','launch_context_invalid','invalid']]){
  const f=fetchFake([body]);
  if(body==='not json')f.impl=async()=>({ok:true,status:200,json:async()=>{throw SyntaxError('bad json')}});
  await assert.rejects(fetchV4LaunchContext(undefined,f.impl),e=>e instanceof BootV4Error&&e.reason===reason&&e.code===code);
 }
});

test('bootV4 hands exactly the launch context to the transport and brings the loader up over verified pages',async()=>{
 const ctx=context(),f=fetchFake([ctx]),h=harness();
 const boot=await bootV4({fetchImpl:f.impl,createPeer:h.createPeer,resolveShell:shell,loaderOptions});
 try{
  assert.equal(f.calls.length,1,'exactly one control-plane request, no HTTP data path');
  assert.deepEqual(h.peerCalls[0].address,ctx.v4.signaling,'signaling data reaches the peer factory unchanged');
  assert.deepEqual(h.peerCalls[0].identity,{instanceId:'instance',userId:'alice'});
  const {session_id,resume_from,...p4}=h.peers[0].messages[0];
  assert.deepEqual(p4,ctx.v4.hello.offer,'native peer receives exactly the launch-context hello (plus the E14 session envelope)');
  assert.equal(typeof session_id,'string');assert.equal(resume_from.length,6);
  assert.deepEqual(boot.transport.options.address,ctx.v4.signaling);assert.equal(typeof boot.transport.options.verifyPackage,'function');
  assert.equal(boot.kernel.identity.device_id,ctx.v4.hello.offer.device_proof.device_id,'device identity from the admitted bootstrap');
  assert.equal(boot.kernel.isLeader,true);assert.equal(boot.kernel.identity?.actor?.id??boot.kernel.actor?.get?.()?.id??'alice','alice');
  assert.equal(boot.transport.session.instance_id,'instance');assert.deepEqual(boot.transport.session.scope_tokens.length,COLLECTIONS.length);
  assert.equal((await boot.transport.scope('notes')).scope_hash,'scope:notes');
  await assert.rejects(boot.transport.scope('not_granted'),e=>e.code==='forbidden');
  const api=boot.kernel.appApi('notes','1');
  // A local write makes `notes` a mirrored collection; the leader subscribes it over the authenticated scope.
  const local=api.put('notes',{id:'client',title:'local'});await local.local;assert.equal((await local.result).status,'accepted');
  for(let n=0;n<200&&!api.get('notes','server').get();n++)await tick();
  assert.equal(api.get('notes','server').get()?.title,'from core','signed page applied through the loader kernel');
  assert.deepEqual(h.peers[0].errors,[]);
 }finally{await boot.loader.close();for(const p of h.peers)p.close();}
});

test('reconnect fetches a fresh single-use hello; never replays the first device proof',async()=>{
 const first=context({nonce:'B'}),second=context({nonce:'E'}),f=fetchFake([first,second]),h=harness();
 const boot=await bootV4({fetchImpl:f.impl,createPeer:h.createPeer,resolveShell:shell,loaderOptions});
 try{
  await boot.transport.reconnect();
  assert.equal(f.calls.length,2);assert.equal(h.peers[1].messages[0].device_proof.nonce,'E'.repeat(43));
 }finally{await boot.loader.close();for(const p of h.peers)p.close();}
});

test('typed errors: hello rejected, forged scope grant, unsigned page, missing verifier',async()=>{
 for(const peerOpts of [{reject:true},{tokenFor:c=>scopeToken(c,{key:FOREIGN})},{tokenFor:c=>scopeToken(c,{device:'other-device'})},{tokenFor:c=>scopeToken(c,{exp:1})}]){
  const h=harness(peerOpts);
  await assert.rejects(bootV4({fetchImpl:fetchFake([context()]).impl,createPeer:h.createPeer,resolveShell:shell,loaderOptions,handshakeTimeoutMs:2000}),e=>e instanceof BootV4Error&&e.reason==='hello_rejected'&&e.code==='forbidden',JSON.stringify(Object.keys(peerOpts)));
  for(const p of h.peers)p.close();
 }
 // An unsigned (fixture) or foreign-key page fails closed after hello; nothing is applied.
 for(const pageExtra of [{sig:'test-signature'},{key_id:'unknown-key'},{participant:'mallory'}]){
  const docId=`forged-${Object.keys(pageExtra)[0]}`,h=harness({pageExtra,docId});const boot=await bootV4({fetchImpl:fetchFake([context()]).impl,createPeer:h.createPeer,resolveShell:shell,loaderOptions});
  try{void boot.kernel.appApi('notes','1').put('notes',{id:'client',title:'local'}).local;for(let n=0;n<200&&!boot.transport.closed;n++)await tick();assert.equal(boot.transport.closed,true);assert.equal(boot.kernel.appApi('notes','1').get('notes',docId).get()??null,null);assert.deepEqual(h.peers[0].errors.filter(e=>!/fixture closed/.test(e.message)),[]);}
  finally{await boot.loader.close();for(const p of h.peers)p.close();}
 }
 const noVerifier=context();noVerifier.v4.packageKeys=[];
 await assert.rejects(bootV4({fetchImpl:fetchFake([noVerifier]).impl,createPeer:harness().createPeer,resolveShell:shell,loaderOptions}),e=>e.reason==='package_verifier_missing');
 await assert.rejects(bootV4({fetchImpl:fetchFake([context()]).impl,createPeer:harness().createPeer,resolveShell:shell,loaderOptions,subtle:{importKey:async()=>{throw new DOMException('Unrecognized name','NotSupportedError')}}}),e=>e.reason==='package_verifier_missing');
 const noV4={...context()};delete noV4.v4;
 await assert.rejects(bootV4({fetchImpl:fetchFake([noV4]).impl,createPeer:harness().createPeer,resolveShell:shell,loaderOptions}),e=>e.reason==='launch_context_unavailable');
});

test('resolveShell is optional: absent, the loader gets no resolver and loads the activated shell entry itself',async()=>{
 const ctx=context(),f=fetchFake([ctx]),h=harness(),seen=[];
 const {createLoader}=await import('../loader.mjs');
 const boot=await bootV4({fetchImpl:f.impl,createPeer:h.createPeer,loaderOptions,loaderFactory:opts=>{seen.push(opts);return createLoader(opts);}});
 try{
  assert.equal(seen.length,1);assert.equal(Object.prototype.hasOwnProperty.call(seen[0],'resolveShell'),false,'createLoader falls back to kernel.loadModule(shell, ver)');
  assert.equal(seen[0].transport,boot.transport);assert.equal(seen[0].instanceId,'instance');assert.deepEqual(seen[0].actor,ctx.v4.actor);
  assert.equal(boot.transport.session.instance_id,'instance');assert.equal(f.calls[0].url,'/api/business-os/launch-context');
 }finally{await boot.loader.close();for(const p of h.peers)p.close();}
 const given=[],resolver=()=>({startShell(){},stopShell(){}}),h2=harness();
 const withShell=await bootV4({fetchImpl:fetchFake([context()]).impl,createPeer:h2.createPeer,resolveShell:resolver,loaderOptions,loaderFactory:opts=>{given.push(opts);return createLoader(opts);}});
 try{assert.equal(given[0].resolveShell,resolver);}finally{await withShell.loader.close();for(const p of h2.peers)p.close();}
});

test('a non-function resolveShell is a typed error before any control-plane request',async()=>{
 for(const value of [null,'v4/frame/index.js',{},42]){
  const f=fetchFake([context()]);
  await assert.rejects(bootV4({fetchImpl:f.impl,createPeer:harness().createPeer,resolveShell:value,loaderOptions}),e=>e instanceof BootV4Error&&e.reason==='invalid_options'&&e.code==='invalid',String(value));
  assert.equal(f.calls.length,0);
 }
});

test('hello_ok capabilities reach the kernel through transport.capabilities (E43.2)',async()=>{
 const required=['agent.coding.cancel','agent.coding.turn'];
 const pages={business_apps:[{id:'coder',active_ver:'1'}],business_app_versions:[{id:'coder@1',app_id:'coder',ver:'1',manifest:{name:'Coder',entry:'index.js',files:{},collections:{},capabilities_required:required,min_app_api:'4.0'}}]};
 const idle=new WebRTCKernelTransport({channels:protocolChannels().client,hello:offer('instance'),resolveScope:()=>null,verifyPackage:async()=>false});
 assert.equal(idle.capabilities,null,'unknown before the authenticated hello');idle.close();
 for(const [grant,expected] of [[[...required,'agent.other'],'activated'],[[],'failed']]){
  const h=harness({grant,pages});
  const boot=await bootV4({fetchImpl:fetchFake([context({capabilities:[...required,'agent.other']})]).impl,createPeer:h.createPeer,loaderOptions:{...loaderOptions,dbName:`ctox-caps-${expected}-${process.pid}`}});
  try{
   assert.deepEqual(boot.transport.capabilities,[...grant].sort());
   let state;for(let n=0;n<300;n++){state=boot.kernel.activation.get('coder').get();if(['activated','failed'].includes(state?.state))break;await tick();}
   assert.equal(state?.state,expected,JSON.stringify(state));
   if(expected==='failed')assert.equal(state.error,'needs_update:agent.coding.cancel,agent.coding.turn');
  }finally{await boot.loader.close();for(const p of h.peers)p.close();}
 }
});
