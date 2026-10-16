// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}.
// Daten nur über api.query/api.get, Befehle über api.command (nie warten), Zustand nur über
// api.status.report in die Zustandszone der Shell. Protokoll: V4-PORT.md.
import { renderHtmlIfChanged } from 'ctox:shared/stable-dom.js';
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
import { buildSupportAgentTaskCommand, buildSupportCommand } from './support-commands.mjs';
import { filterSupportConversations, mergeSupportTimeline, supportQueueCounts } from './support-reducers.mjs';

const SUPPORT_COLLECTIONS = [
  'support_inboxes',
  'support_conversations',
  'support_thread_links',
  'support_identity_links',
  'support_notes',
  'support_conversation_events',
  'support_labels',
  'support_label_assignments',
  'support_views',
  'support_view_filters',
  'support_assignment_policies',
  'support_assignment_events',
  'support_macros',
  'support_automation_rules',
  'support_sla_policies',
  'support_applied_slas',
  'support_sla_events',
  'support_agent_requests',
  'support_agent_suggestions',
  'support_reporting_events',
  'support_reporting_rollups',
];

const READ_ONLY_COLLECTIONS = [
  'communication_threads',
  'communication_messages',
  'ctox_ticket_cases',
  'customer_accounts',
  'customer_contacts',
];

const COLLECTIONS = [
  'business_chats',
  'business_commands',
  'ctox_queue_tasks',
  ...READ_ONLY_COLLECTIONS,
  ...SUPPORT_COLLECTIONS,
];

const STATUS_OPTIONS = ['open', 'waiting', 'resolved'];
const PRIORITY_OPTIONS = ['low', 'normal', 'high', 'urgent'];

const REFRESH_DEBOUNCE_MS = 80;

// Eingebettete Texte = Rückfall (früher: Embedded-Labels, übersteuert von locales/<lang>.json über HTTP).
// Jetzt gilt: api.shell.t('support.<key>') übersteuert, wenn es nicht den Schlüssel selbst liefert
// (Schlüsselraum offen, app-api.d.ts OFFEN-B). Die Werte aus locales/*.json sind hier eingemischt.
const labels = {
  de: {
    kicker: 'Support Desk',
    queueTitle: 'Warteschlangen', // v4-allow: label
    conversationKicker: 'Konversation',
    contextKicker: 'Kontext',
    contextTitle: 'Kunde und Ticket',
    notePlaceholder: 'Interne Notiz',
    noteAction: 'Notiz hinzufügen',
    searchPlaceholder: 'Suche nach Kunde, Ticket oder Nachricht',
    importAction: 'Queue importieren',
    exportAction: 'Queue exportieren',
    openContext: 'Kontext öffnen',
    closeContext: 'Kontext schließen',
    entries: 'Einträge',
    importInvalid: 'Ungültige JSON-Datei.',
    importEmpty: 'Keine Konversationen mit Thread-Bezug in der Datei.',
    allOpen: 'Offen',
    mine: 'Meine',
    unassigned: 'Nicht zugewiesen',
    needsReply: 'Antwort nötig',
    slaRisk: 'SLA-Risiko',
    snoozed: 'Zurückgestellt',
    agentDrafts: 'CTOX',
    status: 'Status',
    priority: 'Priorität',
    assignee: 'Zuordnung',
    team: 'Team',
    customer: 'Kunde',
    ticket: 'Ticket',
    inbox: 'Posteingang',
    updated: 'Aktualisiert',
    lastActivity: 'Letzte Aktivität',
    noValue: 'nicht gesetzt',
    askCtox: 'CTOX fragen',
    showAsList: 'Als Liste anzeigen',
    showAsCards: 'Als Karten anzeigen',
    claim: 'Übernehmen',
    resolve: 'Lösen',
    reopen: 'Wieder öffnen',
    assignToMe: 'Mir zuweisen',
    createTicket: 'Ticket erstellen',
    snoozeOneHour: '1 Std. zurückstellen',
    snoozeTomorrow: 'Bis morgen',
    linkedThreads: 'Kommunikation',
    relatedTicket: 'Verknüpftes Ticket',
    relatedCustomer: 'Verknüpfter Kunde',
    ctoxWork: 'CTOX Arbeit',
    applySuggestion: 'Anwenden',
    rejectSuggestion: 'Ablehnen',
    noteLabel: 'Interne Notiz',
    eventLabel: 'Ereignis',
    agentLabel: 'CTOX Vorschlag',
    chatLabel: 'CTOX Chat',
    messageLabel: 'Nachricht',
    requestFrom: 'Anfrage von {0}',
    requestUntitled: 'Anfrage',
  },
  en: {
    kicker: 'Support Desk',
    queueTitle: 'Queues',
    conversationKicker: 'Conversation',
    contextKicker: 'Context',
    contextTitle: 'Customer and ticket',
    notePlaceholder: 'Internal note',
    noteAction: 'Add note',
    searchPlaceholder: 'Search customer, ticket, or message',
    importAction: 'Import queue',
    exportAction: 'Export queue',
    openContext: 'Open context',
    closeContext: 'Close context',
    entries: 'entries',
    importInvalid: 'Invalid JSON file.',
    importEmpty: 'No thread-linked conversations in the file.',
    allOpen: 'Open',
    mine: 'Mine',
    unassigned: 'Unassigned',
    needsReply: 'Needs reply',
    slaRisk: 'SLA risk',
    snoozed: 'Snoozed',
    agentDrafts: 'CTOX',
    status: 'Status',
    priority: 'Priority',
    assignee: 'Assignee',
    team: 'Team',
    customer: 'Customer',
    ticket: 'Ticket',
    inbox: 'Inbox',
    updated: 'Updated',
    lastActivity: 'Last activity',
    noValue: 'not set',
    askCtox: 'Ask CTOX',
    showAsList: 'Show as list',
    showAsCards: 'Show as cards',
    claim: 'Claim',
    resolve: 'Resolve',
    reopen: 'Reopen',
    assignToMe: 'Assign to me',
    createTicket: 'Create ticket',
    snoozeOneHour: 'Snooze 1h',
    snoozeTomorrow: 'Tomorrow',
    linkedThreads: 'Communication',
    relatedTicket: 'Linked ticket',
    relatedCustomer: 'Linked customer',
    ctoxWork: 'CTOX work',
    applySuggestion: 'Apply',
    rejectSuggestion: 'Reject',
    noteLabel: 'Internal note',
    eventLabel: 'Event',
    agentLabel: 'CTOX suggestion',
    chatLabel: 'CTOX chat',
    messageLabel: 'Message',
    requestFrom: 'Request from {0}',
    requestUntitled: 'Request',
  },
};

// Canonical grammar axes (app-wired via autoWirePaneGrammar(root)): the counted band selects the queue
// (ownership/urgency partition); the collapsed tray narrows it (status /
// priority / focus); search + view-toggle apply last. All are reported on the
// bubbling ctox-pane-grammar-change event.
const BAND_QUEUES = ['open', 'mine', 'unassigned', 'slaRisk'];

// Ansichtszustand, der einen Live-Remount (snapshot/restored) überlebt.
const SNAPSHOT_VERSION = 1;

function initialState() {
  return {
    api: null,
    root: null,
    t: (key, fallback) => fallback || key,
    lang: 'de',
    queue: 'open',
    viewMode: 'cards',
    search: '',
    filters: { status: 'open', priority: 'all', focus: 'all' },
    selectedId: '',
    // Restored selection waits until its record is in the live data (first emission may be empty).
    pendingSelectedId: '',
    pendingNoteDraft: '',
    pendingListScrollTop: 0,
    // Third pane (customer context) is hidden by default and auto-revealed on an
    // explicit selection: visible = hasSelection && !contextCollapsed.
    contextCollapsed: true,
    renderTimer: null,
    disposers: [],
    conversationOverrides: new Map(),
    localNotes: [],
    // „CTOX fragen“: Fehlschläge/Abbrüche der Agentenaufträge (support_agent_requests) dieser Montage → Zone.
    mountedAtMs: 0,
    agentRequestsReported: new Set(),
    data: Object.fromEntries(COLLECTIONS.map((name) => [name, []])),
  };
}

const state = initialState();

