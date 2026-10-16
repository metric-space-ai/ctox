import {createKernel} from '/kernel/kernel.mjs';
import {WebRTCKernelTransport} from '/kernel/transport-webrtc.mjs';
const utf8=new TextEncoder();
const canonical=x=>JSON.stringify(order(x));
function order(x){if(Array.isArray(x))return x.map(order);if(x&&typeof x==='object')return Object.fromEntries(Object.keys(x).sort().map(k=>[k,order(x[k])]));return x;}
const b64=bytes=>btoa(String.fromCharCode(...new Uint8Array(bytes))).replaceAll('+','-').replaceAll('/','_').replaceAll('=','');
const unb64=text=>Uint8Array.from(atob(text.replaceAll('-','+').replaceAll('_','/')),x=>x.charCodeAt(0));
function deadline(promise,label,ms=12000){let timer;return Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error(label)),ms);})]).finally(()=>clearTimeout(timer));}
function observed(live,predicate){return deadline(new Promise(resolve=>{let off;const check=value=>{if(predicate(value)){off?.();resolve(value);}};off=live.subscribe(check);check(live.get());}), 'local observation deadline');}
export async function start(config){
 const started=0,errors=[],wire={maxFrame:0,maxBuffered:0,sent:0,received:0};let generation=0,connection,transport,kernel;
 const key=await crypto.subtle.importKey('pkcs8',Uint8Array.from([0x30,0x2e,0x02,0x01,0x00,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x04,0x22,0x04,0x20,...new Uint8Array(32).fill(11)]),'Ed25519',true,['sign']);
 const serverSeed=await crypto.subtle.importKey('pkcs8',Uint8Array.from([0x30,0x2e,0x02,0x01,0x00,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x04,0x22,0x04,0x20,...new Uint8Array(32).fill(42)]),'Ed25519',true,['sign']);
 const publicJwk=await crypto.subtle.exportKey('jwk',key),serverJwk=await crypto.subtle.exportKey('jwk',serverSeed),serverKey=await crypto.subtle.importKey('raw',unb64(serverJwk.x),'Ed25519',false,['verify']);
 const verify=(signature,bytes)=>crypto.subtle.verify('Ed25519',serverKey,signature,bytes);
 const createPeer=async()=>{
  const peer=new RTCPeerConnection({iceServers:[{urls:[config.turnUrl],username:'bench',credential:'bench'}],iceTransportPolicy:'relay'}),bootstrap=peer.createDataChannel('ctox-rxdb',{ordered:true});
  const socket=new WebSocket(`${config.signalingUrl}?peerId=browser0-g${generation++}&role=browser`);const channels={0:bootstrap};let started=false,chain=Promise.resolve(),candidates=[];
  let resolveChallenge,rejectChallenge;const challenge=deadline(new Promise((r,j)=>{resolveChallenge=r;rejectChallenge=j;}),'native challenge deadline');challenge.catch(()=>{});
  const signal=data=>socket.send(JSON.stringify({type:'signal',room:config.room,receiverPeerId:'native000001',data}));
  const attach=channel=>{channel.addEventListener('message',event=>{wire.received+=typeof event.data==='string'?utf8.encode(event.data).length:event.data.byteLength;});const send=channel.send.bind(channel);channel.send=data=>{const n=typeof data==='string'?utf8.encode(data).length:data.byteLength;wire.sent+=n;wire.maxFrame=Math.max(wire.maxFrame,n);wire.maxBuffered=Math.max(wire.maxBuffered,Object.values(channels).reduce((n,c)=>n+c.bufferedAmount,0)+n);return send(data);};};attach(bootstrap);
  peer.addEventListener('datachannel',event=>{(wire.channels??=[]).push({label:event.channel.label,state:event.channel.readyState});attach(event.channel);});
  const hello=async event=>{try{const value=JSON.parse(event.data);if(value.type!=='hello')throw Error('native hello required');if(value.instance_id!=='e2e-fixture'||value.device_proof.device_id!=='native-fixture'||value.device_proof.public_key!==serverJwk.x||!await verify(unb64(value.device_proof.signature),utf8.encode(value.device_proof.nonce)))throw Error('native proof invalid');bootstrap.removeEventListener('message',hello);resolveChallenge(value);}catch(error){rejectChallenge(error);}};
  bootstrap.addEventListener('message',hello);
  peer.addEventListener('icecandidate',event=>{if(event.candidate)signal({type:'candidate',candidate:event.candidate.toJSON()});});
  socket.addEventListener('open',()=>socket.send(JSON.stringify({type:'join',room:config.room})));
  socket.addEventListener('message',event=>{chain=chain.then(async()=>{const message=JSON.parse(event.data);
   if(message.type==='joined'&&message.otherPeerIds.includes('native000001')&&!started){started=true;await peer.setLocalDescription(await peer.createOffer());signal({type:'offer',sdp:peer.localDescription.sdp});}
   if(message.type==='signal'){const data=message.data??message.signal;if(data.type==='answer'||data.type==='offer'){await peer.setRemoteDescription({type:data.type,sdp:data.sdp});for(const c of candidates)await peer.addIceCandidate(c);candidates=[];if(data.type==='offer'){await peer.setLocalDescription(await peer.createAnswer());signal({type:'answer',sdp:peer.localDescription.sdp});}}else if(data.type==='candidate'){if(peer.remoteDescription)await peer.addIceCandidate(data.candidate);else candidates.push(data.candidate);}}
  }).catch(error=>{errors.push(error.message);rejectChallenge(error);});});
  connection={peer,socket,channels};
  return {peer,channels,challenge,relay:true,close(){socket.close();peer.close();}};
 };
 transport=new WebRTCKernelTransport({challengeFirst:true,createPeer,participant:'alice',hello:async(_,challenge)=>({type:'hello',protocol_ver:4,bootstrap_ver:1,app_api_ver:{min:'4.0',max:'4.0'},instance_id:'e2e-fixture',capabilities:['bench'],device_proof:{device_id:'browser0',public_key:publicJwk.x,nonce:challenge.device_proof.nonce,signature:b64(await crypto.subtle.sign('Ed25519',key,utf8.encode(challenge.device_proof.nonce)))},scopes_wanted:['notes','business_commands'],transport:{codecs:['gzip'],max_frame_bytes:16384,fragmentation:true,reassembly:true}}),
  resolveScope:async(collection,session)=>{
   const token=JSON.parse(session.scope_tokens[0]);if(token.claims.instance_id!=='e2e-fixture'||token.claims.device_id!=='browser0'||token.claims.exp*1000<Date.now()||!await verify(unb64(token.signature),utf8.encode('ctox:v4:scope\0'+canonical(token.claims))))throw Error('scope signature invalid');
   return {scope_hash:token.claims.scope_hash,gen:1};
  },verifyPackage:async packet=>{const {sig,...body}=packet;return packet.instance_id==='e2e-fixture'&&packet.participant==='alice'&&await verify(new Uint8Array(sig),utf8.encode('ctox.sync.v4.stream\0'+canonical(body)));}});
 const applied=[],failures=[],apply=transport.apply.bind(transport),fail=transport.fail.bind(transport);
 transport.apply=async(message,lane)=>{applied.push({type:message.type,lane,collection:message.collection,seq_to:message.seq_to,docs:message.docs?.map(d=>({id:d.id,seq:d.seq}))});return apply(message,lane);};
 transport.fail=error=>{failures.push(error.message);return fail(error);};
 globalThis.e2eDiagnostic=()=>({channels:Object.entries(transport.channels??{}).map(([lane,c])=>({lane,label:c.label,state:c.readyState})),received:wire.channels??[],link:connection?.peer.connectionState,errors,failures,applied,sent:transport.reliable.sent.map(String),acked:transport.reliable.acked.map(String),delivered:transport.reliable.delivered.map(String),wire});
 kernel=createKernel({instanceId:'e2e-fixture',userId:'alice',transport});await kernel.ready;
 const localReadyMs=performance.now()-started,api=kernel.appApi('notes','1'),seed=api.get('notes','seed');
 await transport.ready;await observed(seed,doc=>doc?.title==='actual SQLite seed');
 document.querySelector('pre').textContent='V4 signed native page in local IDB: '+JSON.stringify(seed.get());await new Promise(requestAnimationFrame);
 const firstVisibleMs=performance.now()-started;
 globalThis.e2e={kernel,transport,api,seed,errors,wire,localReadyMs,firstVisibleMs,
  async write(){const handle=api.put('notes',{id:'resend',title:'once despite dropped result',payload:Array.from({length:90000},(_,i)=>String.fromCharCode(33+((Math.imul(i+1,2654435761)>>>16)%90))).join('')});this.handle=handle;await handle.local;return handle.op_id;},
  async reconnect(){await transport.reconnect();return true;},
  async accepted(){return deadline(this.handle.result,'write result deadline');},
  async pushed(){await observed(seed,doc=>doc?.title==='live from writer');document.querySelector('pre').textContent='V4 native writer live push in local IDB: '+JSON.stringify(seed.get());await new Promise(requestAnimationFrame);return kernel.shell.metrics.get();},
  async activation(){await transport.appActivation({app_id:'notes',app_ver:'1',state:'activated',at_seq:String(kernel.sim.checkpoint)});},
  async command(){const before=performance.now(),handle=api.command('bench.noop',{});await handle.local;const result=await deadline(handle.result,'command acceptance deadline');return {roundtripMs:performance.now()-before,status:result.status,semantics:'Core write acceptance; P9 handler completion not measured'};},
  async queueCommands(offset,count){this.batch=[];for(let i=0;i<count;i++){const started=performance.now(),handle=api.command('bench.noop',{ordinal:offset+i});this.batch.push({handle,started});await handle.local;}return this.batch.map(x=>x.handle.id);},
  async batchAccepted(){return deadline(Promise.all(this.batch.map(async({handle,started})=>({id:handle.id,...await handle.result,roundtripMs:performance.now()-started}))),'M3 command result deadline',90000);},
  async control(){
   const state=transport.reliable,ack=state.ack,latencies=[];let pending;
   state.ack=function(lane,upto){ack.call(this,lane,upto);if(lane===0&&pending&&this.acked[0]>=pending.seq)pending.resolve();};
   try{for(let i=0;i<100;i++){
    const before=performance.now(),seq=state.sent[0]+1n;let timer;
    const confirmed=new Promise((resolve,reject)=>{pending={seq,resolve};timer=setTimeout(()=>reject(Error('control ACK deadline')),5000);});
    try{await Promise.all([transport.scheduler.send('control',{type:'priority',collection:'notes',priority:'P1'}),confirmed]);latencies.push(performance.now()-before);}
    finally{clearTimeout(timer);pending=null;}
   }}finally{state.ack=ack;}
   latencies.sort((a,b)=>a-b);return {samples:latencies.length,p50_ms:latencies[50],p95_ms:latencies[95],p99_ms:latencies[99],semantics:'Spur 0 priority send to native E14 ACK after application ingress; no App API acknowledgement'};
  },
  async stats(){const stats=await connection.peer.getStats();let relay=false;for(const row of stats.values())if(row.type==='candidate-pair'&&row.state==='succeeded'&&row.nominated){const local=stats.get(row.localCandidateId);relay=local?.candidateType==='relay';}if(!relay)throw Error('TURN relay not selected');return {relay,firstVisibleMs,localReadyMs,metrics:kernel.shell.metrics.get(),wire,errors};},
  async close(){await kernel.close();}
 };
 return {firstVisibleMs,localReadyMs,origin:'navigationStart',visible:'local seed rendered then rAF; screenshot follows'};
}
