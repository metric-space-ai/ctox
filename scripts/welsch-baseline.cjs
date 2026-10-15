#!/usr/bin/env node
'use strict';
// Measurement only. No test-document writes, protected profile reads, or HTTP data bridge.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');

function percentile(values, p) {
  const v = values.filter(Number.isFinite).sort((a, b) => a - b);
  return v.length ? v[Math.max(0, Math.ceil(p * v.length) - 1)] : null;
}

// Shared by browser instrumentation and synthetic protocol tests. Raw content stays in memory.
function frameObserver(clock) {
  const encoder = new TextEncoder();
  const requests = new Map();
  const transfers = new Map();
  const result = { receivedBytes: 0, sentBytes: 0, unattributedReceivedBytes: 0,
    collectionBytes: {}, messages: [], rpc: [], pushes: [], commandDocuments: [],
    attributionErrors: 0, rejectedTransfers: 0, droppedMetadata: 0 };
  const size = x => typeof x === 'string' ? encoder.encode(x).byteLength : Number(x?.byteLength ?? x?.size ?? 0);
  const keep = (list, row) => { if (list.length < 15000) list.push(row); else result.droppedMetadata++; };
  const collectionName = x => typeof x === 'string' && /^[a-zA-Z0-9_-]{1,140}$/.test(x) ? x : null;
  async function message(payload, bytes, direction, channel, at) {
    const key = channel + ':' + String(payload?.id || '');
    if (direction === 'sent' && payload?.method) {
      if (requests.size > 20000) requests.clear();
      requests.set(key, { at, method: String(payload.method).slice(0, 80), collection: collectionName(payload.collection) });
    }
    const request = requests.get(key);
    const stream = typeof payload?.id === 'string' && payload.id.startsWith('masterChangeStream$');
    const collection = collectionName(payload?.collection) ||
      (stream ? collectionName(payload.id.split(':')[1]) : null) || request?.collection || null;
    if (direction === 'received') {
      if (collection) result.collectionBytes[collection] = (result.collectionBytes[collection] || 0) + bytes;
      else result.unattributedReceivedBytes += bytes;
    }
    const documentBytes = Array.isArray(payload?.result?.documents)
      ? encoder.encode(JSON.stringify(payload.result.documents)).byteLength : 0;
    keep(result.messages, { at, direction, channel, collection, bytes, documentBytes, stream });
    if (direction === 'received' && request && ('result' in payload || 'error' in payload)) {
      keep(result.rpc, { at, startedAt: request.at, durationMs: at - request.at, method: request.method,
        collection, nativeTimeMs: Number(payload?.result?.nativeTimeMs) || null, failed: Boolean(payload.error) });
      requests.delete(key);
    }
    const documents = Array.isArray(payload?.result?.documents) ? payload.result.documents : [];
    // Only metadata, never IDs, field values, tokens, SDP or full documents in evidence.
    if (stream && direction === 'received') {
      const lwts = documents.map(d => Number(d?._meta?.lwt)).filter(x => Number.isFinite(x) && x > 0);
      keep(result.pushes, { at, collection, documents: documents.length,
        sourceLwtMin: lwts.length ? Math.min(...lwts) : null, sourceLwtMax: lwts.length ? Math.max(...lwts) : null });
    }
    if (collection === 'business_commands' && direction === 'received') {
      for (const doc of documents) {
        if (!String(doc?.id || '').startsWith('cmd_workjet_project_list_')) continue;
        keep(result.commandDocuments, { at, id: doc.id, status: doc.status,
          acceptedAtMs: Number(doc.accepted_at_ms) || null, completedAtMs: Number(doc.completed_at_ms) || null });
      }
    }
  }
  async function observe(raw, direction, channel = 'unknown') {
    const at = clock(), bytes = size(raw);
    let bytesOnFailure = bytes;
    result[direction === 'received' ? 'receivedBytes' : 'sentBytes'] += bytes;
    try {
      const payload = typeof raw === 'string' ? JSON.parse(raw) : null;
      if (!payload || typeof payload !== 'object') { if (direction === 'received') result.unattributedReceivedBytes += bytes; return; }
      if (!payload.ctoxFrame) return await message(payload, bytes, direction, channel, at);
      const key = direction + ':' + channel + ':' + payload.transferId;
      if (payload.kind === 'start') {
        if (transfers.size >= 8 || payload.totalBytes > 4 * 1024 * 1024 || payload.totalFrames > 10000) {
          result.rejectedTransfers++; if (direction === 'received') result.unattributedReceivedBytes += bytes; return;
        }
        const previous = transfers.get(key);
        if (previous && direction === 'received') result.unattributedReceivedBytes += previous.bytes;
        transfers.set(key, { total: payload.totalFrames, encoding: payload.encoding || '', chunks: new Map(), bytes, buffered: 0 });
        return;
      }
      const transfer = transfers.get(key);
      if (payload.kind !== 'chunk' || !transfer) { if (direction === 'received') result.unattributedReceivedBytes += bytes; return; }
      transfer.bytes += bytes;
      const data = String(payload.data || '');
      transfer.buffered += size(data) - size(transfer.chunks.get(payload.seq) || '');
      if (transfer.buffered > 4 * 1024 * 1024) {
        transfers.delete(key); result.rejectedTransfers++;
        if (direction === 'received') result.unattributedReceivedBytes += transfer.bytes;
        return;
      }
      transfer.chunks.set(payload.seq, data);
      if (transfer.chunks.size !== transfer.total) return;
      transfers.delete(key);
      bytesOnFailure = transfer.bytes;
      let text = Array.from({ length: transfer.total }, (_, i) => transfer.chunks.get(i) || '').join('');
      if (transfer.encoding) {
        if (transfer.encoding !== 'deflate-raw-base64') throw new Error('unsupported frame encoding');
        const compressed = Uint8Array.from(atob(text), c => c.charCodeAt(0));
        const reader = new Blob([compressed]).stream().pipeThrough(new DecompressionStream('deflate-raw')).getReader();
        const decoder = new TextDecoder(); let length = 0; text = '';
        for (;;) {
          const { done, value } = await reader.read(); if (done) break;
          length += value.byteLength;
          if (length > 8 * 1024 * 1024) { await reader.cancel(); throw new Error('inflated frame measurement cap'); }
          text += decoder.decode(value, { stream: true });
        }
        text += decoder.decode();
      }
      try { await message(JSON.parse(text), transfer.bytes, direction, channel, at); }
      catch { result.attributionErrors++; if (direction === 'received') result.unattributedReceivedBytes += transfer.bytes; }
    } catch { result.attributionErrors++; if (direction === 'received') result.unattributedReceivedBytes += bytesOnFailure; }
  }
  function snapshot() {
    const pendingBytes = [...transfers.entries()].filter(([k]) => k.startsWith('received:')).reduce((s, [, v]) => s + v.bytes, 0);
    return { ...result, pendingReceivedBytes: pendingBytes };
  }
  return { observe, snapshot };
}

