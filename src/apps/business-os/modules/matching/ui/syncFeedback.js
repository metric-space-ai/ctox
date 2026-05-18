const DEFAULT_TIMEOUT_MS = 2800;
const DEFAULT_COOLDOWN_MS = 1500;
const DATA_SYNC_COOLDOWN_MS = 3500;
const MUTATION_SYNC_COOLDOWN_MS = 1400;
const SHOW_PASSIVE_SYNC_TOASTS_KEY = 'ctox.businessOs.matching.showPassiveSyncToasts';

const COLLECTION_ALIASES = Object.freeze({
  source: 'sources',
  sources: 'sources',
  requirement: 'requirements',
  requirements: 'requirements',
  object: 'objects',
  objects: 'objects',
  requirementSource: 'requirementSources',
  requirementSources: 'requirementSources',
  match: 'matches',
  matches: 'matches',
  object_photo_chunks: 'object_photo_chunks',
  match_artifacts: 'match_artifacts'
});

const COLLECTION_LABELS = Object.freeze({
  sources: 'Quelle',
  requirements: 'Anforderung',
  objects: 'Objekt',
  requirementSources: 'RequirementSource',
  matches: 'Match',
  object_photo_chunks: 'Objektbild',
  match_artifacts: 'Match-Artefakt'
});

function safeText(value) {
  return String(value ?? '').trim();
}

function normalizeCollectionName(raw) {
  const key = safeText(raw).toLowerCase();
  return COLLECTION_ALIASES[key] || key;
}

function normalizeOperation(raw) {
  const op = safeText(raw).toUpperCase();
  if (op === 'INSERT' || op === 'CREATE' || op === 'ADD') return 'INSERT';
  if (op === 'DELETE' || op === 'REMOVE') return 'DELETE';
  return 'UPDATE';
}

function shortDocId(raw, maxLen = 22) {
  const s = safeText(raw);
  if (!s) return '';
  if (s.length <= maxLen) return s;
  const head = Math.max(6, Math.floor(maxLen / 2) - 1);
  const tail = Math.max(5, maxLen - head - 1);
  return `${s.slice(0, head)}…${s.slice(-tail)}`;
}

function clampProgress(value) {
  if (!Number.isFinite(value)) return null;
  return Math.max(0, Math.min(100, Math.round(value)));
}

function passiveSyncToastsEnabled() {
  try {
    return localStorage.getItem(SHOW_PASSIVE_SYNC_TOASTS_KEY) === '1';
  } catch {
    return false;
  }
}

