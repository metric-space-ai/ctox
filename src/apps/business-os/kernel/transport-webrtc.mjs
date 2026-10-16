import {KernelTransport,seqGreater} from './transport.mjs';
import {CtoxError,uuidv7,sha256Hex} from './values.mjs';
import {HelloConnection,decimalSeq} from './vendor/hello.mjs';
import {SendScheduler,LANES} from './scheduler.mjs';
import {LaneIngress} from './ingress.mjs';
const clock=()=>Date.now();
const size=65536;
const b64=bytes=>{let s='';for(let i=0;i<bytes.length;i+=32768)s+=String.fromCharCode(...bytes.subarray(i,i+32768));return btoa(s);};
const unb64=s=>Uint8Array.from(atob(s),c=>c.charCodeAt(0));
function deferred(){let resolve,reject;const promise=new Promise((r,j)=>{resolve=r;reject=j});promise.catch(()=>{});return {promise,resolve,reject};}
// Native P5 wire: 37-byte header, decimal u64 application ID, compression per
// fragment (0 raw, 1 zstd, 2 gzip). Kept at the transport boundary: no DB API
// consumes SCTP frames or assigns transport sequence numbers.
const encoder=new TextEncoder(),decoder=new TextDecoder('utf-8',{fatal:true});
const nativeLimits=[128*1024,1024*1024,128*1024,4*1024*1024,8*1024*1024,16*1024*1024];
async function nativeTransform(bytes,codec,decode,adapters,limit=16*1024*1024){
 if(codec==='zstd'){const fn=adapters?.zstd?.[decode?'decode':'encode'];if(!fn)throw new CtoxError('unavailable','zstd adapter required');const output=new Uint8Array(await fn(bytes,limit));if(output.length>limit)throw new CtoxError('quota','Decoded fragment exceeded limit');return output;}
 const stream=new Blob([bytes]).stream().pipeThrough(decode?new DecompressionStream('gzip'):new CompressionStream('gzip'));
 if(!decode)return new Uint8Array(await new Response(stream).arrayBuffer());
 const reader=stream.getReader(),parts=[];let size=0;try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.length;if(size>limit)throw new CtoxError('quota','Decoded fragment exceeded limit');parts.push(value);}}finally{await reader.cancel();}
 const output=new Uint8Array(size);let at=0;for(const part of parts){output.set(part,at);at+=part.length;}return output;
}
export class ReliableClientSession {
 constructor(id=uuidv7()){this.id=id;this.sent=Array(6).fill(0n);this.acked=Array(6).fill(0n);this.delivered=Array(6).fill(0n);this.retained=Array.from({length:6},()=>new Map());this.bytes=Array(6).fill(0);this.generation=0;this.waiters=new Set();}
 hello(){return {session_id:this.id,resume_from:this.delivered.map(String)};}
 attach(remote){
  if(remote.session_id!==this.id||!Array.isArray(remote.resume_from)||remote.resume_from.length!==6)throw new CtoxError('invalid','Session identity mismatch');
  const cursors=remote.resume_from.map(x=>BigInt(decimalSeq(x)));
  for(let lane=0;lane<6;lane++)if(cursors[lane]<this.acked[lane]||cursors[lane]>this.sent[lane])throw new CtoxError('invalid','Invalid resume cursor');
  this.generation++;for(let lane=0;lane<6;lane++)this.ack(lane,String(cursors[lane]));return this.generation;
 }
 ack(lane,upto){
  if(!Number.isInteger(lane)||lane<0||lane>5)throw new CtoxError('invalid','Invalid ACK lane');const seq=BigInt(decimalSeq(upto));
  if(seq>this.sent[lane])throw new CtoxError('invalid','ACK exceeds retained send prefix');
  if(seq>this.acked[lane]){this.acked[lane]=seq;for(const [id,item]of this.retained[lane])if(BigInt(id)<=seq){this.bytes[lane]-=item.raw.length+256;this.retained[lane].delete(id);}}
  for(const wake of this.waiters)wake();this.waiters.clear();
 }
}
export class NativeWire {
 constructor(channels,{apply,onError,adapters={},state=new ReliableClientSession()}){
  this.state=state;this.generation=state.generation;
  Object.assign(this,{channels,apply,onError,adapters});this.codec='gzip';this.frameBytes=16384;this.id=0n;this.closed=false;
  this.queues=Array.from({length:6},()=>[]);this.chains=Array(6).fill(Promise.resolve());this.prepares=Array(6).fill(Promise.resolve());this.pending=new Map();this.queued=Array(6).fill(0);this.incoming=Array(6).fill(0);this.reassembly=0;this.credits=Array(6).fill(0);this.cursor=3;this.listeners=[];
  for(const channel of new Set(Object.values(channels)))this.attach(channel);
 }
 attach(channel){channel.bufferedAmountLowThreshold=16384;for(const name of ['open','bufferedamountlow']){const fn=()=>this.wake();channel.addEventListener(name,fn);this.listeners.push([channel,name,fn]);}}
 configure({codec,frame_bytes}){this.codec=codec;this.frameBytes=frame_bytes;}
 buffered(){return [...new Set(Object.values(this.channels))].reduce((n,c)=>n+(c.bufferedAmount??0),0);}
 plain(message){const text=JSON.stringify(message);if(encoder.encode(text).length>16384)return Promise.reject(new CtoxError('invalid','Control frame too large'));return this.enqueue(0,[text],true);}
 ack(){} // P5 frame credit is SCTP retirement; E14 ACKs messages, not fragments.
 send(lane,message,replay=null){
  lane=typeof lane==='string'?LANES[lane]:lane;
  if(!Number.isInteger(lane)||lane<0||lane>5)return Promise.reject(new CtoxError('invalid','Invalid reliable lane'));
  const raw=encoder.encode(JSON.stringify(message));
  if(raw.length>16*1024*1024||raw.length+256>nativeLimits[lane]||this.queued[lane]+raw.length>nativeLimits[lane])return Promise.reject(new CtoxError('quota','Lane admission exceeded'));
  this.queued[lane]+=raw.length;
  const prepare=this.prepares[lane].then(async()=>{
   if(this.closed)throw new CtoxError('unavailable','Connection closed');
   while(!replay&&this.state.bytes[lane]+raw.length+256>nativeLimits[lane]){
    await new Promise((resolve,reject)=>{const wake=()=>{this.waiting?.delete(stop);resolve();};const stop=()=>{this.state.waiters.delete(wake);reject(new CtoxError('unavailable','Connection closed'));};(this.waiting??=new Set()).add(stop);this.state.waiters.add(wake);});if(this.closed)throw new CtoxError('unavailable','Connection closed');
   }
   const id=replay??String(this.state.sent[lane]+1n);decimalSeq(id);const step=this.frameBytes-37,count=Math.max(1,Math.ceil(raw.length/step)),frames=[];
   for(let index=0;index<count;index++){
    const part=raw.slice(index*step,(index+1)*step);let payload=part,codec=0;
    if(part.length>=1024){const compressed=await nativeTransform(part,this.codec,false,this.adapters);if(compressed.length<part.length){payload=compressed;codec=this.codec==='gzip'?2:1;}}
    const bytes=new Uint8Array(37+payload.length),view=new DataView(bytes.buffer);bytes.set([67,84,88,52,lane,codec,id.length]);bytes.set(encoder.encode(id),7);view.setUint32(27,index);view.setUint32(31,count);view.setUint16(35,part.length);bytes.set(payload,37);frames.push(bytes);
   }
   if(this.closed||this.state.generation!==this.generation)throw new CtoxError('revoked','Retired sender generation');
   if(!replay){this.state.sent[lane]=BigInt(id);this.state.retained[lane].set(id,{message:JSON.parse(decoder.decode(raw)),raw});this.state.bytes[lane]+=raw.length+256;}
   return frames;
  });
  this.prepares[lane]=prepare.then(()=>{},()=>{});
  return prepare.then(frames=>this.enqueue(lane,frames,false)).finally(()=>{this.queued[lane]-=raw.length;});
 }
 replay(){return Promise.all(this.state.retained.flatMap((items,lane)=>[...items].map(([id,item])=>this.send(lane,item.message,id))));}
 async sendAck(lane){
  const message={type:'ack',session_id:this.state.id,lane,upto:String(this.state.delivered[lane])},raw=encoder.encode(JSON.stringify(message)),bytes=new Uint8Array(37+raw.length),v=new DataView(bytes.buffer);bytes.set([67,84,88,52,0,0,1,48]);v.setUint32(31,1);v.setUint16(35,raw.length);bytes.set(raw,37);await this.enqueue(0,[bytes],true);
 }
 enqueue(lane,frames,plain){if(this.closed)return Promise.reject(new CtoxError('unavailable','Connection closed'));return new Promise((resolve,reject)=>{const entry={frames,at:0,resolve,reject};plain?this.queues[lane].unshift(entry):this.queues[lane].push(entry);this.wake();});}
 next(){
  const ready=l=>this.queues[l].length&&(this.channels[l]??this.channels[0])?.readyState==='open';
  for(let lane=0;lane<3;lane++)if(ready(lane))return lane;
  for(let attempt=0;attempt<6;attempt++){const lane=this.cursor;if(!ready(lane)){this.credits[lane]=0;this.cursor=lane===5?3:lane+1;continue;}const entry=this.queues[lane][0],cost=entry.frames[entry.at].byteLength;if(this.credits[lane]<cost){this.credits[lane]+=[8,3,1][lane-3]*this.frameBytes;this.cursor=lane===5?3:lane+1;continue;}this.credits[lane]-=cost;return lane;}return null;
 }
 wake(){if(this.closed||this.scheduled)return;this.scheduled=true;queueMicrotask(()=>{this.scheduled=false;try{for(;;){const lane=this.next();if(lane===null)return;const item=this.queues[lane][0],frame=item.frames[item.at],length=typeof frame==='string'?encoder.encode(frame).length:frame.byteLength;if(this.buffered()+length>65536)return;(this.channels[lane]??this.channels[0]).send(frame);if(++item.at===item.frames.length){this.queues[lane].shift();item.resolve();}}}catch(error){this.onError(error);}});}
 push(lane,bytes){
  if(!Number.isInteger(lane)||lane<0||lane>5)return Promise.reject(new CtoxError('invalid','Invalid ingress lane'));
  // Match the peer's reliable lane windows, plus bounded framing overhead.
  // SCTP's 128 KiB association budget is separate from queued application work.
  const budget=nativeLimits[lane]+37*Math.ceil(nativeLimits[lane]/(this.frameBytes-37));
  if(this.incoming[lane]+bytes.length>budget)return Promise.reject(new CtoxError('quota','Ingress queue exceeded'));this.incoming[lane]+=bytes.length;
  const next=this.chains[lane].then(()=>this.consume(lane,bytes)).finally(()=>{this.incoming[lane]-=bytes.length;});this.chains[lane]=next.catch(this.onError);return next;
 }
 async consume(lane,bytes){
  if(this.closed)throw new CtoxError('unavailable','Ingress closed');
  const bad=()=>{throw new CtoxError('invalid','Invalid native V4 frame');};
  if(bytes.length<37||bytes.length>this.frameBytes||bytes[0]!==67||bytes[1]!==84||bytes[2]!==88||bytes[3]!==52||bytes[4]!==lane)bad();
  const length=bytes[6];if(length<1||length>20||bytes.slice(7+length,27).some(Boolean))bad();const id=decimalSeq(decoder.decode(bytes.slice(7,7+length))),view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),index=view.getUint32(27),count=view.getUint32(31),rawLength=view.getUint16(35),step=this.frameBytes-37;
  if(!count||count>Math.ceil(16*1024*1024/step)||index>=count||rawLength>step||(index+1<count&&rawLength!==step))bad();
  let raw=bytes.slice(37);if(bytes[5]){if(bytes[5]!==({gzip:2,zstd:1})[this.codec]||rawLength<1024)bad();raw=await nativeTransform(raw,this.codec,true,this.adapters,rawLength);}if(raw.length!==rawLength)bad();
  const key=`${lane}:${id}`;let part=this.pending.get(key);if(!part){if(index!==0||this.pending.size>=32)bad();part={count,next:0,parts:[],size:0};this.pending.set(key,part);}
  if(part.next!==index||part.count!==count||part.size+raw.length>16*1024*1024||this.reassembly+raw.length>32*1024*1024)bad();
  part.next++;part.parts.push(raw);part.size+=raw.length;this.reassembly+=raw.length;
  if(part.next===count){
   const joined=new Uint8Array(part.size);let at=0;for(const bytes of part.parts){joined.set(bytes,at);at+=bytes.length;}this.pending.delete(key);this.reassembly-=part.size;const message=JSON.parse(decoder.decode(joined));
   if(id==='0'){if(lane!==0||message.type!=='ack'||message.session_id!==this.state.id||Object.keys(message).sort().join(',')!=='lane,session_id,type,upto')bad();this.state.ack(message.lane,message.upto);return;}
   if(this.state.generation!==this.generation)throw new CtoxError('revoked','Retired session generation');
   if(BigInt(id)<=this.state.delivered[lane]){await this.sendAck(lane);return;}
   if(BigInt(id)!==this.state.delivered[lane]+1n)throw new CtoxError('invalid','Session sequence gap');
   await this.apply(message,lane);
   if(this.closed||this.state.generation!==this.generation)throw new CtoxError('revoked','Retired delivery');this.state.delivered[lane]=BigInt(id);await this.sendAck(lane);
  }
 }
 close(){this.closed=true;for(const stop of this.waiting??[])stop();for(const [c,n,f]of this.listeners)c.removeEventListener(n,f);for(const q of this.queues)for(const entry of q.splice(0))entry.reject(new CtoxError('unavailable','Connection closed'));this.pending.clear();}
}
/** Real browser WebRTC adapter. Pairing/signaling and device signing belong to
 * the host: createPeer(address,identity) returns an RTCDataChannel map. Neither
 * HTTP records nor V3 query/replication fallbacks exist in this adapter.
 */
