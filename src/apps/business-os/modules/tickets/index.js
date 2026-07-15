import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';
import { showBusinessPrompt } from '../../shared/dialogs.js';

const REFRESH_DEBOUNCE_MS = 80;
const LAYOUT_KEY = 'ctox.tickets.layout';
const TICKET_PRIMARY_COLLECTION = 'ctox_ticket_items';
const TICKET_SYNC_START_TIMEOUT_MS = 8000;
const TICKET_HYDRATION_TIMEOUT_MS = 12000;
const TICKET_HYDRATION_POLL_MS = 350;

const labels = {
  de: {
    createTicket: 'Ticket anlegen',
    search: 'Suchen...',
    allStatus: 'Alle Status',
    open: 'Offen',
    pending: 'Pending',
    blocked: 'Blockiert',
    closed: 'Geschlossen',
    loadingTickets: 'Tickets werden geladen...',
    loadingTicketsDetail: 'Die Ticket-Projektionen werden vorbereitet.',
    syncingTickets: 'Tickets werden synchronisiert.',
    syncingTicketsDetail: 'Die Ticketdaten werden gerade aus dem CTOX-Datenstrom geladen.',
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
    createTicket: 'Create ticket',
    search: 'Search...',
    allStatus: 'All status',
    open: 'Open',
    pending: 'Pending',
    blocked: 'Blocked',
    closed: 'Closed',
    loadingTickets: 'Loading tickets...',
    loadingTicketsDetail: 'Ticket projections are being prepared.',
    syncingTickets: 'Syncing tickets.',
    syncingTicketsDetail: 'Ticket data is loading from the CTOX data stream.',
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
  loading: false,
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
  state.loading = true;
  applyStaticLabels();
  wireUi();
  state.resizeCleanup = setupResizers();
  render();
  // The shell-owned module lease already starts every declared ticket
  // collection. Scheduling a second wave here races fast window close and can
  // recreate bridges after the lease has released them.
  await waitForPrimaryTicketDataOrReady();
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
  const createButton = root.querySelector('[data-ticket-create-local]');
  const createLabel = state.t('createTicket', 'Ticket anlegen');
  createButton.setAttribute('aria-label', createLabel);
  createButton.setAttribute('title', createLabel);
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
  // Column resizing is now owned by the shell-global resizer (setupModuleResizers
  // in app.js), wired declaratively from the `.ctox-column-resizer[data-resizer-var]`
  // handles inside the `[data-resize-frame]` root. This DIY wiring is neutralised to
  // avoid double-binding the handles; call sites keep their no-op teardown ref.
  return () => {};
  // eslint-disable-next-line no-unreachable
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

function ticketCollection(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

function wireRealtime() {
  const subscriptions = collectionNames
    .map((name) => ticketCollection(name)?.$?.subscribe?.(() => scheduleRefresh()))
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
  state.loading = false;
  const visible = filteredTickets();
  if (!state.selectedId || !visible.some((ticket) => ticket.id === state.selectedId)) {
    state.selectedId = visible[0]?.id || '';
  }
  render();
}

async function loadCollection(name) {
  const collection = ticketCollection(name);
  if (!collection) return [];
  const docs = await collection.find().exec();
  return docs.map((doc) => doc.toJSON()).filter((doc) => !doc.is_deleted && !doc._deleted);
}

async function startTicketCollections() {
  const sync = state.ctx.sync;
  if (typeof sync?.startCollection !== 'function') return;
  const available = collectionNames.filter((name) => ticketCollection(name));
  if (!available.length) return;
  const primary = available.includes(TICKET_PRIMARY_COLLECTION) ? TICKET_PRIMARY_COLLECTION : available[0];
  try {
    await withTimeout(
      sync.startCollection(primary),
      TICKET_SYNC_START_TIMEOUT_MS,
      `${primary} sync start timed out`,
    );
  } catch (error) {
    recordSyncStartError(primary, error);
  }
  available
    .filter((name) => name !== primary)
    .forEach((name, index) => {
      window.setTimeout(() => {
        try {
          Promise.resolve(sync.startCollection(name))
            .catch((error) => recordSyncStartError(name, error));
        } catch (error) {
          recordSyncStartError(name, error);
        }
      }, index * 100);
    });
}

async function waitForPrimaryTicketDataOrReady(timeoutMs = TICKET_HYDRATION_TIMEOUT_MS) {
  if (typeof state.ctx.sync?.startCollection !== 'function') return;
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    const tickets = await loadCollection(TICKET_PRIMARY_COLLECTION);
    if (tickets.length) {
      state.data = { ...state.data, [TICKET_PRIMARY_COLLECTION]: tickets };
      return;
    }
    if (isCollectionSyncReady(TICKET_PRIMARY_COLLECTION)) return;
    await delay(TICKET_HYDRATION_POLL_MS);
  }
}

function recordSyncStartError(collection, error) {
  console.warn(`[tickets] ${collection} sync start failed`, error);
}

function isCollectionSyncReady(collection) {
  const diagnostics = state.ctx.sync?.diagnostics?.collections?.[collection];
  return isCollectionDiagnosticsReady(diagnostics);
}

function isCollectionDiagnosticsReady(diagnostics) {
  if (!diagnostics) return false;
  const status = diagnostics.connectionStatus || diagnostics.status || '';
  if (['connected', 'running', 'reused'].includes(status)) return true;
  if (diagnostics.connectedAt || diagnostics.initialReplicationAt) return true;
  if (diagnostics.initialReplicationState === 'complete') return true;
  const transport = diagnostics.frameTransport || {};
  return Number(transport.activePeerCount || 0) > 0
    && (Number(transport.sentFrames || 0) > 0 || Number(transport.receivedFrames || 0) > 0);
}

function shouldShowTicketSyncState() {
  if (state.data.ctox_ticket_items.length) return false;
  if (typeof state.ctx.sync?.startCollection !== 'function') return false;
  return !ticketCollection(TICKET_PRIMARY_COLLECTION) || !isCollectionSyncReady(TICKET_PRIMARY_COLLECTION);
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
  if (state.loading || shouldShowTicketSyncState()) {
    list.innerHTML = renderTicketLoadingState();
    return;
  }
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
        ${ticketContextAttrs(ticket, 'inbox')}>
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
    clearRecordContext(detail);
    if (state.loading || shouldShowTicketSyncState()) {
      detail.innerHTML = renderTicketLoadingState('is-centered');
      return;
    }
    detail.innerHTML = renderEmptyState(
      state.t('selectTicket', 'Wähle links ein Ticket aus.'),
      state.t('selectTicketDetail', 'Details, Verlauf und Kontrollen werden danach hier angezeigt.'),
      'is-centered',
    );
    return;
  }
  applyTicketContext(detail, ticket, 'detail');
  const events = eventsForTicket(ticket.ticket_key);
  detail.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(ticket.ticket_key || ticket.id)}</span>
          <h2 class="ctox-pane-title">${escapeHtml(ticket.title || ticket.ticket_key || 'Ticket')}</h2>
        </div>
        <div class="ctox-pane-actions">
          <span class="ctox-badge ${statusBadgeClass(ticket.remote_status)}">${escapeHtml(displayStatus(ticket.remote_status || 'open'))}</span>
        </div>
      </div>
    </header>
    <div class="tickets-detail-scroll os-scrollbar">
      <section class="tickets-section">
        <h3>Ticket</h3>
        <dl class="ctox-fields">
          ${fact(state.t('source', 'Quelle'), ticket.source_system)}
          ${fact(state.t('requester', 'Requester'), ticket.requester)}
          ${fact(state.t('priority', 'Priorität'), ticket.priority)}
          ${fact(state.t('updated', 'Aktualisiert'), formatDate(ticket.updated_at || ticket.last_synced_at))}
        </dl>
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
    clearRecordContext(context);
    if (state.loading || shouldShowTicketSyncState()) {
      context.innerHTML = renderTicketLoadingState('is-context');
      return;
    }
    context.innerHTML = renderEmptyState(
      state.t('controls', 'Kontrollen'),
      state.t('selectTicketDetail', 'Details, Verlauf und Kontrollen werden danach hier angezeigt.'),
      'is-context',
    );
    return;
  }
  applyTicketContext(context, ticket, 'context');
  const cases = casesForTicket(ticket.ticket_key);
  const selfWork = selfWorkForTicket(ticket.ticket_key);
  const clarifications = clarificationsForTicket(ticket.ticket_key);
  const bundles = state.data.ctox_ticket_control_bundles;
  context.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
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
    <li ${recordContextAttrs({
      type: 'ticket_event',
      id: event.event_key || event.id,
      label: event.summary || event.event_type || 'Event',
      submodule: 'timeline',
    })}>
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
    <article class="tickets-context-item" ${recordContextAttrs({
      type: 'ticket_case',
      id: item.case_id || item.id,
      label: item.label || item.case_id,
      submodule: 'cases',
    })}>
      <span>${escapeHtml(item.state || 'case')} · ${escapeHtml(item.risk_level || '')}</span>
      <strong>${escapeHtml(item.label || item.case_id)}</strong>
      <small>${escapeHtml(item.approval_mode || '')} · A${escapeHtml(String(item.autonomy_level || '').replace(/^A/i, ''))}</small>
      <dl class="ctox-fields">
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
        <button type="button" class="ctox-button" data-ticket-action="${escapeAttr(action.id)}" data-case-id="${escapeAttr(item.case_id)}">
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
  if (!state.ctx.commandBus?.dispatch) {
    throw new Error(state.t('commandUnavailable', 'Ticket-Aktionen sind gerade nicht verfügbar.'));
  }
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
  const accepted = await state.ctx.commandBus.dispatch(command, { until: 'accepted' });
  if (accepted?.status === 'failed') {
    throw new Error(commandFailureMessage(accepted, commandId));
  }
  setCommandStatus(state.t('commandDone', 'Befehl abgeschlossen.'));
  await refreshTickets();
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