export async function mount(root, api, { restored } = {}) {
  Object.assign(state, initialState());
  state.mountedAtMs = Date.now();
  // Abos gehören dieser Montage (lokal festgehalten): ein zweites mount() derselben Modulinstanz darf
  // die Abmeldung der ersten nicht verlieren (PORTING.md §4 Falle 1).
  const disposers = state.disposers;
  state.api = api;
  state.root = root;
  if (!root.dataset.app) root.dataset.app = api.app?.id || 'support';
  setLanguage(api.shell.locale.get());
  // CSS lädt die Shell aus dem Manifest; index.html liefert der Service Worker versioniert (§16.15).
  root.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  if (state.root !== root) return { unmount() { root.replaceChildren(); }, snapshot: () => undefined };
  api.shell.setTitle('Support');
  applyStaticLabels();
  applyRestoredView(restored);
  wireUi();
  // Pane-Grammatik verdrahtet die App selbst (keine DOM-Heuristik der Shell, PORTING.md R14).
  autoWirePaneGrammar(root);
  // Seed the module's mirror of the grammar state from the DOM (defaults or restored values).
  seedGrammarState();
  render();
  for (const name of COLLECTIONS) {
    disposers.push(api.query(name, {}).subscribe((rows) => {
      state.data[name] = (Array.isArray(rows) ? rows : []).filter((doc) => doc && !doc.is_deleted);
      if (name === 'support_agent_requests') reportAgentRequests();
      scheduleRefresh();
    }));
  }
  disposers.push(api.shell.locale.subscribe((locale) => {
    const before = state.lang;
    setLanguage(locale);
    if (state.lang === before) return;
    applyStaticLabels();
    applyViewToggle();
    render();
  }));
  return {
    unmount() {
      if (state.renderTimer) window.clearTimeout(state.renderTimer);
      state.renderTimer = null;
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[support] unsubscribe failed', error); }
      }
      root.replaceChildren();
      if (state.root === root) {
        state.api = null;
        state.root = null;
      }
    },
    snapshot: supportSnapshotForRemount,
  };
}

function setLanguage(locale) {
  state.lang = String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
  const fallback = labels[state.lang] || labels.de;
  const api = state.api;
  state.t = (key, text) => {
    const scoped = `support.${key}`;
    const value = typeof api?.shell?.t === 'function' ? api.shell.t(scoped) : scoped;
    if (value && value !== scoped) return value;
    return fallback[key] ?? text ?? key;
  };
}

// snapshot(): nur structured-clone-fähige Werte, offene Eingaben immer mit (PORTING.md §2).
function supportSnapshotForRemount() {
  const root = rootEl();
  return {
    v: SNAPSHOT_VERSION,
    selectedId: state.selectedId || state.pendingSelectedId || '',
    queue: state.queue,
    viewMode: state.viewMode,
    search: root?.querySelector('[data-pg-search]')?.value || '',
    filters: { ...state.filters },
    contextCollapsed: state.contextCollapsed,
    trayOpen: root?.querySelector('[data-pg-tray]') ? !root.querySelector('[data-pg-tray]').hidden : false,
    noteDraft: root?.querySelector('[data-support-note]')?.value || '',
    listScrollTop: root?.querySelector('.support-left .ctox-pane-body')?.scrollTop || 0,
  };
}

// Ansicht sofort aus `restored` in das Markup schreiben (vor autoWirePaneGrammar, das den DOM-Stand liest);
// Auswahl und Entwurf erst übernehmen, wenn der Datensatz in den Live-Daten steht.
function applyRestoredView(restored) {
  if (!restored || typeof restored !== 'object' || restored.v !== SNAPSHOT_VERSION) return;
  const root = rootEl();
  const pane = leftPane();
  if (!root || !pane) return;
  if (typeof restored.search === 'string') {
    const search = pane.querySelector('[data-pg-search]');
    if (search) search.value = restored.search;
  }
  if (restored.viewMode === 'list' || restored.viewMode === 'cards') pane.dataset.pgDefaultView = restored.viewMode;
  if (BAND_QUEUES.includes(restored.queue)) {
    pane.querySelectorAll('[data-pg-band]').forEach((tab) => {
      const on = tab.dataset.pgBand === restored.queue;
      tab.setAttribute('aria-selected', String(on));
      tab.classList.toggle('is-active', on);
    });
  }
  if (restored.filters && typeof restored.filters === 'object') {
    for (const [name, value] of Object.entries(restored.filters)) {
      const select = pane.querySelector(`[data-pg-filter][data-pg-name="${CSS.escape(name)}"]`);
      if (select && typeof value === 'string' && [...select.options].some((option) => option.value === value)) select.value = value;
    }
  }
  if (restored.trayOpen === true) {
    const tray = pane.querySelector('[data-pg-tray]');
    if (tray) tray.hidden = false;
    pane.querySelector('[data-pg-tray-toggle]')?.setAttribute('aria-expanded', 'true');
  }
  if (typeof restored.contextCollapsed === 'boolean') state.contextCollapsed = restored.contextCollapsed;
  if (typeof restored.selectedId === 'string') state.pendingSelectedId = restored.selectedId;
  if (typeof restored.noteDraft === 'string') state.pendingNoteDraft = restored.noteDraft;
  if (Number.isFinite(restored.listScrollTop)) state.pendingListScrollTop = restored.listScrollTop;
  applyPendingRestore();
}

function applyPendingRestore() {
  if (!state.pendingSelectedId) return;
  const id = state.pendingSelectedId;
  if (!(state.data.support_conversations || []).some((item) => item.id === id)) return;
  state.pendingSelectedId = '';
  state.selectedId = id;
  const note = rootEl()?.querySelector('[data-support-note]');
  if (note && state.pendingNoteDraft) note.value = state.pendingNoteDraft;
  state.pendingNoteDraft = '';
  const scrollTop = state.pendingListScrollTop;
  state.pendingListScrollTop = 0;
  if (scrollTop) {
    window.requestAnimationFrame(() => {
      const body = rootEl()?.querySelector('.support-left .ctox-pane-body');
      if (body) body.scrollTop = scrollTop;
    });
  }
}

function applyStaticLabels() {
  const root = rootEl();
  if (!root) return;
  root.querySelectorAll('[data-i18n]').forEach((node) => {
    const key = node.getAttribute('data-i18n') || '';
    node.textContent = state.t(key, node.textContent || key);
  });
  root.querySelectorAll('[data-i18n-placeholder]').forEach((node) => {
    const key = node.getAttribute('data-i18n-placeholder') || '';
    node.setAttribute('placeholder', state.t(key, node.getAttribute('placeholder') || key));
  });
  root.querySelectorAll('[data-i18n-title]').forEach((node) => {
    const key = node.getAttribute('data-i18n-title') || '';
    const value = state.t(key, node.getAttribute('title') || key);
    node.setAttribute('title', value);
    node.setAttribute('aria-label', value);
  });
  const ask = root.querySelector('[data-support-ask-ctox]');
  ask?.setAttribute('title', state.t('askCtox', 'CTOX fragen'));
  ask?.setAttribute('aria-label', state.t('askCtox', 'CTOX fragen'));
}

function wireUi() {
  const root = rootEl();
  // Chrome (search / tray / reset / active-dot / counted band / footer) is
  // wired from the data-pg-* markup by autoWirePaneGrammar(root); the module only listens for the
  // resulting bubbling change event and re-renders. The one exception is the
  // view toggle: it is a single action button now, so the module owns its
  // label/icon and flips [data-pg-default-view] — the attribute
  // wirePaneGrammar() reads when no [data-pg-view] pair exists.
  leftPane()?.addEventListener('ctox-pane-grammar-change', handleGrammarChange);
  root.querySelector('[data-support-view-toggle]')?.addEventListener('click', () => toggleViewMode());
  applyViewToggle();
  // Selection is an in-place class flip, never a list rebuild (design-guide
  // "Re-renders never move the operator").
  const list = root.querySelector('[data-support-list]');
  list?.addEventListener('click', (event) => {
    const row = event.target instanceof Element ? event.target.closest('[data-support-conversation-id]') : null;
    if (!row || !list.contains(row)) return;
    selectConversation(row.getAttribute('data-support-conversation-id') || '');
  });
  list?.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const row = event.target instanceof Element ? event.target.closest('[data-support-conversation-id]') : null;
    if (!row || !list.contains(row)) return;
    event.preventDefault();
    selectConversation(row.getAttribute('data-support-conversation-id') || '');
  });
  root.querySelector('[data-support-import]')?.addEventListener('click', () => {
    handleImport();
  });
  root.querySelector('[data-support-export]')?.addEventListener('click', () => {
    handleExport();
  });
  root.querySelectorAll('[data-support-toggle-context]').forEach((button) => {
    button.addEventListener('click', () => toggleContext());
  });
  root.querySelector('[data-support-note-submit]')?.addEventListener('click', () => {
    createNote();
  });
  root.querySelector('[data-support-note]')?.addEventListener('input', () => renderComposerState());
  root.querySelector('[data-support-ask-ctox]')?.addEventListener('click', () => {
    askCtox();
  });
  root.querySelector('[data-support-context]')?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const suggestionAction = target?.closest('[data-support-suggestion-action]');
    if (suggestionAction) {
      runSuggestionAction(
        suggestionAction.getAttribute('data-support-suggestion-action') || '',
        suggestionAction.getAttribute('data-support-suggestion-id') || '',
      );
      return;
    }
    const action = target?.closest('[data-support-action]');
    if (!action) return;
    runConversationAction(action.getAttribute('data-support-action') || '');
  });
  root.querySelector('[data-support-context]')?.addEventListener('change', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const control = target?.closest('[data-support-control]');
    if (!control) return;
    runConversationControl(control.getAttribute('data-support-control') || '', control.value || '');
  });
}

