// E38.4: one-call v4 browser entry. Launch context (authenticated control plane,
// no business data) -> WebRTCKernelTransport -> createLoader. There is no V2,
// V3 or HTTP data fallback: every failure is a typed BootV4Error.
import {CtoxError} from './values.mjs';
import {createLoader} from './loader.mjs';
import {createKernelSession} from './session.mjs';
import {WebRTCKernelTransport,createReliableChannels} from './transport-webrtc.mjs';
import {decodeHelloFrame} from './vendor/hello.mjs';

export const DEFAULT_LAUNCH_CONTEXT_URL='/api/business-os/launch-context';
export const STREAM_DOMAIN='ctox.sync.v4.stream\0';
export const SCOPE_DOMAIN='ctox:v4:scope\0';
const REASON_CODE=Object.freeze({invalid_options:'invalid',launch_context_unavailable:'unavailable',launch_context_invalid:'invalid',package_verifier_missing:'forbidden',hello_rejected:'forbidden',transport_unavailable:'unavailable'});

export class BootV4Error extends CtoxError {
 constructor(reason,message,detail){super(REASON_CODE[reason]??'invalid',message,{reason,...(detail===undefined?{}:{detail:String(detail)})});this.name='BootV4Error';}
}
const fail=(reason,message,detail)=>{throw new BootV4Error(reason,message,detail);};
const record=v=>v!==null&&typeof v==='object'&&!Array.isArray(v);
const text=v=>typeof v==='string'&&v.trim().length>0;
const B64URL=/^[A-Za-z0-9_-]+$/;
export const canonicalJson=v=>Array.isArray(v)?'['+v.map(canonicalJson).join(',')+']':record(v)?'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+canonicalJson(v[k])).join(',')+'}':JSON.stringify(v);
const utf8=s=>new TextEncoder().encode(s);
const unb64url=s=>{const pad=s.replace(/-/g,'+').replace(/_/g,'/');return Uint8Array.from(atob(pad+'='.repeat((4-pad.length%4)%4)),c=>c.charCodeAt(0));};
const genOf=collection=>{const m=/@g(\d+)$/.exec(collection);return m?Number(m[1]):1;};
function signatureBytes(sig){
 if(Array.isArray(sig)&&sig.length===64&&sig.every(b=>Number.isInteger(b)&&b>=0&&b<256))return Uint8Array.from(sig); // serde Vec<u8>
 if(typeof sig==='string'&&sig.length===86&&B64URL.test(sig))return unb64url(sig);
 return null;
}