function withTimeout(promise, timeoutMs, message) {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), timeoutMs);
    Promise.resolve(promise)
      .then((value) => {
        window.clearTimeout(timer);
        resolve(value);
      })
      .catch((error) => {
        window.clearTimeout(timer);
        reject(error);
      });
  });
}

function renderSelfWork(item) {
  const notes = state.data.ctox_ticket_self_work_notes.filter((note) => note.work_id === item.work_id);
  return `
    <article class="tickets-context-item" ${recordContextAttrs({
      type: 'ticket_self_work',
      id: item.work_id || item.id,
      label: item.title || item.work_id,
      submodule: 'self-work',
    })}>
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
    <article class="tickets-context-item" ${recordContextAttrs({
      type: 'ticket_clarification',
      id: item.clarification_id || item.id,
      label: item.question || item.clarification_id,
      submodule: 'clarifications',
    })}>
      <span>${escapeHtml([item.status, item.target_type, item.target_channel].filter(Boolean).join(' · '))}</span>
      <strong>${escapeHtml(item.question || item.clarification_id)}</strong>
      <small>${escapeHtml(missing || item.unblock_criteria || item.outbound_message_key || '')}</small>
      ${item.inbound_response_body ? `<p class="tickets-note">${escapeHtml(item.inbound_response_body)}</p>` : ''}
      ${canPublish || canResolve ? `
        <div class="tickets-action-row">
          ${canPublish ? `
            <button type="button" class="ctox-button"
              data-clarification-action="publish"
              data-clarification-id="${escapeAttr(item.clarification_id)}">
              ${escapeHtml(state.t('publishClarification', 'Geprüft senden'))}
            </button>
          ` : ''}
          ${canResolve ? `
          <button type="button" class="ctox-button"
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
    <article class="tickets-context-item is-compact" ${recordContextAttrs({
      type: 'ticket_control_bundle',
      id: item.runbook_id || item.id,
      label: item.label || item.runbook_id,
      submodule: 'runbooks',
    })}>
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
  return `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(String(value))}</dd>`;
}

function renderEmptyState(title, body = '', modifier = '') {
  return `
    <div class="tickets-empty ${escapeAttr(modifier)}">
      <strong>${escapeHtml(title)}</strong>
      ${body ? `<span>${escapeHtml(body)}</span>` : ''}
    </div>
  `;
}

function renderTicketLoadingState(modifier = '') {
  if (state.loading) {
    return renderEmptyState(
      state.t('loadingTickets', 'Tickets werden geladen...'),
      state.t('loadingTicketsDetail', 'Die Ticket-Projektionen werden vorbereitet.'),
      `${modifier} is-loading`.trim(),
    );
  }
  return renderEmptyState(
    state.t('syncingTickets', 'Tickets werden synchronisiert.'),
    state.t('syncingTicketsDetail', 'Die Ticketdaten werden gerade aus dem CTOX-Datenstrom geladen.'),
    `${modifier} is-loading`.trim(),
  );
}

function applyTicketContext(element, ticket, submodule) {
  applyRecordContext(element, {
    type: 'ticket',
    id: ticketRecordId(ticket),
    label: ticketRecordLabel(ticket),
    submodule,
  });
}

function applyRecordContext(element, context) {
  if (!element) return;
  const attrs = recordContextObject(context);
  for (const [name, value] of Object.entries(attrs)) {
    if (value) {
      element.setAttribute(name, value);
    } else {
      element.removeAttribute(name);
    }
  }
}

function clearRecordContext(element) {
  if (!element) return;
  for (const name of Object.keys(recordContextObject({}))) {
    element.removeAttribute(name);
  }
}

function ticketContextAttrs(ticket, submodule) {
  return recordContextAttrs({
    type: 'ticket',
    id: ticketRecordId(ticket),
    label: ticketRecordLabel(ticket),
    submodule,
  });
}

function recordContextAttrs(context) {
  return Object.entries(recordContextObject(context))
    .filter(([, value]) => value)
    .map(([name, value]) => `${name}="${escapeAttr(value)}"`)
    .join('\n        ');
}

function recordContextObject(context = {}) {
  return {
    'data-context-module': 'tickets',
    'data-context-submodule': context.submodule || '',
    'data-context-record-type': context.type || '',
    'data-context-record-id': context.id || '',
    'data-context-label': context.label || '',
    'data-record-type': context.type || '',
    'data-record-id': context.id || '',
    'data-label': context.label || '',
  };
}

function ticketRecordId(ticket) {
  return ticket?.ticket_key || ticket?.id || '';
}

function ticketRecordLabel(ticket) {
  return ticket?.title || ticket?.ticket_key || ticket?.id || 'Ticket';
}

function statusBadgeClass(value) {
  const status = String(value || '').toLowerCase();
  if (/closed|done|completed|resolved|verified/.test(status)) return 'is-success';
  if (/blocked|failed|rejected|error/.test(status)) return 'is-danger';
  if (/pending|waiting|clarification/.test(status)) return 'is-warning';
  return '';
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
  isCollectionDiagnosticsReady,
  ticketRecordContextForSmoke: (ticket) => recordContextObject({
    type: 'ticket',
    id: ticketRecordId(ticket),
    label: ticketRecordLabel(ticket),
    submodule: 'inbox',
  }),
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
