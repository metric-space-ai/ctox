import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  ARBZG,
  accumulateUeberlassung,
  checkDailyHours,
  checkOverlaps,
  checkRestPeriods,
  evaluateArbZG,
} from './arbzg.js';

const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;
const base = Date.UTC(2026, 0, 5, 6, 0, 0); // Mon 06:00

function shift(employee_id, project_id, startHour, lengthHours, dayOffset = 0) {
  const start = base + dayOffset * DAY + startHour * HOUR;
  return { id: `${employee_id}-${dayOffset}-${startHour}`, employee_id, project_id, start_time: start, end_time: start + lengthHours * HOUR };
}

test('checkRestPeriods flags < 11h between shifts', () => {
  const shifts = [shift('e1', 'p1', 6, 8, 0), shift('e1', 'p1', 2, 8, 1)]; // ends 14:00, next starts 02:00 = 12h gap → ok
  assert.equal(checkRestPeriods(shifts).length, 0);
  const tight = [shift('e1', 'p1', 14, 6, 0), shift('e1', 'p1', 0, 8, 1)]; // ends 20:00, next 00:00 next day = 4h
  const v = checkRestPeriods(tight);
  assert.equal(v.length, 1);
  assert.equal(v[0].type, 'rest_period');
  assert.ok(v[0].restHours < 11);
});

test('checkDailyHours enforces 8h default and 10h extended', () => {
  const shifts = [shift('e1', 'p1', 6, 9, 0)]; // 9h
  assert.equal(checkDailyHours(shifts).length, 1);
  assert.equal(checkDailyHours(shifts, { extended: true }).length, 0);
  assert.equal(checkDailyHours([shift('e1', 'p1', 6, 11, 0)], { extended: true }).length, 1);
});

test('checkOverlaps detects double-booking', () => {
  const shifts = [shift('e1', 'p1', 6, 8, 0), shift('e1', 'p2', 10, 4, 0)]; // overlap 10-14
  const v = checkOverlaps(shifts);
  assert.equal(v.length, 1);
  assert.equal(v[0].type, 'overlap');
});

test('accumulateUeberlassung flags exceeding the 18-month cap', () => {
  const start = shift('e1', 'p1', 6, 8, 0);
  const end = { ...shift('e1', 'p1', 6, 8, 0), start_time: base + 600 * DAY, end_time: base + 600 * DAY + 8 * HOUR };
  const ledger = accumulateUeberlassung([start, end]);
  assert.equal(ledger.length, 1);
  assert.ok(ledger[0].overCap, `600 days > ${ARBZG.MAX_UEBERLASSUNG_DAYS}`);
});

test('evaluateArbZG aggregates violations and ok flag', () => {
  const clean = [shift('e1', 'p1', 6, 8, 0), shift('e1', 'p1', 6, 8, 7)];
  const res = evaluateArbZG(clean);
  assert.equal(res.ok, true);
  assert.equal(res.violations.length, 0);

  const dirty = [shift('e1', 'p1', 6, 9, 0), shift('e1', 'p1', 0, 8, 0)]; // 9h day + overlap/rest
  const bad = evaluateArbZG(dirty);
  assert.equal(bad.ok, false);
  assert.ok(bad.violations.some((v) => v.type === 'daily_hours'));
});
