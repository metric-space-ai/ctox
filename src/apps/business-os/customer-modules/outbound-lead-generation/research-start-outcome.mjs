// Outcome of a failed research start (`researchLead`).
//
// The command bus marks every rejected submission with `local_inserted`:
// `false` means the failure happened BEFORE the command was written into
// business_commands, so no command exists anywhere (not locally, not in the
// outbox, not on the wire). Field incident 17.10.2026: such a pre-write
// failure carried `transient: true` (no connected peer within the readiness
// window), was read as "still running", and the lead was patched to
// research_status 'running' with a command_id that never existed. The
// reconciler then polled that ghost command forever, and every failed start
// cost one extra lead masterWrite.
//
// Only a failure AFTER the local write may keep the lead "running".

export function researchStartFailedBeforeWrite(error) {
  if (!error || typeof error !== 'object') return false;
  return error.local_inserted === false || error.command_written === false;
}

export function isTransientResearchWaitError(error) {
  if (!error) return false;
  if (researchStartFailedBeforeWrite(error)) return false;
  const code = String(error.code || error.details?.code || '').trim();
  if (code === 'projection_delayed' || code === 'projection_pending') return true;
  const status = String(error.status || error.details?.status || '').trim();
  if (status === 'projection_pending') return true;
  if (error.transient === true || error.details?.transient === true) return true;
  // Die Shell meldet nach 30 s "Die Uebergabe an die Queue wurde ... nicht
  // bestaetigt. Der Auftrag kann trotzdem angenommen worden sein". Am
  // 26.09.2026 kamen 13 von 14 so gemeldeten Starts danach an. Das ist
  // unbestaetigt, nicht gescheitert: der Lead bleibt verfolgbar, kein Dialog.
  if (/nicht best(ä|ae)tigt|kann trotzdem angenommen/i.test(String(error.message || ''))) return true;
  return /wartet noch auf die R/i.test(String(error.message || ''));
}

// 'not_written' -> no lead patch, no command_id on the lead.
// 'running'     -> the command exists; keep the lead trackable.
// 'other'       -> existing not-sent / failed handling in the caller.
export function researchStartFailureOutcome(error) {
  if (researchStartFailedBeforeWrite(error)) return 'not_written';
  if (isTransientResearchWaitError(error)) return 'running';
  return 'other';
}

// A browser start patch ("running" / "queued") must never overwrite a final
// research status the native side wrote AFTER this start began. Field
// observation 17.10.2026: 11 leads showed "running" although their queue task
// was handled, 6 "running" with a failed task, 2 "queued" with a cancelled
// task — a late start patch (or a pending overlay of it) had replaced the
// native final status.
export const TERMINAL_RESEARCH_STATUSES = Object.freeze(['needs_review', 'completed', 'failed']);
const START_CLAIM_STATUSES = new Set(['running', 'queued']);

// Fingerprint of everything only the native writeback may stamp: the research
// status plus every stamped (revision/review) field status. A change between
// the start and the patch proves a native write in between.
export function nativeResearchFingerprint(lead) {
  if (!lead || typeof lead !== 'object') return '';
  const stamps = [];
  const collect = (prefix, statuses) => {
    if (!statuses || typeof statuses !== 'object' || Array.isArray(statuses)) return;
    for (const [field, status] of Object.entries(statuses)) {
      if (!status || typeof status !== 'object') continue;
      if (status.revision == null && status.review == null) continue;
      stamps.push(`${prefix}${field}=${JSON.stringify(status.revision ?? null)}/${JSON.stringify(status.review ?? null)}`);
    }
  };
  collect('', lead.field_status);
  for (const [person, statuses] of Object.entries(lead.person_field_status || {})) collect(`${person}:`, statuses);
  stamps.sort();
  return `${String(lead.research_status || '')}|${stamps.join(',')}`;
}

function researchWrittenAtMs(lead) {
  return Math.max(
    Number(lead?.research_updated_at_ms || 0),
    Number(lead?.payload?.research_finished_at_ms || 0),
  ) || 0;
}

// `current`: the lead as stored right now. `start`: { sinceMs, commandId,
// baseFingerprint } captured when this start began.
export function terminalResearchStatusWins(current, start = {}) {
  const status = String(current?.research_status || '');
  if (!TERMINAL_RESEARCH_STATUSES.includes(status)) return false;
  const commandId = String(start.commandId || '').trim();
  if (commandId && String(current?.command_id || '').trim() === commandId) return true;
  if (start.baseFingerprint != null && start.baseFingerprint !== ''
    && nativeResearchFingerprint(current) !== start.baseFingerprint) return true;
  const sinceMs = Number(start.sinceMs || 0);
  return sinceMs > 0 && researchWrittenAtMs(current) >= sinceMs;
}

export function patchClaimsResearchStart(patch) {
  return START_CLAIM_STATUSES.has(String(patch?.research_status || ''));
}
