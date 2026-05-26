// Verifies the JS decoder can read both inline and deflate-compressed chunks
// produced by the Rust dispatcher. We don't shell out to Rust here — we
// produce known-good envelopes ourselves and prove the format round-trips.
//
// The "known-good" inputs are also fed back through the Node deflate path
// to ensure the JS decoder matches what flate2 produces. The proper
// cross-process test lives in v15_scale_wire_loop (Rust) + the cross-process
// E2E (next task).

import { decodeChunk } from '../dist/ctox-rxdb-js.mjs';
import { deflateRawSync } from 'node:zlib';

// === Inline chunk ===
const inlineChunk = {
  requestId: 'r1',
  sequence: 0,
  documents: [{ id: 'a' }, { id: 'b' }],
  complete: true,
};
const inlineDocs = await decodeChunk(inlineChunk);
assert(inlineDocs.length === 2, `inline chunk produced ${inlineDocs.length} docs`);
assert(inlineDocs[0].id === 'a');

// === Compressed chunk ===
const docs = Array.from({ length: 50 }, (_, i) => ({
  id: `rec-${i}`,
  subject: 'repeated-text-helps-compression repeated-text-helps-compression',
  n: i,
}));
const payload = Buffer.from(JSON.stringify(docs), 'utf8');
const compressed = deflateRawSync(payload);
const compressedChunk = {
  requestId: 'r2',
  sequence: 0,
  complete: true,
  compressed: 'deflate',
  compressedBase64: compressed.toString('base64'),
};
const decoded = await decodeChunk(compressedChunk);
assert(decoded.length === 50, `compressed chunk produced ${decoded.length} docs`);
assert(decoded[0].id === 'rec-0');
assert(decoded[49].id === 'rec-49');
const compressedSize = compressedChunk.compressedBase64.length;
const inlineSize = JSON.stringify(docs).length;
assert(compressedSize < inlineSize, `compressed (${compressedSize}) must be smaller than inline (${inlineSize})`);

// === Unknown compression rejected ===
let caught = null;
try {
  await decodeChunk({ requestId: 'r3', sequence: 0, complete: true, compressed: 'lzma', compressedBase64: 'AAAA' });
} catch (e) {
  caught = e;
}
assert(caught && /unsupported chunk compression/.test(caught.message), 'unknown compression must throw');

console.log('ctox-rxdb-js compression roundtrip smoke OK', {
  inlineDocs: inlineDocs.length,
  compressedDocs: decoded.length,
  compressionRatio: (compressedSize / inlineSize).toFixed(2),
});

function assert(c, m) { if (!c) throw new Error(m); }
