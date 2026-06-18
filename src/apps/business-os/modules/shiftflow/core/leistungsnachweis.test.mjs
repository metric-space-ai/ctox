import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  computeNachweisPay,
  computeNachweisTotals,
  evaluateBillingGate,
  isBillingReleased,
} from './leistungsnachweis.js';

test('computeNachweisTotals tallies by category', () => {
  const totals = computeNachweisTotals([
    { type: 'regular', hours: 8 },
    { type: 'nacht', hours: 2 },
    { type: 'bogus', hours: 1 },
  ]);
  assert.equal(totals.regular, 9); // bogus → regular
  assert.equal(totals.nacht, 2);
  assert.equal(totals.total, 11);
});

test('computeNachweisPay applies surcharges', () => {
  const totals = computeNachweisTotals([{ type: 'regular', hours: 10 }, { type: 'nacht', hours: 10 }]);
  const pay = computeNachweisPay(totals, 20, { nacht: 25 });
  // 10*20 + 10*20*1.25 = 200 + 250 = 450
  assert.equal(pay, 450);
});

test('billing gate requires Entleiher signature and entries', () => {
  assert.ok(!isBillingReleased({ entleiher_signed: false }));
  assert.ok(isBillingReleased({ entleiher_signed: true, signed_at_ms: 1 }));

  const blocked = evaluateBillingGate({ entleiher_signed: false, entries: [] });
  assert.equal(blocked.allowed, false);
  assert.equal(blocked.blockers.length, 2);

  const ok = evaluateBillingGate({ entleiher_signed: true, signed_at_ms: 1, entries: [{ type: 'regular', hours: 8 }] });
  assert.equal(ok.allowed, true);
});
