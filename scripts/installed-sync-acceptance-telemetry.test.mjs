import test from 'node:test';
import assert from 'node:assert/strict';
import { diagnosticScalars, documentAudit, logExcerpt } from './installed-sync-acceptance.mjs';

test('failure telemetry retains diagnostic counts without exporting authority or records', () => {
  const secret = 'DO_NOT_EXPORT_THIS_AUTHORITY';
  const input = {
    phase: 'sync', count: 2, connected: false,
    capability_token: secret,
    nested: { password: secret, endpoint: 'https://example.invalid/?token=' + secret,
      lastError: { code: 'AUTH_DENIED', message: 'Bearer ' + secret },
      documents: [{ id: 'private-record', amount: 42 }],
      arbitraryString: secret, pending: 1 },
  };
  const result = diagnosticScalars(input);
  assert.equal(result.count, 2);
  assert.equal(result.connected, false);
  assert.equal(result.nested.pending, 1);
  assert.equal(result.nested.lastError.code, 'AUTH_DENIED');
  const exported = JSON.stringify(result);
  for (const forbidden of [secret, 'private-record', 'example.invalid', 'amount', 'Bearer'])
    assert.equal(exported.includes(forbidden), false);
});

test('failure telemetry bounds recursive and cyclic diagnostics', () => {
  const input = { pending: 3 }; input.self = input;
  const result = diagnosticScalars(input);
  assert.equal(result.pending, 3);
  assert.doesNotThrow(() => JSON.stringify(result));
  assert.ok(JSON.stringify(result).length < 1000);
});

test('divergence audit separates missing and different documents and preserves only revision clocks', () => {
  const id = 'acceptance-synthetic-1';
  const a = { id, label: 'expected', _rev: '2-abc', _hlc: { wall: 123, counter: 4, node: 'synthetic' }, privateBody: 'NEVER_EXPORT' };
  const b = { ...a, label: 'different', _rev: '1-def' };
  const result = documentAudit([{ id, label: 'expected' }], { A: { [id]: a }, native: {}, B: { [id]: b } });
  assert.deepEqual(result.counts.native, { available: true, found: 0, exact: 0 });
  assert.equal(result.counts.A.exact, 1); assert.equal(result.counts.B.exact, 0);
  assert.equal(result.differences[0].sources.native.missing, true);
  assert.deepEqual(result.differences[0].sources.B.differingFields, ['label']);
  assert.equal(result.differences[0].sources.A.metadata.revision, '2-abc');
  assert.equal(result.differences[0].sources.A.metadata.hlc._hlc.wall, 123);
  assert.equal(JSON.stringify(result).includes('NEVER_EXPORT'), false);
  assert.throws(() => documentAudit([{ id: 'customer-id' }], { native: {} }), /synthetic/);
});

test('log excerpts preserve diagnostic vocabulary without exporting credential or record text', () => {
  const result = logExcerpt('WebRTC timeout AUTH_DENIED token=NEVER_EXPORT https://secret.invalid/ customer-record');
  assert.deepEqual(result.classes, ['WebRTC', 'timeout', 'AUTH_DENIED']);
  assert.equal(JSON.stringify(result).includes('NEVER_EXPORT'), false);
  assert.equal(JSON.stringify(result).includes('customer-record'), false);
  assert.match(result.sha256, /^[a-f0-9]{64}$/);
});
