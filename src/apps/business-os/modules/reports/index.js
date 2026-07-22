import { loadModuleMessages } from '../../shared/i18n.js';

const REPORTS_REFRESH_DEBOUNCE_MS = 80;
const REPORTS_SYNC_RESTART_TIMEOUT_MS = 6000;
const REPORT_COLLECTIONS = [
  'business_module_reports',
  'ctox_bug_reports',
  'business_module_releases',
  'business_commands',
  'ctox_queue_tasks',
  'ctox_task_approval_requests',
  'business_users',
];
const REPORT_DATA_COLLECTIONS = ['business_module_reports', 'ctox_bug_reports'];

const state = {
  ctx: null,
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
  approvals: [],
  users: [],
  cleanup: null,
  contextMenu: null,
  contextMenuCleanup: null,
  renderTimer: null,
  renderKey: '',
  renderedDetailId: '',
  detailScrollByReport: {},
  diagnostics: createDiagnosticsState(),
  t: null,
  lang: 'de',
};

export async function mount(ctx) {
  state.ctx = ctx;
  // Reset volatile state on every mount so a remount can't leak a stale
  // selectedId/renderedDetailId from a previous host element.
  state.selectedId = '';
  state.renderedDetailId = '';
  state.renderKey = '';
  state.diagnostics = createDiagnosticsState();
  await ensureStyles();

  // Load localizations
  const messages = await loadModuleMessages(import.meta.url, ctx.locale || 'de', {});
  state.t = (key, fallback, ...args) => {
    let val = messages[key] ?? fallback ?? key;
    if (args.length) {
      args.forEach((arg, i) => {
        val = String(val).replace(`{${i}}`, arg);
      });
    }
    return val;
  };
  state.lang = ctx.locale === 'en' ? 'en' : 'de';

  ctx.host.innerHTML = await loadModuleMarkup();

  applyStaticLabels(ctx.host, state.t);

  ctx.left.replaceChildren();
  ctx.right.replaceChildren();
  wireUi();
  // Local-first: subscribe first, then load in the background. The shell HTML
  // is already in the DOM; awaiting the (local, but potentially large)
  // multi-collection read before returning made the open feel laggy. The
  // realtime subscription re-renders as soon as the read resolves and on every
  // later change.
  state.cleanup = wireRealtime();
  refreshReports().catch((error) => {
    console.warn('[reports] initial load failed', error);
  });

  // Column resizing is owned by the shell-global resizer (setupModuleResizers
  // in app.js), wired declaratively from the `.ctox-column-resizer[data-resizer-var]`
  // handle inside the `[data-resize-frame]` root — no module JS needed.
  window.addEventListener('ctox-business-os-reports-updated', handleReportsUpdated);
  return () => {
    state.cleanup?.();
    state.contextMenuCleanup?.();
    state.contextMenu?.remove();
    state.contextMenu = null;
    window.removeEventListener('ctox-business-os-reports-updated', handleReportsUpdated);
    if (state.renderTimer) window.clearTimeout(state.renderTimer);
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-reports-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  const styleUrl = new URL('./index.css', import.meta.url);
  // Inherit the module's own cache-buster (index.js is imported with
  // ?v=<build>): fresh JS must never render against a stale cached sheet.
  const version = String(import.meta.url).split('?v=')[1] || '20260722-reports-grammar-v1';
  styleUrl.searchParams.set('v', version);
  link.href = styleUrl.href;
  link.dataset.reportsStyle = 'true';
  document.head.append(link);
}

async function loadModuleMarkup() {
  // Markup inherits the JS cache-buster — like the stylesheet, a deploy must
  // never leave fresh JS binding against stale cached markup (same contract
  // as ctox/coding-agents/knowledge/threads/app-store).
  const version = String(import.meta.url).split('?v=')[1] || '20260722-reports-grammar-v1';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (version ? `?v=${version}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
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

  // Right-pane actions toggle — title and aria label swap depending on state.
  const toggleActions = root.querySelector('[data-toggle-actions]');
  if (toggleActions) {
    toggleActions.dataset.showLabel = t('showActions', 'Aktionen einblenden');
    toggleActions.dataset.hideLabel = t('hideActions', 'Aktionen ausblenden');
    updateToggleActionsAria(root);
  }
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
  const root = state.ctx.host.querySelector('[data-reports-root]');
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
    if (target?.closest('[data-rollback-module]')) {
      const report = currentReportForActions();
      if (report) rollbackSelectedRelease(report);
      return;
    }
    if (target?.closest('[data-delegate-coding]') && !target.closest('[data-delegate-coding]').disabled) {
      const report = currentReportForActions();
      if (report) openDelegateDialog(report);
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
  const list = state.ctx?.host?.querySelector('[data-reports-list]');
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
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function wireRealtime() {
  const subscriptions = REPORT_COLLECTIONS.map((name) => reportCollection(name)?.$?.subscribe?.(() => scheduleRefresh())).filter(Boolean);
  return () => subscriptions.forEach((sub) => {
    try { sub.unsubscribe?.(); } catch {}
  });
}

function reportCollection(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

function handleReportsUpdated() {
  scheduleRefresh();
}

function scheduleRefresh() {
  if (state.renderTimer) return;
  state.renderTimer = window.setTimeout(() => {
    state.renderTimer = null;
    refreshReports().catch((error) => console.warn('[reports] refresh failed', error));
  }, REPORTS_REFRESH_DEBOUNCE_MS);
}

async function refreshReports(options = {}) {
  const previousSelectedId = state.selectedId;
  state.diagnostics = {
    ...state.diagnostics,
    loading: true,
    lastAttemptAt: Date.now(),
    lastManual: Boolean(options.manual),
  };

  const syncErrors = options.restartSync ? await restartReportSync() : [];
  const results = await Promise.all(REPORT_COLLECTIONS.map((name) => loadCollectionResult(name)));
  const byName = Object.fromEntries(results.map((result) => [result.name, result]));
  const reports = byName.business_module_reports?.items || [];
  const bugs = byName.ctox_bug_reports?.items || [];
  const releases = byName.business_module_releases?.items || [];
  const commands = byName.business_commands?.items || [];
  const queue = byName.ctox_queue_tasks?.items || [];
  state.approvals = byName.ctox_task_approval_requests?.items || [];
  state.users = (byName.business_users?.items || []).filter((user) => user && user.active !== false);
  reconcileDelegationBackfill(reports, commands);
  const nextRenderKey = buildRenderKey({ reports, bugs, releases, commands, queue });
  const hadSameData = nextRenderKey === state.renderKey;
  state.reports = reports;
  state.bugs = bugs;
  state.releases = releases;
  state.commands = commands;
  state.queue = queue;
  state.diagnostics = buildDiagnosticsState({
    results,
    syncErrors,
    syncDiagnostics: state.ctx.sync?.diagnostics,
    reportCount: normalizeReportItems({ reports, bugs, commands, queue, t: state.t }).length,
    manual: Boolean(options.manual),
  });
  state.renderKey = nextRenderKey;
  syncSelectionToVisibleItems();
  if (hadSameData && previousSelectedId === state.selectedId) {
    return;
  }
  render();
}

async function restartReportSync() {
  const sync = state.ctx.sync;
  if (!sync) return [];
  const errors = [];
  try {
    if (typeof sync.restartCollections === 'function') {
      await withTimeout(sync.restartCollections(REPORT_COLLECTIONS), REPORTS_SYNC_RESTART_TIMEOUT_MS, 'Sync-Neustart laeuft noch');
      return errors;
    }
  } catch (error) {
    errors.push({ name: 'sync', message: safeErrorMessage(error) });
  }
  await Promise.all(REPORT_COLLECTIONS.map(async (name) => {
    try {
      await withTimeout(sync.startCollection?.(name), REPORTS_SYNC_RESTART_TIMEOUT_MS, 'Sync-Start laeuft noch');
    } catch (error) {
      errors.push({ name, message: safeErrorMessage(error) });
    }
  }));
  return errors;
}

async function loadCollectionResult(name) {
  const collection = reportCollection(name);
  if (!collection) return { name, items: [], missing: true, error: '' };
  try {
    const docs = await collection.find().exec();
    return { name, items: docs.map((doc) => doc.toJSON()), missing: false, error: '' };
  } catch (error) {
    return { name, items: [], missing: false, error: safeErrorMessage(error) };
  }
}

function buildRenderKey(collections) {
  const summarize = (items, fields) => items.map((item) => fields.map((field) => item[field] ?? '').join(':')).sort().join('|');
  return [
    summarize(collections.reports, ['id', 'report_id', 'status', 'updated_at_ms', 'ctox_command_id', 'task_id']),
    summarize(collections.bugs, ['id', 'report_id', 'status', 'updated_at_ms']),
    summarize(collections.releases, ['id', 'version_id', 'module_id', 'version', 'status', 'updated_at_ms']),
    summarize(collections.commands, ['id', 'command_id', 'status', 'updated_at_ms']),
    summarize(collections.queue, ['id', 'task_id', 'status', 'route_status', 'updated_at_ms']),
    summarize(state.approvals || [], ['id', 'approval_request_id', 'status', 'updated_at_ms']),
  ].join('\n');
}

function render({ resetScroll = false } = {}) {
  renderList({ resetScroll });
  renderDetail();
  renderActions();
}

function renderListEmptyState(allItems) {
  if (hasBlockingReportDiagnostic()) {
    return `<div class="ctox-empty"><strong>${escapeHtml(state.t('reportsListUnavailable', 'Noch keine Einträge'))}</strong><span>${escapeHtml(state.t('reportsListUnavailableDetail', 'Wird automatisch gefüllt.'))}</span></div>`;
  }
  if (allItems.length) {
    return `<p class="ctox-empty">${escapeHtml(state.t('noFilteredReports', 'Keine Einträge im aktuellen Filter.'))}</p>`;
  }
  return `<p class="ctox-empty">${escapeHtml(reportStoreEmptyMessage(state.t('noReports', 'Noch keine Einträge.')))}</p>`;
}

function renderDetailEmptyState({ normalized, filtered }) {
  if (hasBlockingReportDiagnostic()) {
    return `<div class="ctox-empty"><strong>${escapeHtml(state.t('reportsUnavailable', 'Bugs & Features sind gerade nicht verfügbar.'))}</strong><span>${escapeHtml(state.t('reportsUnavailableDetail', 'Die Liste wird automatisch gefüllt, sobald Einträge geladen sind.'))}</span></div>`;
  }
  if (!normalized.length) {
    return `<div class="ctox-empty"><strong>${escapeHtml(state.t('noReportsTitle', 'Noch keine Bugs oder Features'))}</strong><span>${escapeHtml(reportStoreEmptyMessage(state.t('noReportsDetail', 'Sobald Bugs oder Feature-Wünsche vorliegen, erscheinen Liste und Details hier.')))}</span></div>`;
  }
  if (!filtered.length) {
    return `<div class="ctox-empty"><strong>${escapeHtml(state.t('noFilteredReportsTitle', 'Filter ohne Treffer'))}</strong><span>${escapeHtml(state.t('noFilteredReportsDetail', 'Suche oder Filter ändern, um wieder Details zu sehen.'))}</span></div>`;
  }
  return `<div class="ctox-empty"><strong>${escapeHtml(state.t('selectReportTitle', 'Eintrag auswählen'))}</strong><span>${escapeHtml(state.t('selectReport', 'Wähle links einen Bug oder Feature-Wunsch aus.'))}</span></div>`;
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
  const scroller = state.ctx.host.querySelector('[data-reports-detail-scroll]');
  if (!scroller) return;
  state.detailScrollByReport[state.renderedDetailId] = scroller.scrollTop;
}

// ---------------------------------------------------------------------------
// Coding-agent delegation: a report is handed to the pi coding agent through
// a Threads approval (threads.ctox_approval.request with a target command).
// On approve the native side enqueues ctox.coding.turn for the target module.
// ---------------------------------------------------------------------------
// Durable Rückweg: once the approved coding turn exists, stamp its command/
// task ids (and a running/blocked status while the report is still open) back
// onto the report record so every surface — not just this app — sees the
// linkage. Idempotent: only writes when a field actually changes.
async function reconcileDelegationBackfill(reports, commands) {
  try {
    const collection = reportCollection('business_module_reports');
    if (!collection) return;
    for (const report of reports || []) {
      const reportId = report?.id || report?.report_id;
      if (!reportId) continue;
      const turn = (commands || []).find((item) => String(item?.command_type || item?.type || '') === 'ctox.coding.turn'
        && item?.payload?.context?.record_id === reportId);
      if (!turn) continue;
      const commandId = String(turn.command_id || turn.id || '');
      const taskId = String(turn.task_id || '');
      const turnStatus = String(turn.status || turn.task_status || '');
      const nextStatus = turnStatus === 'completed' ? 'completed'
        : turnStatus === 'failed' ? 'blocked'
          : 'running';
      const statusChangeAllowed = ['open', 'running'].includes(String(report.status || 'open'));
      const patch = {};
      if (commandId && report.ctox_command_id !== commandId) patch.ctox_command_id = commandId;
      if (taskId && report.task_id !== taskId) patch.task_id = taskId;
      if (statusChangeAllowed && report.status !== nextStatus) patch.status = nextStatus;
      if (!Object.keys(patch).length) continue;
      await collection.upsert({ ...report, ...patch, updated_at_ms: Date.now() });
    }
  } catch (error) {
    console.warn('[reports] delegation backfill skipped', error);
  }
}

function delegationInfoFor(report) {
  const approval = (state.approvals || []).find((item) => {
    const source = item?.source_context || {};
    return item?.target_record_id === report.id || source.record_id === report.id;
  }) || null;
  const command = (state.commands || []).find((item) => {
    if (String(item?.command_type || item?.type || '') !== 'ctox.coding.turn') return false;
    const context = item?.payload?.context || {};
    return context.record_id === report.id;
  }) || null;
  return { approval, command };
}

function delegationStatusHtml(report) {
  const { approval, command } = delegationInfoFor(report);
  if (command) {
    const status = String(command.status || command.task_status || 'running');
    const badge = status === 'completed' ? 'is-success' : status === 'failed' ? 'is-danger' : 'is-warning';
    return `<p class="reports-delegation-status"><span class="ctox-badge ${badge}">Coding Agent · ${escapeHtml(displayStatus(status))}</span> <a href="#coding-agents" class="reports-delegation-link">Im Coding-Agents-Log öffnen</a></p>`;
  }
  if (approval) {
    const status = String(approval.status || 'pending');
    const badge = status === 'approved' ? 'is-success' : status === 'rejected' ? 'is-danger' : 'is-warning';
    const label = status === 'pending' ? `Freigabe offen · ${escapeHtml(approval.reviewer_display_name || approval.reviewer_user_id || '')}`
      : status === 'approved' ? 'Freigegeben' : status === 'rejected' ? 'Abgelehnt' : escapeHtml(status);
    return `<p class="reports-delegation-status"><span class="ctox-badge ${badge}">${label}</span> <a href="#threads" class="reports-delegation-link">In Threads öffnen</a></p>`;
  }
  return '';
}

function editableTargetModules() {
  const modules = Array.isArray(state.ctx?.modules) ? state.ctx.modules : [];
  const seen = new Set();
  return modules
    .filter((mod) => mod && typeof mod.id === 'string' && mod.id.trim() && mod.hidden !== true && mod.editable !== false)
    .map((mod) => ({ id: mod.id.trim(), title: String(mod.title || mod.id).trim() }))
    .filter((mod) => (seen.has(mod.id) ? false : (seen.add(mod.id), true)))
    .sort((left, right) => left.title.localeCompare(right.title, undefined, { sensitivity: 'base' }));
}

function delegationPromptFor(report) {
  const lines = [
    `Bug/Feature-Report aus der Reports-App (${report.kindLabel || report.kind || 'Report'} · ${report.id}):`,
    '',
    `Titel: ${report.title || ''}`,
  ];
  if (report.summary) lines.push('', `Beschreibung: ${report.summary}`);
  if (report.expected) lines.push('', `Erwartet: ${report.expected}`);
  if (report.moduleId) lines.push('', `Betroffenes Modul: ${report.moduleId}`);
  lines.push('', 'Behebe das Problem in einem begrenzten Coding-Turn und halte die Änderung klein.');
  return lines.join('\n');
}

function openDelegateDialog(report) {
  const host = state.ctx.host;
  host.querySelector('[data-delegate-modal]')?.remove();
  const actorId = state.ctx.session?.user_id || state.ctx.session?.userId || '';
  // The reviewer must themselves be authorized to run coding turns for the
  // module (server policy refuses otherwise) — offer only admin/chef roles.
  const reviewers = (state.users || []).filter((user) => (user.id || user.user_id) !== actorId
    && ['admin', 'chef'].includes(String(user.role || '').toLowerCase()));
  const targets = editableTargetModules();
  const preferred = targets.some((mod) => mod.id === report.moduleId) ? report.moduleId : (targets[0]?.id || '');
  const wrap = document.createElement('div');
  wrap.className = 'ctox-modal';
  wrap.setAttribute('data-delegate-modal', '');
  wrap.innerHTML = `
    <div class="ctox-modal-card">
      <header class="ctox-modal-header"><h3 class="ctox-modal-title">An Coding Agent übergeben</h3></header>
      <div class="ctox-modal-body reports-delegate-body">
        <label class="ctox-field-label" for="delegate-target">Ziel-App</label>
        <select class="ctox-select" id="delegate-target">${targets.map((mod) => `<option value="${escapeAttr(mod.id)}" ${mod.id === preferred ? 'selected' : ''}>${escapeHtml(mod.title)} (${escapeHtml(mod.id)})</option>`).join('')}</select>
        <label class="ctox-field-label" for="delegate-reviewer">Freigabe durch</label>
        ${reviewers.length
          ? `<select class="ctox-select" id="delegate-reviewer">${reviewers.map((user) => `<option value="${escapeAttr(user.id || user.user_id)}">${escapeHtml(user.display_name || user.name || user.id || user.user_id)}</option>`).join('')}</select>`
          : `<input class="ctox-input" id="delegate-reviewer" type="text" placeholder="Reviewer-User-ID (z. B. alice)" aria-label="Reviewer-User-ID">`}
        <label class="ctox-field-label" for="delegate-prompt">Auftrag an den Agenten</label>
        <textarea class="ctox-textarea" id="delegate-prompt" rows="8"></textarea>
        <p class="reports-delegate-hint">Die Delegation läuft als Freigabe über Threads; erst nach der Freigabe startet der Coding-Turn.</p>
      </div>
      <footer class="ctox-modal-footer">
        <button type="button" class="ctox-button" data-delegate-cancel>Abbrechen</button>
        <button type="button" class="ctox-button is-primary" data-delegate-submit ${targets.length ? '' : 'disabled'}>Zur Freigabe einreichen</button>
      </footer>
    </div>
  `;
  host.appendChild(wrap);
  const promptInput = wrap.querySelector('#delegate-prompt');
  promptInput.value = delegationPromptFor(report);
  wrap.querySelector('[data-delegate-cancel]').addEventListener('click', () => wrap.remove());
  wrap.addEventListener('click', (event) => { if (event.target === wrap) wrap.remove(); });
  wrap.querySelector('[data-delegate-submit]').addEventListener('click', async () => {
    const target = wrap.querySelector('#delegate-target').value;
    const reviewer = wrap.querySelector('#delegate-reviewer').value.trim();
    const prompt = promptInput.value.trim();
    if (!target || !reviewer || prompt.length < 20) {
      state.ctx.notifications?.notify?.({ title: 'Delegation unvollständig', body: 'Ziel-App, Reviewer und ein aussagekräftiger Auftrag sind nötig.' });
      return;
    }
    try {
      await dispatchModuleCommand({
        commandType: 'threads.ctox_approval.request',
        module: 'threads',
        moduleId: target,
        recordId: report.id,
        source: 'reports-coding-delegation',
        payload: {
          approval_request_id: `approval_${newId()}`,
          title: `Coding-Delegation: ${report.title || report.id}`,
          prompt,
          reviewer_user_id: reviewer,
          target_module: target,
          target_record_id: report.id,
          target_command_type: 'ctox.coding.turn',
          target_payload: { module_id: target, prompt },
          source_context: {
            module: 'reports',
            record_type: report.kind === 'bug' ? 'bug_report' : 'feature_request',
            record_id: report.id,
            label: report.title || report.id,
          },
        },
      });
      wrap.remove();
      state.ctx.notifications?.notify?.({ title: 'Zur Freigabe eingereicht', body: `Der Auftrag wartet in Threads auf die Freigabe.` });
      await refreshReports({});
    } catch (error) {
      state.ctx.notifications?.notify?.({ title: 'Delegation fehlgeschlagen', body: safeErrorMessage(error) });
    }
  });
}

function syncGrammarSurfaces(allItems, searched) {
  const counts = {
    all: searched.length,
    bug: searched.filter((item) => item.kind === 'bug').length,
    feature: searched.filter((item) => item.kind !== 'bug').length,
  };
  const root = state.ctx.host.querySelector('[data-reports-root]');
  const grammar = root?.querySelector('.reports-rail')?.__ctoxPaneGrammar;
  if (grammar) {
    grammar.setCounts?.(counts);
    grammar.setFooter?.(`${allItems.length} Meldungen`);
    return;
  }
  for (const kind of ['all', 'bug', 'feature']) {
    const node = root?.querySelector(`[data-pg-count="${kind}"]`);
    if (node) node.textContent = ` (${counts[kind]})`;
  }
  const footer = root?.querySelector('.reports-rail [data-pg-footer]');
  if (footer) footer.textContent = `${allItems.length} Meldungen`;
}

function renderList({ resetScroll = false } = {}) {
  const list = state.ctx.host.querySelector('[data-reports-list]');
  if (!list) return;
  const items = filteredReports();
  const allItems = normalizedReports();
  // Counted view band (zeros included) + one-line footer.
  const searched = filterReportItems(normalizedReports(), { search: state.search, kind: 'all', status: state.status });
  syncGrammarSurfaces(allItems, searched);
  const well = state.ctx.host.querySelector('.reports-well');
  const savedScrollTop = well ? well.scrollTop : 0;
  list.classList.toggle('is-list-view', state.viewMode === 'list');
  if (!items.length) {
    list.innerHTML = renderListEmptyState(allItems);
    if (resetScroll && well) requestAnimationFrame(() => { well.scrollTop = 0; });
    else if (well) well.scrollTop = savedScrollTop;
    return;
  }
  if (state.viewMode === 'list') {
    list.innerHTML = items.map((report) => `
      <button type="button" class="ctox-list-item report-row-compact ${report.id === state.selectedId ? 'is-selected' : ''}" data-report-id="${escapeAttr(report.id)}" data-context-record-id="${escapeAttr(report.id)}" data-context-record-type="business_report" data-context-label="${escapeAttr(report.title || report.id)}" aria-selected="${report.id === state.selectedId ? 'true' : 'false'}">
        <span class="reports-compact-title">${escapeHtml(report.title)}</span>
        <span class="ctox-badge${statusBadgeClass(report.status)}">${escapeHtml(displayStatus(report.status))}</span>
      </button>
    `).join('');
  } else {
    list.innerHTML = items.map((report) => `
    <button type="button" class="ctox-list-item report-row ${report.id === state.selectedId ? 'is-selected' : ''}" data-report-id="${escapeAttr(report.id)}" data-context-record-id="${escapeAttr(report.id)}" data-context-record-type="business_report" data-context-label="${escapeAttr(report.title || report.id)}" aria-selected="${report.id === state.selectedId ? 'true' : 'false'}">
      <span class="reports-badges">
        <span class="ctox-badge ${report.kind === 'bug' ? 'is-danger' : 'is-feature'}">${escapeHtml(report.kindLabel)}</span>
        <span class="ctox-badge${statusBadgeClass(report.status)}">${escapeHtml(displayStatus(report.status))}</span>
        <span class="ctox-badge">${escapeHtml(report.severity || 'medium')}</span>
      </span>
      <strong>${escapeHtml(report.title)}</strong>
      <small>${escapeHtml(report.moduleId)} · ${escapeHtml(formatDate(report.updatedAt || report.createdAt))}</small>
    </button>
  `).join('');
  }
  if (resetScroll && well) requestAnimationFrame(() => { well.scrollTop = 0; });
  else if (well) well.scrollTop = savedScrollTop;
  // Click handling is wired once via event delegation in wireUi(); re-binding
  // per-button on every renderList() is no longer needed.
}

function renderDetailFooter(report) {
  const node = state.ctx.host.querySelector('.reports-detail [data-pg-footer]');
  if (!node) return;
  if (!report) { node.textContent = ''; return; }
  const { approval, command } = delegationInfoFor(report);
  const delegated = command ? `Coding Agent: ${displayStatus(String(command.status || 'running'))}`
    : approval ? `Freigabe: ${String(approval.status || 'pending')}` : '';
  node.textContent = [report.moduleId, displayStatus(report.status), delegated].filter(Boolean).join(' · ');
}

function renderDetail() {
  const detail = state.ctx.host?.querySelector('[data-reports-detail]');
  if (!detail) {
    // Container not mounted yet (or mount() was torn down) — bail gracefully
    // rather than throwing into nothing. A later refreshReports()/render()
    // will pick the report up once the DOM exists.
    return;
  }
  rememberDetailScroll();
  const normalized = normalizedReports();
  const filtered = filteredReports();
  const report = filtered.find((item) => item.id === state.selectedId) || null;
  renderDetailFooter(report);
  const kindLabelNode = detail.querySelector('[data-report-kind-label]');
  const titleNode = detail.querySelector('[data-report-title]');
  if (!report) {
    state.renderedDetailId = '';
    if (kindLabelNode) kindLabelNode.textContent = '';
    if (titleNode) titleNode.textContent = state.t('selectReportTitle', 'Eintrag auswählen');
    let scroller = detail.querySelector('[data-reports-detail-scroll]');
    if (!scroller) {
      scroller = document.createElement('div');
      scroller.className = 'ctox-pane-scroll reports-detail-scroll';
      scroller.setAttribute('data-reports-detail-scroll', '');
      detail.append(scroller);
    }
    scroller.innerHTML = renderDetailEmptyState({ normalized, filtered });
    return;
  }
  const previousRenderedId = state.renderedDetailId;
  state.renderedDetailId = report.id;
  if (kindLabelNode) kindLabelNode.textContent = `${report.kindLabel} · ${displayStatus(report.status)}`;
  if (titleNode) titleNode.textContent = report.title;
  const attachment = report.attachment;
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
          ${fact(state.t('module', 'Modul'), report.moduleId)}
          ${fact(state.t('severity', 'Priorität'), report.severity || 'medium')}
          ${fact(state.t('command', 'Command'), report.commandId || state.t('notCreated', 'nicht angelegt'))}
          ${fact(state.t('task', 'Task'), report.taskId || state.t('notCreated', 'nicht angelegt'))}
          ${fact(state.t('created', 'Angelegt'), formatDate(report.createdAt))}
          ${fact(state.t('updated', 'Aktualisiert'), formatDate(report.updatedAt))}
        </dl>
      </div>
    </section>
    <section class="ctox-card">
      <header>${escapeHtml(state.t('description', 'Beschreibung'))}</header>
      <div class="ctox-card-body">
        <p>${escapeHtml(report.summary || state.t('noDescription', 'Keine Beschreibung hinterlegt.'))}</p>
      </div>
    </section>
    <section class="ctox-card">
      <header>${escapeHtml(state.t('expectation', 'Erwartung'))}</header>
      <div class="ctox-card-body">
        <p>${escapeHtml(report.expected || state.t('noExpectation', 'Keine Erwartung hinterlegt.'))}</p>
      </div>
    </section>
    <section class="ctox-card">
      <header>${escapeHtml(state.t('whatCtoxChanged', 'Was CTOX geändert hat'))}</header>
      <div class="ctox-card-body">
        <p>${escapeHtml(report.changeSummary || changeFallback(report))}</p>
      </div>
    </section>
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
  const actions = state.ctx.host?.querySelector('[data-reports-actions]');
  if (!actions) return;
  const filtered = filteredReports();
  const report = filtered.find((item) => item.id === state.selectedId) || null;
  if (!report) {
    actions.innerHTML = `
      <div class="ctox-pane-scroll reports-actions-scroll">
        <div class="ctox-empty">
          <span>${escapeHtml(state.t('selectReport', 'Wähle links einen Bug oder Feature-Wunsch aus.'))}</span>
        </div>
      </div>
    `;
    return;
  }
  const releases = releasesForModule(report.moduleId);
  const hasCtoxTask = Boolean(report.commandId || report.taskId);
  const delegation = delegationInfoFor(report);
  const delegationOpen = Boolean(delegation.approval && String(delegation.approval.status || 'pending') === 'pending') || Boolean(delegation.command && !['completed', 'failed'].includes(String(delegation.command.status || '')));
  actions.innerHTML = `
    <div class="ctox-pane-scroll reports-actions-scroll">
      <section class="ctox-card">
        <header>${escapeHtml(state.t('actionsTitle', 'Aktionen'))}</header>
        <div class="ctox-card-body">
          <button type="button" class="ctox-button is-primary reports-actions-task" data-focus-task ${hasCtoxTask ? '' : 'disabled'}>
            ${escapeHtml(state.t('showCtoxTask', 'CTOX Task zeigen'))}
          </button>
        </div>
      </section>
      <section class="ctox-card">
        <header>${escapeHtml(state.t('codingAgent', 'Coding Agent'))}</header>
        <div class="ctox-card-body">
          <button type="button" class="ctox-button is-primary" data-delegate-coding ${delegationOpen ? 'disabled' : ''}>
            ${escapeHtml(state.t('delegateCoding', 'An Coding Agent übergeben'))}
          </button>
          ${delegationStatusHtml(report) || `<p class="reports-delegation-status reports-delegation-empty">${escapeHtml(state.t('delegationNone', 'Noch nicht delegiert. Die Übergabe läuft als Freigabe über Threads.'))}</p>`}
        </div>
      </section>
      <section class="ctox-card">
        <header>${escapeHtml(state.t('rollback', 'Rollback'))}</header>
        <div class="ctox-card-body">
          <div class="reports-rollback">
            <p class="reports-rollback-prompt">${escapeHtml(releases.length ? state.t('rollbackPrompt', 'Wähle eine gespeicherte Modulversion und rolle das betroffene Modul zurück.') : state.t('noReleaseFound', 'Für dieses Modul gibt es noch keine gespeicherte Version.'))}</p>
            <select class="ctox-select" data-rollback-version ${releases.length ? '' : 'disabled'}>
              ${releases.map((release) => `<option value="${escapeAttr(release.versionId)}">v${escapeHtml(release.version)} · ${escapeHtml(release.status || '')} · ${escapeHtml(formatDate(release.createdAt))}</option>`).join('')}
            </select>
            <button type="button" class="ctox-button is-primary reports-rollback-action" data-rollback-module ${releases.length ? '' : 'disabled'}>${escapeHtml(state.t('rollback', 'Rollback'))}</button>
            <small class="reports-rollback-status" data-rollback-status></small>
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
    const status = task.route_status || task.status || report.status || bug.status || command.status || 'open';
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

function createDiagnosticsState() {
  return {
    loading: false,
    lastAttemptAt: 0,
    lastSuccessAt: 0,
    lastManual: false,
    reportCount: 0,
    collections: Object.fromEntries(REPORT_COLLECTIONS.map((name) => [name, {
      name,
      count: 0,
      missing: false,
      error: '',
      syncStatus: '',
      syncError: '',
    }])),
    syncErrors: [],
  };
}

function buildDiagnosticsState({ results, syncErrors, syncDiagnostics, reportCount, manual }) {
  const collections = {};
  for (const result of results) {
    const syncInfo = syncDiagnostics?.collections?.[result.name] || {};
    collections[result.name] = {
      name: result.name,
      count: result.items.length,
      missing: Boolean(result.missing),
      error: result.error || '',
      syncStatus: syncInfo.connectionStatus || syncInfo.status || '',
      syncError: safeErrorMessage(syncInfo.lastError || ''),
    };
  }
  return {
    loading: false,
    lastAttemptAt: Date.now(),
    lastSuccessAt: Date.now(),
    lastManual: manual,
    reportCount,
    collections,
    syncErrors,
  };
}

function hasBlockingReportDiagnostic() {
  const dataCollections = REPORT_DATA_COLLECTIONS.map((name) => state.diagnostics.collections?.[name]).filter(Boolean);
  if (!dataCollections.length) return true;
  const allStoresUnavailable = dataCollections.every((info) => info.missing || info.error);
  const anyDataSyncIssue = dataCollections.some((info) => info.missing || info.error || info.syncError || isUnavailableReportSyncStatus(info.syncStatus));
  return allStoresUnavailable || ((state.diagnostics.reportCount || 0) === 0 && anyDataSyncIssue);
}

function isUnavailableReportSyncStatus(value) {
  return isPendingReportSyncStatus(value) || isUnhealthySyncStatus(value);
}

export function isPendingReportSyncStatus(value) {
  return ['connecting', 'initializing', 'loading', 'pending', 'reconnecting', 'starting', 'syncing', 'waiting']
    .includes(String(value || '').toLowerCase());
}

function isUnhealthySyncStatus(value) {
  return ['failed', 'error', 'stopped'].includes(String(value || '').toLowerCase());
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

function focusCtoxTask(report) {
  window.dispatchEvent(new CustomEvent('ctox-business-os-focus-task', {
    detail: {
      taskId: report.taskId,
      commandId: report.commandId,
      taskStatus: report.status,
      sourceModule: report.moduleId,
    },
  }));
  location.hash = '#ctox';
}

async function rollbackSelectedRelease(report) {
  const actions = state.ctx.host.querySelector('[data-reports-actions]');
  if (!actions) return;
  const status = actions.querySelector('[data-rollback-status]');
  const versionId = actions.querySelector('[data-rollback-version]')?.value || report.rollbackVersionId || '';
  if (!versionId) return;
  if (status) status.textContent = state.t('rollbackRunning', 'Rollback läuft...');
  try {
    await dispatchModuleCommand({
      commandType: 'ctox.module.rollback',
      moduleId: report.moduleId,
      recordId: versionId,
      payload: { module_id: report.moduleId, version_id: versionId },
      source: 'business-os-reports',
    });
    if (status) status.textContent = state.t('rollbackExecuted', 'Rollback ausgeführt.');
    await refreshReports();
  } catch (error) {
    if (status) status.textContent = error.message || String(error);
  }
}

async function dispatchModuleCommand({
  commandType,
  moduleId,
  recordId,
  payload,
  source,
  module = 'ctox',
}) {
  if (!state.ctx.commandBus?.dispatch || !state.ctx.db?.collection?.('business_commands')) {
    throw new Error(state.t('commandsUnavailable', 'Aktionen sind gerade nicht verfügbar.'));
  }
  await Promise.all([
    state.ctx.sync?.startCollection?.('business_commands'),
    state.ctx.sync?.startCollection?.('business_module_releases'),
  ]);
  const commandId = `cmd_${newId()}`;
  return state.ctx.commandBus.dispatch({
    id: commandId,
    module,
    type: commandType,
    record_id: recordId || moduleId,
    inbound_channel: moduleId,
    payload,
    client_context: {
      source,
      module_id: moduleId,
      actor: actorContext(state.ctx.session),
    },
  }, { until: 'accepted' });
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

function changeFallback(report) {
  if (report.commandId || report.taskId) {
    return state.t('reportAccepted', 'CTOX hat den Report angenommen. Command {0}, Task {1}. Sobald der Lauf eine Change Summary schreibt, erscheint sie hier.', report.commandId || '-', report.taskId || '-');
  }
  return state.t('noChangeSummary', 'Noch keine CTOX-Annahme oder Change Summary vorhanden.');
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

function actorContext(session) {
  const user = session?.user || {};
  return {
    id: user.id || '',
    display_name: user.display_name || user.name || user.id || '',
    role: user.role || 'user',
    is_admin: Boolean(user.is_admin),
  };
}

function newId() {
  return globalThis.crypto?.randomUUID?.() || `${Date.now()}_${Math.random().toString(36).slice(2)}`;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function withTimeout(promise, timeoutMs, message) {
  if (!promise || typeof promise.then !== 'function') return promise;
  promise.catch(() => {});
  return Promise.race([
    promise,
    delay(timeoutMs).then(() => { throw new Error(message); }),
  ]);
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

function reportStoreEmptyMessage(prefix = '') {
  const reportInfo = state.diagnostics.collections?.business_module_reports;
  const bugInfo = state.diagnostics.collections?.ctox_bug_reports;
  const count = [reportInfo, bugInfo]
    .filter(Boolean)
    .reduce((sum, info) => sum + Number(info.count || 0), 0);
  const base = prefix || state.t('emptyStoreSummary', 'Noch keine Bugs oder Features verfügbar.');
  if (!count) return base;
  return `${base} ${state.t('emptyStoreCounts', 'Einträge')}: ${count}.`;
}

function initReportsContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu reports-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== 'reports') return;
    const context = reportsCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderReportsContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideReportsContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideReportsContextMenu(state);
  };

  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideReportsContextMenu(state);
    state.contextMenu?.remove();
    state.contextMenu = null;
  };
}

function hideReportsContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifyReportsApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function reportsCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  const visibleReports = filteredReports();
  const allReports = normalizedReports();
  const clickedRow = element?.closest?.('[data-report-id]');
  const clickedReportId = clickedRow?.getAttribute?.('data-report-id') || '';
  const activeReport = resolveReportsContextRecord({
    clickedReportId,
    selectedId: state.selectedId,
    visibleReports,
    allReports,
  });
  const column = reportsColumnFromElement(element, clickedRow);

  return {
    module: 'reports',
    column,
    record_type: activeReport ? 'report' : 'module',
    record_id: activeReport?.id || '',
    label: activeReport?.title || '',
    body_snippet: activeReport?.summary?.slice(0, 500) || '',
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
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

function reportsColumnFromElement(element, clickedRow) {
  if (!element) return 'module';
  // The detail pane header now uses the shared .ctox-pane-header markup too,
  // so the detail check must run before the header/rail heuristics.
  if (!clickedRow && element.closest?.('[data-reports-detail]')) return 'detail';
  if (clickedRow || element.closest?.('[data-reports-list], .reports-rail, .ctox-pane-tools, .ctox-pane-header')) return 'list';
  return 'module';
}

function renderReportsContextMenu(state, context, x, y) {
  const canModifyApp = canModifyReportsApp(state);
  const reportsLabel = state.t('reportsLabel', 'Reports');
  state.contextMenu.innerHTML = `
    <form class="reports-context-chat" data-reports-context-chat-form>
      <header>
        <div>
          <strong>${escapeHtml(state.t('chatToCtox', 'Chat to CTOX'))}</strong>
          <span>${escapeHtml(context.label || reportsLabel)}</span>
        </div>
        <button type="button" data-reports-context-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
      </header>
      <div class="ctox-context-mode" role="radiogroup" aria-label="${escapeHtml(state.t('chatActionLabel', 'CTOX Aufgabe'))}">
        <label><input type="radio" name="contextMode" value="data" checked /> ${escapeHtml(state.t('chatWorkDataLabel', 'Mit Daten arbeiten'))}</label>
        <label><input type="radio" name="contextMode" value="ask" /> ${escapeHtml(state.t('chatAnswerLabel', 'Frage beantworten'))}</label>
        ${canModifyApp ? `<label><input type="radio" name="contextMode" value="app" /> ${escapeHtml(state.t('chatModifyAppLabel', 'App modifizieren'))}</label>` : ''}
      </div>
      <textarea data-reports-context-message placeholder="${escapeHtml(state.t('chatPlaceholder', 'Was soll CTOX hier tun oder prüfen?'))}"></textarea>
      <footer>
        <span data-reports-context-status></span>
        <button type="submit">${escapeHtml(state.t('send', 'Senden'))}</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const clampNumber = (val, min, max) => Math.min(max, Math.max(min, val));
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(x, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(y, 8, maxTop)}px`;

  const form = state.contextMenu.querySelector('[data-reports-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-reports-context-message]');
  state.contextMenu.querySelector('[data-reports-context-close]')?.addEventListener('click', () => hideReportsContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = new FormData(form).get('contextMode') || 'data';
    await dispatchReportsContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchReportsContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-reports-context-status]');
  if (!trimmed) {
    if (status) status.textContent = state.t('chatMissingMessage', 'Nachricht fehlt.');
    return;
  }

  const safeMode = mode === 'app' && canModifyReportsApp(state) ? 'app' : (mode === 'ask' ? 'ask' : 'data');
  const visibleReports = filteredReports();
  const allReports = normalizedReports();
  const contextReportId = context.record_id || '';
  const activeReport = resolveReportsContextRecord({
    clickedReportId: contextReportId,
    selectedId: state.selectedId,
    visibleReports,
    allReports,
  });
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = state.t('chatNotReady', 'Chat ist noch nicht bereit.');
    return;
  }
  if (status) status.textContent = state.t('chatOpening', 'Öffne Chat...');
  const reportsLabel = state.t('reportsLabel', 'Reports');
  const titlePrefix = safeMode === 'app'
    ? state.t('modifyReportsApp', 'Reports App modifizieren')
    : safeMode === 'ask'
      ? state.t('chatAnswerLabel', 'Frage beantworten')
      : state.t('editReport', 'Report bearbeiten');
  const title = `${titlePrefix} · ${context.label || reportsLabel}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die Reports-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Reportdaten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : safeMode === 'ask'
      ? `Beantworte die folgende Frage ausschließlich lesend. Nutze nur vorhandene Daten und Kontext; führe keine Änderungen an Daten, Records, Dateien oder der App aus. Antworte knapp und direkt.\n\n${trimmed}`
      : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'reports',
      source_title: reportsLabel,
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'reports' : (activeReport?.id || 'reports'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        prompt: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : (safeMode === 'ask' ? 'read' : 'data'),
        selected_report: activeReport,
        context,
        thread_key: 'business-os/reports',
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        report_id: activeReport?.id || '',
      },
    },
  }));
  hideReportsContextMenu(state);
}