function scheduleRefresh() {
  if (!state.api || state.renderTimer) return;
  state.renderTimer = window.setTimeout(() => {
    state.renderTimer = null;
    if (!state.api) return;
    refreshSupport();
  }, REFRESH_DEBOUNCE_MS);
}

function refreshSupport() {
  pruneOptimisticState();
  applyPendingRestore();
  const visible = visibleConversations();
  if (!state.pendingSelectedId && (!state.selectedId || !visible.some((item) => item.id === state.selectedId))) {
    state.selectedId = visible[0]?.id || '';
  }
  render();
}

function render() {
  if (!rootEl()) return;
  renderConversationList();
  renderBandCountsAndFooter();
  renderTimeline();
  renderContext();
  renderComposerState();
  applyContextVisibility();
}

// Rebuild the conversation well. Called on data / grammar changes (intentional
// resets — the scroll guard clears its offsets on the grammar-change
// event); NOT called on selection, which is an in-place flip.
// v4: no loading/syncing/empty copy — an empty list renders nothing (PORTING.md R11).
function renderConversationList() {
  const container = rootEl()?.querySelector('[data-support-list]');
  if (!container) return;
  // The view lives on the ROWS now (two different shapes, not two paddings of
  // one shape), so the well itself carries no view class any more.
  const rows = visibleConversations();
  if (!rows.length) {
    renderHtmlIfChanged(container, '', { signature: 'rows:none' });
    return;
  }
  // Selection is applied after the write; do not put selectedId into the signature
  // or every click rebuilds the scrollable well.
  const signature = JSON.stringify({
    view: state.viewMode,
    rows: rows.map((item) => ([
      item.id,
      item.updated_at_ms || 0,
      item.status || '',
      item.subject || '',
      item.customer_name || '',
      item.agent_draft_count || 0,
      item.sla_due_at_ms || 0,
    ])),
  });
  renderHtmlIfChanged(container, rows.map((item) => renderConversationRow(item, state.viewMode)).join(''), { signature });
  applyListSelection();
}

// The two views are deliberately DIFFERENT shapes, not two paddings of the same
// row (Betreiber-Direktive 31.08.):
//
//   cards / shards — 3 lines: bold subject + the urgency badges, then the two
//                    meta lines that actually decide a support conversation
//                    (Status · Priorität · Inbox, and Zuordnung · letzte
//                    Aktivität), on generous padding.
//   list           — exactly ONE dense line: the subject plus a single short
//                    meta hard right (the status word), nothing else.
//
// Every field comes from the conversation records the module already loads —
// no new data path, no schema change.
function renderConversationRow(item, viewMode = state.viewMode) {
  const view = viewMode === 'list' ? 'list' : 'cards';
  const label = conversationLabel(item);
  const attrs = `type="button" role="option" aria-selected="false" class="ctox-list-item support-conversation-row support-conversation-row--${view}" data-support-conversation-id="${escapeAttr(item.id)}" data-context-record-id="${escapeAttr(item.id)}" data-context-record-type="support_conversation" data-context-label="${escapeAttr(label)}"`;
  if (view === 'list') {
    return `
    <button ${attrs}>
      <span class="support-row-title">${escapeHtml(label)}</span>
      <span class="support-row-tail">${escapeHtml(displayStatus(item.status))}</span>
    </button>
  `;
  }
  const risk = isSlaRisk(item) ? `<span class="ctox-badge is-warning">${escapeHtml(state.t('slaRisk', 'SLA'))}</span>` : '';
  const agent = Number(item.agent_draft_count || 0) > 0 ? `<span class="ctox-badge is-info">${escapeHtml(state.t('agentDrafts', 'CTOX'))}</span>` : '';
  const primary = [
    displayStatus(item.status),
    displayPriority(item.priority),
  ].filter(Boolean);
  const secondary = [
    `${state.t('assignee', 'Zuordnung')}: ${item.assignee_id || state.t('unassigned', 'Nicht zugewiesen')}`,
    `${state.t('lastActivity', 'Letzte Aktivität')}: ${formatTime(item.last_activity_at_ms || item.updated_at_ms)}`,
  ];
  return `
    <button ${attrs}>
      <span class="support-row-head">
        <strong class="support-row-title">${escapeHtml(label)}</strong>
        <span class="support-row-badges">${risk}${agent}</span>
      </span>
      <span class="support-row-meta">${escapeHtml(primary.join(' · '))}</span>
      <span class="support-row-meta">${escapeHtml(secondary.join(' · '))}</span>
    </button>
  `;
}

// Selecting a conversation flips the selected class across the EXISTING rows —
// never a list rebuild — so the operator's scroll position is preserved.
function applyListSelection() {
  rootEl()?.querySelector('[data-support-list]')?.querySelectorAll('[data-support-conversation-id]').forEach((rowEl) => {
    const on = (rowEl.getAttribute('data-support-conversation-id') || '') === String(state.selectedId || '');
    rowEl.classList.toggle('is-selected', on);
    rowEl.setAttribute('aria-selected', String(on));
  });
}

// Explicit selection auto-reveals the context pane and updates the main +
// context surfaces in place, without rebuilding the queue list.
function selectConversation(id) {
  state.selectedId = id;
  state.contextCollapsed = false;
  applyListSelection();
  renderTimeline();
  renderContext();
  renderComposerState();
  applyContextVisibility();
}

// Band counts (zeros included) + one-line footer, written through the shell's
// pane-grammar handle when it is wired, else directly onto the count/footer
// nodes (the shell wires asynchronously after mount).
function bandCountsFor(conversations, nowMs, userId) {
  const counts = supportQueueCounts(conversations, nowMs, userId);
  return { open: counts.open, mine: counts.mine, unassigned: counts.unassigned, slaRisk: counts.slaRisk };
}

function renderBandCountsAndFooter() {
  const pane = leftPane();
  const counts = bandCountsFor(conversationsWithOverrides(), Date.now(), currentUserId());
  const pg = pane?.__ctoxPaneGrammar;
  if (pg && typeof pg.setCounts === 'function') {
    pg.setCounts(counts);
  } else {
    for (const [key, value] of Object.entries(counts)) {
      const node = pane?.querySelector(`[data-pg-count="${key}"]`);
      if (node) node.textContent = ` (${value})`;
    }
  }
  const visible = visibleConversations().length;
  // Textdiät R2: der Fuß wiederholte nur den Reiterzähler („5 Einträge · Offen“).
  const footerText = '';
  if (pg && typeof pg.setFooter === 'function') {
    pg.setFooter(footerText);
  } else {
    const node = pane?.querySelector('[data-pg-footer]');
    if (node) node.textContent = footerText;
  }
}

function queueLabel(queue) {
  const map = {
    open: state.t('allOpen', 'Offen'),
    mine: state.t('mine', 'Meine'),
    unassigned: state.t('unassigned', 'Nicht zugewiesen'),
    slaRisk: state.t('slaRisk', 'SLA-Risiko'),
  };
  return map[queue] || map.open;
}

// Auto-reveal: the customer-context pane is visible only when a conversation is
// selected and the operator has not collapsed it. Recomputed on every render.
function computeContextVisible({ hasSelection, userCollapsed }) {
  return Boolean(hasSelection) && !userCollapsed;
}

function applyContextVisibility() {
  const root = rootEl();
  if (!root) return;
  const hasSelection = Boolean(selectedConversation());
  const visible = computeContextVisible({ hasSelection, userCollapsed: state.contextCollapsed });
  root.classList.toggle('is-context-hidden', !visible);
  root.querySelectorAll('[data-support-toggle-context]').forEach((button) => {
    button.setAttribute('aria-expanded', visible ? 'true' : 'false');
    button.disabled = !hasSelection;
  });
}

function toggleContext() {
  if (!selectedConversation()) return;
  state.contextCollapsed = !state.contextCollapsed;
  applyContextVisibility();
}

// The shell reports search / view / tray / band changes here; map them onto
// module state and re-render (an intentional reset — the scroll guard clears
// its offsets first). Selection is kept valid within the new scope.
function handleGrammarChange(event) {
  const detail = event?.detail || {};
  state.search = String(detail.search ?? '').trim().toLowerCase();
  state.viewMode = detail.view === 'list' ? 'list' : 'cards';
  applyViewToggle();
  state.queue = BAND_QUEUES.includes(detail.band) ? detail.band : 'open';
  const filters = detail.filters || {};
  state.filters = {
    status: filters.status || 'open',
    priority: filters.priority || 'all',
    focus: filters.focus || 'all',
  };
  const visible = visibleConversations();
  if (!state.selectedId || !visible.some((item) => item.id === state.selectedId)) {
    state.selectedId = visible[0]?.id || '';
  }
  render();
}

