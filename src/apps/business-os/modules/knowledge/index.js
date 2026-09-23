import { loadModuleMessages } from '../../shared/i18n.js';
import { replaceChildrenIfChanged, renderHtmlIfChanged } from '../../shared/stable-dom.js';

const KNOWLEDGE_RENDER_DEBOUNCE_MS = 80;
const KNOWLEDGE_SYNC_START_WAIT_MS = 1500;
const KNOWLEDGE_INITIAL_RETRY_DELAYS_MS = Object.freeze([750, 1500, 3000, 5000, 8000]);
const KNOWLEDGE_OPEN_TARGET_KEY = 'ctox.businessOs.knowledge.openId';
const KNOWLEDGE_OPEN_DOMAIN_KEY = 'ctox.businessOs.knowledge.openDomain';
const KNOWLEDGE_DATA_COLLECTIONS = Object.freeze([
  'knowledge_items',
  'knowledge_runbooks',
  'knowledge_tables',
]);

const labels = {
  de: {
    sources: 'Quellen',
    runbooks: 'Runbooks',
    selected: 'Ausgewählt',
    noSelection: 'Kein Eintrag ausgewählt',
    detailEmptyHint: 'Wähle links einen Knowledge-Eintrag, um ihn hier anzuzeigen.',
    loading: 'Knowledge wird geladen',
    noItems: 'Keine Knowledge-Einträge gefunden.',
    noResults: 'Keine passenden Knowledge-Einträge gefunden.',
    noVisibleItems: 'Keine Knowledge-Einträge in dieser Ansicht.',
    syncUnavailable: 'Knowledge Store ist noch nicht verbunden.',
    syncingData: 'Daten werden synchronisiert.',
    noRunbooks: 'Keine Runbooks vorhanden.',
    noResources: 'Keine verifizierten Quellen zugeordnet.',
    tableUnavailable: 'Für diesen Eintrag ist keine Tabelle verfügbar.',
    dataIncomplete: 'Daten unvollständig',
    dataIncompleteHint: 'Die Tabelle wird erst angezeigt, wenn alle Daten-Chunks konsistent und vollständig repliziert wurden.',
    queued: 'Command angelegt',
    queueFailed: 'Command konnte nicht angelegt werden',
    edit: 'Bearbeiten',
    closeEditor: 'Editor schließen',
    showAsList: 'Als Liste anzeigen',
    showAsCards: 'Als Karten anzeigen',
    scopeUser: 'User',
    scopeSystem: 'System',
    scopeMixed: 'User + System',
    neverEdited: 'Noch nicht bearbeitet',
  },
  en: {
    sources: 'Sources',
    runbooks: 'Runbooks',
    selected: 'Selected',
    noSelection: 'No entry selected',
    detailEmptyHint: 'Pick a knowledge entry on the left to view it here.',
    loading: 'Loading knowledge',
    noItems: 'No knowledge entries found.',
    noResults: 'No matching knowledge entries found.',
    noVisibleItems: 'No knowledge entries in this view.',
    syncUnavailable: 'Knowledge store is not connected yet.',
    syncingData: 'Syncing data.',
    noRunbooks: 'No runbooks available.',
    noResources: 'No verified sources assigned.',
    tableUnavailable: 'This item has no table.',
    dataIncomplete: 'Incomplete data',
    dataIncompleteHint: 'The table is shown only after all data chunks are replicated consistently and completely.',
    queued: 'Command queued',
    queueFailed: 'Could not queue command',
    edit: 'Edit',
    closeEditor: 'Close editor',
    showAsList: 'Show as list',
    showAsCards: 'Show as cards',
    scopeUser: 'User',
    scopeSystem: 'System',
    scopeMixed: 'User + System',
    neverEdited: 'Never edited',
  },
};

const state = {
  ctx: null,
  lang: 'de',
  items: [],
  runbooks: [],
  tables: [],
  groups: [],
  selectedId: '',
  selectedGroupId: '',
  selectedSkillbookId: '',
  selectedTableId: '',
  selectedRunbookId: '',
  selectedResourceId: '',
  activeTab: 'skill',
  tableOffset: 0,
  tableLimit: 120,
  editing: false,
  sourceScope: 'all',
  sortMode: 'recent',
  sortDir: 'desc',
  flagFilters: new Set(),
  listView: false,
  searchTerm: '',
  messages: null,
  openGroups: new Set(['research/drone-design/drone-bearing-loads']),
  contextMenu: null,
  localSubscriptionCleanup: null,
  readinessCleanup: null,
  collectionReadiness: {},
  syncWarmupPromise: null,
  initialRetryTimer: null,
  initialRetryAttempt: 0,
  refreshInFlight: false,
  refreshPending: false,
  missingCollections: [],
  loadError: '',
};

const els = {};

export async function mount(ctx) {
  await ensureStyles();
  cancelInitialKnowledgeRetry();
  state.initialRetryAttempt = 0;
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  state.messages = await loadModuleMessages(import.meta.url, state.lang, labels);
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();
  bindElements(ctx.host);
  wireEvents();
  let disposed = false;
  renderKnowledgeList();
  renderRunbooks();
  renderEmptyKnowledgeSelection();
  state.localSubscriptionCleanup = wireLocalRealtime();
  state.readinessCleanup = wireCollectionReadiness();
  loadKnowledgeFromLocal({ initial: true }).catch((error) => {
    if (disposed || state.ctx !== ctx) return;
    state.loadError = error?.message || String(error);
    renderKnowledgeList();
    renderEmptyKnowledgeSelection();
  });
  window.addEventListener('message', handleShellMessage);
  return () => {
    disposed = true;
    window.removeEventListener('message', handleShellMessage);
    window.removeEventListener('click', handleContextOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleContextEscape);
    state.localSubscriptionCleanup?.();
    state.localSubscriptionCleanup = null;
    state.readinessCleanup?.();
    state.readinessCleanup = null;
    state.collectionReadiness = {};
    cancelInitialKnowledgeRetry();
    state.contextMenu?.remove();
    state.contextMenu = null;
  };
}

