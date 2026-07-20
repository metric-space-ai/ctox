import { loadModuleMessages } from '../../shared/i18n.js';
import {
  THREAD_COLLECTIONS,
  buildApprovalRequestPayload,
  buildNotePayload,
  buildThreadsCommand,
  splitUserIds,
} from './commands.js';

const THREAD_LIST_LIMIT = 200;
const THREAD_DETAIL_LIMIT = 600;
const APPROVAL_LIST_LIMIT = 200;
const NOTIFICATION_LIST_LIMIT = 50;

const labels = {
  de: {
    refresh: 'Aktualisieren',
    search: 'Threads suchen',
    noThreads: 'Keine relevanten Threads vorhanden.',
    noSelection: 'Kein Thread ausgewählt.',
    commandFailed: 'Aktion konnte nicht abgeschlossen werden.',
  },
  en: {
    refresh: 'Refresh',
    search: 'Search threads',
    noThreads: 'No relevant threads.',
    noSelection: 'No thread selected.',
    commandFailed: 'Action could not be completed.',
  },
};

const state = {
  ctx: null,
  t: (key, fallback) => fallback || key,
  filter: 'inbox',
  search: '',
  selectedId: '',
  mobileView: 'list',
  requestedThreadId: '',
  data: emptyData(),
  cleanup: [],
  busy: false,
  status: '',
};

let els = {};

