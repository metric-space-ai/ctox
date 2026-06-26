// REGRESSION: byte-correct frame chunking (the chars-vs-bytes bug).
//
// Chunks used to be sliced by UTF-16 char count; umlaut/emoji-heavy documents
// then serialized past the 16 KiB SCTP-safe ceiling and the browser killed
// the DataChannel mid-transfer ("mysterious" sync stalls for non-ASCII data).
// Chunking now budgets the JSON-ESCAPED byte length per chunk, mirroring the
// Rust splitter (split_chunks_for_frame in connection_handler_rs.rs).

import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { webrtcNativeTestInternals } from '../src/webrtc-native.mjs';

const testDir = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(testDir, '../src/webrtc-native.mjs'), 'utf8');
const {
  splitFrameChunks,
  jsonEscapedCharLen,
  encodedSize,
  recordReceivedFrame,
  MAX_SERIALIZED_FRAME_BYTES,
} = webrtcNativeTestInternals;
const transferId = 'client-with-a-rather-long-id|frame|1760000000000|42';

const CASES = [
  ['ascii', 'a'.repeat(60_000)],
  ['umlauts', 'äöüßÄÖÜéèê'.repeat(8_000)],
  ['emoji (surrogate pairs)', '👩‍💻🚀😀'.repeat(6_000)],
  ['mixed + escapes', 'Zeile "eins"\n\täh… 🤖 \\backslash\\ '.repeat(3_000)],
  ['control chars', 'data'.repeat(5_000)],
  ['empty', ''],
  ['single char', 'ß'],
];

for (const [label, text] of CASES) {
  const chunks = splitFrameChunks(text, transferId);

  // 1. Lossless reassembly (the Rust receiver concatenates by seq).
  assert(chunks.join('') === text, `${label}: chunks reassemble to the original text`);

  // 2. Every serialized chunk frame stays under the SCTP-safe ceiling.
  chunks.forEach((data, seq) => {
    const frame = JSON.stringify({
      ctoxFrame: 'ctox-rxdb-frame-v1',
      kind: 'chunk',
      transferId,
      attempt: 0,
      seq,
      data,
    });
    const bytes = new TextEncoder().encode(frame).byteLength;
    assert(
      bytes <= MAX_SERIALIZED_FRAME_BYTES,
      `${label}: serialized chunk ${seq} is ${bytes} bytes (ceiling ${MAX_SERIALIZED_FRAME_BYTES})`,
    );
    // Wire contract: per-chunk payload budget is 10240 (JSON-escaped bytes).
    const escapedBytes = new TextEncoder().encode(JSON.stringify(data)).byteLength - 2;
    assert(
      escapedBytes <= 10240,
      `${label}: chunk ${seq} payload is ${escapedBytes} escaped bytes (contract budget 10240)`,
    );
  });

  // 3. No chunk ever splits a surrogate pair (every chunk is well-formed).
  for (const chunk of chunks) {
    if (!chunk) continue;
    const first = chunk.charCodeAt(0);
    const last = chunk.charCodeAt(chunk.length - 1);
    assert(!(first >= 0xdc00 && first <= 0xdfff), `${label}: chunk starts with a low surrogate`);
    assert(!(last >= 0xd800 && last <= 0xdbff), `${label}: chunk ends with a high surrogate`);
  }
}

// 4. Raw UTF-8 byte length parity without allocating a TextEncoder buffer in
// the production hot path.
for (const text of [
  '',
  'plain ascii',
  'äöüßÄÖÜéèê',
  '€',
  '👩‍💻🚀😀',
  'mixed "quotes"\n\tcontrol \u0001 and \\backslash\\',
  '\ud800',
  '\udc00',
]) {
  const expected = new TextEncoder().encode(text).byteLength;
  assert(
    encodedSize(text) === expected,
    `encodedSize(${JSON.stringify(text)}): got ${encodedSize(text)}, TextEncoder says ${expected}`,
  );
}

{
  const OriginalTextEncoder = globalThis.TextEncoder;
  globalThis.TextEncoder = class {
    constructor() {
      throw new Error('encodedSize hot path must not allocate TextEncoder buffers');
    }
  };
  try {
    assert(encodedSize('ä👩‍💻') === 13, 'encodedSize works without TextEncoder');
  } finally {
    globalThis.TextEncoder = OriginalTextEncoder;
  }
}

const encodedSizeSource = source.match(/function encodedSize[\s\S]*?\n}\n/)?.[0] || '';
assert(
  !encodedSizeSource.includes('TextEncoder'),
  'encodedSize implementation must stay allocation-free',
);

// 5. Reassembly ACK bookkeeping must advance incrementally instead of scanning
// from frame 0 on every received chunk.
{
  const entry = {
    totalFrames: 8,
    received: new Map(),
    contiguousSeq: -1,
  };
  assert(recordReceivedFrame(entry, 3, 'd') === -1, 'out-of-order frame leaves contiguous sequence unchanged');
  assert(recordReceivedFrame(entry, 1, 'b') === -1, 'gap at frame 0 keeps contiguous sequence at -1');
  assert(recordReceivedFrame(entry, 0, 'a') === 1, 'receiving frame 0 advances over already received frame 1');
  assert(recordReceivedFrame(entry, 2, 'c') === 3, 'receiving the gap advances over already received frame 3');
  assert(recordReceivedFrame(entry, 3, 'd2') === 3, 'duplicate frame does not rescan or advance');
}

assert(
  !source.includes('highestContiguousSeq'),
  'frame reassembly must not recompute contiguous ACK state by rescanning received chunks',
);

// 6. Escaped-length parity spot checks against JSON.stringify ground truth.
for (const ch of ['a', '"', '\\', '\n', '', 'ä', '€', '👩', '\ud800']) {
  const expected = new TextEncoder().encode(JSON.stringify(ch)).byteLength - 2; // minus quotes
  const got = [...ch].reduce((sum, c) => sum + jsonEscapedCharLen(c), 0);
  assert(
    got === expected,
    `jsonEscapedCharLen(${JSON.stringify(ch)}): got ${got}, JSON.stringify says ${expected}`,
  );
}

console.log('ctox-rxdb frame chunking smoke OK', { cases: CASES.length });

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
