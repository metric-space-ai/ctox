// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}.
// Port-Protokoll: V4-PORT.md; Ersetzungsregeln: ../../v4/tools/port/PORTING.md.
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
// Eingebettete Texte (relativer Modul-Import, kein HTTP-Ladeweg); api.shell.t übersteuert (OFFEN-B).
import messagesDe from './locales/de.json' with { type: 'json' };
import messagesEn from './locales/en.json' with { type: 'json' };

const REPORTS_REFRESH_DEBOUNCE_MS = 80;
// Alle gelesenen Collections sind fremd (Eigentümer ctox/threads/Shell), siehe V4-PORT.md.
const REPORT_COLLECTIONS = [
  'business_module_reports',
  'ctox_bug_reports',
  'business_module_releases',
  'business_commands',
  'ctox_queue_tasks',
];
// App-Katalog (System-Collection, §16.14) für die App-Titel (moduleLabel).
const REPORT_APPS_COLLECTION = 'business_apps';

const state = {
  api: null,
  root: null,
  disposers: [],
  data: {},
  apps: [],
  reports: [],
  bugs: [],
  releases: [],
  commands: [],
  queue: [],
  selectedId: '',
  search: '',
  kind: 'all',
  status: 'all',
  viewMode: 'cards',
  renderTimer: null,
  renderKey: '',
  renderedDetailId: '',
  detailScrollByReport: {},
  pendingRestore: null,
  messages: {},
  t: null,
  lang: 'de',
};

const SNAPSHOT_VIEW_KEYS = Object.freeze(['selectedId', 'search', 'kind', 'status', 'viewMode']);

function resetState() {
  state.disposers = [];
  state.data = {};
  state.apps = [];
  state.reports = [];
  state.bugs = [];
  state.releases = [];
  state.commands = [];
  state.queue = [];
  state.selectedId = '';
  state.search = '';
  state.kind = 'all';
  state.status = 'all';
  state.viewMode = 'cards';
  state.renderTimer = null;
  state.renderKey = '';
  state.renderedDetailId = '';
  state.detailScrollByReport = {};
  state.pendingRestore = null;
}

export async function mount(root, api, { restored } = {}) {
  resetState();
  // Abos gehören dieser Montage (PORTING.md §4 Falle 1).
  const disposers = state.disposers;
  state.api = api;
  state.root = root;
  if (!root.dataset.app) root.dataset.app = api.app?.id || 'reports';
  state.lang = langFrom(api.shell.locale.get());
  state.messages = messagesFor(state.lang);
  state.t = translate;
  // CSS lädt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  root.innerHTML = await loadModuleMarkup();
  applyStaticLabels(root, state.t);
  wireUi();
  autoWirePaneGrammar(root);
  api.shell.setTitle(state.t('reportsLabel', 'Bugs & Features'));
  applyRestoredView(isPlainObject(restored) ? restored : null);
  for (const name of REPORT_COLLECTIONS) {
    disposers.push(api.query(name, {}).subscribe((rows) => {
      state.data[name] = Array.isArray(rows) ? rows : [];
      scheduleRefresh();
    }));
  }
  disposers.push(api.query(REPORT_APPS_COLLECTION, { sort: [{ title: 'asc' }] }).subscribe((rows) => {
    state.apps = Array.isArray(rows) ? rows : [];
  }));
  disposers.push(api.shell.locale.subscribe((locale) => {
    const lang = langFrom(locale);
    if (lang === state.lang) return;
    state.lang = lang;
    state.messages = messagesFor(lang);
    if (state.root !== root) return;
    applyStaticLabels(root, state.t);
    api.shell.setTitle(state.t('reportsLabel', 'Bugs & Features'));
    render();
  }));
  return {
    unmount() {
      if (state.renderTimer) clearTimeout(state.renderTimer);
      state.renderTimer = null;
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[reports] unsubscribe failed', error); }
      }
      root.replaceChildren();
      if (state.root === root) {
        state.api = null;
        state.root = null;
      }
    },
    snapshot: reportsSnapshot,
  };
}

