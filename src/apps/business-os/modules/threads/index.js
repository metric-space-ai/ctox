import { loadModuleMessages } from '../../shared/i18n.js';
import { renderHtmlIfChanged } from '../../shared/stable-dom.js';
import { collectUniquePages } from './paging.js';
import { normalizeInternalDeepLink, sourceDeepLinkFor as buildSourceDeepLink, sourceFocusSupported } from './links.js';
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
const PERSONAL_PAGE_SIZE = 100;

const labels = {
  de: {
    refresh: 'Aktualisieren',
    search: 'Threads suchen',
    noThreads: 'Keine relevanten Threads vorhanden.',
    syncingThreads: 'Threads werden synchronisiert.',
    noSelection: 'Kein Thread ausgewählt.',
    commandFailed: 'Aktion konnte nicht abgeschlossen werden.',
  },
  en: {
    refresh: 'Refresh',
    search: 'Search threads',
    noThreads: 'No relevant threads.',
    syncingThreads: 'Syncing threads.',
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
let mountSequence = 0;

export async function mount(ctx) {
  const metricPrefix = `ctox.threads.${++mountSequence}`;
  state.metrics = { prefix: metricPrefix, recorded: new Set() };
  performance.mark(`${metricPrefix}.mount`);
  state.ctx = ctx;
  state.filter = 'inbox';
  state.listView = false;
  state.search = '';
  state.selectedId = '';
  state.mobileView = 'list';
  state.requestedThreadId = String(ctx.args?.thread_id || ctx.args?.thread || '').trim();
  if (state.requestedThreadId) state.mobileView = 'detail';
  state.data = emptyData();
  state.personalStateByThread = new Map();
  state.threadsReadiness = null;
  state.collectionReadiness = {};
  state.refreshInFlight = null;
  state.status = '';
  state.commandReceipt = null;
  state.personalComplete = false;
  state.recentThreadsComplete = false;
  state.detailCompleteThreadId = '';

  const messages = await loadModuleMessages(import.meta.url, ctx.locale || 'de', labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  await ensureStyles();
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left.replaceChildren();
  ctx.right.replaceChildren();

  bindElements(ctx.host);
  applyLabels();
  wireUi();
  const onAppLaunch = (event) => {
    const args = event?.detail?.args || {};
    const threadId = String(args.thread_id || args.thread || '').trim();
    if (!threadId) return;
    state.requestedThreadId = threadId;
    state.selectedId = threadId;
    state.mobileView = 'detail';
    refresh().catch(showError);
  };
  ctx.host.addEventListener('ctox-business-os-app-launch', onAppLaunch);
  state.cleanup.push(() => ctx.host.removeEventListener('ctox-business-os-app-launch', onAppLaunch));
  restoreDraft();
  updateConnectivity();
  state.cleanup.push(wireRealtime());
  state.cleanup.push(wireReadiness());
  ensureRoster().then(renderRosterSelects).catch(() => {});
  // Whose inbox is this? Anchor the identity in the pane header.
  const kicker = els.root?.querySelector('[data-label="hubKicker"]');
  const displayName = state.ctx?.session?.user?.display_name || currentUserId() || '';
  if (kicker && displayName) kicker.textContent = `Inbox · ${displayName}`;
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
  const version = String(import.meta.url).split('?v=')[1] || '20260722-threads-grammar-v3';
  styleUrl.searchParams.set('v', version);
  link.href = styleUrl.href;
  link.dataset.threadsStyle = 'true';
  document.head.append(link);
}

async function loadModuleMarkup() {
  // Markup inherits the JS cache-buster — like the stylesheet, a deploy must
  // never leave fresh JS binding against stale cached markup (same contract
  // as ctox/coding-agents/knowledge).
  const version = String(import.meta.url).split('?v=')[1] || '20260722-threads-grammar-v3';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (version ? `?v=${version}` : '');
  const html = await fetch(markupHref).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(root) {
  els.root = root.querySelector('[data-threads-root]');
  els.leftPane = root.querySelector('.threads-left');
  els.centerPane = root.querySelector('.threads-center');
  // One-button view switches (queue rail + timeline).
  els.viewToggle = root.querySelector('.threads-left [data-threads-view-toggle]');
  els.centerViewToggle = root.querySelector('.threads-center [data-threads-view-toggle]');
  els.refresh = root.querySelector('[data-refresh]');
  els.search = root.querySelector('[data-pg-search]');
  els.list = root.querySelector('[data-thread-list]');
  els.title = root.querySelector('[data-thread-title]');
  els.source = root.querySelector('[data-thread-source]');
  els.status = root.querySelector('[data-thread-status]');
  els.commandReceipt = root.querySelector('[data-thread-command-receipt]');
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
  els.delegate = root.querySelector('[data-thread-delegate]');
  els.toggleActions = root.querySelector('[data-toggle-actions]');
  els.syncState = root.querySelector('[data-sync-state]');
  els.mobileBack = root.querySelector('[data-mobile-back]');
  els.mobileReply = root.querySelector('[data-mobile-reply]');
  els.mobileSnooze = root.querySelector('[data-mobile-snooze]');
  els.mobileMore = root.querySelector('[data-mobile-more]');
  els.claimButtons = [...root.querySelectorAll('[data-thread-claim]')];
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
  syncViewToggle(els.viewToggle);
  syncViewToggle(els.centerViewToggle);
  // Approval / delegation labels (the static index.html copy ships German;
  // translate the approval-flow strings through the module message catalog).
  const setFilterText = (filter, key, fb) => {
    const btn = els.root?.querySelector(`[data-pg-band="${filter}"]`);
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
  // Pane chrome is SHELL-owned canonical grammar (autoWirePaneGrammar wires
  // the data-pg-* markup once, debounced ~120ms after mount). The module only
  // keeps its state in sync through the bubbling grammar event and re-renders
  // — the same contract the ctox module's task column uses.
  els.leftPane?.addEventListener('ctox-pane-grammar-change', onLeftGrammarChange);
  els.centerPane?.addEventListener('ctox-pane-grammar-change', onCenterGrammarChange);
  // One control, not two (operator directive 31.08.): the card/list switch is
  // an ACTION. This listener is registered at mount — BEFORE the shell wires
  // the pane grammar (~120ms later) — so it flips data-pg-view first and the
  // grammar then reports the NEW view. Before the grammar exists the module
  // applies the switch itself through the same handler.
  for (const button of [els.viewToggle, els.centerViewToggle]) {
    if (!button) continue;
    button.addEventListener('click', () => {
      const next = button.dataset.pgView === 'list' ? 'cards' : 'list';
      button.dataset.pgView = next;
      syncViewToggle(button);
      if (button.closest('.ctox-pane')?.dataset.pgWired === 'true') return;
      if (button === els.centerViewToggle) onCenterGrammarChange({ detail: { view: next } });
      else onLeftGrammarChange({ detail: { view: next, search: state.search, filters: {} } });
    });
  }
  // A band tab click means "primary queue": clear any secondary tray view so
  // the two dimensions never fight (this listener was attached at mount,
  // BEFORE the shell-wired grammar handler, so it runs first).
  els.leftPane?.querySelectorAll('[data-pg-band]').forEach((tab) => {
    tab.addEventListener('click', () => {
      const select = els.leftPane?.querySelector('[data-pg-filter][data-pg-name="view"]');
      if (select && select.value !== '') select.value = '';
    });
  });
  // Header actions: create-note opens the action workbench with the note form
  // focused; export serializes the currently filtered thread list as JSON.
  els.root?.querySelector('[data-action="create-note"]')?.addEventListener('click', () => {
    openActionPanel('note', els.noteBody);
  });
  els.root?.querySelector('[data-action="export-threads"]')?.addEventListener('click', exportVisibleThreads);
  els.list?.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const row = target?.closest?.('[data-thread-id]');
    if (!row) return;
    state.selectedId = row.getAttribute('data-thread-id') || '';
    state.detailCompleteThreadId = '';
    state.mobileView = 'detail';
    persistNavigationState();
    // Selection is an in-place class flip, never a list rebuild — a rebuild
    // resets the scroll position under the operator's pointer.
    applyThreadSelection();
    renderMobileState();
    renderDetail();
    hydrateSelectedThread(state.selectedId).catch(showError);
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
    const deepLink = target?.closest?.('[data-thread-deep-link]');
    if (deepLink) {
      event.preventDefault();
      navigateDeepLink(deepLink.getAttribute('data-thread-deep-link') || '');
      return;
    }
    const rework = target?.closest?.('[data-rework-context]');
    if (rework) {
      requestRework(rework.getAttribute('data-rework-context') || '').catch(showError);
      return;
    }
    const question = target?.closest?.('[data-question-approval]');
    if (question) {
      askApprovalQuestion(question.getAttribute('data-question-approval') || '').catch(showError);
      return;
    }
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
  els.source?.addEventListener('click', () => {
    navigateDeepLink(els.source.dataset.threadDeepLink || '');
  });
  // Delegate is a thread action, not a buried form: open the action pane and
  // put the cursor into the handoff target picker.
  els.root?.querySelector('[data-thread-delegate]')?.addEventListener('click', () => {
    openActionPanel('handoff', els.handoffTarget);
  });
  wireMentionPopup();
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
  els.claimButtons.forEach((button) => button.addEventListener('click', () => claimSelectedThread().catch(showError)));
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

function openActionPanel(name, focusTarget) {
  els.root?.classList.remove('is-actions-hidden');
  els.root?.classList.add('is-context-open');
  els.toggleActions?.setAttribute('aria-pressed', 'true');
  const panel = els.root?.querySelector(`[data-action-panel="${name}"]`);
  if (panel) panel.open = true;
  focusTarget?.focus();
  focusTarget?.scrollIntoView({ block: 'center', behavior: 'smooth' });
}

// Field finding 2026-09-06 (managed tenant): this module keeps running in the
// background after it was opened once. A 10 s poll with ~8 demand queries per
// refresh (two 200-row approval queries among them) queued thousands of
// wire fetches per hour; business command frames waited behind them and took
// 15-20 minutes to reach CTOX, so every research start in the Outbound app
// aborted on its 120 s Sellify pre-flight. Polling therefore only runs while
// the module is actually visible, never overlaps itself, and backs off.
const REALTIME_POLL_MS = 30000;

function moduleIsVisible() {
  if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return false;
  const host = state.ctx?.host;
  if (!host || !host.isConnected) return false;
  // offsetParent is null for display:none subtrees (hidden module windows).
  return host.offsetParent !== null || host === document.body;
}

function wireRealtime() {
  // Demand-query writes also emit collection changes. Subscribing a refresh
  // to those same queries creates a fetch -> change -> refresh feedback loop
  // and continuously replaces clickable approval controls. A bounded poll,
  // gated on visibility, keeps cross-profile updates visible without churn.
  const timer = window.setInterval(() => {
    if (!moduleIsVisible()) return;
    if (state.refreshInFlight) return;
    refresh().catch((error) => console.warn('[threads] refresh failed', error));
  }, REALTIME_POLL_MS);
  const onVisible = () => {
    if (moduleIsVisible() && !state.refreshInFlight) {
      refresh().catch((error) => console.warn('[threads] refresh failed', error));
    }
  };
  document.addEventListener('visibilitychange', onVisible);
  return () => {
    window.clearInterval(timer);
    document.removeEventListener('visibilitychange', onVisible);
  };
}

// Sync readiness is a render hint only: while the user_threads collection
// has not finished its initial replication (ready === false), an empty
// unfiltered source list renders as a syncing shell instead of "no threads".
function readThreadsReadiness() {
  const read = state.ctx?.sync?.collectionReadiness;
  return typeof read === 'function' ? read.call(state.ctx.sync, 'user_threads') : null;
}

function personalCollectionsReady() {
  if (!state.personalComplete) return false;
  return ['user_threads', 'user_thread_states', 'ctox_task_approval_requests']
    .every((name) => {
      const snapshot = state.collectionReadiness[name]
        || state.ctx?.sync?.collectionReadiness?.(name);
      return snapshot?.ready === true;
    });
}

// Re-render on every readiness state transition; the snapshot arrives
// immediately on subscribe and the list render preserves its scroll offset.
function wireReadiness() {
  const subscribe = state.ctx?.sync?.subscribeCollectionReadiness;
  if (typeof subscribe !== 'function') return null;
  const unsubscribe = subscribe.call(state.ctx.sync, 'user_threads', (snapshot) => {
    state.threadsReadiness = snapshot;
    state.collectionReadiness.user_threads = snapshot;
    updateConnectivity();
    render();
  });
  const stops = [unsubscribe];
  for (const name of ['user_thread_states', 'ctox_task_approval_requests']) {
    stops.push(subscribe.call(state.ctx.sync, name, (snapshot) => {
      state.collectionReadiness[name] = snapshot;
      updateConnectivity();
      render();
    }));
  }
  return () => stops.forEach((stop) => { if (typeof stop === 'function') stop(); });
}

async function refresh(options = {}) {
  // Single flight: overlapping refreshes multiply the demand queries below.
  if (state.refreshInFlight) return state.refreshInFlight;
  state.refreshInFlight = refreshOnce(options).finally(() => { state.refreshInFlight = null; });
  return state.refreshInFlight;
}

async function refreshOnce(options = {}) {
  if (options.restartSync) startSync().catch((error) => showError(error));
  const me = currentUserId();
  const [recentThreads, pendingApprovals, recentApprovals, states] = await Promise.all([
    loadCollection('user_threads', recentQuery(THREAD_LIST_LIMIT)),
    me ? loadPersonalPages('ctox_task_approval_requests', { status: 'pending', reviewer_user_id: me })
      : Promise.resolve([]),
    loadCollection('ctox_task_approval_requests', recentQuery(APPROVAL_LIST_LIMIT)),
    me ? loadPersonalPages('user_thread_states', { user_id: me, attention_score: { $gt: 0 } })
      : Promise.resolve([]),
  ]);
  state.personalStateByThread = new Map(states.filter((item) => item.user_id === me).map((item) => [item.thread_id, item]));
  state.recentThreadsComplete = recentThreads.length < THREAD_LIST_LIMIT;
  updateConnectivity();
  const approvalCandidates = mergeRecords(pendingApprovals, recentApprovals);
  const pendingCandidateIds = approvalCandidates
    .filter((item) => item.status === 'pending' && item.reviewer_user_id === me)
    .map((item) => item.id || item.approval_request_id)
    .filter(Boolean);
  const verifiedPendingCandidates = await loadRecordsByIds(
    'ctox_task_approval_requests',
    pendingCandidateIds,
    { strict: true },
  );
  const verifiedIds = new Set(pendingCandidateIds);
  const approvals = mergeRecords(
    approvalCandidates.filter((item) => !verifiedIds.has(item.id || item.approval_request_id)),
    verifiedPendingCandidates,
  );
  const actionableThreadIds = [
    ...approvals.filter((item) => item.status === 'pending' && item.reviewer_user_id === me)
      .map((item) => item.thread_id),
    ...states.filter((item) => Number(item.attention_score || 0) > 0).map((item) => item.thread_id),
  ].filter(Boolean);
  const personalThreadIds = [...actionableThreadIds, state.requestedThreadId].filter(Boolean);
  const personalThreads = await loadRecordsByIds('user_threads', personalThreadIds, { strict: true });
  const availableThreadIds = new Set(personalThreads.map((item) => item.id));
  state.personalComplete = Boolean(me)
    && actionableThreadIds.every((id) => availableThreadIds.has(id));
  updateConnectivity();
  const threads = mergeRecords(recentThreads, personalThreads);
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

  await hydrateSelectedThread(state.selectedId);
}

async function hydrateSelectedThread(threadId) {
  if (!threadId) return;
  const [messages, links, approvals] = await Promise.all([
    loadPersonalPages('user_thread_messages', { thread_id: threadId }),
    loadPersonalPages('user_thread_links', { thread_id: threadId }),
    loadPersonalPages('ctox_task_approval_requests', { thread_id: threadId }),
  ]);
  if (state.selectedId !== threadId) return;
  state.data = {
    ...state.data,
    messages: mergeRecords(state.data.messages, messages),
    links: mergeRecords(state.data.links, links),
    approvals: mergeRecords(state.data.approvals, approvals),
  };
  const selectedBase = {
    threads: state.data.threads.filter((item) => item.id === threadId),
    messages,
    links,
    notifications: state.data.notifications.filter((item) => item.thread_id === threadId),
    approvals,
  };
  const commandIds = linkedCommandIds(selectedBase);
  const commands = await loadRecordsByIds('business_commands', commandIds);
  const taskIds = linkedTaskIds(selectedBase, commands);
  const queue = await loadRecordsByIds('ctox_queue_tasks', taskIds);
  if (state.selectedId !== threadId) return;
  state.data = { ...state.data, commands, queue };
  state.detailCompleteThreadId = threadId;
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

// The personal inbox is keyed by native per-user attention and approval
// records, not by an arbitrary most-recent global thread window. Query pages
// until exhausted so an old pending review remains reachable and counted.
async function loadPersonalPages(name, selector) {
  const records = await collectUniquePages(
    ({ skip, limit }) => loadCollection(name, {
      selector,
      sort: [{ updated_at_ms: 'desc' }],
      skip,
      limit,
    }),
    { pageSize: PERSONAL_PAGE_SIZE, idOf: (item) => item.id || item.approval_request_id },
  );
  return mergeRecords(records);
}

function collectionFor(name) {
  return state.ctx?.db?.collection?.(name) || null;
}

async function loadRecordsByIds(name, ids, options = {}) {
  const collection = collectionFor(name);
  const uniqueIds = [...new Set((ids || []).map((id) => String(id || '').trim()).filter(Boolean))];
  if (!collection?.findOne || !uniqueIds.length) return [];
  // Primary-key lookups use the optimized single-document demand window.
  // A Mango `$in` query over `id` scans large native collections and can hold
  // the shared query collector until its transport deadline.
  const docs = [];
  for (let offset = 0; offset < uniqueIds.length; offset += 20) {
    const batch = uniqueIds.slice(offset, offset + 20);
    docs.push(...await Promise.all(batch.map((id) => {
      const request = collection.findOne(id).exec();
      return options.strict ? request : request.catch(() => null);
    })));
  }
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

function render(options = {}) {
  renderMobileState();
  const threads = visibleThreads();
  renderNotificationPreferences();
  syncGrammarSurfaces(threads.length);
  renderList(threads, options);
  renderDetail();
  if (threads.length || personalCollectionsReady()) recordFirstRenderMetric('first_usable_list');
  if (personalCollectionsReady()) recordFirstRenderMetric('complete_personal_inbox');
  if (state.selectedId && state.detailCompleteThreadId === state.selectedId) {
    recordFirstRenderMetric('selected_detail');
  }
}

function recordFirstRenderMetric(name) {
  const metrics = state.metrics;
  if (!metrics || metrics.recorded.has(name)) return;
  metrics.recorded.add(name);
  const endMark = `${metrics.prefix}.${name}`;
  performance.mark(endMark);
  performance.measure(`ctox.threads.${name}`, `${metrics.prefix}.mount`, endMark);
}

function renderMobileState() {
  els.root?.classList.toggle('is-mobile-detail', state.mobileView === 'detail' && Boolean(state.selectedId));
}

// The glyph and the label name the view the click switches TO, never the one
// on screen. The button carries no aria-pressed: it is an action, not a state.
// The shell grammar still sets one on click (a leftover of the two-button
// era), so every sync strips it again.
function syncViewToggle(button) {
  if (!button) return;
  const next = button.dataset.pgView === 'list' ? 'cards' : 'list';
  const label = next === 'list'
    ? state.t('viewShowList', 'Als Liste anzeigen')
    : state.t('viewShowCards', 'Als Karten anzeigen');
  button.setAttribute('aria-label', label);
  button.title = label;
  button.removeAttribute('aria-pressed');
}

function onLeftGrammarChange(event) {
  const detail = event?.detail || {};
  state.search = String(detail.search ?? '');
  state.listView = detail.view === 'list';
  els.root?.classList.toggle('is-list-view', state.listView);
  syncViewToggle(els.viewToggle);
  // One view dimension, two controls: a secondary tray view wins while set;
  // the neutral tray value hands the view back to the counted band (the
  // module's own band-click listener cleared the tray select first). When
  // nothing is selected anywhere (grammar reset from a secondary view), fall
  // back to the inbox.
  const secondary = String(detail.filters?.view || '');
  if (secondary) {
    state.filter = secondary;
  } else {
    const band = els.leftPane?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand;
    state.filter = band || 'inbox';
  }
  syncSelection();
  // Intentional reset: search/view/filter changes move the content set, so the
  // list scrolls back to the top (the shell scroll guard also clears its
  // recorded offsets on this event).
  render({ resetScroll: true });
}

function onCenterGrammarChange(event) {
  const detail = event?.detail || {};
  state.centerListView = detail.view === 'list';
  els.root?.classList.toggle('is-center-list-view', state.centerListView);
  syncViewToggle(els.centerViewToggle);
}

// Export serializes the currently visible (filtered + searched) thread list
// as a JSON download — the module has no import path, so there is no import
// action (adding one would invent a write flow the domain does not have).
function exportVisibleThreads() {
  const threads = visibleThreads();
  const exportedAt = new Date().toISOString();
  const payload = {
    format: 'ctox-threads-export',
    version: 1,
    exportedAt,
    module: 'threads',
    view: state.filter || 'inbox',
    count: threads.length,
    threads: threads.map((thread) => ({
      threadId: String(thread.id || ''),
      title: String(thread.title || ''),
      kind: String(thread.kind || ''),
      status: String(thread.status || ''),
      sourceModule: String(thread.source_module || ''),
      sourceRecordId: String(thread.source_record_id || ''),
      assignedUserId: String(thread.assigned_user_id || ''),
      updatedAtMs: Number(thread.updated_at_ms || 0),
    })),
  };
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `ctox-threads-${exportedAt.slice(0, 19).replace(/[:T]/g, '-')}.json`;
  anchor.style.display = 'none';
  (state.ctx?.host || document.documentElement).appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

// The primary tab a filter maps to; secondary filters live in the tray only.
const PRIMARY_FILTERS = ['inbox', 'waiting', 'running', 'archived'];
const SECONDARY_FILTERS = ['delegated', 'snoozed', 'team', 'mentions', 'approvals', 'failed', 'all', 'system'];
const THREAD_FILTERS = [...PRIMARY_FILTERS, ...SECONDARY_FILTERS];
const FILTER_LABELS = {
  inbox: 'Jetzt handeln', waiting: 'Wartet auf mich', running: 'AI arbeitet',
  delegated: 'Wartet auf andere', snoozed: 'Später', team: 'Team Queue',
  mentions: 'Erwähnungen', approvals: 'Freigaben', failed: 'Blockiert',
  archived: 'Erledigt / Archiv', all: 'Alle', system: 'CTOX-Tasks (System)',
};

// Counts on the counted view band (ALL four queues, zeros included) + the
// one-line pane footer go through the shell-wired grammar handle when present
// (null-guarded: the shell wires panes debounced ~120ms after mount, so early
// renders fall back to the direct data-pg-* targets). Band/tab selection is
// mirrored here for programmatic filter changes (restore, deep links) so the
// grammar's state read stays consistent.
function syncGrammarSurfaces(visibleCount) {
  const pane = els.leftPane;
  if (!pane) return;
  const me = currentUserId();
  const isAdmin = currentUserRole() === 'chef' || currentUserRole() === 'admin';
  // Counts per primary queue, computed with the same predicate the tab uses.
  const counts = {};
  for (const filter of PRIMARY_FILTERS) {
    counts[filter] = state.data.threads.filter((thread) => threadMatchesFilter(thread, filter, me, isAdmin)).length;
  }
  const displayCounts = Object.fromEntries(
    Object.entries(counts).map(([filter, count]) => [
      filter,
      (filter === 'inbox' ? personalCollectionsReady() : state.recentThreadsComplete)
        ? count : `≥${count}`,
    ]),
  );
  const pg = pane.__ctoxPaneGrammar;
  if (pg?.setCounts) pg.setCounts(displayCounts);
  else for (const [key, value] of Object.entries(displayCounts)) {
    const node = pane.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
  const secondaryActive = !PRIMARY_FILTERS.includes(state.filter);
  for (const tab of pane.querySelectorAll('[data-pg-band]')) {
    const on = !secondaryActive && tab.dataset.pgBand === state.filter;
    tab.setAttribute('aria-selected', String(on));
    tab.classList.toggle('is-active', on);
  }
  const select = pane.querySelector('[data-pg-filter][data-pg-name="view"]');
  if (select) select.value = secondaryActive ? state.filter : '';
  // The grammar's active-dot reads search + tray select; refresh after the
  // programmatic sync above (dot = search active or secondary view set).
  pg?.refreshDot?.();
  const who = state.ctx?.session?.user?.display_name || currentUserId() || '';
  const exact = state.filter === 'inbox' ? personalCollectionsReady() : state.recentThreadsComplete;
  const footerText = `${exact ? '' : 'Mindestens '}${visibleCount} ${visibleCount === 1 ? 'Thread' : 'Threads'} · ${FILTER_LABELS[state.filter] || state.filter}${exact ? '' : ' · weitere Daten möglich'}${who ? ` · als ${who}` : ''}`;
  if (pg?.setFooter) pg.setFooter(footerText);
  else {
    const node = pane.querySelector('[data-pg-footer]');
    if (node) node.textContent = footerText;
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

// One predicate for filtering AND for the counts on the switcher band — the
// numbers a tab shows must be computed by the exact rule the tab applies.
function threadMatchesFilter(thread, filter, me, isAdmin) {
  // Personal relevance applies to EVERYONE including admins — an admin wants
  // their inbox, not a firehose. The wide views are 'all', 'team', 'system'.
  const wideView = ['all', 'team', 'system'].includes(filter);
  if (!wideView && me && !threadRelevantToUser(thread, me)) return false;
  if (filter === 'system') return thread.kind === 'ctox_task';
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
    if (!me) return false;
    if (approvalsForThread(thread.id).some((item) => item.status === 'pending' && item.reviewer_user_id === me)) return true;
    const reasons = new Set(arrayField(userStateForThread(thread.id)?.attention_reasons));
    if (['completed', 'closed', 'archived', 'cancelled', 'canceled'].includes(thread.status)) return false;
    if (thread.kind === 'ctox_task') {
      return reasons.has('Blockiert') || reasons.has('Fehlgeschlagen');
    }
    return reasons.size > 0;
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
  if (state.requestedThreadId) {
    const requested = state.data.threads.find((thread) => thread.id === state.requestedThreadId);
    if (requested) {
      if (!visibleThreads().some((thread) => thread.id === requested.id)) {
        state.filter = requested.status === 'archived' ? 'archived'
          : isSnoozed(requested) ? 'snoozed' : 'all';
      }
      state.selectedId = requested.id;
      state.mobileView = 'detail';
      state.requestedThreadId = '';
      return;
    }
    state.selectedId = '';
    return;
  }
  const visible = visibleThreads();
  if (!visible.some((thread) => thread.id === state.selectedId)) {
    state.selectedId = visible[0]?.id || '';
  }
}

// Inbox-shard anatomy: one glance answers WHAT (title), WHY ME (kind + due),
// HOW URGENT (unread dot, relative time), FROM WHOM (sender initial, foreign
// preview). Three lines, anchored left (dot/avatar) and right (time).
function relativeTime(ms) {
  const t = Number(ms || 0);
  if (!t) return '';
  const diff = Date.now() - t;
  if (diff < 60000) return 'jetzt';
  if (diff < 3600000) return `vor ${Math.round(diff / 60000)} min`;
  if (diff < 86400000) return `vor ${Math.round(diff / 3600000)} h`;
  if (diff < 7 * 86400000) return `vor ${Math.round(diff / 86400000)} d`;
  return formatTime(t);
}

// The accent line says why this needs ME — deduplicated against the title,
// never raw system vocabulary.
function whyMeLine(thread, pendingApprovals) {
  // Server-authoritative first (hub contract): native refresh_thread_states
  // writes attention_reasons in the contract vocabulary; local derivation is
  // only the fallback until states replicate.
  const stored = arrayField(userStateForThread(thread.id)?.attention_reasons);
  if (stored.length) return stored.slice(0, 2).join(' · ');
  const parts = [];
  if (pendingApprovals > 0) parts.push('Freigabe nötig');
  else if (thread.status === 'blocked') parts.push('Blockiert');
  else if (threadHasFailedCtox(thread.id)) parts.push('Fehlgeschlagen');
  else if (threadHasRunningCtox(thread.id)) parts.push('AI arbeitet');
  else if (!thread.assigned_user_id) parts.push('Unzugeteilt');
  const due = Number(thread.due_at_ms || 0);
  if (due) {
    const days = Math.ceil((due - Date.now()) / 86400000);
    if (days < 0) parts.push('Frist überschritten');
    else if (days === 0) parts.push('Frist heute');
    else if (days <= 7) parts.push(`Frist ${new Date(due).toLocaleDateString('de-DE', { weekday: 'short' })}`);
  }
  return parts.join(' · ');
}

// The preview must inform: the latest message from someone OTHER than me,
// prefixed with its sender — my own reply tells me nothing.
function foreignPreview(thread) {
  const me = currentUserId();
  const messages = messagesForThread(thread.id);
  const foreign = [...messages].reverse().find((item) => (item.author_user_id || item.actor_id) !== me);
  const last = foreign || messages[messages.length - 1];
  if (!last?.body) return { sender: '', text: thread.next_step || thread.source_label || '' };
  const sender = last.author_display_name || last.author_user_id || (last.actor_type === 'ai' ? 'CTOX' : '');
  return { sender, text: last.body };
}

// Shard meta: the detail fields an operator needs to triage a thread WITHOUT
// opening it — status, assignment, origin and the deadline date. All of them
// are fields the module already loads (user_threads + the roster it fetches
// for the handoff/note selects); no new query, no new collection.
const STATUS_LABELS = {
  open: ['statusOpen', 'Offen'],
  blocked: ['statusBlocked', 'Blockiert'],
  escalated: ['statusEscalated', 'Eskaliert'],
  snoozed: ['statusSnoozed', 'Später'],
  archived: ['statusArchived', 'Erledigt'],
};

function statusLabel(status) {
  const entry = STATUS_LABELS[String(status || 'open')];
  return entry ? state.t(entry[0], entry[1]) : String(status || '');
}

// Roster names come from the roster the module loads for its people selects;
// before it has replicated the raw id is still more use than nothing.
function rosterName(userId) {
  const id = String(userId || '').trim();
  if (!id) return '';
  return (state.roster || []).find((user) => user.id === id)?.name || id;
}

function threadMetaLine(thread, why) {
  const parts = [];
  const status = statusLabel(thread.status);
  if (status) parts.push(status);
  const assignee = rosterName(thread.assigned_user_id);
  if (assignee) parts.push(assignee);
  const source = String(thread.source_label || thread.source_module || '').trim();
  if (source) parts.push(source);
  const due = Number(thread.due_at_ms || 0);
  // The attention line already shouts near deadlines; meta then adds nothing.
  if (due && !/Frist/.test(String(why || ''))) {
    parts.push(`${state.t('metaDue', 'Frist')} ${new Date(due).toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit' })}`);
  }
  return parts.join(' · ');
}

function threadSenderInitial(preview) {
  const name = String(preview.sender || '').trim();
  if (!name) return '·';
  if (name.toUpperCase() === 'CTOX') return '⌬';
  return name.slice(0, 1).toUpperCase();
}

function renderList(threads, { resetScroll = false } = {}) {
  if (!els.list) return;
  // Data re-renders never move the operator: preserve the scroll offset across
  // the rebuild (intentional resets — search/view/filter — pass resetScroll
  // because the content set changed). Prefer skipping the write entirely when
  // the data signature is unchanged; selection is an in-place class flip.
  if (!threads.length) {
    // Data-driven empty: only when the UNFILTERED source (user_threads) is
    // empty. While that collection has not finished its initial replication
    // (ready === false), this is a sync state, not "no threads". A filtered
    // or searched-out list (source non-empty) stays a plain filter empty.
    const readiness = state.threadsReadiness || readThreadsReadiness();
    if (!state.data.threads.length && (readiness?.ready === false || !personalCollectionsReady())) {
      renderHtmlIfChanged(
        els.list,
        `<div class="ctox-syncing" role="status" aria-live="polite">${escapeHtml(state.t('syncingThreads', 'Threads werden synchronisiert.'))}</div>`,
        { signature: 'state:syncing', preserveScroll: !resetScroll },
      );
    } else {
      renderHtmlIfChanged(
        els.list,
        `<div class="ctox-empty">${escapeHtml(state.t('noThreads', 'Keine relevanten Threads vorhanden.'))}</div>`,
        { signature: 'state:empty', preserveScroll: !resetScroll },
      );
    }
    if (resetScroll) els.list.scrollTop = 0;
    return;
  }
  const me = currentUserId();
  const html = threads.map((thread) => {
    const pending = approvalsForThread(thread.id).filter((item) => item.status === 'pending').length;
    const unread = unreadNotificationsForThread(thread.id, me).length;
    const why = whyMeLine(thread, pending);
    const meta = threadMetaLine(thread, why);
    const preview = foreignPreview(thread);
    return `
      <button type="button" class="ctox-list-item threads-list-item ${unread ? 'is-unread' : ''}"
        data-thread-id="${escapeAttr(thread.id)}"
        data-context-record-id="${escapeAttr(thread.source_record_id || thread.id)}"
        data-context-record-type="thread"
        data-context-label="${escapeAttr(thread.title || '')}"
        aria-selected="false">
        <span class="threads-item-anchor" aria-hidden="true">
          <span class="threads-item-dot"></span>
          <span class="threads-item-avatar">${escapeHtml(threadSenderInitial(preview))}</span>
        </span>
        <span class="threads-item-main">
          <span class="threads-item-top">
            <span class="threads-list-title" title="${escapeHtml(thread.title || thread.id)}">${escapeHtml(thread.title || thread.id)}</span>
            <time class="threads-item-time">${escapeHtml(relativeTime(thread.last_message_at_ms || thread.updated_at_ms))}</time>
          </span>
          <span class="threads-list-meta">${why ? `<b class="threads-attention">${escapeHtml(why)}</b>` : ''}${meta ? `<span class="threads-meta-facts">${escapeHtml(meta)}</span>` : ''}</span>
          <span class="threads-list-preview">${preview.sender ? `<b>${escapeHtml(preview.sender)}:</b> ` : ''}${escapeHtml(preview.text)}</span>
        </span>
      </button>
    `;
  }).join('');
  const signature = JSON.stringify(threads.map((thread) => ([
    thread.id,
    thread.title || '',
    // Meta facts are part of the row now: a status/assignment/deadline change
    // has to repaint the shard even when nothing else moved.
    thread.status || '',
    thread.assigned_user_id || '',
    thread.due_at_ms || 0,
    thread.source_label || thread.source_module || '',
    thread.last_message_at_ms || thread.updated_at_ms || 0,
    approvalsForThread(thread.id).filter((item) => item.status === 'pending').length,
    unreadNotificationsForThread(thread.id, me).length,
    foreignPreview(thread).text,
  ])));
  renderHtmlIfChanged(els.list, html, { signature, preserveScroll: !resetScroll });
  if (resetScroll) els.list.scrollTop = 0;
  applyThreadSelection();
}

// In-place selection: flip is-selected/aria-selected across the existing rows,
// never a list rebuild — a rebuild resets the scroll position under the
// operator's pointer (canonical interaction law; ctox applyTaskSelection).
function applyThreadSelection() {
  els.list?.querySelectorAll('[data-thread-id]').forEach((row) => {
    const on = (row.getAttribute('data-thread-id') || '') === state.selectedId;
    row.classList.toggle('is-selected', on);
    row.setAttribute('aria-selected', String(on));
  });
}

function renderDetail() {
  const thread = state.data.threads.find((item) => item.id === state.selectedId) || null;
  // Publish the open thread as an advisory presence entry (id only).
  try {
    state.ctx?.presence?.set(thread
      ? [{ collection: 'user_threads', recordId: thread.id, mode: 'viewing' }]
      : []);
  } catch {}
  updateThreadPresenceHint(thread);
  if (!thread) {
    const missingRequested = state.requestedThreadId && state.personalComplete;
    const emptyTitle = missingRequested ? 'Thread nicht verfügbar.' : state.t('noSelection', 'Kein Thread ausgewählt.');
    if (els.title) els.title.textContent = emptyTitle;
    if (els.source) {
      els.source.textContent = 'Threads';
      els.source.disabled = true;
      els.source.dataset.threadDeepLink = '';
    }
    if (els.status) els.status.textContent = state.status || 'bereit';
    if (els.timeline) els.timeline.innerHTML = `<div class="ctox-empty">${escapeHtml(emptyTitle)}</div>`;
    if (els.context) els.context.innerHTML = '';
    if (els.messageBody) els.messageBody.disabled = true;
    setThreadActionState(null);
    return;
  }
  if (els.messageBody) els.messageBody.disabled = false;
  if (els.title) els.title.textContent = thread.title || thread.id;
  if (els.source) {
    els.source.textContent = contextLabel(thread);
    const sourceLink = sourceDeepLinkFor(thread);
    els.source.dataset.threadDeepLink = sourceLink;
    els.source.disabled = !sourceLink;
    els.source.classList.toggle('is-linked', Boolean(sourceLink));
    const sourceAction = sourceFocusSupported(thread) ? 'Objekt in der Quell-App öffnen' : 'Quell-App öffnen';
    els.source.title = sourceLink ? sourceAction : '';
    els.source.setAttribute('aria-label', sourceLink ? sourceAction : 'Kein verknüpfter Datensatz');
  }
  if (els.status) {
    const command = linkedCommandsForThread(thread.id).at(-1);
    const task = linkedTasksForThread(thread.id).at(-1);
    const workStatus = task?.status || command?.status;
    els.status.textContent = `Abstimmung: ${statusLabel(thread.status)}${workStatus ? ` · Fachvorgang: ${workStatus}` : ''}`;
  }
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
  const loadingNote = state.detailCompleteThreadId === thread.id
    ? '' : '<div class="ctox-syncing" role="status">Verlauf wird vollständig geladen…</div>';
  const messages = messagesForThread(thread.id);
  const approvals = approvalsForThread(thread.id);
  const timeline = [
    ...messages.map((item) => ({ type: 'message', at: Number(item.created_at_ms || item.updated_at_ms || 0), item })),
    ...approvals.map((item) => ({ type: 'approval', at: Number(item.requested_at_ms || item.created_at_ms || 0), item })),
  ].sort((left, right) => left.at - right.at);
  if (!timeline.length) {
    els.timeline.innerHTML = loadingNote || '<div class="ctox-empty">Noch keine Nachrichten.</div>';
    return;
  }
  const me = currentUserId();
  els.timeline.innerHTML = loadingNote + timeline.map((entry) => {
    if (entry.type === 'approval') return renderApproval(entry.item);
    const message = entry.item;
    const mine = message.author_user_id && message.author_user_id === me;
    const kind = message.event_type || message.kind || 'note';
    const messageLabel = String(message.body || message.source_label || kind || message.id).replace(/\s+/g, ' ').slice(0, 160);
    const sourceLink = sourceDeepLinkFor(message);
    const linkHtml = sourceLink
      ? `<button type="button" class="threads-msg-link" data-thread-deep-link="${escapeAttr(sourceLink)}" title="${sourceFocusSupported(message) ? 'Objekt öffnen' : 'Quell-App öffnen'}">↗ ${escapeHtml(message.source_label || message.source_module || 'Quelle')}</button>`
      : '';
    const isEvent = ['ctox_status', 'approval_request', 'approval_approved', 'approval_rejected', 'handoff', 'status'].includes(kind)
      || (!message.author_user_id && message.actor_type !== 'ai');
    if (isEvent) {
      // System events are protocol, not conversation: one compact line, the
      // human-readable head first, references as links — never a chat bubble.
      const failed = /blockiert|fehlgeschlagen|failed/i.test(message.body || '');
      const head = String(message.body || '').split('\n')[0];
      return `
        <div class="threads-message is-event" data-message-id="${escapeAttr(message.id)}" data-context-record-id="${escapeAttr(message.id)}" data-context-record-type="thread_message" data-context-label="${escapeAttr(messageLabel)}">
          <span class="threads-event-text">${escapeHtml(head)}</span>
          <span class="threads-event-meta">${escapeHtml(relativeTime(message.created_at_ms || message.updated_at_ms))}</span>
          ${linkHtml}
          ${failed ? `<button type="button" class="threads-msg-link is-rework" data-rework-context="${escapeAttr(head)}" title="Neue CTOX-Analyse anfordern">↻ CTOX um Analyse bitten</button>` : ''}
        </div>
      `;
    }
    return `
      <article class="threads-message ${mine ? 'is-mine' : ''}" data-message-id="${escapeAttr(message.id)}" data-context-record-id="${escapeAttr(message.id)}" data-context-record-type="thread_message" data-context-label="${escapeAttr(messageLabel)}">
        <div class="threads-message-meta">${escapeHtml(message.actor_type === 'ai' ? 'CTOX' : (message.author_display_name || message.author_user_id || 'System'))} · ${escapeHtml(relativeTime(message.created_at_ms || message.updated_at_ms))}${linkHtml ? ' · ' : ''}${linkHtml}</div>
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
  const approvalLabel = String(approval.prompt || approval.instruction || approval.source_label || approval.target_module || approval.id).replace(/\s+/g, ' ').slice(0, 160);
  return `
    <article class="threads-approval-card" data-approval-id="${escapeAttr(approval.id)}" data-context-record-id="${escapeAttr(approval.id)}" data-context-record-type="thread_approval" data-context-label="${escapeAttr(approvalLabel)}">
      <div class="threads-card-actions">
        ${sourceDeepLinkFor(approval) ? `<button type="button" class="ctox-pane-icon" data-thread-deep-link="${escapeAttr(sourceDeepLinkFor(approval))}" aria-label="${sourceFocusSupported(approval) ? 'Objekt in der Quell-App öffnen' : 'Quell-App öffnen'}" title="${sourceFocusSupported(approval) ? 'Objekt in der Quell-App öffnen' : 'Quell-App öffnen'}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 5h5v5M19 5l-8 8M9 5H6a1 1 0 0 0-1 1v12a1 1 0 0 0 1-1v-3"/></svg></button>` : ''}
        ${(command?.status === 'failed' || task?.status === 'failed') ? `<button type="button" class="ctox-pane-icon" data-rework-context="${escapeAttr(approval.prompt || approval.instruction || '')}" aria-label="Neue CTOX-Analyse anfordern" title="Neue CTOX-Analyse anfordern"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 10a8 8 0 1 1 2 7"/><path d="M4 5v5h5"/></svg></button>` : ''}
      </div>
      ${canDecide ? `
        <div class="threads-card-actions is-decide">
          <button type="button" class="ctox-pane-icon is-confirm" data-approve-approval="${escapeAttr(approval.id)}" aria-label="${escapeHtml(state.t('approvalApprove', 'Freigeben'))}" title="${escapeHtml(state.t('approvalApprove', 'Freigeben'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l5 5L19 7"/></svg></button>
          <button type="button" class="ctox-pane-icon is-danger" data-reject-approval="${escapeAttr(approval.id)}" aria-label="${escapeHtml(state.t('approvalReject', 'Ablehnen'))}" title="${escapeHtml(state.t('approvalReject', 'Ablehnen'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>
          <button type="button" class="ctox-pane-icon" data-question-approval="${escapeAttr(approval.id)}" aria-label="Rückfrage an Requester" title="Rückfrage an Requester"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 9a3 3 0 1 1 4.2 2.75c-.9.4-1.2 1-1.2 2.05"/><path d="M12 17.3v.2"/></svg></button>
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
      ${command ? `<div class="threads-message-meta"><button type="button" class="threads-msg-link" data-thread-deep-link="#ctox?command_id=${escapeAttr(command.command_id || command.id || '')}" title="Befehl in der CTOX-App öffnen">↗ ${escapeHtml(state.t('approvalCommand', 'Command'))}: ${escapeHtml(command.command_type || '')} · ${escapeHtml(command.status || '')}</button></div>` : ''}
      ${task ? `<div class="threads-message-meta"><button type="button" class="threads-msg-link" data-thread-deep-link="#ctox?task_id=${escapeAttr(task.id || '')}" title="Aufgabe in der CTOX-App öffnen">↗ ${escapeHtml(state.t('approvalTask', 'Task'))}: ${escapeHtml(task.title || task.id)} · ${escapeHtml(task.status || '')}</button></div>` : ''}
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
        <div class="ctox-callout threads-notification-item" data-context-record-id="${escapeAttr(item.id)}" data-context-record-type="thread_notification" data-context-label="${escapeAttr(item.title || item.body_preview || item.notification_type || item.id)}">
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
  const mentionTargets = mentionTargetsIn(body);
  await dispatchThreadsCommand('threads.message.create', {
    thread_id: thread.id,
    body,
    ...(mentionTargets.length ? { target_user_ids: mentionTargets, kind: 'mention', message_kind: 'mention' } : {}),
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
  let commandId = '';
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
    commandId = command.id;
    state.commandReceipt = { commandId, message: `Übermittelt · ${commandId}`, error: false };
    renderCommandReceipt();
    const outcome = await state.ctx.commandBus.dispatch(command, { until });
    state.status = outcome?.status || 'completed';
    state.commandReceipt = {
      commandId: outcome?.command_id || commandId,
      message: `${until === 'terminal' ? 'Bestätigt' : 'Angenommen'} · ${outcome?.command_id || commandId}`,
      error: false,
    };
    renderCommandReceipt();
    return outcome;
  } catch (error) {
    state.commandReceipt = {
      commandId,
      message: `Aktion fehlgeschlagen${commandId ? ` · ${commandId}` : ''}: ${error?.message || String(error)}`,
      error: true,
    };
    renderCommandReceipt();
    throw error;
  } finally {
    setBusy(false);
  }
}

function renderCommandReceipt() {
  if (!els.commandReceipt) return;
  els.commandReceipt.hidden = !state.commandReceipt;
  els.commandReceipt.textContent = state.commandReceipt?.message || '';
  els.commandReceipt.dataset.state = state.commandReceipt?.error ? 'error' : 'info';
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
  [els.watch, els.snooze, els.mobileSnooze, els.archive, els.delegate, ...els.claimButtons].forEach((button) => {
    if (button) button.disabled = disabled;
  });
  const terminal = ['archived', 'completed', 'closed', 'cancelled', 'canceled'].includes(thread?.status);
  for (const button of [els.snooze, els.mobileSnooze, els.archive, els.delegate]) {
    if (button) button.disabled = disabled || terminal;
  }
  for (const button of els.claimButtons) {
    const canClaim = thread && (!thread.assigned_user_id || thread.assigned_user_id === currentUserId())
      && !terminal;
    button.disabled = disabled || !canClaim;
    button.title = thread?.assigned_user_id === currentUserId() ? 'Bereits übernommen' : 'Abstimmung übernehmen';
  }
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
  const deepLink = normalizeInternalDeepLink(row?.deepLink, state.selectedId, registeredModuleIds());
  const valueHtml = deepLink
    ? `<a href="${escapeAttr(deepLink)}" data-thread-deep-link="${escapeAttr(deepLink)}">${escapeHtml(value || deepLink)}</a>`
    : `<span>${escapeHtml(value)}</span>`;
  return `<dt>${escapeHtml(label)}</dt><dd>${valueHtml}</dd>`;
}

// Human-in-the-loop rule: a decision surface must link the OBJECT it decides
// about. Prefer an explicit deep link; otherwise derive the source app hash
// so approving never happens blind.
function sourceDeepLinkFor(entry) {
  return normalizeInternalDeepLink(buildSourceDeepLink(entry, state.selectedId), state.selectedId, registeredModuleIds());
}

function navigateDeepLink(value) {
  const link = normalizeInternalDeepLink(value, state.selectedId, registeredModuleIds());
  if (!link) return;
  persistNavigationState();
  window.location.hash = link;
}

function registeredModuleIds() {
  const modules = state.ctx?.getModules?.() || state.ctx?.modules;
  return Array.isArray(modules) && modules.length
    ? new Set(modules.map((item) => String(item.id || '').trim()))
    : null;
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
  return Boolean(userStateForThread(thread.id))
    || thread.assigned_user_id === userId
    || arrayField(thread.participant_ids).includes(userId)
    || arrayField(thread.watcher_user_ids).includes(userId)
    || notificationsForThread(thread.id).some((item) => item.user_id === userId && item.status !== 'dismissed')
    || approvalsForThread(thread.id).some((item) => item.reviewer_user_id === userId || item.requester_user_id === userId)
    || messagesForThread(thread.id).some((item) => item.author_user_id === userId || arrayField(item.target_user_ids).includes(userId));
}

function threadMentionsUser(threadId, userId) {
  if (!userId) return false;
  return arrayField(userStateForThread(threadId)?.attention_reasons).includes('Erwähnung');
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
  return state.personalStateByThread?.get(threadId) || null;
}

function attentionReasons(thread) {
  return arrayField(userStateForThread(thread.id)?.attention_reasons);
}

function attentionScore(thread) {
  return Number(userStateForThread(thread.id)?.attention_score || 0);
}

// Module-owned UI persistence goes through ctx.storageScope (workspace- and
// actor-scoped by the shell); sessionStorage is only the legacy fallback for
// shells without the storageScope facade.
function storageGet(key) {
  try {
    const scoped = state.ctx?.storageScope?.get?.(key);
    if (scoped != null) return scoped;
  } catch {}
  try { return sessionStorage.getItem(key); } catch { return null; }
}

function storageSet(key, value) {
  try {
    if (state.ctx?.storageScope?.set) { state.ctx.storageScope.set(key, value); return; }
  } catch {}
  try { sessionStorage.setItem(key, value); } catch {}
}

function storageRemove(key) {
  try { state.ctx?.storageScope?.remove?.(key); } catch {}
  try { sessionStorage.removeItem(key); } catch {}
}

function draftKey() {
  return `ctox:threads:draft:${currentUserId() || 'anonymous'}:${state.selectedId || 'new'}`;
}

function persistDraft() {
  storageSet(draftKey(), String(els.messageBody?.value || ''));
}

function restoreDraft() {
  try {
    const saved = storageGet(draftKey());
    if (saved && els.messageBody) els.messageBody.value = saved;
    const nav = JSON.parse(storageGet(`ctox:threads:navigation:${currentUserId() || 'anonymous'}`) || '{}');
    if (!state.requestedThreadId) state.requestedThreadId = String(nav.selectedId || '');
    if (nav.filter && THREAD_FILTERS.includes(nav.filter)) state.filter = nav.filter;
    state.mobileView = nav.mobileView === 'detail' ? 'detail' : 'list';
  } catch {}
}

function clearDraft() {
  storageRemove(draftKey());
}

function persistNavigationState() {
  storageSet(`ctox:threads:navigation:${currentUserId() || 'anonymous'}`, JSON.stringify({
    selectedId: state.selectedId,
    filter: state.filter,
    mobileView: state.mobileView,
  }));
}

function updateConnectivity() {
  const online = navigator.onLine !== false;
  if (els.syncState) {
    const ready = personalCollectionsReady();
    els.syncState.textContent = !online
      ? 'Offline · lokaler Datenstand'
      : ready ? 'Lokaler Datenstand bereit' : 'Threads werden synchronisiert';
    els.syncState.dataset.state = !online ? 'offline' : ready ? 'ready' : 'catching-up';
    els.syncState.classList.toggle('is-danger', !online || !ready);
  }
  els.root?.classList.toggle('is-offline', !online);
}

function notifyActionRequired(notifications) {
  const canShowWebNotification = typeof Notification !== 'undefined'
    && Notification.permission === 'granted';
  const showSystemNotification = state.ctx?.notifications?.showSystem;
  if (!canShowWebNotification && typeof showSystemNotification !== 'function') return;
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
      const decisionHubDedupeKey = item.source_module === 'kundenpipeline' && item.source_record_id
        ? `ctox:decision-hub:notified:${currentUserId()}:${item.source_record_id}`
        : '';
      if (storageGet(dedupeKey) || (decisionHubDedupeKey && storageGet(decisionHubDedupeKey))) return;
      if (decisionHubDedupeKey && typeof showSystemNotification === 'function') {
        const delivered = showSystemNotification({
          kind: 'decision_hub',
          title: item.title || 'Decision Hub',
          message: 'Decision Hub wartet auf deine Entscheidung.',
          tag: `decision-hub:${item.source_record_id}`,
          recordId: item.source_record_id,
          urgency: 'normal',
          action: {
            callback: () => {
              location.hash = `kundenpipeline?record=${encodeURIComponent(item.source_record_id)}`;
            },
          },
        });
        if (!delivered) return;
        storageSet(dedupeKey, '1');
        storageSet(decisionHubDedupeKey, '1');
        return;
      }
      if (!canShowWebNotification) return;
      storageSet(dedupeKey, '1');
      if (decisionHubDedupeKey) storageSet(decisionHubDedupeKey, '1');
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


// ============================================================================
// Team roster + mentions: humans pick people, they never type user ids.
async function ensureRoster() {
  if (Array.isArray(state.roster) && state.roster.length) return state.roster;
  try {
    const collection = await state.ctx.db.collection('business_users');
    const docs = await collection.find({}).exec();
    state.roster = (docs || [])
      .map((doc) => (doc?.toJSON ? doc.toJSON() : doc))
      .filter((user) => user && !user.is_deleted && (user.id || user.user_id))
      .map((user) => ({ id: user.id || user.user_id, name: user.display_name || user.name || user.id || user.user_id }));
  } catch {
    state.roster = [];
  }
  // The current user always exists, even before the roster replicates.
  const me = currentUserId();
  if (me && !state.roster.some((user) => user.id === me)) {
    state.roster.push({ id: me, name: state.ctx?.session?.user?.display_name || me });
  }
  return state.roster;
}

function renderRosterSelects() {
  const roster = state.roster || [];
  for (const select of els.root?.querySelectorAll('select[data-roster-select]') || []) {
    const current = select.value;
    select.innerHTML = ['<option value="">— Person wählen —</option>',
      ...roster.map((user) => `<option value="${escapeAttr(user.id)}">${escapeHtml(user.name)}</option>`)].join('');
    if (current) select.value = current;
  }
}

// @mention parsing: match @token (and @"two words" prefixes) against roster
// names and ids, case-insensitive.
function mentionTargetsIn(body) {
  const roster = state.roster || [];
  const targets = new Set();
  for (const match of String(body || '').matchAll(/@([\p{L}\p{N}_.-]+)/gu)) {
    const token = match[1].toLowerCase();
    for (const user of roster) {
      if (user.id.toLowerCase() === token
        || user.name.toLowerCase() === token
        || user.name.toLowerCase().split(/\s+/)[0] === token) {
        targets.add(user.id);
      }
    }
  }
  return [...targets];
}

// Lightweight @-autocomplete: typing "@pre" over the composer shows matching
// teammates; click or Enter inserts the name.
function wireMentionPopup() {
  const textarea = els.messageBody;
  const popup = els.root?.querySelector('[data-mention-popup]');
  if (!textarea || !popup) return;
  const close = () => { popup.hidden = true; popup.innerHTML = ''; };
  const apply = (user) => {
    const value = textarea.value;
    const upto = value.slice(0, textarea.selectionStart).replace(/@([\p{L}\p{N}_.-]*)$/u, `@${user.name.split(/\s+/)[0]} `);
    textarea.value = upto + value.slice(textarea.selectionStart);
    textarea.focus();
    close();
  };
  textarea.addEventListener('input', async () => {
    const upto = textarea.value.slice(0, textarea.selectionStart);
    const match = upto.match(/@([\p{L}\p{N}_.-]*)$/u);
    if (!match) { close(); return; }
    const roster = await ensureRoster();
    const query = match[1].toLowerCase();
    const hits = roster.filter((user) => !query
      || user.name.toLowerCase().includes(query)
      || user.id.toLowerCase().includes(query)).slice(0, 6);
    if (!hits.length) { close(); return; }
    popup.innerHTML = hits.map((user) => `<button type="button" data-mention-id="${escapeAttr(user.id)}"><b>${escapeHtml(user.name)}</b><span>${escapeHtml(user.id)}</span></button>`).join('');
    popup.hidden = false;
    for (const button of popup.querySelectorAll('[data-mention-id]')) {
      button.addEventListener('click', () => {
        const user = roster.find((entry) => entry.id === button.dataset.mentionId);
        if (user) apply(user);
      });
    }
  });
  textarea.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') close();
    if (event.key === 'Enter' && !popup.hidden) {
      const first = popup.querySelector('[data-mention-id]');
      if (first) { event.preventDefault(); first.click(); }
    }
  });
  textarea.addEventListener('blur', () => { window.setTimeout(close, 200); });
}

// A third decision besides approve/reject: ask the requester before deciding.
// Directed at the requester (mention -> notification), the approval stays open.
async function askApprovalQuestion(approvalId) {
  const approval = approvalById(approvalId);
  if (!approval) return;
  const requester = approval.requester_user_id || '';
  const question = window.prompt(`Rückfrage an ${approval.requester_display_name || requester || 'Requester'}:`);
  if (!question) return;
  await dispatchThreadsCommand('threads.message.create', {
    thread_id: approval.thread_id || state.selectedId,
    body: `Rückfrage zur Freigabe: ${question}`,
    ...(requester ? { target_user_ids: [requester], kind: 'mention', message_kind: 'mention' } : {}),
  }, {
    recordId: approval.thread_id || approvalId,
    sourceModule: approval.source_module || 'threads',
  });
  await refresh();
}


// A failed CTOX command can start a separate analysis request. This does not
// retry or repair the source command; its owner retains that lifecycle.
async function requestRework(context) {
  const thread = selectedThread();
  if (!thread) return;
  const goal = window.prompt('Was soll CTOX analysieren?', `Analysiere den Fehler: ${context}`.trim());
  if (!goal) return;
  await dispatchThreadsCommand('threads.ai.request', {
    thread_id: thread.id,
    goal,
    risk_class: 'internal',
  }, { recordId: thread.id });
  await refresh();
}
