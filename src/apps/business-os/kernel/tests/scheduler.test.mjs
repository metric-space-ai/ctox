import {test} from 'node:test';
import assert from 'node:assert/strict';
import {SendScheduler} from '../scheduler.mjs';
import {LaneIngress} from '../ingress.mjs';
import {decodeFrame,encodeFrame} from '../framing.mjs';
class Channel extends EventTarget {readyState='open';bufferedAmount=0;frames=[];send(data){this.frames.push(data);this.onSend?.(data);}}
const tick=()=>new Promise(r=>setTimeout(r,0));
test('window sends four fragments without ACK and control preempts remaining bulk',async()=>{
 const c=new Channel(),s=new SendScheduler({0:c},{codec:'identity',windowFrames:4});
 const bulk=s.send('transfer',{text:'x'.repeat(200000)});await tick();assert.equal(c.frames.length,4);
 await s.plain({type:'ack',lane:4,upto:'1'});assert.equal(JSON.parse(c.frames[4]).type,'ack');
 let done=false;bulk.then(()=>done=true,()=>{});assert.equal(done,false);
 const control=s.send('control',{text:'c'.repeat(200000)});control.catch(()=>{});await tick();
 await s.plain({type:'ack',lane:1,upto:'1'});assert.equal(JSON.parse(c.frames.at(-1)).type,'ack','ACK bypasses a blocked control window');
 s.close();await assert.rejects(bulk,e=>e.code==='unavailable');await assert.rejects(control,e=>e.code==='unavailable');
});
test('connection buffer budget covers all channels and wakes on low watermark',async()=>{
 const a=new Channel(),b=new Channel();a.bufferedAmount=60000;
 const s=new SendScheduler({0:a,5:b},{codec:'identity'});const sent=s.send('transfer',{text:'x'.repeat(12000)});await tick();assert.equal(b.frames.length,0);
 a.bufferedAmount=0;a.dispatchEvent(new Event('bufferedamountlow'));await sent;assert.equal(b.frames.length,1);s.close();
});
test('gzip reassembly is Unicode byte-correct; final ACK follows application commit',async()=>{
 const c=new Channel(),s=new SendScheduler({0:c},{codec:'gzip'}),acked=[],seen=[];
 const message={text:'ä🧬'.repeat(20000)};await s.send('initial',message);
 const ingress=new LaneIngress({codec:'gzip',apply:async m=>seen.push(m),ack:async(l,n)=>acked.push([l,n])});
 for(const f of c.frames){assert.ok(f.byteLength<=16384);await ingress.push(4,f);}
 assert.deepEqual(seen,[message]);assert.equal(acked.at(-1)[0],4);s.close();ingress.close();
});
test('blocked initial lane does not serialize command-result ingress',async()=>{
 let unblock;const hold=new Promise(r=>unblock=r),seen=[];
 const ingress=new LaneIngress({codec:'identity',apply:async(m,l)=>{if(l===4)await hold;seen.push(m.type)},ack:async()=>{}});
 const frame=(lane,type)=>{const payload=new TextEncoder().encode(JSON.stringify({type}));return encodeFrame({lane,codec:'identity',packet:lane+1,index:0,count:1,total:payload.length,raw:payload.length,seq:1,payload});};
 const slow=ingress.push(4,frame(4,'page'));await ingress.push(1,frame(1,'write_result'));assert.deepEqual(seen,['write_result']);
 unblock();await slow;assert.deepEqual(seen,['write_result','page']);ingress.close();
});
test('malformed fragments and future ACK are observable failures',async()=>{
 const c=new Channel(),s=new SendScheduler({0:c},{codec:'identity'});assert.throws(()=>s.ack(1,'10'),e=>e.code==='invalid');
 const frame=encodeFrame({lane:1,codec:'identity',packet:1,index:1,count:1,total:0,raw:0,seq:1,payload:new Uint8Array()});await assert.rejects(decodeFrame(frame),e=>e.code==='invalid');s.close();
});