function instrument(observerSource) {
  const observeFrames = (0, eval)('(' + observerSource + ')')(() => performance.now());
  const peers = [], attached = new WeakSet();
  const state = window.__welschBaseline = { observer: observeFrames, timelines: [], idb: [], sockets: [],
    commands: [], firstPaintVisibleMs: null, firstIdbValueMs: null, errors: [], peers };
  const now = () => performance.now();
  const safeError = e => String(e?.code || e?.name || 'unknown').slice(0, 100);
  const nativeSend = RTCDataChannel.prototype.send;
  RTCDataChannel.prototype.send = function (data) {
    const value = nativeSend.call(this, data); // Preserve success/throw behavior before observing.
    void observeFrames.observe(data, 'sent', this.label); return value;
  };
  const attach = channel => {
    if (attached.has(channel)) return; attached.add(channel);
    channel.addEventListener('message', e => { void observeFrames.observe(e.data, 'received', channel.label); });
  };
  const NativePeer = window.RTCPeerConnection;
  window.RTCPeerConnection = class extends NativePeer {
    constructor(...args) {
      super(...args); peers.push(this); this.addEventListener('datachannel', e => attach(e.channel));
      const create = this.createDataChannel;
      this.createDataChannel = (...a) => { const channel = create.apply(this, a); attach(channel); return channel; };
    }
  };
  const NativeSocket = window.WebSocket;
  window.WebSocket = class extends NativeSocket {
    constructor(...args) {
      super(...args); const url = new URL(String(args[0]), location.href);
      const row = { host: url.host, path: url.pathname, openedMs: null, errors: 0, closedMs: null };
      state.sockets.push(row);
      this.addEventListener('open', () => { row.openedMs = now(); });
      this.addEventListener('error', () => { row.errors++; });
      this.addEventListener('close', e => { row.closedMs = now(); row.closeCode = e.code; });
    }
  };
  for (const method of ['get', 'getAll', 'openCursor']) {
    const original = IDBObjectStore.prototype[method];
    IDBObjectStore.prototype[method] = function (...args) {
      const request = original.apply(this, args), start = now(), store = this.name;
      request.addEventListener('success', () => {
        const value = request.result;
        const count = Array.isArray(value) ? value.length : value ? 1 : 0;
        if (count && state.firstIdbValueMs === null) state.firstIdbValueMs = now();
        if (state.idb.length < 5000) state.idb.push({ store, method, at: now(), durationMs: now() - start, count });
      }, { once: true }); return request;
    };
  }
  window.addEventListener('ctox-business-command-lifecycle', e => {
    const d = e.detail;
    if (d?.command_type === 'ctox.workjet.project.list') state.commands.push({ at: now(), id: d.command_id, phase: d.phase });
  });
  window.addEventListener('error', e => state.errors.push(safeError(e.error)));
  let previous = '';
  state.timer = setInterval(() => {
    const app = window.CTOX_BUSINESS_OS_APP;
    const collections = Object.fromEntries(Object.entries(app?.syncDiagnostics?.collections || {}).map(([k, d]) =>
      [k, { queryReady: d.queryReady === true, errorCode: typeof d.lastError === 'string' ? d.lastError.slice(0, 100) : d.lastError?.code || null }]));
    const next = JSON.stringify(collections);
    if (next !== previous && state.timelines.length < 3000) { state.timelines.push({ at: now(), collections }); previous = next; }
    if (state.firstPaintVisibleMs === null && document.body && app?.initialModuleOpened && document.body.getBoundingClientRect().height > 0) {
      requestAnimationFrame(() => { if (state.firstPaintVisibleMs === null) state.firstPaintVisibleMs = now(); });
    }
  }, 100);
}

