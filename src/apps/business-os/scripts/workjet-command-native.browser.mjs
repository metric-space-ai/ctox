// Isolated browser -> WebRTC -> Rust/SQLite protocol fixture. No operator state,
// credentials or production instance. The daemon's demo collection stores seeded
// terminal receipts; it does NOT execute the Business OS project-list handler.
import assert from 'node:assert/strict';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import {createRequire} from 'node:module';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { decodeChunk } from '../rxdb/src/chunk-decoder.mjs';

const args = process.argv.slice(2);
const option = name => args[args.indexOf(name) + 1];
const binary = option('--native');
assert.ok(args.includes('--native') && binary?.startsWith('/'), 'explicit isolated native wire-daemon binary required');
if(args.includes('--baseline'))throw Error('Use the recorded V4 kernel baseline; executable legacy input is no longer accepted');
const shell = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const {chromium}=createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE??resolve(process.env.PLAYWRIGHT_MODULE_PATH,'index.js'));
const child = spawn(binary, [], { stdio: ['pipe', 'pipe', 'inherit'] });
const lines = createInterface({ input: child.stdout });
const pending = new Map();
function awaitMessage(key, match, done = () => true) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(key); reject(new Error('native fixture deadline: ' + key)); }, 5000);
    pending.set(key, { match, docs: [], resolve: value => { clearTimeout(timer); resolve(value); }, reject: error => {clearTimeout(timer);reject(error);}, done });
  });
}
lines.on('line', line => {
  const message = JSON.parse(line);
  nativeFrames.push({kind:message.kind,message:message.message,method:message.frame?.method,error:message.frame?.error,params:message.frame?.params?.[0]?.error});
  const failure=message.kind==='error'?message.message:message.frame?.error??(message.frame?.method==='rxdb.query.error'?JSON.stringify(message.frame.params):null);
  if(failure){for(const slot of pending.values())slot.reject(new Error(String(failure)));pending.clear();return;}
  for (const [key, slot] of pending) {
    if (!slot.match(message)) continue;
    slot.docs.push(message);
    if (slot.done(message)) { pending.delete(key); slot.resolve(slot.docs); }
  }
});
const ready = awaitMessage('ready', message => message.kind === 'ready');
const send = value => child.stdin.write(JSON.stringify(value) + '\n');
const nativeQueries = [], nativeFrames = [];
async function nativeRead(request) {
  const started = performance.now();
  if (request.fixtureSeed) {
    const written = awaitMessage(request.requestId, message => message.kind === 'seeded');
    send({ kind: 'seed', collection: 'demo', docs: request.fixtureSeed });
    await written;
    return true;
  }
  const nativeRequest = { ...request, collectionName: 'demo', schemaVersion: 0 };
  const response = awaitMessage(request.requestId,
    message => message.kind === 'wire' && message.frame?.method === 'rxdb.query.chunk'
      && message.frame.params?.[0]?.requestId === request.requestId,
    message => message.frame.params[0].complete === true);
  send({ kind: 'request', peerIdentity: 'isolated-guest-browser', message: {
    id: request.requestId, method: 'rxdb.query.fetch', params: [nativeRequest],
  } });
  const frames = await response;
  const documents = [];
  for (const frame of frames) documents.push(...await decodeChunk(frame.frame.params[0]));
  nativeQueries.push({ durationMs: performance.now() - started, documents: documents.length });
  return { documents, authoritativeRevision: 'isolated-native-fixture' };
}
const server = http.createServer((request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost');
    if (url.pathname === '/') {
      response.writeHead(200, { 'content-type': 'text/html' });
      response.end('<!doctype html><title>Isolated Workjet command wait</title>'); return;
    }
    const relative = url.pathname.replace(/^\/(?:baseline\/)?/, '');
    const path = resolve(shell, relative);
    assert.ok(path.startsWith(shell + sep));
    const content = readFileSync(path);
    response.writeHead(200, { 'content-type': 'text/javascript' }); response.end(content);
  } catch { response.writeHead(404); response.end(); }
});
let browser;
const report = { schema: 'ctox.workjet-command-native-fixture.v1',
  scope: 'V4 kernel on real Chromium IndexedDB plus WebRTC DataChannel and Rust SQLite dispatcher; seeded terminal receipt, not native V4 framing, handler execution/enrollment or installed acceptance',
  nativeBinary: binary, samples: [], nativeQueries, nativeFrames };