export async function mount(ctx) {
  state.ctx = ctx;
  state.filter = 'inbox';
  state.listView = false;
  state.search = '';
  state.selectedId = '';
  state.mobileView = 'list';
  state.requestedThreadId = String(ctx.args?.thread_id || ctx.args?.thread || '').trim();
  state.data = emptyData();
  state.status = '';

  const messages = await loadModuleMessages(import.meta.url, ctx.locale || 'de', labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();

  bindElements(ctx.host);
  applyLabels();
  wireUi();
  restoreDraft();
  updateConnectivity();
  state.cleanup.push(wireRealtime());
  // Presence (advisory UX): publish which thread this user has open and show
  // who else is looking at the same thread. Cleared on unmount.
  state.presenceRemote = [];
  if (ctx.presence?.subscribe) {
    state.cleanup.push(ctx.presence.subscribe((entries) => {
      state.presenceRemote = Array.isArray(entries) ? entries : [];
      updateThreadPresenceHint(state.data.threads.find((item) => item.id === state.selectedId) || null);
    }));
    state.cleanup.push(() => { try { ctx.presence.clear(); } catch {} });
  }
  // The shell already starts manifest collections before mount. Do not keep
  // the window-manager open promise pending while large thread collections
  // catch up: the functional shell is usable immediately and data fills in
  // asynchronously. A direct/dev mount still gets an explicit sync attempt.
  render();
  // The shell-owned module lease starts every manifest collection. A second
  // fire-and-forget start can finish after a fast window close and recreate an
  // unowned bridge, so module mounts must not start the same collections.
  refresh().catch((error) => showError(error));
  [5000].forEach((delayMs) => {
    const timer = window.setTimeout(() => {
      refresh().catch((error) => showError(error));
    }, delayMs);
    state.cleanup.push(() => window.clearTimeout(timer));
  });

  return () => {
    state.cleanup.forEach((fn) => {
      try { fn?.(); } catch {}
    });
    state.cleanup = [];
  };
}

function emptyData() {
  return {
    threads: [],
    messages: [],
    links: [],
    notifications: [],
    approvals: [],
    commands: [],
    queue: [],
    states: [],
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-threads-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  const styleUrl = new URL('./index.css', import.meta.url);
  // Inherit the module's own cache-buster (index.js is imported with
  // ?v=<build>): fresh JS must never render against a stale cached sheet.
  const version = String(import.meta.url).split('?v=')[1] || '20260720-threads-grammar-v2';
  styleUrl.searchParams.set('v', version);
  link.href = styleUrl.href;
  link.dataset.threadsStyle = 'true';
  document.head.append(link);
}

function bindElements(root) {
  els.root = root.querySelector('[data-threads-root]');
  els.refresh = root.querySelector('[data-refresh]');
  els.search = root.querySelector('[data-thread-search]');
  els.filters = [...root.querySelectorAll('[data-filter]')];
  els.list = root.querySelector('[data-thread-list]');
  els.briefing = root.querySelector('[data-personal-briefing]');
  els.title = root.querySelector('[data-thread-title]');
  els.source = root.querySelector('[data-thread-source]');
  els.status = root.querySelector('[data-thread-status]');
  els.timeline = root.querySelector('[data-thread-timeline]');
  els.context = root.querySelector('[data-thread-context]');
  els.messageForm = root.querySelector('[data-message-form]');
  els.messageBody = root.querySelector('[data-message-body]');
  els.noteForm = root.querySelector('[data-note-form]');
  els.noteTarget = root.querySelector('[data-note-target]');
  els.noteBody = root.querySelector('[data-note-body]');
  els.approvalForm = root.querySelector('[data-approval-form]');
  els.approvalReviewer = root.querySelector('[data-approval-reviewer]');
  els.approvalPrompt = root.querySelector('[data-approval-prompt]');
  els.watch = root.querySelector('[data-thread-watch]');
  els.snooze = root.querySelector('[data-thread-snooze]');
  els.archive = root.querySelector('[data-thread-archive]');
  els.toggleActions = root.querySelector('[data-toggle-actions]');
  els.syncState = root.querySelector('[data-sync-state]');
  els.mobileBack = root.querySelector('[data-mobile-back]');
  els.mobileReply = root.querySelector('[data-mobile-reply]');
  els.mobileSnooze = root.querySelector('[data-mobile-snooze]');
  els.mobileMore = root.querySelector('[data-mobile-more]');
  els.claim = root.querySelector('[data-thread-claim]');
  els.handoffForm = root.querySelector('[data-handoff-form]');
  els.handoffTarget = root.querySelector('[data-handoff-target]');
  els.handoffBody = root.querySelector('[data-handoff-body]');
  els.handoffDue = root.querySelector('[data-handoff-due]');
  els.handoffReturnReason = root.querySelector('[data-handoff-return-reason]');
  els.aiForm = root.querySelector('[data-ai-form]');
  els.aiGoal = root.querySelector('[data-ai-goal]');
  els.notificationPreferencesForm = root.querySelector('[data-notification-preferences-form]');
  els.notificationThreshold = root.querySelector('[data-notification-threshold]');
  els.quietStart = root.querySelector('[data-quiet-start]');
  els.quietEnd = root.querySelector('[data-quiet-end]');
  els.notificationApprovals = root.querySelector('[data-notification-approvals]');
  els.notificationMentions = root.querySelector('[data-notification-mentions]');
  els.notificationEscalations = root.querySelector('[data-notification-escalations]');
}

function applyLabels() {
  if (els.refresh) {
    els.refresh.title = state.t('refresh', 'Aktualisieren');
    els.refresh.setAttribute('aria-label', state.t('refresh', 'Aktualisieren'));
  }
  if (els.search) {
    els.search.placeholder = state.t('search', 'Threads suchen');
    els.search.setAttribute('aria-label', state.t('search', 'Threads suchen'));
  }
  // Approval / delegation labels (the static index.html copy ships German;
  // translate the approval-flow strings through the module message catalog).
  const setFilterText = (filter, key, fb) => {
    const btn = els.filters?.find((b) => b.dataset.filter === filter);
    if (btn) btn.textContent = state.t(key, fb);
  };
  setFilterText('approvals', 'filterApprovals', 'Freigaben');
  setFilterText('delegated', 'filterDelegated', 'Delegiert');
  if (els.approvalForm) {
    const heading = els.approvalForm.closest('.threads-panel')?.querySelector('h3');
    if (heading) heading.textContent = state.t('approvalPanelTitle', 'CTOX Freigabe');
    const labelSpans = els.approvalForm.querySelectorAll('label > span');
    if (labelSpans[0]) labelSpans[0].textContent = state.t('approvalReviewerLabel', 'Reviewer');
    if (labelSpans[1]) labelSpans[1].textContent = state.t('approvalPromptLabel', 'Prompt');
    if (els.approvalReviewer) {
      els.approvalReviewer.placeholder = state.t('approvalReviewerPlaceholder', 'erfahrener-user-id');
    }
    if (els.approvalPrompt) {
      els.approvalPrompt.placeholder = state.t('approvalPromptPlaceholder', 'Was CTOX nach Freigabe tun soll');
    }
    const submitBtn = els.approvalForm.querySelector('button[type="submit"]');
    if (submitBtn) submitBtn.textContent = state.t('approvalRequestSubmit', 'Freigabe anfragen');
  }
}

function wireUi() {
  els.refresh?.addEventListener('click', () => refresh({ restartSync: true }));
  els.search?.addEventListener('input', (event) => {
    state.search = event.target.value || '';
    syncSelection();
    render();
  });
  els.filters.forEach((button) => {
    button.addEventListener('click', () => {
      setThreadFilter(button.dataset.filter || 'inbox');
    });
  });
  // Filter tray: toggle, secondary-view dropdown, reset.
  els.root?.querySelector('[data-toggle-filters]')?.addEventListener('click', (event) => {
    const btn = event.currentTarget;
    const panel = els.root.querySelector('[data-filter-advanced]');
    if (!panel) return;
    const open = panel.hasAttribute('hidden');
    if (open) panel.removeAttribute('hidden'); else panel.setAttribute('hidden', '');
    btn.setAttribute('aria-expanded', String(open));
  });
  els.root?.querySelector('select[data-filter-select]')?.addEventListener('change', (event) => {
    setThreadFilter(event.target.value || 'inbox');
  });
  els.root?.querySelector('[data-reset-filters]')?.addEventListener('click', () => {
    if (els.search) els.search.value = '';
    state.search = '';
    setThreadFilter('inbox');
  });
  // Shard cards vs. compact list rendering of the thread well.
  els.root?.querySelectorAll('[data-view-mode]').forEach((button) => {
    button.addEventListener('click', () => {
      state.listView = button.dataset.viewMode === 'list';
      els.root.querySelectorAll('[data-view-mode]').forEach((b) => b.setAttribute('aria-pressed', String((b.dataset.viewMode === 'list') === state.listView)));
      els.root.classList.toggle('is-list-view', state.listView);
    });
  });
  // Same toggle for the main view: timeline as cards or compact rows.
  els.root?.querySelectorAll('[data-center-view]').forEach((button) => {
    button.addEventListener('click', () => {
      state.centerListView = button.dataset.centerView === 'list';
      els.root.querySelectorAll('[data-center-view]').forEach((b) => b.setAttribute('aria-pressed', String((b.dataset.centerView === 'list') === state.centerListView)));
      els.root.classList.toggle('is-center-list-view', state.centerListView);
    });
  });
  els.list?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const row = target?.closest?.('[data-thread-id]');
    if (!row) return;
    state.selectedId = row.getAttribute('data-thread-id') || '';
    state.mobileView = 'detail';
    persistNavigationState();
    render();
  });
  els.context?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const deepLink = target?.closest?.('[data-thread-deep-link]');
    const mark = target?.closest?.('[data-mark-notification]');
    const dismiss = target?.closest?.('[data-dismiss-notification]');
    if (deepLink) {
      event.preventDefault();
      navigateDeepLink(deepLink.getAttribute('data-thread-deep-link') || deepLink.getAttribute('href') || '');
    } else if (mark) {
      updateNotification(mark.getAttribute('data-mark-notification') || '', 'mark_read').catch(showError);
    } else if (dismiss) {
      updateNotification(dismiss.getAttribute('data-dismiss-notification') || '', 'dismiss').catch(showError);
    }
  });
  els.timeline?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const approve = target?.closest?.('[data-approve-approval]');
    const reject = target?.closest?.('[data-reject-approval]');
    const edit = target?.closest?.('[data-edit-approval]');
    if (approve) {
      decideApproval(approve.getAttribute('data-approve-approval') || '', 'approve').catch(showError);
    } else if (reject) {
      decideApproval(reject.getAttribute('data-reject-approval') || '', 'reject').catch(showError);
    } else if (edit) {
      editApproval(edit.getAttribute('data-edit-approval') || '').catch(showError);
    }
  });
  els.watch?.addEventListener('click', () => toggleWatch().catch(showError));
  els.snooze?.addEventListener('click', () => snoozeSelectedThread().catch(showError));
  els.archive?.addEventListener('click', () => archiveSelectedThread().catch(showError));
  els.toggleActions?.addEventListener('click', () => {
    const hidden = els.root?.classList.toggle('is-actions-hidden');
    els.toggleActions.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  });
  els.mobileBack?.addEventListener('click', () => {
    state.mobileView = 'list';
    persistNavigationState();
    render();
  });
  els.mobileReply?.addEventListener('click', () => els.messageBody?.focus());
  els.mobileSnooze?.addEventListener('click', () => snoozeSelectedThread().catch(showError));
  els.mobileMore?.addEventListener('click', () => els.root?.classList.toggle('is-context-open'));
  els.claim?.addEventListener('click', () => claimSelectedThread().catch(showError));
  els.messageBody?.addEventListener('input', persistDraft);
  els.messageForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitMessage().catch(showError);
  });
  els.noteForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitNote().catch(showError);
  });
  els.approvalForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitApprovalRequest().catch(showError);
  });
  els.handoffForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitHandoff().catch(showError);
  });
  els.aiForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitAiRequest().catch(showError);
  });
  els.notificationPreferencesForm?.addEventListener('submit', (event) => {
    event.preventDefault();
    submitNotificationPreferences().catch(showError);
  });
  const onConnectivity = () => updateConnectivity();
  window.addEventListener('online', onConnectivity);
  window.addEventListener('offline', onConnectivity);
  state.cleanup.push(() => window.removeEventListener('online', onConnectivity));
  state.cleanup.push(() => window.removeEventListener('offline', onConnectivity));
  const onHash = () => {
    const params = new URLSearchParams(location.hash.split('?')[1] || '');
    const requested = params.get('thread_id') || params.get('thread') || '';
    if (requested) {
      state.requestedThreadId = requested;
      syncSelection();
      render();
    }
  };
  window.addEventListener('hashchange', onHash);
  state.cleanup.push(() => window.removeEventListener('hashchange', onHash));
}

function wireRealtime() {
  // Demand-query writes also emit collection changes. Subscribing a refresh
  // to those same queries creates a fetch -> change -> refresh feedback loop
  // and continuously replaces clickable approval controls. A short bounded
  // poll keeps cross-profile updates visible without render churn.
  const timer = window.setInterval(() => {
    refresh().catch((error) => console.warn('[threads] refresh failed', error));
  }, 10000);
  return () => window.clearInterval(timer);
}

