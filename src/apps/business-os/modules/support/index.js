import { loadModuleMessages } from '../../shared/i18n.js';
import { SUPPORT_AGENT_SUGGESTION_KINDS, buildSupportAgentTaskCommand, buildSupportCommand } from './support-commands.mjs';
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

const PRIMARY_COLLECTION = 'support_conversations';
const SYNC_START_TIMEOUT_MS = 8000;
const REFRESH_DEBOUNCE_MS = 80;

const labels = {
  de: {
    kicker: 'Support Desk',
    queueTitle: 'Queues',
    conversationKicker: 'Conversation',
    emptyTitle: 'Keine Support-Konversation ausgewählt',
    contextKicker: 'Kontext',
    contextTitle: 'Kunde und Ticket',
    notePlaceholder: 'Interne Notiz',
    noteAction: 'Notiz hinzufügen',
    searchPlaceholder: 'Suche nach Kunde, Ticket oder Nachricht',
    allOpen: 'Offen',
    mine: 'Meine',
    unassigned: 'Unassigned',
    needsReply: 'Antwort nötig',
    slaRisk: 'SLA-Risiko',
    snoozed: 'Snoozed',
    agentDrafts: 'CTOX Entwürfe',
    loadingTitle: 'Support wird synchronisiert',
    loadingBody: 'Konversationen erscheinen, sobald die Support-Projektionen lokal verfügbar sind.',
    emptyListTitle: 'Keine Support-Konversationen',
    emptyListBody: 'Neue Support-Fälle entstehen aus Kommunikation, Tickets und Support-Connectoren.',
    emptyTimelineTitle: 'Keine Konversation ausgewählt',
    emptyTimelineBody: 'Wähle links eine Queue-Konversation.',
    noTimelineTitle: 'Noch keine Timeline',
    noTimelineBody: 'Nachrichten, interne Notizen und CTOX Vorschläge werden hier zusammengeführt.',
    status: 'Status',
    priority: 'Priorität',
    assignee: 'Assignee',
    team: 'Team',
    customer: 'Kunde',
    ticket: 'Ticket',
    inbox: 'Inbox',
    updated: 'Aktualisiert',
    noValue: 'nicht gesetzt',
    askCtox: 'CTOX fragen',
    claim: 'Übernehmen',
    resolve: 'Lösen',
    reopen: 'Wieder öffnen',
    assignToMe: 'Mir zuweisen',
    createTicket: 'Ticket erstellen',
    snoozeOneHour: '1h snoozen',
    snoozeTomorrow: 'Morgen',
    linkedThreads: 'Kommunikation',
    noMessages: 'Keine Nachrichten geladen.',
    relatedTicket: 'Verknüpftes Ticket',
    relatedCustomer: 'Verknüpfter Kunde',
    ctoxWork: 'CTOX Arbeit',
    applySuggestion: 'Anwenden',
    rejectSuggestion: 'Ablehnen',
    commandPending: 'Befehl wurde an CTOX übergeben.',
    commandFailed: 'Befehl konnte nicht übergeben werden.',
    noteLabel: 'Interne Notiz',
    eventLabel: 'Ereignis',
    agentLabel: 'CTOX Vorschlag',
    chatLabel: 'CTOX Chat',
    messageLabel: 'Nachricht',
  },
  en: {
    kicker: 'Support Desk',
    queueTitle: 'Queues',
    conversationKicker: 'Conversation',
    emptyTitle: 'No support conversation selected',
    contextKicker: 'Context',
    contextTitle: 'Customer and ticket',
    notePlaceholder: 'Internal note',
    noteAction: 'Add note',
    searchPlaceholder: 'Search customer, ticket, or message',
    allOpen: 'Open',
    mine: 'Mine',
    unassigned: 'Unassigned',
    needsReply: 'Needs reply',
    slaRisk: 'SLA risk',
    snoozed: 'Snoozed',
    agentDrafts: 'CTOX drafts',
    loadingTitle: 'Syncing support',
    loadingBody: 'Conversations appear once support projections are available locally.',
    emptyListTitle: 'No support conversations',
    emptyListBody: 'New support cases are created from communication, tickets, and support connectors.',
    emptyTimelineTitle: 'No conversation selected',
    emptyTimelineBody: 'Pick a queue conversation on the left.',
    noTimelineTitle: 'No timeline yet',
    noTimelineBody: 'Messages, internal notes, and CTOX suggestions are merged here.',
    status: 'Status',
    priority: 'Priority',
    assignee: 'Assignee',
    team: 'Team',
    customer: 'Customer',
    ticket: 'Ticket',
    inbox: 'Inbox',
    updated: 'Updated',
    noValue: 'not set',
    askCtox: 'Ask CTOX',
    claim: 'Claim',
    resolve: 'Resolve',
    reopen: 'Reopen',
    assignToMe: 'Assign to me',
    createTicket: 'Create ticket',
    snoozeOneHour: 'Snooze 1h',
    snoozeTomorrow: 'Tomorrow',
    linkedThreads: 'Communication',
    noMessages: 'No messages loaded.',
    relatedTicket: 'Linked ticket',
    relatedCustomer: 'Linked customer',
    ctoxWork: 'CTOX work',
    applySuggestion: 'Apply',
    rejectSuggestion: 'Reject',
    commandPending: 'Command was handed to CTOX.',
    commandFailed: 'Command could not be handed off.',
    noteLabel: 'Internal note',
    eventLabel: 'Event',
    agentLabel: 'CTOX suggestion',
    chatLabel: 'CTOX chat',
    messageLabel: 'Message',
  },
};

