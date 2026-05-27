import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';

const REPORTS_REFRESH_DEBOUNCE_MS = 80;

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
  await refreshReports();
  state.cleanup = wireRealtime();

  const resizeCleanup = setupResizers(ctx.host);

  window.addEventListener('ctox-business-os-reports-updated', handleReportsUpdated);
  state.contextMenuCleanup = initReportsContextMenu(state);
  return () => {
    state.cleanup?.();
    state.contextMenuCleanup?.();
    state.contextMenu?.remove();
    state.contextMenu = null;
    resizeCleanup?.();
    window.removeEventListener('ctox-business-os-reports-updated', handleReportsUpdated);
    if (state.renderTimer) window.clearTimeout(state.renderTimer);
  };
}

function setupResizers(host) {
  const leftResizer = host.querySelector('[data-resizer="left"]');
  const containerEl = host.querySelector('[data-reports-root]') || host;

  const cleanups = [];

  if (leftResizer) {
    const resizerL = new CtoxResizer({
      resizerEl: leftResizer,
      containerEl,
      cssVar: '--reports-left-width',
      side: 'left',
      minWidth: 260,
      maxWidth: 500,
      onResize: (width) => localStorage.setItem('ctox.reports.layout.leftWidth', width)
    });
    cleanups.push(() => resizerL.destroy());
  }

  // Set initial width from localStorage
  const leftWidth = localStorage.getItem('ctox.reports.layout.leftWidth') || '320';
  containerEl.style.setProperty('--reports-left-width', `${leftWidth}px`);

  return () => {
    cleanups.forEach(c => c());
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

  // Refresh button
  const refreshBtn = root.querySelector('[data-refresh-reports]');
  if (refreshBtn) refreshBtn.textContent = t('refresh', 'Aktualisieren');

  // Search input placeholder
  const searchInput = root.querySelector('[data-report-search]');
  if (searchInput) searchInput.placeholder = t('searchPlaceholder', 'Suchen...');

  // Kind select options
  const kindSelect = root.querySelector('[data-report-kind]');
  if (kindSelect) {
    kindSelect.innerHTML = `
      <option value="all">${escapeHtml(t('allTypes', 'Alle Typen'))}</option>
      <option value="bug">${escapeHtml(t('bugs', 'Bugs'))}</option>
      <option value="feature">${escapeHtml(t('features', 'Features'))}</option>
    `;
  }

  // Status select options
  const statusSelect = root.querySelector('[data-report-status]');
  if (statusSelect) {
    statusSelect.innerHTML = `
      <option value="all">${escapeHtml(t('allStatus', 'Alle Status'))}</option>
      <option value="open">${escapeHtml(t('open', 'Offen'))}</option>
      <option value="running">${escapeHtml(t('running', 'In Arbeit'))}</option>
      <option value="completed">${escapeHtml(t('completed', 'Erledigt'))}</option>
      <option value="blocked">${escapeHtml(t('blocked', 'Blockiert'))}</option>
    `;
  }
}

function wireUi() {
  const root = state.ctx.host.querySelector('[data-reports-root]');
  if (!root) return;
  root.querySelector('[data-refresh-reports]')?.addEventListener('click', () => refreshReports());
  root.querySelector('[data-report-search]')?.addEventListener('input', (event) => {
    state.search = event.target.value || '';
    render();
  });
  root.querySelector('[data-report-kind]')?.addEventListener('change', (event) => {
    state.kind = event.target.value || 'all';
    render();
  });
  root.querySelector('[data-report-status]')?.addEventListener('change', (event) => {
    state.status = event.target.value || 'all';
    render();
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
      return;
    }
    state.selectedId = reportId;
    render();
  });
}

