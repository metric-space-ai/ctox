// Reads fail transiently while CTOX (re)starts: the shell has no query
// authority yet, a read times out, or a reconnect changes the query
// generation. The collection reloader retries those reads with backoff, and on
// thesen (07.10.2026) the app recovered by itself about 20 s after a restart.
// Until a grace period has passed, such a failure is shown as loading, not as
// an error with "Neu verbinden". Every other failure is shown at once.
export const READ_ERROR_GRACE_MS = 60000;

const TRANSIENT_CODES = new Set([
  'LEAD_QUERY_AUTHORITY_MISSING',
  'LEAD_QUERY_TIMEOUT',
  'LEAD_QUERY_GENERATION_CHANGED',
]);

export function isTransientReadError(reason) {
  if (TRANSIENT_CODES.has(reason?.code)) return true;
  const text = String(reason?.message || reason || '');
  return /authority is unavailable|noch nicht für Datenabfragen bereit|nicht rechtzeitig aus CTOX geladen|während des Ladens geändert/i.test(text);
}

// The entry for a failed read; a continuing failure keeps its first timestamp.
export function readErrorEntry(previous, reason, now = Date.now()) {
  const transient = isTransientReadError(reason);
  return {
    message: String(reason?.message || reason),
    transient,
    since: previous?.transient === transient && Number.isFinite(previous?.since) ? previous.since : now,
  };
}

export function visibleReadErrorKeys(errors, now = Date.now(), graceMs = READ_ERROR_GRACE_MS) {
  if (!errors?.size) return [];
  return [...errors]
    .filter(([, entry]) => !entry?.transient || now - entry.since >= graceMs)
    .map(([key]) => key);
}