const state = {
  ctx: null,
  t: (key, fallback) => fallback || key,
  lang: 'de',
  queue: 'open',
  search: '',
  selectedId: '',
  loading: false,
  renderTimer: null,
  cleanup: null,
  conversationOverrides: new Map(),
  localNotes: [],
  data: Object.fromEntries(COLLECTIONS.map((name) => [name, []])),
};

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  const messages = await loadModuleMessages(import.meta.url, state.lang, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;
  state.loading = true;
  state.selectedId = '';
  state.queue = 'open';
  state.search = '';
  state.conversationOverrides = new Map();
  state.localNotes = [];
  state.data = Object.fromEntries(COLLECTIONS.map((name) => [name, []]));
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  applyStaticLabels();
  wireUi();
  render();
  await startCollections();
  await refreshSupport();
  state.cleanup = wireRealtime();
  return () => {
    state.cleanup?.();
    if (state.renderTimer) window.clearTimeout(state.renderTimer);
    state.ctx = null;
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-support-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.supportStyle = 'true';
  document.head.append(link);
}

function applyStaticLabels() {
  const root = rootEl();
  root.querySelectorAll('[data-i18n]').forEach((node) => {
    const key = node.getAttribute('data-i18n') || '';
    node.textContent = state.t(key, node.textContent || key);
  });
  root.querySelectorAll('[data-i18n-placeholder]').forEach((node) => {
    const key = node.getAttribute('data-i18n-placeholder') || '';
    node.setAttribute('placeholder', state.t(key, node.getAttribute('placeholder') || key));
  });
  const ask = root.querySelector('[data-support-ask-ctox]');
  ask?.setAttribute('title', state.t('askCtox', 'CTOX fragen'));
  ask?.setAttribute('aria-label', state.t('askCtox', 'CTOX fragen'));
}

function wireUi() {
  const root = rootEl();
  root.querySelector('[data-support-filters]')?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const filter = target?.closest('[data-support-filter]');
    const row = target?.closest('[data-support-conversation-id]');
    if (filter) {
      state.queue = filter.getAttribute('data-support-filter') || 'open';
      render();
      return;
    }
    if (row) {
      state.selectedId = row.getAttribute('data-support-conversation-id') || '';
      render();
    }
  });
  root.querySelector('[data-support-note-submit]')?.addEventListener('click', () => {
    createNote().catch((error) => setStatus(error?.message || String(error), true));
  });
  root.querySelector('[data-support-note]')?.addEventListener('input', () => renderComposerState());
  root.querySelector('[data-support-ask-ctox]')?.addEventListener('click', () => {
    askCtox().catch((error) => setStatus(error?.message || String(error), true));
  });
  root.querySelector('[data-support-context]')?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const suggestionAction = target?.closest('[data-support-suggestion-action]');
    if (suggestionAction) {
      runSuggestionAction(
        suggestionAction.getAttribute('data-support-suggestion-action') || '',
        suggestionAction.getAttribute('data-support-suggestion-id') || '',
      ).catch((error) => setStatus(error?.message || String(error), true));
      return;
    }
    const action = target?.closest('[data-support-action]');
    if (!action) return;
    runConversationAction(action.getAttribute('data-support-action') || '')
      .catch((error) => setStatus(error?.message || String(error), true));
  });
  root.querySelector('[data-support-context]')?.addEventListener('change', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const control = target?.closest('[data-support-control]');
    if (!control) return;
    runConversationControl(control.getAttribute('data-support-control') || '', control.value || '')
      .catch((error) => setStatus(error?.message || String(error), true));
  });
}

