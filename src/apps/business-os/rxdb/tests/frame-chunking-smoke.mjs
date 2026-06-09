// REGRESSION: byte-correct frame chunking (the chars-vs-bytes bug).
//
// Chunks used to be sliced by UTF-16 char count; umlaut/emoji-heavy documents
// then serialized past the 16 KiB SCTP-safe ceiling and the browser killed
// the DataChannel mid-transfer ("mysterious" sync stalls for non-ASCII data).
// Chunking now budgets the JSON-ESCAPED byte length per chunk, mirroring the
// Rust splitter (split_chunks_for_frame in connection_handler_rs.rs).

import { webrtcNativeTestInternals } from '../src/webrtc-native.mjs';

const { splitFrameChunks, jsonEscapedCharLen, MAX_SERIALIZED_FRAME_BYTES } = webrtcNativeTestInternals;
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

// 4. Escaped-length parity spot checks against JSON.stringify ground truth.
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