async function ensureStyles() {
  // Carry the module's own cache-buster over to the stylesheet: index.js is
  // imported as index.js?v=<build>, but a bare index.css URL would revalidate
  // against the browser cache and can serve a STALE sheet next to fresh JS.
  const version = String(import.meta.url).split('?v=')[1] || '';
  const href = new URL('./index.css', import.meta.url).pathname + (version ? `?v=${version}` : '');
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

// Monochrome stroke icons for header/close buttons. Delegates to the shell
// getActionIcon (shared/icons.js via mount ctx); the inline paths are the same
// actionIconPaths glyphs as a fallback for older shells.
const ACTION_ICON_FALLBACK_PATHS = {
  add: 'M12 5v14M5 12h14',
  close: 'M6 6l12 12M18 6L6 18',
  download: 'M12 4v11M12 15l-4-4M12 15l4-4M5 19h14',
  export: 'M12 3v11M12 3 8 7M12 3l4 4M5 12v7h14v-7',
  settings: 'M12 8.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7ZM12 3v2.2M12 18.8V21M21 12h-2.2M5.2 12H3M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6M18.4 18.4l-1.6-1.6M7.2 7.2 5.6 5.6',
};

function actionIcon(name) {
  const fromShell = state.ctx?.getActionIcon?.(name);
  if (fromShell) return fromShell;
  const path = ACTION_ICON_FALLBACK_PATHS[name] || ACTION_ICON_FALLBACK_PATHS.add;
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${path}"></path></svg>`;
}

async function loadModuleMarkup() {
  // index.html is the single source of truth for the module markup (same
  // contract as ctox/coding-agents): the shell imports ONLY index.js
  // (app.js moduleBasePath → modules/knowledge/index.js?v=<build>), so the
  // module fetches its own markup here. Markup inherits the JS cache-buster —
  // a deploy must never leave fresh JS binding against stale cached markup.
  const version = String(import.meta.url).split('?v=')[1] || '';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (version ? `?v=${version}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(root) {
  els.root = root.querySelector('[data-knowledge-root]');
  els.leftPane = root.querySelector('.knowledge-left');
  els.centerPane = root.querySelector('.knowledge-center');
  els.list = root.querySelector('[data-knowledge-list]');
  els.viewToggle = root.querySelector('[data-knowledge-view-toggle]');
  els.selectedKind = root.querySelector('[data-selected-kind]');
  els.selectedTitle = root.querySelector('[data-selected-title]');
  els.markdownView = root.querySelector('[data-markdown-view]');
  els.markdownEditor = root.querySelector('[data-markdown-editor]');
  els.skillbookSwitcher = root.querySelector('[data-skillbook-switcher]');
  els.resourceSwitcher = root.querySelector('[data-resource-switcher]');
  els.resourceView = root.querySelector('[data-resource-view]');
  els.skillStatus = root.querySelector('[data-skill-status]');
  els.tableHost = root.querySelector('[data-dataframe-host]');
  els.tableSwitcher = root.querySelector('[data-table-switcher]');
  els.tableTitle = root.querySelector('[data-table-title]');
  els.tableMeta = root.querySelector('[data-table-meta]');
  els.runbookSwitcher = root.querySelector('[data-runbook-switcher]');
  els.runbookView = root.querySelector('[data-runbook-view]');
  els.runbookList = root.querySelector('[data-runbook-list]');
  els.runbookForm = root.querySelector('[data-runbook-form]');
  els.runbookTitle = root.querySelector('[data-runbook-title]');
  els.runbookPrompt = root.querySelector('[data-runbook-prompt]');
  els.runbookStatus = root.querySelector('[data-runbook-status]');
}

function wireEvents() {
  // Pane chrome is SHELL-owned canonical grammar (autoWirePaneGrammar wires
  // the data-pg-* markup once, debounced ~120ms after mount). The module only
  // keeps its state in sync through the bubbling grammar event and re-renders
  // lists — the same contract the ctox module's task column uses. Listeners
  // hang on the pane elements (persistent), never on the rebuilt list well.
  els.leftPane?.addEventListener('ctox-pane-grammar-change', onLeftGrammarChange);
  els.centerPane?.addEventListener('ctox-pane-grammar-change', onCenterGrammarChange);
  // ONE view control, not two (Betreiber-Direktive 31.08.2026). It is an
  // action, so it flips the view instead of selecting one of two states; the
  // shell grammar cannot express that (it reads aria-pressed across several
  // [data-pg-view] buttons), hence the module wiring.
  els.viewToggle?.addEventListener('click', () => {
    state.listView = !state.listView;
    syncViewToggle();
    renderKnowledgeList({ resetScroll: true });
  });
  syncViewToggle();
  state.ctx.host.querySelector('[data-action="prev-rows"]').addEventListener('click', () => pageTable(-1));
  state.ctx.host.querySelector('[data-action="next-rows"]').addEventListener('click', () => pageTable(1));
  state.ctx.host.querySelector('[data-action="export-table-csv"]')?.addEventListener('click', exportActiveTableCsv);
  state.ctx.host.querySelector('[data-action="create-knowledge-book"]')?.addEventListener('click', () => openCreateKnowledgeBookDrawer());
  state.ctx.host.querySelector('[data-action="import-knowledge-book"]')?.addEventListener('click', () => openImportKnowledgeBookDrawer());
  state.ctx.host.querySelector('[data-action="export-knowledge-book"]')?.addEventListener('click', () => openExportKnowledgeBookDrawer());
  state.ctx.host.querySelector('[data-action="configure-knowledge"]')?.addEventListener('click', () => openKnowledgeConfig());
  // Domain-specific leftover: the sort-direction toggle inside the tray has no
  // grammar equivalent (the grammar owns filter VALUES, not direction) — same
  // pattern as ctox's data-task-sort-direction.
  state.ctx.host.querySelector('[data-action="toggle-sort-dir"]')?.addEventListener('click', (event) => {
    state.sortDir = state.sortDir === 'asc' ? 'desc' : 'asc';
    const btn = event.currentTarget;
    btn.dataset.dir = state.sortDir;
    btn.title = state.sortDir === 'asc' ? 'Aufsteigend' : 'Absteigend';
    renderKnowledgeList({ resetScroll: true });
  });
  // Kind chips are module-owned (the grammar filters are select/input values,
  // not aria-pressed chips).
  state.ctx.host.querySelectorAll('[data-pg-tray] [data-flag]').forEach((chip) => {
    chip.addEventListener('click', () => {
      const flag = chip.dataset.flag;
      if (state.flagFilters.has(flag)) state.flagFilters.delete(flag);
      else state.flagFilters.add(flag);
      chip.setAttribute('aria-pressed', String(state.flagFilters.has(flag)));
      renderKnowledgeList({ resetScroll: true });
    });
  });
  // The grammar reset restores search + select defaults and emits; the chips
  // and the sort direction are module state, so the module resets them here
  // (this listener was attached at mount, BEFORE the shell-wired grammar
  // handler, so it runs first and the grammar's emit re-renders with the
  // cleared module state).
  state.ctx.host.querySelector('[data-pg-reset]')?.addEventListener('click', () => {
    state.sortDir = 'desc';
    state.flagFilters.clear();
    const dirBtn = state.ctx.host.querySelector('[data-action="toggle-sort-dir"]');
    if (dirBtn) { dirBtn.dataset.dir = 'desc'; dirBtn.title = 'Absteigend'; }
    state.ctx.host.querySelectorAll('[data-pg-tray] [data-flag]').forEach((chip) => chip.setAttribute('aria-pressed', 'false'));
  });
  // Mobile master-detail: back to the list.
  state.ctx.host.querySelector('[data-action="mobile-back"]')?.addEventListener('click', () => {
    state.ctx.host.querySelector('[data-knowledge-root]')?.classList.remove('is-detail');
  });
  // Fractal header edit: the pencil in the pane header edits whatever the
  // active tab shows — it just triggers the edit control of that panel.
  state.ctx.host.querySelector('[data-action="edit-active"]')?.addEventListener('click', () => {
    const editBtn = state.ctx.host.querySelector('.knowledge-tab-panel:not([hidden]) [data-action="edit-markdown"], .knowledge-tab-panel:not([hidden]) [data-action="edit-runbook"]');
    if (editBtn) editBtn.click();
  });
  state.ctx.host.querySelector('[data-action="edit-markdown"]')?.addEventListener('click', toggleMarkdownEditor);
  state.ctx.host.querySelector('[data-action="save-markdown"]')?.addEventListener('click', queueMarkdownSave);
  state.ctx.host.querySelector('[data-action="cancel-markdown"]')?.addEventListener('click', cancelMarkdownEdit);
  state.ctx.host.querySelector('[data-action="delete-skill"]')?.addEventListener('click', () => {
    const item = state.items.find((entry) => entry.id === state.selectedId);
    if (item) deleteKnowledgeEntry(item, activeGroup());
  });
  state.ctx.host.querySelector('[data-action="delete-runbook"]')?.addEventListener('click', () => {
    const runbook = state.runbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId));
    if (runbook) deleteKnowledgeEntry(runbook, activeGroup());
  });
  state.ctx.host.querySelector('[data-action="edit-runbook"]')?.addEventListener('click', startRunbookEdit);
  state.ctx.host.querySelector('[data-action="save-runbook"]')?.addEventListener('click', queueRunbookSave);
  state.ctx.host.querySelector('[data-action="cancel-runbook"]')?.addEventListener('click', cancelRunbookEdit);
  state.ctx.host.querySelector('[data-action="execute-runbook"]')?.addEventListener('click', executeRunbook);
  els.runbookForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    queueRunbookSave();
  });
}

// The icon names the view the operator switches TO — an action label, never a
// state indicator, so the button carries no aria-pressed.
const VIEW_TOGGLE_ICONS = Object.freeze({
  // switch-to-list: three rules
  list: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>',
  // switch-to-cards: two shards
  cards: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>',
});

function syncViewToggle() {
  const view = state.listView ? 'list' : 'cards';
  els.root?.classList.toggle('is-list-view', state.listView);
  // shared/pane-grammar.js falls back to the pane's default view when no
  // [data-pg-view] button exists. Keep that fallback truthful so the
  // grammar-change event still reports the view the operator actually sees.
  if (els.leftPane) els.leftPane.dataset.pgDefaultView = view;
  const button = els.viewToggle;
  if (!button) return;
  const next = state.listView ? 'cards' : 'list';
  const copy = state.messages || labels[state.lang];
  const label = next === 'list'
    ? (copy.showAsList || labels.de.showAsList)
    : (copy.showAsCards || labels.de.showAsCards);
  button.dataset.knowledgeView = view;
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  button.removeAttribute('aria-pressed');
  const icon = VIEW_TOGGLE_ICONS[next];
  if (icon && button.innerHTML !== icon) button.innerHTML = icon;
}

function onLeftGrammarChange(event) {
  const detail = event?.detail || {};
  state.searchTerm = String(detail.search ?? '');
  // The view belongs to the module-wired toggle, not to the grammar: the
  // grammar would only echo back the data-pg-default-view we just wrote.
  state.sourceScope = ['system', 'user', 'all'].includes(detail.filters?.scope) ? detail.filters.scope : 'all';
  state.sortMode = detail.filters?.sort || 'recent';
  els.root?.classList.toggle('is-list-view', state.listView);
  // Intentional reset: search/view/filter changes move the content set, so the
  // list scrolls back to the top (the shell scroll guard also clears its
  // recorded offsets on this event).
  renderKnowledgeList({ resetScroll: true });
}

function onCenterGrammarChange(event) {
  const detail = event?.detail || {};
  if (detail.band) setTab(detail.band);
}

async function loadKnowledgeFromLocal(options = {}) {
  return runCoalescedRefresh(state, () => refreshKnowledgeFromLocal(options));
}

async function runCoalescedRefresh(status, refresh) {
  if (status.refreshInFlight) {
    status.refreshPending = true;
    return;
  }
  status.refreshInFlight = true;
  try {
    await refresh();
  } finally {
    status.refreshInFlight = false;
    if (status.refreshPending) {
      status.refreshPending = false;
      await runCoalescedRefresh(status, refresh);
    }
  }
}

async function refreshKnowledgeFromLocal(options = {}) {
  state.loadError = '';
  state.missingCollections = [];
  // Local-first: render whatever is in IndexedDB RIGHT NOW. Never block the
  // first paint on a sync round trip — `wireLocalRealtime()` subscribes to
  // the knowledge collections and re-renders the moment replicated records
  // land, and the sync toast surfaces "data still loading". The old initial
  // path awaited a sync warm-up AND polled up to 9s for records before
  // showing anything, which made every Knowledge open feel frozen.
  if (options.initial) {
    // Kick sync off in the background; do NOT await it.
    ensureKnowledgeDataSyncStarted().catch(() => {});
  }
  const snapshot = await readLocalKnowledgeSnapshot();
  state.loadError = snapshot.error || '';
  state.missingCollections = snapshot.missingCollections || [];
  applyKnowledgeRecords(snapshot);
  renderKnowledgeList();
  renderRunbooks();
  if (state.selectedId) await selectKnowledge(state.selectedId);
  else renderEmptyKnowledgeSelection();
  if (state.items.length || state.runbooks.length || state.tables.length) {
    cancelInitialKnowledgeRetry();
    state.initialRetryAttempt = 0;
  } else if (options.initial || options.initialRetry) {
    scheduleInitialKnowledgeRetry(state.ctx);
  }
}

function scheduleInitialKnowledgeRetry(ctx) {
  if (!ctx || state.initialRetryTimer || state.initialRetryAttempt >= KNOWLEDGE_INITIAL_RETRY_DELAYS_MS.length) return;
  const delayMs = KNOWLEDGE_INITIAL_RETRY_DELAYS_MS[state.initialRetryAttempt];
  state.initialRetryAttempt += 1;
  state.initialRetryTimer = window.setTimeout(() => {
    state.initialRetryTimer = null;
    if (state.ctx !== ctx) return;
    loadKnowledgeFromLocal({ initialRetry: true }).catch((error) => {
      if (state.ctx !== ctx) return;
      state.loadError = error?.message || String(error);
      renderKnowledgeList();
    });
  }, delayMs);
}

function cancelInitialKnowledgeRetry() {
  if (state.initialRetryTimer) window.clearTimeout(state.initialRetryTimer);
  state.initialRetryTimer = null;
}

function applyKnowledgeRecords({ items = [], runbooks = [], tables = [] }) {
  const normalizedItems = Array.isArray(items) ? items.map(normalizeStoredKnowledgeRecord).filter(isActiveKnowledgeRecord) : [];
  const normalizedRunbooks = Array.isArray(runbooks) ? runbooks.map(normalizeStoredKnowledgeRecord).filter(isActiveKnowledgeRecord) : [];
  const normalizedTables = mergeKnowledgeTableChunks(
    Array.isArray(tables) ? tables.map(normalizeStoredKnowledgeRecord).filter(isActiveKnowledgeRecord) : [],
  );
  state.tables = normalizedTables;
  state.items = uniqueById([
    ...normalizedItems,
    ...knowledgeItemsFromTables(normalizedTables, normalizedItems),
  ]);
  state.runbooks = normalizedRunbooks;
  state.groups = buildKnowledgeBundles(state.items, state.runbooks, state.tables);
  const requestedId = sessionStorage.getItem(KNOWLEDGE_OPEN_TARGET_KEY) || '';
  if (requestedId && state.items.some((item) => item.id === requestedId)) {
    sessionStorage.removeItem(KNOWLEDGE_OPEN_TARGET_KEY);
    state.selectedId = requestedId;
    state.activeTab = requestedId.startsWith('runbook:') ? 'runbooks' : state.activeTab;
    const group = findGroupForItem(requestedId);
    if (group) {
      state.selectedGroupId = group.id;
      state.openGroups.add(group.id);
    }
  }
  const requestedDomain = sessionStorage.getItem(KNOWLEDGE_OPEN_DOMAIN_KEY) || '';
  const requestedGroup = requestedDomain
    ? state.groups.find((group) => knowledgeGroupMatchesDomain(group, requestedDomain))
    : null;
  if (requestedGroup) {
    sessionStorage.removeItem(KNOWLEDGE_OPEN_DOMAIN_KEY);
    state.selectedGroupId = requestedGroup.id;
    state.openGroups.add(requestedGroup.id);
    state.selectedSkillbookId = firstSkillbookForGroup(requestedGroup)?.id || '';
    const context = skillbookContext(requestedGroup, state.selectedSkillbookId);
    state.selectedId = context.skill?.id || requestedGroup.primaryItemId || requestedGroup.entries[0]?.id || '';
    state.selectedTableId = context.tables[0]?.id || requestedGroup.tableIds?.[0] || '';
    state.selectedRunbookId = normaliseRunbookId(context.runbooks[0]?.id || context.runbookItems[0]?.id || '');
    return;
  }
  const selectedStillExists = state.items.some((item) => item.id === state.selectedId);
  if (selectedStillExists) return;
  const firstGroup = state.groups[0];
  state.selectedGroupId = firstGroup?.id || '';
  if (state.selectedGroupId) state.openGroups.add(state.selectedGroupId);
  state.selectedSkillbookId = firstSkillbookForGroup(firstGroup)?.id || '';
  const firstContext = skillbookContext(firstGroup, state.selectedSkillbookId);
  state.selectedId = firstContext.skill?.id || firstGroup?.primaryItemId || state.items[0]?.id || '';
  state.selectedTableId = firstContext.tables[0]?.id || firstGroup?.tableIds?.[0] || '';
  state.selectedRunbookId = normaliseRunbookId(firstContext.runbooks[0]?.id || firstContext.runbooks[0]?.runbook_id || state.runbooks[0]?.id || '');
}

function knowledgeGroupMatchesDomain(group, domain) {
  const target = normaliseName(domain);
  if (!target || !group) return false;
  const candidates = [
    group.domain,
    group.id,
    ...group.entries.flatMap((entry) => [entry.domain, entry.knowledge_domain, entry.payload?.domain, entry.payload?.knowledge_domain]),
  ];
  return candidates.some((value) => {
    const normalized = normaliseName(value || '');
    return Boolean(normalized) && (normalized === target || normalized.includes(target) || target.includes(normalized));
  });
}

async function readLocalKnowledgeSnapshot() {
  const missingCollections = [];
  const results = await Promise.allSettled(KNOWLEDGE_DATA_COLLECTIONS.map((collectionName) => (
    loadLocalKnowledgeRecords(collectionName, missingCollections)
  )));
  const [items = [], runbooks = [], tables = []] = results.map((result) => (
    result.status === 'fulfilled' ? result.value : []
  ));
  const error = results
    .filter((result) => result.status === 'rejected')
    .map((result) => result.reason?.message || String(result.reason))
    .join('; ');
  return { items, runbooks, tables, missingCollections, error };
}

async function loadLocalKnowledgeRecords(collectionName, missingCollections = state.missingCollections) {
  const collection = knowledgeCollection(collectionName);
  if (!collection) {
    missingCollections.push(collectionName);
    return [];
  }
  const docs = await collection.find().exec();
  return sortKnowledgeRecords(docs
    .map((doc) => normalizeStoredKnowledgeRecord(doc.toJSON()))
    .filter(isActiveKnowledgeRecord));
}

function sortKnowledgeRecords(records) {
  return [...records].sort((left, right) => (
    Number(right?.updated_at_ms || 0) - Number(left?.updated_at_ms || 0)
      || String(left?.id || '').localeCompare(String(right?.id || ''))
  ));
}

function normalizeStoredKnowledgeRecord(record) {
  if (!record || typeof record !== 'object') return record;
  const payload = record.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : null;
  if (!payload) return { ...record };
  return {
    ...payload,
    ...record,
    id: record.id || payload.id || record.table_id || payload.table_id || '',
    kind: record.kind || payload.kind || '',
    title: record.title || payload.title || '',
    subtitle: record.subtitle || payload.subtitle || '',
    summary: record.summary || record.description || payload.summary || payload.description || '',
    source_path: record.source_path || payload.source_path || payload.parquet_path || '',
    updated_at: record.updated_at || payload.updated_at || '',
    updated_at_ms: Number(record.updated_at_ms ?? payload.updated_at_ms ?? 0),
    payload,
  };
}

function isActiveKnowledgeRecord(record) {
  return Boolean(record?.id && !record._deleted && !record.is_deleted);
}

async function ensureKnowledgeDataSyncStarted() {
  // The shell owns a scoped module lease for every declared collection before
  // mount. Starting collections here would pin them as permanent shell
  // consumers and leak their bridges after the Knowledge window closes.
  // Reactive queries below drive foreground priority through the shared active
  // collection registry; they do not need a second app-owned warm-up.
}

function promiseWithTimeout(promise, timeoutMs) {
  return Promise.race([
    promise,
    new Promise((resolve) => window.setTimeout(resolve, timeoutMs)),
  ]);
}

function delay(ms) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function wireLocalRealtime() {
  const collections = ['knowledge_items', 'knowledge_runbooks', 'knowledge_tables'];
  let timer = null;
  const schedule = () => {
    if (timer) return;
    timer = window.setTimeout(() => {
      timer = null;
      loadKnowledgeFromLocal().catch((error) => {
        console.warn('[knowledge] local realtime render failed', error);
      });
    }, KNOWLEDGE_RENDER_DEBOUNCE_MS);
  };
  const subscriptions = collections
    .map((collectionName) => knowledgeCollection(collectionName)?.$?.subscribe?.(schedule) || null)
    .filter(Boolean);
  return () => {
    if (timer) window.clearTimeout(timer);
    timer = null;
    for (const sub of subscriptions) {
      try { sub.unsubscribe?.(); } catch {}
    }
  };
}

function knowledgeCollection(collectionName) {
  return state.ctx?.db?.collection?.(collectionName) || null;
}

// Canonical collection readiness (shell sync API): while a replicated
// collection has not finished its initial sync, an empty local result is a
// SYNC state, not a "no data" state. Subscribing emits an immediate snapshot
// and re-emits only on state changes; each emission re-renders the lists that
// draw from that collection so the syncing shell flips to data/empty live.
function wireCollectionReadiness() {
  const sync = state.ctx?.sync;
  const subscribe = sync?.subscribeCollectionReadiness;
  if (typeof subscribe !== 'function') return () => {};
  const unsubscribes = KNOWLEDGE_DATA_COLLECTIONS.map((collectionName) => {
    const unsubscribe = subscribe.call(sync, collectionName, (snapshot) => {
      state.collectionReadiness[collectionName] = snapshot || null;
      renderKnowledgeList();
      renderRunbooks();
      if (state.activeTab === 'runbooks') {
        renderRunbookWorkspace().catch((error) => console.warn('[knowledge] readiness re-render failed', error));
      }
    });
    return typeof unsubscribe === 'function' ? unsubscribe : () => {};
  });
  return () => {
    for (const unsubscribe of unsubscribes) {
      try { unsubscribe(); } catch {}
    }
  };
}

function knowledgeReadinessSnapshot(collectionName) {
  const snapshot = state.collectionReadiness?.[collectionName];
  if (snapshot) return snapshot;
  const read = state.ctx?.sync?.collectionReadiness;
  return typeof read === 'function' ? read.call(state.ctx.sync, collectionName) : null;
}

// The knowledge list bundles records from ALL data collections, so its
// data-driven empty branch is a sync state until every source is live.
// Missing snapshots fail open (unknown → no syncing shell): readiness is a
// render hint, never a mount blocker.
function knowledgeSourceReadiness() {
  const snapshots = KNOWLEDGE_DATA_COLLECTIONS.map(knowledgeReadinessSnapshot).filter(Boolean);
  if (!snapshots.length) return null;
  if (snapshots.some((snapshot) => snapshot.ready === false)) return { ready: false };
  if (snapshots.every((snapshot) => snapshot.ready === true)) return { ready: true };
  return null;
}

// Rows always win; the .ctox-syncing shell appears only for a data-driven
// empty (the unfiltered replicated source is empty) whose collection is not
// ready yet — selection/filter/permission empties stay .ctox-empty.
function knowledgeListStateHtml({ dataDriven, readiness, message, syncingText }) {
  if (dataDriven && readiness?.ready === false) {
    return `<div class="ctox-syncing" role="status" aria-live="polite"><strong>${escapeHtml(syncingText)}</strong></div>`;
  }
  return `<div class="ctox-empty"><strong>${escapeHtml(message)}</strong></div>`;
}

function renderEmptyKnowledgeSelection() {
  const copy = state.messages || labels[state.lang];
  state.selectedId = '';
  state.selectedGroupId = '';
  state.selectedSkillbookId = '';
  state.selectedTableId = '';
  state.selectedRunbookId = '';
  state.editing = false;
  state.activeTab = 'skill';
  // Right-column header is the detail-view, not a second category label.
  if (els.selectedKind) els.selectedKind.textContent = copy.selected || 'Selected';
  els.selectedTitle.textContent = copy.noSelection || (state.lang === 'en' ? 'No entry selected' : 'Kein Eintrag ausgewählt');
  els.markdownEditor.hidden = true;
  els.markdownView.hidden = false;
  els.markdownEditor.value = '';
  // The list-pane on the left already shows the "no entries" empty-state.
  // Do not repeat it inside the detail-view; show a brief hint instead.
  els.markdownView.innerHTML = `<p>${escapeHtml(copy.detailEmptyHint || (state.lang === 'en' ? 'Pick a knowledge entry on the left to view it here.' : 'Wähle links einen Knowledge-Eintrag, um ihn hier anzuzeigen.'))}</p>`;
  els.tableHost.innerHTML = `<div class="ctox-empty"><strong>${escapeHtml(copy.tableUnavailable)}</strong></div>`;
  if (els.runbookSwitcher) els.runbookSwitcher.innerHTML = '';
  if (els.runbookView) els.runbookView.innerHTML = `<div class="ctox-empty"><strong>${escapeHtml(copy.noRunbooks)}</strong></div>`;
  syncMarkdownEditControls();
  syncRunbookEditControls(false);
  syncKnowledgeTabControls();
}

async function loadKnowledgeDocument(id) {
  const item = state.items.find((entry) => entry.id === id);
  const localMarkdown = localMarkdownForItem(item);
  if (localMarkdown) return { markdown: localMarkdown, source: 'local' };
  return {
    markdown: `# ${item?.title || 'Knowledge'}\n\n${item?.summary || item?.description || item?.subtitle || ''}`,
    source: 'local-summary',
  };
}

function buildKnowledgeBundles(items, runbooks, tables) {
  const allItems = uniqueById([
    ...(Array.isArray(items) ? items : []),
    ...knowledgeItemsFromTables(tables, items),
  ]);
  const runbookItems = allItems.filter((item) => item.kind === 'runbook');
  const tableItems = allItems.filter((item) => item.kind === 'dataframe');
  const skillbookItems = allItems.filter((item) => item.kind === 'skillbook');
  const skillItems = allItems.filter((item) => item.kind === 'skill');
  const resourceItems = allItems.filter((item) => item.kind === 'resource');
  const externalRunbooks = uniqueById((Array.isArray(runbooks) ? runbooks : []).map((runbook) => ({
    ...runbook,
    id: runbook?.id || runbook?.runbook_id || '',
  })));
  const used = new Set();
  const assignedExternalRunbookIds = new Set();

  const makeGroup = (config) => {
    const entries = uniqueById(config.entries || []).filter(Boolean);
    for (const entry of entries) used.add(entry.id);
    const tableIds = entries.filter((entry) => entry.has_table).map((entry) => entry.id);
    const linkedRunbookIds = entries.flatMap((entry) => extractRunbookIds(entry?.linked_runbook_ids ?? entry?.linked_runbooks_json ?? entry?.linked_runbooks));
    const runbookIds = uniqueStrings([
      ...(config.runbookIds || []),
      ...linkedRunbookIds,
      ...entries.filter((entry) => entry.kind === 'runbook').map(runbookIdForItem),
    ].map(normaliseRunbookId));
    return {
      id: config.id,
      title: config.title,
      domainLabel: config.domainLabel,
      domain: config.domain,
      summary: config.summary || '',
      entries,
      primaryItemId: config.primaryItemId || entries.find((entry) => entry.kind === 'skillbook')?.id || entries[0]?.id || '',
      tableIds,
      runbookIds,
    };
  };

  const droneRunbooks = runbookItems.filter(isDroneBearingKnowledge);
  const droneRunbookIds = new Set(droneRunbooks.map((entry) => normaliseRunbookId(runbookIdForItem(entry))));
  const droneSkillbooks = skillbookItems.filter((skillbook) => {
    if (isDroneBearingKnowledge(skillbook)) return true;
    if (bareKnowledgeId(skillbook.id) === 'drone-bearing-design-verified-v1') return true;
    return extractRunbookIds(skillbook.linked_runbook_ids ?? skillbook.linked_runbooks_json ?? skillbook.linked_runbooks)
      .map(normaliseRunbookId)
      .some((id) => droneRunbookIds.has(id));
  });
  const droneSkillbookIds = new Set(droneSkillbooks.map((entry) => bareKnowledgeId(entry.id)));
  const linkedDroneRunbookIds = new Set(droneSkillbooks.flatMap((skillbook) => (
    extractRunbookIds(skillbook.linked_runbook_ids ?? skillbook.linked_runbooks_json ?? skillbook.linked_runbooks)
      .map(normaliseRunbookId)
  )));
  const linkedDroneRunbooks = runbookItems.filter((entry) => (
    droneRunbookIds.has(normaliseRunbookId(runbookIdForItem(entry)))
    || linkedDroneRunbookIds.has(normaliseRunbookId(runbookIdForItem(entry)))
    || droneSkillbookIds.has(bareKnowledgeId(entry.skillbook_id))
  ));
  const externalDroneRunbooks = externalRunbooks.filter(isDroneBearingKnowledge);
  for (const runbook of externalDroneRunbooks) {
    assignedExternalRunbookIds.add(normaliseRunbookId(runbookIdForItem(runbook)));
  }
  const droneEntries = uniqueById([
    ...skillItems.filter((entry) => isDroneBearingKnowledge(entry) || droneSkillbookIds.has(bareKnowledgeId(entry.skillbook_id))),
    ...droneSkillbooks,
    ...linkedDroneRunbooks,
    ...resourceItems.filter((entry) => isDroneBearingKnowledge(entry) || droneSkillbookIds.has(bareKnowledgeId(entry.skillbook_id))),
    ...tableItems.filter((item) => {
      const table = tableForItem(item, tables);
      const domain = table?.domain || item.domain || item.payload?.domain || '';
      return isDroneBearingKnowledge(item)
        || isDroneBearingTable(table)
        || domain === 'drone_bearing_design_verified';
    }),
  ]);
  const groups = [];
  if (droneEntries.length) {
    groups.push(makeGroup({
      id: 'research/drone-design/drone-bearing-loads',
      title: 'Drone Bearing Loads',
      domainLabel: 'Research / Drone Design',
      domain: 'drone_bearing_design_verified',
      summary: 'Skill, Skillbook, Runbook und DataFrames für Drone-Bearing-Load-Recherche.',
      entries: droneEntries,
      runbookIds: uniqueStrings([
        ...linkedDroneRunbooks.map(runbookIdForItem),
        ...externalDroneRunbooks.map(runbookIdForItem),
      ]),
      primaryItemId: droneEntries.find((entry) => entry.kind === 'skillbook')?.id || droneEntries[0]?.id,
    }));
  }

  const externalRunbookOwners = new Map();
  const ordinarySkillbooks = skillbookItems.filter((skillbook) => !used.has(skillbook.id));
  for (const runbook of externalRunbooks) {
    const runbookId = normaliseRunbookId(runbookIdForItem(runbook));
    if (!runbookId || assignedExternalRunbookIds.has(runbookId)) continue;
    let bestOwner = null;
    let bestScore = 0;
    for (const skillbook of ordinarySkillbooks) {
      const linkedIds = new Set(extractRunbookIds(skillbook.linked_runbook_ids ?? skillbook.linked_runbooks_json ?? skillbook.linked_runbooks).map(normaliseRunbookId));
      const score = runbookSkillbookMatchScore(skillbook, runbook, linkedIds);
      if (score > bestScore) {
        bestOwner = skillbook.id;
        bestScore = score;
      }
    }
    if (bestOwner) externalRunbookOwners.set(runbookId, bestOwner);
  }

  for (const skillbook of skillbookItems) {
    if (used.has(skillbook.id)) continue;
    const base = normaliseName(bareId(skillbook.id).replace(/-skillbook$/, ''));
    const linkedRunbooks = new Set(extractRunbookIds(skillbook.linked_runbook_ids ?? skillbook.linked_runbooks_json ?? skillbook.linked_runbooks).map(normaliseRunbookId));
    const relatedRunbooks = runbookItems.filter((item) => {
      const itemId = normaliseRunbookId(runbookIdForItem(item));
      return linkedRunbooks.has(itemId) || item.skillbook_id === bareKnowledgeId(skillbook.id) || item.subtitle?.toLowerCase().includes(base.replaceAll('-', '_')) || tokenOverlap(skillbook, item) >= 2;
    });
    const relatedTables = tableItems.filter((item) => tokenOverlap(skillbook, item) >= 2);
    const relatedSkills = skillItems.filter((item) => tokenOverlap(skillbook, item) >= 2);
    const relatedResources = resourceItems.filter((item) => item.skillbook_id === bareKnowledgeId(skillbook.id));
    const relatedExternalRunbooks = externalRunbooks.filter((runbook) => {
      const id = normaliseRunbookId(runbookIdForItem(runbook));
      return externalRunbookOwners.get(id) === skillbook.id;
    });
    groups.push(makeGroup({
      id: `bundle/${base}`,
      title: skillbook.title || titleFromSlug(base),
      domainLabel: domainLabelFor(skillbook),
      domain: base,
      summary: skillbook.summary || '',
      entries: [skillbook, ...relatedSkills, ...relatedRunbooks, ...relatedResources, ...relatedTables],
      runbookIds: relatedExternalRunbooks.map(runbookIdForItem),
      primaryItemId: skillbook.id,
    }));
  }

  const remainingTablesByDomain = groupBy(tableItems.filter((item) => !used.has(item.id)), (item) => (
    tableForItem(item, tables)?.domain || item.domain || item.payload?.domain || 'tables'
  ));
  for (const [domain, domainTables] of Object.entries(remainingTablesByDomain)) {
    groups.push(makeGroup({
      id: `tables/${domain}`,
      title: titleFromSlug(domain),
      domainLabel: 'DataFrames',
      domain,
      entries: domainTables,
      primaryItemId: domainTables[0]?.id,
    }));
  }

  const remainingSkillsByPath = groupBy(skillItems.filter((item) => !used.has(item.id)), (item) => domainKeyFor(item));
  for (const [key, entries] of Object.entries(remainingSkillsByPath)) {
    groups.push(makeGroup({
      id: `skills/${key}`,
      title: titleFromSlug(key),
      domainLabel: 'Skills',
      domain: key,
      entries,
      primaryItemId: entries[0]?.id,
    }));
  }

  const remainingByDomain = groupBy(allItems.filter((item) => !used.has(item.id)), (item) => domainKeyFor(item));
  for (const [key, entries] of Object.entries(remainingByDomain)) {
    groups.push(makeGroup({
      id: `knowledge/${key}`,
      title: titleFromSlug(key),
      domainLabel: domainLabelFor(entries[0]),
      domain: key,
      entries,
      primaryItemId: entries.find((entry) => ['skillbook', 'skill'].includes(entry.kind))?.id || entries[0]?.id,
    }));
  }

  // Same-titled groups (e.g. a user and a system copy of the same skillbook)
  // must render as ONE shard — merge entries and id-lists instead of showing
  // duplicate cards.
  const byTitle = new Map();
  for (const group of groups.filter((entry) => entry.entries.length)) {
    const key = (group.title || group.id).trim().toLowerCase();
    const existing = byTitle.get(key);
    if (!existing) {
      byTitle.set(key, group);
      continue;
    }
    existing.entries = uniqueById([...existing.entries, ...group.entries]);
    existing.tableIds = uniqueStrings([...existing.tableIds, ...group.tableIds]);
    existing.runbookIds = uniqueStrings([...existing.runbookIds, ...group.runbookIds]);
    existing.summary = existing.summary || group.summary;
  }
  return [...byTitle.values()].sort((a, b) => {
    if (a.id.startsWith('research/drone-design')) return -1;
    if (b.id.startsWith('research/drone-design')) return 1;
    return a.title.localeCompare(b.title);
  });
}

function knowledgeItemsFromTables(tables = [], existingItems = []) {
  const existingIds = new Set((Array.isArray(existingItems) ? existingItems : []).map((item) => item?.id).filter(Boolean));
  return (Array.isArray(tables) ? tables : [])
    .map(knowledgeItemFromTable)
    .filter((item) => item?.id && !existingIds.has(item.id));
}

function mergeKnowledgeTableChunks(tables = []) {
  const groups = new Map();
  for (const rawTable of Array.isArray(tables) ? tables : []) {
    if (!rawTable || typeof rawTable !== 'object') continue;
    const source = rawTable.payload && typeof rawTable.payload === 'object' && !Array.isArray(rawTable.payload)
      ? rawTable.payload
      : rawTable;
    const logicalId = String(
      source.logical_table_id
      || rawTable.logical_table_id
      || source.id
      || rawTable.id
      || '',
    ).trim();
    if (!logicalId) continue;
    if (!groups.has(logicalId)) groups.set(logicalId, []);
    groups.get(logicalId).push({ rawTable, source });
  }

  return [...groups.entries()].map(([logicalId, parts]) => {
    parts.sort((left, right) => (
      Number(left.source.chunk_index ?? left.rawTable.chunk_index ?? 0)
      - Number(right.source.chunk_index ?? right.rawTable.chunk_index ?? 0)
    ));
    const first = parts[0];
    const rows = parts.flatMap(({ rawTable, source }) => firstArray(
      source.rows,
      source.records,
      source.data,
      rawTable.rows,
      rawTable.records,
      rawTable.data,
    ));
    const expectedChunks = Math.max(...parts.map(({ rawTable, source }) => (
      Number(source.chunk_count ?? rawTable.chunk_count ?? 1)
    )).filter(Number.isFinite), 1);
    const rowsComplete = parts.length === expectedChunks && parts.every(({ rawTable, source }) => (
      (source.rows_complete ?? rawTable.rows_complete ?? true) !== false
    ));
    const payload = {
      ...first.source,
      id: logicalId,
      logical_table_id: logicalId,
      chunk_index: 0,
      chunk_count: 1,
      source_chunk_count: expectedChunks,
      chunk_row_offset: 0,
      row_offset: 0,
      chunk_row_count: rows.length,
      row_count: rows.length,
      projected_row_count: rows.length,
      rows_total: rows.length,
      rows_complete: rowsComplete,
      rows,
    };
    return {
      ...first.rawTable,
      ...payload,
      payload,
    };
  });
}

function knowledgeItemFromTable(table) {
  if (!table || typeof table !== 'object') return null;
  const payload = table.payload && typeof table.payload === 'object' && !Array.isArray(table.payload)
    ? table.payload
    : {};
  const id = table.id || payload.id || table.table_id || payload.table_id || '';
  if (!id) return null;
  const rowCount = Number(table.row_count ?? payload.row_count ?? localDataFrameRows({ ...payload, ...table }).length);
  return {
    ...payload,
    ...table,
    id,
    kind: 'dataframe',
    title: table.title || payload.title || titleFromSlug(bareId(id)),
    subtitle: table.subtitle || payload.subtitle || 'Runtime DataFrame',
    summary: table.summary || table.description || payload.summary || payload.description || '',
    source_path: table.source_path || payload.source_path || payload.parquet_path || '',
    has_table: table.has_table ?? payload.has_table ?? true,
    row_count: Number.isFinite(rowCount) ? rowCount : null,
    payload,
  };
}

function findGroupForItem(id) {
  return state.groups.find((group) => group.entries.some((entry) => entry.id === id) || group.tableIds.includes(id) || group.runbookIds.includes(id));
}

function tableForItem(item, tables) {
  const tableId = bareId(item?.id || '');
  return tables.find((table) => bareId(table.id || table.table_id || '') === tableId || table.id === item?.id);
}

function mergeKnowledgeTableData(item, table) {
  if (!item && !table) return null;
  const tablePayload = table?.payload && typeof table.payload === 'object' && !Array.isArray(table.payload)
    ? table.payload
    : {};
  return {
    ...tablePayload,
    ...(table || {}),
    ...(item || {}),
    payload: {
      ...tablePayload,
      ...(item?.payload && typeof item.payload === 'object' && !Array.isArray(item.payload) ? item.payload : {}),
    },
    has_table: Boolean(item?.has_table || table?.has_table || tablePayload.has_table || table),
  };
}

function isDroneBearingTable(table) {
  if (!table) return false;
  const haystack = `${table.domain || ''} ${table.table_key || ''} ${table.title || ''} ${table.description || ''}`.toLowerCase();
  return (haystack.includes('drone') || haystack.includes('uas') || haystack.includes('aerospace')) && haystack.includes('bearing');
}

function isDroneBearingKnowledge(entry) {
  const haystack = `${entry?.id || ''} ${entry?.title || ''} ${entry?.subtitle || ''} ${entry?.summary || ''} ${entry?.description || ''} ${entry?.problem_domain || ''}`.toLowerCase();
  return (haystack.includes('drone') || haystack.includes('uas') || haystack.includes('aerospace')) && haystack.includes('bearing');
}

function tokenOverlap(left, right) {
  const a = new Set(tokensFor(left));
  const b = new Set(tokensFor(right));
  let count = 0;
  for (const token of a) if (b.has(token)) count += 1;
  return count;
}

function tokensFor(value) {
  return `${value?.id || ''} ${value?.title || ''} ${value?.subtitle || ''} ${value?.summary || ''} ${value?.description || ''}`
    .toLowerCase()
    .split(/[^a-z0-9]+/g)
    .filter((token) => token.length > 2 && !['skill', 'book', 'runbook', 'dataframe', 'table'].includes(token));
}

function uniqueById(items) {
  const seen = new Set();
  return items.filter((item) => {
    if (!item?.id || seen.has(item.id)) return false;
    seen.add(item.id);
    return true;
  });
}

function uniqueStrings(values) {
  return [...new Set(values.filter(Boolean))];
}

function extractRunbookIds(value) {
  if (!value) return [];
  if (Array.isArray(value)) return value;
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return [];
    try {
      const parsed = JSON.parse(trimmed);
      if (Array.isArray(parsed)) return parsed;
    } catch (_) {
      // Fall through to comma-separated handling for legacy payloads.
    }
    return trimmed.split(/[\s,]+/g).filter(Boolean);
  }
  return [];
}