async function startCollections() {
  const sync = state.ctx?.sync;
  if (typeof sync?.startCollection !== 'function') return;
  const available = COLLECTIONS.filter((name) => collectionFor(name));
  if (!available.length) return;
  const primary = available.includes(PRIMARY_COLLECTION) ? PRIMARY_COLLECTION : available[0];
  await withTimeout(sync.startCollection(primary), SYNC_START_TIMEOUT_MS, null)
    .catch((error) => console.warn('[support] primary sync start failed', error));
  available
    .filter((name) => name !== primary)
    .forEach((name, index) => {
      window.setTimeout(() => {
        Promise.resolve(sync.startCollection(name))
          .catch((error) => console.warn(`[support] ${name} sync start failed`, error));
      }, index * 80);
    });
}

function wireRealtime() {
  const subscriptions = COLLECTIONS
    .map((name) => collectionFor(name)?.$?.subscribe?.(() => scheduleRefresh()))
    .filter(Boolean);
  return () => subscriptions.forEach((subscription) => {
    try { subscription.unsubscribe?.(); } catch {}
  });
}

function scheduleRefresh() {
  if (state.renderTimer) return;
  state.renderTimer = window.setTimeout(() => {
    state.renderTimer = null;
    refreshSupport().catch((error) => console.warn('[support] refresh failed', error));
  }, REFRESH_DEBOUNCE_MS);
}

async function refreshSupport() {
  const entries = await Promise.all(COLLECTIONS.map(async (name) => [name, await loadCollection(name)]));
  state.data = Object.fromEntries(entries);
  pruneOptimisticState();
  state.loading = false;
  const visible = visibleConversations();
  if (!state.selectedId || !visible.some((item) => item.id === state.selectedId)) {
    state.selectedId = visible[0]?.id || '';
  }
  render();
}

async function loadCollection(name) {
  const collection = collectionFor(name);
  if (!collection?.find) return [];
  const docs = await collection.find().exec();
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && !doc.is_deleted && !doc._deleted);
}

