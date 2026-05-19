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
  renderTimer: null,
  renderKey: '',
  renderedDetailId: '',
  detailScrollByReport: {},
};

export async function mount(ctx) {
  state.ctx = ctx;
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();
  wireUi();
  await refreshReports();
  state.cleanup = wireRealtime();
  window.addEventListener('ctox-business-os-reports-updated', handleReportsUpdated);
  return () => {
    state.cleanup?.();
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

function wireUi() {
  const root = state.ctx.host.querySelector('[data-reports-root]');
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
  const items = filteredReports();
  if (!items.length) {
    list.innerHTML = '<p class="reports-empty">Keine Reports gefunden.</p>';
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
  list.querySelectorAll('[data-report-id]').forEach((button) => {
    button.addEventListener('click', () => {
      state.selectedId = button.dataset.reportId || '';
      render();
    });
  });
}

function renderDetail() {
  const detail = state.ctx.host.querySelector('[data-reports-detail]');
  rememberDetailScroll();
  const report = filteredReports().find((item) => item.id === state.selectedId) || normalizedReports()[0];
  if (!report) {
    state.renderedDetailId = '';
    detail.innerHTML = '<p class="reports-empty">Waehle links einen Report aus.</p>';
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
      <button type="button" data-focus-task>CTOX Task zeigen</button>
    </header>
    <div class="reports-detail-scroll" data-reports-detail-scroll>
      <section class="reports-section">
        <h3>Report</h3>
        <div class="reports-facts">
          ${fact('Modul', report.moduleId)}
          ${fact('Command', report.commandId || 'nicht angelegt')}
          ${fact('Task', report.taskId || 'nicht angelegt')}
          ${fact('Angelegt', formatDate(report.createdAt))}
        </div>
      </section>
      <section class="reports-section">
        <h3>Beschreibung</h3>
        <p>${escapeHtml(report.summary || 'Keine Beschreibung hinterlegt.')}</p>
      </section>
      <section class="reports-section">
        <h3>Erwartung</h3>
        <p>${escapeHtml(report.expected || 'Keine Erwartung hinterlegt.')}</p>
      </section>
      <section class="reports-section">
        <h3>Was CTOX geaendert hat</h3>
        <p>${escapeHtml(report.changeSummary || changeFallback(report))}</p>
      </section>
      ${attachment ? `
        <section class="reports-section">
          <h3>Screenshot und Markup</h3>
          <div class="reports-attachment">
            <span class="reports-attachment-meta">${escapeHtml(attachment.capture_mode || 'capture')}</span>
            <img src="${escapeAttr(attachment.data_url)}" alt="Report Screenshot" />
          </div>
        </section>
      ` : ''}
      <section class="reports-section">
        <h3>Rollback</h3>
        <div class="reports-rollback">
          <p>${escapeHtml(releases.length ? 'Waehle eine gespeicherte Modulversion und rolle das betroffene Modul zurueck.' : 'Fuer dieses Modul gibt es noch keine gespeicherte Version.')}</p>
          <div class="reports-rollback-actions">
            <select data-rollback-version ${releases.length ? '' : 'disabled'}>
              ${releases.map((release) => `<option value="${escapeAttr(release.versionId)}">v${escapeHtml(release.version)} · ${escapeHtml(release.status || '')} · ${escapeHtml(formatDate(release.createdAt))}</option>`).join('')}
            </select>
            <button type="button" data-rollback-module ${releases.length ? '' : 'disabled'}>Rollback</button>
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
      kindLabel: normalizeKind(report.kind || payload.kind) === 'bug' ? 'Bug' : 'Feature',
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
  status.textContent = 'Rollback laeuft...';
  try {
    const res = await fetch('/api/business-os/modules/rollback', {
      method: 'POST',
      headers: state.ctx.authHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ module_id: report.moduleId, version_id: versionId }),
    });
    if (!res.ok) throw new Error(await res.text());
    status.textContent = 'Rollback ausgefuehrt.';
    await refreshReports();
  } catch (error) {
    status.textContent = error.message || String(error);
  }
}

function fact(label, value) {
  return `<dl class="reports-fact"><dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value || '-')}</dd></dl>`;
}

function changeFallback(report) {
  if (report.commandId || report.taskId) {
    return `CTOX hat den Report angenommen. Command ${report.commandId || '-'}, Task ${report.taskId || '-'}. Sobald der Lauf eine Change Summary schreibt, erscheint sie hier.`;
  }
  return 'Noch keine CTOX-Annahme oder Change Summary vorhanden.';
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
    open: 'Offen',
    running: 'In Arbeit',
    completed: 'Erledigt',
    blocked: 'Blockiert',
  }[normalized] || value || 'Offen';
}

function objectValue(value) {
  return value && typeof value === 'object' ? value : {};
}

function formatDate(value) {
  const timestamp = typeof value === 'number' ? value : Date.parse(value || '');
  if (!Number.isFinite(timestamp) || timestamp <= 0) return '-';
  return new Intl.DateTimeFormat('de-DE', {
    day: '2-digit',
    month: '2-digit',
    year: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(timestamp));
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
