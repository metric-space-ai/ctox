import assert from 'node:assert/strict';
import { CtoxWebRtcNativePeer, webrtcNativeTestInternals } from '../src/webrtc-native.mjs';
import { InboundRequestQueue } from '../src/inbound-request-queue.mjs';

const deferred = () => {
  let resolve;
  const promise = new Promise(r => { resolve = r; });
  return { promise, resolve };
};
const settle = async predicate => {
  const end = Date.now() + 1000;
  while (!predicate()) {
    assert.ok(Date.now() < end, 'fixture did not settle');
    await new Promise(r => setTimeout(r, 1));
  }
};
function fixture(protocolPayload) {
  const peer = new CtoxWebRtcNativePeer({
    signalingUrl: 'ws://localhost:0/unused', room: 'inbound-request-progress', protocolPayload,
  });
  const sent = [], errors = [];
  peer.send = (peerId, payload) => sent.push({ peerId, ...payload });
  peer.on('error', event => errors.push(event.detail));
  const connection = { remotePeerId: 'native', channel: null, inboundFrameGeneration: 0 };
  peer.connections.set('native', connection);
  const attach = () => {
    const channel = { label: 'ctox-rxdb', readyState: 'open', close() { this.readyState = 'closed'; } };
    peer.attachChannel(connection, channel);
    return channel;
  };
  return { peer, connection, sent, errors, attach, channel: attach() };
}
const receive = (channel, payload) => channel.onmessage({ data: JSON.stringify(payload) });
function framed(channel, transferId, payload) {
  const text = JSON.stringify(payload);
  receive(channel, { ctoxFrame: 'ctox-rxdb-frame-v1', kind: 'start', transferId, totalFrames: 1, totalBytes: new TextEncoder().encode(text).length });
  receive(channel, { ctoxFrame: 'ctox-rxdb-frame-v1', kind: 'chunk', transferId, seq: 0, data: text });
}

// Both inline and fully reassembled RPCs must release frame ingestion while
// the real request handler waits. No fake handleDataChannelFrame override.
for (const transport of ['inline', 'framed']) {
  const blocked = deferred(), entered = deferred();
  const f = fixture(async () => { entered.resolve(); await blocked.promise; return { role: 'browser' }; });
  const request = { id: 'slow-request', method: 'ctoxProtocol', params: [], collection: 'business_commands' };
  if (transport === 'inline') receive(f.channel, request);
  else framed(f.channel, 'request-transfer', request);
  await entered.promise;
  let reply = false, ack = false, framedReply = false;
  f.peer.pending.set('reply', { timer: null, resolve() { reply = true; } });
  f.peer.pending.set('framed-reply', { timer: null, resolve() { framedReply = true; } });
  f.peer.pendingFrameAcks.set('ack', { peerId: 'native', transferId: 'outgoing', ackSeq: 0, timer: null, resolve() { ack = true; } });
  receive(f.channel, { id: 'reply', result: {} });
  receive(f.channel, { ctoxFrame: 'ctox-rxdb-frame-v1', kind: 'ack', transferId: 'outgoing', ackSeq: 0, final: true });
  framed(f.channel, 'response-transfer', { id: 'framed-reply', result: { ok: true } });
  await f.connection.inboundFrameChain;
  assert.ok(reply && ack && framedReply, transport + ' request blocked independent frame progress');
  assert.ok(f.sent.some(row => row.transferId === 'response-transfer' && row.final === true));
  assert.equal(f.peer.inboundRequests.count, 1, 'unsettled handler retains budget');
  blocked.resolve();
  await settle(() => f.peer.inboundRequests.count === 0);
  assert.ok(f.sent.some(row => row.id === 'slow-request' && row.collection === 'business_commands' && row.result.role === 'browser'));
  f.peer.close();
}

