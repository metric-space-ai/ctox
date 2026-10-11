import { test } from 'node:test';
import assert from 'node:assert/strict';
import dgram from 'node:dgram';
import { once } from 'node:events';
import { performance } from 'node:perf_hooks';
import { UdpRelay } from './udp-relay.cjs';

for (const rtt of [0, 300, 600]) test(`independent UDP echo traverses ${rtt} ms RTT relay`, async () => {
  const native = dgram.createSocket('udp4');
  const browser = dgram.createSocket('udp4');
  const relay = new UdpRelay(rtt);
  try {
    native.bind(0, '127.0.0.1'); browser.bind(0, '127.0.0.1');
    await Promise.all([once(native, 'listening'), once(browser, 'listening')]);
    native.on('message', (data, peer) => native.send(data, peer.port, peer.address));
    const endpoint = (socket, role) => ({ id: role, role });
    const message = (socket) => ({ data: { type: 'candidate', candidate: { candidate:
      `candidate:test 1 udp 123 127.0.0.1 ${socket.address().port} typ host` } } });
    const n = endpoint(native, 'ctox_instance'), b = endpoint(browser, 'browser');
    const rewritten = await relay.rewrite(message(native), n, b);
    const browserOffer = message(browser);
    // The browser advertises a LAN address but actually uses loopback.
    browserOffer.data.candidate.candidate = browserOffer.data.candidate.candidate.replace('127.0.0.1', '172.18.0.2');
    await relay.rewrite(browserOffer, b, n);
    const port = Number(rewritten.data.candidate.candidate.split(' ')[5]);
    assert.notEqual(port, native.address().port);
    const received = once(browser, 'message', { signal: AbortSignal.timeout(3000) });
    const started = performance.now();
    browser.send(Buffer.from('independent-oracle'), port, '127.0.0.1');
    const [data] = await received;
    const elapsedMs = performance.now() - started;
    assert.equal(data.toString(), 'independent-oracle');
    assert.ok(elapsedMs >= rtt - 4, `${elapsedMs}ms < requested ${rtt}ms RTT`);
    const snapshot = await relay.snapshot();
    assert.deepEqual(snapshot.errors, []);
    assert.equal(snapshot.pairs.length, 1);
    assert.equal(snapshot.pairs[0].forwarded.browser, 1);
    assert.equal(snapshot.pairs[0].forwarded.native, 1);
    assert.equal(snapshot.pairs[0].bytes.browser, data.length);
    assert.equal(snapshot.pairs[0].bytes.native, data.length);
    assert.ok(snapshot.pairs[0].holdMinMs >= rtt / 2, 'Every observed datagram retains the monotonic minimum');
  } finally { await relay.close(); native.close(); browser.close(); }
});

test('candidate rewrite rejects direct bypasses and non-isolated native endpoints', async () => {
  const relay = new UdpRelay(300);
  const n = { id: 'n', role: 'ctox_instance' }, b = { id: 'b', role: 'browser' };
  try {
    const candidate = text => ({ data: { candidate: { candidate: text } } });
    assert.equal(await relay.rewrite(candidate('candidate:a 1 tcp 1 127.0.0.1 9999 typ host'), n, b), null);
    assert.equal(await relay.rewrite(candidate('candidate:a 1 udp 1 127.0.0.1 9999 typ srflx'), n, b), null);
    await assert.rejects(relay.rewrite(candidate('candidate:a 1 udp 1 198.51.100.1 9999 typ host'), n, b), /Non-isolated/);
    const sdp = await relay.rewrite({ data: { sdp: 'v=0\r\na=candidate:a 1 tcp 1 127.0.0.1 9999 typ host\r\n' } }, n, b);
    assert.equal(sdp.data.sdp, 'v=0\r\n');
  } finally { await relay.close(); }
});

test('only explicitly bounded RTT cases are accepted', () => {
  for (const value of [-1, 1, 299, NaN, Infinity, '300']) assert.throws(() => new UdpRelay(value), /Relay RTT/);
});
