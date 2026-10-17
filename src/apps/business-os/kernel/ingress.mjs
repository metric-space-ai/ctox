import {CtoxError} from './values.mjs';
import {decodeFrame,transform,FRAME_BYTES,MAX_MESSAGE_BYTES} from './framing.mjs';
/** Each ordered lane has its own chain. A slow initial page never holds control
 * or command results. Frame ACK follows bounded reassembly; the final ACK
 * follows application/IDB commit. No frame is silently discarded.
 */
export class LaneIngress {
 constructor({apply,ack,onError=()=>{},frameBytes=FRAME_BYTES,codec='gzip',adapters={},maxReassemblyBytes=32*1024*1024,maxQueuedBytes=512*1024}={}){
  this.apply=apply;this.ack=ack;this.onError=onError;this.frameBytes=frameBytes;this.codec=codec;this.adapters=adapters;this.maxReassemblyBytes=maxReassemblyBytes;this.maxQueuedBytes=maxQueuedBytes;
  this.chains=Array(6).fill(Promise.resolve());this.seqs=Array(6).fill(0);this.packets=new Map();this.bytes=0;this.queued=0;this.closed=false;
 }
 push(lane,input){
  if(this.closed)return Promise.reject(new CtoxError('unavailable','Ingress closed'));
  const size=input.size??input.byteLength??0;
  if(this.queued+size>this.maxQueuedBytes){const e=new CtoxError('quota','Ingress credit exceeded');this.onError(e);return Promise.reject(e);}
  this.queued+=size;
  const next=this.chains[lane].then(()=>this.consume(lane,input)).finally(()=>{this.queued-=size;});
  this.chains[lane]=next.catch(error=>this.onError(error));return next;
 }
 async consume(lane,input){
  if(this.closed)throw new CtoxError('unavailable','Ingress closed');
  const f=await decodeFrame(input,this.frameBytes);
  if(f.lane!==lane||f.codec!==this.codec)throw new CtoxError('invalid','Lane/codec mismatch');
  if(f.seq<=this.seqs[lane]){await this.ack(lane,this.seqs[lane]);return;}
  if(f.seq!==this.seqs[lane]+1)throw new CtoxError('invalid','Noncontiguous frame sequence');
  const key=`${lane}:${f.packet}`;let p=this.packets.get(key);
  if(!p){if(f.index!==0||this.bytes+f.total>this.maxReassemblyBytes)throw new CtoxError('quota','Reassembly credit exceeded');p={...f,parts:[],received:0};this.packets.set(key,p);this.bytes+=f.total;}
  if(f.index!==p.parts.length||f.count!==p.count||f.total!==p.total||f.raw!==p.raw)throw new CtoxError('invalid','Fragment sequence/header mismatch');
  p.parts.push(f.payload);p.received+=f.payload.length;
  if(p.received>p.total)throw new CtoxError('invalid','Fragment exceeds declared length');
  if(p.parts.length===p.count){
   if(p.received!==p.total)throw new CtoxError('invalid','Truncated package');
   const encoded=new Uint8Array(p.total);let at=0;for(const part of p.parts){encoded.set(part,at);at+=part.length;}
   const raw=await transform(encoded,f.codec,true,this.adapters);if(raw.length!==p.raw||raw.length>MAX_MESSAGE_BYTES)throw new CtoxError('invalid','Decoded length mismatch');
   const message=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(raw));await this.apply(message,lane);
   this.packets.delete(key);this.bytes-=p.total;
  }
  this.seqs[lane]=f.seq;await this.ack(lane,f.seq);
 }
 close(){this.closed=true;this.packets.clear();this.bytes=0;}
}
