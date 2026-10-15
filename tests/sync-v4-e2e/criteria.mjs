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
