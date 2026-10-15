import {CtoxError} from './values.mjs';
import {LANES,FRAME_BYTES,HEADER_BYTES,MAX_MESSAGE_BYTES,encodeFrame,transform} from './framing.mjs';
export {LANES} from './framing.mjs';
/** Single scheduler per SCTP association. Promises resolve on SCTP handoff,
 * not on application acknowledgement. Control ACKs never consume ACK credit.
 */
export class SendScheduler {
 constructor(channels,{frameBytes=FRAME_BYTES,maxBuffered=65536,windowFrames=8,maxQueueBytes=32*1024*1024,codec='gzip',adapters={},onError=()=>{}}={}){
  this.channels=channels;this.frameBytes=Math.min(frameBytes,FRAME_BYTES);this.maxBuffered=Math.min(maxBuffered,128*1024);this.windowFrames=windowFrames;this.maxQueueBytes=maxQueueBytes;this.codec=codec;this.adapters=adapters;this.onError=onError;
  if(this.frameBytes<1024 || this.maxBuffered<this.frameBytes || windowFrames<2)throw new CtoxError('invalid','Invalid scheduler limits');
  this.queues=Array.from({length:6},()=>[]);this.sent=Array(6).fill(0);this.acked=Array(6).fill(0);this.credits=[0,0,0,0,0,0];this.weights=[0,0,0,8,3,1];this.cursor=3;this.visiting=null;this.preparing=0;this.packet=0;this.queuedBytes=0;this.closed=false;this.listeners=[];
  for(const channel of new Set(Object.values(channels))){channel.bufferedAmountLowThreshold=Math.min(16384,this.maxBuffered/2);for(const name of ['bufferedamountlow','open']){const fn=()=>this.wake();channel.addEventListener(name,fn);this.listeners.push([channel,name,fn]);}}
 }
 configure({codec,frame_bytes}){this.codec=codec;this.frameBytes=Math.min(frame_bytes,FRAME_BYTES);}
 buffered(){return [...new Set(Object.values(this.channels))].reduce((n,c)=>n+(c.bufferedAmount??0),0);}
 wake(){if(this.closed||this.scheduled)return;this.scheduled=true;queueMicrotask(()=>{this.scheduled=false;this.pump();});}
 plain(message){return this.enqueue(0,[{plain:JSON.stringify(message)}],true);}
 async send(lane,message){
  lane=typeof lane==='string'?LANES[lane]:lane;if(!Number.isInteger(lane)||lane<0||lane>5)throw new CtoxError('invalid','Invalid reliable lane');
  if(this.closed)throw new CtoxError('unavailable','Scheduler closed');
  const raw=new TextEncoder().encode(JSON.stringify(message));if(raw.length>MAX_MESSAGE_BYTES)throw new CtoxError('quota','Message exceeds limit');
  if(this.preparing+this.queuedBytes+raw.length>this.maxQueueBytes)throw new CtoxError('quota','Send preparation budget exceeded');
  this.preparing+=raw.length;let encoded;try{encoded=await transform(raw,this.codec,false,this.adapters);}finally{this.preparing-=raw.length;}
  const packet=++this.packet;if(packet>0xffffffff)throw new CtoxError('unavailable','Packet sequence exhausted; reconnect');const step=this.frameBytes-HEADER_BYTES,count=Math.max(1,Math.ceil(encoded.length/step));
  const frames=Array.from({length:count},(_,index)=>({lane,codec:this.codec,packet,index,count,total:encoded.length,raw:raw.length,payload:encoded.slice(index*step,(index+1)*step)}));
  return this.enqueue(lane,frames,false);
 }
 enqueue(lane,frames,noAck){
  const bytes=frames.reduce((n,f)=>n+(f.plain?new TextEncoder().encode(f.plain).length:HEADER_BYTES+f.payload.length),0);
  if(this.closed)return Promise.reject(new CtoxError('unavailable','Scheduler closed'));
  if(this.queuedBytes+bytes>this.maxQueueBytes)return Promise.reject(new CtoxError('quota','Send queue full'));
  if(frames.some(f=>f.plain && new TextEncoder().encode(f.plain).length>this.frameBytes))return Promise.reject(new CtoxError('invalid','Control frame too large'));
  return new Promise((resolve,reject)=>{this.queuedBytes+=bytes;const entry={frames,at:0,noAck,resolve,reject};if(noAck)this.queues[lane].unshift(entry);else this.queues[lane].push(entry);this.wake();});
 }
 ack(lane,upto){
  const n=Number(upto);if(!Number.isSafeInteger(n)||n<0||n>this.sent[lane])throw new CtoxError('invalid','ACK exceeds sent prefix');
  this.acked[lane]=Math.max(this.acked[lane],n);this.wake();
 }
 eligible(lane){const q=this.queues[lane][0],c=this.channels[lane]??this.channels[0];return q && c?.readyState==='open' && (q.noAck || this.sent[lane]-this.acked[lane]<this.windowFrames);}
 next(){
  for(const lane of [0,1,2])if(this.eligible(lane))return lane;
  for(let tries=0;tries<9;tries++){
   const lane=this.cursor;
   if(!this.eligible(lane)){this.credits[lane]=0;this.cursor=lane===5?3:lane+1;this.visiting=null;continue;}
   if(this.visiting!==lane){this.credits[lane]+=this.weights[lane]*this.frameBytes;this.visiting=lane;}
   const frame=this.queues[lane][0].frames[this.queues[lane][0].at],cost=HEADER_BYTES+frame.payload.length;
   if(cost<=this.credits[lane]){this.credits[lane]-=cost;return lane;}
   this.cursor=lane===5?3:lane+1;this.visiting=null;
  }
  return null;
 }
 pump(){
  if(this.closed)return;
  try {
   for(;;){const lane=this.next();if(lane===null)return;const q=this.queues[lane][0],frame=q.frames[q.at],channel=this.channels[lane]??this.channels[0];
    const size=frame.plain?new TextEncoder().encode(frame.plain).length:HEADER_BYTES+frame.payload.length;
    if(this.buffered()+size>this.maxBuffered)return;
    if(!q.noAck && this.sent[lane]===0xffffffff)throw new CtoxError('unavailable','Frame sequence exhausted; reconnect');
    const data=frame.plain??encodeFrame({...frame,seq:++this.sent[lane]});channel.send(data);this.queuedBytes-=size;q.at++;
    if(q.at===q.frames.length){this.queues[lane].shift();q.resolve();}
   }
  }catch(error){this.onError(error);this.close(error);}
 }
 close(error=new CtoxError('unavailable','Connection closed')){if(this.closed)return;this.closed=true;for(const [c,n,f]of this.listeners)c.removeEventListener(n,f);for(const q of this.queues)for(const item of q.splice(0))item.reject(error);this.queuedBytes=0;}
}
