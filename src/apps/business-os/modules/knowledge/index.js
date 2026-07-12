import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';

const KNOWLEDGE_RENDER_DEBOUNCE_MS = 80;
const KNOWLEDGE_SYNC_START_WAIT_MS = 1500;
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
    noRunbooks: 'Keine Runbooks vorhanden.',
    tableUnavailable: 'Für diesen Eintrag ist keine Tabelle verfügbar.',
    queued: 'Command angelegt',
    queueFailed: 'Command konnte nicht angelegt werden',
    edit: 'Bearbeiten',
    closeEditor: 'Editor schließen',
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
    noRunbooks: 'No runbooks available.',
    tableUnavailable: 'This item has no table.',
    queued: 'Command queued',
    queueFailed: 'Could not queue command',
    edit: 'Edit',
    closeEditor: 'Close editor',
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
  activeTab: 'skill',
  tableOffset: 0,
  tableLimit: 120,
  editing: false,
  sourceScope: 'all',
  messages: null,
  openGroups: new Set(['research/drone-design/drone-bearing-loads']),
  contextMenu: null,
  resizeCleanup: null,
  localSubscriptionCleanup: null,
  syncWarmupPromise: null,
  refreshInFlight: false,
  missingCollections: [],
  loadError: '',
};

const els = {};

export async function mount(ctx) {
  await ensureStyles();
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  state.messages = await loadModuleMessages(import.meta.url, state.lang, labels);
  ctx.host.innerHTML = documentTemplate();
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();
  bindElements(ctx.host);
  wireEvents();
  state.resizeCleanup = setupKnowledgeColumnResizing();
  await loadKnowledgeFromLocal({ initial: true });
  state.localSubscriptionCleanup = wireLocalRealtime();
  window.addEventListener('message', handleShellMessage);
  return () => {
    window.removeEventListener('message', handleShellMessage);
    window.removeEventListener('click', handleContextOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleContextEscape);
    state.resizeCleanup?.();
    state.resizeCleanup = null;
    state.localSubscriptionCleanup?.();
    state.localSubscriptionCleanup = null;
    state.contextMenu?.remove();
    state.contextMenu = null;
  };
}