// The grammar as the DOM currently states it — the same shape the shell emits,
// so the module can re-emit it after flipping the view without inventing a
// second state channel.
function readGrammarFromDom() {
  const pane = leftPane();
  return {
    search: String(pane?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
    view: pane?.dataset.pgDefaultView === 'list' ? 'list' : 'cards',
    band: pane?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'open',
    filters: {
      status: pane?.querySelector('[data-pg-filter][data-pg-name="status"]')?.value || 'open',
      priority: pane?.querySelector('[data-pg-filter][data-pg-name="priority"]')?.value || 'all',
      focus: pane?.querySelector('[data-pg-filter][data-pg-name="focus"]')?.value || 'all',
    },
  };
}

function seedGrammarState() {
  const pane = leftPane();
  if (!pane) return;
  const grammar = readGrammarFromDom();
  state.search = grammar.search;
  state.viewMode = grammar.view;
  state.queue = grammar.band;
  state.filters = grammar.filters;
}

// One button, one action: icon, title and aria-label always name the view the
// click switches TO. No aria-pressed — this is an action, not a state control
// (Betreiber-Direktive 31.08.).
function applyViewToggle() {
  const button = rootEl()?.querySelector('[data-support-view-toggle]');
  if (!button) return;
  const next = readGrammarFromDom().view === 'list' ? 'cards' : 'list';
  const label = next === 'list'
    ? state.t('showAsList', 'Als Liste anzeigen')
    : state.t('showAsCards', 'Als Karten anzeigen');
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  button.removeAttribute('aria-pressed');
  button.querySelectorAll('[data-support-view-icon]').forEach((icon) => {
    // toggleAttribute, not `.hidden`: these are SVGElements, which do not
    // implement the HTMLElement `hidden` IDL property — assigning it would set
    // a silent JS expando and the icon would never swap.
    icon.toggleAttribute('hidden', icon.dataset.supportViewIcon !== next);
  });
}

function toggleViewMode() {
  const pane = leftPane();
  if (!pane) return;
  pane.dataset.pgDefaultView = readGrammarFromDom().view === 'list' ? 'cards' : 'list';
  applyViewToggle();
  // Re-use the shell's own change channel: its scroll guard clears the recorded
  // offsets (a view switch is an intentional reset) and handleGrammarChange
  // below drives the re-render.
  try {
    pane.dispatchEvent(new CustomEvent('ctox-pane-grammar-change', {
      detail: readGrammarFromDom(),
      bubbles: true,
    }));
  } catch {
    state.viewMode = readGrammarFromDom().view;
    render();
  }
}

function leftPane() {
  return rootEl()?.querySelector('.support-left') || null;
}

function renderTimeline() {
  const title = rootEl()?.querySelector('[data-support-title]');
  const timeline = rootEl()?.querySelector('[data-support-timeline]');
  const item = selectedConversation();
  if (title) title.textContent = item ? conversationLabel(item) : '';
  if (!timeline) return;
  // Without a selection or timeline rows the surface stays empty (no placeholder copy, R11).
  const rows = item ? timelineRows(item) : [];
  if (!rows.length) {
    timeline.replaceChildren();
    return;
  }
  timeline.innerHTML = rows.map(renderTimelineItem).join('');
}

function renderTimelineItem(row) {
  const label = timelineLabel(row);
  return `
    <article class="support-timeline-item is-${escapeAttr(row.kind)}" data-context-record-id="${escapeAttr(row.id)}" data-context-record-type="support_${escapeAttr(row.kind)}" data-context-label="${escapeAttr(label)}">
      <header>
        <strong>${escapeHtml(label)}</strong>
        <span>${escapeHtml(formatTime(row.at))}</span>
      </header>
      <p>${escapeHtml(timelineText(row))}</p>
    </article>
  `;
}

// Secondary reference sections sit in a collapsed <details> so the context
// pane isn't a wall of cards; the agent expands the one they need. Empty
// sections are dropped entirely rather than shown as "nicht gesetzt" cards.
function contextDetails(title, innerHtml) {
  if (!innerHtml) return '';
  return `
    <details class="support-context-details">
      <summary>${escapeHtml(title)}</summary>
      <div class="ctox-card-body">${innerHtml}</div>
    </details>`;
}

function renderContext() {
  const context = rootEl()?.querySelector('[data-support-context]');
  if (!context) return;
  const item = selectedConversation();
  if (!item) {
    context.removeAttribute('data-context-record-id');
    context.removeAttribute('data-context-record-type');
    context.removeAttribute('data-context-label');
    context.replaceChildren();
    return;
  }
  context.setAttribute('data-context-record-id', item.id || '');
  context.setAttribute('data-context-record-type', 'support_conversation');
  context.setAttribute('data-context-label', conversationLabel(item));
  const suggestions = suggestionsFor(item.id);
  const account = customerAccountFor(item);
  const contact = customerContactFor(item);
  const ticket = ticketFor(item);
  const threadLinks = threadLinksFor(item);
  const messages = communicationMessagesFor(item);
  const commands = businessCommandsFor(item);
  const tasks = queueTasksFor(item, commands);
  const actorId = currentUserId();
  const relatedCustomer = account || contact;
  const relatedCustomerType = account ? 'customer_account' : 'customer_contact';
  const relatedCustomerLabel = account?.name || contact?.name || contact?.display_name || contact?.email || relatedCustomer?.id || '';

  // Secondary reference sections: only build the ones that actually have data,
  // then render them as collapsed disclosures below the primary controls.
  const relatedCustomerHtml = relatedCustomer ? `
    <dl class="ctox-fields ctox-fields--stacked" data-context-record-id="${escapeAttr(relatedCustomer.id)}" data-context-record-type="${relatedCustomerType}" data-context-label="${escapeAttr(relatedCustomerLabel)}">
      ${fact(state.t('customer', 'Kunde'), account?.name || readableRef(item.customer_account_id))}
      ${fact('Kontakt', contactDisplayName(contact) || contact?.email || readableRef(item.customer_contact_id))}
      ${fact('E-Mail', contact?.email || account?.domain || '')}
    </dl>` : '';
  const relatedTicketHtml = ticket ? `
    <dl class="ctox-fields ctox-fields--stacked" data-context-record-id="${escapeAttr(ticket.id || item.ticket_case_id)}" data-context-record-type="ticket" data-context-label="${escapeAttr(ticket.title || ticket.summary || ticket.id || item.ticket_case_id)}">
      ${fact('Titel', ticket.title || ticket.summary || readableRef(ticket.id))}
      ${fact(state.t('status', 'Status'), ticket.status || ticket.state || '')}
    </dl>` : '';
  const communicationHtml = (threadLinks.length || messages.length) ? `
    ${threadLinks.map((link) => `
      <p class="support-linked-row" data-context-record-id="${escapeAttr(link.id || `${item.id}:${link.thread_key || 'thread'}`)}" data-context-record-type="support_thread_link" data-context-label="${escapeAttr(link.thread_key || link.channel || link.link_role || 'Support thread')}">
        <strong>${escapeHtml(link.channel || link.link_role || 'thread')}</strong>
        <span>${escapeHtml(link.thread_key || '')}</span>
      </p>
    `).join('')}
    ${messages.length ? `<p class="support-linked-row"><span>${escapeHtml(messageCountLabel(messages.length))}</span></p>` : ''}` : '';
  const ctoxWorkHtml = (commands.length || tasks.length) ? `
    ${commands.slice(0, 3).map((command) => `
      <p class="support-linked-row" data-context-record-id="${escapeAttr(command.id || command.command_id || command.task_id)}" data-context-record-type="business_command" data-context-label="${escapeAttr(command.type || command.command_type || command.id || 'Command')}">
        <strong>${escapeHtml(command.type || command.command_type || 'command')}</strong>
        <span>${escapeHtml([command.status, command.task_status, command.task_id].filter(Boolean).join(' · '))}</span>
      </p>
    `).join('')}
    ${tasks.slice(0, 3).map((task) => `
      <p class="support-linked-row" data-context-record-id="${escapeAttr(task.id)}" data-context-record-type="ctox_queue_task" data-context-label="${escapeAttr(task.title || task.id || 'Task')}">
        <strong>${escapeHtml(task.title || task.id || 'task')}</strong>
        <span>${escapeHtml([task.status, task.task_status, task.id].filter(Boolean).join(' · '))}</span>
      </p>
    `).join('')}` : '';
  // CTOX suggestions stay a top-level card, but only when there is one to act on.
  const suggestionsHtml = suggestions.length ? suggestions.slice(0, 3).map((suggestion) => `
    <div class="support-suggestion-row" data-context-record-id="${escapeAttr(suggestion.id)}" data-context-record-type="support_agent_suggestion" data-context-label="${escapeAttr(suggestion.summary || suggestion.suggestion_kind || suggestion.id)}">
      <p>${escapeHtml(suggestion.summary || suggestion.suggestion_kind || suggestion.id)}</p>
      <small>${escapeHtml([suggestion.suggestion_kind, suggestion.status].filter(Boolean).join(' · '))}</small>
      ${['applied', 'rejected'].includes(String(suggestion.status || '').toLowerCase()) ? '' : `
        <div class="support-context-actions">
          <button type="button" class="ctox-button" data-support-suggestion-action="apply" data-support-suggestion-id="${escapeAttr(suggestion.id)}">${escapeHtml(state.t('applySuggestion', 'Anwenden'))}</button>
          <button type="button" class="ctox-button" data-support-suggestion-action="reject" data-support-suggestion-id="${escapeAttr(suggestion.id)}">${escapeHtml(state.t('rejectSuggestion', 'Ablehnen'))}</button>
        </div>
      `}
    </div>
  `).join('') : '';
  context.innerHTML = `
    <section class="ctox-card">
      <header>${escapeHtml(state.t('contextTitle', 'Kunde und Ticket'))}</header>
      <div class="ctox-card-body">
        <dl class="ctox-fields ctox-fields--stacked">
          ${fact(state.t('status', 'Status'), displayStatus(item.status))}
          ${fact(state.t('priority', 'Priorität'), item.priority)}
          ${fact(state.t('assignee', 'Zuordnung'), item.assignee_id)}
          ${fact(state.t('team', 'Team'), readableRef(item.team_id))}
          ${fact(state.t('customer', 'Kunde'), account?.name || contactDisplayName(contact) || readableRef(item.customer_account_id || item.customer_contact_id))}
          ${fact(state.t('ticket', 'Ticket'), ticket?.title || ticket?.summary || readableRef(item.ticket_case_id))}
          ${fact(state.t('updated', 'Aktualisiert'), formatTime(item.updated_at_ms || item.last_activity_at_ms))}
        </dl>
      </div>
    </section>
    <section class="ctox-card">
      <header>${escapeHtml(state.t('status', 'Status'))}</header>
      <div class="ctox-card-body">
        <div class="support-control-grid">
          <label>
            <span class="ctox-field-label">${escapeHtml(state.t('status', 'Status'))}</span>
            <select class="ctox-select" data-support-control="status">
              ${optionList(STATUS_OPTIONS, normalizeControlValue(item.status, 'open'), displayStatus)}
            </select>
          </label>
          <label>
            <span class="ctox-field-label">${escapeHtml(state.t('priority', 'Priorität'))}</span>
            <select class="ctox-select" data-support-control="priority">
              ${optionList(PRIORITY_OPTIONS, normalizeControlValue(item.priority, 'normal'), displayPriority)}
            </select>
          </label>
        </div>
        <div class="support-context-actions is-grid">
          <button type="button" class="ctox-button" data-support-action="claim">${escapeHtml(state.t('claim', 'Übernehmen'))}</button>
          <button type="button" class="ctox-button" data-support-action="assign-me" ${actorId ? '' : 'disabled'}>${escapeHtml(state.t('assignToMe', 'Mir zuweisen'))}</button>
          <button type="button" class="ctox-button" data-support-action="snooze-1h">${escapeHtml(state.t('snoozeOneHour', '1 Std. zurückstellen'))}</button>
          <button type="button" class="ctox-button" data-support-action="snooze-tomorrow">${escapeHtml(state.t('snoozeTomorrow', 'Bis morgen'))}</button>
          <button type="button" class="ctox-button" data-support-action="${isClosed(item) ? 'reopen' : 'resolve'}">${escapeHtml(isClosed(item) ? state.t('reopen', 'Wieder öffnen') : state.t('resolve', 'Lösen'))}</button>
          <button type="button" class="ctox-button" data-support-action="create-ticket" ${item.ticket_case_id ? 'disabled' : ''}>${escapeHtml(state.t('createTicket', 'Ticket erstellen'))}</button>
        </div>
      </div>
    </section>
    ${suggestionsHtml ? `<section class="ctox-card">
      <header>${escapeHtml(state.t('agentLabel', 'CTOX Vorschlag'))}</header>
      <div class="ctox-card-body">${suggestionsHtml}</div>
    </section>` : ''}
    ${contextDetails(state.t('relatedCustomer', 'Verknüpfter Kunde'), relatedCustomerHtml)}
    ${contextDetails(state.t('relatedTicket', 'Verknüpftes Ticket'), relatedTicketHtml)}
    ${contextDetails(state.t('linkedThreads', 'Kommunikation'), communicationHtml)}
    ${contextDetails(state.t('ctoxWork', 'CTOX Arbeit'), ctoxWorkHtml)}
  `;
}

function renderComposerState() {
  const root = rootEl();
  if (!root) return;
  const selected = Boolean(selectedConversation());
  const note = root.querySelector('[data-support-note]');
  const button = root.querySelector('[data-support-note-submit]');
  const ask = root.querySelector('[data-support-ask-ctox]');
  if (button) button.disabled = !selected || !String(note?.value || '').trim();
  if (ask) ask.disabled = !selected;
}

function createNote() {
  const item = selectedConversation();
  const note = rootEl()?.querySelector('[data-support-note]');
  const body = String(note?.value || '').trim();
  if (!item || !body) return;
  const now = Date.now();
  const command = buildSupportCommand({
    commandType: 'support.note.create',
    recordId: item.id,
    payload: {
      conversation_id: item.id,
      body,
      visibility: 'internal',
      source: 'business-os.support',
    },
    surface: 'support.note.create',
  });
  const optimisticNote = {
    id: `support_note_pending_${command.id}`,
    conversation_id: item.id,
    author_id: currentUserId(),
    body,
    visibility: 'internal',
    source: 'business-os.support',
    created_at_ms: now,
    updated_at_ms: now,
  };
  rememberLocalNote(optimisticNote);
  note.value = '';
  render();
  renderComposerState();
  dispatchSupportCommand(command, {
    onFailed: () => {
      forgetLocalNote(optimisticNote.id);
      render();
    },
  });
}

function askCtox() {
  const item = selectedConversationWithCurrentControls();
  if (!item) return;
  // App-Befehl support.agent.task (Strang SKL-A1): der Handler füllt das Template prompts/agent-task.md und startet
  // den Agenten mit dem App-Skill support/support-workflow; Vorschläge erscheinen live über
  // support_agent_suggestions, Fehlschlag/Abbruch über reportAgentRequests in der Zone.
  dispatchSupportCommand(buildSupportAgentTaskCommand({
    conversationId: item.id,
    title: `Support: ${conversationLabel(item)}`,
    requestKind: 'summary',
    status: item.status || 'open',
    priority: item.priority || 'normal',
    recordSnapshot: supportSnapshot(item),
  }));
}

// Agentenaufträge, die während dieser Montage fehlschlagen oder abgebrochen werden, gehen in die Zustandszone
// (live über die Collection, kein Polling); ein späterer Abschluss räumt den Eintrag.
function reportAgentRequests() {
  const api = state.api;
  if (!api) return;
  for (const request of state.data.support_agent_requests || []) {
    const code = `support.agent.request.${request.id}`;
    const status = String(request.status || '');
    if ((status === 'failed' || status === 'cancelled') && Number(request.updated_at_ms || 0) >= state.mountedAtMs) {
      if (state.agentRequestsReported.has(request.id)) continue;
      state.agentRequestsReported.add(request.id);
      const error = request.payload?.error || status;
      api.status.report({ level: status === 'cancelled' ? 'warn' : 'error', code, text: `support.agent.task: ${error}`, ref: request.command_id || request.id });
    } else if (status === 'completed' && state.agentRequestsReported.delete(request.id)) {
      api.status.clear(code);
    }
  }
}

function runConversationAction(action) {
  const item = selectedConversation();
  if (!item) return;
  if (action === 'assign-me') {
    const assigneeId = currentUserId();
    if (!assigneeId) return;
    rememberConversationOverride(item.id, { assignee_id: assigneeId });
    render();
    dispatchSupportCommand(buildSupportCommand({
      commandType: 'support.conversation.assign',
      recordId: item.id,
      payload: { conversation_id: item.id, assignee_id: assigneeId },
      surface: 'support.conversation.assign',
    }), {
      onFailed: () => {
        forgetConversationOverrideFields(item.id, ['assignee_id']);
        render();
      },
    });
    return;
  }
  if (action === 'snooze-1h' || action === 'snooze-tomorrow') {
    const snoozedUntilMs = action === 'snooze-1h' ? Date.now() + 60 * 60 * 1000 : nextMorningMs();
    rememberConversationOverride(item.id, { snoozed_until_ms: snoozedUntilMs });
    render();
    dispatchSupportCommand(buildSupportCommand({
      commandType: 'support.conversation.snooze',
      recordId: item.id,
      payload: {
        conversation_id: item.id,
        snoozed_until_ms: snoozedUntilMs,
      },
      surface: 'support.conversation.snooze',
    }), {
      onFailed: () => {
        forgetConversationOverrideFields(item.id, ['snoozed_until_ms']);
        render();
      },
    });
    return;
  }
  if (action === 'create-ticket') {
    dispatchSupportCommand(buildSupportCommand({
      commandType: 'support.ticket.create_from_conversation',
      recordId: item.id,
      payload: {
        conversation_id: item.id,
        title: conversationLabel(item),
        summary: timelineRows(item).slice(-6).map(timelineText).filter(Boolean).join('\n\n'),
      },
      surface: 'support.ticket.create_from_conversation',
    }));
    return;
  }
  const type = action === 'claim'
    ? 'support.conversation.claim'
    : action === 'reopen'
      ? 'support.conversation.reopen'
      : 'support.conversation.resolve';
  const optimisticPatch = {};
  if (action === 'claim') {
    const assigneeId = currentUserId();
    if (assigneeId) optimisticPatch.assignee_id = assigneeId;
  } else if (action === 'resolve') {
    optimisticPatch.status = 'resolved';
  } else if (action === 'reopen') {
    optimisticPatch.status = 'open';
  }
  rememberConversationOverride(item.id, optimisticPatch);
  render();
  dispatchSupportCommand(buildSupportCommand({
    commandType: type,
    recordId: item.id,
    payload: { conversation_id: item.id },
    surface: type,
  }), {
    onFailed: () => {
      forgetConversationOverrideFields(item.id, Object.keys(optimisticPatch));
      render();
    },
  });
}

function runConversationControl(control, value) {
  const item = selectedConversation();
  if (!item) return;
  const normalized = String(value || '').trim();
  if (!normalized) return;
  const config = {
    status: ['support.conversation.status', { status: normalized }],
    priority: ['support.conversation.priority', { priority: normalized }],
  }[control];
  if (!config) return;
  const [type, payload] = config;
  rememberConversationOverride(item.id, payload);
  render();
  dispatchSupportCommand(buildSupportCommand({
    commandType: type,
    recordId: item.id,
    payload: { conversation_id: item.id, ...payload },
    surface: type,
  }), {
    onFailed: () => {
      forgetConversationOverrideFields(item.id, Object.keys(payload));
      render();
    },
  });
}

function runSuggestionAction(action, suggestionId) {
  const item = selectedConversation();
  if (!item || !suggestionId) return;
  const type = action === 'reject'
    ? 'support.agent.reject_suggestion'
    : 'support.agent.apply_suggestion';
  dispatchSupportCommand(buildSupportCommand({
    commandType: type,
    recordId: item.id,
    payload: {
      conversation_id: item.id,
      suggestion_id: suggestionId,
    },
    surface: type,
  }));
}

// Befehle: api.command legt das Befehlsdokument lokal an und liefert sofort; die UI wartet nie auf
// Annahme. Der Befehl trägt genau die Handler-Payload (V4-HANDLERS.md) plus client_context (Herkunft),
// record_id geht über opts (§16.19 Nr. 8). Fehlschläge → Zustandszone, Erfolg bleibt still.
function dispatchSupportCommand(command, { onFailed } = {}) {
  const api = state.api;
  if (!api) return null;
  const type = command.command_type;
  const payload = { ...(command.payload || {}), client_context: command.client_context };
  const opts = command.record_id ? { record_id: command.record_id } : undefined;
  const code = `support.command.${type}`;
  let handle;
  try {
    handle = api.command(type, payload, opts);
  } catch (error) {
    api.status.report({ level: 'error', code, text: `${type}: ${error?.code || 'local'} ${error?.message || error}` });
    onFailed?.(error);
    return null;
  }
  followCommand(api, handle, type, code, onFailed);
  return handle;
}

function followCommand(api, handle, type, code, onFailed) {
  let off = null;
  let finished = false;
  const stop = () => { finished = true; off?.(); off = null; };
  off = handle.status.subscribe((value) => {
    if (finished) return;
    if (value?.status === 'failed') {
      api.status.report({ level: 'error', code, text: `${type}: ${value.error || 'failed'}`, ref: handle.id });
      stop();
      onFailed?.(value);
    } else if (value?.status === 'done') {
      api.status.clear(code);
      stop();
    }
  });
  if (finished) off?.();
  handle.local.catch((error) => {
    if (finished) return;
    api.status.report({ level: 'error', code, text: `${type}: ${error?.code || 'local'} ${error?.message || error}`, ref: handle.id });
    stop();
    onFailed?.(error);
  });
  state.disposers.push(stop);
}

// Export the visible queue as an honest, small JSON snapshot of the projection
// (the column's own records) — read-only, no invented fields.
function buildSupportExport(conversations) {
  return (conversations || []).map((item) => ({
    id: item.id,
    status: item.status || 'open',
    priority: item.priority || 'normal',
    assignee_id: item.assignee_id || '',
    team_id: item.team_id || '',
    inbox_id: item.inbox_id || '',
    customer_account_id: item.customer_account_id || '',
    customer_contact_id: item.customer_contact_id || '',
    ticket_case_id: item.ticket_case_id || '',
    primary_thread_key: item.primary_thread_key || '',
    updated_at_ms: Number(item.updated_at_ms || item.last_activity_at_ms || 0),
  }));
}

// Download bleibt app-lokal im Fenster (Anker in root); Erfolg ist still, der Download selbst ist die Rückmeldung.
function handleExport() {
  const rows = buildSupportExport(visibleConversations());
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(rows, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = 'support-queue.json';
    anchor.rel = 'noopener';
    rootEl()?.appendChild(anchor);
    anchor.click();
    anchor.remove();
    state.api?.status.clear('support.export.failed');
  } catch (error) {
    console.error('[support] export failed', error);
    state.api?.status.report({ level: 'error', code: 'support.export.failed', text: `Export: ${error?.message || error}` });
  } finally {
    if (url) window.setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

// Import honestly re-opens conversations from their source threads: each entry
// that carries a thread key is dispatched through the existing
// support.conversation.open_from_thread command. No schema is invented and no
// record is written directly. OFFEN (V4-PORT.md OFFEN-1): the payload carries no
// conversation_id, which the handler requires (V4-HANDLERS.md U1) — kept as is, not invented here.
function parseSupportImport(parsed) {
  const items = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
  return items
    .filter((item) => item && typeof item === 'object')
    .map((item) => ({
      thread_key: String(item.primary_thread_key || item.thread_key || '').trim(),
      inbox_id: String(item.inbox_id || '').trim(),
    }))
    .filter((item) => item.thread_key);
}

function handleImport() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const api = state.api;
    const file = input.files && input.files[0];
    if (!file || !api) return;
    importSupportText(await file.text());
  });
  input.click();
}

