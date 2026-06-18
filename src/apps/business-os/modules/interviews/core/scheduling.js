// core/scheduling.js — pure multi-party slot finding for interview coordination.
// No DOM, no RxDB. (Transcription/video-link generation are native/skill effects.)
//
// Baukasten note: a generic "intersect free windows across parties, propose
// slots" engine. Recruiting maps the parties to recruiter+candidate+client;
// another vertical schedules any multi-party meeting.

/**
 * Intersect busy intervals of all parties to find free slots of a given length
 * within a search window.
 * @param {Array<{busy?: Array<{start: number, end: number}>}>} parties
 * @param {{windowStart: number, windowEnd: number, durationMs: number, stepMs?: number}} opts
 * @returns {Array<{start: number, end: number}>}
 */
export function findCommonSlots(parties, { windowStart, windowEnd, durationMs, stepMs }) {
  const step = Number(stepMs) || durationMs;
  const list = Array.isArray(parties) ? parties : [];
  const slots = [];
  for (let start = windowStart; start + durationMs <= windowEnd; start += step) {
    const end = start + durationMs;
    const free = list.every((party) =>
      (Array.isArray(party?.busy) ? party.busy : []).every((b) => end <= b.start || start >= b.end),
    );
    if (free) slots.push({ start, end });
  }
  return slots;
}

/** Detect a no-show: the meeting passed and no attendance was recorded. */
export function isNoShow(meeting, nowMs) {
  const end = Number(meeting?.end);
  if (!Number.isFinite(end) || nowMs < end) return false;
  return !meeting?.attended;
}

export const MEETING_STATES = ['proposed', 'confirmed', 'rescheduled', 'completed', 'no_show', 'cancelled'];

export function isMeetingState(state) {
  return MEETING_STATES.includes(String(state));
}
