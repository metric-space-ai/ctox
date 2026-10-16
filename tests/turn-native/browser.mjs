import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import http from 'node:http';
import {createRequire} from 'node:module';
const root=process.argv[2], config=JSON.parse(fs.readFileSync(join(root,'config.json')));
const require=createRequire(join(config.modulesRoot,'playwright/package.json'));
const {chromium}=require('playwright');
let browser,launchPromise,server;
process.once('SIGTERM',()=>{void (launchPromise||Promise.resolve()).then(b=>b?.close()).catch(()=>{}).finally(()=>process.exit(143));});
const wait=async path=>{const deadline=Date.now()+30000;while(Date.now()<deadline){if(fs.existsSync(path))return JSON.parse(fs.readFileSync(path));await new Promise(r=>setTimeout(r,20));}throw Error('fixture file deadline');};
const write=(name,data)=>{fs.writeFileSync(join(root,name+'.tmp'),JSON.stringify(data));fs.renameSync(join(root,name+'.tmp'),join(root,name));};
try {
 // This fixture's unprivileged namespace has only lo. Chromium ignores it
 // unless explicitly admitted; this changes no SCTP extension negotiation.
 // ref: chromium/content/public/common/content_switches.cc (kAllowLoopbackInPeerConnection)
 launchPromise=chromium.launch({executablePath:config.chrome,headless:true,args:['--disable-gpu','--no-sandbox',...(config.fixtureLoopback?['--allow-loopback-in-peer-connection']:[])],timeout:15000});
 browser=await launchPromise;
 const page=await browser.newPage();
 if(config.m4){
  const source=resolve(fileURLToPath(new URL('../..',import.meta.url)),'src/apps/business-os');
  server=http.createServer((req,res)=>{
   const name=new URL(req.url,'http://fixture').pathname;
   if(name==='/'){res.end('<!doctype html><title>M4 transport fixture</title>');return;}
   const file=resolve(source,'.'+name);
   if(!file.startsWith(source+'/')||!file.endsWith('.mjs')){res.writeHead(404).end();return;}
   try{res.setHeader('Content-Type','text/javascript');res.end(fs.readFileSync(file));}catch{res.writeHead(404).end();}
  });
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  await page.goto(`http://127.0.0.1:${server.address().port}`);
 }

 await page.evaluate(async iceServer=>{
  globalThis.pc=new RTCPeerConnection({iceTransportPolicy:'relay',iceServers:[iceServer]});
  globalThis.received=[];globalThis.dc=pc.createDataChannel('turn-native');
  globalThis.m4channels={};globalThis.m4wireErrors=[];globalThis.m4maxReceived=0;globalThis.m4maxSent=0;globalThis.m4bufferPeak=0;
  pc.addEventListener('datachannel',({channel})=>{
   const labels=['control','commands','interactive','live','initial-sync','transfer'];
   const lane=labels.findIndex(x=>channel.label==='ctox-v4-'+x);
   if(lane>=0){m4channels[lane]=channel;channel.binaryType='arraybuffer';
    channel.addEventListener('message',event=>{m4maxReceived=Math.max(m4maxReceived,event.data.byteLength);globalThis.m4wire?.push(lane,new Uint8Array(event.data)).catch(e=>m4wireErrors.push(e.message));});
    const send=channel.send.bind(channel);channel.send=data=>{const n=typeof data==='string'?new TextEncoder().encode(data).length:data.byteLength;m4maxSent=Math.max(m4maxSent,n);m4bufferPeak=Math.max(m4bufferPeak,Object.values(m4channels).reduce((n,c)=>n+c.bufferedAmount,0)+n);return send(data);};}
  });
  dc.onmessage=e=>received.push(e.data);
  await pc.setLocalDescription(await pc.createOffer());
  await new Promise((resolve,reject)=>{const t=setTimeout(()=>reject(Error('browser gathering')),10000);const done=()=>{if(pc.iceGatheringState==='complete'){clearTimeout(t);resolve();}};pc.addEventListener('icegatheringstatechange',done);done();});
 },{urls:config.browserUrl,...config.initial});
 write('offer.json',await page.evaluate(()=>pc.localDescription.toJSON()));
 await page.evaluate(answer=>pc.setRemoteDescription(answer),await wait(join(root,'answer.json')));
 await page.waitForFunction(()=>dc.readyState==='open',null,{timeout:15000});
 if(config.m4){await runM4(page);}
 else {
 await page.evaluate(()=>{for(let i=0;i<64;i++)dc.send(`turn-message-${i}`);});
 await page.waitForFunction(()=>received.length===64,null,{timeout:15000});
 // Native refreshes its signaling credentials after five seconds, while this
 // exact data channel/PeerConnection remains live. No reconnect is permitted.
 await new Promise(r=>setTimeout(r,12500));
 await page.evaluate(()=>{if(dc.readyState!=='open')throw Error('credential renewal dropped live channel');for(let i=64;i<128;i++)dc.send(`turn-message-${i}`);});
 await page.waitForFunction(()=>received.length===128,null,{timeout:15000});
 const result=await page.evaluate(async()=>{
  const stats=await pc.getStats();const transport=[...stats.values()].find(s=>s.type==='transport'&&s.selectedCandidatePairId);
  const pair=stats.get(transport?.selectedCandidatePairId),local=stats.get(pair?.localCandidateId),remote=stats.get(pair?.remoteCandidateId);
  return {pass:received.every((m,i)=>m===`turn-message-${i}`)&&local?.candidateType==='relay'&&remote?.candidateType==='relay',
    selected:{local:local?.candidateType,remote:remote?.candidateType,relayProtocol:local?.relayProtocol},delivered:received.length,lost:128-received.length};
 });
 result.browserVersion=browser.version(); result.nodeVersion=process.version;
 write('browser-result.json',result);
 if(!result.pass)throw Error('selected relay or reliable delivery failed');
 }
} catch(error) {
 try {
  const pages=browser?.contexts().flatMap(context=>context.pages())||[];
  if(pages[0]) {
   const ice=await pages[0].evaluate(async()=>{
    if(!globalThis.pc)return {};
    const stats=[...(await pc.getStats()).values()].filter(s=>s.type==='local-candidate'||s.type==='candidate-pair');
    return {gathering:pc.iceGatheringState,connection:pc.connectionState,candidateCount:(pc.localDescription?.sdp.match(/^a=candidate:/gm)||[]).length,stats};
   });
   write('browser-ice.json',ice);
  }
 } catch {}
 write('browser-result.json',{pass:false,error:error.message});process.exitCode=1;
}
finally {await browser?.close();if(server)await new Promise(r=>server.close(r));}

