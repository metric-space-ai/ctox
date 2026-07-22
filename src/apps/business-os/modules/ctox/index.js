import { showBusinessAlert, showBusinessConfirm } from '../../shared/dialogs.js';
import { loadModuleMessages } from '../../shared/i18n.js';

const FLOW_WIDTH = 1760;
const FLOW_HEIGHT = 1050;
const NODE_WIDTH = 136;
const NODE_HEIGHT = 76;
const DEFAULT_ZOOM = 1;
const MIN_ZOOM = 0.72;
const MAX_ZOOM = 1.8;
const HARNESS_REFRESH_MS = 4000;
const LOCAL_RENDER_DEBOUNCE_MS = 80;
const HARNESS_STALL_GRACE_MS = 90 * 1000;
const HARNESS_WAITING_STATUSES = new Set(['queued', 'pending', 'accepted']);
const HARNESS_ACTIVE_STATUSES = new Set(['running', 'leased', 'review', 'drafting']);
const HARNESS_TERMINAL_STATUSES = new Set(['completed', 'done', 'sent', 'approved', 'healthy', 'handled', 'cancelled', 'failed', 'blocked']);
const HARNESS_SUCCESS_STATUSES = new Set(['completed', 'done', 'sent', 'approved', 'healthy']);
const HARNESS_PROBLEM_TERMINAL_STATUSES = new Set(['handled', 'cancelled', 'failed', 'blocked']);
const CTOX_STYLE_BUILD = '20260721-reference-flow-pins1';

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
    editTask: 'Task bearbeiten',
    taskTitle: 'Titel',
    taskPrompt: 'Prompt',
    taskPromptRedacted: 'Prompt ausgeblendet, da er Code, Stack- oder Web-Stack-Daten enthält.',
    redactedTechnicalDetail: 'Technische Details ausgeblendet',
    saveTask: 'Speichern',
    resumeTask: 'Als Folgeauftrag fortsetzen',
    deleteTask: 'Löschen',
    deleteTaskConfirm: 'Diesen CTOX Task wirklich löschen?',
    taskSaved: 'Task gespeichert.',
    taskResumed: 'Folgeauftrag angelegt.',
    taskDeleted: 'Task gelöscht.',
    taskActionFailed: 'Aktion fehlgeschlagen.',
    chefAdminOnly: 'Nur Chef oder Admin dürfen Tasks ändern.',
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
    agentPreparing: 'Agent wird vorbereitet',
    agentWorking: 'Agent arbeitet',
    agentCompleted: 'Agent-Durchlauf abgeschlossen',
    agentTimeout: 'Zeitlimit des Agenten erreicht',
    modelUsageUpdated: 'Modellnutzung aktualisiert',
    toolStarted: 'Werkzeug gestartet',
    toolFinished: 'Werkzeug abgeschlossen',
    connected: 'verbunden',
    notLive: 'nicht live',
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
    importTasks: 'Tasks importieren',
    exportTasks: 'Tasks exportieren',
    tasksImported: '{count} Tasks importiert.',
    taskImportFailed: 'Import fehlgeschlagen — keine importierbaren Tasks in der Datei.',
    chatToCtox: 'Chat to CTOX',
    workWithData: 'Mit Daten arbeiten',
    modifyApp: 'App modifizieren',
    contextPrompt: 'Was soll CTOX hier tun oder prüfen?',
    missingMessage: 'Nachricht fehlt.',
    chatNotReady: 'Chat ist noch nicht bereit.',
    openChat: 'Öffne Chat...',
    noWorkHere: 'Hier liegt gerade keine Arbeit.',
    noRecentWork: 'Noch keine aktuelle Arbeit erfasst.',
    noMetrics: 'keine Live-Tokenmetriken',
    routing: 'Routing',
    inbound: 'Inbound',
    outbound: 'Outbound',
    ticketsOpen: 'Offene Tickets',
    runtimePolicy: 'Runtime / Policies',
    queued: 'Command angelegt',
    webStack: 'Web Stack',
    webStackSources: 'Quellen',
    webStackCredentials: 'Credentials',
    webStackMissing: 'fehlen',
    webStackConfigured: 'konfiguriert',
    webStackSecret: 'Secret',
    webStackCredentialValue: 'Credential-Wert',
    webStackSaveCredential: 'Speichern',
    webStackVerifyCredential: 'Prüfen',
    webStackAuthAssist: 'Login im Browser',
    webStackRxdbOnly: 'Browser-Stream über RxDB, Secrets im CTOX Secret Store.',
    webStackLoading: 'Web Stack Projektion wird geladen…',
    webStackConnecting: 'RxDB ist verbunden, die CTOX Web-Stack-Projektion fehlt noch.',
    webStackUnavailable: 'Web Stack ist gerade nicht erreichbar.',
    webStackSyncRequired: 'Verbindung prüfen',
    webStackCheckProjection: 'Web-Stack-Projektion neu einlesen',
    webStackProjectionMissing: 'Der Web Stack ist gerade nicht vollständig verfügbar. Die reaktive Verbindung prüft weiter.',
    webStackCredentialSaved: 'Credential gespeichert.',
    webStackAuthQueued: 'Browser-Login angefordert.',
    webStackRecentCaptures: 'Letzte Captures',
    webStackNoCaptures: 'Noch keine Browser-Captures.',
    webStackRecentExtracts: 'Letzte Extracts',
    webStackNoExtracts: 'Noch keine Browser-Extracts.',
    timelineUnavailable: 'Keine Timeline-Ereignisse verfügbar',
    timelineUnavailableDetail: 'Der Regler ist deaktiviert, bis CTOX mehr als einen Schritt projiziert.',
    flowProjectionMissing: 'RxDB verbunden, CTOX Flow-Projektion fehlt',
    harnessHealth: 'Harness Health',
    harnessCriticalTitle: 'CTOX Harness verarbeitet keine Queue',
    harnessCriticalMessage: '{count} Aufgaben warten seit {age}; keine geleaste oder laufende Verarbeitung sichtbar.',
    harnessCriticalProjection: '{count} Aufgaben warten seit {age}; RxDB ist verbunden, aber die CTOX Flow-Projektion fehlt.',
    harnessWarningTitle: 'Queue wartet auf CTOX Harness',
    harnessWarningMessage: '{count} Aufgaben warten; noch keine Lease sichtbar.',
    harnessOpenTask: 'Task öffnen',
    harnessHealthy: 'Harness verarbeitet Queue',
    auxShow: 'Status & Quellen',
    auxHide: 'Status & Quellen ausblenden',
    harnessKicker: 'Harness',
    taskSearch: 'Tasks suchen',
    cardsView: 'Shard-Ansicht',
    compactFlowView: 'Kompakter Live Flow',
    filters: 'Filter',
    resetFilters: 'Filter zurücksetzen',
    allSources: 'Alle Quellen',
    allTasks: 'Alle Tasks',
    pinnedOnly: 'Nur Pins',
    sortUpdated: 'Aktualisiert',
    sortTitle: 'Titel',
    sortSource: 'Quelle',
    sortStatus: 'Status',
    sortDirection: 'Sortierrichtung wechseln',
    viewAll: 'Alle',
    viewWorking: 'Arbeitet',
    viewWaiting: 'Wartet',
    viewDone: 'Erledigt',
    entries: 'Einträge',
    pinTask: 'Task anpinnen',
    unpinTask: 'Pin lösen',
    pinned: 'Angepinnt',
    sessionPins: 'Pins gelten für diese Sitzung',
    pipelineQueued: 'Queue',
    pipelineWorking: 'Arbeit',
    pipelineReview: 'Review',
    pipelineDone: 'Fertig',
    flowFooterEmpty: 'Kein Task ausgewählt',
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
    editTask: 'Edit task',
    taskTitle: 'Title',
    taskPrompt: 'Prompt',
    taskPromptRedacted: 'Prompt hidden because it contains code, stack, or Web Stack data.',
    redactedTechnicalDetail: 'Technical details hidden',
    saveTask: 'Save',
    resumeTask: 'Continue as follow-up',
    deleteTask: 'Delete',
    deleteTaskConfirm: 'Delete this CTOX task?',
    taskSaved: 'Task saved.',
    taskResumed: 'Follow-up task queued.',
    taskDeleted: 'Task deleted.',
    taskActionFailed: 'Action failed.',
    chefAdminOnly: 'Only chef or admin can change tasks.',
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
    agentPreparing: 'Preparing agent',
    agentWorking: 'Agent is working',
    agentCompleted: 'Agent turn completed',
    agentTimeout: 'Agent turn timed out',
    modelUsageUpdated: 'Model usage updated',
    toolStarted: 'Tool started',
    toolFinished: 'Tool finished',
    connected: 'connected',
    notLive: 'not live',
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
    importTasks: 'Import tasks',
    exportTasks: 'Export tasks',
    tasksImported: '{count} tasks imported.',
    taskImportFailed: 'Import failed — no importable tasks in the file.',
    chatToCtox: 'Chat to CTOX',
    workWithData: 'Work with data',
    modifyApp: 'Modify app',
    contextPrompt: 'What should CTOX do or check here?',
    missingMessage: 'Message is missing.',
    chatNotReady: 'Chat is not ready yet.',
    openChat: 'Opening chat...',
    noWorkHere: 'No work here right now.',
    noRecentWork: 'No recent work recorded yet.',
    noMetrics: 'no live token metrics',
    routing: 'Routing',
    inbound: 'Inbound',
    outbound: 'Outbound',
    ticketsOpen: 'Open tickets',
    runtimePolicy: 'Runtime / policies',
    queued: 'Command queued',
    webStack: 'Web Stack',
    webStackSources: 'Sources',
    webStackCredentials: 'Credentials',
    webStackMissing: 'missing',
    webStackConfigured: 'configured',
    webStackSecret: 'Secret',
    webStackCredentialValue: 'Credential value',
    webStackSaveCredential: 'Save',
    webStackVerifyCredential: 'Verify',
    webStackAuthAssist: 'Login in Browser',
    webStackRxdbOnly: 'Browser stream over RxDB, secrets in CTOX Secret Store.',
    webStackLoading: 'Loading Web Stack projection…',
    webStackConnecting: 'RxDB is connected, but the CTOX Web Stack projection is still missing.',
    webStackUnavailable: 'Web Stack is currently unreachable.',
    webStackSyncRequired: 'Check connection',
    webStackCheckProjection: 'Reload Web Stack projection',
    webStackProjectionMissing: 'The Web Stack is not fully available right now. The reactive connection keeps checking.',
    webStackCredentialSaved: 'Credential saved.',
    webStackAuthQueued: 'Browser login requested.',
    webStackRecentCaptures: 'Recent captures',
    webStackNoCaptures: 'No browser captures yet.',
    webStackRecentExtracts: 'Recent extracts',
    webStackNoExtracts: 'No browser extracts yet.',
    timelineUnavailable: 'No timeline events available',
    timelineUnavailableDetail: 'The scrubber is disabled until CTOX projects more than one step.',
    flowProjectionMissing: 'RxDB connected, CTOX flow projection missing',
    harnessHealth: 'Harness health',
    harnessCriticalTitle: 'CTOX harness is not processing the queue',
    harnessCriticalMessage: '{count} tasks have been waiting for {age}; no leased or running work is visible.',
    harnessCriticalProjection: '{count} tasks have been waiting for {age}; RxDB is connected, but the CTOX flow projection is missing.',
    harnessWarningTitle: 'Queue is waiting for CTOX harness',
    harnessWarningMessage: '{count} tasks are waiting; no lease is visible yet.',
    harnessOpenTask: 'Open task',
    harnessHealthy: 'Harness is processing queue',
    auxShow: 'Status & sources',
    auxHide: 'Hide status & sources',
    harnessKicker: 'Harness',
    taskSearch: 'Search tasks',
    cardsView: 'Shard view',
    compactFlowView: 'Compact live flow',
    filters: 'Filters',
    resetFilters: 'Reset filters',
    allSources: 'All sources',
    allTasks: 'All tasks',
    pinnedOnly: 'Pinned only',
    sortUpdated: 'Updated',
    sortTitle: 'Title',
    sortSource: 'Source',
    sortStatus: 'Status',
    sortDirection: 'Change sort direction',
    viewAll: 'All',
    viewWorking: 'Working',
    viewWaiting: 'Waiting',
    viewDone: 'Done',
    entries: 'entries',
    pinTask: 'Pin task',
    unpinTask: 'Unpin task',
    pinned: 'Pinned',
    sessionPins: 'Pins last for this session',
    pipelineQueued: 'Queued',
    pipelineWorking: 'Working',
    pipelineReview: 'Review',
    pipelineDone: 'Done',
    flowFooterEmpty: 'No task selected',
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
  ctx.host.innerHTML = await loadModuleMarkup();
  const launchFocusTask = normalizeFocusTask(ctx.args);
  if (launchFocusTask) persistFocusTask(launchFocusTask);

  const state = {
    ctx,
    lang: ctx.locale === 'en' ? 'en' : 'de',
    flow: emptyHarnessFlow(),
    model: null,
    selectedStepIndex: 0,
    selectedTaskStepIndex: 0,
    selectedTaskId: null,
    selectedNodeId: '',
    zoom: DEFAULT_ZOOM,
    statusMessage: '',
    runtimeStatus: 'Loading status',
    focusTask: launchFocusTask || readFocusTask(),
    detailDrawer: null,
    taskSearch: '',
    taskViewMode: 'cards',
    taskPrimaryView: 'all',
    taskSourceFilter: 'all',
    taskPinFilter: 'all',
    taskSort: 'updated',
    taskSortDirection: 'desc',
    // Tray open/close is now shell-owned (data-pg-tray); the module keeps no
    // filter-tray state of its own.
    // On-demand Web Stack panel (main view), hidden by default; toggled from a
    // collected header icon. State survives the reactive re-renders.
    webStackPanelOpen: false,
    // No declared CTOX collection is suitable for user UI preferences; pins
    // therefore survive reactive re-renders for this mount session only.
    pinnedTaskIds: new Set(),
    userNavigatedTimeline: false,
    liveBaseSeconds: 0,
    liveStartedAt: Date.now(),
    liveTicker: null,
    refreshTimer: null,
    localSubscriptionCleanup: null,
    refreshInFlight: false,
    disposed: false,
    focusTaskOpenDrawer: false,
    harnessHealth: null,
    harnessToastId: '',
    harnessToastKey: '',
    layoutResizeCleanup: null,
    flowViewport: { left: 0, top: 0 },
    webStack: {
      loading: true,
      error: '',
      notice: '',
      data: null,
    },
  };

  const harness = ctx.host.querySelector('[data-ctox-harness]');
  if (harness) harness.__ctoxState = state;
  const teardownShellMessages = wireShellMessages(state);
  state.layoutResizeCleanup = wireColumnResize(state);
  await loadCtoxMessages(state.lang);
  renderLoading(state);
  startLiveTicker(state);
  state.localSubscriptionCleanup = wireLocalRealtime(state);
  // A cold RxDB/WebRTC lease must not block the OS window from becoming
  // operable. Hydrate in the background while the compact loading workspace is
  // already visible, then let the normal refresh interval take over.
  state.refreshInFlight = true;
  void renderFromLocalCache(state)
    .catch((error) => {
      if (!state.disposed) console.warn('[ctox] initial local render failed', error);
    })
    .finally(() => {
      state.refreshInFlight = false;
    });
  state.refreshTimer = window.setInterval(() => refresh(state), HARNESS_REFRESH_MS);
  return () => {
    state.disposed = true;
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
  const [commands, queueTasks, bugReports, webStack] = await Promise.all([
    loadLocalCommands(state.ctx).catch(() => []),
    loadLocalQueueTasks(state.ctx).catch(() => []),
    loadLocalBugReports(state.ctx).catch(() => []),
    loadLocalWebStackOverview(state.ctx).catch((error) => ({ ok: false, error: error.message || String(error) })),
  ]);
  if (state.disposed) return;
  state.webStack = {
    loading: false,
    error: webStack?.ok ? '' : (webStack?.error || 'Web Stack status unavailable'),
    notice: state.webStack?.notice || '',
    data: webStack?.ok ? webStack : state.webStack?.data,
  };
  state.flow = await loadHarnessFlowSnapshot(state.ctx).catch(() => emptyHarnessFlow('harness_flow_unavailable'));
  if (state.disposed) return;
  const bundle = mergeBundleWithCommands(ctoxSeed, commands, queueTasks, bugReports);
  const metrics = aggregateFlowMetrics(state.flow);
  state.liveBaseSeconds = Number.isFinite(metrics.seconds) ? metrics.seconds : 0;
  state.liveStartedAt = Date.now();
  state.model = buildHarnessModel(bundle, state.flow, state.lang);
  state.harnessHealth = deriveHarnessHealth(state);
  state.focusTask = readFocusTask();
  reconcileSelection(state);
  render(state);
  syncDetailDrawer(state);
}

