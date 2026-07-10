// Cross-process V1.5 E2E: spawn the Rust wire daemon, drive `rxdb.query.fetch`
// through it via stdio, decode the resulting chunks in JS, verify every doc
// arrives intact across the process boundary.
//
// This is the proof that the JS demand-loader and the Rust dispatcher
// actually agree on the wire-bytes — no in-process mocks, no shared memory.

import { spawn } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';

import { decodeChunk } from '../dist/ctox-rxdb-js.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const daemonBin = resolve(here, '..', '..', '..', '..', 'core', 'rxdb', 'runtime', 'build', 'cargo-target', 'release', 'examples', 'v15_wire_daemon');
// Cargo target is under repo runtime/; fall back to runtime/build/cargo-target.
const fallbackBin = resolve(here, '..', '..', '..', '..', '..', 'runtime', 'build', 'cargo-target', 'release', 'examples', 'v15_wire_daemon');
const debugFallbackBin = resolve(here, '..', '..', '..', '..', '..', 'runtime', 'build', 'cargo-target', 'debug', 'examples', 'v15_wire_daemon');
import { existsSync } from 'node:fs';
const bin = [daemonBin, fallbackBin, debugFallbackBin].find((candidate) => existsSync(candidate));
if (!bin) {
  console.error('daemon binary not found at', daemonBin, fallbackBin, 'or', debugFallbackBin);
  process.exit(2);
}

const child = spawn(bin, [], { stdio: ['pipe', 'pipe', 'inherit'] });
const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });

// Collect inbound frames per pending request.
const pending = new Map();
function awaitFrames(filter, timeoutMs = 30_000) {
  return new Promise((resolveP, rejectP) => {
    const collected = [];
    const id = Symbol();
    pending.set(id, { filter, resolveP, rejectP, collected });
    setTimeout(() => {
      if (pending.has(id)) {
        pending.delete(id);
        rejectP(new Error(`timeout waiting for ${filter.label}`));
      }
    }, timeoutMs);
  });
}

lines.on('line', (line) => {
  const msg = safeParse(line);
  if (!msg) return;
  for (const [id, slot] of pending) {
    if (slot.filter.match(msg)) {
      slot.collected.push(msg);
      if (slot.filter.done(slot.collected, msg)) {
        pending.delete(id);
        slot.resolveP(slot.collected);
      }
    }
  }
});

function safeParse(s) { try { return JSON.parse(s); } catch { return null; } }
function send(obj) { child.stdin.write(JSON.stringify(obj) + '\n'); }

// 1. Wait for ready.
await awaitFrames({
  label: 'ready',
  match: (m) => m.kind === 'ready',
  done: (col) => col.length >= 1,
});

// 2. Seed the daemon's collection with 5000 synthetic docs.
const docs = Array.from({ length: 5000 }, (_, i) => ({
  id: `cp-${i.toString().padStart(5, '0')}`,
  payload: `Synthetic record ${i} for cross-process wire test`,
  _rev: '1-cp',
  _deleted: false,
  _meta: { lwt: i + 1 },
  _attachments: {},
}));
send({ kind: 'seed', collection: 'demo', docs });
await awaitFrames({
  label: 'seeded',
  match: (m) => m.kind === 'seeded',
  done: (col) => col.length >= 1,
});

// 3. Issue a real query-fetch request.
const requestId = 'cross-r1';
const seedStart = Date.now();
send({
  kind: 'request',
  peerIdentity: 'browser-cp-1',
  message: {
    id: 'msg-cp-1',
    method: 'rxdb.query.fetch',
    params: [{
      requestId,
      collectionName: 'demo',
      schemaVersion: 0,
      queryFingerprint: 'cp-fp',
      query: { selector: {}, sort: [], limit: null, skip: 0 },
      window: { offset: 0, limit: 5000 },
    }],
  },
});

// 4. Collect frames until the terminal chunk arrives.
const wireFrames = await awaitFrames({
  label: 'chunks',
  match: (m) => m.kind === 'wire' && m.frame?.method === 'rxdb.query.chunk',
  done: (col, latest) => {
    const params = latest.frame?.params?.[0];
    return params?.complete === true && params?.requestId === requestId;
  },
}, 60_000);
const dispatchMs = Date.now() - seedStart;

// 5. Decode every chunk and verify total docs.
let totalDocs = 0;
let compressedChunks = 0;
let totalWireBytes = 0;
const reassembled = [];
for (const wf of wireFrames) {
  const chunk = wf.frame.params[0];
  totalWireBytes += JSON.stringify(chunk).length;
  if (chunk.compressed) compressedChunks += 1;
  const docsFromChunk = await decodeChunk(chunk);
  totalDocs += docsFromChunk.length;
  for (const d of docsFromChunk) reassembled.push(d);
}

assert(totalDocs === 5000, `expected 5000 docs across the wire, got ${totalDocs}`);
// Order: docs were inserted as cp-00000..cp-04999; daemon streams in storage
// order (effectively ID order after the matcher).
const ids = reassembled.map((d) => d.id).sort();
assert(ids[0] === 'cp-00000' && ids[4999] === 'cp-04999', 'first/last IDs match');
const allUnique = new Set(ids).size === ids.length;
assert(allUnique, 'no duplicate IDs');

console.log('ctox-rxdb-js cross-process wire smoke OK', {
  docs: totalDocs,
  chunks: wireFrames.length,
  compressedChunks,
  wireBytesKB: Math.round(totalWireBytes / 1024),
  dispatchMs,
  throughputDocsPerSec: Math.round(totalDocs / (dispatchMs / 1000)),
});

// 6. Shut the daemon down cleanly. End stdin so the daemon's `for line in
// stdin` loop terminates, then wait for exit with a short fallback kill.
send({ kind: 'shutdown' });
child.stdin.end();
lines.close();
await new Promise((resolveExit) => {
  const killer = setTimeout(() => {
    try { child.kill('SIGTERM'); } catch {}
  }, 2000);
  child.once('exit', () => { clearTimeout(killer); resolveExit(); });
});
process.exit(0);

function assert(c, m) { if (!c) throw new Error(m); }