function importSupportText(text) {
  const api = state.api;
  if (!api) return 0;
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch {
    api.status.clear('support.import.empty');
    api.status.report({ level: 'warn', code: 'support.import.invalid', text: state.t('importInvalid', 'Ungültige JSON-Datei.') });
    return 0;
  }
  api.status.clear('support.import.invalid');
  const candidates = parseSupportImport(parsed);
  if (!candidates.length) {
    api.status.report({ level: 'warn', code: 'support.import.empty', text: state.t('importEmpty', 'Keine Konversationen mit Thread-Bezug in der Datei.') });
    return 0;
  }
  api.status.clear('support.import.empty');
  let count = 0;
  for (const candidate of candidates) {
    const handle = dispatchSupportCommand(buildSupportCommand({
      commandType: 'support.conversation.open_from_thread',
      payload: { thread_key: candidate.thread_key, inbox_id: candidate.inbox_id },
      surface: 'support.conversation.open_from_thread',
    }));
    if (handle) count += 1;
  }
  return count;
}

// Compose the three grammar axes: band (ownership/urgency queue) selects the
// base partition; the tray narrows it (status / priority / focus); search
// applies inside filterSupportConversations. All predicates are the existing
// reducer helpers — command flows and schemas are untouched.
function visibleConversations() {
  const userId = currentUserId();
  const queue = state.queue;
  if (queue === 'mine' && !userId) return [];
  const status = state.filters.status || 'open';
  const priority = state.filters.priority && state.filters.priority !== 'all' ? state.filters.priority : '';
  let rows = filterSupportConversations(conversationsWithOverrides(), {
    status,
    priority,
    assigneeId: queue === 'mine' ? userId : queue === 'unassigned' ? 'unassigned' : '',
    query: state.search,
  });
  if (queue === 'slaRisk') rows = rows.filter(isSlaRisk);
  const focus = state.filters.focus;
  if (focus === 'needsReply') {
    rows = rows.filter((item) => Number(item.unread_count || 0) > 0 || Number(item.waiting_since_ms || 0) > 0);
  } else if (focus === 'snoozed') {
    rows = rows.filter((item) => Number(item.snoozed_until_ms || 0) > Date.now());
  } else if (focus === 'agentDrafts') {
    const suggestionIds = new Set((state.data.support_agent_suggestions || []).map((suggestion) => suggestion.conversation_id));
    rows = rows.filter((item) => suggestionIds.has(item.id) || Number(item.agent_draft_count || 0) > 0);
  }
  return rows;
}