async function ensureStyles() {
  const href = new URL('./index.css', import.meta.url).pathname;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

// Monochrome stroke icons for header/close buttons. Delegates to the shell's
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

function documentTemplate() {
  const copy = state.messages || labels[state.lang];
  return `
    <main class="knowledge-module" data-knowledge-root data-resize-frame>
      <section class="ctox-pane knowledge-pane knowledge-left" aria-label="Knowledge">
        <header class="ctox-pane-header ctox-pane-band">
          <div class="ctox-pane-title-row">
            <div class="ctox-pane-titles">
              <span class="ctox-pane-kicker">Research</span>
              <h2 class="ctox-pane-title">Knowledge</h2>
            </div>
            <div class="ctox-pane-actions">
              <button class="ctox-pane-icon" type="button" data-action="create-knowledge-book" aria-label="Knowledge Book erstellen" title="Knowledge Book erstellen">${actionIcon('add')}</button>
              <button class="ctox-pane-icon" type="button" data-action="configure-knowledge" aria-label="Knowledge konfigurieren" title="Knowledge konfigurieren">${actionIcon('settings')}</button>
              <button class="ctox-pane-icon" type="button" data-action="import-knowledge-book" aria-label="Knowledge Book importieren" title="Knowledge Book importieren">${actionIcon('download')}</button>
              <button class="ctox-pane-icon" type="button" data-action="export-knowledge-book" aria-label="Knowledge Books exportieren" title="Knowledge Books exportieren">${actionIcon('export')}</button>
            </div>
          </div>
          <div class="ctox-pane-tools">
            <input class="ctox-pane-search" data-search placeholder="Suchen..." />
          </div>
        </header>
        <div class="knowledge-scope-switch">
          <div class="ctox-pane-tabs" role="tablist" aria-label="Knowledge Quelle">
            <button type="button" class="ctox-pane-tab" role="tab" data-scope="user" aria-selected="false">User</button>
            <button type="button" class="ctox-pane-tab" role="tab" data-scope="system" aria-selected="false">System</button>
            <button type="button" class="ctox-pane-tab" role="tab" data-scope="all" aria-selected="true">Alle</button>
          </div>
        </div>
        <div class="knowledge-scroll" data-knowledge-list>
          <div class="ctox-empty"><strong>${copy.loading}</strong></div>
        </div>
      </section>
      <button class="ctox-column-resizer" type="button" data-resizer="left" data-resizer-var="--knowledge-left-width" data-resizer-min="300" data-resizer-max="720" aria-label="Spaltenbreite anpassen"></button>
      <section class="ctox-pane knowledge-pane knowledge-center" aria-label="Knowledge Dokument">
        <header class="ctox-pane-header ctox-pane-band knowledge-center-head">
          <div class="ctox-pane-title-row">
            <div class="ctox-pane-titles">
              <span class="ctox-pane-kicker" data-selected-kind>${escapeHtml(copy.selected)}</span>
              <h2 class="ctox-pane-title" data-selected-title>${escapeHtml(copy.noSelection)}</h2>
            </div>
            <div class="ctox-pane-actions">
              <div class="ctox-pane-tabs" role="tablist" aria-label="Knowledge Ansicht">
                <button type="button" class="ctox-pane-tab" role="tab" data-tab="skill" aria-selected="true">Skill</button>
                <button type="button" class="ctox-pane-tab" role="tab" data-tab="runbooks" aria-selected="false">Runbooks</button>
                <button type="button" class="ctox-pane-tab" role="tab" data-tab="data" aria-selected="false">Data</button>
              </div>
            </div>
          </div>
        </header>
        <div class="knowledge-tab-panel" data-panel="skill">
          <div class="ctox-toolbar knowledge-edit-bar" data-skill-toolbar>
            <div class="knowledge-edit-actions">
              <button class="ctox-button" type="button" data-action="edit-markdown">Bearbeiten</button>
              <button class="ctox-button is-primary" type="button" data-action="save-markdown" hidden>An CTOX geben</button>
              <button class="ctox-button" type="button" data-action="cancel-markdown" hidden>Abbrechen</button>
            </div>
            <span class="knowledge-edit-status" data-skill-status></span>
          </div>
          <article class="markdown-document" data-markdown-view></article>
          <textarea class="markdown-editor" data-markdown-editor hidden></textarea>
        </div>
        <div class="knowledge-tab-panel" data-panel="runbooks" hidden>
          <div class="knowledge-secondary-switcher" data-runbook-switcher></div>
          <div class="ctox-toolbar knowledge-edit-bar" data-runbook-toolbar>
            <div class="knowledge-edit-actions">
              <button class="ctox-button" type="button" data-action="edit-runbook">Bearbeiten</button>
              <button class="ctox-button is-primary" type="button" data-action="save-runbook" hidden>An CTOX geben</button>
              <button class="ctox-button" type="button" data-action="cancel-runbook" hidden>Abbrechen</button>
              <button class="ctox-button" type="button" data-action="execute-runbook">Ausführen</button>
            </div>
            <span class="knowledge-edit-status" data-runbook-status></span>
          </div>
          <article class="runbook-document" data-runbook-view></article>
          <form class="runbook-editor" data-runbook-form hidden>
            <input class="ctox-input" data-runbook-title placeholder="Runbook-Titel" />
            <textarea class="ctox-textarea" data-runbook-prompt placeholder="Runbook-Anweisung"></textarea>
          </form>
        </div>
        <div class="knowledge-tab-panel" data-panel="data" hidden>
          <div class="table-switcher" data-table-switcher></div>
          <div class="dataframe-bar">
            <div><strong data-table-title>DataFrame</strong><span data-table-meta></span></div>
            <div class="table-pager">
              <button class="ctox-button" type="button" data-action="export-table-csv">CSV</button>
              <button class="ctox-button" type="button" data-action="prev-rows">Zurück</button>
              <button class="ctox-button" type="button" data-action="next-rows">Weiter</button>
            </div>
          </div>
          <div class="dataframe-host" data-dataframe-host></div>
        </div>
      </section>
    </main>
  `;
}

function bindElements(root) {
  els.root = root.querySelector('[data-knowledge-root]');
  els.list = root.querySelector('[data-knowledge-list]');
  els.search = root.querySelector('[data-search]');
  els.kindFilter = root.querySelector('[data-kind-filter]');
  els.selectedKind = root.querySelector('[data-selected-kind]');
  els.selectedTitle = root.querySelector('[data-selected-title]');
  els.markdownView = root.querySelector('[data-markdown-view]');
  els.markdownEditor = root.querySelector('[data-markdown-editor]');
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
  els.search.addEventListener('input', renderKnowledgeList);
  els.kindFilter?.addEventListener('change', renderKnowledgeList);
  for (const button of state.ctx.host.querySelectorAll('[data-scope]')) {
    button.addEventListener('click', () => setSourceScope(button.dataset.scope || 'user'));
  }
  for (const button of state.ctx.host.querySelectorAll('[data-tab]')) {
    button.addEventListener('click', () => setTab(button.dataset.tab || 'book'));
  }
  state.ctx.host.querySelector('[data-action="prev-rows"]').addEventListener('click', () => pageTable(-1));
  state.ctx.host.querySelector('[data-action="next-rows"]').addEventListener('click', () => pageTable(1));
  state.ctx.host.querySelector('[data-action="export-table-csv"]')?.addEventListener('click', exportActiveTableCsv);
  state.ctx.host.querySelector('[data-action="create-knowledge-book"]')?.addEventListener('click', () => openCreateKnowledgeBookDrawer());
  state.ctx.host.querySelector('[data-action="import-knowledge-book"]')?.addEventListener('click', () => openImportKnowledgeBookDrawer());
  state.ctx.host.querySelector('[data-action="export-knowledge-book"]')?.addEventListener('click', () => openExportKnowledgeBookDrawer());
  state.ctx.host.querySelector('[data-action="configure-knowledge"]').addEventListener('click', () => openKnowledgeConfig());
  state.ctx.host.querySelector('[data-action="edit-markdown"]')?.addEventListener('click', toggleMarkdownEditor);
  state.ctx.host.querySelector('[data-action="save-markdown"]')?.addEventListener('click', queueMarkdownSave);
  state.ctx.host.querySelector('[data-action="cancel-markdown"]')?.addEventListener('click', cancelMarkdownEdit);
  state.ctx.host.querySelector('[data-action="edit-runbook"]')?.addEventListener('click', startRunbookEdit);
  state.ctx.host.querySelector('[data-action="save-runbook"]')?.addEventListener('click', queueRunbookSave);
  state.ctx.host.querySelector('[data-action="cancel-runbook"]')?.addEventListener('click', cancelRunbookEdit);
  state.ctx.host.querySelector('[data-action="execute-runbook"]')?.addEventListener('click', executeRunbook);
  els.runbookForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    queueRunbookSave();
  });
  initKnowledgeContextMenu();
}

