import {CtoxError} from './values.mjs';
export const LANES=Object.freeze({control:0,commands:1,interactive:2,live:3,initial:4,transfer:5,realtime:6});
export const HEADER_BYTES=32,FRAME_BYTES=16384,MAX_MESSAGE_BYTES=16*1024*1024;
const MAGIC=0x43545834;
export const codecId=codec=>codec==='gzip'?1:codec==='zstd'?2:0;
export async function transform(bytes,codec,decode=false,adapters={}){
 if(codec==='identity')return bytes;
 if(adapters[codec])return adapters[codec][decode?'decode':'encode'](bytes);
 if(codec!=='gzip')throw new CtoxError('unavailable',`Codec ${codec} unavailable`);
 const stream=new Blob([bytes]).stream().pipeThrough(decode?new DecompressionStream('gzip'):new CompressionStream('gzip'));
 const reader=stream.getReader(),chunks=[];let size=0;
 for(;;){const {done,value}=await reader.read();if(done)break;size+=value.byteLength;if(size>MAX_MESSAGE_BYTES){await reader.cancel();throw new CtoxError('quota','Decoded message exceeds limit');}chunks.push(value);}
 const result=new Uint8Array(size);let at=0;for(const chunk of chunks){result.set(chunk,at);at+=chunk.length;}return result;
}
export function encodeFrame({lane,codec,packet,index,count,total,raw,seq,payload}){
 const out=new Uint8Array(HEADER_BYTES+payload.length),d=new DataView(out.buffer);
 d.setUint32(0,MAGIC);d.setUint8(4,lane);d.setUint8(5,codecId(codec));d.setUint16(6,0);
 for(const [at,value]of [[8,packet],[12,index],[16,count],[20,total],[24,raw],[28,seq]])d.setUint32(at,value);
 out.set(payload,HEADER_BYTES);return out;
}
export async function decodeFrame(input,frameBytes=FRAME_BYTES){
 const bytes=input instanceof Blob?new Uint8Array(await input.arrayBuffer()):input instanceof ArrayBuffer?new Uint8Array(input):new Uint8Array(input.buffer,input.byteOffset,input.byteLength);
 if(bytes.length<HEADER_BYTES || bytes.length>frameBytes)throw new CtoxError('invalid','Invalid frame size');
 const d=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
 if(d.getUint32(0)!==MAGIC || d.getUint16(6)!==0)throw new CtoxError('invalid','Invalid V4 frame header');
 const f={lane:d.getUint8(4),codec:['identity','gzip','zstd'][d.getUint8(5)],packet:d.getUint32(8),index:d.getUint32(12),count:d.getUint32(16),total:d.getUint32(20),raw:d.getUint32(24),seq:d.getUint32(28),payload:bytes.slice(HEADER_BYTES)};
 if(f.lane>5 || !f.codec || !f.count || f.index>=f.count || !f.seq || f.total>MAX_MESSAGE_BYTES || f.raw>MAX_MESSAGE_BYTES || f.count>Math.ceil(MAX_MESSAGE_BYTES/(frameBytes-HEADER_BYTES))+1)throw new CtoxError('invalid','Invalid V4 fragment');
 return f;
}
