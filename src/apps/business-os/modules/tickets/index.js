import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';
import { showBusinessPrompt } from '../../shared/dialogs.js';

const REFRESH_DEBOUNCE_MS = 80;
const LAYOUT_KEY = 'ctox.tickets.layout';

const labels = {
  de: {
    createLocal: 'Neu',
    search: 'Suchen...',
    allStatus: 'Alle Status',
    open: 'Offen',
    pending: 'Pending',
    blocked: 'Blockiert',
    closed: 'Geschlossen',
    noTickets: 'Noch keine Tickets verfügbar.',
    noTicketsDetail: 'Neue Tickets erscheinen hier, sobald sie für CTOX bereitstehen.',
    selectTicket: 'Wähle links ein Ticket aus.',
    selectTicketDetail: 'Details, Verlauf und Kontrollen werden danach hier angezeigt.',
    timeline: 'Timeline',
    cases: 'Cases',
    selfWork: 'Self-work',
    controls: 'Kontrollen',
    approvals: 'Approvals',
    verification: 'Verification',
    writebacks: 'Writebacks',
    label: 'Label',
    source: 'Quelle',
    requester: 'Requester',
    priority: 'Priorität',
    status: 'Status',
    updated: 'Aktualisiert',
    noEvents: 'Keine Events vorhanden.',
    noCase: 'Kein Case für dieses Ticket.',
    noSelfWork: 'Kein Self-work verknüpft.',
    clarifications: 'Rückfragen',
    noClarifications: 'Keine offenen Rückfragen.',
    requestClarification: 'Rückfrage',
    publishClarification: 'Geprüft senden',
    resolveClarification: 'Antwort erfassen',
    promptQuestion: 'Rückfrage',
    promptMissingInputs: 'Fehlende Werte (kommagetrennt)',
    promptReviewSummary: 'Prüfnotiz',
    promptResponseKey: 'Antwort-Referenz',
    promptResponseBody: 'Antwortinhalt',
    commandPending: 'Befehl wird verarbeitet...',
    commandDone: 'Befehl abgeschlossen.',
    commandUnavailable: 'Ticket-Aktionen sind gerade nicht verfügbar.',
    promptTicketTitle: 'Ticket-Titel',
    promptTicketBody: 'Beschreibung',
    promptRationale: 'Begründung',
    promptSummary: 'Zusammenfassung',
    promptEvidence: 'Nachweis',
    promptInternalNote: 'Interne Notiz',
    promptReply: 'Antwort',
    promptConfirm: 'Übernehmen',
    promptCancel: 'Abbrechen',
  },
  en: {
    createLocal: 'New',
    search: 'Search...',
    allStatus: 'All status',
    open: 'Open',
    pending: 'Pending',
    blocked: 'Blocked',
    closed: 'Closed',
    noTickets: 'No tickets available yet.',
    noTicketsDetail: 'New tickets appear here once they are ready for CTOX.',
    selectTicket: 'Select a ticket on the left.',
    selectTicketDetail: 'Details, timeline, and controls appear here after selection.',
    timeline: 'Timeline',
    cases: 'Cases',
    selfWork: 'Self-work',
    controls: 'Controls',
    approvals: 'Approvals',
    verification: 'Verification',
    writebacks: 'Writebacks',
    label: 'Label',
    source: 'Source',
    requester: 'Requester',
    priority: 'Priority',
    status: 'Status',
    updated: 'Updated',
    noEvents: 'No events available.',
    noCase: 'No case for this ticket.',
    noSelfWork: 'No linked self-work.',
    clarifications: 'Clarifications',
    noClarifications: 'No open clarifications.',
    requestClarification: 'Clarify',
    publishClarification: 'Send reviewed',
    resolveClarification: 'Record answer',
    promptQuestion: 'Clarification question',
    promptMissingInputs: 'Missing values (comma-separated)',
    promptReviewSummary: 'Review note',
    promptResponseKey: 'Answer reference',
    promptResponseBody: 'Answer body',
    commandPending: 'Command is being processed...',
    commandDone: 'Command completed.',
    commandUnavailable: 'Ticket actions are unavailable right now.',
    promptTicketTitle: 'Ticket title',
    promptTicketBody: 'Description',
    promptRationale: 'Rationale',
    promptSummary: 'Summary',
    promptEvidence: 'Evidence',
    promptInternalNote: 'Internal note',
    promptReply: 'Reply',
    promptConfirm: 'Apply',
    promptCancel: 'Cancel',
  },
};

