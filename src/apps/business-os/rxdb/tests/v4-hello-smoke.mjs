import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { decimalSeq, decodeHelloFrame, negotiateHello, HelloConnection, HelloRejected } from '../src/hello.mjs';

// The same serialized fixture is consumed by the native tests; test-only proof
// and scope strings are never used as production authority.
const fixture = JSON.parse(readFileSync(new URL('../../../../core/rxdb/src/plugins/replication_webrtc/hello_fixture.json', import.meta.url), 'utf8'));
const clone = (value) => structuredClone(value);
const policy = () => ({ instance_id: 'test-instance', bootstrap_min: 3, bootstrap_max: 4,
  app_api_ver: { min: '4.0', max: '4.1' }, capabilities: ['terminal', 'worktree'],
  required_capabilities: ['terminal'], transport: { codecs: ['zstd', 'gzip'],
    max_frame_bytes: 16384, fragmentation: true, reassembly: true } });
const rejects = (fn, reason) => assert.throws(fn, (error) => error instanceof HelloRejected && error.reason === reason);

test('V4 native/browser fixtures roundtrip all messages and preserve decimal u64', () => {
  for (const frame of Object.values(fixture)) assert.deepEqual(decodeHelloFrame(JSON.stringify(frame)), frame);
  for (const seq of ['0', '9007199254740993', '18446744073709551615']) assert.equal(decimalSeq(seq), seq);
  for (const value of [0, 9007199254740992, -1, 1.5, '', '01', '+1', '-1', '1e3',
    '18446744073709551616', ' 1']) assert.throws(() => decimalSeq(value));
  const unsafe = clone(fixture.hello_ok); unsafe.head_seq = 9007199254740992;
  rejects(() => decodeHelloFrame(JSON.stringify(unsafe)), 'invalid_hello');
});
test('V4 negotiates common API, deterministic codec, capabilities and frame ceiling; N-1 bootstrap', () => {
  const result = negotiateHello(policy(), fixture.hello);
  assert.equal(result.app_api_ver, '4.1');
  assert.deepEqual(result.capabilities, ['terminal', 'worktree']);
  assert.deepEqual(result.transport, fixture.hello_ok.transport);
  const older = clone(fixture.hello); older.bootstrap_ver = 3; older.transport.max_frame_bytes = 65536;
  assert.equal(negotiateHello(policy(), older).transport.frame_bytes, 16384);
  older.transport.codecs = ['gzip', 'zstd'];
  assert.equal(negotiateHello(policy(), older).transport.codec, 'zstd');
});
test('V4 incompatible versions, instance, capabilities and framing reject without fallback', () => {
  const cases = [
    ['protocol_incompatible', (h) => { h.protocol_ver = 3; }],
    ['bootstrap_incompatible', (h) => { h.bootstrap_ver = 2; }],
    ['instance_mismatch', (h) => { h.instance_id = 'other-instance'; }],
    ['app_api_incompatible', (h) => { h.app_api_ver = { min: '5.0', max: '5.0' }; }],
    ['capability_missing', (h) => { h.capabilities = []; }],
    ['transport_incompatible', (h) => { h.transport.codecs = []; }],
    ['transport_incompatible', (h) => { h.transport.fragmentation = false; }],
    ['transport_incompatible', (h) => { h.transport.reassembly = false; }],
    ['transport_incompatible', (h) => { h.transport.max_frame_bytes = 1023; }],
  ];
  for (const [reason, change] of cases) {
    const hello = clone(fixture.hello); change(hello);
    rejects(() => negotiateHello(policy(), hello), reason);
  }
});
test('V4 validates structure, bounded UTF-8 wire bytes and proof shape', () => {
  for (const input of ['null', '{}', '[]', '{', ' '.repeat(65537)])
    rejects(() => decodeHelloFrame(input), 'invalid_hello');
  const hello = clone(fixture.hello); hello.device_proof.signature = '';
  rejects(() => decodeHelloFrame(JSON.stringify(hello)), 'invalid_hello');
  const unknown = clone(fixture.hello); unknown.bypass = true;
  rejects(() => decodeHelloFrame(JSON.stringify(unknown)), 'invalid_hello');
  const bounds = clone(fixture.hello); bounds.app_api_ver.min = '04.0';
  rejects(() => decodeHelloFrame(JSON.stringify(bounds)), 'invalid_hello');
  const large = clone(fixture.hello); large.instance_id = '界'.repeat(30000);
  rejects(() => decodeHelloFrame(JSON.stringify(large)), 'invalid_hello');
});
test('V4 browser lifecycle only activates after validated hello_ok; state projection contains no tokens', () => {
  const states = [];
  const peer = new HelloConnection(fixture.hello, (state) => states.push(state));
  rejects(() => peer.activate(), 'unexpected_message');
  const outgoing = peer.connected(); outgoing.instance_id = 'mutated';
  assert.equal(peer.receive(JSON.stringify(fixture.hello_ok)).head_seq, '18446744073709551615');
  assert.equal(peer.snapshot.state, 'negotiated');
  peer.activate(); peer.close(); peer.close();
  assert.deepEqual(states.map((s) => s.state), ['connecting', 'authenticating', 'negotiated', 'live', 'draining', 'closed']);
  assert.equal(JSON.stringify(states).includes('test-issued-scope'), false);
  rejects(() => peer.receive(JSON.stringify(fixture.hello_ok)), 'unexpected_message');
});
test('V4 server rejection is visible in the retained browser state and closes the session', () => {
  const states = [];
  const peer = new HelloConnection(fixture.hello, (state) => states.push(state)); peer.connected();
  rejects(() => peer.receive(JSON.stringify(fixture.hello_reject)), 'app_api_incompatible');
  assert.equal(peer.snapshot.state, 'closed');
  assert.deepEqual(peer.snapshot.rejection, { reason: 'app_api_incompatible', required: 'overlapping app API range' });
  assert.deepEqual(states.slice(-2).map((s) => s.state), ['draining', 'closed']);
});
test('V4 browser refuses server selections outside the original offer', () => {
  const cases = [
    ['protocol_incompatible', (ok) => { ok.protocol_ver = 3; }],
    ['bootstrap_incompatible', (ok) => { ok.bootstrap_ver = 3; }],
    ['instance_mismatch', (ok) => { ok.instance_id = 'other-instance'; }],
    ['app_api_incompatible', (ok) => { ok.app_api_ver = '5.0'; }],
    ['capability_missing', (ok) => { ok.capabilities.push('unoffered'); }],
    ['transport_incompatible', (ok) => { ok.transport.codec = 'zstd'; }],
    ['transport_incompatible', (ok) => { ok.transport.frame_bytes = 16384; }],
  ];
  for (const [reason, change] of cases) {
    const peer = new HelloConnection(fixture.hello, () => {}); peer.connected();
    const response = clone(fixture.hello_ok); change(response);
    rejects(() => peer.receive(JSON.stringify(response)), reason);
    assert.equal(peer.snapshot.state, 'closed');
  }
});
test('V4 browser handshake deadline covers connecting, authenticating and negotiated', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  for (const boundary of [0, 1, 2]) {
    const peer = new HelloConnection(fixture.hello, () => {}, { handshakeTimeoutMs: 1000 });
    if (boundary >= 1) peer.connected();
    if (boundary === 2) peer.receive(JSON.stringify(fixture.hello_ok));
    t.mock.timers.tick(1001);
    assert.equal(peer.snapshot.state, 'closed');
    assert.equal(peer.snapshot.rejection.reason, 'handshake_timeout');
  }
  const live = new HelloConnection(fixture.hello, () => {}, { handshakeTimeoutMs: 1000 });
  live.connected(); live.receive(JSON.stringify(fixture.hello_ok)); live.activate();
  t.mock.timers.tick(1001);
  assert.equal(live.snapshot.state, 'live');
  live.close();
});
test('V4 rejects missing required browser capabilities and matches native UTF-8 bounds', () => {
  const peer = new HelloConnection(fixture.hello, () => {}, { requiredCapabilities: ['terminal'] });
  peer.connected();
  const response = clone(fixture.hello_ok); response.capabilities = ['worktree'];
  rejects(() => peer.receive(JSON.stringify(response)), 'capability_missing');
  const invalid = clone(fixture.hello); invalid.capabilities.push('界'.repeat(100));
  rejects(() => decodeHelloFrame(JSON.stringify(invalid)), 'invalid_hello');
});
