import { loadModuleMessages } from '../../shared/i18n.js';

const REPORTS_REFRESH_DEBOUNCE_MS = 80;
const REPORTS_SYNC_RESTART_TIMEOUT_MS = 6000;
const REPORT_COLLECTIONS = [
  'business_module_reports',
  'ctox_bug_reports',
  'business_module_releases',
  'business_commands',
  'ctox_queue_tasks',
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

  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;

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
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.reportsStyle = 'true';
  document.head.append(link);
}

function applyStaticLabels(host, t) {
  const root = host.querySelector('[data-reports-root]');
  if (!root) return;

  // Refresh button keeps the static SVG markup in index.html — only the
  // tooltip / aria-label are translated.
  const refreshBtn = root.querySelector('[data-refresh-reports]');
  if (refreshBtn) {
    const label = t('refresh', 'Aktualisieren');
    refreshBtn.title = label;
    refreshBtn.setAttribute('aria-label', label);
  }

  // Search input placeholder
  const searchInput = root.querySelector('[data-report-search]');
  if (searchInput) {
    searchInput.placeholder = t('searchPlaceholder', 'Suchen...');
    searchInput.setAttribute('aria-label', t('searchLabel', 'Bugs und Features suchen'));
  }

  // Status select options (kind is now chips in markup; status stays as the
  // single primary filter, with values translatable so en/de stay readable).
  const statusSelect = root.querySelector('[data-report-status]');
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
  root.querySelector('[data-refresh-reports]')?.addEventListener('click', () => refreshReports({ restartSync: true, manual: true }));
  root.querySelector('[data-report-search]')?.addEventListener('input', (event) => {
    state.search = event.target.value || '';
    syncSelectionToVisibleItems();
    render();
  });
  root.querySelector('[data-report-status]')?.addEventListener('change', (event) => {
    state.status = event.target.value || 'all';
    syncSelectionToVisibleItems();
    render();
  });
  // Kind chips live in static markup; toggle the .is-active state instead of
  // rewriting the strip on every render so the focus ring survives.
  root.querySelectorAll('[data-report-kind-chips] [data-report-kind]').forEach((chip) => {
    chip.addEventListener('click', () => {
      const next = chip.getAttribute('data-report-kind') || 'all';
      if (next === state.kind) return;
      state.kind = next;
      root.querySelectorAll('[data-report-kind-chips] [data-report-kind]').forEach((node) => {
        node.classList.toggle('is-active', node === chip);
      });
      syncSelectionToVisibleItems();
      render();
    });
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
    if (target?.closest('[data-rollback-module]')) {
      const report = currentReportForActions();
      if (report) rollbackSelectedRelease(report);
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
    render();
  });
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
  renderDiagnostics();

  const syncErrors = options.restartSync ? await restartReportSync() : [];
  const results = await Promise.all(REPORT_COLLECTIONS.map((name) => loadCollectionResult(name)));
  const byName = Object.fromEntries(results.map((result) => [result.name, result]));
  const reports = byName.business_module_reports?.items || [];
  const bugs = byName.ctox_bug_reports?.items || [];
  const releases = byName.business_module_releases?.items || [];
  const commands = byName.business_commands?.items || [];
  const queue = byName.ctox_queue_tasks?.items || [];
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
    renderDiagnostics();
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
  ].join('\n');
}

function render() {
  renderList();
  renderDetail();
  renderActions();
  renderDiagnostics();
}

function renderDiagnostics() {
  const root = state.ctx?.host?.querySelector?.('[data-reports-root]');
  if (!root) return;
  const refreshBtn = root.querySelector('[data-refresh-reports]');
  if (refreshBtn) {
    refreshBtn.disabled = Boolean(state.diagnostics.loading);
    refreshBtn.setAttribute('aria-busy', state.diagnostics.loading ? 'true' : 'false');
    refreshBtn.title = refreshDiagnosticTitle(state.diagnostics);
  }
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

function renderList() {
  const list = state.ctx.host.querySelector('[data-reports-list]');
  if (!list) return;
  const items = filteredReports();
  const allItems = normalizedReports();
  if (!items.length) {
    list.innerHTML = renderListEmptyState(allItems);
    return;
  }
  list.innerHTML = items.map((report) => `
    <button type="button" class="ctox-list-item report-row ${report.id === state.selectedId ? 'is-selected' : ''}" data-report-id="${escapeAttr(report.id)}" data-context-record-id="${escapeAttr(report.id)}" data-context-record-type="business_report" data-context-label="${escapeAttr(report.title || report.id)}">
      <span class="reports-badges">
        <span class="ctox-badge ${report.kind === 'bug' ? 'is-danger' : 'is-feature'}">${escapeHtml(report.kindLabel)}</span>
        <span class="ctox-badge${statusBadgeClass(report.status)}">${escapeHtml(displayStatus(report.status))}</span>
        <span class="ctox-badge">${escapeHtml(report.severity || 'medium')}</span>
      </span>
      <strong>${escapeHtml(report.title)}</strong>
      <small>${escapeHtml(report.moduleId)} · ${escapeHtml(formatDate(report.updatedAt || report.createdAt))}</small>
    </button>
  `).join('');
  // Click handling is wired once via event delegation in wireUi(); re-binding
  // per-button on every renderList() is no longer needed.
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

function diagnosticsMessages(diagnostics, t = defaultTranslate) {
  if (diagnostics.loading) return [t('refreshRunning', 'Bugs & Features werden aktualisiert...')];
  const messages = [];
  for (const name of REPORT_DATA_COLLECTIONS) {
    const info = diagnostics.collections?.[name];
    if (!info) continue;
    if (info.missing) messages.push(t('reportsUnavailableDetail', 'Die Liste wird automatisch gefüllt, sobald Einträge geladen sind.'));
    if (info.error) messages.push(t('reportsUnavailableDetail', 'Die Liste wird automatisch gefüllt, sobald Einträge geladen sind.'));
    if (isUnavailableReportSyncStatus(info.syncStatus) || info.syncError) {
      messages.push(t('reportsUnavailableDetail', 'Die Liste wird automatisch gefüllt, sobald Einträge geladen sind.'));
    }
  }
  for (const error of diagnostics.syncErrors || []) {
    if (error) messages.push(t('reportsUnavailableDetail', 'Die Liste wird automatisch gefüllt, sobald Einträge geladen sind.'));
  }
  return [...new Set(messages)];
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

function refreshDiagnosticTitle(diagnostics) {
  if (diagnostics.loading) return state.t('refreshRunning', 'Bugs & Features werden aktualisiert...');
  const messages = diagnosticsMessages(diagnostics, state.t);
  if (messages.length) return messages.join(' ');
  if (diagnostics.lastSuccessAt) {
    return state.t('refreshTitleOk', 'Zuletzt aktualisiert: {0}', formatDate(diagnostics.lastSuccessAt));
  }
  return state.t('refresh', 'Aktualisieren');
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
    module: 'ctox',
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
