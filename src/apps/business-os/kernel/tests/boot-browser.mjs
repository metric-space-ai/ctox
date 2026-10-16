// E38.4 real-Chromium acceptance for bootV4. Run from the repository root with an existing Playwright package:
//   PLAYWRIGHT_MODULE=<playwright-core> [CHROME_BIN=<chromium>] node src/apps/business-os/kernel/tests/boot-browser.mjs
// Real: HTTP launch-context fetch, Ed25519 WebCrypto verification, RTCPeerConnection + createReliableChannels,
// the built-in signaling client, /sw.js registration and createLoader. Fixture: the signaling server is an in-page
// WebSocket relay and the native side is FakeNativePeer on a second in-page RTCPeerConnection (no daemon, no tenant).
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {resolve,extname} from 'node:path';
import {createRequire} from 'node:module';
import {generateKeyPairSync} from 'node:crypto';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE??'playwright');
const app=resolve('src/apps/business-os'),kernel=resolve(app,'kernel');
const {privateKey,publicKey}=generateKeyPairSync('ed25519');
const signingJwk=privateKey.export({format:'jwk'}),publicRaw=publicKey.export({format:'jwk'}).x;
const COLLECTIONS=['business_apps','business_app_versions','business_commands','files','shell_preferences','app_user_preferences','app_drafts','notes'];
const requests=[];let contextFetches=0;
function launchContext(){
 const nonce=Buffer.from(String(++contextFetches).padStart(32,'0')).toString('base64url').slice(0,43);
 return {session:{},config:null,designTemplates:[],v4:{instanceId:'boot-browser',userId:'alice',actor:{id:'alice',role:'owner'},
  signaling:{urls:['wss://signal.invalid/ws'],room:'ctox-v4:boot-browser',peerId:'browser-1',nativePeerId:'native-1',token:'signal-token',iceServers:[]},
  hello:{offer:{type:'hello',protocol_ver:4,bootstrap_ver:1,app_api_ver:{min:'4.0',max:'4.0'},instance_id:'boot-browser',capabilities:[],
   device_proof:{device_id:'browser-device',public_key:'A'.repeat(43),nonce,signature:'C'.repeat(86)},scopes_wanted:COLLECTIONS.filter(c=>c!=='notes'),
   transport:{codecs:['gzip'],max_frame_bytes:16384,fragmentation:true,reassembly:true}}},
  packageKeys:[{keyId:'instance-key-1',alg:'Ed25519',publicKey:publicRaw}]}};
}
const server=createServer(async(req,res)=>{
 const path=decodeURIComponent(new URL(req.url,'http://local').pathname);requests.push(path);
 if(/(?:^|\/)(?:fake-core[^/]*|testkit)\.mjs$/.test(path)){res.statusCode=404;res.end('Excluded from production slot');return;}
 try{
  if(path==='/api/business-os/launch-context'){
   if(req.headers.cookie!=='ctox_session=fixture'){res.statusCode=401;res.end('{}');return;}
   res.setHeader('Content-Type','application/json');res.end(JSON.stringify(launchContext()));return;
  }
  if(path==='/sw.js'){res.setHeader('Content-Type','text/javascript');res.end("self.addEventListener('install',()=>self.skipWaiting());self.addEventListener('message',event=>{if(event.data==='ctox:claim')event.waitUntil(clients.claim());});");return;}
  if(path==='/'){res.setHeader('Content-Type','text/html');res.setHeader('Set-Cookie','ctox_session=fixture; Path=/; SameSite=Strict');res.end('<!doctype html><meta charset="utf-8"><title>bootV4 acceptance</title><pre id="result">Running bootV4 acceptance</pre>');return;}
  const file=resolve(kernel,'.'+path);
  if(!file.startsWith(kernel+'/'))throw Error('outside kernel root (E43.1: no /rxdb/* or /shared/* bootstrap routes)');
  res.setHeader('Content-Type',['.mjs','.js'].includes(extname(file))?'text/javascript':'text/plain');res.end(await readFile(file));
 }catch{res.statusCode=404;res.end('Not found');}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
let browser,failed=false;const errors=[];
try{
 browser=await chromium.launch({headless:true,...(process.env.CHROME_BIN?{executablePath:process.env.CHROME_BIN}:{})});
 const page=await (await browser.newContext()).newPage();page.on('pageerror',e=>errors.push(String(e)));
 await page.goto(`http://127.0.0.1:${server.address().port}/`);
 const result=await page.evaluate(async({signingJwk,collections})=>{
  const {bootV4,BootV4Error}=await import('/boot.mjs');
  const {canonicalJson,STREAM_DOMAIN,SCOPE_DOMAIN}=await import('/boot.mjs');
  const {FakeNativePeer}=await import('/tests/fake-native-peer.mjs');
  const {decodeHelloFrame,negotiateHello}=await import('/vendor/hello.mjs');
  const enc=s=>new TextEncoder().encode(s),b64url=bytes=>btoa(String.fromCharCode(...bytes)).replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'');
  const key=await crypto.subtle.importKey('jwk',signingJwk,{name:'Ed25519'},false,['sign']);
  const signBytes=async s=>new Uint8Array(await crypto.subtle.sign({name:'Ed25519'},key,enc(s)));
  const shellCollections=collections.filter(c=>c!=='notes'),tokens={};
  for(const appId of ['shell','notes']){const claims={instance_id:'boot-browser',user_id:'alice',app_id:appId,scope_hash:`scope:${appId}`,rights_ver:'1',device_id:'browser-device',exp:Math.floor(Date.now()/1000)+600,collections:(appId==='shell'?shellCollections:['notes']).map(c=>({collection:c,gen:1,rights:['read','write'],field_mask_digest:'fixture:all'}))};tokens[appId]=b64url(enc(canonicalJson({claims,signature:b64url(await signBytes(SCOPE_DOMAIN+canonicalJson(claims)))})));}
  class SignedNative extends FakeNativePeer {
   async receive(data){
    if(typeof data==='string'&&JSON.parse(data).type!=='ack'){const m=JSON.parse(data),{session_id,resume_from,...p4}=m;this.native=session_id!==undefined;if(this.native)this.sessionId=session_id;const hello=decodeHelloFrame(JSON.stringify(p4));this.messages.push(m);
     const selected=negotiateHello({instance_id:this.instanceId,bootstrap_min:1,bootstrap_max:1,app_api_ver:{min:'4.0',max:'4.0'},capabilities:[],required_capabilities:[],transport:hello.transport},hello);
     this.channels[0].send(JSON.stringify({type:'hello_ok',protocol_ver:4,instance_id:this.instanceId,...selected,epoch:this.epoch,head_seq:this.head,scope_tokens:[tokens.shell],rights_ver:'1',core_time_ms:Date.now(),...(this.native?{session_id:this.sessionId,resume_from:this.received.map(String)}:{})}));return;}
    return super.receive(data);
   }
   async apply(m,lane){if(m.type==='scope.grant.request'){this.messages.push(m);if(lane!==0)throw Error('grant must use control');return this.send('control',{type:'scope.grant',token:tokens[m.app_id]});}return super.apply(m,lane);}
   async page(collection,docs=[]){const m={type:'page',instance_id:this.instanceId,key_id:'instance-key-1',participant:'alice',collection,scope_hash:`scope:${shellCollections.includes(collection)?'shell':collection}`,gen:1,epoch:'1',package_id:`pkg-${Math.random()}`,seq_from:'0',seq_to:this.head,cursor_to:this.head,head_seq:this.head,complete:true,backlog:0,core_sent_at_ms:Date.now(),docs};
    return this.send('initial',{...m,sig:[...await signBytes(STREAM_DOMAIN+canonicalJson(m))]});}
  }
  // Native side: a second real RTCPeerConnection answering the browser's offer.
  const lanes={'ctox-v4-control':0,'ctox-v4-commands':1,'ctox-v4-interactive':2,'ctox-v4-live':3,'ctox-v4-initial-sync':4,'ctox-v4-transfer':5};
  const nativePc=new RTCPeerConnection({iceServers:[]}),incoming={},buffered=[],labels=[];let native,toBrowser,signalLog=[];
  const nativeReady=new Promise(resolveReady=>{nativePc.ondatachannel=({channel})=>{labels.push(channel.label);const lane=lanes[channel.label];incoming[lane]=channel;channel.binaryType='arraybuffer';
   const hold=e=>buffered.push([channel,e.data,hold]);channel.addEventListener('message',hold);
   if(Object.keys(incoming).length===6){native=new SignedNative(incoming,{instanceId:'boot-browser',onSubscribe:async(sub,p)=>{if(sub.collection==='notes')await p.page('notes',[{id:'server',data:{id:'server',title:'via real WebRTC'},seq:p.head,rev:'r'}]);}});
    for(const [c,data,fn] of buffered.splice(0)){c.removeEventListener('message',fn);c.dispatchEvent(new MessageEvent('message',{data}));}resolveReady();}}});
  nativePc.onicecandidate=e=>{if(e.candidate)toBrowser?.({type:'signal',senderPeerId:'native-1',data:{type:'candidate',candidate:e.candidate.toJSON()}});};
  // Signaling fixture: an in-page relay with the WebSocket surface the boot signaling client uses.
  const sockets=[];
  globalThis.WebSocket=class {
   constructor(url){this.url=url;this.readyState=0;sockets.push(this);toBrowser=m=>queueMicrotask(()=>this.onmessage?.({data:JSON.stringify(m)}));setTimeout(()=>{this.readyState=1;this.onopen?.();},0);}
   async send(raw){const m=JSON.parse(raw);signalLog.push(m.type+(m.data?.type?':'+m.data.type:''));
    if(m.type==='join'){toBrowser({type:'joined',peerId:'browser-1'});return;}
    if(m.type!=='signal'||m.receiverPeerId!=='native-1')return;
    if(m.data.type==='offer'){await nativePc.setRemoteDescription(m.data);const answer=await nativePc.createAnswer();await nativePc.setLocalDescription(answer);toBrowser({type:'signal',senderPeerId:'native-1',data:{type:'answer',sdp:answer.sdp}});}
    else if(m.data.type==='candidate')await nativePc.addIceCandidate(m.data.candidate);
   }
   close(){this.readyState=3;}
  };
  // Exactly what Shell v4's v4/boot.js does: no resolveShell, the loader resolves the activated shell entry itself.
  const boot=await bootV4({launchContextUrl:'/api/business-os/launch-context'});
  await nativeReady;await boot.transport.grantApp('notes');
  const api=boot.kernel.appApi('notes','1');const local=api.put('notes',{id:'client',title:'local'});await local.local;const receipt=await local.result;
  for(let n=0;n<400&&!api.get('notes','server').get();n++)await new Promise(r=>setTimeout(r,10));
  const url=new URL(sockets[0].url);
  const out={
   online:boot.kernel.metrics.get().link?.state,leader:boot.kernel.isLeader,storage:boot.kernel.storageKind,
   serviceWorker:boot.loader.registration?.active?.scriptURL??null,swScope:boot.loader.registration?.scope??null,
   helloOffer:native.messages[0].device_proof.device_id,scopes:boot.transport.session.scope_tokens.length,receipt:receipt.status,
   page:api.get('notes','server').get()?.title??null,labels:labels.sort(),signal:signalLog,
   signalingQuery:Object.fromEntries(['room','peerId','role','protocol','token'].map(k=>[k,url.searchParams.get(k)])),
   peerConnection:boot.transport.endpoint.peer instanceof RTCPeerConnection,nativeErrors:native.errors.map(String),
  };
  // Typed error in the real browser: the context endpoint without the session cookie answers 401.
  document.cookie='ctox_session=; Max-Age=0; Path=/';
  try{await bootV4({launchContextUrl:'/api/business-os/launch-context'});out.unauthenticated='no error';}catch(error){out.unauthenticated=`${error instanceof BootV4Error}:${error.reason}:${error.code}`;}
  await boot.loader.close();native.close();nativePc.close();
  document.querySelector('#result').textContent='PASS: bootV4 launch context -> real WebRTC transport -> loader';
  return out;
 },{signingJwk,collections:COLLECTIONS});
 const expect=(cond,label)=>{if(!cond){failed=true;console.error('FAIL',label);}};
 expect(!requests.some(path=>/(?:^|\/)(?:fake-core[^/]*|testkit)\.mjs$/.test(path)),'E52 boot never requests unserved fake-core or testkit modules');
 expect(result.online==='online','link online');expect(result.leader===true,'leader');expect(result.storage==='indexeddb','IndexedDB');
 expect(result.serviceWorker&&new URL(result.serviceWorker).pathname==='/sw.js'&&new URL(result.serviceWorker).searchParams.get('db')==='ctox-boot-browser-alice'&&new URL(result.swScope).pathname==='/','same-origin /sw.js?db=<kernel IDB> scope /');
 expect(result.helloOffer==='browser-device','native received the launch-context hello');expect(result.scopes===1,'one verified Shell grant; app grants loaded on control lane');
 expect(result.receipt==='accepted','write accepted over WebRTC');expect(result.page==='via real WebRTC','signed page verified and applied');
 expect(JSON.stringify(result.labels)===JSON.stringify(['ctox-v4-commands','ctox-v4-control','ctox-v4-initial-sync','ctox-v4-interactive','ctox-v4-live','ctox-v4-transfer']),'native reliable lane names');expect(result.peerConnection,'real RTCPeerConnection');
 expect(result.signalingQuery.token==='signal-token'&&result.signalingQuery.role==='browser'&&result.signalingQuery.room==='ctox-v4:boot-browser','signaling query from launch context');
 expect(result.unauthenticated==='true:launch_context_unavailable:unavailable','typed unauthenticated failure');
 expect(result.nativeErrors.length===0,'native fixture clean');expect(errors.length===0,'no page errors: '+errors.join('; '));
 const api=requests.filter(p=>p.startsWith('/api/'));
 expect(api.every(p=>p==='/api/business-os/launch-context')&&api.length===2,'only the launch-context control plane over HTTP');
 console.log(JSON.stringify({...result,httpApi:api,contextFetches},null,1));
 console.log(failed?'BOOT_BROWSER FAIL':'BOOT_BROWSER PASS');
}finally{await browser?.close();server.close();}
if(failed)process.exitCode=1;
