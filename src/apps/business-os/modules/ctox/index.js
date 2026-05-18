import { collections } from './schema.js';
import { loadModuleMessages } from '../../shared/i18n.js';

const FLOW_WIDTH = 1760;
const FLOW_HEIGHT = 1050;
const NODE_WIDTH = 136;
const NODE_HEIGHT = 76;
const DEFAULT_ZOOM = 1;
const LEFT_COLUMN_WIDTH_KEY = 'ctox.businessOs.ctox.leftColumnWidth';
const LEFT_COLUMN_MIN = 220;
const LEFT_COLUMN_MAX = 560;
const HARNESS_FLOW_CACHE_KEY = 'ctox.businessOs.ctox.lastHarnessFlow';
const HARNESS_REFRESH_MS = 4000;
const LOCAL_RENDER_DEBOUNCE_MS = 80;
const CTOX_FETCH_TIMEOUT_MS = 1500;
const CTOX_STYLE_BUILD = '20260518-communications1';

const labels = {
  de: {
    now: 'Jetzt',
    noActiveWork: 'Keine aktive Arbeit',
    idleDetail: 'Wartet auf den nächsten CTOX-Lauf oder ein Live-Ereignis.',
    loadingRuntime: 'CTOX Runtime wird geladen',
    loadingRuntimeDetail: 'Flow, Queue und Status werden aktualisiert.',
    loadingQueue: 'Tasks werden geladen',
    loadingQueueDetail: 'Synchronisiere aktuelle Arbeit.',
    live: 'Live',
    recentWork: 'Zuletzt',
    tasks: 'Tasks',
    newestFirst: 'neueste zuerst',
    taskSteps: 'Zwischenschritte',
    currentWork: 'Aktuell',
    waitingWork: 'Wartet',
    doneWork: 'Erledigt',
    blockedWork: 'Blockiert',
    selectedTask: 'Ausgewählter Task',
    inboundChannels: 'Inbound-Kanäle',
    inboundItems: 'Eingänge',
    inboundEndpoint: 'Task-Eingang',
    outboundEndpoint: 'Task-Abschluss',
    openOutcome: 'Abschluss offen',
    unprovenOutcome: 'Abschluss nicht belegt',
    taskDetail: 'Task-Details',
    currentStep: 'Aktuelle Station',
    source: 'Quelle',
    status: 'Status',
    created: 'Angelegt',
    summary: 'Zusammenfassung',
    evidence: 'Evidenz',
    stationDetail: 'Stationsdetails',
    tools: 'Werkzeuge',
    openTaskDetail: 'Details im Drawer anzeigen',
    activityPath: 'Aktivitätspfad',
    finishedWork: 'Erledigt',
    liveFlow: 'CTOX Live Flow',
    doingNow: 'Was CTOX gerade tut',
    measurements: 'Messung',
    inputTokens: 'Input Tokens',
    outputTokens: 'Output Tokens',
    toolCalls: 'Tool Calls',
    elapsed: 'Zeit',
    notCaptured: 'nicht erfasst',
    connected: 'verbunden',
    fallback: 'Fallback-Daten',
    notLogged: 'Zeit nicht geloggt',
    timeline: 'Timeline',
    queue: 'Pipeline',
    active: 'aktiv',
    nowQueue: 'Jetzt',
    messages: 'Nachrichten',
    tickets: 'Tickets',
    backlog: 'Backlog',
    task: 'Neue Aufgabe',
    sendNow: 'Direkt senden',
    addQueue: 'In Queue',
    instruction: 'CTOX Anweisung',
    priority: 'Priorität',
    send: 'Senden',
    sending: 'Sendet...',
    runtime: 'Runtime',
    model: 'Modell',
    mode: 'Modus',
    context: 'Kontext',
    maxRun: 'Max. Laufzeit',
    applyModel: 'Modell anwenden',
    communicationRule: 'Kommunikationsregel',
    verifyRule: 'Regel prüfen',
    queueAction: 'CTOX Live Flow weiterführen',
    addTask: 'Aufgabe anlegen',
    modifyApp: 'App modifizieren',
    inspectContext: 'Kontext inspizieren',
    refreshHarness: 'Harness aktualisieren',
    noWorkHere: 'Hier liegt gerade keine Arbeit.',
    noRecentWork: 'Noch keine aktuelle Arbeit erfasst.',
    noMetrics: 'keine Live-Tokenmetriken',
    routing: 'Routing',
    inbound: 'Inbound',
    outbound: 'Outbound',
    ticketsOpen: 'Offene Tickets',
    runtimePolicy: 'Runtime / Policies',
    queued: 'Command angelegt',
  },
  en: {
    now: 'Now',
    noActiveWork: 'No active work',
    idleDetail: 'Waiting for the next CTOX run or live event.',
    loadingRuntime: 'Loading CTOX runtime',
    loadingRuntimeDetail: 'Updating flow, queue, and status.',
    loadingQueue: 'Loading tasks',
    loadingQueueDetail: 'Syncing current work.',
    live: 'Live',
    recentWork: 'Recent work',
    tasks: 'Tasks',
    newestFirst: 'newest first',
    taskSteps: 'Steps',
    currentWork: 'Current',
    waitingWork: 'Waiting',
    doneWork: 'Done',
    blockedWork: 'Blocked',
    selectedTask: 'Selected task',
    inboundChannels: 'Inbound channels',
    inboundItems: 'inbound',
    inboundEndpoint: 'Task inbound',
    outboundEndpoint: 'Task outcome',
    openOutcome: 'Outcome open',
    unprovenOutcome: 'Outcome not proven',
    taskDetail: 'Task details',
    currentStep: 'Current station',
    source: 'Source',
    status: 'Status',
    created: 'Created',
    summary: 'Summary',
    evidence: 'Evidence',
    stationDetail: 'Station details',
    tools: 'Tools',
    openTaskDetail: 'Show details in drawer',
    activityPath: 'Activity path',
    finishedWork: 'Finished work',
    liveFlow: 'CTOX live flow',
    doingNow: 'What CTOX is doing now',
    measurements: 'Measurements',
    inputTokens: 'Input tokens',
    outputTokens: 'Output tokens',
    toolCalls: 'Tool calls',
    elapsed: 'Time',
    notCaptured: 'not captured',
    connected: 'connected',
    fallback: 'fallback data',
    notLogged: 'time not logged',
    timeline: 'Timeline',
    queue: 'Pipeline',
    active: 'active',
    nowQueue: 'Now',
    messages: 'Messages',
    tickets: 'Tickets',
    backlog: 'Backlog',
    task: 'New task',
    sendNow: 'Send now',
    addQueue: 'Add to queue',
    instruction: 'CTOX instruction',
    priority: 'Priority',
    send: 'Send',
    sending: 'Sending...',
    runtime: 'Runtime',
    model: 'Model',
    mode: 'Mode',
    context: 'Context',
    maxRun: 'Max run time',
    applyModel: 'Apply model',
    communicationRule: 'Communication rule',
    verifyRule: 'Verify rule',
    queueAction: 'Continue CTOX live flow',
    addTask: 'Add task',
    modifyApp: 'Modify app',
    inspectContext: 'Inspect context',
    refreshHarness: 'Refresh harness',
    noWorkHere: 'No work here right now.',
    noRecentWork: 'No recent work recorded yet.',
    noMetrics: 'no live token metrics',
    routing: 'Routing',
    inbound: 'Inbound',
    outbound: 'Outbound',
    ticketsOpen: 'Open tickets',
    runtimePolicy: 'Runtime / policies',
    queued: 'Command queued',
  },
};

// Canonical display model: src/service/core_state_machine.rs:review_harness_transition_catalog().
const STATE_MACHINE_NODES = [
  { id: 'queued', label: 'Waiting in queue', phase: 'Queued', x: 330, y: 520, lines: ['Work is in the review harness queue.'], tools: ['NoProof'] },
  { id: 'leased', label: 'Picked up', phase: 'Leased', x: 510, y: 520, lines: ['CTOX has leased the queued work.'], tools: ['NoProof'] },
  { id: 'running', label: 'Working', phase: 'Running', x: 690, y: 520, lines: ['The worker is executing the leased work.'], tools: ['NoProof'] },
  { id: 'awaiting-review', label: 'Ready for review', phase: 'AwaitingReview', x: 870, y: 520, lines: ['WorkerFinished moved the work into review.'], tools: ['WorkerFinished'] },
  { id: 'review-queued', label: 'Review waiting', phase: 'ReviewQueued', x: 1050, y: 520, lines: ['StartReview queued the review.'], tools: ['StartReview'] },
  { id: 'reviewing', label: 'Under review', phase: 'Reviewing', x: 1230, y: 520, lines: ['SpawnReviewer started the reviewer.'], tools: ['SpawnReviewer'] },
  { id: 'review-passed', label: 'Review passed', phase: 'ReviewPassed', x: 1050, y: 790, lines: ['ReviewPass approved the work for validation.'], tools: ['ReviewPass'] },
  { id: 'review-rejected', label: 'Review failed', phase: 'ReviewRejected', x: 1230, y: 790, lines: ['ReviewReject sends the work to rework.'], tools: ['ReviewReject'] },
  { id: 'review-unavailable', label: 'Review unavailable', phase: 'ReviewUnavailable', x: 1230, y: 880, lines: ['The reviewer was unavailable.'], tools: ['ReviewUnavailable'] },
  { id: 'review-retry', label: 'Retry review', phase: 'ReviewRetry', x: 1050, y: 880, lines: ['RetryReview returns to AwaitingReview.'], tools: ['RetryReview'] },
  { id: 'rework-required', label: 'Rework needed', phase: 'ReworkRequired', x: 690, y: 880, lines: ['ReworkRequired requeues the same main work or fails after budget.'], tools: ['RequeueSameMainWork', 'ReviewRoundsExhausted', 'ValidatorFail'] },
  { id: 'awaiting-validation', label: 'Needs evidence', phase: 'AwaitingValidation', x: 870, y: 790, lines: ['ReviewPass requires validation before success.'], tools: ['ReviewPass'] },
  { id: 'validating', label: 'Checking evidence', phase: 'Validating', x: 690, y: 790, lines: ['RunValidator checks the result evidence.'], tools: ['RunValidator'] },
  { id: 'passed', label: 'Evidence confirmed', phase: 'Passed', x: 510, y: 790, lines: ['ValidatorPass is the only terminal success.'], tools: ['ValidatorPass'] },
  { id: 'model-failed', label: 'Work failed', phase: 'ModelFailed', x: 510, y: 880, lines: ['WorkerFailed or exhausted review/validation budget stopped the work.'], tools: ['WorkerFailed', 'ReviewRoundsExhausted', 'ValidatorReworkExhausted'] },
  { id: 'infra-failed', label: 'Service failed', phase: 'InfraFailed', x: 1050, y: 990, lines: ['InfraError, ReviewRetriesExhausted, or ValidatorInfraError stopped the work.'], tools: ['InfraError', 'ReviewRetriesExhausted', 'ValidatorInfraError'] },
];

const STATE_MACHINE_EDGES = [
  ['queued', 'leased'], ['leased', 'running'],
  ['running', 'awaiting-review', 'WorkerFinished'], ['running', 'model-failed', 'WorkerFailed', 'down'], ['running', 'infra-failed', 'InfraError', 'down'],
  ['awaiting-review', 'review-queued', 'StartReview'], ['review-queued', 'reviewing', 'SpawnReviewer'],
  ['reviewing', 'review-passed', 'ReviewPass'], ['reviewing', 'review-rejected', 'ReviewReject'], ['reviewing', 'review-unavailable', 'ReviewUnavailable'],
  ['review-passed', 'awaiting-validation', 'ReviewPass'], ['review-rejected', 'rework-required', 'ReviewReject'],
  ['review-unavailable', 'review-retry', 'ReviewUnavailable'], ['review-unavailable', 'infra-failed', 'ReviewRetriesExhausted'],
  ['review-retry', 'awaiting-review', 'RetryReview', 'loop'], ['rework-required', 'queued', 'RequeueSameMainWork', 'loop'], ['rework-required', 'model-failed', 'ReviewRoundsExhausted'],
  ['awaiting-validation', 'validating', 'RunValidator'], ['validating', 'passed', 'ValidatorPass'], ['validating', 'rework-required', 'ValidatorFail'],
  ['validating', 'model-failed', 'ValidatorReworkExhausted'], ['validating', 'infra-failed', 'ValidatorInfraError'],
].map(([from, to, label, route]) => ({ from, to, label, route: route || 'normal' }));

const TRACE_ORDER = STATE_MACHINE_NODES.map((node) => node.id);
const TRACE_ORDER_INDEX = new Map(TRACE_ORDER.map((id, index) => [id, index]));
const REVIEW_HARNESS_NODE_IDS = STATE_MACHINE_NODES.map((node) => node.id);
const REVIEW_HARNESS_NODE_SET = new Set(REVIEW_HARNESS_NODE_IDS);
const REVIEW_HARNESS_EDGES = STATE_MACHINE_EDGES;

const COMMUNICATION_NODES = [
  { id: 'comm-inbound-observed', state: 'InboundObserved', label: 'Inbound observed', phase: 'FounderCommunication', x: 150, y: 135, lines: ['A communication message exists in communication_messages.'] },
  { id: 'comm-context-built', state: 'ContextBuilt', label: 'Context built', phase: 'FounderCommunication', x: 330, y: 135, lines: ['BuildContext created the answer context.'] },
  { id: 'comm-reply-needed', state: 'ReplyNeeded', label: 'Reply needed', phase: 'FounderCommunication', x: 510, y: 135, lines: ['CTOX determined that this communication needs a response.'] },
  { id: 'comm-no-response-needed', state: 'NoResponseNeeded', label: 'No response needed', phase: 'FounderCommunication', x: 510, y: 45, lines: ['CTOX determined that no response should be sent.'] },
  { id: 'comm-drafting', state: 'Drafting', label: 'Drafting', phase: 'FounderCommunication', x: 690, y: 135, lines: ['DraftReply is composing the outbound response.'] },
  { id: 'comm-draft-ready', state: 'DraftReady', label: 'Draft ready', phase: 'FounderCommunication', x: 870, y: 135, lines: ['A draft exists and is ready for review.'] },
  { id: 'comm-reviewing', state: 'Reviewing', label: 'Reviewing', phase: 'FounderCommunication', x: 1050, y: 135, lines: ['RequestReview moved the draft into review.'] },
  { id: 'comm-approved', state: 'Approved', label: 'Approved', phase: 'FounderCommunication', x: 1230, y: 135, lines: ['Approve allowed the protected outbound send.'] },
  { id: 'comm-rework-required', state: 'ReworkRequired', label: 'Rework required', phase: 'FounderCommunication', x: 1050, y: 245, lines: ['Review required rework before any send.'] },
  { id: 'comm-sending', state: 'Sending', label: 'Sending', phase: 'FounderCommunication', x: 1410, y: 135, lines: ['Send is in progress through the communication adapter.'] },
  { id: 'comm-sent', state: 'Sent', label: 'Sent', phase: 'FounderCommunication', x: 1590, y: 135, lines: ['The outbound message was accepted by the channel adapter.'] },
  { id: 'comm-send-failed', state: 'SendFailed', label: 'Send failed', phase: 'FounderCommunication', x: 1410, y: 245, lines: ['The outbound provider failed; delivery repair is required.'] },
  { id: 'comm-delivery-repair', state: 'DeliveryRepair', label: 'Delivery repair', phase: 'FounderCommunication', x: 1230, y: 245, lines: ['Repair the failed delivery without recomposing a new artifact.'] },
  { id: 'comm-awaiting-ack', state: 'AwaitingAcknowledgement', label: 'Awaiting acknowledgement', phase: 'FounderCommunication', x: 1590, y: 245, lines: ['The message was sent and CTOX is waiting for acknowledgement.'] },
  { id: 'comm-done', state: 'Done', label: 'Done', phase: 'FounderCommunication', x: 1590, y: 330, lines: ['The communication thread is complete.'] },
  { id: 'comm-escalated', state: 'Escalated', label: 'Escalated', phase: 'FounderCommunication', x: 690, y: 245, lines: ['ReplyNeeded could not proceed and was escalated.'] },
];