function wireRealtime() {
  const names = ['business_module_reports', 'ctox_bug_reports', 'business_module_releases', 'business_commands', 'ctox_queue_tasks'];
  const subscriptions = names.map((name) => state.ctx.db?.raw?.[name]?.$?.subscribe?.(() => scheduleRefresh())).filter(Boolean);
  return () => subscriptions.forEach((sub) => {
    try { sub.unsubscribe?.(); } catch {}
  });
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

async function refreshReports() {
  const previousSelectedId = state.selectedId;
  const [reports, bugs, releases, commands, queue] = await Promise.all([
    loadCollection('business_module_reports'),
    loadCollection('ctox_bug_reports'),
    loadCollection('business_module_releases'),
    loadCollection('business_commands'),
    loadCollection('ctox_queue_tasks'),
  ]);
  const nextRenderKey = buildRenderKey({ reports, bugs, releases, commands, queue });
  const hadSameData = nextRenderKey === state.renderKey;
  state.reports = reports;
  state.bugs = bugs;
  state.releases = releases;
  state.commands = commands;
  state.queue = queue;
  state.renderKey = nextRenderKey;
  const items = filteredReports();
  if (!state.selectedId || !items.some((item) => item.id === state.selectedId)) {
    state.selectedId = items[0]?.id || '';
  }
  if (hadSameData && previousSelectedId === state.selectedId) return;
  render();
}

async function loadCollection(name) {
  const collection = state.ctx.db?.raw?.[name];
  if (!collection) return [];
  const docs = await collection.find().exec();
  return docs.map((doc) => doc.toJSON());
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
  if (!items.length) {
    list.innerHTML = `<p class="reports-empty">${escapeHtml(state.t('noReports', 'Keine Reports gefunden.'))}</p>`;
    return;
  }
  list.innerHTML = items.map((report) => `
    <button type="button" class="report-row ${report.id === state.selectedId ? 'is-selected' : ''}" data-report-id="${escapeAttr(report.id)}">
      <div class="reports-badges">
        <span class="reports-badge ${report.kind === 'bug' ? 'is-bug' : 'is-feature'}">${escapeHtml(report.kindLabel)}</span>
        <span class="reports-badge">${escapeHtml(displayStatus(report.status))}</span>
        <span class="reports-badge">${escapeHtml(report.severity || 'medium')}</span>
      </div>
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
  // Look the selected report up against the unfiltered list as well — if the
  // user clicked a row while a filter was being narrowed, the selection must
  // still resolve to the clicked entry instead of silently falling back to
  // the first visible row (which left the panel looking blank/stale).
  const normalized = normalizedReports();
  const filtered = filteredReports();
  const report = filtered.find((item) => item.id === state.selectedId)
    || normalized.find((item) => item.id === state.selectedId)
    || filtered[0]
    || normalized[0]
    || null;
  if (!report) {
    state.renderedDetailId = '';
    detail.innerHTML = `<p class="reports-empty">${escapeHtml(state.t('selectReport', 'Wähle links einen Report aus.'))}</p>`;
    return;
  }
  const previousRenderedId = state.renderedDetailId;
  state.renderedDetailId = report.id;
  const releases = releasesForModule(report.moduleId);
  const attachment = report.attachment;
  detail.innerHTML = `
    <header class="reports-detail-head">
      <div>
        <span>${escapeHtml(report.kindLabel)} · ${escapeHtml(displayStatus(report.status))}</span>
        <h1>${escapeHtml(report.title)}</h1>
      </div>
      <button type="button" class="os-btn" data-focus-task>${escapeHtml(state.t('showCtoxTask', 'CTOX Task zeigen'))}</button>
    </header>
    <div class="reports-detail-scroll os-scrollbar" data-reports-detail-scroll>
      <section class="reports-section">
        <h3>${escapeHtml(state.t('report', 'Report'))}</h3>
        <div class="reports-facts">
          ${fact(state.t('module', 'Modul'), report.moduleId)}
          ${fact(state.t('severity', 'Priorität'), report.severity || 'medium')}
          ${fact(state.t('command', 'Command'), report.commandId || state.t('notCreated', 'nicht angelegt'))}
          ${fact(state.t('task', 'Task'), report.taskId || state.t('notCreated', 'nicht angelegt'))}
          ${fact(state.t('created', 'Angelegt'), formatDate(report.createdAt))}
          ${fact(state.t('updated', 'Aktualisiert'), formatDate(report.updatedAt))}
        </div>
      </section>
      <section class="reports-section">
        <h3>${escapeHtml(state.t('description', 'Beschreibung'))}</h3>
        <p>${escapeHtml(report.summary || state.t('noDescription', 'Keine Beschreibung hinterlegt.'))}</p>
      </section>
      <section class="reports-section">
        <h3>${escapeHtml(state.t('expectation', 'Erwartung'))}</h3>
        <p>${escapeHtml(report.expected || state.t('noExpectation', 'Keine Erwartung hinterlegt.'))}</p>
      </section>
      <section class="reports-section">
        <h3>${escapeHtml(state.t('whatCtoxChanged', 'Was CTOX geändert hat'))}</h3>
        <p>${escapeHtml(report.changeSummary || changeFallback(report))}</p>
      </section>
      ${attachment ? `
        <section class="reports-section">
          <h3>${escapeHtml(state.t('screenshotAndMarkup', 'Screenshot und Markup'))}</h3>
          <div class="reports-attachment">
            <span class="reports-attachment-meta">${escapeHtml(attachment.capture_mode || 'capture')}</span>
            <img src="${escapeAttr(attachment.data_url)}" alt="Report Screenshot" />
          </div>
        </section>
      ` : ''}
      <section class="reports-section">
        <h3>${escapeHtml(state.t('rollback', 'Rollback'))}</h3>
        <div class="reports-rollback">
          <p>${escapeHtml(releases.length ? state.t('rollbackPrompt', 'Wähle eine gespeicherte Modulversion und rolle das betroffene Modul zurück.') : state.t('noReleaseFound', 'Für dieses Modul gibt es noch keine gespeicherte Version.'))}</p>
          <div class="reports-rollback-actions">
            <select class="os-select" data-rollback-version ${releases.length ? '' : 'disabled'}>
              ${releases.map((release) => `<option value="${escapeAttr(release.versionId)}">v${escapeHtml(release.version)} · ${escapeHtml(release.status || '')} · ${escapeHtml(formatDate(release.createdAt))}</option>`).join('')}
            </select>
            <button type="button" class="os-btn os-btn-primary" data-rollback-module ${releases.length ? '' : 'disabled'}>${escapeHtml(state.t('rollback', 'Rollback'))}</button>
          </div>
          <small data-rollback-status></small>
        </div>
      </section>
    </div>
  `;
  detail.querySelector('[data-focus-task]')?.addEventListener('click', () => focusCtoxTask(report));
  detail.querySelector('[data-rollback-module]')?.addEventListener('click', () => rollbackSelectedRelease(report));
  const scroller = detail.querySelector('[data-reports-detail-scroll]');
  if (scroller) {
    const shouldRestore = previousRenderedId === report.id || Object.prototype.hasOwnProperty.call(state.detailScrollByReport, report.id);
    restoreDetailScroll(scroller, report.id, shouldRestore);
    scroller.addEventListener('scroll', () => {
      state.detailScrollByReport[report.id] = scroller.scrollTop;
    }, { passive: true });
  }
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
  const query = state.search.trim().toLowerCase();
  return normalizedReports().filter((report) => {
    if (state.kind !== 'all' && report.kind !== state.kind) return false;
    if (state.status !== 'all' && normalizeStatus(report.status) !== state.status) return false;
    if (!query) return true;
    return [report.title, report.summary, report.expected, report.moduleId, report.commandId, report.taskId]
      .some((value) => String(value || '').toLowerCase().includes(query));
  });
}

function normalizedReports() {
  const bugById = new Map(state.bugs.map((bug) => [bug.id || bug.report_id, bug]));
  const commandById = new Map(state.commands.map((command) => [command.command_id || command.id, command]));
  const queueByTaskId = new Map(state.queue.map((task) => [task.id || task.task_id, task]));
  return state.reports.map((report) => {
    const id = report.report_id || report.id;
    const bug = bugById.get(id) || {};
    const payload = objectValue(bug.payload);
    const clientContext = objectValue(report.client_context || bug.evidence);
    const commandId = report.ctox_command_id || payload.ctox_command_id || '';
    const taskId = report.task_id || payload.task_id || '';
    const command = commandById.get(commandId) || {};
    const task = queueByTaskId.get(taskId) || {};
    const status = task.route_status || task.status || report.status || bug.status || command.status || 'open';
    const changeSummary = payload.change_summary
      || payload.ctox_change_summary
      || clientContext.ctox_change_summary
      || task.result?.summary
      || task.result_summary
      || '';
    return {
      id,
      kind: normalizeKind(report.kind || payload.kind),
      kindLabel: normalizeKind(report.kind || payload.kind) === 'bug' ? state.t('bugs', 'Bug') : state.t('features', 'Feature'),
      severity: report.severity || bug.severity || '',
      title: report.title || bug.title || id,
      summary: report.summary || bug.description || '',
      expected: report.expected || payload.expected || '',
      status,
      moduleId: report.module_id || bug.module || 'ctox',
      commandId,
      taskId,
      changeSummary,
      rollbackVersionId: payload.rollback_version_id || clientContext.rollback_version_id || '',
      attachment: objectValue(clientContext.attachment),
      createdAt: report.created_at_ms || bug.created_at_ms || report.updated_at_ms || 0,
      updatedAt: report.updated_at_ms || bug.updated_at_ms || report.created_at_ms || 0,
    };
  }).sort((left, right) => (right.updatedAt || 0) - (left.updatedAt || 0));
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
  const detail = state.ctx.host.querySelector('[data-reports-detail]');
  const status = detail.querySelector('[data-rollback-status]');
  const versionId = detail.querySelector('[data-rollback-version]')?.value || report.rollbackVersionId || '';
  if (!versionId) return;
  status.textContent = state.t('rollbackRunning', 'Rollback läuft...');
  try {
    await dispatchModuleCommand({
      commandType: 'ctox.module.rollback',
      moduleId: report.moduleId,
      recordId: versionId,
      payload: { module_id: report.moduleId, version_id: versionId },
      source: 'business-os-reports',
    });
    status.textContent = state.t('rollbackExecuted', 'Rollback ausgeführt.');
    await refreshReports();
  } catch (error) {
    status.textContent = error.message || String(error);
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
    throw new Error('business_commands collection is required for module governance commands');
  }
  await Promise.all([
    state.ctx.sync?.startCollection?.('business_commands'),
    state.ctx.sync?.startCollection?.('business_module_releases'),
  ]);
  const commandId = `cmd_${newId()}`;
  await state.ctx.commandBus.dispatch({
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
  });
  return waitForCommandProjection(commandId);
}

async function waitForCommandProjection(commandId, timeoutMs = 45000) {
  const collection = state.ctx.db?.collection?.('business_commands');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const doc = await collection?.findOne(commandId).exec();
    const data = doc?.toJSON?.();
    if (data && data.status && data.status !== 'pending_sync') {
      if (data.status === 'failed') throw new Error(data.error || `Command ${commandId} failed`);
      return data;
    }
    await delay(300);
  }
  throw new Error(state.t('commandNotSynced', 'Command {0} wurde nicht synchronisiert.', commandId));
}

function fact(label, value) {
  return `<dl class="reports-fact"><dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value || '-')}</dd></dl>`;
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
  return value && typeof value === 'object' ? value : {};
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

  state.ctx.host.addEventListener('contextmenu', handleContextMenu);
  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    state.ctx.host.removeEventListener('contextmenu', handleContextMenu);
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
  const activeReport = filteredReports().find((item) => item.id === state.selectedId) || normalizedReports()[0] || null;

  return {
    module: 'reports',
    column: state.selectedId ? 'detail' : 'list',
    record_type: activeReport ? 'report' : 'module',
    record_id: activeReport?.id || '',
    label: activeReport?.title || '',
    body_snippet: activeReport?.summary?.slice(0, 500) || '',
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderReportsContextMenu(state, context, x, y) {
  ensureCtoxContextMenuStyles();
  const canModifyApp = canModifyReportsApp(state);
  state.contextMenu.innerHTML = `
    <form class="reports-context-chat" data-reports-context-chat-form>
      <header>
        <div>
          <strong>${escapeHtml(state.t('chatToCtox', 'Chat to CTOX'))}</strong>
          <span>${escapeHtml(context.label || 'Reports')}</span>
        </div>
        <button type="button" data-reports-context-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
      </header>
      ${canModifyApp ? `
        <div class="ctox-context-mode" role="radiogroup" aria-label="${escapeHtml(state.t('chatActionLabel', 'CTOX Aufgabe'))}">
          <label><input type="radio" name="contextMode" value="data" checked /> ${escapeHtml(state.t('chatWorkDataLabel', 'Mit Daten arbeiten'))}</label>
          <label><input type="radio" name="contextMode" value="app" /> ${escapeHtml(state.t('chatModifyAppLabel', 'App modifizieren'))}</label>
        </div>
      ` : ''}
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
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
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

  const safeMode = mode === 'app' && canModifyReportsApp(state) ? 'app' : 'data';
  const activeReport = filteredReports().find((item) => item.id === state.selectedId) || normalizedReports()[0] || null;
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = state.t('chatNotReady', 'Chat ist noch nicht bereit.');
    return;
  }
  if (status) status.textContent = state.t('chatOpening', 'Oeffne Chat...');
  const title = `${safeMode === 'app' ? 'Reports App modifizieren' : 'Report bearbeiten'} · ${context.label || 'Reports'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die Reports-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Reportdaten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'reports',
      source_title: 'Reports',
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
        target: safeMode === 'app' ? 'app' : 'data',
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

function ensureCtoxContextMenuStyles() {
  if (document.getElementById('ctox-unified-context-menu-style')) return;
  const style = document.createElement('style');
  style.id = 'ctox-unified-context-menu-style';
  style.textContent = `
    .ctox-context-menu {
      position: absolute;
      z-index: 2400;
      width: min(560px, calc(100vw - 24px));
      max-width: calc(100% - 16px);
      overflow: hidden;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-panel, 12px);
      background: color-mix(in srgb, var(--bo-surface, var(--surface, #fff)) 75%, transparent);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      box-shadow: 0 18px 50px rgba(0, 0, 0, 0.25);
      padding: 6px;
      font-family: system-ui, -apple-system, sans-serif;
      animation: ctox-menu-fade-in 0.15s ease-out;
    }
    @keyframes ctox-menu-fade-in {
      from { opacity: 0; transform: scale(0.97); }
      to { opacity: 1; transform: scale(1); }
    }
    .ctox-context-menu form {
      display: grid;
      grid-template-columns: minmax(0, 1fr);
      gap: 10px;
      min-width: 0;
      padding: 12px;
      margin: 0;
    }
    .ctox-context-menu form header,
    .ctox-context-menu form footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      min-width: 0;
    }
    .ctox-context-menu .ctox-context-mode {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 6px;
      min-width: 0;
    }
    .ctox-context-menu .ctox-context-mode label {
      display: flex;
      align-items: center;
      gap: 7px;
      min-width: 0;
      min-height: 30px;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      color: var(--bo-muted, var(--muted, #64747c));
      font-size: 11.5px;
      font-weight: 760;
      padding: 0 8px;
      cursor: pointer;
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      margin: 0;
    }
    .ctox-context-menu .ctox-context-mode label:hover {
      border-color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu .ctox-context-mode input {
      margin: 0;
      accent-color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu form header div {
      min-width: 0;
    }
    .ctox-context-menu form strong,
    .ctox-context-menu form span {
      display: block;
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .ctox-context-menu form strong {
      color: var(--bo-text, var(--text, #18222d));
      font-size: 12.5px;
      font-weight: 820;
    }
    .ctox-context-menu form span {
      color: var(--bo-muted, var(--muted, #64747c));
      font-size: 11px;
      font-weight: 700;
    }
    .ctox-context-menu form footer > span {
      display: flex;
      align-items: center;
      gap: 6px;
      flex-wrap: wrap;
      white-space: normal;
      font-size: 11px;
      color: var(--bo-muted, var(--muted, #64747c));
    }
    .ctox-context-menu form textarea {
      width: 100%;
      box-sizing: border-box;
      min-height: 92px;
      max-height: 180px;
      min-width: 0;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      color: var(--bo-text, var(--text, #18222d));
      font: 12.5px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif;
      padding: 9px;
      resize: vertical;
    }
    .ctox-context-menu form textarea:focus {
      outline: none;
      border-color: var(--bo-accent, #23665f);
      box-shadow: 0 0 0 2px color-mix(in srgb, var(--bo-accent, #23665f) 25%, transparent);
    }
    .ctox-context-menu form button {
      flex: 0 0 auto;
      min-height: 30px;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-control, 6px);
      background: var(--bo-surface-muted, var(--surface-2, #eef3f7));
      color: var(--bo-text, var(--text, #18222d));
      font: inherit;
      font-size: 12px;
      font-weight: 760;
      cursor: pointer;
      padding: 0 10px;
    }
    .ctox-context-menu form button:hover {
      background: color-mix(in srgb, var(--bo-text, #18222d) 8%, var(--bo-surface-muted, #eef3f7));
    }
    .ctox-context-menu form button[type="submit"] {
      border-color: var(--bo-accent, #23665f);
      background: color-mix(in srgb, var(--bo-accent, #23665f) 14%, var(--bo-surface, #fff));
      color: var(--bo-accent, #23665f);
    }
    .ctox-context-menu form button[type="submit"]:hover {
      background: color-mix(in srgb, var(--bo-accent, #23665f) 22%, var(--bo-surface, #fff));
    }
    .ctox-context-menu form [data-reports-context-close] {
      width: 30px;
      min-width: 30px;
      padding: 0;
      text-align: center;
      font-size: 18px;
      border: none;
      background: none;
      color: var(--bo-muted, var(--muted, #64747c));
      cursor: pointer;
    }
  `;
  document.head.append(style);
}