function phaseMetrics(data, expected = null) {
  const names = expected || [...new Set(data.timelines.flatMap(t => Object.keys(t.collections)))];
  const all = data.timelines.find(t => names.length && names.every(n => t.collections[n]?.queryReady));
  const first = data.timelines.find(t => Object.values(t.collections).some(x => x.queryReady));
  const cutoff = all?.at ?? data.durationMs;
  const perCollection = {}, perCollectionDocumentBytes = {};
  for (const m of data.frames.messages) {
    if (m.direction !== 'received' || !m.collection || m.at > cutoff) continue;
    perCollection[m.collection] = (perCollection[m.collection] || 0) + m.bytes;
    perCollectionDocumentBytes[m.collection] = (perCollectionDocumentBytes[m.collection] || 0) + (m.documentBytes || 0);
  }
  const attributedBytes = Object.values(perCollection).reduce((s, v) => s + v, 0);
  const documentBytes = Object.values(perCollectionDocumentBytes).reduce((s, v) => s + v, 0);
  return { expectedCollections: names, firstCollectionQueryReadyMs: first?.at ?? null,
    allCollectionsQueryReadyMs: all?.at ?? null, perCollectionReceivedBytesUntilReady: perCollection,
    attributedReceivedBytesUntilReady: attributedBytes,
    perCollectionDocumentJsonBytesUntilReady: perCollectionDocumentBytes,
    documentJsonBytesUntilReady: documentBytes,
    documentJsonMBpsUntilReady: all?.at > 0 ? documentBytes / 1e6 / (all.at / 1000) : null,
    attributedWireMBpsUntilReady: all?.at > 0 ? attributedBytes / 1e6 / (all.at / 1000) : null,
    firstIdbValueMs: data.firstIdbValueMs, visibleDesktopProxyMs: data.firstPaintVisibleMs,
    readyObserved: Boolean(all) };
}

