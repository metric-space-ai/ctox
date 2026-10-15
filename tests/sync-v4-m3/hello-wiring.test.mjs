import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import { CtoxWebRtcNativePeer } from '../../src/apps/business-os/rxdb/src/webrtc-native.mjs';
const fixture = JSON.parse(fs.readFileSync(new URL('../../src/core/rxdb/src/plugins/replication_webrtc/hello_fixture.json', import.meta.url)));

function setup(overrides = {}) {
  const sent = [], events = [];
  const peer = new CtoxWebRtcNativePeer({ signalingUrl: 'ws://localhost.invalid', room: 'test',
    v4Hello: { verifyServerHello: async () => true,
      createHello: async ({ challenge }) => ({ ...structuredClone(fixture.hello),
        device_proof: { ...fixture.hello.device_proof, nonce: challenge } }), ...overrides } });
  peer.recordConnectionEvent = () => {};
  peer.scheduleReconnect = () => {};
  for (const name of ['peer-open', 'peer-state', 'error']) peer.on(name, value => events.push({ name, value }));
  const channel = { label: 'rxdb', readyState: 'open', send: text => sent.push(JSON.parse(text)), close() {} };
  const connection = { remotePeerId: 'native', channel, inboundFrameGeneration: 0,
    pendingCandidates: [], peer: { close() {} }, signalStats: {} };
  peer.connections.set('native', connection);
  peer.attachChannel(connection, channel);
  channel.onopen();
  return { peer, channel, connection, sent, events };
}

test('physical open does not admit application traffic before hello_ok', async () => {
  const f = setup();
  try {
    assert.equal(f.peer.send('native', { id: 'early', method: 'masterWrite' }), false);
    assert.equal(f.events.filter(e => e.name === 'peer-open').length, 0);
    await f.peer.enqueueInboundDataChannelFrame(f.connection, f.channel, fixture.hello);
    assert.equal(f.sent[0].type, 'hello');
    await f.peer.enqueueInboundDataChannelFrame(f.connection, f.channel, fixture.hello_ok);
    assert.equal(f.connection.v4Live, true);
    assert.equal(f.events.filter(e => e.name === 'peer-open').length, 1);
    assert.deepEqual(f.events.filter(e => e.name === 'peer-state').map(e => e.value.state),
      ['authenticating', 'connecting', 'authenticating', 'negotiated', 'live']);
  } finally { f.peer.close(); }
});

test('incompatible API rejects visibly once and never emits peer-open', async () => {
  const f = setup();
  await f.peer.enqueueInboundDataChannelFrame(f.connection, f.channel, fixture.hello);
  await f.peer.enqueueInboundDataChannelFrame(f.connection, f.channel,
    { ...fixture.hello_ok, app_api_ver: '9.0' });
  assert.equal(f.events.filter(e => e.name === 'error' && e.value.code === 'hello_reject').length, 1);
  assert.equal(f.events.filter(e => e.name === 'peer-open').length, 0);
  assert.equal(f.peer.connections.size, 0);
  f.peer.close();
});

test('paired-server verification cannot be skipped by a compatible hello', async () => {
  let signatures = 0;
  const f = setup({ verifyServerHello: async () => false,
    createHello: async () => { signatures++; return fixture.hello; } });
  await f.peer.enqueueInboundDataChannelFrame(f.connection, f.channel, fixture.hello);
  assert.equal(signatures, 0);
  assert.equal(f.sent.length, 0);
  assert.equal(f.events.find(e => e.name === 'error').value.reason, 'authentication_failed');
  f.peer.close();
});

test('a retired asynchronous signer cannot send into its successor', async () => {
  let finish;
  const pending = new Promise(resolve => { finish = resolve; });
  const f = setup({ createHello: () => pending });
  const receiving = f.peer.enqueueInboundDataChannelFrame(f.connection, f.channel, fixture.hello);
  await Promise.resolve(); await Promise.resolve();
  f.peer.removeConnection('native', 'test-retired', null, { reconnect: false });
  f.peer.connections.set('native', { remotePeerId: 'native', channel: {} });
  finish(fixture.hello);
  await receiving;
  assert.equal(f.sent.length, 0);
  assert.equal(f.events.filter(e => e.name === 'peer-open').length, 0);
  f.peer.connections.delete('native'); f.peer.close();
});

test('configured V4 never falls back to a legacy first frame', async () => {
  const f = setup();
  await f.peer.enqueueInboundDataChannelFrame(f.connection, f.channel,
    { id: 'legacy', method: 'ctoxProtocol', params: [] });
  assert.equal(f.events.find(e => e.name === 'error').value.code, 'hello_reject');
  assert.equal(f.events.filter(e => e.name === 'peer-open').length, 0);
  f.peer.close();
});

test('auxiliary traffic cannot bypass V4 authentication', async () => {
  const f = setup();
  await f.peer.handleAuxiliaryChannelMessage(f.connection, 'aux', '{"id":"early","method":"masterWrite"}');
  assert.equal(f.events.filter(e => e.name === 'error' && e.value.code === 'hello_reject').length, 1);
  assert.equal(f.events.filter(e => e.name === 'peer-open').length, 0);
  f.peer.close();
});
