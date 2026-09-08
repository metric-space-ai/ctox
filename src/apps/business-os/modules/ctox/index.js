import { showBusinessAlert, showBusinessConfirm } from '../../shared/dialogs.js?v=20260816-browser-sync-guards-v141';
import { renderListOrState } from '../../shared/list-state.js';
import { crewCreatureHtml, syncCrewProceduralMotion, crewMemberExpression, crewMemberExpressionTtlMs } from '../../shared/business-chat.js?v=20260909-shell-v2-crew-compact-v357';
import { canUseBusinessPermission, BusinessOsPermissions } from '../../shared/permissions.js?v=20260816-browser-sync-guards-v141';
import { workspaceDataState } from './data-state.js?v=20260906-data-state-v1';

const FLOW_WIDTH = 1760;
const FLOW_HEIGHT = 1050;
const NODE_WIDTH = 136;
const NODE_HEIGHT = 76;
const DEFAULT_ZOOM = 1;
const MIN_ZOOM = 0.72;
const MAX_ZOOM = 1.8;
const HARNESS_EVENT_LIMIT = 200;
const LOCAL_COLLECTION_LIMIT = 120;
const LOCAL_RENDER_DEBOUNCE_MS = 80;
const HARNESS_STALL_GRACE_MS = 90 * 1000;
const HARNESS_WAITING_STATUSES = new Set(['queued', 'pending', 'accepted']);
const HARNESS_ACTIVE_STATUSES = new Set(['running', 'leased', 'review', 'drafting']);
const HARNESS_TERMINAL_STATUSES = new Set(['completed', 'done', 'sent', 'approved', 'healthy', 'handled', 'cancelled', 'failed', 'blocked']);
const HARNESS_SUCCESS_STATUSES = new Set(['completed', 'done', 'sent', 'approved', 'healthy']);
const HARNESS_PROBLEM_TERMINAL_STATUSES = new Set(['handled', 'cancelled', 'failed', 'blocked']);
const CTOX_STYLE_BUILD = '20260909-shell-v2-crew-compact-v357';
// Replicated collections whose rows feed the task list (via
// mergeBundleWithCommands). The data-driven empty branch is gated on their
// combined readiness so an initial sync never reads as "no work".
const TASK_SOURCE_COLLECTIONS = ['ctox_queue_tasks', 'business_commands', 'ctox_bug_reports'];