async function collect(page, durationMs) {
  await page.waitForTimeout(durationMs);
  return page.evaluate(async () => {
    const s = window.__welschBaseline;
    const connections = [];
    for (const peer of s.peers) {
      const report = await peer.getStats();
      const pairs = [...report.values()].filter(x => x.type === 'candidate-pair' && x.state === 'succeeded');
      const transport = [...report.values()].find(x => x.type === 'transport' && x.selectedCandidatePairId);
      const selected = transport ? report.get(transport.selectedCandidatePairId) : pairs.find(x => x.nominated);
      if (selected) {
        const local = report.get(selected.localCandidateId), remote = report.get(selected.remoteCandidateId);
        connections.push({ connectionState: peer.connectionState, localCandidateType: local?.candidateType || null,
          remoteCandidateType: remote?.candidateType || null,
          relay: local && remote ? local.candidateType === 'relay' || remote.candidateType === 'relay' : null,
          rttMs: Number.isFinite(selected.currentRoundTripTime) ? selected.currentRoundTripTime * 1000 : null,
          bytesReceived: selected.bytesReceived ?? null, bytesSent: selected.bytesSent ?? null });
      } else connections.push({ connectionState: peer.connectionState, relay: null, rttMs: null });
    }
    return { timeOrigin: performance.timeOrigin, durationMs: performance.now(), timelines: s.timelines, idb: s.idb, firstIdbValueMs: s.firstIdbValueMs,
      firstPaintVisibleMs: s.firstPaintVisibleMs, frames: s.observer.snapshot(), connections, sockets: s.sockets,
      commands: s.commands, errors: s.errors,
      appBuild: [...document.scripts].map(x => x.src).filter(x => x.includes('/app.js')).map(x => new URL(x).searchParams.get('v')) };
  });
}

async function readCommand(page) {
  return page.evaluate(async () => {
    if (typeof window.workjetProjectControl !== 'function') return { skipped: 'Product read-command entry point unavailable' };
    const start = performance.now(), known = new Set(window.__welschBaseline.commands.map(x => x.id));
    let timing = null;
    const diagnostics = { onCommandTiming(value) {
      timing = { stagesMs: Object.fromEntries(Object.entries(value.stagesMs || {}).filter(([, v]) => Number.isFinite(v))),
        pendingStage: value.pendingStage || null, pendingMs: value.pendingMs ?? null };
    } };
    try {
      const value = await Promise.race([
        window.workjetProjectControl({ action: 'project.list' }, diagnostics),
        new Promise((_, reject) => setTimeout(() => reject(Object.assign(new Error(), { code: 'baseline_read_timeout' })), 35000)),
      ]);
      const events = window.__welschBaseline.commands.filter(x => !known.has(x.id));
      return { start, end: performance.now(), durationMs: performance.now() - start, events, timing,
        count: Number.isFinite(value?.count) ? value.count : Array.isArray(value?.projects) ? value.projects.length : null,
        completed: true, operation: 'project.list', domainWrite: false };
    } catch (e) {
      return { start, end: performance.now(), durationMs: performance.now() - start, completed: false,
        code: String(e?.code || e?.name || 'unknown').slice(0, 120), timing,
        events: window.__welschBaseline.commands.filter(x => !known.has(x.id)) };
    }
  });
}