const collectionNames = [
  'ctox_ticket_items',
  'ctox_ticket_events',
  'ctox_ticket_event_routing_state',
  'ctox_ticket_cases',
  'ctox_ticket_self_work_items',
  'ctox_ticket_self_work_notes',
  'ctox_ticket_label_assignments',
  'ctox_ticket_control_bundles',
  'ctox_ticket_approvals',
  'ctox_ticket_verifications',
  'ctox_ticket_writebacks',
  'ctox_ticket_clarification_requests',
];

const state = {
  ctx: null,
  lang: 'de',
  t: (key, fallback) => fallback || key,
  selectedId: '',
  search: '',
  status: 'all',
  renderTimer: null,
  cleanup: null,
  resizeCleanup: null,
  data: Object.fromEntries(collectionNames.map((name) => [name, []])),
};

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  const messages = await loadModuleMessages(import.meta.url, state.lang, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();
  applyStaticLabels();
  wireUi();
  state.resizeCleanup = setupResizers();
  await refreshTickets();
  state.cleanup = wireRealtime();
  return () => {
    state.cleanup?.();
    state.resizeCleanup?.();
    if (state.renderTimer) window.clearTimeout(state.renderTimer);
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-tickets-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.ticketsStyle = 'true';
  document.head.append(link);
}

function applyStaticLabels() {
  const root = state.ctx.host.querySelector('[data-tickets-root]');
  root.querySelector('[data-ticket-create-local]').textContent = state.t('createLocal', 'Neu');
  root.querySelector('[data-ticket-search]').placeholder = state.t('search', 'Suchen...');
  root.querySelector('[data-ticket-state]').innerHTML = `
    <option value="all">${escapeHtml(state.t('allStatus', 'Alle Status'))}</option>
    <option value="open">${escapeHtml(state.t('open', 'Offen'))}</option>
    <option value="pending">${escapeHtml(state.t('pending', 'Pending'))}</option>
    <option value="blocked">${escapeHtml(state.t('blocked', 'Blockiert'))}</option>
    <option value="closed">${escapeHtml(state.t('closed', 'Geschlossen'))}</option>
  `;
}

function wireUi() {
  const root = state.ctx.host.querySelector('[data-tickets-root]');
  root.querySelector('[data-ticket-search]')?.addEventListener('input', (event) => {
    state.search = event.target.value || '';
    render();
  });
  root.querySelector('[data-ticket-state]')?.addEventListener('change', (event) => {
    state.status = event.target.value || 'all';
    render();
  });
  root.querySelector('[data-ticket-list]')?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const row = target?.closest('[data-ticket-id]');
    if (!row) return;
    state.selectedId = row.getAttribute('data-ticket-id') || '';
    render();
  });
  root.querySelector('[data-ticket-create-local]')?.addEventListener('click', () => {
    createLocalTicket().catch((error) => setCommandStatus(error?.message || String(error), true));
  });
  root.querySelector('[data-ticket-context]')?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const clarificationAction = target?.closest('[data-clarification-action]');
    if (clarificationAction) {
      runClarificationAction(clarificationAction)
        .catch((error) => setCommandStatus(error?.message || String(error), true));
      return;
    }
    const action = target?.closest('[data-ticket-action]');
    if (!action) return;
    runCaseAction(action).catch((error) => setCommandStatus(error?.message || String(error), true));
  });
}

