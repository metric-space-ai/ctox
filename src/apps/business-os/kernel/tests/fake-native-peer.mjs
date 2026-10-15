import {SendScheduler} from '../scheduler.mjs';
import {LaneIngress} from '../ingress.mjs';
import {decodeHelloFrame,negotiateHello} from '../../rxdb/src/hello.mjs';
import {sha256Hex} from '../values.mjs';
export const offer=instance_id=>({type:'hello',protocol_ver:4,bootstrap_ver:1,app_api_ver:{min:'4.0',max:'4.0'},instance_id,capabilities:[],device_proof:{device_id:'test-device',public_key:'A'.repeat(43),nonce:'B'.repeat(43),signature:'C'.repeat(86)},scopes_wanted:[],transport:{codecs:['gzip'],max_frame_bytes:16384,fragmentation:true,reassembly:true}});
export const scopeFor=collection=>({scope_hash:`scope:${collection}`,gen:Number(/@g(\d+)$/.exec(collection)?.[1]??1)});
class MemoryChannel extends EventTarget {
 readyState='open';bufferedAmount=0;
 send(data){const value=typeof data==='string'?data:data.slice();queueMicrotask(()=>{if(this.other.readyState==='open')this.other.dispatchEvent(new MessageEvent('message',{data:value}));});}
 close(){if(this.readyState==='closed')return;this.readyState='closed';this.dispatchEvent(new Event('close'));}
}
export function protocolChannels(){const client={},server={};for(let i=0;i<6;i++){client[i]=new MemoryChannel();server[i]=new MemoryChannel();client[i].other=server[i];server[i].other=client[i];}return {client,server};}
/** Test-only native protocol stand-in. It handles the documented logical V4
 * messages; it is never selected by the product transport.
 */
export class FakeNativePeer {
 constructor(channels,{instanceId='instance',head='9007199254740999',clockOffset=5000,onSubscribe}={}){
  this.channels=channels;this.instanceId=instanceId;this.head=head;this.clockOffset=clockOffset;this.onSubscribe=onSubscribe;this.messages=[];this.errors=[];this.blobs=new Map();this.uploads=new Map();this.listeners=[];
  this.scheduler=new SendScheduler(channels,{codec:'gzip',onError:e=>this.errors.push(e)});
  this.ingress=new LaneIngress({codec:'gzip',apply:(m,l)=>this.apply(m,l),ack:(lane,upto)=>this.scheduler.plain({type:'ack',lane,upto:String(upto)}),onError:e=>this.errors.push(e)});
  for(const channel of Object.values(channels)){channel.binaryType='arraybuffer';const fn=e=>{this.receive(e.data).catch(err=>this.errors.push(err));};channel.addEventListener('message',fn);this.listeners.push([channel,fn]);}
 }
 async receive(data){
  if(typeof data==='string'){
   const m=JSON.parse(data);if(m.type==='ack'){this.scheduler.ack(m.lane,m.upto);return;}
   const hello=decodeHelloFrame(data);this.messages.push(hello);
   const selected=negotiateHello({instance_id:this.instanceId,bootstrap_min:1,bootstrap_max:1,app_api_ver:{min:'4.0',max:'4.0'},capabilities:[],required_capabilities:[],transport:hello.transport},hello);
   this.scheduler.configure(selected.transport);
   await this.scheduler.plain({type:'hello_ok',protocol_ver:4,instance_id:this.instanceId,...selected,epoch:'1',head_seq:this.head,scope_tokens:['test-scope-token'],rights_ver:'1',core_time_ms:Date.now()+this.clockOffset});return;
  }
  const bytes=data instanceof ArrayBuffer?new Uint8Array(data):data;await this.ingress.push(bytes[4],bytes);
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
 send(lane,message){return this.scheduler.send(lane,message);}
 close(){for(const [c,f]of this.listeners)c.removeEventListener('message',f);this.scheduler.close();this.ingress.close();for(const c of Object.values(this.channels))c.close?.();}
}