const COMMUNICATION_EDGES = [
  ['comm-inbound-observed', 'comm-context-built'],
  ['comm-context-built', 'comm-reply-needed'],
  ['comm-context-built', 'comm-no-response-needed', 'up'],
  ['comm-reply-needed', 'comm-drafting'],
  ['comm-drafting', 'comm-draft-ready'],
  ['comm-draft-ready', 'comm-reviewing'],
  ['comm-reviewing', 'comm-approved'],
  ['comm-reviewing', 'comm-rework-required', 'down'],
  ['comm-rework-required', 'comm-context-built', 'loop'],
  ['comm-approved', 'comm-sending'],
  ['comm-sending', 'comm-sent'],
  ['comm-sending', 'comm-send-failed', 'down'],
  ['comm-send-failed', 'comm-delivery-repair'],
  ['comm-delivery-repair', 'comm-sending', 'loop'],
  ['comm-sent', 'comm-awaiting-ack', 'down'],
  ['comm-awaiting-ack', 'comm-done', 'down'],
  ['comm-no-response-needed', 'comm-done', 'up'],
  ['comm-reply-needed', 'comm-escalated', 'down'],
].map(([from, to, route]) => ({ from, to, route: route || 'normal' }));

const COMMUNICATION_NODE_MAP = new Map(COMMUNICATION_NODES.map((node) => [node.id, node]));
const COMMUNICATION_STATE_TO_NODE = new Map(COMMUNICATION_NODES.map((node) => [normalizeCoreStateKey(node.state), node.id]));

const ctoxSeed = {
  runs: [],
  queue: [],
  communications: [],
  tickets: [],
  tools: [],
};

export async function mount(ctx) {
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;

  const state = {
    ctx,
    lang: ctx.locale === 'en' ? 'en' : 'de',
    flow: fallbackHarnessFlow(),
    model: null,
    selectedStepIndex: 0,
    selectedTaskId: null,
    zoom: DEFAULT_ZOOM,
    statusMessage: '',
    runtimeStatus: 'Loading status',
    focusTask: readFocusTask(),
    detailDrawer: null,
    openTaskSections: new Set(['current']),
    userNavigatedTimeline: false,
    liveBaseSeconds: 0,
    liveStartedAt: Date.now(),
    liveTicker: null,
    refreshTimer: null,
    localSubscriptionCleanup: null,
    refreshInFlight: false,
    layoutResizeCleanup: null,
    flowViewport: { left: 0, top: 0 },
  };

  applyHarnessColumnWidth(ctx.host, readStoredLeftColumnWidth());
  const harness = ctx.host.querySelector('[data-ctox-harness]');
  if (harness) harness.__ctoxState = state;
  state.layoutResizeCleanup = wireColumnResize(state);
  await loadCtoxMessages(state.lang);
  await renderFromLocalCache(state);
  startLiveTicker(state);
  state.localSubscriptionCleanup = wireLocalRealtime(state);
  refresh(state);
  state.refreshTimer = window.setInterval(() => refresh(state), HARNESS_REFRESH_MS);
  const teardownShellMessages = wireShellMessages(state);
  return () => {
    window.clearInterval(state.liveTicker);
    window.clearInterval(state.refreshTimer);
    state.localSubscriptionCleanup?.();
    state.layoutResizeCleanup?.();
    if (harness) delete harness.__ctoxState;
    teardownShellMessages();
  };
}

async function loadCtoxMessages(lang) {
  const language = lang === 'en' ? 'en' : 'de';
  labels[language] = await loadModuleMessages(import.meta.url, language, labels);
}

async function renderFromLocalCache(state) {
  const [commands, queueTasks, bugReports] = await Promise.all([
    loadLocalCommands(state.ctx).catch(() => []),
    loadLocalQueueTasks(state.ctx).catch(() => []),
    loadLocalBugReports(state.ctx).catch(() => []),
  ]);
  state.flow = loadCachedHarnessFlow() || state.flow || fallbackHarnessFlow();
  const bundle = mergeBundleWithCommands(ctoxSeed, commands, queueTasks, bugReports);
  const metrics = aggregateFlowMetrics(state.flow);
  state.liveBaseSeconds = Number.isFinite(metrics.seconds) ? metrics.seconds : 0;
  state.liveStartedAt = Date.now();
  state.model = buildHarnessModel(bundle, state.flow);
  state.focusTask = readFocusTask();
  reconcileSelection(state);
  render(state);
}

function wireLocalRealtime(state) {
  const collectionsToWatch = ['business_commands', 'ctox_queue_tasks', 'ctox_bug_reports'];
  let renderTimer = null;
  const scheduleRender = () => {
    if (renderTimer) return;
    renderTimer = window.setTimeout(() => {
      renderTimer = null;
      renderFromLocalCache(state).catch((error) => {
        console.warn('[ctox] local realtime render failed', error);
      });
    }, LOCAL_RENDER_DEBOUNCE_MS);
  };
  const subscriptions = collectionsToWatch
    .map((collectionName) => {
      const collection = state.ctx.db?.raw?.[collectionName];
      return collection?.$?.subscribe?.(scheduleRender) || null;
    })
    .filter(Boolean);
  return () => {
    if (renderTimer) window.clearTimeout(renderTimer);
    renderTimer = null;
    for (const sub of subscriptions) {
      try { sub.unsubscribe?.(); } catch {}
    }
  };
}

async function refresh(state) {
  if (state.refreshInFlight) return;
  state.refreshInFlight = true;
  const useHttpBridge = state.ctx?.sync?.config?.http_bridge_available !== false;
  try {
    const [flow, commands, queueTasks, bugReports, status] = await Promise.all([
      useHttpBridge
        ? loadHarnessFlow()
        : Promise.resolve(loadCachedHarnessFlow() || fallbackHarnessFlow('http_bridge_disabled')),
      loadLocalCommands(state.ctx).catch(() => []),
      loadLocalQueueTasks(state.ctx).catch(() => []),
      loadLocalBugReports(state.ctx).catch(() => []),
      useHttpBridge
        ? loadStatus().catch((error) => ({ ok: false, error: String(error?.message || error) }))
        : Promise.resolve({ ok: true, runtime: 'local-first', now_ms: Date.now() }),
    ]);
    if (flow?.ok) saveCachedHarnessFlow(flow);
    const nextFlow = flow?.ok ? flow : (state.flow || loadCachedHarnessFlow() || fallbackHarnessFlow(flow?.error || ''));
    const bundle = mergeBundleWithCommands(ctoxSeed, commands, queueTasks, bugReports);
    state.flow = nextFlow;
    const metrics = aggregateFlowMetrics(nextFlow);
    state.liveBaseSeconds = Number.isFinite(metrics.seconds) ? metrics.seconds : 0;
    state.liveStartedAt = Date.now();
    state.model = buildHarnessModel(bundle, nextFlow);
    state.focusTask = readFocusTask();
    reconcileSelection(state);
    state.runtimeStatus = status?.ok ? displayFlowMode(status.runtime || 'native-rust') : (status.error || 'Status unavailable');
    render(state);
  } finally {
    state.refreshInFlight = false;
  }
}

async function renderLoading(state) {
  const t = labels[state.lang];
  state.ctx.host.querySelector('[data-ctox-left]').innerHTML = `
    <div class="ctox-panel-title">
      <span>${escapeHtml(t.tasks)}</span>
      <strong>${escapeHtml(t.loadingQueue)}</strong>
      <small>${escapeHtml(t.loadingQueueDetail)}</small>
    </div>
    <div class="ctox-loading-list" aria-hidden="true">
      <span></span>
      <span></span>
      <span></span>
    </div>
  `;
  state.ctx.host.querySelector('[data-ctox-main]').innerHTML = `
    <section class="ctox-loading-state" aria-live="polite" aria-busy="true">
      <div>
        <strong>${escapeHtml(t.loadingRuntime)}</strong>
        <span>${escapeHtml(t.loadingRuntimeDetail)}</span>
      </div>
    </section>
  `;
}

function render(state) {
  renderLeft(state);
  renderMain(state);
  wireContextMenu(state);
  updateLiveIndicators(state);
}

function readStoredLeftColumnWidth() {
  const stored = localStorage.getItem(LEFT_COLUMN_WIDTH_KEY);
  if (!stored) return 340;
  const width = Number(stored);
  return Number.isFinite(width) ? clampMetric(width, LEFT_COLUMN_MIN, LEFT_COLUMN_MAX) : 340;
}

function applyHarnessColumnWidth(host, width) {
  const harness = host?.querySelector?.('[data-ctox-harness]');
  if (!harness) return;
  harness.style.setProperty('--ctox-left-width', `${Math.round(clampMetric(width, LEFT_COLUMN_MIN, LEFT_COLUMN_MAX))}px`);
}

function wireColumnResize(state) {
  const harness = state.ctx.host.querySelector('[data-ctox-harness]');
  const handle = state.ctx.host.querySelector('[data-ctox-column-resizer]');
  if (!harness || !handle) return () => {};
  let drag = null;
  const onPointerMove = (event) => {
    if (!drag) return;
    const nextWidth = clampMetric(drag.width + event.clientX - drag.x, LEFT_COLUMN_MIN, Math.min(LEFT_COLUMN_MAX, Math.max(LEFT_COLUMN_MIN, harness.clientWidth - 420)));
    applyHarnessColumnWidth(state.ctx.host, nextWidth);
    localStorage.setItem(LEFT_COLUMN_WIDTH_KEY, String(Math.round(nextWidth)));
  };
  const endDrag = () => {
    if (!drag) return;
    drag = null;
    document.body.classList.remove('ctox-column-resizing');
    handle.classList.remove('is-dragging');
  };
  const onPointerDown = (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const left = state.ctx.host.querySelector('[data-ctox-left]');
    drag = { x: event.clientX, width: left?.getBoundingClientRect().width || readStoredLeftColumnWidth() };
    document.body.classList.add('ctox-column-resizing');
    handle.classList.add('is-dragging');
    handle.setPointerCapture?.(event.pointerId);
  };
  handle.addEventListener('pointerdown', onPointerDown);
  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', endDrag);
  window.addEventListener('pointercancel', endDrag);
  return () => {
    handle.removeEventListener('pointerdown', onPointerDown);
    window.removeEventListener('pointermove', onPointerMove);
    window.removeEventListener('pointerup', endDrag);
    window.removeEventListener('pointercancel', endDrag);
    document.body.classList.remove('ctox-column-resizing');
  };
}

function renderLeft(state) {
  const t = labels[state.lang];
  const model = state.model;
  const left = state.ctx.host.querySelector('[data-ctox-left]');
  const groups = taskGroups(model.tasks);
  const activeCount = groups.current.length;
  syncOpenTaskSections(state, groups);
  left.innerHTML = `
    <div class="ctox-panel-title ctox-context-item" data-context-label="${escapeAttr(t.tasks)}" data-context-record-id="ctox-tasks">
      <span>${escapeHtml(t.tasks)}</span>
      <strong>${escapeHtml(model.tasks.length ? `${activeCount} ${t.active}` : t.noActiveWork)}</strong>
    </div>
    ${inboundChannelPanel(model.inboundChannels, state)}
    <div class="ctox-task-board">
      ${taskSection('current', t.currentWork, groups.current, state)}
      ${taskSection('blocked', t.blockedWork, groups.blocked, state)}
      ${taskSection('waiting', t.waitingWork, groups.waiting, state)}
      ${taskSection('done', t.doneWork, groups.done, state)}
    </div>
  `;
  left.querySelectorAll('[data-task-section]').forEach((section) => {
    section.addEventListener('toggle', () => {
      if (section.open) state.openTaskSections.add(section.dataset.taskSection);
      else state.openTaskSections.delete(section.dataset.taskSection);
    });
  });
  left.querySelectorAll('[data-task-id]').forEach((button) => {
    button.addEventListener('click', () => {
      selectTask(state, button.dataset.taskId, { drawer: true, center: false });
    });
  });
}

function syncOpenTaskSections(state, groups) {
  if (!state.openTaskSections?.size) state.openTaskSections = new Set(['current']);
  const selected = getSelectedTask(state);
  const selectedGroup = selected ? groupKeyForTask(selected) : '';
  if (selectedGroup) state.openTaskSections.add(selectedGroup);
  if (!groups.current.length && groups.blocked.length && !selectedGroup) state.openTaskSections.add('blocked');
}

function groupKeyForTask(task) {
  const status = normalizeCommandStatus(task?.status || '');
  if (['running', 'leased', 'review', 'drafting'].includes(status)) return 'current';
  if (['blocked', 'failed', 'cancelled', 'handled'].includes(status)) return 'blocked';
  if (['done', 'completed', 'sent', 'approved', 'healthy'].includes(status)) return 'done';
  return 'waiting';
}

function taskSection(key, title, tasks, state) {
  const t = labels[state.lang];
  const open = state.openTaskSections?.has(key) || tasks.some((task) => task.id === state.selectedTaskId);
  if (!tasks.length && !open) return '';
  return `
    <details class="ctox-task-section" data-task-section="${escapeAttr(key)}" ${open ? 'open' : ''}>
      <summary><span>${escapeHtml(title)}</span><strong>${tasks.length}</strong></summary>
      <div class="ctox-task-list">
        ${tasks.length ? tasks.map((task) => taskRow(task, state)).join('') : `<p>${escapeHtml(t.noWorkHere)}</p>`}
      </div>
    </details>
  `;
}