try {
  await ready;
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();
  const nativePage = await context.newPage();
  const origin = `http://127.0.0.1:${server.address().port}`;
  await Promise.all([page.goto(origin), nativePage.goto(origin)]);
  await nativePage.exposeFunction('nativeRead', nativeRead);
  await nativePage.evaluate(() => {
    globalThis.pc = new RTCPeerConnection({ iceServers: [] });
    pc.ondatachannel = event => {
      globalThis.channel = event.channel;
      channel.onmessage = async event => {
        const { id, request } = JSON.parse(event.data);
        try { channel.send(JSON.stringify({ id, result: await nativeRead(request) })); }
        catch (error) { channel.send(JSON.stringify({ id, error: String(error) })); }
      };
    };
  });
  const offer = await page.evaluate(async () => {
    globalThis.pc = new RTCPeerConnection({ iceServers: [] });
    globalThis.channel = pc.createDataChannel('ctox-query-fixture');
    globalThis.requests = new Map();
    channel.onmessage = event => {
      const reply = JSON.parse(event.data); const slot = requests.get(reply.id);
      if (!slot) return; requests.delete(reply.id);
      reply.error ? slot.reject(new Error(reply.error)) : slot.resolve(reply.result);
    };
    globalThis.queryNative = request => new Promise((resolve, reject) => {
      requests.set(request.requestId, { resolve, reject });
      channel.send(JSON.stringify({ id: request.requestId, request }));
    });
    await pc.setLocalDescription(await pc.createOffer());
    await new Promise(resolve => { if (pc.iceGatheringState === 'complete') resolve();
      else pc.onicegatheringstatechange = () => { if (pc.iceGatheringState === 'complete') resolve(); }; });
    return pc.localDescription.toJSON();
  });
  const answer = await nativePage.evaluate(async offer => {
    await pc.setRemoteDescription(offer); await pc.setLocalDescription(await pc.createAnswer());
    await new Promise(resolve => { if (pc.iceGatheringState === 'complete') resolve();
      else pc.onicegatheringstatechange = () => { if (pc.iceGatheringState === 'complete') resolve(); }; });
    return pc.localDescription.toJSON();
  }, offer);
  await page.evaluate(answer => pc.setRemoteDescription(answer), answer);
  await page.waitForFunction(() => channel.readyState === 'open', { timeout: 5000 });
  for (const retained of [false,true]) {
    const sample=await page.evaluate(async retained=>{
      const {createKernel}=await import('/kernel/kernel.mjs');
      const {KernelTransport}=await import('/kernel/transport.mjs');
      const {observeCommand}=await import('/kernel/command-observer.mjs');
      let seq=1,requestsStarted=0,phase='ready',nativeFailure=null;
      class NativeFixture extends KernelTransport {
        async connect(_,events){this.events=events;events.link({state:'online',rtt_ms:1,relay:false});}
        async subscribe(){}
        async write(entry){try{phase='write';
          const terminal={...entry.body,status:'done',result:{ok:true,collection:'workjet_projects',count:0,truncated:false},_rev:'1-fixture',_deleted:false,_meta:{lwt:Date.now()},_attachments:{}};
          requestsStarted++;
          await queryNative({requestId:entry.op_id+'-seed',fixtureSeed:[{id:terminal.id,payload:JSON.stringify(terminal),_rev:'1-fixture',_deleted:false,_meta:{lwt:Date.now()},_attachments:{}}]});
          phase='seeded';
          const response=await queryNative({requestId:entry.op_id+'-read',queryFingerprint:'s9:'+entry.id,query:{selector:{id:entry.id},limit:1}});
          phase='read';
          const stored=response.documents.find(doc=>doc.id===entry.id);
          const doc=stored?JSON.parse(stored.payload):null;
          if(!doc || doc.status!=='done')throw Error('native SQLite terminal receipt missing');
          await this.events.write_result({op_id:entry.op_id,status:'accepted',rev:'native-'+seq,seq:String(seq++),gen:1});
          await this.events.page({collection:'business_commands',epoch:'1',scope_hash:'local',gen:1,cursor_to:String(seq),complete:true,docs:[{id:doc.id,data:doc,seq:String(seq++),rev:'native-terminal'}]});phase='page';
        }catch(error){nativeFailure=String(error);throw error;}}
      }
      const kernel=createKernel({instanceId:'native-delivery-'+retained,userId:'fixture',multiTab:false,transport:new NativeFixture()});await kernel.ready;
      const api=kernel.appApi('probe','1'),held=retained?api.query('business_commands',{}):null;let off;
      if(held)off=held.subscribe(()=>{});
      const started=performance.now(),handle=api.command('probe.project.list',{limit:10});let diagnostic;
      const observer=observeCommand(handle,{timeoutMs:600,onTiming:value=>{diagnostic=value}});
      try{
        await observer.local;const [,terminal]=await Promise.all([observer.accepted,observer.done]);
        if(terminal.status!=='done')throw Error('terminal receipt missing');
        return {retained,id:handle.id,status:terminal.status,elapsedMs:performance.now()-started,requestsStarted,diagnostic,channel:channel.readyState,queueRows:api.query('ctox_queue_tasks',{}).get().length,storage:kernel.storageKind};
      }catch(error){throw Error(JSON.stringify({message:error.message,phase,nativeFailure,requestsStarted,timing:observer.snapshot(),status:kernel.status.forApp('kernel').get()}));}finally{off?.();observer.close();await kernel.logout({deleteData:true});}
    },retained);
    report.samples.push(sample);
    assert.equal(sample.status,'done');assert.ok(sample.elapsedMs<600);assert.equal(sample.storage,'indexeddb');assert.equal(sample.channel,'open');
    assert.equal(sample.requestsStarted,1);assert.equal(sample.queueRows,0);
    assert.ok(sample.diagnostic.stagesMs.local_persistence>=0);assert.ok(sample.diagnostic.stagesMs.native_acceptance>=0);assert.ok(sample.diagnostic.stagesMs.total>=0);
  }
  await page.evaluate(() => { channel.close(); pc.close(); });
  await nativePage.evaluate(() => { channel.close(); pc.close(); });
  await context.close();
  if (args.includes('--output')) writeFileSync(option('--output'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report));
} catch(error) {
  report.error=String(error);if(args.includes('--output'))writeFileSync(option('--output'),JSON.stringify(report,null,2)+'\n');console.error('NATIVE_FIXTURE_FAILURE '+JSON.stringify(report));throw error;
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
  if (child.exitCode === null && child.signalCode === null) {
    const exited = new Promise(resolve => child.once('exit', resolve));
    send({ kind: 'shutdown' }); child.stdin.end();
    const stopTimer = setTimeout(() => child.kill('SIGTERM'), 2000);
    const killTimer = setTimeout(() => child.kill('SIGKILL'), 5000);
    await exited;
    clearTimeout(stopTimer); clearTimeout(killTimer);
  }
  lines.close();
}