function summarize(run) {
  const repetitions = run.repetitions;
  const distribution = values => ({ n: values.filter(Number.isFinite).length,
    p50: percentile(values, .5), p95: percentile(values, .95), p99: percentile(values, .99) });
  const metric = (phase, name) => distribution(repetitions.map(r => r[phase]?.metrics?.[name]));
  return {
    quantileMethod: 'nearest rank; 3 repetitions do not establish population p95/p99',
    initialFirstCollectionMs: metric('initial', 'firstCollectionQueryReadyMs'),
    initialAllCollectionsMs: metric('initial', 'allCollectionsQueryReadyMs'),
    initialDocumentJsonMBps: metric('initial', 'documentJsonMBpsUntilReady'),
    initialAttributedWireMBps: metric('initial', 'attributedWireMBpsUntilReady'),
    reloadAllCollectionsMs: metric('reload', 'allCollectionsQueryReadyMs'),
    reloadDesktopVisibleProxyMs: metric('reload', 'visibleDesktopProxyMs'),
    reloadFirstIdbValueMs: metric('reload', 'firstIdbValueMs'),
    commandApiMs: distribution(repetitions.map(r => r.readCommand?.completed ? r.readCommand.durationMs : null)),
    commandSubmitToTerminalMs: distribution(repetitions.map(r => r.readCommand?.timing?.stagesMs?.total)),
    commandSubmitToAcceptedMs: distribution(repetitions.map(r => {
      const events = r.readCommand?.events || [];
      const submitted = events.find(e => e.phase === 'dispatch_started');
      const accepted = events.find(e => e.id === submitted?.id && e.phase === 'accepted');
      return accepted && submitted ? accepted.at - submitted.at : null;
    })),
    commandReadFailures: repetitions.filter(r => !r.readCommand?.completed).map(r => ({ index: r.index, code: r.readCommand?.code || r.error?.code || 'not_measured' })),
    rttMs: distribution(repetitions.map(r => r.reload?.connections?.find(c => c.connectionState === 'connected' && Number.isFinite(c.rttMs))?.rttMs)),
    passivePush: repetitions.map(r => {
      const phase = r.afterCommand;
      const pushes = (phase?.frames?.pushes || []).filter(p => p.at >= r.readCommand.end);
      return { repetition: r.index, observationMs: phase ? phase.durationMs - r.readCommand.end : null,
        frames: pushes.length, documents: pushes.reduce((n, p) => n + p.documents, 0),
        collectionCounts: Object.fromEntries([...new Set(pushes.map(p => p.collection || 'unknown'))].map(c =>
          [c, pushes.filter(p => (p.collection || 'unknown') === c).length])),
        latencyMs: null, reason: 'No server send/commit clock ground truth; document LWT may describe catch-up/replay. No synthetic writes.' };
    }),
  };
}

async function selfTest() {
  assert.equal(percentile([3, 1, 2], .5), 2); assert.equal(percentile([1, 2, 3], .99), 3);
  assert.equal(percentile([], .95), null);
  let t = 0; const observer = frameObserver(() => ++t);
  await observer.observe(JSON.stringify({ id: 'r1', method: 'masterChangesSince', collection: 'probe' }), 'sent', 'rx');
  const text = JSON.stringify({ id: 'r1', result: { documents: [{ name: 'ä' }] } });
  const chunks = [text.slice(0, 12), text.slice(12)];
  const raw = [{ ctoxFrame: 'v1', kind: 'start', transferId: 'f', totalFrames: 2, totalBytes: text.length },
    ...chunks.map((data, seq) => ({ ctoxFrame: 'v1', kind: 'chunk', transferId: 'f', seq, data }))].map(JSON.stringify);
  for (const x of raw) await observer.observe(x, 'received', 'rx');
  assert.equal(observer.snapshot().collectionBytes.probe, raw.reduce((n, s) => n + new TextEncoder().encode(s).length, 0));
  assert.equal(observer.snapshot().rpc[0].method, 'masterChangesSince');
  await observer.observe('€', 'received');
  assert.equal(observer.snapshot().unattributedReceivedBytes, 3);
  const metrics = phaseMetrics({ durationMs: 500, timelines: [{ at: 10, collections: { a: { queryReady: true } } },
    { at: 50, collections: { a: { queryReady: true }, b: { queryReady: true } } }], frames: { messages: [] } });
  assert.equal(metrics.allCollectionsQueryReadyMs, 50); // Do not mistake a partial early set for all collections.
  assert.equal(phaseMetrics({ durationMs: 5, timelines: [], frames: { messages: [] } }).allCollectionsQueryReadyMs, null);
  assert.equal(summarize({ repetitions: [{ readCommand: { completed: false } }] }).commandApiMs.n, 0);
  console.log('baseline runner self-test: quantiles, UTF-8 bytes, chunk/request attribution, unknown bytes, dynamic scope, absent readiness passed');
}