function wireLocalRealtime(state) {
  const collectionsToWatch = ['business_commands', 'ctox_runtime_settings', 'ctox_queue_tasks', 'ctox_bug_reports'];
  let renderTimer = null;
  const scheduleRender = () => {
    if (state.disposed || state.refreshInFlight) return;
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
      const collection = ctoxCollection(state.ctx, collectionName);
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
  if (state.disposed || state.refreshInFlight) return;
  state.refreshInFlight = true;
  try {
    const [commands, queueTasks, bugReports, webStack, harnessFlow] = await Promise.all([
      loadLocalCommands(state.ctx).catch(() => []),
      loadLocalQueueTasks(state.ctx).catch(() => []),
      loadLocalBugReports(state.ctx).catch(() => []),
      loadLocalWebStackOverview(state.ctx).catch((error) => ({ ok: false, error: error.message || String(error) })),
      loadHarnessFlowSnapshot(state.ctx).catch(() => emptyHarnessFlow('harness_flow_unavailable')),
    ]);
    state.webStack = {
      loading: false,
      error: webStack?.ok ? '' : (webStack?.error || 'Web Stack status unavailable'),
      notice: state.webStack?.notice || '',
      data: webStack?.ok ? webStack : state.webStack?.data,
    };
    const nextFlow = harnessFlow?.ok ? harnessFlow : emptyHarnessFlow('rxdb_flow_projection_unavailable');
    if (state.disposed) return;
    const bundle = mergeBundleWithCommands(ctoxSeed, commands, queueTasks, bugReports);
    state.flow = nextFlow;
    const metrics = aggregateFlowMetrics(nextFlow);
    state.liveBaseSeconds = Number.isFinite(metrics.seconds) ? metrics.seconds : 0;
    state.liveStartedAt = Date.now();
    state.model = buildHarnessModel(bundle, nextFlow, state.lang);
    state.harnessHealth = deriveHarnessHealth(state);
    state.focusTask = readFocusTask();
    reconcileSelection(state);
    state.runtimeStatus = state.ctx?.sync?.mode === 'webrtc'
      ? displayFlowMode('rxdb-webrtc')
      : (state.ctx?.sync?.config?.native_rxdb_peer_reason || 'native CTOX RxDB peer is not available');
    render(state);
    syncDetailDrawer(state);
  } finally {
    state.refreshInFlight = false;
  }
}

function renderLoading(state) {
  const t = labels[state.lang];
  const main = state.ctx.host.querySelector('[data-ctox-main]');
  buildTaskColumn(state, { loading: true });
  if (main) {
    main.innerHTML = `
      <header class="ctox-pane-header ctox-pane-band">
        <div class="ctox-pane-title-row">
          <div class="ctox-pane-titles">
            <span class="ctox-pane-kicker">${escapeHtml(t.liveFlow)}</span>
            <h2 class="ctox-pane-title">${escapeHtml(t.doingNow)}</h2>
          </div>
          <div class="ctox-pane-actions"></div>
        </div>
      </header>
      <div class="ctox-pane-body ctox-flow-well">
        <section class="ctox-empty" aria-live="polite" aria-busy="true">
          <div><strong>${escapeHtml(t.loadingRuntime)}</strong><span>${escapeHtml(t.loadingRuntimeDetail)}</span></div>
        </section>
      </div>
      <footer class="ctox-harness-footer">${escapeHtml(t.loadingRuntime)}</footer>
    `;
  }
}

function render(state) {
  // Data refresh path: re-render ONLY the list content inside the well (never
  // the header/filterbar/search input — the operator never moves), then the
  // flow canvas / drawer as before.
  renderTaskList(state);
  renderMain(state);
  syncHarnessHealthUiState(state);
  updateLiveIndicators(state);
  updateHarnessHealthAlerts(state);
}

function deriveHarnessHealth(state) {
  const tasks = Array.isArray(state?.model?.tasks) ? state.model.tasks : [];
  const waitingTasks = tasks.filter(taskIsHarnessWaiting);
  const activeTasks = tasks.filter(taskIsHarnessActive);
  const flowProjectionMissing = harnessFlowProjectionMissing(state);
  const now = Date.now();
  const oldestWaitingAt = waitingTasks.reduce((oldest, task) => {
    const timestamp = taskTimestampMs(task);
    return Number.isFinite(timestamp) ? Math.min(oldest, timestamp) : oldest;
  }, Number.POSITIVE_INFINITY);
  const oldestWaitingAgeMs = waitingTasks.length && Number.isFinite(oldestWaitingAt)
    ? Math.max(0, now - oldestWaitingAt)
    : 0;
  const stalled = waitingTasks.length > 0
    && activeTasks.length === 0
    && (flowProjectionMissing || oldestWaitingAgeMs >= HARNESS_STALL_GRACE_MS);
  const waitingWithoutLease = waitingTasks.length > 0 && activeTasks.length === 0;
  const severity = stalled ? 'critical' : (waitingWithoutLease ? 'warning' : 'ok');
  const reason = stalled
    ? (flowProjectionMissing ? 'flow_projection_missing' : 'queue_stalled')
    : (waitingWithoutLease ? 'queue_waiting' : 'healthy');
  const focusTask = waitingTasks[0] || null;
  return {
    ok: severity !== 'critical',
    severity,
    reason,
    waitingCount: waitingTasks.length,
    activeCount: activeTasks.length,
    oldestWaitingAgeMs,
    flowProjectionMissing,
    focusTaskId: focusTask?.id || '',
    focusTaskTitle: focusTask?.title || '',
  };
}

function taskIsHarnessWaiting(task) {
  if (!task || taskIsHarnessTerminal(task) || taskIsHarnessActive(task)) return false;
  const statuses = taskHarnessStatuses(task);
  return statuses.some((status) => HARNESS_WAITING_STATUSES.has(status));
}

function taskIsHarnessActive(task) {
  if (!task || taskIsHarnessTerminal(task)) return false;
  return taskHarnessStatuses(task).some((status) => HARNESS_ACTIVE_STATUSES.has(status));
}

function taskIsHarnessTerminal(task) {
  return taskHarnessStatuses(task).some((status) => HARNESS_TERMINAL_STATUSES.has(status));
}

function taskHarnessStatuses(task) {
  const raw = [
    task?.status,
    task?.routeStatus,
    task?.route_status,
    task?.task_status,
  ].filter((value) => String(value || '').trim());
  return raw.length
    ? raw.map((value) => normalizeCommandStatus(value))
    : ['queued'];
}

function taskTimestampMs(task) {
  const candidates = [task?.createdAt, task?.startedAt, task?.timestamp, task?.updatedAt];
  for (const value of candidates) {
    const parsed = Date.parse(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return Number.NaN;
}

function harnessFlowProjectionMissing(state) {
  if (state?.flow?.ok) return false;
  const error = String(state?.flow?.error || '').toLowerCase();
  if (error.includes('projection')) return true;
  if (error.includes('rxdb')) return true;
  return state?.ctx?.sync?.mode === 'webrtc';
}

function syncHarnessHealthUiState(state) {
  const harness = state.ctx.host.querySelector('[data-ctox-harness]');
  if (!harness) return;
  const health = state.harnessHealth || deriveHarnessHealth(state);
  const title = harnessHealthTitle(state, health);
  const message = harnessHealthMessage(state, health);
  const detail = health.severity === 'ok' ? title : `${title}: ${message}`;
  harness.dataset.harnessHealth = health.severity;
  harness.title = detail;
  harness.setAttribute('aria-label', detail);
  harness.classList.toggle('has-critical-harness', health.severity === 'critical');
  harness.classList.toggle('has-warning-harness', health.severity === 'warning');
  harness.querySelectorAll('[data-harness-health-tooltip]').forEach((element) => {
    element.title = detail;
    element.setAttribute('aria-label', detail);
  });
}

function harnessHealthTitle(state, health) {
  const t = labels[state.lang];
  if (health?.severity === 'critical') return t.harnessCriticalTitle;
  if (health?.severity === 'warning') return t.harnessWarningTitle;
  return t.harnessHealthy;
}

function harnessHealthMessage(state, health) {
  const t = labels[state.lang];
  const values = {
    count: String(health?.waitingCount || 0),
    age: formatRelativeAge(health?.oldestWaitingAgeMs || 0, state.lang),
  };
  if (health?.severity === 'critical' && health.flowProjectionMissing) {
    return interpolateLabel(t.harnessCriticalProjection, values);
  }
  if (health?.severity === 'critical') {
    return interpolateLabel(t.harnessCriticalMessage, values);
  }
  if (health?.severity === 'warning') {
    return interpolateLabel(t.harnessWarningMessage, values);
  }
  return t.harnessHealthy;
}

function updateHarnessHealthAlerts(state) {
  const health = state.harnessHealth || deriveHarnessHealth(state);
  const notifications = state.ctx?.notifications;
  if (!notifications?.show) return;
  if (!health || health.severity !== 'critical') {
    if (state.harnessToastId && notifications.close) notifications.close(state.harnessToastId);
    state.harnessToastId = '';
    state.harnessToastKey = '';
    return;
  }
  const key = `${health.reason}:${health.waitingCount}:${health.focusTaskId}`;
  if (state.harnessToastId && state.harnessToastKey === key) return;
  if (state.harnessToastId && notifications.close) notifications.close(state.harnessToastId);
  state.harnessToastKey = key;
  state.harnessToastId = notifications.show({
    type: 'error',
    icon: '!',
    title: harnessHealthTitle(state, health),
    message: harnessHealthMessage(state, health),
    time: 0,
    action: health.focusTaskId
      ? {
          label: labels[state.lang].harnessOpenTask,
          callback: () => selectTask(state, health.focusTaskId, { drawer: true, center: true }),
        }
      : null,
  });
}

function interpolateLabel(template, values) {
  return String(template || '').replace(/\{([a-zA-Z0-9_]+)\}/g, (_match, key) => values[key] ?? '');
}

function formatRelativeAge(ms, lang) {
  const seconds = Math.max(0, Math.floor(Number(ms) / 1000));
  if (!Number.isFinite(seconds) || seconds < 60) {
    return lang === 'de' ? 'unter 1 Min.' : 'under 1 min';
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return lang === 'de' ? `${minutes} Min.` : `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  if (hours < 24) {
    if (!restMinutes) return lang === 'de' ? `${hours} Std.` : `${hours} hr`;
    return lang === 'de' ? `${hours} Std. ${restMinutes} Min.` : `${hours} hr ${restMinutes} min`;
  }
  const days = Math.floor(hours / 24);
  return lang === 'de' ? `${days} Tg.` : `${days} d`;
}

function wireColumnResize(state) {
  // Column resizing is owned by the shell-global resizer (setupModuleResizers
  // in app.js), which wires the `.ctox-column-resizer[data-resizer-var]` handle
  // in index.html declaratively (including width persistence). The module must
  // NOT DIY-wire it here or the handle gets double-wired. Return a no-op
  // teardown; mount/unmount semantics are preserved.
  return () => {};
}

// The task column chrome is SHELL-owned canonical grammar: the module builds
// the data-pg-* markup ONCE (here) and the shell wires search / view toggle /
// tray / reset / active-dot / band behaviour (autoWirePaneGrammar). It is never
// rebuilt on a data refresh — only the list inside the well is re-rendered — so
// the operator never loses focus or scroll position on the 4s refresh.
function buildTaskColumn(state, options = {}) {
  const left = state.ctx.host.querySelector('[data-ctox-left]');
  if (!left) return;
  left.innerHTML = taskColumnMarkup(state.model?.tasks || [], state, options);
  // Force the shell to (re)wire the freshly built chrome: the pane may still
  // carry a stale wired-marker + grammar handle from a previous build (e.g. a
  // language switch), which autoWirePaneGrammar would otherwise skip.
  left.removeAttribute('data-pg-wired');
  left.__ctoxPaneGrammar = null;
  wireTaskColumn(state);
}

// Persistent, delegated wiring on the pane element (survives list rebuilds).
// Search / view / tray / reset / band are the shell's job (reported through the
// bubbling ctox-pane-grammar-change event); the module only owns record actions
// (select / pin) and the domain-specific sort-direction toggle.
function wireTaskColumn(state) {
  const left = state.ctx.host.querySelector('[data-ctox-left]');
  if (!left || left.__ctoxTaskWired) return;
  left.__ctoxTaskWired = true;
  left.addEventListener('ctox-pane-grammar-change', (event) => onTaskGrammarChange(state, event));
  left.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target : null;
    if (!target) return;
    const importAction = target.closest('[data-task-import]');
    if (importAction) {
      openTaskImportPicker(state);
      return;
    }
    const exportAction = target.closest('[data-task-export]');
    if (exportAction) {
      exportVisibleTasks(state);
      return;
    }
    const direction = target.closest('[data-task-sort-direction]');
    if (direction) {
      state.taskSortDirection = state.taskSortDirection === 'asc' ? 'desc' : 'asc';
      direction.innerHTML = actionIcon(state, state.taskSortDirection === 'asc' ? 'chevronUp' : 'chevronDown');
      renderTaskList(state);
      return;
    }
    const pin = target.closest('[data-pin-task-id]');
    if (pin) {
      event.preventDefault();
      event.stopPropagation();
      toggleTaskPin(state, pin.dataset.pinTaskId);
      renderTaskList(state);
      return;
    }
    const select = target.closest('[data-select-task-id]');
    if (select) selectTask(state, select.dataset.selectTaskId, { drawer: true, center: true });
  });
}

function onTaskGrammarChange(state, event) {
  const detail = event?.detail || {};
  state.taskSearch = String(detail.search ?? state.taskSearch ?? '');
  state.taskViewMode = detail.view === 'list' ? 'list' : 'cards';
  state.taskPrimaryView = detail.band || 'all';
  const filters = detail.filters || {};
  state.taskSourceFilter = filters.source || 'all';
  state.taskPinFilter = filters.pin || 'all';
  state.taskSort = filters.sort || 'updated';
  // Intentional reset: a list rebuild here is correct (the shell scroll guard
  // clears recorded offsets on this event).
  renderTaskList(state);
}

// Data-refresh path: re-render ONLY the list content + counts/footer. Never the
// header/filterbar/search input.
function renderTaskList(state) {
  const left = state.ctx.host.querySelector('[data-ctox-left]');
  if (!left) return;
  const list = left.querySelector('[data-task-list]');
  if (!list) { buildTaskColumn(state); return; }
  const tasks = state.model?.tasks || [];
  const cards = state.taskViewMode !== 'list';
  list.className = `ctox-list ctox-task-list ${cards ? 'is-cards' : 'is-compact-flow'}`;
  list.innerHTML = taskListInner(tasks, state);
  updateTaskSourceOptions(state, left, tasks);
  renderTaskCountsAndFooter(state, left, tasks);
}

// In-place selection: flip is-selected/aria-selected across the existing rows,
// never a list rebuild (the flow canvas / drawer still re-render on selection).
function applyTaskSelection(state) {
  const list = state.ctx.host.querySelector('[data-ctox-left] [data-task-list]');
  if (!list) return;
  list.querySelectorAll('[data-task-id]').forEach((row) => {
    const on = (row.getAttribute('data-task-id') || '') === String(state.selectedTaskId || '');
    row.classList.toggle('is-selected', on);
    row.setAttribute('aria-selected', String(on));
  });
}

// The source filter is a data-pg-filter select the shell wired; only rewrite its
// <option>s when the source set actually changes so a plain refresh never
// touches the filterbar (and the wired listener is preserved).
function updateTaskSourceOptions(state, left, tasks) {
  const select = left.querySelector('[data-pg-filter][data-pg-name="source"]');
  if (!select) return;
  const t = labels[state.lang];
  const options = taskSourceOptions(tasks);
  const signature = `${state.lang}::${options.map((item) => `${item.value}:${item.label}`).join('|')}`;
  if (select.__ctoxSourceSig === signature) return;
  select.__ctoxSourceSig = signature;
  const current = state.taskSourceFilter || 'all';
  if (current !== 'all' && !options.some((item) => item.value === current)) state.taskSourceFilter = 'all';
  select.innerHTML = `<option value="all">${escapeHtml(t.allSources)}</option>`
    + options.map((item) => `<option value="${escapeAttr(item.value)}">${escapeHtml(item.label)}</option>`).join('');
  select.value = state.taskSourceFilter;
}

function renderTaskCountsAndFooter(state, left, tasks) {
  const t = labels[state.lang];
  const counts = taskPrimaryViewCounts(tasks, state);
  const visibleTasks = filterAndSortTasks(tasks, state);
  const viewLabel = taskPrimaryViewLabel(state.taskPrimaryView, t);
  const scopeLabel = state.taskPinFilter === 'pinned' ? `${viewLabel} · ${t.pinned}` : viewLabel;
  const footerText = `${visibleTasks.length} ${t.entries} · ${scopeLabel}${state.pinnedTaskIds.size ? ` · ${state.pinnedTaskIds.size} ${t.pinned}` : ''}`;
  const pg = left.__ctoxPaneGrammar;
  if (pg?.setCounts) pg.setCounts(counts);
  else for (const [key, value] of Object.entries(counts)) {
    const node = left.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
  if (pg?.setFooter) pg.setFooter(footerText);
  else {
    const node = left.querySelector('[data-pg-footer]');
    if (node) node.textContent = footerText;
  }
}

// Header actions: export serializes the currently visible (filtered + sorted)
// task records as a JSON download. Import reads such a file (or a plain array
// of {title, instruction|prompt}) and creates real work through the EXISTING
// task creation path (business_os.chat.task via dispatchCtoxTaskMutation) —
// the same command type the shared chat and the resume flow dispatch.
function exportVisibleTasks(state) {
  const visibleTasks = filterAndSortTasks(state.model?.tasks || [], state);
  const exportedAt = new Date().toISOString();
  const payload = {
    format: 'ctox-task-export',
    version: 1,
    exportedAt,
    module: 'ctox',
    view: state.taskPrimaryView || 'all',
    count: visibleTasks.length,
    tasks: visibleTasks.map((task) => ({
      taskId: String(task.id || ''),
      commandId: String(task.commandId || ''),
      title: taskDisplayTitle(task, state),
      status: String(task.routeStatus || task.status || ''),
      source: String(task.channel || task.source || task.moduleId || ''),
      prompt: String(task.prompt || ''),
      updatedAt: String(task.updatedAt || ''),
      createdAt: String(task.createdAt || task.timestamp || ''),
    })),
  };
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `ctox-tasks-${exportedAt.slice(0, 19).replace(/[:T]/g, '-')}.json`;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function openTaskImportPicker(state) {
  const t = labels[state.lang];
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files?.[0];
    if (!file) return;
    try {
      const created = await importTasksFromFile(state, file);
      showBusinessAlert(t.tasksImported.replace('{count}', String(created)));
    } catch {
      showBusinessAlert(t.taskImportFailed);
    }
  }, { once: true });
  input.click();
}

async function importTasksFromFile(state, file) {
  const parsed = JSON.parse(await file.text());
  const items = Array.isArray(parsed) ? parsed : (Array.isArray(parsed?.tasks) ? parsed.tasks : null);
  if (!items) throw new Error('unsupported task import format');
  const entries = items
    .map((item) => ({
      title: String(item?.title || '').trim(),
      instruction: String(item?.instruction || item?.prompt || '').trim(),
    }))
    .filter((item) => item.title)
    .slice(0, 50);
  if (!entries.length) throw new Error('no importable tasks in file');
  for (const entry of entries) {
    await dispatchCtoxTaskMutation(state, {
      commandType: 'business_os.chat.task',
      payload: {
        title: entry.title,
        instruction: entry.instruction || entry.title,
        imported: true,
        source: 'ctox-task-import',
      },
      commandPath: 'ctox_task_import',
    });
  }
  refresh(state).catch(() => {});
  return entries.length;
}

function taskListInner(tasks, state, options = {}) {
  const t = labels[state.lang];
  if (options.loading) return '<div class="ctox-loading-list" aria-hidden="true"><span></span><span></span><span></span></div>';
  const cards = state.taskViewMode !== 'list';
  const visibleTasks = filterAndSortTasks(tasks, state);
  if (!visibleTasks.length) return `<div class="ctox-empty"><span>${escapeHtml(t.noWorkHere)}</span></div>`;
  return visibleTasks.map((task) => (cards ? taskCardMarkup(task, state) : compactTaskFlowRow(task, state))).join('');
}

function taskColumnMarkup(tasks, state, options = {}) {
  const t = labels[state.lang];
  const counts = taskPrimaryViewCounts(tasks, state);
  const sourceOptions = taskSourceOptions(tasks);
  const viewLabel = taskPrimaryViewLabel(state.taskPrimaryView, t);
  const scopeLabel = state.taskPinFilter === 'pinned' ? `${viewLabel} · ${t.pinned}` : viewLabel;
  const visibleCount = filterAndSortTasks(tasks, state).length;
  const footerText = `${visibleCount} ${t.entries} · ${scopeLabel}${state.pinnedTaskIds.size ? ` · ${state.pinnedTaskIds.size} ${t.pinned}` : ''}`;
  const cards = state.taskViewMode !== 'list';
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(t.harnessKicker)}</span>
          <h2 class="ctox-pane-title">${escapeHtml(t.tasks)}</h2>
        </div>
        <div class="ctox-pane-actions">
          <button type="button" class="ctox-pane-icon" data-task-import aria-label="${escapeAttr(t.importTasks)}" title="${escapeAttr(t.importTasks)}">${actionIcon(state, 'download')}</button>
          <button type="button" class="ctox-pane-icon" data-task-export aria-label="${escapeAttr(t.exportTasks)}" title="${escapeAttr(t.exportTasks)}">${actionIcon(state, 'export')}</button>
        </div>
      </div>
      <div class="ctox-filterbar">
        <input class="ctox-pane-search" type="search" data-pg-search value="${escapeAttr(state.taskSearch || '')}" placeholder="${escapeAttr(t.taskSearch)}" aria-label="${escapeAttr(t.taskSearch)}">
        <div class="ctox-view-toggle" role="group" aria-label="${escapeAttr(t.mode)}">
          <button type="button" class="ctox-pane-icon" data-pg-view="cards" aria-pressed="${cards}" aria-label="${escapeAttr(t.cardsView)}" title="${escapeAttr(t.cardsView)}">${cardsViewIcon()}</button>
          <button type="button" class="ctox-pane-icon" data-pg-view="list" aria-pressed="${!cards}" aria-label="${escapeAttr(t.compactFlowView)}" title="${escapeAttr(t.compactFlowView)}">${listViewIcon()}</button>
        </div>
        <button type="button" class="ctox-pane-icon ctox-filter-toggle" data-pg-tray-toggle aria-expanded="false" aria-label="${escapeAttr(t.filters)}" title="${escapeAttr(t.filters)}">${actionIcon(state, 'filter')}</button>
      </div>
      <div class="ctox-filter-tray" data-pg-tray hidden>
        <div class="ctox-filter-row">
          <select class="ctox-select" data-pg-filter data-pg-name="source" data-pg-default="all" aria-label="${escapeAttr(t.source)}">
            <option value="all">${escapeHtml(t.allSources)}</option>
            ${sourceOptions.map((item) => `<option value="${escapeAttr(item.value)}" ${state.taskSourceFilter === item.value ? 'selected' : ''}>${escapeHtml(item.label)}</option>`).join('')}
          </select>
          <select class="ctox-select" data-pg-filter data-pg-name="pin" data-pg-default="all" aria-label="${escapeAttr(t.pinned)}">
            <option value="all" ${state.taskPinFilter !== 'pinned' ? 'selected' : ''}>${escapeHtml(t.allTasks)}</option>
            <option value="pinned" ${state.taskPinFilter === 'pinned' ? 'selected' : ''}>${escapeHtml(t.pinnedOnly)}</option>
          </select>
          <select class="ctox-select" data-pg-filter data-pg-name="sort" data-pg-default="updated" aria-label="${escapeAttr(t.newestFirst)}">
            <option value="updated" ${state.taskSort === 'updated' ? 'selected' : ''}>${escapeHtml(t.sortUpdated)}</option>
            <option value="title" ${state.taskSort === 'title' ? 'selected' : ''}>${escapeHtml(t.sortTitle)}</option>
            <option value="source" ${state.taskSort === 'source' ? 'selected' : ''}>${escapeHtml(t.sortSource)}</option>
            <option value="status" ${state.taskSort === 'status' ? 'selected' : ''}>${escapeHtml(t.sortStatus)}</option>
          </select>
          <button type="button" class="ctox-sort-dir" data-task-sort-direction aria-label="${escapeAttr(t.sortDirection)}" title="${escapeAttr(t.sortDirection)}">${actionIcon(state, state.taskSortDirection === 'asc' ? 'chevronUp' : 'chevronDown')}</button>
          <button type="button" class="ctox-sort-dir" data-pg-reset aria-label="${escapeAttr(t.resetFilters)}" title="${escapeAttr(t.resetFilters)}">${resetIcon()}</button>
        </div>
      </div>
    </header>
    <nav class="ctox-view-switch" aria-label="${escapeAttr(t.tasks)}">
      <div class="ctox-pane-tabs" role="tablist">
        ${taskViewTab('all', t.viewAll, counts.all, state)}
        ${taskViewTab('working', t.viewWorking, counts.working, state)}
        ${taskViewTab('waiting', t.viewWaiting, counts.waiting, state)}
        ${taskViewTab('done', t.viewDone, counts.done, state)}
      </div>
    </nav>
    <div class="ctox-pane-body ctox-well">
      <div class="ctox-list ctox-task-list ${cards ? 'is-cards' : 'is-compact-flow'}" data-task-list>${taskListInner(tasks, state, options)}</div>
    </div>
    <footer class="ctox-pane-footer"><span data-pg-footer>${escapeHtml(footerText)}</span></footer>
  `;
}

function taskViewTab(view, label, count, state) {
  const selected = (state.taskPrimaryView || 'all') === view;
  return `<button type="button" class="ctox-pane-tab ${selected ? 'is-active' : ''}" role="tab" data-pg-band="${escapeAttr(view)}" aria-selected="${selected}">${escapeHtml(label)}<span class="view-count" data-pg-count="${escapeAttr(view)}"> (${count})</span></button>`;
}

function taskCardMarkup(task, state) {
  const t = labels[state.lang];
  const selected = task.id === state.selectedTaskId;
  const pinned = state.pinnedTaskIds.has(task.id);
  const title = taskDisplayTitle(task, state);
  const source = task.channelLabel || displayWorkSource(task.channel || task.source || task.moduleId || 'ctox');
  const meta = [source, displayStatus(task.routeStatus || task.status, state.lang), formatShortTimestamp(task.updatedAt || task.createdAt || task.timestamp)].filter(Boolean).join(' · ');
  return `
    <article class="ctox-list-item ctox-task-card ${selected ? 'is-selected' : ''} ${pinned ? 'is-pinned' : ''}"
      data-task-id="${escapeAttr(task.id)}" data-context-record-id="${escapeAttr(task.id)}" data-context-record-type="ctox_task" data-context-label="${escapeAttr(title)}">
      <button type="button" class="ctox-task-selector" data-select-task-id="${escapeAttr(task.id)}" aria-label="${escapeAttr(`${t.openTaskDetail}: ${title}`)}">
        <strong>${escapeHtml(title)}</strong>
        <small>${escapeHtml(meta)}</small>
        ${taskPipelineMarkup(task, state)}
      </button>
      <div class="ctox-task-actions">
        <button type="button" class="ctox-pane-icon ${pinned ? 'is-active' : ''}" data-pin-task-id="${escapeAttr(task.id)}" aria-pressed="${pinned}" aria-label="${escapeAttr(pinned ? t.unpinTask : t.pinTask)}" title="${escapeAttr(pinned ? t.unpinTask : t.pinTask)}">${actionIcon(state, 'pin')}</button>
      </div>
    </article>
  `;
}

function compactTaskFlowRow(task, state) {
  const t = labels[state.lang];
  const selected = task.id === state.selectedTaskId;
  const pinned = state.pinnedTaskIds.has(task.id);
  const title = taskDisplayTitle(task, state);
  const source = task.channelLabel || displayWorkSource(task.channel || task.source || task.moduleId || 'ctox');
  return `
    <article class="ctox-list-item ctox-task-flow-row ${selected ? 'is-selected' : ''} ${pinned ? 'is-pinned' : ''}"
      data-compact-flow data-task-id="${escapeAttr(task.id)}" data-context-record-id="${escapeAttr(task.id)}" data-context-record-type="ctox_task" data-context-label="${escapeAttr(title)}">
      <button type="button" class="ctox-task-selector" data-select-task-id="${escapeAttr(task.id)}" aria-label="${escapeAttr(`${t.openTaskDetail}: ${title}`)}">
        <span class="ctox-task-flow-copy"><strong>${escapeHtml(title)}</strong><small>${escapeHtml(source)}</small></span>
        ${taskPipelineMarkup(task, state, { compact: true })}
      </button>
      <div class="ctox-task-actions">
        <button type="button" class="ctox-pane-icon ${pinned ? 'is-active' : ''}" data-pin-task-id="${escapeAttr(task.id)}" aria-pressed="${pinned}" aria-label="${escapeAttr(pinned ? t.unpinTask : t.pinTask)}" title="${escapeAttr(pinned ? t.unpinTask : t.pinTask)}">${actionIcon(state, 'pin')}</button>
      </div>
    </article>
  `;
}

function taskPipelineMarkup(task, state, options = {}) {
  const t = labels[state.lang];
  const current = taskPipelineStage(task);
  const problem = ['blocked', 'failed', 'cancelled'].includes(normalizeCommandStatus(task.routeStatus || task.status));
  const stages = [t.pipelineQueued, t.pipelineWorking, t.pipelineReview, t.pipelineDone];
  return `<div class="ctox-task-pipeline ${options.compact ? 'is-compact' : ''} ${problem ? 'is-problem' : ''}" aria-label="${escapeAttr(stages[current])}" data-flow-stage="${current}">${stages.map((label, index) => `<span class="${index < current ? 'is-complete' : index === current ? 'is-current' : 'is-future'}"><i aria-hidden="true"></i><em>${escapeHtml(label)}</em></span>`).join('')}</div>`;
}

function taskPipelineStage(task) {
  const statuses = taskStatusCandidates(task);
  if (statuses.some((status) => ['completed', 'done', 'sent', 'approved', 'healthy'].includes(status))) return 3;
  if (statuses.some((status) => ['review', 'awaiting-review', 'reviewing', 'validating'].includes(status))) return 2;
  if (statuses.some((status) => ['running', 'leased', 'working', 'drafting'].includes(status))) return 1;
  return 0;
}

function taskSourceOptions(tasks) {
  const sources = new Map();
  for (const task of tasks) {
    const value = taskCategoryKey(task);
    if (!sources.has(value)) sources.set(value, taskCategoryLabel(task));
  }
  return Array.from(sources, ([value, label]) => ({ value, label })).sort((left, right) => left.label.localeCompare(right.label));
}

function filterAndSortTasks(tasks, state, options = {}) {
  const filtered = tasks.filter((task) => taskMatchesSecondaryFilters(task, state));
  const primary = options.ignorePrimary ? filtered : filtered.filter((task) => taskMatchesPrimaryView(task, state.taskPrimaryView || 'all'));
  const direction = state.taskSortDirection === 'asc' ? 1 : -1;
  return [...primary].sort((left, right) => {
    const pinned = Number(state.pinnedTaskIds.has(right.id)) - Number(state.pinnedTaskIds.has(left.id));
    if (pinned) return pinned;
    let comparison = 0;
    if (state.taskSort === 'title') comparison = taskDisplayTitle(left, state).localeCompare(taskDisplayTitle(right, state));
    else if (state.taskSort === 'source') comparison = taskCategoryLabel(left).localeCompare(taskCategoryLabel(right));
    else if (state.taskSort === 'status') comparison = displayStatus(left.status, state.lang).localeCompare(displayStatus(right.status, state.lang));
    else comparison = taskTimestampMs(left) - taskTimestampMs(right);
    return comparison * direction;
  });
}

function taskMatchesSecondaryFilters(task, state) {
  const query = String(state.taskSearch || '').trim().toLowerCase();
  if (query) {
    const haystack = [task.title, task.summary, task.source, task.channelLabel, task.status, task.routeStatus].filter(Boolean).join(' ').toLowerCase();
    if (!haystack.includes(query)) return false;
  }
  if (state.taskSourceFilter && state.taskSourceFilter !== 'all' && taskCategoryKey(task) !== state.taskSourceFilter) return false;
  if (state.taskPinFilter === 'pinned' && !state.pinnedTaskIds.has(task.id)) return false;
  return true;
}

function taskMatchesPrimaryView(task, view) {
  const statuses = taskStatusCandidates(task);
  const done = statuses.some((status) => HARNESS_SUCCESS_STATUSES.has(status));
  const working = !done && statuses.some((status) => HARNESS_ACTIVE_STATUSES.has(status));
  if (view === 'working') return working;
  if (view === 'waiting') return !done && !working;
  if (view === 'done') return done;
  return true;
}

function taskPrimaryViewCounts(tasks, state) {
  const scoped = filterAndSortTasks(tasks, state, { ignorePrimary: true });
  return {
    all: scoped.length,
    working: scoped.filter((task) => taskMatchesPrimaryView(task, 'working')).length,
    waiting: scoped.filter((task) => taskMatchesPrimaryView(task, 'waiting')).length,
    done: scoped.filter((task) => taskMatchesPrimaryView(task, 'done')).length,
  };
}

function taskPrimaryViewLabel(view, t) {
  if (view === 'working') return t.viewWorking;
  if (view === 'waiting') return t.viewWaiting;
  if (view === 'done') return t.viewDone;
  return t.viewAll;
}

function toggleTaskPin(state, taskId) {
  if (!taskId) return;
  if (state.pinnedTaskIds.has(taskId)) state.pinnedTaskIds.delete(taskId);
  else state.pinnedTaskIds.add(taskId);
}

function cardsViewIcon() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg>';
}

function listViewIcon() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg>';
}

function resetIcon() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 10a8 8 0 1 1 2 7"/><path d="M4 5v5h5"/></svg>';
}

function actionIcon(state, name) {
  const fromShell = state.ctx?.getActionIcon?.(name);
  if (fromShell) return fromShell;
  const paths = {
    add: 'M12 5v14M5 12h14',
    filter: 'M4 6h16M7 12h10M10 18h4',
    pin: 'M9 4h6l-1 7 3 2v2H7v-2l3-2-1-7ZM12 15v5',
    chevronUp: 'M6 15l6-6 6 6',
    chevronDown: 'M6 9l6 6 6-6',
    close: 'M6 6l12 12M18 6L6 18',
    refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4',
    open: 'M14 5h5v5M19 5l-8 8M11 5H5v14h14v-6',
    play: 'M8 5.5v13l10-6.5-10-6.5Z',
    trash: 'M5 7h14M10 7V5h4v2M8 7l1 13h6l1-13M10.5 11v5M13.5 11v5',
  };
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${paths[name] || paths.open}"></path></svg>`;
}

function taskCategoryKey(task) {
  return normalizeInboundChannel(task?.channel || task?.channelLabel || task?.source || task?.moduleId || 'ctox');
}

function taskCategoryLabel(task) {
  return task?.channelLabel || displayWorkSource(task?.channel || task?.source || task?.moduleId || 'ctox');
}

// Drawer-only contextual status maps onto the kit badge variants; the task
// column itself intentionally has no standing status badges.
function statusBadgeVariant(tone) {
  if (tone === 'tone-ok') return 'is-success';
  if (tone === 'tone-blocked') return 'is-danger';
  if (tone === 'tone-running') return 'is-info';
  return 'is-warning';
}

function friendlyWebStackStatus(webStack, t) {
  if (webStack?.loading) return t.webStackLoading;
  const raw = String(webStack?.error || '').trim();
  if (!raw) return webStack?.notice || t.webStackRxdbOnly;
  const lower = raw.toLowerCase();
  if (lower.includes('projection is not available') || lower.includes('rxdb')) return t.webStackConnecting;
  if (lower.includes('not available') || lower.includes('unavailable')) return t.webStackUnavailable;
  if (lower.includes('command bus')) return t.webStackConnecting;
  // Unknown error shape — never surface raw stack/projection error text in the UI.
  return t.webStackUnavailable;
}

function webStackProjectionMissing(webStack) {
  const raw = String(webStack?.error || '').trim().toLowerCase();
  return Boolean(raw && (raw.includes('projection is not available') || raw.includes('ctox_runtime_settings') || raw.includes('rxdb')));
}

function browserExtractSummary(fields = {}, lang = 'en') {
  return Object.entries(fields || {})
    .filter(([, value]) => value !== null && value !== undefined && String(value).trim())
    .slice(0, 4)
    .map(([key, value]) => `${key}: ${safeTaskDisplayText(value, lang, { max: 80 })}`)
    .join(' · ');
}

function webStackIcon() {
  return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.6 2.7 2.6 15.3 0 18M12 3c-2.6 2.7-2.6 15.3 0 18"/></svg>';
}

// On-demand Web Stack section for the main view — a hidden-by-default popover
// toggled from the collected header icon. Ported from the original left-column
// aux panel onto kit classes; the shape (status, credential sources, recent
// captures/extracts, refresh) is unchanged.
function webStackPanel(state) {
  const t = labels[state.lang];
  const webStack = state.webStack || {};
  const data = webStack.data || {};
  const summary = data.summary || {};
  const sources = Array.isArray(data.sources) ? data.sources : [];
  const credentialSources = sources
    .filter((source) => source?.credential?.required)
    .sort((left, right) => Number(left.credential.configured) - Number(right.credential.configured) || String(left.id).localeCompare(String(right.id)));
  const firstMissing = credentialSources.find((source) => !source.credential.configured) || credentialSources[0];
  const selectedSecret = firstMissing?.credential?.secret_name || '';
  const hasCredentialOptions = credentialSources.length > 0;
  const rows = credentialSources.slice(0, 5).map((source) => {
    const configured = Boolean(source.credential.configured);
    return `
      <article class="ctox-web-stack-source ${configured ? 'is-configured' : 'is-missing'}">
        <span>
          <strong>${escapeHtml(source.id)}</strong>
          <small>${escapeHtml(source.credential.secret_name || '')}</small>
        </span>
        <button type="button" class="ctox-button ctox-button--sm" data-webstack-auth-source="${escapeAttr(source.id)}" data-webstack-auth-secret="${escapeAttr(source.credential.secret_name || '')}">
          ${escapeHtml(configured ? t.webStackAuthAssist : t.webStackVerifyCredential)}
        </button>
      </article>
    `;
  }).join('');
  const captureRows = recentWebStackBrowserCaptures(state).slice(0, 3).map((capture) => `
    <article class="ctox-web-stack-capture" data-task-id="${escapeAttr(capture.taskId)}" data-context-label="${escapeAttr(capture.sourceId || capture.captureScript || capture.taskId)}">
      <span><strong>${escapeHtml(capture.sourceId || capture.captureScript || capture.title)}</strong><small>${escapeHtml([capture.captureScript, capture.frameId].filter(Boolean).join(' · '))}</small></span>
      <small>${escapeHtml(formatShortTimestamp(capture.timestamp))}</small>
    </article>
  `).join('');
  const extractRows = recentWebStackBrowserExtracts(state).slice(0, 3).map((extract) => `
    <article class="ctox-web-stack-capture is-extract" data-command-id="${escapeAttr(extract.commandId)}" data-context-label="${escapeAttr(extract.sourceId || extract.captureScript || extract.commandId)}">
      <span><strong>${escapeHtml(extract.sourceId || extract.captureScript || extract.title)}</strong><small>${escapeHtml(extract.summary || extract.captureScript || extract.commandId)}</small></span>
      <small>${escapeHtml(formatShortTimestamp(extract.timestamp))}</small>
    </article>
  `).join('');

  const friendlyStatus = friendlyWebStackStatus(webStack, t);
  const projectionMissing = webStackProjectionMissing(webStack);
  const headerSummary = webStack.loading
    ? t.webStackLoading
    : projectionMissing
      ? t.webStackSyncRequired
      : `${summary.credential_configured || 0}/${summary.credential_required || 0} ${t.webStackConfigured}`;
  const statusTone = webStack.error ? 'is-warning' : (webStack.notice ? 'is-info' : '');
  return `
    <section class="ctox-web-stack-panel ctox-context-item" data-webstack-panel data-context-label="${escapeAttr(t.webStack)}" data-context-record-id="ctox-web-stack" ${state.webStackPanelOpen ? '' : 'hidden'}>
      <header class="ctox-pane-title-row ctox-web-stack-head">
        <div class="ctox-pane-titles ctox-web-stack-head-titles">
          <span class="ctox-pane-kicker">${escapeHtml(t.webStack)}</span>
          <strong class="ctox-badge ${statusTone}">${escapeHtml(headerSummary)}</strong>
        </div>
        <div class="ctox-pane-actions ctox-web-stack-head-actions">
          <button type="button" class="ctox-pane-icon" data-webstack-check-projection aria-label="${escapeAttr(t.webStackCheckProjection)}" title="${escapeAttr(t.webStackCheckProjection)}">${actionIcon(state, 'refresh')}</button>
          <button type="button" class="ctox-pane-icon" data-webstack-close aria-label="${escapeAttr(t.auxHide)}" title="${escapeAttr(t.auxHide)}">${actionIcon(state, 'close')}</button>
        </div>
      </header>
      <div class="ctox-web-stack-body">
        <div class="ctox-callout ctox-web-stack-status ${statusTone}" role="status">${escapeHtml(friendlyStatus)}</div>
        ${projectionMissing ? `<div class="ctox-callout is-info ctox-web-stack-diagnostic">${escapeHtml(t.webStackProjectionMissing)}</div>` : ''}
        ${hasCredentialOptions && !projectionMissing ? `<small>${escapeHtml(`${t.webStackSecret}: ${selectedSecret}`)}</small>` : ''}
        <div class="ctox-web-stack-source-list">
          ${!projectionMissing && rows ? rows : `<small>${escapeHtml(t.webStackSources)}: ${Number(summary.sources || 0)}${projectionMissing ? ` · ${t.webStackSyncRequired}` : ''}</small>`}
        </div>
        <div class="ctox-web-stack-capture-list">
          <span>${escapeHtml(t.webStackRecentCaptures)}</span>
          ${captureRows || `<small>${escapeHtml(t.webStackNoCaptures)}</small>`}
        </div>
        <div class="ctox-web-stack-capture-list">
          <span>${escapeHtml(t.webStackRecentExtracts)}</span>
          ${extractRows || `<small>${escapeHtml(t.webStackNoExtracts)}</small>`}
        </div>
      </div>
    </section>
  `;
}

function recentWebStackBrowserCaptures(state) {
  const tasks = state.model?.tasks || [];
  return tasks
    .map((task) => {
      const artifact = task.browserContextArtifact || task.browser_context_artifact || null;
      if (artifact?.kind !== 'browser_context') return null;
      const context = artifact.browser_context || {};
      return {
        taskId: task.taskId || task.id || '',
        title: task.title || '',
        sourceId: artifact.source_id || context.source_id || '',
        captureScript: artifact.capture_script || context.capture_script || '',
        frameId: context.frame_id || '',
        timestamp: task.updatedAt || task.createdAt || task.timestamp || '',
      };
    })
    .filter(Boolean)
    .sort((left, right) => Date.parse(right.timestamp || 0) - Date.parse(left.timestamp || 0));
}

function recentWebStackBrowserExtracts(state) {
  const tasks = state.model?.tasks || [];
  return tasks
    .map((task) => {
      const artifact = task.browserExtractArtifact || null;
      if (artifact?.kind !== 'browser_extract') return null;
      return {
        commandId: task.commandId || artifact.command_id || task.id || '',
        title: task.title || '',
        sourceId: artifact.source_id || '',
        captureScript: artifact.capture_script || '',
        summary: browserExtractSummary(artifact.fields, state.lang),
        timestamp: task.updatedAt || task.createdAt || task.timestamp || '',
      };
    })
    .filter(Boolean)
    .sort((left, right) => Date.parse(right.timestamp || 0) - Date.parse(left.timestamp || 0));
}

function wireWebStackPanel(state, root) {
  root.querySelector('[data-webstack-close]')?.addEventListener('click', () => {
    state.webStackPanelOpen = false;
    const panel = root.querySelector('[data-webstack-panel]');
    if (panel) panel.hidden = true;
    const toggle = root.querySelector('[data-webstack-toggle]');
    toggle?.classList.remove('is-active');
    toggle?.setAttribute('aria-pressed', 'false');
    toggle?.setAttribute('aria-expanded', 'false');
  });
  root.querySelector('[data-webstack-check-projection]')?.addEventListener('click', async () => {
    state.webStack = { ...(state.webStack || {}), loading: true, notice: '' };
    renderMain(state);
    await refreshWebStackPanel(state);
  });
  root.querySelectorAll('[data-webstack-auth-source]').forEach((button) => {
    button.addEventListener('click', async () => {
      const sourceId = button.dataset.webstackAuthSource || '';
      const secretName = button.dataset.webstackAuthSecret || '';
      const source = (state.webStack?.data?.sources || []).find((candidate) => candidate.id === sourceId);
      if (source?.credential?.configured) await requestWebStackAuthAssist(state, source);
      else await verifyWebStackCredential(state, sourceId, secretName);
    });
  });
}

function taskSteps(task, state) {
  if (!task) return [];
  if (isExactCommunicationFlow(task, state)) return communicationTaskSteps(task, state);
  const timeline = state.model?.timeline || [];
  if (timeline.length && taskMatchesHarnessFlow(task, state)) {
    const steps = timeline.map((node, index) => ({
      id: node.id,
      label: node.label,
      detail: clip(cleanUiCopy(node.lines?.[0] || node.phase || itemSummary(task) || ''), 180),
      timestamp: node.timestamp || '',
      metrics: metricsLabel(node, state.lang),
      active: node.status === 'active' || index === timeline.length - 1,
      timelineIndex: index,
    }));
    return withRouteStatusStep(steps, task, state);
  }
  return taskStatusSteps(task, state);
}

function communicationTaskSteps(task, state) {
  const trace = communicationTraceFromFlow(state.flow, task);
  const activeId = trace.at(-1) || 'comm-inbound-observed';
  return trace.map((id) => {
    const node = COMMUNICATION_NODE_MAP.get(id);
    return {
      id,
      label: node?.label || displayStatus(task?.routeStatus || task?.status, state.lang),
      detail: cleanUiCopy(node?.lines?.[0] || task?.summary || task?.target || ''),
      timestamp: task?.updatedAt || task?.createdAt || '',
      metrics: '',
      active: id === activeId,
      timelineIndex: -1,
      flowKind: 'communication',
    };
  });
}

function withRouteStatusStep(steps, task, state) {
  const routeNode = routeStatusNodeId(task?.routeStatus || task?.status);
  if (!routeNode || steps.some((step) => step.id === routeNode)) return steps;
  return steps
    .map((step) => ({ ...step, active: false }))
    .concat({
      id: routeNode,
      label: displayStatus(task?.routeStatus || task?.status, state.lang),
      detail: taskDetailText(task?.resultSummary || task?.summary || task?.target || task?.source || '', state),
      timestamp: task?.updatedAt || task?.createdAt || '',
      metrics: '',
      active: true,
      timelineIndex: -1,
    });
}

function taskMatchesHarnessFlow(task, state) {
  if (!task || !state) return false;
  const source = state.flow?.flow?.source || {};
  const ids = new Set([source.message_key, source.work_id].filter(Boolean));
  if (ids.has(task.id) || ids.has(task.taskId) || ids.has(task.commandId) || ids.has(task.runId)) return true;
  return false;
}

function taskStatusSteps(task, state) {
  const status = normalizeCommandStatus(task.status || task.routeStatus);
  const timeline = state.model?.timeline || [];
  const findIndex = (id) => {
    if (!id) return -1;
    const index = timeline.findIndex((node) => node.id === id);
    return index >= 0 ? index : -1;
  };
  const steps = [];
  const routeNode = routeStatusNodeId(task.routeStatus || task.status);
  steps.push(routeNode
    ? {
        id: routeNode,
        label: displayStatus(status, state.lang),
        detail: taskDetailText(task.resultSummary || task.summary || task.target || task.source || '', state),
        active: true,
      }
    : {
        id: 'queued',
        label: displayStatus(status, state.lang),
        detail: taskDetailText(task.resultSummary || task.summary || task.target || task.source || labels[state.lang].unprovenOutcome, state),
        active: true,
        unverified: true,
      });
  if (taskMatchesHarnessFlow(task, state)) {
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
  const selectedNodeOverride = state.selectedNodeId ? model.nodeMap.get(state.selectedNodeId) : null;
  const selectedNode = selectedNodeOverride
    || (taskStepView
      ? taskStepView.node
      : model.timeline[timelineIndex] || model.nodes.find((node) => node.id === model.activeNodeId) || model.nodes[0]);
  const visibleTrace = selectedNodeOverride
    ? buildVisibleTraceWindow([selectedNodeOverride])
    : taskStepView
      ? buildVisibleTraceFromSteps(model, taskStepView.steps, taskStepView.index)
      : buildVisibleTrace(model.timeline, timelineIndex);
  const metricSubject = metricSubjectTask(state, selectedTask);
  const live = isLiveMetricSubject(metricSubject, state);
  const metrics = metricSubject ? aggregateFlowMetrics(state.flow) : emptyMetrics();
  const elapsedSeconds = live ? liveElapsedSeconds(state) : metrics.seconds;
  const flowSource = flowSourceView(state);
  const main = state.ctx.host.querySelector('[data-ctox-main]');
  const previousViewport = readFlowViewport(state);
  const viewBox = flowViewBox(selectedTask, state);
  main.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(t.liveFlow)}</span>
          <h2 class="ctox-pane-title">${escapeHtml(t.doingNow)}</h2>
        </div>
        <div class="ctox-pane-actions">
          <button type="button" class="ctox-pane-icon ${state.webStackPanelOpen ? 'is-active' : ''}" data-webstack-toggle aria-pressed="${state.webStackPanelOpen}" aria-expanded="${state.webStackPanelOpen}" aria-label="${escapeAttr(t.webStack)}" title="${escapeAttr(t.webStack)}">${webStackIcon()}</button>
          ${selectedTask ? `<button type="button" class="ctox-pane-icon" data-open-selected-task aria-label="${escapeAttr(t.openTaskDetail)}" title="${escapeAttr(t.openTaskDetail)}">${actionIcon(state, 'open')}</button>` : ''}
        </div>
      </div>
    </header>
    ${webStackPanel(state)}
    <section class="ctox-metrics-strip" aria-label="${escapeAttr(t.measurements)}">
      ${metricCard(t.inputTokens, metrics.inputTokens, 'tokens', state.lang)}
      ${metricCard(t.outputTokens, metrics.outputTokens, 'tokens', state.lang)}
      ${metricCard(t.toolCalls, metrics.toolCalls, 'count', state.lang)}
      ${metricCard(t.elapsed, elapsedSeconds, 'seconds', state.lang, { live })}
    </section>
    <div class="ctox-canvas-container ctox-flow-well">
      <div class="ctox-flow-toolbar" aria-label="Flow chart controls" data-flow-control>
        <button type="button" class="ctox-pane-icon" data-zoom="-" aria-label="Zoom out" title="Zoom out" ${state.zoom <= MIN_ZOOM ? 'disabled' : ''}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="M5 12h14"/></svg></button>
        <span>${Math.round(state.zoom * 100)}%</span>
        <button type="button" class="ctox-pane-icon" data-zoom="+" aria-label="Zoom in" title="Zoom in" ${state.zoom >= MAX_ZOOM ? 'disabled' : ''}>${actionIcon(state, 'add')}</button>
      </div>
      <div class="ctox-flow-canvas" data-flow-canvas>
        <div class="ctox-flow-canvas-inner" style="width:${FLOW_WIDTH * state.zoom}px;height:${viewBox.height * state.zoom}px;min-height:${viewBox.height * state.zoom}px">
          ${flowSvg(model, selectedNode, visibleTrace, selectedTask, state, taskStepView, viewBox)}
        </div>
      </div>
    </div>
    ${timelinePanel(state, selectedTask, selectedNode, metrics)}
    <footer class="ctox-harness-footer" data-harness-health-tooltip>${escapeHtml(selectedTask ? taskDisplayTitle(selectedTask, state) : t.flowFooterEmpty)} · ${escapeHtml(flowSource.mode)} · ${escapeHtml(flowSource.status)}${live ? ` · ${escapeHtml(t.live)}` : ''}</footer>
  `;
  restoreFlowViewport(state, previousViewport);
  main.querySelector('[data-webstack-toggle]')?.addEventListener('click', () => {
    state.webStackPanelOpen = !state.webStackPanelOpen;
    const panel = main.querySelector('[data-webstack-panel]');
    if (panel) panel.hidden = !state.webStackPanelOpen;
    main.querySelector('[data-webstack-toggle]')?.classList.toggle('is-active', state.webStackPanelOpen);
    main.querySelector('[data-webstack-toggle]')?.setAttribute('aria-pressed', String(state.webStackPanelOpen));
    main.querySelector('[data-webstack-toggle]')?.setAttribute('aria-expanded', String(state.webStackPanelOpen));
    if (state.webStackPanelOpen) refreshWebStackPanel(state);
  });
  wireWebStackPanel(state, main);
  main.querySelector('[data-open-selected-task]')?.addEventListener('click', () => {
    if (selectedTask) selectTask(state, selectedTask.id, { drawer: true, center: false });
  });
  main.querySelectorAll('[data-zoom]').forEach((button) => {
    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      const action = button.dataset.zoom;
      zoomFlowFromControl(state, action);
    });
  });
  main.querySelectorAll('[data-timeline-step]').forEach((button) => {
    button.addEventListener('click', () => {
      setTimelineStep(state, Number(button.dataset.timelineStep), { center: true });
    });
  });
  main.querySelectorAll('[data-task-step-index]').forEach((button) => {
    button.addEventListener('click', () => {
      setTaskTimelineStep(state, Number(button.dataset.taskStepIndex), { center: true });
    });
  });
  main.querySelectorAll('[data-task-id]').forEach((button) => {
    button.addEventListener('click', () => {
      selectTask(state, button.dataset.taskId, { drawer: true, center: true });
    });
  });
  main.querySelector('[data-timeline-range]')?.addEventListener('input', (event) => {
    if (event.target.dataset.taskTimelineRange === 'true') {
      setTaskTimelineStep(state, Number(event.target.value), { center: true });
      return;
    }
    const mappedSteps = event.target.dataset.timelineRangeSteps
      ? event.target.dataset.timelineRangeSteps.split(',').map((value) => Number(value))
      : null;
    setTimelineStep(state, mappedSteps?.[Number(event.target.value)] ?? Number(event.target.value), { center: true });
  });
  main.querySelectorAll('[data-node-id]').forEach((node) => {
    node.addEventListener('click', () => {
      selectFlowNode(state, node.dataset.nodeId, { drawer: true });
    });
    node.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      selectFlowNode(state, node.dataset.nodeId, { drawer: true });
    });
  });
  wireCanvasDrag(main.querySelector('[data-flow-canvas]'));
  updateLiveIndicators(state);
}

function emptyMetrics() {
  return { inputTokens: null, outputTokens: null, toolCalls: null, seconds: null };
}

function timelinePanel(state, selectedTask, selectedNode, metrics) {
  const t = labels[state.lang];
  if (!selectedTask) {
    const max = Math.max(state.model.timeline.length - 1, 0);
    const value = clampIndex(state.selectedStepIndex, state.model.timeline.length);
    const hasRange = max > 0;
    return `
      <section class="ctox-timeline-panel ${hasRange ? '' : 'is-disabled'}" aria-label="Activity timeline" style="--timeline-progress:${escapeAttr(progressPercent(value, max))}%">
        <div class="ctox-timeline-head">
          <div>
            <span class="ctox-pane-kicker">${escapeHtml(t.timeline)}</span>
            ${timelineLiveStatusMarkup(selectedTask, selectedNode, state)}
          </div>
          <strong>${escapeHtml(hasRange ? (selectedNode?.label || '') : t.timelineUnavailable)}</strong>
        </div>
        <div class="ctox-timeline-scrub">
          <input aria-label="Select activity event" max="${max}" min="0" step="1" type="range" value="${value}" data-timeline-range ${hasRange ? '' : 'disabled aria-disabled="true"'} />
        </div>
        <div class="ctox-timeline-detail">
          <span>${escapeHtml(hasRange ? (selectedNode?.phase || '') : t.notLive)}</span>
          <p>${escapeHtml(hasRange ? (selectedNode?.lines?.[0] || 'No detail is available for this event yet.') : t.timelineUnavailableDetail)}</p>
          <small>${escapeHtml(selectedNode ? metricsLabel(selectedNode, state.lang) : '')}</small>
        </div>
      </section>
    `;
  }
  const steps = taskSteps(selectedTask, state);
  const selectedTaskStepIndex = clampMetric(state.selectedTaskStepIndex || 0, 0, Math.max(steps.length - 1, 0));
  const activeStepIndex = state.userNavigatedTimeline
    ? selectedTaskStepIndex
    : Math.max(0, steps.findIndex((step) => step.active));
  const current = steps[activeStepIndex] || steps.find((step) => step.active) || steps.at(-1);
  const max = Math.max(steps.length - 1, 0);
  const hasRange = max > 0;
  return `
    <section class="ctox-timeline-panel is-task-timeline ${hasRange ? '' : 'is-disabled'}" aria-label="${escapeAttr(t.taskSteps)}" style="--timeline-progress:${escapeAttr(progressPercent(activeStepIndex, max))}%">
      <div class="ctox-timeline-head">
        <div>
          <span class="ctox-pane-kicker">${escapeHtml(t.timeline)}</span>
          ${timelineLiveStatusMarkup(selectedTask, current, state)}
        </div>
        <strong>${escapeHtml(hasRange ? selectedTask.title : t.timelineUnavailable)}</strong>
      </div>
      <div class="ctox-timeline-scrub">
        <input aria-label="${escapeAttr(t.taskSteps)}" max="${max}" min="0" step="1" type="range" value="${activeStepIndex}" data-timeline-range data-task-timeline-range="true" ${hasRange ? '' : 'disabled aria-disabled="true"'} />
        <div class="ctox-timeline-scale" role="list" ${hasRange ? '' : 'aria-disabled="true"'}>
          ${steps.map((step, index) => `
            <button type="button" role="listitem" class="${index < activeStepIndex ? 'is-done' : ''} ${index === activeStepIndex ? 'is-current' : ''}" data-task-step-index="${index}" data-context-record-id="${escapeAttr(`${selectedTask.id}:${step.id || index}`)}" data-context-record-type="ctox_task_step" data-context-label="${escapeAttr(step.label)}" ${hasRange ? '' : 'disabled'}>
              <span>${String(index + 1).padStart(2, '0')}</span>
              <strong>${escapeHtml(step.label)}</strong>
              <small>${escapeHtml(stepMetaLabel(step, state))}</small>
            </button>
          `).join('')}
        </div>
      </div>
      <div class="ctox-timeline-detail">
        <span>${escapeHtml(hasRange ? (current?.label || t.currentStep) : t.notLive)}</span>
        <p>${escapeHtml(hasRange ? (current?.detail || selectedNode?.lines?.[0] || itemSummary(selectedTask) || t.noRecentWork) : t.timelineUnavailableDetail)}</p>
        <small>${escapeHtml(current ? `${stepMetaLabel(current, state)} · ${current.metrics || ''}` : selectedNode ? metricsLabel(selectedNode, state.lang) : '')}</small>
      </div>
    </section>
  `;
}

function progressPercent(value, max) {
  if (!Number.isFinite(max) || max <= 0) return 100;
  return Math.round((clampMetric(value, 0, max) / max) * 100);
}

function flowSvg(model, selectedNode, visibleTrace, selectedTask, state, taskStepView = null, viewBox = flowViewBox(selectedTask, state)) {
  const communicationOnly = isCommunicationFlow(selectedTask, state);
  const harnessOffsetY = reviewHarnessOffsetY(selectedTask, state);
  return `
    <svg class="ctox-flow-diagram" viewBox="0 ${viewBox.y} ${FLOW_WIDTH} ${viewBox.height}" preserveAspectRatio="xMidYMin meet" role="img" aria-label="CTOX work flow diagram">
      <defs>
        <marker id="ctox-flow-arrow" markerHeight="8" markerWidth="8" orient="auto" refX="7" refY="4">
          <path d="M0,0 L8,4 L0,8 Z"></path>
        </marker>
      </defs>
      <g class="ctox-flow-lanes" aria-hidden="true">
        ${communicationOnly ? `
          <rect x="18" y="18" width="${FLOW_WIDTH - 36}" height="340" rx="16"></rect>
          <text x="34" y="44">Founder communication state machine</text>
        ` : `
          <g transform="translate(0 ${harnessOffsetY})">
          <rect x="18" y="388" width="${FLOW_WIDTH - 36}" height="260" rx="16"></rect>
          <rect x="18" y="688" width="${FLOW_WIDTH - 36}" height="340" rx="16"></rect>
          <text x="34" y="414">Review harness queue and execution</text>
          <text x="34" y="714">Review harness evidence check</text>
          </g>
        `}
      </g>
      ${communicationFlowSvg(selectedTask, state, taskStepView)}
      ${communicationOnly ? '' : `<g class="ctox-review-harness-flow" transform="translate(0 ${harnessOffsetY})">`}
      ${communicationOnly ? '' : taskEndpointFlowSvg(model, selectedTask, selectedNode, visibleTrace, state)}
      ${communicationOnly ? '' : model.edges.map((edge) => {
        const from = model.nodeMap.get(edge.from);
        const to = model.nodeMap.get(edge.to);
        if (!from || !to) return '';
        const strength = visibleTrace.edgeStrength.get(edgeKey(edge.from, edge.to)) || 0;
        const activeEdge = model.liveWork && edge.to === selectedNode?.id && strength > 0;
        return `<path class="ctox-flow-edge ${strength > 0 ? 'is-observed' : ''} ${activeEdge ? 'is-active-edge' : ''}" d="${edgePath(from, to, edge.route)}" style="--edge-strength:${strength}"></path>`;
      }).join('')}
      ${communicationOnly ? '' : model.nodes.map((node) => flowNodeSvg(node, selectedNode, visibleTrace.nodeStrength.get(node.id) || 0, state.lang)).join('')}
      ${communicationOnly ? '' : '</g>'}
    </svg>
  `;
}

function flowViewBox(selectedTask, state) {
  if (isCommunicationFlow(selectedTask, state)) return { y: 0, height: 380 };
  return { y: 54, height: 740 };
}

function reviewHarnessOffsetY(selectedTask, state) {
  return isCommunicationFlow(selectedTask, state) ? 0 : -300;
}

function selectedNodeVisualY(node, selectedTask, state) {
  return (node?.y || 0) + reviewHarnessOffsetY(selectedTask, state);
}

function taskEndpointFlowSvg(model, selectedTask, selectedNode, visibleTrace, state) {
  return `
    ${inboundEndpointFlowSvg(model, selectedTask, state)}
    ${outboundEndpointFlowSvg(model, selectedTask, selectedNode, visibleTrace, state)}
  `;
}

function communicationFlowSvg(selectedTask, state, taskStepView = null) {
  if (!isCommunicationFlow(selectedTask, state)) return '';
  const trace = communicationTraceFromFlow(state.flow, selectedTask);
  const live = isHarnessLive(state);
  const selectedCommunicationNodeId = taskStepView?.step?.flowKind === 'communication' ? taskStepView.step.id : '';
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
      ${COMMUNICATION_NODES.map((node) => communicationNodeSvg(
        node,
        observed.has(node.id),
        selectedCommunicationNodeId ? selectedCommunicationNodeId === node.id : live && trace.at(-1) === node.id
      )).join('')}
    </g>
  `;
}

function communicationNodeSvg(node, observed, current) {
  return `
    <g class="ctox-flow-node-g ctox-communication-node ${observed ? 'is-observed is-trace' : 'is-possible'} ${current ? 'is-current is-selected' : ''}"
       data-context-record-id="${escapeAttr(node.id)}" data-context-record-type="ctox_flow_node" data-context-label="${escapeAttr(node.label)}"
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
  if (isExactCommunicationFlow(task, state)) return true;
  if (task) return false;
  return flowHasFounderCommunicationEvidence(state?.flow);
}

function isExactCommunicationFlow(task, state) {
  return Boolean(taskMatchesHarnessFlow(task, state) && flowHasFounderCommunicationEvidence(state?.flow));
}

function flowHasFounderCommunicationEvidence(flowResult) {
  const flow = flowResult?.flow || {};
  const sourceKind = String(flow.source?.source_kind || '').toLowerCase();
  if (sourceKind === 'communication' || sourceKind === 'founder_communication') return true;
  for (const block of flow.blocks || []) {
    for (const branch of block.branches || []) {
      const text = [branch.title, ...(branch.lines || [])].join(' ');
      if (/\bFounderCommunication\b/.test(text)) return true;
      const matches = text.matchAll(/Accepted:\s*([A-Za-z]+)\s*->\s*([A-Za-z]+)\s*\(([^)]+)\)/g);
      for (const match of matches) {
        const from = COMMUNICATION_STATE_TO_NODE.get(normalizeCoreStateKey(match[1]));
        const to = COMMUNICATION_STATE_TO_NODE.get(normalizeCoreStateKey(match[2]));
        if (from && to) return true;
      }
    }
  }
  return false;
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
  const fallbackNode = selectedNode?.id || routeStatusNodeId(task?.routeStatus || task?.status);
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
  if (taskMatchesHarnessFlow(task, state)) {
    const last = state.model?.timeline?.at?.(-1);
    if (last && ['passed', 'model-failed', 'infra-failed'].includes(last.id) && last.status === 'done') return last.id;
  }
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

function flowNodeSvg(node, selectedNode, traceStrength, lang = 'de') {
  const isVisibleTrace = traceStrength > 0;
  const isSelected = node.id === selectedNode?.id;
  const hasLiveRing = isSelected && node.status === 'active';
  const ring = !hasLiveRing ? '' : node.shape === 'diamond'
    ? `<path class="ctox-flow-node-live-ring" d="M 0 ${-NODE_HEIGHT / 2 - 8} L ${NODE_WIDTH / 2 + 10} 0 L 0 ${NODE_HEIGHT / 2 + 8} L ${-NODE_WIDTH / 2 - 10} 0 Z"></path>`
    : `<rect class="ctox-flow-node-live-ring" x="${-NODE_WIDTH / 2 - 9}" y="${-NODE_HEIGHT / 2 - 9}" width="${NODE_WIDTH + 18}" height="${NODE_HEIGHT + 18}" rx="16"></rect>`;
  const shape = node.shape === 'diamond'
    ? `<path class="ctox-flow-node-diamond" d="M 0 ${-NODE_HEIGHT / 2} L ${NODE_WIDTH / 2} 0 L 0 ${NODE_HEIGHT / 2} L ${-NODE_WIDTH / 2} 0 Z"></path>`
    : `<rect class="ctox-flow-node-box" x="${-NODE_WIDTH / 2}" y="${-NODE_HEIGHT / 2}" width="${NODE_WIDTH}" height="${NODE_HEIGHT}" rx="12"></rect>`;
  return `
    <g class="ctox-flow-node-g is-${escapeAttr(node.status)} ${isVisibleTrace ? 'is-observed is-trace' : 'is-possible'} ${isSelected ? 'is-current is-selected' : ''}"
       data-node-id="${escapeAttr(node.id)}" data-context-record-id="${escapeAttr(node.id)}" data-context-record-type="ctox_flow_node" data-context-label="${escapeAttr(node.label)}" role="button" style="--trace-strength:${traceStrength}" tabindex="0" transform="translate(${node.x} ${node.y})">
      <title>${escapeHtml(`${node.phase}: ${node.label}\n${metricsLabel(node, lang)}\n${node.lines.join('\n')}`)}</title>
      ${ring}
      ${shape}
      <text class="ctox-flow-node-phase" x="${-NODE_WIDTH / 2 + 10}" y="${-NODE_HEIGHT / 2 + 16}">${escapeHtml(node.phase)}</text>
      <text class="ctox-flow-node-title" x="${-NODE_WIDTH / 2 + 10}" y="${-NODE_HEIGHT / 2 + 34}">
        ${wrapSvgText(node.label).map((line, index) => `<tspan x="${-NODE_WIDTH / 2 + 10}" dy="${index === 0 ? 0 : 15}">${escapeHtml(line)}</tspan>`).join('')}
      </text>
      <text class="ctox-flow-node-metrics" x="${-NODE_WIDTH / 2 + 10}" y="${NODE_HEIGHT / 2 - 8}">${escapeHtml(metricsLabel(node, lang))}</text>
    </g>
  `;
}

function buildHarnessModel(data, flow, lang = 'de') {
  const tasks = applyHarnessFlowStatus(buildTaskList(data), flow)
    .filter(isTaskOverviewItemVisible);
  const activeTask = tasks.find((task) => normalizeCommandStatus(task.status) === 'running') || null;
  const activeRun = data.runs.find((run) => run.status === 'running') || null;
  const liveWork = Boolean(activeTask || activeRun);
  const displayFlow = shouldDisplayHarnessFlow(flow, tasks) ? flow : emptyHarnessFlow('no_live_work');
  const observedIds = observedPathFromFlow(displayFlow);
  const observedIdSet = new Set(observedIds);
  const tracePosition = new Map(observedIds.map((id, index) => [id, index]));
  const activeTraceIndex = Math.max(0, observedIds.length - 1);
  const activeNodeId = liveWork ? (observedIds.at(-1) || 'running') : (observedIds.at(-1) || 'queued');
  const activeIndex = Math.max(0, observedIds.lastIndexOf(activeNodeId));
  const detailByNode = observedDetailsFromFlow(displayFlow, lang);
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
    inboundChannels: buildInboundChannels(tasks),
    recentTasks: buildRecentTasks(data),
    queueNow: data.queue.filter((item) => ['queued', 'running', 'leased', 'pending'].includes(item.status) || item.priority === 'urgent'),
    reviewItems: data.communications.filter((item) => item.status === 'review' || item.status === 'drafting'),
    blockedTickets: data.tickets.filter((ticket) => ticket.status === 'blocked' || ticket.status === 'review' || ticket.status === 'running'),
    openTickets: data.tickets.filter((ticket) => ticket.status !== 'done'),
  };
}

function shouldDisplayHarnessFlow(flowResult, tasks) {
  if (!flowResult?.ok) return true;
  const observedIds = observedPathFromFlow(flowResult);
  const lastNode = observedIds.at(-1) || '';
  if (!['passed', 'model-failed', 'infra-failed'].includes(lastNode)) return true;
  const source = flowResult?.flow?.source || {};
  const ids = new Set([source.message_key, source.work_id].filter(Boolean));
  if (!ids.size) return false;
  return tasks.some((task) => ids.has(task.id) || ids.has(task.taskId) || ids.has(task.commandId) || ids.has(task.runId));
}

function applyHarnessFlowStatus(tasks, flowResult) {
  const source = flowResult?.flow?.source || {};
  const ids = new Set([source.message_key, source.work_id].filter(Boolean));
  if (!ids.size) return tasks;
  const observedIds = observedPathFromFlow(flowResult);
  const terminalNode = observedIds.findLast?.((id) => ['passed', 'model-failed', 'infra-failed'].includes(id))
    || [...observedIds].reverse().find((id) => ['passed', 'model-failed', 'infra-failed'].includes(id));
  if (!terminalNode) return tasks;
  const status = terminalNode === 'passed' ? 'completed' : 'failed';
  const summary = terminalSummaryFromFlow(flowResult) || (terminalNode === 'passed' ? 'Completed by CTOX harness' : 'CTOX harness marked this queue item failed');
  return tasks.map((task) => {
    if (!ids.has(task.id) && !ids.has(task.taskId) && !ids.has(task.commandId) && !ids.has(task.runId)) return task;
    return {
      ...task,
      status,
      routeStatus: status,
      resultSummary: task.resultSummary || summary,
      summary: task.summary || summary,
    };
  });
}

function terminalSummaryFromFlow(flowResult) {
  const lines = [];
  for (const block of flowResult?.flow?.blocks || []) {
    for (const branch of block.branches || []) {
      const id = branchToNodeId(branch.kind, branch.title || '', branch.lines || []);
      if (['passed', 'model-failed', 'infra-failed'].includes(id)) {
        lines.push(...(branch.lines || []));
      }
    }
  }
  return cleanUiCopy(lines.join(' · ')).slice(0, 280);
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

function buildInboundChannels(tasks) {
  const channels = new Map();
  for (const item of tasks || []) addInboundChannel(channels, item);
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
  const focused = model.tasks.find((task) => isFocusedTask(task, focusTask));
  if (focused) return focused.id;
  if (previousId && model.tasks.some((task) => task.id === previousId)) return previousId;
  const groups = taskGroups(model.tasks);
  return (groups.current[0] || groups.waiting[0] || groups.blocked[0] || groups.done[0] || model.tasks[0]).id;
}

function reconcileSelection(state) {
  const previousTaskId = state.selectedTaskId;
  const previousStepIndex = state.selectedStepIndex;
  state.selectedTaskId = resolveSelectedTaskId(state.model, state.focusTask, state.selectedTaskId);
  if (state.selectedNodeId && !state.model?.nodeMap?.has?.(state.selectedNodeId)) state.selectedNodeId = '';
  const selectedTaskChanged = previousTaskId !== state.selectedTaskId;
  if (state.userNavigatedTimeline && !selectedTaskChanged && Number.isFinite(previousStepIndex)) {
    state.selectedStepIndex = clampIndex(previousStepIndex, state.model?.timeline?.length || 1);
    const task = getSelectedTask(state);
    const steps = taskSteps(task, state);
    state.selectedTaskStepIndex = clampMetric(state.selectedTaskStepIndex || 0, 0, Math.max(steps.length - 1, 0));
    return;
  }
  state.selectedStepIndex = timelineIndexForSelectedTask(state) ?? focusedTimelineIndex(state.model, state.focusTask);
  state.selectedTaskStepIndex = activeTaskStepIndex(getSelectedTask(state), state);
}

function getSelectedTask(state) {
  return state.model?.tasks?.find((task) => task.id === state.selectedTaskId) || null;
}

function getFocusedTask(state) {
  return state.model?.tasks?.find((task) => isFocusedTask(task, state.focusTask)) || null;
}

function openFocusedTaskDrawer(state) {
  const task = getFocusedTask(state);
  if (!task) return false;
  state.selectedTaskId = task.id;
  state.selectedNodeId = '';
  state.userNavigatedTimeline = false;
  const nextIndex = timelineIndexForSelectedTask(state);
  if (nextIndex !== null) state.selectedStepIndex = nextIndex;
  state.selectedTaskStepIndex = activeTaskStepIndex(task, state);
  state.detailDrawer = { type: 'task', taskId: task.id };
  state.focusTaskOpenDrawer = false;
  return true;
}

function timelineIndexForSelectedTask(state) {
  const task = getSelectedTask(state);
  if (!task) return null;
  const steps = taskSteps(task, state);
  const current = steps.find((step) => step.active) || steps.at(-1);
  return current ? current.timelineIndex : null;
}

function activeTaskStepIndex(task, state) {
  if (!task) return 0;
  const steps = taskSteps(task, state);
  return Math.max(0, steps.findIndex((step) => step.active));
}

function selectTask(state, taskId, options = {}) {
  if (!taskId) return;
  state.selectedTaskId = taskId;
  state.selectedNodeId = '';
  state.userNavigatedTimeline = false;
  const task = getSelectedTask(state);
  const nextIndex = timelineIndexForSelectedTask(state);
  if (nextIndex !== null) state.selectedStepIndex = nextIndex;
  state.selectedTaskStepIndex = activeTaskStepIndex(task, state);
  if (options.drawer) state.detailDrawer = { type: 'task', taskId };
  // Selection is an in-place class flip across the existing task rows (no list
  // rebuild); the flow canvas / drawer may re-render on selection.
  applyTaskSelection(state);
  renderMain(state);
  if (options.center !== false) centerSelectedNode(state);
  syncDetailDrawer(state);
}

function setTimelineStep(state, nextIndex, options = {}) {
  state.selectedNodeId = '';
  state.selectedStepIndex = clampIndex(nextIndex, state.model?.timeline?.length || 1);
  state.userNavigatedTimeline = true;
  renderMain(state);
  if (options.center) centerSelectedNode(state);
  syncDetailDrawer(state);
}

function setTaskTimelineStep(state, nextIndex, options = {}) {
  const task = getSelectedTask(state);
  if (!task) return;
  const steps = taskSteps(task, state);
  state.selectedNodeId = '';
  state.selectedTaskStepIndex = clampMetric(nextIndex, 0, Math.max(steps.length - 1, 0));
  state.userNavigatedTimeline = true;
  renderMain(state);
  if (options.center) centerSelectedNode(state);
  syncDetailDrawer(state);
}

function selectFlowNode(state, nodeId, options = {}) {
  if (!nodeId || !state.model?.nodeMap?.has?.(nodeId)) return;
  const nextIndex = findLastTimelineIndex(state.model.timeline, nodeId);
  state.selectedNodeId = nodeId;
  state.selectedStepIndex = nextIndex;
  state.userNavigatedTimeline = true;
  const task = getSelectedTask(state);
  if (task) {
    const steps = taskSteps(task, state);
    const stepIndex = steps.findIndex((step) => step.id === nodeId);
    if (stepIndex >= 0) state.selectedTaskStepIndex = stepIndex;
  }
  if (options.drawer) state.detailDrawer = { type: 'node', nodeId };
  renderMain(state);
  if (options.center !== false) centerSelectedNode(state);
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
    const node = state.model?.nodeMap?.get(state.detailDrawer.nodeId)
      || state.model?.timeline?.[clampIndex(state.selectedStepIndex, state.model.timeline.length)];
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
  const selectedTaskStepIndex = clampMetric(state.selectedTaskStepIndex || 0, 0, Math.max(steps.length - 1, 0));
  const displayTitle = taskDisplayTitle(task, state);
  const titleField = taskFieldDisplay(task.title || '', state);
  const promptField = taskPromptDisplay(task, state);
  const summary = taskDetailText(itemSummary(task) || '', state);
  const resultSummaryText = taskDetailText(task.resultSummary || '', state);
  const target = displayPathLike(task.target || task.commandId || task.taskId || '');
  const sourceLine = [
    displayWorkSource(task.source || task.moduleId || 'ctox'),
    formatShortTimestamp(task.createdAt || task.startedAt || task.timestamp),
  ].filter(Boolean).join(' · ');
  const showSummary = summary && summary !== task.target && summary !== task.commandId && summary !== task.taskId;
  const body = document.createElement('div');
  body.className = 'drawer-body ctox-task-drawer';
  body.setAttribute('data-context-record-id', task.id);
  body.setAttribute('data-context-record-type', 'ctox_task');
  body.setAttribute('data-context-label', displayTitle);
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <span class="ctox-pane-kicker">${escapeHtml(t.taskDetail)}</span>
        <h2>${escapeHtml(displayTitle)}</h2>
        <small>${escapeHtml(sourceLine)}</small>
      </div>
      <button class="ctox-pane-icon ctox-drawer-close" type="button" data-close-ctox-drawer aria-label="Schließen" title="Schließen">${actionIcon(state, 'close')}</button>
    </header>
    <section class="ctox-callout is-info ctox-task-status-strip">
      <div>
        <strong class="ctox-badge ${statusBadgeVariant(statusClass(task.status))}">${escapeHtml(displayStatus(task.status, state.lang))}</strong>
        ${target ? `<small>${escapeHtml(target)}</small>` : ''}
      </div>
      ${taskLiveStatusMarkup(task, state)}
    </section>
    <form class="ctox-card ctox-task-edit" data-ctox-task-edit>
      <header>
        <div class="ctox-task-edit-heading">
          <span>${escapeHtml(t.editTask)}</span>
          ${canModifyCtoxApp(state) ? '' : `<small>${escapeHtml(t.chefAdminOnly)}</small>`}
        </div>
        <div class="ctox-pane-actions">
          ${canResumeCtoxTask(task) ? `<button type="button" class="ctox-pane-icon" data-ctox-task-resume aria-label="${escapeAttr(t.resumeTask)}" title="${escapeAttr(t.resumeTask)}" ${state.ctx?.commandBus?.dispatch ? '' : 'disabled'}>${actionIcon(state, 'play')}</button>` : ''}
          <button type="button" class="ctox-pane-icon" data-ctox-task-delete aria-label="${escapeAttr(t.deleteTask)}" title="${escapeAttr(t.deleteTask)}" ${canModifyCtoxApp(state) ? '' : 'disabled'}>${actionIcon(state, 'trash')}</button>
        </div>
      </header>
      <div class="ctox-card-body">
        <label class="ctox-task-edit-field">
          <span class="ctox-field-label">${escapeHtml(t.taskTitle)}</span>
          <input class="ctox-input" type="text" name="${titleField.redacted ? 'titleDisplay' : 'title'}" value="${escapeAttr(titleField.text)}" ${canModifyCtoxApp(state) && !titleField.redacted ? '' : 'disabled'}>
          ${titleField.redacted ? `<small>${escapeHtml(t.redactedTechnicalDetail)}</small>` : ''}
        </label>
        <label class="ctox-task-edit-field">
          <span class="ctox-field-label">${escapeHtml(t.taskPrompt)}</span>
          <textarea class="ctox-textarea" name="${promptField.redacted ? 'promptDisplay' : 'prompt'}" rows="4" ${canModifyCtoxApp(state) && !promptField.redacted ? '' : 'disabled'}>${escapeHtml(promptField.text)}</textarea>
          ${promptField.redacted ? `<small>${escapeHtml(t.taskPromptRedacted)}</small>` : ''}
        </label>
        <label class="ctox-task-edit-field">
          <span class="ctox-field-label">${escapeHtml(t.priority)}</span>
          <select class="ctox-select" name="priority" ${canModifyCtoxApp(state) ? '' : 'disabled'}>
            ${['urgent', 'high', 'normal', 'low'].map((priority) => `<option value="${priority}" ${String(task.priority || 'normal') === priority ? 'selected' : ''}>${escapeHtml(displayPriority(priority))}</option>`).join('')}
          </select>
        </label>
      </div>
      <footer class="ctox-task-edit-footer">
        <button type="submit" class="ctox-button is-primary" ${canModifyCtoxApp(state) ? '' : 'disabled'}>${escapeHtml(t.saveTask)}</button>
        <small data-ctox-task-action-status></small>
      </footer>
    </form>
    ${showSummary ? `
      <section class="ctox-card">
        <header>${escapeHtml(t.summary)}</header>
        <div class="ctox-card-body">
          <p>${escapeHtml(summary)}</p>
        </div>
      </section>
    ` : ''}
    ${resultSummaryText ? `
      <section class="ctox-card">
        <header>${escapeHtml(t.evidence)}</header>
        <div class="ctox-card-body">
          <p>${escapeHtml(resultSummaryText)}</p>
        </div>
      </section>
    ` : ''}
    <section class="ctox-drawer-timeline">
      <header>
        <h3>${escapeHtml(t.timeline)}</h3>
        <small>${escapeHtml(`${steps.length} ${t.taskSteps}`)}</small>
      </header>
      <div class="ctox-drawer-steps">
        ${steps.map((step, index) => `
          <button type="button" class="${index === selectedTaskStepIndex ? 'is-current' : ''}" data-drawer-task-step="${index}" data-context-record-id="${escapeAttr(`${task.id}:${step.id || index}`)}" data-context-record-type="ctox_task_step" data-context-label="${escapeAttr(step.label)}">
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
  body.querySelector('[data-ctox-task-edit]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    await saveCtoxTaskFromDrawer(state, task, event.currentTarget);
  });
  body.querySelector('[data-ctox-task-delete]')?.addEventListener('click', async () => {
    await deleteCtoxTaskFromDrawer(state, task, body);
  });
  body.querySelector('[data-ctox-task-resume]')?.addEventListener('click', async () => {
    await resumeCtoxTaskFromDrawer(state, task, body);
  });
  body.querySelectorAll('[data-drawer-task-step]').forEach((button) => {
    button.addEventListener('click', () => {
      setTaskTimelineStep(state, Number(button.dataset.drawerTaskStep), { center: true });
    });
  });
  return body;
}

