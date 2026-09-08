import { loadModuleMessages } from '../../shared/i18n.js';
import { showBusinessPrompt } from '../../shared/dialogs.js?v=20260816-browser-sync-guards-v141';
import { crewCreatureHtml } from '../../shared/business-chat.js?v=20260906-crew-home-v339';
import { canUseBusinessPermission, BusinessOsPermissions } from '../../shared/permissions.js?v=20260816-browser-sync-guards-v141';

const REFRESH_DEBOUNCE_MS = 80;
const TICKET_PRIMARY_COLLECTION = 'ctox_ticket_items';

const labels = {
  de: {
    kicker: "Crew",
    listTitle: 'Tickets',
    createTicket: 'Ticket anlegen',
    import: 'Importieren',
    export: 'Exportieren',
    imported: 'Importiert',
    importInvalid: 'Ungültige JSON-Datei.',
    importEmpty: 'Keine Datensätze in der Datei.',
    search: 'Suchen...',
    showAsList: 'Als Liste anzeigen',
    showAsCards: 'Als Karten anzeigen',
    allStatus: 'Alle Status',
    open: 'Offen',
    pending: "Wartet",
    blocked: 'Blockiert',
    closed: 'Geschlossen',
    bandAll: 'Alle',
    bandOpen: 'Offen',
    bandPending: 'Wartend',
    bandClosed: 'Geschlossen',
    entries: 'Einträge',
    openOps: "Aktionen einblenden",
    closeOps: "Aktionen ausblenden",
    operations: "Aktionen",
    loadingTickets: 'Tickets werden geladen...',
    loadingTicketsDetail: "Die Ticketdaten werden geladen.",
    syncingTickets: 'Tickets werden synchronisiert.',
    syncingTicketsDetail: "Die Ticketdaten werden aktualisiert.",
    noTickets: 'Noch keine Tickets verfügbar.',
    noTicketsDetail: "Neue Tickets erscheinen hier, sobald sie vorliegen.",
    noTicketsFiltered: 'Kein Ticket passt zum Filter.',
    selectTicket: 'Wähle links ein Ticket aus.',
    selectTicketDetail: "Verlauf, Nachweise und Aktionen erscheinen danach hier.",
    timeline: "Verlauf",
    evidence: 'Nachweise',
    verification: "Prüfung",
    writebacks: "Übertragene Ergebnisse",
    cases: "Vorgänge",
    selfWork: "Arbeit der Crew",
    approvals: "Freigaben",
    source: 'Quelle',
    requester: "Anfragende Person",
    priority: 'Priorität',
    updated: 'Aktualisiert',
    noEvents: "Noch keine Ereignisse.",
    noCase: "Kein Vorgang für dieses Ticket.",
    noSelfWork: "Noch kein Crew-Auftrag verknüpft.",
    clarifications: 'Rückfragen',
    noClarifications: 'Keine offenen Rückfragen.',
    approve: "Freigeben",
    reject: "Ablehnen",
    execute: "Ausführen",
    verify: "Prüfen",
    internalNote: 'Interne Notiz',
    publicReply: 'Antwort',
    close: "Schließen",
    requestClarification: 'Rückfrage',
    publishClarification: 'Geprüft senden',
    resolveClarification: 'Antwort erfassen',
    runbooks: "Arbeitsanleitungen",
    noRunbooks: "Keine Arbeitsanleitungen.",
    promptQuestion: 'Rückfrage',
    promptMissingInputs: 'Fehlende Werte (kommagetrennt)',
    promptReviewSummary: 'Prüfnotiz',
    promptResponseKey: 'Antwort-Referenz',
    promptResponseBody: 'Antwortinhalt',
    commandPending: "Aktion wird ausgeführt…",
    commandDone: "Aktion abgeschlossen.",
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
    kicker: "Crew",
    listTitle: 'Tickets',
    createTicket: 'Create ticket',
    import: 'Import',
    export: 'Export',
    imported: 'Imported',
    importInvalid: 'Invalid JSON file.',
    importEmpty: 'No records in the file.',
    search: 'Search...',
    showAsList: 'Show as list',
    showAsCards: 'Show as cards',
    allStatus: 'All status',
    open: 'Open',
    pending: 'Pending',
    blocked: 'Blocked',
    closed: 'Closed',
    bandAll: 'All',
    bandOpen: 'Open',
    bandPending: 'Wartend',
    bandClosed: 'Closed',
    entries: 'entries',
    entryOne: 'entry',
    openOps: 'Show operations',
    closeOps: 'Hide operations',
    operations: 'Operations',
    loadingTickets: 'Loading tickets...',
    loadingTicketsDetail: "Loading ticket data.",
    syncingTickets: 'Syncing tickets.',
    syncingTicketsDetail: "Updating ticket data.",
    noTickets: 'No tickets available yet.',
    noTicketsDetail: "New tickets appear here as they become available.",
    noTicketsFiltered: 'No ticket matches the filter.',
    selectTicket: 'Select a ticket on the left.',
    selectTicketDetail: 'Timeline, evidence, and operations appear here after selection.',
    timeline: 'Timeline',
    evidence: 'Evidence',
    verification: 'Verification',
    writebacks: "Transferred results",
    cases: 'Cases',
    selfWork: "Crew work",
    approvals: 'Approvals',
    source: 'Source',
    requester: 'Requester',
    priority: 'Priority',
    updated: 'Updated',
    noEvents: 'No events available.',
    noCase: 'No case for this ticket.',
    noSelfWork: "No crew task linked yet.",
    clarifications: 'Clarifications',
    noClarifications: 'No open clarifications.',
    approve: 'Approve',
    reject: 'Reject',
    execute: 'Execute',
    verify: 'Verify',
    internalNote: 'Internal note',
    publicReply: 'Reply',
    close: 'Close',
    requestClarification: 'Clarify',
    publishClarification: 'Send reviewed',
    resolveClarification: 'Record answer',
    runbooks: 'Runbooks',
    noRunbooks: "No work instructions.",
    promptQuestion: 'Clarification question',
    promptMissingInputs: 'Missing values (comma-separated)',
    promptReviewSummary: 'Review note',
    promptResponseKey: 'Answer reference',
    promptResponseBody: 'Answer body',
    commandPending: "Action in progress…",
    commandDone: "Action completed.",
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

// Icon glyphs (shared style: viewBox 0 0 24 24, stroke currentColor 1.8, round
// caps). Kept inline so lifecycle/element actions render as collected icons.
const ICON = {
  approve: '<path d="M20 6L9 17l-5-5"/>',
  reject: '<path d="M6 6l12 12M18 6L6 18"/>',
  execute: '<path d="M8 5.5v13l11-6.5z"/>',
  verify: '<path d="M12 3l7 4v5c0 4-3 7-7 8-4-1-7-4-7-8V7z"/><path d="M9 12l2 2 4-4"/>',
  clarify: '<circle cx="12" cy="12" r="9"/><path d="M9.6 9.2a2.4 2.4 0 1 1 3 2.4c-.8.3-1.6 1-1.6 2M12 17h.01"/>',
  note: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/>',
  reply: '<path d="M9 17l-5-5 5-5"/><path d="M4 12h11a5 5 0 0 1 5 5v1"/>',
  close: '<rect x="3" y="4" width="18" height="4" rx="1"/><path d="M5 8v11a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8"/><path d="M10 12h4"/>',
  publish: '<path d="M22 2L11 13"/><path d="M22 2l-7 20-4-9-9-4z"/>',
  resolve: '<path d="M21 15a2 2 0 0 1-2 2H8l-4 4V5a2 2 0 0 1 2-2h13a2 2 0 0 1 2 2z"/><path d="M8.5 10.5l2 2 4-4"/>',
  ops: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/>',
  collapseOps: '<path d="M9 6l6 6-6 6"/>',
};

const state = {
  ctx: null,
  lang: 'de',
  t: (key, fallback) => fallback || key,
  selectedId: '',
  search: '',
  status: 'all',
  band: 'all',
  view: 'cards',
  // On-demand operations pane: 'auto' shows it only when an operation flow needs
  // it, 'open' pins it open, 'closed' pins it hidden. Reset to 'auto' on select.
  opsMode: 'auto',
  renderTimer: null,
  cleanup: null,
  loading: false,
  ticketReadiness: null,
  data: Object.fromEntries(collectionNames.map((name) => [name, []])),
};

// ---------------------------------------------------------------------------
// Pure helpers (exported for tests — no DOM, no RxDB).
// ---------------------------------------------------------------------------

// Which counted band a ticket's remote status belongs to.
export function ticketBandOf(status) {
  const text = String(status || '').toLowerCase();
  if (/closed|done|completed|resolved|verified/.test(text)) return 'closed';
  if (/pending|waiting|clarification/.test(text)) return 'pending';
  return 'open';
}

// Tray status filter (all/open/pending/blocked/closed) — composes with the band.
export function matchesTicketStatusFilter(status, filter) {
  if (!filter || filter === 'all') return true;
  const text = String(status || '').toLowerCase();
  if (filter === 'closed') return /closed|done|completed|resolved|verified/.test(text);
  if (filter === 'pending') return /pending|waiting|clarification/.test(text);
  if (filter === 'blocked') return /blocked|failed|rejected|error/.test(text);
  if (filter === 'open') return !/closed|done|completed|resolved/.test(text);
  return text.includes(filter);
}

export function filterTicketRows(rows, { band = 'all', status = 'all', search = '' } = {}) {
  const needle = String(search || '').trim().toLowerCase();
  return (Array.isArray(rows) ? rows : []).filter((ticket) => {
    const remote = String(ticket.remote_status || '');
    if (band !== 'all' && ticketBandOf(remote) !== band) return false;
    if (!matchesTicketStatusFilter(remote, status)) return false;
    if (!needle) return true;
    const hay = [
      ticket.ticket_key,
      ticket.title,
      ticket.body_text,
      ticket.requester,
      ticket.priority,
      ticket.source_system,
      remote,
    ].join(' ').toLowerCase();
    return hay.includes(needle);
  });
}

// Counted band tallies (zeros included) over an already search/status-filtered
// row set — the band itself is not applied here.
export function countsForTickets(rows) {
  const list = Array.isArray(rows) ? rows : [];
  const counts = { all: list.length, open: 0, pending: 0, closed: 0 };
  for (const ticket of list) counts[ticketBandOf(ticket.remote_status)] += 1;
  return counts;
}

// The operations pane auto-reveals only when a flow needs it: an open
// clarification, or a case awaiting approval.
export function ticketOpsFlowActive(cases, clarifications) {
  const openClarification = (Array.isArray(clarifications) ? clarifications : [])
    .some((item) => !['resolved', 'cancelled'].includes(String(item.status || '').toLowerCase()));
  const pendingCase = (Array.isArray(cases) ? cases : [])
    .some((item) => ['approval_pending', 'needs_approval', 'pending_approval'].includes(String(item.state || '').toLowerCase()));
  return Boolean(openClarification || pendingCase);
}

// Resolve the on-demand operations pane visibility from its mode + flow state.
export function resolveOpsVisible(opsMode, flowActive) {
  if (opsMode === 'open') return true;
  if (opsMode === 'closed') return false;
  return Boolean(flowActive);
}

// Two densities, not two paddings (Betreiber-Direktive 31.08.):
//   KARTEN — bold title + status badge over two muted meta lines carrying the
//            ticket's real detail fields (key/source, assignment/updated).
//   LISTE  — exactly ONE line: title plus the status badge as the single short
//            meta on the right.
// Both stay pure selectors: no inline expansion in either view.
// `rows` are shaped ({ id, key, title, status, source, subtitle, updated }).
export function ticketRowHtml(row, opts = {}) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selected = Boolean(opts.selected);
  const badge = '<span class="ctox-badge ' + statusBadgeClass(row.status) + '">' + escapeHtml(displayStatus(row.status || 'open')) + '</span>';
  const attrs = ' class="ctox-list-item ticket-row ticket-row--' + view + (selected ? ' is-selected' : '') + '"'
    + ' role="button" tabindex="0" aria-selected="' + (selected ? 'true' : 'false') + '"'
    + ' data-context-module="tickets" data-context-submodule="inbox"'
    + ' data-context-record-type="ticket" data-context-record-id="' + escapeAttr(row.id) + '"'
    + ' data-context-label="' + escapeAttr(row.title || row.key || row.id) + '"'
    + ' data-record-type="ticket" data-record-id="' + escapeAttr(row.id) + '" data-label="' + escapeAttr(row.title || row.key || row.id) + '"';
  const crew = row.crew && row.crew.html
    ? '<span class="ctox-flow-creature-shell ticket-row-crew" title="' + escapeAttr([row.crew.name, row.crew.sentence].filter(Boolean).join(' · ')) + '">' + row.crew.html + '</span>'
    : '';
  if (view === 'list') {
    return '<div' + attrs + '>' + crew + '<span class="ticket-row-title">' + escapeHtml(row.title || row.key || 'Ticket') + '</span>' + badge + '</div>';
  }
  const metaTop = [row.key, row.source || 'ctox'].filter(Boolean).map(escapeHtml).join(' · ');
  const metaSub = [row.crew ? [row.crew.name, row.crew.sentence].filter(Boolean).join(' · ') : '', row.subtitle, row.updated].filter(Boolean).map(escapeHtml).join(' · ');
  return '<div' + attrs + '>'
    + '<div class="ticket-row-head">' + crew + '<span class="ticket-row-title">' + escapeHtml(row.title || row.key || 'Ticket') + '</span>' + badge + '</div>'
    + (metaTop ? '<div class="ticket-row-meta">' + metaTop + '</div>' : '')
    + (metaSub ? '<div class="ticket-row-meta ticket-row-meta--sub">' + metaSub + '</div>' : '')
    + '</div>';
}

export function renderTicketList(rows, opts = {}) {
  const list = Array.isArray(rows) ? rows : [];
  if (!list.length) {
    return '<div class="ctox-empty"><strong>' + escapeHtml(opts.emptyText || 'Noch keine Einträge.') + '</strong></div>';
  }
  return list.map((row) => ticketRowHtml(row, { view: opts.view, selected: row.id && row.id === opts.selectedId })).join('');
}

export function resolveTicketListState({ loading = false, sourceCount = 0, readiness = null } = {}) {
  if (loading) return 'loading';
  if (Number(sourceCount) > 0) return 'content';
  return readiness?.ready === false ? 'syncing' : 'empty';
}

// ---------------------------------------------------------------------------
// Mount
// ---------------------------------------------------------------------------

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  const messages = await loadModuleMessages(import.meta.url, state.lang, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;
  state.selectedId = '';
  state.opsMode = 'auto';
  await ensureStyles();
  const markupVersion = String(import.meta.url).split('?v=')[1] || '';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (markupVersion ? `?v=${markupVersion}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
  ctx.host.innerHTML = html;
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();
  state.loading = true;
  state.ticketReadiness = readPrimaryTicketReadiness();
  applyStaticLabels();
  seedGrammarState();
  wireUi();
  const stopReadiness = wireTicketReadiness();
  state.cleanup = stopReadiness;
  render();
  await refreshTickets();
  const stopRealtime = wireRealtime();
  state.cleanup = () => {
    stopReadiness();
    stopRealtime();
  };
  return () => {
    state.cleanup?.();
    if (state.renderTimer) window.clearTimeout(state.renderTimer);
  };
}

async function ensureStyles() {
  const cssVersion = String(import.meta.url).split('?v=')[1] || '';
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  let link = document.querySelector('link[data-tickets-style]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'stylesheet';
    link.dataset.ticketsStyle = 'true';
    document.head.append(link);
  }
  if (link.getAttribute('href') !== cssHref) link.href = cssHref;
}

function root() {
  return state.ctx.host.querySelector('[data-tickets-root]');
}

function applyStaticLabels() {
  const el = root();
  if (!el) return;
  el.querySelectorAll('[data-copy]').forEach((node) => {
    const value = state.t(node.dataset.copy, node.textContent);
    if (value) node.textContent = value;
  });
  const createButton = el.querySelector('[data-action="new"]');
  if (createButton) {
    const createLabel = state.t('createTicket', 'Ticket anlegen');
    createButton.setAttribute('aria-label', createLabel);
    createButton.setAttribute('title', createLabel);
  }
  const search = el.querySelector('[data-pg-search]');
  if (search) search.placeholder = state.t('search', 'Suchen...');
  syncViewToggle();
}

// Seed the cached grammar state from the DOM before the shell fires its first
// change event (the shell wires the pane asynchronously after mount).
function seedGrammarState() {
  const el = root();
  if (!el) return;
  state.search = (el.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase();
  // ONE toggle button: data-pg-view carries the CURRENT view (no aria-pressed).
  state.view = el.querySelector('[data-tickets-view-toggle]')?.dataset.pgView === 'list' ? 'list' : 'cards';
  state.band = el.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all';
  state.status = el.querySelector('[data-pg-filter][data-pg-name="status"]')?.value || 'all';
}

function wireUi() {
  const el = root();
  if (!el) return;
  el.addEventListener('click', onRootClick);
  // One button, one action. Capture phase on the root runs BEFORE the
  // shell-wired grammar listener sitting on the button itself, so the grammar
  // reads the already-flipped data-pg-view and reports the new view exactly
  // once.
  el.addEventListener('click', onViewToggleCapture, true);
  el.addEventListener('keydown', onListKey);
  // The shell reports search / view / tray / band changes on this bubbling
  // event; re-render (with a list rebuild — an intentional reset).
  el.addEventListener('ctox-pane-grammar-change', onGrammarChange);
  syncViewToggle();
}

function onViewToggleCapture(event) {
  const target = event.target instanceof Element ? event.target : null;
  const button = target?.closest('[data-tickets-view-toggle]');
  if (!button || !root()?.contains(button)) return;
  button.dataset.pgView = button.dataset.pgView === 'list' ? 'cards' : 'list';
  syncViewToggle();
}

// The shard/list switch is ONE button and therefore an action, not a state:
// icon, aria-label and title name the view the next click produces. The shared
// grammar wiring stamps aria-pressed on every [data-pg-view] button it sees;
// for a single-button switch that attribute would claim a toggle state the
// control does not have, so it is stripped again here.
function syncViewToggle() {
  const button = root()?.querySelector('[data-tickets-view-toggle]');
  if (!button) return;
  const t = typeof state.t === 'function' ? state.t : (_key, fallback) => fallback;
  const goesToList = button.dataset.pgView !== 'list';
  const label = goesToList
    ? t('showAsList', 'Als Liste anzeigen')
    : t('showAsCards', 'Als Karten anzeigen');
  button.setAttribute('aria-label', label);
  button.title = label;
  button.removeAttribute('aria-pressed');
}

function onGrammarChange(event) {
  const detail = event?.detail || {};
  state.search = String(detail.search ?? state.search ?? '').trim().toLowerCase();
  state.view = detail.view || state.view || 'cards';
  state.band = detail.band || 'all';
  state.status = (detail.filters && detail.filters.status) || 'all';
  // The grammar stamps aria-pressed on the single toggle; strip it again and
  // keep icon/label pointing at the view the next click produces.
  syncViewToggle();
  syncSelectionToVisible();
  render();
}

function onRootClick(event) {
  const target = event.target instanceof Element ? event.target : null;
  if (!target) return;
  const clarification = target.closest('[data-clarification-action]');
  if (clarification) {
    runClarificationAction(clarification).catch((error) => setCommandStatus(error?.message || String(error), true));
    return;
  }
  const caseAction = target.closest('[data-ticket-action]');
  if (caseAction) {
    runCaseAction(caseAction).catch((error) => setCommandStatus(error?.message || String(error), true));
    return;
  }
  const action = target.closest('[data-action]');
  if (action) {
    onAction(action.dataset.action);
    return;
  }
  const list = state.ctx.host.querySelector('[data-ticket-list]');
  const row = target.closest('[data-context-record-id]');
  if (row && list && list.contains(row)) {
    selectRecord(row.getAttribute('data-context-record-id') || '');
  }
}

function onListKey(event) {
  if (event.key !== 'Enter' && event.key !== ' ') return;
  const target = event.target instanceof Element ? event.target : null;
  const list = state.ctx.host.querySelector('[data-ticket-list]');
  const row = target?.closest('[data-context-record-id]');
  if (!row || !list || !list.contains(row)) return;
  event.preventDefault();
  selectRecord(row.getAttribute('data-context-record-id') || '');
}

function onAction(action) {
  if (action === 'new') {
    createLocalTicket().catch((error) => setCommandStatus(error?.message || String(error), true));
  } else if (action === 'import') {
    importTickets();
  } else if (action === 'export') {
    exportTickets();
  } else if (action === 'toggle-ops') {
    state.opsMode = state.opsMode === 'open' ? 'closed' : 'open';
    applyOpsVisibility();
  } else if (action === 'close-ops') {
    state.opsMode = 'closed';
    applyOpsVisibility();
  }
}

function ticketCollection(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

function wireRealtime() {
  const subscriptions = [...collectionNames, 'ctox_queue_tasks', 'ctox_crew_members']
    .map((name) => ticketCollection(name)?.$?.subscribe?.(() => scheduleRefresh()))
    .filter(Boolean);
  return () => subscriptions.forEach((sub) => {
    try { sub.unsubscribe?.(); } catch {}
  });
}

function readPrimaryTicketReadiness() {
  const read = state.ctx?.sync?.collectionReadiness;
  return typeof read === 'function'
    ? read.call(state.ctx.sync, TICKET_PRIMARY_COLLECTION)
    : null;
}

function wireTicketReadiness() {
  const subscribe = state.ctx?.sync?.subscribeCollectionReadiness;
  if (typeof subscribe !== 'function') return () => {};
  const unsubscribe = subscribe.call(state.ctx.sync, TICKET_PRIMARY_COLLECTION, (snapshot) => {
    state.ticketReadiness = snapshot;
    render();
  });
  return typeof unsubscribe === 'function' ? unsubscribe : () => {};
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
  state.crew = await loadCrewForTickets();
  state.loading = false;
  syncSelectionToVisible();
  render();
}

// --- Crew on tickets: the member holding a ticket's queue task -----------------
// Ticket-born work reaches the harness as a queue task carrying `ticket_key`;
// the task's `crew_member_id` is the member at work, its routing fields say
// why it waits. Both reads are bounded; nothing here is a second data path.
async function loadCrewForTickets() {
  const tasks = ticketCollection('ctox_queue_tasks');
  const members = ticketCollection('ctox_crew_members');
  const readBounded = async (collection, selector, limit) => {
    if (!collection) return [];
    try {
      const docs = await collection.find({ selector, limit }).exec();
      return docs.map((doc) => doc.toJSON());
    } catch {
      return [];
    }
  };
  const [taskDocs, memberDocs] = await Promise.all([
    readBounded(tasks, { ticket_key: { $gt: '' } }, 200),
    readBounded(members, { archived: false }, 64),
  ]);
  return {
    tasks: taskDocs.filter((doc) => doc && doc.ticket_key),
    members: memberDocs,
  };
}

function taskForTicket(ticketKey) {
  const key = String(ticketKey || '').trim();
  if (!key) return null;
  const tasks = (state.crew?.tasks || []).filter((task) => task.ticket_key === key);
  tasks.sort((left, right) => Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0));
  return tasks[0] || null;
}

function crewMemberFor(task) {
  const id = String(task?.crew_member_id || task?.crew_assigned_member_id || '').trim();
  return id ? (state.crew?.members || []).find((member) => member.id === id) || null : null;
}

function crewTaskState(task) {
  const status = String(task?.route_status || task?.status || '').toLowerCase();
  if (['leased', 'running', 'processing'].includes(status)) return 'running';
  if (['failed', 'error', 'cancelled', 'canceled'].includes(status)) return 'failed';
  if (['handled', 'completed', 'done', 'success'].includes(status)) return 'success';
  return 'queued';
}

function crewWaitSentence(task) {
  if (!task) return '';
  const t = state.t;
  const status = String(task.route_status || task.status || '').toLowerCase();
  const hold = String(task.hold_reason || '');
  const holdText = {
    technical: t('holdTechnical', 'technischer Halt'),
    missing_review_evidence: t('holdMissingReviewEvidence', 'Review-Nachweis fehlt'),
    missing_artifact: t('holdMissingArtifact', 'Ergebnis fehlt'),
    waiting_external: t('holdWaitingExternal', 'wartet auf Rückmeldung'),
    aborted_by_owner: t('holdAbortedByOwner', 'vom Owner abgebrochen'),
  }[hold] || (hold ? hold.replace(/[_:]+/g, ' ') : '');
  if (status === 'failed') return `${t('crewFailed', 'gescheitert')}${holdText ? ` · ${holdText}` : ''}`;
  if (status === 'blocked') {
    const waits = task.wait_entity_id ? ` ${t('waitsFor', 'wartet auf')} ${task.wait_entity_type || ''} ${task.wait_entity_id}`.replace(/\s+/g, ' ') : '';
    return `${t('crewBlocked', 'blockiert')}${holdText ? ` · ${holdText}` : ''}${waits}`;
  }
  if (task.retry_not_before) {
    const at = new Date(task.retry_not_before);
    return `${t('retryAt', 'Wiederholung um')} ${Number.isFinite(at.getTime()) ? at.toLocaleTimeString(state.lang === 'en' ? 'en-GB' : 'de-DE', { hour: '2-digit', minute: '2-digit' }) : ''}`.trim();
  }
  if (status === 'leased' || status === 'running') return t('crewWorking', 'im Einsatz');
  if (status === 'pending' || status === 'queued') return t('crewQueued', 'wartet in der Queue');
  return displayStatus(status || 'open');
}

function crewCreatureFor(task, placement = 'map') {
  const member = crewMemberFor(task);
  return crewCreatureHtml({
    crewKey: member ? member.id : (task?.command_id || task?.id || 'crew'),
    crewIdentity: member ? { name: member.name, shape: member.shape, color: member.color } : null,
    executionProgress: task?.execution_progress || null,
  }, crewTaskState(task), placement);
}

function mayAssignCrew() {
  return canUseBusinessPermission({
    session: state.ctx?.session,
    governance: state.ctx?.governance,
    permission: BusinessOsPermissions.CrewManage,
    scopeType: 'record',
    scopeId: 'ctox.crew.assign',
  });
}

function crewCardHtml(ticket) {
  const task = taskForTicket(ticket.ticket_key);
  if (!task) return '';
  const member = crewMemberFor(task);
  const t = state.t;
  const status = String(task.route_status || task.status || '').toLowerCase();
  const assignable = mayAssignCrew() && !task.lease_owner && ['pending', 'queued', 'blocked'].includes(status) && (state.crew?.members || []).length > 0;
  const taskId = String(task.message_key || task.task_id || task.id || '').replace(/^queue-/, '');
  return `
    <section class="ctox-card tickets-crew-card">
      <header>${escapeHtml(t('crew', 'Crew'))}</header>
      <div class="ctox-card-body tickets-crew-body">
        <span class="ctox-flow-creature-shell tickets-crew-portrait">${crewCreatureFor(task)}</span>
        <div class="tickets-crew-facts">
          <strong>${escapeHtml(member ? member.name : t('crewUnassigned', 'Crew, noch niemand zugeordnet'))}</strong>
          <small>${escapeHtml(crewWaitSentence(task))}</small>
          ${assignable ? `
            <label class="tickets-crew-assign">
              <span>${escapeHtml(t('assignMember', 'Zuweisen'))}</span>
              <select class="ctox-select" data-crew-assign="${escapeAttr(taskId)}">
                <option value="">${escapeHtml(t('assignChoose', 'Mitglied wählen'))}</option>
                ${(state.crew.members || []).map((item) => `<option value="${escapeAttr(item.id)}" ${item.id === task.crew_assigned_member_id ? 'selected' : ''}>${escapeHtml(item.name)}</option>`).join('')}
              </select>
            </label>` : ''}
          <small data-crew-assign-status></small>
        </div>
      </div>
    </section>`;
}

async function assignCrewMember(taskId, memberId, statusNode) {
  try {
    await dispatchTicketCommand('ctox.crew.assign', taskId, { task_id: taskId, member_id: memberId });
    if (statusNode) statusNode.textContent = state.t('assigned', 'Zugewiesen.');
    await refreshTickets();
  } catch (error) {
    if (statusNode) statusNode.textContent = String(error?.message || error);
  }
}

async function loadCollection(name) {
  const collection = ticketCollection(name);
  if (!collection) return [];
  const docs = await collection.find().exec();
  return docs.map((doc) => doc.toJSON()).filter((doc) => !doc.is_deleted && !doc._deleted);
}

// ---------------------------------------------------------------------------
// Row shaping + selection
// ---------------------------------------------------------------------------

function sortedTickets() {
  return [...state.data.ctox_ticket_items]
    .sort((left, right) => Number(right.updated_at_ms || 0) - Number(left.updated_at_ms || 0));
}

function visibleTickets() {
  return filterTicketRows(sortedTickets(), { band: state.band, status: state.status, search: state.search });
}

// Search + status filtered (band ignored) — the basis for the band counts.
function scopedTickets() {
  return filterTicketRows(sortedTickets(), { band: 'all', status: state.status, search: state.search });
}

function shapeTicket(ticket) {
  const label = labelForTicket(ticket.ticket_key);
  const task = taskForTicket(ticket.ticket_key);
  return {
    id: ticket.id,
    key: ticket.ticket_key || ticket.id,
    crew: task ? { html: crewCreatureFor(task, 'map'), name: crewMemberFor(task)?.name || '', sentence: crewWaitSentence(task) } : null,
    title: ticket.title || ticket.ticket_key || 'Ticket',
    status: ticket.remote_status || 'open',
    source: ticket.source_system || 'ctox',
    subtitle: label?.label || ticket.priority || ticket.requester || '',
    // Card meta only — the list row stays one line and never paints it.
    updated: formatDate(ticket.updated_at || ticket.last_synced_at),
  };
}

function syncSelectionToVisible() {
  const visible = visibleTickets();
  if (!state.selectedId || !visible.some((ticket) => ticket.id === state.selectedId)) {
    state.selectedId = visible[0]?.id || '';
  }
}

function selectRecord(id) {
  if (!id) return;
  state.selectedId = id;
  // New ticket → operations pane returns to auto (reveals only if a flow needs
  // it). Selection is an in-place class flip, never a list rebuild.
  state.opsMode = 'auto';
  applyListSelection();
  renderDetail();
  renderOps();
  applyOpsVisibility();
}

function applyListSelection() {
  const list = state.ctx.host.querySelector('[data-ticket-list]');
  list?.querySelectorAll('[data-context-record-id]').forEach((rowEl) => {
    const on = (rowEl.getAttribute('data-context-record-id') || '') === String(state.selectedId || '');
    rowEl.classList.toggle('is-selected', on);
    rowEl.setAttribute('aria-selected', String(on));
  });
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

function render() {
  renderList();
  renderCountsAndFooter();
  renderDetail();
  renderOps();
  applyOpsVisibility();
}

function renderCountsAndFooter() {
  const el = root();
  const scoped = scopedTickets();
  const counts = countsForTickets(scoped);
  const pg = el?.querySelector('.tickets-left')?.__ctoxPaneGrammar;
  if (pg && typeof pg.setCounts === 'function') {
    pg.setCounts(counts);
  } else {
    for (const [key, value] of Object.entries(counts)) {
      const node = el?.querySelector(`[data-pg-count="${key}"]`);
      if (node) node.textContent = ` (${value})`;
    }
  }
  const scopeLabel = {
    all: state.t('bandAll', 'Alle'),
    open: state.t('bandOpen', 'Offen'),
    pending: state.t('bandPending', 'Pending'),
    closed: state.t('bandClosed', 'Geschlossen'),
  }[state.band] || state.t('bandAll', 'Alle');
  const visibleCount = visibleTickets().length;
  const footerText = `${visibleCount} ${visibleCount === 1 ? state.t('entryOne', 'Eintrag') : state.t('entries', 'Einträge')} · ${scopeLabel}`;
  if (pg && typeof pg.setFooter === 'function') {
    pg.setFooter(footerText);
  } else {
    const node = el?.querySelector('[data-pg-footer]');
    if (node) node.textContent = footerText;
  }
}

function renderList() {
  const list = state.ctx.host.querySelector('[data-ticket-list]');
  if (!list) return;
  const listState = resolveTicketListState({
    loading: state.loading,
    sourceCount: state.data.ctox_ticket_items.length,
    readiness: state.ticketReadiness,
  });
  if (listState === 'loading' || listState === 'syncing') {
    list.innerHTML = renderTicketLoadingState(listState);
    return;
  }
  const rows = visibleTickets().map(shapeTicket);
  const emptyText = state.data.ctox_ticket_items.length
    ? state.t('noTicketsFiltered', 'Kein Ticket passt zum Filter.')
    : state.t('noTickets', 'Noch keine Tickets verfügbar.');
  list.innerHTML = renderTicketList(rows, { view: state.view, selectedId: state.selectedId, emptyText });
  // Export ist nur mit Daten sinnvoll; der Knopf startet statisch deaktiviert
  // (data-empty-disabled in index.html) und folgt hier dem Datenbestand.
  const exportBtn = state.ctx.host.querySelector('[data-action="export"][data-empty-disabled]');
  if (exportBtn) exportBtn.disabled = state.data.ctox_ticket_items.length === 0;
}

function renderDetail() {
  const detail = state.ctx.host.querySelector('[data-ticket-detail]');
  if (!detail) return;
  // The pane head is static markup from index.html and must exist in every
  // state; this renderer only refines its texts/actions and owns the body.
  const headKicker = detail.querySelector('[data-detail-kicker]');
  const headTitle = detail.querySelector('[data-detail-title]');
  const headActions = detail.querySelector('[data-detail-actions]');
  const body = detail.querySelector('[data-ticket-detail-body]');
  if (!headKicker || !headTitle || !headActions || !body) return;
  const ticket = selectedTicket();
  if (!ticket) {
    clearRecordContext(detail);
    headKicker.textContent = state.t('kicker', 'Crew');
    headTitle.textContent = state.t('detailColumnTitle', 'Ticket');
    headActions.innerHTML = '';
    body.innerHTML = state.loading
      ? renderTicketLoadingState('loading')
      : (sortedTickets().length
        ? renderEmptyState(
          state.t('selectTicket', 'Wähle links ein Ticket aus.'),
          state.t('selectTicketDetail', 'Verlauf, Nachweise und Operationen erscheinen danach hier.'),
        )
        : renderEmptyState(
          state.t('noTicketsYet', 'Noch keine Tickets.'),
          state.t('noTicketsYetDetail', 'Neue Tickets erscheinen hier, sobald die Crew oder jemand aus dem Team eines anlegt.'),
        ));
    return;
  }
  applyTicketContext(detail, ticket, 'detail');
  const events = eventsForTicket(ticket.ticket_key);
  const primary = casesForTicket(ticket.ticket_key)[0] || null;
  const verifications = primary ? verificationsForCase(primary.case_id) : [];
  const writebacks = primary ? writebacksForCase(primary.case_id) : [];
  const opsOpen = resolveOpsVisible(state.opsMode, ticketFlowActive(ticket));
  const opsLabel = opsOpen ? state.t('closeOps', 'Operationen ausblenden') : state.t('openOps', 'Operationen einblenden');
  const evidenceHtml = (verifications.length || writebacks.length)
    ? `<section class="ctox-card">
        <header>${escapeHtml(state.t('evidence', 'Nachweise'))}</header>
        <div class="ctox-card-body">
          <dl class="ctox-fields">
            ${verifications.length ? fact(state.t('verification', 'Verification'), displayStatus(verifications[0].status || '')) : ''}
            ${writebacks.length ? fact(state.t('writebacks', 'Writebacks'), String(writebacks.length)) : ''}
          </dl>
        </div>
      </section>`
    : '';
  headKicker.textContent = ticket.ticket_key || ticket.id;
  headTitle.innerHTML = `${escapeHtml(ticket.title || ticket.ticket_key || 'Ticket')} <span class="ctox-badge ${statusBadgeClass(ticket.remote_status)}">${escapeHtml(displayStatus(ticket.remote_status || 'open'))}</span>`;
  headActions.innerHTML = `
    ${primary ? caseActionIconsHtml(primary) : ''}
    <button type="button" class="ctox-pane-icon${opsOpen ? ' is-active' : ''}" data-action="toggle-ops" aria-pressed="${opsOpen ? 'true' : 'false'}" aria-label="${escapeAttr(opsLabel)}" title="${escapeAttr(opsLabel)}">${iconSvg(ICON.ops)}</button>
  `;
  body.innerHTML = `
    <div class="ctox-pane-scroll tickets-detail-scroll os-scrollbar">
      <section class="ctox-card">
        <div class="ctox-card-body">
          <dl class="ctox-fields">
            ${fact(state.t('source', 'Quelle'), ticket.source_system)}
            ${fact(state.t('requester', 'Requester'), ticket.requester)}
            ${fact(state.t('priority', 'Priorität'), ticket.priority)}
            ${fact(state.t('updated', 'Aktualisiert'), formatDate(ticket.updated_at || ticket.last_synced_at))}
          </dl>
          ${ticket.body_text ? `<p class="tickets-body">${escapeHtml(ticket.body_text)}</p>` : ''}
        </div>
      </section>
      ${crewCardHtml(ticket)}
      <section class="ctox-card">
        <header>${escapeHtml(state.t('timeline', 'Timeline'))}</header>
        <div class="ctox-card-body">
          ${events.length ? `<ol class="ticket-timeline">${events.map(renderEvent).join('')}</ol>` : `<p class="tickets-inline-empty">${escapeHtml(state.t('noEvents', 'Keine Events vorhanden.'))}</p>`}
        </div>
      </section>
      ${evidenceHtml}
    </div>
  `;
  body.querySelector('[data-crew-assign]')?.addEventListener('change', (event) => {
    const select = event.currentTarget;
    const memberId = String(select.value || '');
    if (!memberId) return;
    select.setAttribute('disabled', 'disabled');
    assignCrewMember(select.dataset.crewAssign, memberId, body.querySelector('[data-crew-assign-status]'));
  });
}

function renderOps() {
  const ops = state.ctx.host.querySelector('[data-ticket-ops]');
  if (!ops) return;
  // Static pane head (index.html) exists in every state; this renderer only
  // refines its texts/actions and owns the body.
  const headKicker = ops.querySelector('[data-ops-kicker]');
  const headTitle = ops.querySelector('[data-ops-title]');
  const headActions = ops.querySelector('[data-ops-actions]');
  const body = ops.querySelector('[data-ticket-ops-body]');
  if (!headKicker || !headTitle || !headActions || !body) return;
  headKicker.textContent = state.t('operations', 'Operationen');
  const ticket = selectedTicket();
  if (!ticket) {
    clearRecordContext(ops);
    headTitle.textContent = state.t('detailColumnTitle', 'Ticket');
    headActions.innerHTML = '';
    body.innerHTML = '';
    return;
  }
  applyTicketContext(ops, ticket, 'operations');
  const cases = casesForTicket(ticket.ticket_key);
  const selfWork = selfWorkForTicket(ticket.ticket_key);
  const clarifications = clarificationsForTicket(ticket.ticket_key);
  const bundles = state.data.ctox_ticket_control_bundles;
  headTitle.textContent = ticket.ticket_key || ticket.id;
  headActions.innerHTML = `
    <button type="button" class="ctox-pane-icon" data-action="close-ops" aria-label="${escapeAttr(state.t('closeOps', 'Operationen ausblenden'))}" title="${escapeAttr(state.t('closeOps', 'Operationen ausblenden'))}">${iconSvg(ICON.collapseOps)}</button>
  `;
  body.innerHTML = `
    <div class="ctox-pane-scroll tickets-ops-scroll os-scrollbar">
      <section class="tickets-section">
        <h3 class="ctox-field-label">${escapeHtml(state.t('cases', 'Cases'))}</h3>
        ${cases.length ? cases.map(renderCase).join('') : `<p class="tickets-inline-empty">${escapeHtml(state.t('noCase', 'Kein Case für dieses Ticket.'))}</p>`}
      </section>
      <section class="tickets-section">
        <h3 class="ctox-field-label">${escapeHtml(state.t('selfWork', 'Self-work'))}</h3>
        ${selfWork.length ? selfWork.map(renderSelfWork).join('') : `<p class="tickets-inline-empty">${escapeHtml(state.t('noSelfWork', 'Kein Self-work verknüpft.'))}</p>`}
      </section>
      <section class="tickets-section">
        <h3 class="ctox-field-label">${escapeHtml(state.t('clarifications', 'Rückfragen'))}</h3>
        ${clarifications.length ? clarifications.map(renderClarification).join('') : `<p class="tickets-inline-empty">${escapeHtml(state.t('noClarifications', 'Keine offenen Rückfragen.'))}</p>`}
      </section>
      <section class="tickets-section">
        <h3 class="ctox-field-label">${escapeHtml(state.t('runbooks', 'Runbooks'))}</h3>
        ${bundles.length ? bundles.slice(0, 8).map(renderBundle).join('') : `<p class="tickets-inline-empty">${escapeHtml(state.t('noRunbooks', 'Keine Control Bundles.'))}</p>`}
      </section>
    </div>
  `;
}

function ticketFlowActive(ticket) {
  if (!ticket) return false;
  return ticketOpsFlowActive(casesForTicket(ticket.ticket_key), clarificationsForTicket(ticket.ticket_key));
}

function applyOpsVisibility() {
  const el = root();
  if (!el) return;
  const ticket = selectedTicket();
  const open = Boolean(ticket) && resolveOpsVisible(state.opsMode, ticketFlowActive(ticket));
  el.classList.toggle('is-ops-hidden', !open);
  const toggle = el.querySelector('[data-action="toggle-ops"]');
  if (toggle) {
    toggle.setAttribute('aria-pressed', open ? 'true' : 'false');
    toggle.classList.toggle('is-active', open);
    const label = open ? state.t('closeOps', 'Operationen ausblenden') : state.t('openOps', 'Operationen einblenden');
    toggle.setAttribute('aria-label', label);
    toggle.setAttribute('title', label);
  }
}

// Collected lifecycle action icons for a case (approve/reject/execute/verify/
// clarify/note/reply/close), state-gated exactly as actionsForCase.
function caseActionIconsHtml(item) {
  return actionsForCase(item).map((action) => (
    `<button type="button" class="ctox-pane-icon" data-ticket-action="${escapeAttr(action.id)}" data-case-id="${escapeAttr(item.case_id)}" aria-label="${escapeAttr(action.label)}" title="${escapeAttr(action.label)}">${iconSvg(ICON[action.icon] || ICON.execute)}</button>`
  )).join('');
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
  const verifications = verificationsForCase(item.case_id);
  const writebacks = writebacksForCase(item.case_id);
  const clarifications = state.data.ctox_ticket_clarification_requests.filter((clarification) => clarification.case_id === item.case_id);
  const actions = caseActionIconsHtml(item);
  return `
    <article class="ctox-card" ${recordContextAttrs({
    type: 'ticket_case',
    id: item.case_id || item.id,
    label: item.label || item.case_id,
    submodule: 'cases',
  })}>
      <header>
        <span>${escapeHtml([item.state || 'case', item.risk_level].filter(Boolean).join(' · '))}</span>
        ${actions ? `<span class="tickets-card-actions">${actions}</span>` : ''}
      </header>
      <div class="ctox-card-body">
        <strong class="tickets-context-item-title">${escapeHtml(item.label || item.case_id)}</strong>
        <p class="tickets-context-item-meta">${escapeHtml(item.approval_mode || '')} · A${escapeHtml(String(item.autonomy_level || '').replace(/^A/i, ''))}</p>
        <dl class="ctox-fields">
          ${fact(state.t('approvals', 'Approvals'), String(approvals.length))}
          ${fact(state.t('verification', 'Verification'), verifications[0]?.status || '')}
          ${fact(state.t('writebacks', 'Writebacks'), String(writebacks.length))}
          ${fact(state.t('clarifications', 'Rückfragen'), String(clarifications.length))}
        </dl>
      </div>
    </article>
  `;
}

function actionsForCase(item) {
  const stateValue = String(item.state || '').toLowerCase();
  const actions = [];
  if (['approval_pending', 'needs_approval', 'pending_approval'].includes(stateValue)) {
    actions.push(
      { id: 'approve', label: state.t('approve', 'Approve'), icon: 'approve' },
      { id: 'reject', label: state.t('reject', 'Reject'), icon: 'reject' },
    );
  }
  if (stateValue === 'executable') actions.push({ id: 'execute', label: state.t('execute', 'Execute'), icon: 'execute' });
  if (stateValue === 'executing') actions.push({ id: 'verify', label: state.t('verify', 'Verify'), icon: 'verify' });
  if (stateValue === 'writeback_pending') {
    actions.push(
      { id: 'internal-note', label: state.t('internalNote', 'Interne Notiz'), icon: 'note' },
      { id: 'public-reply', label: state.t('publicReply', 'Antwort'), icon: 'reply' },
      { id: 'close', label: state.t('close', 'Close'), icon: 'close' },
    );
  }
  const hasOpenClarification = state.data.ctox_ticket_clarification_requests.some((clarification) => (
    clarification.case_id === item.case_id
    && !['resolved', 'cancelled'].includes(String(clarification.status || '').toLowerCase())
  ));
  if (!hasOpenClarification && !['closed', 'done', 'completed', 'verified', 'writeback_pending'].includes(stateValue)) {
    actions.push({ id: 'request-clarification', label: state.t('requestClarification', 'Rückfrage'), icon: 'clarify' });
  }
  return actions;
}

function renderSelfWork(item) {
  const notes = state.data.ctox_ticket_self_work_notes.filter((note) => note.work_id === item.work_id);
  return `
    <article class="ctox-card" ${recordContextAttrs({
    type: 'ticket_self_work',
    id: item.work_id || item.id,
    label: item.title || item.work_id,
    submodule: 'self-work',
  })}>
      <header>${escapeHtml([item.kind || 'self-work', item.state].filter(Boolean).join(' · '))}</header>
      <div class="ctox-card-body">
        <strong class="tickets-context-item-title">${escapeHtml(item.title || item.work_id)}</strong>
        <p class="tickets-context-item-meta">${escapeHtml([item.assigned_to, `${notes.length} notes`].filter(Boolean).join(' · '))}</p>
      </div>
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
  const actions = [
    canPublish ? `<button type="button" class="ctox-pane-icon" data-clarification-action="publish" data-clarification-id="${escapeAttr(item.clarification_id)}" aria-label="${escapeAttr(state.t('publishClarification', 'Geprüft senden'))}" title="${escapeAttr(state.t('publishClarification', 'Geprüft senden'))}">${iconSvg(ICON.publish)}</button>` : '',
    canResolve ? `<button type="button" class="ctox-pane-icon" data-clarification-action="resolve" data-clarification-id="${escapeAttr(item.clarification_id)}" aria-label="${escapeAttr(state.t('resolveClarification', 'Antwort erfassen'))}" title="${escapeAttr(state.t('resolveClarification', 'Antwort erfassen'))}">${iconSvg(ICON.resolve)}</button>` : '',
  ].filter(Boolean).join('');
  return `
    <article class="ctox-card" ${recordContextAttrs({
    type: 'ticket_clarification',
    id: item.clarification_id || item.id,
    label: item.question || item.clarification_id,
    submodule: 'clarifications',
  })}>
      <header>
        <span>${escapeHtml([item.status, item.target_type, item.target_channel].filter(Boolean).join(' · '))}</span>
        ${actions ? `<span class="tickets-card-actions">${actions}</span>` : ''}
      </header>
      <div class="ctox-card-body">
        <strong class="tickets-context-item-title">${escapeHtml(item.question || item.clarification_id)}</strong>
        <p class="tickets-context-item-meta">${escapeHtml(missing || item.unblock_criteria || item.outbound_message_key || '')}</p>
        ${item.inbound_response_body ? `<p class="tickets-note">${escapeHtml(item.inbound_response_body)}</p>` : ''}
      </div>
    </article>
  `;
}

function renderBundle(item) {
  return `
    <article class="ctox-card" ${recordContextAttrs({
    type: 'ticket_control_bundle',
    id: item.runbook_id || item.id,
    label: item.label || item.runbook_id,
    submodule: 'runbooks',
  })}>
      <header>${escapeHtml(item.support_mode || 'support')}</header>
      <div class="ctox-card-body">
        <strong class="tickets-context-item-title">${escapeHtml(item.label || item.runbook_id)}</strong>
        <p class="tickets-context-item-meta">${escapeHtml(item.approval_mode || '')} · ${escapeHtml(item.verification_profile_id || '')}</p>
      </div>
    </article>
  `;
}

// ---------------------------------------------------------------------------
// Commands (flows unchanged)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Import / Export (honest, small — no HTTP, no direct projection writes)
// ---------------------------------------------------------------------------

// Export the currently visible tickets as a JSON download.
function exportTickets() {
  const rows = visibleTickets();
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(rows, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = 'tickets.json';
    anchor.rel = 'noopener';
    root()?.appendChild(anchor);
    anchor.click();
    anchor.remove();
  } catch (error) {
    console.error('[tickets] export failed', error);
  } finally {
    if (url) window.setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

// Import creates local tickets from a JSON array of { title, body } via the
// existing ctox.ticket.local.create command — projections stay server-owned.
function importTickets() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    let parsed;
    try { parsed = JSON.parse(await file.text()); } catch {
      setCommandStatus(state.t('importInvalid', 'Ungültige JSON-Datei.'), true);
      return;
    }
    const items = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
    const candidates = items.filter((item) => item && typeof item === 'object' && String(item.title || '').trim());
    if (!candidates.length) {
      setCommandStatus(state.t('importEmpty', 'Keine Datensätze in der Datei.'), true);
      return;
    }
    let count = 0;
    for (const item of candidates) {
      try {
        await dispatchTicketCommand('ctox.ticket.local.create', `local:${String(item.title).trim()}`, {
          title: String(item.title).trim(),
          body: String(item.body || item.body_text || '').trim(),
          status: 'open',
          priority: String(item.priority || 'normal'),
        });
        count += 1;
      } catch (error) {
        console.error('[tickets] import failed', error);
      }
    }
    setCommandStatus(`${state.t('imported', 'Importiert')}: ${count}`);
  });
  input.click();
}

// ---------------------------------------------------------------------------
// Selectors + utils
// ---------------------------------------------------------------------------

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

function selectedTicket() {
  return state.data.ctox_ticket_items.find((ticket) => ticket.id === state.selectedId)
    || visibleTickets()[0]
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

function verificationsForCase(caseId) {
  return state.data.ctox_ticket_verifications.filter((verification) => verification.case_id === caseId);
}

function writebacksForCase(caseId) {
  return state.data.ctox_ticket_writebacks.filter((writeback) => writeback.case_id === caseId);
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

function iconSvg(paths) {
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}

function renderEmptyState(title, body = '') {
  return `
    <div class="ctox-empty">
      <strong>${escapeHtml(title)}</strong>
      ${body ? `<span>${escapeHtml(body)}</span>` : ''}
    </div>
  `;
}

function renderTicketLoadingState(kind) {
  if (kind === 'loading') {
    return renderEmptyState(
      state.t('loadingTickets', 'Tickets werden geladen...'),
      state.t('loadingTicketsDetail', 'Die Ticketdaten werden geladen.'),
    );
  }
  return `
    <div class="ctox-empty ctox-syncing">
      <strong>${escapeHtml(state.t('syncingTickets', 'Tickets werden synchronisiert.'))}</strong>
      <span>${escapeHtml(state.t('syncingTicketsDetail', 'Die Ticketdaten werden gerade aus dem CTOX-Datenstrom geladen.'))}</span>
    </div>
  `;
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
    if (value) element.setAttribute(name, value);
    else element.removeAttribute(name);
  }
}

function clearRecordContext(element) {
  if (!element) return;
  for (const name of Object.keys(recordContextObject({}))) {
    element.removeAttribute(name);
  }
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