function setupResizers() {
  const root = state.ctx.host.querySelector('[data-tickets-root]');
  const left = root.querySelector('[data-resizer="left"]');
  const right = root.querySelector('[data-resizer="right"]');
  const saved = readLayout();
  if (saved.left) root.style.setProperty('--tickets-left-width', `${saved.left}px`);
  if (saved.right) root.style.setProperty('--tickets-right-width', `${saved.right}px`);
  const cleanups = [];
  if (left) {
    const resizer = new CtoxResizer({
      resizerEl: left,
      containerEl: root,
      cssVar: '--tickets-left-width',
      side: 'left',
      minWidth: 260,
      maxWidth: 520,
      onResize: (width) => writeLayout({ ...readLayout(), left: width }),
    });
    cleanups.push(() => resizer.destroy());
  }
  if (right) {
    const resizer = new CtoxResizer({
      resizerEl: right,
      containerEl: root,
      cssVar: '--tickets-right-width',
      side: 'right',
      minWidth: 260,
      maxWidth: 560,
      onResize: (width) => writeLayout({ ...readLayout(), right: width }),
    });
    cleanups.push(() => resizer.destroy());
  }
  return () => cleanups.forEach((cleanup) => cleanup());
}

function readLayout() {
  try {
    return JSON.parse(localStorage.getItem(LAYOUT_KEY) || '{}');
  } catch {
    return {};
  }
}

function writeLayout(next) {
  localStorage.setItem(LAYOUT_KEY, JSON.stringify(next));
}

function wireRealtime() {
  const subscriptions = collectionNames
    .map((name) => state.ctx.db?.raw?.[name]?.$?.subscribe?.(() => scheduleRefresh()))
    .filter(Boolean);
  return () => subscriptions.forEach((sub) => {
    try { sub.unsubscribe?.(); } catch {}
  });
}

function scheduleRefresh() {
  if (state.renderTimer) return;
  state.renderTimer = window.setTimeout(() => {
    state.renderTimer = null;
    refreshTickets().catch((error) => console.warn('[tickets] refresh failed', error));
  }, REFRESH_DEBOUNCE_MS);
}

async function refreshTickets() {
  const entries = await Promise.all(collectionNames.map(async (name) => [name, await loadCollection(name)]));
  state.data = Object.fromEntries(entries);
  const visible = filteredTickets();
  if (!state.selectedId || !visible.some((ticket) => ticket.id === state.selectedId)) {
    state.selectedId = visible[0]?.id || '';
  }
  render();
}

async function loadCollection(name) {
  const collection = state.ctx.db?.raw?.[name];
  if (!collection) return [];
  const docs = await collection.find().exec();
  return docs.map((doc) => doc.toJSON()).filter((doc) => !doc.is_deleted && !doc._deleted);
}

function filteredTickets() {
  const query = state.search.trim().toLowerCase();
  return [...state.data.ctox_ticket_items]
    .sort((left, right) => Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0))
    .filter((ticket) => {
      const status = String(ticket.remote_status || '').toLowerCase();
      const statusMatch = state.status === 'all'
        || status.includes(state.status)
        || (state.status === 'open' && !status.includes('closed'));
      if (!statusMatch) return false;
      if (!query) return true;
      const haystack = [
        ticket.ticket_key,
        ticket.title,
        ticket.body_text,
        ticket.requester,
        ticket.priority,
        ticket.source_system,
        status,
      ].join(' ').toLowerCase();
      return haystack.includes(query);
    });
}

function render() {
  renderList();
  renderDetail();
  renderContext();
}

function renderList() {
  const list = state.ctx.host.querySelector('[data-ticket-list]');
  const tickets = filteredTickets();
  if (!tickets.length) {
    list.innerHTML = renderEmptyState(
      state.t('noTickets', 'Noch keine Tickets verfügbar.'),
      state.t('noTicketsDetail', 'Neue Tickets erscheinen hier, sobald sie für CTOX bereitstehen.'),
    );
    return;
  }
  list.innerHTML = tickets.map((ticket) => {
    const label = labelForTicket(ticket.ticket_key);
    const selected = ticket.id === state.selectedId ? 'is-selected' : '';
    return `
      <button type="button" class="ticket-row ${selected}" data-ticket-id="${escapeAttr(ticket.id)}"
        data-context-module="tickets"
        data-context-submodule="inbox"
        data-context-record-type="ticket"
        data-context-record-id="${escapeAttr(ticket.ticket_key || ticket.id)}"
        data-context-label="${escapeAttr(ticket.title || ticket.ticket_key || ticket.id)}"
        data-context-skill="product_engineering/business-basic-module-development">
        <span class="ticket-row-meta">
          <span>${escapeHtml(ticket.source_system || 'ctox')}</span>
          <span>${escapeHtml(displayStatus(ticket.remote_status || 'open'))}</span>
        </span>
        <strong>${escapeHtml(ticket.title || ticket.ticket_key || 'Ticket')}</strong>
        <small>${escapeHtml(label?.label || ticket.priority || ticket.requester || ticket.ticket_key || '')}</small>
      </button>
    `;
  }).join('');
}

