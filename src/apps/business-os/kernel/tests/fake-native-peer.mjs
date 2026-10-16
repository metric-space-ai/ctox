import {decodeHelloFrame,negotiateHello} from '../vendor/hello.mjs';
import {sha256Hex} from '../values.mjs';
export const offer=instance_id=>({type:'hello',protocol_ver:4,bootstrap_ver:1,app_api_ver:{min:'4.0',max:'4.0'},instance_id,capabilities:[],device_proof:{device_id:'test-device',public_key:'A'.repeat(43),nonce:'B'.repeat(43),signature:'C'.repeat(86)},scopes_wanted:[],transport:{codecs:['gzip'],max_frame_bytes:16384,fragmentation:true,reassembly:true}});
export const scopeFor=collection=>({scope_hash:`scope:${collection}`,gen:Number(/@g(\d+)$/.exec(collection)?.[1]??1)});
class MemoryChannel extends EventTarget {
 readyState='open';bufferedAmount=0;
 send(data){const value=typeof data==='string'?data:data.slice();queueMicrotask(()=>{if(this.other.readyState==='open')this.other.dispatchEvent(new MessageEvent('message',{data:value}));});}
 close(){if(this.readyState==='closed')return;this.readyState='closed';this.dispatchEvent(new Event('close'));}
}
export function protocolChannels(){const client={},server={};for(let i=0;i<6;i++){client[i]=new MemoryChannel();server[i]=new MemoryChannel();client[i].other=server[i];server[i].other=client[i];}return {client,server};}
const laneId={control:0,commands:1,interactive:2,live:3,initial:4,transfer:5};
async function gzip(bytes,decode=false){return new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(decode?new DecompressionStream('gzip'):new CompressionStream('gzip'))).arrayBuffer());}
/** Independent test-native wire implementation: deliberately does NOT import
 * the client's framer, scheduler or ingress. Only P4's canonical hello is shared.
 */