function bareKnowledgeId(id) {
  let value = String(id || '');
  while (/^[a-z]+:/.test(value)) value = value.replace(/^[a-z]+:/, '');
  return value;
}

function normaliseRunbookId(id) {
  const bare = bareKnowledgeId(id);
  return bare ? `runbook:${bare}` : '';
}

function runbookIdMatches(left, right) {
  return normaliseRunbookId(left) === normaliseRunbookId(right);
}

function runbookIdForItem(item) {
  return item?.runbook_id
    || item?.payload?.runbook_id
    || item?.runbookId
    || item?.payload?.runbookId
    || item?.id
    || '';
}

function associationValues(item, fields) {
  const payload = item?.payload && typeof item.payload === 'object' && !Array.isArray(item.payload)
    ? item.payload
    : {};
  return uniqueStrings(fields.flatMap((field) => {
    const value = item?.[field] ?? payload[field];
    return Array.isArray(value) ? value : [value];
  }).map((value) => String(value || '').trim()).filter(Boolean));
}

function associationPath(value) {
  const parts = String(value || '')
    .replace(/^[a-z]+:/i, '')
    .replaceAll('\\', '/')
    .split('/')
    .filter(Boolean);
  if (!parts.length) return '';
  const branchIndex = parts.findIndex((part) => ['runbooks', 'resources', 'sources'].includes(part.toLowerCase()));
  if (branchIndex > 0) parts.splice(branchIndex);
  else if (/\.[a-z0-9]+$/i.test(parts[parts.length - 1])) parts.pop();
  return normaliseName(parts.join('/'));
}

function runbookSkillbookMatchScore(skillbook, runbook, linkedRunbookIds = new Set()) {
  const runbookId = normaliseRunbookId(runbookIdForItem(runbook));
  if (linkedRunbookIds.has(runbookId)) return 4;

  const skillbookIds = new Set(associationValues(skillbook, ['id', 'skillbook_id', 'skillbookId'])
    .map(bareKnowledgeId)
    .map(normaliseName)
    .filter(Boolean));
  const runbookSkillbookIds = associationValues(runbook, ['skillbook_id', 'skillbookId', 'knowledge_skillbook_id'])
    .map(bareKnowledgeId)
    .map(normaliseName)
    .filter(Boolean);
  if (runbookSkillbookIds.some((id) => skillbookIds.has(id))) return 3;

  const skillbookDomains = new Set(associationValues(skillbook, ['domain', 'problem_domain', 'knowledge_domain'])
    .map(normaliseName)
    .filter(Boolean));
  const runbookDomains = associationValues(runbook, ['domain', 'problem_domain', 'knowledge_domain'])
    .map(normaliseName)
    .filter(Boolean);
  if (runbookDomains.some((domain) => skillbookDomains.has(domain))) return 2;

  const skillbookPaths = new Set(associationValues(skillbook, ['source_path', 'path', 'skillbook_path'])
    .map(associationPath)
    .filter(Boolean));
  const runbookPaths = associationValues(runbook, ['source_path', 'path', 'skillbook_path'])
    .map(associationPath)
    .filter(Boolean);
  return runbookPaths.some((path) => skillbookPaths.has(path)) ? 1 : 0;
}

function bareId(id) {
  return String(id || '').replace(/^[^:]+:/, '');
}

function normaliseName(value) {
  return String(value || '').trim().toLowerCase().replace(/_/g, '-').replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}