function collectionFor(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

function render() {
  renderQueues();
  renderTimeline();
  renderContext();
  renderComposerState();
}

function renderQueues() {
  const container = rootEl().querySelector('[data-support-filters]');
  if (!container) return;
  const conversations = conversationsWithOverrides();
  const counts = supportQueueCounts(conversations, Date.now(), currentUserId());
  const filters = [
    ['open', state.t('allOpen', 'Offen'), counts.open],
    ['mine', state.t('mine', 'Meine'), counts.mine],
    ['unassigned', state.t('unassigned', 'Unassigned'), counts.unassigned],
    ['needsReply', state.t('needsReply', 'Antwort nötig'), counts.needsReply],
    ['slaRisk', state.t('slaRisk', 'SLA-Risiko'), counts.slaRisk],
    ['snoozed', state.t('snoozed', 'Snoozed'), counts.snoozed],
    ['agentDrafts', state.t('agentDrafts', 'CTOX Entwürfe'), counts.agentDrafts],
  ];
  container.innerHTML = `
    <label class="support-search">
      <input type="search" data-support-search value="${escapeAttr(state.search)}" placeholder="${escapeAttr(state.t('searchPlaceholder', 'Suche'))}">
    </label>
    ${filters.map(([id, label, count]) => `
      <button type="button" class="support-filter-row ${id === state.queue ? 'is-active' : ''}" data-support-filter="${escapeAttr(id)}">
        <span>${escapeHtml(label)}</span>
        <small>${Number(count || 0)}</small>
      </button>
    `).join('')}
    <div class="support-conversation-list">
      ${renderConversationRows()}
    </div>
  `;
  container.querySelector('[data-support-search]')?.addEventListener('input', (event) => {
    state.search = event.target.value || '';
    render();
  });
}

function renderConversationRows() {
  if (state.loading && !state.data.support_conversations?.length) {
    return renderEmptyState(state.t('loadingTitle', 'Support wird synchronisiert'), state.t('loadingBody', ''));
  }
  const rows = visibleConversations();
  if (!rows.length) {
    return renderEmptyState(state.t('emptyListTitle', 'Keine Support-Konversationen'), state.t('emptyListBody', ''));
  }
  return rows.map((item) => {
    const selected = item.id === state.selectedId ? 'is-selected' : '';
    const label = conversationLabel(item);
    const risk = isSlaRisk(item) ? `<span class="support-badge is-risk">${escapeHtml(state.t('slaRisk', 'SLA'))}</span>` : '';
    const agent = Number(item.agent_draft_count || 0) > 0 ? `<span class="support-badge is-agent">${escapeHtml(state.t('agentDrafts', 'CTOX'))}</span>` : '';
    return `
      <button type="button" class="support-conversation-row ${selected}" data-support-conversation-id="${escapeAttr(item.id)}">
        <span class="support-row-meta">
          <span>${escapeHtml(item.inbox_id || state.t('inbox', 'Inbox'))}</span>
          <span>${escapeHtml(item.priority || 'normal')}</span>
        </span>
        <strong>${escapeHtml(label)}</strong>
        <span class="support-row-foot">
          <span>${escapeHtml(displayStatus(item.status))}</span>
          <span>${risk}${agent}</span>
        </span>
      </button>
    `;
  }).join('');
}

function renderTimeline() {
  const title = rootEl().querySelector('[data-support-title]');
  const timeline = rootEl().querySelector('[data-support-timeline]');
  const item = selectedConversation();
  if (title) title.textContent = item ? conversationLabel(item) : state.t('emptyTitle', 'Keine Support-Konversation ausgewählt');
  if (!timeline) return;
  if (!item) {
    timeline.innerHTML = renderEmptyState(state.t('emptyTimelineTitle', 'Keine Konversation ausgewählt'), state.t('emptyTimelineBody', ''));
    return;
  }
  const rows = timelineRows(item);
  if (!rows.length) {
    timeline.innerHTML = renderEmptyState(state.t('noTimelineTitle', 'Noch keine Timeline'), state.t('noTimelineBody', ''));
    return;
  }
  timeline.innerHTML = rows.map(renderTimelineItem).join('');
}

function renderTimelineItem(row) {
  const label = timelineLabel(row);
  return `
    <article class="support-timeline-item is-${escapeAttr(row.kind)}">
      <header>
        <strong>${escapeHtml(label)}</strong>
        <span>${escapeHtml(formatTime(row.at))}</span>
      </header>
      <p>${escapeHtml(timelineText(row))}</p>
    </article>
  `;
}

function renderContext() {
  const context = rootEl().querySelector('[data-support-context]');
  if (!context) return;
  const item = selectedConversation();
  if (!item) {
    context.innerHTML = renderEmptyState(state.t('contextTitle', 'Kunde und Ticket'), state.t('emptyTimelineBody', ''));
    return;
  }
  const suggestions = suggestionsFor(item.id);
  const account = customerAccountFor(item);
  const contact = customerContactFor(item);
  const ticket = ticketFor(item);
  const threadLinks = threadLinksFor(item);
  const messages = communicationMessagesFor(item);
  const commands = businessCommandsFor(item);
  const tasks = queueTasksFor(item, commands);
  const actorId = currentUserId();
  context.innerHTML = `
    <section class="support-context-section">
      <h3>${escapeHtml(state.t('contextTitle', 'Kunde und Ticket'))}</h3>
      <div class="support-facts">
        ${fact(state.t('status', 'Status'), displayStatus(item.status))}
        ${fact(state.t('priority', 'Priorität'), item.priority)}
        ${fact(state.t('assignee', 'Assignee'), item.assignee_id)}
        ${fact(state.t('team', 'Team'), item.team_id)}
        ${fact(state.t('customer', 'Kunde'), item.customer_account_id || item.customer_contact_id)}
        ${fact(state.t('ticket', 'Ticket'), item.ticket_case_id)}
        ${fact(state.t('updated', 'Aktualisiert'), formatTime(item.updated_at_ms || item.last_activity_at_ms))}
      </div>
    </section>
    <section class="support-context-section">
      <h3>${escapeHtml(state.t('status', 'Status'))}</h3>
      <div class="support-control-grid">
        <label>
          <span>${escapeHtml(state.t('status', 'Status'))}</span>
          <select data-support-control="status">
            ${optionList(STATUS_OPTIONS, normalizeControlValue(item.status, 'open'), displayStatus)}
          </select>
        </label>
        <label>
          <span>${escapeHtml(state.t('priority', 'Priorität'))}</span>
          <select data-support-control="priority">
            ${optionList(PRIORITY_OPTIONS, normalizeControlValue(item.priority, 'normal'))}
          </select>
        </label>
      </div>
      <div class="support-context-actions is-grid">
        <button type="button" data-support-action="claim">${escapeHtml(state.t('claim', 'Übernehmen'))}</button>
        <button type="button" data-support-action="assign-me" ${actorId ? '' : 'disabled'}>${escapeHtml(state.t('assignToMe', 'Mir zuweisen'))}</button>
        <button type="button" data-support-action="snooze-1h">${escapeHtml(state.t('snoozeOneHour', '1h snoozen'))}</button>
        <button type="button" data-support-action="snooze-tomorrow">${escapeHtml(state.t('snoozeTomorrow', 'Morgen'))}</button>
        <button type="button" data-support-action="${isClosed(item) ? 'reopen' : 'resolve'}">${escapeHtml(isClosed(item) ? state.t('reopen', 'Wieder öffnen') : state.t('resolve', 'Lösen'))}</button>
        <button type="button" data-support-action="create-ticket" ${item.ticket_case_id ? 'disabled' : ''}>${escapeHtml(state.t('createTicket', 'Ticket erstellen'))}</button>
      </div>
    </section>
    <section class="support-context-section">
      <h3>${escapeHtml(state.t('relatedCustomer', 'Verknüpfter Kunde'))}</h3>
      ${account || contact ? `
        <div class="support-facts">
          ${fact(state.t('customer', 'Kunde'), account?.name || account?.id || item.customer_account_id)}
          ${fact('Kontakt', contact?.name || contact?.display_name || contact?.email || item.customer_contact_id)}
          ${fact('E-Mail', contact?.email || account?.domain || '')}
        </div>
      ` : `<p class="support-status">${escapeHtml(state.t('noValue', 'nicht gesetzt'))}</p>`}
    </section>
    <section class="support-context-section">
      <h3>${escapeHtml(state.t('relatedTicket', 'Verknüpftes Ticket'))}</h3>
      ${ticket ? `
        <div class="support-facts">
          ${fact('ID', ticket.id || item.ticket_case_id)}
          ${fact('Titel', ticket.title || ticket.summary || ticket.id)}
          ${fact(state.t('status', 'Status'), ticket.status || ticket.state || '')}
        </div>
      ` : `<p class="support-status">${escapeHtml(item.ticket_case_id || state.t('noValue', 'nicht gesetzt'))}</p>`}
    </section>
    <section class="support-context-section">
      <h3>${escapeHtml(state.t('linkedThreads', 'Kommunikation'))}</h3>
      ${threadLinks.length ? threadLinks.map((link) => `
        <p class="support-linked-row">
          <strong>${escapeHtml(link.channel || link.link_role || 'thread')}</strong>
          <span>${escapeHtml(link.thread_key || '')}</span>
        </p>
      `).join('') : `<p class="support-status">${escapeHtml(item.primary_thread_key || state.t('noValue', 'nicht gesetzt'))}</p>`}
      <p class="support-status">${escapeHtml(messages.length ? localMessagesLoadedLabel(messages.length) : state.t('noMessages', 'Keine Nachrichten geladen.'))}</p>
    </section>
    <section class="support-context-section">
      <h3>${escapeHtml(state.t('agentLabel', 'CTOX Vorschlag'))}</h3>
      ${suggestions.length ? suggestions.slice(0, 3).map((suggestion) => `
        <div class="support-suggestion-row">
          <p>${escapeHtml(suggestion.summary || suggestion.suggestion_kind || suggestion.id)}</p>
          <small>${escapeHtml([suggestion.suggestion_kind, suggestion.status].filter(Boolean).join(' · '))}</small>
          ${['applied', 'rejected'].includes(String(suggestion.status || '').toLowerCase()) ? '' : `
            <div class="support-context-actions">
              <button type="button" data-support-suggestion-action="apply" data-support-suggestion-id="${escapeAttr(suggestion.id)}">${escapeHtml(state.t('applySuggestion', 'Anwenden'))}</button>
              <button type="button" data-support-suggestion-action="reject" data-support-suggestion-id="${escapeAttr(suggestion.id)}">${escapeHtml(state.t('rejectSuggestion', 'Ablehnen'))}</button>
            </div>
          `}
        </div>
      `).join('') : `<p class="support-status">${escapeHtml(state.t('noTimelineBody', ''))}</p>`}
    </section>
    <section class="support-context-section">
      <h3>${escapeHtml(state.t('ctoxWork', 'CTOX Arbeit'))}</h3>
      ${commands.length || tasks.length ? `
        ${commands.slice(0, 3).map((command) => `
          <p class="support-linked-row">
            <strong>${escapeHtml(command.command_type || command.type || 'command')}</strong>
            <span>${escapeHtml([command.status, command.task_status, command.task_id].filter(Boolean).join(' · '))}</span>
          </p>
        `).join('')}
        ${tasks.slice(0, 3).map((task) => `
          <p class="support-linked-row">
            <strong>${escapeHtml(task.title || task.id || 'task')}</strong>
            <span>${escapeHtml([task.status, task.task_status, task.id].filter(Boolean).join(' · '))}</span>
          </p>
        `).join('')}
      ` : `<p class="support-status">${escapeHtml(state.t('noValue', 'nicht gesetzt'))}</p>`}
    </section>
    <p class="support-status" data-support-status></p>
  `;
}

function renderComposerState() {
  const root = rootEl();
  const selected = Boolean(selectedConversation());
  const note = root.querySelector('[data-support-note]');
  const button = root.querySelector('[data-support-note-submit]');
  const ask = root.querySelector('[data-support-ask-ctox]');
  if (button) button.disabled = !selected || !String(note?.value || '').trim();
  if (ask) ask.disabled = !selected;
}

async function createNote() {
  const item = selectedConversation();
  const note = rootEl().querySelector('[data-support-note]');
  const body = String(note?.value || '').trim();
  if (!item || !body) return;
  const now = Date.now();
  const command = buildSupportCommand({
    type: 'support.note.create',
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
  setStatus(state.t('commandPending', 'Befehl wurde an CTOX übergeben.'));
  renderComposerState();
  try {
    await dispatchSupportCommand(command);
  } catch (error) {
    forgetLocalNote(optimisticNote.id);
    render();
    throw error;
  }
}

async function askCtox() {
  const item = selectedConversationWithCurrentControls();
  if (!item) return;
  const title = `Support: ${conversationLabel(item)}`;
  const instruction = buildAgentInstruction(item);
  const recordSnapshot = supportSnapshot(item);
  const writebackContract = supportWritebackContract(item.id);
  if (typeof state.ctx?.businessChat?.submitTask === 'function') {
    await state.ctx.businessChat.submitTask({
      module: 'support',
      recordId: item.id,
      title,
      instruction,
      prompt: instruction,
      userMessage: 'Summarize this support conversation and propose the next action.',
      requestKind: 'summary',
      threadKey: `business-os/support/${item.id}`,
      requiredSkills: ['business-os-support-workflow'],
      recordSnapshot,
      writebackContract,
      surface: 'support.agent.summary',
      open: true,
    });
  } else {
    await dispatchSupportCommand(buildSupportAgentTaskCommand({
      conversationId: item.id,
      title,
      instruction,
      requestKind: 'summary',
      recordSnapshot,
    }));
  }
  setStatus(state.t('commandPending', 'Befehl wurde an CTOX übergeben.'));
}

async function runConversationAction(action) {
  const item = selectedConversation();
  if (!item) return;
  if (action === 'assign-me') {
    const assigneeId = currentUserId();
    if (!assigneeId) throw new Error(state.t('commandFailed', 'Befehl konnte nicht übergeben werden.'));
    rememberConversationOverride(item.id, { assignee_id: assigneeId });
    render();
    try {
      await dispatchSupportCommand(buildSupportCommand({
        type: 'support.conversation.assign',
        recordId: item.id,
        payload: { conversation_id: item.id, assignee_id: assigneeId },
        surface: 'support.conversation.assign',
      }));
    } catch (error) {
      forgetConversationOverrideFields(item.id, ['assignee_id']);
      render();
      throw error;
    }
    setStatus(state.t('commandPending', 'Befehl wurde an CTOX übergeben.'));
    return;
  }
  if (action === 'snooze-1h' || action === 'snooze-tomorrow') {
    const snoozedUntilMs = action === 'snooze-1h' ? Date.now() + 60 * 60 * 1000 : nextMorningMs();
    rememberConversationOverride(item.id, { snoozed_until_ms: snoozedUntilMs });
    render();
    try {
      await dispatchSupportCommand(buildSupportCommand({
        type: 'support.conversation.snooze',
        recordId: item.id,
        payload: {
          conversation_id: item.id,
          snoozed_until_ms: snoozedUntilMs,
        },
        surface: 'support.conversation.snooze',
      }));
    } catch (error) {
      forgetConversationOverrideFields(item.id, ['snoozed_until_ms']);
      render();
      throw error;
    }
    setStatus(state.t('commandPending', 'Befehl wurde an CTOX übergeben.'));
    return;
  }
  if (action === 'create-ticket') {
    await dispatchSupportCommand(buildSupportCommand({
      type: 'support.ticket.create_from_conversation',
      recordId: item.id,
      payload: {
        conversation_id: item.id,
        title: conversationLabel(item),
        summary: timelineRows(item).slice(-6).map(timelineText).filter(Boolean).join('\n\n'),
      },
      surface: 'support.ticket.create_from_conversation',
    }));
    setStatus(state.t('commandPending', 'Befehl wurde an CTOX übergeben.'));
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
  try {
    await dispatchSupportCommand(buildSupportCommand({
      type,
      recordId: item.id,
      payload: { conversation_id: item.id },
      surface: type,
    }));
  } catch (error) {
    forgetConversationOverrideFields(item.id, Object.keys(optimisticPatch));
    render();
    throw error;
  }
  setStatus(state.t('commandPending', 'Befehl wurde an CTOX übergeben.'));
}

async function runConversationControl(control, value) {
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
  try {
    await dispatchSupportCommand(buildSupportCommand({
      type,
      recordId: item.id,
      payload: { conversation_id: item.id, ...payload },
      surface: type,
    }));
  } catch (error) {
    forgetConversationOverrideFields(item.id, Object.keys(payload));
    render();
    throw error;
  }
  setStatus(state.t('commandPending', 'Befehl wurde an CTOX übergeben.'));
}

async function runSuggestionAction(action, suggestionId) {
  const item = selectedConversation();
  if (!item || !suggestionId) return;
  const type = action === 'reject'
    ? 'support.agent.reject_suggestion'
    : 'support.agent.apply_suggestion';
  await dispatchSupportCommand(buildSupportCommand({
    type,
    recordId: item.id,
    payload: {
      conversation_id: item.id,
      suggestion_id: suggestionId,
    },
    surface: type,
  }));
  setStatus(state.t('commandPending', 'Befehl wurde an CTOX übergeben.'));
}

async function dispatchSupportCommand(command) {
  if (!state.ctx?.commandBus?.dispatch) {
    throw new Error(state.t('commandFailed', 'Befehl konnte nicht übergeben werden.'));
  }
  return state.ctx.commandBus.dispatch(command);
}

function visibleConversations() {
  const userId = currentUserId();
  if (state.queue === 'mine' && !userId) return [];
  const base = filterSupportConversations(conversationsWithOverrides(), {
    status: queueStatusFilter(state.queue),
    assigneeId: state.queue === 'mine' ? userId : state.queue === 'unassigned' ? 'unassigned' : '',
    query: state.search,
  });
  if (state.queue === 'needsReply') {
    return base.filter((item) => Number(item.unread_count || 0) > 0 || Number(item.waiting_since_ms || 0) > 0);
  }
  if (state.queue === 'slaRisk') return base.filter(isSlaRisk);
  if (state.queue === 'agentDrafts') {
    const suggestionIds = new Set((state.data.support_agent_suggestions || []).map((item) => item.conversation_id));
    return base.filter((item) => suggestionIds.has(item.id) || Number(item.agent_draft_count || 0) > 0);
  }
  return base;
}

function queueStatusFilter(queue) {
  if (!queue || ['mine', 'unassigned', 'needsReply', 'slaRisk', 'agentDrafts'].includes(queue)) return 'open';
  return queue;
}

function selectedConversation() {
  return conversationsWithOverrides().find((item) => item.id === state.selectedId) || null;
}

function selectedConversationWithCurrentControls() {
  const item = selectedConversation();
  if (!item) return null;
  const root = rootEl();
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
  return String(state.ctx?.session?.user?.id || state.ctx?.session?.userId || '').trim();
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

function businessCommandsFor(item) {
  return (state.data.business_commands || [])
    .filter((command) => {
      const moduleId = command.module || command.client_context?.module || command.payload?.source_module;
      const recordId = command.record_id || command.client_context?.record_id || command.payload?.record_id;
      const threadKey = command.payload?.thread_key || command.client_context?.thread_key;
      return moduleId === 'support'
        && (recordId === item.id || threadKey === `business-os/support/${item.id}`);
    })
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
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

function supportWritebackContract(conversationId) {
  return {
    command_type: 'support.agent.writeback',
    collection: 'support_agent_suggestions',
    record_id: conversationId,
    source_collection: 'support_conversations',
    allowed_suggestion_kinds: [...SUPPORT_AGENT_SUGGESTION_KINDS],
    required_human_action: 'review',
  };
}

function buildAgentInstruction(item) {
  return [
    'Analysiere diese CTOX Support-Konversation.',
    'Liefere eine knappe Zusammenfassung, Risiko-/SLA-Einschaetzung, Antwortentwurf und naechste Aktion.',
    'Nutze den writeback_contract aus dem Command und schreibe strukturierte Vorschlaege fuer support.agent.writeback zurueck.',
    `Conversation: ${item.id}`,
    `Status: ${item.status || 'open'}`,
    `Priority: ${item.priority || 'normal'}`,
  ].join('\n');
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
    return [payload.direction, payload.channel].filter(Boolean).join(' · ') || state.t('messageLabel', 'Nachricht');
  }
  return state.t('messageLabel', 'Nachricht');
}

function conversationLabel(item) {
  return item.search_text
    || item.customer_account_id
    || item.customer_contact_id
    || item.ticket_case_id
    || item.primary_thread_key
    || item.id
    || 'Support';
}

function isSlaRisk(item) {
  const dueAt = Number(item.sla_due_at_ms || item.resolution_due_at_ms || 0);
  return dueAt > 0 && dueAt - Date.now() <= 60 * 60 * 1000 && !isClosed(item);
}

function isClosed(item) {
  return ['resolved', 'closed', 'done'].includes(String(item.status || '').toLowerCase());
}

function displayStatus(status) {
  return String(status || 'open').replace(/_/g, ' ');
}

function fact(label, value) {
  const display = value || state.t('noValue', 'nicht gesetzt');
  return `
    <div class="support-fact">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(display)}</strong>
    </div>
  `;
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

function setStatus(message, isError = false) {
  const status = rootEl().querySelector('[data-support-status]');
  if (!status) return;
  status.textContent = message || '';
  status.classList.toggle('is-error', Boolean(isError));
}

function renderEmptyState(title, body) {
  return `
    <div class="support-empty">
      <strong>${escapeHtml(title)}</strong>
      <span>${escapeHtml(body)}</span>
    </div>
  `;
}

function rootEl() {
  return state.ctx.host.querySelector('[data-support-root]');
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

function localMessagesLoadedLabel(count) {
  return state.lang === 'en'
    ? `${count} local messages loaded`
    : `${count} Nachrichten lokal geladen`;
}

function withTimeout(promise, timeoutMs, fallback) {
  return Promise.race([
    Promise.resolve(promise),
    new Promise((resolve, reject) => {
      window.setTimeout(() => {
        if (fallback === null) resolve(null);
        else reject(new Error(fallback || 'timeout'));
      }, timeoutMs);
    }),
  ]);
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
  buildAgentInstruction,
  supportSnapshot,
  timelineRows,
  visibleConversations,
};
