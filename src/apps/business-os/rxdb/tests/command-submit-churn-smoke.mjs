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
    incomingFrames: new Map(),
    incomingTransferProgressing: CtoxWebRtcNativePeer.prototype.incomingTransferProgressing,
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

// (7) Lead initial replication must make progress instead of restarting.
// 7a. A framed answer that keeps streaming in past the request budget is
// awaited, not discarded and re-requested from zero.
{
  const peer = fakePeer();
  const result = peer.request('native-1', 'masterChangesSince', [null, 10], 40);
  const { frame, hooks } = peer.sent[0];
  hooks.onSent();
  await delay(10);
  const transfer = { peerId: 'native-1', createdAt: Date.now(), lastProgressAt: Date.now() };
  peer.incomingFrames.set('t1', transfer);
  const ticker = setInterval(() => { transfer.lastProgressAt = Date.now(); }, 10);
  await delay(130); // > 3 x the 40 ms budget, answer still streaming
  clearInterval(ticker);
  peer.incomingFrames.delete('t1');
  answer(peer, frame.id, { documents: [], checkpoint: { id: 'lead_9', lwt: 9 } });
  assert.deepEqual((await result).checkpoint, { id: 'lead_9', lwt: 9 },
    'a progressing framed answer must not time out');
  assert.deepEqual(peer.removed, []);
}
// 7b. A stalled transfer (no progress) still times out.
{
  const peer = fakePeer();
  const result = peer.request('native-1', 'masterChangesSince', [null, 10], 30);
  peer.sent[0].hooks.onSent();
  peer.incomingFrames.set('t1', { peerId: 'native-1', createdAt: Date.now(), lastProgressAt: Date.now() - 1_000 });
  await assert.rejects(result, /Timed out waiting for WebRTC response masterChangesSince/);
}
// 7c. A timed-out pull batch is retried smaller from the SAME checkpoint.
{
  const { getReplicationStateClass } = replicationWebRtcTestInternals;
  const calls = [];
  const fake = {
    cancelled: false,
    collection: { name: 'outbound_lead_generation_leads' },
    requestTimeoutMsFor: () => 15_000,
    isTransientMasterChangesSinceError: () => true,
    waitForOpenPeerId: async () => 'native-1',
    peer: {
      async request(_peerId, method, params) {
        calls.push({ method, checkpoint: params[0], batchSize: params[1] });
        if (calls.length < 3) throw new Error('Timed out waiting for WebRTC response masterChangesSince');
        return { documents: [], checkpoint: params[0] };
      },
    },
  };
  const checkpoint = { id: 'lead_4', lwt: 4 };
  const response = await getReplicationStateClass().prototype.requestMasterChangesSince
    .call(fake, 'native-1', checkpoint, 10);
  assert.deepEqual(calls.map((call) => call.batchSize), [10, 5, 2], 'retries shrink the batch');
  assert.ok(calls.every((call) => call.checkpoint === checkpoint), 'retries resume from the same checkpoint');
  assert.deepEqual(response.result.checkpoint, checkpoint);
}
// 7d. A partial (never completed) pull checkpoint retained across a reconnect
// stays valid while the native head only moved forward.
{
  const { remoteCheckpointStillCovers } = replicationWebRtcTestInternals;
  const retained = {
    validityKey: 'gen-1|outbound_lead_generation_leads|schema-1|epoch-a',
    pull: { id: 'lead_40', lwt: 40 },
    firstPullCompletedAtMs: null,
  };
  assert.equal(remoteCheckpointStillCovers(
    retained,
    'gen-1|outbound_lead_generation_leads|schema-1|epoch-b',
    { checkpoint: { latestLwt: 55 } },
  ), true, 'reconnect resumes the partial initial pull instead of restarting from zero');
}

console.log('ctox-rxdb command submit churn smoke OK');
process.exit(0);