function titleFromSlug(value) {
  return String(value || 'Knowledge')
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function domainKeyFor(item) {
  const subtitle = String(item.subtitle || '').split('·').map((part) => part.trim()).filter(Boolean);
  return normaliseName(subtitle[subtitle.length - 1] || item.kind || 'knowledge');
}

function domainLabelFor(item) {
  const subtitle = String(item.subtitle || '').split('·').map((part) => part.trim()).filter(Boolean);
  return subtitle.length ? subtitle.join(' / ') : groupLabel(item.kind || 'knowledge');
}

function clampNumber(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function skillbooksForGroup(group) {
  if (!group) return [];
  return group.entries.filter((entry) => entry.kind === 'skillbook');
}

function firstSkillbookForGroup(group) {
  return skillbooksForGroup(group)[0] || null;
}

function selectedSkillbookForGroup(group) {
  if (!group) return null;
  return skillbooksForGroup(group).find((entry) => entry.id === state.selectedSkillbookId) || firstSkillbookForGroup(group);
}

function activeGroup() {
  return state.groups.find((entry) => entry.id === state.selectedGroupId) || findGroupForItem(state.selectedId) || state.groups[0] || null;
}

function skillbookContext(group = activeGroup(), skillbook = selectedSkillbookForGroup(group)) {
  if (!group) return { skillbook: null, entries: [], skill: null, runbookItems: [], runbooks: [], resources: [], tables: [] };
  const skillbookEntry = typeof skillbook === 'string' ? group.entries.find((entry) => entry.id === skillbook) : skillbook;
  const allSkillbooks = skillbooksForGroup(group);
  const scopedEntries = !skillbookEntry || allSkillbooks.length <= 1
    ? group.entries
    : group.entries.filter((entry) => entry.id === skillbookEntry.id || relatedToSkillbook(skillbookEntry, entry));
  const entries = scopedEntries.length ? scopedEntries : group.entries;
  const skill = entries.find((entry) => entry.kind === 'skill')
    || group.entries.find((entry) => entry.kind === 'skill')
    || skillbookEntry
    || group.entries.find((entry) => entry.kind === 'skillbook')
    || group.entries[0]
    || null;
  const runbookItems = entries.filter((entry) => entry.kind === 'runbook');
  const linkedRunbookIds = new Set([
    ...extractRunbookIds(skillbookEntry?.linked_runbook_ids ?? skillbookEntry?.linked_runbooks_json ?? skillbookEntry?.linked_runbooks).map(normaliseRunbookId),
    ...runbookItems.map(runbookIdForItem).map(normaliseRunbookId),
  ]);
  const groupRunbookIds = new Set((group.runbookIds || []).map(normaliseRunbookId).filter(Boolean));
  const runbooks = state.runbooks.filter((runbook) => {
    const id = normaliseRunbookId(runbook.id || runbook.runbook_id);
    if (linkedRunbookIds.size) return linkedRunbookIds.has(id);
    if (!groupRunbookIds.has(id)) return false;
    return !skillbookEntry || allSkillbooks.length <= 1 || relatedToSkillbook(skillbookEntry, runbook);
  });
  // Sources and tables are domain assets. They must not disappear merely
  // because a user selected one of several explanatory Knowledge Books.
  const tables = uniqueById(group.entries.filter((entry) => entry.has_table));
  const resources = knowledgeResourcesForEntries(group.entries);
  return { skillbook: skillbookEntry || null, entries, skill, runbookItems, runbooks, resources, tables };
}

function knowledgeResourcesForEntries(entries = []) {
  return uniqueById(entries.filter((entry) => entry?.kind === 'resource' || entry?.source_path));
}

function relatedToSkillbook(skillbook, entry) {
  if (!skillbook || !entry) return true;
  if (entry.id === skillbook.id) return true;
  if (entry.skillbook_id && entry.skillbook_id === bareKnowledgeId(skillbook.id)) return true;
  const base = normaliseName(bareId(skillbook.id).replace(/-skillbook$/, ''));
  const haystack = `${entry.id || ''} ${entry.title || ''} ${entry.subtitle || ''} ${entry.summary || ''} ${entry.description || ''} ${entry.problem_domain || ''}`.toLowerCase();
  return haystack.includes(base.replaceAll('-', '_')) || haystack.includes(base) || tokenOverlap(skillbook, entry) >= 2;
}

async function selectSkillbook(group, skillbook) {
  if (!group) return;
  // Mobile master-detail: selecting an entry switches to the content view.
  state.ctx?.host?.querySelector('[data-knowledge-root]')?.classList.add('is-detail');
  const skillbookEntry = typeof skillbook === 'string' ? group.entries.find((entry) => entry.id === skillbook) : skillbook;
  const context = skillbookContext(group, skillbookEntry);
  state.selectedGroupId = group.id;
  state.selectedSkillbookId = context.skillbook?.id || '';
  state.selectedTableId = context.tables[0]?.id || '';
  state.selectedRunbookId = normaliseRunbookId(context.runbooks[0]?.id || context.runbooks[0]?.runbook_id || context.runbookItems[0]?.id || '');
  state.tableOffset = 0;
  const targetId = state.activeTab === 'data'
    ? state.selectedTableId || context.skill?.id || context.skillbook?.id || group.primaryItemId
    : context.skill?.id || context.skillbook?.id || group.primaryItemId;
  await selectKnowledge(targetId);
}

function renderKnowledgeList({ resetScroll = false } = {}) {
  const copy = state.messages || labels[state.lang];
  const term = state.searchTerm;
  // Data re-renders never move the operator: preserve the scroll offset across
  // the rebuild (intentional resets — search/view/filter — pass resetScroll
  // because the content set changed). The shell scroll guard backs this up.
  const scrollTop = resetScroll ? 0 : (els.list?.scrollTop || 0);
  let visibleGroups = state.groups
    .map((group) => ({
      ...group,
      entries: group.entries.filter((entry) => {
        return state.sourceScope === 'all' || sourceScopeFor(entry) === state.sourceScope;
      }),
    }))
    .filter((group) => {
      if (!group.entries.length) return false;
      if (state.sourceScope === 'all' && isInternalSkillOnlyGroup(group)) return false;
      if (!term) return true;
      return `${group.title} ${group.summary || ''} ${group.domain || ''} ${group.entries.map((entry) => `${entry.title} ${entry.subtitle || ''} ${entry.summary || ''}`).join(' ')}`.toLowerCase().includes(term);
    });
  // Advanced filters: only groups that carry the selected kinds.
  if (state.flagFilters && state.flagFilters.size) {
    visibleGroups = visibleGroups.filter((group) => {
      const has = {
        skillbooks: skillbooksForGroup(group).length > 0,
        runbooks: group.runbookIds.length > 0,
        tables: group.tableIds.length > 0,
      };
      return [...state.flagFilters].every((flag) => has[flag]);
    });
  }
  // Advanced sort: pick the field comparator (ascending sense), then apply the
  // chosen direction. More fields than the old three, each reversible.
  const ascending = {
    recent: (a, b) => groupRecency(a) - groupRecency(b),
    created: (a, b) => groupCreated(a) - groupCreated(b),
    name: (a, b) => (a.title || '').localeCompare(b.title || '', 'de'),
    entries: (a, b) => groupSize(a) - groupSize(b),
  }[state.sortMode] || ((a, b) => groupRecency(a) - groupRecency(b));
  const dir = state.sortDir === 'asc' ? 1 : -1;
  visibleGroups.sort((a, b) => ascending(a, b) * dir);
  if (!visibleGroups.length) {
    // Only the UNFILTERED empty source (knowledgeEmptyStateMessage's noItems
    // branch) is data-driven and gets the readiness gate; search/filter and
    // error empties stay plain .ctox-empty.
    const dataDriven = !state.loadError && !state.missingCollections.length && !term && !state.items.length;
    const emptyHtml = knowledgeListStateHtml({
      dataDriven,
      readiness: knowledgeSourceReadiness(),
      message: knowledgeEmptyStateMessage(copy, term),
      syncingText: copy.syncingData,
    });
    renderHtmlIfChanged(els.list, emptyHtml, {
      signature: `empty:${emptyHtml}`,
      preserveScroll: !resetScroll,
    });
    if (resetScroll) els.list.scrollTop = 0;
  } else {
    const signature = JSON.stringify({
      // The view is part of the signature: cards and list are two different
      // markups, so a view switch must actually rewrite the well.
      view: state.listView ? 'list' : 'cards',
      scope: state.sourceScope,
      term,
      sortMode: state.sortMode,
      sortDir: state.sortDir,
      flags: [...(state.flagFilters || [])].sort(),
      groups: visibleGroups.map((group) => ([
        group.id,
        group.title || '',
        group.summary || '',
        group.domain || '',
        group.entries.map((entry) => entry.id).join(','),
        group.runbookIds?.join?.(',') || '',
        group.tableIds?.join?.(',') || '',
      ])),
    });
    replaceChildrenIfChanged(
      els.list,
      visibleGroups.map((group) => renderKnowledgeBundle(group, state.listView ? 'list' : 'cards')),
      { signature, preserveScroll: !resetScroll },
    );
    if (resetScroll) els.list.scrollTop = 0;
    else els.list.scrollTop = scrollTop;
    applyKnowledgeSelection();
  }
  // One-line pane footer via the shell-wired grammar handle (null-guarded: the
  // shell wires the panes debounced ~120ms after mount, so early renders fall
  // back to the direct data-pg-footer target).
  const n = visibleGroups.length;
  const scopeLabel = state.sourceScope === 'all' ? 'Alle' : state.sourceScope === 'system' ? 'System' : 'User';
  const footerText = `${n} ${n === 1 ? 'Eintrag' : 'Einträge'} · ${scopeLabel}`;
  const pg = els.leftPane?.__ctoxPaneGrammar;
  if (pg?.setFooter) pg.setFooter(footerText);
  else {
    const node = els.leftPane?.querySelector('[data-pg-footer]');
    if (node) node.textContent = footerText;
  }
}

function isInternalSkillOnlyGroup(group) {
  if (!group?.entries?.length || groupSize(group) > 0) return false;
  return group.entries.every((entry) => (
    entry.kind === 'skill' && sourceScopeFor(entry) === 'system'
  ));
}

function knowledgeEmptyStateMessage(copy, term = '') {
  if (state.loadError || state.missingCollections.length) return copy.syncUnavailable;
  if (term) return copy.noResults;
  if (state.items.length) return copy.noVisibleItems;
  return copy.noItems;
}

function sourceScopeFor(entry) {
  const source = String(entry?.source_path || entry?.source_system || entry?.subtitle || '').toLowerCase();
  if (source.startsWith('embedded:skills/system') || source.includes('ctox_core')) return 'system';
  return 'user';
}

// Newest entry timestamp in a group, for the "Zuletzt bearbeitet" sort.
function groupRecency(group) {
  let max = 0;
  for (const entry of group.entries || []) {
    const t = Number(entry.updated_at_ms || entry.updated_at || entry.payload?.updated_at_ms || 0) || 0;
    if (t > max) max = t;
  }
  return max;
}

// Earliest creation timestamp in a group, for the "Erstellt" sort.
function groupCreated(group) {
  let min = Infinity;
  for (const entry of group.entries || []) {
    const t = Number(entry.created_at_ms || entry.created_at || entry.payload?.created_at_ms || 0) || 0;
    if (t && t < min) min = t;
  }
  return min === Infinity ? 0 : min;
}

// Total content count of a group (skillbooks + runbooks + tables), for the
// "Anzahl Einträge" sort.
function groupSize(group) {
  return skillbooksForGroup(group).length + (group.runbookIds?.length || 0) + (group.tableIds?.length || 0);
}

// Scannable count badges (Research-style numbers) — only non-zero kinds show,
// so you can tell whether a shard has content before clicking it.
function bundleCountsHtml(skillbooks, runbooks, tables) {
  // Parenthesized counts for every kind — zeros included — as ONE inline text
  // line, not pill badges: three numbers must not cost two rows of chrome.
  const part = (n, label) => `<span class="kb-count${n > 0 ? '' : ' kb-zero'}">${label} (${n})</span>`;
  return [part(skillbooks, 'Skillbooks'), part(runbooks, 'Runbooks'), part(tables, 'Tabellen')].join(' · ');
}

// Short absolute day for the shard meta line. Deliberately not a relative
// phrase ("vor 3 Tagen"): a shard meta must stay stable while the list is open.
function shortDay(ms, lang) {
  const t = Number(ms) || 0;
  if (!t) return '';
  const d = new Date(t);
  if (Number.isNaN(d.getTime())) return '';
  return d.toLocaleDateString(lang === 'en' ? 'en-GB' : 'de-DE', {
    day: '2-digit', month: '2-digit', year: 'numeric',
  });
}

// Where the group's entries come from — the one fact the operator needs before
// touching a skillbook (a system copy is not editable like a user one).
function groupScopeLabel(group, copy) {
  const scopes = new Set((group.entries || []).map((entry) => sourceScopeFor(entry)));
  if (scopes.size > 1) return copy.scopeMixed || labels.de.scopeMixed;
  if (scopes.has('system')) return copy.scopeSystem || labels.de.scopeSystem;
  return copy.scopeUser || labels.de.scopeUser;
}

// The two views are genuinely different densities, not one row with a class on
// it (Betreiber-Direktive 31.08.2026):
//   cards -> bold title + TWO meta lines (kicker/scope, counts/last edit),
//            generous padding
//   list  -> exactly ONE line: title left, one short kicker right, max density
// Both read fields the module already loads — no new data path, no new schema.
function renderKnowledgeBundle(group, view = 'cards') {
  const section = document.createElement('section');
  // Selection is applied after the write via applyKnowledgeSelection.
  section.className = 'knowledge-bundle';
  section.dataset.shellV2Accent = '';
  section.dataset.bundleId = group.id;
  section.dataset.contextModule = 'knowledge';
  section.dataset.contextRecordType = 'knowledge-group';
  section.dataset.contextRecordId = group.id;
  section.dataset.contextLabel = group.title;
  section.dataset.knowledgeColumn = 'sources';
  section.setAttribute('aria-selected', 'false');
  const tableCount = group.tableIds.length;
  const runbookCount = group.runbookIds.length;
  const skillbookCount = skillbooksForGroup(group).length;
  const copy = state.messages || labels[state.lang];
  const domain = group.domainLabel || 'Knowledge';
  const fullTitle = escapeHtml(`${domain} · Skillbooks (${skillbookCount}) · Runbooks (${runbookCount}) · Tabellen (${tableCount})`);
  // The shard is a pure SELECTOR: no inline expansion — the content pane's tabs
  // and second-level switcher are the one and only navigation into
  // skillbooks/runbooks/tables. Expanding here would duplicate that navigation
  // inside the list.
  if (view === 'list') {
    section.classList.add('is-compact');
    section.innerHTML = `
    <button class="bundle-select" type="button" title="${fullTitle}">
      <strong>${escapeHtml(group.title)}</strong>
      <small class="bundle-tag">${escapeHtml(domain)}</small>
    </button>
  `;
  } else {
    const edited = shortDay(groupRecency(group), state.lang) || (copy.neverEdited || labels.de.neverEdited);
    section.innerHTML = `
    <button class="bundle-select" type="button" title="${fullTitle}">
      <strong>${escapeHtml(group.title)}</strong>
      <small class="bundle-meta"><span class="bundle-domain">${escapeHtml(domain)}</span> · <span class="bundle-scope">${escapeHtml(groupScopeLabel(group, copy))}</span></small>
      <small class="bundle-meta"><span class="bundle-counts">${bundleCountsHtml(skillbookCount, runbookCount, tableCount)}</span> · <span class="bundle-edited">${escapeHtml(edited)}</span></small>
    </button>
  `;
  }
  section.querySelector('.bundle-select').addEventListener('click', () => {
    state.selectedGroupId = group.id;
    const skillbook = selectedSkillbookForGroup(group);
    state.selectedSkillbookId = skillbook?.id || '';
    const context = skillbookContext(group, skillbook);
    state.selectedId = context.skill?.id || skillbook?.id || group.primaryItemId || group.entries[0]?.id || '';
    state.selectedTableId = context.tables[0]?.id || group.tableIds[0] || '';
    state.selectedRunbookId = normaliseRunbookId(context.runbooks[0]?.id || context.runbooks[0]?.runbook_id || group.runbookIds[0] || state.selectedRunbookId);
    selectSkillbook(group, skillbook);
  });
  return section;
}

// Deletion is destructive — always confirm. Wired to the module delete path
// if present; otherwise it surfaces a clear message rather than silently no-op.
function deleteKnowledgeEntry(item, group) {
  const name = item.title || item.id;
  if (!window.confirm(`„${name}" wirklich löschen?`)) return;
  if (typeof requestKnowledgeDelete === 'function') {
    requestKnowledgeDelete(item, group);
  } else if (state.ctx?.commandBus?.dispatch) {
    state.ctx.commandBus.dispatch({ command_type: 'knowledge.entry.delete', payload: { id: item.id } }).catch(() => {});
  }
}

// In-place selection: flip is-selected/aria-selected across the existing rows,
// never a list rebuild — a rebuild resets the scroll position under the
// operator's pointer (canonical interaction law; ctox applyTaskSelection).
function applyKnowledgeSelection() {
  els.list?.querySelectorAll('.knowledge-bundle').forEach((row) => {
    const on = (row.dataset.bundleId || '') === state.selectedGroupId;
    row.classList.toggle('is-selected', on);
    row.setAttribute('aria-selected', String(on));
  });
}

async function selectKnowledge(id) {
  if (!id) return;
  state.selectedId = id;
  const group = findGroupForItem(id);
  if (group) {
    state.selectedGroupId = group.id;
    const item = state.items.find((entry) => entry.id === id);
    if (item?.kind === 'skillbook') {
      state.selectedSkillbookId = item.id;
    } else if (!state.selectedSkillbookId || !skillbooksForGroup(group).some((entry) => entry.id === state.selectedSkillbookId)) {
      state.selectedSkillbookId = firstSkillbookForGroup(group)?.id || '';
    }
    const context = skillbookContext(group, state.selectedSkillbookId);
    if (!state.selectedTableId || !group.tableIds.includes(state.selectedTableId)) {
      state.selectedTableId = context.tables[0]?.id || group.tableIds[0] || '';
    }
    const contextRunbookIds = new Set(context.runbooks.map((runbook) => normaliseRunbookId(runbook.id || runbook.runbook_id)));
    if (contextRunbookIds.size && !contextRunbookIds.has(normaliseRunbookId(state.selectedRunbookId))) {
      const firstRunbook = context.runbooks[0];
      state.selectedRunbookId = normaliseRunbookId(firstRunbook.id || firstRunbook.runbook_id);
    }
  }
  state.tableOffset = 0;
  state.editing = false;
  const item = state.items.find((entry) => entry.id === id);
  if (els.selectedKind) els.selectedKind.textContent = groupLabel(item?.kind || 'knowledge');
  els.selectedTitle.textContent = item?.title || 'Knowledge';
  // Selection is an in-place class flip, never a list rebuild — a rebuild
  // resets the scroll position under the operator's pointer.
  applyKnowledgeSelection();
  const doc = await loadKnowledgeDocument(id);
  els.markdownEditor.hidden = true;
  els.markdownView.hidden = false;
  els.markdownEditor.value = doc.markdown || '';
  els.markdownView.innerHTML = markdownToHtml(doc.markdown || '');
  syncMarkdownEditControls();
  syncKnowledgeTabControls();
  await renderActiveTab();
}

function renderRunbooks() {
  if (!els.runbookList) return;
  const copy = state.messages || labels[state.lang];
  const group = state.groups.find((entry) => entry.id === state.selectedGroupId);
  const groupRunbookIds = new Set((group?.runbookIds || []).map(normaliseRunbookId).filter(Boolean));
  const visibleRunbooks = group
    ? state.runbooks.filter((runbook) => groupRunbookIds.has(normaliseRunbookId(runbook.id || runbook.runbook_id)))
    : state.runbooks;
  if (!visibleRunbooks.length) {
    // Group-scoped filtering is a selection empty; only an entirely empty
    // knowledge_runbooks source is data-driven and readiness-gated.
    els.runbookList.innerHTML = knowledgeListStateHtml({
      dataDriven: !state.runbooks.length,
      readiness: knowledgeReadinessSnapshot('knowledge_runbooks'),
      message: copy.noRunbooks,
      syncingText: copy.syncingData,
    });
    fillRunbookForm(null);
    return;
  }
  if (!visibleRunbooks.some((runbook) => runbookIdMatches(runbook.id || runbook.runbook_id, state.selectedRunbookId))) {
    state.selectedRunbookId = normaliseRunbookId(visibleRunbooks[0].id || visibleRunbooks[0].runbook_id);
  }
  els.runbookList.replaceChildren(...visibleRunbooks.map((runbook) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'runbook-item';
    button.dataset.runbookId = runbook.id;
    button.dataset.contextModule = 'knowledge';
    button.dataset.contextRecordType = 'runbook';
    button.dataset.contextRecordId = runbook.id;
    button.dataset.contextLabel = runbook.title || runbook.id;
    button.dataset.knowledgeColumn = 'runbooks';
    const isActiveRunbook = runbookIdMatches(runbook.id || runbook.runbook_id, state.selectedRunbookId);
    button.classList.toggle('is-selected', isActiveRunbook);
    button.setAttribute('aria-selected', String(isActiveRunbook));
    button.innerHTML = `<strong>${escapeHtml(runbook.title || runbook.id)}</strong><span>${escapeHtml(`${runbook.status || ''} · ${runbook.problem_domain || ''}`)}</span>`;
    button.addEventListener('click', () => {
      state.selectedRunbookId = normaliseRunbookId(runbook.id || runbook.runbook_id);
      renderRunbooks();
    });
    return button;
  }));
  fillRunbookForm(visibleRunbooks.find((runbook) => runbookIdMatches(runbook.id || runbook.runbook_id, state.selectedRunbookId)) || visibleRunbooks[0]);
}

function fillRunbookForm(runbook) {
  if (!els.runbookTitle || !els.runbookPrompt || !els.runbookStatus) return;
  els.runbookTitle.value = runbook?.title || '';
  els.runbookPrompt.value = runbook?.prompt || runbook?.instruction || runbook?.description || '';
  els.runbookPrompt.placeholder = runbook ? 'Runbook-Anweisung aus dem CTOX Knowledge Store' : '';
  els.runbookStatus.textContent = '';
}

function setTab(tab) {
  const nextTab = ({ book: 'skill', table: 'data' })[tab] || tab;
  if (isKnowledgeTabDisabled(nextTab, state.selectedId)) {
    syncKnowledgeTabControls();
    return;
  }
  state.activeTab = ['skill', 'runbooks', 'resources', 'data'].includes(nextTab) ? nextTab : 'skill';
  state.editing = false;
  els.markdownEditor.hidden = true;
  els.markdownView.hidden = false;
  syncMarkdownEditControls();
  syncRunbookEditControls(false);
  syncKnowledgeTabControls();
  renderActiveTab();
}

function setActionHidden(action, hidden) {
  const button = state.ctx.host.querySelector(`[data-action="${action}"]`);
  if (button) button.hidden = hidden;
}

// Counts on the counted view band + one-line pane footer, so you can see what
// a selection holds before clicking a tab (Skill · Runbooks (n) · Tabellen
// (n)). Goes through the shell-wired grammar handle when present (null-
// guarded: the shell wires panes debounced ~120ms after mount, so early
// renders fall back to the direct data-pg-* targets).
function updateViewCounts() {
  const pane = els.centerPane;
  if (!pane) return;
  const context = skillbookContext();
  const counts = {
    skill: context.skill ? 1 : 0,
    runbooks: context.runbooks.length,
    resources: context.resources.length,
    data: context.tables.length,
  };
  const pg = pane.__ctoxPaneGrammar;
  if (pg?.setCounts) pg.setCounts(counts);
  else for (const [key, value] of Object.entries(counts)) {
    const node = pane.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
  const item = state.items.find((entry) => entry.id === state.selectedId);
  const footerText = item
    ? [groupLabel(item.kind || 'knowledge'), domainLabelFor(item)].filter(Boolean).join(' · ')
    : '';
  if (pg?.setFooter) pg.setFooter(footerText);
  else {
    const node = pane.querySelector('[data-pg-footer]');
    if (node) node.textContent = footerText;
  }
}

async function renderActiveTab() {
  updateViewCounts();
  if (!hasKnowledgeSelection() && state.activeTab !== 'skill') {
    state.activeTab = 'skill';
    renderEmptyKnowledgeSelection();
    return;
  }
  if (state.activeTab === 'skill') {
    const context = skillbookContext();
    renderSkillbookSwitcher();
    if (context.skill?.id && state.selectedId !== context.skill.id) {
      await selectKnowledge(context.skill.id);
      return;
    }
    renderSelectionHeader();
    return;
  }
  if (state.activeTab === 'runbooks') {
    await renderRunbookWorkspace();
    return;
  }
  if (state.activeTab === 'resources') {
    await renderResourceWorkspace();
    return;
  }
  if (state.activeTab === 'data') {
    renderTableSwitcher();
    await renderTable();
  }
}

function renderSkillbookSwitcher() {
  if (!els.skillbookSwitcher) return;
  const group = activeGroup();
  const skillbooks = skillbooksForGroup(group);
  els.skillbookSwitcher.hidden = skillbooks.length <= 1;
  els.skillbookSwitcher.replaceChildren(...skillbooks.map((skillbook) => {
    const button = document.createElement('button');
    button.type = 'button';
    const active = skillbook.id === (state.selectedSkillbookId || firstSkillbookForGroup(group)?.id);
    button.className = `ctox-chip${active ? ' is-active' : ''}`;
    button.setAttribute('aria-selected', String(active));
    button.textContent = skillbook.title || titleFromSlug(bareId(skillbook.id));
    button.addEventListener('click', () => selectSkillbook(group, skillbook));
    return button;
  }));
}

function syncKnowledgeTabControls() {
  if (isKnowledgeTabDisabled(state.activeTab, state.selectedId)) state.activeTab = 'skill';
  for (const button of state.ctx.host.querySelectorAll('[data-pg-band]')) {
    const disabled = isKnowledgeTabDisabled(button.dataset.pgBand, state.selectedId);
    const selected = button.dataset.pgBand === state.activeTab;
    button.disabled = disabled;
    button.setAttribute('aria-disabled', String(disabled));
    button.setAttribute('aria-selected', String(selected));
    button.classList.toggle('is-active', selected);
  }
  for (const panel of state.ctx.host.querySelectorAll('[data-panel]')) {
    panel.hidden = panel.dataset.panel !== state.activeTab;
  }
}

function isKnowledgeTabDisabled(tab, selectedId = state.selectedId) {
  return ['runbooks', 'resources', 'data'].includes(tab) && !selectedId;
}

function hasKnowledgeSelection() {
  return Boolean(state.selectedId && state.items.some((entry) => entry.id === state.selectedId));
}

function renderSelectionHeader() {
  const group = activeGroup();
  const context = skillbookContext(group, state.selectedSkillbookId);
  const item = state.items.find((entry) => entry.id === state.selectedId) || context.skill;
  const isProceduralSkill = context.skill?.kind === 'skill';
  if (els.selectedKind) els.selectedKind.textContent = isProceduralSkill ? 'Skill' : 'Knowledge Book';
  els.selectedTitle.textContent = context.skill?.title || context.skillbook?.title || item?.title || group?.title || 'Knowledge';
  syncMarkdownEditControls();
}

async function renderRunbookWorkspace() {
  const copy = state.messages || labels[state.lang];
  const context = skillbookContext();
  const visibleRunbooks = context.runbooks;
  if (els.selectedKind) els.selectedKind.textContent = 'Runbooks';
  els.selectedTitle.textContent = context.skillbook?.title || activeGroup()?.title || 'Knowledge';
  if (!visibleRunbooks.length) {
    els.runbookSwitcher.hidden = true;
    // Same gate as the runbook list: group/skillbook-scoped empties are
    // selection states, an empty knowledge_runbooks source is data-driven.
    els.runbookView.innerHTML = knowledgeListStateHtml({
      dataDriven: !state.runbooks.length,
      readiness: knowledgeReadinessSnapshot('knowledge_runbooks'),
      message: copy.noRunbooks,
      syncingText: copy.syncingData,
    });
    fillRunbookForm(null);
    syncRunbookEditControls(false);
    return;
  }
  if (!visibleRunbooks.some((runbook) => runbookIdMatches(runbook.id || runbook.runbook_id, state.selectedRunbookId))) {
    state.selectedRunbookId = normaliseRunbookId(visibleRunbooks[0].id || visibleRunbooks[0].runbook_id);
  }
  els.runbookSwitcher.hidden = visibleRunbooks.length <= 1;
  els.runbookSwitcher.replaceChildren(...visibleRunbooks.map((runbook) => {
    const button = document.createElement('button');
    button.type = 'button';
    const isActive = runbookIdMatches(runbook.id || runbook.runbook_id, state.selectedRunbookId);
    button.className = `ctox-chip${isActive ? ' is-active' : ''}`;
    button.dataset.contextModule = 'knowledge';
    button.dataset.contextRecordType = 'runbook';
    button.dataset.contextRecordId = runbook.id || runbook.runbook_id || '';
    button.dataset.contextLabel = runbook.title || runbook.id || runbook.runbook_id || '';
    button.dataset.knowledgeColumn = 'runbooks';
    button.setAttribute('aria-selected', String(runbookIdMatches(runbook.id || runbook.runbook_id, state.selectedRunbookId)));
    button.textContent = runbook.title || runbook.id || runbook.runbook_id || 'Runbook';
    button.addEventListener('click', () => {
      state.selectedRunbookId = normaliseRunbookId(runbook.id || runbook.runbook_id);
      state.editing = false;
      renderRunbookWorkspace();
    });
    return button;
  }));
  const runbook = visibleRunbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId)) || visibleRunbooks[0];
  const runbookItem = context.runbookItems.find((entry) => runbookIdMatches(runbookIdForItem(entry), runbook.id || runbook.runbook_id));
  let markdown = '';
  if (runbookItem?.id) {
    const doc = await loadKnowledgeDocument(runbookItem.id);
    markdown = doc.markdown || '';
  }
  els.runbookView.innerHTML = markdown
    ? markdownToHtml(markdown)
    : runbookDetailsHtml(runbook);
  fillRunbookForm(runbook);
  syncRunbookEditControls(state.editing);
}