async function runM4(page){
 await page.waitForFunction(()=>Object.keys(m4channels).length===6&&Object.values(m4channels).every(c=>c.readyState==='open'),null,{timeout:15000});
 await page.evaluate(async payload=>{
  const {NativeWire,ReliableClientSession}=await import('/kernel/transport-webrtc.mjs');
  const state=new ReliableClientSession('browser-m4');state.attach({session_id:'browser-m4',resume_from:Array(6).fill('0')});
  globalThis.m4received=0;globalThis.m4bytes=0;globalThis.m4pending=new Map();
  globalThis.m4start=performance.now();
  globalThis.m4wire=new NativeWire(m4channels,{state,onError:e=>m4wireErrors.push(e.message),apply:async(message,lane)=>{
   if(lane===5){if(message.type!=='bulk'||message.ordinal!==m4received||message.payload!==payload)throw Error('M4 bulk corruption/gap/duplicate');m4received++;m4bytes+=new TextEncoder().encode(JSON.stringify(message)).length;dc.send('m4-bulk-delivered');}
   else {const key=lane+':'+message.ordinal,resolve=m4pending.get(key);if(!resolve)throw Error('M4 unexpected control reply');m4pending.delete(key);resolve();}
  }});
 },config.payload);
 write('m4-browser-ready.json',{ready:true});
 const metrics=await page.evaluate(async()=>{
  const samples={control:[],commands:[]};
  for(let id=0;id<64;id++)for(const [lane,name]of [[1,'commands'],[0,'control']]){
   const start=performance.now();let timer;
   const response=new Promise((resolve,reject)=>{m4pending.set(lane+':'+id,resolve);timer=setTimeout(()=>reject(Error('M4 '+name+' deadline')),10000);});
   try{await m4wire.send(lane,{type:'probe',ordinal:id});await response;samples[name].push(performance.now()-start);}finally{clearTimeout(timer);}
  }
  dc.send('m4-stop');
  return Object.fromEntries(Object.entries(samples).map(([lane,values])=>{values.sort((a,b)=>a-b);return [lane,{p95_ms:values[Math.floor(values.length*.95)],p99_ms:values[Math.floor(values.length*.99)],roundtrips_ms:values}];}));
 });
 const native=await wait(join(root,'m4-native-result.json'));
 await page.waitForFunction(()=>m4wire.state.acked.every((seq,lane)=>seq===m4wire.state.sent[lane]),null,{timeout:10000});
 const measured=await page.evaluate(async()=>{
  const stats=await pc.getStats(),t=[...stats.values()].find(x=>x.type==='transport'&&x.selectedCandidatePairId),pair=stats.get(t?.selectedCandidatePairId),local=stats.get(pair?.localCandidateId),remote=stats.get(pair?.remoteCandidateId);
  return {max_received_frame:m4maxReceived,max_sent_frame:m4maxSent,browser_buffer_peak:m4bufferPeak,delivery_acknowledged:m4wire.state.acked.map(String),sent:m4wire.state.sent.map(String),bulk_received:m4received,bulk_bytes:m4bytes,bulk_seconds:(performance.now()-m4start)/1000,errors:m4wireErrors,local:local?.candidateType,remote:remote?.candidateType};
 });
 const result={...metrics,...measured,native,nominal_rtt_ms:config.rtt,loss_percent:1,browserVersion:browser.version(),nodeVersion:process.version,
  pass:measured.errors.length===0&&measured.max_received_frame<=16384&&measured.max_sent_frame<=16384&&measured.browser_buffer_peak<=131072&&native.buffer_peak<=131072&&measured.bulk_received===native.bulk_sent&&measured.bulk_bytes===native.bulk_bytes&&measured.local==='relay'&&measured.remote==='relay',
  bulk_mb_s:measured.bulk_bytes/measured.bulk_seconds/1e6,scope:'Chromium NativeWire/E14 to native scheduler; fixture authority, not enrollment'};
 const target={20:120,150:250,600:700}[config.rtt];
 result.performance_pass=result.pass&&metrics.control.p99_ms<=target&&metrics.commands.p99_ms<=target&&(config.rtt!==20||result.bulk_mb_s>=5);
 write('browser-result.json',result);if(!result.pass)throw Error('M4 Chromium functional invariant failed');
}
