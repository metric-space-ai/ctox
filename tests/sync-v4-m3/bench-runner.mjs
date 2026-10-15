import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import crypto from 'node:crypto';
import readline from 'node:readline';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { CLASSES, reconcile, quantiles } from './counters.mjs';

const config = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const require = createRequire(path.join(config.modulesRoot, 'playwright/package.json'));
const { chromium } = require('playwright'), { WebSocketServer } = require('ws');
const sleep = ms => new Promise(r => setTimeout(r, ms));
const out = config.out;
fs.mkdirSync(out, { recursive: true });
const evidence = { schema: 'ctox.transport.bench.v1', startedAt: new Date().toISOString(), config,
  pids: { controller: process.pid }, nativeEvents: [], connections: [], profilesApplied: config.netem,
  benchmarkEligible: config.durationSeconds >= 600, noApplicationReplay: true, terminal: false };
const save = () => fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify(evidence, null, 2));
const children = [], contexts = [], routes = new Map(), membership = new Map(), peerIds = new Map();
let browser, server, wsServer, finished = false;
const inflight = new Map(), attemptTimes = new Map();
const nativeTx = [], status = [], nativeExit = {};
let native;
const hashFile = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
function broadcast() {
  for (const [id, row] of routes) if (row.room) {
    const peers = [...routes].filter(([other, x]) => other !== id && x.room === row.room)
      .map(([peerId, x]) => ({ peerId, role: x.role, protocol: 'ctox-rxdb-v1', client: x.client }));
    row.socket.send(JSON.stringify({ type: 'joined', otherPeerIds: peers.map(x => x.peerId), peers }));
  }
}
function nativeSend(row) {
  const at = performance.now();
  attemptTimes.set(row.id, at);
  nativeTx.push({ kind: 'attempt', at, ...row });

  return new Promise(resolve => {
    const timer = setTimeout(() => {
      nativeTx.push({ kind: 'send_completion_timeout', at: performance.now(), id: row.id, class: row.class });
      inflight.delete(row.id); resolve(false);
    }, 35000);

    inflight.set(row.id, { resolve: value => { clearTimeout(timer); resolve(value); } });
    native.stdin.write(JSON.stringify({ kind: 'send', ...row }) + '\n');
  });
}
async function cleanup() {
  if (finished) return; finished = true;
  for (const context of contexts) await context.close().catch(() => {});
  if (browser) await browser.close().catch(() => {});
  for (const child of children) if (child.exitCode === null) child.kill('SIGTERM');
  await Promise.all(children.map(child => child.exitCode !== null ? undefined
    : Promise.race([new Promise(resolve => child.once('exit', resolve)), sleep(2000)])));
  for (const child of children) if (child.exitCode === null) child.kill('SIGKILL');
  for (const { resolve } of inflight.values()) resolve(false); inflight.clear();
  if (wsServer) { for (const client of wsServer.clients) client.terminate(); wsServer.close(); }
  if (server) await new Promise(r => server.close(r));
  evidence.finishedAt = new Date().toISOString(); evidence.terminal = true; save();
}
process.on('SIGTERM', () => { evidence.interrupted = true; void cleanup().then(() => process.exit(143)); });
try {
  evidence.revisions = { nativeBinarySha256: hashFile(config.nativeBin), nativeRevisionDeclared: config.nativeRevision,
    browserRevisionDeclared: config.browserRevision, browserTransportSha256: hashFile(path.join(config.browserRoot, 'webrtc-native.mjs')),
    adapterSha256: hashFile(config.browserAdapter) };
  const turn = spawn(config.turnserver, ['-n', '--no-auth', '--no-cli', '--no-tls', '--no-dtls', '--no-tcp-relay',
    '--allow-loopback-peers', '--relay-threads=2', '--userdb=' + out + '/turn.sqlite', '--listening-ip=127.0.0.1', '--relay-ip=127.0.0.1', `--listening-port=${config.turnPort}`,
    '--min-port=49152', '--max-port=49251', `--pidfile=${out}/turn.pid`, `--log-file=${out}/turn.log`, '--no-stdout-log'],
  { env: { ...process.env, LD_LIBRARY_PATH: config.turnLibs || '' }, stdio: ['ignore', 'ignore', fs.openSync(path.join(out, 'turn-stderr.log'), 'w')] });
  children.push(turn); evidence.pids.turn = turn.pid;
  server = http.createServer((request, response) => {
    const url = new URL(request.url, 'http://bench.invalid');
    let file;
    if (url.pathname === '/') { response.writeHead(200, { 'content-type': 'text/html' }); response.end('<!doctype html><title>CTOX transport bench — no apps</title>'); return; }
    if (url.pathname === '/bench/browser.mjs') file = config.browserAdapter;
    else if (url.pathname.startsWith('/runtime/')) {
      file = path.resolve(config.browserRoot, '.' + url.pathname.slice('/runtime'.length));
      if (!file.startsWith(path.resolve(config.browserRoot) + path.sep) || !file.endsWith('.mjs')) file = null;
    }
    if (!file || !fs.existsSync(file)) { response.writeHead(404); response.end(); return; }
    response.writeHead(200, { 'content-type': 'text/javascript', 'cache-control': 'no-store' }); response.end(fs.readFileSync(file));
  });
  wsServer = new WebSocketServer({ noServer: true });
  server.on('upgrade', (req, socket, head) => {
    if (new URL(req.url, 'http://bench').pathname !== '/signal') { socket.destroy(); return; }
    wsServer.handleUpgrade(req, socket, head, ws => wsServer.emit('connection', ws, req));
  });
  wsServer.on('connection', (socket, req) => {
    const url = new URL(req.url, 'http://bench'); const id = url.searchParams.get('peerId') || crypto.randomUUID();
    const row = { socket, role: url.searchParams.get('role') || 'browser', client: url.searchParams.get('client') || id, room: null };
    routes.set(id, row); socket.send(JSON.stringify({ type: 'init', yourPeerId: id }));
    evidence.signaling ||= [];

    socket.on('message', raw => {
      let message; try { message = JSON.parse(raw); } catch { return; }
      if (evidence.signaling.length < 1000) evidence.signaling.push({ at: performance.now(), from: id,
        type: message.type, receiver: message.receiverPeerId,
        signalType: (message.data || message.signal)?.type,
        candidate: (message.data || message.signal)?.candidate?.candidate });


      if (message.type === 'join') { row.room = message.room; membership.set(id, row.client); broadcast(); }
      else if (message.type === 'signal') {
        const target = routes.get(message.receiverPeerId);
        if (row.room && target?.room === row.room) target.socket.send(JSON.stringify({ ...message, senderPeerId: id }));
      }
    });
    socket.on('close', () => { if (routes.get(id) === row) routes.delete(id); broadcast(); });
  });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  const port = server.address().port, room = 'bench-' + crypto.randomBytes(8).toString('hex');
  const signalingUrl = `ws://127.0.0.1:${port}/signal`, turnUrl = `turn:127.0.0.1:${config.turnPort}?transport=udp`;
  native = spawn(config.nativeBin, [], { stdio: ['pipe', 'pipe', fs.openSync(path.join(out, 'native-stderr.log'), 'w')] });
  children.push(native); evidence.pids.native = native.pid;
  native.on('exit', (code, signal) => { Object.assign(nativeExit, { code, signal }); });
  readline.createInterface({ input: native.stdout }).on('line', line => {
    let event; try { event = JSON.parse(line); } catch { return; }
    if (evidence.nativeEvents.length < 250000) evidence.nativeEvents.push({ at: performance.now(), ...event });
    else evidence.nativeMetadataOverflow = true;
    if (event.kind === 'connected') {
      const client = membership.get(event.peer) || event.peer;
      peerIds.set(client.split('-g')[0], event.peer);
    }
    if (event.kind === 'disconnected') for (const [slot, id] of peerIds) if (id === event.peer) peerIds.delete(slot);
    if (event.kind === 'send_result') { nativeTx.push(event); inflight.get(event.id)?.resolve(event.ok); inflight.delete(event.id); }
    if (event.kind === 'ack') {
      const at = performance.now(), offeredAt = attemptTimes.get(event.id);
      nativeTx.push({ ...event, at, transportLatencyMs: event.latencyMs,
        latencyMs: Number.isFinite(offeredAt) ? at - offeredAt : null, class: event.id.split(':')[2] });
      attemptTimes.delete(event.id);
    }

    if (event.kind === 'status') {
      status.push(event.transport);
      if (status.length % 5 === 0) fs.writeFileSync(path.join(out, 'progress.json'), JSON.stringify({
        at: new Date().toISOString(), phase: evidence.phase || 'establishing',
        nativeEvents: evidence.nativeEvents.length,
        receivedMessages: evidence.nativeEvents.filter(e => e.kind === 'received').length,
        receivedBodyBytes: evidence.nativeEvents.filter(e => e.kind === 'received').reduce((n, e) => n + e.bytes, 0),
        terminal: false,
      }, null, 2));
    }

  });
  native.stdin.write(JSON.stringify({ signalingUrl: signalingUrl + '?peerId=native000001&role=ctox_instance', room, turnUrl, nativeHostOnly: true }) + '\n');
  const browserTmpRoot = config.browserTmpRoot || '/home/metricspace/build-lane/tmp';
  fs.mkdirSync(browserTmpRoot, { recursive: true });
  const browserTmp = fs.mkdtempSync(path.join(browserTmpRoot, 'bt-'));

  evidence.browserTmp = browserTmp;
  browser = await chromium.launch({ headless: true, executablePath: config.chrome, env: { ...process.env, TMPDIR: browserTmp } });
  evidence.browserVersion = browser.version();
  const pages = [];
  for (let i = 0; i < config.peers; i++) {
    const context = await browser.newContext(); contexts.push(context);
    const page = await context.newPage(); pages.push(page);
    page.on('pageerror', error => {
      evidence.pageErrors ||= [];
      if (evidence.pageErrors.length < 100) evidence.pageErrors.push(String(error.message));
    });

    await page.goto(`http://127.0.0.1:${port}/`);
    await page.evaluate(async values => { const adapter = await import('/bench/browser.mjs'); await adapter.start(values); },
      { signalingUrl, room, turnUrl, slot: `browser${i}` });
  }
  // Bounded establishment deadline; no resource/admission retry job.
  await Promise.all(pages.map(page => page.waitForFunction(() => window.bench?.ready(), null, { timeout: 45000 })));
  await sleep(2000);
  const proof = await Promise.all(pages.map(page => page.evaluate(() => window.bench.snapshot().samples)));
  if (!proof.every(rows => rows.some(row => row.relay === true))) throw new Error('forced_turn_not_verified');
  evidence.relayProof = proof;
  const start = performance.now(), deadline = start + config.durationSeconds * 1000;
  evidence.phase = 'load'; evidence.loadStartedAt = new Date().toISOString(); save();

  await Promise.all(pages.map(page => page.evaluate(() => window.bench.begin())));
  let running = true, sequence = 0;
  const offerNative = (slot, cls) => nativeSend({ id: `n:${slot}:${cls}:${sequence++}`, class: cls, peer: peerIds.get(slot) || 'not-connected' });
  const timer = setInterval(() => { for (let i = 0; i < config.peers; i++) void offerNative(`browser${i}`, 'control'); }, 100);
  const classesTimer = setInterval(() => { for (let i = 0; i < config.peers; i++) for (const cls of CLASSES.filter(c => !['control', 'bulk'].includes(c))) void offerNative(`browser${i}`, cls); }, 1000);
  const statusTimer = setInterval(() => { native.stdin.write('{"kind":"status"}\n'); }, 1000);
  const bulkTasks = Array.from({ length: config.peers }, (_, i) => (async () => {
    while (running) { const ok = await offerNative(`browser${i}`, 'bulk'); if (!ok) await sleep(100); }
  })());
  let nextDrop = start + 30000, drops = 0;
  while (performance.now() < deadline) {
    const wait = Math.min(deadline, nextDrop) - performance.now(); if (wait > 0) await sleep(wait);
    if (performance.now() < deadline && performance.now() >= nextDrop) {
      await Promise.all(pages.map(page => page.evaluate(() => window.bench.drop())));
      drops++; nextDrop += 30000;
    }
  }
  running = false; clearInterval(timer); clearInterval(classesTimer); clearInterval(statusTimer);
  evidence.phase = 'drain';

  await Promise.all(pages.map(page => page.evaluate(() => window.bench.stop())));
  evidence.offeredDurationSeconds = (performance.now() - start) / 1000; evidence.injectedDisconnectsPerBrowser = drops;
  await Promise.race([Promise.all(bulkTasks), sleep(35000)]); await sleep(10000);
  native.stdin.write('{"kind":"status"}\n');
  await sleep(100);
  evidence.browsers = await Promise.all(pages.map(page => page.evaluate(() => window.bench.snapshot())));
  const browserEvents = evidence.browsers.flatMap(b => b.events);
  const browserUnderLoad = evidence.browsers.flatMap(b => {
    const start = b.events.find(e => e.kind === 'measurement_start')?.at ?? Infinity;
    const stop = b.events.find(e => e.kind === 'measurement_stop')?.at ?? -Infinity;
    return b.events.filter(e => e.at >= start && e.at <= stop);
  });
  evidence.measurements = {
    browserToNative: reconcile(browserEvents, evidence.nativeEvents, config.durationSeconds),
    nativeToBrowser: reconcile(nativeTx, browserEvents, config.durationSeconds),
    semantics: { browserSend: 'queue admission, not SCTP completion (legacy API limit)', nativeSend: 'awaited existing transport send completion',
      noReplay: true, disconnect: 'close/recreate browser transport each 30s; no production peer/process touched',
      unsupportedClasses: ['unreliable_audio_video: legacy adapter has no exposed unreliable-channel contract'] },
    rttMs: quantiles(evidence.browsers.flatMap(b => b.samples.filter(s => s.connectionState === 'connected').map(s => s.rttMs))),
    relayAlwaysObserved: evidence.browsers.every(b => b.samples.length && b.samples.every(s => s.relay)),
    maxBrowserBufferedBytes: Math.max(...evidence.browsers.map(b => b.maxBufferedBytes)),
    maxNativeBufferedBytes: status.reduce((n, x) => Math.max(n, Number(x.lastBufferedAmount || 0)), 0),
    nativeBufferObservation: 'Published lastBufferedAmount only; not a physical native buffer maximum',
    controlPingUnderLoadMs: {
      browserToNative: quantiles(browserUnderLoad.filter(e => e.kind === 'ack' && e.class === 'control' && !e.error).map(e => e.latencyMs)),
      nativeToBrowser: quantiles(nativeTx.filter(e => e.kind === 'ack' && e.class === 'control' && !e.error && e.at <= deadline).map(e => e.latencyMs)),
    },
    bulkReceivedUnderLoadMBps: {
      browserToNative: evidence.nativeEvents.filter(e => e.kind === 'received' && e.class === 'bulk' && e.at >= start && e.at <= deadline).reduce((n, e) => n + e.bytes, 0) / 1e6 / config.durationSeconds,
      nativeToBrowser: browserUnderLoad.filter(e => e.kind === 'received' && e.class === 'bulk').reduce((n, e) => n + e.bytes, 0) / 1e6 / config.durationSeconds,
    },
    nativeTransportStatusLast: status.at(-1),
  };
  evidence.nativeTx = nativeTx; evidence.nativeExit = nativeExit;
  evidence.phase = 'completed'; evidence.measured = true;
  save();
  for (const page of pages) await page.evaluate(() => window.bench.close());
} catch (error) {
  evidence.browserDiagnostics = await Promise.all(contexts.flatMap(c => c.pages()).map(async page => {
    try { return await page.evaluate(() => window.bench?.snapshot() || null); }
    catch { return null; }
  }));
  evidence.error = String(error.message || error).replace(/https?:\/\/\S+/g, '[URL]').slice(-10000);
  process.exitCode = 1;
} finally {
  await cleanup();
  if (evidence.browserTmp) fs.rmSync(evidence.browserTmp, { recursive: true, force: true });
}
