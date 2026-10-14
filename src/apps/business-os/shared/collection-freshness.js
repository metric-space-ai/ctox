import { collectionFreshnessFromDiagnostics, COLLECTION_FRESHNESS_MAX_AGE_MS } from './sync-contract.js?v=20261014-shell-v2-query-ready-coalesce2';

// A warning changes the interpretation of cached data, not its availability.
export function renderCollectionFreshnessWarning(warning, { collections = [], diagnostics, language = 'de', compact = false, contextKey, sessionKey, nowMs = Date.now() } = {}) {
  if (!warning) return;
  if (compact) return renderCompactFreshness(warning, { collections, diagnostics, language, contextKey, sessionKey, nowMs });
  // No state text, ever: cached data stays visible while sync retries.
  warning.hidden = true;
  warning.textContent = '';
}

const compactStates = new WeakMap();

export function compactFreshnessPresentation({ collections = [], diagnostics, language = 'de', nowMs = Date.now(), online = true, confirmedAfterMs = 0 } = {}, pendingSince = nowMs) {
  const entries = collections.map((collection) => collectionFreshnessFromDiagnostics(
    collection, diagnostics?.collections?.[collection], { syncMode: diagnostics?.mode, nowMs },
  )).filter((entry) => entry.requiresPullConfirmation);
  const pullAt = (entry) => Number(diagnostics?.collections?.[entry.collection]?.frameTransport?.lastSuccessfulPullAtMs || 0);
  const pending = entries.filter((entry) => !entry.ready || !online || pullAt(entry) <= confirmedAfterMs);
  // Freshness drives retries and state only; it never renders text (owner
  // rule 14.10.2026: no state prose for users).
  if (!pending.length) return {
    state: collections.length ? 'healthy' : 'idle', pending: false, label: '', title: '',
    // A quiet stream cannot leave an expired confirmation green indefinitely.
    nextRefreshMs: entries.length ? Math.max(1, Math.min(...entries.map((entry) =>
      pullAt(entry) + COLLECTION_FRESHNESS_MAX_AGE_MS + 1 - nowMs))) : null,
  };
  const offline = !online || pending.some((entry) => entry.state === 'offline-pending');
  const delayed = nowMs - pendingSince >= 30_000;
  return {
    state: offline ? 'offline' : delayed ? 'unconfirmed' : 'syncing',
    pending: true, label: '', title: '',
  };
}

function renderCompactFreshness(warning, options) {
  let entry = compactStates.get(warning);
  if (!entry) {
    const view = warning.ownerDocument?.defaultView;
    entry = { since: null, timer: null, scope: '', contextKey: null, sessionKey: null,
      online: view?.navigator?.onLine !== false, confirmedAfterMs: 0 };
    compactStates.set(warning, entry);
    const connectivity = (event) => {
      entry.online = event.type !== 'offline';
      // Network restoration is not a new authoritative pull.
      if (entry.online) entry.confirmedAfterMs = Date.now();
      if (entry.options) renderCompactFreshness(warning, { ...entry.options, nowMs: Date.now() });
    };
    view?.addEventListener('offline', connectivity);
    view?.addEventListener('online', connectivity);
    view?.addEventListener('pagehide', () => {
      clearTimeout(entry.timer);
      view?.removeEventListener('offline', connectivity);
      view?.removeEventListener('online', connectivity);
      compactStates.delete(warning);
    }, { once: true });
  }
  clearTimeout(entry.timer);
  entry.timer = null;
  options = { ...options, online: entry.online, confirmedAfterMs: entry.confirmedAfterMs };
  const scope = [...options.collections].sort().join('\u0000');
  if (entry.scope !== scope || entry.contextKey !== options.contextKey || entry.sessionKey !== options.sessionKey) {
    clearTimeout(entry.timer);
    entry.timer = null;
    entry.since = null;
    entry.scope = scope;
    entry.contextKey = options.contextKey;
    entry.sessionKey = options.sessionKey;
  }
  let presentation = compactFreshnessPresentation(options, entry.since ?? options.nowMs);
  if (presentation.pending) {
    entry.since ??= options.nowMs;
    presentation = compactFreshnessPresentation(options, entry.since);
  } else {
    entry.since = null;
    clearTimeout(entry.timer);
    entry.timer = null;
  }
  entry.options = options;
  warning.hidden = false;
  warning.dataset.syncState = presentation.state;
  warning.title = presentation.title;
  const label = warning.querySelector('[data-sync-state-label]');
  if (label) label.textContent = presentation.label;
  const nextRefreshMs = presentation.pending
    ? (options.nowMs - entry.since < 30_000 ? 30_000 - (options.nowMs - entry.since) : null)
    : presentation.nextRefreshMs;
  if (nextRefreshMs !== null) {
    entry.timer = setTimeout(() => {
      entry.timer = null;
      renderCompactFreshness(warning, { ...entry.options, nowMs: Date.now() });
    }, Math.max(1, nextRefreshMs));
  }
}
