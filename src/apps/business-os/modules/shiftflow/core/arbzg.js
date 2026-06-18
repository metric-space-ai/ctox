// core/arbzg.js — pure German working-time (ArbZG) and AÜG max-assignment
// (Höchstüberlassungsdauer) checks for the dispatch/timesheet engine.
// No DOM, no RxDB, no business_commands.
//
// Baukasten note: this is a generic "evaluate scheduling rules over a set of
// shifts" engine. The shipped ruleset is German ArbZG/AÜG; another vertical
// swaps the thresholds. The previous shiftflow code only checked weekly hours +
// double-booking yet claimed "Ruhezeiten geprüft" — these helpers make the rest
// period, daily cap and Überlassungsdauer checks real.

const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;

export const ARBZG = {
  MIN_REST_MS: 11 * HOUR, // §5 ArbZG — 11h rest between shifts
  MAX_DAILY_MS: 8 * HOUR, // §3 ArbZG — 8h werktäglich
  MAX_DAILY_EXT_MS: 10 * HOUR, // §3 ArbZG — up to 10h with compensation
  MAX_WEEKLY_HOURS: 48, // 6 × 8h
  MAX_UEBERLASSUNG_DAYS: 18 * 30, // §1 AÜG ~18 months
  UEBERLASSUNG_WARN_RATIO: 0.85,
};

/** @typedef {{id?: string, employee_id?: string, project_id?: string, start_time?: number, end_time?: number}} Shift */

export function shiftDurationMs(shift) {
  const start = Number(shift?.start_time);
  const end = Number(shift?.end_time);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return 0;
  return end - start;
}

function byEmployee(shifts) {
  const map = new Map();
  for (const shift of shifts || []) {
    const key = shift?.employee_id || '';
    if (!key) continue;
    if (!map.has(key)) map.set(key, []);
    map.get(key).push(shift);
  }
  for (const list of map.values()) list.sort((a, b) => Number(a.start_time) - Number(b.start_time));
  return map;
}

function dayKey(ms) {
  return Math.floor(Number(ms) / DAY);
}

/** §5 — 11h rest between consecutive shifts of the same employee. */
export function checkRestPeriods(shifts) {
  const violations = [];
  for (const [employeeId, list] of byEmployee(shifts)) {
    for (let i = 1; i < list.length; i += 1) {
      const gap = Number(list[i].start_time) - Number(list[i - 1].end_time);
      if (gap >= 0 && gap < ARBZG.MIN_REST_MS) {
        violations.push({
          type: 'rest_period',
          severity: 'error',
          employeeId,
          shiftId: list[i].id,
          restHours: Math.round((gap / HOUR) * 10) / 10,
        });
      }
    }
  }
  return violations;
}

/** §3 — daily hours per employee, capped at 8h (or 10h extended). */
export function checkDailyHours(shifts, { extended = false } = {}) {
  const cap = extended ? ARBZG.MAX_DAILY_EXT_MS : ARBZG.MAX_DAILY_MS;
  const perDay = new Map();
  for (const shift of shifts || []) {
    const employeeId = shift?.employee_id;
    if (!employeeId) continue;
    const key = `${employeeId}|${dayKey(shift.start_time)}`;
    perDay.set(key, (perDay.get(key) || 0) + shiftDurationMs(shift));
  }
  const violations = [];
  for (const [key, total] of perDay) {
    if (total > cap) {
      const [employeeId] = key.split('|');
      violations.push({
        type: 'daily_hours',
        severity: 'error',
        employeeId,
        hours: Math.round((total / HOUR) * 10) / 10,
        capHours: cap / HOUR,
      });
    }
  }
  return violations;
}

/** Weekly hours per employee vs. their target (default 48h ArbZG ceiling). */
export function checkWeeklyHours(shifts, employeesById = new Map()) {
  const totals = new Map();
  for (const shift of shifts || []) {
    const employeeId = shift?.employee_id;
    if (!employeeId) continue;
    totals.set(employeeId, (totals.get(employeeId) || 0) + shiftDurationMs(shift));
  }
  const violations = [];
  for (const [employeeId, total] of totals) {
    const hours = total / HOUR;
    const emp = employeesById.get?.(employeeId);
    const target = Number(emp?.weekly_target_hours) || ARBZG.MAX_WEEKLY_HOURS;
    if (hours > target) {
      violations.push({
        type: 'weekly_hours',
        severity: 'warning',
        employeeId,
        hours: Math.round(hours * 10) / 10,
        targetHours: target,
      });
    }
  }
  return violations;
}

/** Double-booking: overlapping shifts for the same employee. */
export function checkOverlaps(shifts) {
  const violations = [];
  for (const [employeeId, list] of byEmployee(shifts)) {
    for (let i = 1; i < list.length; i += 1) {
      if (Number(list[i].start_time) < Number(list[i - 1].end_time)) {
        violations.push({ type: 'overlap', severity: 'error', employeeId, shiftId: list[i].id });
      }
    }
  }
  return violations;
}

/**
 * §1 AÜG — cumulative assignment span per (worker, Entleiher=project). Flags
 * assignments approaching or exceeding the Höchstüberlassungsdauer.
 */
export function accumulateUeberlassung(shifts, { capDays = ARBZG.MAX_UEBERLASSUNG_DAYS } = {}) {
  const spans = new Map(); // employee|project -> {min, max}
  for (const shift of shifts || []) {
    const employeeId = shift?.employee_id;
    const projectId = shift?.project_id;
    if (!employeeId || !projectId) continue;
    const key = `${employeeId}|${projectId}`;
    const span = spans.get(key) || { min: Infinity, max: -Infinity };
    span.min = Math.min(span.min, Number(shift.start_time));
    span.max = Math.max(span.max, Number(shift.end_time));
    spans.set(key, span);
  }
  const results = [];
  for (const [key, span] of spans) {
    if (!Number.isFinite(span.min) || !Number.isFinite(span.max)) continue;
    const [employeeId, projectId] = key.split('|');
    const days = Math.ceil((span.max - span.min) / DAY);
    const overCap = days > capDays;
    const nearCap = !overCap && days >= capDays * ARBZG.UEBERLASSUNG_WARN_RATIO;
    results.push({ employeeId, projectId, days, capDays, overCap, nearCap });
  }
  return results;
}

/** Run the full ruleset and return flat violations plus the Überlassung ledger. */
export function evaluateArbZG(shifts, { employeesById = new Map(), extended = false, capDays } = {}) {
  const violations = [
    ...checkRestPeriods(shifts),
    ...checkDailyHours(shifts, { extended }),
    ...checkWeeklyHours(shifts, employeesById),
    ...checkOverlaps(shifts),
  ];
  const ueberlassung = accumulateUeberlassung(shifts, capDays ? { capDays } : {});
  for (const entry of ueberlassung) {
    if (entry.overCap) {
      violations.push({ type: 'ueberlassung_exceeded', severity: 'error', employeeId: entry.employeeId, projectId: entry.projectId, days: entry.days, capDays: entry.capDays });
    } else if (entry.nearCap) {
      violations.push({ type: 'ueberlassung_near', severity: 'warning', employeeId: entry.employeeId, projectId: entry.projectId, days: entry.days, capDays: entry.capDays });
    }
  }
  return { violations, ueberlassung, ok: violations.length === 0 };
}