export class WebRTCKernelTransport extends KernelTransport {
 constructor(options){super();this.options=options.authSession?{...options,hello:(...args)=>options.authSession.hello(...args)}:options;this.reliable=new ReliableClientSession();this.now=options.now??clock;this.readyState=deferred();this.ready=this.readyState.promise;this.listeners=[];this.pending=new Map();this.closed=false;this.scopes=new Map();this.samples=0;this.offset=null;}
 async connect(identity,events){
  if(this.events)throw new CtoxError('conflict','Transport already bound');this.identity={...identity};this.events=events;this.started=this.now();
  try {
   const early=this.options.challengeFirst?await this.options.createPeer(this.options.address,identity):null;
   const challenge=early?await early.challenge:undefined;
   const hello=typeof this.options.hello==='function'?await this.options.hello(identity,challenge):this.options.hello;
   if(hello?.instance_id!==identity.instanceId)throw new CtoxError('forbidden','Hello identity mismatch');
   if(typeof this.options.verifyPackage!=='function')throw new CtoxError('forbidden','Authenticated package verifier required');
   this.hello=new HelloConnection(hello,state=>{if(state.state==='closed'&&state.rejection)this.fail(new CtoxError('unavailable',`hello rejected: ${state.rejection.reason}`));},{handshakeTimeoutMs:this.options.handshakeTimeoutMs??10000});
   const endpoint=early??(this.options.channels?{channels:this.options.channels,peer:this.options.peer}:await Promise.race([this.options.createPeer(this.options.address,identity),this.ready.then(()=>{throw new CtoxError('unavailable','Handshake ended before connection setup');})]));
   if(this.closed){endpoint.close?.();throw new CtoxError('unavailable','Transport closed');}
   this.endpoint=endpoint;this.channels=endpoint.channels;
   if(endpoint.peer?.addEventListener){
    const peerState=()=>{if(['disconnected','failed','closed'].includes(endpoint.peer.connectionState)||['disconnected','failed','closed'].includes(endpoint.peer.iceConnectionState))this.fail(new CtoxError('unavailable','WebRTC peer disconnected'));};
    this.listen(endpoint.peer,'connectionstatechange',peerState);this.listen(endpoint.peer,'iceconnectionstatechange',peerState);peerState();if(this.closed)throw new CtoxError('unavailable','WebRTC peer disconnected');
   }
   if(!this.channels?.[0])throw new CtoxError('invalid','Control data channel missing');
   if(this.options.wireFormat==='k2-fixture'){
    this.scheduler=new SendScheduler(this.channels,{codec:'gzip',adapters:this.options.codecs,onError:e=>this.fail(e)});
    this.ingress=new LaneIngress({apply:(m,l)=>this.apply(m,l),ack:(lane,upto)=>this.scheduler.plain({type:'ack',lane,upto:String(upto)}),onError:e=>this.fail(e),adapters:this.options.codecs});
   }else this.ingress=this.scheduler=new NativeWire(this.channels,{state:this.reliable,apply:(m,l)=>this.apply(m,l),onError:e=>this.fail(e),adapters:this.options.codecs});
   for(const channel of new Set(Object.values(this.channels)))this.attachChannel(channel);
   if(this.options.challengeFirst)this.listen(endpoint.peer,'datachannel',event=>{
    const names=['control','commands','interactive','live','initial-sync','transfer'],lane=names.indexOf(event.channel.label.replace(/^ctox-v4-/,''));
    if(lane<0)return;this.channels[lane]=event.channel;this.scheduler.attach(event.channel);this.attachChannel(event.channel);this.channelsChanged?.();
   });
   const control=this.channels[0];
   if(control.readyState==='open')await this.startHello();else this.listen(control,'open',()=>{this.startHello().catch(e=>this.fail(e));});
   await this.ready;
  }catch(error){this.fail(error);throw error;}
 }
 /** E43.2: capabilities granted by the Core in hello_ok; null until the authenticated hello completed. */
 get capabilities(){return this.session?[...this.session.capabilities??[]]:null;}
 attachChannel(channel){
  channel.binaryType='arraybuffer';this.listen(channel,'open',()=>this.channelsChanged?.());
  this.listen(channel,'message',event=>{this.receive(event.data).catch(e=>this.fail(e));});
  this.listen(channel,'close',()=>this.fail(new CtoxError('unavailable','WebRTC channel closed')));
  this.listen(channel,'error',()=>this.fail(new CtoxError('unavailable','WebRTC channel error')));
 }
 listen(target,name,fn){target.addEventListener(name,fn);this.listeners.push([target,name,fn]);}
 async startHello(){this.started=this.now();const hello=this.hello.connected();await this.scheduler.plain(this.options.wireFormat==='k2-fixture'?hello:{...hello,...this.reliable.hello()});}
 async receive(data){
  if(this.closed)return;
  if(typeof data==='string'){
   let message;try{message=JSON.parse(data)}catch{throw new CtoxError('invalid','Invalid control JSON')}
   if(message.type==='ack'){
    if(this.options.wireFormat!=='k2-fixture')throw new CtoxError('invalid','Native ACK must be framed');
    if(!this.session || !Number.isInteger(message.lane)||message.lane<0||message.lane>5)throw new CtoxError('invalid','Unexpected ACK');
    decimalSeq(message.upto);this.scheduler.ack(message.lane,message.upto);return;
   }
   if(this.session)throw new CtoxError('invalid','Business messages must be framed');
   // P4-a owns the strict negotiation fields. This optional clock extension
   // grants no authority and is validated independently (§16.18).
   const {core_time_ms,session_id,resume_from,...contract}=message;
   const selected=this.hello.receive(JSON.stringify(contract));
   if(selected.type!=='hello_ok')throw new CtoxError('unavailable','Negotiation rejected');
   if(selected.transport.codec==='zstd'&&!this.options.codecs?.zstd)throw new CtoxError('unavailable','Negotiated codec adapter unavailable');
   if(this.options.wireFormat!=='k2-fixture'){this.reliable.attach({session_id,resume_from});this.scheduler.generation=this.reliable.generation;}
   await this.options.authSession?.acceptHello(selected);
   this.session=selected;this.scheduler.configure(selected.transport);this.ingress.codec=selected.transport.codec;this.ingress.frameBytes=selected.transport.frame_bytes;
   if(core_time_ms!==undefined){if(!Number.isFinite(core_time_ms)||core_time_ms<0)throw new CtoxError('invalid','Invalid Core clock');this.offset=core_time_ms-(this.started+this.now())/2;}
   this.rtt=this.now()-this.started;this.events.session?.({epoch:selected.epoch,head_seq:selected.head_seq,rights_ver:selected.rights_ver});this.hello.activate();
   if(this.options.challengeFirst){
    await new Promise((resolve,reject)=>{const check=()=>{if(Object.keys(this.channels).length===6&&Object.values(this.channels).every(c=>c.label.startsWith('ctox-v4-')&&c.readyState==='open')){clearTimeout(timer);this.channelsChanged=null;resolve();}};const timer=setTimeout(()=>{this.channelsChanged=null;reject(new CtoxError('unavailable','Native lane open deadline'));},10000);this.channelsChanged=check;check();});
   }
   if(this.options.wireFormat!=='k2-fixture')await this.scheduler.replay();
   this.events.link({state:'online',rtt_ms:this.rtt,relay:!!this.endpoint.relay});this.readyState.resolve(selected);return;

  }
  if(!this.session)throw new CtoxError('forbidden','Data before authenticated hello');
  const bytes=data instanceof Blob?new Uint8Array(await data.arrayBuffer()):data instanceof ArrayBuffer?new Uint8Array(data):data;
  const lane=bytes?.[4];if(!Number.isInteger(lane)||lane>5)throw new CtoxError('invalid','Invalid lane');await this.ingress.push(lane,bytes);
 }
 async apply(message,lane){
  if(this.closed)return;
  switch(message.type){
   case 'live':case 'page': {
    if(!await this.options.verifyPackage(message,this.session))throw new CtoxError('forbidden','Page signature invalid');
    const cursor=decimalSeq(message.cursor_to??message.seq_to),head=decimalSeq(message.head_seq??message.watermark_seq),from=decimalSeq(message.seq_from??'0'),to=decimalSeq(message.seq_to??cursor);
    const sub=this.scopes.get(message.collection);
    if(!sub || message.scope_hash!==sub.scope_hash || String(message.epoch)!==String(this.session.epoch) || message.gen!==sub.gen)throw new CtoxError('revoked','Page scope/epoch/generation mismatch');
    if(!message.package_id||seqGreater(cursor,head)||seqGreater(from,to)||seqGreater(to,cursor)||(message.complete&&cursor!==head))throw new CtoxError('invalid','Invalid page progress proof');
    for(const doc of message.docs??[])if(seqGreater(decimalSeq(doc.seq),cursor))throw new CtoxError('invalid','Document exceeds cursor');
    if(!Number.isFinite(message.core_sent_at_ms)||message.core_sent_at_ms<0)throw new CtoxError('invalid','Invalid send timestamp');
    await this.events.page({...message,cursor_to:cursor,docs:(message.docs??[]).map(doc=>({...doc,data:doc.body??doc.data??doc.doc}))});
    if(this.offset!==null)this.events.timing?.({push_latency_ms:Math.max(0,this.now()+this.offset-message.core_sent_at_ms),clock_offset_ms:this.offset,clock_uncertainty_ms:this.rtt/2,samples:++this.samples});
    return;
   }
   case 'scope_revoke':if(!await this.options.verifyPackage(message,this.session))throw new CtoxError('forbidden','Revocation signature invalid');await this.events.revoke?.(message);return;
   case 'write_result':
    if(typeof message.op_id!=='string'||!['accepted','rejected','migrated','merged'].includes(message.status))throw new CtoxError('invalid','Invalid write result');
    if(message.status!=='rejected')decimalSeq(message.seq);await this.events.write_result(message);return;
   case 'transfer_data':case 'transfer_result': {
    const pending=this.pending.get(message.request_id);if(!pending)return;
    this.pending.delete(message.request_id);clearTimeout(pending.timer);
    message.error?pending.reject(new CtoxError(message.error.code??'unavailable',message.error.message)):pending.resolve(message);return;
   }
   case 'actor':if(message.actor?.id!==this.identity.userId)throw new CtoxError('forbidden','Actor mismatch');this.events.actor?.(message.actor);return;
   case 'status':this.events.status?.(message.app_id,message.item);return;
   default:throw new CtoxError('invalid',`Unknown V4 message ${message.type}`);
  }
 }
 async scope(collection){await this.ready;if(this.options.authSession)return this.options.authSession.scope(collection);const resolved=await this.options.resolveScope?.(collection,this.session);if(!resolved?.scope_hash || !Number.isSafeInteger(resolved.gen))throw new CtoxError('forbidden','Canonical subscription scope missing');return {...resolved,collection,epoch:this.session.epoch};}
 async subscribe(scope){await this.ready;const sub={...scope,epoch:this.session.epoch};decimalSeq(sub.checkpoint_seq??'0');this.scopes.set(scope.collection,sub);if(this.options.wireFormat==='k2-fixture')await this.scheduler.send('control',{type:'subscribe',...sub,checkpoint:sub.checkpoint_seq==='0'?null:{instance_id:this.identity.instanceId,participant:sub.participant??this.identity.userId,collection:sub.collection,scope_hash:sub.scope_hash,gen:sub.gen,epoch:String(this.session.epoch),seq:sub.checkpoint_seq}});
  else await this.scheduler.send('control',{type:'subscribe',collection:sub.collection,scope_hash:sub.scope_hash,gen:sub.gen,priority:sub.priority??'P2',checkpoint:sub.checkpoint_seq==='0'?null:{instance_id:this.identity.instanceId,participant:this.options.participant??this.identity.userId,collection:sub.collection,scope_hash:sub.scope_hash,gen:sub.gen,epoch:String(this.session.epoch),seq:sub.checkpoint_seq}});
}
 async write(entry){await this.ready;decimalSeq(entry.base_seq);if(this.options.wireFormat!=='k2-fixture'&&(typeof entry.client_seq!=='string'||decimalSeq(entry.client_seq)==='0'))throw new CtoxError('invalid','Positive durable client_seq required');if(!entry.op_id)throw new CtoxError('invalid','op_id missing');try{const wire=this.options.wireFormat==='k2-fixture'?{type:'write',...entry}:{type:'write',op_id:entry.op_id,client_seq:decimalSeq(entry.client_seq),prev_op_id:entry.prev_op_id??null,collection:entry.collection,id:entry.id,base_rev:entry.base_rev??null,base_seq:entry.base_seq==='0'&&entry.base_rev==null?null:entry.base_seq??null,gen:entry.gen,schema_ver:entry.schema_ver,app_ver:entry.app_ver,op:entry.op==='remove'?'delete':entry.op,body:entry.body??null};await this.scheduler.send('commands',wire);
}catch(error){this.fail(error);throw error;}}
 async appActivation(report){await this.ready;await this.scheduler.send('commands',{type:'app_activation',...report,at_seq:decimalSeq(report.at_seq)});}
 priority(hint){this.ready.then(()=>Promise.all(hint.collections.map(collection=>this.scheduler.send('control',{type:'priority',collection,priority:'P1'})))).catch(e=>this.events?.status?.('kernel',{level:'warn',code:'transport.priority',text:e.message}));}
 async request(type,body){
  await this.ready;if(this.closed)throw new CtoxError('unavailable','Transport closed');const request_id=uuidv7(),done=deferred();
  const timer=setTimeout(()=>{this.pending.delete(request_id);done.reject(new CtoxError('unavailable','Transfer deadline exceeded'));},this.options.transferTimeoutMs??10000);
  this.pending.set(request_id,{...done,timer});
  this.scheduler.send('transfer',{type,request_id,...body}).catch(error=>{clearTimeout(timer);this.pending.delete(request_id);done.reject(error)});return done.promise;
 }
 async download(ref,kind){
  const parts=[];let total=null,mime='application/octet-stream';
  const take=(reply,offset)=>{
   if(reply.ref!==ref||reply.offset!==offset||!Number.isSafeInteger(reply.total)||reply.total<offset||reply.total>2_000_000_000||(total!==null&&reply.total!==total))throw new CtoxError('invalid','Invalid transfer range');
   let part;try{part=unb64(reply.data);}catch{throw new CtoxError('invalid','Invalid transfer encoding');}
   if(part.length!==Math.min(size,reply.total-offset))throw new CtoxError('invalid','Invalid transfer chunk length');parts.push(part);total=reply.total;mime=reply.mime??mime;
  };
  take(await this.request('transfer_get',{ref,kind,offset:0,limit:size}),0);
  for(let offset=size;offset<total;){const batch=[];for(let n=0;n<4&&offset<total;n++,offset+=size){const at=offset;batch.push(this.request('transfer_get',{ref,kind,offset:at,limit:size}).then(reply=>({reply,offset:at})));}for(const item of await Promise.all(batch))take(item.reply,item.offset);}
  const blob=new Blob(parts,{type:mime});if(await sha256Hex(blob)!==ref)throw new CtoxError('invalid','Transfer hash mismatch',{reason:'hash_mismatch'});return blob;
 }
 async artifact(hash){return (await this.download(hash,'artifact')).text();}
 async blob(ref,range){
  if(range===undefined)return this.download(ref,'blob');
  const {start,end}=range??{};
  if(!Number.isSafeInteger(start)||!Number.isSafeInteger(end)||start<0||end<start)throw new CtoxError('invalid','Invalid byte range',{path:'range'});
  const first=Math.floor(start/size)*size,parts=[];let total=null,mime='application/octet-stream';
  const take=async(reply,offset)=>{
   if(reply.ref!==ref||reply.offset!==offset||!Number.isSafeInteger(reply.total)||reply.total<offset||(total!==null&&reply.total!==total))throw new CtoxError('invalid','Invalid transfer range');
   if(end>reply.total)throw new CtoxError('invalid','Byte range outside file',{path:'range'});
   let part;try{part=unb64(reply.data);}catch{throw new CtoxError('invalid','Invalid transfer encoding');}
   if(part.length!==Math.min(size,reply.total-offset))throw new CtoxError('invalid','Invalid transfer chunk length');
   if(typeof reply.sha256!=='string'||await sha256Hex(part)!==reply.sha256)throw new CtoxError('invalid','Transfer chunk hash mismatch');
   total=reply.total;mime=reply.mime??mime;parts.push(part);
  };
  // Fetch complete aligned CAS chunks; only verified bytes may be sliced.
  await take(await this.request('transfer_get',{ref,kind:'blob',offset:first,limit:size}),first);
  for(let offset=first+size;offset<end;){
   const batch=[];for(let n=0;n<4&&offset<end;n++,offset+=size){const at=offset;batch.push(this.request('transfer_get',{ref,kind:'blob',offset:at,limit:size}).then(reply=>({reply,offset:at})));}
   for(const item of await Promise.all(batch))await take(item.reply,item.offset);
  }
  return new Blob(parts,{type:mime}).slice(start-first,end-first,mime);
 }
 async upload(ref,blob){
  if(await sha256Hex(blob)!==ref)throw new CtoxError('invalid','Upload hash mismatch');
  const accepted=result=>{if(result.status!=='accepted'||result.ref!==ref)throw new CtoxError('unavailable','Upload not accepted');};
  let batch=[];
  for(let offset=0;offset<blob.size||offset===0;){
   const at=offset,data=new Uint8Array(await blob.slice(at,at+size).arrayBuffer());offset+=data.length;const final=offset===blob.size;
   if(final){for(const result of await Promise.all(batch))accepted(result);batch=[];accepted(await this.request('transfer_put',{ref,offset:at,total:blob.size,mime:blob.type,data:b64(data),final:true}));break;}
   batch.push(this.request('transfer_put',{ref,offset:at,total:blob.size,mime:blob.type,data:b64(data),final:false}));
   if(batch.length===4){for(const result of await Promise.all(batch))accepted(result);batch=[];}
  }
 }
 async reconnect(address=this.options.address){
  if(!this.options.createPeer)throw new CtoxError('unavailable','Host connection factory required for reconnect');
  const identity=this.identity,events=this.events;events.link({state:'offline',rtt_ms:0,relay:false});this.close();
  this.options={...this.options,address,channels:undefined,peer:undefined};this.closed=false;this.events=null;this.session=null;this.offset=null;this.readyState=deferred();this.ready=this.readyState.promise;
  return this.connect(identity,events);
 }
 fail(error){if(this.closed)return;this.events?.status?.('kernel',{level:'error',code:'transport.failed',text:error.message});this.events?.link({state:'offline',rtt_ms:0,relay:false});this.readyState.reject(error);this.close();}
 close(){
  if(this.closed)return;this.closed=true;this.options.authSession?.close();this.hello?.close();this.scheduler?.close();this.ingress?.close();
  for(const [t,n,f]of this.listeners)t.removeEventListener(n,f);this.listeners=[];
  for(const p of this.pending.values()){clearTimeout(p.timer);p.reject(new CtoxError('unavailable','Transport closed'));}this.pending.clear();
  this.readyState.reject(new CtoxError('unavailable','Transport closed'));
  if(this.options.ownsConnection!==false){this.endpoint?.close?.();this.endpoint?.peer?.close();for(const channel of new Set(Object.values(this.channels??{})))channel.close?.();}
 }
}
export function createReliableChannels(peer){const channels={};for(const [name,lane]of Object.entries(LANES))if(lane<6)channels[lane]=peer.createDataChannel(`ctox-v4-${name}`,{ordered:true});return channels;}