async function refresh(options = {}) {
  if (options.restartSync) startSync().catch((error) => showError(error));
  const me = currentUserId();
  const [recentThreads, pendingApprovals, recentApprovals, states] = await Promise.all([
    loadCollection('user_threads', recentQuery(THREAD_LIST_LIMIT)),
    loadCollection('ctox_task_approval_requests', recentQuery(APPROVAL_LIST_LIMIT, { status: 'pending' })),
    loadCollection('ctox_task_approval_requests', recentQuery(APPROVAL_LIST_LIMIT)),
    loadCollection('user_thread_states', recentQuery(THREAD_LIST_LIMIT, me ? { user_id: me } : {})),
  ]);
  const approvalCandidates = mergeRecords(pendingApprovals, recentApprovals);
  const pendingCandidateIds = approvalCandidates
    .filter((item) => item.status === 'pending')
    .slice(0, 20)
    .map((item) => item.id || item.approval_request_id)
    .filter(Boolean);
  const verifiedPendingCandidates = await loadRecordsByIds(
    'ctox_task_approval_requests',
    pendingCandidateIds,
  );
  const approvals = mergeRecords(approvalCandidates, verifiedPendingCandidates);
  const approvalThreadIds = approvals.map((item) => item.thread_id).filter(Boolean);
  const approvalThreads = await loadRecordsByIds('user_threads', approvalThreadIds);
  const threads = mergeRecords(recentThreads, approvalThreads);
  const threadIds = threads.map((item) => item.id || item.thread_id).filter(Boolean);
  const [messages, links, notifications] = await Promise.all([
    loadCollection('user_thread_messages', relatedQuery('thread_id', threadIds, THREAD_DETAIL_LIMIT)),
    loadCollection('user_thread_links', relatedQuery('thread_id', threadIds, THREAD_DETAIL_LIMIT)),
    loadCollection('user_notifications', recentQuery(
      NOTIFICATION_LIST_LIMIT,
      me ? { user_id: me } : {},
    )),
  ]);
  const base = { threads, messages, links, notifications, approvals, states };
  notifyActionRequired(notifications);
  // Render the collaborative state before optional command/task enrichment.
  // Historical tracking lookups must never hold the inbox or an approval
  // decision card behind dozens of unrelated command-id demand reads.
  state.data = { ...base, commands: [], queue: [] };
  syncSelection();
  render();

  const selectedThreadId = state.selectedId;
  const selectedBase = {
    threads: base.threads.filter((item) => item.id === selectedThreadId),
    messages: base.messages.filter((item) => item.thread_id === selectedThreadId),
    links: base.links.filter((item) => item.thread_id === selectedThreadId),
    notifications: base.notifications.filter((item) => item.thread_id === selectedThreadId),
    approvals: base.approvals.filter((item) => item.thread_id === selectedThreadId),
  };
  const commandIds = linkedCommandIds(selectedBase).slice(0, 10);
  const commands = await loadRecordsByIds('business_commands', commandIds);
  const taskIds = linkedTaskIds(selectedBase, commands).slice(0, 10);
  const queue = await loadRecordsByIds('ctox_queue_tasks', taskIds);
  state.data = {
    ...base,
    commands,
    queue,
  };
  syncSelection();
  render();
}

async function loadCollection(name, query = {}) {
  const collection = collectionFor(name);
  if (!collection?.find) return [];
  const docs = await collection.find(query).exec();
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
}