function langFrom(locale) {
  return String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function messagesFor(lang) {
  return { ...messagesDe, ...(lang === 'en' ? messagesEn : {}) };
}

function translate(key, fallback, ...args) {
  let val = state.messages?.[key] ?? fallback ?? key;
  const t = state.api?.shell?.t;
  if (typeof t === 'function') {
    const scoped = `reports.${key}`;
    const shellValue = t.call(state.api.shell, scoped);
    if (typeof shellValue === 'string' && shellValue && shellValue !== scoped) val = shellValue;
  }
  args.forEach((arg, i) => {
    val = String(val).replace(`{${i}}`, arg);
  });
  return val;
}

// Snapshot für den Live-Remount (§10.2a Nr. 1): Auswahl, Filter, Ansicht.
function reportsSnapshot() {
  const snap = { v: 1 };
  for (const key of SNAPSHOT_VIEW_KEYS) snap[key] = state[key];
  snap.actionsHidden = Boolean(state.root?.querySelector('[data-reports-root]')?.classList.contains('is-actions-hidden'));
  snap.detailScrollTop = state.root?.querySelector('[data-reports-detail-scroll]')?.scrollTop || 0;
  return snap;
}

function applyRestoredView(restored) {
  state.pendingRestore = restored;
  if (!restored) return;
  for (const key of SNAPSHOT_VIEW_KEYS) {
    if (typeof restored[key] === typeof state[key]) state[key] = restored[key];
  }
  const root = state.root?.querySelector('[data-reports-root]');
  if (root && typeof restored.actionsHidden === 'boolean') {
    root.classList.toggle('is-actions-hidden', restored.actionsHidden);
    updateToggleActionsAria(root);
  }
  const search = root?.querySelector('[data-pg-search]');
  if (search) search.value = state.search;
  const statusSelect = root?.querySelector('[data-pg-filter][data-pg-name="status"]');
  if (statusSelect) statusSelect.value = state.status;
  for (const tab of root?.querySelectorAll('[data-pg-band]') || []) {
    const active = tab.getAttribute('data-pg-band') === state.kind;
    tab.classList.toggle('is-active', active);
    tab.setAttribute('aria-selected', active ? 'true' : 'false');
  }
  syncViewToggleButton(root);
}

// Offene Eingaben erst zurückspielen, wenn der ausgewählte Datensatz in den Live-Daten steht.
function applyPendingRestore() {
  const restored = state.pendingRestore;
  if (!restored) return;
  const items = normalizedReports();
  if (restored.selectedId && !items.some((item) => item.id === restored.selectedId)) return;
  state.pendingRestore = null;
  const scroller = state.root?.querySelector('[data-reports-detail-scroll]');
  if (scroller && Number.isFinite(restored.detailScrollTop)) scroller.scrollTop = restored.detailScrollTop;
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function applyStaticLabels(host, t) {
  const root = host.querySelector('[data-reports-root]');
  if (!root) return;

  // Header export action keeps the static SVG markup in index.html — only
  // the tooltip / aria-label are translated.
  const exportBtn = root.querySelector('[data-action="export-json"]');
  if (exportBtn) {
    const label = t('exportJson', 'Gefilterte Liste als JSON exportieren');
    exportBtn.title = label;
    exportBtn.setAttribute('aria-label', label);
  }

  // Search input placeholder
  const searchInput = root.querySelector('[data-pg-search]');
  if (searchInput) {
    searchInput.placeholder = t('searchPlaceholder', 'Suchen...');
    searchInput.setAttribute('aria-label', t('searchLabel', 'Bugs und Features suchen'));
  }

  // Status select options (kind is the counted band in markup; status stays
  // the single tray filter, with values translatable so en/de stay readable).
  const statusSelect = root.querySelector('[data-pg-filter][data-pg-name="status"]');
  if (statusSelect) {
    statusSelect.setAttribute('aria-label', t('statusFilterLabel', 'Status filtern'));
    statusSelect.innerHTML = `
      <option value="all">${escapeHtml(t('allStatus', 'Alle Status'))}</option>
      <option value="open">${escapeHtml(t('open', 'Offen'))}</option>
      <option value="running">${escapeHtml(t('running', 'In Arbeit'))}</option>
      <option value="completed">${escapeHtml(t('completed', 'Erledigt'))}</option>
      <option value="blocked">${escapeHtml(t('blocked', 'Blockiert'))}</option>
    `;
  }

  // Type chip strip labels (aria-label on the strip; option text is static).
  const kindChips = root.querySelector('[data-report-kind-chips]');
  if (kindChips) {
    kindChips.setAttribute('aria-label', t('kindFilterLabel', 'Typ filtern'));
  }

  // One-button view switch — icon/label always name the target view.
  syncViewToggleButton(root);

  // Detail head primary — starts disabled (nothing selected yet) and gets its
  // localized label immediately, so an `en` mount never shows the German
  // fallback that ships in the static markup.
  syncHeadDelegateButton(null);

  // Right-pane actions toggle — title and aria label swap depending on state.
  const toggleActions = root.querySelector('[data-toggle-actions]');
  if (toggleActions) {
    toggleActions.dataset.showLabel = t('showActions', 'Aktionen einblenden');
    toggleActions.dataset.hideLabel = t('hideActions', 'Aktionen ausblenden');
    updateToggleActionsAria(root);
  }
}

// One-button view switch (Betreiber-Direktive 31.08.2026). The button is an
// action, not a state, so it carries no aria-pressed; icon, aria-label and
// title always describe the view it switches TO. `data-pg-view` stays on it
// because shared/pane-grammar.js reads a pane's view from there — it must
// always hold the CURRENT view, otherwise an unrelated grammar emit (a search
// keystroke, a tray filter, a band tab) would flip the mode as a side effect.
const VIEW_ICON_LIST = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>';
const VIEW_ICON_CARDS = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>';

function syncViewToggleButton(root) {
  const button = root?.querySelector('[data-reports-view-toggle]');
  if (!button) return;
  // The shell's generic view-button wiring stamps aria-pressed on click; a
  // single toggle is an action, so the state attribute is removed again — on
  // every sync, even when the view itself did not move.
  button.removeAttribute('aria-pressed');
  const view = state.viewMode === 'list' ? 'list' : 'cards';
  // Only touch the icon when the view actually moved. Rewriting innerHTML
  // detaches the <svg> the click originated on, which silently breaks any
  // delegated handler further up the bubble path (measured, 31.08.2026).
  if (button.getAttribute('data-pg-view') === view && button.firstElementChild) return;
  const cards = view === 'cards';
  const t = state.t || ((key, fallback) => fallback ?? key);
  const label = cards
    ? t('showAsList', 'Als Liste anzeigen')
    : t('showAsCards', 'Als Karten anzeigen');
  button.setAttribute('data-pg-view', view);
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  button.innerHTML = cards ? VIEW_ICON_LIST : VIEW_ICON_CARDS;
}

// Enablement of the detail head's single filled action. Mirrors the actions
// pane exactly: without a selected report the handover is not available — the
// button then dims (`:disabled`) instead of vanishing, so the head never
// changes height between reports. A report already in work (coding_turn_id)
// still opens the code view: the run is visible there.
function syncHeadDelegateButton(report) {
  const button = state.root?.querySelector('[data-head-delegate]');
  if (!button) return;
  const t = state.t || ((key, fallback) => fallback ?? key);
  button.disabled = !report;
  const title = !report
    ? t('selectReportTitle', 'Eintrag auswählen')
    : isCodingRunning(report)
      ? t('delegateCodingRunning', 'In Arbeit – in der Code-Ansicht öffnen')
      : t('delegateCoding', 'An Coding Agent übergeben');
  button.setAttribute('aria-label', title);
  button.setAttribute('title', title);
}

function updateToggleActionsAria(root) {
  const toggle = root?.querySelector('[data-toggle-actions]');
  if (!toggle) return;
  const hidden = root.classList.contains('is-actions-hidden');
  toggle.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  const label = hidden ? toggle.dataset.showLabel : toggle.dataset.hideLabel;
  if (label) {
    toggle.setAttribute('aria-label', label);
    toggle.setAttribute('title', label);
  }
}

function wireUi() {
  const root = state.root?.querySelector('[data-reports-root]');
  if (!root) return;
  // Header action: JSON export of the currently filtered list.
  root.querySelector('[data-action="export-json"]')?.addEventListener('click', () => exportVisibleReports());
  // Pane chrome is SHELL-owned canonical grammar (autoWirePaneGrammar wires
  // the data-pg-* markup once, debounced ~120ms after mount): search input,
  // shard/list toggle, collapsed tray with reset + active-dot, counted kind
  // band. The module only keeps its state in sync through the bubbling
  // grammar event and re-renders — the same contract knowledge/threads/
  // app-store use.
  root.querySelector('.reports-rail')?.addEventListener('ctox-pane-grammar-change', onRailGrammarChange);
  // One-button view switch. Bound on the button itself, not delegated: the
  // shell's grammar listener runs on the same click and re-renders, and a
  // delegated handler would receive an already-detached SVG target.
  // `data-pg-view` always carries the CURRENT view, so whichever of the two
  // listeners runs first, the grammar never flips the mode as a side effect.
  root.querySelector('[data-reports-view-toggle]')?.addEventListener('click', () => {
    state.viewMode = state.viewMode === 'list' ? 'cards' : 'list';
    syncViewToggleButton(root);
    renderList({ resetScroll: true });
  });
  // The one filled primary action of the detail head: hand the selected report
  // over to the coding agent. Same target as the actions-pane button, but the
  // pane is collapsed by default, so the head is where the flow action has to
  // live. Enablement is synced from renderDetail() (see syncHeadDelegateButton).
  root.querySelector('[data-head-delegate]')?.addEventListener('click', (event) => {
    if (event.currentTarget.disabled) return;
    const report = currentReportForActions();
    if (report) openInCodeView(report);
  });
  // Right actions column is collapsible — same toggle pattern threads/tickets
  // use. The toggle stays in the detail header so the actions pane never has
  // to render its own chrome.
  const toggleActions = root.querySelector('[data-toggle-actions]');
  if (toggleActions) {
    toggleActions.addEventListener('click', () => {
      root.classList.toggle('is-actions-hidden');
      updateToggleActionsAria(root);
    });
  }
  // Event delegation on the actions pane covers the focus-task and rollback
  // controls without re-binding listeners on every renderActions() rewrite.
  const actionsPane = root.querySelector('[data-reports-actions]');
  actionsPane?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest('[data-focus-task]')) {
      const report = currentReportForActions();
      if (report) focusCtoxTask(report);
      return;
    }
    const navigateButton = target?.closest('[data-navigate-app]');
    if (navigateButton) {
      state.api?.shell.navigate(navigateButton.getAttribute('data-navigate-app'));
      return;
    }
    if (target?.closest('[data-rollback-module]')) {
      const report = currentReportForActions();
      if (report) rollbackSelectedRelease(report);
      return;
    }
    const codeButton = target?.closest('[data-delegate-coding], [data-open-code-view]');
    if (codeButton && !codeButton.disabled) {
      const report = currentReportForActions();
      if (report) openInCodeView(report);
    }
  });
  // Use event delegation on the list container so the click handler survives
  // every renderList() innerHTML rewrite. Previously the per-button listeners
  // were re-attached inside renderList(), but a missed re-attach (or a
  // mid-flight rerender from realtime subscriptions) could orphan them and
  // leave the detail panel blank when a row was clicked.
  const list = root.querySelector('[data-reports-list]');
  list?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const button = target?.closest('[data-report-id]');
    if (!button || !list.contains(button)) return;
    const reportId = button.getAttribute('data-report-id') || '';
    if (!reportId || reportId === state.selectedId) {
      // Still re-render the detail in case it was previously cleared by a
      // refresh race — selecting the already-selected item should not be a no-op.
      state.selectedId = reportId;
      renderDetail();
      renderActions();
      return;
    }
    state.selectedId = reportId;
    // In-place selection flip — a selection click never rebuilds the list
    // (scroll + focus stay put); only detail and actions re-render.
    applyReportsSelection();
    renderDetail();
    renderActions();
  });
}

