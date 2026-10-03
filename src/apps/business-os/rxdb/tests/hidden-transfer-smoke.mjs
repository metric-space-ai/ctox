// A background tab must advance ACK windows without a page timer firing.
import assert from 'node:assert/strict';
import { CtoxWebRtcNativePeer } from '../src/webrtc-native.mjs';

const nativeSetTimeout = globalThis.setTimeout;
const nativeClearTimeout = globalThis.clearTimeout;
const timers = new Set();
const peers = [];
globalThis.setTimeout = (callback, ms) => {
  const timer = { callback, ms };
  timers.add(timer);
  return timer;
};
globalThis.clearTimeout = (timer) => timers.delete(timer);

function pair() {
  const errors = [];
  const received = [[], []];
  const endpoints = ['browser', 'remote'].map((clientId, index) => {
    const peer = new CtoxWebRtcNativePeer({
      clientId, room: 'ctox-business-os:test:hidden', signalingUrl: 'ws://unused.invalid',
    });
    peers.push(peer);
    peer.events.on('error', (error) => errors.push(error));
    peer.events.on('message', ({ detail }) => received[index].push(detail.payload));
    return peer;
  });
  endpoints.forEach((peer, index) => {
    const remote = endpoints[1 - index];
    const channel = {
      readyState: 'open', bufferedAmount: 0,
      send(text) {
        assert.ok(new TextEncoder().encode(text).length <= 16384, 'wire frame ceiling');
        queueMicrotask(() => {
          const connection = remote.connections.get(peer.options.clientId);
          remote.enqueueInboundDataChannelFrame(connection, connection.channel, JSON.parse(text));
        });
      },
    };
    peer.connections.set(remote.options.clientId, { remotePeerId: remote.options.clientId, channel });
  });
  return { endpoints, received, errors };
}

async function settleUntil(condition) {
  for (let count = 0; count < 15000 && !condition(); count += 1) await Promise.resolve();
  assert.ok(condition(), 'both directions must complete without any page timer firing');
}

try {
  // Large pull alone: proves that a hidden page is not inherently stalled.
  {
    const { endpoints: [browser, remote], received, errors } = pair();
    const leads = { id: 'pull', result: Array.from({ length: 20 }, (_, id) => ({ id, text: 'x'.repeat(92000) })) };
    remote.send('browser', leads);
    await settleUntil(() => received[0].length === 1 && remote.pendingFrameAcks.size === 0);
    assert.deepEqual(received[0], [leads]);
    assert.deepEqual(errors, []);
  }
  // Simultaneous pull + command: each sender is waiting for ACKs while it must
  // ACK the opposite direction. Previously only the 50 ms polling timer did so.
  {
    const { endpoints: [browser, remote], received, errors } = pair();
    const command = { id: 'push', result: { text: 'ä'.repeat(9500) } };
    const leads = { id: 'pull', result: Array.from({ length: 20 }, (_, id) => ({ id, text: 'x'.repeat(92000) })) };
    browser.send('remote', command);
    remote.send('browser', leads);
    await settleUntil(() => received.every((items) => items.length === 1)
      && browser.pendingFrameAcks.size === 0 && remote.pendingFrameAcks.size === 0);
    assert.deepEqual(received, [[leads], [command]]);
    assert.deepEqual(errors, []);
    assert.equal(browser.transportStats.retryCount, 0);
    assert.equal(remote.transportStats.retryCount, 0);
    assert.equal(browser.connections.get('remote').sendQueue.controlWake ?? null, null);
  }
  // Exercise real RPC correlation and a durable-acceptance-shaped response too.
  {
    const { endpoints: [browser, remote], received, errors } = pair();
    let accepted = 0;
    remote.handleRequest = async (_peer, method, params, collection) => {
      assert.equal(method, 'masterWrite');
      assert.equal(collection, 'business_commands');
      assert.equal(params[0].text.length, 19000);
      accepted += 1;
      return { accepted: true };
    };
    let receipt = null;
    const push = browser.request('remote', 'masterWrite', [{ text: 'x'.repeat(19000) }], 15000, 'business_commands')
      .then((value) => { receipt = value; });
    remote.send('browser', { id: 'large-pull', result: 'x'.repeat(1_840_000) });
    await settleUntil(() => receipt !== null && received[0].some((frame) => frame.id === 'large-pull'));
    await push;
    assert.deepEqual(receipt, { accepted: true });
    assert.equal(accepted, 1);
    assert.equal(browser.pending.size, 0);
    assert.deepEqual(errors, []);
  }
  // Hidden is not itself evidence of throttling. The existing diagnostic
  // timer provides a bounded, expiring delay sample; no extra polling timer.
  {
    const { endpoints: [browser] } = pair();
    const previousDocument = globalThis.document;
    const originalNow = Date.now;
    try {
      let now = 100000;
      Date.now = () => now;
      globalThis.document = { hidden: true };
      assert.equal(browser.getTransportStatus().throttled, false);
      browser.lastTransportStatusEmitAtMs = now;
      browser.emitTransportStatus();
      const timer = browser.transportStatusEmitTimer;
      now += 1000;
      timer.callback();
      assert.equal(browser.getTransportStatus().pageHidden, true);
      assert.equal(browser.getTransportStatus().lastPageTimerDelayMs, 750);
      assert.equal(browser.getTransportStatus().throttled, true);
      globalThis.document.hidden = false;
      assert.equal(browser.getTransportStatus().throttled, false);
      globalThis.document.hidden = true;
      now += 30001;
      assert.equal(browser.getTransportStatus().throttled, false);
    } finally {
      Date.now = originalNow;
      if (previousDocument === undefined) delete globalThis.document;
      else globalThis.document = previousDocument;
    }
  }
  await settleUntil(() => peers.every((peer) => peer.pendingFrameAcks.size === 0
    && [...peer.connections.values()].every((connection) => !connection.sendQueue?.draining)));
  console.log('hidden transfer smoke OK: 1.84 MB pull + 19 KB push, no progress timers, real RPC, delay diagnostics');
} finally {
  for (const peer of peers) {
    peer.connections.clear();
    peer.close();
  }
  globalThis.setTimeout = nativeSetTimeout;
  globalThis.clearTimeout = nativeClearTimeout;
}