function renderDetail() {
  const detail = state.ctx.host.querySelector('[data-ticket-detail]');
  const ticket = selectedTicket();
  if (!ticket) {
    detail.innerHTML = renderEmptyState(
      state.t('selectTicket', 'Wähle links ein Ticket aus.'),
      state.t('selectTicketDetail', 'Details, Verlauf und Kontrollen werden danach hier angezeigt.'),
      'is-centered',
    );
    return;
  }
  const events = eventsForTicket(ticket.ticket_key);
  detail.innerHTML = `
    <header class="tickets-detail-head">
      <div>
        <span>${escapeHtml(ticket.ticket_key || ticket.id)}</span>
        <h1>${escapeHtml(ticket.title || ticket.ticket_key || 'Ticket')}</h1>
      </div>
      <span class="tickets-status">${escapeHtml(displayStatus(ticket.remote_status || 'open'))}</span>
    </header>
    <div class="tickets-detail-scroll os-scrollbar">
      <section class="tickets-section">
        <h3>Ticket</h3>
        <div class="tickets-facts">
          ${fact(state.t('source', 'Quelle'), ticket.source_system)}
          ${fact(state.t('requester', 'Requester'), ticket.requester)}
          ${fact(state.t('priority', 'Priorität'), ticket.priority)}
          ${fact(state.t('updated', 'Aktualisiert'), formatDate(ticket.updated_at || ticket.last_synced_at))}
        </div>
        <p class="tickets-body">${escapeHtml(ticket.body_text || '')}</p>
      </section>
      <section class="tickets-section">
        <h3>${escapeHtml(state.t('timeline', 'Timeline'))}</h3>
        ${events.length ? `<ol class="ticket-timeline">${events.map(renderEvent).join('')}</ol>` : `<p class="tickets-empty">${escapeHtml(state.t('noEvents', 'Keine Events vorhanden.'))}</p>`}
      </section>
    </div>
  `;
}

function renderContext() {
  const context = state.ctx.host.querySelector('[data-ticket-context]');
  const ticket = selectedTicket();
  if (!ticket) {
    context.innerHTML = renderEmptyState(
      state.t('controls', 'Kontrollen'),
      state.t('selectTicketDetail', 'Details, Verlauf und Kontrollen werden danach hier angezeigt.'),
      'is-context',
    );
    return;
  }
  const cases = casesForTicket(ticket.ticket_key);
  const selfWork = selfWorkForTicket(ticket.ticket_key);
  const clarifications = clarificationsForTicket(ticket.ticket_key);
  const bundles = state.data.ctox_ticket_control_bundles;
  context.innerHTML = `
    <header class="ctox-pane-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('controls', 'Kontrollen'))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(ticket.ticket_key || ticket.id)}</h2>
        </div>
      </div>
    </header>
    <div class="tickets-context-scroll os-scrollbar">
      <section class="tickets-section">
        <h3>${escapeHtml(state.t('cases', 'Cases'))}</h3>
        ${cases.length ? cases.map(renderCase).join('') : `<p class="tickets-empty">${escapeHtml(state.t('noCase', 'Kein Case für dieses Ticket.'))}</p>`}
      </section>
      <section class="tickets-section">
        <h3>${escapeHtml(state.t('selfWork', 'Self-work'))}</h3>
        ${selfWork.length ? selfWork.map(renderSelfWork).join('') : `<p class="tickets-empty">${escapeHtml(state.t('noSelfWork', 'Kein Self-work verknüpft.'))}</p>`}
      </section>
      <section class="tickets-section">
        <h3>${escapeHtml(state.t('clarifications', 'Rückfragen'))}</h3>
        ${clarifications.length ? clarifications.map(renderClarification).join('') : `<p class="tickets-empty">${escapeHtml(state.t('noClarifications', 'Keine offenen Rückfragen.'))}</p>`}
      </section>
      <section class="tickets-section">
        <h3>Runbooks</h3>
        ${bundles.length ? bundles.slice(0, 8).map(renderBundle).join('') : `<p class="tickets-empty">Keine Control Bundles.</p>`}
      </section>
    </div>
  `;
}