async function renderResourceWorkspace() {
  const copy = state.messages || labels[state.lang];
  const context = skillbookContext();
  const resources = context.resources;
  if (els.selectedKind) els.selectedKind.textContent = 'Quellen';
  els.selectedTitle.textContent = context.skillbook?.title || activeGroup()?.title || 'Knowledge';
  if (!resources.length) {
    els.resourceSwitcher.hidden = true;
    els.resourceView.innerHTML = `<div class="ctox-empty"><strong>${escapeHtml(copy.noResources)}</strong></div>`;
    return;
  }
  if (!resources.some((resource) => resource.id === state.selectedResourceId)) {
    state.selectedResourceId = resources[0].id;
  }
  els.resourceSwitcher.hidden = resources.length <= 1;
  els.resourceSwitcher.replaceChildren(...resources.map((resource) => {
    const button = document.createElement('button');
    button.type = 'button';
    const active = resource.id === state.selectedResourceId;
    button.className = `ctox-chip${active ? ' is-active' : ''}`;
    button.setAttribute('aria-selected', String(active));
    button.textContent = resource.title || resource.source_id || resource.id;
    button.title = resource.title || resource.id;
    button.addEventListener('click', () => {
      state.selectedResourceId = resource.id;
      renderResourceWorkspace();
    });
    return button;
  }));
  const resource = resources.find((entry) => entry.id === state.selectedResourceId) || resources[0];
  const doc = await loadKnowledgeDocument(resource.id);
  const sourcePath = String(resource.source_path || '').trim();
  const sourcePathHtml = sourcePath
    ? `<p class="knowledge-resource-path"><strong>Quelle</strong><code>${escapeHtml(sourcePath)}</code></p>`
    : '';
  els.resourceView.innerHTML = `${sourcePathHtml}${markdownToHtml(doc.markdown || '')}`;
}

function runbookDetailsHtml(runbook) {
  const instruction = runbook?.prompt || runbook?.instruction || runbook?.description || '';
  const inputs = listValue(runbook?.inputs ?? runbook?.required_inputs ?? runbook?.input_schema);
  const toolActions = listValue(runbook?.tool_actions);
  const verification = listValue(runbook?.verification);
  const escalateWhen = listValue(runbook?.escalate_when);
  const output = listValue(runbook?.output_schema ?? runbook?.expected_output ?? runbook?.writeback_policy);
  return `
    <header class="runbook-document-head">
      <span>${escapeHtml(runbook?.status || 'Runbook')}</span>
      <h1>${escapeHtml(runbook?.title || runbook?.id || runbook?.runbook_id || 'Runbook')}</h1>
    </header>
    <dl class="ctox-fields runbook-meta">
      <dt>Domain</dt><dd>${escapeHtml(runbook?.problem_domain || '-')}</dd>
      <dt>ID</dt><dd>${escapeHtml(runbook?.id || runbook?.runbook_id || '-')}</dd>
    </dl>
    ${instruction ? `<section><h2>Ausführungsauftrag</h2><p>${escapeHtml(instruction)}</p></section>` : ''}
    ${renderKnowledgeListSection('Erforderliche Eingaben', inputs)}
    ${renderKnowledgeListSection('Werkzeuge und Schritte', toolActions)}
    ${renderKnowledgeListSection('Prüfungen', verification)}
    ${renderKnowledgeListSection('Abbruch und Eskalation', escalateWhen)}
    ${renderKnowledgeListSection('Ergebnis und Writeback', output)}
    ${!instruction && !inputs.length && !toolActions.length
      ? '<div class="ctox-empty"><strong>Kein ausführbarer Runbook-Vertrag vorhanden.</strong></div>'
      : ''}
  `;
}

// Sibling titles often share a long common prefix ("Outbound Firmenqualifizierung
// · Unternehmen / · Ansprechpartner / …"). Truncation would render them
// identical, so the shared " · "-prefix is dropped and only the distinguishing
// part is shown (full title stays in the tooltip).
function distinguishingLabels(titles) {
  const split = titles.map((title) => String(title || '').split(' · '));
  if (split.length < 2) return split.map((parts) => parts.join(' · '));
  let prefix = 0;
  while (split.every((parts) => parts.length > prefix + 1 && parts[prefix] === split[0][prefix])) prefix += 1;
  return split.map((parts) => parts.slice(prefix).join(' · ') || parts.join(' · '));
}

function renderTableSwitcher() {
  const context = skillbookContext();
  const tables = context.tables;
  const labels = distinguishingLabels(tables.map((table) => table.title || table.id));
  els.tableSwitcher.hidden = tables.length <= 1;
  els.tableSwitcher.replaceChildren(...tables.map((table, index) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = `ctox-chip${table.id === activeTableId() ? ' is-active' : ''}`;
    button.dataset.contextModule = 'knowledge';
    button.dataset.contextRecordType = 'dataframe';
    button.dataset.contextRecordId = table.id;
    button.dataset.contextLabel = table.title || table.id;
    button.dataset.knowledgeColumn = 'workspace';
    button.setAttribute('aria-selected', String(table.id === activeTableId()));
    button.textContent = labels[index];
    button.title = table.title || table.id;
    button.addEventListener('click', () => {
      state.selectedTableId = table.id;
      state.selectedId = table.id;
      state.tableOffset = 0;
      // In-place selection flip in the left rail — never a list rebuild.
      applyKnowledgeSelection();
      renderTableSwitcher();
      renderTable();
    });
    return button;
  }));
}

async function renderTable() {
  const copy = state.messages || labels[state.lang];
  const tableId = activeTableId();
  const item = state.items.find((entry) => entry.id === tableId);
  const tableRecord = tableForItem(item || { id: tableId }, state.tables);
  const tableSource = mergeKnowledgeTableData(item, tableRecord);
  if (els.selectedKind) els.selectedKind.textContent = 'Data';
  els.selectedTitle.textContent = tableSource?.title || skillbookContext().skillbook?.title || 'DataFrame';
  if (!tableSource?.has_table) {
    els.tableTitle.textContent = 'DataFrame';
    els.tableMeta.textContent = copy.tableUnavailable;
    els.tableHost.innerHTML = `<div class="ctox-empty"><strong>${copy.tableUnavailable}</strong></div>`;
    return;
  }
  try {
    const completeness = dataFrameCompleteness(tableSource);
    if (!completeness.complete) {
      els.tableTitle.textContent = schemaTitleForTable(tableSource);
      els.tableMeta.textContent = `${copy.dataIncomplete} · ${completeness.reason}`;
      els.tableHost.innerHTML = `<div class="ctox-empty knowledge-error" role="alert"><strong>${escapeHtml(copy.dataIncomplete)}</strong><span>${escapeHtml(copy.dataIncompleteHint)}</span><span>${escapeHtml(completeness.reason)}</span></div>`;
      return;
    }
    const localRows = completeness.rows;
    const schema = localDataFrameSchema(tableSource);
    const rows = localRows.length
      ? {
          returned: localRows.slice(state.tableOffset, state.tableOffset + state.tableLimit).length,
          rows: localRows.slice(state.tableOffset, state.tableOffset + state.tableLimit),
        }
      : { returned: 0, rows: [] };
    els.tableTitle.textContent = schema.title || tableSource.title || 'DataFrame';
    const totalRows = Number.isFinite(Number(schema.row_count)) ? Number(schema.row_count) : localRows.length;
    const firstVisible = totalRows ? state.tableOffset + 1 : 0;
    const lastVisible = Math.min(totalRows, state.tableOffset + rows.returned);
    els.tableMeta.textContent = `${schema.columns?.length || 0} Spalten · Zeilen ${firstVisible.toLocaleString('de-DE')}-${lastVisible.toLocaleString('de-DE')} von ${totalRows.toLocaleString('de-DE')}`;
    const previous = state.ctx.host.querySelector('[data-action="prev-rows"]');
    const next = state.ctx.host.querySelector('[data-action="next-rows"]');
    if (previous) previous.disabled = state.tableOffset <= 0;
    if (next) next.disabled = lastVisible >= totalRows;
    renderDataFrameTable(schema.columns || [], rows.rows || []);
  } catch (error) {
    els.tableHost.innerHTML = `<div class="ctox-empty knowledge-error"><strong>DataFrame konnte nicht geladen werden</strong><span>${escapeHtml(error.message || String(error))}</span></div>`;
  }
}

function schemaTitleForTable(table) {
  return table?.title || table?.payload?.title || 'DataFrame';
}

function activeTableId() {
  const selected = state.items.find((entry) => entry.id === state.selectedId);
  if (selected?.has_table) return selected.id;
  const group = findGroupForItem(state.selectedId) || state.groups.find((entry) => entry.id === state.selectedGroupId);
  const context = skillbookContext(group, state.selectedSkillbookId);
  return state.selectedTableId || context.tables[0]?.id || group?.tableIds?.[0] || '';
}

function renderDataFrameTable(columns, rows) {
  if (!columns.length) {
    els.tableHost.innerHTML = '<div class="ctox-empty"><strong>Keine Spalten</strong></div>';
    return;
  }
  const normalColumns = normalizeColumns(columns);
  const table = document.createElement('table');
  table.className = 'ctox-table dataframe-table';
  table.innerHTML = `
    <thead><tr>${normalColumns.map((column) => dataframeHeaderHtml(column)).join('')}</tr></thead>
    <tbody>${rows.map((row) => `<tr>${normalColumns.map((column) => `<td>${escapeHtml(formatCell(valueForColumn(row, column), column))}</td>`).join('')}</tr>`).join('')}</tbody>
  `;
  els.tableHost.replaceChildren(table);
}

function dataframeHeaderHtml(column) {
  const label = columnHeaderLabel(column);
  const help = columnHeaderHelp(column);
  const meta = [
    column.dtype || column.type || '',
    column.unit ? `Einheit: ${column.unit}` : '',
    column.key && column.key !== label ? `Key: ${column.key}` : '',
  ].filter(Boolean).join(' · ');
  return `
    <th title="${escapeHtml(help || meta)}">
      <span class="column-label">${escapeHtml(label)}</span>
      ${help ? `<span class="column-help" tabindex="0" aria-label="${escapeHtml(help)}" data-tooltip="${escapeHtml(help)}">i</span>` : ''}
    </th>
  `;
}

function valueForColumn(row, column) {
  if (!row || typeof row !== 'object') return '';
  const keys = [column?.key, column?.name, column?.field, column?.id, column?.label].filter(Boolean);
  for (const key of keys) {
    if (Object.prototype.hasOwnProperty.call(row, key)) return row[key];
  }
  return '';
}

function pageTable(direction) {
  state.tableOffset = Math.max(0, state.tableOffset + direction * state.tableLimit);
  renderTable();
}

function syncMarkdownEditControls(options = {}) {
  const isEditing = state.activeTab === 'skill' && state.editing;
  const canEdit = canEditSelectedMarkdown();
  setActionHidden('edit-markdown', isEditing);
  setActionHidden('save-markdown', !isEditing);
  setActionHidden('cancel-markdown', !isEditing);
  const editButton = state.ctx.host.querySelector('[data-action="edit-markdown"]');
  if (editButton) {
    editButton.disabled = !canEdit;
    editButton.setAttribute('aria-disabled', String(!canEdit));
  }
  if (els.skillStatus && !isEditing && !options.keepStatus) els.skillStatus.textContent = '';
}

function toggleMarkdownEditor() {
  if (!canEditSelectedMarkdown()) return;
  state.editing = !state.editing;
  els.markdownEditor.hidden = !state.editing;
  els.markdownView.hidden = state.editing;
  syncMarkdownEditControls();
}

function cancelMarkdownEdit() {
  state.editing = false;
  els.markdownEditor.hidden = true;
  els.markdownView.hidden = false;
  syncMarkdownEditControls();
}

async function queueMarkdownSave() {
  if (!canEditSelectedMarkdown()) return;
  const item = state.items.find((entry) => entry.id === state.selectedId);
  const markdown = state.editing ? els.markdownEditor.value : els.markdownView.textContent;
  const result = await dispatchKnowledgeCommand({
    command_type: 'ctox.knowledge.document.modify',
    record_id: state.selectedId,
    payload: {
      title: `Knowledge Änderung · ${item?.title || state.selectedId}`,
      instruction: `Prüfe und persistiere die folgende Knowledge-Änderung im CTOX Knowledge Store. Erhalte Skill-, Skillbook-, Runbook- und Ressourcenstruktur; schreibe Änderungen über die passende CTOX-Schicht zurück.`,
      markdown,
      selected_item: item,
    },
  });
  if (els.skillStatus) els.skillStatus.textContent = result?.ok ? `${(state.messages || labels[state.lang]).queued} · ${result.task_id || result.command_id}` : (state.messages || labels[state.lang]).queueFailed;
  if (result?.ok) {
    els.markdownView.innerHTML = markdownToHtml(markdown || '');
    state.editing = false;
    els.markdownEditor.hidden = true;
    els.markdownView.hidden = false;
    syncMarkdownEditControls({ keepStatus: true });
  }
  showCommandStatus(result);
}

function canEditSelectedMarkdown(selectedId = state.selectedId, items = state.items) {
  return Boolean(selectedId && items.some((entry) => entry.id === selectedId));
}