async function main() {
  if (process.argv.includes('--self-test')) return selfTest();
  const arg = (name, fallback) => { const i = process.argv.indexOf(name); return i < 0 ? fallback : process.argv[i + 1]; };
  const playwright = require(arg('--playwright', 'playwright'));
  const url = arg('--url', 'http://127.0.0.1:18765/');
  if (!['127.0.0.1', 'localhost'].includes(new URL(url).hostname)) throw new Error('Runner requires an explicitly forwarded loopback URL');
  const out = path.resolve(arg('--out', './welsch-baseline-' + new Date().toISOString().replace(/[:.]/g, '-')));
  fs.mkdirSync(out, { recursive: true, mode: 0o700 });
  const initialMs = Number(arg('--initial-ms', '45000')), reloadMs = Number(arg('--reload-ms', '30000'));
  if (!(initialMs >= 1000 && initialMs <= 90000 && reloadMs >= 1000 && reloadMs <= 90000)) throw new Error('Phase duration outside 1–90s bound');
  const run = { schema: 'ctox.welsch.baseline.v1', startedAt: new Date().toISOString(), pid: process.pid,
    sourceHead: arg('--source-head', 'unknown'), repetitions: [], protocolObservationOnly: true,
    passivePushLimit: 'No authorized test collection. LWT arrival-age is not commit-to-browser latency; no product probe writes.',
    command: 'One authorized project.list read per fresh context; command audit documents persist, project data unchanged.',
    stopCondition: '3 contexts x bounded initial/reload/read; browser closes in finally; external timeout 900s' };
  const save = () => fs.writeFileSync(path.join(out, 'baseline.json'), JSON.stringify(run, null, 2));
  let browser, browserTmp;
  const originalTmp = process.env.TMPDIR;
  try {
    const tempRoot = arg('--browser-tmp-root', originalTmp ? path.dirname(originalTmp) : '');
    if (!tempRoot) throw new Error('Use lane TMPDIR or --browser-tmp-root for disposable browser data');
    browserTmp = fs.mkdtempSync(path.join(tempRoot, 'wb-'));
    fs.chmodSync(browserTmp, 0o700);
    if (browserTmp.length > 60) throw new Error('Browser temporary path must fit the Linux Unix-socket path limit');
    // Chrome adds com.google.Chrome.XXXXXX/SingletonSocket here (AF_UNIX sun_path <=108).
    // Only this test process changes TMPDIR; no HOME override or existing profile access.
    process.env.TMPDIR = browserTmp;
    run.browserTmp = browserTmp;
    browser = await playwright.chromium.launch({ headless: true,
      ...(arg('--executable', '') ? { executablePath: arg('--executable', '') } : {}) });
    run.browserVersion = browser.version();
    for (let i = 0; i < 3; i++) {
      const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
      const repetition = { index: i + 1, startedAt: new Date().toISOString() }; run.repetitions.push(repetition); save();
      try {
        await context.addInitScript({ content: '(' + instrument.toString() + ')(' + JSON.stringify(frameObserver.toString()) + ')' });
        const page = await context.newPage();
        await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
        repetition.initial = await collect(page, initialMs);
        repetition.initial.metrics = phaseMetrics(repetition.initial); save();
        repetition.readCommand = await readCommand(page);
        repetition.afterCommand = await collect(page, 15000); save();
        await page.reload({ waitUntil: 'domcontentloaded', timeout: 30000 });
        repetition.reload = await collect(page, reloadMs);
        repetition.reload.metrics = phaseMetrics(repetition.reload, repetition.initial.metrics.expectedCollections);
        repetition.completed = true;
      } catch (e) { repetition.error = { name: e.name, code: e.code || null }; }
      finally { await context.close(); repetition.finishedAt = new Date().toISOString(); save(); }
    }
    run.completed = run.repetitions.every(x => x.completed); run.summary = summarize(run);
    if (!run.completed) process.exitCode = 1;
  } catch (e) {
    // Launch/driver diagnostics contain paths and browser stderr, never page payloads.
    run.error = { name: e.name, code: e.code || null, message: String(e.message || '').slice(-12000)
      .replace(/https?:\/\/\S+/g, '[URL]').replace(/eyJ[A-Za-z0-9_.-]{30,}/g, '[REDACTED]') };
    throw e;
  } finally {
    if (browser) await browser.close();
    if (originalTmp === undefined) delete process.env.TMPDIR; else process.env.TMPDIR = originalTmp;
    if (browserTmp) fs.rmSync(browserTmp, { recursive: true, force: true });
    run.browserTmpRemoved = Boolean(browserTmp);
    run.finishedAt = new Date().toISOString(); run.terminal = true; save();
  }
  console.log(JSON.stringify({ output: out, terminal: run.terminal, completed: run.completed, repetitions: run.repetitions.length }));
}
if (require.main === module) main().catch(e => { console.error(e.name + ': ' + String(e.code || 'runner_failed')); process.exitCode = 1; });
module.exports = { percentile, frameObserver, phaseMetrics, summarize };
