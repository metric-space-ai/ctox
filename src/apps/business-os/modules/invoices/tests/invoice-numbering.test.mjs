// tests/invoice-numbering.test.mjs — exercises the pure numbering helpers.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { formatNumber, advanceSeries, detectGaps } from '../core/invoice-numbering.js';

test('formatNumber pads counter to 4 digits by default', () => {
  assert.equal(
    formatNumber({ prefix: 'RE-', next_value: 7, fiscal_year: 2026 }),
    'RE-2026-0007'
  );
});

test('formatNumber does not duplicate the fiscal year in the prefix', () => {
  assert.equal(
    formatNumber({ prefix: 'RE-2026-', next_value: 7, fiscal_year: 2026 }),
    'RE-2026-0007'
  );
});

test('formatNumber rejects invalid inputs', () => {
  assert.throws(() => formatNumber({ prefix: '', next_value: 1, fiscal_year: 2026 }));
  assert.throws(() => formatNumber({ prefix: 'RE-', next_value: 0, fiscal_year: 2026 }));
  assert.throws(() => formatNumber({ prefix: 'RE-', next_value: 1.5, fiscal_year: 2026 }));
});

test('formatNumber honours custom padding', () => {
  assert.equal(
    formatNumber({ prefix: 'X-', next_value: 1, fiscal_year: 2026, padding: 6 }),
    'X-2026-000001'
  );
});

test('advanceSeries increments next_value', () => {
  const next = advanceSeries({ next_value: 7, gap_policy: 'strict_no_gaps' });
  assert.equal(next.next_value, 8);
  assert.equal(next.last_issued_number, null);
});

test('advanceSeries with voided_value marks last issued as voided', () => {
  const next = advanceSeries({
    next_value: 7,
    gap_policy: 'reserved_then_voided',
    voided_value: 6,
  });
  assert.equal(next.next_value, 8);
  assert.equal(next.last_issued_number, 'voided:6');
});

test('advanceSeries rejects unknown gap_policy', () => {
  assert.throws(() => advanceSeries({ next_value: 1, gap_policy: 'whatever' }));
});

test('detectGaps finds missing numbers in a series', () => {
  const issued = [1, 2, 4, 5, 7];
  const gaps = detectGaps(issued, 1, 7);
  assert.deepEqual(gaps, [3, 6]);
});

test('detectGaps returns empty when no gaps', () => {
  assert.deepEqual(detectGaps([1, 2, 3], 1, 3), []);
});

test('detectGaps returns empty for invalid bounds', () => {
  assert.deepEqual(detectGaps([1, 2, 3], 5, 3), []);
});