function canResumeCtoxTask(task) {
  return ['blocked', 'failed', 'cancelled', 'canceled', 'completed', 'done', 'handled']
    .includes(normalizeCommandStatus(task?.status));
}

async function resumeCtoxTaskFromDrawer(state, task, body) {
  const t = labels[state.lang];
  const status = body.querySelector('[data-ctox-task-action-status]');
  const button = body.querySelector('[data-ctox-task-resume]');
  const sourceTaskId = nativeTaskId(task);
  if (!sourceTaskId || !state.ctx?.commandBus?.dispatch) {
    if (status) status.textContent = t.taskActionFailed;
    return;
  }
  button?.setAttribute('disabled', 'disabled');
  if (status) status.textContent = '';
  try {
    const title = taskDisplayTitle(task, state);
    const commandId = `cmd_ctox_task_resume_${crypto.randomUUID()}`;
    await state.ctx.commandBus.dispatch({
      id: commandId,
      module: 'ctox',
      command_type: 'business_os.chat.task',
      record_id: sourceTaskId,
      payload: {
        title: `${t.resumeTask}: ${title}`,
        instruction: `Continue the durable CTOX work from source task ${sourceTaskId}. Preserve its prior evidence and resolve the remaining or retryable work.`,
        source_task_id: sourceTaskId,
        source_command_id: task.commandId || '',
        continuation: true,
      },
      client_context: {
        source_module: 'ctox',
        command_path: 'ctox_task_resume_follow_up',
        source_task_id: sourceTaskId,
        source_command_id: task.commandId || '',
        actor: state.ctx.session?.user || {},
      },
    }, { until: 'accepted' });
    if (status) status.textContent = t.taskResumed;
    refresh(state).catch(() => {});
  } catch (error) {
    if (status) status.textContent = humanTaskActionError(error, t);
  } finally {
    button?.removeAttribute('disabled');
  }
}