/** Validates the `v4` member of the launch-context response. Pure; exported for tests and the daemon contract. */
export function parseV4LaunchContext(payload){
 if(!record(payload)||!Object.prototype.hasOwnProperty.call(payload,'v4')||payload.v4===null)fail('launch_context_unavailable','Launch context carries no v4 member');
 const v4=payload.v4;
 if(!record(v4))fail('launch_context_invalid','Launch context v4 member must be an object');
 const {instanceId,userId,actor,signaling,hello,packageKeys}=v4;
 if(!text(instanceId)||!text(userId))fail('launch_context_invalid','instanceId and userId required');
 if(!record(actor)||actor.id!==userId||!text(actor.role))fail('launch_context_invalid','actor {id === userId, role} required');
 if(!record(signaling))fail('launch_context_invalid','signaling required');
 const {urls,room,peerId,nativePeerId,token,iceServers}=signaling;
 if(!Array.isArray(urls)||!urls.length||!urls.every(u=>{try{return ['ws:','wss:'].includes(new URL(u).protocol);}catch{return false;}}))fail('launch_context_invalid','signaling.urls must be ws:/wss: URLs');
 if(![room,peerId,nativePeerId,token].every(text)||peerId===nativePeerId)fail('launch_context_invalid','signaling room, peerId, nativePeerId and token required');
 if(!Array.isArray(iceServers)||!iceServers.every(s=>record(s)&&(text(s.urls)||(Array.isArray(s.urls)&&s.urls.length&&s.urls.every(text)))&&(s.username===undefined||typeof s.username==='string')&&(s.credential===undefined||typeof s.credential==='string')))fail('launch_context_invalid','signaling.iceServers must be RTCIceServer entries');
 if(!record(hello)||!record(hello.offer))fail('launch_context_invalid','hello.offer required');
 let offer;try{offer=decodeHelloFrame(JSON.stringify(hello.offer));}catch(error){fail('launch_context_invalid','hello.offer is not a valid V4 hello',error.message);}
 if(offer.type!=='hello'||offer.instance_id!==instanceId)fail('launch_context_invalid','hello.offer must be a hello for instanceId');
 if(packageKeys===undefined||packageKeys===null||(Array.isArray(packageKeys)&&!packageKeys.length))fail('package_verifier_missing','Launch context carries no package verification key');
 if(!Array.isArray(packageKeys)||!packageKeys.every(k=>record(k)&&text(k.keyId)&&k.alg==='Ed25519'&&typeof k.publicKey==='string'&&k.publicKey.length===43&&B64URL.test(k.publicKey))||new Set(packageKeys.map(k=>k.keyId)).size!==packageKeys.length)fail('launch_context_invalid','packageKeys must be unique {keyId, alg:"Ed25519", publicKey: base64url(32 bytes)}');
 return Object.freeze({instanceId,userId,actor:Object.freeze({...actor}),
  signaling:Object.freeze({urls:[...urls],room,peerId,nativePeerId,token,iceServers:iceServers.map(s=>({...s}))}),
  hello:Object.freeze({offer}),packageKeys:Object.freeze(packageKeys.map(k=>Object.freeze({keyId:k.keyId,alg:k.alg,publicKey:k.publicKey})))});
}

export async function fetchV4LaunchContext(url=DEFAULT_LAUNCH_CONTEXT_URL,fetchImpl=globalThis.fetch){
 if(typeof fetchImpl!=='function')fail('launch_context_unavailable','fetch unavailable');
 let response;
 try{response=await fetchImpl(url,{method:'GET',credentials:'same-origin',cache:'no-store',headers:{Accept:'application/json'}});}
 catch(error){fail('launch_context_unavailable','Launch context request failed',error?.message??error);}
 if(!response?.ok)fail('launch_context_unavailable',`Launch context HTTP ${response?.status??'unknown'}`);
 let payload;try{payload=await response.json();}catch(error){fail('launch_context_invalid','Launch context is not JSON',error?.message??error);}
 return parseV4LaunchContext(payload);
}

async function importPackageKeys(keys,subtle){
 const imported=new Map();
 try{for(const key of keys)imported.set(key.keyId,await subtle.importKey('raw',unb64url(key.publicKey),{name:'Ed25519'},false,['verify']));}
 catch(error){fail('package_verifier_missing','Ed25519 package verification unavailable',error?.message??error);}
 return imported;
}

/** Instance-key verifier for signed stream packages (v4_stream.rs StreamPacket: domain + canonical JSON without sig). */
export function createPackageVerifier({instanceId,userId,keys,subtle=globalThis.crypto?.subtle}){
 return async message=>{
  if(!record(message))return false;
  const {sig,...unsigned}=message;
  const key=keys.get(unsigned.key_id),signature=signatureBytes(sig);
  if(!key||!signature||unsigned.instance_id!==instanceId||unsigned.participant!==userId)return false;
  try{return await subtle.verify({name:'Ed25519'},key,signature,utf8(STREAM_DOMAIN+canonicalJson(unsigned)));}catch{return false;}
 };
}

export function decodeScopeToken(token){
 if(typeof token!=='string'||token.length>4096||!B64URL.test(token))return null;
 try{const value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(unb64url(token)));return record(value)&&record(value.claims)&&typeof value.signature==='string'?value:null;}catch{return null;}
}

