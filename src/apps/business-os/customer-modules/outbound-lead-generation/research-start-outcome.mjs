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