function renderEvent(event) {
  const route = state.data.ctox_ticket_event_routing_state.find((item) => item.event_key === event.event_key);
  return `
    <li>
      <span>${escapeHtml(formatDate(event.external_created_at || event.observed_at))}</span>
      <strong>${escapeHtml(event.summary || event.event_type || 'Event')}</strong>
      <small>${escapeHtml([event.direction, event.event_type, route?.route_status].filter(Boolean).join(' · '))}</small>
      ${event.body_text ? `<p>${escapeHtml(event.body_text)}</p>` : ''}
    </li>
  `;
}

function renderCase(item) {
  const approvals = state.data.ctox_ticket_approvals.filter((approval) => approval.case_id === item.case_id);
  const verifications = state.data.ctox_ticket_verifications.filter((verification) => verification.case_id === item.case_id);
  const writebacks = state.data.ctox_ticket_writebacks.filter((writeback) => writeback.case_id === item.case_id);
  const clarifications = state.data.ctox_ticket_clarification_requests.filter((clarification) => clarification.case_id === item.case_id);
  return `
    <article class="tickets-context-item">
      <span>${escapeHtml(item.state || 'case')} · ${escapeHtml(item.risk_level || '')}</span>
      <strong>${escapeHtml(item.label || item.case_id)}</strong>
      <small>${escapeHtml(item.approval_mode || '')} · A${escapeHtml(String(item.autonomy_level || '').replace(/^A/i, ''))}</small>
      <dl>
        ${fact(state.t('approvals', 'Approvals'), String(approvals.length))}
        ${fact(state.t('verification', 'Verification'), verifications[0]?.status || '')}
        ${fact(state.t('writebacks', 'Writebacks'), String(writebacks.length))}
        ${fact(state.t('clarifications', 'Rückfragen'), String(clarifications.length))}
      </dl>
      ${renderCaseActions(item)}
    </article>
  `;
}

function renderCaseActions(item) {
  const actions = actionsForCase(item);
  if (!actions.length) return '';
  return `
    <div class="tickets-action-row">
      ${actions.map((action) => `
        <button type="button" class="tickets-command-button" data-ticket-action="${escapeAttr(action.id)}" data-case-id="${escapeAttr(item.case_id)}">
          ${escapeHtml(action.label)}
        </button>
      `).join('')}
    </div>
  `;
}

function actionsForCase(item) {
  const stateValue = String(item.state || '').toLowerCase();
  const actions = [];
  if (['approval_pending', 'needs_approval', 'pending_approval'].includes(stateValue)) {
    actions.push({ id: 'approve', label: 'Approve' }, { id: 'reject', label: 'Reject' });
  }
  if (stateValue === 'executable') actions.push({ id: 'execute', label: 'Execute' });
  if (stateValue === 'executing') actions.push({ id: 'verify', label: 'Verify' });
  if (stateValue === 'writeback_pending') {
    actions.push(
      { id: 'internal-note', label: 'Internal note' },
      { id: 'public-reply', label: 'Reply' },
      { id: 'close', label: 'Close' },
    );
  }
  const hasOpenClarification = state.data.ctox_ticket_clarification_requests.some((clarification) => (
    clarification.case_id === item.case_id
    && !['resolved', 'cancelled'].includes(String(clarification.status || '').toLowerCase())
  ));
  if (!hasOpenClarification && !['closed', 'done', 'completed', 'verified', 'writeback_pending'].includes(stateValue)) {
    actions.push({ id: 'request-clarification', label: state.t('requestClarification', 'Rückfrage') });
  }
  return actions;
}