function syncRunbookEditControls(isEditing = state.activeTab === 'runbooks' && state.editing, options = {}) {
  const hasRunbook = Boolean(state.selectedId && state.selectedRunbookId);
  setActionHidden('edit-runbook', isEditing || !hasRunbook);
  setActionHidden('save-runbook', !isEditing);
  setActionHidden('cancel-runbook', !isEditing);
  setActionHidden('execute-runbook', isEditing || !hasRunbook);
  if (els.runbookView) els.runbookView.hidden = isEditing;
  if (els.runbookForm) els.runbookForm.hidden = !isEditing;
  if (els.runbookStatus && !isEditing && !options.keepStatus) els.runbookStatus.textContent = '';
}

function startRunbookEdit() {
  const runbook = state.runbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId));
  fillRunbookForm(runbook);
  state.editing = true;
  syncRunbookEditControls(true);
}

function cancelRunbookEdit() {
  state.editing = false;
  syncRunbookEditControls(false);
}

async function queueRunbookSave() {
  const copy = state.messages || labels[state.lang];
  const runbook = state.runbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId));
  const result = await dispatchKnowledgeCommand({
    command_type: 'ctox.knowledge.runbook.modify',
    record_id: state.selectedRunbookId,
    payload: {
      title: `Runbook Änderung · ${els.runbookTitle.value || runbook?.title || state.selectedRunbookId}`,
      instruction: `Prüfe und persistiere die Runbook-Änderung im CTOX Knowledge Store. Aktualisiere Runbook, Items, Ressourcenbindungen und Ausführungskontrakt konsistent.`,
      runbook,
      draft: {
        title: els.runbookTitle?.value || runbook?.title || '',
        prompt: els.runbookPrompt?.value || runbook?.prompt || runbook?.instruction || '',
      },
    },
  });
  if (els.runbookStatus) els.runbookStatus.textContent = result?.ok ? `${copy.queued} · ${result.task_id || result.command_id}` : copy.queueFailed;
  else showCommandStatus(result);
  if (result?.ok) {
    state.editing = false;
    syncRunbookEditControls(false, { keepStatus: true });
  }
}

async function executeRunbook() {
  const copy = state.messages || labels[state.lang];
  const runbook = state.runbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId));
  const item = state.items.find((entry) => entry.id === state.selectedId);
  const context = skillbookContext();
  const runbookItem = context.runbookItems.find((entry) => (
    runbookIdMatches(runbookIdForItem(entry), runbook?.id || runbook?.runbook_id || state.selectedRunbookId)
  ));
  const instruction = localMarkdownForItem(runbookItem)
    || els.runbookPrompt?.value
    || runbook?.prompt
    || runbook?.instruction
    || runbook?.description
    || '';
  if (!runbook || !instruction.trim()) {
    if (els.runbookStatus) els.runbookStatus.textContent = 'Runbook ist nicht vollständig ausführbar.';
    return;
  }
  const result = await dispatchKnowledgeCommand({
    command_type: 'ctox.knowledge.runbook.execute',
    record_id: state.selectedRunbookId,
    payload: {
      title: `Runbook ausführen · ${runbook?.title || state.selectedRunbookId}`,
      instruction,
      selected_item: item,
      runbook,
      runbook_item: runbookItem || null,
      required_output: 'Ergebnis, Prüfstatus, offene Eingaben, Evidence-Lineage und Writeback-Artefakte',
      priority: 'normal',
      thread_key: 'business-os/knowledge',
    },
  });
  if (els.runbookStatus) els.runbookStatus.textContent = result?.ok ? `${copy.queued} · ${result.task_id || result.command_id}` : copy.queueFailed;
  else showCommandStatus(result);
}

async function dispatchKnowledgeCommand(command) {
  const clientContext = {
    active_tab: state.activeTab,
    selected_knowledge_id: state.selectedId,
    selected_runbook_id: state.selectedRunbookId,
    ...(command.client_context || {}),
  };
  if (state.ctx.commandBus) {
    return state.ctx.commandBus.dispatch({
      ...command,
      module: 'knowledge',
      client_context: clientContext,
    });
  }
  throw new Error('The local command service is not available.');
}

function showCommandStatus(result) {
  const copy = state.messages || labels[state.lang];
  const message = result?.ok ? `${copy.queued} · ${result.task_id || result.command_id}` : copy.queueFailed;
  state.ctx.openBottomDrawer(drawerContent('Knowledge Command', message));
}

function openCreateKnowledgeBookDrawer() {
  const body = knowledgeActionDrawer({
    title: 'Knowledge Book erstellen',
    subtitle: 'Neues Skillbook mit Skill, Runbooks und optionalen Datenquellen anlegen',
    actionLabel: 'Erstellen lassen',
    commandType: 'ctox.knowledge.book.create',
    recordId: 'knowledge:create',
    commandTitle: 'Knowledge Book erstellen',
    fields: `
      <label>Titel <input name="title" required placeholder="z. B. Customer Onboarding Knowledge" /></label>
      <label>Domain / Pfad <input name="domain" placeholder="research/customer-onboarding" /></label>
      <label>Status
        <select name="status">
          <option value="draft">Draft</option>
          <option value="active">Active</option>
          <option value="imported">Imported</option>
        </select>
      </label>
      <label>Beschreibung <textarea name="summary" rows="3" placeholder="Wofür dieses Knowledge Book genutzt wird"></textarea></label>
      <label>Initialer Inhalt <textarea name="markdown" rows="8" placeholder="# Titel&#10;&#10;Skill, Runbooks und Datenanforderungen beschreiben"></textarea></label>
    `,
    buildPayload: (data) => ({
      title: `Knowledge Book erstellen · ${data.title || 'Untitled'}`,
      instruction: 'Lege ein neues Knowledge Book im CTOX Knowledge Store an. Erzeuge die Skillbook-Struktur, einen initialen Skill und bereite Runbook-/DataFrame-Slots vor.',
      knowledge_book: {
        title: data.title,
        domain: data.domain,
        status: data.status,
        summary: data.summary,
        markdown: data.markdown,
      },
    }),
  });
  state.ctx.openLeftDrawer(body);
}

function openImportKnowledgeBookDrawer() {
  const body = knowledgeActionDrawer({
    title: 'Knowledge Book importieren',
    subtitle: 'Markdown, Ordner, URL oder bestehende Runtime-Quelle in Knowledge übernehmen',
    actionLabel: 'Import starten',
    commandType: 'ctox.knowledge.book.import',
    recordId: 'knowledge:import',
    commandTitle: 'Knowledge Book importieren',
    fields: `
      <label>Import-Typ
        <select name="source_type">
          <option value="path">Pfad / Ordner</option>
          <option value="markdown">Markdown / Text</option>
          <option value="url">URL</option>
          <option value="runtime">Runtime Knowledge Source</option>
        </select>
      </label>
      <label>Quelle <input name="source" required placeholder="/path/to/knowledge-book oder https://..." /></label>
      <label>Ziel-Domain <input name="domain" placeholder="research/drone-design" /></label>
      <label>Import-Anweisung <textarea name="instruction" rows="7" placeholder="Wie Skill, Runbooks und Tabellen aus dieser Quelle geschnitten werden sollen"></textarea></label>
    `,
    buildPayload: (data) => ({
      title: `Knowledge Book importieren · ${data.source || data.source_type}`,
      instruction: 'Importiere die angegebene Quelle als Knowledge Book. Extrahiere Skillbook, Skill, Runbooks und DataFrame-Definitionen, ohne bestehende Knowledge-Struktur unkontrolliert zu ueberschreiben.',
      import_request: {
        source_type: data.source_type,
        source: data.source,
        domain: data.domain,
        instruction: data.instruction,
      },
    }),
  });
  state.ctx.openLeftDrawer(body);
}

function openExportKnowledgeBookDrawer() {
  const body = knowledgeActionDrawer({
    title: 'Knowledge Books exportieren',
    subtitle: 'Ausgewählte Knowledge-Struktur als Datei oder Bundle erzeugen',
    actionLabel: 'Export starten',
    commandType: 'ctox.knowledge.book.export',
    recordId: state.selectedSkillbookId || state.selectedGroupId || 'knowledge:export',
    commandTitle: 'Knowledge Books exportieren',
    fields: `
      <label>Umfang
        <select name="scope">
          <option value="selected">Aktuelle Auswahl</option>
          <option value="visible">Sichtbare Knowledge Books</option>
          <option value="all_user">Alle User Knowledge Books</option>
          <option value="all">Alle Knowledge Books</option>
        </select>
      </label>
      <label>Format
        <select name="format">
          <option value="markdown_bundle">Markdown Bundle</option>
          <option value="json">JSON</option>
          <option value="parquet_manifest">Parquet Manifest</option>
        </select>
      </label>
      <label>Zielpfad <input name="destination" required placeholder="runtime/knowledge/exports/" /></label>
      <label>Export-Anweisung <textarea name="instruction" rows="5" placeholder="Optional: Filter, Namensschema oder Strukturvorgaben"></textarea></label>
    `,
    buildPayload: (data) => ({
      title: `Knowledge Books exportieren · ${data.scope}`,
      instruction: 'Exportiere Knowledge Books aus dem CTOX Knowledge Store mit Skillbook-, Runbook- und DataFrame-Metadaten.',
      export_request: {
        scope: data.scope,
        format: data.format,
        destination: data.destination,
        instruction: data.instruction,
        selected_group_id: state.selectedGroupId,
        selected_skillbook_id: state.selectedSkillbookId,
        selected_knowledge_id: state.selectedId,
      },
    }),
  });
  state.ctx.openLeftDrawer(body);
}

function knowledgeActionDrawer({ title, subtitle, fields, actionLabel, commandType, recordId, commandTitle, buildPayload }) {
  const body = document.createElement('div');
  body.className = 'drawer-body knowledge-edit-drawer knowledge-action-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>${escapeHtml(title)}</h2>
        <p>${escapeHtml(subtitle)}</p>
      </div>
      <button class="ctox-pane-icon" type="button" data-close-drawer aria-label="Schließen">${actionIcon('close')}</button>
    </header>
    <form class="knowledge-action-form">
      <div class="knowledge-action-fields">${fields}</div>
      <footer class="knowledge-drawer-actions">
        <span data-command-status></span>
        <button class="ctox-button is-primary" type="submit" disabled aria-disabled="true">${escapeHtml(actionLabel)}</button>
      </footer>
    </form>
  `;
  const form = body.querySelector('form');
  const status = body.querySelector('[data-command-status]');
  const submitButton = form.querySelector('button[type="submit"]');
  const requiredFields = Array.from(form.querySelectorAll('[required][name]')).map((input) => input.name);
  const updateSubmitState = () => {
    const valid = isKnowledgeActionFormReady(Object.fromEntries(new FormData(form).entries()), requiredFields) && form.checkValidity();
    submitButton.disabled = !valid;
    submitButton.setAttribute('aria-disabled', String(!valid));
  };
  body.querySelector('[data-close-drawer]').addEventListener('click', state.ctx.closeDrawers);
  form.addEventListener('input', updateSubmitState);
  form.addEventListener('change', updateSubmitState);
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!form.reportValidity()) {
      updateSubmitState();
      return;
    }
    const data = Object.fromEntries(new FormData(form).entries());
    status.textContent = 'Sende...';
    const payload = buildPayload(data);
    const result = await dispatchKnowledgeCommand({
      type: commandType,
      record_id: recordId,
      payload: {
        ...payload,
        source_module: 'knowledge',
        selected_group_id: state.selectedGroupId,
        selected_skillbook_id: state.selectedSkillbookId,
        selected_knowledge_id: state.selectedId,
      },
      client_context: {
        action: commandType,
        drawer: title,
      },
    });
    const trackingId = result?.task_id || result?.command_id || '';
    status.textContent = result?.ok ? `Task-ID: ${trackingId || 'angelegt'}` : 'Konnte nicht angelegt werden.';
    showCommandStatus(result);
  });
  updateSubmitState();
  return body;
}

async function openKnowledgeConfig() {
  const item = state.items.find((entry) => entry.id === state.selectedId);
  let markdown = els.markdownEditor.value || els.markdownView.textContent || '';
  const doc = item ? await loadKnowledgeDocument(item.id) : null;
  markdown = doc?.markdown || markdown;
  const body = document.createElement('div');
  body.className = 'drawer-body knowledge-edit-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>${escapeHtml(item?.title || 'Knowledge')}</h2>
        <p>${escapeHtml(`${groupLabel(item?.kind || 'knowledge')} · ${sourceScopeFor(item || {})}`)}</p>
      </div>
      <button class="ctox-pane-icon" type="button" data-close-drawer aria-label="Schließen">${actionIcon('close')}</button>
    </header>
    <dl class="ctox-fields knowledge-drawer-meta">
      <dt>Quelle</dt><dd>${escapeHtml(item?.source_path || 'CTOX Knowledge Store')}</dd>
      <dt>Struktur</dt><dd>${escapeHtml(`${state.groups.length} Gruppen · ${state.items.length} Einträge · ${state.tables.length} Tabellen`)}</dd>
    </dl>
    <div class="knowledge-drawer-editor">
      <textarea data-drawer-markdown required aria-label="Knowledge Markdown bearbeiten">${escapeHtml(markdown)}</textarea>
    </div>
    <footer class="knowledge-drawer-actions">
      <button class="ctox-button is-primary" type="button" data-drawer-save disabled aria-disabled="true">An CTOX geben</button>
    </footer>
  `;
  body.querySelector('[data-close-drawer]').addEventListener('click', state.ctx.closeDrawers);
  const configTextarea = body.querySelector('[data-drawer-markdown]');
  const configSave = body.querySelector('[data-drawer-save]');
  const updateConfigSubmit = () => {
    const valid = Boolean(item?.id && configTextarea.value.trim());
    configSave.disabled = !valid;
    configSave.setAttribute('aria-disabled', String(!valid));
  };
  configTextarea.addEventListener('input', updateConfigSubmit);
  body.querySelector('[data-drawer-save]').addEventListener('click', async () => {
    updateConfigSubmit();
    if (configSave.disabled) return;
    els.markdownEditor.value = configTextarea.value;
    state.editing = true;
    await queueMarkdownSave();
  });
  updateConfigSubmit();
  state.ctx.openLeftDrawer(body);
}

function openRunbookConfig() {
  const runbook = state.runbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId));
  state.ctx.openRightDrawer(drawerContent('Runbook Runtime', [
    ['Ausführung', 'CTOX Task Queue'],
    ['Command history', 'Local command data'],
    ['Ausgewählt', runbook?.title || state.selectedRunbookId || 'kein Runbook'],
    ['Status', runbook?.status || 'unbekannt'],
  ]));
}

function drawerContent(title, rows) {
  const body = document.createElement('div');
  body.className = 'drawer-body';
  const content = Array.isArray(rows)
    ? `<dl class="ctox-fields knowledge-config-list">${rows.map(([key, value]) => `<dt>${escapeHtml(key)}</dt><dd>${escapeHtml(value)}</dd>`).join('')}</dl>`
    : `<p>${escapeHtml(rows)}</p>`;
  body.innerHTML = `<header class="drawer-header-row"><div><h2>${escapeHtml(title)}</h2></div><button class="ctox-pane-icon" type="button" data-close-drawer aria-label="Schließen">${actionIcon('close')}</button></header>${content}`;
  body.querySelector('[data-close-drawer]').addEventListener('click', state.ctx.closeDrawers);
  return body;
}

function initKnowledgeContextMenu() {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu knowledge-context-menu';
  menu.hidden = true;
  els.root.append(menu);
  state.contextMenu = menu;

  window.addEventListener('click', handleContextOutsideClick, { capture: true });
  window.addEventListener('keydown', handleContextEscape);
}

function handleContextEscape(event) {
  if (event.key === 'Escape') hideKnowledgeContextMenu();
}

function handleContextOutsideClick(event) {
  if (state.contextMenu?.contains(event.target)) return;
  hideKnowledgeContextMenu();
}

function commandContextFromElement(target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  const record = element?.closest?.('[data-context-record-id]');
  const panel = element?.closest?.('.knowledge-pane');
  const field = element?.closest?.('input, textarea, select, button');
  const column =
    record?.dataset.knowledgeColumn ||
    (panel?.classList.contains('knowledge-left') ? 'sources' : panel?.classList.contains('knowledge-center') ? 'workspace' : 'module');
  return {
    module: 'knowledge',
    column,
    field: field?.name || field?.dataset.action || field?.dataset.tab || '',
    record_type: record?.dataset.contextRecordType || (state.activeTab === 'data' ? 'dataframe' : 'knowledge'),
    record_id: record?.dataset.contextRecordId || (state.activeTab === 'data' ? activeTableId() : state.selectedId),
    label: record?.dataset.contextLabel || '',
    active_tab: state.activeTab,
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderKnowledgeContextMenu(context, x, y) {
  const canModifyApp = canModifyKnowledgeApp();
  state.contextMenu.innerHTML = `
    <form class="knowledge-context-chat" data-context-chat-form>
      <header>
        <div>
          <strong>Chat to CTOX</strong>
          <span>${escapeHtml(contextSummary(context))}</span>
        </div>
        <button class="ctox-pane-icon" type="button" data-context-close aria-label="Schließen">${actionIcon('close')}</button>
      </header>
      ${canModifyApp ? `
        <div class="ctox-choice-group knowledge-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label class="ctox-choice"><input type="radio" name="contextMode" value="data" checked /><span>Mit Daten arbeiten</span></label>
          <label class="ctox-choice"><input type="radio" name="contextMode" value="app" /><span>App modifizieren</span></label>
        </div>
      ` : ''}
      <textarea class="ctox-textarea" data-context-message placeholder="Was soll CTOX hier tun oder prüfen?"></textarea>
      <footer>
        <span data-context-status></span>
        <button class="ctox-button is-primary" type="submit">Senden</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const rootRect = els.root.getBoundingClientRect();
  const localX = x - rootRect.left;
  const localY = y - rootRect.top;
  const maxLeft = Math.max(8, rootRect.width - rect.width - 8);
  const maxTop = Math.max(8, rootRect.height - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(localX, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(localY, 8, maxTop)}px`;
  const form = state.contextMenu.querySelector('[data-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-context-message]');
  state.contextMenu.querySelector('[data-context-close]')?.addEventListener('click', hideKnowledgeContextMenu);
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
    await dispatchContextChat(context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

function canModifyKnowledgeApp() {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function contextSummary(context) {
  const parts = [
    context.column || 'module',
    context.record_type || '',
    context.label || context.record_id || '',
  ].filter(Boolean);
  return parts.join(' · ') || 'Knowledge';
}

function activeRunbookForContext(context) {
  if (context?.record_type === 'runbook' && context.record_id) {
    const direct = state.runbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, context.record_id));
    if (direct) return direct;
  }
  const group = findGroupForItem(context?.record_id || state.selectedId) || state.groups.find((entry) => entry.id === state.selectedGroupId);
  const groupRunbookIds = new Set((group?.runbookIds || []).map(normaliseRunbookId).filter(Boolean));
  if (groupRunbookIds.size) {
    const selectedInGroup = state.runbooks.find((entry) => groupRunbookIds.has(normaliseRunbookId(entry.id || entry.runbook_id)) && runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId));
    if (selectedInGroup) return selectedInGroup;
    const firstInGroup = state.runbooks.find((entry) => groupRunbookIds.has(normaliseRunbookId(entry.id || entry.runbook_id)));
    if (firstInGroup) return firstInGroup;
  }
  return state.runbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId)) || null;
}