async function saveCtoxTaskFromDrawer(state, task, form) {
  const t = labels[state.lang];
  const status = form.querySelector('[data-ctox-task-action-status]');
  const submit = form.querySelector('button[type="submit"]');
  const formData = new FormData(form);
  const titleControl = form.elements.title;
  const promptControl = form.elements.prompt;
  const payload = {
    task_id: nativeTaskId(task),
    title: titleControl && !titleControl.disabled
      ? String(formData.get('title') || '').trim()
      : String(task.title || '').trim(),
    prompt: promptControl && !promptControl.disabled
      ? String(formData.get('prompt') || '').trim()
      : String(task.prompt || '').trim(),
    priority: String(formData.get('priority') || 'normal').trim(),
  };
  if (!payload.task_id) {
    if (status) status.textContent = t.taskActionFailed;
    return;
  }
  submit?.setAttribute('disabled', 'disabled');
  if (status) status.textContent = '';
  try {
    await dispatchCtoxTaskMutation(state, {
      commandType: 'ctox.task.update',
      payload,
      commandPath: 'ctox_task_update',
    });
    applyTaskMutationToModel(state, task.id, payload);
    if (status) status.textContent = t.taskSaved;
    render(state);
    syncDetailDrawer(state);
  } catch (error) {
    if (status) status.textContent = humanTaskActionError(error, t);
  } finally {
    submit?.removeAttribute('disabled');
  }
}

