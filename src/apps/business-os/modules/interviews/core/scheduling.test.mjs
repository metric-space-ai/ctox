import assert from 'node:assert/strict';
import { test } from 'node:test';

import { findCommonSlots, isMeetingState, isNoShow } from './scheduling.js';

const H = 60 * 60 * 1000;

test('findCommonSlots intersects party availability', () => {
  const parties = [
    { busy: [{ start: 0, end: 2 * H }] }, // recruiter busy 0-2h
    { busy: [{ start: 3 * H, end: 4 * H }] }, // candidate busy 3-4h
  ];
  const slots = findCommonSlots(parties, { windowStart: 0, windowEnd: 5 * H, durationMs: H, stepMs: H });
  // free 1h slots: 2-3 and 4-5
  assert.deepEqual(slots.map((s) => s.start / H), [2, 4]);
});

test('isNoShow only after the meeting without attendance', () => {
  assert.ok(isNoShow({ end: 100, attended: false }, 200));
  assert.ok(!isNoShow({ end: 100, attended: true }, 200));
  assert.ok(!isNoShow({ end: 100 }, 50));
});

test('meeting states are validated', () => {
  assert.ok(isMeetingState('confirmed'));
  assert.ok(!isMeetingState('teleport'));
});