async function loadKnowledgeFromLocal(options = {}) {
  if (state.refreshInFlight) return;
  state.refreshInFlight = true;
  state.loadError = '';
  state.missingCollections = [];
  try {
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
  } finally {
    state.refreshInFlight = false;
  }
}

function applyKnowledgeRecords({ items = [], runbooks = [], tables = [] }) {
  const normalizedItems = Array.isArray(items) ? items.map(normalizeStoredKnowledgeRecord).filter(isActiveKnowledgeRecord) : [];
  const normalizedRunbooks = Array.isArray(runbooks) ? runbooks.map(normalizeStoredKnowledgeRecord).filter(isActiveKnowledgeRecord) : [];
  const normalizedTables = Array.isArray(tables) ? tables.map(normalizeStoredKnowledgeRecord).filter(isActiveKnowledgeRecord) : [];
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
  let items = [];
  let runbooks = [];
  let tables = [];
  let error = '';
  try {
    [items, runbooks, tables] = await Promise.all(KNOWLEDGE_DATA_COLLECTIONS.map((collectionName) => (
      loadLocalKnowledgeRecords(collectionName, missingCollections)
    )));
  } catch (err) {
    error = err?.message || String(err);
  }
  return { items, runbooks, tables, missingCollections, error };
}

async function loadLocalKnowledgeRecords(collectionName, missingCollections = state.missingCollections) {
  const collection = knowledgeCollection(collectionName);
  if (!collection) {
    missingCollections.push(collectionName);
    return [];
  }
  const docs = await collection.find({ sort: [{ updated_at_ms: 'desc' }] }).exec();
  return docs
    .map((doc) => normalizeStoredKnowledgeRecord(doc.toJSON()))
    .filter(isActiveKnowledgeRecord);
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
  if (!state.ctx?.sync?.startCollection) return;
  if (!state.syncWarmupPromise) {
    state.syncWarmupPromise = Promise.allSettled(
      KNOWLEDGE_DATA_COLLECTIONS.map((collectionName) => state.ctx.sync.startCollection(collectionName))
    ).then((results) => {
      if (results.some((result) => result.status === 'rejected')) state.syncWarmupPromise = null;
      return results;
    });
  }
  await promiseWithTimeout(state.syncWarmupPromise, KNOWLEDGE_SYNC_START_WAIT_MS).catch(() => {});
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

function renderEmptyKnowledgeSelection() {
  const copy = state.messages || labels[state.lang];
  state.selectedId = '';
  state.selectedGroupId = '';
  state.selectedSkillbookId = '';
  state.selectedTableId = '';
  state.selectedRunbookId = '';
  state.editing = false;
  state.activeTab = 'skill';
  // Right-column header is the detail-view, not a second "Knowledge" heading.
  // When no item is selected, show a neutral kicker/title that does not duplicate
  // the left-column "Knowledge" heading.
  els.selectedKind.textContent = copy.selected || 'Selected';
  els.selectedTitle.textContent = copy.noSelection || (state.lang === 'en' ? 'No entry selected' : 'Kein Eintrag ausgewählt');
  els.markdownEditor.hidden = true;
  els.markdownView.hidden = false;
  els.markdownEditor.value = '';
  // The list-pane on the left already shows the "no entries" empty-state.
  // Don't repeat it inside the detail-view; show a brief hint instead.
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
  const used = new Set();

  const makeGroup = (config) => {
    const entries = uniqueById(config.entries || []).filter(Boolean);
    for (const entry of entries) used.add(entry.id);
    const tableIds = entries.filter((entry) => entry.has_table).map((entry) => entry.id);
    const linkedRunbookIds = entries.flatMap((entry) => extractRunbookIds(entry?.linked_runbook_ids ?? entry?.linked_runbooks_json ?? entry?.linked_runbooks));
    const runbookIds = uniqueStrings([
      ...(config.runbookIds || []),
      ...linkedRunbookIds,
      ...entries.filter((entry) => entry.kind === 'runbook').map((entry) => entry.id || entry.runbook_id),
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

  const droneEntries = uniqueById([
    ...skillItems.filter(isDroneBearingKnowledge),
    ...skillbookItems.filter(isDroneBearingKnowledge),
    ...runbookItems.filter(isDroneBearingKnowledge),
    ...tableItems.filter((item) => {
      const table = tableForItem(item, tables);
      return isDroneBearingKnowledge(item) || isDroneBearingTable(table);
    }),
  ]);
  const groups = [];
  if (droneEntries.length) {
    groups.push(makeGroup({
      id: 'research/drone-design/drone-bearing-loads',
      title: 'Drone Bearing Loads',
      domainLabel: 'Research / Drone Design',
      domain: 'drone_design',
      summary: 'Skill, Skillbook, Runbook und DataFrames für Drone-Bearing-Load-Recherche.',
      entries: droneEntries,
      runbookIds: runbooks.filter(isDroneBearingKnowledge).map((runbook) => `runbook:${runbook.id}`),
      primaryItemId: droneEntries.find((entry) => entry.kind === 'skillbook')?.id || droneEntries[0]?.id,
    }));
  }

  for (const skillbook of skillbookItems) {
    if (used.has(skillbook.id)) continue;
    const base = normaliseName(bareId(skillbook.id).replace(/-skillbook$/, ''));
    const linkedRunbooks = new Set(extractRunbookIds(skillbook.linked_runbook_ids ?? skillbook.linked_runbooks_json ?? skillbook.linked_runbooks).map(normaliseRunbookId));
    const relatedRunbooks = runbookItems.filter((item) => {
      const itemId = normaliseRunbookId(item.id || item.runbook_id);
      return linkedRunbooks.has(itemId) || item.skillbook_id === bareKnowledgeId(skillbook.id) || item.subtitle?.toLowerCase().includes(base.replaceAll('-', '_')) || tokenOverlap(skillbook, item) >= 2;
    });
    const relatedTables = tableItems.filter((item) => tokenOverlap(skillbook, item) >= 2);
    const relatedSkills = skillItems.filter((item) => tokenOverlap(skillbook, item) >= 2);
    groups.push(makeGroup({
      id: `bundle/${base}`,
      title: skillbook.title || titleFromSlug(base),
      domainLabel: domainLabelFor(skillbook),
      domain: base,
      summary: skillbook.summary || '',
      entries: [skillbook, ...relatedSkills, ...relatedRunbooks, ...relatedTables],
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

  return groups.filter((group) => group.entries.length).sort((a, b) => {
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

const KNOWLEDGE_LAYOUT_KEY = 'ctox.businessOs.knowledge.columnLayout';
const KNOWLEDGE_COL_MIN = Object.freeze({ left: 300, center: 420 });
const KNOWLEDGE_COL_LEFT_MAX = 720;

function setupKnowledgeColumnResizing() {
  // Column resizing is now owned by the shell-global resizer (app.js
  // `setupModuleResizers`): the `.ctox-column-resizer[data-resizer-var=
  // "--knowledge-left-width"]` handle in index.html, inside the
  // `[data-resize-frame]` root, gets drag/keyboard/persistence for free.
  // This DIY (createElement + CtoxResizer + ResizeObserver + localStorage)
  // is retired; we no-op so call sites keep working without dangling refs.
  return () => {};

  // eslint-disable-next-line no-unreachable
  const root = els.root;
  if (!root) return null;

  const handle = document.createElement('div');
  handle.className = 'knowledge-col-resizer';
  handle.dataset.resizer = 'left';
  handle.setAttribute('role', 'separator');
  handle.setAttribute('aria-orientation', 'vertical');
  handle.setAttribute('aria-label', 'Spaltenbreite anpassen');
  handle.dataset.resizer = 'left';
  root.append(handle);

  const setLeftWidth = (width) => {
    const next = clampKnowledgeLeftWidth(root, width);
    root.style.setProperty('--knowledge-left-width', `${next}px`);
    localStorage.setItem('ctox.knowledge.layout.leftWidth', String(next));
    return next;
  };
  const resizer = new CtoxResizer({
    resizerEl: handle,
    containerEl: root,
    cssVar: '--knowledge-left-width',
    side: 'left',
    minWidth: KNOWLEDGE_COL_MIN.left,
    maxWidth: knowledgeLeftMaxForRoot(root),
    onResize: setLeftWidth,
  });

  const leftWidth = localStorage.getItem('ctox.knowledge.layout.leftWidth') || '390';
  setLeftWidth(Number.parseFloat(leftWidth));
  const resizeObserver = new ResizeObserver(() => {
    resizer.maxWidth = knowledgeLeftMaxForRoot(root);
    handle.setAttribute('aria-valuemax', String(resizer.maxWidth));
    resizer.setWidth(clampKnowledgeLeftWidth(root, resizer.readCurrentWidth()));
  });
  resizeObserver.observe(root);

  return () => {
    resizeObserver.disconnect();
    resizer.destroy();
    handle.remove();
  };
}

function knowledgeLeftMaxForRoot(root) {
  const metrics = getKnowledgeGridMetrics(root);
  const available = (metrics?.contentWidth || root?.clientWidth || 0) - 12 - KNOWLEDGE_COL_MIN.center;
  return Math.max(KNOWLEDGE_COL_MIN.left, Math.min(KNOWLEDGE_COL_LEFT_MAX, available));
}

function clampKnowledgeLeftWidth(root, width) {
  return clampNumber(Number(width) || KNOWLEDGE_COL_MIN.left, KNOWLEDGE_COL_MIN.left, knowledgeLeftMaxForRoot(root));
}

function getKnowledgeGridMetrics(root) {
  if (!root) return null;
  const cs = getComputedStyle(root);
  const gap = Number.parseFloat(cs.columnGap || cs.gap || '0') || 0;
  const padLeft = Number.parseFloat(cs.paddingLeft || '0') || 0;
  const padRight = Number.parseFloat(cs.paddingRight || '0') || 0;
  const contentWidth = Math.max(0, root.clientWidth - padLeft - padRight);
  const trackTotal = Math.max(0, contentWidth - gap);
  return { gap, padLeft, contentWidth, trackTotal };
}

function readKnowledgeGridTrackPixels(root) {
  if (!root) return null;
  const tracks = String(getComputedStyle(root).gridTemplateColumns || '')
    .split(/\s+/)
    .map((part) => Number.parseFloat(part))
    .filter((number) => Number.isFinite(number) && number > 0);
  if (tracks.length < 2) return null;
  return { left: tracks[0], center: tracks[1] };
}

function clampKnowledgeColumns(widths, trackTotal) {
  if (!widths || !Number.isFinite(trackTotal) || trackTotal <= 0) return null;
  if (trackTotal < KNOWLEDGE_COL_MIN.left + KNOWLEDGE_COL_MIN.center) return null;
  const maxLeft = Math.max(KNOWLEDGE_COL_MIN.left, Math.min(KNOWLEDGE_COL_LEFT_MAX, trackTotal - KNOWLEDGE_COL_MIN.center));
  const left = Math.round(clampNumber(Number(widths.left) || KNOWLEDGE_COL_MIN.left, KNOWLEDGE_COL_MIN.left, maxLeft));
  const center = Math.round(trackTotal - left);
  if (center < KNOWLEDGE_COL_MIN.center) return null;
  return { left, center };
}

function columnPixelsToRatios(widths) {
  if (!widths) return null;
  const left = Number(widths.left) || 0;
  const center = Number(widths.center) || 0;
  const sum = left + center;
  if (sum <= 0) return null;
  return {
    left: Number((left / sum).toFixed(6)),
    center: Number((center / sum).toFixed(6)),
  };
}

function sanitizeKnowledgeColumnLayout(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const left = Number(raw.left);
  const center = Number(raw.center);
  if (![left, center].every(Number.isFinite)) return null;
  if (left <= 0 || center <= 0) return null;
  const sum = left + center;
  if (sum <= 0) return null;
  return { left: left / sum, center: center / sum };
}

function columnRatiosToPixels(ratios, trackTotal) {
  const safe = sanitizeKnowledgeColumnLayout(ratios);
  if (!safe) return null;
  return clampKnowledgeColumns({
    left: safe.left * trackTotal,
    center: safe.center * trackTotal,
  }, trackTotal);
}

function readKnowledgeColumnLayout() {
  try {
    return sanitizeKnowledgeColumnLayout(JSON.parse(window.localStorage.getItem(KNOWLEDGE_LAYOUT_KEY) || 'null'));
  } catch (_) {
    return null;
  }
}

function writeKnowledgeColumnLayout(ratios) {
  try {
    window.localStorage.setItem(KNOWLEDGE_LAYOUT_KEY, JSON.stringify(ratios));
  } catch (_) {
    // Ignore unavailable storage.
  }
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
  if (!group) return { skillbook: null, entries: [], skill: null, runbookItems: [], runbooks: [], tables: [] };
  const skillbookEntry = typeof skillbook === 'string' ? group.entries.find((entry) => entry.id === skillbook) : skillbook;
  const allSkillbooks = skillbooksForGroup(group);
  const scopedEntries = !skillbookEntry || allSkillbooks.length <= 1
    ? group.entries
    : group.entries.filter((entry) => entry.id === skillbookEntry.id || relatedToSkillbook(skillbookEntry, entry));
  const entries = scopedEntries.length ? scopedEntries : group.entries;
  const skill = entries.find((entry) => entry.kind === 'skill')
    || group.entries.find((entry) => entry.kind === 'skill' && (!skillbookEntry || tokenOverlap(skillbookEntry, entry) >= 1))
    || skillbookEntry
    || group.entries.find((entry) => ['skillbook', 'skill'].includes(entry.kind))
    || group.entries[0]
    || null;
  const runbookItems = entries.filter((entry) => entry.kind === 'runbook');
  const linkedRunbookIds = new Set([
    ...extractRunbookIds(skillbookEntry?.linked_runbook_ids ?? skillbookEntry?.linked_runbooks_json ?? skillbookEntry?.linked_runbooks).map(normaliseRunbookId),
    ...runbookItems.map((entry) => entry.id || entry.runbook_id).map(normaliseRunbookId),
  ]);
  const groupRunbookIds = new Set((group.runbookIds || []).map(normaliseRunbookId).filter(Boolean));
  const runbooks = state.runbooks.filter((runbook) => {
    const id = normaliseRunbookId(runbook.id || runbook.runbook_id);
    if (linkedRunbookIds.size) return linkedRunbookIds.has(id);
    if (!groupRunbookIds.has(id)) return false;
    return !skillbookEntry || allSkillbooks.length <= 1 || relatedToSkillbook(skillbookEntry, runbook);
  });
  const tables = entries.filter((entry) => entry.has_table);
  return { skillbook: skillbookEntry || null, entries, skill, runbookItems, runbooks, tables };
}

function relatedToSkillbook(skillbook, entry) {
  if (!skillbook || !entry) return true;
  if (entry.id === skillbook.id) return true;
  const base = normaliseName(bareId(skillbook.id).replace(/-skillbook$/, ''));
  const haystack = `${entry.id || ''} ${entry.title || ''} ${entry.subtitle || ''} ${entry.summary || ''} ${entry.description || ''} ${entry.problem_domain || ''}`.toLowerCase();
  return haystack.includes(base.replaceAll('-', '_')) || haystack.includes(base) || tokenOverlap(skillbook, entry) >= 2;
}

async function selectSkillbook(group, skillbook) {
  if (!group) return;
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

function renderKnowledgeList() {
  const copy = state.messages || labels[state.lang];
  const term = els.search.value.trim().toLowerCase();
  const visibleGroups = state.groups
    .map((group) => ({
      ...group,
      entries: group.entries.filter((entry) => {
        return state.sourceScope === 'all' || sourceScopeFor(entry) === state.sourceScope;
      }),
    }))
    .filter((group) => {
      if (!group.entries.length) return false;
      if (!term) return true;
      return `${group.title} ${group.summary || ''} ${group.domain || ''} ${group.entries.map((entry) => `${entry.title} ${entry.subtitle || ''} ${entry.summary || ''}`).join(' ')}`.toLowerCase().includes(term);
    });
  if (!visibleGroups.length) {
    els.list.innerHTML = `<div class="ctox-empty"><strong>${escapeHtml(knowledgeEmptyStateMessage(copy, term))}</strong></div>`;
    return;
  }
  els.list.replaceChildren(...visibleGroups.map((group) => renderKnowledgeBundle(group)));
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

function renderKnowledgeBundle(group) {
  const section = document.createElement('section');
  section.className = 'knowledge-bundle';
  section.dataset.bundleId = group.id;
  section.dataset.contextModule = 'knowledge';
  section.dataset.contextRecordType = 'knowledge-group';
  section.dataset.contextRecordId = group.id;
  section.dataset.contextLabel = group.title;
  section.dataset.knowledgeColumn = 'sources';
  section.dataset.open = String(state.openGroups.has(group.id));
  section.setAttribute('aria-current', String(group.id === state.selectedGroupId));
  const tableCount = group.tableIds.length;
  const runbookCount = group.runbookIds.length;
  const skillbookCount = skillbooksForGroup(group).length;
  section.innerHTML = `
    <button class="knowledge-bundle-head" type="button">
      <span class="bundle-caret" aria-hidden="true"></span>
      <span class="bundle-domain">${escapeHtml(group.domainLabel || 'Knowledge')}</span>
      <strong>${escapeHtml(group.title)}</strong>
      <small>${escapeHtml(`${skillbookCount} Skillbooks · ${runbookCount} Runbooks · ${tableCount} Tabellen`)}</small>
    </button>
    <div class="knowledge-bundle-items"></div>
  `;
  section.querySelector('.knowledge-bundle-head').addEventListener('click', () => {
    const wasOpen = state.openGroups.has(group.id);
    const wasSelected = state.selectedGroupId === group.id;
    state.selectedGroupId = group.id;
    const skillbook = selectedSkillbookForGroup(group);
    state.selectedSkillbookId = skillbook?.id || '';
    const context = skillbookContext(group, skillbook);
    state.selectedId = context.skill?.id || skillbook?.id || group.primaryItemId || group.entries[0]?.id || '';
    state.selectedTableId = context.tables[0]?.id || group.tableIds[0] || '';
    state.selectedRunbookId = normaliseRunbookId(context.runbooks[0]?.id || context.runbooks[0]?.runbook_id || group.runbookIds[0] || state.selectedRunbookId);
    if (wasSelected && wasOpen) {
      state.openGroups.delete(group.id);
      renderKnowledgeList();
      renderActiveTab();
      return;
    }
    state.openGroups.add(group.id);
    selectSkillbook(group, skillbook);
  });
  const list = section.querySelector('.knowledge-bundle-items');
  list.append(renderSkillbookList(group));
  return section;
}

function renderSkillbookList(group) {
  const block = document.createElement('div');
  block.className = 'knowledge-kind-group';
  block.innerHTML = '<div class="knowledge-kind-title">Skillbooks</div>';
  const skillbooks = skillbooksForGroup(group);
  if (!skillbooks.length) {
    const fallback = group.entries.find((entry) => entry.id === group.primaryItemId) || group.entries[0];
    if (fallback) block.append(renderSkillbookItem(fallback, group));
    return block;
  }
  for (const skillbook of skillbooks) block.append(renderSkillbookItem(skillbook, group));
  return block;
}

function renderSkillbookItem(item, group) {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'knowledge-item knowledge-skillbook-item';
  button.dataset.knowledgeId = item.id;
  button.dataset.contextModule = 'knowledge';
  button.dataset.contextRecordType = item.kind;
  button.dataset.contextRecordId = item.id;
  button.dataset.contextLabel = item.title || item.id;
  button.dataset.knowledgeColumn = 'sources';
  button.setAttribute('aria-current', String(group.id === state.selectedGroupId && item.id === selectedSkillbookForGroup(group)?.id));
  const context = skillbookContext(group, item);
  button.innerHTML = `
    <strong>${escapeHtml(item.title || item.id)}</strong>
    <small>${escapeHtml(`${context.runbooks.length} Runbooks · ${context.tables.length} Tabellen`)}</small>
  `;
  button.addEventListener('click', () => {
    state.openGroups.add(group.id);
    selectSkillbook(group, item);
  });
  return button;
}

function groupEntriesByKind(entries) {
  const order = ['skill', 'skillbook', 'runbook', 'dataframe'];
  const grouped = groupBy(entries, (entry) => entry.kind || 'knowledge');
  return order.reduce((acc, key) => {
    if (grouped[key]?.length) acc[key] = grouped[key];
    return acc;
  }, {});
}

function itemMeta(item) {
  if (item.has_table) {
    const table = tableForItem(item, state.tables);
    if (Number.isFinite(Number(table?.row_count))) return `${Number(table.row_count).toLocaleString('de-DE')} Zeilen`;
    return 'Tabelle';
  }
  if (item.file_count) return `${item.file_count} Dateien`;
  return item.subtitle || '';
}

function renderKnowledgeListLegacy() {
  const term = els.search.value.trim().toLowerCase();
  const kind = els.kindFilter?.value || 'all';
  const visible = state.items.filter((item) => {
    if (kind !== 'all' && item.kind !== kind) return false;
    if (!term) return true;
    return `${item.title} ${item.subtitle || ''} ${item.summary || ''}`.toLowerCase().includes(term);
  });
  if (!visible.length) {
    els.list.innerHTML = `<div class="ctox-empty"><strong>${labels[state.lang].noItems}</strong></div>`;
    return;
  }
  const groups = groupBy(visible, (item) => item.kind || 'knowledge');
  els.list.replaceChildren(...Object.entries(groups).map(([group, items]) => renderKnowledgeGroup(group, items)));
}

function renderKnowledgeGroup(group, items) {
  const section = document.createElement('section');
  section.className = 'knowledge-group';
  section.innerHTML = `<div class="knowledge-group-title">${escapeHtml(groupLabel(group))}</div>`;
  for (const item of items) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'knowledge-item';
    button.dataset.knowledgeId = item.id;
    button.dataset.contextModule = 'knowledge';
    button.dataset.contextRecordType = item.kind;
    button.dataset.contextRecordId = item.id;
    button.dataset.contextLabel = item.title || item.id;
    button.setAttribute('aria-current', String(item.id === state.selectedId));
    button.innerHTML = `
      <strong>${escapeHtml(item.title || item.id)}</strong>
      <span>${escapeHtml(item.subtitle || item.summary || '')}</span>
      <small>${escapeHtml(item.has_table ? 'DataFrame' : item.file_count ? `${item.file_count} Dateien` : '')}</small>
    `;
    button.addEventListener('click', () => selectKnowledge(item.id));
    section.append(button);
  }
  return section;
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
  els.selectedKind.textContent = groupLabel(item?.kind || 'knowledge');
  els.selectedTitle.textContent = item?.title || 'Knowledge';
  renderKnowledgeList();
  const doc = await loadKnowledgeDocument(id);
  els.markdownEditor.hidden = true;
  els.markdownView.hidden = false;
  els.markdownEditor.value = doc.markdown || '';
  els.markdownView.innerHTML = markdownToHtml(doc.markdown || '');
  syncMarkdownEditControls();
  syncKnowledgeTabControls();
  await renderActiveTab();
}

function setSourceScope(scope) {
  state.sourceScope = ['system', 'user', 'all'].includes(scope) ? scope : 'user';
  for (const button of state.ctx.host.querySelectorAll('[data-scope]')) {
    button.setAttribute('aria-selected', String(button.dataset.scope === state.sourceScope));
  }
  const firstVisibleGroup = state.groups.find((group) => group.entries.some((entry) => state.sourceScope === 'all' || sourceScopeFor(entry) === state.sourceScope));
  if (firstVisibleGroup) {
    state.selectedGroupId = firstVisibleGroup.id;
    state.openGroups.add(firstVisibleGroup.id);
    const firstSkillbook = skillbooksForGroup(firstVisibleGroup).find((entry) => state.sourceScope === 'all' || sourceScopeFor(entry) === state.sourceScope) || firstSkillbookForGroup(firstVisibleGroup);
    selectSkillbook(firstVisibleGroup, firstSkillbook);
    return;
  }
  renderKnowledgeList();
  renderEmptyKnowledgeSelection();
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
    els.runbookList.innerHTML = `<div class="ctox-empty"><strong>${copy.noRunbooks}</strong></div>`;
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
    button.setAttribute('aria-current', String(runbookIdMatches(runbook.id || runbook.runbook_id, state.selectedRunbookId)));
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
  state.activeTab = ['skill', 'runbooks', 'data'].includes(nextTab) ? nextTab : 'skill';
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

async function renderActiveTab() {
  if (!hasKnowledgeSelection() && state.activeTab !== 'skill') {
    state.activeTab = 'skill';
    renderEmptyKnowledgeSelection();
    return;
  }
  if (state.activeTab === 'skill') {
    const context = skillbookContext();
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
  if (state.activeTab === 'data') {
    renderTableSwitcher();
    await renderTable();
  }
}

function syncKnowledgeTabControls() {
  if (isKnowledgeTabDisabled(state.activeTab, state.selectedId)) state.activeTab = 'skill';
  for (const button of state.ctx.host.querySelectorAll('[data-tab]')) {
    const disabled = isKnowledgeTabDisabled(button.dataset.tab, state.selectedId);
    button.disabled = disabled;
    button.setAttribute('aria-disabled', String(disabled));
    button.setAttribute('aria-selected', String(button.dataset.tab === state.activeTab));
  }
  for (const panel of state.ctx.host.querySelectorAll('[data-panel]')) {
    panel.hidden = panel.dataset.panel !== state.activeTab;
  }
}

function isKnowledgeTabDisabled(tab, selectedId = state.selectedId) {
  return ['runbooks', 'data'].includes(tab) && !selectedId;
}

function hasKnowledgeSelection() {
  return Boolean(state.selectedId && state.items.some((entry) => entry.id === state.selectedId));
}

function renderSelectionHeader() {
  const group = activeGroup();
  const context = skillbookContext(group, state.selectedSkillbookId);
  const item = state.items.find((entry) => entry.id === state.selectedId) || context.skill;
  els.selectedKind.textContent = 'Skill';
  els.selectedTitle.textContent = context.skillbook?.title || item?.title || group?.title || 'Knowledge';
  syncMarkdownEditControls();
}

async function renderRunbookWorkspace() {
  const copy = state.messages || labels[state.lang];
  const context = skillbookContext();
  const visibleRunbooks = context.runbooks;
  els.selectedKind.textContent = 'Runbooks';
  els.selectedTitle.textContent = context.skillbook?.title || activeGroup()?.title || 'Knowledge';
  if (!visibleRunbooks.length) {
    els.runbookSwitcher.hidden = true;
    els.runbookView.innerHTML = `<div class="ctox-empty"><strong>${copy.noRunbooks}</strong></div>`;
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
    button.setAttribute('aria-current', String(runbookIdMatches(runbook.id || runbook.runbook_id, state.selectedRunbookId)));
    button.textContent = runbook.title || runbook.id || runbook.runbook_id || 'Runbook';
    button.addEventListener('click', () => {
      state.selectedRunbookId = normaliseRunbookId(runbook.id || runbook.runbook_id);
      state.editing = false;
      renderRunbookWorkspace();
    });
    return button;
  }));
  const runbook = visibleRunbooks.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, state.selectedRunbookId)) || visibleRunbooks[0];
  const runbookItem = context.runbookItems.find((entry) => runbookIdMatches(entry.id || entry.runbook_id, runbook.id || runbook.runbook_id));
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

function runbookDetailsHtml(runbook) {
  const instruction = runbook?.prompt || runbook?.instruction || runbook?.description || '';
  return `
    <header class="runbook-document-head">
      <span>${escapeHtml(runbook?.status || 'Runbook')}</span>
      <h1>${escapeHtml(runbook?.title || runbook?.id || runbook?.runbook_id || 'Runbook')}</h1>
    </header>
    <dl class="ctox-fields runbook-meta">
      <dt>Domain</dt><dd>${escapeHtml(runbook?.problem_domain || '-')}</dd>
      <dt>ID</dt><dd>${escapeHtml(runbook?.id || runbook?.runbook_id || '-')}</dd>
    </dl>
    ${instruction ? `<pre><code>${escapeHtml(instruction)}</code></pre>` : '<div class="ctox-empty"><strong>Keine Runbook-Anweisung vorhanden.</strong></div>'}
  `;
}

function renderTableSwitcher() {
  const context = skillbookContext();
  const tables = context.tables;
  els.tableSwitcher.hidden = tables.length <= 1;
  els.tableSwitcher.replaceChildren(...tables.map((table) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = `ctox-chip${table.id === activeTableId() ? ' is-active' : ''}`;
    button.dataset.contextModule = 'knowledge';
    button.dataset.contextRecordType = 'dataframe';
    button.dataset.contextRecordId = table.id;
    button.dataset.contextLabel = table.title || table.id;
    button.dataset.knowledgeColumn = 'workspace';
    button.setAttribute('aria-current', String(table.id === activeTableId()));
    button.textContent = table.title || table.id;
    button.addEventListener('click', () => {
      state.selectedTableId = table.id;
      state.selectedId = table.id;
      state.tableOffset = 0;
      renderKnowledgeList();
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
  els.selectedKind.textContent = 'Data';
  els.selectedTitle.textContent = tableSource?.title || skillbookContext().skillbook?.title || 'DataFrame';
  if (!tableSource?.has_table) {
    els.tableTitle.textContent = 'DataFrame';
    els.tableMeta.textContent = copy.tableUnavailable;
    els.tableHost.innerHTML = `<div class="ctox-empty"><strong>${copy.tableUnavailable}</strong></div>`;
    return;
  }
  try {
    const localRows = localDataFrameRows(tableSource);
    const schema = localDataFrameSchema(tableSource);
    const rows = localRows.length
      ? {
          returned: localRows.slice(state.tableOffset, state.tableOffset + state.tableLimit).length,
          rows: localRows.slice(state.tableOffset, state.tableOffset + state.tableLimit),
        }
      : { returned: 0, rows: [] };
    els.tableTitle.textContent = schema.title || tableSource.title || 'DataFrame';
    const totalRows = Number.isFinite(Number(schema.row_count)) ? Number(schema.row_count) : localRows.length;
    const total = `${totalRows.toLocaleString('de-DE')} Zeilen`;
    els.tableMeta.textContent = `${schema.columns?.length || 0} Spalten · ${total}`;
    renderDataFrameTable(schema.columns || [], rows.rows || []);
  } catch (error) {
    els.tableHost.innerHTML = `<div class="ctox-empty knowledge-error"><strong>DataFrame konnte nicht geladen werden</strong><span>${escapeHtml(error.message || String(error))}</span></div>`;
  }
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
    type: 'ctox.knowledge.document.modify',
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
    type: 'ctox.knowledge.runbook.modify',
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
  const result = await dispatchKnowledgeCommand({
    type: 'ctox.knowledge.runbook.execute',
    record_id: state.selectedRunbookId,
    payload: {
      title: `Runbook ausführen · ${runbook?.title || state.selectedRunbookId}`,
      instruction: els.runbookPrompt?.value || runbook?.prompt || runbook?.instruction || runbook?.description || '',
      selected_item: item,
      runbook,
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
  throw new Error('RxDB command bus is not available');
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
    ['Command Store', 'RxDB business_commands'],
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

  els.root.addEventListener('contextmenu', (event) => {
    const context = commandContextFromElement(event.target);
    event.preventDefault();
    event.stopPropagation();
    renderKnowledgeContextMenu(context, event.clientX, event.clientY);
  });
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
  const rows = localDataFrameRows(item);
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
    row_count: Number(item?.row_count ?? item?.payload?.row_count ?? rows.length),
  };
}

function localDataFrameRows(item) {
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
    item.payload?.markdown,
    item.payload?.content_markdown,
    item.payload?.document_markdown,
    item.payload?.text,
  ];
  return candidates.find((value) => typeof value === 'string' && value.trim()) || '';
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
  const rows = localDataFrameRows(tableSource);
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
  document.body.append(link);
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
  buildKnowledgeBundles,
  canEditSelectedMarkdown,
  isKnowledgeActionFormReady,
  isKnowledgeTabDisabled,
  knowledgeItemsFromTables,
  knowledgeGroupMatchesDomain,
  knowledgeEmptyStateMessage,
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
  sourceScopeFor,
  valueForColumn,
};