async function deleteCtoxTaskFromDrawer(state, task, body) {
  const t = labels[state.lang];
  const confirmed = await showBusinessConfirm(t.deleteTaskConfirm, {
    title: 'Task löschen',
    confirmLabel: 'Löschen',
  });
  if (!confirmed) return;
  const status = body.querySelector('[data-ctox-task-action-status]');
  const button = body.querySelector('[data-ctox-task-delete]');
  const payload = {
    task_id: nativeTaskId(task),
    command_id: task.commandId || '',
  };
  if (!payload.task_id) {
    if (status) status.textContent = t.taskActionFailed;
    return;
  }
  button?.setAttribute('disabled', 'disabled');
  if (status) status.textContent = '';
  try {
    await dispatchCtoxTaskMutation(state, {
      commandType: 'ctox.task.delete',
      payload,
      commandPath: 'ctox_task_delete',
    });
    removeTaskFromModel(state, task.id);
    state.detailDrawer = null;
    state.selectedTaskId = null;
    state.ctx.closeDrawers?.();
    render(state);
    refresh(state).catch(() => {});
  } catch (error) {
    if (status) status.textContent = humanTaskActionError(error, t);
  } finally {
    button?.removeAttribute('disabled');
  }
}

async function dispatchCtoxTaskMutation(state, { commandType, payload, commandPath }) {
  if (!state.ctx?.commandBus?.dispatch) {
    throw new Error('RxDB command bus is not available');
  }
  const commandId = `cmd_${commandType.replace(/[^a-z0-9]+/gi, '_')}_${crypto.randomUUID()}`;
  return state.ctx.commandBus.dispatch({
    id: commandId,
    module: 'ctox',
    command_type: commandType,
    record_id: payload.task_id || '',
    inbound_channel: 'business_os.ctox',
    payload,
    client_context: {
      source_module: 'ctox',
      command_path: commandPath,
      actor: state.ctx.session?.user || {},
    },
  });
}

