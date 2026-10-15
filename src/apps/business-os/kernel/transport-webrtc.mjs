import {KernelTransport,seqGreater} from './transport.mjs';
import {CtoxError,uuidv7,sha256Hex} from './values.mjs';
import {HelloConnection,decimalSeq} from '../rxdb/src/hello.mjs';
import {SendScheduler,LANES} from './scheduler.mjs';
import {LaneIngress} from './ingress.mjs';
const clock=()=>Date.now();
const size=65536;
const b64=bytes=>{let s='';for(let i=0;i<bytes.length;i+=32768)s+=String.fromCharCode(...bytes.subarray(i,i+32768));return btoa(s);};
const unb64=s=>Uint8Array.from(atob(s),c=>c.charCodeAt(0));
function deferred(){let resolve,reject;const promise=new Promise((r,j)=>{resolve=r;reject=j});promise.catch(()=>{});return {promise,resolve,reject};}
/** Real browser WebRTC adapter. Pairing/signaling and device signing belong to
 * the host: createPeer(address,identity) returns an RTCDataChannel map. Neither
 * HTTP records nor V3 query/replication fallbacks exist in this adapter.
 */
export class WebRTCKernelTransport extends KernelTransport {
 constructor(options){super();this.options=options;this.now=options.now??clock;this.readyState=deferred();this.ready=this.readyState.promise;this.listeners=[];this.pending=new Map();this.closed=false;this.scopes=new Map();this.samples=0;this.offset=null;}
 async connect(identity,events){
  if(this.events)throw new CtoxError('conflict','Transport already bound');this.identity={...identity};this.events=events;this.started=this.now();
  try {
   const hello=typeof this.options.hello==='function'?await this.options.hello(identity):this.options.hello;
   if(hello?.instance_id!==identity.instanceId)throw new CtoxError('forbidden','Hello identity mismatch');
   if(typeof this.options.verifyPackage!=='function')throw new CtoxError('forbidden','Authenticated package verifier required');
   this.hello=new HelloConnection(hello,state=>{if(state.state==='closed'&&state.rejection)this.fail(new CtoxError('unavailable',`hello rejected: ${state.rejection.reason}`));},{handshakeTimeoutMs:this.options.handshakeTimeoutMs??10000});
   const endpoint=this.options.channels?{channels:this.options.channels,peer:this.options.peer}:await Promise.race([this.options.createPeer(this.options.address,identity),this.ready.then(()=>{throw new CtoxError('unavailable','Handshake ended before connection setup');})]);
   if(this.closed){endpoint.close?.();throw new CtoxError('unavailable','Transport closed');}
   this.endpoint=endpoint;this.channels=endpoint.channels;
   if(!this.channels?.[0])throw new CtoxError('invalid','Control data channel missing');
   this.scheduler=new SendScheduler(this.channels,{codec:'gzip',adapters:this.options.codecs,onError:e=>this.fail(e)});
   this.ingress=new LaneIngress({apply:(m,l)=>this.apply(m,l),ack:(lane,upto)=>this.scheduler.plain({type:'ack',lane,upto:String(upto)}),onError:e=>this.fail(e),adapters:this.options.codecs});
   for(const channel of new Set(Object.values(this.channels))){
    channel.binaryType='arraybuffer';
    this.listen(channel,'message',event=>{this.receive(event.data).catch(e=>this.fail(e));});
    this.listen(channel,'close',()=>this.fail(new CtoxError('unavailable','WebRTC channel closed')));
    this.listen(channel,'error',()=>this.fail(new CtoxError('unavailable','WebRTC channel error')));
   }
   const control=this.channels[0];
   if(control.readyState==='open')await this.startHello();else this.listen(control,'open',()=>{this.startHello().catch(e=>this.fail(e));});
   await this.ready;
  }catch(error){this.fail(error);throw error;}
 }
 listen(target,name,fn){target.addEventListener(name,fn);this.listeners.push([target,name,fn]);}
 async startHello(){this.started=this.now();await this.scheduler.plain(this.hello.connected());}
 async receive(data){
  if(this.closed)return;
  if(typeof data==='string'){
   let message;try{message=JSON.parse(data)}catch{throw new CtoxError('invalid','Invalid control JSON')}
   if(message.type==='ack'){
    if(!this.session || !Number.isInteger(message.lane)||message.lane<0||message.lane>5)throw new CtoxError('invalid','Unexpected ACK');
    decimalSeq(message.upto);this.scheduler.ack(message.lane,message.upto);return;
   }
   if(this.session)throw new CtoxError('invalid','Business messages must be framed');
   // P4-a owns the strict negotiation fields. This optional clock extension
   // grants no authority and is validated independently (§16.18).
   const {core_time_ms,...contract}=message;
   const selected=this.hello.receive(JSON.stringify(contract));
   if(selected.type!=='hello_ok')throw new CtoxError('unavailable','Negotiation rejected');
   if(selected.transport.codec==='zstd'&&!this.options.codecs?.zstd)throw new CtoxError('unavailable','Negotiated codec adapter unavailable');
   this.session=selected;this.scheduler.configure(selected.transport);this.ingress.codec=selected.transport.codec;this.ingress.frameBytes=selected.transport.frame_bytes;
   if(core_time_ms!==undefined){if(!Number.isFinite(core_time_ms)||core_time_ms<0)throw new CtoxError('invalid','Invalid Core clock');this.offset=core_time_ms-(this.started+this.now())/2;}
   this.rtt=this.now()-this.started;this.events.session?.({epoch:selected.epoch,head_seq:selected.head_seq,rights_ver:selected.rights_ver});this.hello.activate();this.events.link({state:'online',rtt_ms:this.now()-this.started,relay:!!this.endpoint.relay});this.readyState.resolve(selected);return;
  }
  if(!this.session)throw new CtoxError('forbidden','Data before authenticated hello');
  const bytes=data instanceof Blob?new Uint8Array(await data.arrayBuffer()):data instanceof ArrayBuffer?new Uint8Array(data):data;
  const lane=bytes?.[4];if(!Number.isInteger(lane)||lane>5)throw new CtoxError('invalid','Invalid lane');await this.ingress.push(lane,bytes);
 }
 async apply(message,lane){
  if(this.closed)return;
  switch(message.type){
   case 'page': {
    if(!await this.options.verifyPackage(message,this.session))throw new CtoxError('forbidden','Page signature invalid');
    const cursor=decimalSeq(message.cursor_to),head=decimalSeq(message.head_seq??message.watermark_seq),from=decimalSeq(message.seq_from??'0'),to=decimalSeq(message.seq_to??cursor);
    const sub=this.scopes.get(message.collection);
    if(!sub || message.scope_hash!==sub.scope_hash || String(message.epoch)!==String(this.session.epoch) || message.gen!==sub.gen)throw new CtoxError('revoked','Page scope/epoch/generation mismatch');
    if(!message.package_id||seqGreater(cursor,head)||seqGreater(from,to)||seqGreater(to,cursor)||(message.complete&&cursor!==head))throw new CtoxError('invalid','Invalid page progress proof');
    for(const doc of message.docs??[])if(seqGreater(decimalSeq(doc.seq),cursor))throw new CtoxError('invalid','Document exceeds cursor');
    if(!Number.isFinite(message.core_sent_at_ms)||message.core_sent_at_ms<0)throw new CtoxError('invalid','Invalid send timestamp');
    await this.events.page(message);
    if(this.offset!==null)this.events.timing?.({push_latency_ms:Math.max(0,this.now()+this.offset-message.core_sent_at_ms),clock_offset_ms:this.offset,clock_uncertainty_ms:this.rtt/2,samples:++this.samples});
    return;
   }
   case 'scope_revoke':if(!await this.options.verifyPackage(message,this.session))throw new CtoxError('forbidden','Revocation signature invalid');await this.events.revoke?.(message);return;
   case 'write_result':
    if(typeof message.op_id!=='string'||!['accepted','rejected','migrated'].includes(message.status))throw new CtoxError('invalid','Invalid write result');
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
 async scope(collection){await this.ready;const resolved=await this.options.resolveScope?.(collection,this.session);if(!resolved?.scope_hash || !Number.isSafeInteger(resolved.gen))throw new CtoxError('forbidden','Canonical subscription scope missing');return {...resolved,collection,epoch:this.session.epoch};}
 async subscribe(scope){await this.ready;const sub={...scope,epoch:this.session.epoch};decimalSeq(sub.checkpoint_seq??'0');this.scopes.set(scope.collection,sub);await this.scheduler.send('control',{type:'subscribe',...sub});}
 async write(entry){await this.ready;decimalSeq(entry.base_seq);if(!entry.op_id)throw new CtoxError('invalid','op_id missing');try{await this.scheduler.send('commands',{type:'write',...entry});}catch(error){this.fail(error);throw error;}}
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
  const blob=new Blob(parts,{type:mime});if(await sha256Hex(blob)!==ref)throw new CtoxError('invalid','Transfer hash mismatch');return blob;
 }
 async artifact(hash){return (await this.download(hash,'artifact')).text();}
 async blob(ref){return this.download(ref,'blob');}
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
  if(this.closed)return;this.closed=true;this.hello?.close();this.scheduler?.close();this.ingress?.close();
  for(const [t,n,f]of this.listeners)t.removeEventListener(n,f);this.listeners=[];
  for(const p of this.pending.values()){clearTimeout(p.timer);p.reject(new CtoxError('unavailable','Transport closed'));}this.pending.clear();
  this.readyState.reject(new CtoxError('unavailable','Transport closed'));
  if(this.options.ownsConnection!==false){this.endpoint?.close?.();this.endpoint?.peer?.close();for(const channel of new Set(Object.values(this.channels??{})))channel.close?.();}
 }
}
export function createReliableChannels(peer){const channels={};for(const [name,lane]of Object.entries(LANES))if(lane<6)channels[lane]=peer.createDataChannel(`ctox-v4-${name}`,{ordered:true});return channels;}
