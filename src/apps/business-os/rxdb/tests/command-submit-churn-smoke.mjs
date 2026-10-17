// REGRESSION (field incident 17.10.2026): research commands were never
// written while the browser replaced its WebRTC connection every 2-3 minutes.
//   (3) a framed business_commands / ctox_queue_tasks masterWrite rode the
//       low lane behind multi-hundred-KB lead batches;
//   (4) lead push batches were up to 2 MiB per framed transfer;
//   (5) request deadlines started at enqueue, so time in this browser's own
//       send queue counted as "native did not answer" and a ctoxProtocol /
//       token timeout recycled the whole connection.
import assert from 'node:assert/strict';

import { CtoxWebRtcNativePeer, webrtcNativeTestInternals } from '../src/webrtc-native.mjs';
import { replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';

const { classifySendPriority, MAX_SERIALIZED_FRAME_BYTES } = webrtcNativeTestInternals;
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// (3) command-plane framed writes keep the normal lane regardless of size.
{
  const bulky = 'x'.repeat(80 * 1024);
  const frame = (collection) => ({ id: 'w1', method: 'masterWrite', collection, params: [[{ bulky }]] });
  const text = (payload) => JSON.stringify(payload);
  for (const collection of ['business_commands', 'ctox_queue_tasks']) {
    assert.equal(classifySendPriority(frame(collection), text(frame(collection))), 'normal',
      `framed ${collection} masterWrite must not ride the low lane`);
  }
  assert.equal(
    classifySendPriority(frame('outbound_lead_generation_leads'), text(frame('outbound_lead_generation_leads'))),
    'low', 'bulk record writes stay on the low lane');
  const small = { id: 'w2', method: 'masterWrite', collection: 'business_commands', params: [[{ a: 1 }]] };
  assert.equal(classifySendPriority(small, text(small)), 'high', 'inline command writes stay high');
  assert.ok(MAX_SERIALIZED_FRAME_BYTES > 0);
}

// (4) lead push batches are bounded at 512 KiB per framed transfer.
{
  const { boundedDirectPushBatches, DIRECT_PUSH_BATCH_MAX_BYTES } = replicationWebRtcTestInternals;
  assert.equal(DIRECT_PUSH_BATCH_MAX_BYTES, 512 * 1024, 'direct push batch limit must be 512 KiB');
  const docs = Array.from({ length: 8 }, (_, index) => ({ id: `lead_${index}`, blob: 'y'.repeat(150 * 1024) }));
  const batches = boundedDirectPushBatches(docs);
  assert.ok(batches.length >= 3, `8 x 150 KiB must split into >= 3 batches, got ${batches.length}`);
  for (const batch of batches) {
    const bytes = new TextEncoder().encode(batch.map((doc) => JSON.stringify(doc)).join('')).byteLength;
    assert.ok(batch.length === 1 || bytes <= DIRECT_PUSH_BATCH_MAX_BYTES, `batch of ${bytes} bytes exceeds the limit`);
  }
  assert.equal(batches.flat().length, docs.length, 'no document is dropped');
}

// (5) request deadlines start when the frame leaves the browser.
function fakePeer() {
  const connection = { remotePeerId: 'native-1', channel: { readyState: 'open' } };
  const peer = {
    options: { clientId: 'browser-1' },
    requestCounter: 0,
    pending: new Map(),
    connections: new Map([['native-1', connection]]),
    forceInitiatorPeers: new Set(),
    removed: [],
    events: [],
    sent: [],
    recordConnectionEvent(_connection, type, detail) { this.events.push({ type, ...detail }); },
    removeConnection(peerId, reason) { this.removed.push({ peerId, reason }); },
    scheduleReconnect() {},
    send(_peerId, frame, hooks) { this.sent.push({ frame, hooks }); return true; },
  };
  peer.request = CtoxWebRtcNativePeer.prototype.request.bind(peer);
  return peer;
}
function answer(peer, id, result) {
  const pending = peer.pending.get(id);
  assert.ok(pending, 'request must still be pending when its answer arrives');
  peer.pending.delete(id);
  clearTimeout(pending.timer);
  pending.resolve(result);
}

// 5a. Queued 60 ms (> the 50 ms budget measured from enqueue), answered 30 ms
// after the real send: must resolve. The enqueue-time deadline rejected it.
{
  const peer = fakePeer();
  const result = peer.request('native-1', 'ctoxProtocol', [{}], 50);
  const { frame, hooks } = peer.sent[0];
  assert.equal(typeof hooks?.onSent, 'function', 'request must hand send-time hooks to the send queue');
  await delay(40);
  hooks.onSent();
  await delay(30);
  answer(peer, frame.id, { ok: true });
  assert.deepEqual(await result, { ok: true }, 'deadline must start at send, not at enqueue');
  assert.deepEqual(peer.removed, [], 'no connection recycle for local queue time');
}

// 5b. Never left the local queue: rejects with the typed, retryable queue
// timeout and never recycles the connection.
{
  const peer = fakePeer();
  const result = peer.request('native-1', 'ctoxProtocol', [{}], 30);
  await assert.rejects(result, (error) => error.code === 'ctox_webrtc_request_send_queue_timeout'
    && error.retryable === true
    && /Timed out waiting for WebRTC response ctoxProtocol/.test(error.message));
  assert.deepEqual(peer.removed, [], 'local send-queue time must never replace the connection');
  assert.ok(peer.events.some((event) => event.type === 'request-send-queue-timeout'));
  assert.equal(peer.sent[0].hooks.cancelled(), true, 'the abandoned item is dropped from the queue');
}

// 5c. A genuine no-response after the send still recycles (unchanged policy).
{
  const peer = fakePeer();
  const result = peer.request('native-1', 'ctoxProtocol', [{}], 30);
  peer.sent[0].hooks.onSent();
  await assert.rejects(result, /Timed out waiting for WebRTC response ctoxProtocol/);
  assert.equal(peer.removed[0]?.reason, 'request-timeout-ctoxProtocol');
}

// 5d. The send worker reports the real hand-off and skips cancelled items.
{
  const sentTexts = [];
  const connection = {
    remotePeerId: 'native-1',
    channel: { readyState: 'open', send(text) { sentTexts.push(text); } },
    sendQueue: { high: [], normal: [], low: [], draining: false, nextSequence: 0, queuedBytes: 0, scheduleCursor: 0 },
  };
  const order = [];
  const peer = {
    closed: false,
    connections: new Map([['native-1', connection]]),
    transportStats: { sentScheduledFrames: 0 },
    recordTransportStatus(update) { Object.assign(this.transportStats, update); },
    refreshSendQueueStatus() {},
    recordSentInlineFrame() {},
    async waitForSendBuffer() {},
    async sendFramed(_connection, text) { order.push(`framed:${text}`); },
    removeConnection() {},
    events: { emit() {} },
  };
  connection.sendQueue.high.push({ text: 'cancelled', inline: true, priority: 'high', hooks: { cancelled: () => true, onSent: () => order.push('sent:cancelled') } });
  connection.sendQueue.high.push({ text: 'inline', inline: true, priority: 'high', hooks: { cancelled: () => false, onSent: () => order.push('sent:inline') } });
  connection.sendQueue.normal.push({ text: 'big', inline: false, priority: 'normal', hooks: { cancelled: () => false, onSent: () => order.push('sent:big') } });
  await CtoxWebRtcNativePeer.prototype.drainSendQueue.call(peer, connection);
  assert.deepEqual(sentTexts, ['inline'], 'a cancelled request frame is never put on the wire');
  assert.deepEqual(order, ['sent:inline', 'framed:big', 'sent:big'], 'onSent fires after the real hand-off');
}

console.log('ctox-rxdb command submit churn smoke OK');
process.exit(0);