export function createSyncFeedback({ scope = 'default' } = {}) {
  const scopeSafe = safeText(scope) || 'default';
  const hostId = `sync-feedback-host-${scopeSafe}`;
  let host = null;
  let lastDataSyncAt = 0;
  const dedupeMap = new Map();
  const persistentNotes = new Map();
  const dismissedProgressIds = new Set();

  function ensureHost() {
    if (host && host.isConnected) return host;
    host = document.getElementById(hostId);
    if (host) return host;

    host = document.createElement('div');
    host.id = hostId;
    host.className = 'sync-feedback-host';
    document.body.appendChild(host);
    return host;
  }

  function show(message, { type = 'info', timeoutMs = DEFAULT_TIMEOUT_MS, dedupeKey, cooldownMs = DEFAULT_COOLDOWN_MS } = {}) {
    const msg = safeText(message);
    if (!msg) return;

    const key = safeText(dedupeKey) || `${type}:${msg}`;
    const now = Date.now();
    const last = dedupeMap.get(key) || 0;
    if (now - last < Math.max(0, Number(cooldownMs) || 0)) return;
    dedupeMap.set(key, now);

    const root = ensureHost();
    const note = document.createElement('div');
    note.className = `sync-feedback sync-feedback-${type}`;
    note.setAttribute('role', 'status');
    note.textContent = msg;
    root.appendChild(note);

    requestAnimationFrame(() => note.classList.add('is-visible'));

    const life = Math.max(800, Number(timeoutMs) || DEFAULT_TIMEOUT_MS);
    setTimeout(() => {
      note.classList.remove('is-visible');
      setTimeout(() => note.remove(), 240);
    }, life);
  }

  function removePersistentNote(id) {
    const key = safeText(id);
    if (!key) return;

    const entry = persistentNotes.get(key);
    if (!entry) return;

    if (entry.removeTimer) {
      clearTimeout(entry.removeTimer);
      entry.removeTimer = null;
    }

    if (entry.cleanupTimer) {
      clearTimeout(entry.cleanupTimer);
      entry.cleanupTimer = null;
    }

    entry.element.classList.remove('is-visible');
    entry.cleanupTimer = setTimeout(() => {
      entry.element.remove();
      const current = persistentNotes.get(key);
      if (current === entry) {
        persistentNotes.delete(key);
      }
    }, 240);
  }

  function ensurePersistentProgress(id, type = 'info') {
    const key = safeText(id);
    if (!key) return null;

    let entry = persistentNotes.get(key);
    if (entry?.element?.isConnected) {
      if (entry.removeTimer) {
        clearTimeout(entry.removeTimer);
        entry.removeTimer = null;
      }
      if (entry.cleanupTimer) {
        clearTimeout(entry.cleanupTimer);
        entry.cleanupTimer = null;
      }
      entry.element.className = `sync-feedback sync-feedback-${type} sync-feedback-progress`;
      requestAnimationFrame(() => entry.element.classList.add('is-visible'));
      return entry;
    }

    const root = ensureHost();
    const element = document.createElement('div');
    element.className = `sync-feedback sync-feedback-${type} sync-feedback-progress`;
    element.setAttribute('role', 'status');
    element.innerHTML = `
      <div class="sync-feedback-progress-head">
        <strong class="sync-feedback-progress-title"></strong>
        <span class="sync-feedback-progress-meta"></span>
        <button type="button" class="sync-feedback-progress-close" aria-label="Hinweis schließen">×</button>
      </div>
      <div class="sync-feedback-progress-detail"></div>
      <div class="sync-feedback-progress-bar" aria-hidden="true"><i></i></div>
    `;
    root.appendChild(element);

    entry = {
      element,
      title: element.querySelector('.sync-feedback-progress-title'),
      meta: element.querySelector('.sync-feedback-progress-meta'),
      close: element.querySelector('.sync-feedback-progress-close'),
      detail: element.querySelector('.sync-feedback-progress-detail'),
      bar: element.querySelector('.sync-feedback-progress-bar'),
      fill: element.querySelector('.sync-feedback-progress-bar > i'),
      removeTimer: null,
      cleanupTimer: null
    };
    persistentNotes.set(key, entry);

    entry.close?.addEventListener('click', (evt) => {
      evt.preventDefault();
      evt.stopPropagation();
      dismissedProgressIds.add(key);
      removePersistentNote(key);
    });

    requestAnimationFrame(() => element.classList.add('is-visible'));
    return entry;
  }

  function upsertProgress(id, {
    title = '',
    detail = '',
    meta = '',
    value = null,
    type = 'info',
    indeterminate = false
  } = {}) {
    const key = safeText(id);
    if (!key || dismissedProgressIds.has(key)) return;

    const entry = ensurePersistentProgress(id, type);
    if (!entry) return;

    const progress = clampProgress(value);
    const isIndeterminate = indeterminate || progress == null;

    entry.element.className = `sync-feedback sync-feedback-${type} sync-feedback-progress`;
    entry.title.textContent = safeText(title) || 'Fortschritt';
    entry.meta.textContent = safeText(meta);
    entry.meta.style.display = safeText(meta) ? '' : 'none';
    entry.detail.textContent = safeText(detail);
    entry.detail.style.display = safeText(detail) ? '' : 'none';
    entry.bar.classList.toggle('is-indeterminate', isIndeterminate);
    entry.fill.style.width = isIndeterminate ? '38%' : `${progress}%`;
  }

  function clearProgress(id, { delayMs = 0 } = {}) {
    const key = safeText(id);
    if (!key) return;
    dismissedProgressIds.delete(key);

    const entry = persistentNotes.get(key);
    if (!entry) return;

    if (entry.removeTimer) {
      clearTimeout(entry.removeTimer);
      entry.removeTimer = null;
    }

    const delay = Math.max(0, Number(delayMs) || 0);
    if (!delay) {
      removePersistentNote(key);
      return;
    }

    entry.removeTimer = setTimeout(() => {
      const current = persistentNotes.get(key);
      if (!current) return;
      current.removeTimer = null;
      removePersistentNote(key);
    }, delay);
  }

  function reportDataChange({ collectionName = '' } = {}) {
    if (!passiveSyncToastsEnabled()) return;

    const now = Date.now();
    if (now - lastDataSyncAt < DATA_SYNC_COOLDOWN_MS) return;
    lastDataSyncAt = now;

    const col = safeText(collectionName);
    const label = col ? ` (${col})` : '';
    show(`Sync: Datenänderung übernommen${label}.`, {
      type: 'success',
      dedupeKey: `data:${col || 'any'}`,
      cooldownMs: DATA_SYNC_COOLDOWN_MS
    });
  }

  function reportSyncFailure(message = '') {
    const text = safeText(message) || 'Sync fehlgeschlagen.';
    show(text, { type: 'error', dedupeKey: `sync-error:${text}`, cooldownMs: 4000 });
  }

  function reportMutationEvent({
    collectionName = '',
    operation = '',
    docId = '',
    isRemote = null,
    extraCount = 0
  } = {}) {
    const colKey = normalizeCollectionName(collectionName);
    const entity = COLLECTION_LABELS[colKey] || safeText(collectionName) || 'Datensatz';
    const op = normalizeOperation(operation);
    const idLabel = shortDocId(docId);
    const count = Math.max(0, Number(extraCount) || 0);

    let type = 'info';
    let action = 'geändert';
    if (op === 'INSERT') {
      type = 'success';
      action = 'hinzugefügt';
    } else if (op === 'DELETE') {
      type = 'warn';
      action = 'gelöscht';
    }

    const actor = isRemote === true
      ? 'Peer-Änderung'
      : isRemote === false
      ? 'Lokal gespeichert'
      : 'Sync';

    let msg = `${actor}: ${entity} ${action}`;
    if (idLabel) msg += ` (${idLabel})`;
    if (count > 0) msg += `, +${count} weitere`;
    msg += '.';

    show(msg, {
      type,
      dedupeKey: `mut:${isRemote === true ? 'remote' : isRemote === false ? 'local' : 'unknown'}:${op}:${colKey}:${idLabel}:${count > 0 ? 'multi' : 'single'}`,
      cooldownMs: MUTATION_SYNC_COOLDOWN_MS
    });
  }

  function wireWebRTCStatus() {
    const wiredKey = `__syncFeedbackWired_${scopeSafe}`;
    if (window[wiredKey]) return;
    window[wiredKey] = true;

    window.addEventListener('rxdb:webrtc-init', () => {
      show('Sync verbunden (WebRTC aktiv).', {
        type: 'info',
        dedupeKey: 'webrtc-init',
        cooldownMs: 5000
      });
    });

    window.addEventListener('rxdb:webrtc-error', (evt) => {
      const msg = safeText(evt?.detail?.err);
      reportSyncFailure(msg ? `Sync-Fehler: ${msg}` : 'Sync-Fehler: Verbindung instabil.');
    }, true);

    window.addEventListener('rxdb:webrtc-stopped', () => {
      show('Sync pausiert (lokaler Modus).', {
        type: 'warn',
        dedupeKey: 'webrtc-stopped',
        cooldownMs: 5000
      });
    });
  }

  return {
    ensureHost,
    show,
    upsertProgress,
    clearProgress,
    reportDataChange,
    reportMutationEvent,
    reportSyncFailure,
    wireWebRTCStatus,
  };
}
