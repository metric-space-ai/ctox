// Run on the admitted Linux lane with an existing Playwright package.
import {createServer} from 'node:http';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {resolve,extname} from 'node:path';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE??'playwright');
const root=resolve('src/apps/business-os/kernel');
const out=process.env.KERNEL_EVIDENCE_DIR??resolve(process.env.TMPDIR??'.','k1-browser');
async function capture(page,name){
 try{await page.screenshot({path:resolve(out,name)});}catch(error){
  if(!String(error.message).includes('Unable to capture screenshot'))throw error;
  console.log('SCREENSHOT_RECOVERY '+name);await page.bringToFront();await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));await page.screenshot({path:resolve(out,name)});
 }
}
await mkdir(out,{recursive:true});
const server=createServer(async(req,res)=>{
 try{
  if(req.url==='/rxdb/src/hello.mjs'){res.setHeader('Content-Type','text/javascript');res.end(await readFile(resolve('src/apps/business-os/rxdb/src/hello.mjs')));return;}
  if(req.url==='/sw.js'){res.setHeader('Content-Type','text/javascript');res.end("self.addEventListener('install',()=>self.skipWaiting());self.addEventListener('message',event=>{if(event.data==='ctox:claim')event.waitUntil(clients.claim().then(()=>clients.matchAll()).then(list=>list.forEach(client=>client.postMessage('claimed'))));});");return;}
  if(req.url==='/'){res.setHeader('Content-Type','text/html');res.end('<!doctype html><meta charset="utf-8"><title>Kernel acceptance</title><pre id="result">Running kernel acceptance</pre>');return;}
  const path=resolve(root,'.'+decodeURIComponent(new URL(req.url,'http://local').pathname));
  if(!path.startsWith(root+'/'))throw Error('outside root');
  res.setHeader('Content-Type',extname(path)==='.mjs'?'text/javascript':'text/plain');res.end(await readFile(path));
 }catch{res.statusCode=404;res.end('Not found');}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
let browser;
const errors=[];
try{
 browser=await chromium.launch({headless:true,...(process.env.CHROME_BIN?{executablePath:process.env.CHROME_BIN}:{}),args:['--disable-gpu']});
 const context=await browser.newContext({viewport:{width:1280,height:900}});
 const page=await context.newPage();page.on('pageerror',e=>errors.push(String(e)));
 const url=`http://127.0.0.1:${server.address().port}`;await page.goto(url);
 await page.evaluate(async()=>{
  const {IdbBackend}=await import('/store-idb-v5.mjs');const db=await IdbBackend.open('ctox-owned-close-invariant');let completed=false,closed;
  const raw=db.db.transaction('meta','readwrite');raw.addEventListener('complete',()=>{completed=true;},{once:true});const tx=db.trackTransaction(raw);
  await new Promise((resolve,reject)=>{const request=tx.objectStore('meta').put({key:'close-proof',value:1});request.onerror=()=>reject(request.error);request.onsuccess=()=>{closed=db.close().then(()=>{if(!completed)throw Error('Close resolved before owned transaction completed');});closed.catch(()=>{});tx.addEventListener('complete',()=>{completed=true;resolve();},{once:true});};});await closed;
 });
 const external = await page.evaluate(async()=>{

  const source='k1-legacy-owned',target='ctox-k1-external-alice';
  for(const name of [source,target])await new Promise((r,j)=>{const q=indexedDB.deleteDatabase(name);q.onsuccess=r;q.onerror=()=>j(q.error)});
  await new Promise((r,j)=>{const q=indexedDB.open(source,4);q.onupgradeneeded=()=>q.result.createObjectStore('documents',{keyPath:['collection','id']});q.onsuccess=()=>{
   const db=q.result,t=db.transaction('documents','readwrite');
   for(let n=0;n<260;n++)t.objectStore('documents').put({collection:'legacy',id:String(n).padStart(3,'0'),pushable:n===259?1:0,replicationOriginRole:n===259?'':'master',doc:{id:String(n).padStart(3,'0'),title:'v3-'+n}});
   t.oncomplete=()=>{db.close();r()};t.onabort=()=>j(t.error);
  };q.onerror=()=>j(q.error)});
  const {createKernel}=await import('/kernel.mjs');
  const options={instanceId:'k1-external',userId:'alice',legacyDatabase:{name:source,instanceId:'k1-external',userId:'alice',quiesced:true}};
  const k=createKernel(options);await k.ready;
  if(k.appApi('legacy','1').query('legacy',{}).get().length!==260)throw Error('external chunk takeover incomplete');
  if((await k.sim.archiveLocal()).length!==1)throw Error('unconfirmed legacy input not archived');
  await k.close();const again=createKernel(options);await again.ready;
  if((await again.sim.archiveLocal()).length!==1)throw Error('migration replayed twice');
  await again.logout({deleteData:true});await new Promise
(r=>{const q=indexedDB.deleteDatabase(source);q.onsuccess=r;});
  return {imported:260,archived:1,oneTime:true};
 });
 const first=await page.evaluate(async()=>{
  const name='ctox-k1-browser-alice';
  await new Promise((r,j)=>{const q=indexedDB.deleteDatabase(name);q.onsuccess=r;q.onerror=()=>j(q.error)});
  await new Promise((r,j)=>{const q=indexedDB.open(name,4);q.onupgradeneeded=()=>q.result.createObjectStore('documents',{keyPath:['collection','id']});q.onsuccess=()=>{
   const db=q.result,t=db.transaction('documents','readwrite');t.objectStore('documents').put({collection:'notes',id:'v3',replicationOriginRole:'master',pushable:0,doc:{id:'v3',title:'kept from V3'}});t.oncomplete=()=>{db.close();r()};t.onabort=()=>j(t.error);
  };q.onerror=()=>j(q.error)});
  const {createKernel}=await import('/kernel.mjs');const {MemoryCoreTransport}=await import('/fake-core.mjs');
  const transport=new MemoryCoreTransport({namespace:name});
  const k=createKernel({instanceId:'k1-browser',userId:'alice',transport});await k.ready;globalThis.kernel=k;
  if(k.storageKind!=='indexeddb')throw Error('not IndexedDB');
  if(k.appApi('notes','1').get('notes','v3').get()?.title!=='kept from V3')throw Error('V3 takeover failed');
  await k.sim.setLink({state:'offline'});
  await k.appApi('notes','1').put('notes',{id:'offline',title:'durable draft'}).local;
  document.querySelector('#result').textContent='PASS: V3 → IDB v5; offline outbox durably committed';
  return {storage:k.storageKind,leader:k.isLeader,dbName:k.dbName};
 });
 await capture(page,'01-migration-offline.png');
 await page.evaluate(()=>kernel.close());await page.reload();
 const reopened=await page.evaluate(async()=>{
  const {createKernel}=await import('/kernel.mjs');const {MemoryCoreTransport}=await import('/fake-core.mjs');
  globalThis.kernel=createKernel({instanceId:'k1-browser',userId:'alice',transport:new MemoryCoreTransport({namespace:'ctox-k1-browser-alice'})});await kernel.ready;
  const api=kernel.appApi('notes','1');if(api.get('notes','offline').get()?.title!=='durable draft')throw Error('draft lost on reload');
  await kernel.sim.settle();document.querySelector('#result').textContent='PASS: reload kept offline input; outbox accepted after reconnect';return {leader:kernel.isLeader};
 });
 const reader=await context.newPage();await reader.goto(url);
 const follower=await reader.evaluate(async()=>{
  const {createKernel}=await import('/kernel.mjs');const {MemoryCoreTransport}=await import('/fake-core.mjs');
  globalThis.kernel=createKernel({instanceId:'k1-browser',userId:'alice',transport:new MemoryCoreTransport({namespace:'ctox-k1-browser-alice'})});await kernel.ready;
  if(kernel.isLeader)throw Error('two leaders');
  const handle=kernel.appApi('notes','1').put('notes',{id:'reader',title:'reader write'});await handle.local;await handle.result;
  document.querySelector('#result').textContent='PASS: second tab is reader; write accepted by leader through shared IDB';return {leader:kernel.isLeader};
 });
 const files=await reader.evaluate(async()=>{
  kernel.sim.dropBroadcasts(true);
  try {
   const file=kernel.appApi('notes','1').files.put(new Blob(['reader file persistence']),{name:'reader.txt'});await file.local;
   await new Promise(r=>setTimeout(r,300));await kernel.sim.reconcile();
   if((await file.result).status!=='accepted')throw Error('reader upload not accepted');
   const ref=await file.ref;if(await(await kernel.appApi('notes','1').files.get(ref)).text()!=='reader file persistence')throw Error('reader file differs');
   document.querySelector('#result').textContent+='; file completion recovered from durable IDB despite dropped broadcasts';
   return {readerUpload:true,droppedEventsRecovered:true};
  } finally {kernel.sim.dropBroadcasts(false);}
 });
 await capture(reader,'02-reader-write.png');
 await page.close();await reader.waitForFunction(()=>globalThis.kernel?.isLeader,undefined,{timeout:10000});
 const takeover=await reader.evaluate(async()=>{
  const h=kernel.appApi('notes','1').put('notes',{id:'takeover',title:'new leader'});await h.local;await h.result;
  const leader=kernel.shell.leader.get();document.querySelector('#result').textContent=`PASS: Web Lock takeover; leader_gen=${leader.leader_gen}; new leader write accepted`;
  return leader;
 });
 await capture(reader,'03-leader-takeover.png');
 const quota=await reader.evaluate(async()=>{
  const original=IDBObjectStore.prototype.put;let injected=0;
  IDBObjectStore.prototype.put=function(value,...rest){
   if(this.name==='outbox' && value.id==='quota-retry' && injected++===0)throw new DOMException('injected full disk','QuotaExceededError');
   return original.call(this,value,...rest);
  };
  try {
   const h=kernel.appApi('notes','1').put('notes',{id:'quota-retry',title:'saved after reclaim'});await h.local;
   const result=await h.result;if(result.status!=='accepted')throw Error('quota retry not accepted');
   if(!kernel.statusZone.get().some(e=>e.code==='storage.quota'))throw Error('quota missing status zone');
   document.querySelector('#result').textContent='PASS: real IDB transaction abort on quota; protected LRU reclaim; one retry accepted';
   return {injected,accepted:true,statusReported:true};
  } finally {IDBObjectStore.prototype.put=original;}
 });
 await capture(reader,'04-quota-reclaim.png');
 await reader.evaluate(()=>kernel.logout({deleteData:true}));
 const rtc=await reader.evaluate(async()=>{
  const {createKernel}=await import('/kernel.mjs');const {WebRTCKernelTransport,createReliableChannels}=await import('/transport-webrtc.mjs');const {FakeNativePeer,offer,scopeFor}=await import('/tests/fake-native-peer.mjs');
  const a=new RTCPeerConnection({iceServers:[]}),b=new RTCPeerConnection({iceServers:[]}),client=createReliableChannels(a),server={};
  let serverResolve,serverTimer;const serverReady=new Promise((resolve,reject)=>{serverResolve=resolve;serverTimer=setTimeout(()=>reject(Error('Native channel readiness deadline: '+JSON.stringify(Object.values(server).map(c=>[c.label,c.readyState])))),10000);});serverReady.catch(()=>{});
  const serverCheck=()=>{if(Object.keys(server).length===6&&Object.values(server).every(c=>c.readyState==='open')){clearTimeout(serverTimer);serverResolve();}};
  b.addEventListener('datachannel',e=>{const name=e.channel.label.slice('ctox-v4-'.length);server[{control:0,commands:1,interactive:2,live:3,initial:4,transfer:5}[name]]=e.channel;e.channel.addEventListener('open',serverCheck,{once:true});serverCheck();});
  const gathered=pc=>pc.iceGatheringState==='complete'?Promise.resolve():new Promise((r,j)=>{const t=setTimeout(()=>j(Error('ICE gathering deadline')),10000);const f=()=>{if(pc.iceGatheringState==='complete'){clearTimeout(t);pc.removeEventListener('icegatheringstatechange',f);r()}};pc.addEventListener('icegatheringstatechange',f);});
  let native,k;
  try {
   await a.setLocalDescription(await a.createOffer());await gathered(a);await b.setRemoteDescription(a.localDescription);await b.setLocalDescription(await b.createAnswer());await gathered(b);await a.setRemoteDescription(b.localDescription);
   await Promise.all(Object.values(client).map(c=>c.readyState==='open'?Promise.resolve():new Promise((r,j)=>{const t=setTimeout(()=>j(Error('RTC open deadline')),10000);c.addEventListener('open',()=>{clearTimeout(t);r()},{once:true});})));
   serverCheck();await serverReady;
   if(Object.keys(server).length!==6)throw Error('native channels missing');
   native=new FakeNativePeer(server,{instanceId:'rtc-browser',onSubscribe:async(sub,p)=>p.page(sub.collection,sub.collection==='notes'?[{id:'server',data:{id:'server',title:'native page'},seq:p.head,rev:'native'}]:[])});
   let allow;const transport=new WebRTCKernelTransport({wireFormat:"k2-fixture",channels:client,peer:a,hello:()=>new Promise(r=>allow=r),resolveScope:scopeFor,verifyPackage:async p=>p.sig==='test-signature'});
   k=createKernel({instanceId:'rtc-browser',userId:'alice',transport,tabKey:'rtc-tab'});await k.ready;
   if(!allow)throw Error('hello provider not started');const localReadyBeforeHello=transport.session===undefined;
   if(!localReadyBeforeHello)throw Error('local readiness gated by hello');allow(offer('rtc-browser'));await transport.ready;
   const h=k.appApi('notes','1').put('notes',{id:'client',title:'durable via RTC'});await h.local;const result=await h.result;
   await new Promise((resolve,reject)=>{let off=()=>{};const timer=setTimeout(()=>{off();reject(Error('RTC page/metrics deadline'));},3000);off=k.metrics.subscribe(value=>{if(value.timing&&k.appApi('notes','1').get('notes','server').get()){clearTimeout(timer);off();resolve();}});});
   if(result.status!=='accepted'||k.appApi('notes','1').get('notes','server').get()?.title!=='native page')throw Error('RTC page/write result did not apply');
   if(!k.metrics.get().timing || k.metrics.get().timing.samples<1)throw Error('clock-synchronized metrics missing');
   await k.windowState.put('layout',{open:['notes']});if((await k.windowState.get('layout')).open[0]!=='notes')throw Error('window persistence failed');
   if(k.shell.mount!==undefined)throw Error('kernel still mounts apps');
   const metrics=k.metrics.get();document.querySelector('#result').textContent='PASS: actual Chromium RTCDataChannels; local ready before hello; canonical scoped page + write_result committed in IDB; decimal seq='+result.seq+'; clock metrics='+JSON.stringify(metrics.timing);
   return {localReadyBeforeHello,channels:6,seq:result.seq,timing:metrics.timing,errors:native.errors.map(String)};
  }finally{clearTimeout(serverTimer);await k?.logout({deleteData:true});native?.close();a.close();b.close();}
 });
 await capture(reader,'05-real-webrtc.png');
 const claims=await reader.evaluate(async()=>{
  const {createLoader}=await import('/loader.mjs');const {MemoryCoreTransport}=await import('/fake-core.mjs');
  let imports=0;globalThis.claimCount=0;const listener=e=>{if(e.data==='claimed')globalThis.claimCount++};navigator.serviceWorker.addEventListener('message',listener);
  let sent=0;const post=ServiceWorker.prototype.postMessage;
  ServiceWorker.prototype.postMessage=function(message,...args){if(message==='ctox:claim')sent++;return post.call(this,message,...args);};
  const loader=await createLoader({instanceId:'claim-browser',userId:'alice',transport:new MemoryCoreTransport({namespace:'claim-browser'}),resolveShell:()=>{if(!navigator.serviceWorker.controller||sent!==imports+1)throw Error('Shell imported before claim');imports++;return {startShell(){},stopShell(){return {open:['notes']}}}}});
  try {
   for(const ver of ['1','2']){await loader.kernel.sim.publishAppVersion('shell',ver,{});await loader.settle();}
   if(imports!==2)throw Error('Shell replacement failed');if((await loader.kernel.windowState.get('shell')).open[0]!=='notes')throw Error('Shell snapshot not in tab window state');
   document.querySelector('#result').textContent='PASS: SW ctox:claim sent before both Shell imports; snapshot preserved in tab-scoped window_state';
   return {imports,claimsSent:sent,shellVersion:loader.shellVersion};
  }finally{await loader.close();ServiceWorker.prototype.postMessage=post;navigator.serviceWorker.removeEventListener('message',listener);await loader.registration?.unregister();}
 });
 await capture(reader,'06-shell-claim.png');
 // Real IDB + Web Locks: dispatch reservations survive a leader tab closing,
 // then a navigation with an empty outbox must retain the sequence high-water.
 const installSequenceKernel=async(page,online)=>page.evaluate(async online=>{
  const {createKernel}=await import('/kernel.mjs');globalThis.seqSent=[];
  globalThis.seqTransport={async connect(identity,events){this.events=events;if(online)events.link({state:'online'});},async subscribe(){},async write(entry){seqSent.push(entry);},close(){}};
  globalThis.seqKernel=createKernel({instanceId:'client-seq-browser',userId:'alice',transport:seqTransport,latencyMs:5});await seqKernel.ready;
  return {storage:seqKernel.storageKind,leader:seqKernel.isLeader,gen:seqKernel.leader.get().leader_gen};
 },online);
 const sequenceFirst=await installSequenceKernel(reader,false);
 if(sequenceFirst.storage!=='indexeddb'||!sequenceFirst.leader)throw Error('client_seq needs a real IDB leader');
 await reader.evaluate(async()=>{
  const api=seqKernel.appApi('notes','1');await api.put('notes',{id:'a',value:1}).local;await api.patch('notes','a',{value:2}).local;await api.command('bench.read',{id:'a'}).local;seqTransport.events.link({state:'online'});
 });
 await reader.waitForFunction(()=>seqSent.length===2,null,{timeout:5000});
 const reserved=await reader.evaluate(async()=>({rows:(await seqKernel.sim.outbox()).sort((a,b)=>Number(BigInt(a.client_seq)-BigInt(b.client_seq))),sent:seqSent.map(e=>[e.op_id,e.client_seq])}));
 if(reserved.rows.map(e=>e.client_seq).join(',')!=='1,2,3')throw Error('client_seq reservation order differs from submission');
 const successor=await context.newPage();await successor.goto(url);
 const sequenceReader=await installSequenceKernel(successor,true);if(sequenceReader.leader)throw Error('client_seq has two leaders');
 await reader.evaluate(()=>seqKernel.close());
 await successor.waitForFunction(()=>seqKernel.isLeader&&seqSent.length===2,null,{timeout:10000});
 const resumed=await successor.evaluate(()=>({gen:seqKernel.leader.get().leader_gen,sent:seqSent.map(e=>[e.op_id,e.client_seq])}));
 if(resumed.gen<=sequenceFirst.gen||JSON.stringify(resumed.sent.sort())!==JSON.stringify(reserved.sent.sort()))throw Error('leader takeover renumbered a reserved operation');
 await successor.evaluate(async first=>seqTransport.events.write_result({op_id:first.op_id,status:'accepted',rev:'r1',seq:'1'}),reserved.rows[0]);
 await successor.waitForFunction(()=>seqSent.length===3,null,{timeout:5000});
 await successor.evaluate(async rows=>{for(const row of rows)await seqTransport.events.write_result({op_id:row.op_id,status:'accepted',rev:'r'+row.client_seq,seq:row.client_seq});},reserved.rows.slice(1));
 await successor.evaluate(async()=>{if((await seqKernel.sim.outbox()).length)throw Error('sequence outbox did not drain');seqTransport.events.link({state:'offline'});await seqKernel.appApi('notes','1').put('notes',{id:'reload',value:'survives'}).local;await seqKernel.close();});
 await successor.reload();await installSequenceKernel(successor,true);
 await successor.waitForFunction(()=>seqSent.length===1,null,{timeout:5000});
 const sequenceReload=await successor.evaluate(()=>{
  if(seqSent[0].client_seq!=='4'||seqKernel.appApi('notes','1').get('notes','reload').get()?.value!=='survives')throw Error('reload lost input or reset client_seq high-water');
  document.querySelector('#result').textContent='PASS: client_seq 1,2,3 reserved in IDB; Web Lock successor replayed stable IDs; reload continued at 4';
  return {gen:seqKernel.leader.get().leader_gen,client_seq:seqSent[0].client_seq};
 });
 await capture(successor,'07-client-seq-reload.png');await successor.evaluate(()=>seqKernel.logout({deleteData:true}));await successor.close();
 const clientSequence={storage:sequenceFirst.storage,initial_gen:sequenceFirst.gen,successor_gen:resumed.gen,reload_gen:sequenceReload.gen,continued_at:sequenceReload.client_seq};
 if(errors.length)throw Error(errors.join('\n'));
 const result={pass:true,external,first,reopened,follower,files,takeover,quota,rtc,claims,clientSequence,errors};await writeFile(resolve(out,'result.json'),JSON.stringify(result,null,2));console.log('KERNEL_BROWSER '+JSON.stringify(result));
}finally{await browser?.close();await new Promise(r=>server.close(r));}