async function dispatchContextAction(action, context) {
  const item = itemForCommandContext(context);
  const runbook = activeRunbookForContext(context);
  const selectedKnowledgeId = selectedKnowledgeIdForContext(context, item);
  const selectedTableId = selectedTableIdForContext(context, item);
  const selectedRunbookId = normaliseRunbookId(runbook?.id || runbook?.runbook_id || '');
  if (selectedRunbookId) state.selectedRunbookId = selectedRunbookId;
  const result = await dispatchKnowledgeCommand({
    type: action.type,
    record_id: context.record_id,
    payload: {
      title: `${action.label} · ${context.label || item?.title || runbook?.title || 'Knowledge'}`,
      instruction: `${action.label}. Nutze den Kontext aus dem Knowledge-Modul und schreibe Änderungen über die CTOX Queue, nicht direkt im Browser.`,
      selected_item: item,
      selected_runbook: runbook,
      selected_table_id: selectedTableId,
      context,
    },
    client_context: {
      action: 'context-menu',
      context_action: action.type,
      column: context.column,
      record_type: context.record_type,
      selected_knowledge_id: selectedKnowledgeId,
      selected_runbook_id: selectedRunbookId,
      selected_table_id: selectedTableId,
    },
  });
  showCommandStatus(result);
}

async function dispatchContextChat(context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-context-status]');
  if (!trimmed) {
    if (status) status.textContent = 'Nachricht fehlt.';
    return;
  }
  const safeMode = mode === 'app' && canModifyKnowledgeApp() ? 'app' : 'data';
  const item = itemForCommandContext(context);
  const runbook = activeRunbookForContext(context);
  const selectedKnowledgeId = selectedKnowledgeIdForContext(context, item);
  const selectedTableId = selectedTableIdForContext(context, item);
  const selectedRunbookId = normaliseRunbookId(runbook?.id || runbook?.runbook_id || '');
  if (status) status.textContent = 'Sende...';
  const result = await dispatchKnowledgeCommand({
    type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'ctox.knowledge.chat',
    record_id: safeMode === 'app' ? 'knowledge' : (context.record_id || selectedKnowledgeId || 'knowledge'),
    payload: {
      title: `${safeMode === 'app' ? 'Knowledge App modifizieren' : 'Knowledge Daten bearbeiten'} · ${context.label || item?.title || runbook?.title || context.column || 'Knowledge'}`,
      instruction: safeMode === 'app'
        ? `Modifiziere die Knowledge-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Daten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
        : trimmed,
      prompt: trimmed,
      user_message: trimmed,
      mode: safeMode,
      target: safeMode === 'app' ? 'app' : 'data',
      selected_item: item,
      selected_runbook: runbook,
      selected_table_id: selectedTableId,
      context,
      thread_key: 'business-os/knowledge',
    },
    client_context: {
      action: 'context-chat',
      mode: safeMode,
      column: context.column,
      record_type: context.record_type,
      selected_knowledge_id: selectedKnowledgeId,
      selected_runbook_id: selectedRunbookId,
      selected_table_id: selectedTableId,
    },
  });
  const trackingId = result?.task_id || result?.command_id || '';
  if (result?.ok && trackingId) rememberCtoxTask({ taskId: result.task_id, commandId: result.command_id, trackingId, context, mode: safeMode });
  if (status) {
    if (result?.ok) {
      status.innerHTML = `Task-ID: <code>${escapeHtml(trackingId || 'unbekannt')}</code> <button class="ctox-button" type="button" data-open-ctox-task>Im CTOX Modul öffnen</button>`;
      status.querySelector('[data-open-ctox-task]')?.addEventListener('click', () => {
        hideKnowledgeContextMenu();
        location.hash = 'ctox';
      });
    } else {
      status.textContent = 'Konnte nicht angelegt werden.';
    }
  }
}

function rememberCtoxTask({ taskId, commandId, trackingId, context, mode }) {
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify({
      taskId: taskId || trackingId,
      commandId: commandId || '',
      module: 'knowledge',
      source: 'knowledge-context-chat',
      mode,
      recordId: context?.record_id || '',
      recordType: context?.record_type || '',
      label: context?.label || '',
      createdAt: new Date().toISOString(),
    }));
  } catch (_) {
    // Ignore unavailable session storage.
  }
}

function itemForCommandContext(context) {
  const recordId = context?.record_id || '';
  return state.items.find((entry) => entry.id === recordId)
    || state.items.find((entry) => entry.id === state.selectedId)
    || null;
}

function selectedKnowledgeIdForContext(context, item) {
  const recordId = context?.record_id || '';
  if (recordId && context?.record_type !== 'knowledge-group') return recordId;
  if (item?.id) return item.id;
  const group = state.groups.find((entry) => entry.id === recordId);
  return group?.primaryItemId || state.selectedId || '';
}

function selectedTableIdForContext(context, item) {
  if (context?.record_type === 'dataframe' && context.record_id) return context.record_id;
  if (item?.has_table && item.id) return item.id;
  const group = findGroupForItem(context?.record_id || item?.id || state.selectedId)
    || state.groups.find((entry) => entry.id === context?.record_id)
    || null;
  if (!group) return '';
  const contextTables = skillbookContext(group, state.selectedSkillbookId).tables;
  if (state.selectedTableId && contextTables.some((entry) => entry.id === state.selectedTableId)) return state.selectedTableId;
  return contextTables[0]?.id || '';
}

function hideKnowledgeContextMenu() {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function handleShellMessage(event) {
  if (event.data?.type === 'ctox-business-os-language') {
    state.lang = event.data.lang === 'en' ? 'en' : 'de';
  }
}

function localDataFrameSchema(item) {
  const completeness = dataFrameCompleteness(item);
  const rows = completeness.rows;
  const rawColumns = firstArray(
    item?.columns,
    item?.schema?.columns,
    item?.payload?.columns,
    item?.payload?.schema?.columns,
    item?.dataframe?.columns,
    item?.payload?.dataframe?.columns,
  );
  const columns = normalizeColumns(rawColumns?.length ? rawColumns : Object.keys(rows[0] || {}));
  return {
    title: item?.title || item?.payload?.title || 'DataFrame',
    columns,
    row_count: Number(completeness.expectedRows ?? item?.row_count ?? item?.payload?.row_count ?? rows.length),
    complete: completeness.complete,
    completeness,
  };
}

function localDataFrameRows(item) {
  return dataFrameCompleteness(item).rows;
}

function rawDataFrameRows(item) {
  const rows = firstArray(
    item?.rows,
    item?.records,
    item?.data,
    item?.payload?.rows,
    item?.payload?.records,
    item?.payload?.data,
    item?.dataframe?.rows,
    item?.payload?.dataframe?.rows,
  );
  return rows.map((row) => row && typeof row === 'object' ? row : { value: row });
}

function dataFrameCompleteness(item) {
  const chunks = dataframeChunks(item);
  const rootRowsComplete = explicitRowsComplete(item);
  if (!chunks && rootRowsComplete !== undefined && rootRowsComplete !== true) {
    return incompleteDataFrame(rootRowsComplete === false ? 'rows_complete=false' : 'invalid rows_complete');
  }
  if (!chunks) {
    const rows = rawDataFrameRows(item);
    return {
      complete: true,
      rows,
      expectedRows: rows.length,
      actualRows: rows.length,
      chunkCount: 1,
      reason: '',
    };
  }
  return validateKnowledgeTableChunks(chunks, item);
}

function dataframeChunks(item) {
  const explicitChunks = [
    item?.chunks,
    item?.payload?.chunks,
    item?.dataframe?.chunks,
    item?.payload?.dataframe?.chunks,
  ].find(Array.isArray);
  if (explicitChunks) return explicitChunks;
  if (hasChunkMetadata(item)) return [item];
  return null;
}

function hasChunkMetadata(record) {
  return [
    'chunk_index',
    'chunk_count',
    'row_offset',
    'rows_offset',
    'rows_complete',
  ].some((key) => firstPresentValue(record, [key]) !== undefined);
}

function validateKnowledgeTableChunks(chunks, table = {}) {
  const records = Array.isArray(chunks) ? chunks : [];
  const metadataRecords = [table, ...records].filter((record) => record && typeof record === 'object');
  if (!records.length) return incompleteDataFrame('no chunks');

  const chunkCountCandidates = metadataRecords.flatMap((record) => numericFieldValues(record, ['chunk_count']));
  if (chunkCountCandidates.some((value) => !Number.isFinite(value))) {
    return incompleteDataFrame('invalid chunk_count');
  }
  const chunkCountValues = uniqueNumbers(chunkCountCandidates);
  if (chunkCountValues.length !== 1 || chunkCountValues[0] < 1) {
    return incompleteDataFrame('inconsistent chunk_count');
  }
  const expectedChunkCount = chunkCountValues[0];
  if (expectedChunkCount !== records.length) {
    return incompleteDataFrame(`chunk_count=${expectedChunkCount}, received=${records.length}`);
  }

  const normalized = records.map((record, position) => {
    const rows = rawDataFrameRows(record);
    if (['chunk_index', 'row_offset', 'rows_offset', 'chunk_row_offset', 'row_start', 'start_row', 'start_offset', 'offset', 'chunk_row_count', 'rows_count', 'row_count_in_chunk', 'row_count']
      .some((key) => numericFieldValues(record, [key]).length > 1 && new Set(numericFieldValues(record, [key])).size > 1)) {
      return { record, position, invalidMetadata: true, rows };
    }
    return {
      record,
      position,
      index: numericField(record, ['chunk_index']),
      offset: numericField(record, ['row_offset', 'rows_offset', 'chunk_row_offset', 'row_start', 'start_row', 'start_offset', 'offset']),
      rowCount: numericField(record, ['chunk_row_count', 'rows_count', 'row_count_in_chunk']),
      rows,
      rowsComplete: explicitRowsComplete(record),
    };
  });

  if (normalized.some((chunk) => chunk.invalidMetadata)) {
    return incompleteDataFrame('conflicting chunk metadata');
  }

  if (normalized.some((chunk) => !Number.isInteger(chunk.index) || chunk.index < 0)) {
    return incompleteDataFrame('invalid chunk_index');
  }
  if (new Set(normalized.map((chunk) => chunk.index)).size !== normalized.length) {
    return incompleteDataFrame('duplicate chunk_index');
  }
  const sorted = [...normalized].sort((left, right) => left.index - right.index);
  if (sorted.some((chunk, index) => chunk.index !== index)) {
    return incompleteDataFrame('non-contiguous chunk_index');
  }
  // Offsets are derivable when absent everywhere: chunk_index is already proven
  // contiguous, so cumulative row lengths ARE the offsets. Only a mix of
  // explicit and missing offsets stays fail-closed.
  if (sorted.every((chunk) => chunk.offset == null)) {
    let derived = 0;
    for (const chunk of sorted) {
      chunk.offset = derived;
      derived += chunk.rows.length;
    }
  }
  if (sorted.some((chunk) => !Number.isInteger(chunk.offset) || chunk.offset < 0)) {
    return incompleteDataFrame('missing or invalid row offset');
  }
  if (sorted.some((chunk) => chunk.rowCount != null && chunk.rowCount !== chunk.rows.length)) {
    return incompleteDataFrame('chunk row total mismatch');
  }
  const rowsCompleteValues = metadataRecords.flatMap((record) => explicitRowsCompleteValues(record));
  if (rowsCompleteValues.some((value) => value !== true && value !== false)) {
    return incompleteDataFrame('invalid rows_complete');
  }
  if (rowsCompleteValues.some((value) => value === false)) {
    return incompleteDataFrame('rows_complete=false');
  }

  const totalCandidates = [];
  for (const key of ['rows_total', 'total_rows', 'total_row_count', 'expected_row_count']) {
    for (const record of metadataRecords) {
      totalCandidates.push(...numericFieldValues(record, [key]));
    }
  }
  totalCandidates.push(...numericFieldValues(table, ['row_count']));
  if (!totalCandidates.length) {
    const chunkRowCounts = records.map((record) => numericField(record, ['row_count']));
    if (chunkRowCounts.every((value) => value != null && value === chunkRowCounts[0])) {
      totalCandidates.push(chunkRowCounts[0]);
    }
  }
  const uniqueTotals = uniqueNumbers(totalCandidates);
  if (totalCandidates.some((value) => !Number.isFinite(value)) || uniqueTotals.length !== 1 || uniqueTotals[0] < 0) {
    return incompleteDataFrame('inconsistent or missing row total');
  }
  const expectedRows = uniqueTotals[0];

  const chunkRowCountValues = records.map((record) => numericFieldValues(record, ['row_count']));
  const presentChunkRowCounts = chunkRowCountValues.filter((values) => values.length);
  if (presentChunkRowCounts.length && presentChunkRowCounts.length !== records.length) {
    return incompleteDataFrame('missing chunk row_count');
  }
  if (presentChunkRowCounts.length === records.length) {
    const rowCounts = sorted.map((chunk) => numericFieldValues(chunk.record, ['row_count'])[0]);
    const countsAreTotal = rowCounts.every((value) => value === expectedRows);
    const countsAreChunkSizes = rowCounts.every((value, index) => value === sorted[index].rows.length);
    if (rowCounts.some((value) => !Number.isFinite(value)) || (!countsAreTotal && !countsAreChunkSizes)) {
      return incompleteDataFrame('chunk row_count mismatch');
    }
  }

  let nextOffset = 0;
  for (const chunk of sorted) {
    if (chunk.offset !== nextOffset) return incompleteDataFrame('row offsets contain a gap or overlap');
    nextOffset += chunk.rows.length;
  }
  if (nextOffset !== expectedRows) {
    return incompleteDataFrame(`row total=${expectedRows}, assembled=${nextOffset}`);
  }

  return {
    complete: true,
    rows: sorted.flatMap((chunk) => chunk.rows),
    expectedRows,
    actualRows: nextOffset,
    chunkCount: expectedChunkCount,
    reason: '',
  };
}

function incompleteDataFrame(reason) {
  return {
    complete: false,
    rows: [],
    expectedRows: null,
    actualRows: 0,
    chunkCount: 0,
    reason: String(reason || 'unknown chunk error'),
  };
}

function explicitRowsComplete(record) {
  const value = explicitRowsCompleteValues(record)[0];
  if (value === undefined || value === null || value === '') return undefined;
  return value;
}

function explicitRowsCompleteValues(record) {
  return rawFieldValues(record, ['rows_complete']).map((value) => {
    if (value === true || value === false) return value;
    if (typeof value === 'string' && value.trim().toLowerCase() === 'true') return true;
    if (typeof value === 'string' && value.trim().toLowerCase() === 'false') return false;
    return value;
  });
}

function numericField(record, keys) {
  const value = numericFieldValues(record, keys)[0];
  if (value === undefined || value === null || value === '') return null;
  return Number.isFinite(value) ? value : null;
}

function numericFieldValues(record, keys) {
  return rawFieldValues(record, keys).map((value) => Number(value));
}

function rawFieldValues(record, keys) {
  const payload = record?.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : null;
  const nested = record?.dataframe && typeof record.dataframe === 'object' && !Array.isArray(record.dataframe)
    ? record.dataframe
    : null;
  const containers = [record, payload, nested].filter(Boolean);
  return keys.flatMap((key) => containers
    .filter((container) => valuePresent(container, key))
    .map((container) => container[key])
    .filter((value) => value !== undefined && value !== null && value !== ''));
}

function firstPresentValue(record, keys) {
  const payload = record?.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : null;
  const nested = record?.dataframe && typeof record.dataframe === 'object' && !Array.isArray(record.dataframe)
    ? record.dataframe
    : null;
  for (const key of keys) {
    if (valuePresent(record, key)) return record[key];
    if (valuePresent(payload, key)) return payload[key];
    if (valuePresent(nested, key)) return nested[key];
  }
  return undefined;
}

function valuePresent(record, key) {
  return Boolean(record && Object.prototype.hasOwnProperty.call(record, key));
}

function uniqueNumbers(values) {
  return [...new Set(values.filter((value) => Number.isFinite(value)))];
}

function firstArray(...values) {
  return values.find(Array.isArray) || [];
}

function normalizeColumns(columns) {
  return (columns || []).map((column) => {
    if (typeof column === 'string') return enrichColumn({ key: column, name: column, label: titleFromSlug(column) });
    const key = column?.key || column?.id || column?.name || column?.field || '';
    return enrichColumn({ ...column, key, name: column?.name || key, label: column?.label || column?.title || titleFromSlug(key) });
  }).filter((column) => column.key);
}

function enrichColumn(column) {
  const inferred = inferColumnSemantics(column);
  const unit = normalizeUnit(inferred.unit || column.unit || column.units || '');
  return {
    ...column,
    unit,
    description: column.description || column.help || inferred.description || '',
    metricUnit: inferred.metricUnit || unit,
    valueKind: column.valueKind || inferred.valueKind || '',
  };
}

function inferColumnSemantics(column = {}) {
  const key = String(column.key || column.name || column.label || '').toLowerCase();
  const nameUnit = inferredUnitFromColumnName(column);
  const declaredUnit = normalizeUnit(column.unit || column.units || '');
  const explicitUnit = nameUnit && nameUnit !== declaredUnit ? nameUnit : (declaredUnit || nameUnit);
  if (key.includes('propeller') && (key.includes('size') || key.includes('dimension'))) {
    return {
      unit: explicitUnit || 'in',
      metricUnit: 'mm',
      valueKind: 'propeller_size',
      description: 'Propellergröße wie 9x5 bedeutet 9 Zoll Durchmesser und 5 Zoll Steigung. Knowledge normalisiert dies als Durchmesser x Steigung in Millimetern für metrischen Vergleich und CSV-Export.',
    };
  }
  if (/load_case|case$|measurement_kind|derivation_method/.test(key)) {
    return {
      unit: explicitUnit,
      metricUnit: explicitUnit,
      valueKind: '',
      description: 'Categorical provenance field; use it for grouping and filtering, not as a numeric measurement.',
    };
  }
  const unitByToken = [
    [/thrust|force|load|bearing_load|weight_force/, 'N', 'Kraft oder Last in Newton.'],
    [/torque|moment/, 'N m', 'Moment beziehungsweise Drehmoment in Newtonmetern.'],
    [/diameter|width|height|length|span|pitch|distance/, 'mm', 'Länge in Millimetern.'],
    [/mass/, 'kg', 'Mass in kilograms.'],
    [/weight/, 'kg', 'Weight/mass value normalized to kilograms when source data carries mass units.'],
    [/voltage|volt/, 'V', 'Voltage in volts.'],
    [/current|amp/, 'A', 'Current in amperes.'],
    [/power|watt/, 'W', 'Power in watts.'],
    [/capacity/, 'Ah', 'Capacity in ampere hours unless the source column states another unit.'],
    [/rpm|rev/, 'rpm', 'Rotational speed in revolutions per minute.'],
    [/speed|velocity/, 'm/s', 'Speed in metres per second.'],
    [/temperature|temp/, 'deg C', 'Temperature in degrees Celsius.'],
  ];
  for (const [pattern, unit, description] of unitByToken) {
    if (pattern.test(key)) return { unit: explicitUnit || unit, metricUnit: normalizeUnit(unit), valueKind: 'numeric', description };
  }
  if (/score|ratio|percent|share|count|qty|quantity|number|index|value/.test(key)) {
    return { unit: explicitUnit, metricUnit: explicitUnit, valueKind: 'numeric', description: 'Numeric value; exported without thousands separators and with a comma as decimal separator.' };
  }
  return { unit: explicitUnit, metricUnit: explicitUnit, valueKind: '' };
}

function inferredUnitFromColumnName(column = {}) {
  const candidates = [column.key, column.name, column.field, column.id, column.label, column.title].filter(Boolean);
  for (const candidate of candidates) {
    const unit = inferredUnitFromText(candidate);
    if (unit) return unit;
  }
  return '';
}

function inferredUnitFromText(value = '') {
  const tokens = String(value || '')
    .toLowerCase()
    .split(/[^a-z0-9°/]+/)
    .filter(Boolean);
  if (!tokens.length) return '';
  const last = tokens[tokens.length - 1];
  const previous = tokens[tokens.length - 2] || '';
  if ((previous === 'n' || previous === 'newton') && (last === 'm' || last === 'meter' || last === 'metre')) return 'N m';
  if (last === 'nm') return 'N m';
  const unit = normalizeUnit(last);
  return unit === last && !knownUnitToken(last) ? '' : unit;
}

function knownUnitToken(token) {
  return new Set([
    'n',
    'newton',
    'newtons',
    'nm',
    'mm',
    'millimeter',
    'millimetre',
    'cm',
    'm',
    'meter',
    'metre',
    'in',
    'inch',
    'inches',
    'ft',
    'feet',
    'kg',
    'kilogram',
    'lb',
    'lbs',
    'pound',
    'oz',
    'v',
    'volt',
    'a',
    'amp',
    'ampere',
    'w',
    'watt',
    'ah',
    'rpm',
    'm/s',
    'km/h',
    'c',
    '°c',
    'celsius',
  ]).has(token);
}

function normalizeUnit(unit) {
  const raw = String(unit || '').trim();
  if (!raw) return '';
  const normal = raw.toLowerCase().replace(/\s+/g, ' ');
  return ({
    n: 'N',
    newton: 'N',
    newtons: 'N',
    nm: 'N m',
    'n*m': 'N m',
    'n-m': 'N m',
    'newton meter': 'N m',
    'newton metre': 'N m',
    mm: 'mm',
    millimeter: 'mm',
    millimetre: 'mm',
    cm: 'cm',
    m: 'm',
    meter: 'm',
    metre: 'm',
    in: 'in',
    inch: 'in',
    inches: 'in',
    ft: 'ft',
    feet: 'ft',
    kg: 'kg',
    kilogram: 'kg',
    g: 'g',
    gram: 'g',
    lb: 'lb',
    lbs: 'lb',
    pound: 'lb',
    oz: 'oz',
    v: 'V',
    volt: 'V',
    a: 'A',
    amp: 'A',
    ampere: 'A',
    w: 'W',
    watt: 'W',
    ah: 'Ah',
    rpm: 'rpm',
    'm/s': 'm/s',
    'km/h': 'km/h',
    c: 'deg C',
    '°c': 'deg C',
    celsius: 'deg C',
  })[normal] || raw;
}

function columnHeaderLabel(column) {
  const base = String(column.label || column.name || column.key || 'Column').trim();
  const metricUnit = metricUnitForColumn(column);
  if (!metricUnit) return localizedColumnBaseLabel(base, column);
  let label = localizedColumnBaseLabel(stripUnitSuffix(base, metricUnit), column);
  if (column.unit && column.unit !== metricUnit) label = stripUnitSuffix(label, column.unit);
  if (column.valueKind === 'propeller_size') return `${label} (Durchmesser x Steigung, ${metricUnit})`;
  return `${label} (${metricUnit})`;
}

function localizedColumnBaseLabel(label, column = {}) {
  const key = String(column.key || column.name || column.field || column.id || '').toLowerCase();
  if (column.valueKind === 'propeller_size' || /propeller.*(size|dimension)|prop.*size/.test(key)) return 'Propellergröße';
  if (/prop.*diameter|diameter/.test(key)) return 'Durchmesser';
  if (/prop.*pitch|pitch/.test(key)) return 'Steigung';
  if (/torque|moment/.test(key)) return 'Moment/Torque';
  if (/rpm|rev/.test(key)) return 'Drehzahl';
  if (/thrust|force/.test(key)) return 'Kraft';
  return label;
}

function labelEndsWithUnitInParens(label, unit) {
  const suffix = `(${String(unit || '').trim()})`.toLocaleLowerCase();
  return Boolean(suffix.length > 2 && String(label || '').trim().toLocaleLowerCase().endsWith(suffix));
}

function stripUnitSuffix(label, unit = '') {
  let value = String(label || '').replace(/\s*\([^)]*\)\s*$/g, '').trim();
  if (!unit) return value;
  const compactUnit = String(unit).replace(/\s+/g, '');
  const aliases = new Set([unit, compactUnit]);
  if (unit === 'N m') aliases.add('Nm');
  for (const alias of aliases) {
    if (!alias) continue;
    value = stripTrailingUnitAlias(value, alias);
  }
  return value || String(label || '').trim();
}

function stripTrailingUnitAlias(value, alias) {
  const text = String(value || '');
  const normalizedAlias = String(alias || '').trim().toLocaleLowerCase();
  if (!text || !normalizedAlias) return text.trim();
  const lower = text.toLocaleLowerCase();
  if (!lower.endsWith(normalizedAlias)) return text.trim();
  const prefix = text.slice(0, text.length - normalizedAlias.length);
  if (!prefix || !/[\s_-]$/.test(prefix)) return text.trim();
  return prefix.replace(/[\s_-]+$/g, '').trim();
}

function metricUnitForColumn(column) {
  if (column.valueKind === 'propeller_size') return 'mm';
  return ({
    in: 'mm',
    ft: 'm',
    lb: 'kg',
    oz: 'g',
  })[column.unit] || column.metricUnit || column.unit || '';
}

function columnHeaderHelp(column) {
  const parts = [];
  if (column.description) parts.push(column.description);
  if (column.unit) parts.push(`Source unit: ${column.unit}.`);
  const metricUnit = metricUnitForColumn(column);
  if (metricUnit && metricUnit !== column.unit) parts.push(`Shown/exported metric unit: ${metricUnit}.`);
  if (column.dtype || column.type) parts.push(`Type: ${column.dtype || column.type}.`);
  if (column.key) parts.push(`Column key: ${column.key}.`);
  if (!parts.length) return '';
  return parts.join(' ');
}

function localMarkdownForItem(item) {
  if (!item) return '';
  const candidates = [
    item.markdown,
    item.content_markdown,
    item.document_markdown,
    item.skill_markdown,
    item.prompt_markdown,
    item.chunk_text,
    item.payload?.markdown,
    item.payload?.content_markdown,
    item.payload?.document_markdown,
    item.payload?.chunk_text,
    item.payload?.text,
  ];
  return candidates.find((value) => typeof value === 'string' && value.trim())
    || proceduralKnowledgeMarkdown(item);
}

function proceduralKnowledgeMarkdown(item) {
  const kind = String(item?.kind || '').toLowerCase();
  if (!['skill', 'skillbook', 'runbook'].includes(kind)) return '';
  const sections = [];
  const title = item.title || item.name || item.id || 'Knowledge';
  const mission = firstKnowledgeText(item, ['mission', 'summary', 'description', 'entry_action']);
  sections.push(`# ${title}`);
  if (mission) sections.push(mission);
  appendMarkdownSection(sections, 'Einsatz', firstKnowledgeValue(item, ['entry_action', 'primary_channel', 'routing_taxonomy']));
  appendMarkdownSection(sections, 'Nicht verhandelbare Regeln', firstKnowledgeValue(item, ['non_negotiable_rules', 'guardrails', 'constraints']));
  appendMarkdownSection(sections, 'Erforderliche Eingaben', firstKnowledgeValue(item, ['inputs', 'required_inputs', 'input_schema', 'resolver_contract']));
  appendMarkdownSection(sections, 'Arbeitsablauf', firstKnowledgeValue(item, ['workflow_backbone', 'resolve_flow', 'steps', 'tool_actions']));
  appendMarkdownSection(sections, 'Prüf- und Abbruchbedingungen', firstKnowledgeValue(item, ['verification', 'quality_gates', 'escalate_when', 'runtime_policy']));
  appendMarkdownSection(sections, 'Ergebnisvertrag', firstKnowledgeValue(item, ['answer_contract', 'execution_contract', 'output_schema', 'writeback_flow', 'writeback_policy']));
  appendMarkdownSection(sections, 'Verknüpfte Runbooks', firstKnowledgeValue(item, ['linked_runbooks', 'linked_runbook_ids']));
  return sections.length > 1 ? sections.join('\n\n') : '';
}

function firstKnowledgeText(item, keys) {
  const value = firstKnowledgeValue(item, keys);
  if (typeof value === 'string') return value.trim();
  return '';
}

function firstKnowledgeValue(item, keys) {
  for (const key of keys) {
    const value = item?.[key] ?? item?.payload?.[key];
    if (value !== undefined && value !== null && value !== '') return value;
  }
  return null;
}

function appendMarkdownSection(sections, title, value) {
  const items = listValue(value);
  if (!items.length) return;
  sections.push(`## ${title}\n\n${items.map((entry) => `- ${entry}`).join('\n')}`);
}

function listValue(value) {
  if (value == null || value === '') return [];
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return [];
    if ((trimmed.startsWith('[') || trimmed.startsWith('{'))) {
      try {
        return listValue(JSON.parse(trimmed));
      } catch {}
    }
    return trimmed.split(/\r?\n|;\s*/).map((entry) => entry.replace(/^[-*]\s*/, '').trim()).filter(Boolean);
  }
  if (Array.isArray(value)) return value.flatMap(listValue);
  if (typeof value === 'object') {
    return Object.entries(value).map(([key, entry]) => `${titleFromSlug(key)}: ${knowledgeValueText(entry)}`);
  }
  return [String(value)];
}

function knowledgeValueText(value) {
  if (Array.isArray(value)) return value.map(knowledgeValueText).join(', ');
  if (value && typeof value === 'object') {
    return Object.entries(value).map(([key, entry]) => `${titleFromSlug(key)}=${knowledgeValueText(entry)}`).join('; ');
  }
  return String(value ?? '');
}

function renderKnowledgeListSection(title, items) {
  if (!items.length) return '';
  return `<section><h2>${escapeHtml(title)}</h2><ul>${items.map((entry) => `<li>${escapeHtml(entry)}</li>`).join('')}</ul></section>`;
}

function markdownToHtml(markdown) {
  const lines = String(markdown || '').replace(/\r\n/g, '\n').split('\n');
  const html = [];
  let paragraph = [];
  let list = false;
  let code = null;
  const flushParagraph = () => {
    if (paragraph.length) {
      html.push(`<p>${inlineMarkdown(paragraph.join(' '))}</p>`);
      paragraph = [];
    }
  };
  const closeList = () => {
    if (list) {
      html.push('</ul>');
      list = false;
    }
  };
  for (const line of lines) {
    if (line.startsWith('```')) {
      flushParagraph();
      closeList();
      if (code) {
        html.push(`<pre><code>${escapeHtml(code.join('\n'))}</code></pre>`);
        code = null;
      } else {
        code = [];
      }
      continue;
    }
    if (code) {
      code.push(line);
      continue;
    }
    if (!line.trim()) {
      flushParagraph();
      closeList();
      continue;
    }
    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      flushParagraph();
      closeList();
      html.push(`<h${heading[1].length}>${inlineMarkdown(heading[2])}</h${heading[1].length}>`);
      continue;
    }
    const bullet = /^[-*]\s+(.+)$/.exec(line);
    if (bullet) {
      flushParagraph();
      if (!list) {
        html.push('<ul>');
        list = true;
      }
      html.push(`<li>${inlineMarkdown(bullet[1])}</li>`);
      continue;
    }
    paragraph.push(line.trim());
  }
  flushParagraph();
  closeList();
  return html.join('\n');
}