function selectedConversation() {
  return conversationsWithOverrides().find((item) => item.id === state.selectedId) || null;
}

function selectedConversationWithCurrentControls() {
  const item = selectedConversation();
  if (!item) return null;
  const root = rootEl();
  if (!root) return item;
  const status = String(root.querySelector('[data-support-control="status"]')?.value || '').trim();
  const priority = String(root.querySelector('[data-support-control="priority"]')?.value || '').trim();
  return {
    ...item,
    ...(status ? { status } : {}),
    ...(priority ? { priority } : {}),
  };
}

function conversationsWithOverrides() {
  return (state.data.support_conversations || []).map((item) => {
    const override = state.conversationOverrides.get(item.id);
    return override ? { ...item, ...override } : item;
  });
}

function rememberConversationOverride(conversationId, patch) {
  if (!conversationId || !patch || typeof patch !== 'object') return;
  if (!Object.keys(patch).length) return;
  const existing = state.conversationOverrides.get(conversationId) || {};
  state.conversationOverrides.set(conversationId, {
    ...existing,
    ...patch,
    updated_at_ms: Date.now(),
  });
}

function forgetConversationOverrideFields(conversationId, keys) {
  if (!conversationId || !Array.isArray(keys) || !keys.length) return;
  const existing = state.conversationOverrides.get(conversationId);
  if (!existing) return;
  const next = { ...existing };
  for (const key of keys) delete next[key];
  delete next.updated_at_ms;
  if (Object.keys(next).length) {
    state.conversationOverrides.set(conversationId, { ...next, updated_at_ms: Date.now() });
  } else {
    state.conversationOverrides.delete(conversationId);
  }
}