/** Minimal signaling client for the native v4 peer: join, offer, trickle ICE, reliable lane channels. */
export function createSignalingPeerFactory({WebSocketImpl=globalThis.WebSocket,RTCPeerConnectionImpl=globalThis.RTCPeerConnection,timeoutMs=15000}={}){
 return async function createPeer(address){
  if(typeof WebSocketImpl!=='function'||typeof RTCPeerConnectionImpl!=='function')throw new CtoxError('unavailable','WebRTC/WebSocket unavailable');
  let last;
  for(const url of address.urls){try{return await connectOnce(url,address);}catch(error){last=error;}}
  throw last;
 };
 function connectOnce(base,address){
  return new Promise((resolve,reject)=>{
   const url=new URL(base);
   for(const [k,v]of Object.entries({room:address.room,peerId:address.peerId,client:address.peerId,role:'browser',protocol:'ctox-v4',token:address.token}))url.searchParams.set(k,v);
   const ws=new WebSocketImpl(url.href),peer=new RTCPeerConnectionImpl({iceServers:address.iceServers});
   const channels=createReliableChannels(peer),pending=[];let offered=false,remote=false,settled=false;
   const close=()=>{try{ws.close();}catch{}try{peer.close();}catch{}};
   const done=(error)=>{if(settled)return;settled=true;clearTimeout(timer);if(error){close();reject(error);}else resolve({channels,peer,relay:false,close});};
   const timer=setTimeout(()=>done(new CtoxError('unavailable','Signaling/WebRTC connect deadline')),timeoutMs);
   const signal=data=>{if(ws.readyState===1)ws.send(JSON.stringify({type:'signal',room:address.room,senderPeerId:address.peerId,receiverPeerId:address.nativePeerId,data}));};
   peer.onicecandidate=event=>{if(event.candidate)signal({type:'candidate',candidate:event.candidate.toJSON?.()??event.candidate});};
   channels[0].addEventListener('open',()=>done());
   ws.onopen=()=>ws.send(JSON.stringify({type:'join',room:address.room}));
   ws.onerror=()=>done(new CtoxError('unavailable','Signaling socket error'));
   ws.onclose=()=>done(new CtoxError('unavailable','Signaling socket closed'));
   ws.onmessage=async event=>{
    try{
     const message=JSON.parse(typeof event.data==='string'?event.data:new TextDecoder().decode(event.data));
     if(message.type==='ctoxError')return done(new CtoxError(message.code==='peer_revoked'?'revoked':'unavailable',`Signaling rejected: ${message.code??'error'}`));
     if(message.type==='joined'&&!offered){offered=true;const offer=await peer.createOffer();await peer.setLocalDescription(offer);signal({type:offer.type,sdp:offer.sdp});return;}
     if(message.type!=='signal'||String(message.senderPeerId??message.sender??'')!==address.nativePeerId)return;
     const data=message.data??message.signal;
     if(data?.type==='answer'){await peer.setRemoteDescription({type:'answer',sdp:data.sdp});remote=true;for(const c of pending.splice(0))await peer.addIceCandidate(c);}
     else if(data?.type==='candidate'&&data.candidate){if(remote)await peer.addIceCandidate(data.candidate);else pending.push(data.candidate);}
    }catch(error){done(error instanceof CtoxError?error:new CtoxError('unavailable','Signaling failure',{detail:String(error?.message??error)}));}
   };
  });
 }
}

const isHelloRejection=error=>error?.name==='HelloRejected'||error?.code==='forbidden'||/hello rejected|Negotiation rejected/i.test(String(error?.message??''));

/**
 * bootV4: fetch the authenticated launch context, build the WebRTC kernel transport and start the loader.
 * `resolveShell` is optional (absent: createLoader loads the activated shell's manifest entry via kernel.loadModule).
 * `createPeer`, `loaderOptions`, `loaderFactory` and `subtle` are host/test seams; identity, transport and hello always come from the
 * launch context. Never falls back to V2 or HTTP data paths.
 */
