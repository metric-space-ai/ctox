// The research reconcile decides on the lead copy held in memory but writes to
// the current document. When the document has moved on (the native writeback
// stored the result), the decision no longer holds. On thesen (09.10.2026) a
// browser set 151 researched leads ("needs_review") to "failed" from stale
// "running" copies against old failed commands and replaced their payload with
// the old one.
export function abgleichBasisVeraltet(basis, current) {
  if (basis?._rev && current?._rev && basis._rev !== current._rev) return true;
  return String(basis?.research_status || '') !== String(current?.research_status || '')
    || String(basis?.command_id || '') !== String(current?.command_id || '');
}

// Verified fields are a result, even while status or payload show another state.
export function hatBelegteFelder(lead) {
  return Object.values(lead?.field_status || {}).some((entry) => entry?.status === 'verified');
}