function rememberLocalNote(note) {
  if (!note?.conversation_id || !note.body) return;
  state.localNotes = [
    ...state.localNotes.filter((item) => item.id !== note.id),
    note,
  ];
}

function forgetLocalNote(noteId) {
  if (!noteId) return;
  state.localNotes = state.localNotes.filter((note) => note.id !== noteId);
}

function notesFor(conversationId) {
  const source = (state.data.support_notes || []).filter((note) => note.conversation_id === conversationId);
  const local = state.localNotes.filter((note) => {
    if (note.conversation_id !== conversationId) return false;
    return !source.some((item) => item.body === note.body && item.visibility === note.visibility);
  });
  return [...source, ...local];
}

function pruneOptimisticState() {
  const conversations = state.data.support_conversations || [];
  for (const [id, override] of state.conversationOverrides) {
    const source = conversations.find((item) => item.id === id);
    if (!source) continue;
    const pending = {};
    for (const [key, value] of Object.entries(override)) {
      if (key === 'updated_at_ms') continue;
      if (String(source[key] ?? '') !== String(value ?? '')) pending[key] = value;
    }
    if (Object.keys(pending).length) {
      state.conversationOverrides.set(id, { ...pending, updated_at_ms: override.updated_at_ms || Date.now() });
    } else {
      state.conversationOverrides.delete(id);
    }
  }

  const notes = state.data.support_notes || [];
  state.localNotes = state.localNotes.filter((note) => !notes.some((item) => (
    item.conversation_id === note.conversation_id
    && item.body === note.body
    && item.visibility === note.visibility
  )));
}

function currentUserId() {
  // Nur Spiegel (api.actor); entschieden wird im Core.
  return String(state.api?.actor?.id || '').trim();
}

function timelineRows(item) {
  const conversationId = item.id;
  const messages = [
    ...communicationMessagesFor(item),
    ...businessChatRows(item),
  ];
  return mergeSupportTimeline({
    messages,
    notes: notesFor(conversationId),
    events: (state.data.support_conversation_events || []).filter((event) => event.conversation_id === conversationId),
    suggestions: suggestionsFor(conversationId),
  });
}

function threadLinksFor(item) {
  return (state.data.support_thread_links || [])
    .filter((link) => link.conversation_id === item.id)
    .sort((a, b) => String(a.thread_key || '').localeCompare(String(b.thread_key || '')));
}

function threadKeysFor(item) {
  const keys = new Set();
  if (item.primary_thread_key) keys.add(item.primary_thread_key);
  for (const link of threadLinksFor(item)) {
    if (link.thread_key) keys.add(link.thread_key);
  }
  return keys;
}

function communicationMessagesFor(item) {
  const threadKeys = threadKeysFor(item);
  if (!threadKeys.size) return [];
  return (state.data.communication_messages || [])
    .filter((message) => threadKeys.has(message.thread_key))
    .sort((a, b) => timestampForMessage(a) - timestampForMessage(b));
}

function businessChatRows(item) {
  const threadKeys = threadKeysFor(item);
  if (item.id) threadKeys.add(`business-os/support/${item.id}`);
  return (state.data.business_chats || [])
    .filter((chat) => threadKeys.has(chat.thread_key) || threadKeys.has(chat.contextMeta?.thread_key) || threadKeys.has(chat.id))
    .flatMap((chat) => (Array.isArray(chat.messages) ? chat.messages : []).map((message) => ({
      id: message.id || `${chat.id}:${message.createdAt || message.commandId || message.taskId || ''}`,
      message_key: message.id || `${chat.id}:${message.createdAt || ''}`,
      observed_at: Number(message.createdAt || chat.updated_at_ms || 0),
      body: message.text || '',
      role: message.role || 'ctox',
      status: message.status || '',
      chat_id: chat.id,
    })));
}