const labels = {
  de: {
    now: 'Jetzt',
    loadingRuntime: "Crew wird geladen",
    loadingRuntimeDetail: "Aufgaben und Fortschritt werden aktualisiert.",
    live: 'Live',
    tasks: "Aufgaben",
    newestFirst: 'neueste zuerst',
    taskSteps: 'Zwischenschritte',
    selectedTask: "Ausgewählte Aufgabe",
    inboundChannels: "Eingangskanäle",
    inboundItems: 'Eingänge',
    inboundEndpoint: "Aufgabeneingang",
    outboundEndpoint: "Aufgabenabschluss",
    openOutcome: 'Abschluss offen',
    unprovenOutcome: 'Abschluss nicht belegt',
    taskDetail: "Aufgabendetails",
    editTask: "Aufgabe bearbeiten",
    taskTitle: 'Titel',
    taskPrompt: "Auftrag",
    saveTask: 'Speichern',
    resumeTask: 'Als Folgeauftrag fortsetzen',
    deleteTask: 'Löschen',
    deleteTaskConfirm: "Diese Aufgabe wirklich löschen?",
    taskSaved: "Aufgabe gespeichert.",
    taskResumed: 'Folgeauftrag angelegt.',
    taskDeleted: "Aufgabe gelöscht.",
    taskActionFailed: 'Aktion fehlgeschlagen.',
    memoryTitle: "Gedächtnis",
    knowledge: "Was es weiß",
    experience: "Was es erlebt hat",
    editMemory: "Bearbeiten",
    saveMemory: "Speichern",
    cancelEdit: "Abbrechen",
    confirmAnchor: "Bestätigen",
    memoryConfirmed: "bestätigt",
    memoryHypothesis: "Hypothese",
    memoryEmpty: "Noch nichts. Entsteht mit den ersten Einsätzen.",
    noDomain: "noch ohne Fachgebiet",
    assignmentsWord: "Einsätze",
    newMember: "Neues Mitglied",
    archiveMember: "Archivieren",
    createMember: "Anlegen",
    memberCreated: "Angelegt.",
    shapeLabel: "Form",
    colorLabel: "Farbe",
    shape_round: "rund",
    shape_blob: "Tropfen",
    shape_square: "eckig",
    shape_triangle: "Dreieck",
    retryLoad: "Erneut laden",
    syncDisconnected: "Verbindung unterbrochen – die Anzeige kann veraltet sein",
    openInChat: "Im Chat öffnen",
    zoomOut: "Verkleinern",
    zoomIn: "Vergrößern",
    flowControls: "Flussdiagramm-Steuerung",
    activityTimeline: "Aktivitätsverlauf",
    selectActivityEvent: "Ereignis wählen",
    noEventDetail: "Zu diesem Ereignis liegt noch kein Detail vor.",
    flowDiagram: "Arbeitsablauf der Crew",
    laneCommunication: "Abstimmung",
    laneQueue: "Aufgaben und Bearbeitung",
    laneEvidence: "Nachweisprüfung",
    crewHome: "Crew zu Hause",
    atHome: "zu Hause",
    restingAfterFailure: "erholt sich nach einem Fehlschlag",
    readingMemory: "liest sein Gedächtnis",
    memoryLinePlaceholder: "Ein Satz je Zeile: was dieses Wesen sicher weiß.",
    outcomeExecutionError: "Fehler bei der Ausführung",
    outcomeCompleted: "erledigt",
    outcomeReviewRejected: "Prüfung nicht bestanden",
    notPermittedForRole: "für deine Rolle nicht freigegeben",
    noLiveMetrics: "keine Live-Messwerte",
    noPlanYet: "noch kein Plan",
    sourceUnavailable: "Quelle nicht verbunden",
    retryAt: "Wiederholung um",
    entryOne: "Eintrag",
    learningFromAssignment: "lernt aus dem Einsatz",
    noCrewMember: "ohne Crew-Zuordnung",
    close: "Schließen",
    memberName: "Name",
    soul: "Seele",
    voice: "Stimme",
    voiceHint: "Ein Satz, in dem das Mitglied spricht.",
    sketch: "Skizze",
    specialties: "Spezialitäten",
    specialtiesHint: "kommagetrennt",
    spec_modules: "Apps",
    spec_command_types: "Auftragsarten",
    spec_skills: "Fähigkeiten",
    spec_tags: "Stichwörter",
    cv: "Lebenslauf",
    tasksTotal: "Einsätze",
    succeededCount: "gelungen",
    failedCount: "gescheitert",
    reviewPassedCount: "Prüfung bestanden",
    reviewRejectedCount: "Prüfung nicht bestanden",
    avgElapsed: "Ø Dauer",
    lastActive: "zuletzt aktiv",
    timesheet: "Stundenzettel",
    noRuns: "Noch keine Einsätze.",
    saveMember: "Speichern",
    memberSaved: "Gespeichert.",
    tokensWord: "Tokens",
    toolsWord: "Werkzeuge",
    axisThorough: "Gründlich",
    axisFast: "Schnell",
    axisCareful: "Vorsichtig",
    axisBold: "Mutig",
    axisTerse: "Knapp",
    axisThoroughText: "Ausführlich",
    axisByTheBook: "Regeltreu",
    axisCreative: "Kreativ",
    axisAsks: "Fragt nach",
    axisAssumes: "Nimmt an",
    cancelTask: "Abbrechen",
    blockTask: "Blockieren",
    releaseTask: "Freigeben",
    retryTask: "Wiederholen",
    assignTask: "Zuweisen",
    assignChoose: "Mitglied wählen",
    cancelReasonDefault: "Vom Nutzer abgebrochen",
    blockReasonDefault: "Vom Nutzer blockiert",
    pauseReasonDefault: "Vom Nutzer pausiert",
    controlApplied: "Übernommen.",
    harnessRunning: "Läuft",
    harnessPaused: "Pausiert",
    harnessStopped: "Gestoppt",
    onDuty: "im Einsatz",
    capacity: "Kapazität",
    countWaiting: "warten",
    countWorking: "im Einsatz",
    countBlocked: "blockiert",
    pressureActive: "Hohe Auslastung",
    pauseHarness: "Crew pausieren",
    resumeHarness: "Crew weiterarbeiten lassen",
    holdTechnical: "technischer Grund",
    holdMissingReviewEvidence: "Prüfergebnis fehlt",
    holdMissingArtifact: "Arbeitsergebnis fehlt",
    holdWaitingExternal: "wartet auf eine Rückmeldung",
    holdAbortedByOwner: "vom Nutzer abgebrochen",
    holdOther: "blockiert",
    failureRetryable: "wiederholbar",
    failureTerminal: "endgültig",
    failedWord: "Gescheitert",
    blockedWord: "Blockiert",
    waitsFor: "wartet auf",
    retryAt: "Wiederholung um",
    retryPending: "Wiederholung steht aus",
    attemptOne: "Versuch",
    attemptMany: "Versuche",
    attemptLabel: "Versuch",
    worksSince: "arbeitet seit",
    worksOn: "arbeitet daran",
    leasedSince: "übernommen um",
    leasedWord: "übernommen",
    assignedTo: "zugewiesen an",
    crewMember: "Crew-Mitglied",
    leaseOwner: "Übernommen von",
    until: "bis",
    chefAdminOnly: "Nur Verantwortliche und Administratoren dürfen Aufgaben ändern.",
    currentStep: 'Aktuelle Station',
    source: 'Quelle',
    status: 'Status',
    created: 'Angelegt',
    summary: 'Zusammenfassung',
    evidence: "Nachweise",
    stationDetail: 'Stationsdetails',
    tools: 'Werkzeuge',
    openTaskDetail: "Details anzeigen",
    liveFlow: "Crew im Einsatz",
    doingNow: "Was die Crew gerade tut",
    measurements: 'Messung',
    inputTokens: "Gelesene Tokens",
    outputTokens: "Erzeugte Tokens",
    toolCalls: "Werkzeugeinsätze",
    reasoningTurns: "Denkschritte",
    elapsed: 'Zeit',
    notCaptured: 'nicht erfasst',
    executionProgress: 'Fortschritt',
    step: 'Schritt',
    activityTurnSingular: "Arbeitsschritt",
    activityTurnPlural: "Arbeitsschritte",
    executionPhases: {
      work: 'Bearbeitung',
      working: 'Bearbeitung',
      plan: 'Planung',
      planning: 'Planung',
      review: 'Prüfung',
      validation: 'Nachweis',
      validating: 'Nachweis',
      rework: 'Nacharbeit',
      completed: 'Abgeschlossen',
      done: 'Abgeschlossen',
    },
    agentPreparing: "Crew bereitet sich vor",
    agentWorking: "Crew arbeitet",
    agentCompleted: "Arbeitsschritt abgeschlossen",
    agentTimeout: "Zeitlimit für diesen Arbeitsschritt erreicht",
    modelUsageUpdated: "Textverbrauch aktualisiert",
    toolStarted: 'Werkzeug gestartet',
    toolFinished: 'Werkzeug abgeschlossen',
    connected: 'verbunden',
    notLive: 'nicht live',
    notLogged: "Zeit nicht erfasst",
    timeline: "Verlauf",
    queue: "Aufgabenliste",
    active: 'aktiv',
    tickets: 'Tickets',
    task: 'Neue Aufgabe',
    instruction: "Auftrag an die Crew",
    priority: 'Priorität',
    send: 'Senden',
    sending: 'Sendet...',
    runtime: "Betrieb",
    model: 'Modell',
    mode: 'Modus',
    context: 'Kontext',
    importTasks: "Aufgaben importieren",
    exportTasks: "Aufgaben exportieren",
    tasksImported: "{count} Aufgaben importiert.",
    taskImportFailed: "Import fehlgeschlagen – die Datei enthält keine lesbaren Aufgaben.",
    noWorkHere: 'Hier liegt gerade keine Arbeit.',
    syncingTasks: "Aufgaben werden aktualisiert.",
    noRecentWork: 'Noch keine aktuelle Arbeit erfasst.',
    noMetrics: "noch keine Verbrauchsdaten",
    routing: "Zuordnung",
    inbound: "Eingang",
    outbound: "Ausgang",
    queued: "Auftrag angelegt",
    webStack: "Internet-Zugänge",
    webStackSources: 'Quellen',
    webStackCredentials: "Zugangsdaten",
    webStackMissing: 'fehlen',
    webStackConfigured: 'konfiguriert',
    webStackSecret: "Zugangsschlüssel",
    webStackCredentialValue: "Zugangsschlüssel",
    webStackSaveCredential: 'Speichern',
    webStackVerifyCredential: 'Prüfen',
    webStackAuthAssist: 'Login im Browser',
    webStackRxdbOnly: "Zugangsdaten werden verschlüsselt gespeichert.",
    webStackLoading: "Internet-Zugänge werden geladen…",
    webStackConnecting: "Verbunden. Die Zugangsdaten werden noch geladen.",
    webStackUnavailable: "Internet-Zugänge sind gerade nicht erreichbar.",
    webStackSyncRequired: 'Verbindung prüfen',
    webStackCheckProjection: "Zugänge neu laden",
    webStackProjectionMissing: "Die Zugangsdaten sind noch nicht vollständig geladen.",
    webStackCredentialSaved: "Zugangsdaten gespeichert.",
    webStackAuthQueued: 'Browser-Login angefordert.',
    webStackRecentCaptures: "Letzte Aufnahmen",
    webStackNoCaptures: "Noch keine Browser-Aufnahmen.",
    webStackRecentExtracts: "Zuletzt gelesene Inhalte",
    webStackNoExtracts: "Noch keine Inhalte gelesen.",
    timelineUnavailable: "Noch kein Verlauf verfügbar",
    timelineUnavailableDetail: "Der Verlauf wird mit dem nächsten Arbeitsschritt verfügbar.",
    flowProjectionMissing: "Verbunden. Der Arbeitsstand wird noch geladen.",
    harnessHealth: "Crew-Status",
    harnessCriticalTitle: "Die Crew arbeitet gerade keine Aufgaben ab",
    harnessCriticalMessage: "Offene Aufgaben: {count}. Längste Wartezeit: {age}.",
    harnessCriticalProjection: "Offene Aufgaben: {count}. Längste Wartezeit: {age}. Der aktuelle Arbeitsstand ist noch nicht verfügbar.",
    harnessWarningTitle: "Aufgaben warten auf die Crew",
    harnessWarningMessage: "Offene Aufgaben: {count}. Noch kein Arbeitsbeginn sichtbar.",
    harnessOpenTask: "Aufgabe öffnen",
    harnessHealthy: "Die Crew bearbeitet die Aufgabenliste",
    auxShow: 'Status & Quellen',
    auxHide: 'Status & Quellen ausblenden',
    harnessKicker: "Crew",
    taskSearch: "Aufgaben suchen",
    showAsList: 'Als Liste anzeigen',
    showAsCards: 'Als Karten anzeigen',
    filters: 'Filter',
    resetFilters: 'Filter zurücksetzen',
    allSources: 'Alle Quellen',
    allTasks: "Alle Aufgaben",
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
    pinTask: "Aufgabe anpinnen",
    unpinTask: 'Pin lösen',
    pinned: 'Angepinnt',
    pipelineQueued: "Wartet",
    pipelineWorking: 'Arbeit',
    pipelineReview: "Prüfung",
    pipelineDone: 'Fertig',
    flowFooterEmpty: "Keine Aufgabe ausgewählt",
  },
  en: {
    now: 'Now',
    loadingRuntime: "Loading the crew",
    loadingRuntimeDetail: "Updating tasks and progress.",
    live: 'Live',
    tasks: 'Tasks',
    newestFirst: 'newest first',
    taskSteps: 'Steps',
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
    saveTask: 'Save',
    resumeTask: 'Continue as follow-up',
    deleteTask: 'Delete',
    deleteTaskConfirm: "Delete this task?",
    taskSaved: 'Task saved.',
    taskResumed: 'Follow-up task queued.',
    taskDeleted: 'Task deleted.',
    taskActionFailed: 'Action failed.',
    memoryTitle: "Memory",
    knowledge: "What it knows",
    experience: "What it has been through",
    editMemory: "Edit",
    saveMemory: "Save",
    cancelEdit: "Cancel",
    confirmAnchor: "Confirm",
    memoryConfirmed: "confirmed by the owner",
    memoryHypothesis: "hypothesis",
    memoryEmpty: "Nothing yet. It grows with the first assignments.",
    noDomain: "no field of work yet",
    assignmentsWord: "assignments",
    newMember: "New member",
    archiveMember: "Archive",
    createMember: "Create",
    memberCreated: "Created.",
    shapeLabel: "Shape",
    colorLabel: "Colour",
    shape_round: "round",
    shape_blob: "blob",
    shape_square: "square",
    shape_triangle: "triangle",
    retryLoad: "Reload",
    syncDisconnected: "Disconnected – the view may be out of date",
    openInChat: "Open in chat",
    zoomOut: "Zoom out",
    zoomIn: "Zoom in",
    flowControls: "Flow chart controls",
    activityTimeline: "Activity timeline",
    selectActivityEvent: "Select activity event",
    noEventDetail: "No detail is available for this event yet.",
    flowDiagram: "Crew workflow",
    laneCommunication: "Coordination",
    laneQueue: "Tasks and work",
    laneEvidence: "Evidence check",
    crewHome: "Crew at home",
    atHome: "at home",
    restingAfterFailure: "recovering after a failure",
    readingMemory: "reading its memory",
    memoryLinePlaceholder: "One sentence per line: what this member knows for sure.",
    outcomeExecutionError: "execution error",
    outcomeCompleted: "done",
    outcomeReviewRejected: "review rejected",
    notPermittedForRole: "not released for your role",
    noLiveMetrics: "no live measurements",
    noPlanYet: "no plan yet",
    sourceUnavailable: "source not connected",
    retryAt: "retry at",
    entryOne: "entry",
    learningFromAssignment: "learning from the assignment",
    noCrewMember: "no crew member",
    close: "Close",
    memberName: "Name",
    soul: "Soul",
    voice: "Voice",
    voiceHint: "One sentence in the member's own words.",
    sketch: "Sketch",
    specialties: "Specialties",
    specialtiesHint: "comma separated",
    spec_modules: "Modules",
    spec_command_types: "Commands",
    spec_skills: "Skills",
    spec_tags: "Tags",
    cv: "Track record",
    tasksTotal: "Assignments",
    succeededCount: "succeeded",
    failedCount: "failed",
    reviewPassedCount: "Review passed",
    reviewRejectedCount: "Review rejected",
    avgElapsed: "Avg. duration",
    lastActive: "last active",
    timesheet: "Timesheet",
    noRuns: "No assignments yet.",
    saveMember: "Save",
    memberSaved: "Saved.",
    tokensWord: "tokens",
    toolsWord: "tools",
    axisThorough: "Thorough",
    axisFast: "Fast",
    axisCareful: "Careful",
    axisBold: "Bold",
    axisTerse: "Terse",
    axisThoroughText: "Detailed",
    axisByTheBook: "By the book",
    axisCreative: "Creative",
    axisAsks: "Asks",
    axisAssumes: "Assumes",
    cancelTask: "Cancel",
    blockTask: "Block",
    releaseTask: "Release",
    retryTask: "Retry",
    assignTask: "Assign",
    assignChoose: "Choose member",
    cancelReasonDefault: "Cancelled by owner",
    blockReasonDefault: "Blocked by owner",
    pauseReasonDefault: "Paused by owner",
    controlApplied: "Applied.",
    harnessRunning: "Running",
    harnessPaused: "Paused",
    harnessStopped: "Stopped",
    onDuty: "on duty",
    capacity: "Capacity",
    countWaiting: "waiting",
    countWorking: "on duty",
    countBlocked: "blocked",
    pressureActive: "High workload",
    pauseHarness: "Pause the crew",
    resumeHarness: "Resume the crew",
    holdTechnical: "technical reason",
    holdMissingReviewEvidence: "review evidence missing",
    holdMissingArtifact: "work result missing",
    holdWaitingExternal: "waiting for external input",
    holdAbortedByOwner: "aborted by owner",
    holdOther: "blocked",
    failureRetryable: "retryable",
    failureTerminal: "final",
    failedWord: "Failed",
    blockedWord: "Blocked",
    waitsFor: "waits for",
    retryAt: "retry at",
    retryPending: "retry pending",
    attemptOne: "attempt",
    attemptMany: "attempts",
    attemptLabel: "Attempt",
    worksSince: "working since",
    worksOn: "is working on it",
    leasedSince: "picked up at",
    leasedWord: "picked up",
    assignedTo: "assigned to",
    crewMember: "Crew member",
    leaseOwner: "Picked up by",
    until: "until",
    chefAdminOnly: 'Only chef or admin can change tasks.',
    currentStep: 'Current station',
    source: 'Source',
    status: 'Status',
    created: 'Created',
    summary: 'Summary',
    evidence: 'Evidence',
    stationDetail: 'Station details',
    tools: 'Tools',
    openTaskDetail: "Show details",
    liveFlow: "Crew at work",
    doingNow: "What the crew is doing",
    measurements: 'Measurements',
    inputTokens: 'Input tokens',
    outputTokens: 'Output tokens',
    toolCalls: 'Tool calls',
    reasoningTurns: 'Reasoning',
    elapsed: 'Time',
    notCaptured: 'not captured',
    executionProgress: 'Progress',
    step: 'Step',
    activityTurnSingular: 'turn',
    activityTurnPlural: 'turns',
    executionPhases: {
      work: 'Model work',
      working: 'Model work',
      plan: 'Planning',
      planning: 'Planning',
      review: 'Review',
      validation: 'Evidence',
      validating: 'Evidence',
      rework: 'Rework',
      completed: 'Completed',
      done: 'Completed',
    },
    agentPreparing: "Crew is preparing",
    agentWorking: "Crew is working",
    agentCompleted: "Work step completed",
    agentTimeout: "Time limit reached for this step",
    modelUsageUpdated: 'Model usage updated',
    toolStarted: 'Tool started',
    toolFinished: 'Tool finished',
    connected: 'connected',
    notLive: 'not live',
    notLogged: 'time not logged',
    timeline: 'Timeline',
    queue: "Task list",
    active: 'active',
    tickets: 'Tickets',
    task: 'New task',
    instruction: "Instructions for the crew",
    priority: 'Priority',
    send: 'Send',
    sending: 'Sending...',
    runtime: "Operation",
    model: 'Model',
    mode: 'Mode',
    context: 'Context',
    importTasks: 'Import tasks',
    exportTasks: 'Export tasks',
    tasksImported: '{count} tasks imported.',
    taskImportFailed: 'Import failed — no importable tasks in the file.',
    noWorkHere: 'No work here right now.',
    syncingTasks: 'Syncing tasks.',
    noRecentWork: 'No recent work recorded yet.',
    noMetrics: 'no live token metrics',
    routing: "Assignment",
    inbound: 'Inbound',
    outbound: 'Outbound',
    queued: "Task created",
    webStack: "Online access",
    webStackSources: 'Sources',
    webStackCredentials: 'Credentials',
    webStackMissing: 'missing',
    webStackConfigured: 'configured',
    webStackSecret: 'Secret',
    webStackCredentialValue: 'Credential value',
    webStackSaveCredential: 'Save',
    webStackVerifyCredential: 'Verify',
    webStackAuthAssist: 'Login in Browser',
    webStackRxdbOnly: "Credentials are stored encrypted.",
    webStackLoading: "Loading online access…",
    webStackConnecting: "Connected. Access details are still loading.",
    webStackUnavailable: "Online access is currently unavailable.",
    webStackSyncRequired: 'Check connection',
    webStackCheckProjection: "Reload access details",
    webStackProjectionMissing: "Access details have not finished loading.",
    webStackCredentialSaved: 'Credential saved.',
    webStackAuthQueued: 'Browser login requested.',
    webStackRecentCaptures: "Recent recordings",
    webStackNoCaptures: "No browser recordings yet.",
    webStackRecentExtracts: "Recently read content",
    webStackNoExtracts: "No content read yet.",
    timelineUnavailable: 'No timeline events available',
    timelineUnavailableDetail: "The timeline becomes available with the next step.",
    flowProjectionMissing: "Connected. Work progress is still loading.",
    harnessHealth: "Crew status",
    harnessCriticalTitle: "The crew is not working on tasks right now",
    harnessCriticalMessage: "Open tasks: {count}. Longest wait: {age}.",
    harnessCriticalProjection: "Open tasks: {count}. Longest wait: {age}. Current progress is not available yet.",
    harnessWarningTitle: "Tasks are waiting for the crew",
    harnessWarningMessage: "Open tasks: {count}. No work has started yet.",
    harnessOpenTask: 'Open task',
    harnessHealthy: "The crew is working through the task list",
    auxShow: 'Status & sources',
    auxHide: 'Hide status & sources',
    harnessKicker: "Crew",
    taskSearch: 'Search tasks',
    showAsList: 'Show as list',
    showAsCards: 'Show as cards',
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

// Owner-facing copy for the flow nodes. The catalog above keeps the machine
// names for tooltips and tests; what the screen shows comes from here.
const FLOW_NODE_COPY = {
  de: {
    queued: ['Aufgabenliste', 'Wartet auf den Start'],
    leased: ['Übernommen', 'Bereit zum Start'],
    running: ['Arbeit', 'Arbeitet'],
    'awaiting-review': ['Prüfung', 'Bereit zur Prüfung'],
    'review-queued': ['Prüfung', 'Prüfung steht aus'],
    reviewing: ['Prüfung', 'Wird geprüft'],
    'review-passed': ['Prüfung', 'Prüfung bestanden'],
    'review-rejected': ['Prüfung', 'Prüfung nicht bestanden'],
    'review-unavailable': ['Prüfung', 'Prüfung nicht verfügbar'],
    'review-retry': ['Prüfung', 'Prüfung wiederholen'],
    'rework-required': ['Nacharbeit', 'Nacharbeit nötig'],
    'awaiting-validation': ['Nachweis', 'Nachweis nötig'],
    validating: ['Nachweis', 'Nachweis wird geprüft'],
    passed: ['Fertig', 'Nachweis bestätigt'],
    'model-failed': ['Gescheitert', 'Arbeit gescheitert'],
    'infra-failed': ['Gescheitert', 'Dienst gescheitert'],
  },
  en: {
    queued: ['Task list', 'Waiting to start'],
    leased: ['Picked up', 'Ready to start'],
    running: ['Work', 'Working'],
    'awaiting-review': ['Review', 'Ready for review'],
    'review-queued': ['Review', 'Review waiting'],
    reviewing: ['Review', 'Under review'],
    'review-passed': ['Review', 'Review passed'],
    'review-rejected': ['Review', 'Review failed'],
    'review-unavailable': ['Review', 'Review unavailable'],
    'review-retry': ['Review', 'Retry review'],
    'rework-required': ['Rework', 'Rework needed'],
    'awaiting-validation': ['Evidence', 'Needs evidence'],
    validating: ['Evidence', 'Checking evidence'],
    passed: ['Done', 'Evidence confirmed'],
    'model-failed': ['Failed', 'Work failed'],
    'infra-failed': ['Failed', 'Service failed'],
  },
};

function flowNodeCopy(node, lang) {
  const copy = FLOW_NODE_COPY[lang === 'en' ? 'en' : 'de'][node.id] || FLOW_NODE_COPY.en[node.id];
  return copy ? { phase: copy[0], label: copy[1] } : { phase: node.phase, label: node.label };
}

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
  if (!ctx?.host) {
    throw new Error('CTOX mount requires ctx.host');
  }
  await ensureStyles();
  // Markup is the durable workspace shell. Everything after this point is
  // fail-soft: the windowed Business OS shell replaces the whole host with a
  // recovery dialog on any thrown mount error, so secondary wiring (readiness,
  // realtime, i18n) must never take the app down once the harness is visible.
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
    dataLoaded: false,
    dataError: '',
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
    // Epoch ms of a real persisted execution start, or null when nothing is
    // measurably running. Only a finite anchor may drive the live clock.
    liveAnchorMs: null,
    liveTicker: null,
    localSubscriptionCleanup: null,
    interactionGuardCleanup: null,
    // Selected task's own projections (events, runs) and the server flow blob.
    selectedLive: null,
    blobFlow: null,
    bundle: null,
    mainInteracting: false,
    timelineScrubbing: false,
    mainRenderPending: false,
    rerenderAfterRefresh: false,
    focusTaskConsumed: false,
    realtimeCollectionCount: 0,
    readinessCleanup: null,
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
  let teardownShellMessages = () => {};
  try {
    teardownShellMessages = wireShellMessages(state);
    state.layoutResizeCleanup = wireColumnResize(state);
    await loadCtoxMessages(state.lang);
    renderLoading(state);
    startLiveTicker(state);
    state.localSubscriptionCleanup = wireLocalRealtime(state);
    state.readinessCleanup = wireTaskSourceReadiness(state);
    state.interactionGuardCleanup = wireMainInteractionGuard(state);
    // A cold RxDB/WebRTC lease must not block the OS window from becoming
    // operable. Hydrate in the background while the compact loading workspace is
    // already visible; from then on RxDB change subscriptions drive every render
    // (no poll loop). A failed first read is shown, never swallowed.
    void renderFromLocalCache(state).catch((error) => {
      if (!state.disposed) console.warn('[ctox] initial local render failed', error);
    });
  } catch (error) {
    // Keep the markup host and a usable teardown. The shell recovery dialog is
    // worse than a loading/partial harness for transient wiring failures.
    if (!state.disposed) console.warn('[ctox] mount wiring failed; keeping harness shell', error);
    try {
      showDataError(state, error);
    } catch {}
  }
  return () => {
    state.disposed = true;
    window.clearInterval(state.liveTicker);
    window.clearTimeout(state.expressionRefresh);
    try { state.localSubscriptionCleanup?.(); } catch {}
    try { state.interactionGuardCleanup?.(); } catch {}
    try { state.readinessCleanup?.(); } catch {}
    try { state.layoutResizeCleanup?.(); } catch {}
    if (harness) delete harness.__ctoxState;
    try { teardownShellMessages(); } catch {}
  };
}

async function loadCtoxMessages(lang) {
  const language = lang === 'en' ? 'en' : 'de';
  const response = await fetch(moduleAssetUrl(`./locales/${language}.json`));
  if (!response.ok) throw new Error(`CTOX locale: HTTP ${response.status}`);
  labels[language] = { ...labels[language], ...await response.json() };
}

async function renderFromLocalCache(state) {
  if (state.disposed) return;
  if (state.refreshInFlight) {
    state.rerenderAfterRefresh = true;
    return;
  }
  state.refreshInFlight = true;
  try {
    await hydrateFromLocal(state);
    state.dataError = '';
  } catch (error) {
    if (state.disposed) return;
    // Keep the last successful model, selection and task list; the footer
    // (or the empty workspace) names the failure and offers a retry.
    showDataError(state, error);
    throw error;
  } finally {
    state.refreshInFlight = false;
  }
  if (state.rerenderAfterRefresh && !state.disposed) {
    state.rerenderAfterRefresh = false;
    window.setTimeout(() => {
      renderFromLocalCache(state).catch((error) => {
        if (!state.disposed) console.warn('[ctox] follow-up render failed', error);
      });
    }, LOCAL_RENDER_DEBOUNCE_MS);
  }
}

// One load path. Every collection is a bounded RxDB read; there is no HTTP
// fallback and no poll loop — subscriptions (wireLocalRealtime) call this.
async function hydrateFromLocal(state) {
  // The two task sources fail loudly: a failed read must never look like an
  // idle harness (showDataError keeps the last good model). Secondary sources
  // degrade quietly.
  const [commands, queueTasks, bugReports, webStack, blobFlow, crewMembers, harnessStatus, channelAccounts] = await Promise.all([
    loadLocalCommands(state.ctx),
    loadLocalQueueTasks(state.ctx),
    loadLocalBugReports(state.ctx).catch(() => []),
    loadLocalWebStackOverview(state.ctx).catch((error) => ({ ok: false, error: error.message || String(error) })),
    loadHarnessFlowSnapshot(state.ctx).catch(() => emptyHarnessFlow('harness_flow_unavailable')),
    loadLocalCrewMembers(state.ctx).catch(() => []),
    loadLocalHarnessStatus(state.ctx).catch(() => null),
    loadLocalChannelAccounts(state.ctx).catch(() => null),
  ]);
  if (state.disposed) return;
  state.crewMembers = crewMembers;
  state.channelAccounts = channelAccounts;
  state.harnessStatus = harnessStatus;
  armExpressionRefresh(state);
  state.webStack = {
    loading: false,
    error: webStack?.ok ? '' : (webStack?.error || 'Web Stack status unavailable'),
    notice: state.webStack?.notice || '',
    data: webStack?.ok ? webStack : state.webStack?.data,
  };
  state.blobFlow = blobFlow?.ok ? blobFlow : emptyHarnessFlow('rxdb_flow_projection_unavailable');
  state.bundle = mergeBundleWithCommands(ctoxSeed, commands, queueTasks, bugReports);
  // First pass with the server blob decides the selection; the second pass
  // swaps in the selected task's own event stream when the blob is not about it.
  state.flow = state.blobFlow;
  state.model = buildHarnessModel(state.bundle, state.flow, state.lang, state.channelAccounts);
  state.dataLoaded = true;
  state.dataError = '';
  state.focusTask = state.focusTaskConsumed ? null : readFocusTask();
  reconcileSelection(state);
  const selected = getSelectedTask(state);
  const key = taskLiveKey(selected);
  const live = key ? await loadSelectedTaskLive(state.ctx, selected) : { key: '', events: [], runs: [], flow: null };
  if (state.disposed) return;
  state.selectedLive = live;
  applyLiveFlow(state);
  state.harnessHealth = deriveHarnessHealth(state);
  state.runtimeStatus = state.ctx?.sync?.mode === 'webrtc'
    ? displayFlowMode('rxdb-webrtc')
    : (state.ctx?.sync?.config?.native_rxdb_peer_reason || 'native CTOX RxDB peer is not available');
  render(state);
  syncDetailDrawer(state);
}

// Kept for callers that want an explicit re-read after a command (controls).
async function refresh(state) {
  return renderFromLocalCache(state);
}

function changeConcernsSelectedTask(state, change) {
  const doc = change?.documentData || change?.document || change?.doc || null;
  if (!doc || typeof doc !== 'object') return true; // unknown shape: stay safe, debounce coalesces
  const task = getSelectedTask(state);
  const key = taskLiveKey(task);
  if (!key) return false;
  return doc.task_id === key || (Boolean(doc.command_id) && doc.command_id === task?.commandId);
}

function wireLocalRealtime(state) {
  const collectionsToWatch = ['business_commands', 'communication_accounts', 'ctox_runtime_settings', 'ctox_queue_tasks', 'ctox_bug_reports', 'ctox_crew_members', 'ctox_harness_status', 'ctox_runs', 'ctox_harness_events'];
  const selectedTaskOnly = new Set(['ctox_runs', 'ctox_harness_events']);
  let renderTimer = null;
  const scheduleRender = () => {
    if (state.disposed) return;
    if (state.refreshInFlight) {
      state.rerenderAfterRefresh = true;
      return;
    }
    if (renderTimer) return;
    renderTimer = window.setTimeout(() => {
      renderTimer = null;
      renderFromLocalCache(state).catch((error) => {
        console.warn('[ctox] local realtime render failed', error);
        showDataError(state, error);
      });
    }, LOCAL_RENDER_DEBOUNCE_MS);
  };
  const subscriptions = collectionsToWatch
    .map((collectionName) => {
      const collection = ctoxCollection(state.ctx, collectionName);
      if (!collection?.$?.subscribe) return null;
      return collection.$.subscribe((change) => {
        if (selectedTaskOnly.has(collectionName) && !changeConcernsSelectedTask(state, change)) return;
        scheduleRender();
      }) || null;
    })
    .filter(Boolean);
  state.realtimeCollectionCount = subscriptions.length;
  return () => {
    if (renderTimer) window.clearTimeout(renderTimer);
    renderTimer = null;
    for (const sub of subscriptions) {
      try { sub.unsubscribe?.(); } catch {}
    }
  };
}

function dataState(state) {
  let available = false;
  let error = state.dataError;
  try {
    available = Boolean(ctoxCollection(state.ctx, 'business_commands') && ctoxCollection(state.ctx, 'ctox_queue_tasks'));
  } catch (failure) {
    error ||= failure?.message || String(failure);
  }
  const tasks = state.model?.tasks || [];
  // Once a read has completed and rows are on screen, a still catching-up
  // replication is not "loading" any more: the footer would otherwise say
  // "Wird geladen…" over a full task list for as long as the room syncs.
  let readiness = taskSourceReadiness(state);
  if (state.dataLoaded && tasks.length && readiness && readiness.state !== 'offline-pending') readiness = null;
  return workspaceDataState({
    error,
    available,
    readiness,
    loaded: state.dataLoaded,
    tasks,
  });
}

function dataStatusMarkup(state) {
  const status = dataState(state);
  if (status.kind === 'ready') return '';
  const message = labels[state.lang]['dataState_' + status.kind] || labels[state.lang].loadingRuntime;
  const retry = status.kind === 'error'
    ? ` <button type="button" class="ctox-button is-small" data-ctox-retry-load>${escapeHtml(labels[state.lang].retryLoad)}</button>`
    : '';
  return `<span data-ctox-data-state="${status.kind}" role="${status.kind === 'error' ? 'alert' : 'status'}">${escapeHtml(message)}${status.reason ? ` <small class="ctox-data-state-reason" title="${escapeAttr(status.reason)}">${escapeHtml(status.reason)}</small>` : ''}</span>${retry}`;
}

function showDataError(state, error) {
  if (state.disposed) return;
  const message = error?.message || String(error);
  // A read the peer refuses is a role question, not a broken store: say so
  // in the owner's words and keep the raw code in the tooltip.
  state.dataError = /UNAUTHORIZED|not authorized/i.test(message)
    ? `${labels[state.lang].notPermittedForRole} (${message.split(':')[0].trim()})`
    : message;
  // Keep the last successful model, selection and task list when a read fails.
  if (state.model) render(state);
  else renderLoading(state);
}

// Anonymous placeholder: asleep while loading or offline, X eyes on a failure —
// the two must not look alike (Review-Befund B4).
function dataPlaceholderMarkup(kind = 'loading') {
  const eyes = kind === 'error'
    ? '<path class="ctox-data-placeholder-eyes" d="M22 27l8 10M30 27l-8 10M37 27l8 10M45 27l-8 10"/>'
    : '<path class="ctox-data-placeholder-eyes" d="M21 32q5 5 10 0M36 32q5 5 10 0"/>';
  return `<div class="ctox-data-placeholder is-${escapeAttr(kind)}" aria-hidden="true"><svg viewBox="0 0 64 64"><path d="M32 7c15 0 26 10 26 25S48 58 32 58 7 48 7 32 17 7 32 7Z"/>${eyes}</svg></div>`;
}

// The flow canvas without a selected task and without current data: the state
// line and the placeholder, never a coloured "waiting in queue" node.
function emptyWorkspaceMarkup(state) {
  const kind = dataState(state).kind;
  return `<div class="ctox-canvas-container ctox-flow-well">
      <section class="ctox-empty" aria-busy="${kind === 'loading'}">
        ${dataPlaceholderMarkup(kind)}
        ${dataStatusMarkup(state)}
      </section>
    </div>`;
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
        <section class="ctox-empty" aria-busy="${dataState(state).kind === 'loading'}">
          ${dataPlaceholderMarkup(dataState(state).kind)}
          ${dataStatusMarkup(state)}
        </section>
      </div>
      <footer class="ctox-harness-footer"></footer>
    `;
    main.querySelector('[data-ctox-retry-load]')?.addEventListener('click', () => {
      state.dataError = '';
      renderFromLocalCache(state).catch(() => {});
    });
  }
}

function render(state) {
  // Data refresh path: re-render ONLY the list content inside the well (never
  // the header/filterbar/search input — the operator never moves), then the
  // flow canvas / drawer as before.
  renderTaskList(state);
  if (mainIsBusy(state)) {
    state.mainRenderPending = true;
  } else {
    state.mainRenderPending = false;
    renderMain(state);
  }
  syncHarnessHealthUiState(state);
  // renderMain() has just recomputed state.liveAnchorMs from the persisted
  // telemetry, so arm or disarm the clock to match what is actually running.
  syncLiveTicker(state);
  updateHarnessHealthAlerts(state);
}

// Arms the 1s clock only while a real anchor exists, and disarms it the moment
// the work settles. Idempotent: re-rendering with an unchanged anchor keeps the
// existing interval instead of restarting it.
function syncLiveTicker(state) {
  const anchored = Number.isFinite(state?.liveAnchorMs);
  if (anchored && state.liveTicker) {
    updateLiveIndicators(state);
    return;
  }
  if (!anchored && !state.liveTicker) {
    updateLiveIndicators(state);
    return;
  }
  startLiveTicker(state);
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
  const authoritative = authoritativeTaskStatus(task);
  if (authoritative) return [authoritative];
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
  // Health judges the server projection (the blob), not the selected task's
  // own flow: a finished task without events is not a missing projection.
  const flow = state?.blobFlow || state?.flow;
  if (flow?.ok) return false;
  const error = String(flow?.error || '').toLowerCase();
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
  wireCompactMenus(left);
}

function wireCompactMenus(container) {
  for (const details of container.querySelectorAll('.ctox-more-actions')) {
    const summary = details.querySelector('summary');
    const panel = details.querySelector('.ctox-more-actions-body');
    panel.setAttribute('popover', 'auto');
    summary.setAttribute('aria-expanded', 'false');
    summary.addEventListener('click', (event) => {
      event.preventDefault();
      if (panel.matches(':popover-open')) { panel.hidePopover(); return; }
      details.open = true;
      const rect = summary.getBoundingClientRect();
      panel.style.left = Math.max(8, Math.min(rect.right - 240, window.innerWidth - 248)) + 'px';
      panel.style.top = Math.min(rect.bottom + 4, window.innerHeight - 120) + 'px';
      panel.showPopover();
      summary.setAttribute('aria-expanded', 'true');
    });
    panel.addEventListener('toggle', (event) => {
      if (event.newState === 'closed') {
        details.open = false;
        summary.setAttribute('aria-expanded', 'false');
      }
    });
    panel.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') summary.focus();
    });
    panel.addEventListener('click', (event) => {
      if (event.target.closest('button')) panel.hidePopover();
    });
  }
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
    // One-button view switch: the shell's grammar listener has already run on
    // the same click (target phase) and reported the UNCHANGED current view, so
    // flipping here is the single source of the mode change.
    const viewToggle = target.closest('[data-ctox-view-toggle]');
    if (viewToggle) {
      state.taskViewMode = state.taskViewMode === 'list' ? 'cards' : 'list';
      syncViewToggleButton(state, viewToggle);
      renderTaskList(state);
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
    if (select) selectTask(state, select.dataset.selectTaskId, { drawer: false, center: true });
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
  const footerText = `${visibleTasks.length} ${visibleTasks.length === 1 ? t.entryOne : t.entries} · ${scopeLabel}${state.pinnedTaskIds.size ? ` · ${state.pinnedTaskIds.size} ${t.pinned}` : ''}`;
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
  // Wie oben: der Download-Anker gehoert in den Modul-Host, nicht auf den
  // Desktop-Body.
  (state.ctx?.host || document.documentElement).appendChild(anchor);
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
  if (!visibleTasks.length) {
    // Filter-empty (rows exist, the current filter hides them) stays a plain
    // empty; only the data-driven empty (replicated sources have no rows at
    // all) is gated on the collections' initial-sync readiness.
    if (tasks.length) return `<div class="ctox-empty"><span>${escapeHtml(t.noWorkHere)}</span></div>`;
    return renderListOrState([], taskSourceReadiness(state), {
      empty: t.noWorkHere,
      syncing: t.syncingTasks,
    });
  }
  return visibleTasks.map((task) => (cards ? taskCardMarkup(task, state) : compactTaskFlowRow(task, state))).join('');
}

// Combined readiness of the replicated collections backing the task list. The
// list counts as unready while ANY source still waits for its initial sync
// (never-synced / catching-up / offline-pending all surface ready === false).
function taskSourceReadiness(state) {
  const read = state.ctx?.sync?.collectionReadiness;
  if (typeof read !== 'function') return null;
  const snapshots = [];
  for (const name of TASK_SOURCE_COLLECTIONS) {
    try {
      const snapshot = read.call(state.ctx.sync, name);
      if (snapshot) snapshots.push(snapshot);
    } catch {}
  }
  if (!snapshots.length) return null;
  return snapshots.find((snapshot) => snapshot.ready === false) || snapshots[0];
}

// Re-render the list content (never the chrome) when a backing collection
// flips its readiness state, so the syncing shell resolves to rows or the
// honest empty state without waiting for the next poll tick.
//
// Fail-soft on purpose: the shell's subscribeCollectionReadiness can throw
// synchronously (e.g. when the module host is no longer connected, or when an
// immediate listener re-render fails). That used to abort mount() and surface
// the window recovery dialog ("CTOX konnte nicht geladen werden") even though
// the harness markup and the replicated collections were already available.
function wireTaskSourceReadiness(state) {
  const subscribe = state.ctx?.sync?.subscribeCollectionReadiness;
  if (typeof subscribe !== 'function') return () => {};
  const unsubscribes = [];
  // The crew and harness collections are leased later than the task sources
  // (the shell starts a module's collections one by one); their readiness
  // flip is the moment the first hydrate can finally see the members.
  for (const name of [...TASK_SOURCE_COLLECTIONS, 'ctox_crew_members', 'ctox_harness_status']) {
    try {
      const unsubscribe = subscribe.call(state.ctx.sync, name, () => {
        if (state.disposed) return;
        try {
          // Collections that were missing at mount now exist: subscribe again
          // (no poll loop backs this up any more) and hydrate once.
          try { state.localSubscriptionCleanup?.(); } catch {}
          state.localSubscriptionCleanup = wireLocalRealtime(state);
          renderFromLocalCache(state).catch((error) => {
            if (!state.disposed) console.warn('[ctox] readiness hydrate failed', error);
          });
        } catch (error) {
          if (!state.disposed) console.warn('[ctox] readiness re-render failed', error);
        }
      });
      if (typeof unsubscribe === 'function') unsubscribes.push(unsubscribe);
    } catch (error) {
      if (!state.disposed) {
        console.warn(`[ctox] readiness subscription failed for ${name}`, error);
      }
    }
  }
  return () => {
    for (const unsubscribe of unsubscribes) {
      try { unsubscribe(); } catch {}
    }
  };
}

// Betreiber-Direktive 31.08.2026: the shard/list switch is ONE control, not a
// pressed pair. The icon and the label name the view the click switches TO, so
// the button is an action and carries no aria-pressed state.
//
// `data-pg-view` stays on it (the shell's canonical grammar reads the pane's
// view from there) but always holds the CURRENT view: an unrelated grammar emit
// — a search keystroke, a tray filter, a band tab — recomputes `detail.view`
// from this attribute and must never flip the mode as a side effect.
function viewToggleLabel(state) {
  const t = labels[state.lang];
  return state.taskViewMode !== 'list' ? t.showAsList : t.showAsCards;
}

function syncViewToggleButton(state, button) {
  if (!button) return;
  const cards = state.taskViewMode !== 'list';
  const label = viewToggleLabel(state);
  button.setAttribute('data-pg-view', cards ? 'cards' : 'list');
  // The shell's generic view-button wiring stamps aria-pressed on click; a
  // single toggle is an action, so the state attribute is removed again.
  button.removeAttribute('aria-pressed');
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  button.innerHTML = cards ? listViewIcon() : cardsViewIcon();
}

function taskColumnMarkup(tasks, state, options = {}) {
  const t = labels[state.lang];
  const counts = taskPrimaryViewCounts(tasks, state);
  const sourceOptions = taskSourceOptions(tasks);
  const viewLabel = taskPrimaryViewLabel(state.taskPrimaryView, t);
  const scopeLabel = state.taskPinFilter === 'pinned' ? `${viewLabel} · ${t.pinned}` : viewLabel;
  const visibleCount = filterAndSortTasks(tasks, state).length;
  const footerText = `${visibleCount} ${visibleCount === 1 ? t.entryOne : t.entries} · ${scopeLabel}${state.pinnedTaskIds.size ? ` · ${state.pinnedTaskIds.size} ${t.pinned}` : ''}`;
  const cards = state.taskViewMode !== 'list';
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <h2 class="ctox-pane-title">${escapeHtml(t.tasks)}</h2>
        </div>
        <div class="ctox-pane-actions">
          <details class="ctox-more-actions">
            <summary aria-label="${escapeAttr(state.lang === 'de' ? 'Weitere Aktionen' : 'More actions')}">···</summary>
            <div class="ctox-more-actions-body">
              <button type="button" class="ctox-button" data-task-import>${escapeHtml(t.importTasks)}</button>
              <button type="button" class="ctox-button" data-task-export>${escapeHtml(t.exportTasks)}</button>
            </div>
          </details>
        </div>
      </div>
    </header>
    <div class="ctox-filterbar">
      <input class="ctox-pane-search" type="search" data-pg-search value="${escapeAttr(state.taskSearch || '')}" placeholder="${escapeAttr(t.taskSearch)}" aria-label="${escapeAttr(t.taskSearch)}">
      <button type="button" class="ctox-pane-icon ctox-view-mode-toggle" data-ctox-view-toggle data-pg-view="${cards ? 'cards' : 'list'}" aria-label="${escapeAttr(viewToggleLabel(state))}" title="${escapeAttr(viewToggleLabel(state))}">${cards ? listViewIcon() : cardsViewIcon()}</button>
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

// Card view (Betreiber-Direktive 31.08.2026): the roomy shard — bold title,
// one meta row carrying the detail fields the console already loads (status,
// assignment/source, last change) and the labelled four-stage pipeline.
function taskCardMarkup(task, state) {
  const t = labels[state.lang];
  const selected = task.id === state.selectedTaskId;
  const pinned = state.pinnedTaskIds.has(task.id);
  const title = taskDisplayTitle(task, state);
  const source = task.channelLabel || displayWorkSource(task.channel || task.source || task.moduleId || 'ctox');
  const status = displayStatus(task.routeStatus || task.status, state.lang);
  const changed = formatShortTimestamp(task.updatedAt || task.createdAt || task.timestamp);
  const problem = ['blocked', 'failed', 'cancelled'].includes(normalizeCommandStatus(task.routeStatus || task.status));
  const reason = taskSummaryReason(task, state);
  // The card says who and how, not why: the member's creature carries the
  // state, the reason lives in the tooltip and the drawer (Owner 08.09.).
  // The one exception is a task that stopped — blocked, failed, cancelled —
  // where the reason is the fact the reader came for; it gets one quiet line.
  const member = taskCrewMember(task, state);
  const crewStatus = taskCrewStatus(task);
  const portrait = member
    ? `<span class="ctox-flow-creature-shell ctox-task-portrait" title="${escapeAttr(member.name)}">${memberCreatureHtml(member, state, crewStatus === 'running' ? 'running' : crewStatus === 'failed' ? 'failed' : memberCreatureState(member))}</span>`
    : '';
  const tooltip = [status, source, changed, reason].filter(Boolean).join(' · ');
  return `
    <article class="ctox-list-item ctox-task-card ${selected ? 'is-selected' : ''} ${pinned ? 'is-pinned' : ''} ${member ? 'has-member' : ''}"
      data-task-id="${escapeAttr(task.id)}" data-context-record-id="${escapeAttr(task.id)}" data-context-record-type="ctox_task" data-context-label="${escapeAttr(title)}">
      <button type="button" class="ctox-task-selector" data-select-task-id="${escapeAttr(task.id)}" aria-label="${escapeAttr(`${state.lang === 'de' ? 'Aufgabe auswählen' : 'Select task'}: ${title}`)}" title="${escapeAttr(tooltip)}">
        ${portrait}
        <strong>${escapeHtml(title)}</strong>
        <small class="ctox-task-meta">${status ? `<span class="ctox-task-meta-status ${problem ? 'is-problem' : ''}">${escapeHtml(status)}</span>` : ''}${changed ? `<span>${escapeHtml(changed)}</span>` : ''}</small>
        ${problem && reason ? `<span class="ctox-task-reason is-problem">${escapeHtml(reason)}</span>` : ''}
        ${taskPipelineMarkup(task, state)}
      </button>
      <div class="ctox-task-actions">
        <button type="button" class="ctox-pane-icon ${pinned ? 'is-active' : ''}" data-pin-task-id="${escapeAttr(task.id)}" aria-pressed="${pinned}" aria-label="${escapeAttr(pinned ? t.unpinTask : t.pinTask)}" title="${escapeAttr(pinned ? t.unpinTask : t.pinTask)}">${actionIcon(state, 'pin')}</button>
      </div>
    </article>
  `;
}

// List view (Betreiber-Direktive 31.08.2026): EXACTLY one dense line per entry
// — title plus a single short meta on the right. The four-stage pipeline stays
// in the markup as a 4-segment micro bar (its stage labels are screen-reader
// text in list mode, see index.css); every other detail field belongs to the
// card view.
function compactTaskFlowRow(task, state) {
  const t = labels[state.lang];
  const selected = task.id === state.selectedTaskId;
  const pinned = state.pinnedTaskIds.has(task.id);
  const title = taskDisplayTitle(task, state);
  return `
    <article class="ctox-list-item ctox-task-flow-row ${selected ? 'is-selected' : ''} ${pinned ? 'is-pinned' : ''}"
      data-compact-flow data-task-id="${escapeAttr(task.id)}" data-context-record-id="${escapeAttr(task.id)}" data-context-record-type="ctox_task" data-context-label="${escapeAttr(title)}">
      <button type="button" class="ctox-task-selector" data-select-task-id="${escapeAttr(task.id)}" aria-label="${escapeAttr(`${t.openTaskDetail}: ${title}`)}">
        <strong>${escapeHtml(title)}</strong>
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
  if (problem) stages[current] = displayStatus(task.routeStatus || task.status, state.lang);
  return `<div class="ctox-task-pipeline ${options.compact ? 'is-compact' : ''} ${problem ? 'is-problem' : ''}" aria-label="${escapeAttr(stages[current])}" data-flow-stage="${current}">${stages.map((label, index) => `<span class="${index < current ? 'is-complete' : index === current ? 'is-current' : 'is-future'}"><i aria-hidden="true"></i><em>${escapeHtml(label)}</em></span>`).join('')}</div>`;
}

function taskPipelineStage(task) {
  const statuses = taskStatusCandidates(task);
  if (routingProblemStatus(task)) return Number(task.failureAttemptCount || task.attempt || 0) > 0 ? 1 : 0;
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
  if (view === 'waiting') return !done && !working && !taskIsHarnessTerminal(task);
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
    pause: 'M8 5v14M16 5v14',
    chat: 'M4 5h16v10H9l-5 4V5Z',
    edit: 'M4 20h4l10-10-4-4L4 16v4ZM13 7l4 4',
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

// The Web Stack overview lives in the drawer (it is runtime plumbing, not part
// of the harness view). The header icon opens it; the panel markup is unchanged.
function openWebStackDrawer(state) {
  state.detailDrawer = { type: 'webstack' };
  syncDetailDrawer(state);
  renderMain(state);
  void refreshWebStackPanel(state);
}

function webStackDrawer(state) {
  const body = document.createElement('div');
  body.className = 'drawer-body ctox-task-drawer ctox-webstack-drawer';
  body.setAttribute('data-context-record-type', 'ctox_web_stack');
  body.innerHTML = webStackPanel({ ...state, webStackPanelOpen: true });
  wireWebStackPanel(state, body);
  return body;
}

function wireWebStackPanel(state, root) {
  root.querySelector('[data-webstack-close]')?.addEventListener('click', () => {
    closeDetailDrawer(state);
    renderMain(state);
  });
  root.querySelector('[data-webstack-check-projection]')?.addEventListener('click', async () => {
    state.webStack = { ...(state.webStack || {}), loading: true, notice: '' };
    syncDetailDrawer(state);
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
  // A final routing transition can precede command/plan projection delivery.
  // Show it even when the last plan still has an in-progress step.
  if (routingProblemStatus(task)) return taskStatusSteps(task, state);
  if (isExactCommunicationFlow(task, state)) return communicationTaskSteps(task, state);
  // The persisted execution plan outranks the audit stream: it is the durable
  // record of what the model actually planned and completed, with a per-step
  // activity-turn count.
  const planSteps = executionPlanSteps(task, state);
  if (planSteps.length) return planSteps;
  const timeline = state.model?.timeline || [];
  if (timeline.length && taskMatchesHarnessFlow(task, state)) {
    const steps = timeline.map((node, index) => ({
      id: node.id,
      label: node.label,
      detail: clip(cleanUiCopy(node.label || node.phase || itemSummary(task) || ''), 180),
      timestamp: node.timestamp || '',
      metrics: metricsLabel(node, state.lang),
      active: node.status === 'active' || index === timeline.length - 1,
      timelineIndex: index,
    }));
    return withRouteStatusStep(steps, task, state);
  }
  return taskStatusSteps(task, state);
}

// Renders `execution_progress.steps` — the plan revision the harness persisted.
// Step status is authoritative (completed prefix, at most one in_progress, a
// pending suffix); the per-step activity-turn count is the real, deduplicated
// number of model turns attributed to that step.
function executionPlanSteps(task, state) {
  const progress = taskExecutionProgress(task);
  if (!progress?.steps?.length) return [];
  const t = labels[state.lang] || labels.de;
  const activePosition = Number.isFinite(progress.currentStep)
    ? progress.currentStep
    : (progress.steps.find((step) => step.status === 'in_progress')?.position ?? null);
  return progress.steps.map((step, index) => {
    const turns = Number.isFinite(step.activityTurns) ? step.activityTurns : null;
    return {
      id: `plan-${progress.revision ?? 0}-${step.position ?? index + 1}`,
      label: step.label || `${t.step} ${step.position ?? index + 1}`,
      detail: clip(cleanUiCopy(step.label || ''), 180),
      // Plan steps carry no per-step timestamp; say so rather than borrow one.
      timestamp: '',
      metrics: turns === null
        ? t.notCaptured
        : `${turns} ${turns === 1 ? t.activityTurnSingular : t.activityTurnPlural}`,
      status: step.status,
      activityTurns: turns,
      active: activePosition === null
        ? index === progress.steps.length - 1
        : step.position === activePosition,
      timelineIndex: -1,
      flowKind: 'execution_plan',
    };
  });
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
  return Boolean(task && state && flowMatchesTask(state.flow, task));
}

function flowMatchesTask(flowResult, task) {
  if (!task) return false;
  const source = flowResult?.flow?.source || {};
  const ids = new Set([source.message_key, source.work_id].filter(Boolean));
  if (ids.has(task.id) || ids.has(task.taskId) || ids.has(task.commandId) || ids.has(task.runId)) return true;
  return false;
}

function taskStatusSteps(task, state) {
  const status = authoritativeTaskStatus(task) || normalizeCommandStatus(task.routeStatus || task.status);
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
        detail: taskReasonText(task, state) || taskDetailText(task.resultSummary || task.summary || task.target || task.source || '', state),
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
  // Locale readiness can arrive before the first data hydration. Keep the
  // existing loading surface until the model is available.
  if (!model) return;
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
  // Metrics describe the task the operator is looking at. The previous build
  // aggregated the flow only for a *running* active task and rendered
  // emptyMetrics() otherwise, so the strip read "nicht erfasst" permanently.
  const metricSubject = metricSubjectTask(state, selectedTask);
  const metrics = metricSubject ? taskTelemetry(metricSubject, state) : emptyTelemetry();
  const live = metrics.live;
  state.liveAnchorMs = live ? metrics.startedAtMs : null;
  // A settled task shows its measured duration; a genuinely working task with a
  // real start timestamp shows a clock anchored to that timestamp. No anchor
  // means no number — never a free-running animation.
  const elapsedSeconds = live ? liveElapsedSeconds(state) : metrics.seconds;
  const main = state.ctx.host.querySelector('[data-ctox-main]');
  const panelTaskId = selectedTask?.id || '';
  if (state.compactPanelTaskId !== panelTaskId) {
    state.compactPanelTaskId = panelTaskId;
    state.jobEditorOpen = false;
    state.historyOpen = false;
  }
  const history = timelinePanel(state, selectedTask, selectedNode, metrics);
  const hasHistory = selectedTask ? taskSteps(selectedTask, state).length > 1 : state.model.timeline.length > 1;
  const previousViewport = readFlowViewport(state);
  const viewBox = flowViewBox(selectedTask, state);
  // Without a selected task and without current data the workspace itself
  // carries the state line; the footer must not repeat it.
  const stateInWorkspace = !selectedTask && Boolean(state.ctx) && dataState(state).kind !== 'ready';
  const dataNotice = stateInWorkspace ? '' : (dataStatusMarkup(state) || (!syncIsConnected(state) ? escapeHtml(t.syncDisconnected) : ''));
  main.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <h2 class="ctox-pane-title">${escapeHtml(selectedTask ? taskDisplayTitle(selectedTask, state) : t.doingNow)}</h2>
          ${state.harnessStatus?.paused ? `<small class="ctox-paused-note">${escapeHtml(t.harnessPaused)}</small>` : ''}
        </div>
        <div class="ctox-pane-actions">
          ${selectedTask ? `<button type="button" class="ctox-button ctox-job-toggle" data-job-toggle aria-expanded="${Boolean(state.jobEditorOpen)}">${escapeHtml(t.editTask)}</button>` : ''}
          <details class="ctox-more-actions">
            <summary aria-label="${escapeAttr(state.lang === 'de' ? 'Crew verwalten' : 'Manage crew')}">···</summary>
            <div class="ctox-more-actions-body">
              ${harnessControlsMarkup(state)}
              <button type="button" class="ctox-button" data-manage-channels>${escapeHtml(state.lang === 'de' ? 'Kanäle verwalten' : 'Manage channels')}</button>
              <button type="button" class="ctox-button" data-webstack-toggle>${escapeHtml(t.webStack)}</button>
              ${selectedTask ? `<button type="button" class="ctox-button" data-open-selected-task>${escapeHtml(t.openTaskDetail)}</button>` : ''}
              ${crewStripMarkup(state)}
            </div>
          </details>
        </div>
      </div>
    </header>
    <section class="ctox-job-panel" data-job-panel ${state.jobEditorOpen ? '' : 'hidden'} aria-label="${escapeAttr(t.editTask)}"></section>
    ${shouldShowCrewHome(state) ? crewHomeMarkup(state) : stateInWorkspace ? emptyWorkspaceMarkup(state) : `<div class="ctox-canvas-container ctox-flow-well">
      <div class="ctox-flow-toolbar" aria-label="${escapeAttr(t.flowControls)}" data-flow-control>
        <button type="button" class="ctox-pane-icon" data-zoom="-" aria-label="${escapeAttr(t.zoomOut)}" title="${escapeAttr(t.zoomOut)}" ${state.zoom <= MIN_ZOOM ? 'disabled' : ''}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="M5 12h14"/></svg></button>
        <span>${Math.round(state.zoom * 100)}%</span>
        <button type="button" class="ctox-pane-icon" data-zoom="+" aria-label="${escapeAttr(t.zoomIn)}" title="${escapeAttr(t.zoomIn)}" ${state.zoom >= MAX_ZOOM ? 'disabled' : ''}>${actionIcon(state, 'add')}</button>
      </div>
      <div class="ctox-flow-canvas" data-flow-canvas>
        <div class="ctox-flow-canvas-inner" style="width:${FLOW_WIDTH * state.zoom}px;height:${viewBox.height * state.zoom}px;min-height:${viewBox.height * state.zoom}px">
          ${flowSvg(model, selectedNode, visibleTrace, selectedTask, state, taskStepView, viewBox)}
        </div>
      </div>
    </div>`}
    <details class="ctox-history-fold" ${state.historyOpen && hasHistory ? 'open' : ''} ${hasHistory ? '' : 'hidden'}>
      <summary>${escapeHtml(t.timeline)}${dataNotice ? `<span class="ctox-history-connection">${dataNotice}</span>` : ''}</summary>
      <div class="ctox-history-content">${history}${executionProgressBar(metrics, state)}${metricsStripMarkup(metrics, elapsedSeconds, live, state)}</div>
    </details>
    ${!hasHistory && dataNotice ? `<footer class="ctox-harness-footer" data-harness-health-tooltip>${dataNotice}</footer>` : ''}
  `;
  restoreFlowViewport(state, previousViewport);
  const editor = main.querySelector('[data-job-panel]');
  wireCompactMenus(main);
  main.querySelector('[data-manage-channels]')?.addEventListener('click', () => {
    window.CTOX_BUSINESS_OS_APP?.openSettingsDrawer?.({ initialTab: 'channels' });
  });
  const mountEditor = () => {
    if (selectedTask && !editor.firstElementChild) editor.append(taskDrawer(selectedTask, state, { editorOnly: true }));
  };
  if (state.jobEditorOpen) mountEditor();
  main.querySelector('[data-job-toggle]')?.addEventListener('click', (event) => {
    state.jobEditorOpen = !state.jobEditorOpen;
    editor.hidden = !state.jobEditorOpen;
    event.currentTarget.setAttribute('aria-expanded', String(state.jobEditorOpen));
    if (state.jobEditorOpen) mountEditor();
  });
  main.querySelector('.ctox-history-fold')?.addEventListener('toggle', (event) => {
    state.historyOpen = event.currentTarget.open;
  });
  main.querySelector('[data-harness-pause]')?.addEventListener('click', () => {
    runHarnessControl(state, 'pause', !state.harnessStatus?.paused);
  });
  main.querySelector('[data-harness-capacity]')?.addEventListener('change', (event) => {
    runHarnessControl(state, 'capacity', event.currentTarget.value);
  });
  main.querySelector('[data-webstack-toggle]')?.addEventListener('click', () => {
    if (state.detailDrawer?.type === 'webstack') {
      closeDetailDrawer(state);
      return;
    }
    openWebStackDrawer(state);
  });
  wireCrewHome(state, main);
  main.querySelector('[data-ctox-retry-load]')?.addEventListener('click', () => {
    state.dataError = '';
    renderFromLocalCache(state).catch(() => {});
  });
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
  wireTimelineStepButtons(state, main);
  main.querySelectorAll('[data-task-id]').forEach((button) => {
    button.addEventListener('click', () => {
      selectTask(state, button.dataset.taskId, { drawer: true, center: true });
    });
    if (button.classList.contains('ctox-flow-creature-slot')) {
      button.addEventListener('keydown', (event) => {
        if (event.key !== 'Enter' && event.key !== ' ') return;
        event.preventDefault();
        selectTask(state, button.dataset.taskId, { drawer: true, center: true });
      });
    }
  });
  main.querySelector('[data-timeline-range]')?.addEventListener('input', (event) => {
    // While the pointer holds the slider, the input must not be replaced.
    const light = Boolean(state.timelineScrubbing);
    if (event.target.dataset.taskTimelineRange === 'true') {
      setTaskTimelineStep(state, Number(event.target.value), { center: !light, light });
      return;
    }
    const mappedSteps = event.target.dataset.timelineRangeSteps
      ? event.target.dataset.timelineRangeSteps.split(',').map((value) => Number(value))
      : null;
    setTimelineStep(state, mappedSteps?.[Number(event.target.value)] ?? Number(event.target.value), { center: !light, light });
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
  syncCrewProceduralMotion(main);
  updateLiveIndicators(state);
}

// A phase the locale does not know is shown as words, never as the enum.
function humanizePhase(phase) {
  const words = String(phase || '').replace(/[_\-:]+/g, ' ').trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : '';
}

function emptyMetrics() {
  return { inputTokens: null, outputTokens: null, toolCalls: null, seconds: null };
}

// The execution bar renders the SERVER-COMPUTED percent only. Plan steps own
// the first 90 percent (round(90 * completed / total)); completed model work
// holds at 90 through review; validated review sets 100 (HARNESS.md). When no
// plan has been persisted the bar carries no fill and says so — there is
// deliberately no indeterminate/marquee state.
function executionProgressBar(metrics, state) {
  const t = labels[state.lang];
  const measured = Number.isFinite(metrics?.percent);
  const percent = measured ? clampMetric(metrics.percent, 0, 100) : 0;
  const stepsKnown = Number.isFinite(metrics?.totalSteps) && metrics.totalSteps > 0;
  const stepLabel = stepsKnown
    ? `${t.step} ${clampMetric(metrics.currentStep ?? ((metrics.completedSteps || 0) + 1), 1, metrics.totalSteps)}/${metrics.totalSteps}`
    : '';
  const phaseLabel = metrics?.phase ? (t.executionPhases?.[String(metrics.phase).toLowerCase()] || humanizePhase(metrics.phase)) : '';
  return `
    <section class="ctox-execution-progress ${measured ? '' : 'is-unmeasured'}"
      aria-label="${escapeAttr(t.executionProgress)}"
      style="--execution-progress:${escapeAttr(String(percent))}%">
      <div class="ctox-execution-progress-head">
        <span class="ctox-pane-kicker">${escapeHtml(t.executionProgress)}</span>
        <strong>${escapeHtml(measured ? `${percent}%` : '—')}</strong>
      </div>
      <div class="ctox-execution-progress-track"
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax="100"
        ${measured ? `aria-valuenow="${percent}"` : `aria-valuetext="${escapeAttr(t.notCaptured)}"`}>
        <i aria-hidden="true"></i>
      </div>
      <div class="ctox-execution-progress-meta">
        <span>${escapeHtml(stepLabel || (measured ? '' : t.noPlanYet))}</span>
        <span>${escapeHtml(phaseLabel)}</span>
      </div>
    </section>
  `;
}

function timelinePanel(state, selectedTask, selectedNode, metrics) {
  const t = labels[state.lang];
  if (!selectedTask) {
    const max = Math.max(state.model.timeline.length - 1, 0);
    const value = clampIndex(state.selectedStepIndex, state.model.timeline.length);
    const hasRange = max > 0;
    return `
      <section class="ctox-timeline-panel ${hasRange ? '' : 'is-disabled'}" aria-label="${escapeAttr(t.activityTimeline)}" style="--timeline-progress:${escapeAttr(progressPercent(value, max))}%">
        <div class="ctox-timeline-head">
          <div>
            <span class="ctox-pane-kicker">${escapeHtml(t.timeline)}</span>
            ${timelineLiveStatusMarkup(selectedTask, selectedNode, state)}
          </div>
          <strong>${escapeHtml(hasRange ? (selectedNode?.label || '') : t.timelineUnavailable)}</strong>
        </div>
        <div class="ctox-timeline-scrub">
          <input aria-label="${escapeAttr(t.selectActivityEvent)}" max="${max}" min="0" step="1" type="range" value="${value}" data-timeline-range ${hasRange ? '' : 'disabled aria-disabled="true"'} />
        </div>
        <div class="ctox-timeline-detail">
          <span>${escapeHtml(hasRange ? (selectedNode?.phase || '') : t.notLive)}</span>
          <p>${escapeHtml(hasRange ? (selectedNode?.lines?.[0] || t.noEventDetail) : t.timelineUnavailableDetail)}</p>
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
        <ul class="ctox-timeline-scale" ${hasRange ? '' : 'aria-disabled="true"'}>
          ${steps.map((step, index) => `
            <li><button type="button" class="${index < activeStepIndex ? 'is-done' : ''} ${index === activeStepIndex ? 'is-current' : ''}" data-task-step-index="${index}" data-context-record-id="${escapeAttr(`${selectedTask.id}:${step.id || index}`)}" data-context-record-type="ctox_task_step" data-context-label="${escapeAttr(step.label)}" ${hasRange ? '' : 'disabled'}>
              <span>${String(index + 1).padStart(2, '0')}</span>
              <strong>${escapeHtml(step.label)}</strong>
              <small>${escapeHtml(stepMetaLabel(step, state))}</small>
            </button></li>
          `).join('')}
        </ul>
      </div>
      <div class="ctox-timeline-detail">
        <span>${escapeHtml(hasRange ? (current?.label || t.currentStep) : t.notLive)}</span>
        <p>${escapeHtml(hasRange ? (current?.detail || selectedNode?.lines?.[0] || itemSummary(selectedTask) || t.noRecentWork) : t.timelineUnavailableDetail)}</p>
        <small>${escapeHtml(current ? `${stepMetaLabel(current, state)} · ${current.metrics || ''}` : selectedNode ? metricsLabel(selectedNode, state.lang) : '')}</small>
      </div>
    </section>
  `;
}

// Scrub POSITION within the event range — not a progress measurement. With a
// single event (max <= 0) the position is trivially the end, hence 100.
// That value must never paint a full bar: both call sites set `is-disabled`
// on exactly that condition (hasRange = max > 0), and the disabled rule in
// index.css forces the scrub fill to width 0. So an unmeasurable timeline
// renders an EMPTY track, never a full one.
function progressPercent(value, max) {
  if (!Number.isFinite(max) || max <= 0) return 100;
  return Math.round((clampMetric(value, 0, max) / max) * 100);
}

function flowSvg(model, selectedNode, visibleTrace, selectedTask, state, taskStepView = null, viewBox = flowViewBox(selectedTask, state)) {
  const t = labels[state?.lang] || labels.de;
  const communicationOnly = isCommunicationFlow(selectedTask, state);
  const harnessOffsetY = reviewHarnessOffsetY(selectedTask, state);
  // Wo steht der ausgewaehlte Task GERADE im Loop? Dieser Knoten wird markiert,
  // damit die Frage "wo steckt er" ohne Suchen beantwortet ist.
  const standortNodeId = selectedTask ? (taskCrewNodeId(selectedTask, model) || '') : '';
  return `
    <svg class="ctox-flow-diagram" viewBox="0 ${viewBox.y} ${FLOW_WIDTH} ${viewBox.height}" preserveAspectRatio="xMidYMin meet" role="img" aria-label="${escapeAttr(t.flowDiagram)}">
      <defs>
        <marker id="ctox-flow-arrow" markerHeight="8" markerWidth="8" orient="auto" refX="7" refY="4">
          <path d="M0,0 L8,4 L0,8 Z"></path>
        </marker>
      </defs>
      <g class="ctox-flow-lanes" aria-hidden="true">
        ${communicationOnly ? `
          <rect x="18" y="18" width="${FLOW_WIDTH - 36}" height="340" rx="16"></rect>
          <text x="34" y="44">${escapeHtml(t.laneCommunication)}</text>
        ` : `
          <g transform="translate(0 ${harnessOffsetY})">
          <rect x="18" y="388" width="${FLOW_WIDTH - 36}" height="260" rx="16"></rect>
          <rect x="18" y="688" width="${FLOW_WIDTH - 36}" height="340" rx="16"></rect>
          <text x="34" y="414">${escapeHtml(t.laneQueue)}</text>
          <text x="34" y="714">${escapeHtml(t.laneEvidence)}</text>
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
      ${communicationOnly ? '' : model.nodes.map((node) => flowNodeSvg(node, selectedNode, visibleTrace.nodeStrength.get(node.id) || 0, state.lang, standortNodeId)).join('')}
      ${communicationOnly ? '' : flowCrewSvg(model, selectedTask, state)}
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
  const selectedChannel = selectedTask ? inferInboundChannel(selectedTask) : '';
  const selected = channels.find((channel) => channel.id === selectedChannel);
  const endpoint = selected || channels[0] || null;
  const queued = model.nodeMap.get('queued') || { x: 330, y: 520 };
  const nodeX = 44;
  const nodeWidth = 144;
  const nodeY = queued.y - 26;
  const selectedEdgeY = nodeY + 26;
  const queueLeft = queued.x - NODE_WIDTH / 2;
  const queueApproachX = Math.max(nodeX + nodeWidth + 22, queueLeft - 26);
  if (!endpoint) return `<g class="ctox-flow-inbound"><text class="ctox-flow-inbound-label" x="${nodeX}" y="${nodeY - 14}">${escapeHtml(t.inboundChannels)}</text><text class="ctox-flow-channel-count" x="${nodeX}" y="${nodeY + 16}">${escapeHtml(model.inboundChannelsAvailable === false ? (state.lang === 'de' ? 'Kanäle nicht verfügbar' : 'Channels unavailable') : (state.lang === 'de' ? 'Keine Kanäle eingerichtet' : 'No channels configured'))}</text></g>`;
  const detail = state.lang === 'de' ? `${endpoint.count} ${endpoint.count === 1 ? 'Aufgabe' : 'Aufgaben'}` : `${endpoint.count} ${endpoint.count === 1 ? 'task' : 'tasks'}`;
  return `
    <g class="ctox-flow-inbound" aria-label="Eingänge für die Crew">
      <text class="ctox-flow-inbound-label" x="${nodeX}" y="${nodeY - 14}">${escapeHtml(t.inboundChannels)}</text>
      <path class="ctox-flow-channel-edge ${selected ? 'is-selected' : ''}" d="M ${nodeX + nodeWidth} ${selectedEdgeY} L ${queueApproachX} ${selectedEdgeY} L ${queueApproachX} ${queued.y} L ${queueLeft} ${queued.y}"></path>
      <g class="ctox-flow-channel-node ${selected ? 'is-selected' : ''}" transform="translate(${nodeX} ${nodeY})">
        <rect width="${nodeWidth}" height="52" rx="12"></rect>
        <text class="ctox-flow-channel-name" x="12" y="19">${escapeHtml(clip(endpoint.label, 18))}</text>
        <text class="ctox-flow-channel-count" x="12" y="36">${escapeHtml(clip(detail || endpoint.kind, 20))}</text>
      </g>
      ${channels.filter((channel) => channel.id !== endpoint.id).slice(0, 4).map((channel, index) => {
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
  const label = String(channel || '').toLowerCase() === 'unavailable'
    ? labels[state.lang].sourceUnavailable
    : (task?.channelLabel || inboundChannelLabel(channel));
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

function flowNodeSvg(node, selectedNode, traceStrength, lang = 'de', standortNodeId = '') {
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
    <g class="ctox-flow-node-g is-${escapeAttr(node.status)} ${isVisibleTrace ? 'is-observed is-trace' : 'is-possible'} ${isSelected ? 'is-current is-selected' : ''} ${standortNodeId && node.id === standortNodeId ? 'is-crew-hier' : ''}"
       data-node-id="${escapeAttr(node.id)}" data-context-record-id="${escapeAttr(node.id)}" data-context-record-type="ctox_flow_node" data-context-label="${escapeAttr(node.label)}" role="button" style="--trace-strength:${traceStrength}" tabindex="0" transform="translate(${node.x} ${node.y})">
      <title>${escapeHtml(node.label)}</title>
      ${ring}
      ${shape}
      <text class="ctox-flow-node-phase" x="${-NODE_WIDTH / 2 + 10}" y="${-NODE_HEIGHT / 2 + 16}">${escapeHtml(node.phase)}</text>
      <text class="ctox-flow-node-title" x="${-NODE_WIDTH / 2 + 10}" y="${-NODE_HEIGHT / 2 + 34}">
        ${wrapSvgText(node.label).map((line, index) => `<tspan x="${-NODE_WIDTH / 2 + 10}" dy="${index === 0 ? 0 : 15}">${escapeHtml(line)}</tspan>`).join('')}
      </text>
      ${Number.isFinite(node.inputTokens) && Number.isFinite(node.outputTokens) ? `<text class="ctox-flow-node-metrics" x="${-NODE_WIDTH / 2 + 10}" y="${NODE_HEIGHT / 2 - 8}">${escapeHtml(metricsLabel(node, lang))}</text>` : ''}
    </g>
  `;
}

function flowCrewSvg(model, selectedTask, state) {
  // Owner-Befund 04.09.2026: "ich sehe aber ganz viele wesen und visuell ist
  // gar nicht klar, wo er gerade im harness loop steckt." Bis zu zwoelf Wesen
  // standen gleichzeitig auf der Karte - auch fuer laengst gescheiterte Tasks -
  // und stapelten sich auf denselben Knoten. Wer wissen will, wo SEIN Task
  // steht, findet ihn darin nicht.
  //
  // Es zeigt jetzt: der ausgewaehlte Task IMMER, dazu nur die tatsaechlich
  // laufenden. Alles andere ist Vergangenheit und gehoert nicht auf die Karte.
  const alle = taskCrewCandidates(model);
  const tasks = alle.filter((task) => {
    if (selectedTask && task.id === selectedTask.id) return true;
    return taskCrewStatus(task) === 'running';
  });
  if (!tasks.length && selectedTask) tasks.push(selectedTask);
  if (!tasks.length && state.ctx) {
    const node = model.nodeMap.get('queued');
    if (node) return `<foreignObject x="${node.x - 82}" y="${node.y - NODE_HEIGHT / 2 - 62}" width="56" height="56" aria-hidden="true"><div xmlns="http://www.w3.org/1999/xhtml">${dataPlaceholderMarkup()}</div></foreignObject>`;
  }
  const occupied = new Map();
  return tasks.map((task) => {
    const nodeId = taskCrewNodeId(task, model);
    const node = model.nodeMap.get(nodeId) || model.nodeMap.get('queued');
    if (!node) return '';
    const slot = occupied.get(node.id) || 0;
    occupied.set(node.id, slot + 1);
    const column = slot % 4;
    const row = Math.floor(slot / 4);
    const x = node.x - 82 + column * 42;
    const y = node.y - NODE_HEIGHT / 2 - 52 - row * 40;
    const selected = task.id === selectedTask?.id;
    // Without a live channel, or before the first complete read, nothing on
    // screen is current: the crew sleeps.
    const status = state?.ctx && (!syncIsConnected(state) || dataState(state).kind !== 'ready') ? 'queued' : taskCrewStatus(task);
    // Der Knoten, auf dem das ausgewaehlte Wesen steht, ist der Schritt, an dem
    // der Task GERADE arbeitet. Er wird markiert, damit die Karte die Frage
    // "wo steckt er im Loop" ohne Suchen beantwortet.
    if (selected) state.crewStandortNodeId = node.id;
    const member = taskCrewMember(task, state);
    const memberLabel = member ? member.name : (labels[state?.lang]?.noCrewMember || labels.de.noCrewMember);
    const title = `${memberLabel} · ${taskDisplayTitle(task, state)} · ${task.id}`;
    const liveTask = withLiveActivity(task, state?.selectedLive);
    const creature = crewCreatureHtml({
      ...liveTask,
      // With a member the creature IS the member: same seed and identity as at
      // home and in the chat bar. Without one it stays a neutral grey creature.
      // Without a member the seeded identity stays, so the chat bar and the map
      // keep showing the same creature for the same task; the title says so.
      crewKey: member ? member.id : (task.commandId || task.command_id || task.taskId || task.task_id || task.id),
      crewIdentity: member ? memberIdentity(member) : null,  // null = the neutral crew creature (shared)
      executionProgress: liveTask.executionProgress || liveTask.execution_progress,
    }, status, 'map');
    return `
      <foreignObject class="ctox-flow-creature-slot ${selected ? 'is-selected' : ''}" x="${x}" y="${y}" width="48" height="48"
        data-task-id="${escapeAttr(task.id)}" data-creature-node-id="${escapeAttr(node.id)}" role="button" tabindex="0"
        aria-label="${escapeAttr(title)}">
        <div class="ctox-flow-creature-shell" xmlns="http://www.w3.org/1999/xhtml" title="${escapeAttr(title)}">${creature}</div>
      </foreignObject>
    `;
  }).join('');
}

function taskCrewCandidates(model) {
  const tasks = Array.isArray(model?.tasks) ? model.tasks : [];
  const priority = (task) => {
    const status = taskCrewStatus(task);
    if (status === 'running') return 0;
    if (status === 'queued') return 1;
    if (status === 'failed') return 2;
    return 3;
  };
  return [...tasks]
    .sort((left, right) => priority(left) - priority(right) || taskTimestampMs(right) - taskTimestampMs(left))
    .slice(0, 12);
}

function taskCrewNodeId(task, model) {
  const active = model?.activeTask;
  const matchesActive = active && (
    (task.id && task.id === active.id)
    || (task.taskId && task.taskId === active.taskId)
    || (task.commandId && task.commandId === active.commandId)
  );
  if (matchesActive) {
    return model.activeNodeId || authoritativeTaskNodeId(task);
  }
  return authoritativeTaskNodeId(task);
}

function taskCrewStatus(task) {
  const status = authoritativeTaskStatus(task) || normalizeCommandStatus(task?.routeStatus || task?.status);
  if (HARNESS_PROBLEM_TERMINAL_STATUSES.has(status)) return 'failed';
  if (HARNESS_ACTIVE_STATUSES.has(status) || status === 'review') return 'running';
  if (HARNESS_SUCCESS_STATUSES.has(status)) return 'success';
  return 'queued';
}

function buildHarnessModel(data, flow, lang = 'de', channelAccounts = []) {
  const tasks = applyHarnessFlowStatus(buildTaskList(data), flow)
    .filter(isTaskOverviewItemVisible);
  const activeTask = tasks.find(taskIsHarnessActive) || null;
  const activeRun = data.runs.find((run) => run.status === 'running') || null;
  const liveWork = Boolean(activeTask || activeRun);
  const displayFlow = shouldDisplayHarnessFlow(flow, tasks) ? flow : emptyHarnessFlow('no_live_work');
  const observedIds = reconcileObservedPathWithAuthoritativeTask(
    observedPathFromFlow(displayFlow),
    activeTask,
    displayFlow,
  );
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
      ...flowNodeCopy(node, lang),
      machinePhase: node.phase,
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
    inboundChannels: buildInboundChannels(tasks, channelAccounts),
    inboundChannelsAvailable: channelAccounts !== null,
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
  const summary = terminalSummaryFromFlow(flowResult) || (terminalNode === 'passed' ? 'Completed by the crew' : 'The crew could not complete this task');
  return tasks.map((task) => {
    if (!ids.has(task.id) && !ids.has(task.taskId) && !ids.has(task.commandId) && !ids.has(task.runId)) return task;
    if (authoritativeTaskStatus(task)) return task;
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
    title: ticket.title || ticket.summary || ticket.id || 'Ticket',
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

function buildInboundChannels(tasks, accounts = []) {
  const channels = new Map();
  for (const account of accounts || []) {
    if (!account?.channel || account._deleted === true || account.is_deleted === true || account.enabled === false) continue;
    const key = normalizeInboundChannel(account.channel);
    // Native accounts also contain internal scheduling routes; those are not
    // communication adapters (see channels::configured_channel_summary).
    if (['queue', 'cron', 'plan'].includes(key)) continue;
    // A task's module is provenance, not an installed communication adapter.
    if (!channels.has(key)) channels.set(key, { id: key, label: inboundChannelLabel(key), count: 0, active: false });
  }
  for (const item of tasks || []) {
    if (channels.has(inferInboundChannel(item))) addInboundChannel(channels, item);
  }
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
  // Owner-Befund 04.09.2026: "wenn ich auf die ID klicke, warum komm ich dann
  // zu Käfer dialog?" - Wer aus dem Chat auf einen bestimmten Task klickt, hat
  // GENAU DIESEN gemeint. Ist er noch nicht im Modell (der Task wird gerade
  // erst angelegt, das Modell hinkt Sekunden bis Minuten hinterher), wurde hier
  // stillschweigend ein FREMDER Task ausgewaehlt - der erste der Liste, in der
  // Praxis ein alter Fehlerbericht. Der Nutzer landete bei etwas, das er nie
  // angeklickt hat, und hielt die Verfolgung fuer kaputt.
  //
  // Ein angeforderter Task wird jetzt nicht mehr ersetzt: bis er auftaucht,
  // bleibt die bisherige Auswahl stehen (oder gar keine), und der Aufrufer
  // kann das als "wird noch geladen" anzeigen. Sobald das Modell ihn kennt,
  // greift der Treffer oben.
  const angefordert = Boolean(focusTask && (focusTask.taskId || focusTask.commandId));
  if (angefordert) {
    return previousId && model.tasks.some((task) => task.id === previousId) ? previousId : null;
  }
  if (previousId && model.tasks.some((task) => task.id === previousId)) return previousId;
  const groups = taskGroups(model.tasks);
  return (groups.current[0] || groups.waiting[0] || groups.blocked[0] || groups.done[0] || model.tasks[0]).id;
}

function reconcileSelection(state) {
  const previousTaskId = state.selectedTaskId;
  const previousStepIndex = state.selectedStepIndex;
  state.selectedTaskId = resolveSelectedTaskId(state.model, state.focusTask, state.selectedTaskId);
  // A deep link is a one-time request: once the task is on screen the operator
  // may select anything else, and a later mount must not jump back to it.
  if (state.focusTask && !state.focusTaskConsumed && isFocusedTask(getSelectedTask(state), state.focusTask)) {
    state.focusTaskConsumed = true;
    clearPersistedFocusTask();
  }
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
  state.detailDrawer = state.focusTaskOpenDrawer ? { type: 'task', taskId: task.id } : null;
  if (!state.focusTaskOpenDrawer) state.ctx.closeDrawers();
  state.jobEditorOpen = false;
  state.historyOpen = false;
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
  applyLiveFlow(state);
  renderMain(state);
  if (options.center !== false) centerSelectedNode(state);
  syncDetailDrawer(state);
  void refreshSelectedTaskLive(state).catch((error) => {
    if (!state.disposed) console.warn('[ctox] selected task live load failed', error);
  });
}

function setTimelineStep(state, nextIndex, options = {}) {
  state.selectedNodeId = '';
  state.selectedStepIndex = clampIndex(nextIndex, state.model?.timeline?.length || 1);
  state.userNavigatedTimeline = true;
  if (options.light) {
    patchTimelinePanel(state);
    return;
  }
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
  if (options.light) {
    patchTimelinePanel(state);
    return;
  }
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
  if (state.detailDrawer.type === 'webstack') {
    state.ctx.openLeftDrawer(webStackDrawer(state));
    return;
  }
  if (state.detailDrawer.type === 'new-member') {
    return;
  }
  if (state.detailDrawer.type === 'member') {
    const member = crewMemberById(state, state.detailDrawer.memberId);
    if (!member || drawerIsBusy()) return;
    // Rebuild the drawer only when its facts changed; a rebuild resets the
    // scroll position, so keep it across the swap.
    const data = state.memberDrawerData?.memberId === member.id ? state.memberDrawerData : null;
    const signature = JSON.stringify([member.id, member.updated_at_ms, member.state, memberCreatureState(member), data?.runs?.length || 0, data?.runs?.[0]?.finished_at_ms || 0]);
    if (state.memberDrawerSignature === signature) return;
    state.memberDrawerSignature = signature;
    const previous = document.querySelector('.ctox-member-drawer');
    const scroller = previous ? scrollParentOf(previous) : null;
    const scrollTop = scroller?.scrollTop || 0;
    state.ctx.openLeftDrawer(crewMemberDrawer(member, state));
    if (scrollTop) {
      const next = document.querySelector('.ctox-member-drawer');
      const nextScroller = next ? scrollParentOf(next) : null;
      if (nextScroller) nextScroller.scrollTop = scrollTop;
    }
    return;
  }
  if (state.detailDrawer.type === 'node') {
    const node = state.model?.nodeMap?.get(state.detailDrawer.nodeId)
      || state.model?.timeline?.[clampIndex(state.selectedStepIndex, state.model.timeline.length)];
    if (node) state.ctx.openLeftDrawer(flowNodeDrawer(node, getSelectedTask(state), state));
  }
}

function scrollParentOf(node) {
  let current = node;
  while (current && current !== document.body) {
    const overflow = getComputedStyle(current).overflowY;
    if ((overflow === 'auto' || overflow === 'scroll') && current.scrollHeight > current.clientHeight) return current;
    current = current.parentElement;
  }
  return null;
}

function closeDetailDrawer(state) {
  const wasWebStack = state.detailDrawer?.type === 'webstack';
  state.detailDrawer = null;
  state.ctx.closeDrawers();
  if (wasWebStack && state.model) renderMain(state);
}

function taskDrawer(task, state, { editorOnly = false } = {}) {
  const t = labels[state.lang];
  const steps = taskSteps(task, state);
  const selectedTaskStepIndex = clampMetric(state.selectedTaskStepIndex || 0, 0, Math.max(steps.length - 1, 0));
  const displayTitle = taskDisplayTitle(task, state);
  const titleField = taskFieldDisplay(task.title || '', state);
  const promptField = taskPromptDisplay(task, state);
  const summary = taskDetailText(itemSummary(task) || '', state);
  const resultSummaryText = String(task.resultSummary || '').trim() === promptField.text
    ? '' : taskDetailText(task.resultSummary || '', state);
  const target = displayPathLike(task.target || task.commandId || task.taskId || '');
  const sourceLine = [
    displayWorkSource(task.source || task.moduleId || 'ctox'),
    formatShortTimestamp(task.createdAt || task.startedAt || task.timestamp),
  ].filter(Boolean).join(' · ');
  const showSummary = summary && summary !== task.target && summary !== task.commandId && summary !== task.taskId
    && summary !== taskDetailText(promptField.text, state);
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
      <button class="ctox-pane-icon ctox-drawer-close" type="button" data-close-ctox-drawer aria-label="${escapeAttr(t.close)}" title="${escapeAttr(t.close)}">${actionIcon(state, 'close')}</button>
    </header>
    <section class="ctox-callout ${['blocked', 'failed'].includes(normalizeCommandStatus(task.routeStatus || task.status)) ? 'is-danger' : 'is-info'} ctox-task-status-strip">
      <div>
        <strong class="ctox-badge ${statusBadgeVariant(statusClass(task.routeStatus || task.status))}">${escapeHtml(displayStatus(task.routeStatus || task.status, state.lang))}</strong>
        ${target ? `<small>${escapeHtml(target)}</small>` : ''}
      </div>
      ${taskSummaryReason(task, state) ? `<p class="ctox-task-reason-line">${escapeHtml(taskSummaryReason(task, state))}</p>` : ''}
      ${taskLeaseLineMarkup(task, state)}
      ${taskLiveStatusMarkup(task, state)}
      ${taskControlsMarkup(task, state)}
    </section>
    ${!editorOnly && promptField.text ? `<section class="ctox-task-description"><h3>${escapeHtml(t.taskPrompt)}</h3><p>${escapeHtml(promptField.text)}</p></section>` : ''}
    <details class="ctox-drawer-edit-fold" ${editorOnly ? 'open' : ''}>
    <summary>${escapeHtml(t.editTask)}</summary>
    <form class="ctox-card ctox-task-edit" data-ctox-task-edit>
      <header>
        <div class="ctox-task-edit-heading">
          <span>${escapeHtml(t.editTask)}</span>
          ${canModifyCtoxApp(state) ? '' : `<small>${escapeHtml(t.chefAdminOnly)}</small>`}
        </div>
        <div class="ctox-pane-actions">
          ${task.commandId ? `<button type="button" class="ctox-pane-icon" data-ctox-open-chat aria-label="${escapeAttr(t.openInChat)}" title="${escapeAttr(t.openInChat)}">${actionIcon(state, 'chat')}</button>` : ''}
          ${canResumeCtoxTask(task) ? `<button type="button" class="ctox-pane-icon" data-ctox-task-resume aria-label="${escapeAttr(t.resumeTask)}" title="${escapeAttr(t.resumeTask)}" ${state.ctx?.commandBus?.dispatch ? '' : 'disabled'}>${actionIcon(state, 'play')}</button>` : ''}
          <button type="button" class="ctox-pane-icon" data-ctox-task-delete aria-label="${escapeAttr(t.deleteTask)}" title="${escapeAttr(t.deleteTask)}" ${canModifyCtoxApp(state) ? '' : 'disabled'}>${actionIcon(state, 'trash')}</button>
        </div>
      </header>
      <div class="ctox-card-body">
        <label class="ctox-task-edit-field">
          <span class="ctox-field-label">${escapeHtml(t.taskTitle)}</span>
          <input class="ctox-input" type="text" name="title" value="${escapeAttr(titleField.text)}" ${canModifyCtoxApp(state) ? '' : 'disabled'}>
        </label>
        <label class="ctox-task-edit-field">
          <span class="ctox-field-label">${escapeHtml(t.taskPrompt)}</span>
          <textarea class="ctox-textarea" name="prompt" rows="4" ${canModifyCtoxApp(state) ? '' : 'disabled'}>${escapeHtml(promptField.text)}</textarea>
        </label>
        <label class="ctox-task-edit-field">
          <span class="ctox-field-label">${escapeHtml(t.priority)}</span>
          <select class="ctox-select" name="priority" ${canModifyCtoxApp(state) ? '' : 'disabled'}>
            ${['urgent', 'high', 'normal', 'low'].map((priority) => `<option value="${priority}" ${String(task.priority || 'normal') === priority ? 'selected' : ''}>${escapeHtml(displayPriority(priority, state.lang))}</option>`).join('')}
          </select>
        </label>
      </div>
      <footer class="ctox-task-edit-footer">
        <button type="submit" class="ctox-button is-primary" ${canModifyCtoxApp(state) ? '' : 'disabled'}>${escapeHtml(t.saveTask)}</button>
        <small data-ctox-task-action-status></small>
      </footer>
    </form>
    </details>
    ${taskDiagnosticMarkup(task, state)}
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
  if (editorOnly) {
    for (const child of [...body.children]) {
      if (!child.matches('.ctox-drawer-edit-fold, .ctox-task-status-strip')) child.remove();
    }
  }
  const editForm = body.querySelector('[data-ctox-task-edit]');
  const draft = state.taskEditDrafts?.get(task.id);
  for (const name of ['title', 'prompt', 'priority']) {
    if (draft && editForm?.elements.namedItem(name)) editForm.elements.namedItem(name).value = draft[name];
  }
  editForm?.addEventListener('input', (event) => {
    state.taskEditDrafts ||= new Map();
    state.taskEditDrafts.set(task.id, Object.fromEntries(
      ['title', 'prompt', 'priority'].map((name) => [name, event.currentTarget.elements.namedItem(name).value]),
    ));
  });
  body.querySelector('[data-ctox-task-delete]')?.addEventListener('click', async () => {
    await deleteCtoxTaskFromDrawer(state, task, body);
  });
  body.querySelector('[data-ctox-task-resume]')?.addEventListener('click', async () => {
    await resumeCtoxTaskFromDrawer(state, task, body);
  });
  body.querySelectorAll('[data-open-crew-member]').forEach((button) => {
    button.addEventListener('click', () => openCrewMemberDrawer(state, button.dataset.openCrewMember));
  });
  body.querySelector('[data-ctox-open-chat]')?.addEventListener('click', () => {
    openTaskInChat(state, task);
  });
  body.querySelectorAll('[data-ctox-task-control]').forEach((button) => {
    button.addEventListener('click', async () => {
      await runTaskControl(state, task, button.dataset.ctoxTaskControl, body);
    });
  });
  body.querySelector('[data-ctox-task-assign]')?.addEventListener('change', async (event) => {
    const memberId = String(event.currentTarget.value || '');
    if (memberId) await runTaskControl(state, task, 'assign', body, { memberId });
  });
  body.querySelectorAll('[data-drawer-task-step]').forEach((button) => {
    button.addEventListener('click', () => {
      setTaskTimelineStep(state, Number(button.dataset.drawerTaskStep), { center: true });
    });
  });
  return body;
}

function taskLeaseLineMarkup(task, state) {
  const t = labels[state.lang];
  const bits = [];
  const member = taskCrewMember(task, state);
  const memberBit = member
    ? `<button type="button" class="ctox-task-member" data-open-crew-member="${escapeAttr(member.id)}"><span class="ctox-flow-creature-shell ctox-task-member-portrait">${memberCreatureHtml(member, state)}</span>${escapeHtml(task.crewMemberId === member.id ? member.name : `${t.assignedTo} ${member.name}`)}</button>`
    : '';
  if (task.leaseOwner) bits.push(`${t.leaseOwner}: ${task.leaseOwner}${task.leaseExpiresAt ? ` (${t.until} ${formatClockTime(task.leaseExpiresAt)})` : ''}`);
  if (Number.isFinite(task.attempt) && task.attempt > 0) bits.push(`${t.attemptLabel} ${task.attempt}`);
  if (!bits.length && !memberBit) return '';
  const selection = taskSelectionSentence(task, state);
  return `<small class="ctox-task-lease-line">${memberBit}${bits.map((bit) => `<span>${escapeHtml(bit)}</span>`).join('')}</small>${selection ? `<small class="ctox-task-selection-line">${escapeHtml(selection)}</small>` : ''}`;
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
    state.taskEditDrafts?.delete(task.id);
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
    throw new Error('Die Verbindung zur Crew ist nicht verfügbar.');
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
      <button class="ctox-pane-icon ctox-drawer-close" type="button" data-close-ctox-drawer aria-label="${escapeAttr(t.close)}" title="${escapeAttr(t.close)}">${actionIcon(state, 'close')}</button>
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
  const index = state.userNavigatedTimeline ? taskIndex : (activeIndex >= 0 ? activeIndex : Math.max(0, byTimeline));
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

function reconcileObservedPathWithAuthoritativeTask(observedIds, task, flowResult) {
  const currentNodeId = authoritativeTaskNodeId(task);
  if (!currentNodeId) return observedIds;
  const base = flowMatchesTask(flowResult, task)
    ? observedIds.filter((id) => !['passed', 'model-failed', 'infra-failed'].includes(id))
    : [];
  const reconciled = [...base];
  for (const nodeId of authoritativeTaskPath(currentNodeId)) {
    if (!reconciled.includes(nodeId)) reconciled.push(nodeId);
  }
  if (reconciled.at(-1) !== currentNodeId) reconciled.push(currentNodeId);
  return reconciled.filter((id) => REVIEW_HARNESS_NODE_SET.has(id));
}

function authoritativeTaskPath(nodeId) {
  if (nodeId === 'queued') return ['queued'];
  if (nodeId === 'leased') return ['queued', 'leased'];
  if (nodeId === 'running') return ['queued', 'leased', 'running'];
  if (nodeId === 'awaiting-review') return ['queued', 'leased', 'running', 'awaiting-review'];
  if (nodeId === 'validating') return ['queued', 'leased', 'running', 'awaiting-review', 'awaiting-validation', 'validating'];
  return [nodeId];
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
  const runtimeQueue = queueTasks.map((doc) => ({
    id: doc.id || doc.task_id || doc.command_id,
    taskId: doc.task_id || doc.id || '',
    commandId: doc.command_id || '',
    title: doc.title || doc.command_type || doc.id || 'Aufgabe',
    prompt: doc.prompt || doc.payload?.prompt || doc.payload?.instruction || '',
    source: doc.source_module || doc.module || 'ctox',
    channel: inferInboundChannel(doc),
    priority: doc.priority || 'normal',
    status: routingProblemStatus(doc) || normalizeCommandStatus(doc.status || doc.task_status || doc.route_status),
    routeStatus: doc.route_status || '',
    target: doc.command_type || doc.thread_key || 'ctox queue',
    browserContextArtifact: doc.browser_context_artifact || null,
    result: doc.result || null,
    resultSummary: resultSummary(doc.result),
    // Durable execution telemetry from the service projection. Without this the
    // metric strip and the progress bar have no measured input at all.
    executionProgress: normalizeExecutionProgress(doc.execution_progress || doc.executionProgress),
    leasedAt: doc.leased_at || '',
    ackedAt: doc.acked_at || '',
    // Durable routing truth (PR #58/#59/#61): who holds the lease, why the task
    // waits, when it retries, how it failed, and which crew member is bound.
    leaseOwner: doc.lease_owner || '',
    leaseWorkerId: doc.lease_worker_id || '',
    leaseExpiresAt: doc.lease_expires_at || '',
    firstPendingAt: doc.first_pending_at || '',
    attempt: Number.isFinite(Number(doc.attempt)) ? Number(doc.attempt) : null,
    failureClass: doc.failure_class || '',
    failureAttemptCount: Number.isFinite(Number(doc.failure_attempt_count)) ? Number(doc.failure_attempt_count) : 0,
    retryNotBefore: doc.retry_not_before || '',
    holdReason: doc.hold_reason || '',
    waitEntityType: doc.wait_entity_type || '',
    waitEntityId: doc.wait_entity_id || '',
    statusNote: doc.status_note || '',
    error: doc.error || '',
    crewMemberId: doc.crew_member_id || '',
    crewAssignedMemberId: doc.crew_assigned_member_id || '',
    updatedAtMs: Number.isFinite(Number(doc.updated_at_ms)) ? Number(doc.updated_at_ms) : null,
    createdAt: new Date(doc.updated_at_ms || Date.now()).toISOString(),
    updatedAt: new Date(doc.updated_at_ms || Date.now()).toISOString(),
  })).filter((item) => item.id);
  const runtimeByTaskId = new Map(runtimeQueue.map((item) => [String(item.taskId || item.id), item]));
  const runtimeByCommandId = new Map(runtimeQueue
    .filter((item) => item.commandId)
    .map((item) => [String(item.commandId), item]));
  const commandQueue = commands
    .filter((doc) => isQueuedCommandLifecycle(doc) || isLegacyBrowserExtractCommand(doc))
    .map((doc) => commandTaskFromProjection(doc, runtimeByTaskId, runtimeByCommandId))
    .filter(Boolean);
  const tickets = bugReports.map((doc) => ({
    id: doc.id || doc.report_id,
    title: doc.title || doc.surface || doc.id || 'Ticket',
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

function commandTaskFromProjection(doc, runtimeByTaskId, runtimeByCommandId) {
  const commandId = String(doc.command_id || doc.id || '').trim();
  if (!commandId) return null;
  const executionTaskId = commandExecutionTaskId(doc);
  const queueTask = (executionTaskId && runtimeByTaskId.get(executionTaskId))
    || runtimeByCommandId.get(commandId)
    || null;
  const lifecycle = hasAuthoritativeCommandLifecycle(doc);
  const extractArtifact = isLegacyBrowserExtractCommand(doc)
    ? browserExtractArtifactFromCommand(doc)
    : null;
  // The durable routing state is the truth (a failed/blocked/cancelled queue
  // task must never count as "working" because the command lifecycle still
  // says running) — Befund "Arbeitet (4)" bei vier Fehlern, 05.09.2026.
  const routingTruth = normalizeCommandStatus(queueTask?.routeStatus || '');
  const status = HARNESS_PROBLEM_TERMINAL_STATUSES.has(routingTruth)
    ? routingTruth
    : (lifecycle ? authoritativeTaskStatus(doc) : normalizeCommandStatus(doc.status));
  const taskId = executionTaskId || queueTask?.taskId || '';
  return {
    ...(queueTask || {}),
    id: queueTask?.id || taskId || `command-${commandId}`,
    taskId,
    commandId,
    title: doc.payload?.title
      || queueTask?.title
      || (extractArtifact
        ? `Browser Extract: ${extractArtifact.source_id || extractArtifact.capture_script || commandId}`
        : displayCommandTitle(doc)),
    prompt: doc.payload?.instruction || queueTask?.prompt || '',
    source: doc.module || doc.payload?.source_module || queueTask?.source || 'ctox',
    channel: inferInboundChannel(doc),
    priority: doc.payload?.priority || queueTask?.priority || 'normal',
    status,
    routeStatus: HARNESS_PROBLEM_TERMINAL_STATUSES.has(routingTruth)
      ? String(queueTask?.routeStatus || routingTruth)
      : (lifecycle
        ? (String(doc.execution_phase) === 'terminal'
          ? String(doc.terminal_status || doc.status || 'terminal')
          : String(doc.execution_phase))
        : (doc.task_status || doc.status || '')),
    executionPhase: lifecycle ? String(doc.execution_phase) : '',
    execution_phase: lifecycle ? String(doc.execution_phase) : '',
    terminalStatus: lifecycle ? String(doc.terminal_status || 'none') : '',
    terminal_status: lifecycle ? String(doc.terminal_status || 'none') : '',
    projectionVersion: lifecycle ? Number(doc.projection_version || 0) : null,
    target: doc.command_type || queueTask?.target || 'business_os.command',
    // The service projects `execution_progress` onto business_commands as well;
    // prefer the command copy, fall back to the queue-task copy.
    executionProgress: normalizeExecutionProgress(doc.execution_progress || doc.executionProgress)
      || queueTask?.executionProgress
      || null,
    updatedAtMs: Number.isFinite(Number(doc.updated_at_ms))
      ? Number(doc.updated_at_ms)
      : (queueTask?.updatedAtMs ?? null),
    browserExtractArtifact: extractArtifact,
    result: doc.result || queueTask?.result || null,
    resultSummary: (extractArtifact ? browserExtractSummary(extractArtifact.fields) : '')
      || resultSummary(doc.result)
      || queueTask?.resultSummary
      || '',
    createdAt: new Date(doc.created_at_ms || Date.parse(queueTask?.createdAt || '') || doc.updated_at_ms || Date.now()).toISOString(),
    updatedAt: new Date(doc.updated_at_ms || Date.parse(queueTask?.updatedAt || '') || Date.now()).toISOString(),
  };
}

function commandExecutionTaskId(doc = {}) {
  const explicit = String(doc.execution_task_id || '').trim();
  if (explicit) return explicit;
  if (Number(doc.contract_version) === 2 || doc.execution_mode === 'control') return '';
  return String(doc.task_id || '').trim();
}

function hasAuthoritativeCommandLifecycle(doc = {}) {
  return Boolean(String(doc.execution_phase || '').trim());
}

function isQueuedCommandLifecycle(doc = {}) {
  if (!hasAuthoritativeCommandLifecycle(doc)) return false;
  return doc.execution_mode === 'queue'
    || Boolean(String(doc.execution_task_id || '').trim());
}

function isLegacyBrowserExtractCommand(doc = {}) {
  return doc.command_type === 'browser.capture.extract' || Boolean(doc.result?.extract);
}

function isQueueOverviewItemVisible(item) {
  return isTaskOverviewItemVisible(item);
}

function isTaskOverviewItemVisible(item) {
  const statuses = taskStatusCandidates(item);
  // Keep bounded native queue failures inspectable/retryable. Legacy tickets
  // without routing evidence still follow the existing overview filter.
  if (statuses.some((status) => HARNESS_PROBLEM_TERMINAL_STATUSES.has(status))) return Boolean(item.routeStatus || item.route_status);
  if (statuses.some((status) => HARNESS_WAITING_STATUSES.has(status) || HARNESS_ACTIVE_STATUSES.has(status))) return true;
  if (statuses.some((status) => HARNESS_SUCCESS_STATUSES.has(status))) return true;
  if (item?.priority === 'urgent') return true;
  return !statuses.some((status) => HARNESS_TERMINAL_STATUSES.has(status));
}

function taskStatusCandidates(item = {}) {
  const authoritative = authoritativeTaskStatus(item);
  if (authoritative) return [authoritative];
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
    email: 'E-Mail',
    whatsapp: 'WhatsApp',
    teams: 'Microsoft Teams',
    google_chat: 'Google Chat',
    'business_os.llm.chat': 'LLM Chat',
    'business-os': 'Business OS',
    ctox: 'Crew',
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
  if (typeof location === 'undefined') return null;
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

function routingProblemStatus(task = {}) {
  const route = normalizeCommandStatus(task?.routeStatus || task?.route_status || '');
  // A handled route still needs the command's review/terminal evidence; it
  // must not mask an explicitly completed command as an unverified outcome.
  return ['failed', 'cancelled', 'blocked'].includes(route) ? route : '';
}

function authoritativeTaskStatus(task = {}) {
  task = task || {};
  const routing = routingProblemStatus(task);
  if (routing) return routing;
  const durablePhase = taskExecutionProgress(task)?.phase || '';
  const phase = String(task.executionPhase || task.execution_phase || durablePhase).trim().toLowerCase();
  if (!phase) return '';
  if (phase === 'terminal') {
    return normalizeCommandStatus(task.terminalStatus || task.terminal_status || task.status || 'completed');
  }
  if (['waiting_dependencies', 'waiting-dependencies', 'accepted', 'queued', 'retry_wait', 'retry-wait'].includes(phase)) return 'queued';
  if (['leased', 'running', 'work', 'working'].includes(phase)) return 'running';
  if (['review', 'awaiting_review', 'awaiting-review', 'validating'].includes(phase)) return 'review';
  if (phase === 'completed') return 'completed';
  if (phase === 'blocked') return 'blocked';
  return normalizeCommandStatus(phase);
}

function authoritativeTaskNodeId(task = {}) {
  task = task || {};
  const routing = routingProblemStatus(task);
  if (routing) return routeStatusNodeId(routing);
  const phase = String(task.executionPhase || task.execution_phase || '').trim().toLowerCase();
  if (!phase) return routeStatusNodeId(task.routeStatus || task.status);
  if (['waiting_dependencies', 'waiting-dependencies', 'accepted', 'queued', 'retry_wait', 'retry-wait'].includes(phase)) return 'queued';
  if (phase === 'leased') return 'leased';
  if (phase === 'running') return 'running';
  if (phase === 'awaiting_review' || phase === 'awaiting-review') return 'awaiting-review';
  if (phase === 'validating') return 'validating';
  if (phase === 'blocked') return 'model-failed';
  if (phase === 'terminal') {
    const terminal = normalizeCommandStatus(task.terminalStatus || task.terminal_status || task.status);
    return terminal === 'completed' ? 'passed' : 'model-failed';
  }
  return routeStatusNodeId(phase);
}

function normalizeCommandStatus(status) {
  const value = String(status || '').toLowerCase();
  if (['accepted', 'pending', 'waiting_dependencies', 'waiting-dependencies', 'retry_wait', 'retry-wait'].includes(value)) return 'queued';
  if (value === 'leased' || value === 'working') return 'running';
  if (['awaiting_review', 'awaiting-review', 'validating'].includes(value)) return 'review';
  if (value === 'done') return 'completed';
  if (value === 'handled') return 'handled';
  if (value === 'cancelled' || value === 'canceled') return 'cancelled';
  if (value === 'blocked' || value === 'stale_missing_native') return 'blocked';
  if (['failed', 'fail', 'error', 'errored', 'model_failed', 'model-failed', 'infra_failed', 'infra-failed'].includes(value)) return 'failed';
  return value || 'queued';
}

function routeStatusNodeId(status) {
  const value = String(status || '').toLowerCase();
  if (['accepted', 'pending', 'queued', 'waiting_dependencies', 'waiting-dependencies', 'retry_wait', 'retry-wait'].includes(value)) return 'queued';
  if (value === 'leased') return 'leased';
  if (value === 'running' || value === 'working') return 'running';
  if (value === 'awaiting_review' || value === 'awaiting-review' || value === 'review') return 'awaiting-review';
  if (value === 'validating') return 'validating';
  if (value === 'completed' || value === 'done' || value === 'handled' || value === 'terminal') return 'passed';
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
  if (state.disposed) return;
  syncDetailDrawer(state);
}

async function verifyWebStackCredential(state, sourceId, secretName) {
  const source = (state.webStack?.data?.sources || []).find((candidate) => candidate.id === sourceId);
  const configured = Boolean(source?.credential?.configured);
  state.webStack = {
    ...(state.webStack || {}),
    loading: false,
    error: '',
    notice: configured
      ? `${secretName || sourceId}: Zugangsdaten sind sicher gespeichert.`
      : `${secretName || sourceId}: Zugangsdaten fehlen. Bitte hinterlegen.`,
  };
  renderMain(state);
}

async function requestWebStackAuthAssist(state, source) {
  const t = labels[state.lang];
  if (!state.ctx?.commandBus?.dispatch) {
    state.webStack = { ...(state.webStack || {}), error: 'Die Verbindung zur Crew ist nicht verfügbar.' };
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
  // Newest first, bounded at the query (never "first 200 by primary key, then
  // keep 20" — that hid the newest work on busy workspaces, Befund 05.09.2026).
  let localDocs;
  try {
    localDocs = await collection.find({
      selector: { updated_at_ms: { $gt: 0 } },
      sort: [{ updated_at_ms: 'desc' }],
      limit: LOCAL_COLLECTION_LIMIT,
    }).exec();
  } catch {
    const fallback = await collection.find().limit(LOCAL_COLLECTION_LIMIT).exec();
    localDocs = fallback.sort((left, right) => (right.updated_at_ms || 0) - (left.updated_at_ms || 0));
  }
  return localDocs.map((doc) => doc.toJSON());
}

async function loadLocalChannelAccounts(ctx) {
  const collection = ctoxCollection(ctx, 'communication_accounts');
  if (!collection) return null;
  const docs = await collection.find({ selector: {}, limit: 200 }).exec();
  // No account addresses, credentials or adapter diagnostics enter this view.
  return docs.map((doc) => {
    const account = doc.toJSON();
    return { channel: account.channel, enabled: account.enabled, _deleted: account._deleted, is_deleted: account.is_deleted };
  });
}

async function loadLocalCrewMembers(ctx) {
  const collection = ctoxCollection(ctx, 'ctox_crew_members');
  if (!collection) return [];
  const docs = await collection.find({ selector: { archived: false }, limit: 64 }).exec();
  return docs.map((doc) => doc.toJSON());
}

function crewMemberById(state, memberId) {
  if (!memberId) return null;
  return (state?.crewMembers || []).find((member) => member.id === memberId) || null;
}

async function loadLocalHarnessStatus(ctx) {
  const collection = ctoxCollection(ctx, 'ctox_harness_status');
  if (!collection) return null;
  const doc = await collection.findOne({ selector: { id: 'harness' } }).exec();
  return doc ? doc.toJSON() : null;
}

// --- Live data of the selected task (slice 3) -------------------------------
// The flow canvas, the creature impulse and the metric strip follow the durable
// per-task projections (`ctox_harness_events`, `ctox_runs`) instead of the one
// global `runtime_settings.harness_flow` blob, which only ever described the task
// the server last looked at. The blob stays a richer superset when it matches.

function taskLiveKey(task) {
  if (!task) return '';
  return nativeTaskId(task) || String(task.commandId || task.id || '');
}

async function findLocalDocs(collection, selector, limit, sortField = 'updated_at_ms', direction = 'desc') {
  try {
    const docs = await collection.find({ selector, sort: [{ [sortField]: direction }], limit }).exec();
    return docs.map((doc) => doc.toJSON());
  } catch {
    const docs = await collection.find({ selector, limit }).exec();
    const rows = docs.map((doc) => doc.toJSON());
    rows.sort((a, b) => (Number(b?.[sortField]) || 0) - (Number(a?.[sortField]) || 0));
    return direction === 'desc' ? rows : rows.reverse();
  }
}

async function loadLocalHarnessEvents(ctx, task) {
  const collection = ctoxCollection(ctx, 'ctox_harness_events');
  const taskId = nativeTaskId(task);
  if (!collection || !taskId) return [];
  let rows = await findLocalDocs(collection, { task_id: taskId }, HARNESS_EVENT_LIMIT);
  if (!rows.length && task?.commandId) {
    rows = await findLocalDocs(collection, { command_id: task.commandId }, HARNESS_EVENT_LIMIT);
  }
  // Newest 200 from the store, handed on oldest first.
  return rows.reverse();
}

async function loadLocalRunsForTask(ctx, task) {
  const collection = ctoxCollection(ctx, 'ctox_runs');
  const taskId = nativeTaskId(task);
  if (!collection || !taskId) return [];
  return findLocalDocs(collection, { task_id: taskId }, 32);
}

async function loadSelectedTaskLive(ctx, task) {
  const key = taskLiveKey(task);
  if (!key) return { key: '', events: [], runs: [], flow: null };
  const [events, runs] = await Promise.all([
    loadLocalHarnessEvents(ctx, task).catch(() => []),
    loadLocalRunsForTask(ctx, task).catch(() => []),
  ]);
  return { key, events, runs, flow: harnessFlowFromEvents(task, events) };
}

const HARNESS_EVENT_LEDGER_KINDS = {
  tool_started: 'worker.tool_started',
  tool_completed: 'worker.tool_completed',
  thinking: 'worker.thinking',
  plan_updated: 'worker.plan_updated',
  token_usage: 'worker.token_usage',
  turn_completed: 'worker.turn_completed',
  phase: 'worker.phase',
  crew_selected: 'crew.selected',
  crew_selection_unavailable: 'crew.selection_unavailable',
};

function hasFiniteValue(value) {
  return value !== null && value !== undefined && Number.isFinite(Number(value));
}

// Projected events carry the same facts as the ledger rows the server used to
// build `harness_flow`; this rebuilds the ledger shape so the existing readers
// (observedPathFromFlow, observedDetailsFromFlow, aggregateFlowMetrics) work
// unchanged. Token usage events are cumulative per attempt (direct_session).
function harnessFlowFromEvents(task, events) {
  if (!Array.isArray(events) || !events.length) return null;
  const ledger = events.map((event) => {
    const metadata = {};
    if (event.tool_name || event.tool_type) {
      metadata.tool = { name: event.tool_name || '', type: event.tool_type || '', call_id: event.call_id || '', success: event.success ?? null };
    }
    const usage = event.usage || {};
    if ([usage.input, usage.output, usage.total].some(hasFiniteValue)) {
      metadata.usage = {
        input_tokens: hasFiniteValue(usage.input) ? Number(usage.input) : null,
        output_tokens: hasFiniteValue(usage.output) ? Number(usage.output) : null,
        reasoning_output_tokens: hasFiniteValue(usage.reasoning) ? Number(usage.reasoning) : null,
        total_tokens: hasFiniteValue(usage.total) ? Number(usage.total) : null,
      };
      if (event.kind === 'token_usage') metadata.metrics_mode = 'cumulative';
    }
    if (hasFiniteValue(event.runtime_seconds)) metadata.runtime = { seconds: Number(event.runtime_seconds) };
    if (hasFiniteValue(event.attempt)) metadata.attempt = Number(event.attempt);
    if (hasFiniteValue(event.step_position)) metadata.step_position = Number(event.step_position);
    return {
      event_id: String(event.id || ''),
      event_kind: HARNESS_EVENT_LEDGER_KINDS[event.kind] || `worker.${event.kind || 'phase'}`,
      title: String(event.title || ''),
      body_text: '',
      created_at: new Date(Number(event.created_at_ms) || 0).toISOString(),
      metadata_json: JSON.stringify(metadata),
    };
  });
  return {
    ok: true,
    mode: 'rxdb-webrtc',
    error: '',
    ascii: '',
    flow: {
      schema_version: 1,
      source: { message_key: nativeTaskId(task), work_id: null, source_kind: 'ctox_harness_events' },
      ledger_events: ledger,
      blocks: [],
    },
  };
}

function liveActivityFromEvents(events) {
  if (!Array.isArray(events) || !events.length) return null;
  let thinking = 0;
  let tools = 0;
  let lastKind = '';
  let updatedAtMs = 0;
  for (const event of events) {
    if (event.kind === 'thinking') { thinking += 1; lastKind = 'thinking'; }
    else if (event.kind === 'tool_started') { tools += 1; lastKind = 'tool'; }
    else if (event.kind === 'tool_completed') { lastKind = 'tool'; }
    updatedAtMs = Math.max(updatedAtMs, Number(event.created_at_ms) || 0);
  }
  return { total: thinking + tools, thinking, tools, last_kind: lastKind, updated_at_ms: updatedAtMs };
}

// The creature impulse (syncCrewProceduralMotion) reads activity turns from the
// execution progress. When the event stream is newer than the projected plan,
// the plan keeps its steps and takes the fresher activity counters.
function withLiveActivity(task, live) {
  if (!task || !live || live.key !== taskLiveKey(task)) return task;
  const activity = liveActivityFromEvents(live.events);
  if (!activity) return task;
  const raw = task.execution_progress || task.executionProgress;
  if (!raw || typeof raw !== 'object' || !Array.isArray(raw.steps) || !raw.steps.length) return task;
  const current = Number(raw.updated_at_ms ?? raw.updatedAtMs) || 0;
  if (current >= activity.updated_at_ms) return task;
  const merged = {
    ...raw,
    activity_turns: { total: activity.total, thinking: activity.thinking, tools: activity.tools, last_kind: activity.last_kind },
    updated_at_ms: activity.updated_at_ms,
  };
  return { ...task, execution_progress: merged, executionProgress: merged };
}

function aggregateRunMetrics(runs) {
  if (!Array.isArray(runs) || !runs.length) return null;
  const sum = (key) => {
    let total = null;
    for (const run of runs) {
      const value = run?.metrics?.[key];
      if (!hasFiniteValue(value)) continue;
      total = (total || 0) + Number(value);
    }
    return total;
  };
  const elapsed = sum('elapsed_ms');
  return {
    inputTokens: sum('input_tokens'),
    outputTokens: sum('output_tokens'),
    toolCalls: sum('tool_calls'),
    thinkingTurns: sum('thinking_turns'),
    seconds: elapsed === null ? null : Math.round(elapsed / 1000),
  };
}

// Which flow describes the selected task: the server blob when it is about this
// task, otherwise the task's own event stream, otherwise nothing (never another
// task's flow).
function flowForSelectedTask(state) {
  const task = getSelectedTask(state);
  const blob = state.blobFlow || emptyHarnessFlow();
  if (!task) return blob;
  if (flowMatchesTask(blob, task)) return blob;
  const live = state.selectedLive;
  if (live?.flow && live.key === taskLiveKey(task)) return live.flow;
  return emptyHarnessFlow('no_task_flow');
}

function applyLiveFlow(state) {
  if (!state.bundle) return false;
  const flow = flowForSelectedTask(state);
  if (flow === state.flow) return false;
  state.flow = flow;
  state.model = buildHarnessModel(state.bundle, flow, state.lang, state.channelAccounts);
  reconcileSelection(state);
  return true;
}

async function refreshSelectedTaskLive(state) {
  const task = getSelectedTask(state);
  const key = taskLiveKey(task);
  if (!key || state.selectedLive?.key === key) return;
  const live = await loadSelectedTaskLive(state.ctx, task);
  if (state.disposed || taskLiveKey(getSelectedTask(state)) !== key) return;
  state.selectedLive = live;
  applyLiveFlow(state);
  if (mainIsBusy(state)) state.mainRenderPending = true;
  else renderMain(state);
  syncLiveTicker(state);
  syncDetailDrawer(state);
}

// --- Render deferral while the operator is interacting -----------------------
// Data-driven renders rebuild the main pane. While a pointer is down on the
// timeline slider or the canvas, or an input inside the pane has focus, the
// rebuild waits; it runs as soon as the interaction ends.

// --- Three honest states (slice 6) ------------------------------------------
// idle -> crew at home (slice 4); sync not connected -> creatures sleep and the
// footer says so; load failed -> reason plus a retry button. Never an endless
// "syncing".

function syncIsConnected(state) {
  const sync = state?.ctx?.sync;
  if (!sync) return false;
  if (sync.mode !== 'webrtc') return false;
  const diagnostics = sync.diagnostics || null;
  if (diagnostics && typeof diagnostics === 'object') {
    if (diagnostics.peerConnected === false) return false;
    const channel = String(diagnostics.channelState || '').toLowerCase();
    if (channel && channel !== 'open') return false;
  }
  return true;
}

function mainIsBusy(state) {
  if (state.mainInteracting) return true;
  const main = state.ctx?.host?.querySelector?.('[data-ctox-main]');
  if (main?.querySelector('.ctox-more-actions[open]')) return true;
  const active = typeof document !== 'undefined' ? document.activeElement : null;
  if (!main || !active || !main.contains(active)) return false;
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(active.tagName);
}

function flushPendingMainRender(state) {
  if (!state.mainRenderPending || state.disposed || mainIsBusy(state)) return;
  state.mainRenderPending = false;
  renderMain(state);
  syncLiveTicker(state);
  syncDetailDrawer(state);
}

function wireMainInteractionGuard(state) {
  const main = state.ctx?.host?.querySelector?.('[data-ctox-main]');
  if (!main) return () => {};
  const begin = (event) => {
    const target = event.target?.closest?.('[data-timeline-range],[data-flow-canvas]');
    if (!target) return;
    state.mainInteracting = true;
    state.timelineScrubbing = target.matches('[data-timeline-range]');
  };
  const end = () => {
    if (!state.mainInteracting) return;
    state.mainInteracting = false;
    const scrubbed = state.timelineScrubbing;
    state.timelineScrubbing = false;
    if (scrubbed) {
      // The slider moved through light patches; settle with a full pane render.
      state.mainRenderPending = false;
      renderMain(state);
      centerSelectedNode(state);
      syncLiveTicker(state);
      syncDetailDrawer(state);
      return;
    }
    flushPendingMainRender(state);
  };
  const blur = () => {
    window.setTimeout(() => flushPendingMainRender(state), 0);
  };
  main.addEventListener('pointerdown', begin);
  main.addEventListener('focusout', blur);
  window.addEventListener('pointerup', end);
  window.addEventListener('pointercancel', end);
  return () => {
    main.removeEventListener('pointerdown', begin);
    main.removeEventListener('focusout', blur);
    window.removeEventListener('pointerup', end);
    window.removeEventListener('pointercancel', end);
  };
}

// While the slider is being dragged the range input must survive; only the
// parts around it are re-rendered from the same markup function.
function patchTimelinePanel(state) {
  const main = state.ctx?.host?.querySelector?.('[data-ctox-main]');
  const panel = main?.querySelector('.ctox-timeline-panel');
  if (!panel) {
    renderMain(state);
    return;
  }
  const model = state.model;
  const selectedTask = getSelectedTask(state);
  const timelineIndex = clampIndex(state.selectedStepIndex, model.timeline.length);
  const taskStepView = selectedTask ? selectedTaskStepView(selectedTask, state) : null;
  const selectedNode = taskStepView
    ? taskStepView.node
    : model.timeline[timelineIndex] || model.nodes.find((node) => node.id === model.activeNodeId) || model.nodes[0];
  const metricSubject = metricSubjectTask(state, selectedTask);
  const metrics = metricSubject ? taskTelemetry(metricSubject, state) : emptyTelemetry();
  const template = document.createElement('template');
  template.innerHTML = timelinePanel(state, selectedTask, selectedNode, metrics).trim();
  const next = template.content.firstElementChild;
  if (!next) return;
  panel.setAttribute('style', next.getAttribute('style') || '');
  panel.className = next.className;
  for (const selector of ['.ctox-timeline-head', '.ctox-timeline-scale', '.ctox-timeline-detail']) {
    const from = next.querySelector(selector);
    const to = panel.querySelector(selector);
    if (from && to) to.innerHTML = from.innerHTML;
  }
  const range = panel.querySelector('[data-timeline-range]');
  const nextRange = next.querySelector('[data-timeline-range]');
  if (range && nextRange) {
    range.max = nextRange.max;
    if (range.value !== nextRange.value) range.value = nextRange.value;
  }
  wireTimelineStepButtons(state, panel);
}

function wireTimelineStepButtons(state, root) {
  root.querySelectorAll('[data-timeline-step]').forEach((button) => {
    button.addEventListener('click', () => {
      setTimelineStep(state, Number(button.dataset.timelineStep), { center: true });
    });
  });
  root.querySelectorAll('[data-task-step-index]').forEach((button) => {
    button.addEventListener('click', () => {
      setTaskTimelineStep(state, Number(button.dataset.taskStepIndex), { center: true });
    });
  });
}

// Task -> chat: the shell window owns the chat bar. From inside the module
// frame the request travels up as a message; when the module is mounted
// inline, the same event is dispatched on the window directly.
function openTaskInChat(state, task) {
  const detail = { commandId: task?.commandId || '', taskId: nativeTaskId(task) || task?.id || '', source: 'ctox' };
  const message = { type: 'ctox-business-os-open-chat', ...detail };
  try {
    if (window.parent && window.parent !== window) {
      window.parent.postMessage(message, window.location.origin);
      return;
    }
  } catch {}
  window.dispatchEvent(new CustomEvent('ctox-business-os-open-chat', { detail }));
}

function clearPersistedFocusTask() {
  try {
    sessionStorage.removeItem('ctox.businessOs.focusTask');
  } catch {}
}

// --- Crew members as creatures (slice 4) ------------------------------------
// The creature of a task is the member that holds it (`crew_member_id`, or the
// owner's assignment while it waits). Name, colour and shape come from
// `ctox_crew_members`; the drawing itself stays the shared creature.

const NEUTRAL_CREW_COLOR = '#7d7f84';
const SOUL_AXES = Object.freeze([
  { key: 'gruendlichkeit_vs_tempo', left: 'axisThorough', right: 'axisFast' },
  { key: 'vorsicht_vs_mut', left: 'axisCareful', right: 'axisBold' },
  { key: 'knapp_vs_ausfuehrlich', left: 'axisTerse', right: 'axisThoroughText' },
  { key: 'regeltreu_vs_kreativ', left: 'axisByTheBook', right: 'axisCreative' },
  { key: 'nachfragen_vs_annehmen', left: 'axisAsks', right: 'axisAssumes' },
]);
const SPECIALTY_KEYS = Object.freeze(['modules', 'command_types', 'skills', 'tags']);

function memberIdentity(member) {
  if (!member) return null;
  return { name: String(member.name || ''), color: String(member.color || NEUTRAL_CREW_COLOR), shape: String(member.shape || 'round') };
}

function taskCrewMember(task, state) {
  if (!task) return null;
  return crewMemberById(state, task.crewMemberId || task.crew_member_id)
    || crewMemberById(state, task.crewAssignedMemberId || task.crew_assigned_member_id)
    || null;
}

function taskByNativeId(state, nativeId) {
  if (!nativeId) return null;
  return (state?.model?.tasks || []).find((task) => nativeTaskId(task) === nativeId || task.id === nativeId || task.taskId === nativeId) || null;
}

// Expression from the projection stamps (reading right after the memory was
// read, learning right after the tick), else the duty state.
function memberCreatureState(member, nowMs = Date.now()) {
  return crewMemberExpression(member, nowMs);
}

// reading/learning decay: re-render the crew when the earliest expression ends.
function armExpressionRefresh(state) {
  window.clearTimeout(state.expressionRefresh);
  state.expressionRefresh = null;
  if (state.disposed) return;
  const now = Date.now();
  const ttl = (state.crewMembers || [])
    .map((member) => crewMemberExpressionTtlMs(member, now))
    .filter((value) => value > 0);
  if (!ttl.length) return;
  state.expressionRefresh = window.setTimeout(() => {
    state.expressionRefresh = null;
    if (state.disposed) return;
    render(state);
    armExpressionRefresh(state);
  }, Math.min(...ttl) + 50);
}

function memberStateLine(member, state) {
  const t = labels[state.lang];
  const expression = memberCreatureState(member);
  if (expression === 'reading') return t.readingMemory;
  if (expression === 'learning') return t.learningFromAssignment;
  if (member?.state === 'on_duty') {
    const task = taskByNativeId(state, member.active_task_id);
    return task ? taskDisplayTitle(task, state) : t.onDuty;
  }
  if (member?.state === 'resting_after_failure') return t.restingAfterFailure;
  return t.atHome;
}

// What a member has become good at: derived from its successful attempts
// (server-side `domain`), never typed by the owner.
function memberDomainLine(member, state) {
  const t = labels[state.lang];
  const domain = Array.isArray(member?.domain) ? member.domain.filter(Boolean) : [];
  const total = Number(member?.stats?.tasks_total) || 0;
  const bits = [];
  if (domain.length) bits.push(domain.map((item) => displayWorkSource(item)).join(', '));
  else bits.push(t.noDomain);
  if (total > 0) bits.push(`${total} ${t.assignmentsWord}`);
  return bits.join(' · ');
}

function memberCreatureHtml(member, state, taskState = memberCreatureState(member)) {
  if (state?.ctx && !syncIsConnected(state)) taskState = 'idle';
  const task = member?.active_task_id ? taskByNativeId(state, member.active_task_id) : null;
  const liveTask = task ? withLiveActivity(task, state?.selectedLive) : null;
  return crewCreatureHtml({
    crewKey: member.id,
    crewIdentity: memberIdentity(member),
    executionProgress: liveTask?.executionProgress || liveTask?.execution_progress || null,
  }, taskState, 'map');
}

// While work runs, the rest of the crew stays in view as one quiet row under
// the metrics (Review-Befund B7): creature, name, what each one does now.
function crewStripMarkup(state) {
  const t = labels[state.lang];
  const members = (state.crewMembers || []).filter((member) => !member.archived);
  if (!members.length) return '';
  const items = members.map((member) => {
    const line = memberStateLine(member, state);
    const stateClass = String(member.state || 'home').replace(/[^a-z_]/g, '');
    return `
      <button type="button" class="ctox-crew-strip-member is-${escapeAttr(stateClass)}" data-crew-member-id="${escapeAttr(member.id)}"
        aria-label="${escapeAttr(`${member.name}: ${line}`)}" title="${escapeAttr(`${member.name} · ${line}`)}">
        <span class="ctox-flow-creature-shell ctox-crew-strip-creature">${memberCreatureHtml(member, state)}</span>
      </button>`;
  }).join('');
  return `<section class="ctox-crew-strip" aria-label="${escapeAttr(t.crewHome)}">${items}</section>`;
}

function crewHomeMarkup(state) {
  const t = labels[state.lang];
  const members = (state.crewMembers || []).filter((member) => !member.archived);
  const cards = members.map((member) => {
    const line = memberStateLine(member, state);
    const stateClass = String(member.state || 'home').replace(/[^a-z_]/g, '');
    return `
      <button type="button" class="ctox-crew-home-member is-${escapeAttr(stateClass)}" data-crew-member-id="${escapeAttr(member.id)}"
        aria-label="${escapeAttr(`${member.name}: ${line}`)}" title="${escapeAttr(`${member.name} · ${line}`)}">
        <span class="ctox-flow-creature-shell ctox-crew-home-creature">${memberCreatureHtml(member, state)}</span>
        <strong>${escapeHtml(member.name)}</strong>
        <small>${escapeHtml(line)}</small>
        <small class="ctox-crew-home-domain">${escapeHtml(memberDomainLine(member, state))}</small>
      </button>`;
  }).join('');
  const canCreate = mayManageCrew(state, 'ctox.crew.member.create') && Boolean(state.ctx?.commandBus?.dispatch);
  return `
    <section class="ctox-canvas-container ctox-flow-well ctox-crew-home" data-crew-home aria-label="${escapeAttr(t.crewHome)}">
      <div class="ctox-crew-home-row">${cards}${canCreate ? `
        <button type="button" class="ctox-crew-home-member is-new" data-crew-new-member aria-label="${escapeAttr(t.newMember)}" title="${escapeAttr(t.newMember)}">
          <span class="ctox-crew-home-new-icon">${actionIcon(state, 'add')}</span>
          <strong>${escapeHtml(t.newMember)}</strong>
        </button>` : ''}</div>
    </section>`;
}

// The home is the idle view: no live work AND no task chosen. A chosen task
// always gets the map with its creature on the station it stands at.
function shouldShowCrewHome(state) {
  const members = (state.crewMembers || []).filter((member) => !member.archived);
  if (!members.length) return false;
  if (state.selectedTaskId) return false;
  return !state.model?.liveWork;
}

function wireCrewHome(state, main) {
  main.querySelectorAll('[data-crew-member-id]').forEach((button) => {
    button.addEventListener('click', () => {
      openCrewMemberDrawer(state, button.dataset.crewMemberId);
    });
  });
  main.querySelector('[data-crew-new-member]')?.addEventListener('click', () => {
    state.detailDrawer = { type: 'new-member' };
    state.ctx.openLeftDrawer(newCrewMemberDrawer(state));
  });
}

const CREW_MEMBER_COLORS = Object.freeze(['#1685ee', '#00aa9a', '#7d7f84', '#7c6df2', '#e97255', '#34a26f']);
const CREW_MEMBER_SHAPES = Object.freeze(['round', 'blob', 'square', 'triangle']);

// The pool is owner-managed: a new member starts with a persona and no memory.
function newCrewMemberDrawer(state) {
  const t = labels[state.lang];
  const body = document.createElement('div');
  body.className = 'drawer-body ctox-task-drawer ctox-member-drawer';
  body.setAttribute('data-context-record-type', 'ctox_crew_member_new');
  body.setAttribute('data-context-label', t.newMember);
  const preview = (shape, color) => crewCreatureHtml({ crewKey: `new:${shape}:${color}`, crewIdentity: { name: '', shape, color } }, 'idle', 'map');
  body.innerHTML = `
    <header class="drawer-header-row ctox-member-header">
      <div class="ctox-member-header-identity">
        <span class="ctox-flow-creature-shell ctox-member-portrait" data-new-member-preview>${preview('round', CREW_MEMBER_COLORS[0])}</span>
        <div>
          <span class="ctox-pane-kicker">${escapeHtml(t.crewMember)}</span>
          <h2>${escapeHtml(t.newMember)}</h2>
        </div>
      </div>
      <button class="ctox-pane-icon ctox-drawer-close" type="button" data-close-ctox-drawer aria-label="${escapeAttr(t.close)}" title="${escapeAttr(t.close)}">${actionIcon(state, 'close')}</button>
    </header>
    <form class="ctox-card ctox-member-soul" data-new-member-form>
      <header>${escapeHtml(t.soul)}</header>
      <div class="ctox-card-body">
        <label class="ctox-task-edit-field"><span class="ctox-field-label">${escapeHtml(t.memberName)}</span><input class="ctox-input" name="name" maxlength="60" required /></label>
        <div class="ctox-member-identity-row">
          <label class="ctox-task-edit-field"><span class="ctox-field-label">${escapeHtml(t.shapeLabel)}</span>
            <select class="ctox-select" name="shape">${CREW_MEMBER_SHAPES.map((shape) => `<option value="${shape}">${escapeHtml(t[`shape_${shape}`] || shape)}</option>`).join('')}</select></label>
          <label class="ctox-task-edit-field"><span class="ctox-field-label">${escapeHtml(t.colorLabel)}</span>
            <select class="ctox-select" name="color">${CREW_MEMBER_COLORS.map((color) => `<option value="${color}">${color}</option>`).join('')}</select></label>
        </div>
        <div class="ctox-soul-axes">${SOUL_AXES.map((axis) => soulAxisMarkup(axis, 50, true, t)).join('')}</div>
        <label class="ctox-task-edit-field"><span class="ctox-field-label">${escapeHtml(t.voice)}</span><input class="ctox-input" name="voice" maxlength="200" required /><small>${escapeHtml(t.voiceHint)}</small></label>
        <label class="ctox-task-edit-field"><span class="ctox-field-label">${escapeHtml(t.sketch)}</span><textarea class="ctox-textarea" name="sketch" rows="3" maxlength="600"></textarea></label>
        <div class="ctox-task-edit-actions">
          <button type="submit" class="ctox-button is-primary">${escapeHtml(t.createMember)}</button>
          <small data-member-status></small>
        </div>
      </div>
    </form>`;
  body.querySelector('[data-close-ctox-drawer]')?.addEventListener('click', () => closeDetailDrawer(state));
  const form = body.querySelector('[data-new-member-form]');
  const updatePreview = () => {
    const data = new FormData(form);
    const slot = body.querySelector('[data-new-member-preview]');
    if (slot) slot.innerHTML = preview(String(data.get('shape') || 'round'), String(data.get('color') || CREW_MEMBER_COLORS[0]));
  };
  form.addEventListener('input', () => { body.dataset.dirty = '1'; });
  form.addEventListener('change', updatePreview);
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const data = new FormData(form);
    const status = body.querySelector('[data-member-status]');
    const submit = form.querySelector('button[type="submit"]');
    const soul = { sketch: String(data.get('sketch') || '').trim(), voice: String(data.get('voice') || '').trim() };
    for (const axis of SOUL_AXES) soul[axis.key] = clampMetric(Number(data.get(axis.key)) || 50, 0, 100);
    submit?.setAttribute('disabled', 'disabled');
    try {
      await dispatchCtoxTaskMutation(state, {
        commandType: 'ctox.crew.member.create',
        payload: { name: String(data.get('name') || '').trim(), shape: String(data.get('shape') || 'round'), color: String(data.get('color') || CREW_MEMBER_COLORS[0]), soul, specialties: {} },
        commandPath: 'ctox_crew_member_create',
      });
      body.dataset.dirty = '0';
      if (status) status.textContent = t.memberCreated;
      closeDetailDrawer(state);
      refresh(state).catch(() => {});
    } catch (error) {
      if (status) status.textContent = humanTaskActionError(error, t);
      submit?.removeAttribute('disabled');
    }
  });
  return body;
}

// The router's decision for a task, as the owner reads it: "Milo: <reason>".
function taskSelectionSentence(task, state) {
  const live = state?.selectedLive;
  if (!task || !live || live.key !== taskLiveKey(task)) return '';
  const event = [...(live.events || [])].reverse().find((item) => item.kind === 'crew_selected');
  if (!event?.title) return '';
  const title = String(event.title);
  // routed/selected: "<kind>: <Name> (<id>): <reason>"
  const judged = /^(routed|selected):\s*(.*?)\s*\(([^)]*)\):\s*(.*)$/s.exec(title);
  if (judged) return `${judged[2]}: ${judged[4]}`;
  // assigned/continuity: "<kind>: <reason>: <Name> (<id>)"
  const pinned = /^(assigned|continuity):\s*(.*?):\s*(.*?)\s*\(([^)]*)\)$/s.exec(title);
  if (pinned) return `${pinned[3]}: ${pinned[2]}`;
  return title;
}

// --- Member profile drawer ------------------------------------------------------

async function loadLocalRunsForMember(ctx, memberId) {
  const collection = ctoxCollection(ctx, 'ctox_runs');
  if (!collection || !memberId) return [];
  return findLocalDocs(collection, { crew_member_id: memberId }, 40, 'updated_at_ms', 'desc');
}

function mayManageCrew(state, commandType = 'ctox.crew.member.update') {
  return canUseBusinessPermission({
    session: state.ctx?.session,
    governance: state.ctx?.governance,
    permission: BusinessOsPermissions.CrewManage,
    scopeType: 'record',
    scopeId: commandType,
  });
}

function openCrewMemberDrawer(state, memberId) {
  const member = crewMemberById(state, memberId);
  if (!member) return;
  state.detailDrawer = { type: 'member', memberId };
  state.memberDrawerSignature = '';
  state.memberDrawerData = state.memberDrawerData?.memberId === memberId ? state.memberDrawerData : { memberId, runs: null };
  state.ctx.openLeftDrawer(crewMemberDrawer(member, state));
  void loadLocalRunsForMember(state.ctx, memberId).catch(() => []).then((runs) => {
    if (state.disposed || state.detailDrawer?.type !== 'member' || state.detailDrawer.memberId !== memberId) return;
    state.memberDrawerData = { memberId, runs };
    syncDetailDrawer(state);
  });
}

function drawerIsBusy() {
  const active = typeof document !== 'undefined' ? document.activeElement : null;
  const body = active?.closest?.('.ctox-member-drawer');
  if (!body) return false;
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(active.tagName) || body.dataset.dirty === '1';
}

function formatDurationShort(ms, lang) {
  const seconds = Math.round(Number(ms) / 1000);
  if (!Number.isFinite(seconds) || seconds <= 0) return '';
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

function soulAxisMarkup(axis, value, editable, t) {
  const number = clampMetric(Number(value) || 0, 0, 100);
  return `
    <label class="ctox-soul-axis">
      <span class="ctox-soul-axis-left">${escapeHtml(t[axis.left])}</span>
      <input type="range" min="0" max="100" step="1" value="${number}" name="${escapeAttr(axis.key)}" data-soul-axis ${editable ? '' : 'disabled'} aria-label="${escapeAttr(`${t[axis.left]} ↔ ${t[axis.right]}`)}" />
      <span class="ctox-soul-axis-right">${escapeHtml(t[axis.right])}</span>
    </label>`;
}

function memberStatsMarkup(member, state) {
  const t = labels[state.lang];
  const stats = member?.stats;
  if (!stats || typeof stats !== 'object') return '';
  const rows = [
    [t.tasksTotal, stats.tasks_total],
    [t.succeededCount, stats.succeeded],
    [t.failedCount, stats.failed],
    [t.reviewPassedCount, stats.review_passed],
    [t.reviewRejectedCount, stats.review_rejected],
    [t.avgElapsed, formatDurationShort(stats.avg_elapsed_ms, state.lang)],
    [t.lastActive, stats.last_active_at ? formatShortTimestamp(stats.last_active_at) : ''],
  ].filter(([, value]) => value !== null && value !== undefined && value !== '' && !(typeof value === 'number' && Number.isNaN(value)));
  if (!rows.length) return '';
  return `
    <section class="ctox-card">
      <header>${escapeHtml(t.cv)}</header>
      <div class="ctox-card-body">
        <dl class="ctox-fields ctox-member-stats">
          ${rows.map(([label, value]) => `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(String(value))}</dd>`).join('')}
        </dl>
      </div>
    </section>`;
}

// Continuity documents ("- key: value" lines under "## Entries") as one line
// per entry, mirroring the harness renderer so the owner reads what the
// member reads.
function memoryEntries(content, startKey, tagKey, primaryKey, secondaryKey = null) {
  const entries = [];
  let current = null;
  for (const raw of String(content || '').split('\n')) {
    const trimmed = raw.trim().replace(/^- /, '').trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const index = trimmed.indexOf(':');
    if (index < 0) continue;
    const key = trimmed.slice(0, index).trim().toLowerCase();
    const value = trimmed.slice(index + 1).trim().replace(/\s+/g, ' ');
    if (key === startKey) {
      if (current) entries.push(current);
      current = {};
    }
    if (current && value && !(key in current)) current[key] = value;
  }
  if (current) entries.push(current);
  return entries
    .filter((entry) => entry[primaryKey])
    .map((entry) => ({
      id: entry[startKey] || '',
      tag: entry[tagKey] || '',
      text: entry[primaryKey],
      more: secondaryKey ? entry[secondaryKey] || '' : '',
      scope: entry.scope || '',
      source: entry.source_ref || '',
    }));
}

function memberMemoryMarkup(member, state) {
  const t = labels[state.lang];
  const memory = member.memory && typeof member.memory === 'object' ? member.memory : null;
  if (!memory) return '';
  const editable = mayManageCrew(state, 'ctox.crew.memory.update') && Boolean(state.ctx?.commandBus?.dispatch);
  const anchors = memoryEntries(memory.anchors, 'anchor_id', 'anchor_type', 'statement');
  const narrative = memoryEntries(memory.narrative, 'entry_id', 'event_type', 'summary', 'consequence');
  const tagLabel = (tag) => ({ owner_confirmed: t.memoryConfirmed, hypothesis: t.memoryHypothesis }[tag] || tag);
  const anchorItems = anchors.map((entry) => `
    <li class="ctox-memory-entry ${entry.tag === 'owner_confirmed' ? 'is-confirmed' : ''}" data-anchor-id="${escapeAttr(entry.id)}">
      <p>${escapeHtml(entry.text)}</p>
      <small>${escapeHtml([tagLabel(entry.tag), entry.scope, entry.source ? `${t.evidence} ${entry.source.slice(0, 18)}` : ''].filter(Boolean).join(' · '))}</small>
      ${editable && entry.tag === 'hypothesis' && entry.id ? `<button type="button" class="ctox-button" data-memory-confirm="${escapeAttr(entry.id)}">${escapeHtml(t.confirmAnchor)}</button>` : ''}
    </li>`).join('');
  const narrativeItems = narrative.slice().reverse().map((entry) => `
    <li class="ctox-memory-entry">
      <p>${escapeHtml(entry.text)}${entry.more ? ` <span class="ctox-memory-consequence">${escapeHtml(entry.more)}</span>` : ''}</p>
      ${entry.tag ? `<small>${escapeHtml(entry.tag)}</small>` : ''}
    </li>`).join('');
  // The owner edits statements, one per line; the document syntax stays ours.
  const document = (kind, title, items, raw) => `
    <section class="ctox-card ctox-memory-doc" data-memory-kind="${kind}">
      <header>${escapeHtml(title)}${editable && kind === 'anchors' ? `<button type="button" class="ctox-pane-icon" data-memory-edit="${kind}" aria-label="${escapeAttr(t.editMemory)}" title="${escapeAttr(t.editMemory)}">${actionIcon(state, 'edit')}</button>` : ''}</header>
      <div class="ctox-card-body">
        ${items ? `<ul class="ctox-memory-entries" data-memory-view>${items}</ul>` : `<p class="ctox-memory-empty" data-memory-view>${escapeHtml(t.memoryEmpty)}</p>`}
        ${editable && kind === 'anchors' ? `
          <form class="ctox-memory-editor" data-memory-form="${kind}" hidden>
            <textarea class="ctox-textarea" name="body" rows="6" spellcheck="false" placeholder="${escapeAttr(t.memoryLinePlaceholder)}">${escapeHtml(anchors.map((entry) => entry.text).join('\n'))}</textarea>
            <div class="ctox-task-edit-actions">
              <button type="submit" class="ctox-button is-primary">${escapeHtml(t.saveMemory)}</button>
              <button type="button" class="ctox-button" data-memory-cancel>${escapeHtml(t.cancelEdit)}</button>
              <small data-memory-status></small>
            </div>
          </form>` : ''}
      </div>
    </section>`;
  return `
    <div class="ctox-member-memory">
      <span class="ctox-pane-kicker">${escapeHtml(t.memoryTitle)}</span>
      ${document('anchors', t.knowledge, anchorItems, memory.anchors)}
      ${document('narrative', t.experience, narrativeItems, memory.narrative)}
    </div>`;
}

async function saveMemberMemory(state, member, kind, body, statusNode) {
  const t = labels[state.lang];
  try {
    await dispatchCtoxTaskMutation(state, {
      commandType: 'ctox.crew.memory.update',
      payload: { member_id: member.id, kind, mode: 'full', body },
      commandPath: 'ctox_crew_memory_update',
    });
    if (statusNode) statusNode.textContent = t.memberSaved;
    return true;
  } catch (error) {
    if (statusNode) statusNode.textContent = humanTaskActionError(error, t);
    return false;
  }
}

// Statements typed by the owner become owner-confirmed anchors in the
// document syntax the harness reads (see memoryEntries); existing entries keep
// their ids and types when their statement is unchanged.
function anchorsDocumentFromLines(text, previousDocument) {
  const previous = memoryEntries(previousDocument, 'anchor_id', 'anchor_type', 'statement');
  const now = new Date().toISOString();
  const lines = String(text || '').split('\n').map((line) => line.trim()).filter(Boolean);
  const entries = lines.map((statement, index) => {
    const known = previous.find((entry) => entry.text === statement);
    return [
      `anchor_id: ${known?.id || `owner_${Date.now().toString(36)}_${index + 1}`}`,
      `anchor_type: ${known?.tag || 'owner_confirmed'}`,
      `statement: ${statement}`,
      'source_class: owner',
      `source_ref: ${known?.source || 'owner'}`,
      `observed_at: ${now}`,
      'confidence: high',
    ].join('\n');
  });
  return `# CONTINUITY ANCHORS\n\n## Entries\n${entries.join('\n\n')}\n`;
}

// Confirming a hypothesis rewrites only that entry's type in the document.
function confirmAnchorBody(anchorsDocument, anchorId) {
  const lines = String(anchorsDocument || '').split('\n');
  let inside = false;
  return lines.map((line) => {
    const trimmed = line.trim().replace(/^- /, '');
    if (/^anchor_id:/i.test(trimmed)) inside = trimmed.slice('anchor_id:'.length).trim() === anchorId;
    if (inside && /^anchor_type:\s*hypothesis\s*$/i.test(trimmed)) {
      return line.replace(/hypothesis\s*$/i, 'owner_confirmed');
    }
    return line;
  }).join('\n');
}

function wireMemberMemory(state, member, body) {
  body.querySelectorAll('[data-memory-edit]').forEach((button) => {
    button.addEventListener('click', () => {
      const doc = button.closest('[data-memory-kind]');
      const form = doc?.querySelector('[data-memory-form]');
      const view = doc?.querySelector('[data-memory-view]');
      if (!form) return;
      form.hidden = !form.hidden;
      if (view) view.hidden = !form.hidden;
      if (!form.hidden) {
        body.dataset.dirty = '1';
        form.querySelector('textarea')?.focus();
      } else {
        body.dataset.dirty = '0';
      }
    });
  });
  body.querySelectorAll('[data-memory-form]').forEach((form) => {
    form.querySelector('[data-memory-cancel]')?.addEventListener('click', () => {
      form.hidden = true;
      const view = form.closest('[data-memory-kind]')?.querySelector('[data-memory-view]');
      if (view) view.hidden = false;
      body.dataset.dirty = '0';
    });
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      const kind = form.dataset.memoryForm;
      const text = String(new FormData(form).get('body') || '').trim();
      const body = kind === 'anchors' ? anchorsDocumentFromLines(text, member.memory?.anchors) : text;
      if (!body) return;
      const ok = await saveMemberMemory(state, member, kind, body, form.querySelector('[data-memory-status]'));
      if (ok) body.dataset.dirty = '0';
    });
  });
  body.querySelectorAll('[data-memory-confirm]').forEach((button) => {
    button.addEventListener('click', async () => {
      const next = confirmAnchorBody(member.memory?.anchors, button.dataset.memoryConfirm);
      if (next === String(member.memory?.anchors || '')) return;
      button.setAttribute('disabled', 'disabled');
      const ok = await saveMemberMemory(state, member, 'anchors', next, body.querySelector('[data-memory-status]'));
      if (!ok) button.removeAttribute('disabled');
    });
  });
}

// Run outcomes in the owner's words.
function displayRunOutcome(value, state) {
  const t = labels[state.lang];
  const key = String(value || '').toLowerCase().replace(/[^a-z]/g, '');
  const map = {
    executionerror: t.outcomeExecutionError,
    error: t.outcomeExecutionError,
    failed: t.failedWord,
    completed: t.outcomeCompleted,
    success: t.outcomeCompleted,
    reviewrejected: t.outcomeReviewRejected,
  };
  return map[key] || displayStatus(value, state.lang);
}

function memberTimesheetMarkup(member, state) {
  const t = labels[state.lang];
  const data = state.memberDrawerData?.memberId === member.id ? state.memberDrawerData : null;
  const runs = data?.runs;
  if (runs === null || runs === undefined) return '';
  const rows = runs.map((run) => {
    const task = taskByNativeId(state, run.task_id);
    const title = task ? taskDisplayTitle(task, state) : String(run.task_id || '');
    const when = formatShortTimestamp(run.finished_at_ms || run.started_at_ms || run.updated_at_ms);
    const outcome = displayRunOutcome(run.agent_outcome || run.status || '', state);
    const facts = [
      formatDurationShort(run.metrics?.elapsed_ms, state.lang),
      hasFiniteValue(run.metrics?.input_tokens) ? `${formatMetricValue(Number(run.metrics.input_tokens) + (Number(run.metrics?.output_tokens) || 0), 'tokens', state.lang)} ${t.tokensWord}` : '',
      hasFiniteValue(run.metrics?.tool_calls) ? `${run.metrics.tool_calls} ${t.toolsWord}` : '',
    ].filter(Boolean).join(' · ');
    return `
      <li class="ctox-timesheet-row ${['failed', 'error'].includes(String(run.agent_outcome || run.status || '').toLowerCase()) ? 'is-problem' : ''}" ${task ? `data-timesheet-task-id="${escapeAttr(task.id)}"` : ''}>
        <span class="ctox-timesheet-when">${escapeHtml(when)}</span>
        <span class="ctox-timesheet-title">${escapeHtml(title)}</span>
        <span class="ctox-timesheet-outcome">${escapeHtml(outcome)}</span>
        <small>${escapeHtml(facts)}</small>
      </li>`;
  }).join('');
  return `
    <section class="ctox-card">
      <header>${escapeHtml(t.timesheet)}</header>
      <div class="ctox-card-body">
        ${rows ? `<ul class="ctox-timesheet">${rows}</ul>` : `<p>${escapeHtml(t.noRuns)}</p>`}
      </div>
    </section>`;
}

function crewMemberDrawer(member, state) {
  const t = labels[state.lang];
  const editable = mayManageCrew(state) && Boolean(state.ctx?.commandBus?.dispatch);
  const soul = member.soul && typeof member.soul === 'object' ? member.soul : null;
  const specialties = member.specialties && typeof member.specialties === 'object' ? member.specialties : null;
  const line = memberStateLine(member, state);
  const activeTask = member.active_task_id ? taskByNativeId(state, member.active_task_id) : null;
  const body = document.createElement('div');
  body.className = 'drawer-body ctox-task-drawer ctox-member-drawer';
  body.setAttribute('data-context-record-id', member.id);
  body.setAttribute('data-context-record-type', 'ctox_crew_member');
  body.setAttribute('data-context-label', member.name || '');
  body.innerHTML = `
    <header class="drawer-header-row ctox-member-header">
      <div class="ctox-member-header-identity">
        <span class="ctox-flow-creature-shell ctox-member-portrait">${memberCreatureHtml(member, state)}</span>
        <div>
          <span class="ctox-pane-kicker">${escapeHtml(t.crewMember)}</span>
          <h2>${escapeHtml(member.name)}</h2>
          <small>${escapeHtml(line)}</small>
          <small>${escapeHtml(memberDomainLine(member, state))}</small>
        </div>
      </div>
      <button class="ctox-pane-icon ctox-drawer-close" type="button" data-close-ctox-drawer aria-label="${escapeAttr(t.close)}" title="${escapeAttr(t.close)}">${actionIcon(state, 'close')}</button>
    </header>
    ${activeTask ? `
      <section class="ctox-task-status-strip ctox-callout">
        <div>
          <span class="ctox-badge">${escapeHtml(t.onDuty)}</span>
          <small>${escapeHtml(taskDisplayTitle(activeTask, state))}</small>
        </div>
        <button type="button" class="ctox-pane-icon" data-member-open-task="${escapeAttr(activeTask.id)}" aria-label="${escapeAttr(t.openTaskDetail)}" title="${escapeAttr(t.openTaskDetail)}">${actionIcon(state, 'open')}</button>
      </section>` : ''}
    ${soul ? `
      <form class="ctox-card ctox-member-soul" data-member-form>
        <header>${escapeHtml(t.soul)}</header>
        <div class="ctox-card-body">
          <label class="ctox-task-edit-field">
            <span class="ctox-field-label">${escapeHtml(t.memberName)}</span>
            <input class="ctox-input" name="name" maxlength="60" value="${escapeAttr(member.name || '')}" ${editable ? '' : 'disabled'} />
          </label>
          <div class="ctox-soul-axes">
            ${SOUL_AXES.map((axis) => soulAxisMarkup(axis, soul[axis.key], editable, t)).join('')}
          </div>
          <label class="ctox-task-edit-field">
            <span class="ctox-field-label">${escapeHtml(t.voice)}</span>
            <input class="ctox-input" name="voice" maxlength="200" value="${escapeAttr(soul.voice || '')}" ${editable ? '' : 'disabled'} />
            <small>${escapeHtml(t.voiceHint)}</small>
          </label>
          <label class="ctox-task-edit-field">
            <span class="ctox-field-label">${escapeHtml(t.sketch)}</span>
            <textarea class="ctox-textarea" name="sketch" rows="3" maxlength="600" ${editable ? '' : 'disabled'}>${escapeHtml(soul.sketch || '')}</textarea>
          </label>
          ${specialties ? `
            <div class="ctox-member-specialties">
              <span class="ctox-field-label">${escapeHtml(t.specialties)}</span>
              ${SPECIALTY_KEYS.map((key) => `
                <label class="ctox-task-edit-field">
                  <span class="ctox-field-label">${escapeHtml(t[`spec_${key}`])}</span>
                  <input class="ctox-input" name="spec_${escapeAttr(key)}" value="${escapeAttr((Array.isArray(specialties[key]) ? specialties[key] : []).join(', '))}" ${editable ? '' : 'disabled'} placeholder="${escapeAttr(t.specialtiesHint)}" />
                </label>`).join('')}
            </div>` : ''}
          ${editable ? `
            <div class="ctox-task-edit-actions">
              <button type="submit" class="ctox-button is-primary">${escapeHtml(t.saveMember)}</button>
              <small data-member-status></small>
            </div>` : ''}
        </div>
      </form>` : ''}
    ${memberStatsMarkup(member, state)}
    ${memberMemoryMarkup(member, state)}
    ${memberTimesheetMarkup(member, state)}
    ${mayManageCrew(state) && !member.archived ? `<div class="ctox-member-archive"><button type="button" class="ctox-button is-danger" data-member-archive>${escapeHtml(t.archiveMember)}</button><small data-member-archive-status></small></div>` : ''}
  `;
  body.querySelector('[data-close-ctox-drawer]')?.addEventListener('click', () => closeDetailDrawer(state));
  body.querySelector('[data-member-open-task]')?.addEventListener('click', (event) => {
    selectTask(state, event.currentTarget.dataset.memberOpenTask, { drawer: true, center: true });
  });
  body.querySelectorAll('[data-timesheet-task-id]').forEach((row) => {
    row.addEventListener('click', () => selectTask(state, row.dataset.timesheetTaskId, { drawer: true, center: true }));
  });
  const form = body.querySelector('[data-member-form]');
  if (form) {
    form.addEventListener('input', () => { body.dataset.dirty = '1'; });
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      await saveCrewMemberFromDrawer(state, member, body);
    });
  }
  wireMemberMemory(state, member, body);
  body.querySelector('[data-member-archive]')?.addEventListener('click', async (event) => {
    const button = event.currentTarget;
    const status = body.querySelector('[data-member-archive-status]');
    button.setAttribute('disabled', 'disabled');
    try {
      await dispatchCtoxTaskMutation(state, { commandType: 'ctox.crew.member.update', payload: { member_id: member.id, archived: true }, commandPath: 'ctox_crew_member_archive' });
      closeDetailDrawer(state);
      refresh(state).catch(() => {});
    } catch (error) {
      if (status) status.textContent = humanTaskActionError(error, t);
      button.removeAttribute('disabled');
    }
  });
  return body;
}

async function saveCrewMemberFromDrawer(state, member, body) {
  const t = labels[state.lang];
  const form = body.querySelector('[data-member-form]');
  const status = body.querySelector('[data-member-status]');
  const submit = form?.querySelector('button[type="submit"]');
  if (!form) return;
  const data = new FormData(form);
  const soul = { ...(member.soul || {}) };
  for (const axis of SOUL_AXES) soul[axis.key] = clampMetric(Number(data.get(axis.key)) || 0, 0, 100);
  soul.voice = String(data.get('voice') || '').trim();
  soul.sketch = String(data.get('sketch') || '').trim();
  const payload = { member_id: member.id, name: String(data.get('name') || member.name).trim(), soul };
  if (member.specialties && typeof member.specialties === 'object') {
    const specialties = {};
    for (const key of SPECIALTY_KEYS) {
      specialties[key] = String(data.get(`spec_${key}`) || '').split(',').map((value) => value.trim()).filter(Boolean);
    }
    payload.specialties = specialties;
  }
  submit?.setAttribute('disabled', 'disabled');
  if (status) status.textContent = '';
  try {
    await dispatchCtoxTaskMutation(state, { commandType: 'ctox.crew.member.update', payload, commandPath: 'ctox_crew_member_update' });
    body.dataset.dirty = '0';
    if (status) status.textContent = t.memberSaved;
  } catch (error) {
    if (status) status.textContent = humanTaskActionError(error, t);
  } finally {
    submit?.removeAttribute('disabled');
  }
}

function mayManageTask(state, task) {
  const scopeId = nativeTaskId(task) || task?.id || '';
  return canUseBusinessPermission({
    session: state.ctx?.session,
    governance: state.ctx?.governance,
    permission: BusinessOsPermissions.CtoxTaskManage,
    scopeType: 'task',
    scopeId,
  });
}

function mayManageWorkspace(state) {
  return canUseBusinessPermission({
    session: state.ctx?.session,
    governance: state.ctx?.governance,
    permission: BusinessOsPermissions.CtoxTaskManage,
    scopeType: 'workspace',
  });
}

function mayAssignCrew(state) {
  return canUseBusinessPermission({
    session: state.ctx?.session,
    governance: state.ctx?.governance,
    permission: BusinessOsPermissions.CrewManage,
    scopeType: 'record',
    scopeId: 'ctox.crew.assign',
  });
}

// Which controls a task offers, derived from the durable routing state only.
// leased/running: cancel (through the originating command). pending: block,
// assign. failed/blocked: release, retry. Nothing else, nothing invented.
function taskControlSpec(task, state) {
  const status = normalizeCommandStatus(task.routeStatus || task.status);
  const controls = [];
  const taskManage = mayManageTask(state, task);
  if (status === 'running') {
    if (task.commandId && taskManage) controls.push('cancel');
  } else if (status === 'queued') {
    if (taskManage) controls.push('block');
    if (mayAssignCrew(state) && (state.crewMembers || []).length) controls.push('assign');
  } else if (status === 'blocked') {
    if (taskManage) controls.push('release', 'retry');
  } else if (status === 'failed') {
    if (taskManage) controls.push('retry');
  }
  return controls;
}

function taskControlsMarkup(task, state) {
  const t = labels[state.lang];
  const spec = taskControlSpec(task, state);
  if (!spec.length) return '';
  const buttons = spec.map((control) => {
    if (control === 'assign') {
      const options = (state.crewMembers || []).map((member) => `<option value="${escapeAttr(member.id)}">${escapeHtml(member.name)}</option>`).join('');
      return `<label class="ctox-task-control-assign"><span class="ctox-field-label">${escapeHtml(t.assignTask)}</span><select class="ctox-select" data-ctox-task-assign><option value="">${escapeHtml(t.assignChoose)}</option>${options}</select></label>`;
    }
    const label = { cancel: t.cancelTask, block: t.blockTask, release: t.releaseTask, retry: t.retryTask }[control];
    const danger = control === 'cancel' || control === 'block';
    return `<button type="button" class="ctox-button ${danger ? 'is-danger' : 'is-primary'}" data-ctox-task-control="${control}">${escapeHtml(label)}</button>`;
  }).join('');
  return `<div class="ctox-task-controls" data-ctox-task-controls>${buttons}<small data-ctox-task-control-status></small></div>`;
}

async function runTaskControl(state, task, control, body, extra = {}) {
  const t = labels[state.lang];
  const status = body.querySelector('[data-ctox-task-control-status]');
  const buttons = body.querySelectorAll('[data-ctox-task-control], [data-ctox-task-assign]');
  const taskId = nativeTaskId(task);
  buttons.forEach((el) => el.setAttribute('disabled', 'disabled'));
  if (status) status.textContent = '';
  try {
    if (control === 'cancel') {
      if (!state.ctx?.commandBus?.cancel) throw new Error('Die Verbindung zur Crew ist nicht verfügbar.');
      await state.ctx.commandBus.cancel(task.commandId, { reason: t.cancelReasonDefault, until: 'accepted' });
    } else if (control === 'block') {
      await dispatchCtoxTaskMutation(state, { commandType: 'ctox.queue.block', payload: { task_id: taskId, reason: t.blockReasonDefault }, commandPath: 'ctox_queue_block' });
    } else if (control === 'release') {
      await dispatchCtoxTaskMutation(state, { commandType: 'ctox.queue.release', payload: { task_id: taskId }, commandPath: 'ctox_queue_release' });
    } else if (control === 'retry') {
      await dispatchCtoxTaskMutation(state, { commandType: 'ctox.queue.retry', payload: { task_id: taskId }, commandPath: 'ctox_queue_retry' });
    } else if (control === 'assign') {
      if (!extra.memberId) return;
      await dispatchCtoxTaskMutation(state, { commandType: 'ctox.crew.assign', payload: { task_id: taskId, member_id: extra.memberId }, commandPath: 'ctox_crew_assign' });
    }
    if (status) status.textContent = t.controlApplied;
    refresh(state).catch(() => {});
  } catch (error) {
    if (status) status.textContent = humanTaskActionError(error, t);
  } finally {
    buttons.forEach((el) => el.removeAttribute('disabled'));
  }
}

function harnessStatusText(state) {
  const t = labels[state.lang];
  const h = state.harnessStatus;
  if (!h) return '';
  const bits = [];
  if (!h.service_running) bits.push(t.harnessStopped);
  else if (h.paused) bits.push(h.pause_reason ? `${t.harnessPaused} · ${h.pause_reason}` : t.harnessPaused);
  else bits.push(t.harnessRunning);
  const active = crewMemberName(state, h.active_crew_member_id);
  if (active) bits.push(`${active} ${t.onDuty}`);
  if (Number.isFinite(Number(h.worker_capacity))) bits.push(`${t.capacity} ${h.worker_capacity}`);
  const counts = [];
  if (Number(h.pending_count) > 0) counts.push(`${h.pending_count} ${t.countWaiting}`);
  if (Number(h.leased_count) > 0) counts.push(`${h.leased_count} ${t.countWorking}`);
  if (Number(h.blocked_count) > 0) counts.push(`${h.blocked_count} ${t.countBlocked}`);
  if (counts.length) bits.push(counts.join(' · '));
  if (h.pressure_active) bits.push(t.pressureActive);
  return bits.join(' · ');
}

function harnessControlsMarkup(state) {
  const t = labels[state.lang];
  const h = state.harnessStatus;
  if (!h || !mayManageWorkspace(state)) return '';
  const paused = Boolean(h.paused);
  const capacity = Number(h.worker_capacity) || 1;
  return `
    <button type="button" class="ctox-button ${paused ? 'is-active' : ''}" data-harness-pause aria-pressed="${paused}">${escapeHtml(paused ? t.resumeHarness : t.pauseHarness)}</button>
    <label class="ctox-harness-capacity" title="${escapeAttr(t.capacity)}"><span class="ctox-field-label">${escapeHtml(t.capacity)}</span><select class="ctox-select" data-harness-capacity aria-label="${escapeAttr(t.capacity)}">${[1,2,3,4,5,6,7,8].map((n) => `<option value="${n}" ${n === capacity ? 'selected' : ''}>${n}</option>`).join('')}</select></label>`;
}

async function runHarnessControl(state, control, value) {
  const t = labels[state.lang];
  try {
    if (control === 'pause') {
      await dispatchCtoxTaskMutation(state, { commandType: 'ctox.queue.pause', payload: { task_id: '', paused: Boolean(value), reason: value ? t.pauseReasonDefault : '' }, commandPath: 'ctox_queue_pause' });
    } else if (control === 'capacity') {
      await dispatchCtoxTaskMutation(state, { commandType: 'ctox.queue.capacity', payload: { task_id: '', workers: Number(value) }, commandPath: 'ctox_queue_capacity' });
    }
    refresh(state).catch(() => {});
  } catch (error) {
    state.ctx?.notifications?.show?.({ title: t.taskActionFailed, message: humanTaskActionError(error, t), tone: 'danger', time: 6000 });
  }
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
  // Prefer the shell session, but accept the flattened ctx.user projection used
  // by some launch paths. local-dev is the offline operator identity and must
  // keep write affordances even when its role string is only "user".
  const user = state.ctx.session?.user || state.ctx.user || {};
  const userId = String(user.id || '').trim().toLowerCase();
  if (userId === 'local-dev' || user.is_admin === true) return true;
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
    // The chat bar lives in the shell window; the module runs in its own frame.
    // A deep link therefore arrives as a message, not as a DOM event.
    if (event.data?.type === 'ctox-business-os-focus-task') focusHandler({ detail: event.data.focus || event.data });
  };
  const preferenceHandler = (event) => {
    applyLanguage(event.detail?.language);
  };
  const focusHandler = (event) => {
    const focusTask = persistFocusTask(event.detail);
    if (!focusTask) return;
    state.focusTask = focusTask;
    state.focusTaskConsumed = false;
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

function stepMetaLabel(step) {
  return formatShortTimestamp(step?.timestamp) || '';
}

// The ticker exists only to advance a clock that is already anchored to a real
// persisted start timestamp on a task the harness reports as `working`. With no
// anchor there is nothing to advance, so no interval is armed at all — the
// previous build ticked unconditionally every second from a `liveStartedAt`
// that was reset to Date.now() on every refresh, which is an animation, not a
// measurement.
function startLiveTicker(state) {
  window.clearInterval(state.liveTicker);
  state.liveTicker = null;
  updateLiveIndicators(state);
  if (!Number.isFinite(state.liveAnchorMs)) return;
  state.liveTicker = window.setInterval(() => {
    if (!Number.isFinite(state.liveAnchorMs)) {
      window.clearInterval(state.liveTicker);
      state.liveTicker = null;
      updateLiveIndicators(state);
      return;
    }
    updateLiveIndicators(state);
  }, 1000);
}

function updateLiveIndicators(state) {
  const display = formatMetricValue(liveElapsedSeconds(state), 'seconds', state.lang);
  document.querySelectorAll('[data-module-root="ctox"] [data-live-elapsed], .ctox-task-drawer [data-live-elapsed]').forEach((node) => {
    node.textContent = display;
  });
}

// Returns null (-> "nicht erfasst") unless a real start timestamp is anchored.
function liveElapsedSeconds(state) {
  const anchor = state?.liveAnchorMs;
  if (!Number.isFinite(anchor)) return null;
  return Math.max(0, Math.floor((Date.now() - anchor) / 1000));
}

// The operator reads the strip for whichever task is selected. Restricting the
// subject to a *running* active task was why the strip was permanently empty:
// a settled task carries perfectly good persisted telemetry and must show it.
function metricSubjectTask(state, selectedTask = null) {
  if (selectedTask) return selectedTask;
  const activeTask = state?.model?.activeTask;
  if (activeTask) return activeTask;
  return null;
}

function isLiveMetricSubject(task, state) {
  if (!task || !state?.model?.activeTask) return false;
  return task.id === state.model.activeTask.id
    && taskMatchesHarnessFlow(task, state)
    && normalizeCommandStatus(task.status) === 'running';
}

function flowSourceView(state) {
  const t = labels[state.lang];
  const projection = state.blobFlow || state.flow;
  if (projection?.ok === false && state.ctx?.sync?.mode === 'webrtc') {
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

// ---------------------------------------------------------------------------
// Durable execution telemetry (HARNESS.md "Durable Execution Plans and Activity
// Turns"). The service projects `execution_progress` onto `ctox_queue_tasks`
// and `business_commands`; it is the PERSISTENCE AUTHORITY for progress and for
// the deduplicated thinking/tool turn counters. The older harness-flow stream
// stays an audit projection and is the only source of token counts.
//
// Everything below fails to `null`, never to a plausible-looking number: an
// unknown metric must render as "nicht erfasst" and a bar with no measured
// progress must stand still.
// ---------------------------------------------------------------------------
function normalizeExecutionProgress(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const int = (value) => (Number.isFinite(Number(value)) ? Math.trunc(Number(value)) : null);
  const totalSteps = int(raw.total_steps ?? raw.totalSteps);
  const completedSteps = int(raw.completed_steps ?? raw.completedSteps);
  const rawPercent = int(raw.percent);
  const percent = rawPercent === null ? null : clampMetric(rawPercent, 0, 100);
  const steps = Array.isArray(raw.steps)
    ? raw.steps.map((step, index) => ({
      position: int(step?.position) ?? index + 1,
      label: String(step?.label || '').trim(),
      status: String(step?.status || '').trim().toLowerCase(),
      activityTurns: int(step?.activity_turns ?? step?.activityTurns),
    })).filter((step) => step.label || step.status)
    : [];
  const turns = raw.activity_turns || raw.activityTurns || null;
  const thinkingTurns = int(turns?.thinking);
  const toolTurns = int(turns?.tools);
  const totalTurns = int(turns?.total)
    ?? (thinkingTurns === null && toolTurns === null ? null : (thinkingTurns || 0) + (toolTurns || 0));
  const phase = String(raw.phase || '').trim().toLowerCase();
  const reviewStatus = String(raw.review?.status || raw.review_status || '').trim().toLowerCase();
  const updatedAtMs = int(raw.updated_at_ms ?? raw.updatedAtMs);
  // A payload that carries none of the load-bearing fields is not telemetry.
  if (percent === null && totalSteps === null && !steps.length && totalTurns === null) return null;
  return {
    version: int(raw.version) ?? 1,
    revision: int(raw.revision),
    phase,
    percent,
    currentStep: int(raw.current_step ?? raw.currentStep),
    completedSteps,
    totalSteps,
    steps,
    reviewStatus,
    thinkingTurns,
    toolTurns,
    totalTurns,
    lastActivityKind: String(turns?.last_kind || turns?.lastKind || '').trim().toLowerCase(),
    updatedAtMs,
  };
}

function taskExecutionProgress(task) {
  if (!task) return null;
  return task.executionProgress || normalizeExecutionProgress(task.execution_progress) || null;
}

// The phase is authoritative for "is this still moving?". Only `working` means
// the model is still producing turns; `review` and `completed` are settled and
// must never drive a running clock.
function executionProgressIsWorking(progress) {
  return Boolean(progress) && progress.phase === 'working';
}

function executionStartedAtMs(task) {
  const candidates = [task?.leasedAt, task?.leased_at, task?.ackedAt, task?.acked_at, task?.startedAt, task?.started_at];
  for (const candidate of candidates) {
    const parsed = Date.parse(String(candidate || ''));
    if (Number.isFinite(parsed)) return parsed;
  }
  return null;
}

// The single source for the metric strip. Tokens come from the harness-flow
// audit projection and ONLY when that projection actually describes this task;
// tool calls and reasoning turns come from the durable activity-turn counters.
function taskTelemetry(task, state) {
  const progress = taskExecutionProgress(task);
  const flowMatches = Boolean(task) && taskMatchesHarnessFlow(task, state);
  const flowMetrics = flowMatches ? aggregateFlowMetrics(state?.flow) : emptyMetrics();
  // Finished attempts are measured in `ctox_runs`; a working attempt streams
  // through the flow. A settled task therefore reads its runs, a live one the flow.
  const runMetrics = task && state?.selectedLive && state.selectedLive.key === taskLiveKey(task)
    ? aggregateRunMetrics(state.selectedLive.runs)
    : null;
  const startedAtMs = executionStartedAtMs(task);
  const updatedAtMs = Number.isFinite(Number(task?.updatedAtMs))
    ? Number(task.updatedAtMs)
    : (progress?.updatedAtMs ?? null);
  const live = executionProgressIsWorking(progress) && Number.isFinite(startedAtMs);
  let seconds = live ? flowMetrics.seconds : (runMetrics?.seconds ?? flowMetrics.seconds);
  if (seconds === null && Number.isFinite(startedAtMs) && Number.isFinite(updatedAtMs) && updatedAtMs >= startedAtMs) {
    seconds = Math.round((updatedAtMs - startedAtMs) / 1000);
  }
  const pick = (liveValue, settledValue) => (live ? (liveValue ?? settledValue ?? null) : (settledValue ?? liveValue ?? null));
  return {
    inputTokens: pick(flowMetrics.inputTokens, runMetrics?.inputTokens),
    outputTokens: pick(flowMetrics.outputTokens, runMetrics?.outputTokens),
    // `activity_turns.tools` is the deduplicated durable count and outranks the
    // audit stream; fall back to the runs, then the flow, when no plan exists.
    toolCalls: progress?.toolTurns ?? pick(flowMetrics.toolCalls, runMetrics?.toolCalls),
    thinkingTurns: progress?.thinkingTurns ?? runMetrics?.thinkingTurns ?? null,
    seconds,
    percent: progress?.percent ?? null,
    completedSteps: progress?.completedSteps ?? null,
    totalSteps: progress?.totalSteps ?? null,
    currentStep: progress?.currentStep ?? null,
    phase: progress?.phase || '',
    reviewStatus: progress?.reviewStatus || '',
    startedAtMs,
    live,
    progress,
  };
}

function emptyTelemetry() {
  return {
    inputTokens: null,
    outputTokens: null,
    toolCalls: null,
    thinkingTurns: null,
    seconds: null,
    percent: null,
    completedSteps: null,
    totalSteps: null,
    currentStep: null,
    phase: '',
    reviewStatus: '',
    startedAtMs: null,
    live: false,
    progress: null,
  };
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

// Five cards while something is measured; one quiet line when nothing is —
// the same fact five times in a row was the "nicht erfasst" flood.
function metricsStripMarkup(metrics, elapsedSeconds, live, state) {
  const t = labels[state.lang];
  const values = [metrics.inputTokens, metrics.outputTokens, metrics.thinkingTurns, metrics.toolCalls, elapsedSeconds];
  const anyMeasured = values.some((value) => value !== null && value !== undefined);
  if (!anyMeasured) {
    return `<section class="ctox-metrics-strip is-quiet" aria-label="${escapeAttr(t.measurements)}"><span class="ctox-metrics-quiet">${escapeHtml(t.noLiveMetrics)}</span></section>`;
  }
  return `
    <section class="ctox-metrics-strip" aria-label="${escapeAttr(t.measurements)}">
      ${metricCard(t.inputTokens, metrics.inputTokens, 'tokens', state.lang)}
      ${metricCard(t.outputTokens, metrics.outputTokens, 'tokens', state.lang)}
      ${metricCard(t.reasoningTurns, metrics.thinkingTurns, 'count', state.lang)}
      ${metricCard(t.toolCalls, metrics.toolCalls, 'count', state.lang)}
      ${metricCard(t.elapsed, elapsedSeconds, 'seconds', state.lang, { live })}
    </section>`;
}

function metricCard(label, value, kind, lang, options = {}) {
  const display = value === null || value === undefined ? '—' : formatMetricValue(value, kind, lang);
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
  if (mode === 'ctox_cli' || mode === 'ctox_core') return 'Crew';
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
    fallback: nativeTaskId(task) || 'Aufgabe',
    max: 120,
  });
}

// Operator text (title, prompt, summary) is shown as written. Secrets are not
// projected by the server, and rewriting the operator's own words (the old
// regex redaction, underscore/hyphen mangling) hid real content behind
// "technical details hidden" without protecting anything.
function taskFieldDisplay(value) {
  return { redacted: false, text: String(value || '').trim() };
}

function taskPromptDisplay(task) {
  return { redacted: false, text: String(task?.prompt || task?.description || task?.summary || '').trim() };
}

function taskDetailText(value, state) {
  return safeTaskDisplayText(value, state.lang, { max: 280 });
}

function safeTaskDisplayText(value, lang = 'de', options = {}) {
  const text = String(value || '').trim();
  const fallback = options.fallback || '';
  if (!text) return fallback;
  return clip(text.replace(/\s+/g, ' ').trim(), options.max || 180) || fallback;
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

function itemSummary(item) {
  if ('summary' in item) return item.summary;
  if ('acceptance' in item) return item.acceptance;
  if ('promise' in item) return item.promise;
  return item.target || '';
}

function formatShortTimestamp(value) {
  if (value === null || value === undefined || value === '') return '';
  const parsed = typeof value === 'number' ? value : (/^\d{12,}$/.test(String(value)) ? Number(value) : Date.parse(value));
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
    .replace(/^ctox[-_\s]*/i, 'Crew ')
    .trim()
    .split(/[/:]+/)
    .filter(Boolean)
    .map((part) => part.replace(/[_-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase()).replace(/\bCtox\b/g, 'Crew').replace(/\bOs\b/g, 'OS'))
    .join(' / ');
}

function displayPathLike(value) {
  if (!/^[a-z0-9_-]+(\/[a-z0-9_-]+)+$/i.test(value || '')) return value || '';
  return displayWorkSource(value);
}

function displayPriority(priority, lang = 'de') {
  const labelsByPriority = lang === 'en'
    ? { urgent: 'Urgent', high: 'High', normal: 'Normal', low: 'Low' }
    : { urgent: 'Dringend', high: 'Hoch', normal: 'Normal', low: 'Niedrig' };
  return labelsByPriority[priority] || displayStatus(priority, lang);
}

const HOLD_REASON_KEYS = {
  technical: 'holdTechnical',
  missing_review_evidence: 'holdMissingReviewEvidence',
  missing_artifact: 'holdMissingArtifact',
  waiting_external: 'holdWaitingExternal',
  aborted_by_owner: 'holdAbortedByOwner',
};

function displayHoldReason(reason, state) {
  const t = labels[state.lang];
  const raw = String(reason || '').trim();
  if (!raw) return '';
  const value = raw.toLowerCase().replace(/^technical:\s*/, 'technical');
  const key = HOLD_REASON_KEYS[value] || HOLD_REASON_KEYS[value.split(':')[0]];
  if (key && t[key]) return t[key];
  return t.holdOther || raw.replace(/[_-]+/g, ' ');
}

function displayFailureClass(failureClass, state) {
  const t = labels[state.lang];
  const value = String(failureClass || '').trim().toLowerCase();
  if (!value) return '';
  if (value === 'retryable') return t.failureRetryable;
  if (value === 'terminal') return t.failureTerminal;
  if (value === 'technical') return t.holdTechnical;
  return value.replace(/[_-]+/g, ' ');
}

function formatClockTime(value) {
  const date = value ? new Date(value) : null;
  if (!date || Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

function crewMemberName(state, memberId) {
  const member = crewMemberById(state, memberId);
  return member?.name || '';
}

// One sentence of truth per task: why it waits, when it retries, how it
// failed, who holds it. Built only from durable routing fields — never from
// guesses — and empty when there is nothing worth saying.
function taskSummaryReason(task, state) {
  const note = String(task.statusNote || task.error || '').trim();
  const de = state.lang !== 'en';
  const rules = [
    [/model API.*rate.limit/i, 'Der Modelldienst hat zu viele Anfragen erhalten.', 'The model service received too many requests.'],
    [/model API|worker-runtime-api-failure/i, 'Der Modelldienst war nicht erreichbar.', 'The model service could not be reached.'],
    [/completion review.*(?:verdict|finish|timeout)/i, 'Die Abschlussprüfung hat nicht rechtzeitig geantwortet.', 'The final review did not respond in time.'],
    [/MCP.*handshake|thread\/start/i, 'Die Verbindung zu einem Werkzeug konnte nicht aufgebaut werden.', 'A connection to a tool could not be established.'],
    [/app.*validation.*(?:failed|exhausted)/i, 'Die App-Prüfung ist fehlgeschlagen.', 'The app validation failed.'],
    [/SQLITE|database is locked|queue:|\blease\b|worker error|technical:/i, 'Ein technischer Fehler hat die Bearbeitung unterbrochen.', 'A technical error interrupted the work.'],
  ];
  const rule = rules.find(([pattern]) => pattern.test(note));
  if (!rule) return taskReasonText(task, state);
  const count = Number(task.failureAttemptCount || 0);
  const attempts = count > 1 ? ` · ${count} ${labels[state.lang].attemptMany}` : '';
  return rule[de ? 1 : 2] + attempts;
}

function taskDiagnosticMarkup(task, state) {
  const note = String(task.statusNote || task.error || '').trim();
  if (!note) return '';
  return `<details class="ctox-task-diagnostics"><summary>${state.lang === 'de' ? 'Technische Details' : 'Technical details'}</summary><pre>${escapeHtml(note)}</pre></details>`;
}

function taskReasonText(task, state) {
  const t = labels[state.lang];
  const status = normalizeCommandStatus(task.routeStatus || task.status);
  const parts = [];
  const retryAt = formatClockTime(task.retryNotBefore);
  const attempts = Number(task.failureAttemptCount || 0);
  if (status === 'failed') {
    const klass = displayFailureClass(task.failureClass, state);
    parts.push(klass ? `${t.failedWord} · ${klass}` : t.failedWord);
    if (attempts) parts.push(`${attempts} ${attempts === 1 ? t.attemptOne : t.attemptMany}`);
    if (retryAt && new Date(task.retryNotBefore).getTime() > Date.now()) parts.push(`${t.retryAt} ${retryAt}`);
  } else if (status === 'blocked') {
    const reason = displayHoldReason(task.holdReason, state);
    parts.push(reason ? `${t.blockedWord} · ${reason}` : t.blockedWord);
    if (task.waitEntityId) parts.push(`${t.waitsFor} ${task.waitEntityType ? `${task.waitEntityType} ` : ''}${task.waitEntityId}`);
  } else if (task.retryNotBefore && new Date(task.retryNotBefore).getTime() > Date.now()) {
    parts.push(retryAt ? `${t.retryAt} ${retryAt}` : t.retryPending);
    if (attempts) parts.push(`${attempts} ${attempts === 1 ? t.attemptOne : t.attemptMany}`);
  } else if (status === 'running') {
    const name = crewMemberName(state, task.crewMemberId);
    const since = formatClockTime(task.leasedAt);
    if (name) parts.push(since ? `${name} ${t.worksSince} ${since}` : `${name} ${t.worksOn}`);
    else if (task.leaseOwner) parts.push(since ? `${t.leasedSince} ${since}` : t.leasedWord);
  } else if (status === 'queued') {
    const assigned = crewMemberName(state, task.crewAssignedMemberId);
    if (assigned) parts.push(`${t.assignedTo} ${assigned}`);
  }
  const note = String(task.statusNote || task.error || '').trim();
  if (note && (status === 'failed' || status === 'blocked')) parts.push(note.length > 140 ? `${note.slice(0, 137)}…` : note);
  return parts.join(' · ');
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
  return payload.title || payload.instruction || doc.command_type || doc.command_id || 'Auftrag';
}

function resultSummary(result) {
  if (!result || typeof result !== 'object') return '';
  if (Array.isArray(result.record_ids)) return `${result.record_ids.length} records · ${result.definition_id || result.collection || 'business_records'}`;
  if (result.record_id) return `${result.record_id} · ${result.definition_id || result.collection || 'business_records'}`;
  if (result.artifact_path) return result.artifact_path;
  return '';
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
  const response = await fetch(moduleAssetUrl('./index.html'));
  if (!response.ok) {
    throw new Error(`CTOX markup unavailable: HTTP ${response.status}`);
  }
  return response.text();
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
  taskPromptDisplay,
  aggregateFlowMetrics,
  crewHomeMarkup,
  crewMemberDrawer,
  memoryEntries,
  confirmAnchorBody,
  memberDomainLine,
  taskSelectionSentence,
  memberIdentity,
  memberCreatureState,
  crewStripMarkup,
  shouldShowCrewHome,
  taskCrewMember,
  aggregateRunMetrics,
  harnessFlowFromEvents,
  liveActivityFromEvents,
  withLiveActivity,
  flowForSelectedTask,
  reconcileSelection,
  changeConcernsSelectedTask,
  normalizeExecutionProgress,
  taskTelemetry,
  emptyTelemetry,
  executionProgressBar,
  executionPlanSteps,
  liveElapsedSeconds,
  metricSubjectTask,
  authoritativeTaskNodeId,
  authoritativeTaskStatus,
  buildHarnessModel,
  buildInboundChannels,
  inboundEndpointFlowSvg,
  canModifyCtoxApp,
  clampMetric,
  deriveHarnessHealth,
  eventToNodeId,
  flowSvg,
  flowSourceView,
  formatRelativeAge,
  friendlyWebStackStatus,
  labels,
  mergeBundleWithCommands,
  progressPercent,
  safeTaskDisplayText,
  setFlowZoom,
  taskSteps,
  taskSummaryReason,
  taskDiagnosticMarkup,
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
  renderMain,
  applyTaskSelection,
  webStackPanel,
  taskPipelineStage,
  flowCrewSvg,
  taskCrewNodeId,
  taskCrewStatus,
  wireTaskSourceReadiness,
};
