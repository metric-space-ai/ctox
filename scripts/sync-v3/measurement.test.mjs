import test from 'node:test';
import assert from 'node:assert/strict';
import { loadFixture, fixtureDocument, validateFixture } from './fixture.mjs';
import { DataChannelByteMeter, counterDelta } from './transport-counters.mjs';

const report = (id, bytesReceived = 0, bytesSent = 0) => ({ type: 'data-channel', id,
  label: 'legacy', state: 'open', bytesReceived, bytesSent, messagesReceived: 0, messagesSent: 0 });
const peer = (...reports) => ({ getStats: async () => new Map(reports.map((item) => [item.id, item])) });

test('RFC scale and byte units are pinned, deterministic and bounded per document', async () => {
  const fixture = await loadFixture();
  assert.deepEqual(fixture.collections.map(({ count, documentBytes }) => [count, documentBytes]),
    [[850, 80000], [6000, 29500], [400, 27000], [2000, 1024]]);
  assert.equal(fixture.collections.reduce((sum, item) => sum + item.count * item.documentBytes, 0), 257848000);
  const ids = new Set();
  for (const collection of fixture.collections) {
    for (let index = 0; index < collection.count; index += 1) {
      const row = fixtureDocument(collection, index);
      assert.equal(Buffer.byteLength(JSON.stringify(row)), collection.documentBytes);
      assert.equal(row.fixtureOnly, true);
      assert.equal(ids.has(row.id), false);
      ids.add(row.id);
    }
    assert.deepEqual(fixtureDocument(collection, 0), fixtureDocument(collection, 0));
    assert.throws(() => fixtureDocument(collection, collection.count));
  }
  assert.equal(ids.size, 9250);
  assert.throws(() => validateFixture({ ...fixture, collections: [...fixture.collections.slice(0, 3), fixture.collections[0]] }));
  assert.throws(() => validateFixture({ ...fixture, collections: fixture.collections.map((item) => ({ ...item, count: 10001 })) }));
});

test('twenty collection aliases count one actual peer only once', async () => {
  const entry = report('same', 10, 20);
  const connection = peer(entry);
  const meter = new DataChannelByteMeter();
  const before = await meter.snapshot(Array(20).fill(connection));
  entry.bytesReceived += 1234;
  entry.bytesSent += 5678;
  const after = await meter.snapshot(Array(20).fill(connection));
  assert.equal(after.connections, 1);
  assert.equal(after.channels.length, 1);
  assert.deepEqual(counterDelta(before, after).totals,
    { bytesReceived: 1234, bytesSent: 5678, messagesReceived: 0, messagesSent: 0 });
});

test('equal report IDs on different connections and different SCTP channels stay distinct', async () => {
  const first = report('channel0', 100, 200);
  const second = report('channel0', 300, 400);
  const otherChannel = report('channel1', 500, 600);
  const peers = [peer(first, otherChannel, { id: 'ice', type: 'candidate-pair', bytesReceived: 999999 }), peer(second)];
  const meter = new DataChannelByteMeter();
  const before = await meter.snapshot(peers);
  first.bytesReceived += 7;
  second.bytesReceived += 11;
  otherChannel.bytesReceived += 13;
  const after = await meter.snapshot(peers);
  assert.equal(after.channels.length, 3);
  assert.equal(counterDelta(before, after).totals.bytesReceived, 31);
});

test('missing stats, counters and unsafe numbers never become zero bytes', async () => {
  const meter = new DataChannelByteMeter();
  await assert.rejects(meter.snapshot([]), /No actual/);
  await assert.rejects(meter.snapshot([peer()]), /unavailable/);
  await assert.rejects(meter.snapshot([{}]), /Invalid RTCPeerConnection/);
  for (const value of [undefined, NaN, Infinity, -1, Number.MAX_SAFE_INTEGER + 1]) {
    const invalid = report('bad');
    invalid.bytesReceived = value;
    await assert.rejects(meter.snapshot([peer(invalid)]), /invalid/);
  }
  const missing = report('missing');
  delete missing.bytesReceived;
  await assert.rejects(meter.snapshot([peer(missing)]), /invalid/);
});

test('reconnect, counter reset, missing channel, incompatible meter and duplicate report reject intervals', async () => {
  const entry = report('same', 100, 200);
  const connection = peer(entry);
  const meter = new DataChannelByteMeter();
  const before = await meter.snapshot([connection]);
  entry.bytesReceived = 99;
  assert.throws(() => counterDelta(before, { ...before, channels: [entry] }), /identity/);
  assert.throws(() => counterDelta(before, { ...before, channels: [] }), /missing/);
  assert.throws(() => counterDelta(before, { ...before, channels: [...before.channels, ...before.channels] }), /duplicated/);
  assert.throws(() => counterDelta(before, { ...before, channels: [{ ...before.channels[0], bytesReceived: 99 }] }), /reset/);
  const afterReconnect = await meter.snapshot([peer(report('same', 200, 300))]);
  assert.throws(() => counterDelta(before, afterReconnect), /changed/);
  const anotherMeter = await new DataChannelByteMeter().snapshot([connection]);
  assert.throws(() => counterDelta(before, anotherMeter), /Incompatible/);
});