function suggestionsFor(conversationId) {
  return (state.data.support_agent_suggestions || [])
    .filter((suggestion) => suggestion.conversation_id === conversationId)
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

// v4 command documents are {id, type, payload, status, app_id, actor_id, created_at, record_id?};
// older projected rows (module/command_type/client_context) are still read.
function businessCommandsFor(item) {
  return (state.data.business_commands || [])
    .filter((command) => {
      const moduleId = command.app_id || command.module || command.payload?.client_context?.module || command.client_context?.module || command.payload?.source_module;
      const recordId = command.record_id || command.payload?.client_context?.record_id || command.client_context?.record_id || command.payload?.record_id || command.payload?.conversation_id;
      const threadKey = command.payload?.thread_key || command.client_context?.thread_key;
      return moduleId === 'support'
        && (recordId === item.id || threadKey === `business-os/support/${item.id}`);
    })
    .sort((a, b) => commandTime(b) - commandTime(a));
}

function commandTime(command) {
  const parsed = Date.parse(String(command.created_at || ''));
  return Number(command.updated_at_ms || command.created_at_ms || 0) || (Number.isFinite(parsed) ? parsed : 0);
}

function queueTasksFor(item, commands = businessCommandsFor(item)) {
  const taskIds = new Set(commands.map((command) => command.task_id).filter(Boolean));
  return (state.data.ctox_queue_tasks || [])
    .filter((task) => taskIds.has(task.id)
      || task.thread_key === `business-os/support/${item.id}`
      || task.metadata?.business_os_record_id === item.id)
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
}

function customerAccountFor(item) {
  const accountId = item.customer_account_id || customerContactFor(item)?.account_id || '';
  if (!accountId) return null;
  return (state.data.customer_accounts || []).find((account) => account.id === accountId) || null;
}

function customerContactFor(item) {
  if (item.customer_contact_id) {
    const direct = (state.data.customer_contacts || []).find((contact) => contact.id === item.customer_contact_id);
    if (direct) return direct;
  }
  const link = (state.data.support_identity_links || [])
    .find((identity) => identity.customer_contact_id && identity.conversation_id === item.id);
  if (!link?.customer_contact_id) return null;
  return (state.data.customer_contacts || []).find((contact) => contact.id === link.customer_contact_id) || null;
}

function ticketFor(item) {
  if (!item.ticket_case_id) return null;
  return (state.data.ctox_ticket_cases || []).find((ticket) => ticket.id === item.ticket_case_id) || null;
}

function supportSnapshot(item) {
  return {
    conversation: item,
    thread_links: threadLinksFor(item),
    recent_messages: communicationMessagesFor(item).slice(-20),
    notes: notesFor(item.id).slice(-10),
    events: (state.data.support_conversation_events || []).filter((event) => event.conversation_id === item.id).slice(-20),
    suggestions: suggestionsFor(item.id).slice(0, 5),
    customer_account: customerAccountFor(item),
    customer_contact: customerContactFor(item),
    ticket: ticketFor(item),
  };
}

function timelineText(row) {
  const payload = row.payload || {};
  if (row.kind === 'note') return payload.body || '';
  if (row.kind === 'event') return payload.summary || payload.event_type || '';
  if (row.kind === 'agent_suggestion') return payload.summary || JSON.stringify(payload.payload || {});
  return payload.body_text || payload.preview || payload.body || payload.text || payload.subject || payload.status || '';
}

function timelineLabel(row) {
  const payload = row.payload || {};
  if (row.kind === 'note') return state.t('noteLabel', 'Interne Notiz');
  if (row.kind === 'event') return state.t('eventLabel', 'Ereignis');
  if (row.kind === 'agent_suggestion') return state.t('agentLabel', 'CTOX Vorschlag');
  if (row.kind === 'message' && payload.chat_id) return state.t('chatLabel', 'CTOX Chat');
  if (row.kind === 'message') {
    return [...new Set([wordLabel(MESSAGE_LABELS, payload.direction), wordLabel(MESSAGE_LABELS, payload.channel)].filter(Boolean))].join(' · ') || state.t('messageLabel', 'Nachricht');
  }
  return state.t('messageLabel', 'Nachricht');
}

// Konversationstitel wie Zendesk (Ticket-Betreff = Betreff der ersten Nachricht): Betreff der ersten Nachricht,
// sonst der Anfang ihres Texts (max. 60 Zeichen), sonst „Anfrage von <Name>“. Nie Suchtext oder eine ID.
const TITLE_MAX = 60;

function conversationLabel(item) {
  const first = firstMessageFor(item);
  const subject = cleanLine(first?.subject);
  if (subject) return clip(subject, TITLE_MAX);
  const body = cleanLine(first?.body_text || first?.preview || stripTags(first?.body_html));
  if (body) return clip(body, TITLE_MAX);
  const name = requesterName(item, first);
  return name ? state.t('requestFrom', 'Anfrage von {0}').replace('{0}', name) : state.t('requestUntitled', 'Anfrage');
}

// Erste Nachricht je Konversation über die Thread-Schlüssel; Index je Nachrichten-Array (eine Liste rendert
// viele Zeilen, das Filtern aller Nachrichten je Zeile wäre O(Zeilen × Nachrichten)).
let firstMessageIndex = { source: null, byThread: new Map() };
function firstMessageFor(item) {
  const messages = state.data.communication_messages || [];
  if (firstMessageIndex.source !== messages) {
    const byThread = new Map();
    for (const message of messages) {
      if (!message?.thread_key) continue;
      const prev = byThread.get(message.thread_key);
      if (!prev || timestampForMessage(message) < timestampForMessage(prev)) byThread.set(message.thread_key, message);
    }
    firstMessageIndex = { source: messages, byThread };
  }
  let first = null;
  for (const key of threadKeysFor(item)) {
    const candidate = firstMessageIndex.byThread.get(key);
    if (candidate && (!first || timestampForMessage(candidate) < timestampForMessage(first))) first = candidate;
  }
  return first;
}

function requesterName(item, first) {
  const contact = customerContactFor(item);
  const contactName = contactDisplayName(contact);
  if (contactName) return contactName;
  const sender = cleanLine(first?.sender_display);
  if (sender) return sender;
  const account = customerAccountFor(item);
  return cleanLine(account?.name || account?.display_name) || cleanLine(contact?.email || first?.sender_address);
}

function contactDisplayName(contact) {
  return cleanLine([contact?.first_name, contact?.last_name].filter(Boolean).join(' ') || contact?.name || contact?.display_name);
}

// Textdiät Regel 4: technische Referenzen (team-001, uuid, Hex) nie anzeigen; lesbare Werte bleiben.
function readableRef(value) {
  const v = cleanLine(value);
  if (!v) return '';
  if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v)) return '';
  if (/^[a-z]+(?:[-_][a-z]+)*[-_:]{1,2}[0-9a-z_-]*\d[0-9a-z_-]*$/i.test(v) || /^[0-9a-f]{12,}$/i.test(v)) return '';
  return v;
}

function cleanLine(value) {
  return String(value ?? '').replace(/\s+/g, ' ').trim();
}

function stripTags(html) {
  return String(html ?? '').replace(/<[^>]*>/g, ' ');
}

function clip(value, max) {
  if (value.length <= max) return value;
  const cut = value.slice(0, max - 1);
  const space = cut.lastIndexOf(' ');
  return (space > max * 0.6 ? cut.slice(0, space) : cut).replace(/[\s,.;:–-]+$/, '') + '…';
}

function isSlaRisk(item) {
  const dueAt = Number(item.sla_due_at_ms || item.resolution_due_at_ms || 0);
  return dueAt > 0 && dueAt - Date.now() <= 60 * 60 * 1000 && !isClosed(item);
}

function isClosed(item) {
  return ['resolved', 'closed', 'done'].includes(String(item.status || '').toLowerCase());
}

// Textdiät Regel 4: Status/Priorität/Kanal in deutschen Nutzerbegriffen (Zendesk DE); unbekannte Rohwerte bleiben
// unsichtbar statt als „needs_review“/„all“ in der Liste zu stehen.
const STATUS_LABELS = Object.freeze({
  de: { new: 'Neu', open: 'Offen', waiting: 'Wartend', pending: 'Wartend', on_hold: 'Angehalten', resolved: 'Gelöst', solved: 'Gelöst', closed: 'Geschlossen', completed: 'Erledigt', done: 'Erledigt', failed: 'Fehlgeschlagen', cancelled: 'Abgebrochen', snoozed: 'Zurückgestellt' },
  en: { new: 'New', open: 'Open', waiting: 'Waiting', pending: 'Pending', on_hold: 'On hold', resolved: 'Resolved', solved: 'Solved', closed: 'Closed', completed: 'Completed', done: 'Done', failed: 'Failed', cancelled: 'Cancelled', snoozed: 'Snoozed' }, // v4-allow: en; v4-allow: label (Ticketstatus, Fachbegriff)
});
const PRIORITY_LABELS = Object.freeze({
  de: { low: 'Niedrig', normal: 'Normal', high: 'Hoch', urgent: 'Dringend' },
  en: { low: 'Low', normal: 'Normal', high: 'High', urgent: 'Urgent' },
});
const MESSAGE_LABELS = Object.freeze({
  de: { inbound: 'Eingehend', outbound: 'Ausgehend', email: 'E-Mail', mail: 'E-Mail', chat: 'Chat', phone: 'Telefon', sms: 'SMS', whatsapp: 'WhatsApp', web: 'Web' },
  en: { inbound: 'Inbound', outbound: 'Outbound', email: 'Email', mail: 'Email', chat: 'Chat', phone: 'Phone', sms: 'SMS', whatsapp: 'WhatsApp', web: 'Web' },
});
function wordLabel(table, value) {
  const key = String(value || '').trim().toLowerCase();
  return (table[state.lang] || table.de)[key] || '';
}
function displayStatus(status) {
  return wordLabel(STATUS_LABELS, status || 'open');
}
function displayPriority(priority) {
  return wordLabel(PRIORITY_LABELS, priority || 'normal');
}

function fact(label, value) {
  const display = value || state.t('noValue', 'nicht gesetzt');
  return `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(display)}</dd>`;
}

function optionList(values, selected, labeler = (value) => value) {
  const normalizedSelected = String(selected || '').toLowerCase();
  return values.map((value) => `
    <option value="${escapeAttr(value)}" ${String(value).toLowerCase() === normalizedSelected ? 'selected' : ''}>${escapeHtml(labeler(value))}</option>
  `).join('');
}

function normalizeControlValue(value, fallback) {
  return String(value || fallback || '').trim().toLowerCase();
}

function rootEl() {
  return state.root?.querySelector('[data-support-root]') || null;
}

function formatTime(value) {
  const timestamp = Number(value || 0);
  if (!timestamp) return state.t('noValue', 'nicht gesetzt');
  return new Intl.DateTimeFormat(state.lang === 'en' ? 'en' : 'de', {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(timestamp));
}

function timestampForMessage(message) {
  const parsed = Date.parse(String(message.external_created_at || message.observed_at || ''));
  if (Number.isFinite(parsed)) return parsed;
  return Number(message.updated_at_ms || message.created_at_ms || 0);
}

function nextMorningMs() {
  const next = new Date();
  next.setDate(next.getDate() + 1);
  next.setHours(9, 0, 0, 0);
  return next.getTime();
}

function messageCountLabel(count) {
  const n = Number(count) || 0;
  if (state.lang === 'en') return `${n} ${n === 1 ? 'message' : 'messages'}`;
  return `${n} ${n === 1 ? 'Nachricht' : 'Nachrichten'}`;
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function escapeAttr(value) {
  return escapeHtml(value);
}

export const __supportTestHooks = {
  renderConversationRow,
  supportSnapshot,
  timelineRows,
  visibleConversations,
  bandCountsFor,
  computeContextVisible,
  buildSupportExport,
  parseSupportImport,
  importSupportText,
  businessCommandsFor,
  conversationLabel,
};
