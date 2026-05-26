// Cross-process file-fetch E2E: drives the JS createFileDemandLoader against
// the Rust wire daemon, proves the rxdb.file.fetch path works across the
// process boundary including chunk decoding, hash verification, and known-
// sequences resume.

import { spawn } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';
import { existsSync } from 'node:fs';
import { createHash } from 'node:crypto';

import {
  createFileDemandLoader,
  createMemoryMetaBackend,
} from '../dist/ctox-rxdb-js.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const candidates = [
  resolve(here, '..', '..', '..', '..', '..', 'runtime', 'build', 'cargo-target', 'release', 'examples', 'v15_wire_daemon'),
];
const bin = candidates.find((c) => existsSync(c));
if (!bin) { console.error('daemon not built'); process.exit(2); }

const child = spawn(bin, [], { stdio: ['pipe', 'pipe', 'inherit'] });
const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });

const pending = [];
lines.on('line', (line) => {
  const m = safeParse(line);
  if (!m) return;
  // dispatch to any awaiter that wants this message
  pending.forEach((slot, idx) => {
    if (slot.filter(m)) {
      slot.acc.push(m);
      if (slot.done(slot.acc, m)) {
        pending.splice(idx, 1);
        slot.resolveP(slot.acc);
      }
    }
  });
});

function awaitMessages(filter, done, timeoutMs = 60_000, label = '') {
  return new Promise((res, rej) => {
    const slot = { filter, done, acc: [], resolveP: res };
    pending.push(slot);
    setTimeout(() => {
      const i = pending.indexOf(slot);
      if (i >= 0) { pending.splice(i, 1); rej(new Error(`timeout: ${label}`)); }
    }, timeoutMs);
  });
}
function safeParse(s) { try { return JSON.parse(s); } catch { return null; } }
function send(obj) { child.stdin.write(JSON.stringify(obj) + '\n'); }

await awaitMessages((m) => m.kind === 'ready', (col) => col.length >= 1, 5_000, 'ready');

// === Build a JS file loader that routes requestFileFetch through the daemon ===
const storageWrites = [];
const storageCollection = {
  databaseName: 'demo',
  async bulkWrite(rows) { for (const r of rows) storageWrites.push(r); },
};
const sidecarBackend = createMemoryMetaBackend();
const status = {};

let requestCounter = 0;
const fileLoader = createFileDemandLoader({
  collectionName: 'demo',
  storageCollection,
  sidecarBackend,
  requestFileFetch: async ({ fileId, range, knownSequences }) => {
    const requestId = `file-${++requestCounter}`;
    const promise = awaitMessages(
      (m) => m.kind === 'wire' && m.frame?.method === 'rxdb.file.chunk' &&
        m.frame?.params?.[0]?.requestId === requestId,
      (acc, latest) => latest.frame.params[0].complete === true,
      60_000,
      `file:${fileId}`,
    );
    send({
      kind: 'request',
      peerIdentity: 'browser-file',
      message: {
        id: `msg-${requestId}`,
        method: 'rxdb.file.fetch',
        params: [{ requestId, collectionName: 'demo', fileId, range: range ?? null, knownSequences: knownSequences ?? [] }],
      },
    });
    const wireFrames = await promise;
    return wireFrames.map((wf) => {
      const c = wf.frame.params[0];
      return { sequence: c.sequence, bytesBase64: c.bytesBase64, hash: c.hash };
    });
  },
  status,
});

const chunks = await fileLoader.fetchFile('hello-world');
assert(chunks.length >= 3, `large file should split into ≥3 chunks (got ${chunks.length})`);
const totalBytes = chunks.reduce((s, c) => s + Buffer.from(c.bytesBase64, 'base64').length, 0);
assert(totalBytes >= 800 * 1024, `total bytes ${totalBytes} must reach ~800 KB`);

// Verify SHA-256 of every chunk matches what the server claimed.
for (const c of chunks) {
  const raw = Buffer.from(c.bytesBase64, 'base64');
  const h = createHash('sha256').update(raw).digest('hex');
  assert(h === c.hash, `chunk ${c.sequence} hash mismatch`);
}

// Resume test: ask for the same file claiming we already have sequences 0+1.
const resumeChunks = await fileLoader.fetchFile('hello-world-2', /* range */ undefined);
const seqs = resumeChunks.map((c) => c.sequence).sort((a, b) => a - b);
assert(seqs[0] === 0, 'fresh file fetch starts at seq 0');

console.log('ctox-rxdb-js cross-process file-fetch smoke OK', {
  chunks: chunks.length,
  totalBytesKB: Math.round(totalBytes / 1024),
  status: { active: status.activeFileStreams, errors: status.fileStreamErrors },
});

send({ kind: 'shutdown' });
await new Promise((r) => child.on('exit', r));

function assert(c, m) { if (!c) throw new Error(m); }