// Grammar state application (rail pane: search, cards/list view, counted kind
// band, status tray filter). Intentional reset: grammar changes move the
// content set, so the well scrolls back to the top (the shell scroll guard
// also clears its recorded offsets on this event).
function onRailGrammarChange(event) {
  const detail = event?.detail || {};
  state.search = String(detail.search ?? '');
  state.viewMode = detail.view === 'list' ? 'list' : 'cards';
  if (detail.band) state.kind = detail.band;
  state.status = String(detail.filters?.status ?? 'all') || 'all';
  syncSelectionToVisibleItems();
  render({ resetScroll: true });
}

// Selection is an in-place flip over the existing rows — never a list
// rebuild for a selection click (an innerHTML rebuild would clamp the
// well's scrollTop to 0).
function applyReportsSelection() {
  const list = state.root?.querySelector('[data-reports-list]');
  if (!list) return;
  for (const row of list.querySelectorAll('[data-report-id]')) {
    const selected = row.getAttribute('data-report-id') === state.selectedId;
    row.classList.toggle('is-selected', selected);
    row.setAttribute('aria-selected', selected ? 'true' : 'false');
  }
}

// Export serializes the currently visible (filtered + searched) report list
// as a JSON download — reports arrive through the CTOX intake flows, so
// there is no import action (adding one would invent a write flow the domain
// does not have). Same contract as the threads list export.
function exportVisibleReports() {
  const items = filteredReports();
  const exportedAt = new Date().toISOString();
  const payload = {
    format: 'ctox-reports-export',
    version: 1,
    exportedAt,
    module: 'reports',
    filter: { search: state.search || '', kind: state.kind || 'all', status: state.status || 'all' },
    count: items.length,
    reports: items.map((report) => ({
      reportId: String(report.id || ''),
      title: String(report.title || ''),
      kind: String(report.kind || ''),
      status: String(report.status || ''),
      severity: String(report.severity || ''),
      moduleId: String(report.moduleId || ''),
      commandId: String(report.commandId || ''),
      taskId: String(report.taskId || ''),
      createdAt: String(report.createdAt || ''),
      updatedAt: String(report.updatedAt || ''),
    })),
  };
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `ctox-reports-${exportedAt.slice(0, 19).replace(/[:T]/g, '-')}.json`;
  anchor.style.display = 'none';
  (state.root || document.documentElement).appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function scheduleRefresh() {
  if (state.renderTimer) return;
  state.renderTimer = setTimeout(() => {
    state.renderTimer = null;
    try { refreshReports(); } catch (error) { console.warn('[reports] refresh failed', error); }
  }, REPORTS_REFRESH_DEBOUNCE_MS);
}

// Live-Abfragen liefern lokal und vollständig (§10.2a); die Ansicht wird nur aus diesem Stand berechnet.
function refreshReports() {
  if (!state.root) return;
  const previousSelectedId = state.selectedId;
  const reports = state.data.business_module_reports || [];
  const bugs = state.data.ctox_bug_reports || [];
  const releases = state.data.business_module_releases || [];
  const commands = state.data.business_commands || [];
  const queue = state.data.ctox_queue_tasks || [];
  const nextRenderKey = buildRenderKey({ reports, bugs, releases, commands, queue });
  const hadSameData = nextRenderKey === state.renderKey;
  state.reports = reports;
  state.bugs = bugs;
  state.releases = releases;
  state.commands = commands;
  state.queue = queue;
  state.renderKey = nextRenderKey;
  syncSelectionToVisibleItems();
  if (hadSameData && previousSelectedId === state.selectedId && !state.pendingRestore) {
    return;
  }
  render();
  applyPendingRestore();
}

function buildRenderKey(collections) {
  const summarize = (items, fields) => items.map((item) => fields.map((field) => item[field] ?? '').join(':')).sort().join('|');
  return [
    summarize(collections.reports, ['id', 'report_id', 'status', 'updated_at_ms', 'ctox_command_id', 'task_id', 'coding_turn_id']),
    summarize(collections.bugs, ['id', 'report_id', 'status', 'updated_at_ms']),
    summarize(collections.releases, ['id', 'version_id', 'module_id', 'version', 'status', 'updated_at_ms']),
    summarize(collections.commands, ['id', 'command_id', 'status', 'updated_at_ms']),
    summarize(collections.queue, ['id', 'task_id', 'status', 'route_status', 'updated_at_ms']),
  ].join('\n');
}

function render({ resetScroll = false } = {}) {
  renderList({ resetScroll });
  renderDetail();
  renderActions();
}

function syncSelectionToVisibleItems() {
  const items = filteredReports();
  if (!items.length) {
    state.selectedId = '';
    return;
  }
  if (!state.selectedId || !items.some((item) => item.id === state.selectedId)) {
    state.selectedId = items[0]?.id || '';
  }
}

function rememberDetailScroll() {
  if (!state.renderedDetailId) return;
  const scroller = state.root?.querySelector('[data-reports-detail-scroll]');
  if (!scroller) return;
  state.detailScrollByReport[state.renderedDetailId] = scroller.scrollTop;
}

// ---------------------------------------------------------------------------
// Übergabe an den Coding Agent (Strang LOOP Runde 2, Kern §16.44). Coding-Aufträge entstehen nur in der Code-Ansicht
// der Ziel-App (eigener Entwurf, eigene Revision): „An Coding Agent übergeben“ öffnet sie über
// api.shell.openSource(moduleId, {tab:'reports', reportId}) direkt auf dem Reiter „Meldungen“ mit markierter Meldung;
// dort „Agent beauftragen“ (v4/source/source-mode.js assignReport). Der frühere Weg über eine threads-Freigabe
// (threads-Freigabeanfrage → ctox.coding.turn als Reviewer) ist entfallen: er konnte §16.44 nicht erfüllen
// (draft_not_owned, payload_invalid, veraltete base_draft_rev). „In Arbeit“ folgt aus report.coding_turn_id, das die
// Code-Ansicht beim Beauftragen setzt (v4/shell/report.js linkReportToTurn).
// ---------------------------------------------------------------------------
/** Ziel der Übergabe: Code-Ansicht der betroffenen App, Reiter „Meldungen“, Meldung markiert. */
export function codeViewTargetFor(report) {
  return { appId: report.moduleId, options: { tab: 'reports', reportId: report.rowId || report.id } };
}

function openInCodeView(report) {
  if (!report?.moduleId) return false;
  const { appId, options } = codeViewTargetFor(report);
  // Nicht möglich (kein apps.source.view, App ohne Quelltext) → die Shell meldet es in der Zustandszone.
  return state.api?.shell.openSource(appId, options) ?? false;
}

export function isCodingRunning(report) {
  return Boolean(report?.codingTurnId) && normalizeStatus(report.status) === 'running';
}

function delegationStatusHtml(report) {
  if (!report.codingTurnId) return '';
  const status = normalizeStatus(report.status);
  const badge = status === 'completed' ? 'is-success' : status === 'blocked' ? 'is-danger' : 'is-warning';
  return `<p class="reports-delegation-status" data-coding-turn="${escapeAttr(report.codingTurnId)}"><span class="ctox-badge ${badge}">Coding Agent · ${escapeHtml(displayStatus(status))}</span> <button type="button" class="reports-delegation-link" data-open-code-view>${escapeHtml(state.t('openCodeView', 'In der Code-Ansicht öffnen'))}</button></p>`;
}

function syncGrammarSurfaces(allItems, searched) {
  const counts = {
    all: searched.length,
    bug: searched.filter((item) => item.kind === 'bug').length,
    feature: searched.filter((item) => item.kind !== 'bug').length,
  };
  const root = state.root?.querySelector('[data-reports-root]');
  const grammar = root?.querySelector('.reports-rail')?.__ctoxPaneGrammar;
  if (grammar) {
    grammar.setCounts?.(counts);
    grammar.setFooter?.(''); // Textdiät R2: „6 Meldungen“ wiederholte nur den Reiterzähler „Alle (6)“.
    return;
  }
  for (const kind of ['all', 'bug', 'feature']) {
    const node = root?.querySelector(`[data-pg-count="${kind}"]`);
    if (node) node.textContent = ` (${counts[kind]})`;
  }
  const footer = root?.querySelector('.reports-rail [data-pg-footer]');
  if (footer) footer.textContent = '';
}

function renderList({ resetScroll = false } = {}) {
  const list = state.root?.querySelector('[data-reports-list]');
  if (!list) return;
  syncViewToggleButton(state.root?.querySelector('[data-reports-root]'));
  const items = filteredReports();
  const allItems = normalizedReports();
  // Counted view band (zeros included) + one-line footer.
  const searched = filterReportItems(normalizedReports(), { search: state.search, kind: 'all', status: state.status });
  syncGrammarSurfaces(allItems, searched);
  const well = state.root?.querySelector('.ctox-well');
  const savedScrollTop = well ? well.scrollTop : 0;
  list.classList.toggle('is-list-view', state.viewMode === 'list');
  if (!items.length) {
    // Leere Liste rendert nichts (Zustand nur in der Zustandszone).
    list.replaceChildren();
    if (resetScroll && well) requestAnimationFrame(() => { well.scrollTop = 0; });
    else if (well) well.scrollTop = savedScrollTop;
    return;
  }
  if (state.viewMode === 'list') {
    // LIST: exactly ONE compact line per entry — title plus a single short
    // meta on the right. Maximum density, no badges, no wrapping.
    list.innerHTML = items.map((report) => `
      <button type="button" class="ctox-list-item report-row-compact ${report.id === state.selectedId ? 'is-selected' : ''}" data-report-id="${escapeAttr(report.id)}" data-context-record-id="${escapeAttr(report.id)}" data-context-record-type="business_report" data-context-label="${escapeAttr(report.title || report.id)}" aria-selected="${report.id === state.selectedId ? 'true' : 'false'}">
        <span class="reports-compact-title">${escapeHtml(report.title)}</span>
        <span class="reports-compact-meta">${escapeHtml(displayStatus(report.status))}</span>
      </button>
    `).join('');
  } else {
    // CARDS (shards): title in bold, then the meta lines that actually decide
    // triage — type/status/severity badges and module · date. Roomier padding.
    list.innerHTML = items.map((report) => `
    <button type="button" class="ctox-list-item report-row ${report.id === state.selectedId ? 'is-selected' : ''}" data-report-id="${escapeAttr(report.id)}" data-context-record-id="${escapeAttr(report.id)}" data-context-record-type="business_report" data-context-label="${escapeAttr(report.title || report.id)}" aria-selected="${report.id === state.selectedId ? 'true' : 'false'}">
      <strong class="report-row-title">${escapeHtml(report.title)}</strong>
      <span class="reports-badges">
        <span class="ctox-badge ${report.kind === 'bug' ? 'is-danger' : 'is-feature'}">${escapeHtml(report.kindLabel)}</span>
        <span class="ctox-badge${statusBadgeClass(report.status)}">${escapeHtml(displayStatus(report.status))}</span>
        <span class="ctox-badge">${escapeHtml(severityLabel(report.severity))}</span>
      </span>
      <small class="report-row-sub">${escapeHtml([moduleLabel(report.moduleId), formatDate(report.updatedAt || report.createdAt)].filter(Boolean).join(' · '))}</small>
    </button>
  `).join('');
  }
  if (resetScroll && well) requestAnimationFrame(() => { well.scrollTop = 0; });
  else if (well) well.scrollTop = savedScrollTop;
  // Click handling is wired once via event delegation in wireUi(); re-binding
  // per-button on every renderList() is no longer needed.
}

function renderDetailFooter(report) {
  const node = state.root?.querySelector('.reports-detail [data-pg-footer]');
  if (!node) return;
  if (!report) { node.textContent = ''; return; }
  // Textdiät: Modul-ID und Status stehen schon im Kopf (Kicker) – der Fuß trägt nur noch die Übergabe.
  node.textContent = report.codingTurnId ? `Coding Agent: ${displayStatus(report.status)}` : '';
}

function renderDetail() {
  const detail = state.root?.querySelector('[data-reports-detail]');
  if (!detail) {
    // Container not mounted yet (or mount() was torn down) — bail gracefully
    // rather than throwing into nothing. A later refreshReports()/render()
    // will pick the report up once the DOM exists.
    return;
  }
  rememberDetailScroll();
  const filtered = filteredReports();
  const report = filtered.find((item) => item.id === state.selectedId) || null;
  renderDetailFooter(report);
  syncHeadDelegateButton(report);
  const kindLabelNode = detail.querySelector('[data-report-kind-label]');
  const titleNode = detail.querySelector('[data-report-title]');
  if (!report) {
    state.renderedDetailId = '';
    if (kindLabelNode) kindLabelNode.textContent = '';
    if (titleNode) titleNode.textContent = '';
    let scroller = detail.querySelector('[data-reports-detail-scroll]');
    if (!scroller) {
      scroller = document.createElement('div');
      scroller.className = 'ctox-pane-scroll reports-detail-scroll';
      scroller.setAttribute('data-reports-detail-scroll', '');
      detail.append(scroller);
    }
    scroller.replaceChildren();
    return;
  }
  const previousRenderedId = state.renderedDetailId;
  state.renderedDetailId = report.id;
  if (kindLabelNode) kindLabelNode.textContent = `${report.kindLabel} · ${displayStatus(report.status)}`;
  if (titleNode) titleNode.textContent = report.title;
  // normalizeReportItems() runs the raw value through objectValue(), which
  // returns `{}` for a missing attachment — truthy. Gate on the actual image
  // source, otherwise every report without a screenshot renders an empty
  // "Screenshot und Markup" card with a broken <img> (measured 31.08.2026).
  const attachment = report.attachment?.data_url ? report.attachment : null;
  let scroller = detail.querySelector('[data-reports-detail-scroll]');
  if (!scroller) {
    scroller = document.createElement('div');
    scroller.className = 'ctox-pane-scroll reports-detail-scroll';
    scroller.setAttribute('data-reports-detail-scroll', '');
    detail.append(scroller);
  }
  scroller.innerHTML = `
    <section class="ctox-card">
      <header>${escapeHtml(state.t('report', 'Eintrag'))}</header>
      <div class="ctox-card-body">
        <dl class="ctox-fields">
          ${moduleLabel(report.moduleId) ? fact(state.t('module', 'Modul'), moduleLabel(report.moduleId)) : ''}
          ${fact(state.t('severity', 'Priorität'), severityLabel(report.severity))}
          ${fact(state.t('created', 'Angelegt'), formatDate(report.createdAt))}
          ${fact(state.t('updated', 'Aktualisiert'), formatDate(report.updatedAt))}
        </dl>
      </div>
    </section>
    ${textCard(state.t('description', 'Beschreibung'), report.summary)}
    ${textCard(state.t('expectation', 'Erwartung'), report.expected)}
    ${textCard(state.t('whatCtoxChanged', 'Was CTOX geändert hat'), report.changeSummary)}
    ${attachment ? `
      <section class="ctox-card">
        <header>${escapeHtml(state.t('screenshotAndMarkup', 'Screenshot und Markup'))}</header>
        <div class="ctox-card-body">
          <div class="reports-attachment">
            <span class="reports-attachment-meta">${escapeHtml(attachment.capture_mode || 'capture')}</span>
            <img src="${escapeAttr(attachment.data_url)}" alt="Report Screenshot" />
          </div>
        </div>
      </section>
    ` : ''}
  `;
  const shouldRestore = previousRenderedId === report.id || Object.prototype.hasOwnProperty.call(state.detailScrollByReport, report.id);
  restoreDetailScroll(scroller, report.id, shouldRestore);
  scroller.addEventListener('scroll', () => {
    state.detailScrollByReport[report.id] = scroller.scrollTop;
  }, { passive: true });
}

// Right-pane actions: show CTOX task + Rollback form. Hidden by default via
// .is-actions-hidden on the module root; toggled from the detail header so
// the most-common case (just reading a report) keeps the full detail width.
function renderActions() {
  const actions = state.root?.querySelector('[data-reports-actions]');
  if (!actions) return;
  const filtered = filteredReports();
  const report = filtered.find((item) => item.id === state.selectedId) || null;
  if (!report) {
    actions.innerHTML = '<div class="ctox-pane-scroll reports-actions-scroll"></div>';
    return;
  }
  const releases = releasesForModule(report.moduleId);
  const hasCtoxTask = Boolean(report.commandId || report.taskId);
  actions.innerHTML = `
    <div class="ctox-pane-scroll reports-actions-scroll">
      <section class="ctox-card">
        <header>${escapeHtml(state.t('actionsTitle', 'Aktionen'))}</header>
        <div class="ctox-card-body">
          <button type="button" class="ctox-button is-primary reports-actions-task" data-focus-task ${hasCtoxTask ? '' : 'disabled'}>
            ${escapeHtml(state.t('showCtoxTask', 'CTOX-Aufgabe zeigen'))}
          </button>
        </div>
      </section>
      <section class="ctox-card">
        <header>${escapeHtml(state.t('codingAgent', 'Coding Agent'))}</header>
        <div class="ctox-card-body">
          <button type="button" class="ctox-button is-primary" data-delegate-coding title="${escapeAttr(state.t('delegateCodingTitle', 'Öffnet die Code-Ansicht der App mit dieser Meldung'))}">
            ${escapeHtml(state.t('delegateCoding', 'An Coding Agent übergeben'))}
          </button>
          ${delegationStatusHtml(report)}
        </div>
      </section>
      <section class="ctox-card">
        <header>${escapeHtml(state.t('rollback', 'Zurücksetzen'))}</header>
        <div class="ctox-card-body">
          <div class="reports-rollback">
            ${releases.length ? `<p class="reports-rollback-prompt">${escapeHtml(state.t('rollbackPrompt', 'Wähle eine gespeicherte Modulversion und rolle das betroffene Modul zurück.'))}</p>` : ''}
            <select class="ctox-select" data-rollback-version ${releases.length ? '' : 'disabled'}>
              ${releases.map((release) => `<option value="${escapeAttr(release.versionId)}">v${escapeHtml(release.version)} · ${escapeHtml(release.status || '')} · ${escapeHtml(formatDate(release.createdAt))}</option>`).join('')}
            </select>
            <button type="button" class="ctox-button is-primary reports-rollback-action" data-rollback-module ${releases.length ? '' : 'disabled'}>${escapeHtml(state.t('rollback', 'Zurücksetzen'))}</button>
          </div>
        </div>
      </section>
    </div>
  `;
}

// Click delegation looks up the currently selected report. Centralising the
// lookup keeps the actions handlers free of stale report references across
// re-renders.
function currentReportForActions() {
  const filtered = filteredReports();
  return filtered.find((item) => item.id === state.selectedId) || null;
}

function restoreDetailScroll(scroller, reportId, shouldRestore) {
  const target = shouldRestore ? state.detailScrollByReport[reportId] || 0 : 0;
  const apply = () => { scroller.scrollTop = target; };
  apply();
  requestAnimationFrame(() => {
    apply();
    scroller.querySelectorAll('img').forEach((img) => {
      if (img.complete) return;
      img.addEventListener('load', apply, { once: true });
      img.addEventListener('error', apply, { once: true });
    });
  });
}

function filteredReports() {
  return filterReportItems(normalizedReports(), {
    search: state.search,
    kind: state.kind,
    status: state.status,
  });
}

function normalizedReports() {
  return normalizeReportItems({
    reports: state.reports,
    bugs: state.bugs,
    commands: state.commands,
    queue: state.queue,
    t: state.t,
  });
}

export function normalizeReportItems({ reports = [], bugs = [], commands = [], queue = [], t = defaultTranslate } = {}) {
  const reportById = new Map();
  const bugById = new Map();
  const ids = [];
  for (const report of reports) {
    const id = reportIdFor(report);
    if (!id) continue;
    if (!reportById.has(id)) ids.push(id);
    reportById.set(id, report);
  }
  for (const bug of bugs) {
    const id = reportIdFor(bug);
    if (!id) continue;
    if (!reportById.has(id) && !bugById.has(id)) ids.push(id);
    bugById.set(id, bug);
  }
  const commandById = keyedByAny(commands, ['command_id', 'id']);
  const queueByTaskId = keyedByAny(queue, ['task_id', 'id']);
  return ids.map((id) => {
    const report = reportById.get(id) || {};
    const bug = bugById.get(id) || {};
    const payload = objectValue(bug.payload || report.payload);
    const clientContext = objectValue(report.client_context || bug.evidence);
    const commandId = report.ctox_command_id || payload.ctox_command_id || report.command_id || '';
    const taskId = report.task_id || payload.task_id || '';
    const command = commandById.get(commandId) || {};
    const task = queueByTaskId.get(taskId) || {};
    const baseStatus = task.route_status || task.status || report.status || bug.status || command.status || 'open';
    // LOOP Runde 2: ein Agent-Lauf aus der Code-Ansicht (coding_turn_id) heißt „In Arbeit“, bis er behoben/blockiert ist.
    const codingTurnId = typeof report.coding_turn_id === 'string' ? report.coding_turn_id : '';
    const status = codingTurnId && normalizeStatus(baseStatus) === 'open' ? 'running' : baseStatus;
    const kind = normalizeKind(report.kind || payload.kind || bug.kind);
    const changeSummary = payload.change_summary
      || payload.ctox_change_summary
      || clientContext.ctox_change_summary
      || task.result?.summary
      || task.result_summary
      || '';
    return {
      id,
      kind,
      kindLabel: kind === 'bug' ? t('bugs', 'Bug') : t('features', 'Feature'),
      severity: report.severity || bug.severity || '',
      title: report.title || bug.title || id,
      summary: report.summary || bug.description || '',
      expected: report.expected || payload.expected || '',
      status,
      moduleId: report.module_id || bug.module || report.inbound_channel || bug.inbound_channel || 'ctox',
      // Zeile in business_module_reports (Markierung im Reiter „Meldungen“ der Code-Ansicht).
      rowId: typeof report.id === 'string' ? report.id : '',
      codingTurnId,
      commandId,
      taskId,
      changeSummary,
      rollbackVersionId: payload.rollback_version_id || clientContext.rollback_version_id || '',
      attachment: objectValue(clientContext.attachment),
      createdAt: report.created_at_ms || bug.created_at_ms || report.updated_at_ms || bug.updated_at_ms || 0,
      updatedAt: report.updated_at_ms || bug.updated_at_ms || report.created_at_ms || bug.created_at_ms || 0,
    };
  }).sort((left, right) => (right.updatedAt || 0) - (left.updatedAt || 0));
}

export function filterReportItems(items, { search = '', kind = 'all', status = 'all' } = {}) {
  const query = String(search || '').trim().toLowerCase();
  return items.filter((report) => {
    if (kind !== 'all' && report.kind !== kind) return false;
    if (status !== 'all' && normalizeStatus(report.status) !== status) return false;
    if (!query) return true;
    return [report.title, report.summary, report.expected, report.moduleId, report.commandId, report.taskId]
      .some((value) => String(value || '').toLowerCase().includes(query));
  });
}

function keyedByAny(items, keys) {
  const map = new Map();
  for (const item of items) {
    for (const key of keys) {
      const value = item?.[key];
      if (value) map.set(value, item);
    }
  }
  return map;
}

function reportIdFor(item) {
  return item?.report_id || item?.id || '';
}

function defaultTranslate(_key, fallback) {
  return fallback;
}

function releasesForModule(moduleId) {
  return state.releases
    .filter((release) => release.module_id === moduleId)
    .map((release) => ({
      versionId: release.version_id || release.id,
      version: release.version || 0,
      status: release.status || '',
      createdAt: release.created_at_ms || release.updated_at_ms || 0,
    }))
    .sort((left, right) => (right.version || 0) - (left.version || 0));
}

// Sprung in die CTOX-App über api.shell.navigate (Ziel liest die Parameter über onRoute).
function focusCtoxTask(report) {
  state.api?.shell.navigate('ctox', {
    task_id: report.taskId,
    command_id: report.commandId,
    task_status: report.status,
    source_module: report.moduleId,
  });
}

function rollbackSelectedRelease(report) {
  const actions = state.root?.querySelector('[data-reports-actions]');
  if (!actions) return;
  const versionId = actions.querySelector('[data-rollback-version]')?.value || report.rollbackVersionId || '';
  if (!versionId) return;
  dispatchModuleCommand({
    commandType: 'ctox.module.rollback',
    moduleId: report.moduleId,
    recordId: versionId,
    payload: { module_id: report.moduleId, version_id: versionId },
    source: 'business-os-reports',
    label: state.t('rollback', 'Zurücksetzen'),
  });
}

// Befehl absetzen, nie warten (R5). failed bzw. lokal ungültig → Zustandszone, done → Eintrag löschen.
function dispatchModuleCommand({
  commandType,
  moduleId,
  recordId,
  payload,
  source,
  label = commandType,
  module = 'ctox',
}) {
  const api = state.api;
  if (!api) return null;
  const code = `reports.command.${commandType}`;
  const record_id = recordId || moduleId;
  const failText = (error) => `${state.t('commandFailed', 'Aktion {0} ist fehlgeschlagen.', label)}${error ? ` ${error}` : ''}`;
  let handle;
  try {
    handle = api.command(commandType, {
      ...payload,
      module,
      record_id,
      inbound_channel: moduleId,
      client_context: {
        source,
        module_id: moduleId,
        actor: actorContext(),
      },
    }, { record_id });
  } catch (error) {
    api.status.report({ level: 'error', code, text: failText(safeErrorMessage(error)) });
    return null;
  }
  handle.local?.catch?.((error) => api.status.report({ level: 'error', code, text: failText(safeErrorMessage(error)), ref: handle.id }));
  let off = null;
  let finished = false;
  const stop = () => {
    finished = true;
    if (off) { off(); off = null; }
  };
  off = handle.status.subscribe((commandState) => {
    if (commandState?.status === 'failed') {
      api.status.report({ level: 'error', code, text: failText(commandState.error || ''), ref: handle.id });
      stop();
    } else if (commandState?.status === 'done') {
      api.status.clear(code);
      stop();
    }
  });
  if (finished && off) { off(); off = null; }
  if (off) state.disposers.push(() => off?.());
  return handle;
}

// Leere Textfelder rendern keine Karte (keine Platzhaltertexte im Inhalt).
function textCard(label, value) {
  if (!String(value || '').trim()) return '';
  return `
    <section class="ctox-card">
      <header>${escapeHtml(label)}</header>
      <div class="ctox-card-body">
        <p>${escapeHtml(value)}</p>
      </div>
    </section>`;
}

function fact(label, value) {
  return `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value || '-')}</dd>`;
}

function statusBadgeClass(status) {
  const normalized = normalizeStatus(status);
  if (normalized === 'completed') return ' is-success';
  if (normalized === 'blocked') return ' is-danger';
  if (normalized === 'running') return ' is-warning';
  return '';
}

function normalizeKind(value) {
  const text = String(value || '').toLowerCase();
  return text.includes('feature') || text.includes('request') ? 'feature' : 'bug';
}

function normalizeStatus(value) {
  const text = String(value || '').toLowerCase();
  if (['done', 'completed', 'handled', 'passed', 'approved'].includes(text)) return 'completed';
  if (['running', 'leased', 'working', 'review', 'drafting'].includes(text)) return 'running';
  if (['blocked', 'failed', 'cancelled', 'canceled'].includes(text)) return 'blocked';
  return 'open';
}

// Textdiät Regel 4: Rohwerte (high/medium/low, pending, App-IDs) in Nutzerbegriffen.
function severityLabel(value) {
  const key = String(value || 'medium').trim().toLowerCase();
  const de = { critical: 'Kritisch', urgent: 'Dringend', high: 'Hoch', medium: 'Mittel', normal: 'Mittel', low: 'Niedrig' };
  const en = { critical: 'Critical', urgent: 'Urgent', high: 'High', medium: 'Medium', normal: 'Medium', low: 'Low' };
  return (state.lang === 'en' ? en : de)[key] || '';
}

function moduleLabel(id) {
  const key = String(id || '').trim();
  const mod = (Array.isArray(state.apps) ? state.apps : []).find((item) => item && item.id === key);
  // DARK: nur der App-Titel aus dem Katalog; ohne Katalogeintrag nichts (keine Roh-ID wie „module-001“).
  return String(mod?.title || mod?.name || '');
}

function displayStatus(value) {
  const normalized = normalizeStatus(value);
  return {
    open: state.t('open', 'Offen'),
    running: state.t('running', 'In Arbeit'),
    completed: state.t('completed', 'Erledigt'),
    blocked: state.t('blocked', 'Blockiert'),
  }[normalized] || value || state.t('open', 'Offen');
}

function objectValue(value) {
  if (value && typeof value === 'object') return value;
  if (typeof value !== 'string') return {};
  const trimmed = value.trim();
  if (!trimmed || !/^[{[]/.test(trimmed)) return {};
  try {
    const parsed = JSON.parse(trimmed);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function formatDate(value) {
  const timestamp = typeof value === 'number' ? value : Date.parse(value || '');
  if (!Number.isFinite(timestamp) || timestamp <= 0) return '-';
  return new Intl.DateTimeFormat(state.lang === 'en' ? 'en-US' : 'de-DE', {
    day: '2-digit',
    month: '2-digit',
    year: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(timestamp));
}

function actorContext() {
  const actor = state.api?.actor || {};
  return {
    id: actor.id || '',
    display_name: actor.display_name || actor.id || '',
    role: actor.role || 'user',
    is_admin: String(actor.role || '') === 'admin',
  };
}

function safeErrorMessage(error) {
  if (!error) return '';
  if (typeof error === 'string') return error;
  return error.message || error.reason || String(error);
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}

export function resolveReportsContextRecord({
  clickedReportId = '',
  selectedId = '',
  visibleReports = [],
  allReports = [],
} = {}) {
  return visibleReports.find((item) => item.id === clickedReportId)
    || visibleReports.find((item) => item.id === selectedId)
    || allReports.find((item) => item.id === clickedReportId)
    || allReports.find((item) => item.id === selectedId)
    || allReports[0]
    || null;
}