async function createLocalTicket() {
  const title = await promptText(state.t('promptTicketTitle', 'Ticket-Titel'), '', true);
  if (!title?.trim()) return;
  const body = await promptText(state.t('promptTicketBody', 'Beschreibung'));
  await dispatchTicketCommand('ctox.ticket.local.create', `local:${title.trim()}`, {
    title: title.trim(),
    body: body || '',
    status: 'open',
    priority: 'normal',
  });
}

async function runCaseAction(actionEl) {
  const action = actionEl.getAttribute('data-ticket-action');
  const caseId = actionEl.getAttribute('data-case-id');
  if (!action || !caseId) return;
  if (action === 'approve') {
    await dispatchTicketCommand('ctox.ticket.approve', caseId, { case_id: caseId, status: 'approved' });
  } else if (action === 'reject') {
    const rationale = await promptText(state.t('promptRationale', 'Begründung'));
    await dispatchTicketCommand('ctox.ticket.approve', caseId, { case_id: caseId, status: 'rejected', rationale });
  } else if (action === 'execute') {
    const summary = await promptText(state.t('promptSummary', 'Zusammenfassung'), '', true);
    if (!summary.trim()) return;
    await dispatchTicketCommand('ctox.ticket.execute', caseId, { case_id: caseId, summary });
  } else if (action === 'verify') {
    const summary = await promptText(state.t('promptEvidence', 'Nachweis'));
    await dispatchTicketCommand('ctox.ticket.verify', caseId, { case_id: caseId, status: 'passed', summary });
  } else if (action === 'request-clarification') {
    const question = await promptText(state.t('promptQuestion', 'Rückfrage'), '', true);
    if (!question?.trim()) return;
    const missingCsv = await promptText(state.t('promptMissingInputs', 'Fehlende Werte (kommagetrennt)'));
    await dispatchTicketCommand('ctox.ticket.request_clarification', caseId, {
      case_id: caseId,
      question,
      missing_inputs: parseCsvInput(missingCsv || ''),
      target_type: 'requester',
      target_channel: 'ticket',
      unblock_criteria: missingCsv?.trim()
        ? `Requester supplies: ${missingCsv.trim()}`
        : 'Requester supplies the missing information.',
    });
  } else if (action === 'internal-note' || action === 'public-reply') {
    const body = await promptText(
      action === 'internal-note'
        ? state.t('promptInternalNote', 'Interne Notiz')
        : state.t('promptReply', 'Antwort'),
      '',
      true,
    );
    if (!body.trim()) return;
    await dispatchTicketCommand('ctox.ticket.writeback_comment', caseId, {
      case_id: caseId,
      body,
      internal: action === 'internal-note',
    });
  } else if (action === 'close') {
    const summary = await promptText(state.t('promptSummary', 'Zusammenfassung'));
    await dispatchTicketCommand('ctox.ticket.close', caseId, { case_id: caseId, summary });
  }
}

async function promptText(title, defaultValue = '', required = false) {
  const value = await showBusinessPrompt(title, {
    title,
    message: required ? title : '',
    defaultValue,
    confirmLabel: state.t('promptConfirm', 'Übernehmen'),
    cancelLabel: state.t('promptCancel', 'Abbrechen'),
    kind: 'info',
  });
  if (value === null) return null;
  const text = String(value || '').trim();
  return required && !text ? null : text;
}

