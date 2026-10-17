import test from 'node:test';
import assert from 'node:assert/strict';
import {
  NAMESPACE_URL, dayBucketStartMs, firstString, jsonContains, jsonScalarEq, normalizePriority,
  normalizeStatus, normalizeVisibility, numberField, stringArrayField, trimToChars, uuidV5, boolField,
} from '../_shared.js';

test('uuidV5 matches RFC 4122 reference values (python uuid.uuid5)', () => {
  assert.equal(uuidV5(NAMESPACE_URL, 'mail:thread-1'), 'e8bdc8bf-0d2f-5c62-b7df-eb99a1aa4d1e');
  assert.equal(uuidV5(NAMESPACE_URL, 'conv_sla_1:sla_1'), 'ebf88dd4-b045-516c-ab98-d32ca45a6828');
  assert.equal(uuidV5(NAMESPACE_URL, 'support:support.reply.pending_approval:count:86400000'), 'f937252c-4fa7-5b7a-9909-aad0955888ed');
  assert.equal(uuidV5(NAMESPACE_URL, 'Grüße ✓ 𝄞'), '55a026b6-03aa-582b-9cb3-57d4208d788d');
});

test('normalizers follow support.rs:4311-4334', () => {
  assert.equal(normalizeStatus(' Closed '), 'resolved');
  assert.equal(normalizeStatus('pending'), 'waiting');
  assert.equal(normalizeStatus('sleeping'), 'snoozed');
  assert.equal(normalizeStatus('whatever'), 'open');
  assert.equal(normalizePriority('CRITICAL'), 'urgent');
  assert.equal(normalizePriority('medium'), 'normal');
  assert.equal(normalizeVisibility('customer'), 'public');
  assert.equal(normalizeVisibility('team'), 'internal');
});

test('field readers mirror serde_json semantics', () => {
  assert.equal(firstString({ a: '  ', b: ' x ' }, ['a', 'b']), 'x');
  assert.equal(firstString({ a: 5 }, ['a']), undefined);
  assert.equal(numberField({ c: 0.8 }, ['c']), undefined, 'floats are not i64');
  assert.equal(numberField({ c: 3 }, ['c']), 3);
  assert.equal(numberField({ c: '3' }, ['c']), undefined);
  assert.equal(boolField({ f: 'Yes' }, ['f']), true);
  assert.equal(boolField({ f: 'maybe' }, ['f']), undefined);
  assert.deepEqual(stringArrayField({ ids: 'x', conversation_ids: ['a'] }, ['ids', 'conversation_ids']), [], 'first present key wins');
  assert.deepEqual(stringArrayField({ ids: [' a ', '', 3, 'b'] }, ['ids']), ['a', 'b']);
});

test('scalar comparison and contains', () => {
  assert.ok(jsonScalarEq('High', 'high'));
  assert.ok(jsonScalarEq('3', 3));
  assert.ok(jsonScalarEq(true, 'true'));
  assert.ok(!jsonScalarEq(3, 4));
  assert.ok(jsonScalarEq(null, ''), 'Null renders as empty string');
  assert.ok(jsonContains(['vip', 'b'], 'VIP'));
  assert.ok(jsonContains('Printer offline', 'offline'));
  assert.ok(!jsonContains('Printer offline', 'Offline'), 'string contains is case-sensitive');
});

test('trimToChars counts unicode scalar values, dayBucket truncates to UTC day', () => {
  assert.equal(trimToChars('𝄞𝄞𝄞', 2), '𝄞𝄞');
  assert.equal(dayBucketStartMs(86_400_000 * 3 + 5), 86_400_000 * 3);
  assert.equal(dayBucketStartMs(-5), 0);
});