function nativeTaskId(task) {
  return String(task?.taskId || task?.id || '').replace(/^queue-/, '').trim();
}

function applyTaskMutationToModel(state, taskId, patch) {
  const tasks = state.model?.tasks || [];
  const index = tasks.findIndex((item) => item.id === taskId);
  if (index < 0) return;
  const next = {
    ...tasks[index],
    title: patch.title || tasks[index].title,
    prompt: patch.prompt ?? tasks[index].prompt,
    priority: patch.priority || tasks[index].priority,
    status: patch.status || tasks[index].status,
    routeStatus: patch.route_status || tasks[index].routeStatus,
  };
  tasks.splice(index, 1, next);
}

function removeTaskFromModel(state, taskId) {
  if (!state.model?.tasks) return;
  state.model.tasks = state.model.tasks.filter((item) => item.id !== taskId);
}

function humanTaskActionError(error, t) {
  const message = String(error?.message || error || '');
  if (message.includes('403') || /chef|admin/i.test(message)) return t.chefAdminOnly;
  return t.taskActionFailed;
}

function flowNodeDrawer(node, task, state) {
  const t = labels[state.lang];
  const body = document.createElement('div');
  body.className = 'drawer-body ctox-task-drawer ctox-node-drawer';
  body.setAttribute('data-context-record-id', node.id);
  body.setAttribute('data-context-record-type', 'ctox_flow_node');
  body.setAttribute('data-context-label', node.label);
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <span class="ctox-pane-kicker">${escapeHtml(t.stationDetail)}</span>
        <h2>${escapeHtml(node.label)}</h2>
      </div>
      <button class="ctox-pane-icon ctox-drawer-close" type="button" data-close-ctox-drawer aria-label="Schließen" title="Schließen">${actionIcon(state, 'close')}</button>
    </header>
    <section class="ctox-card">
      <div class="ctox-card-body">
        <dl class="ctox-fields">
          ${nodeLiveFactMarkup(node, task, state)}
          <dt>${escapeHtml(t.currentStep)}</dt><dd>${escapeHtml(node.phase || '')}</dd>
          <dt>${escapeHtml(t.status)}</dt><dd>${escapeHtml(displayStatus(node.status, state.lang))}</dd>
          <dt>${escapeHtml(t.taskDetail)}</dt><dd>${escapeHtml(task?.title || t.noRecentWork)}</dd>
          <dt>${escapeHtml(t.measurements)}</dt><dd>${escapeHtml(metricsLabel(node, state.lang))}</dd>
        </dl>
      </div>
    </section>
    <section class="ctox-card">
      <header>${escapeHtml(t.summary)}</header>
      <div class="ctox-card-body">
        ${(node.lines || []).map((line) => `<p>${escapeHtml(line)}</p>`).join('') || `<p>${escapeHtml(t.noRecentWork)}</p>`}
      </div>
    </section>
    ${node.tools?.length ? `
      <section class="ctox-card">
        <header>${escapeHtml(t.tools)}</header>
        <div class="ctox-card-body">
          <div class="ctox-node-tools">
            ${node.tools.map((tool) => `<span class="ctox-badge">${escapeHtml(tool)}</span>`).join('')}
          </div>
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
  const taskIndex = clampMetric(state.selectedTaskStepIndex || 0, 0, Math.max(steps.length - 1, 0));
  const index = state.userNavigatedTimeline ? taskIndex : (byTimeline >= 0 ? byTimeline : Math.max(0, activeIndex));
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
  const flow = flowResult?.flow || emptyHarnessFlow().flow;
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

function observedDetailsFromFlow(flowResult, lang = 'de') {
  const flow = flowResult?.flow || emptyHarnessFlow().flow;
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
    const metadata = parseMetadata(event.metadata_json);
    const metrics = firstExplicitMetrics([event, metadata]);
    const eventLine = workerEventLabel(event, metadata, lang);
    const eventTool = readString(metadata?.tool || {}, ['name']);
    const lines = id === 'running'
      ? [...(existing?.lines || []), eventLine].filter(Boolean).slice(-5)
      : (existing?.lines?.length ? existing.lines : [eventLine, cleanUiCopy(event.body_text)].filter(Boolean));
    const tools = [...(existing?.tools || []), eventTool].filter(Boolean);
    map.set(id, {
      inputTokens: metrics?.inputTokens ?? existing?.inputTokens ?? null,
      outputTokens: metrics?.outputTokens ?? existing?.outputTokens ?? null,
      toolCalls: metrics?.toolCalls ?? existing?.toolCalls ?? null,
      seconds: metrics?.seconds ?? existing?.seconds ?? 0,
      timestamp: event.created_at || firstTimestamp([event, metadata]) || existing?.timestamp || '',
      lines,
      tools: [...new Set(tools)].slice(-6),
    });
  }
  return map;
}

function workerEventLabel(event, metadata, lang = 'de') {
  const t = labels[lang] || labels.en;
  const kind = String(event?.event_kind || '');
  const toolName = readString(metadata?.tool || {}, ['name']);
  if (kind === 'worker.tool_started') return `${t.toolStarted}: ${toolName || t.tools}`;
  if (kind === 'worker.tool_completed') return `${t.toolFinished}: ${toolName || t.tools}`;
  if (kind === 'worker.token_usage') return t.modelUsageUpdated;
  if (kind === 'worker.turn_started') return t.agentWorking;
  if (kind === 'worker.turn_completed') return t.agentCompleted;
  if (kind === 'worker.turn_timeout') return t.agentTimeout;
  if (kind === 'worker.phase') {
    return metadata?.phase === 'invoke-model' ? t.agentWorking : t.agentPreparing;
  }
  return cleanUiCopy(event?.title || event?.body_text || '');
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
    // Curved loop back
    return `M ${x1} ${y1} C ${x1} ${y1 + offset * 0.7}, ${x2} ${midY + offset * 0.3}, ${x2} ${y2}`;
  }
  if (route === 'up' || route === 'down') {
    const offset = route === 'up' ? -54 : 54;
    const midY = Math.max(36, Math.min(FLOW_HEIGHT - 36, (from.y + to.y) / 2 + offset));
    return `M ${x1} ${y1} C ${x1} ${midY}, ${x2} ${midY}, ${x2} ${y2}`;
  }
  if (Math.abs(x2 - x1) < 1 || Math.abs(y2 - y1) < 1) return `M ${x1} ${y1} L ${x2} ${y2}`;

  // Normal horizontal / vertical curve
  const dx = x2 - x1;
  const controlOffset = Math.max(36, Math.min(120, Math.abs(dx) * 0.5));
  if (horizontal) {
    return `M ${x1} ${y1} C ${x1 + (to.x >= from.x ? controlOffset : -controlOffset)} ${y1}, ${x2 - (to.x >= from.x ? controlOffset : -controlOffset)} ${y2}, ${x2} ${y2}`;
  } else {
    return `M ${x1} ${y1} C ${x1} ${y1 + (to.y >= from.y ? controlOffset : -controlOffset)}, ${x2} ${y2 - (to.y >= from.y ? controlOffset : -controlOffset)}, ${x2} ${y2}`;
  }
}

function mergeBundleWithCommands(bundle, commands, queueTasks = [], bugReports = []) {
  const commandQueue = commands
    .filter((doc) => doc.command_type === 'browser.capture.extract' || doc.result?.extract)
    .map((doc) => {
      const extractArtifact = browserExtractArtifactFromCommand(doc);
      return {
        id: `command-${doc.command_id || doc.id}`,
        taskId: doc.task_id || '',
        commandId: doc.command_id || doc.id || '',
        title: doc.payload?.title || `Browser Extract: ${extractArtifact.source_id || extractArtifact.capture_script || doc.command_id || doc.id}`,
        prompt: doc.payload?.instruction || '',
        source: doc.module || doc.payload?.source_module || 'browser',
        channel: inferInboundChannel(doc),
        priority: doc.payload?.priority || 'normal',
        status: normalizeCommandStatus(doc.status),
        routeStatus: doc.task_status || doc.status || '',
        target: doc.command_type || 'browser.capture.extract',
        browserExtractArtifact: extractArtifact,
        result: doc.result || null,
        resultSummary: browserExtractSummary(extractArtifact.fields) || resultSummary(doc.result),
        createdAt: new Date(doc.created_at_ms || doc.updated_at_ms || Date.now()).toISOString(),
        updatedAt: new Date(doc.updated_at_ms || Date.now()).toISOString(),
      };
    })
    .filter((item) => item.id && item.browserExtractArtifact?.kind === 'browser_extract');
  const runtimeQueue = queueTasks.map((doc) => ({
    id: doc.id || doc.task_id || doc.command_id,
    taskId: doc.task_id || doc.id || '',
    commandId: doc.command_id || '',
    title: doc.title || doc.command_type || doc.id || 'CTOX queue task',
    prompt: doc.prompt || doc.payload?.prompt || doc.payload?.instruction || '',
    source: doc.source_module || doc.module || 'ctox',
    channel: inferInboundChannel(doc),
    priority: doc.priority || 'normal',
    status: normalizeCommandStatus(doc.status || doc.task_status || doc.route_status),
    routeStatus: doc.route_status || '',
    target: doc.command_type || doc.thread_key || 'ctox queue',
    browserContextArtifact: doc.browser_context_artifact || null,
    result: doc.result || null,
    resultSummary: resultSummary(doc.result),
    createdAt: new Date(doc.updated_at_ms || Date.now()).toISOString(),
    updatedAt: new Date(doc.updated_at_ms || Date.now()).toISOString(),
  })).filter((item) => item.id);
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
    queue: mergeById([...runtimeQueue, ...commandQueue], bundle.queue)
      .filter(isQueueOverviewItemVisible),
    tickets: mergeById(tickets, bundle.tickets),
  };
}

function isQueueOverviewItemVisible(item) {
  return isTaskOverviewItemVisible(item);
}

function isTaskOverviewItemVisible(item) {
  const statuses = taskStatusCandidates(item);
  // Queue documents can retain an old primary `queued` status while route/task
  // evidence has already reached a terminal failure. Terminal evidence wins so
  // stale red cards do not stay in the live pipeline forever.
  if (statuses.some((status) => HARNESS_PROBLEM_TERMINAL_STATUSES.has(status))) return false;
  if (statuses.some((status) => HARNESS_WAITING_STATUSES.has(status) || HARNESS_ACTIVE_STATUSES.has(status))) return true;
  if (statuses.some((status) => HARNESS_SUCCESS_STATUSES.has(status))) return true;
  if (item?.priority === 'urgent') return true;
  return !statuses.some((status) => HARNESS_TERMINAL_STATUSES.has(status));
}

function taskStatusCandidates(item = {}) {
  return [
    item.status,
    item.task_status,
    item.routeStatus,
    item.route_status,
    item.result?.status,
    item.result?.task_status,
  ].map(normalizeCommandStatus).filter(Boolean);
}

function browserExtractArtifactFromCommand(doc = {}) {
  const result = doc.result && typeof doc.result === 'object' ? doc.result : {};
  const extract = result.extract && typeof result.extract === 'object' ? result.extract : {};
  const payload = doc.payload && typeof doc.payload === 'object' ? doc.payload : {};
  return {
    kind: 'browser_extract',
    schema_version: 1,
    stream: result.stream || 'rxdb',
    command_id: doc.command_id || doc.id || '',
    source_id: extract.sourceId || extract.source_id || payload.source_id || '',
    capture_script: result.capture_script || extract.captureScript || extract.capture_script || payload.capture_script || '',
    status: result.status || doc.status || '',
    fields: extract.fields && typeof extract.fields === 'object' ? extract.fields : {},
    url: extract.url || '',
    title: extract.title || '',
    secret_value_in_payload: false,
    frame_data_in_payload: false,
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
    return normalizeFocusTask(parsed);
  } catch {}
  return null;
}

function normalizeFocusTask(value) {
  if (!value || typeof value !== 'object') return null;
  const taskId = String(value.taskId || value.task_id || '').trim();
  const commandId = String(value.commandId || value.command_id || '').trim();
  if (!taskId && !commandId) return null;
  return {
    taskId,
    commandId,
    taskStatus: String(value.taskStatus || value.task_status || value.status || '').trim(),
    sourceModule: String(value.sourceModule || value.source_module || value.source || 'business-os').trim() || 'business-os',
    openDrawer: Boolean(value.openDrawer || value.open_drawer || value.drawer === '1' || value.drawer === true),
  };
}

function persistFocusTask(focusTask) {
  const normalized = normalizeFocusTask(focusTask);
  if (!normalized) return null;
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(normalized));
  } catch {}
  return normalized;
}

function readFocusTaskFromHash() {
  const query = String(location.hash || '').split('?')[1] || '';
  if (!query) return null;
  const params = new URLSearchParams(query);
  const taskId = params.get('task_id') || params.get('taskId') || '';
  const commandId = params.get('command_id') || params.get('commandId') || '';
  if (!taskId && !commandId) return null;
  return normalizeFocusTask({
    taskId,
    commandId,
    taskStatus: params.get('task_status') || params.get('status') || '',
    sourceModule: params.get('source') || 'matching',
    openDrawer: params.get('drawer') === '1' || params.get('open') === 'drawer',
  });
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
  if (value === 'accepted' || value === 'pending') return 'queued';
  if (value === 'leased' || value === 'working') return 'running';
  if (value === 'done') return 'completed';
  if (value === 'handled') return 'handled';
  if (value === 'cancelled' || value === 'canceled') return 'cancelled';
  if (value === 'blocked' || value === 'stale_missing_native') return 'blocked';
  if (['failed', 'fail', 'error', 'errored', 'model_failed', 'model-failed', 'infra_failed', 'infra-failed'].includes(value)) return 'failed';
  return value || 'queued';
}

function routeStatusNodeId(status) {
  const value = String(status || '').toLowerCase();
  if (value === 'accepted' || value === 'pending' || value === 'queued') return 'queued';
  if (value === 'leased') return 'leased';
  if (value === 'running' || value === 'working') return 'running';
  if (value === 'completed' || value === 'done' || value === 'handled') return 'passed';
  if (value === 'failed' || value === 'cancelled' || value === 'canceled' || value === 'blocked' || value === 'stale_missing_native') return 'model-failed';
  return '';
}

async function loadLocalCommands(ctx) {
  return (await loadLocalCollection(ctx, 'business_commands')).filter((doc) => !isInternalSmokeDoc(doc));
}

async function loadLocalQueueTasks(ctx) {
  return (await loadLocalCollection(ctx, 'ctox_queue_tasks')).filter((doc) => !isInternalSmokeDoc(doc));
}

async function loadLocalBugReports(ctx) {
  return loadLocalCollection(ctx, 'ctox_bug_reports');
}

async function loadHarnessFlowSnapshot(ctx) {
  try {
    const collection = ctoxCollection(ctx, 'ctox_runtime_settings');
    if (!collection) return emptyHarnessFlow('rxdb_flow_projection_unavailable');
    const doc = await collection.findOne('runtime-settings').exec();
    const runtimeSettings = doc?.toJSON?.() || null;
    return runtimeSettings?.harness_flow
      || runtimeSettings?.harnessFlow
      || emptyHarnessFlow('rxdb_flow_projection_unavailable');
  } catch (error) {
    if (isVolatileLocalRxDbError(error)) return emptyHarnessFlow('rxdb_flow_projection_unavailable');
    console.warn('[ctox] harness flow projection unavailable', error);
    return emptyHarnessFlow('rxdb_flow_projection_unavailable');
  }
}

function isVolatileLocalRxDbError(error) {
  const text = String(error?.message || error || '');
  return /QUERY_CANCELLED|replication-cancel|WebRTC replication cancelled|IDBDatabase.*closing|database connection is closing|collection is closed|closed collection|RxDB Error-Code: COL21/i.test(text);
}

async function loadLocalWebStackOverview(ctx) {
  const collection = ctoxCollection(ctx, 'ctox_runtime_settings');
  if (!collection) return { ok: false, error: 'ctox_runtime_settings collection is not available' };
  const doc = await collection.findOne('runtime-settings').exec();
  const runtimeSettings = doc?.toJSON?.() || null;
  const webStack = runtimeSettings?.web_stack || null;
  if (!webStack?.ok) return { ok: false, error: 'Web Stack projection is not available in RxDB' };
  return webStack;
}

function webStackStateFromRefreshResult(previous, data) {
  return {
    loading: false,
    error: data?.ok ? '' : (data?.error || 'Web Stack status unavailable'),
    notice: previous?.notice || '',
    data: data?.ok ? data : previous?.data,
  };
}

async function refreshWebStackPanel(state) {
  try {
    const data = await loadLocalWebStackOverview(state.ctx);
    state.webStack = webStackStateFromRefreshResult(state.webStack, data);
  } catch (error) {
    state.webStack = {
      ...(state.webStack || {}),
      loading: false,
      error: error.message || String(error),
    };
  }
  renderMain(state);
}

async function verifyWebStackCredential(state, sourceId, secretName) {
  const source = (state.webStack?.data?.sources || []).find((candidate) => candidate.id === sourceId);
  const configured = Boolean(source?.credential?.configured);
  state.webStack = {
    ...(state.webStack || {}),
    loading: false,
    error: '',
    notice: configured
      ? `${secretName || sourceId}: Credential ist im CTOX Secret Store vorhanden.`
      : `${secretName || sourceId}: Credential fehlt im CTOX Secret Store. Hinterlegen bleibt aus Datenschutzgründen außerhalb von RxDB.`,
  };
  renderMain(state);
}

async function requestWebStackAuthAssist(state, source) {
  const t = labels[state.lang];
  if (!state.ctx?.commandBus?.dispatch) {
    state.webStack = { ...(state.webStack || {}), error: 'RxDB command bus is not available' };
    renderMain(state);
    return;
  }
  const now = Date.now();
  const sourceId = source?.id || '';
  const sourceSlug = sourceId.replace(/[^a-z0-9]+/gi, '_').replace(/^_+|_+$/g, '').toLowerCase() || 'source';
  const commandId = `web_stack_auth_assist_${now}_${Math.random().toString(36).slice(2, 10)}`;
  const host = String(sourceId || '').replace(/^https?:\/\//, '').split('/')[0];
  const browserAssist = source?.browser_assist || {};
  const targetUrl = browserAssist.target_url || (host ? `https://${host}` : 'https://example.com');
  const allowedDomains = Array.isArray(browserAssist.allowed_domains) && browserAssist.allowed_domains.length
    ? browserAssist.allowed_domains
    : [host, ...(source?.host_suffixes || [])].filter(Boolean);
  await state.ctx.commandBus.dispatch({
    id: commandId,
    module: 'ctox',
    command_type: 'web_stack.auth_assist.request',
    record_id: sourceId,
    inbound_channel: 'business_os.ctox.web_stack',
    payload: {
      session_id: `browser_session_web_stack_auth_${sourceSlug}`,
      tab_id: `browser_tab_web_stack_auth_${sourceSlug}`,
      source_id: sourceId,
      secret_name: source?.credential?.secret_name || '',
      target_url: targetUrl,
      allowed_domains: allowedDomains,
      verify_selector: browserAssist.verify_selector || '',
      credential_selector: browserAssist.credential_selector || '',
      capture_script: browserAssist.capture_script || '',
      purpose: 'web_stack_auth',
      expires_at_ms: now + 30 * 60 * 1000,
      browser_stream: 'rxdb',
      secret_value_in_rxdb: false,
    },
    client_context: {
      source_module: 'ctox',
      command_path: 'web_stack_auth_assist',
      actor: state.ctx.session?.user || {},
    },
  });
  state.webStack = { ...(state.webStack || {}), error: '', notice: t.webStackAuthQueued };
  renderMain(state);
}

async function loadLocalCollection(ctx, collectionName) {
  const collection = ctoxCollection(ctx, collectionName);
  if (!collection) return [];
  const query = collection.find();
  const previewQuery = typeof query?.limit === 'function' ? query.limit(200) : query;
  const localDocs = await previewQuery.exec();
  return localDocs
    .map((doc) => doc.toJSON())
    .sort((left, right) => (right.updated_at_ms || 0) - (left.updated_at_ms || 0))
    .slice(0, 20);
}

function ctoxCollection(ctx, collectionName) {
  return ctx?.db?.collection?.(collectionName) || null;
}

function isInternalSmokeDoc(doc) {
  return doc?.command_type === 'business_os.smoke'
    || doc?.client_context?.source === 'rxdb-smoke'
    || doc?.payload?.client_context?.source === 'rxdb-smoke'
    || doc?.payload?.title === 'WebRTC command smoke'
    || doc?.title === 'WebRTC command smoke';
}

function emptyHarnessFlow(error = '') {
  return {
    ok: false,
    mode: 'unavailable',
    error,
    ascii: '',
    flow: {
      schema_version: 1,
      source: { message_key: null, work_id: null, source_kind: 'unavailable' },
      ledger_events: [],
      blocks: [],
    },
  };
}

function canModifyCtoxApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function wireShellMessages(state) {
  const applyLanguage = (lang) => {
    const nextLang = lang === 'en' ? 'en' : 'de';
    loadCtoxMessages(nextLang).then(() => {
      state.lang = nextLang;
      // Rebuild the (localized) column chrome once, then take the normal render
      // path. buildTaskColumn clears the wired-marker so the shell re-wires.
      buildTaskColumn(state);
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
  const focusHandler = (event) => {
    const focusTask = persistFocusTask(event.detail);
    if (!focusTask) return;
    state.focusTask = focusTask;
    state.focusTaskOpenDrawer = focusTask.openDrawer;
    if (!state.model) return;
    reconcileSelection(state);
    openFocusedTaskDrawer(state);
    render(state);
    centerSelectedNode(state);
    syncDetailDrawer(state);
  };
  window.addEventListener('message', messageHandler);
  window.addEventListener('ctox-business-os-preferences', preferenceHandler);
  window.addEventListener('ctox-business-os-focus-task', focusHandler);
  return () => {
    window.removeEventListener('message', messageHandler);
    window.removeEventListener('ctox-business-os-preferences', preferenceHandler);
    window.removeEventListener('ctox-business-os-focus-task', focusHandler);
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
    const state = scroller.closest('[data-ctox-harness]')?.__ctoxState;
    if (!state) return;
    const previousZoom = state.zoom;
    const nextZoom = state.zoom + (event.deltaY < 0 ? 0.12 : -0.12);
    setFlowZoom(state, nextZoom);
    if (state.zoom === previousZoom) return;
    state.flowViewport = {
      left: Math.max(0, (scroller.scrollLeft + event.offsetX) * (state.zoom / previousZoom) - event.offsetX),
      top: Math.max(0, (scroller.scrollTop + event.offsetY) * (state.zoom / previousZoom) - event.offsetY),
    };
    renderMain(state);
  }, { passive: false });
}

function zoomFlowFromControl(state, action) {
  const scroller = state.ctx.host.querySelector('[data-flow-canvas]');
  const previousZoom = state.zoom;
  const nextZoom = action === 'reset'
    ? DEFAULT_ZOOM
    : state.zoom + (action === '+' ? 0.12 : -0.12);
  setFlowZoom(state, nextZoom);
  if (state.zoom === previousZoom) return;
  const viewport = readFlowViewport(state);
  if (scroller) {
    const anchorX = scroller.clientWidth / 2;
    const anchorY = scroller.clientHeight / 2;
    const ratio = state.zoom / previousZoom;
    state.flowViewport = {
      left: Math.max(0, (viewport.left + anchorX) * ratio - anchorX),
      top: Math.max(0, (viewport.top + anchorY) * ratio - anchorY),
    };
  }
  renderMain(state);
}

function setFlowZoom(state, value) {
  state.zoom = clampMetric(Math.round(value * 100) / 100, MIN_ZOOM, MAX_ZOOM);
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
  const selectedTask = getSelectedTask(state);
  const node = (state.selectedNodeId ? state.model?.nodeMap?.get(state.selectedNodeId) : null)
    || selectedTaskStepView(selectedTask, state)?.node
    || state.model.timeline[clampIndex(state.selectedStepIndex, state.model.timeline.length)];
  const scroller = state.ctx.host.querySelector('[data-flow-canvas]');
  if (!node || !scroller) return;
  requestAnimationFrame(() => {
    const left = Math.max(0, node.x * state.zoom - scroller.clientWidth / 2);
    const top = Math.max(0, selectedNodeVisualY(node, selectedTask, state) * state.zoom - scroller.clientHeight / 2);
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

function metricSubjectTask(state) {
  const activeTask = state?.model?.activeTask;
  if (taskOwnsCurrentHarnessMetrics(activeTask, state)) return activeTask;
  return null;
}

function taskOwnsCurrentHarnessMetrics(task, state) {
  if (!task || !taskMatchesHarnessFlow(task, state)) return false;
  const status = normalizeCommandStatus(task.routeStatus || task.status);
  return status === 'running';
}

function isLiveMetricSubject(task, state) {
  if (!task || !state?.model?.activeTask) return false;
  return task.id === state.model.activeTask.id
    && taskMatchesHarnessFlow(task, state)
    && normalizeCommandStatus(task.status) === 'running';
}

function flowSourceView(state) {
  const t = labels[state.lang];
  if (state.flow?.ok === false && state.ctx?.sync?.mode === 'webrtc') {
    return {
      mode: state.runtimeStatus || displayFlowMode('rxdb-webrtc'),
      status: t.flowProjectionMissing,
    };
  }
  // Suppress the placeholder "Unavailable / unavailable" pair: when no flow data
  // is available, show the CTOX core mode with a clear "not live" status instead
  // of leaking the raw 'unavailable' enum value into the UI.
  const rawMode = state.flow?.mode || 'ctox_core';
  const mode = rawMode === 'unavailable' ? displayFlowMode('ctox_core') : displayFlowMode(rawMode);
  return {
    mode,
    status: state.flow?.ok ? t.connected : t.notLive,
  };
}

function isHarnessLive(state) {
  const activeTask = state?.model?.activeTask;
  return Boolean(state?.flow?.ok && isLiveMetricSubject(activeTask, state));
}

function liveStatusMarkup(state, options = {}) {
  const t = labels[state.lang];
  const classes = ['ctox-live-chip'];
  if (options.compact) classes.push('is-compact');
  if (state.flow?.ok === false) classes.push('is-unavailable');
  return `
    <span class="${classes.join(' ')}">
      <i aria-hidden="true"></i>
      <span>${escapeHtml(state.flow?.ok === false ? t.notLive : t.live)}</span>
      <strong data-live-elapsed>${escapeHtml(formatMetricValue(liveElapsedSeconds(state), 'seconds', state.lang))}</strong>
    </span>
  `;
}

function taskLiveStatusMarkup(task, state) {
  const status = normalizeCommandStatus(task?.status);
  if (status !== 'running' || task?.id !== state.model?.activeTask?.id) return '';
  if (!isHarnessLive(state)) return '';
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
  return `<dt>${escapeHtml(t.live)}</dt><dd>${liveStatusMarkup(state, { compact: true })}</dd>`;
}

function aggregateFlowMetrics(flowResult) {
  const metrics = { inputTokens: null, outputTokens: null, toolCalls: null, seconds: null };
  const add = (candidate, cumulative = false) => {
    if (!candidate) return;
    const merge = (current, next) => cumulative ? Math.max(current || 0, next) : (current || 0) + next;
    if (candidate.inputTokens !== null) metrics.inputTokens = merge(metrics.inputTokens, candidate.inputTokens);
    if (candidate.outputTokens !== null) metrics.outputTokens = merge(metrics.outputTokens, candidate.outputTokens);
    if (candidate.toolCalls !== null && candidate.toolCalls !== undefined) metrics.toolCalls = merge(metrics.toolCalls, candidate.toolCalls);
    if (candidate.seconds !== null && candidate.seconds !== undefined) metrics.seconds = Math.max(metrics.seconds || 0, candidate.seconds);
  };
  const flow = flowResult?.flow || {};
  for (const event of flow.ledger_events || []) {
    const metadata = parseMetadata(event.metadata_json);
    add(
      firstExplicitMetrics([event, metadata]),
      metadata?.metrics_mode === 'cumulative',
    );
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
  return String(mode || 'unavailable').replace(/[_-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
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
  if (kind === 'guard') return guardBranchNode({ title, lines });
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
  if (/\b(current queue state|reload status):\s*(failed|cancelled|canceled|blocked)\b/.test(text) || /\b(direct session timeout|queue error|failed)\b/.test(text)) return 'model-failed';
  if (/\b(current queue state|reload status):\s*(handled|completed|done|passed)\b/.test(text)) return 'passed';
  if (/\b(current queue state|reload status):\s*(leased|working|running)\b/.test(text) || /\b(leased by|lease time)\b/.test(text)) return 'leased';
  return null;
}

function guardBranchNode(branch) {
  const text = branchText(branch);
  if (/\baccepted:\s*(leased|running|pending|queued)\s*->\s*failed\b/.test(text)) return 'model-failed';
  if (/\baccepted:\s*(leased|running|pending|queued)\s*->\s*(handled|completed|passed|done)\b/.test(text)) return 'passed';
  if (/\baccepted:\s*.*->\s*(infrafailed|infra failed)\b/.test(text)) return 'infra-failed';
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
  if (value.includes('worker.turn_timeout')) return 'model-failed';
  if (value.includes('worker.')) return 'running';
  if (value.includes('work.outcome') && /\b(success|succeeded|completed|done|passed)\b/.test(value)) return 'passed';
  if (value.includes('work.outcome') && /\b(failed|failure|error|blocked)\b/.test(value)) return 'model-failed';
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
  return null;
}

function taskDisplayTitle(task, state) {
  return safeTaskDisplayText(itemTitle(task), state.lang, {
    fallback: nativeTaskId(task) || 'CTOX task',
    max: 120,
  });
}

function taskFieldDisplay(value, state) {
  const text = String(value || '').trim();
  const redacted = hasSensitiveUiLeak(text);
  return {
    redacted,
    text: redacted
      ? labels[state.lang]?.redactedTechnicalDetail || labels.en.redactedTechnicalDetail
      : cleanUiCopy(text),
  };
}

function taskPromptDisplay(task, state) {
  const text = String(task?.prompt || task?.summary || '').trim();
  const redacted = hasSensitiveUiLeak(text);
  return {
    redacted,
    text: redacted
      ? labels[state.lang]?.redactedTechnicalDetail || labels.en.redactedTechnicalDetail
      : cleanUiCopy(text),
  };
}

function taskDetailText(value, state) {
  return safeTaskDisplayText(value, state.lang, { max: 280 });
}

function safeTaskDisplayText(value, lang = 'de', options = {}) {
  const text = String(value || '').trim();
  const fallback = options.fallback || '';
  if (!text) return fallback;
  if (hasSensitiveUiLeak(text)) {
    return labels[lang]?.redactedTechnicalDetail || labels.en.redactedTechnicalDetail;
  }
  return clip(cleanUiCopy(text).replace(/\s+/g, ' ').trim(), options.max || 180) || fallback;
}

function hasSensitiveUiLeak(value) {
  const text = String(value || '');
  if (!text.trim()) return false;
  const lower = text.toLowerCase();
  return [
    /```/,
    /<\/?(script|style|html|body|pre|code|div|span|table|iframe)\b/i,
    /(?:^|\n)\s*(?:import|export|function|class|const|let|var)\s+[A-Za-z_$]/,
    /(?:^|\n)\s*(?:async\s+)?(?:function\s*)?\([^)]*\)\s*=>/,
    /(?:^|\n)\s*[.#]?[A-Za-z0-9_-]+\s*\{[^}]*:[^}]*\}/,
    /\b(?:TypeError|ReferenceError|SyntaxError|RangeError|Stack trace)\b/,
    /\bat\s+.+:\d+:\d+\)?/,
    /\b(?:api[_-]?key|access[_-]?token|refresh[_-]?token|secret|password|credential|authorization)\b/i,
    /\bbearer\s+[A-Za-z0-9._~+/=-]{12,}/i,
    /\b(?:web_stack|browser_context|frame_data|capture_script|secret_value_in_payload|ctox_runtime_settings)\b/i,
  ].some((pattern) => pattern.test(text))
    || (lower.includes('web stack') && /\b(secret|credential|capture|source|extract|frame|payload)\b/i.test(text));
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
  status = normalizeCommandStatus(status);
  if (['done', 'completed', 'sent', 'approved', 'healthy'].includes(status)) return 'tone-ok';
  if (['running', 'review', 'drafting', 'leased', 'queued'].includes(status)) return 'tone-running';
  if (['blocked', 'failed', 'fail'].includes(status)) return 'tone-blocked';
  return 'tone-warning';
}

function displayWorkSource(source) {
  return String(source || 'ctox')
    .replace(/^ctox[-_\s]*/i, 'CTOX ')
    .trim()
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
  status = normalizeCommandStatus(status);
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

function moduleAssetUrl(relativePath) {
  const asset = new URL(relativePath, import.meta.url);
  const scriptVersion = new URL(import.meta.url).searchParams.get('v') || CTOX_STYLE_BUILD;
  asset.searchParams.set('v', scriptVersion);
  return asset;
}

async function loadModuleMarkup() {
  return fetch(moduleAssetUrl('./index.html')).then((response) => response.text());
}

async function ensureStyles() {
  const asset = moduleAssetUrl('./index.css');
  const href = `${asset.pathname}${asset.search}`;
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

export const __ctoxTestHooks = {
  aggregateFlowMetrics,
  clampMetric,
  deriveHarnessHealth,
  eventToNodeId,
  flowSourceView,
  formatRelativeAge,
  friendlyWebStackStatus,
  labels,
  progressPercent,
  safeTaskDisplayText,
  setFlowZoom,
  taskSteps,
  timelinePanel,
  observedDetailsFromFlow,
  webStackStateFromRefreshResult,
  webStackProjectionMissing,
  normalizeFocusTask,
  resolveSelectedTaskId,
  compactTaskFlowRow,
  filterAndSortTasks,
  taskColumnMarkup,
  taskListInner,
  renderTaskList,
  applyTaskSelection,
  webStackPanel,
  taskPipelineStage,
};