async function dispatchTicketCommand(commandType, recordId, payload) {
  const collection = state.ctx.db?.collection?.('business_commands');
  if (!collection) throw new Error(state.t('commandUnavailable', 'Ticket-Aktionen sind gerade nicht verfügbar.'));
  await state.ctx.sync?.startCollection?.('business_commands');
  const commandId = `cmd_${randomId()}`;
  setCommandStatus(state.t('commandPending', 'Befehl wird verarbeitet...'));
  const command = {
    id: commandId,
    module: 'tickets',
    type: commandType,
    command_type: commandType,
    record_id: recordId || '',
    inbound_channel: 'tickets',
    payload,
    client_context: {
      source: 'business-os.tickets',
      module_id: 'tickets',
      actor: actorContext(state.ctx.session),
    },
  };
  if (state.ctx.commandBus?.dispatch) {
    await state.ctx.commandBus.dispatch(command);
  } else {
    const now = Date.now();
    await collection.incrementalUpsert?.({
      ...command,
      status: 'pending_sync',
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
  await waitForCommandProjection(commandId);
  setCommandStatus(state.t('commandDone', 'Befehl abgeschlossen.'));
  await refreshTickets();
}

async function waitForCommandProjection(commandId, timeoutMs = 45000) {
  const collection = state.ctx.db?.collection?.('business_commands');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const doc = await collection?.findOne(commandId).exec();
    const data = doc?.toJSON?.();
    if (data && data.status && data.status !== 'pending_sync') {
      if (data.status === 'failed') throw new Error(commandFailureMessage(data, commandId));
      return data;
    }
    await delay(300);
  }
  throw new Error(`Aktion ${commandId} wurde noch nicht abgeschlossen.`);
}

function commandFailureMessage(data, commandId) {
  return data?.error || data?.result?.error || `Aktion ${commandId} ist fehlgeschlagen.`;
}

function setCommandStatus(message, isError = false) {
  const el = state.ctx.host.querySelector('[data-ticket-command-status]');
  if (!el) return;
  el.hidden = !message;
  el.textContent = message || '';
  el.dataset.state = isError ? 'error' : 'info';
}

function actorContext(session) {
  const user = session?.user || {};
  return {
    id: user.id || 'business-os-user',
    display_name: user.display_name || user.name || user.id || 'Business OS User',
    role: user.role || 'user',
    is_admin: Boolean(user.is_admin),
  };
}

function randomId() {
  return globalThis.crypto?.randomUUID?.().replaceAll('-', '')
    || `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`;
}

function delay(ms) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function renderSelfWork(item) {
  const notes = state.data.ctox_ticket_self_work_notes.filter((note) => note.work_id === item.work_id);
  return `
    <article class="tickets-context-item">
      <span>${escapeHtml(item.kind || 'self-work')} · ${escapeHtml(item.state || '')}</span>
      <strong>${escapeHtml(item.title || item.work_id)}</strong>
      <small>${escapeHtml([item.assigned_to, `${notes.length} notes`].filter(Boolean).join(' · '))}</small>
    </article>
  `;
}

function renderClarification(item) {
  const status = String(item.status || '').toLowerCase();
  const canPublish = ['draft', 'send_failed'].includes(status)
    && item.target_type === 'requester'
    && item.target_channel === 'ticket';
  const canResolve = !['resolved', 'cancelled'].includes(status);
  const missing = Array.isArray(item.missing_inputs) ? item.missing_inputs.join(', ') : '';
  return `
    <article class="tickets-context-item">
      <span>${escapeHtml([item.status, item.target_type, item.target_channel].filter(Boolean).join(' · '))}</span>
      <strong>${escapeHtml(item.question || item.clarification_id)}</strong>
      <small>${escapeHtml(missing || item.unblock_criteria || item.outbound_message_key || '')}</small>
      ${item.inbound_response_body ? `<p class="tickets-note">${escapeHtml(item.inbound_response_body)}</p>` : ''}
      ${canPublish || canResolve ? `
        <div class="tickets-action-row">
          ${canPublish ? `
            <button type="button" class="tickets-command-button"
              data-clarification-action="publish"
              data-clarification-id="${escapeAttr(item.clarification_id)}">
              ${escapeHtml(state.t('publishClarification', 'Geprüft senden'))}
            </button>
          ` : ''}
          ${canResolve ? `
          <button type="button" class="tickets-command-button"
            data-clarification-action="resolve"
            data-clarification-id="${escapeAttr(item.clarification_id)}">
            ${escapeHtml(state.t('resolveClarification', 'Antwort erfassen'))}
          </button>
          ` : ''}
        </div>
      ` : ''}
    </article>
  `;
}

async function runClarificationAction(actionEl) {
  const action = actionEl.getAttribute('data-clarification-action');
  const clarificationId = actionEl.getAttribute('data-clarification-id');
  if (!clarificationId) return;
  if (action === 'publish') {
    const reviewSummary = await promptText(
      state.t('promptReviewSummary', 'Prüfnotiz'),
      'Clarification question reviewed for this ticket.',
      true,
    );
    if (!reviewSummary?.trim()) return;
    await dispatchTicketCommand('ctox.ticket.publish_clarification', clarificationId, {
      clarification_id: clarificationId,
      reviewed_by: state.ctx?.session?.user?.id || 'business-os',
      review_summary: reviewSummary,
    });
    return;
  }
  if (action !== 'resolve') return;
  const responseKey = await promptText(state.t('promptResponseKey', 'Antwort-Referenz'), `manual:${Date.now()}`, true);
  if (!responseKey?.trim()) return;
  const body = await promptText(state.t('promptResponseBody', 'Antwortinhalt'));
  await dispatchTicketCommand('ctox.ticket.resolve_clarification', clarificationId, {
    clarification_id: clarificationId,
    response_key: responseKey,
    body: body || '',
  });
}

function renderBundle(item) {
  return `
    <article class="tickets-context-item is-compact">
      <span>${escapeHtml(item.support_mode || 'support')}</span>
      <strong>${escapeHtml(item.label || item.runbook_id)}</strong>
      <small>${escapeHtml(item.approval_mode || '')} · ${escapeHtml(item.verification_profile_id || '')}</small>
    </article>
  `;
}

function selectedTicket() {
  return state.data.ctox_ticket_items.find((ticket) => ticket.id === state.selectedId)
    || filteredTickets()[0]
    || null;
}

function eventsForTicket(ticketKey) {
  return state.data.ctox_ticket_events
    .filter((event) => event.ticket_key === ticketKey)
    .sort((left, right) => Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0));
}

function casesForTicket(ticketKey) {
  return state.data.ctox_ticket_cases
    .filter((item) => item.ticket_key === ticketKey)
    .sort((left, right) => Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0));
}