function taskRow(task, state) {
  const t = labels[state.lang];
  const focused = isFocusedTask(task, state.focusTask);
  const selected = task.id === state.selectedTaskId;
  const channel = task.channelLabel || displayWorkSource(task.channel || task.source || task.moduleId || 'ctox');
  return `
    <button type="button" class="ctox-task-row ctox-context-item ${focused ? 'is-focused-task' : ''} ${selected ? 'is-selected' : ''} ${statusClass(task.status)}" data-task-id="${escapeAttr(task.id)}" data-context-label="${escapeAttr(task.title)}" data-context-record-id="${escapeAttr(task.id)}" data-ctox-task-id="${escapeAttr(task.taskId || task.id)}" data-ctox-command-id="${escapeAttr(task.commandId || '')}" aria-label="${escapeAttr(`${t.openTaskDetail}: ${task.title}`)}">
      <span class="ctox-task-status">${escapeHtml(displayStatus(task.status, state.lang))}</span>
      <span class="ctox-task-copy">
        <strong>${escapeHtml(task.title)}</strong>
        <small>${escapeHtml(channel)}</small>
      </span>
    </button>
  `;
}

function inboundChannelPanel(channels, state) {
  const t = labels[state.lang];
  if (!channels?.length) return '';
  return `
    <section class="ctox-inbound-panel ctox-context-item" data-context-label="${escapeAttr(t.inboundChannels)}" data-context-record-id="ctox-inbound-channels">
      <header>
        <span>${escapeHtml(t.inboundChannels)}</span>
        <strong>${channels.reduce((sum, channel) => sum + channel.count, 0)}</strong>
      </header>
      <div class="ctox-inbound-list">
        ${channels.map((channel) => `
          <article class="${channel.active ? 'is-active' : ''}">
            <span>${escapeHtml(channel.label)}</span>
            <small>${escapeHtml(`${channel.count} ${t.inboundItems}`)}</small>
          </article>
        `).join('')}
      </div>
    </section>
  `;
}

function taskSteps(task, state) {
  const timeline = state.model?.timeline || [];
  if (timeline.length && taskMatchesHarnessFlow(task, state)) {
    return timeline.map((node, index) => ({
      id: node.id,
      label: node.label,
      detail: clip(cleanUiCopy(node.lines?.[0] || node.phase || itemSummary(task) || ''), 180),
      timestamp: node.timestamp || '',
      metrics: metricsLabel(node, state.lang),
      active: node.status === 'active' || index === timeline.length - 1,
      timelineIndex: index,
    }));
  }
  return taskStatusSteps(task, state);
}

function taskMatchesHarnessFlow(task, state) {
  if (!task || !state) return false;
  if (isFocusedTask(task, state.focusTask)) return true;
  const source = state.flow?.flow?.source || {};
  const ids = new Set([source.message_key, source.work_id].filter(Boolean));
  if (ids.has(task.id) || ids.has(task.taskId) || ids.has(task.commandId) || ids.has(task.runId)) return true;
  const currentTask = state.model?.tasks?.find((item) => normalizeCommandStatus(item.status) === 'running');
  return Boolean(currentTask && task.id === currentTask.id);
}

function taskStatusSteps(task, state) {
  const status = normalizeCommandStatus(task.status);
  const timeline = state.model?.timeline || [];
  const findIndex = (id) => {
    const index = timeline.findIndex((node) => node.id === id);
    return index >= 0 ? index : clampIndex(state.selectedStepIndex, timeline.length);
  };
  const steps = [];
  if (status === 'queued') {
    steps.push({ id: 'queued', label: displayStatus('queued', state.lang), detail: task.target || task.summary || task.source || '', active: true });
  } else if (status === 'running') {
    steps.push({ id: 'queued', label: displayStatus('queued', state.lang), detail: task.target || task.summary || task.source || '', active: false });
    steps.push({ id: 'running', label: displayStatus('running', state.lang), detail: task.summary || task.target || task.commandId || task.taskId || task.id, active: status === 'running' });
  } else if (status === 'failed' || status === 'blocked') {
    steps.push({ id: 'model-failed', label: state.lang === 'en' ? 'Needs attention' : 'Braucht Klärung', detail: task.resultSummary || task.summary || status, active: true });
  } else {
    steps.push({ id: 'queued', label: displayStatus(status, state.lang), detail: task.resultSummary || task.summary || task.target || task.source || '', active: true });
  }
  if (isFocusedTask(task, state.focusTask) || task.taskId === state.flow?.flow?.source?.message_key) {
    for (const block of state.flow?.flow?.blocks || []) {
      if (block.kind === 'task') {
        steps.push({
          id: 'queued',
          label: block.title || block.kind,
          detail: (block.lines || []).join(' · '),
          active: false,
        });
      }
      if (block.kind === 'attempt' && blockHasExplicitRuntimeEvidence(block)) {
        steps.push({
          id: 'running',
          label: block.title || block.kind,
          detail: (block.lines || []).join(' · '),
          active: false,
        });
      }
      for (const branch of block.branches || []) {
        const nodeId = branchToNodeId(branch.kind, branch.title || '', branch.lines || []);
        if (!nodeId) continue;
        steps.push({
          id: nodeId,
          label: branch.title || branch.kind,
          detail: (branch.lines || []).join(' · '),
          active: false,
        });
      }
    }
  }
  return steps.map((step) => ({ ...step, timelineIndex: findIndex(step.id), detail: clip(cleanUiCopy(step.detail), 180) }));
}

function renderMain(state) {
  const t = labels[state.lang];
  const model = state.model;
  const timelineIndex = clampIndex(state.selectedStepIndex, model.timeline.length);
  const selectedTask = getSelectedTask(state);
  const taskStepView = selectedTask ? selectedTaskStepView(selectedTask, state) : null;
  const selectedNode = taskStepView?.node || model.timeline[timelineIndex] || model.nodes.find((node) => node.id === model.activeNodeId) || model.nodes[0];
  const visibleTrace = taskStepView
    ? buildVisibleTraceFromSteps(model, taskStepView.steps, taskStepView.index)
    : buildVisibleTrace(model.timeline, timelineIndex);
  const metrics = aggregateFlowMetrics(state.flow);
  const live = isHarnessLive(state);
  const elapsedSeconds = live ? liveElapsedSeconds(state) : metrics.seconds;
  const main = state.ctx.host.querySelector('[data-ctox-main]');
  const previousViewport = readFlowViewport(state);
  main.innerHTML = `
    <header class="ctox-flow-head">
      <div>
        <span>${escapeHtml(t.liveFlow)}</span>
        <h1>${escapeHtml(t.doingNow)}</h1>
      </div>
      <div class="ctox-flow-source">
        <strong>${escapeHtml(displayFlowMode(state.flow.mode || 'ctox_core'))}</strong>
        <span>${escapeHtml(state.flow.ok ? t.connected : t.fallback)}</span>
        ${live ? liveStatusMarkup(state) : ''}
      </div>
    </header>
    <section class="ctox-metrics-strip" aria-label="${escapeAttr(t.measurements)}">
      ${metricCard(t.inputTokens, metrics.inputTokens, 'tokens', state.lang)}
      ${metricCard(t.outputTokens, metrics.outputTokens, 'tokens', state.lang)}
      ${metricCard(t.toolCalls, metrics.toolCalls, 'count', state.lang)}
      ${metricCard(t.elapsed, elapsedSeconds, 'seconds', state.lang, { live })}
    </section>
    <div class="ctox-flow-canvas" data-flow-canvas>
      <div class="ctox-flow-toolbar" aria-label="Flow chart controls" data-flow-control>
        <button type="button" data-zoom="-">-</button>
        <span>${Math.round(state.zoom * 100)}%</span>
        <button type="button" data-zoom="+">+</button>
        <button type="button" data-zoom="reset">Reset</button>
      </div>
      <div class="ctox-flow-canvas-inner" style="width:${FLOW_WIDTH * state.zoom}px;height:${FLOW_HEIGHT * state.zoom}px">
        ${flowSvg(model, selectedNode, visibleTrace, selectedTask, state)}
      </div>
    </div>
    ${timelinePanel(state, selectedTask, selectedNode, metrics)}
  `;
  restoreFlowViewport(state, previousViewport);
  main.querySelectorAll('[data-zoom]').forEach((button) => {
    button.addEventListener('click', () => {
      const action = button.dataset.zoom;
      state.zoom = action === 'reset' ? DEFAULT_ZOOM : clampMetric(Math.round((state.zoom + (action === '+' ? 0.12 : -0.12)) * 100) / 100, 0.72, 1.8);
      renderMain(state);
    });
  });
  main.querySelectorAll('[data-timeline-step]').forEach((button) => {
    button.addEventListener('click', () => {
      setTimelineStep(state, Number(button.dataset.timelineStep), { center: true });
    });
  });
  main.querySelector('[data-timeline-range]')?.addEventListener('input', (event) => {
    const mappedSteps = event.target.dataset.timelineRangeSteps
      ? event.target.dataset.timelineRangeSteps.split(',').map((value) => Number(value))
      : null;
    setTimelineStep(state, mappedSteps?.[Number(event.target.value)] ?? Number(event.target.value), { center: true });
  });
  main.querySelectorAll('[data-node-id]').forEach((node) => {
    node.addEventListener('click', () => {
      const nextIndex = findLastTimelineIndex(model.timeline, node.dataset.nodeId);
      state.detailDrawer = { type: 'node', nodeId: node.dataset.nodeId };
      setTimelineStep(state, nextIndex, { center: false });
    });
  });
  wireCanvasDrag(main.querySelector('[data-flow-canvas]'));
  updateLiveIndicators(state);
}

function timelinePanel(state, selectedTask, selectedNode, metrics) {
  const t = labels[state.lang];
  if (!selectedTask) {
    const max = Math.max(state.model.timeline.length - 1, 0);
    const value = clampIndex(state.selectedStepIndex, state.model.timeline.length);
    return `
      <section class="ctox-timeline-panel" aria-label="Activity timeline" style="--timeline-progress:${escapeAttr(progressPercent(value, max))}%">
        <div class="ctox-timeline-head">
          <div>
            <span>${escapeHtml(t.timeline)}</span>
            ${timelineLiveStatusMarkup(selectedTask, selectedNode, state)}
          </div>
          <strong>${escapeHtml(selectedNode?.label || '')}</strong>
        </div>
        <div class="ctox-timeline-scrub">
          <input aria-label="Select activity event" max="${max}" min="0" step="1" type="range" value="${value}" data-timeline-range />
        </div>
        <div class="ctox-timeline-detail">
          <span>${escapeHtml(selectedNode?.phase || '')}</span>
          <p>${escapeHtml(selectedNode?.lines?.[0] || 'No detail is available for this event yet.')}</p>
          <small>${escapeHtml(selectedNode ? metricsLabel(selectedNode, state.lang) : '')}</small>
        </div>
      </section>
    `;
  }
  const steps = taskSteps(selectedTask, state);
  const selectedTimelineIndex = clampIndex(state.selectedStepIndex, state.model.timeline.length);
  const selectedStepIndex = steps.findIndex((step) => step.timelineIndex === selectedTimelineIndex);
  const activeStepIndex = state.userNavigatedTimeline && selectedStepIndex >= 0
    ? selectedStepIndex
    : Math.max(0, steps.findIndex((step) => step.active));
  const current = steps[activeStepIndex] || steps.find((step) => step.active) || steps.at(-1);
  const max = Math.max(steps.length - 1, 0);
  return `
    <section class="ctox-timeline-panel is-task-timeline" aria-label="${escapeAttr(t.taskSteps)}" style="--timeline-progress:${escapeAttr(progressPercent(activeStepIndex, max))}%">
      <div class="ctox-timeline-head">
        <div>
          <span>${escapeHtml(t.timeline)}</span>
          ${timelineLiveStatusMarkup(selectedTask, current, state)}
        </div>
        <strong>${escapeHtml(selectedTask.title)}</strong>
      </div>
      <div class="ctox-timeline-scrub">
        <input aria-label="${escapeAttr(t.taskSteps)}" max="${max}" min="0" step="1" type="range" value="${activeStepIndex}" data-timeline-range data-timeline-range-steps="${escapeAttr(steps.map((step) => step.timelineIndex).join(','))}" />
        <div class="ctox-timeline-scale" role="list">
          ${steps.map((step, index) => `
            <button type="button" role="listitem" class="${index < activeStepIndex ? 'is-done' : ''} ${index === activeStepIndex ? 'is-current' : ''}" data-timeline-step="${step.timelineIndex}">
              <span>${String(index + 1).padStart(2, '0')}</span>
              <strong>${escapeHtml(step.label)}</strong>
              <small>${escapeHtml(stepMetaLabel(step, state))}</small>
            </button>
          `).join('')}
        </div>
      </div>
      <div class="ctox-timeline-detail">
        <span>${escapeHtml(current?.label || t.currentStep)}</span>
        <p>${escapeHtml(current?.detail || selectedNode?.lines?.[0] || itemSummary(selectedTask) || t.noRecentWork)}</p>
        <small>${escapeHtml(current ? `${stepMetaLabel(current, state)} · ${current.metrics || ''}` : selectedNode ? metricsLabel(selectedNode, state.lang) : '')}</small>
      </div>
    </section>
  `;
}

function progressPercent(value, max) {
  if (!Number.isFinite(max) || max <= 0) return 100;
  return Math.round((clampMetric(value, 0, max) / max) * 100);
}

function flowSvg(model, selectedNode, visibleTrace, selectedTask, state) {
  return `
    <svg class="ctox-flow-diagram" viewBox="0 0 ${FLOW_WIDTH} ${FLOW_HEIGHT}" preserveAspectRatio="xMidYMin meet" role="img" aria-label="CTOX work flow diagram">
      <defs>
        <marker id="ctox-flow-arrow" markerHeight="8" markerWidth="8" orient="auto" refX="7" refY="4">
          <path d="M0,0 L8,4 L0,8 Z"></path>
        </marker>
      </defs>
      <g class="ctox-flow-lanes" aria-hidden="true">
        <rect x="18" y="18" width="${FLOW_WIDTH - 36}" height="340" rx="16"></rect>
        <rect x="18" y="388" width="${FLOW_WIDTH - 36}" height="260" rx="16"></rect>
        <rect x="18" y="688" width="${FLOW_WIDTH - 36}" height="340" rx="16"></rect>
        <text x="34" y="44">Founder communication state machine</text>
        <text x="34" y="414">Review harness queue and execution</text>
        <text x="34" y="714">Review harness evidence check</text>
      </g>
      ${communicationFlowSvg(selectedTask, state)}
      ${taskEndpointFlowSvg(model, selectedTask, selectedNode, visibleTrace, state)}
      ${model.edges.map((edge) => {
        const from = model.nodeMap.get(edge.from);
        const to = model.nodeMap.get(edge.to);
        if (!from || !to) return '';
        const strength = visibleTrace.edgeStrength.get(edgeKey(edge.from, edge.to)) || 0;
        return `<path class="ctox-flow-edge ${strength > 0 ? 'is-observed' : ''} ${edge.to === selectedNode?.id && strength > 0 ? 'is-active-edge' : ''}" d="${edgePath(from, to, edge.route)}" style="--edge-strength:${strength}"></path>`;
      }).join('')}
      ${model.nodes.map((node) => flowNodeSvg(node, selectedNode, visibleTrace.nodeStrength.get(node.id) || 0)).join('')}
    </svg>
  `;
}

