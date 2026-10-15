// Acceptance oracles consume measured values; they do not simulate a peer/store.
export function completeReload(m) {
  return Number.isFinite(m.visibleMs) && m.visibleMs <= 1000 && m.rows > 0
    && m.preQueryLocalCount === m.rows && m.missing.length === 0 && m.duplicates === 0;
}
export function completeFirstWindow(m) {
  return Number.isFinite(m.visibleMs) && m.visibleMs <= 1000 && !m.emptyRender && m.missing.length === 0;
}
export function checkpointDelivered(observation, control, nativeRow, injected) {
  return observation.found === true && control.found === true && control.nativeConfirmed === true
    && nativeRow?.id === injected.id && nativeRow?.lwt === injected.lwt;
}

export function catalogAccepted(report, expected) {
  return expected.length > 0 && report.terminal === true && report.cleanup === 'owned groups absent'
    && report.scenarios.length === expected.length
    && new Set(report.scenarios.map(s=>s.id)).size === expected.length
    && expected.every(id=>report.scenarios.some(s=>s.id===id && s.status==='measured' && s.pass===true && s.acceptanceComplete===true));
}