function collectionFor(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

async function loadRecordsByIds(name, ids) {
  const collection = collectionFor(name);
  const uniqueIds = [...new Set((ids || []).map((id) => String(id || '').trim()).filter(Boolean))];
  if (!collection?.findOne || !uniqueIds.length) return [];
  // Primary-key lookups use the optimized single-document demand window.
  // A Mango `$in` query over `id` scans large native collections and can hold
  // the shared query collector until its transport deadline.
  const docs = await Promise.all(uniqueIds.map((id) => collection.findOne(id).exec().catch(() => null)));
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
}

function recentQuery(limit, selector = {}) {
  return {
    selector,
    sort: [{ updated_at_ms: 'desc' }],
    limit,
  };
}

function relatedQuery(field, ids, limit) {
  const uniqueIds = [...new Set((ids || []).map((id) => String(id || '').trim()).filter(Boolean))];
  if (!uniqueIds.length) return { selector: { id: '__ctox_no_record__' }, limit: 1 };
  return {
    selector: { [field]: { $in: uniqueIds } },
    sort: [{ updated_at_ms: 'desc' }],
    limit,
  };
}

function mergeRecords(...groups) {
  const byId = new Map();
  groups.flat().forEach((item) => {
    const id = String(item?.id || item?.thread_id || item?.approval_request_id || '').trim();
    if (id) byId.set(id, item);
  });
  return [...byId.values()].sort((a, b) => Number(b?.updated_at_ms || 0) - Number(a?.updated_at_ms || 0));
}

function render() {
  els.root?.classList.toggle('is-mobile-detail', state.mobileView === 'detail' && Boolean(state.selectedId));
  const threads = visibleThreads();
  renderBriefing();
  renderNotificationPreferences();
  updateFilterControls(threads.length);
  renderList(threads);
  renderDetail(threads);
}

// The primary tab a filter maps to; secondary filters live in the tray only.
const PRIMARY_FILTERS = ['inbox', 'waiting', 'running', 'archived'];
const FILTER_LABELS = {
  inbox: 'Jetzt handeln', waiting: 'Wartet auf mich', running: 'AI arbeitet',
  delegated: 'Wartet auf andere', snoozed: 'Später', team: 'Team Queue',
  mentions: 'Erwähnungen', approvals: 'Freigaben', failed: 'Blockiert',
  archived: 'Erledigt / Archiv', all: 'Alle',
};

function setThreadFilter(filter) {
  state.filter = filter || 'inbox';
  syncSelection();
  render();
}

function updateFilterControls(visibleCount) {
  const root = els.root;
  if (!root) return;
  const me = currentUserId();
  const isAdmin = currentUserRole() === 'chef' || currentUserRole() === 'admin';
  // Counts per primary queue, computed with the same predicate the tab uses.
  for (const filter of ['inbox', 'waiting', 'running']) {
    const el = root.querySelector(`[data-count-${filter}]`);
    if (el) el.textContent = ` (${state.data.threads.filter((thread) => threadMatchesFilter(thread, filter, me, isAdmin)).length})`;
  }
  for (const button of els.filters) {
    button.setAttribute('aria-selected', String(button.dataset.filter === state.filter));
  }
  const select = root.querySelector('select[data-filter-select]');
  if (select && select.value !== state.filter) select.value = state.filter;
  // Accent dot on the filter toggle whenever a tray-only filter is active —
  // the band alone would otherwise show no selection at all.
  root.querySelector('[data-toggle-filters]')?.classList.toggle('has-active-filters', !PRIMARY_FILTERS.includes(state.filter));
  const footer = root.querySelector('[data-threads-footer-count]');
  if (footer) {
    footer.textContent = `${visibleCount} ${visibleCount === 1 ? 'Thread' : 'Threads'} · ${FILTER_LABELS[state.filter] || state.filter}`;
  }
}

function renderNotificationPreferences() {
  const prefs = personalPreferences();
  if (!prefs || els.notificationPreferencesForm?.contains(document.activeElement)) return;
  if (els.notificationThreshold) els.notificationThreshold.value = String(prefs.priority_threshold ?? 20);
  if (els.quietStart) els.quietStart.value = prefs.quiet_start || '';
  if (els.quietEnd) els.quietEnd.value = prefs.quiet_end || '';
  const types = new Set(arrayField(prefs.notification_types));
  if (els.notificationApprovals) els.notificationApprovals.checked = !types.size || types.has('approval');
  if (els.notificationMentions) els.notificationMentions.checked = !types.size || types.has('mention');
  if (els.notificationEscalations) els.notificationEscalations.checked = !types.size || types.has('escalation');
}

function renderBriefing() {
  if (!els.briefing) return;
  const me = currentUserId();
  const role = currentUserRole();
  const isAdmin = role === 'chef' || role === 'admin';
  const relevant = state.data.threads.filter((thread) => isAdmin || !me || threadRelevantToUser(thread, me));
  const decisions = relevant.filter((thread) => approvalsForThread(thread.id)
    .some((approval) => approval.status === 'pending' && (isAdmin || approval.reviewer_user_id === me))).length;
  const blocked = relevant.filter((thread) => thread.status === 'blocked' || threadHasFailedCtox(thread.id)).length;
  const aiResults = relevant.filter((thread) => messagesForThread(thread.id)
    .some((message) => message.actor_type === 'ai' && message.execution_status === 'completed')).length;
  const items = [[decisions, 'Entscheidungen'], [blocked, 'Blockaden'], [aiResults, 'AI-Ergebnisse']];
  els.briefing.innerHTML = items.map(([value, label]) => `
    <div class="threads-briefing-item">
      <span class="threads-briefing-value">${value}</span>
      <span class="threads-briefing-label">${escapeHtml(label)}</span>
    </div>
  `).join('');
}

// One predicate for filtering AND for the counts on the switcher band — the
// numbers a tab shows must be computed by the exact rule the tab applies.
function threadMatchesFilter(thread, filter, me, isAdmin) {
  if (!isAdmin && me && !threadRelevantToUser(thread, me)) return false;
  if (filter === 'archived') return thread.status === 'archived';
  if (thread.status === 'archived') return false;
  if (filter === 'snoozed') return isSnoozed(thread);
  if (isSnoozed(thread)) return false;
  if (filter === 'approvals') {
    return approvalsForThread(thread.id).some((item) => item.status === 'pending' && (!me || item.reviewer_user_id === me || isAdmin));
  }
  if (filter === 'mentions') return threadMentionsUser(thread.id, me);
  if (filter === 'team') return !thread.assigned_user_id || thread.status === 'escalated';
  if (filter === 'waiting') return threadWaitingOnUser(thread.id, me, isAdmin);
  if (filter === 'delegated') return threadDelegatedByUser(thread, me);
  if (filter === 'running') return threadHasRunningCtox(thread.id);
  if (filter === 'failed') return threadHasFailedCtox(thread.id) || thread.status === 'blocked';
  if (filter === 'watching') return arrayField(thread.watcher_user_ids).includes(me);
  if (filter === 'inbox') {
    return !me
      || unreadNotificationsForThread(thread.id, me).length > 0
      || arrayField(thread.participant_ids).includes(me)
      || approvalsForThread(thread.id).some((item) => item.reviewer_user_id === me && item.status === 'pending');
  }
  return true;
}

function visibleThreads() {
  const me = currentUserId();
  const isAdmin = currentUserRole() === 'chef' || currentUserRole() === 'admin';
  const search = state.search.trim().toLowerCase();
  return state.data.threads
    .filter((thread) => threadMatchesFilter(thread, state.filter, me, isAdmin))
    .filter((thread) => {
      if (!search) return true;
      const haystack = [
        thread.title,
        thread.source_module,
        thread.source_label,
        thread.source_record_id,
        ...messagesForThread(thread.id).map((item) => item.body),
      ].join(' ').toLowerCase();
      return haystack.includes(search);
    })
    .sort((left, right) => attentionScore(right) - attentionScore(left)
      || Number(right.last_message_at_ms || right.updated_at_ms || 0) - Number(left.last_message_at_ms || left.updated_at_ms || 0));
}

function syncSelection() {
  const visible = visibleThreads();
  if (state.requestedThreadId) {
    const requested = visible.find((thread) => thread.id === state.requestedThreadId);
    if (requested) {
      state.selectedId = requested.id;
      state.mobileView = 'detail';
      state.requestedThreadId = '';
      return;
    }
  }
  if (!visible.some((thread) => thread.id === state.selectedId)) {
    state.selectedId = visible[0]?.id || '';
  }
}

function renderList(threads) {
  if (!els.list) return;
  if (!threads.length) {
    els.list.innerHTML = `<div class="ctox-empty">${escapeHtml(state.t('noThreads', 'Keine relevanten Threads vorhanden.'))}</div>`;
    return;
  }
  els.list.innerHTML = threads.map((thread) => {
    const messages = messagesForThread(thread.id);
    const last = messages[messages.length - 1];
    const pending = approvalsForThread(thread.id).filter((item) => item.status === 'pending').length;
    const reasons = attentionReasons(thread);
    return `
      <button type="button" class="ctox-list-item threads-list-item ${thread.id === state.selectedId ? 'is-selected' : ''}"
        data-thread-id="${escapeAttr(thread.id)}"
        data-record-id="${escapeAttr(thread.source_record_id || thread.id)}"
        data-record-type="thread"
        data-title="${escapeAttr(thread.title || '')}">
        <span class="threads-list-title">${escapeHtml(thread.title || thread.id)}</span>
        <span class="threads-attention" aria-label="Priorität ${attentionScore(thread)}">${reasons.slice(0, 2).map(escapeHtml).join(' · ')}</span>
        <span class="threads-list-meta">${escapeHtml(thread.source_module || 'threads')} · ${escapeHtml(formatTime(thread.last_message_at_ms || thread.updated_at_ms))}${pending ? ` · ${pending} offen` : ''}</span>
        <span class="threads-list-preview">${escapeHtml(last?.body || thread.source_label || '')}</span>
      </button>
    `;
  }).join('');
}

function renderDetail(threads) {
  const thread = threads.find((item) => item.id === state.selectedId) || null;
  // Publish the open thread as an advisory presence entry (id only).
  try {
    state.ctx?.presence?.set(thread
      ? [{ collection: 'user_threads', recordId: thread.id, mode: 'viewing' }]
      : []);
  } catch {}
  updateThreadPresenceHint(thread);
  if (!thread) {
    if (els.title) els.title.textContent = state.t('noSelection', 'Kein Thread ausgewählt.');
    if (els.source) els.source.textContent = 'Threads';
    if (els.status) els.status.textContent = state.status || 'bereit';
    if (els.timeline) els.timeline.innerHTML = `<div class="ctox-empty">${escapeHtml(state.t('noSelection', 'Kein Thread ausgewählt.'))}</div>`;
    if (els.context) els.context.innerHTML = '';
    if (els.messageBody) els.messageBody.disabled = true;
    setThreadActionState(null);
    return;
  }
  if (els.messageBody) els.messageBody.disabled = false;
  if (els.title) els.title.textContent = thread.title || thread.id;
  if (els.source) els.source.textContent = contextLabel(thread);
  if (els.status) els.status.textContent = state.status || thread.status || 'open';
  setThreadActionState(thread);
  renderTimeline(thread);
  renderContext(thread);
}

// Presence hint next to the thread status: other users with the same thread
// open right now. Advisory only.
function updateThreadPresenceHint(thread) {
  let hint = els.root?.querySelector('[data-threads-presence-hint]') || null;
  const ownActorId = state.ctx?.actor?.id || '';
  const peers = thread
    ? (state.presenceRemote || []).filter((entry) => entry
      && entry.collection === 'user_threads'
      && entry.recordId === thread.id
      && entry.actorId
      && entry.actorId !== ownActorId)
    : [];
  if (!peers.length) {
    hint?.remove();
    return;
  }
  if (!hint && els.status) {
    hint = document.createElement('span');
    hint.className = 'threads-presence-hint';
    hint.setAttribute('data-threads-presence-hint', '');
    els.status.insertAdjacentElement('afterend', hint);
  }
  if (!hint) return;
  const names = [...new Set(peers.map((entry) => entry.actorName || entry.actorId))].join(', ');
  hint.textContent = `👁 ${names}`;
  hint.title = `${names} ${state.t('presenceViewing', 'sieht sich diesen Thread gerade an')}`;
}

function renderTimeline(thread) {
  const messages = messagesForThread(thread.id);
  const approvals = approvalsForThread(thread.id);
  const timeline = [
    ...messages.map((item) => ({ type: 'message', at: Number(item.created_at_ms || item.updated_at_ms || 0), item })),
    ...approvals.map((item) => ({ type: 'approval', at: Number(item.requested_at_ms || item.created_at_ms || 0), item })),
  ].sort((left, right) => left.at - right.at);
  if (!timeline.length) {
    els.timeline.innerHTML = '<div class="ctox-empty">Noch keine Nachrichten.</div>';
    return;
  }
  const me = currentUserId();
  els.timeline.innerHTML = timeline.map((entry) => {
    if (entry.type === 'approval') return renderApproval(entry.item);
    const message = entry.item;
    const mine = message.author_user_id && message.author_user_id === me;
    return `
      <article class="threads-message ${mine ? 'is-mine' : ''}" data-message-id="${escapeAttr(message.id)}">
        <div class="threads-message-meta">${escapeHtml(message.actor_type === 'ai' ? 'AI' : (message.author_display_name || message.author_user_id || 'system'))} · ${escapeHtml(formatTime(message.created_at_ms || message.updated_at_ms))} · ${escapeHtml(message.event_type || message.kind || 'note')}</div>
        <div class="threads-message-body">${escapeHtml(message.body || '')}</div>
      </article>
    `;
  }).join('');
}

function renderApproval(approval) {
  const command = approval.approved_command_id ? commandById(approval.approved_command_id) : null;
  const task = approval.approved_task_id ? taskById(approval.approved_task_id) : null;
  const me = currentUserId();
  const canDecide = approval.status === 'pending'
    && (approval.reviewer_user_id === me || currentUserRole() === 'admin' || currentUserRole() === 'chef');
  const risk = approvalRisk(approval);
  const impact = approvalImpact(approval);
  const evidence = approvalEvidence(approval);
  return `
    <article class="threads-approval-card" data-approval-id="${escapeAttr(approval.id)}">
      ${canDecide ? `
        <div class="threads-card-actions">
          <button type="button" class="ctox-pane-icon is-confirm" data-approve-approval="${escapeAttr(approval.id)}" aria-label="${escapeHtml(state.t('approvalApprove', 'Freigeben'))}" title="${escapeHtml(state.t('approvalApprove', 'Freigeben'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l5 5L19 7"/></svg></button>
          <button type="button" class="ctox-pane-icon is-danger" data-reject-approval="${escapeAttr(approval.id)}" aria-label="${escapeHtml(state.t('approvalReject', 'Ablehnen'))}" title="${escapeHtml(state.t('approvalReject', 'Ablehnen'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>
          <button type="button" class="ctox-pane-icon" data-edit-approval="${escapeAttr(approval.id)}" aria-label="${escapeHtml(state.t('approvalEdit', 'Bearbeiten'))}" title="${escapeHtml(state.t('approvalEdit', 'Bearbeiten'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20.5l4.3-1 9.1-9.1a2.1 2.1 0 0 0-3-3L5.3 16.2 4 20.5Z"/><path d="M13.5 5.5l3 3"/></svg></button>
        </div>
      ` : ''}
      <div class="threads-message-meta">${escapeHtml(state.t('approvalHeadingPrefix', 'CTOX Freigabe'))} · ${escapeHtml(approval.status || 'pending')} · ${escapeHtml(formatTime(approval.requested_at_ms || approval.created_at_ms))}</div>
      <div class="threads-message-body">${escapeHtml(approval.prompt || '')}</div>
      <dl class="ctox-fields">
        <dt>Ziel</dt><dd>${escapeHtml(approval.target_module || approval.source_module || 'CTOX')}</dd>
        <dt>Risiko</dt><dd>${escapeHtml(risk)}</dd>
        <dt>Erwartete Auswirkung</dt><dd>${escapeHtml(impact)}</dd>
        <dt>Evidenz</dt><dd>${escapeHtml(evidence)}</dd>
      </dl>
      <div class="threads-message-meta">${escapeHtml(state.t('approvalRequester', 'Requester'))}: ${escapeHtml(approval.requester_display_name || approval.requester_user_id || '')} · ${escapeHtml(state.t('approvalReviewerShort', 'Reviewer'))}: ${escapeHtml(approval.reviewer_display_name || approval.reviewer_user_id || '')}</div>
      ${command ? `<div class="threads-message-meta">${escapeHtml(state.t('approvalCommand', 'Command'))}: ${escapeHtml(command.command_type || '')} · ${escapeHtml(command.status || '')}</div>` : ''}
      ${task ? `<div class="threads-message-meta">${escapeHtml(state.t('approvalTask', 'Task'))}: ${escapeHtml(task.title || task.id)} · ${escapeHtml(task.status || '')}</div>` : ''}
    </article>
  `;
}

function renderContext(thread) {
  const links = linksForThread(thread.id);
  const approvals = approvalsForThread(thread.id);
  const unread = unreadNotificationsForThread(thread.id, currentUserId());
  const recentMessages = messagesForThread(thread.id);
  const lastMessage = recentMessages[recentMessages.length - 1];
  const openApproval = approvals.find((item) => item.status === 'pending');
  const nextStep = openApproval
    ? `Freigabe durch ${openApproval.reviewer_display_name || openApproval.reviewer_user_id}`
    : (thread.next_step || (threadHasRunningCtox(thread.id) ? 'AI-Ergebnis abwarten' : 'Nächsten Schritt festlegen'));
  const rows = [
    { label: 'Was ist passiert?', value: lastMessage?.body || thread.source_label || 'Noch keine Aktivität' },
    { label: 'Was braucht mich?', value: attentionReasons(thread).join(', ') || 'Aktuell keine direkte Aktion' },
    { label: 'Was passiert als Nächstes?', value: nextStep },
    { label: 'Quelle', value: contextLabel(thread) },
    { label: 'Record', value: [thread.source_record_type, thread.source_record_id].filter(Boolean).join(' / ') },
    { label: 'Deep Link', value: thread.source_deep_link, deepLink: thread.source_deep_link },
    { label: 'Teilnehmer', value: arrayField(thread.participant_ids).join(', ') },
    { label: 'Watcher', value: arrayField(thread.watcher_user_ids).join(', ') },
    { label: 'Ungelesen', value: String(unread.length) },
    { label: 'Snoozed bis', value: thread.snoozed_until_ms ? formatTime(thread.snoozed_until_ms) : '' },
    { label: 'Offene Freigaben', value: String(approvals.filter((item) => item.status === 'pending').length) },
    ...links.map((link) => ({
      label: link.link_type || link.link_role || 'Link',
      value: [link.app_collection, link.source_module, link.source_record_type, link.source_record_id, link.source_label].filter(Boolean).join(' / '),
      deepLink: link.deep_link,
    })),
  ].filter((row) => String(row.value || row.deepLink || '').trim());
  const rowHtml = rows.length
    ? `<dl class="ctox-fields ctox-fields--stacked">${rows.map(renderContextRow).join('')}</dl>`
    : '<div class="ctox-empty">Kein verknüpfter App-Kontext.</div>';
  const notificationHtml = unread.length
    ? `<div class="threads-notification-list">${unread.map((item) => `
        <div class="ctox-callout threads-notification-item">
          <span>${escapeHtml(item.title || item.notification_type || 'Notification')}</span>
          <div class="threads-notification-actions">
            <button type="button" class="ctox-pane-icon" data-mark-notification="${escapeAttr(item.id)}" aria-label="Gelesen" title="Gelesen"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l5 5L19 7"/></svg></button>
            <button type="button" class="ctox-pane-icon" data-dismiss-notification="${escapeAttr(item.id)}" aria-label="Ausblenden" title="Ausblenden"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>
          </div>
        </div>
      `).join('')}</div>`
    : '';
  els.context.innerHTML = rowHtml + notificationHtml;
}

async function submitMessage() {
  const thread = state.data.threads.find((item) => item.id === state.selectedId);
  const body = String(els.messageBody?.value || '').trim();
  if (!thread || !body) return;
  await dispatchThreadsCommand('threads.message.create', {
    thread_id: thread.id,
    body,
  }, {
    recordId: thread.id,
    sourceModule: thread.source_module || 'threads',
  });
  els.messageBody.value = '';
  clearDraft();
  await refresh();
}

async function claimSelectedThread() {
  const thread = selectedThread();
  if (!thread) return;
  await dispatchThreadsCommand('threads.thread.claim', {
    thread_id: thread.id,
    expected_updated_at_ms: Number(thread.updated_at_ms || 0),
  }, { recordId: thread.id });
  await refresh();
}

async function submitHandoff() {
  const thread = selectedThread();
  const target = String(els.handoffTarget?.value || '').trim();
  const expectation = String(els.handoffBody?.value || '').trim();
  const dueAt = els.handoffDue?.value ? new Date(els.handoffDue.value).getTime() : 0;
  const returnReason = String(els.handoffReturnReason?.value || '').trim();
  if (!thread || !target || !expectation) return;
  await dispatchThreadsCommand('threads.handoff.create', {
    thread_id: thread.id,
    target_user_id: target,
    expectation,
    due_at_ms: Number.isFinite(dueAt) ? dueAt : 0,
    return_reason: returnReason,
  }, { recordId: thread.id });
  els.handoffBody.value = '';
  if (els.handoffDue) els.handoffDue.value = '';
  if (els.handoffReturnReason) els.handoffReturnReason.value = '';
  await refresh();
}

async function submitAiRequest() {
  const thread = selectedThread();
  const goal = String(els.aiGoal?.value || '').trim();
  if (!thread || !goal) return;
  await dispatchThreadsCommand('threads.ai.request', {
    thread_id: thread.id,
    goal,
    risk_class: 'internal',
  }, { recordId: thread.id });
  els.aiGoal.value = '';
  await refresh();
}

async function submitNotificationPreferences() {
  const notificationTypes = [];
  if (els.notificationApprovals?.checked) notificationTypes.push('approval');
  if (els.notificationMentions?.checked) notificationTypes.push('mention');
  if (els.notificationEscalations?.checked) notificationTypes.push('escalation');
  await dispatchThreadsCommand('threads.notification.preferences.update', {
    priority_threshold: Number(els.notificationThreshold?.value || 20),
    quiet_start: String(els.quietStart?.value || ''),
    quiet_end: String(els.quietEnd?.value || ''),
    notification_types: notificationTypes,
  }, { recordId: currentUserId() });
  await refresh();
}

async function submitNote() {
  const body = String(els.noteBody?.value || '').trim();
  const targetUserIds = splitUserIds(els.noteTarget?.value || '');
  if (!body) return;
  const payload = buildNotePayload({
    body,
    targetUserIds,
    threadId: state.selectedId,
    sourceContext: selectedSourceContext(),
  });
  await dispatchThreadsCommand('threads.note.create', payload, {
    recordId: payload.thread_id,
    sourceModule: payload.source_context?.module || 'threads',
  });
  if (els.noteBody) els.noteBody.value = '';
  await refresh();
}

async function submitApprovalRequest() {
  const prompt = String(els.approvalPrompt?.value || '').trim();
  const reviewerUserId = String(els.approvalReviewer?.value || '').trim();
  if (!prompt || !reviewerUserId) return;
  const payload = buildApprovalRequestPayload({
    prompt,
    reviewerUserId,
    threadId: state.selectedId,
    sourceContext: selectedSourceContext(),
  });
  await dispatchThreadsCommand('threads.ctox_approval.request', payload, {
    recordId: payload.thread_id,
    sourceModule: payload.source_context?.module || 'threads',
  });
  if (els.approvalPrompt) els.approvalPrompt.value = '';
  await refresh();
}

async function decideApproval(approvalId, decision) {
  if (!approvalId) return;
  const approval = approvalById(approvalId);
  const expectedUpdatedAt = Number(approval?.updated_at_ms || 0);
  if (!expectedUpdatedAt) throw new Error('Approval version unavailable.');
  if (decision === 'approve' && approvalNeedsExplicitConfirmation(approval)) {
    const confirmed = window.confirm(`Riskante Aktion wirklich freigeben?\n\n${approvalImpact(approval)}`);
    if (!confirmed) return;
  }
  const note = decision === 'reject' ? window.prompt('Begründung oder Änderungswunsch:') || '' : '';
  await dispatchThreadsCommand(
    decision === 'approve' ? 'threads.ctox_approval.approve' : 'threads.ctox_approval.reject',
    {
      approval_request_id: approvalId,
      expected_updated_at_ms: expectedUpdatedAt,
      decision_note: note,
    },
    {
      recordId: approvalId,
      sourceModule: 'threads',
      until: 'local',
    },
  );
  // The command can reach terminal state just before the native approval
  // projection is visible to a demand query. Re-read the immutable approval
  // id directly; a cached `status=pending` list query cannot prove the latest
  // state when historical pull is intentionally disabled.
  for (let attempt = 0; attempt < 20 && approvalById(approvalId)?.status === 'pending'; attempt += 1) {
    await new Promise((resolve) => window.setTimeout(resolve, 500));
    const [latest] = await loadRecordsByIds('ctox_task_approval_requests', [approvalId]);
    if (!latest) continue;
    state.data.approvals = mergeRecords(
      state.data.approvals.filter((item) => item.id !== approvalId && item.approval_request_id !== approvalId),
      [latest],
    );
    syncSelection();
    render();
  }
  refresh().catch((error) => showError(error));
}

function approvalRisk(approval) {
  return String(
    approval?.risk_class
      || approval?.target_payload?.risk_class
      || approval?.target_payload?.risk
      || 'mittel',
  ).trim().toLowerCase() || 'mittel';
}

function approvalImpact(approval) {
  return String(
    approval?.expected_impact
      || approval?.target_payload?.expected_impact
      || approval?.target_payload?.impact
      || approval?.instruction
      || approval?.prompt
      || 'Ausführung des angeforderten Commands',
  ).trim();
}

function approvalEvidence(approval) {
  const refs = arrayField(approval?.evidence_refs || approval?.target_payload?.evidence_refs);
  if (refs.length) return refs.slice(0, 3).join(', ');
  return [approval?.source_module, approval?.source_record_type, approval?.source_record_id]
    .filter(Boolean)
    .join(' / ') || 'Thread-Verlauf';
}

function approvalNeedsExplicitConfirmation(approval) {
  return ['high', 'critical', 'hoch', 'kritisch', 'external', 'financial', 'personal_data', 'irreversible']
    .includes(approvalRisk(approval));
}

async function editApproval(approvalId) {
  if (!approvalId) return;
  const approval = approvalById(approvalId);
  const expectedUpdatedAt = Number(approval?.updated_at_ms || 0);
  if (!expectedUpdatedAt) throw new Error('Approval version unavailable.');
  const prompt = window.prompt('Finaler CTOX Prompt:', approval?.prompt || '');
  if (!prompt || !prompt.trim()) return;
  await dispatchThreadsCommand('threads.ctox_approval.edit', {
    approval_request_id: approvalId,
    expected_updated_at_ms: expectedUpdatedAt,
    prompt: prompt.trim(),
    instruction: prompt.trim(),
  }, {
    recordId: approvalId,
    sourceModule: 'threads',
  });
  await refresh();
}

async function toggleWatch() {
  const thread = selectedThread();
  if (!thread) return;
  const watching = arrayField(thread.watcher_user_ids).includes(currentUserId());
  await dispatchThreadsCommand(watching ? 'threads.thread.unwatch' : 'threads.thread.watch', {
    thread_id: thread.id,
  }, {
    recordId: thread.id,
    sourceModule: 'threads',
  });
  await refresh();
}

async function snoozeSelectedThread() {
  const thread = selectedThread();
  if (!thread) return;
  const hours = Number(window.prompt('Snooze in Stunden:', '24') || 0);
  if (!Number.isFinite(hours) || hours <= 0) return;
  await dispatchThreadsCommand('threads.thread.snooze', {
    thread_id: thread.id,
    snoozed_until_ms: Date.now() + Math.round(hours * 60 * 60 * 1000),
  }, {
    recordId: thread.id,
    sourceModule: 'threads',
  });
  await refresh();
}

async function archiveSelectedThread() {
  const thread = selectedThread();
  if (!thread) return;
  await dispatchThreadsCommand('threads.thread.archive', {
    thread_id: thread.id,
  }, {
    recordId: thread.id,
    sourceModule: 'threads',
  });
  await refresh();
}

async function updateNotification(notificationId, action) {
  if (!notificationId) return;
  await dispatchThreadsCommand(
    action === 'dismiss' ? 'threads.notification.dismiss' : 'threads.notification.mark_read',
    { notification_id: notificationId },
    {
      recordId: notificationId,
      sourceModule: 'threads',
    },
  );
  await refresh();
}

async function dispatchThreadsCommand(
  commandType,
  payload,
  { recordId = '', sourceModule = 'threads', until = 'accepted' } = {},
) {
  if (!state.ctx?.commandBus?.dispatch) throw new Error('Command bus unavailable.');
  setBusy(true);
  try {
    const command = buildThreadsCommand({
      commandType,
      payload,
      recordId,
      sourceModule,
      actor: actorContext(),
      clientContext: {
        visible_scope: {
          app: { module_id: 'threads', app_id: 'threads' },
          source: payload?.source_context || selectedSourceContext(),
        },
      },
    });
    const outcome = await state.ctx.commandBus.dispatch(command, { until });
    state.status = outcome?.status || 'completed';
    return outcome;
  } finally {
    setBusy(false);
  }
}

function setBusy(busy) {
  state.busy = busy;
  [els.noteForm, els.approvalForm, els.messageForm, els.handoffForm, els.aiForm, els.notificationPreferencesForm].forEach((form) => {
    form?.querySelectorAll?.('button, input, textarea').forEach((control) => {
      if (control === els.messageBody && !state.selectedId) {
        control.disabled = true;
      } else {
        control.disabled = busy;
      }
    });
  });
  setThreadActionState(selectedThread());
  if (els.status) els.status.textContent = busy ? 'sendet' : (state.status || 'bereit');
}

function showError(error) {
  console.warn('[threads] action failed', error);
  state.status = state.t('commandFailed', 'Aktion konnte nicht abgeschlossen werden.');
  if (els.status) els.status.textContent = state.status;
}

function selectedThread() {
  return state.data.threads.find((item) => item.id === state.selectedId) || null;
}

function approvalById(approvalId) {
  return state.data.approvals.find((item) => item.id === approvalId || item.approval_request_id === approvalId) || null;
}

function setThreadActionState(thread) {
  const disabled = !thread || state.busy;
  [els.watch, els.snooze, els.archive, els.claim].forEach((button) => {
    if (button) button.disabled = disabled;
  });
  if (els.watch && thread) {
    const watching = arrayField(thread.watcher_user_ids).includes(currentUserId());
    const label = watching ? 'Unwatch' : 'Watch';
    els.watch.title = label;
    els.watch.setAttribute('aria-label', label);
    els.watch.setAttribute('aria-pressed', watching ? 'true' : 'false');
  }
}

function selectedSourceContext() {
  const thread = state.data.threads.find((item) => item.id === state.selectedId);
  if (!thread) return { module: 'threads', label: 'Threads' };
  const link = linksForThread(thread.id).find((item) => item.deep_link);
  return {
    module: thread.source_module || 'threads',
    record_type: thread.source_record_type || 'thread',
    record_id: thread.source_record_id || thread.id,
    label: thread.source_label || thread.title || thread.id,
    deep_link: thread.source_deep_link || link?.deep_link || '',
  };
}

function renderContextRow(row) {
  const label = row?.label || '';
  const value = row?.value || row?.deepLink || '';
  const deepLink = String(row?.deepLink || '').trim();
  const valueHtml = deepLink
    ? `<a href="${escapeAttr(deepLink)}" data-thread-deep-link="${escapeAttr(deepLink)}">${escapeHtml(value || deepLink)}</a>`
    : `<span>${escapeHtml(value)}</span>`;
  return `<dt>${escapeHtml(label)}</dt><dd>${valueHtml}</dd>`;
}

function navigateDeepLink(value) {
  const link = String(value || '').trim();
  if (!link) return;
  if (link.startsWith('#')) {
    window.location.hash = link;
  } else if (link.startsWith('/') || link.startsWith('?')) {
    window.location.assign(link);
  }
}

function messagesForThread(threadId) {
  return state.data.messages
    .filter((item) => item.thread_id === threadId)
    .sort((left, right) => Number(left.created_at_ms || left.updated_at_ms || 0) - Number(right.created_at_ms || right.updated_at_ms || 0));
}

function linksForThread(threadId) {
  return state.data.links.filter((item) => item.thread_id === threadId);
}

function approvalsForThread(threadId) {
  return state.data.approvals.filter((item) => item.thread_id === threadId);
}

function notificationsForThread(threadId) {
  return state.data.notifications.filter((item) => item.thread_id === threadId);
}

function unreadNotificationsForThread(threadId, userId) {
  return notificationsForThread(threadId)
    .filter((item) => (!userId || item.user_id === userId) && item.status === 'unread');
}

function threadRelevantToUser(thread, userId) {
  if (!userId) return true;
  return arrayField(thread.participant_ids).includes(userId)
    || arrayField(thread.watcher_user_ids).includes(userId)
    || notificationsForThread(thread.id).some((item) => item.user_id === userId && item.status !== 'dismissed')
    || approvalsForThread(thread.id).some((item) => item.reviewer_user_id === userId || item.requester_user_id === userId)
    || messagesForThread(thread.id).some((item) => item.author_user_id === userId || arrayField(item.target_user_ids).includes(userId));
}

function threadMentionsUser(threadId, userId) {
  if (!userId) return false;
  return messagesForThread(threadId).some((item) => arrayField(item.target_user_ids).includes(userId) || item.kind === 'mention')
    || notificationsForThread(threadId).some((item) => item.user_id === userId && ['mention', 'mentioned'].includes(item.notification_type || item.reason));
}

function threadWaitingOnUser(threadId, userId, isAdmin) {
  return approvalsForThread(threadId).some((item) => item.status === 'pending' && (isAdmin || !userId || item.reviewer_user_id === userId))
    || notificationsForThread(threadId).some((item) => (!userId || item.user_id === userId) && ['waiting_on_user', 'approval_request', 'approval_requested'].includes(item.notification_type || item.reason));
}

function threadDelegatedByUser(thread, userId) {
  if (!userId) return false;
  return thread.created_by_id === userId
    || messagesForThread(thread.id).some((item) => item.author_user_id === userId)
    || approvalsForThread(thread.id).some((item) => item.requester_user_id === userId);
}

function threadHasRunningCtox(threadId) {
  return linkedCommandsForThread(threadId).some((item) => statusIn(item.status || item.task_status, ['pending_sync', 'queued', 'running', 'in_progress']))
    || linkedTasksForThread(threadId).some((item) => statusIn(item.status, ['queued', 'running', 'in_progress']));
}

function threadHasFailedCtox(threadId) {
  return linkedCommandsForThread(threadId).some((item) => statusIn(item.status || item.task_status, ['failed', 'blocked', 'error']))
    || linkedTasksForThread(threadId).some((item) => statusIn(item.status, ['failed', 'blocked', 'error']));
}

function linkedCommandsForThread(threadId) {
  const ids = new Set([
    ...messagesForThread(threadId).map((item) => item.command_id),
    ...linksForThread(threadId).map((item) => item.command_id),
    ...approvalsForThread(threadId).map((item) => item.approved_command_id),
  ].map((id) => String(id || '').trim()).filter(Boolean));
  return state.data.commands.filter((item) => ids.has(item.id) || ids.has(item.command_id));
}

function linkedTasksForThread(threadId) {
  const ids = new Set([
    ...linksForThread(threadId).map((item) => item.task_id),
    ...approvalsForThread(threadId).map((item) => item.approved_task_id),
    ...linkedCommandsForThread(threadId).map((item) => item.task_id),
  ].map((id) => String(id || '').trim()).filter(Boolean));
  return state.data.queue.filter((item) => ids.has(item.id) || ids.has(item.task_id));
}

function linkedCommandIds(data = state.data) {
  return [
    ...data.messages.map((item) => item.command_id),
    ...data.links.map((item) => item.command_id),
    ...data.approvals.map((item) => item.approved_command_id),
  ];
}

function linkedTaskIds(data = state.data, commands = state.data.commands) {
  return [
    ...data.links.map((item) => item.task_id),
    ...data.approvals.map((item) => item.approved_task_id),
    ...commands.map((item) => item.task_id),
  ];
}

function isSnoozed(thread) {
  const until = Number(thread.snoozed_until_ms || 0);
  return thread.status === 'snoozed' && until > Date.now();
}

function statusIn(status, values) {
  const clean = String(status || '').trim().toLowerCase();
  return values.includes(clean);
}

function commandById(commandId) {
  return state.data.commands.find((item) => item.id === commandId || item.command_id === commandId) || null;
}

function taskById(taskId) {
  return state.data.queue.find((item) => item.id === taskId || item.task_id === taskId) || null;
}

function contextLabel(thread) {
  return [thread.source_module, thread.source_label || thread.source_record_id].filter(Boolean).join(' · ') || 'Threads';
}

function userStateForThread(threadId) {
  return state.data.states?.find((item) => item.thread_id === threadId && item.user_id === currentUserId()) || null;
}

function attentionReasons(thread) {
  const stored = arrayField(userStateForThread(thread.id)?.attention_reasons);
  if (stored.length) return stored;
  const reasons = [];
  if (approvalsForThread(thread.id).some((item) => item.status === 'pending' && item.reviewer_user_id === currentUserId())) reasons.push('Freigabe');
  if (threadMentionsUser(thread.id, currentUserId())) reasons.push('Erwähnung');
  if (thread.assigned_user_id === currentUserId()) reasons.push('Zugewiesen');
  if (thread.status === 'blocked' || threadHasFailedCtox(thread.id)) reasons.push('Blockiert');
  if (Number(thread.due_at_ms || 0) > 0 && Number(thread.due_at_ms) < Date.now() + 86400000) reasons.push('Frist');
  return reasons;
}

function attentionScore(thread) {
  const stored = Number(userStateForThread(thread.id)?.attention_score);
  if (Number.isFinite(stored) && stored > 0) return stored;
  const weights = { Freigabe: 100, Blockiert: 90, Frist: 80, Erwähnung: 70, Zugewiesen: 50 };
  return attentionReasons(thread).reduce((score, reason) => score + (weights[reason] || 10), 0);
}

function draftKey() {
  return `ctox:threads:draft:${currentUserId() || 'anonymous'}:${state.selectedId || 'new'}`;
}

function persistDraft() {
  try { sessionStorage.setItem(draftKey(), String(els.messageBody?.value || '')); } catch {}
}

function restoreDraft() {
  try {
    const saved = sessionStorage.getItem(draftKey());
    if (saved && els.messageBody) els.messageBody.value = saved;
    const nav = JSON.parse(sessionStorage.getItem(`ctox:threads:navigation:${currentUserId() || 'anonymous'}`) || '{}');
    if (!state.requestedThreadId) state.requestedThreadId = String(nav.selectedId || '');
    if (nav.filter && els.filters.some((button) => button.dataset.filter === nav.filter)) state.filter = nav.filter;
    state.mobileView = nav.mobileView === 'detail' ? 'detail' : 'list';
  } catch {}
}

function clearDraft() {
  try { sessionStorage.removeItem(draftKey()); } catch {}
}

function persistNavigationState() {
  try {
    sessionStorage.setItem(`ctox:threads:navigation:${currentUserId() || 'anonymous'}`, JSON.stringify({
      selectedId: state.selectedId,
      filter: state.filter,
      mobileView: state.mobileView,
    }));
  } catch {}
}

function updateConnectivity() {
  const online = navigator.onLine !== false;
  if (els.syncState) {
    els.syncState.textContent = online ? 'synchronisiert' : 'offline';
    els.syncState.dataset.state = online ? 'online' : 'offline';
    els.syncState.classList.toggle('is-danger', !online);
  }
  els.root?.classList.toggle('is-offline', !online);
}

function notifyActionRequired(notifications) {
  if (typeof Notification === 'undefined' || Notification.permission !== 'granted') return;
  const preferences = personalPreferences();
  if (isQuietTime(preferences)) return;
  const enabledTypes = new Set(arrayField(preferences?.notification_types));
  const threshold = Number(preferences?.priority_threshold ?? 20);
  const allowed = new Set([
    'approval_requested', 'approval_request', 'mention', 'mentioned',
    'escalated', 'deadline', 'ctox_failed', 'ai_failed',
  ]);
  notifications
    .filter((item) => item.status === 'unread' && allowed.has(item.notification_type || item.reason))
    .filter((item) => notificationCategoryEnabled(item, enabledTypes))
    .filter((item) => attentionScore(state.data.threads.find((thread) => thread.id === item.thread_id) || {}) >= threshold)
    .slice(0, 3)
    .forEach((item) => {
      const dedupeKey = `ctox:threads:notified:${currentUserId()}:${item.id}`;
      try {
        if (sessionStorage.getItem(dedupeKey)) return;
        sessionStorage.setItem(dedupeKey, '1');
      } catch {}
      const notice = new Notification(item.title || 'CTOX braucht deine Aufmerksamkeit', {
        body: 'In Threads ist eine Aktion für dich offen.',
        tag: `ctox-thread-${item.thread_id || item.id}`,
      });
      notice.onclick = () => {
        window.focus();
        location.hash = `threads?thread_id=${encodeURIComponent(item.thread_id || '')}`;
        notice.close();
      };
    });
}

function personalPreferences() {
  return state.data.states?.find((item) => item.user_id === currentUserId() && item.thread_id === '__preferences__')
    ?.notification_preferences || null;
}

function notificationCategoryEnabled(item, enabledTypes) {
  if (!enabledTypes.size) return true;
  const type = String(item?.notification_type || item?.reason || '');
  if (type.includes('approval')) return enabledTypes.has('approval');
  if (type.includes('mention')) return enabledTypes.has('mention');
  return enabledTypes.has('escalation');
}

function isQuietTime(preferences) {
  const start = String(preferences?.quiet_start || '');
  const end = String(preferences?.quiet_end || '');
  if (!start || !end) return false;
  const now = new Date();
  const current = now.getHours() * 60 + now.getMinutes();
  const minutes = (value) => {
    const [hour, minute] = value.split(':').map(Number);
    return hour * 60 + minute;
  };
  const from = minutes(start);
  const to = minutes(end);
  return from <= to ? current >= from && current < to : current >= from || current < to;
}

function currentUserId() {
  return String(state.ctx?.session?.user?.id || '').trim();
}

function currentUserRole() {
  return String(state.ctx?.session?.user?.role || (state.ctx?.session?.user?.is_admin ? 'admin' : 'user') || 'user').trim();
}

function actorContext() {
  const user = state.ctx?.session?.user || {};
  const id = String(user.id || '').trim();
  if (!id) return null;
  return {
    id,
    display_name: user.display_name || user.name || id,
    role: user.role || (user.is_admin ? 'admin' : 'user'),
    is_admin: Boolean(user.is_admin),
  };
}

function arrayField(value) {
  return Array.isArray(value) ? value.map((item) => String(item || '').trim()).filter(Boolean) : [];
}

function formatTime(value) {
  const ms = Number(value || 0);
  if (!Number.isFinite(ms) || ms <= 0) return '';
  try {
    return new Intl.DateTimeFormat(document.documentElement.lang === 'en' ? 'en' : 'de', {
      day: '2-digit',
      month: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(ms));
  } catch {
    return new Date(ms).toLocaleString();
  }
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}