function taskEndpointFlowSvg(model, selectedTask, selectedNode, visibleTrace, state) {
  return `
    ${inboundEndpointFlowSvg(model, selectedTask, state)}
    ${outboundEndpointFlowSvg(model, selectedTask, selectedNode, visibleTrace, state)}
  `;
}

function communicationFlowSvg(selectedTask, state) {
  if (!isCommunicationFlow(selectedTask, state)) return '';
  const trace = communicationTraceFromFlow(state.flow, selectedTask);
  const observed = new Set(trace);
  const edgeObserved = new Set();
  trace.forEach((id, index) => {
    const previous = trace[index - 1];
    if (previous) edgeObserved.add(edgeKey(previous, id));
  });
  return `
    <g class="ctox-communication-flow" aria-label="Founder communication state machine">
      ${COMMUNICATION_EDGES.map((edge) => {
        const from = COMMUNICATION_NODE_MAP.get(edge.from);
        const to = COMMUNICATION_NODE_MAP.get(edge.to);
        if (!from || !to) return '';
        const active = edgeObserved.has(edgeKey(edge.from, edge.to));
        return `<path class="ctox-flow-edge ctox-communication-edge ${active ? 'is-observed' : ''}" d="${edgePath(from, to, edge.route)}" style="--edge-strength:${active ? 0.92 : 0}"></path>`;
      }).join('')}
      ${COMMUNICATION_NODES.map((node) => communicationNodeSvg(node, observed.has(node.id), trace.at(-1) === node.id)).join('')}
    </g>
  `;
}

function communicationNodeSvg(node, observed, current) {
  return `
    <g class="ctox-flow-node-g ctox-communication-node ${observed ? 'is-observed is-trace' : 'is-possible'} ${current ? 'is-current is-selected' : ''}"
       style="--trace-strength:${observed ? 0.86 : 0}" transform="translate(${node.x} ${node.y})">
      ${current ? `<rect class="ctox-flow-node-live-ring" x="${-NODE_WIDTH / 2 - 8}" y="${-NODE_HEIGHT / 2 - 8}" width="${NODE_WIDTH + 16}" height="${NODE_HEIGHT + 16}" rx="16"></rect>` : ''}
      <rect class="ctox-flow-node-box" x="${-NODE_WIDTH / 2}" y="${-NODE_HEIGHT / 2}" width="${NODE_WIDTH}" height="${NODE_HEIGHT}" rx="12"></rect>
      <text class="ctox-flow-node-phase" x="${-NODE_WIDTH / 2 + 10}" y="${-NODE_HEIGHT / 2 + 16}">${escapeHtml(node.state)}</text>
      <text class="ctox-flow-node-title" x="${-NODE_WIDTH / 2 + 10}" y="${-NODE_HEIGHT / 2 + 34}">
        ${wrapSvgText(node.label).map((line, index) => `<tspan x="${-NODE_WIDTH / 2 + 10}" dy="${index === 0 ? 0 : 15}">${escapeHtml(line)}</tspan>`).join('')}
      </text>
    </g>
  `;
}

function isCommunicationFlow(task, state) {
  const sourceKind = String(state.flow?.flow?.source?.source_kind || '').toLowerCase();
  if (sourceKind === 'message' || sourceKind === 'communication') return true;
  const channel = normalizeInboundChannel(task?.channel || task?.source || '');
  return channel === 'business_os.llm.chat' || channel.includes('communication') || channel.includes('email') || channel.includes('chat');
}

function communicationTraceFromFlow(flowResult, selectedTask) {
  const flow = flowResult?.flow || {};
  const ids = [];
  const push = (id) => {
    if (!id || ids.at(-1) === id) return;
    ids.push(id);
  };
  if (flow.source?.message_key || isCommunicationFlow(selectedTask, { flow: flowResult })) push('comm-inbound-observed');
  for (const block of flow.blocks || []) {
    for (const branch of block.branches || []) {
      for (const line of branch.lines || []) {
        const match = String(line).match(/Accepted:\s*([A-Za-z]+)\s*->\s*([A-Za-z]+)\s*\(([^)]+)\)/);
        if (!match) continue;
        const from = COMMUNICATION_STATE_TO_NODE.get(normalizeCoreStateKey(match[1]));
        const to = COMMUNICATION_STATE_TO_NODE.get(normalizeCoreStateKey(match[2]));
        if (from) push(from);
        if (to) push(to);
      }
    }
  }
  return ids.length ? ids : ['comm-inbound-observed'];
}

function normalizeCoreStateKey(value) {
  return String(value || '').replace(/[^a-z0-9]/gi, '').toLowerCase();
}

function inboundEndpointFlowSvg(model, selectedTask, state) {
  const channels = model.inboundChannels || [];
  const t = labels[state.lang];
  const endpoint = inboundEndpointForTask(selectedTask, state);
  const selectedChannel = normalizeInboundChannel(endpoint.id);
  const queued = model.nodeMap.get('queued') || { x: 330, y: 520 };
  const nodeX = 44;
  const nodeWidth = 144;
  const nodeY = queued.y - 26;
  const selectedEdgeY = nodeY + 26;
  const queueLeft = queued.x - NODE_WIDTH / 2;
  const queueApproachX = Math.max(nodeX + nodeWidth + 22, queueLeft - 26);
  const detail = endpoint.detail || (channels.length ? `${channels.reduce((sum, channel) => sum + channel.count, 0)} ${t.inboundItems}` : '');
  return `
    <g class="ctox-flow-inbound" aria-label="Inbound channels feeding CTOX queue">
      <text class="ctox-flow-inbound-label" x="${nodeX}" y="${nodeY - 14}">${escapeHtml(t.inboundEndpoint)}</text>
      <path class="ctox-flow-channel-edge is-selected" d="M ${nodeX + nodeWidth} ${selectedEdgeY} L ${queueApproachX} ${selectedEdgeY} L ${queueApproachX} ${queued.y} L ${queueLeft} ${queued.y}"></path>
      <g class="ctox-flow-channel-node is-selected" transform="translate(${nodeX} ${nodeY})">
        <rect width="${nodeWidth}" height="52" rx="12"></rect>
        <text class="ctox-flow-channel-name" x="12" y="19">${escapeHtml(clip(endpoint.label, 18))}</text>
        <text class="ctox-flow-channel-count" x="12" y="36">${escapeHtml(clip(detail || endpoint.kind, 20))}</text>
      </g>
      ${channels.filter((channel) => channel.id !== selectedChannel).slice(0, 4).map((channel, index) => {
        const x = nodeX;
        const y = nodeY + 66 + index * 56;
        const edgeY = y + 22;
        const d = `M ${x + nodeWidth} ${edgeY} L ${queueApproachX} ${edgeY} L ${queueApproachX} ${queued.y} L ${queueLeft} ${queued.y}`;
        return `
          <path class="ctox-flow-channel-edge" d="${d}"></path>
          <g class="ctox-flow-channel-node" transform="translate(${x} ${y})">
            <rect width="${nodeWidth}" height="44" rx="12"></rect>
            <text class="ctox-flow-channel-name" x="12" y="18">${escapeHtml(clip(channel.label, 18))}</text>
            <text class="ctox-flow-channel-count" x="12" y="34">${escapeHtml(`${channel.count} ${t.inboundItems}`)}</text>
          </g>
        `;
      }).join('')}
    </g>
  `;
}

function outboundEndpointFlowSvg(model, selectedTask, selectedNode, visibleTrace, state) {
  const t = labels[state.lang];
  const endpoint = outboundEndpointForTask(selectedTask, selectedNode, state);
  const sourceNode = endpoint.fromNodeId ? model.nodeMap.get(endpoint.fromNodeId) : null;
  if (!sourceNode) return '';
  const x = FLOW_WIDTH - 176;
  const y = Math.max(126, Math.min(FLOW_HEIGHT - 84, sourceNode.y - 26));
  const sourceHalfW = (sourceNode.shape === 'diamond' ? NODE_WIDTH * 0.58 : NODE_WIDTH) / 2;
  const d = `M ${sourceNode.x + sourceHalfW} ${sourceNode.y} L ${x - 24} ${sourceNode.y} L ${x - 24} ${y + 26} L ${x} ${y + 26}`;
  const observed = Boolean(visibleTrace.nodeStrength.get(sourceNode.id)) || endpoint.closed;
  return `
    <g class="ctox-flow-outbound" aria-label="Task outcome endpoint">
      <text class="ctox-flow-inbound-label" x="${x}" y="${y - 12}">${escapeHtml(t.outboundEndpoint)}</text>
      <path class="ctox-flow-channel-edge is-outbound ${observed ? 'is-selected' : ''} ${endpoint.closed ? 'is-terminal' : 'is-open'}" d="${d}"></path>
      <g class="ctox-flow-channel-node is-outbound ${observed ? 'is-selected' : ''} ${endpoint.closed ? 'is-terminal' : 'is-open'}" transform="translate(${x} ${y})">
        <rect width="144" height="52" rx="12"></rect>
        <text class="ctox-flow-channel-name" x="12" y="19">${escapeHtml(clip(endpoint.label, 20))}</text>
        <text class="ctox-flow-channel-count" x="12" y="36">${escapeHtml(clip(endpoint.detail, 22))}</text>
      </g>
    </g>
  `;
}

function inboundEndpointForTask(task, state) {
  const source = state.flow?.flow?.source || {};
  const channel = task?.channel || task?.inbound_channel || source.source_kind || inferInboundChannel(task || {});
  const label = task?.channelLabel || inboundChannelLabel(channel);
  const detail = [
    task?.taskId || task?.commandId || task?.ticketId || source.message_key || source.work_id || '',
    task?.source ? displayWorkSource(task.source) : '',
  ].filter(Boolean).join(' · ');
  return {
    id: normalizeInboundChannel(channel),
    kind: source.source_kind || 'task',
    label,
    detail,
  };
}

function outboundEndpointForTask(task, selectedNode, state) {
  const t = labels[state.lang];
  const status = normalizeCommandStatus(task?.status || '');
  const terminalNode = terminalNodeForTask(task, selectedNode, state);
  const terminalLabels = {
    passed: state.lang === 'en' ? 'Delivered / closed' : 'Ausgeliefert / geschlossen',
    'model-failed': state.lang === 'en' ? 'Failed' : 'Fehlgeschlagen',
    'infra-failed': state.lang === 'en' ? 'Service failure' : 'Servicefehler',
  };
  if (terminalNode) {
    return {
      fromNodeId: terminalNode,
      label: terminalLabels[terminalNode] || displayStatus(status, state.lang),
      detail: outboundDetailForTask(task, state) || (terminalNode === 'passed' ? 'ValidatorPass' : displayStatus(status, state.lang)),
      closed: true,
    };
  }
  const looksClosed = ['completed', 'done', 'sent', 'approved', 'handled'].includes(status);
  const fallbackNode = selectedNode?.id || state.model?.activeNodeId || 'queued';
  return {
    fromNodeId: fallbackNode,
    label: looksClosed ? t.unprovenOutcome : t.openOutcome,
    detail: outboundDetailForTask(task, state) || displayStatus(status || 'queued', state.lang),
    closed: false,
  };
}

function terminalNodeForTask(task, selectedNode, state) {
  const status = normalizeCommandStatus(task?.status || '');
  if (selectedNode && ['passed', 'model-failed', 'infra-failed'].includes(selectedNode.id) && selectedNode.status === 'done') return selectedNode.id;
  const last = state.model?.timeline?.at?.(-1);
  if (last && ['passed', 'model-failed', 'infra-failed'].includes(last.id) && last.status === 'done') return last.id;
  if (['failed', 'cancelled'].includes(status)) return 'model-failed';
  return null;
}

function outboundDetailForTask(task, state) {
  if (!task) return '';
  const payload = task.payload && typeof task.payload === 'object' ? task.payload : {};
  const context = task.client_context && typeof task.client_context === 'object' ? task.client_context : {};
  const result = task.result && typeof task.result === 'object' ? task.result : {};
  const candidates = [
    task.outbound_channel,
    task.destination,
    task.recipient,
    task.resultSummary,
    result.outbound_channel,
    result.destination,
    result.recipient,
    payload.outbound_channel,
    payload.destination,
    payload.reply_to,
    payload.recipient,
    context.outbound_channel,
    context.destination,
    context.reply_to,
    context.recipient,
  ];
  const value = candidates.find((candidate) => String(candidate || '').trim());
  if (value) return cleanUiCopy(String(value));
  return task.channelLabel || inboundChannelLabel(task.channel || inferInboundChannel(task));
}

function flowNodeSvg(node, selectedNode, traceStrength) {
  const isVisibleTrace = traceStrength > 0;
  const isSelected = node.id === selectedNode?.id;
  const ring = !isSelected ? '' : node.shape === 'diamond'
    ? `<path class="ctox-flow-node-live-ring" d="M 0 ${-NODE_HEIGHT / 2 - 8} L ${NODE_WIDTH / 2 + 10} 0 L 0 ${NODE_HEIGHT / 2 + 8} L ${-NODE_WIDTH / 2 - 10} 0 Z"></path>`
    : `<rect class="ctox-flow-node-live-ring" x="${-NODE_WIDTH / 2 - 9}" y="${-NODE_HEIGHT / 2 - 9}" width="${NODE_WIDTH + 18}" height="${NODE_HEIGHT + 18}" rx="16"></rect>`;
  const shape = node.shape === 'diamond'
    ? `<path class="ctox-flow-node-diamond" d="M 0 ${-NODE_HEIGHT / 2} L ${NODE_WIDTH / 2} 0 L 0 ${NODE_HEIGHT / 2} L ${-NODE_WIDTH / 2} 0 Z"></path>`
    : `<rect class="ctox-flow-node-box" x="${-NODE_WIDTH / 2}" y="${-NODE_HEIGHT / 2}" width="${NODE_WIDTH}" height="${NODE_HEIGHT}" rx="12"></rect>`;
  return `
    <g class="ctox-flow-node-g is-${escapeAttr(node.status)} ${isVisibleTrace ? 'is-observed is-trace' : 'is-possible'} ${isSelected ? 'is-current is-selected' : ''}"
       data-node-id="${escapeAttr(node.id)}" role="button" style="--trace-strength:${traceStrength}" tabindex="0" transform="translate(${node.x} ${node.y})">
      <title>${escapeHtml(`${node.phase}: ${node.label}\n${metricsLabel(node, 'en')}\n${node.lines.join('\n')}`)}</title>
      ${ring}
      ${shape}
      <text class="ctox-flow-node-phase" x="${-NODE_WIDTH / 2 + 10}" y="${-NODE_HEIGHT / 2 + 16}">${escapeHtml(node.phase)}</text>
      <text class="ctox-flow-node-title" x="${-NODE_WIDTH / 2 + 10}" y="${-NODE_HEIGHT / 2 + 34}">
        ${wrapSvgText(node.label).map((line, index) => `<tspan x="${-NODE_WIDTH / 2 + 10}" dy="${index === 0 ? 0 : 15}">${escapeHtml(line)}</tspan>`).join('')}
      </text>
      <text class="ctox-flow-node-metrics" x="${-NODE_WIDTH / 2 + 10}" y="${NODE_HEIGHT / 2 - 8}">${escapeHtml(metricsLabel(node, 'en'))}</text>
    </g>
  `;
}