function selfWorkForTicket(ticketKey) {
  return state.data.ctox_ticket_self_work_items
    .filter((item) => item.remote_ticket_id === ticketKey || item.metadata?.ticket_key === ticketKey)
    .sort((left, right) => Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0));
}

function clarificationsForTicket(ticketKey) {
  return state.data.ctox_ticket_clarification_requests
    .filter((item) => item.ticket_key === ticketKey)
    .sort((left, right) => Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0));
}

function labelForTicket(ticketKey) {
  return state.data.ctox_ticket_label_assignments.find((item) => item.ticket_key === ticketKey) || null;
}

function fact(label, value) {
  if (value === undefined || value === null || value === '') return '';
  return `<div><dt>${escapeHtml(label)}</dt><dd>${escapeHtml(String(value))}</dd></div>`;
}

function renderEmptyState(title, body = '', modifier = '') {
  return `
    <div class="tickets-empty ${escapeAttr(modifier)}">
      <strong>${escapeHtml(title)}</strong>
      ${body ? `<span>${escapeHtml(body)}</span>` : ''}
    </div>
  `;
}

function displayStatus(value) {
  return String(value || '')
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function parseCsvInput(value) {
  return String(value || '')
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

function formatDate(value) {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(state.lang === 'en' ? 'en-US' : 'de-DE', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function escapeAttr(value) {
  return escapeHtml(value).replaceAll('`', '&#096;');
}

export const __ticketTestHooks = {
  commandFailureMessage,
  setCommandStatusForSmoke(ctx, message, isError = false) {
    const previousCtx = state.ctx;
    state.ctx = ctx;
    try {
      setCommandStatus(message, isError);
    } finally {
      state.ctx = previousCtx;
    }
  },
};
