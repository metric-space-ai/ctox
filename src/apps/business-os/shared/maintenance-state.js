export const CTOX_MAINTENANCE_MESSAGE = 'CTOX wird aktualisiert – Daten bleiben erhalten';
export const CTOX_MAINTENANCE_SYNC_MESSAGE = 'Daten werden nach dem Update synchronisiert';

const DEMAND_ONLY_COLLECTIONS = new Set([
  'desktop_file_chunks',
  'document_blob_chunks',
  'spreadsheet_blob_chunks',
]);

export function normalizeMaintenancePayload(payload, { rememberedLeaseId = '' } = {}) {
  const source = payload?.state && typeof payload.state === 'object' ? payload.state : null;
  const leaseId = String(source?.lease_id || rememberedLeaseId || '').trim();
  const rememberedPending = Boolean(rememberedLeaseId && leaseId === rememberedLeaseId);
  const active = Boolean(payload?.active || rememberedPending);
  return Object.freeze({
    active,
    leaseId,
    phase: String(source?.phase || (active ? 'preparing' : 'idle')),
    status: String(source?.status || (active ? 'active' : 'idle')),
    targetRelease: String(source?.target_release || ''),
    percent: Math.max(0, Math.min(100, Number(source?.progress?.percent || 0))),
    detail: String(source?.progress?.message || ''),
    serviceActive: source?.service_active === true,
    replicationUp: source?.replication_up === true,
    initialReplicationComplete: source?.initial_replication_complete === true,
    retryable: source?.retryable === true || source?.status === 'stale' || source?.status === 'failed',
    retryAction: String(source?.retry_action || ''),
    error: String(source?.last_error || ''),
    message: String(payload?.message || (active ? CTOX_MAINTENANCE_MESSAGE : '')),
  });
}

export function maintenanceRequiredCollections(moduleLike) {
  const values = Array.isArray(moduleLike?.collections) ? moduleLike.collections : [];
  return [...new Set(values
    .map((value) => String(value || '').trim())
    .filter((value) => value && !DEMAND_ONLY_COLLECTIONS.has(value)))]
    .sort();
}

export function maintenancePhaseLabel(state, locale = 'de') {
  const de = locale !== 'en';
  if (state?.status === 'stale') return de ? 'Upgrade unterbrochen' : 'Upgrade interrupted';
  if (state?.status === 'failed') return de ? 'Upgrade fehlgeschlagen' : 'Upgrade failed';
  if (state?.phase === 'waiting_collections') {
    return de ? CTOX_MAINTENANCE_SYNC_MESSAGE : 'Data is syncing after the update';
  }
  if (state?.phase === 'waiting_replication') {
    return de ? 'CTOX-Dienst aktiv · Replikation wird verbunden' : 'CTOX service active · reconnecting replication';
  }
  return state?.detail || (de ? CTOX_MAINTENANCE_MESSAGE : 'CTOX is being updated – your data is preserved');
}

export function isDataEmptyStateText(text) {
  const value = String(text || '').replace(/\s+/g, ' ').trim().toLowerCase();
  if (!value) return false;
  if (/\b(keine|kein|no)\b.{0,60}\b(ausgewählt|selected)\b/i.test(value)) return false;
  return /\b(noch keine|keine)\b.{0,80}\b(verfügbar|vorhanden|gefunden|angelegt|geladen|einträge|quellen|daten|knowledge|dokument|datei|tabelle)/i.test(value)
    || /\b(no|nothing)\b.{0,80}\b(available|found|loaded|entries|sources|data|documents|files|spreadsheets|knowledge)/i.test(value);
}