function buildHarnessModel(data, flow) {
  const tasks = buildTaskList(data);
  const activeTask = tasks.find((task) => normalizeCommandStatus(task.status) === 'running') || null;
  const activeRun = data.runs.find((run) => run.status === 'running') || null;
  const liveWork = Boolean(activeTask || activeRun);
  const observedIds = observedPathFromFlow(flow);
  const observedIdSet = new Set(observedIds);
  const tracePosition = new Map(observedIds.map((id, index) => [id, index]));
  const activeTraceIndex = Math.max(0, observedIds.length - 1);
  const activeNodeId = liveWork ? (observedIds.at(-1) || 'running') : (observedIds.at(-1) || 'queued');
  const activeIndex = Math.max(0, observedIds.lastIndexOf(activeNodeId));
  const detailByNode = observedDetailsFromFlow(flow);
  const nodes = STATE_MACHINE_NODES.map((node) => {
    const observed = observedIdSet.has(node.id);
    const detail = observed ? detailByNode.get(node.id) : null;
    return {
      ...node,
      status: nodeStatus(node.id, observedIds, activeIndex, liveWork),
      inputTokens: observed ? detail?.inputTokens ?? null : null,
      outputTokens: observed ? detail?.outputTokens ?? null : null,
      toolCalls: observed ? detail?.toolCalls ?? null : null,
      seconds: observed ? detail?.seconds ?? 0 : 0,
      timestamp: observed ? detail?.timestamp || '' : '',
      lines: detail?.lines?.length ? detail.lines : node.lines,
      tools: detail?.tools?.length ? detail.tools : node.tools,
      observed,
      traceStrength: observed ? Math.max(0.52, 1 - (activeTraceIndex - (tracePosition.get(node.id) || 0)) * 0.055) : 0,
    };
  });
  const nodeMap = new Map(nodes.map((node) => [node.id, node]));
  const timeline = observedIds.map((id) => nodeMap.get(id)).filter(Boolean);
  return {
    activeRun,
    activeTask,
    liveWork,
    nodes,
    edges: REVIEW_HARNESS_EDGES,
    nodeMap,
    timeline: timeline.length ? timeline : [nodeMap.get(activeNodeId) || nodes[0]],
    activeNodeId,
    completedRuns: data.runs.filter((run) => run.status === 'completed'),
    tasks,
    inboundChannels: buildInboundChannels(data),
    recentTasks: buildRecentTasks(data),
    queueNow: data.queue.filter((item) => ['queued', 'running', 'leased', 'pending'].includes(item.status) || item.priority === 'urgent'),
    reviewItems: data.communications.filter((item) => item.status === 'review' || item.status === 'drafting'),
    blockedTickets: data.tickets.filter((ticket) => ticket.status === 'blocked' || ticket.status === 'review' || ticket.status === 'running'),
    openTickets: data.tickets.filter((ticket) => ticket.status !== 'done'),
  };
}

function buildRecentTasks(data) {
  const runTasks = data.runs.map((run) => ({ id: `run-${run.id}`, title: run.title, status: run.status, source: `${run.moduleId}/${run.submoduleId}`, summary: run.summary, timestamp: run.startedAt }));
  const queueTasks = data.queue.map((item) => ({ id: `queue-${item.id}`, taskId: item.id, commandId: item.commandId || '', title: item.title, status: item.status, source: item.source, summary: item.target, timestamp: item.createdAt }));
  return [...runTasks, ...queueTasks].sort((left, right) => Date.parse(right.timestamp) - Date.parse(left.timestamp)).slice(0, 8);
}

function buildTaskList(data) {
  const runTasks = data.runs.map((run) => ({
    id: `run-${run.id}`,
    runId: run.id,
    title: run.title,
    status: normalizeCommandStatus(run.status),
    source: `${run.moduleId || 'ctox'}/${run.submoduleId || 'run'}`,
    channel: inferInboundChannel(run),
    channelLabel: inboundChannelLabel(inferInboundChannel(run)),
    summary: run.summary || '',
    model: run.model || '',
    startedAt: run.startedAt,
    createdAt: run.startedAt,
    timestamp: run.startedAt,
    resultSummary: run.summary || '',
  }));
  const queueTasks = data.queue.map((item) => ({
      ...item,
      taskId: item.id,
      status: normalizeCommandStatus(item.status),
      channel: item.channel || inferInboundChannel(item),
      channelLabel: inboundChannelLabel(item.channel || inferInboundChannel(item)),
      timestamp: item.createdAt,
      resultSummary: item.resultSummary || resultSummary(item.result),
    }));
  const ticketTasks = data.tickets.map((ticket) => ({
    ...ticket,
    id: `ticket-${ticket.id}`,
    ticketId: ticket.id,
    title: ticket.title || ticket.summary || ticket.id || 'CTOX ticket',
    status: normalizeCommandStatus(ticket.status || ticket.severity || 'open'),
    source: ticket.source || ticket.module || ticket.surface || 'ctox',
    channel: ticket.channel || inferInboundChannel(ticket),
    channelLabel: inboundChannelLabel(ticket.channel || inferInboundChannel(ticket)),
    target: ticket.surface || ticket.severity || 'ticket',
    timestamp: ticket.createdAt || ticket.updatedAt,
    resultSummary: ticket.description || ticket.summary || '',
  }));
  return [...queueTasks, ...runTasks, ...ticketTasks]
    .sort((left, right) => Date.parse(right.timestamp || right.createdAt || 0) - Date.parse(left.timestamp || left.createdAt || 0));
}

function buildInboundChannels(data) {
  const channels = new Map();
  for (const item of data.queue || []) addInboundChannel(channels, item);
  for (const run of data.runs || []) addInboundChannel(channels, run);
  for (const ticket of data.tickets || []) addInboundChannel(channels, ticket);
  return Array.from(channels.values())
    .sort((left, right) => right.active - left.active || right.count - left.count || left.label.localeCompare(right.label));
}

function addInboundChannel(channels, item) {
  const key = inferInboundChannel(item);
  const label = inboundChannelLabel(key);
  const status = normalizeCommandStatus(item.status || item.task_status || item.routeStatus || '');
  const active = ['running', 'leased', 'review', 'drafting', 'queued', 'pending'].includes(status);
  const entry = channels.get(key) || { id: key, label, count: 0, active: false };
  entry.count += 1;
  entry.active = entry.active || active;
  channels.set(key, entry);
}

function taskGroups(tasks) {
  const groups = { current: [], blocked: [], waiting: [], done: [] };
  const currentCandidates = [];
  for (const task of tasks) {
    const status = normalizeCommandStatus(task.status);
    if (['completed', 'done', 'sent', 'approved'].includes(status)) {
      groups.done.push(task);
    } else if (['blocked', 'failed', 'cancelled', 'handled'].includes(status)) {
      groups.blocked.push(task);
    } else if (['running', 'leased', 'review', 'drafting'].includes(status)) {
      currentCandidates.push(task);
    } else {
      groups.waiting.push(task);
    }
  }
  const current = currentCandidates[0] || null;
  if (current) groups.current.push(current);
  for (const task of currentCandidates.slice(1)) {
    groups.waiting.unshift({ ...task, status: 'queued' });
  }
  return groups;
}

function resolveSelectedTaskId(model, focusTask, previousId) {
  if (!model?.tasks?.length) return null;
  if (previousId && model.tasks.some((task) => task.id === previousId)) return previousId;
  const focused = model.tasks.find((task) => isFocusedTask(task, focusTask));
  if (focused) return focused.id;
  const groups = taskGroups(model.tasks);
  return (groups.current[0] || groups.waiting[0] || groups.blocked[0] || groups.done[0] || model.tasks[0]).id;
}

function reconcileSelection(state) {
  const previousTaskId = state.selectedTaskId;
  const previousStepIndex = state.selectedStepIndex;
  state.selectedTaskId = resolveSelectedTaskId(state.model, state.focusTask, state.selectedTaskId);
  const selectedTaskChanged = previousTaskId !== state.selectedTaskId;
  if (state.userNavigatedTimeline && !selectedTaskChanged && Number.isFinite(previousStepIndex)) {
    state.selectedStepIndex = clampIndex(previousStepIndex, state.model?.timeline?.length || 1);
    return;
  }
  state.selectedStepIndex = timelineIndexForSelectedTask(state) ?? focusedTimelineIndex(state.model, state.focusTask);
}

function getSelectedTask(state) {
  return state.model?.tasks?.find((task) => task.id === state.selectedTaskId) || null;
}

function timelineIndexForSelectedTask(state) {
  const task = getSelectedTask(state);
  if (!task) return null;
  const steps = taskSteps(task, state);
  const current = steps.find((step) => step.active) || steps.at(-1);
  return current ? current.timelineIndex : null;
}

function selectTask(state, taskId, options = {}) {
  if (!taskId) return;
  state.selectedTaskId = taskId;
  state.userNavigatedTimeline = false;
  const task = getSelectedTask(state);
  const groupKey = groupKeyForTask(task);
  if (groupKey) state.openTaskSections.add(groupKey);
  const nextIndex = timelineIndexForSelectedTask(state);
  if (nextIndex !== null) state.selectedStepIndex = nextIndex;
  if (options.drawer) state.detailDrawer = { type: 'task', taskId };
  render(state);
  if (options.center !== false) centerSelectedNode(state);
  syncDetailDrawer(state);
}

function setTimelineStep(state, nextIndex, options = {}) {
  state.selectedStepIndex = clampIndex(nextIndex, state.model?.timeline?.length || 1);
  state.userNavigatedTimeline = true;
  render(state);
  if (options.center) centerSelectedNode(state);
  syncDetailDrawer(state);
}

function syncDetailDrawer(state) {
  if (!state.detailDrawer) return;
  if (state.detailDrawer.type === 'task') {
    const task = state.model?.tasks?.find((item) => item.id === state.detailDrawer.taskId) || getSelectedTask(state);
    if (task) state.ctx.openLeftDrawer(taskDrawer(task, state));
    return;
  }
  if (state.detailDrawer.type === 'node') {
    const node = state.model?.timeline?.[clampIndex(state.selectedStepIndex, state.model.timeline.length)]
      || state.model?.nodeMap?.get(state.detailDrawer.nodeId);
    if (node) state.ctx.openLeftDrawer(flowNodeDrawer(node, getSelectedTask(state), state));
  }
}

function closeDetailDrawer(state) {
  state.detailDrawer = null;
  state.ctx.closeDrawers();
}

function taskDrawer(task, state) {
  const t = labels[state.lang];
  const steps = taskSteps(task, state);
  const selectedTimelineIndex = clampIndex(state.selectedStepIndex, state.model?.timeline?.length || 1);
  const summary = cleanUiCopy(itemSummary(task) || '');
  const target = displayPathLike(task.target || task.commandId || task.taskId || '');
  const sourceLine = [
    displayWorkSource(task.source || task.moduleId || 'ctox'),
    formatShortTimestamp(task.createdAt || task.startedAt || task.timestamp),
  ].filter(Boolean).join(' · ');
  const showSummary = summary && summary !== task.target && summary !== task.commandId && summary !== task.taskId;
  const body = document.createElement('div');
  body.className = 'drawer-body ctox-task-drawer';
  body.innerHTML = `
    <header class="ctox-detail-header">
      <div>
        <span>${escapeHtml(t.taskDetail)}</span>
        <h2>${escapeHtml(task.title)}</h2>
        <small>${escapeHtml(sourceLine)}</small>
      </div>
      <button class="icon-button ctox-drawer-close" type="button" data-close-ctox-drawer aria-label="Schließen">×</button>
    </header>
    <section class="ctox-task-status-strip">
      <div>
        <strong class="${escapeAttr(statusClass(task.status))}">${escapeHtml(displayStatus(task.status, state.lang))}</strong>
        ${target ? `<small>${escapeHtml(target)}</small>` : ''}
      </div>
      ${taskLiveStatusMarkup(task, state)}
    </section>
    ${showSummary ? `
      <section class="ctox-detail-summary">
        <p>${escapeHtml(summary)}</p>
      </section>
    ` : ''}
    ${task.resultSummary ? `
      <section class="ctox-detail-summary">
        <span>${escapeHtml(t.evidence)}</span>
        <p>${escapeHtml(task.resultSummary)}</p>
      </section>
    ` : ''}
    <section class="ctox-drawer-section ctox-drawer-timeline">
      <header>
        <h3>${escapeHtml(t.timeline)}</h3>
        <small>${escapeHtml(`${steps.length} ${t.taskSteps}`)}</small>
      </header>
      <div class="ctox-drawer-steps">
        ${steps.map((step, index) => `
          <button type="button" class="${step.timelineIndex === selectedTimelineIndex ? 'is-current' : ''}" data-drawer-step="${step.timelineIndex}">
            <span>${String(index + 1).padStart(2, '0')}</span>
            <strong>${escapeHtml(step.label)}</strong>
            <small>${escapeHtml(stepMetaLabel(step, state))}</small>
            <em>${escapeHtml(step.detail || t.noRecentWork)}</em>
          </button>
        `).join('')}
      </div>
    </section>
  `;
  body.querySelector('[data-close-ctox-drawer]')?.addEventListener('click', () => closeDetailDrawer(state));
  body.querySelectorAll('[data-drawer-step]').forEach((button) => {
    button.addEventListener('click', () => {
      setTimelineStep(state, Number(button.dataset.drawerStep), { center: true });
    });
  });
  return body;
}