function inlineMarkdown(value) {
  return escapeHtml(value)
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/`(.+?)`/g, '<code>$1</code>');
}

function groupBy(items, getKey) {
  return items.reduce((acc, item) => {
    const key = getKey(item);
    acc[key] ||= [];
    acc[key].push(item);
    return acc;
  }, {});
}

function groupLabel(kind) {
  return ({
    skill: 'Skills',
    skillbook: 'Skillbooks',
    runbook: 'Runbooks',
    dataframe: 'DataFrames',
  })[kind] || 'Knowledge';
}

function formatCell(value, column = null) {
  const normalized = canonicalCellValue(value, column);
  if (normalized == null) return '';
  return String(normalized);
}

function canonicalCellValue(value, column = null) {
  if (value == null) return '';
  if (column?.valueKind === 'propeller_size') return normalizePropellerSize(value);
  if (typeof value === 'number') return convertAndFormatNumber(value, column);
  if (typeof value === 'string') {
    const normalizedPropeller = column?.valueKind === 'propeller_size' ? normalizePropellerSize(value) : '';
    if (normalizedPropeller) return normalizedPropeller;
    const parsed = parseNumericString(value);
    if (parsed != null && isNumericColumn(column)) return convertAndFormatNumber(parsed, column);
    return value.trim();
  }
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

function isNumericColumn(column) {
  if (!column) return false;
  const type = String(column.type || column.dtype || '').toLowerCase();
  if (/number|integer|float|double|decimal/.test(type)) return true;
  return Boolean(column.valueKind === 'numeric' || column.unit || column.metricUnit);
}

function convertAndFormatNumber(value, column = null) {
  const converted = convertToMetric(Number(value), column);
  return formatPlainNumber(converted);
}

function convertToMetric(value, column = null) {
  if (!Number.isFinite(value) || !column?.unit) return value;
  if (column.unit === 'in') return value * 25.4;
  if (column.unit === 'ft') return value * 0.3048;
  if (column.unit === 'lb') return value * 0.45359237;
  if (column.unit === 'oz') return value * 28.349523125;
  return value;
}

function normalizePropellerSize(value) {
  const text = String(value ?? '').trim().toLowerCase().replace(/\s+/g, '');
  const match = text.match(/^([0-9]+(?:[.,][0-9]+)?)x([0-9]+(?:[.,][0-9]+)?)(?:in|inch|")?$/);
  if (!match) return String(value ?? '').trim();
  const diameterIn = parseNumericString(match[1]);
  const pitchIn = parseNumericString(match[2]);
  if (diameterIn == null || pitchIn == null) return String(value ?? '').trim();
  return `${formatPlainNumber(diameterIn * 25.4)} x ${formatPlainNumber(pitchIn * 25.4)}`;
}

function parseNumericString(value) {
  const text = String(value ?? '').trim();
  if (!text) return null;
  if (!/^[+-]?(?:\d+|\d{1,3}(?:[., ]\d{3})+)(?:[.,]\d+)?$/.test(text)) return null;
  let normalized = text.replace(/\s+/g, '');
  const lastComma = normalized.lastIndexOf(',');
  const lastDot = normalized.lastIndexOf('.');
  if (lastComma >= 0 && lastDot >= 0) {
    const decimal = lastComma > lastDot ? ',' : '.';
    const thousands = decimal === ',' ? /\./g : /,/g;
    normalized = normalized.replace(thousands, '').replace(decimal, '.');
  } else if (lastComma >= 0) {
    const groups = normalized.split(',');
    normalized = groups.length === 2 && groups[1].length !== 3
      ? normalized.replace(',', '.')
      : normalized.replace(/,/g, '');
  } else if ((normalized.match(/\./g) || []).length > 1) {
    normalized = normalized.replace(/\./g, '');
  }
  const number = Number(normalized);
  return Number.isFinite(number) ? number : null;
}

function formatPlainNumber(value, decimalSeparator = ',') {
  if (!Number.isFinite(value)) return '';
  const clean = Object.is(value, -0) ? 0 : value;
  if (Number.isInteger(clean)) return String(clean);
  let text = clean.toString();
  if (text.includes('e')) text = clean.toFixed(12);
  return text
    .replace(/(\.\d*?[1-9])0+$/g, '$1')
    .replace(/\.0+$/g, '')
    .replace('.', decimalSeparator);
}

function exportActiveTableCsv() {
  const tableId = activeTableId();
  const item = state.items.find((entry) => entry.id === tableId);
  const tableRecord = tableForItem(item || { id: tableId }, state.tables);
  const tableSource = mergeKnowledgeTableData(item, tableRecord);
  if (!tableSource?.has_table) return;
  const completeness = dataFrameCompleteness(tableSource);
  if (!completeness.complete) return;
  const rows = completeness.rows;
  const schema = localDataFrameSchema(tableSource);
  const csv = dataframeToCsv(schema.columns || [], rows);
  downloadTextFile(
    `${normaliseName(schema.title || tableSource.title || tableId || 'knowledge-table') || 'knowledge-table'}.csv`,
    csv,
    'text/csv;charset=utf-8'
  );
}

function dataframeToCsv(columns, rows) {
  const normalColumns = normalizeColumns(columns);
  const delimiter = ';';
  const header = normalColumns.map((column) => csvEscape(columnHeaderLabel(column))).join(delimiter);
  const body = rows.map((row) => normalColumns.map((column) => (
    csvEscape(canonicalCellValue(valueForColumn(row, column), column))
  )).join(delimiter));
  return [header, ...body].join('\n');
}

function csvEscape(value) {
  const text = String(value ?? '');
  return /[";\n\r]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

function downloadTextFile(filename, content, type = 'text/plain;charset=utf-8') {
  const blob = new Blob([content], { type });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  // Der Desktop-Body ist nicht der Host dieses Moduls: ein Anker dort haengt
  // ausserhalb des App-Fensters (siehe scripts/assert-shell-v2-contract.mjs,
  // Regel body-overlay).
  (els.root || state.ctx?.host || document.documentElement).append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  })[char]);
}

function isKnowledgeActionFormReady(values, requiredFields = []) {
  return requiredFields.every((name) => String(values?.[name] || '').trim().length > 0);
}

export const __knowledgeTestHooks = {
  renderKnowledgeBundle,
  groupScopeLabel,
  shortDay,
  buildKnowledgeBundles,
  isInternalSkillOnlyGroup,
  canEditSelectedMarkdown,
  isKnowledgeActionFormReady,
  isKnowledgeTabDisabled,
  knowledgeItemsFromTables,
  mergeKnowledgeTableChunks,
  knowledgeGroupMatchesDomain,
  knowledgeEmptyStateMessage,
  knowledgeListStateHtml,
  runCoalescedRefresh,
  validateKnowledgeTableChunks,
  dataFrameCompleteness,
  localDataFrameRows,
  localDataFrameSchema,
  mergeKnowledgeTableData,
  canonicalCellValue,
  columnHeaderHelp,
  columnHeaderLabel,
  dataframeToCsv,
  formatCell,
  normalizeStoredKnowledgeRecord,
  normalizeColumns,
  knowledgeResourcesForEntries,
  sourceScopeFor,
  sortKnowledgeRecords,
  valueForColumn,
};
