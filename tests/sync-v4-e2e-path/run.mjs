// Bounded Linux bench using DevOps' loopback TURN/signaling setup. No product credentials.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import crypto from 'node:crypto';
import readline from 'node:readline';
import {spawn} from 'node:child_process';
import {createRequire} from 'node:module';
import assert from 'node:assert/strict';
const out=path.resolve(process.env.E2E_OUT),root=process.cwd();fs.mkdirSync(out,{recursive:true});
const require=createRequire(process.env.PLAYWRIGHT_MODULE),{chromium}=require('playwright'),{WebSocketServer}=require('ws');
const report={started:new Date().toISOString(),path:'Chromium kernel ↔ TURN ↔ native P4/P5/P6/P8 SQLite store',pass:false,events:[],errors:[]};
let native,browser,server,wss,turn,page;const sockets=new Map(),owned=[];
const waiters=new Set();
function emit(event){report.events.push(event);for(const waiter of [...waiters])if(waiter.predicate(event)){waiters.delete(waiter);clearTimeout(waiter.timer);waiter.resolve(event);}}
function event(predicate){return new Promise((resolve,reject)=>{const waiter={predicate,resolve,timer:setTimeout(()=>{waiters.delete(waiter);reject(Error('native event deadline'));},15000)};waiters.add(waiter);});}
function send(kind){native.stdin.write(JSON.stringify({kind})+'\n');}
async function command(kind,expected=kind){const ready=event(row=>row.kind===expected);send(kind);return ready;}
async function stop(child){if(!child||child.exitCode!==null)return;child.kill('SIGTERM');await Promise.race([new Promise(r=>child.once('exit',r)),new Promise(r=>setTimeout(r,1500))]);if(child.exitCode===null)child.kill('SIGKILL');}
try{
 const turnPort=34793;
 turn=spawn(process.env.TURN_BIN,['-n','--lt-cred-mech','--realm=e2e.test','--user=bench:bench','--no-cli','--no-tls','--no-dtls','--no-tcp-relay','--allow-loopback-peers','--relay-threads=2',`--userdb=${out}/turn.sqlite`,'--listening-ip=127.0.0.1','--relay-ip=127.0.0.1',`--listening-port=${turnPort}`,'--min-port=49300','--max-port=49399',`--pidfile=${out}/turn.pid`,`--log-file=${out}/turn.log`,'--no-stdout-log'],{env:{...process.env,LD_LIBRARY_PATH:process.env.TURN_LIBS??''},stdio:['ignore','ignore',fs.openSync(path.join(out,'turn.stderr'),'w')]});owned.push(turn);
 const broadcast=()=>{for(const [id,row]of sockets)if(row.room){const peers=[...sockets].filter(([other,x])=>other!==id&&x.room===row.room).map(([peerId,x])=>({peerId,role:x.role,protocol:'ctox-rxdb-v1'}));row.socket.send(JSON.stringify({type:'joined',otherPeerIds:peers.map(p=>p.peerId),peers}));}};
 server=http.createServer((request,response)=>{const name=new URL(request.url,'http://local').pathname;let file;
  if(name==='/'){response.setHeader('Content-Type','text/html');response.end('<!doctype html><title>Native V4 integration</title><pre>Starting native V4 path</pre>');return;}
  if(name==='/browser.mjs')file=path.join(root,'tests/sync-v4-e2e-path/browser.mjs');
  else if(name.startsWith('/kernel/'))file=path.resolve(root,'src/apps/business-os/kernel','.'+name.slice(7));
  else if(name==='/rxdb/src/hello.mjs')file=path.join(root,'src/apps/business-os/rxdb/src/hello.mjs');
  if(!file||!file.startsWith(root+'/')||!file.endsWith('.mjs')){response.writeHead(404);response.end();return;}
  try{response.setHeader('Content-Type','text/javascript');response.end(fs.readFileSync(file));}catch{response.writeHead(404);response.end();}
 });
 wss=new WebSocketServer({server,path:'/signal'});wss.on('connection',(socket,req)=>{const url=new URL(req.url,'http://local'),id=url.searchParams.get('peerId')??crypto.randomUUID(),row={socket,room:null,role:url.searchParams.get('role')??'browser'};sockets.set(id,row);socket.send(JSON.stringify({type:'init',yourPeerId:id}));
  socket.on('message',raw=>{const message=JSON.parse(raw);if(message.type==='join'){row.room=message.room;broadcast();}else if(message.type==='signal'){const target=sockets.get(message.receiverPeerId);if(row.room&&target?.room===row.room)target.socket.send(JSON.stringify({...message,senderPeerId:id}));}});
  socket.on('close',()=>{if(sockets.get(id)===row)sockets.delete(id);broadcast();});
 });await new Promise(r=>server.listen(0,'127.0.0.1',r));
 const port=server.address().port,room='e2e-'+crypto.randomBytes(8).toString('hex'),signalingUrl=`ws://127.0.0.1:${port}/signal`,turnUrl=`turn:127.0.0.1:${turnPort}?transport=udp`;
 native=spawn(process.env.E2E_NATIVE,[],{stdio:['pipe','pipe',fs.openSync(path.join(out,'native.stderr'),'w')]});owned.push(native);
 readline.createInterface({input:native.stdout}).on('line',line=>{try{emit(JSON.parse(line));}catch{report.errors.push('invalid native stdout');}});
 const ready=event(row=>row.kind==='ready');native.stdin.write(JSON.stringify({db:path.join(out,'sync.sqlite'),signalingUrl:signalingUrl+'?peerId=native000001&role=ctox_instance',turnUrl,room})+'\n');await ready;
 browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_BIN??'/usr/bin/google-chrome',args:['--disable-gpu']});const context=await browser.newContext();page=await context.newPage();page.on('pageerror',error=>report.errors.push(error.message));
 await page.goto(`http://127.0.0.1:${port}`);report.first=await page.evaluate(async config=>(await import('/browser.mjs')).start(config),{signalingUrl,turnUrl,room});await page.screenshot({path:path.join(out,'01-native-page.png')});
 await command('push','pushed');report.push=await page.evaluate(()=>e2e.pushed());
 await command('arm_drop','drop_armed');const committed=event(row=>row.kind==='write_committed');report.op_id=await page.evaluate(()=>e2e.write());report.beforeDrop=await committed;
 assert.equal(report.beforeDrop.op_id,report.op_id);assert.equal(report.beforeDrop.status,'accepted');
 await command('drop','dropped');await page.waitForFunction(()=>e2e.kernel.shell.metrics.get().link.state==='offline',{timeout:12000});
 await page.evaluate(()=>e2e.reconnect());report.result=await page.evaluate(()=>e2e.accepted());assert.equal(report.result.status,'accepted');assert.equal(report.result.seq,report.beforeDrop.seq,'resend must return the committed result');
 const activation=event(row=>row.kind==='activation_committed');await page.evaluate(()=>e2e.activation());await activation;
 report.beforeCommand=await command('status');assert.equal(report.beforeCommand.ops,1);assert.equal(report.beforeCommand.resend_rows,1);assert.equal(report.beforeCommand.head_seq,report.beforeDrop.seq,"resend must not allocate another global sequence");
 report.command=await page.evaluate(()=>e2e.command());assert.equal(report.command.status,'accepted');report.status=await command('status');assert.equal(report.status.ops,2);assert.equal(report.status.activations,1);
 report.browser=await page.evaluate(()=>e2e.stats());assert.equal(report.browser.errors.length,0);assert.ok(report.browser.wire.maxFrame<=16384);assert.ok(report.browser.wire.maxBuffered<=131072);
 await page.screenshot({path:path.join(out,'02-resend-once.png')});
 await page.evaluate(async()=>{const {IdbBackend}=await import('/kernel/store-idb-v5.mjs');const db=await IdbBackend.open('ctox-close-proof');let completed=false;
  const tx=db.trackTransaction(db.db.transaction('meta','readwrite'));const store=tx.objectStore('meta');let closed;
  await new Promise((resolve,reject)=>{const req=store.put({key:'close-proof',value:1});req.onerror=()=>reject(req.error);req.onsuccess=()=>{closed=db.close().then(()=>{if(!completed)throw Error('Close resolved before owned transaction completed');});tx.addEventListener('complete',()=>{completed=true;resolve();},{once:true});};});await closed;
 });
 report.pass=report.errors.length===0;console.log('E2E_PATH '+JSON.stringify({pass:report.pass,first:report.first,push:report.push,command:report.command,resend:report.result,relay:report.browser.relay}));assert.ok(report.pass);
}catch(error){report.error=error.stack;console.error(error);process.exitCode=1;}
finally{
 if(page)await page.evaluate(()=>globalThis.e2e?.close()).catch(()=>{});if(browser)await browser.close();
 for(const waiter of waiters){clearTimeout(waiter.timer);waiter.resolve({kind:'cleanup'});}waiters.clear();
 for(const child of owned.reverse())await stop(child);
 if(wss){for(const socket of wss.clients)socket.terminate();await new Promise(r=>wss.close(r));}if(server)await new Promise(r=>server.close(r));
 report.finished=new Date().toISOString();report.processesStopped=true;fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(report,null,2));
}