// Requests keep their arrival order. A handler failure produces its correlated
// error, then the next request runs; a full request budget does not block ACKs.
{
  const blocked = deferred(), entered = deferred();
  const f = fixture(async () => { entered.resolve(); await blocked.promise; throw new Error('controlled protocol failure'); });
  receive(f.channel, { id: 'blocked', method: 'ctoxProtocol' });
  await entered.promise;
  const limit = webrtcNativeTestInternals.MAX_INBOUND_REQUESTS;
  for (let i = 1; i < limit; i++) receive(f.channel, { id: 'queued-' + i, method: 'token' });
  receive(f.channel, { id: 'over-budget', method: 'token' });
  await f.connection.inboundFrameChain;
  assert.equal(f.peer.inboundRequests.count, limit);
  assert.ok(!f.sent.some(row => row.id === 'queued-1'), 'queued request ran out of order');
  assert.equal(f.sent.find(row => row.id === 'over-budget').error.code, 'ctox_webrtc_inbound_request_budget_exceeded');
  blocked.resolve();
  await settle(() => f.peer.inboundRequests.count === 0);
  assert.ok(f.sent.find(row => row.id === 'blocked').error);
  assert.deepEqual(f.sent.filter(row => row.id?.startsWith('queued-')).map(row => row.id),
    Array.from({ length: limit - 1 }, (_, i) => 'queued-' + (i + 1)));
  assert.equal(f.peer.inboundRequests.bytes, 0);
  f.peer.close();
}

// A replacement cancels queued requests and signals the running adapter, but
// cannot pretend an uncooperative adapter has released its memory reservation.
for (const lateFailure of [false, true]) {
  const blocked = deferred(), entered = deferred();
  let signal;
  const f = fixture(async options => {
    signal = options.signal; entered.resolve(); await blocked.promise;
    if (lateFailure) throw new Error('stale failure');
    return { old: true };
  });
  receive(f.channel, { id: 'old-running', method: 'ctoxProtocol' });
  await entered.promise;
  receive(f.channel, { id: 'old-queued', method: 'token' });
  await f.connection.inboundFrameChain;
  const next = f.attach();
  f.channel.onclose();
  f.channel.onerror();
  assert.equal(f.peer.connections.get('native'), f.connection, 'late close cannot remove replacement');
  assert.equal(f.connection.lastError ?? null, null, 'late error cannot poison replacement');
  assert.equal(signal.aborted, true);
  assert.equal(f.peer.inboundRequests.count, 1);
  receive(next, { id: 'new-request', method: 'token' });
  await f.connection.inboundFrameChain;
  await settle(() => f.sent.some(row => row.id === 'new-request'));
  blocked.resolve();
  await settle(() => f.peer.inboundRequests.count === 0);
  assert.ok(!f.sent.some(row => row.id === 'old-running' || row.id === 'old-queued'));
  assert.ok(!f.errors.some(error => String(error.message).includes('stale failure')));
  assert.equal(f.peer.inboundRequests.owners.size, 0);
  f.peer.close();
}

// Byte admission counts running plus queued work across owners. Cancellation
// releases only work that did not start; handler failures cannot leak a slot.
{
  const blocked = deferred(), owner = {}, other = {};
  const errors = [];
  const queue = new InboundRequestQueue({ maxCount: 4, maxBytes: 16, onError: error => errors.push(error.message) });
  assert.ok(queue.enqueue(owner, 8, () => blocked.promise));
  assert.ok(queue.enqueue(owner, 8, () => { throw new Error('must not run'); }));
  assert.equal(queue.enqueue(other, 1, async () => {}), false);
  queue.cancel(owner);
  assert.equal(queue.count, 1);
  assert.equal(queue.bytes, 8);
  assert.ok(queue.enqueue(other, 8, async () => { throw new Error('expected'); }));
  await settle(() => errors.length === 1);
  blocked.resolve();
  await settle(() => queue.count === 0);
  assert.equal(queue.bytes, 0);
  assert.equal(queue.owners.size, 0);
  assert.deepEqual(errors, ['expected']);
}
console.log('inbound RPC progress, ordering, budgets and generation fencing PASS (component fixture; no real WebRTC)');