function flowNodeDrawer(node, task, state) {
  const t = labels[state.lang];
  const body = document.createElement('div');
  body.className = 'drawer-body ctox-task-drawer ctox-node-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <span>${escapeHtml(t.stationDetail)}</span>
        <h2>${escapeHtml(node.label)}</h2>
      </div>
      <button class="icon-button ctox-drawer-close" type="button" data-close-ctox-drawer aria-label="Schließen">×</button>
    </header>
    <dl class="ctox-task-facts">
      ${nodeLiveFactMarkup(node, task, state)}
      <div><dt>${escapeHtml(t.currentStep)}</dt><dd>${escapeHtml(node.phase || '')}</dd></div>
      <div><dt>${escapeHtml(t.status)}</dt><dd>${escapeHtml(displayStatus(node.status, state.lang))}</dd></div>
      <div><dt>${escapeHtml(t.taskDetail)}</dt><dd>${escapeHtml(task?.title || t.noRecentWork)}</dd></div>
      <div><dt>${escapeHtml(t.measurements)}</dt><dd>${escapeHtml(metricsLabel(node, state.lang))}</dd></div>
    </dl>
    <section class="ctox-drawer-section">
      <h3>${escapeHtml(t.summary)}</h3>
      ${(node.lines || []).map((line) => `<p>${escapeHtml(line)}</p>`).join('') || `<p>${escapeHtml(t.noRecentWork)}</p>`}
    </section>
    ${node.tools?.length ? `
      <section class="ctox-drawer-section">
        <h3>${escapeHtml(t.tools)}</h3>
        <div class="ctox-node-tools">
          ${node.tools.map((tool) => `<span>${escapeHtml(tool)}</span>`).join('')}
        </div>
      </section>
    ` : ''}
  `;
  body.querySelector('[data-close-ctox-drawer]')?.addEventListener('click', () => closeDetailDrawer(state));
  return body;
}

function buildVisibleTrace(timeline, timelineIndex) {
  const window = timeline.slice(Math.max(0, timelineIndex - 4), timelineIndex + 1);
  return buildVisibleTraceWindow(window);
}

function buildVisibleTraceFromSteps(model, steps, stepIndex) {
  const window = steps
    .slice(Math.max(0, stepIndex - 4), stepIndex + 1)
    .map((step) => model.nodeMap.get(step.id))
    .filter(Boolean);
  return buildVisibleTraceWindow(window);
}

function buildVisibleTraceWindow(window) {
  const nodeStrength = new Map();
  const edgeStrength = new Map();
  window.forEach((node, index) => {
    const strength = window.length <= 1 ? 1 : 0.28 + (index / (window.length - 1)) * 0.72;
    nodeStrength.set(node.id, Math.max(nodeStrength.get(node.id) || 0, strength));
    const previous = window[index - 1];
    if (previous) edgeStrength.set(edgeKey(previous.id, node.id), strength);
  });
  return { edgeStrength, nodeStrength };
}

function selectedTaskStepView(task, state) {
  if (!task) return null;
  const steps = taskSteps(task, state);
  if (!steps.length) return null;
  const selectedTimelineIndex = clampIndex(state.selectedStepIndex, state.model.timeline.length);
  const byTimeline = steps.findIndex((step) => step.timelineIndex === selectedTimelineIndex);
  const activeIndex = steps.findIndex((step) => step.active);
  const index = state.userNavigatedTimeline && byTimeline >= 0 ? byTimeline : Math.max(0, activeIndex);
  const step = steps[index] || steps[0];
  return { steps, index, step, node: state.model.nodeMap.get(step.id) || null };
}

function nodeStatus(id, observedIds, activeIndex, liveWork) {
  const index = observedIds.lastIndexOf(id);
  if (index === -1) return 'waiting';
  if (index < activeIndex) return 'done';
  if (index === activeIndex) return liveWork ? 'active' : 'done';
  return 'waiting';
}

function observedPathFromFlow(flowResult) {
  if (flowResult?.ok === false) return [];
  const flow = flowResult?.flow || fallbackHarnessFlow().flow;
  const ids = [];
  const seen = new Set();
  const push = (id) => {
    if (!id || seen.has(id)) return;
    seen.add(id);
    ids.push(id);
  };
  let reviewPassed = false;
  for (const block of flow.blocks || []) {
    if (block.kind === 'task') push('queued');
    if (block.kind === 'attempt') {
      if (blockHasExplicitRuntimeEvidence(block)) {
        push('leased');
        push('running');
      }
    }
    for (const branch of block.branches || []) {
      const reviewOutcome = reviewBranchOutcome(branch);
      if (branch.kind === 'queue_pickup') push(queuePickupNode(branch));
      if (branch.kind === 'review') {
        if (reviewOutcome === 'passed' || reviewOutcome === 'rejected') {
          push('awaiting-review');
          push('review-queued');
          push('reviewing');
        }
        if (reviewOutcome === 'passed') {
          push('review-passed');
          reviewPassed = true;
        }
        if (reviewOutcome === 'rejected') {
          push('review-rejected');
          push('rework-required');
        }
      }
      if (branch.kind === 'verification' && reviewPassed && branchHasValidationEvidence(branch)) {
        push('awaiting-validation');
        push('validating');
        push('passed');
      }
    }
  }
  for (const event of flow.ledger_events || []) {
    push(eventToNodeId(event.event_kind || '', event.title || ''));
  }
  if (ids.length === 0) push('queued');
  return ids.filter((id) => REVIEW_HARNESS_NODE_SET.has(id));
}

function observedDetailsFromFlow(flowResult) {
  const flow = flowResult?.flow || fallbackHarnessFlow().flow;
  const map = new Map();
  const add = (id, lines, tools, rawSources = []) => {
    const metrics = firstExplicitMetrics(rawSources);
    const timestamp = firstTimestamp(rawSources);
    map.set(id, {
      inputTokens: metrics?.inputTokens ?? null,
      outputTokens: metrics?.outputTokens ?? null,
      toolCalls: metrics?.toolCalls ?? null,
      seconds: metrics?.seconds ?? 0,
      timestamp,
      lines: (lines || []).map(cleanUiCopy),
      tools: (tools || []).map(cleanUiCopy),
    });
  };
  for (const block of flow.blocks || []) {
    const tools = (block.branches || []).map((branch) => `${branch.kind}: ${branch.title}`);
    if (block.kind === 'task') add('queued', block.lines, tools, [block]);
    if (block.kind === 'attempt' && blockHasExplicitRuntimeEvidence(block)) add('running', block.lines, tools, [block]);
    for (const branch of block.branches || []) {
      const id = branchToNodeId(branch.kind, branch.title || '', branch.lines || []);
      if (id) add(id, branch.lines, [`${branch.kind}: ${branch.title}`], [branch, block]);
    }
  }
  for (const event of flow.ledger_events || []) {
    const id = eventToNodeId(event.event_kind || '', event.title || '');
    if (!id) continue;
    const existing = map.get(id);
    const metrics = firstExplicitMetrics([event, parseMetadata(event.metadata_json)]);
    map.set(id, {
      inputTokens: metrics?.inputTokens ?? existing?.inputTokens ?? null,
      outputTokens: metrics?.outputTokens ?? existing?.outputTokens ?? null,
      toolCalls: metrics?.toolCalls ?? existing?.toolCalls ?? null,
      seconds: metrics?.seconds ?? existing?.seconds ?? 0,
      timestamp: event.created_at || firstTimestamp([event, parseMetadata(event.metadata_json)]) || existing?.timestamp || '',
      lines: existing?.lines?.length ? existing.lines : [cleanUiCopy(event.title), cleanUiCopy(event.body_text)].filter(Boolean),
      tools: existing?.tools || [],
    });
  }
  return map;
}

function firstExplicitMetrics(rawSources) {
  for (const source of rawSources) {
    const metrics = explicitMetrics(source);
    if (metrics) return metrics;
  }
  return null;
}

function firstTimestamp(rawSources) {
  for (const source of rawSources) {
    if (!source || typeof source !== 'object') continue;
    const nested = [source, source.metrics, source.runtime, source.stats].filter(Boolean);
    for (const values of nested) {
      const timestamp = readString(values, ['created_at', 'createdAt', 'observed_at', 'observedAt', 'started_at', 'startedAt', 'finished_at', 'finishedAt', 'updated_at', 'updatedAt']);
      if (timestamp) return timestamp;
    }
  }
  return '';
}

function explicitMetrics(source) {
  if (!source || typeof source !== 'object') return null;
  const nested = [source, source.metrics, source.usage, source.token_usage, source.tokenUsage, source.runtime, source.stats].filter(Boolean);
  let inputTokens = null;
  let outputTokens = null;
  let toolCalls = null;
  let durationSeconds = null;
  let elapsedFromTimestamps = null;
  for (const values of nested) {
    if (!values || typeof values !== 'object') continue;
    inputTokens ??= readNumber(values, ['input_tokens', 'inputTokens', 'prompt_tokens', 'promptTokens', 'tokens_in', 'tokensIn']);
    outputTokens ??= readNumber(values, ['output_tokens', 'outputTokens', 'completion_tokens', 'completionTokens', 'tokens_out', 'tokensOut']);
    toolCalls ??= readNumber(values, ['tool_calls', 'toolCalls', 'tool_call_count', 'toolCallCount']);
    durationSeconds ??= readNumber(values, ['duration_seconds', 'durationSeconds', 'elapsed_seconds', 'elapsedSeconds', 'seconds']) ?? millisToSeconds(readNumber(values, ['duration_ms', 'durationMs', 'elapsed_ms', 'elapsedMs']));
    elapsedFromTimestamps ??= elapsedSeconds(readString(values, ['started_at', 'startedAt']), readString(values, ['finished_at', 'finishedAt']));
  }
  if (inputTokens === null && outputTokens === null && toolCalls === null && durationSeconds === null && elapsedFromTimestamps === null) return null;
  return {
    inputTokens: inputTokens === null ? null : Math.max(0, Math.round(inputTokens)),
    outputTokens: outputTokens === null ? null : Math.max(0, Math.round(outputTokens)),
    toolCalls: toolCalls === null ? null : Math.max(0, Math.round(toolCalls)),
    seconds: durationSeconds === null && elapsedFromTimestamps === null ? null : Math.max(0, Math.round(durationSeconds ?? elapsedFromTimestamps ?? 0)),
  };
}

function edgePath(from, to, route = 'normal') {
  const horizontal = Math.abs(to.x - from.x) >= Math.abs(to.y - from.y);
  const fromHalfW = (from.shape === 'diamond' ? NODE_WIDTH * 0.58 : NODE_WIDTH) / 2;
  const toHalfW = (to.shape === 'diamond' ? NODE_WIDTH * 0.58 : NODE_WIDTH) / 2;
  const fromHalfH = (from.shape === 'diamond' ? NODE_HEIGHT * 0.58 : NODE_HEIGHT) / 2;
  const toHalfH = (to.shape === 'diamond' ? NODE_HEIGHT * 0.58 : NODE_HEIGHT) / 2;
  let x1 = from.x;
  let y1 = from.y;
  let x2 = to.x;
  let y2 = to.y;
  if (horizontal) {
    x1 += to.x >= from.x ? fromHalfW : -fromHalfW;
    x2 -= to.x >= from.x ? toHalfW : -toHalfW;
  } else {
    y1 += to.y >= from.y ? fromHalfH : -fromHalfH;
    y2 -= to.y >= from.y ? toHalfH : -toHalfH;
  }
  if (route === 'loop') {
    const offset = to.y >= from.y ? 88 : -88;
    const midY = Math.max(36, Math.min(FLOW_HEIGHT - 36, Math.max(from.y, to.y) + offset));
    return `M ${x1} ${y1} L ${x1} ${midY} L ${x2} ${midY} L ${x2} ${y2}`;
  }
  if (route === 'up' || route === 'down') {
    const offset = route === 'up' ? -54 : 54;
    const midY = Math.max(36, Math.min(FLOW_HEIGHT - 36, (from.y + to.y) / 2 + offset));
    return `M ${x1} ${y1} L ${x1} ${midY} L ${x2} ${midY} L ${x2} ${y2}`;
  }
  if (Math.abs(x2 - x1) < 1 || Math.abs(y2 - y1) < 1) return `M ${x1} ${y1} L ${x2} ${y2}`;
  const midX = (x1 + x2) / 2;
  return `M ${x1} ${y1} L ${midX} ${y1} L ${midX} ${y2} L ${x2} ${y2}`;
}

function mergeBundleWithCommands(bundle, commands, queueTasks = [], bugReports = []) {
  const runtimeQueue = queueTasks.map((doc) => ({
    id: doc.id || doc.task_id || doc.command_id,
    commandId: doc.command_id || '',
    title: doc.title || doc.command_type || doc.id || 'CTOX queue task',
    source: doc.source_module || doc.module || 'ctox',
    channel: inferInboundChannel(doc),
    priority: doc.priority || 'normal',
    status: normalizeCommandStatus(doc.status || doc.route_status),
    routeStatus: doc.route_status || '',
    target: doc.command_type || doc.thread_key || 'ctox queue',
    result: doc.result || null,
    resultSummary: resultSummary(doc.result),
    createdAt: new Date(doc.updated_at_ms || Date.now()).toISOString(),
  })).filter((item) => item.id);
  const commandQueue = commands.map((doc) => ({
    id: doc.task_id || doc.command_id || doc.id,
    commandId: doc.command_id || doc.id,
    title: displayCommandTitle(doc),
    source: doc.module || 'business-os',
    channel: inferInboundChannel(doc),
    priority: doc.command_type?.includes('runtime') ? 'high' : 'normal',
    status: normalizeCommandStatus(doc.task_status || doc.status),
    target: doc.command_type || 'ctox command',
    result: doc.result || null,
    resultSummary: resultSummary(doc.result),
    createdAt: new Date(doc.updated_at_ms || Date.now()).toISOString(),
  }));
  const tickets = bugReports.map((doc) => ({
    id: doc.id || doc.report_id,
    title: doc.title || doc.surface || doc.id || 'CTOX ticket',
    status: normalizeCommandStatus(doc.status || doc.severity || 'open'),
    severity: doc.severity || '',
    module: doc.module || doc.module_id || 'ctox',
    surface: doc.surface || '',
    source: doc.module || doc.module_id || doc.surface || 'ctox',
    channel: inferInboundChannel(doc),
    description: doc.description || doc.summary || '',
    evidence: doc.evidence || null,
    createdAt: new Date(doc.created_at_ms || doc.updated_at_ms || Date.now()).toISOString(),
    updatedAt: new Date(doc.updated_at_ms || doc.created_at_ms || Date.now()).toISOString(),
  })).filter((item) => item.id);
  return {
    ...bundle,
    queue: mergeById(commandQueue, mergeById(runtimeQueue, bundle.queue)),
    tickets: mergeById(tickets, bundle.tickets),
  };
}

function inferInboundChannel(item = {}) {
  const payload = item.payload && typeof item.payload === 'object' ? item.payload : {};
  const clientContext = item.client_context && typeof item.client_context === 'object' ? item.client_context : {};
  const candidates = [
    item.inbound_channel,
    item.channel,
    item.channel_id,
    item.source_channel,
    item.source_kind,
    item.source_module,
    item.module,
    item.moduleId,
    payload.inbound_channel,
    payload.channel,
    payload.source_channel,
    payload.sourceModule,
    payload.module,
    clientContext.inbound_channel,
    clientContext.channel,
    clientContext.source_channel,
    clientContext.sourceModule,
    clientContext.module,
    item.source,
  ];
  const value = candidates.find((candidate) => String(candidate || '').trim());
  return normalizeInboundChannel(value || 'business-os');
}

function normalizeInboundChannel(value) {
  const raw = String(value || 'business-os').trim().toLowerCase().replace(/\s+/g, '-');
  if (raw.includes('llm') && raw.includes('chat')) return 'business_os.llm.chat';
  if (raw.includes('requirement') || raw.includes('matching')) return 'requirement-matching';
  if (raw.includes('document')) return 'documents';
  if (raw.includes('knowledge')) return 'knowledge';
  if (raw.includes('ctox')) return 'ctox';
  if (raw.includes('business')) return 'business-os';
  return raw || 'business-os';
}

function inboundChannelLabel(channel) {
  const normalized = normalizeInboundChannel(channel);
  const labelsById = {
    'business_os.llm.chat': 'LLM Chat',
    'business-os': 'Business OS',
    ctox: 'CTOX',
    documents: 'Documents',
    knowledge: 'Knowledge',
    'requirement-matching': 'Requirement Matching',
  };
  return labelsById[normalized] || displayWorkSource(normalized);
}

function readFocusTask() {
  const focusFromHash = readFocusTaskFromHash();
  if (focusFromHash) return focusFromHash;
  try {
    const parsed = JSON.parse(sessionStorage.getItem('ctox.businessOs.focusTask') || 'null');
    if (parsed && (parsed.taskId || parsed.commandId)) return parsed;
  } catch {}
  return null;
}

function readFocusTaskFromHash() {
  const query = String(location.hash || '').split('?')[1] || '';
  if (!query) return null;
  const params = new URLSearchParams(query);
  const taskId = params.get('task_id') || params.get('taskId') || '';
  const commandId = params.get('command_id') || params.get('commandId') || '';
  if (!taskId && !commandId) return null;
  return {
    taskId,
    commandId,
    taskStatus: params.get('task_status') || params.get('status') || '',
    sourceModule: params.get('source') || 'matching',
  };
}

function focusedTimelineIndex(model, focusTask) {
  if (!model?.timeline?.length) return 0;
  if (!focusTask) return clampIndex(model.timeline.length - 1, model.timeline.length);
  const focused = model.queueNow.find((item) => isFocusedTask(item, focusTask))
    || model.recentTasks.find((item) => item.id === `queue-${focusTask.taskId}` || item.id === `queue-${focusTask.commandId}`);
  const status = normalizeCommandStatus(focused?.status || focusTask.taskStatus || 'queued');
  const targetNode = status === 'running' ? 'running' : status === 'completed' ? 'passed' : status === 'failed' ? 'model-failed' : 'queued';
  const index = model.timeline.findIndex((node) => node.id === targetNode);
  return index >= 0 ? index : clampIndex(model.timeline.length - 1, model.timeline.length);
}

function isFocusedTask(item, focusTask) {
  if (!item || !focusTask) return false;
  return Boolean(
    (focusTask.taskId && item.id === focusTask.taskId) ||
    (focusTask.taskId && item.taskId === focusTask.taskId) ||
    (focusTask.commandId && (item.id === focusTask.commandId || item.commandId === focusTask.commandId))
  );
}

function normalizeCommandStatus(status) {
  const value = String(status || '').toLowerCase();
  if (value === 'accepted' || value === 'pending' || value === 'queued_local') return 'queued';
  if (value === 'leased' || value === 'working') return 'running';
  if (value === 'done') return 'completed';
  if (value === 'handled') return 'handled';
  if (value === 'cancelled') return 'cancelled';
  if (value === 'blocked') return 'blocked';
  if (value === 'failed') return 'failed';
  return value || 'queued';
}

async function loadLocalCommands(ctx) {
  return loadLocalCollection(ctx, 'business_commands');
}

async function loadLocalQueueTasks(ctx) {
  return loadLocalCollection(ctx, 'ctox_queue_tasks');
}

async function loadLocalBugReports(ctx) {
  return loadLocalCollection(ctx, 'ctox_bug_reports');
}

async function loadLocalCollection(ctx, collectionName) {
  const collection = ctx.db?.raw?.[collectionName];
  if (!collection) return [];
  const localDocs = await collection.find().exec();
  return localDocs
    .map((doc) => doc.toJSON())
    .sort((left, right) => (right.updated_at_ms || 0) - (left.updated_at_ms || 0))
    .slice(0, 20);
}

async function loadHarnessFlow() {
  try {
    const res = await fetchWithTimeout('/api/business-os/ctox/harness-flow');
    if (!res.ok) throw new Error(`harness_flow_${res.status}`);
    const payload = await res.json();
    if (payload?.flow?.blocks?.length) return payload;
    return fallbackHarnessFlow(payload?.error);
  } catch (error) {
    return fallbackHarnessFlow(error?.message || String(error));
  }
}

async function fetchWithTimeout(url, options = {}) {
  const controller = new AbortController();
  const timer = window.setTimeout(() => controller.abort(), CTOX_FETCH_TIMEOUT_MS);
  try {
    return await fetch(url, {
      cache: 'no-store',
      ...options,
      signal: controller.signal,
    });
  } finally {
    window.clearTimeout(timer);
  }
}

function loadCachedHarnessFlow() {
  try {
    const cached = JSON.parse(localStorage.getItem(HARNESS_FLOW_CACHE_KEY) || 'null');
    return cached?.flow ? cached : null;
  } catch {
    return null;
  }
}

function saveCachedHarnessFlow(flow) {
  try {
    if (flow?.flow?.blocks?.length) {
      localStorage.setItem(HARNESS_FLOW_CACHE_KEY, JSON.stringify(flow));
    }
  } catch {}
}

async function loadStatus() {
  const res = await fetchWithTimeout('/api/business-os/status');
  if (!res.ok) throw new Error(`status_${res.status}`);
  return res.json();
}

function fallbackHarnessFlow(error = '') {
  return {
    ok: false,
    mode: 'fallback',
    error,
    ascii: '',
    flow: {
      schema_version: 1,
      source: { message_key: null, work_id: null, source_kind: 'fallback' },
      ledger_events: [],
      blocks: [],
    },
  };
}

function wireContextMenu(state) {
  state.ctx.host.querySelector('[data-ctox-harness]')?.addEventListener('contextmenu', (event) => {
    event.preventDefault();
    const item = event.target.closest('.ctox-context-item,[data-node-id]') || event.currentTarget;
    const label = item.dataset.contextLabel || item.dataset.nodeId || 'CTOX Harness';
    const recordId = item.dataset.contextRecordId || item.dataset.nodeId || 'ctox-harness';
    state.ctx.openRightDrawer(contextDrawer(state, { label, recordId }));
  }, { once: true });
}

function contextDrawer(state, context) {
  const t = labels[state.lang];
  const body = document.createElement('div');
  body.className = 'drawer-body ctox-context-drawer';
  body.innerHTML = `
    <h2>${escapeHtml(context.label)}</h2>
    <button type="button" data-modify-app>${escapeHtml(t.modifyApp)}</button>
    <button type="button" data-inspect-context>${escapeHtml(t.inspectContext)}</button>
    <button type="button" data-refresh-harness>${escapeHtml(t.refreshHarness)}</button>
    <small>module=ctox · record=${escapeHtml(context.recordId)}</small>
  `;
  body.querySelector('[data-modify-app]')?.addEventListener('click', async () => {
    await state.ctx.commandBus.dispatch({
      module: 'ctox',
      type: 'business_os.app.modify',
      payload: { instruction: `Modify the CTOX Business OS app around context ${context.label}.`, context },
      client_context: { module: 'ctox', surface: 'context-menu', record_id: context.recordId },
    });
    state.ctx.closeDrawers();
  });
  body.querySelector('[data-inspect-context]')?.addEventListener('click', () => {
    body.querySelector('small').textContent = `module=ctox · record=${context.recordId} · sync=${state.ctx.syncConfig?.sync_room || 'unknown'}`;
  });
  body.querySelector('[data-refresh-harness]')?.addEventListener('click', async () => {
    state.ctx.closeDrawers();
    await refresh(state);
  });
  return body;
}

function wireShellMessages(state) {
  const applyLanguage = (lang) => {
    const nextLang = lang === 'en' ? 'en' : 'de';
    loadCtoxMessages(nextLang).then(() => {
      state.lang = nextLang;
      render(state);
    }).catch((error) => {
      console.warn('[ctox] language switch failed', error);
    });
  };
  const messageHandler = (event) => {
    if (event.data?.type === 'ctox-business-os-language') applyLanguage(event.data.lang);
    if (event.data?.type === 'ctox-business-os-preferences') applyLanguage(event.data.language);
  };
  const preferenceHandler = (event) => {
    applyLanguage(event.detail?.language);
  };
  window.addEventListener('message', messageHandler);
  window.addEventListener('ctox-business-os-preferences', preferenceHandler);
  return () => {
    window.removeEventListener('message', messageHandler);
    window.removeEventListener('ctox-business-os-preferences', preferenceHandler);
  };
}

function wireCanvasDrag(scroller) {
  if (!scroller) return;
  let drag = null;
  const rememberViewport = () => {
    const state = scroller.closest('[data-ctox-harness]')?.__ctoxState;
    if (state) state.flowViewport = { left: scroller.scrollLeft, top: scroller.scrollTop };
  };
  scroller.addEventListener('pointerdown', (event) => {
    if (event.target.closest('[data-node-id],[data-flow-control]')) return;
    drag = { x: event.clientX, y: event.clientY, left: scroller.scrollLeft, top: scroller.scrollTop };
    scroller.setPointerCapture(event.pointerId);
  });
  scroller.addEventListener('pointermove', (event) => {
    if (!drag) return;
    scroller.scrollLeft = drag.left - (event.clientX - drag.x);
    scroller.scrollTop = drag.top - (event.clientY - drag.y);
    rememberViewport();
  });
  scroller.addEventListener('pointerup', () => { rememberViewport(); drag = null; });
  scroller.addEventListener('pointercancel', () => { rememberViewport(); drag = null; });
  scroller.addEventListener('scroll', rememberViewport, { passive: true });
  scroller.addEventListener('wheel', (event) => {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
  }, { passive: false });
}

function readFlowViewport(state) {
  const scroller = state.ctx.host.querySelector('[data-flow-canvas]');
  if (!scroller) return state.flowViewport || { left: 0, top: 0 };
  const viewport = { left: scroller.scrollLeft, top: scroller.scrollTop };
  state.flowViewport = viewport;
  return viewport;
}

function restoreFlowViewport(state, viewport) {
  const scroller = state.ctx.host.querySelector('[data-flow-canvas]');
  if (!scroller || !viewport) return;
  requestAnimationFrame(() => {
    const left = Math.max(0, Math.min(viewport.left || 0, scroller.scrollWidth - scroller.clientWidth));
    const top = Math.max(0, Math.min(viewport.top || 0, scroller.scrollHeight - scroller.clientHeight));
    scroller.scrollLeft = left;
    scroller.scrollTop = top;
    state.flowViewport = { left, top };
  });
}

function centerSelectedNode(state) {
  const node = selectedTaskStepView(getSelectedTask(state), state)?.node
    || state.model.timeline[clampIndex(state.selectedStepIndex, state.model.timeline.length)];
  const scroller = state.ctx.host.querySelector('[data-flow-canvas]');
  if (!node || !scroller) return;
  requestAnimationFrame(() => {
    const left = Math.max(0, node.x * state.zoom - scroller.clientWidth / 2);
    const top = Math.max(0, node.y * state.zoom - scroller.clientHeight / 2);
    state.flowViewport = { left, top };
    scroller.scrollTo({
      left,
      top,
      behavior: 'smooth',
    });
  });
}

function edgeKey(from, to) {
  return `${from}->${to}`;
}

function findLastTimelineIndex(timeline, nodeId) {
  const index = timeline.map((node) => node.id).lastIndexOf(nodeId);
  return index === -1 ? Math.max(0, timeline.length - 1) : index;
}

function metricsLabel(node, lang) {
  if (node.inputTokens === null || node.outputTokens === null) return labels[lang]?.noMetrics || labels.en.noMetrics;
  const toolLabel = node.toolCalls === null || node.toolCalls === undefined ? labels[lang]?.notCaptured || labels.en.notCaptured : `${node.toolCalls} tools`;
  return `${formatTokenCount(node.inputTokens)}/${formatTokenCount(node.outputTokens)} tokens (${toolLabel}, ${node.seconds}s)`;
}

function stepMetaLabel(step, state) {
  const t = labels[state.lang] || labels.de;
  const timestamp = formatShortTimestamp(step?.timestamp);
  return timestamp || t.notLogged;
}

function startLiveTicker(state) {
  window.clearInterval(state.liveTicker);
  updateLiveIndicators(state);
  state.liveTicker = window.setInterval(() => updateLiveIndicators(state), 1000);
}

function updateLiveIndicators(state) {
  const display = formatMetricValue(liveElapsedSeconds(state), 'seconds', state.lang);
  document.querySelectorAll('[data-module-root="ctox"] [data-live-elapsed], .ctox-task-drawer [data-live-elapsed]').forEach((node) => {
    node.textContent = display;
  });
}

function liveElapsedSeconds(state) {
  const base = Number.isFinite(state.liveBaseSeconds) ? state.liveBaseSeconds : 0;
  const startedAt = Number.isFinite(state.liveStartedAt) ? state.liveStartedAt : Date.now();
  return base + Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
}

function isHarnessLive(state) {
  const activeTask = state?.model?.activeTask;
  return Boolean(state?.flow?.ok && activeTask && normalizeCommandStatus(activeTask.status) === 'running');
}

function liveStatusMarkup(state, options = {}) {
  const t = labels[state.lang];
  const classes = ['ctox-live-chip'];
  if (options.compact) classes.push('is-compact');
  if (state.flow?.ok === false) classes.push('is-fallback');
  return `
    <span class="${classes.join(' ')}">
      <i aria-hidden="true"></i>
      <span>${escapeHtml(state.flow?.ok === false ? t.fallback : t.live)}</span>
      <strong data-live-elapsed>${escapeHtml(formatMetricValue(liveElapsedSeconds(state), 'seconds', state.lang))}</strong>
    </span>
  `;
}

function taskLiveStatusMarkup(task, state) {
  const status = normalizeCommandStatus(task?.status);
  if (status !== 'running' || task?.id !== state.model?.activeTask?.id) return '';
  return liveStatusMarkup(state, { compact: true });
}

function timelineLiveStatusMarkup(task, node, state) {
  if (task) return taskLiveStatusMarkup(task, state);
  if (node?.status !== 'active' || !isHarnessLive(state)) return '';
  return liveStatusMarkup(state, { compact: true });
}

function nodeLiveFactMarkup(node, task, state) {
  if (node?.status !== 'active') return '';
  if (!isHarnessLive(state)) return '';
  if (task && normalizeCommandStatus(task.status) !== 'running') return '';
  const t = labels[state.lang];
  return `<div><dt>${escapeHtml(t.live)}</dt><dd>${liveStatusMarkup(state, { compact: true })}</dd></div>`;
}

function aggregateFlowMetrics(flowResult) {
  const metrics = { inputTokens: null, outputTokens: null, toolCalls: null, seconds: null };
  const add = (candidate) => {
    if (!candidate) return;
    if (candidate.inputTokens !== null) metrics.inputTokens = (metrics.inputTokens || 0) + candidate.inputTokens;
    if (candidate.outputTokens !== null) metrics.outputTokens = (metrics.outputTokens || 0) + candidate.outputTokens;
    if (candidate.toolCalls !== null && candidate.toolCalls !== undefined) metrics.toolCalls = (metrics.toolCalls || 0) + candidate.toolCalls;
    if (candidate.seconds !== null && candidate.seconds !== undefined) metrics.seconds = Math.max(metrics.seconds || 0, candidate.seconds);
  };
  const flow = flowResult?.flow || {};
  for (const event of flow.ledger_events || []) {
    add(firstExplicitMetrics([event, parseMetadata(event.metadata_json)]));
  }
  for (const block of flow.blocks || []) {
    add(firstExplicitMetrics([block]));
    for (const branch of block.branches || []) add(firstExplicitMetrics([branch]));
  }
  return metrics;
}

function metricCard(label, value, kind, lang, options = {}) {
  const display = formatMetricValue(value, kind, lang);
  return `
    <div class="ctox-metric-card ${value === null || value === undefined ? 'is-empty' : ''} ${options.live ? 'is-live' : ''}">
      <span>${escapeHtml(label)}</span>
      <strong ${options.live ? 'data-live-elapsed' : ''}>${escapeHtml(display)}</strong>
    </div>
  `;
}

function formatMetricValue(value, kind, lang) {
  if (value === null || value === undefined) return labels[lang]?.notCaptured || labels.en.notCaptured;
  if (kind === 'seconds') {
    if (value >= 60) return `${Math.floor(value / 60)}m ${Math.round(value % 60)}s`;
    return `${Math.round(value)}s`;
  }
  if (kind === 'tokens') return formatTokenCount(value);
  return formatTokenCount(value);
}

function formatTokenCount(value) {
  return new Intl.NumberFormat('en-US', { maximumFractionDigits: 0 }).format(value);
}

function displayFlowMode(mode) {
  if (mode === 'ctox_cli' || mode === 'ctox_core') return 'CTOX core';
  return String(mode || 'fallback').replace(/[_-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function wrapSvgText(label) {
  if (label.length <= 16) return [label];
  const parts = label.split(/(?=[A-Z])|\s+/).filter(Boolean);
  const lines = [];
  let current = '';
  for (const part of parts) {
    const next = current ? `${current} ${part}` : part;
    if (next.length > 15 && current) {
      lines.push(current);
      current = part;
    } else {
      current = next;
    }
  }
  if (current) lines.push(current);
  return lines.slice(0, 2);
}

function branchToNodeId(kind, title, lines = []) {
  if (kind === 'queue_pickup') return queuePickupNode({ title, lines });
  if (kind === 'review') {
    const outcome = reviewBranchOutcome({ title, lines });
    if (outcome === 'passed') return 'review-passed';
    if (outcome === 'rejected') return 'review-rejected';
    return null;
  }
  if (kind === 'verification' && branchHasValidationEvidence({ title, lines })) return 'validating';
  return null;
}

function queuePickupNode(branch) {
  const text = branchText(branch);
  if (/\b(current queue state|reload status):\s*(leased|working|running)\b/.test(text) || /\b(leased by|lease time)\b/.test(text)) return 'leased';
  return null;
}

function reviewBranchOutcome(branch) {
  const text = branchText(branch);
  if (/\b(no persisted review result|not found|not yet|pending)\b/.test(text)) return 'unknown';
  if (/\b(ReviewPass|review_pass|review pass|review passed|completion_review_verdict=pass)\b/i.test(text)) return 'passed';
  if (/\b(ReviewReject|review_reject|review reject|review failed)\b/i.test(text)) return 'rejected';
  return 'unknown';
}

function branchHasValidationEvidence(branch) {
  const text = branchText(branch);
  if (/\b(no .*validation|no .*verification|not found|not yet|pending)\b/.test(text)) return false;
  return /\b(ValidatorPass|validator_pass|validator pass)\b/i.test(text);
}

function blockHasExplicitRuntimeEvidence(block) {
  if (explicitMetrics(block)) return true;
  return branchText(block).includes('tokens') && !branchText(block).includes('not instrumented yet');
}

function branchText(record) {
  return [record?.title, ...(record?.lines || [])].filter(Boolean).join(' ').toLowerCase();
}

function eventToNodeId(kind, title) {
  const value = `${kind} ${title}`.toLowerCase();
  if (/\b(workerfinished|worker_finished|worker finished)\b/.test(value)) return 'awaiting-review';
  if (/\b(workerfailed|worker_failed|worker failed)\b/.test(value)) return 'model-failed';
  if (/\b(infraerror|infra_error|infra error)\b/.test(value)) return 'infra-failed';
  if (/\b(startreview|start_review|start review)\b/.test(value)) return 'review-queued';
  if (/\b(spawnreviewer|spawn_reviewer|spawn reviewer)\b/.test(value)) return 'reviewing';
  if (/\b(reviewpass|review_pass|review pass|review passed)\b/.test(value)) return 'review-passed';
  if (/\b(reviewreject|review_reject|review reject|review failed)\b/.test(value)) return 'review-rejected';
  if (/\b(reviewunavailable|review_unavailable|review unavailable)\b/.test(value)) return 'review-unavailable';
  if (/\b(reviewretriesexhausted|review_retries_exhausted|review retries exhausted)\b/.test(value)) return 'infra-failed';
  if (/\b(retryreview|retry_review|retry review)\b/.test(value)) return 'awaiting-review';
  if (/\b(requeuesamemainwork|requeue_same_main_work|requeue same main work)\b/.test(value)) return 'queued';
  if (/\b(reviewroundsexhausted|review_rounds_exhausted|review rounds exhausted)\b/.test(value)) return 'model-failed';
  if (/\b(runvalidator|run_validator|run validator)\b/.test(value)) return 'validating';
  if (/\b(validatorpass|validator_pass|validator pass)\b/.test(value)) return 'passed';
  if (/\b(validatorfail|validator_fail|validator fail)\b/.test(value)) return 'rework-required';
  if (/\b(validatorreworkexhausted|validator_rework_exhausted|validator rework exhausted)\b/.test(value)) return 'model-failed';
  if (/\b(validatorinfraerror|validator_infra_error|validator infra error)\b/.test(value)) return 'infra-failed';
  if (value.includes('worker.token_usage')) return 'running';
  return null;
}

function cleanUiCopy(value = '') {
  return String(value)
    .replaceAll('ReviewHarness', 'Review process')
    .replaceAll('FounderCommunication', 'Founder communication')
    .replaceAll('WorkerFinished', 'Work finished')
    .replaceAll('ReviewPass', 'Review passed')
    .replaceAll('ReviewReject', 'Review failed')
    .replaceAll('ReworkRequired', 'Rework needed')
    .replaceAll('InfraFailed', 'Service failed')
    .replaceAll('ModelFailed', 'Work failed')
    .replaceAll('RunValidator', 'Check evidence')
    .replaceAll('StartReview', 'Start review')
    .replaceAll('SpawnReviewer', 'Start reviewer')
    .replaceAll('QueueItem', 'Work item')
    .replaceAll('BackingWorkQueued', 'Follow-up work added')
    .replaceAll('ReplyNeeded', 'Reply needed')
    .replaceAll('NoResponseNeeded', 'No reply needed')
    .replaceAll('ValidatorPass', 'Evidence confirmed')
    .replaceAll('WorkerFailed', 'Work failed')
    .replaceAll('ReviewRetriesExhausted', 'Review retries used up')
    .replaceAll('ReviewRoundsExhausted', 'Rework limit reached')
    .replace(/[_-]+/g, ' ');
}

function itemTitle(item) {
  return item?.title || item?.thread || item?.name || 'Current work';
}

function itemStatus(item) {
  return item?.status || 'unknown';
}

function itemSummary(item) {
  if ('summary' in item) return item.summary;
  if ('acceptance' in item) return item.acceptance;
  if ('promise' in item) return item.promise;
  return item.target || '';
}

function itemMeta(item) {
  if ('model' in item) return `${item.model} · ${formatShortTimestamp(item.startedAt)}`;
  if ('owner' in item) return `${item.owner} · ${displayPriority(item.priority)}`;
  if ('recipient' in item) return `${item.recipient} · ${displayPriority(item.priority)}`;
  return `${displayWorkSource(item.source || 'ctox')} · ${formatShortTimestamp(item.createdAt || new Date().toISOString())}`;
}

function formatShortTimestamp(value) {
  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed)) return value || '';
  return new Intl.DateTimeFormat('de-DE', { day: '2-digit', hour: '2-digit', minute: '2-digit', month: '2-digit' }).format(new Date(parsed));
}

function statusClass(status) {
  if (['done', 'completed', 'sent', 'approved', 'healthy'].includes(status)) return 'tone-ok';
  if (['running', 'review', 'drafting', 'leased', 'queued'].includes(status)) return 'tone-running';
  if (['blocked', 'failed', 'fail'].includes(status)) return 'tone-blocked';
  return 'tone-warning';
}

function displayWorkSource(source) {
  return String(source || 'ctox')
    .replace(/^ctox[-_\s]*/i, 'CTOX ')
    .split(/[/:]+/)
    .filter(Boolean)
    .map((part) => part.replace(/[_-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase()).replace(/\bCtox\b/g, 'CTOX').replace(/\bOs\b/g, 'OS'))
    .join(' / ');
}

function displayPathLike(value) {
  if (!/^[a-z0-9_-]+(\/[a-z0-9_-]+)+$/i.test(value || '')) return value || '';
  return displayWorkSource(value);
}

function displayPriority(priority) {
  const labelsByPriority = { urgent: 'Urgent', high: 'High', normal: 'Normal', low: 'Low' };
  return labelsByPriority[priority] || displayStatus(priority, 'en');
}

function displayStatus(status, lang = 'de') {
  const de = { approved: 'Freigegeben', blocked: 'Blockiert', completed: 'Erledigt', done: 'Erledigt', drafting: 'Entwurf', fail: 'Fehler', failed: 'Fehler', handled: 'Ohne Review-Beleg', healthy: 'OK', idle: 'Idle', leased: 'Übernommen', open: 'Offen', queued: 'Wartet', review: 'Review', running: 'Arbeitet', sent: 'Gesendet', unknown: 'Unbekannt' };
  const en = { approved: 'Approved', blocked: 'Blocked', completed: 'Done', done: 'Done', drafting: 'Drafting', fail: 'Failed', failed: 'Failed', handled: 'No review proof', healthy: 'Healthy', idle: 'Idle', leased: 'Picked up', open: 'Open', queued: 'Waiting', review: 'In review', running: 'Working', sent: 'Sent', unknown: 'Unknown' };
  const table = lang === 'en' ? en : de;
  return table[status] || String(status || '').replace(/[_-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function displayCommandTitle(doc) {
  const payload = doc.payload || {};
  return payload.title || payload.instruction || doc.command_type || doc.command_id || 'CTOX command';
}

function resultSummary(result) {
  if (!result || typeof result !== 'object') return '';
  if (Array.isArray(result.record_ids)) return `${result.record_ids.length} records · ${result.definition_id || result.collection || 'business_records'}`;
  if (result.record_id) return `${result.record_id} · ${result.definition_id || result.collection || 'business_records'}`;
  if (result.artifact_path) return result.artifact_path;
  return '';
}

function communicationPolicyInstruction(policy) {
  if (policy === 'reviewed-all-external') return 'Set CTOX communication policy to require review for every external message. Confirm the effective setting in the harness/state store and report the proof path.';
  if (policy === 'internal-only-autonomy') return 'Set CTOX communication policy so internal TUI/business-os instructions can proceed autonomously, while all owner-visible or external communication remains review-gated. Confirm the effective setting in the harness/state store and report the proof path.';
  return 'Set CTOX communication policy to strict founder review: no founder or owner-visible mail/chat may be sent without draft, full thread context, recipient/CC validation, review approval, automatic reviewed-send, and persisted send proof. Confirm the effective setting in the harness/state store and report the proof path.';
}

function defaultComposeText(lang) {
  if (lang === 'en') return 'Continue the most important open CTOX Business OS work. If code changes are needed, update the native Business OS module and keep the reusable template clean.';
  return 'Führe die wichtigste offene CTOX Business OS Arbeit fort. Wenn Codeänderungen nötig sind, aktualisiere das native Business OS Modul und halte die wiederverwendbare Vorlage sauber.';
}

function parseMetadata(value) {
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === 'object' ? parsed : null;
  } catch {
    return null;
  }
}

function readNumber(record, keys) {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'number' && Number.isFinite(value)) return value;
    if (typeof value === 'string' && value.trim()) {
      const parsed = Number(value);
      if (Number.isFinite(parsed)) return parsed;
    }
  }
  return null;
}

function readString(record, keys) {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) return value;
  }
  return null;
}

function millisToSeconds(value) {
  return value === null ? null : value / 1000;
}

function elapsedSeconds(startedAt, finishedAt) {
  if (!startedAt) return null;
  const start = Date.parse(startedAt);
  const finish = finishedAt ? Date.parse(finishedAt) : Date.now();
  if (!Number.isFinite(start) || !Number.isFinite(finish) || finish < start) return null;
  return (finish - start) / 1000;
}

function mergeById(primary, secondary) {
  const byId = new Map();
  [...secondary, ...primary].forEach((item) => byId.set(item.id, item));
  return Array.from(byId.values());
}

function clampMetric(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function clampIndex(index, length) {
  if (length <= 0) return 0;
  return Math.max(0, Math.min(length - 1, Number.isFinite(index) ? index : length - 1));
}

function clip(value, max) {
  const text = String(value || '');
  return text.length > max ? `${text.slice(0, max - 1)}...` : text;
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${CTOX_STYLE_BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/'/g, '&#39;');
}
