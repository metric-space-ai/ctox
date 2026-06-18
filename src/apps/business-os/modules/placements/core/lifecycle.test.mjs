import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  canTransitionOffer,
  computePlacementFee,
  earlyLeaveOutcome,
  guaranteeStatus,
  isTerminalOfferState,
} from './lifecycle.js';

const DAY = 24 * 60 * 60 * 1000;
const START = Date.UTC(2026, 0, 1);

test('offer transitions follow the state machine', () => {
  assert.ok(canTransitionOffer('draft', 'extended'));
  assert.ok(canTransitionOffer('extended', 'accepted'));
  assert.ok(canTransitionOffer('negotiating', 'extended'));
  assert.ok(!canTransitionOffer('accepted', 'extended'));
  assert.ok(!canTransitionOffer('draft', 'accepted'));
  assert.ok(isTerminalOfferState('declined'));
});

test('computePlacementFee handles percent and flat', () => {
  assert.equal(computePlacementFee({ feeType: 'percent', feePercent: 25, annualSalary: 60000 }), 15000);
  assert.equal(computePlacementFee({ feeType: 'flat', flatFee: 8000 }), 8000);
});

test('guaranteeStatus reflects the clock', () => {
  const placement = { start_ms: START, guarantee_days: 90 };
  assert.equal(guaranteeStatus(placement, START - DAY).status, 'pending');
  assert.equal(guaranteeStatus(placement, START + 30 * DAY).status, 'active');
  assert.equal(guaranteeStatus(placement, START + 200 * DAY).status, 'elapsed');
  assert.equal(guaranteeStatus(placement, START + 30 * DAY).remainingDays, 60);
});

test('earlyLeaveOutcome computes pro-rata clawback within guarantee', () => {
  const placement = { start_ms: START, guarantee_days: 90, fee: 9000 };
  const within = earlyLeaveOutcome(placement, START + 30 * DAY);
  assert.equal(within.withinGuarantee, true);
  assert.equal(within.servedDays, 30);
  assert.equal(within.clawback, 6000); // 60/90 * 9000
  const after = earlyLeaveOutcome(placement, START + 200 * DAY);
  assert.equal(after.withinGuarantee, false);
  assert.equal(after.clawback, 0);
});
