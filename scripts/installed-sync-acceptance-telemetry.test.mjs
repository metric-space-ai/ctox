import test from 'node:test';
import assert from 'node:assert/strict';
import { diagnosticScalars } from './installed-sync-acceptance.mjs';

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