export class FakeNativePeer {
 constructor(channels,{instanceId='instance',head='9007199254740999',clockOffset=5000,onSubscribe,epoch='1'}={}){
  Object.assign(this,{channels,instanceId,head,clockOffset,onSubscribe,epoch});this.messages=[];this.errors=[];this.blobs=new Map();this.uploads=new Map();this.listeners=[];this.closed=false;
  this.sent=Array(6).fill(0);this.acked=Array(6).fill(0);this.received=Array(6).fill(0);this.packet=0;this.parts=new Map();this.waiters=new Set();this.ingress=Array(6).fill(Promise.resolve());this.egress=Array(6).fill(Promise.resolve());
  for(const channel of Object.values(channels)){
   channel.binaryType='arraybuffer';channel.bufferedAmountLowThreshold=16384;
   for(const [name,fn]of [['message',e=>{this.receive(e.data).catch(err=>this.errors.push(err));}],['bufferedamountlow',()=>this.wake()]]){channel.addEventListener(name,fn);this.listeners.push([channel,name,fn]);}
  }
 }
 wake(){for(const wake of [...this.waiters])wake();}
 async credit(lane,length){
  while(!this.closed&&(this.sent[lane]-this.acked[lane]>=8||Object.values(this.channels).reduce((n,c)=>n+c.bufferedAmount,0)+length>65536)){
   await new Promise((resolve,reject)=>{const wake=()=>{clearTimeout(timer);this.waiters.delete(wake);resolve();};const timer=setTimeout(()=>{this.waiters.delete(wake);reject(Error('Native fixture credit deadline'));},5000);this.waiters.add(wake);});
  }
  if(this.closed)throw Error('Native fixture closed');
 }
 async receive(data){
  if(typeof data==='string'){
   const m=JSON.parse(data);if(m.type==='ack'){const n=Number(m.upto);if(!Number.isSafeInteger(n)||n>this.sent[m.lane])throw Error('Invalid client ACK');this.acked[m.lane]=Math.max(n,this.acked[m.lane]);this.wake();return;}
   const hello=decodeHelloFrame(data);this.messages.push(hello);
   const selected=negotiateHello({instance_id:this.instanceId,bootstrap_min:1,bootstrap_max:1,app_api_ver:{min:'4.0',max:'4.0'},capabilities:[],required_capabilities:[],transport:hello.transport},hello);
   this.channels[0].send(JSON.stringify({type:'hello_ok',protocol_ver:4,instance_id:this.instanceId,...selected,epoch:this.epoch,head_seq:this.head,scope_tokens:['test-scope-token'],rights_ver:'1',core_time_ms:Date.now()+this.clockOffset}));return;
  }
  const bytes=data instanceof ArrayBuffer?new Uint8Array(data):data,lane=bytes[4];if(lane>5)throw Error('Invalid native lane');
  const next=this.ingress[lane].then(()=>this.consume(bytes,lane));this.ingress[lane]=next.catch(()=>{});return next;
 }
 async consume(bytes,lane){
  const d=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
  if(bytes.length<32||bytes.length>16384||d.getUint32(0)!==0x43545834||d.getUint8(5)!==1||d.getUint16(6)!==0)throw Error('Native frame header mismatch');
  const packet=d.getUint32(8),index=d.getUint32(12),count=d.getUint32(16),total=d.getUint32(20),raw=d.getUint32(24),seq=d.getUint32(28),key=`${lane}:${packet}`;
  if(seq!==this.received[lane]+1||!count||index>=count||total>16777216||raw>16777216)throw Error('Native frame progress mismatch');
  let parts=this.parts.get(key);if(!parts){if(index!==0)throw Error('Missing native first fragment');parts={chunks:[],total,raw,count};this.parts.set(key,parts);}
  if(parts.chunks.length!==index||parts.total!==total||parts.raw!==raw||parts.count!==count)throw Error('Native fragment mismatch');parts.chunks.push(bytes.slice(32));
  if(parts.chunks.length===count){
   const encoded=new Uint8Array(total);let at=0;for(const chunk of parts.chunks){encoded.set(chunk,at);at+=chunk.length;}if(at!==total)throw Error('Native encoded length mismatch');
   const decoded=await gzip(encoded,true);if(decoded.length!==raw)throw Error('Native raw length mismatch');await this.apply(JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(decoded)),lane);this.parts.delete(key);
  }
  this.received[lane]=seq;this.channels[0].send(JSON.stringify({type:'ack',lane,upto:String(seq)}));
 }
 send(lane,message){lane=typeof lane==='string'?laneId[lane]:lane;const next=this.egress[lane].then(()=>this.frame(lane,message));this.egress[lane]=next.catch(()=>{});return next;}
 async frame(lane,message){
  const raw=new TextEncoder().encode(JSON.stringify(message)),encoded=await gzip(raw),packet=++this.packet,step=16352,count=Math.max(1,Math.ceil(encoded.length/step));
  for(let index=0;index<count;index++){
   const payload=encoded.slice(index*step,(index+1)*step),frame=new Uint8Array(32+payload.length),d=new DataView(frame.buffer);await this.credit(lane,frame.length);
   d.setUint32(0,0x43545834);d.setUint8(4,lane);d.setUint8(5,1);for(const [at,value]of [[8,packet],[12,index],[16,count],[20,encoded.length],[24,raw.length],[28,++this.sent[lane]]])d.setUint32(at,value);frame.set(payload,32);this.channels[lane].send(frame);
  }
 }
 async apply(m,lane){
  this.messages.push(m);
  if(m.type==='subscribe'){if(lane!==0)throw Error('subscribe outside control');if(this.onSubscribe)await this.onSubscribe(m,this);return;}
  if(m.type==='write'){if(lane!==1||typeof m.base_seq!=='string'||!m.op_id)throw Error('invalid write wire');await this.send('commands',{type:'write_result',op_id:m.op_id,status:'accepted',rev:'core-rev',seq:String(BigInt(this.head)+1n)});return;}
  if(m.type==='priority'||m.type==='app_activation')return;
  if(m.type==='transfer_get'){
   const blob=this.blobs.get(m.ref);if(!blob)throw Error('missing CAS test object');const bytes=new Uint8Array(await blob.slice(m.offset,m.offset+m.limit).arrayBuffer());let text='';for(const b of bytes)text+=String.fromCharCode(b);
   await this.send('transfer',{type:'transfer_data',request_id:m.request_id,ref:m.ref,offset:m.offset,total:blob.size,mime:blob.type,data:btoa(text)});return;
  }
  if(m.type==='transfer_put'){
   const chunks=this.uploads.get(m.ref)??[];chunks.push({offset:m.offset,data:Uint8Array.from(atob(m.data),c=>c.charCodeAt(0))});this.uploads.set(m.ref,chunks);
   if(m.final){const blob=new Blob(chunks.sort((a,b)=>a.offset-b.offset).map(x=>x.data),{type:m.mime});if(await sha256Hex(blob)!==m.ref)throw Error('upload CAS mismatch');this.blobs.set(m.ref,blob);}
   await this.send('transfer',{type:'transfer_result',request_id:m.request_id,ref:m.ref,offset:m.offset,status:'accepted'});return;
  }
  throw Error(`unexpected native message ${m.type}`);
 }
 page(collection,docs=[],extra={}){return this.send('initial',{type:'page',collection,...scopeFor(collection),epoch:'1',package_id:`pkg-${Math.random()}`,seq_from:'0',seq_to:this.head,cursor_to:this.head,head_seq:this.head,complete:true,backlog:0,core_sent_at_ms:Date.now()+this.clockOffset,docs,sig:'test-signature',...extra});}
 close(){if(this.closed)return;this.closed=true;this.wake();for(const [c,n,f]of this.listeners)c.removeEventListener(n,f);for(const c of Object.values(this.channels))c.close?.();}
}
