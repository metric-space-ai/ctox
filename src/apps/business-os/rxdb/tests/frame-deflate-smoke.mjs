// Framed transfers announced with `encoding: deflate-raw-base64` (sent by the
// native peer once the browser advertises ctox-rxdb-frame-deflate-v1) must be
// reassembled, inflated and parsed into the same frame the plain path yields;
// unknown encodings must fail loudly instead of being parsed as JSON.
import { deflateRawSync } from 'node:zlib';
import {
  CTOX_FRAME_DEFLATE_CAPABILITY,
  CtoxWebRtcNativePeer,
  frameDeflateSupported,
} from '../src/webrtc-native.mjs';
import { CTOX_FRAME_PROTOCOL } from '../src/frame-contract.generated.mjs';

function newPeer(room) {
  const peer = new CtoxWebRtcNativePeer({ signalingUrl: 'ws://localhost:0/ignored', room });
  peer.sent = [];
  peer.send = (peerId, payload) => { peer.sent.push({ peerId, payload }); };
  peer.delivered = [];
  peer.handleDataChannelFrame = async (_peerId, payload) => { peer.delivered.push(payload); };
  peer.errors = [];
  peer.events.on('error', (event) => peer.errors.push(event?.detail ?? event));
  return peer;
}

async function deliver(peer, transferId, text, encoding) {
  const chunks = [];
  for (let offset = 0; offset < text.length; offset += 8000) chunks.push(text.slice(offset, offset + 8000));
  await peer.handleTransportFrame('ctox-core', {
    ctoxFrame: CTOX_FRAME_PROTOCOL,
    kind: 'start',
    transferId,
    attempt: 0,
    totalFrames: chunks.length,
    totalBytes: Buffer.byteLength(text),
    ...(encoding ? { encoding } : {}),
  });
  for (const [seq, data] of chunks.entries()) {
    await peer.handleTransportFrame('ctox-core', {
      ctoxFrame: CTOX_FRAME_PROTOCOL,
      kind: 'chunk',
      transferId,
      attempt: 0,
      seq,
      data,
    });
  }
}

assert(CTOX_FRAME_DEFLATE_CAPABILITY === 'ctox-rxdb-frame-deflate-v1', 'capability name matches the native constant');
assert(frameDeflateSupported(), 'node exposes DecompressionStream, so the capability is advertised');

const documents = Array.from({ length: 600 }, (_, index) => ({
  id: `lead_${index}`,
  name: `Firma ${index} GmbH – Düsseldorf`,
  evidence: 'https://www.northdata.de/registry evidence '.repeat(6),
}));
const frame = { id: 'pull-1', result: { documents, checkpoint: { id: 'lead_599', lwt: 1791223528816 } } };
const plain = JSON.stringify(frame);

{
  const peer = newPeer('deflate-ok');
  const encoded = deflateRawSync(Buffer.from(plain)).toString('base64');
  assert(encoded.length * 3 < plain.length, `payload compresses at least 3x (${plain.length} -> ${encoded.length})`);
  await deliver(peer, 'ctox-core|frame|1', encoded, 'deflate-raw-base64');
  assert(peer.errors.length === 0, `no transport errors (${JSON.stringify(peer.errors)})`);
  assert(peer.delivered.length === 1, 'exactly one frame delivered');
  assert(JSON.stringify(peer.delivered[0]) === plain, 'inflated frame equals the original payload');
  assert(peer.sent.some(({ payload }) => payload.kind === 'ack' && payload.final), 'final ack sent');
}

{
  const peer = newPeer('deflate-plain');
  await deliver(peer, 'ctox-core|frame|2', plain, null);
  assert(peer.errors.length === 0 && peer.delivered.length === 1, 'plain frames still decode unchanged');
}

{
  const peer = newPeer('deflate-unknown');
  await deliver(peer, 'ctox-core|frame|3', Buffer.from(plain).toString('base64'), 'zstd-base64');
  assert(peer.delivered.length === 0, 'unknown encodings are not delivered');
  assert(peer.errors.some((error) => error.code === 'ctox_webrtc_frame_decode_failed'), `unknown encoding surfaces a decode error (${JSON.stringify(peer.errors)})`);
}

console.log('frame-deflate-smoke: ok');

function assert(c, m) { if (!c) throw new Error(m); }