export async function bootV4({launchContextUrl=DEFAULT_LAUNCH_CONTEXT_URL,resolveShell,fetchImpl=globalThis.fetch,createPeer,codecs,subtle=globalThis.crypto?.subtle,loaderOptions={},handshakeTimeoutMs,loaderFactory=createLoader}={}){
 // Optional: absent -> the loader resolves the activated shell itself (kernel.loadModule('shell', ver) = manifest entry).
 if(resolveShell!==undefined&&typeof resolveShell!=='function')throw new BootV4Error('invalid_options','resolveShell must be a function when given');
 const context=await fetchV4LaunchContext(launchContextUrl,fetchImpl);
 if(!subtle)fail('package_verifier_missing','WebCrypto unavailable');
 const keys=await importPackageKeys(context.packageKeys,subtle);
 const {instanceId,userId,actor}=context;
 let current=context,used=false;
 // A launch-context hello is single-use (device proof nonce); every reconnect fetches a fresh one.
 const hello=async()=>{
  if(!used){used=true;return current.hello.offer;}
  const next=await fetchV4LaunchContext(launchContextUrl,fetchImpl);
  if(next.instanceId!==instanceId||next.userId!==userId)throw new CtoxError('forbidden','Launch context identity changed');
  current=next;return next.hello.offer;
 };
 const verifyScopeToken=async(token,{rights_ver})=>{
  const value=decodeScopeToken(token),claims=value?.claims;
  if(!claims||claims.instance_id!==instanceId||claims.device_id!==current.hello.offer.device_proof.device_id||String(claims.rights_ver)!==String(rights_ver)||!text(claims.scope_hash)||!(Number(claims.exp)*1000>Date.now()))return false;
  const signature=signatureBytes(value.signature);if(!signature)return false;
  const bytes=utf8(SCOPE_DOMAIN+canonicalJson(claims));
  for(const key of keys.values()){try{if(await subtle.verify({name:'Ed25519'},key,signature,bytes))return true;}catch{}}
  return false;
 };
 // hello_ok.scope_tokens[i] answers hello.scopes_wanted[i] (native policy issues them in request order).
 const resolveScope=(collection,selected)=>{
  const index=current.hello.offer.scopes_wanted.indexOf(collection),claims=index<0?null:decodeScopeToken(selected.scope_tokens[index])?.claims;
  return claims?{scope_hash:claims.scope_hash,gen:genOf(collection),participant:userId}:null;
 };
 const authSession=createKernelSession({instanceId,userId,hello,verifyScopeToken,resolveScope});
 const peerFactory=createPeer??createSignalingPeerFactory();
 const transport=new WebRTCKernelTransport({authSession,verifyPackage:createPackageVerifier({instanceId,userId,keys,subtle}),createPeer:(_address,identity)=>peerFactory(current.signaling,identity),address:context.signaling,codecs,...(handshakeTimeoutMs?{handshakeTimeoutMs}:{})});
 const {instanceId:_i,userId:_u,actor:_a,transport:_t,resolveShell:_r,...extra}=loaderOptions;
 let loader;
 try{loader=await loaderFactory({...extra,instanceId,userId,actor,transport,...(resolveShell===undefined?{}:{resolveShell})});}
 catch(error){transport.close();throw error;}
 if(loader.kernel.isLeader){
  try{await transport.ready;}
  catch(error){
   await loader.close().catch(()=>{});
   throw new BootV4Error(isHelloRejection(error)?'hello_rejected':'transport_unavailable',isHelloRejection(error)?'Native peer rejected the v4 hello':'v4 transport did not come up',error?.message??error);
  }
 }
 return Object.freeze({loader,kernel:loader.kernel,transport,launchContext:context});
}
