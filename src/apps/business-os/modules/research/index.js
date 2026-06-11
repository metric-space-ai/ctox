import { loadModuleMessages } from '../../shared/i18n.js';

const BUILD = '20260608-skf-dashboard-sync-refresh1';
const DEFAULT_AXIS_X = 'evidence_strength';
const DEFAULT_AXIS_Y = 'topic_fit';
const ROW_LIMIT = 5000;
const COLLECTION_READ_TIMEOUT_MS = 10000;
const POST_SYNC_REFRESH_LIMIT = 3;
const RESEARCH_COLLECTIONS = Object.freeze([
  'business_commands',
  'ctox_queue_tasks',
  'research_tasks',
  'research_runs',
  'research_notes',
  'knowledge_tables',
]);
const STOP_TERMS = new Set(['eine', 'einen', 'einer', 'eines', 'und', 'oder', 'auf', 'basis', 'nutze', 'score', 'quellen', 'source', 'sources', 'dashboard', 'research', 'knowledge', 'base', 'table', 'data', 'fuer', 'from', 'with', 'that', 'this', 'the']);

const BASE_AXES = Object.freeze([
  { id: 'evidence_strength', label: 'Evidence strength' },
  { id: 'topic_fit', label: 'Topic fit' },
  { id: 'source_quality', label: 'Source quality' },
  { id: 'actionability', label: 'Actionability' },
  { id: 'coverage', label: 'Coverage' },
  { id: 'portfolio_priority', label: 'Portfolio priority' },
]);

const BEARING_AXES = Object.freeze([
  { id: 'evidence_strength', label: 'Evidence strength' },
  { id: 'direct_load_relevance', label: 'Load relevance' },
  { id: 'data_density', label: 'Data density' },
  { id: 'reuse_readiness', label: 'Reuse readiness' },
  { id: 'portfolio_priority', label: 'Portfolio priority' },
]);

const COMPETITIVE_AI_AXES = Object.freeze([
  { id: 'overlap', label: 'Overlap' },
  { id: 'buyer_clarity', label: 'Buyer clarity' },
  { id: 'autonomous_agent_depth', label: 'Autonomous agent depth' },
  { id: 'enterprise_readiness', label: 'Enterprise readiness' },
  { id: 'trust_compliance', label: 'Trust/compliance' },
  { id: 'integration_api', label: 'Integration/API' },
  { id: 'pricing_clarity', label: 'Pricing clarity' },
  { id: 'proof_customer_evidence', label: 'Customer proof' },
  { id: 'evidence_strength', label: 'Evidence strength' },
  { id: 'portfolio_priority', label: 'Portfolio priority' },
]);

const RESEARCH_TABLE_CONTRACT = Object.freeze({
  source_catalog: {
    title: 'Source Catalog',
    columns: [
      'source_id',
      'title',
      'source_url',
      'source_type',
      'publisher',
      'discovery_query',
      'discovered_at',
      'read_status',
      'contribution_note',
      'evidence_relevance',
      'review_status',
    ],
  },
  evidence_points: {
    title: 'Evidence Points',
    columns: [
      'evidence_id',
      'source_id',
      'criterion_id',
      'fact_label',
      'fact_value',
      'fact_unit',
      'quote',
      'source_url',
      'extracted_at',
      'confidence',
    ],
  },
  evaluation_matrix: {
    title: 'Evaluation Matrix',
    columns: [
      'option_id',
      'source_id',
      'title',
      'criterion_scores_json',
      'weighted_total',
      'confidence',
      'rationale',
      'updated_at',
    ],
  },
});

const DRONE_SOURCES_METADATA = Object.freeze({
  'nasa-mtb2': {
    group: 'nasa',
    kind: 'Windkanal / Rotorlasten',
    tags: ['rotorload', 'windtunnel', 'nasa'],
    fields: 'Kräfte/Momente je Rotor, Betriebspunkt, Testmatrix, Rotorpositionen',
    use: 'Beste öffentliche Basis für Mehrkomponenten-Rotorlasten; danach Geometrie und Vorzeichen sauber klären.',
    missing: 'Keine motorinterne Lagerreaktion, keine reale Feldalterung, Lagerabstand muss separat kommen.',
    links: [
      ['NASA Artikel', 'https://www.nasa.gov/directorates/armd/aavp/armd-aavp-rvlt/multirotor-test-bed/'],
      ['Rotorcraft Programmseite', 'https://rotorcraft.arc.nasa.gov/Research/Programs/MTB2.html'],
      ['Data Report PDF', 'https://rotorcraft.arc.nasa.gov/Publications/files/MTB2_Data_Report_05222025.pdf'],
      ['ReadMe XLSX', 'https://rotorcraft.arc.nasa.gov/Research/Programs/MTB2ReadMe.xlsx'],
      ['MTB2 Data XLSX', 'https://rotorcraft.arc.nasa.gov/Research/Programs/mtbii_data_tables_v2.xlsx'],
      ['Rotor Positions XLSX', 'https://rotorcraft.arc.nasa.gov/Research/Programs/MTB2_Rotor_Positions_public_v1.xlsx']
    ]
  },
  'uiuc': {
    group: 'bench',
    kind: 'Windkanal / Propellerkennfeld',
    tags: ['bench', 'propeller', 'windtunnel'],
    fields: 'CT, CP, Schub-/Drehmomentkoeffizienten, Advance Ratio, statische Sweeps',
    use: 'Propellerkennfelder für Schub/Drehmoment bei RPM und Luftgeschwindigkeit.',
    missing: 'Keine Lagerreaktionen, kaum Querkräfte/Momente am Motor.',
    links: [
      ['UIUC Propeller Database', 'https://m-selig.ae.illinois.edu/props/propDB.html'],
      ['Download Archiv', 'https://m-selig.ae.illinois.edu/props/UIUC-propDB.zip']
    ]
  },
  'apc': {
    group: 'bench',
    kind: 'Hersteller / Propellerdaten',
    tags: ['bench', 'propeller', 'geometry'],
    fields: 'Performance-Files und Geometriedaten für APC-Propeller',
    use: 'Schnelle Kennfeldquelle, besonders wenn APC-Propeller im Design vorkommen.',
    missing: 'Herstellerfokus; keine Mehrrotor-Interaktion und keine Lagerdaten.',
    links: [
      ['APC Performance Data', 'https://www.apcprop.com/technical-information/performance-data/'],
      ['APC Engineering / Geometry', 'https://www.apcprop.com/technical-information/engineering/']
    ]
  },
  'tyto-db': {
    group: 'bench',
    kind: 'Prüfstand / Motor-Prop-ESC',
    tags: ['bench', 'motor', 'esc', 'propeller'],
    fields: 'Schub, Drehmoment, RPM, Spannung, Strom, elektrische/mechanische Leistung, Effizienz',
    use: 'Gute Quelle für reale Motor-Propeller-ESC-Kombinationen und Plausibilisierung von Herstellerwerten.',
    missing: 'Meist stationär; Querlasten und Rotorinteraktion fehlen.',
    links: [
      ['Tyto Database', 'https://database.tytorobotics.com/'],
      ['How-to Artikel', 'https://www.tytorobotics.com/blogs/articles/how-to-use-the-database-for-drone-motors-propellers-and-escs']
    ]
  },
  'mendeley30': {
    group: 'bench',
    kind: 'Prüfstand / Zeitreihen',
    tags: ['bench', 'propeller', 'timeseries'],
    fields: '100-Hz-Daten, 60 s je Fall, Flight Stand 50, Hover-Bedingung',
    use: 'Nützlich für größere Multicopter-Propeller und Streuung über kurze Zeitfenster.',
    missing: 'Hover/Prüfstand, keine reale Flugumgebung.',
    links: [
      ['Mendeley Dataset', 'https://data.mendeley.com/datasets/69hhwc3fd3']
    ]
  },
  'kde': {
    group: 'bench',
    kind: 'Herstellerdaten',
    tags: ['bench', 'manufacturer', 'motor'],
    fields: 'Motor/ESC/Propeller-Performance-Charts je Produktfamilie',
    use: 'Guter Plausibilitätscheck bei konkreten KDE-Komponenten.',
    missing: 'Herstellerabhängig, oft tabellarisch ohne Rohdaten und Querlasten.',
    links: [
      ['KDE Dynamometer Development', 'https://www.kdedirect.com/pages/dynamometer-development']
    ]
  },
  'px4-review': {
    group: 'flight',
    kind: 'Fluglogs',
    tags: ['flightlog', 'duty', 'px4'],
    fields: 'ULog, Aktuatorausgänge, Sensorik, Batteriesystem, Flugzustände; je nach Log auch weitere Topics',
    use: 'Reale Missionsprofile und Zeitanteile; gut zum Aufbau eines Duty Cycles.',
    missing: 'Lasten müssen über Kennfelder oder Modelle abgeleitet werden.',
    links: [
      ['PX4 Flight Review', 'https://review.px4.io/'],
      ['PX4 Flight Reporting', 'https://docs.px4.io/main/en/getting_started/flight_reporting'],
      ['PX4 Statistical Log Analysis', 'https://docs.px4.io/main/uk/dev_log/flight_log_analysis_statistical']
    ]
  },
  'ardupilot': {
    group: 'flight',
    kind: 'Fluglogs / Vibration',
    tags: ['flightlog', 'vibration', 'ardupilot'],
    fields: 'DataFlash Logs, ACC/GYR, hochfrequente IMU-Samples, FFT-Analyse',
    use: 'Sehr nützlich für Unwucht, Propellerschäden und Resonanzanalyse.',
    missing: 'Keine direkten Rotorlasten; Motordaten hängen stark von Setup und Parametern ab.',
    links: [
      ['DataFlash Logs', 'https://ardupilot.org/copter/docs/common-downloading-and-analyzing-data-logs-in-mission-planner.html'],
      ['IMU Batch Sampler', 'https://ardupilot.org/copter/docs/common-imu-batchsampling.html'],
      ['Raw IMU Logging', 'https://ardupilot.org/dev/docs/common-raw-imu-logging.html']
    ]
  },
  'vid': {
    group: 'flight',
    kind: 'Realflug / Dynamikdaten',
    tags: ['flightlog', 'rotorload', 'dynamics'],
    fields: 'Rotor speed, motor current, control inputs, Ground-Truth 6-axis force, Visual-Inertial-Daten',
    use: 'Gute Brücke zwischen Fluglog und Dynamikdaten; interessant für externe Kraftschätzung.',
    missing: 'Spezifische Plattform; Übertragbarkeit auf eigene Motor-/Lagergeometrie prüfen.',
    links: [
      ['arXiv Paper', 'https://arxiv.org/abs/2103.11152'],
      ['VID Dataset GitHub', 'https://github.com/ZJU-FAST-Lab/VID-Dataset'],
      ['VID Platform GitHub', 'https://github.com/ZJU-FAST-Lab/VID-Flight-Platform']
    ]
  },
  'fault-vib': {
    group: 'flight',
    kind: 'Vibration / Fehlerdaten',
    tags: ['vibration', 'fault', 'bench'],
    fields: 'Vibrationsdaten aus Ground Tests mit Propellerfehlern und unterschiedlichen Drehzahlen',
    use: 'Gut für Risikofälle: Unwucht, beschädigte Propeller, Zustandsüberwachung.',
    missing: 'Nicht als Nennlastquelle verwenden; Ground-Test statt Flug.',
    links: [
      ['Mendeley Fault Dataset', 'https://data.mendeley.com/datasets/xkvfjmm8zg']
    ]
  },
  'px4-sih': {
    group: 'simulation',
    kind: 'Simulation',
    tags: ['simulation', 'px4', 'propeller'],
    fields: 'CT(J), CP(J), Advance Ratio, physikalische Parameter, Aktuatorausgänge',
    use: 'Lastfälle systematisch erzeugen und mit UIUC/NASA/Prüfstandsdaten kalibrieren.',
    missing: 'Kein Ersatz für gemessene Rotorlasten; Modellparameter bestimmen Ergebnis.',
    links: [
      ['PX4 SIH Simulation', 'https://docs.px4.io/main/en/sim_sih/']
    ]
  },
  'rotors': {
    group: 'simulation',
    kind: 'Simulation / MAV',
    tags: ['simulation', 'gazebo', 'mav'],
    fields: 'Multirotor-Modelle, IMU/Odometrie/Sensoren, Controller- und World-Dateien',
    use: 'Nützlich für Architektur- und Reglerlastfälle; Messdaten zur Kalibrierung nötig.',
    missing: 'Aerodynamische Details und Lagerkräfte nur modellabhängig.',
    links: [
      ['RotorS GitHub', 'https://github.com/ethz-asl/rotors_simulator']
    ]
  }
});

const state = {
  ctx: null,
  lang: 'de',
  t: (key, fallback) => fallback ?? key,
  tasks: [],
  runs: [],
  notes: [],
  commands: [],
  queueTasks: [],
  knowledgeBases: [],
  selectedTaskId: '',
  selectedSourceId: '',
  selectedReportId: '',
  reportContents: {},
  activeTab: 'sources',
  sourcesViewMode: 'shards',
  showDiagram: false,
  sourceSearchTerm: '',
  sourceActiveTag: 'all',
  mapMode: 'portfolio',
  sourceRows: [],
  curatedRows: [],
  measurementRows: [],
  sourceModels: [],
  map: {
    scale: 1,
    panX: 0,
    panY: 0,
    drag: null,
  },
  status: '',
  diagnostics: {
    collections: {},
    reloadStartedAt: 0,
    reloadFinishedAt: 0,
    reloadCount: 0,
    postSyncRefreshes: 0,
  },
  refreshTimer: null,
  cleanup: [],
  contextMenu: null,
};

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';

  // Load dynamic translations
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, {});
  state.t = (key, fallback, ...args) => {
    let val = messages[key] ?? fallback ?? key;
    if (args.length) {
      args.forEach((arg, i) => {
        val = val.replace(`{${i}}`, arg);
      });
    }
    return val;
  };

  await ensureStyles();
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  bindEvents(ctx.host);
  
  // Globals for reports explorer
  window.selectReport = (reportId) => {
    state.selectedReportId = reportId;
    renderCenter();
  };
  window.showPromptViewer = (filename) => {
    showPromptViewer(filename);
  };
  
  // Local-first: subscribe BEFORE starting sync so any replicated write
  // re-renders, then start the WebRTC bridges in the BACKGROUND. Awaiting the
  // 6-collection bridge handshake here used to freeze the Research open for
  // 1-2s before anything appeared; the sync toast covers "still loading" and
  // `schedulePostSyncRefresh` + `wireRealtime` refresh once data lands.
  wireRealtime();
  wireSyncDiagnosticsRefresh();
  state.cleanup.push(initResearchContextMenu());
  startResearchCollections().catch((error) => {
    console.warn('[research] background sync start failed', error);
  });
  await refreshAll({ seed: true });
  schedulePostSyncRefresh(1200);
  return () => {
    // Cleanup globals
    delete window.selectReport;
    delete window.showPromptViewer;
    
    state.cleanup.forEach((fn) => fn?.());
    state.cleanup = [];
    if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
    state.refreshTimer = null;
    state.contextMenu?.remove();
    state.contextMenu = null;
    ctx.host.replaceChildren();
  };
}

async function startResearchCollections() {
  await Promise.all(RESEARCH_COLLECTIONS.map(async (collection) => {
    if (typeof state.ctx.sync?.startCollection !== 'function') {
      markCollectionDiagnostic(collection, 'sync', 'local', state.t('localOnly', 'Lokaler Modus'));
      return;
    }
    try {
      await state.ctx.sync.startCollection(collection);
      markCollectionDiagnostic(collection, 'sync', 'ok', state.t('syncReady', 'Sync bereit'));
    } catch (error) {
      markCollectionDiagnostic(collection, 'sync', 'failed', errorMessage(error));
    }
  }));
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  return doc.body.innerHTML;
}

function bindEvents(root) {
  root.addEventListener('click', async (event) => {
    const action = event.target.closest('[data-action]')?.dataset.action;
    if (!action) return;
    const target = event.target.closest('[data-action]');
    if (action === 'select-task') {
      state.selectedTaskId = target.dataset.taskId || '';
      state.selectedSourceId = '';
      await loadDashboardData();
      render();
    } else if (action === 'select-source') {
      state.selectedSourceId = target.dataset.sourceId || '';
      render();
    } else if (action === 'tab') {
      state.activeTab = target.dataset.tab || 'sources';
      renderCenter();
    } else if (action === 'map-mode') {
      state.mapMode = target.dataset.mapMode || 'portfolio';
      // Zoom & Pan state is persistent, do not reset view!
      renderCenter();
    } else if (action === 'refresh') {
      await refreshAll();
    } else if (action === 'new-task') {
      openTaskDialog();
    } else if (action === 'edit-task') {
      openTaskDialog(selectedTask());
    } else if (action === 'reset-map') {
      resetMapView();
    } else if (action === 'run-research') {
      await runSelectedResearch();
    } else if (action === 'open-knowledge') {
      openKnowledgeTable(target.dataset.tableId || '');
    } else if (action === 'source-detail') {
      openSourceDrawer(target.dataset.sourceId || '');
    } else if (action === 'focus-ctox-run') {
      focusCtoxRun(target.dataset.taskQueueId || '', target.dataset.commandId || '');
    } else if (action === 'sources-view') {
      state.sourcesViewMode = target.dataset.viewMode || 'shards';
      renderCenter();
    } else if (action === 'toggle-diagram') {
      state.showDiagram = !state.showDiagram;
      const centerBody = root.querySelector('.research-center-body');
      if (centerBody) {
        if (state.showDiagram) {
          centerBody.classList.remove('has-hidden-map');
        } else {
          centerBody.classList.add('has-hidden-map');
        }
      }
      renderCenter();
    } else if (action === 'source-tag-filter') {
      state.sourceActiveTag = target.dataset.tagId || 'all';
      renderCenter();
    }
  });
  root.addEventListener('change', (event) => {
    const axis = event.target.closest('[data-axis-select]');
    if (!axis) return;
    updateTaskAxis(axis.dataset.axisSelect, axis.value).catch((error) => {
      console.error('[research] axis update failed', error);
    });
  });
  root.addEventListener('input', (event) => {
    const searchInput = event.target.closest('[data-action="source-search"]');
    if (searchInput) {
      const selectionStart = searchInput.selectionStart;
      const selectionEnd = searchInput.selectionEnd;
      state.sourceSearchTerm = searchInput.value;
      renderCenter();
      const restoredInput = document.getElementById('research-source-search-input');
      if (restoredInput) {
        restoredInput.focus();
        if (selectionStart !== null && selectionEnd !== null) {
          restoredInput.setSelectionRange(selectionStart, selectionEnd);
        }
      }
    }
  });
  root.addEventListener('wheel', handleMapWheel, { passive: false });
  root.addEventListener('pointerdown', handleMapPointerDown);
  root.addEventListener('pointermove', handleMapPointerMove);
  root.addEventListener('pointerup', stopMapDrag);
  root.addEventListener('pointercancel', stopMapDrag);
}

async function refreshAll({ seed = false } = {}) {
  state.diagnostics.reloadStartedAt = Date.now();
  state.diagnostics.reloadFinishedAt = 0;
  state.diagnostics.reloadCount += 1;
  setStatus(state.t('loadingKnowledge', 'Knowledge wird geladen...'));
  state.knowledgeBases = await loadKnowledgeBases();
  await loadLocalState();
  if (seed) await ensureTasksFromKnowledgeBases();
  if (!state.selectedTaskId || !state.tasks.some((task) => task.id === state.selectedTaskId)) {
    state.selectedTaskId = state.tasks[0]?.id || '';
  }
  await loadDashboardData();
  render();
  state.diagnostics.reloadFinishedAt = Date.now();
  setStatus(reloadStatusText());
}

async function loadLocalState() {
  const [tasks, runs, notes, commands, queueTasks] = await Promise.all([
    findAll(state.ctx.db.research_tasks, 'research_tasks'),
    findAll(state.ctx.db.research_runs, 'research_runs'),
    findAll(state.ctx.db.research_notes, 'research_notes'),
    findAll(state.ctx.db.business_commands, 'business_commands'),
    findAll(state.ctx.db.ctox_queue_tasks, 'ctox_queue_tasks'),
  ]);
  if (tasks.length || !state.tasks.length) {
    state.tasks = tasks
      .filter((task) => isVisibleResearchTask(task))
      .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
  }
  if (runs.length || !state.runs.length) state.runs = runs;
  if (notes.length || !state.notes.length) state.notes = notes;
  if (commands.length || !state.commands.length) state.commands = commands;
  if (queueTasks.length || !state.queueTasks.length) state.queueTasks = queueTasks;
}

function wireRealtime() {
  const raw = state.ctx?.db || {};
  const collections = [
    raw.research_tasks,
    raw.research_runs,
    raw.research_notes,
    raw.business_commands,
    raw.ctox_queue_tasks,
    raw.knowledge_tables,
  ].filter(Boolean);
  for (const collection of collections.filter((collection) => collection !== raw.knowledge_tables)) {
    const subscription = collection.$?.subscribe?.(() => scheduleLocalRefresh(80));
    if (subscription?.unsubscribe) state.cleanup.push(() => subscription.unsubscribe());
  }
  const knowledgeSubscription = raw.knowledge_tables?.$?.subscribe?.(() => scheduleKnowledgeRefresh(120));
  if (knowledgeSubscription?.unsubscribe) state.cleanup.push(() => knowledgeSubscription.unsubscribe());
}

function wireSyncDiagnosticsRefresh() {
  const listener = (event) => {
    const collections = event?.detail?.collections
      || event?.detail?.snapshot?.collections
      || window.ctoxBusinessOsSyncDiagnostics?.collections
      || {};
    const hasResearchCollection = RESEARCH_COLLECTIONS.some((name) => {
      const info = collections[name];
      return info?.initialReplicationState === 'complete' || info?.status === 'connected' || info?.status === 'reused';
    });
    if (hasResearchCollection) schedulePostSyncRefresh(250);
  };
  window.addEventListener('ctox-business-os-sync-diagnostics', listener);
  state.cleanup.push(() => window.removeEventListener('ctox-business-os-sync-diagnostics', listener));
}

function schedulePostSyncRefresh(delay = 250) {
  if (state.diagnostics.postSyncRefreshes >= POST_SYNC_REFRESH_LIMIT) return;
  state.diagnostics.postSyncRefreshes += 1;
  scheduleKnowledgeRefresh(delay);
}

function scheduleLocalRefresh(delay = 80) {
  if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
  state.refreshTimer = window.setTimeout(async () => {
    state.refreshTimer = null;
    await loadLocalState();
    render();
  }, delay);
}

function scheduleKnowledgeRefresh(delay = 120) {
  if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
  state.refreshTimer = window.setTimeout(async () => {
    state.refreshTimer = null;
    state.knowledgeBases = await loadKnowledgeBases();
    await loadLocalState();
    await ensureTasksFromKnowledgeBases();
    if (!state.selectedTaskId || !state.tasks.some((task) => task.id === state.selectedTaskId)) {
      state.selectedTaskId = state.tasks[0]?.id || '';
    }
    await loadDashboardData();
    render();
  }, delay);
}

function isVisibleResearchTask(task) {
  if (/^outbound(?:_|$)/.test(String(task.knowledge_domain || ''))) return false;
  if (!task?.payload?.seeded_from_knowledge) return true;
  const base = state.knowledgeBases.find((item) => item.domain === task.knowledge_domain);
  return Boolean(base && isResearchKnowledgeBase(base));
}

async function ensureTasksFromKnowledgeBases() {
  for (const base of state.knowledgeBases.filter(isResearchKnowledgeBase)) {
    const existing = state.tasks.find((task) => task.knowledge_domain === base.domain);
    if (existing) continue;
    const now = Date.now();
    const task = {
      id: `research_${slugId(base.domain)}`,
      title: base.title,
      prompt: defaultPromptForKnowledgeBase(base),
      criteria: state.t('evidenceNoteText', 'Nutze die vorhandene Knowledge Base als Ausgangspunkt. Score nur belegte Quellen und trenne Rohkandidaten von kuratierten Dashboard-Ergebnissen.'),
      status: 'ready',
      knowledge_domain: base.domain,
      source_catalog_key: tableKey(base, ['source_catalog', 'sources', 'curated_sources']) || 'source_catalog',
      curated_table_key: tableKey(base, ['evaluation_matrix', 'load_data_library', 'curated_sources', 'source_library']) || 'evaluation_matrix',
      measurements_table_key: tableKey(base, ['evidence_points', 'measured_load_points', 'measurements']) || 'evidence_points',
      x_axis: defaultAxisPairForTask(base).x,
      y_axis: defaultAxisPairForTask(base).y,
      payload: {
        seeded_from_knowledge: true,
        scoring_dimensions: inferScoringDimensions({ knowledge_domain: base.domain, title: base.title, prompt: defaultPromptForKnowledgeBase(base), criteria: '' }),
        scoring_weights: scoringWeights(inferScoringDimensions({ knowledge_domain: base.domain, title: base.title, prompt: defaultPromptForKnowledgeBase(base), criteria: '' })),
        table_contract: RESEARCH_TABLE_CONTRACT,
        source_table_ids: base.tables.map((table) => table.id),
      },
      created_at_ms: now,
      updated_at_ms: now,
    };
    await upsertDoc(state.ctx.db.research_tasks, task).catch((error) => {
      console.warn('[research] could not persist seeded task', error);
    });
    state.tasks.push(task);
  }
}

async function loadKnowledgeBases() {
  const tables = await loadKnowledgeTables();
  return knowledgeBasesFromTables(tables);
}

function knowledgeBasesFromTables(tables = []) {
  const byDomain = new Map();
  for (const rawTable of tables) {
    const source = rawTable?.payload && typeof rawTable.payload === 'object' ? rawTable.payload : rawTable;
    const domain = String(source?.domain || '').trim();
    const tableKey = String(source?.table_key || '').trim();
    if (!domain || !tableKey) continue;
    const table = {
      ...source,
      id: source.id || rawTable?.id || `${domain}:${tableKey}`,
      domain,
      table_key: tableKey,
    };
    if (!byDomain.has(domain)) {
      byDomain.set(domain, {
        id: domain,
        domain,
        title: titleFromDomain(domain),
        description: '',
        tables: [],
      });
    }
    const base = byDomain.get(domain);
    base.tables.push(table);
    if (!base.description && table.description) base.description = table.description;
  }
  return [...byDomain.values()]
    .map((base) => ({ ...base, tables: base.tables.sort((a, b) => String(a.table_key).localeCompare(String(b.table_key))) }))
    .sort((a, b) => scoreResearchBase(b) - scoreResearchBase(a) || a.title.localeCompare(b.title));
}

async function loadKnowledgeTables() {
  return findAll(state.ctx?.db?.knowledge_tables, 'knowledge_tables');
}

function isResearchKnowledgeBase(base) {
  if (!base?.tables?.length) return false;
  if (/^outbound(?:_|$)/.test(String(base.domain || ''))) return false;
  const text = [base.domain, base.title, base.description, ...base.tables.flatMap((table) => [table.table_key, table.title, table.description])].join(' ').toLowerCase();
  return /research|source|catalog|load|bearing|measurement|evidence|market|competitive|portfolio/.test(text);
}

function scoreResearchBase(base) {
  const keys = new Set(base.tables.map((table) => table.table_key));
  let score = 0;
  if (keys.has('source_catalog')) score += 6;
  if (keys.has('curated_sources') || keys.has('load_data_library')) score += 4;
  if (keys.has('measured_load_points') || keys.has('measurements')) score += 3;
  if (/research|bearing|load|competitive/i.test(base.domain)) score += 2;
  return score;
}

async function loadDashboardData() {
  const task = selectedTask();
  state.sourceRows = [];
  state.curatedRows = [];
  state.measurementRows = [];
  state.sourceModels = [];
  if (!task) return;
  const base = knowledgeBaseForTask(task);
  const sourceTable = tableForKey(base, task.source_catalog_key) || firstTableMatching(base, /source|catalog|curated/i);
  const curatedTable = tableForKey(base, task.curated_table_key) || firstTableMatching(base, /library|curated/i);
  const measurementTable = tableForKey(base, task.measurements_table_key) || firstTableMatching(base, /measure|load|point/i);
  const [sourceRows, curatedRows, measurementRows] = await Promise.all([
    sourceTable ? fetchTableRows(sourceTable.id) : Promise.resolve([]),
    curatedTable && curatedTable.id !== sourceTable?.id ? fetchTableRows(curatedTable.id) : Promise.resolve([]),
    measurementTable && measurementTable.id !== sourceTable?.id && measurementTable.id !== curatedTable?.id ? fetchTableRows(measurementTable.id) : Promise.resolve([]),
  ]);
  state.sourceRows = sourceRows;
  state.curatedRows = curatedRows;
  state.measurementRows = measurementRows;
  state.sourceModels = buildSourceModels(task, sourceRows, curatedRows, measurementRows);
  if (!state.selectedSourceId || !state.sourceModels.some((item) => item.id === state.selectedSourceId)) {
    state.selectedSourceId = state.sourceModels[0]?.id || '';
  }
}

async function fetchTableRows(tableId) {
  if (!tableId) return [];
  const table = state.knowledgeBases
    .flatMap((base) => base.tables || [])
    .find((entry) => entry.id === tableId);
  const rows = firstArray(
    table?.rows,
    table?.records,
    table?.data,
    table?.payload?.rows,
    table?.payload?.records,
    table?.payload?.data,
    table?.dataframe?.rows,
    table?.payload?.dataframe?.rows,
  );
  if (rows.length) {
    return rows.slice(0, ROW_LIMIT).map((row) => row && typeof row === 'object' ? row : { value: row });
  }
  // Record-shaped rows flow exclusively through the RxDB/WebRTC mesh: CTOX, as
  // the authoritative peer, materializes the parquet records into the synced
  // knowledge_tables doc. There is no HTTP fallback — if a doc carries no rows
  // yet, we surface nothing until replication delivers them.
  markCollectionDiagnostic('knowledge_tables', 'read', 'ok', `0 synced rows (${String(tableId || '')})`);
  return [];
}

function buildSourceModels(task, sourceRows, curatedRows, measurementRows) {
  const measurementAgg = aggregateMeasurements(measurementRows);
  const curatedBySource = new Map();
  for (const row of curatedRows) {
    const id = sourceId(row);
    if (id) curatedBySource.set(id, row);
  }
  const raw = sourceRows.length ? sourceRows : curatedRows;
  return raw.map((row, index) => {
    const id = sourceId(row) || `source_${index + 1}`;
    const title = firstString(row, ['title', 'source_title', 'name']) || `Source ${index + 1}`;
    const sourceClass = firstString(row, ['source_class', 'type', 'bucket', 'record_type']) || 'source';
    const note = firstString(row, ['contribution_note', 'contribution', 'summary', 'relevance_to_bearing_design', 'use']) || '';
    const curated = curatedBySource.get(id);
    const agg = measurementAgg.get(id) || null;
    const axisDefs = scoringDimensionsForTask(task);
    const dimensions = scoreDimensions(row, curated, agg, task, axisDefs);
    return {
      id,
      rank: index + 1,
      title,
      subtitle: sourceClass,
      url: firstString(row, ['source_url', 'url', 'direct_url', 'doi']) || '',
      sourceClass,
      note,
      row,
      curated,
      measurements: agg,
      dimensions,
      score: dimensions.portfolio_priority,
      grade: gradeForScore(dimensions.portfolio_priority),
    };
  }).sort((a, b) => b.score - a.score).map((item, index) => ({ ...item, rank: index + 1 }));
}

function aggregateMeasurements(rows) {
  const bySource = new Map();
  for (const row of rows || []) {
    const id = sourceId(row);
    if (!id) continue;
    const current = bySource.get(id) || {
      count: 0,
      maxAxial: 0,
      maxRadial: 0,
      maxRpm: 0,
      files: new Set(),
    };
    current.count += 1;
    current.maxAxial = Math.max(current.maxAxial, numberValue(row.axial_load_N ?? row.thrust_N));
    current.maxRadial = Math.max(current.maxRadial, numberValue(row.radial_load_N));
    current.maxRpm = Math.max(current.maxRpm, numberValue(row.rpm));
    if (row.source_file) current.files.add(String(row.source_file));
    bySource.set(id, current);
  }
  for (const value of bySource.values()) {
    value.files = [...value.files];
  }
  return bySource;
}

function scoreDimensions(row, curated, measurements, task, axisDefs = BASE_AXES) {
  const text = [row.title, row.name, row.description, row.summary, row.contribution_note, row.relevance_to_bearing_design, row.bucket, row.source_class, row.record_type, curated?.use, curated?.fields].join(' ').toLowerCase();
  const sourceClass = String(row.source_class || row.bucket || curated?.record_type || '').toLowerCase();
  let evidence = 38;
  if (/dataset|repository|zenodo|figshare|dataverse|csv|xlsx|parquet/.test(text)) evidence = 88;
  else if (/agency|standard|regulatory|nasa|faa|easa|dod|osti|dtic/.test(text)) evidence = 78;
  else if (/scholarly|paper|doi|springer|ieee|aiaa|semantic/.test(sourceClass + text)) evidence = 66;
  else if (/web|manufacturer|vendor|datasheet/.test(sourceClass + text)) evidence = 52;
  if (row.doi || /\bdoi\b|openalex|arxiv/.test(text)) evidence += 6;
  if (row.source_url || row.url) evidence += 4;
  if (measurements?.count) evidence += 15; // High-fidelity boost for sources with active telemetry/measured data points!

  let relevance = 30;
  for (const term of ['bearing', 'load', 'thrust', 'torque', 'rpm', 'propeller', 'rotor', 'vibration', 'force', 'moment']) {
    if (text.includes(term)) relevance += 7;
  }
  if (measurements?.count) relevance += 12;

  const dataDensity = Math.min(96, 28 + (measurements?.count || 0) * 5 + (curated ? 18 : 0) + (text.length > 260 ? 12 : 0));
  const reuseReadiness = Math.min(96, 34 + (curated ? 22 : 0) + (measurements?.count ? 26 : 0) + (/csv|xlsx|parquet|dataset|database/.test(text) ? 14 : 0));
  const sourceQuality = Math.min(96, evidence * 0.78 + (hasUrl(row) ? 10 : 0) + (/official|customer|case|docs|security|compliance|api|integration/.test(text) ? 8 : 0));
  const actionability = Math.min(96, 30 + (/pricing|demo|contact|docs|api|integration|onboard|trial|workflow|use case|customer|case/.test(text) ? 28 : 0) + (curated ? 18 : 0) + (hasUrl(row) ? 8 : 0));
  const coverage = Math.min(96, 28 + Math.min(32, text.length / 18) + (measurements?.count ? 14 : 0) + (curated ? 12 : 0));
  const topicFit = topicFitScore(task, text, row);
  const overlap = Math.min(96, topicFit + (/competitor|platform|agent|employee|worker|enterprise|autonomous|managed|team|workflow/.test(text) ? 12 : 0));
  const buyerClarity = Math.min(96, 30 + (/buyer|persona|enterprise|team|department|role|use case|solution|customer|sales|support|operations|hr|it/.test(text) ? 28 : 0) + (/official|homepage|product|pricing|case/.test(text) ? 12 : 0));
  const autonomousAgentDepth = Math.min(96, 24 + (/autonomous|agentic|agent|worker|employee|multi-agent|workflow|orchestration|tool use|executes|delegates|team/.test(text) ? 36 : 0) + (/copilot|assistant/.test(text) ? -8 : 0));
  const enterpriseReadiness = Math.min(96, 28 + (/enterprise|security|sso|soc 2|gdpr|compliance|admin|governance|sla|deployment|integration|api/.test(text) ? 34 : 0));
  const trustCompliance = Math.min(96, 26 + (/security|compliance|soc 2|iso|gdpr|privacy|trust|audit|case study|customer|testimonial/.test(text) ? 34 : 0) + (hasUrl(row) ? 6 : 0));
  const integrationApi = Math.min(96, 24 + (/api|integration|connector|webhook|sdk|slack|salesforce|hubspot|zendesk|jira|microsoft|google/.test(text) ? 38 : 0));
  const pricingClarity = Math.min(96, 22 + (/pricing|price|plan|seat|usage|quote|trial|demo/.test(text) ? 40 : 0));
  const proofCustomerEvidence = Math.min(96, 24 + (/customer|case study|testimonial|logo|proof|review|gartner|forrester|report|benchmark|study/.test(text) ? 34 : 0) + (evidence > 70 ? 8 : 0));
  const portfolio = weightedAverage([
    [topicFit, 0.24],
    [evidence, 0.22],
    [sourceQuality, 0.18],
    [actionability, 0.18],
    [coverage, 0.18],
  ]);
  const scores = {
    evidence_strength: clampScore(evidence),
    topic_fit: clampScore(topicFit),
    source_quality: clampScore(sourceQuality),
    actionability: clampScore(actionability),
    coverage: clampScore(coverage),
    direct_load_relevance: clampScore(relevance),
    data_density: clampScore(dataDensity),
    reuse_readiness: clampScore(reuseReadiness),
    overlap: clampScore(overlap),
    buyer_clarity: clampScore(buyerClarity),
    autonomous_agent_depth: clampScore(autonomousAgentDepth),
    enterprise_readiness: clampScore(enterpriseReadiness),
    trust_compliance: clampScore(trustCompliance),
    integration_api: clampScore(integrationApi),
    pricing_clarity: clampScore(pricingClarity),
    proof_customer_evidence: clampScore(proofCustomerEvidence),
    portfolio_priority: clampScore(portfolio),
  };
  for (const axis of axisDefs) {
    const direct = numberValue(row[axis.id] ?? curated?.[axis.id]);
    if (direct) scores[axis.id] = normalizeScoreScale(direct);
  }
  
  // High-fidelity keyword filter on Title to prevent crawler noise / cross-domain leakage!
  const titleText = String(row.title || row.name || '').toLowerCase();
  const hasDroneTopic = /propeller|rotor|uav|drone|bearing|load|force|moment|thrust|torque|rpm|vibration|spindel|motor|flight|telemetry|aerodynamic|blade|windtunnel|w\u00e4lzlager|lager|schub|drehmoment|last|messung|pr\u00fcfstand|spindle|vibrat|flight|telemetr|testing|bench|load cell|stanag|mil-std/i.test(titleText);
  if (!hasDroneTopic) {
    scores.topic_fit = 10;
  }

  const weightedCriteria = axisDefs
    .filter((axis) => axis.id !== 'portfolio_priority')
    .map((axis) => [scores[axis.id] ?? topicFitScore(task, text, row), Number(axis.weight || 1)]);
  if (weightedCriteria.length) scores.portfolio_priority = clampScore(weightedAverage(weightedCriteria));
  return scores;
}

function render() {
  renderLeft();
  renderCenter();
  renderRight();
}

function renderLeft() {
  const root = pane('left');
  if (!root) return;
  const task = selectedTask();
  root.innerHTML = `
    <header class="research-pane-header">
      <div><span>${escapeHtml(state.t('webResearch', 'Web Research'))}</span><h2>${escapeHtml(state.t('knowledgeDashboards', 'Knowledge Dashboards'))}</h2></div>
      <div class="research-header-actions">
        <button type="button" class="research-icon-button" data-action="refresh" aria-label="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}" title="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}">${iconSvg('refresh')}</button>
        <button type="button" class="research-icon-button" data-action="new-task" aria-label="${escapeHtml(state.t('createResearch', 'Research anlegen'))}" title="${escapeHtml(state.t('createResearch', 'Research anlegen'))}">${iconSvg('plus')}</button>
      </div>
    </header>
    <div class="research-left-scroll">
      <section class="research-section">
        <div class="research-section-head">
          <strong>${escapeHtml(state.t('tasks', 'Aufgaben'))}</strong>
          <span>${state.tasks.length} ${escapeHtml(state.t('active', 'aktiv'))}</span>
        </div>
        <div class="research-task-list">
          ${state.tasks.map(renderTaskButton).join('') || renderNoTasksEmpty()}
        </div>
      </section>
    </div>
  `;
}

function renderTaskButton(task) {
  const isActive = task.id === state.selectedTaskId;
  const base = knowledgeBaseForTask(task);
  const rows = base?.tables?.reduce((sum, table) => sum + Number(table.row_count || 0), 0) || 0;
  return `
    <button type="button" class="research-task-item${isActive ? ' is-active' : ''}" data-action="select-task" data-task-id="${escapeHtml(task.id)}">
      <strong>${escapeHtml(task.title)}</strong>
      <span>${escapeHtml(task.knowledge_domain)} · ${rows.toLocaleString(state.lang === 'de' ? 'de-DE' : 'en-US')} ${escapeHtml(state.t('rows', 'rows'))}</span>
    </button>
  `;
}

function renderRankingRow(source) {
  const selected = source.id === state.selectedSourceId;
  return `
    <button type="button" class="research-rank-row${selected ? ' is-selected' : ''}" data-action="select-source" data-source-id="${escapeHtml(source.id)}" data-context-record-id="${escapeHtml(source.id)}" data-context-record-type="source" data-context-label="${escapeHtml(source.title)}">
      <span class="research-rank">#${source.rank}</span>
      <span class="research-rank-main"><strong>${escapeHtml(source.title)}</strong><small>${escapeHtml(source.subtitle)}</small></span>
      <span class="research-grade research-grade-${source.grade.toLowerCase()}">${source.grade}</span>
      <span class="research-score">${(source.score / 10).toFixed(1)}</span>
    </button>
  `;
}

function renderNoTasksEmpty() {
  const empty = emptyStateForNoTask();
  return `
    <div class="research-empty research-empty-card">
      <strong>${escapeHtml(empty.title)}</strong>
      <span>${escapeHtml(empty.body)}</span>
    </div>
  `;
}

function renderNoSourcesEmpty(task) {
  const failure = diagnosticFailures()[0];
  let body = state.t('noSourcesLoaded', 'Noch keine Quellen geladen.');
  if (failure) {
    body = state.t('sourcesTemporarilyUnavailable', 'Quellen sind gerade nicht verfügbar. Bitte später erneut versuchen.');
  } else if (task && !knowledgeBaseForTask(task)) {
    body = state.t('selectedDomainMissing', 'Die ausgewählte Knowledge Base ist gerade nicht verfügbar.');
  } else if (task) {
    body = state.t('selectedDomainNoSources', 'Diese Knowledge Base enthält noch keine Quellen für dieses Dashboard.');
  }
  return `<div class="research-empty research-empty-card"><strong>${escapeHtml(state.t('sources', 'Sources'))}</strong><span>${escapeHtml(body)}</span></div>`;
}

function emptyStateForNoTask() {
  const failure = diagnosticFailures()[0];
  if (failure) {
    return {
      title: state.t('researchUnavailableTitle', 'Research ist gerade nicht verfügbar'),
      body: state.t('researchUnavailableBody', 'Dashboards erscheinen automatisch, sobald die Knowledge Base verfügbar ist.'),
    };
  }
  if (!state.knowledgeBases.length) {
    return {
      title: state.t('noKnowledgeDomains', 'Noch keine Knowledge Base verfügbar'),
      body: state.t('noKnowledgeDomainsBody', 'Lege eine Knowledge Base an oder lade Inhalte, um ein Research-Dashboard zu starten.'),
    };
  }
  return {
    title: state.t('noResearchTask', 'Keine Research-Aufgabe'),
    body: state.t('createTaskBase', 'Lege eine Aufgabe auf Basis einer Knowledge Base an.'),
  };
}

function renderCenter() {
  const root = pane('center');
  if (!root) return;
  const task = selectedTask();
  if (!task) {
    root.innerHTML = renderNoTaskCenter();
    return;
  }
  const axisPair = normalizedAxisPair(task);
  const xAxis = axisPair.x;
  const yAxis = axisPair.y;
  const isGraphMode = state.mapMode === 'discovery';
  root.innerHTML = `
    <header class="research-pane-header research-center-header">
      <div><span>${escapeHtml(task.knowledge_domain)}</span><h2>${escapeHtml(task.title)}</h2></div>
      <div class="research-center-actions">
        ${state.showDiagram ? `<span class="research-map-hint">Scroll zoom · drag pan</span>` : ''}
        <button type="button"
                class="research-button"
                data-action="toggle-diagram"
                style="margin:0; background:color-mix(in srgb, var(--research-accent) 12%, var(--research-surface-2)); border-color:color-mix(in srgb, var(--research-accent) 25%, var(--research-line)); color:var(--research-accent); font-weight:800; font-size:11px; padding:0 12px; height:30px; border-radius:6px;"
                title="${state.showDiagram ? 'Diagramm ausblenden' : 'Diagramm einblenden'}"
                aria-label="${state.showDiagram ? 'Diagramm ausblenden' : 'Diagramm einblenden'}"
                aria-pressed="${!state.showDiagram}">
          ${state.showDiagram ? 'Karte ausblenden ✖' : 'Karte einblenden 🗺️'}
        </button>
      </div>
    </header>
    <div class="research-center-body${state.showDiagram ? '' : ' has-hidden-map'}">
      <section class="research-map-panel">
        <div class="research-map-head">
          <div><strong>${isGraphMode ? escapeHtml(state.t('discoveryGraph', 'Discovery Graph')) : escapeHtml(state.t('portfolioMap', 'Portfolio Map'))}</strong><span>${isGraphMode ? escapeHtml(state.t('discoverySub', 'Knowledge, Quellen, Messpunkte')) : `${escapeHtml(axisLabel(yAxis))} ${escapeHtml(state.t('portfolioSub', 'gegen'))} ${escapeHtml(axisLabel(xAxis))}`}</span></div>
          ${mapModeToggle()}
          <button type="button" class="research-map-reset" data-action="reset-map" aria-label="${escapeHtml(state.t('resetMapView', 'Kartenansicht zurücksetzen'))}">${escapeHtml(state.t('reset', 'Reset'))}</button>
          <button type="button" class="research-map-reset" data-action="toggle-diagram" title="${state.showDiagram ? 'Einklappen' : 'Ausklappen'}" style="margin-left: 6px;">${state.showDiagram ? 'Einklappen' : 'Ausklappen'}</button>
        </div>
        <div class="research-portfolio-map${isGraphMode ? ' is-discovery-graph' : ''}">
          <div class="research-map-grid" aria-hidden="true"></div>
          <div class="research-map-content" data-map-content style="${mapTransformStyle()}">
            ${isGraphMode ? renderDiscoveryGraph(task) : state.sourceModels.map((source) => renderMapPoint(source, xAxis, yAxis)).join('')}
          </div>
          ${isGraphMode ? '' : axisSelect('y', yAxis, 'map')}
          ${isGraphMode ? '' : axisSelect('x', xAxis, 'map')}
        </div>
      </section>
      <section class="research-workbench">
        <div class="research-tabs-container">
          <div class="research-tabs" role="tablist" aria-label="Research views">
            ${tabButton('sources', `${state.t('sources', 'Sources')} (${state.sourceModels.length})`)}
            ${tabButton('measurements', `${state.t('measurements', 'Measurements')} (${state.measurementRows.length})`)}
            ${tabButton('knowledge', `${state.t('knowledge', 'Knowledge')} (${state.curatedRows.length})`)}
            ${tabButton('reports', `Fachberichte (12)`)}
          </div>
          ${state.activeTab === 'sources' ? `
            <div class="research-view-toggle">
              <button type="button"
                      class="research-view-btn${state.sourcesViewMode === 'table' ? ' is-active' : ''}"
                      data-action="sources-view"
                      data-view-mode="table"
                      aria-label="${escapeHtml(state.t('tableView', 'Tabelle'))}"
                      title="${escapeHtml(state.t('tableView', 'Tabelle'))}">
                ${iconSvg('table')}
              </button>
              <button type="button"
                      class="research-view-btn${state.sourcesViewMode === 'shards' ? ' is-active' : ''}"
                      data-action="sources-view"
                      data-view-mode="shards"
                      aria-label="${escapeHtml(state.t('shardsView', 'Karten'))}"
                      title="${escapeHtml(state.t('shardsView', 'Karten'))}">
                ${iconSvg('grid')}
              </button>
            </div>
          ` : ''}
        </div>
        <div class="research-table-host">
          ${renderActiveTable(task)}
        </div>
      </section>
    </div>
  `;
}

function renderNoTaskCenter() {
  const empty = emptyStateForNoTask();
  return `
    <header class="research-pane-header research-center-header">
      <div><span>${escapeHtml(state.t('webResearch', 'Web Research'))}</span><h2>${escapeHtml(state.t('evidenceWorkbench', 'Portfolio Map & Evidence Workbench'))}</h2></div>
      <div class="research-header-actions">
        <button type="button" class="research-icon-button" data-action="refresh" aria-label="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}" title="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}">${iconSvg('refresh')}</button>
        <button type="button" class="research-icon-button" data-action="new-task" aria-label="${escapeHtml(state.t('createResearch', 'Research anlegen'))}" title="${escapeHtml(state.t('createResearch', 'Research anlegen'))}">${iconSvg('plus')}</button>
      </div>
    </header>
    <div class="research-center-empty-body">
      <section class="research-empty-state research-empty-state-panel">
        <strong>${escapeHtml(empty.title)}</strong>
        <span>${escapeHtml(empty.body)}</span>
      </section>
      <section class="research-workbench research-empty-workbench" aria-label="${escapeHtml(state.t('sources', 'Sources'))}">
        <div class="research-tabs-container">
          <div class="research-tabs" role="tablist" aria-label="Research views">
            ${disabledTabButton('sources', state.t('sources', 'Sources'))}
            ${disabledTabButton('measurements', state.t('measurements', 'Measurements'))}
            ${disabledTabButton('knowledge', state.t('knowledge', 'Knowledge'))}
          </div>
        </div>
        <div class="research-empty-workbench-body">
          <label class="research-empty-search-row">
            <span>${escapeHtml(state.t('sourceSearch', 'Quellensuche'))}</span>
            <input type="text" disabled placeholder="${escapeHtml(state.t('searchSourcesPlaceholder', 'Quelle suchen: NASA, UIUC, Tyto, PX4, Vibration ...'))}" />
          </label>
          <p>${escapeHtml(state.t('noTaskControlsHint', 'Suche, Filter, Portfolio Map und Tabellen werden aktiv, sobald mindestens eine lokale Knowledge Domain mit Quellen geladen ist.'))}</p>
        </div>
      </section>
    </div>
  `;
}

function mapModeToggle() {
  return `
    <div class="research-map-mode" role="group" aria-label="Research map view">
      <button type="button" data-action="map-mode" data-map-mode="portfolio" aria-pressed="${state.mapMode !== 'discovery'}">${escapeHtml(state.t('map', 'Map'))}</button>
      <button type="button" data-action="map-mode" data-map-mode="discovery" aria-pressed="${state.mapMode === 'discovery'}">${escapeHtml(state.t('graph', 'Graph'))}</button>
    </div>
  `;
}

function renderMapPoint(source, xAxis, yAxis) {
  const jitter = pointJitter(source);
  const x = clampScore((source.dimensions[xAxis] ?? source.score) + jitter.x);
  const y = clampScore((source.dimensions[yAxis] ?? source.score) + jitter.y);
  const labelled = source.rank <= 2 || source.id === state.selectedSourceId;
  return `
    <button type="button" class="research-map-point research-point-${source.grade.toLowerCase()}${labelled ? ' is-labelled' : ' is-compact'}${source.id === state.selectedSourceId ? ' is-selected' : ''}"
      data-action="select-source"
      data-source-id="${escapeHtml(source.id)}"
      style="--x:${mapPercent(x)}%; --y:${100 - mapPercent(y)}%;"
      title="${escapeHtml(source.title)}">
      <span>${escapeHtml(shortLabel(source.title))}</span>
    </button>
  `;
}

function renderDiscoveryGraph(task) {
  const graph = discoveryGraph(task);
  return `
    <svg class="research-discovery-edges" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
      ${graph.edges.map((edge) => {
        const from = graph.nodeById.get(edge.from);
        const to = graph.nodeById.get(edge.to);
        if (!from || !to) return '';
        return `<line class="research-discovery-edge research-discovery-edge-${edge.kind}" x1="${from.x}" y1="${from.y}" x2="${to.x}" y2="${to.y}" />`;
      }).join('')}
    </svg>
    ${graph.nodes.map((node) => {
      const source = node.sourceId ? state.sourceModels.find((item) => item.id === node.sourceId) : null;
      const action = source ? 'data-action="select-source"' : '';
      const selected = source?.id === state.selectedSourceId;
      return `
        <button type="button" class="research-graph-node research-graph-node-${node.kind}${selected ? ' is-selected' : ''}"
          ${action}
          ${source ? `data-source-id="${escapeHtml(source.id)}"` : ''}
          style="--x:${node.x}%; --y:${node.y}%;"
          title="${escapeHtml(node.title)}">
          <span>${escapeHtml(node.label)}</span>
          ${node.meta ? `<small>${escapeHtml(node.meta)}</small>` : ''}
        </button>
      `;
    }).join('')}
  `;
}

function getSearchCluster(source) {
  const meta = DRONE_SOURCES_METADATA[source.id];
  const tags = meta?.tags || [];
  const text = [source.id, source.title, source.sourceClass, source.note, meta?.kind, ...(meta?.tags || [])].join(' ').toLowerCase();

  if (tags.includes('simulation') || /simulation|modell|gazebo|sih|virtuell|cfd|ansys|numerical/i.test(text)) {
    return 'simulation';
  }
  if (tags.includes('vibration') || tags.includes('fault') || /vibration|unwucht|schaden|fault|pitting|edm|abrasiv|sand/i.test(text)) {
    return 'vibration';
  }
  if (tags.includes('flightlog') || tags.includes('duty') || /flight|flug|telemetry|telemetrie|mission|ulog|blackbox/i.test(text)) {
    return 'flightlog';
  }
  if (tags.includes('bench') || tags.includes('motor') || /bench|pr\u00fcfstand|motor|esc|spindel|dynamometer|dyno|messstand|t-motor|kde|apc/i.test(text)) {
    return 'bench';
  }
  if (tags.includes('rotorload') || tags.includes('windtunnel') || /rotor|propeller|thrust|force|moment|aerodynamic|windtunnel|windkanal/i.test(text)) {
    return 'rotorload';
  }
  return 'rotorload';
}

function discoveryGraph(task) {
  const base = knowledgeBaseForTask(task);
  const nodes = [];
  const edges = [];
  const pushNode = (node) => {
    if (nodes.some((item) => item.id === node.id)) return;
    nodes.push(node);
  };
  const topSources = [];
  const cIds = ["rotorload", "bench", "flightlog", "vibration", "simulation"];
  cIds.forEach(cId => {
    const clusterSources = state.sourceModels
      .filter(s => getSearchCluster(s) === cId)
      .sort((a, b) => b.score - a.score)
      .slice(0, 8);
    topSources.push(...clusterSources);
  });
  
  // Group sources by cluster to prevent vertical overlaps and messy criss-cross lines!
  const sourcesByCluster = {
    rotorload: [],
    bench: [],
    flightlog: [],
    vibration: [],
    simulation: []
  };
  topSources.forEach(source => {
    const cluster = getSearchCluster(source);
    if (sourcesByCluster[cluster]) {
      sourcesByCluster[cluster].push(source);
    } else {
      sourcesByCluster.rotorload.push(source);
    }
  });

  const sourceLayout = new Map();
  const clusters = [
    { id: 'rotorload', y: 20 },
    { id: 'bench', y: 35 },
    { id: 'flightlog', y: 50 },
    { id: 'vibration', y: 65 },
    { id: 'simulation', y: 80 }
  ];

  clusters.forEach(c => {
    const list = sourcesByCluster[c.id];
    const len = list.length;
    list.forEach((source, index) => {
      // 3 columns: x = 52, 68, 84 to prevent horizontal overlaps!
      const col = index % 3;
      const row = Math.floor(index / 3);
      const totalRows = Math.ceil(len / 3);
      
      const x = 52 + col * 16;
      // Compact vertical row spacing (6%) centered around cluster's y coordinate
      const rowOffset = (row - (totalRows - 1) / 2) * 6;
      const y = c.y + rowOffset;
      
      sourceLayout.set(source.id, { x, y });
    });
  });
  pushNode({
    id: 'knowledge',
    kind: 'knowledge',
    label: base?.title || task.title,
    title: task.knowledge_domain || task.title,
    meta: `${base?.tables?.length || 0} Tabellen`,
    x: 14,
    y: 50,
  });

  const searchClusters = [
    { id: 'rotorload', label: 'Rotorlasten & Aerodynamik', y: 20 },
    { id: 'bench', label: 'Prüfstand & Motoren', y: 35 },
    { id: 'flightlog', label: 'Fluglogs & Lastprofile', y: 50 },
    { id: 'vibration', label: 'Vibration & Defekte', y: 65 },
    { id: 'simulation', label: 'Simulation & Modelle', y: 80 }
  ];

  searchClusters.forEach((cluster) => {
    const clusterSources = topSources.filter((source) => getSearchCluster(source) === cluster.id);
    if (clusterSources.length > 0) {
      pushNode({
        id: `cluster_${cluster.id}`,
        kind: 'class',
        label: cluster.label,
        title: cluster.label,
        meta: `${clusterSources.length} Quellen`,
        x: 36,
        y: cluster.y
      });
      edges.push({ from: 'knowledge', to: `cluster_${cluster.id}`, kind: 'class' });
    }
  });

  topSources.forEach((source, index) => {
    const clusterId = getSearchCluster(source);
    const layout = sourceLayout.get(source.id) || { x: 72, y: 50 };
    const id = `source_${source.id}`;
    pushNode({
      id,
      kind: source.grade.toLowerCase() === 'a' ? 'source-strong' : 'source',
      label: shortLabel(source.title),
      title: source.title,
      meta: `${source.grade} · ${(source.score / 10).toFixed(1)}`,
      sourceId: source.id,
      x: clampNumber(layout.x, 58, 84),
      y: clampNumber(layout.y, 12, 88),
    });
    edges.push({ from: `cluster_${clusterId}`, to: id, kind: 'source' });
    if (source.measurements?.count && index < 5) {
      const measureId = `measurement_${source.id}`;
      pushNode({
        id: measureId,
        kind: 'measurement',
        label: `${source.measurements.count} Messpunkte`,
        title: `${source.title}: ${source.measurements.count} Messpunkte`,
        meta: source.measurements.maxAxial ? `${formatNumber(source.measurements.maxAxial)} N axial` : '',
        x: 92,
        y: clampNumber(layout.y + 3, 14, 90),
      });
      edges.push({ from: id, to: measureId, kind: 'measurement' });
    }
  });
  return { nodes, edges, nodeById: new Map(nodes.map((node) => [node.id, node])) };
}

function mapPercent(score) {
  return 12 + (clampScore(score) * 0.76);
}

function mapTransformStyle() {
  const scale = clampNumber(state.map.scale || 1, 0.6, 2.6);
  const panX = Math.round(Number(state.map.panX) || 0);
  const panY = Math.round(Number(state.map.panY) || 0);
  return `transform: translate(${panX}px, ${panY}px) scale(${scale});`;
}

function handleMapWheel(event) {
  const map = event.target.closest?.('.research-portfolio-map');
  if (!map || !state.ctx.host.contains(map)) return;
  if (event.target.closest('select, input, textarea, a')) return;
  event.preventDefault();
  const oldScale = clampNumber(state.map.scale || 1, 0.6, 2.6);
  const nextScale = clampNumber(oldScale * (event.deltaY > 0 ? 0.9 : 1.1), 0.6, 2.6);
  const rect = map.getBoundingClientRect();
  const originX = event.clientX - rect.left - rect.width / 2;
  const originY = event.clientY - rect.top - rect.height / 2;
  const ratio = nextScale / oldScale;
  state.map.panX = originX - (originX - state.map.panX) * ratio;
  state.map.panY = originY - (originY - state.map.panY) * ratio;
  state.map.scale = nextScale;
  updateMapTransform();
}

function handleMapPointerDown(event) {
  const map = event.target.closest?.('.research-portfolio-map');
  if (!map || !state.ctx.host.contains(map)) return;
  if (event.target.closest('select, button, input, textarea, a, label')) return;
  state.map.drag = {
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    panX: Number(state.map.panX) || 0,
    panY: Number(state.map.panY) || 0,
  };
  map.setPointerCapture?.(event.pointerId);
  map.classList.add('is-panning');
  event.preventDefault();
}

function handleMapPointerMove(event) {
  const drag = state.map.drag;
  if (!drag || drag.pointerId !== event.pointerId) return;
  state.map.panX = drag.panX + event.clientX - drag.startX;
  state.map.panY = drag.panY + event.clientY - drag.startY;
  updateMapTransform();
}

function stopMapDrag(event) {
  const drag = state.map.drag;
  if (!drag || (event?.pointerId !== undefined && drag.pointerId !== event.pointerId)) return;
  const map = state.ctx.host.querySelector('.research-portfolio-map');
  map?.classList.remove('is-panning');
  state.map.drag = null;
}

function resetMapView() {
  state.map.scale = 1;
  state.map.panX = 0;
  state.map.panY = 0;
  updateMapTransform();
}

function updateMapTransform() {
  const content = state.ctx.host.querySelector('[data-map-content]');
  if (content) content.style.transform = mapTransformStyle().replace('transform: ', '').replace(/;$/, '');
}

function renderActiveTable(task) {
  if (state.activeTab === 'measurements') return renderMeasurementsTable();
  if (state.activeTab === 'knowledge') return renderKnowledgeTables(task);
  if (state.activeTab === 'reports') return renderReportsWorkbench(task);
  return renderSourcesWorkbench();
}

function renderSourcesTable(filteredList = state.sourceModels) {
  const task = selectedTask();
  const axisPair = normalizedAxisPair(task);
  const xAxis = axisPair.x;
  const yAxis = axisPair.y;
  return `
    <table class="research-data-table" style="table-layout: fixed; width: 100%;">
      <colgroup>
        <col style="width: 48%;" />
        <col style="width: 14%;" />
        <col style="width: 14%;" />
        <col style="width: 8%;" />
        <col style="width: 8%;" />
        <col style="width: 8%;" />
      </colgroup>
      <thead>
        <tr>
          <th>${escapeHtml(state.t('sourceLabel', 'Source'))}</th>
          <th>${escapeHtml(state.t('classLabel', 'Class'))}</th>
          <th style="text-align: right;">${escapeHtml(state.t('scoreLabel', 'Score'))}</th>
          <th style="text-align: right;">${escapeHtml(axisLabel(yAxis, task))}</th>
          <th style="text-align: right;">${escapeHtml(axisLabel(xAxis, task))}</th>
          <th style="text-align: right;"></th>
        </tr>
      </thead>
      <tbody>
        ${filteredList.map((source) => `
          <tr class="${source.id === state.selectedSourceId ? 'is-selected' : ''}">
            <td><button type="button" data-action="select-source" data-source-id="${escapeHtml(source.id)}"><strong>${escapeHtml(source.title)}</strong><span>${escapeHtml(source.id)}</span></button></td>
            <td>${escapeHtml(source.sourceClass)}</td>
            <td style="text-align: right;"><span class="research-score-pill research-grade-${source.grade.toLowerCase()}">${source.grade} · ${(source.score / 10).toFixed(1)}</span></td>
            <td style="text-align: right;">${Math.round(source.dimensions[yAxis] ?? 0)}</td>
            <td style="text-align: right;">${Math.round(source.dimensions[xAxis] ?? 0)}</td>
            <td style="text-align: right;">${source.url ? `<a href="${escapeHtml(source.url)}" target="_blank" rel="noreferrer">${escapeHtml(state.t('openLabel', 'Open'))}</a>` : ''}</td>
          </tr>
        `).join('') || `<tr><td colspan="6">${escapeHtml(state.t('noSources', 'Keine Quellen vorhanden.'))}</td></tr>`}
      </tbody>
    </table>
  `;
}

function renderSourcesWorkbench() {
  const activeTag = state.sourceActiveTag || 'all';
  const subthemes = [
    { id: 'all', label: state.t('subthemeAll', 'Alle') },
    { id: 'rotorload', label: state.t('subthemeRotorload', 'Rotorlast') },
    { id: 'bench', label: state.t('subthemeBench', 'Prüfstand') },
    { id: 'flightlog', label: state.t('subthemeFlightlog', 'Fluglog') },
    { id: 'vibration', label: state.t('subthemeVibration', 'Vibration') },
    { id: 'simulation', label: state.t('subthemeSimulation', 'Simulation') }
  ];

  const filtered = filteredSources();

  return `
    <div class="research-sources-shards-wrapper">
      <div class="research-sources-shards-toolbar">
        <input type="text"
               class="research-sources-shards-search"
               id="research-source-search-input"
               data-action="source-search"
               placeholder="${escapeHtml(state.t('searchSourcesPlaceholder', 'Quelle suchen: NASA, UIUC, Tyto, PX4, Vibration ...'))}"
               value="${escapeHtml(state.sourceSearchTerm || '')}"
               autocomplete="off" />
        <div class="research-sources-shards-filters">
          ${subthemes.map((theme) => `
            <button type="button"
                    class="research-tag-pill${activeTag === theme.id ? ' is-active' : ''}"
                    data-action="source-tag-filter"
                    data-tag-id="${theme.id}">
              ${escapeHtml(theme.label)}
            </button>
          `).join('')}
        </div>
      </div>
      <div class="research-sources-shards-scroll">
        ${state.sourcesViewMode === 'shards' ? `
          <div class="research-sources-shards-grid">
            ${filtered.map(renderSourceCard).join('') || `
              <div class="research-empty" style="grid-column: 1 / -1; padding: 40px; text-align: center; color: var(--research-muted);">
                ${escapeHtml(state.t('noSources', 'Keine Quellen vorhanden.'))}
              </div>
            `}
          </div>
        ` : renderSourcesTable(filtered)}
      </div>
    </div>
  `;
}

function filteredSources() {
  const activeTag = state.sourceActiveTag || 'all';
  const searchTerm = (state.sourceSearchTerm || '').trim().toLowerCase();

  return state.sourceModels.filter((source) => {
    if (activeTag !== 'all') {
      const meta = DRONE_SOURCES_METADATA[source.id];
      const tags = meta?.tags || [];
      if (!tags.includes(activeTag)) return false;
    }
    if (searchTerm) {
      const titleMatch = (source.title || '').toLowerCase().includes(searchTerm);
      const idMatch = (source.id || '').toLowerCase().includes(searchTerm);
      const classMatch = (source.sourceClass || '').toLowerCase().includes(searchTerm);

      const meta = DRONE_SOURCES_METADATA[source.id];
      const kindMatch = meta?.kind ? meta.kind.toLowerCase().includes(searchTerm) : false;
      const fieldsMatch = meta?.fields ? meta.fields.toLowerCase().includes(searchTerm) : false;
      const useMatch = meta?.use ? meta.use.toLowerCase().includes(searchTerm) : false;
      const missingMatch = meta?.missing ? meta.missing.toLowerCase().includes(searchTerm) : false;
      const tagMatch = meta?.tags ? meta.tags.some(t => t.toLowerCase().includes(searchTerm)) : false;

      if (!titleMatch && !idMatch && !classMatch && !kindMatch && !fieldsMatch && !useMatch && !missingMatch && !tagMatch) {
        return false;
      }
    }
    return true;
  });
}

function renderSourceCard(source) {
  const isSelected = source.id === state.selectedSourceId;
  const meta = DRONE_SOURCES_METADATA[source.id];

  const kind = meta?.kind || source.sourceClass || 'Quelle';
  const tags = meta?.tags || [source.sourceClass.toLowerCase()];
  const fields = meta?.fields || source.summary || state.t('noSummaryAvailable', 'Keine Zusammenfassung.');
  const use = meta?.use || 'Verfügbare quantitative Datenpunkte für das Dashboard.';
  const missing = meta?.missing || 'Nicht separat aufbereitete Lückenanalyse.';

  return `
    <div class="research-source-card${isSelected ? ' is-selected' : ''}"
         data-action="select-source"
         data-source-id="${escapeHtml(source.id)}">
      <div class="research-source-card-top">
        <div>
          <h3 class="research-source-card-title">${escapeHtml(source.title)}</h3>
          <div class="research-source-card-subtitle">${escapeHtml(kind)}</div>
        </div>
        <span class="research-source-card-badge ${source.grade.toLowerCase()}">
          ${escapeHtml(gradeFullText(source.grade))}
        </span>
      </div>
      <div class="research-source-card-chips">
        ${tags.map(tag => `<span class="research-source-card-chip">${escapeHtml(tag)}</span>`).join('')}
      </div>
      <div class="research-source-card-kv">
        <span class="k">Daten</span>
        <span class="v">${escapeHtml(fields)}</span>
        <span class="k">Nutzen</span>
        <span class="v">${escapeHtml(use)}</span>
        <span class="k">Lücke</span>
        <span class="v">${escapeHtml(missing)}</span>
      </div>
      ${meta?.links && meta.links.length > 0 ? `
        <div class="research-source-card-actions">
          ${meta.links.map((link, idx) => `
            <a href="${escapeHtml(link[1])}"
               class="research-source-card-btn${idx === 0 ? ' primary' : ''}"
               target="_blank"
               rel="noreferrer"
               onclick="event.stopPropagation();">
              ${escapeHtml(link[0])}
            </a>
          `).join('')}
        </div>
      ` : source.url ? `
        <div class="research-source-card-actions">
          <a href="${escapeHtml(source.url)}"
             class="research-source-card-btn primary"
             target="_blank"
             rel="noreferrer"
             onclick="event.stopPropagation();">
            ${escapeHtml(state.t('openLabel', 'Öffnen'))}
          </a>
        </div>
      ` : ''}
    </div>
  `;
}

function gradeFullText(grade) {
  const g = String(grade || '').toUpperCase();
  if (g === 'A') return 'A · Ausgezeichnet';
  if (g === 'B') return 'B · Gut';
  if (g === 'C') return 'C · Ergänzend';
  if (g === 'D') return 'D · Risiko';
  return g;
}

function renderMeasurementsTable() {
  return `
    <table class="research-data-table" style="table-layout: fixed; width: 100%;">
      <colgroup>
        <col style="width: 25%;" />
        <col style="width: 15%;" />
        <col style="width: 15%;" />
        <col style="width: 15%;" />
        <col style="width: 15%;" />
        <col style="width: 15%;" />
      </colgroup>
      <thead>
        <tr>
          <th>${escapeHtml(state.t('sourceLabel', 'Source'))}</th>
          <th>Prop</th>
          <th style="text-align: right;">RPM</th>
          <th style="text-align: right;">Axial N</th>
          <th style="text-align: right;">Radial N</th>
          <th>Method</th>
        </tr>
      </thead>
      <tbody>
        ${state.measurementRows.slice(0, 120).map((row) => `
          <tr>
            <td>${escapeHtml(row.source_id || '')}</td>
            <td>${escapeHtml([row.prop_diameter_in, row.prop_pitch_in].filter(isPresent).join(' x '))}</td>
            <td style="text-align: right;">${formatNumber(row.rpm)}</td>
            <td style="text-align: right;">${formatNumber(row.axial_load_N ?? row.thrust_N)}</td>
            <td style="text-align: right;">${formatNumber(row.radial_load_N)}</td>
            <td>${escapeHtml(firstString(row, ['confidence', 'derivation_method']).slice(0, 90))}</td>
          </tr>
        `).join('') || `<tr><td colspan="6">${escapeHtml(state.t('noMeasurements', 'Keine Messpunkte vorhanden.'))}</td></tr>`}
      </tbody>
    </table>
  `;
}

function renderKnowledgeTables(task) {
  const base = knowledgeBaseForTask(task);
  return `
    <div class="research-knowledge-list">
      ${(base?.tables || []).map((table) => `
        <button type="button" data-action="open-knowledge" data-table-id="${escapeHtml(table.id)}">
          <strong>${escapeHtml(table.title || table.table_key)}</strong>
          <span>${escapeHtml(table.table_key)} · ${Number(table.row_count || 0).toLocaleString(state.lang === 'de' ? 'de-DE' : 'en-US')} ${escapeHtml(state.t('rows', 'rows'))}</span>
        </button>
      `).join('') || `<div class="research-empty">${escapeHtml(state.t('noKnowledgeConnected', 'Keine Knowledge-Tabellen verknüpft.'))}</div>`}
    </div>
  `;
}

function renderRight() {
  const root = pane('right');
  if (!root) return;
  const task = selectedTask();
  const source = selectedSource();
  const latestRun = latestRunForTask(task?.id);
  const runInfo = researchRunInfo(task);
  const notes = computedDecisionNotes(source);
  const axisPair = normalizedAxisPair(task);
  const canRun = canRunResearchTask(task);
  root.innerHTML = `
    <header class="research-pane-header">
      <div><span>${escapeHtml(state.t('context', 'Context'))}</span><h2>${escapeHtml(task?.title || 'Research')}</h2></div>
      <button type="button" class="research-button primary" data-action="run-research" ${canRun ? '' : 'disabled aria-disabled="true"'} title="${escapeHtml(runResearchHint(task, runInfo))}">${runInfo.hasRun ? escapeHtml(state.t('researchFortsetzen', 'Research fortsetzen')) : escapeHtml(state.t('researchStarten', 'Research starten'))}</button>
    </header>
    <div class="research-right-scroll">
      <section class="research-context-block">
        <span class="research-kicker">Knowledge Base</span>
        <strong>${escapeHtml(task?.knowledge_domain || state.t('noDomain', 'Keine Domain'))}</strong>
        <p>${escapeHtml(task?.prompt || state.t('defaultTaskDesc', 'Research-Dashboard auf Basis einer vorhandenen Knowledge Base.'))}</p>
        ${task?.criteria ? `<small>${escapeHtml(task.criteria)}</small>` : ''}
        ${task ? `<button type="button" class="research-button" data-action="edit-task">${escapeHtml(state.t('editScoring', 'Scoring bearbeiten'))}</button>` : ''}
      </section>
      ${renderScoringModel(task)}
      <section class="research-metric-grid">
        <div><strong>${state.sourceModels.length}</strong><span>${escapeHtml(state.t('sources', 'Sources'))}</span></div>
        <div><strong>${state.measurementRows.length}</strong><span>${escapeHtml(state.t('measurements', 'Measurements'))}</span></div>
        <div><strong>${avgScore()}</strong><span>${escapeHtml(state.t('avgScore', 'Avg score'))}</span></div>
        <div><strong>${runInfo.status || latestRun?.status || task?.status || 'ready'}</strong><span>${escapeHtml(state.t('status', 'Status'))}</span></div>
      </section>
      ${renderRunPanel(runInfo)}
      <section class="research-context-block">
        <span class="research-kicker">${escapeHtml(state.t('selectedSource', 'Selected Source'))}</span>
        ${source ? `
          <strong style="font-size: 13px; display: block; margin-bottom: 8px; color: var(--research-text);">${escapeHtml(source.title)}</strong>
          <p style="font-size: 11.5px; line-height: 1.4; color: var(--research-muted); margin-bottom: 12px;">${escapeHtml(source.note || state.t('noSummaryAvailable', 'Keine Zusammenfassung vorhanden.'))}</p>
          
          <div class="research-metric-profile" style="margin-bottom: 16px; display: flex; flex-direction: column; gap: 8px;">
            <!-- Overall Score Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Overall Score</span>
                <span>${(source.score / 10).toFixed(1)}%</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.score / 10 > 75 ? 'good' : source.score / 10 > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${(source.score / 10).toFixed(1)}%;"></div>
              </div>
            </div>
            
            <!-- Source Quality Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Source Quality</span>
                <span>${Math.round(source.dimensions.source_quality || 0)}%</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.dimensions.source_quality > 75 ? 'good' : source.dimensions.source_quality > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${Math.round(source.dimensions.source_quality || 0)}%;"></div>
              </div>
            </div>
            
            <!-- Evidence Strength Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Evidence Strength</span>
                <span>${Math.round(source.dimensions.evidence_strength || 0)}%</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.dimensions.evidence_strength > 75 ? 'good' : source.dimensions.evidence_strength > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${Math.round(source.dimensions.evidence_strength || 0)}%;"></div>
              </div>
            </div>
            
            <!-- Topic Fit Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Topic Fit</span>
                <span>${Math.round(source.dimensions.topic_fit || 0)}%</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.dimensions.topic_fit > 75 ? 'good' : source.dimensions.topic_fit > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${Math.round(source.dimensions.topic_fit || 0)}%;"></div>
              </div>
            </div>
            
            <!-- Actionability Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Actionability</span>
                <span>${Math.round(source.dimensions.actionability || 0)}%</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.dimensions.actionability > 75 ? 'good' : source.dimensions.actionability > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${Math.round(source.dimensions.actionability || 0)}%;"></div>
              </div>
            </div>
          </div>
          
          <button type="button" class="research-button" data-action="source-detail" data-source-id="${escapeHtml(source.id)}" style="width: 100%; text-align: center;">${escapeHtml(state.t('details', 'Details'))}</button>
        ` : `<p>${escapeHtml(state.t('selectSourcePrompt', 'Wähle eine Quelle aus.'))}</p>`}
      </section>
      <section class="research-context-block">
        <div class="research-section-head flush"><strong>${escapeHtml(state.t('decisionNotes', 'Decision notes'))}</strong><span>${escapeHtml(state.t('auto', 'auto'))}</span></div>
        <div class="research-note-stack">
          ${notes.map((note) => `<div class="research-note research-note-${note.kind}"><strong>${escapeHtml(note.title)}</strong><span>${escapeHtml(note.body)}</span></div>`).join('')}
        </div>
      </section>
    </div>
  `;
}

function renderRunPanel(runInfo) {
  return `
    <section class="research-run-panel">
      <div class="research-section-head flush">
        <strong>${escapeHtml(state.t('researchRun', 'Research Run'))}</strong>
        <span>${escapeHtml(runInfo.updatedLabel || state.t('noActiveRun', 'kein Lauf'))}</span>
      </div>
      ${runInfo.run || runInfo.command || runInfo.queueTask ? `
        <div class="research-run-state research-run-${escapeHtml(runInfo.statusKind)}">
          <span></span>
          <div>
            <strong>${escapeHtml(runInfo.statusLabel)}</strong>
            <small>${escapeHtml(runInfo.title || runInfo.commandType || 'Systematic Research')}</small>
          </div>
        </div>
        <dl class="research-run-facts">
          <div><dt>${escapeHtml(state.t('command', 'Command'))}</dt><dd>${escapeHtml(shortId(runInfo.commandId))}</dd></div>
          <div><dt>${escapeHtml(state.t('queue', 'Queue'))}</dt><dd>${escapeHtml(shortId(runInfo.taskQueueId))}</dd></div>
          <div><dt>${escapeHtml(state.t('thread', 'Thread'))}</dt><dd>${escapeHtml(runInfo.threadKey || '-')}</dd></div>
        </dl>
        <div class="research-run-actions">
          <button type="button" class="research-button" data-action="focus-ctox-run" data-command-id="${escapeHtml(runInfo.commandId)}" data-task-queue-id="${escapeHtml(runInfo.taskQueueId)}" ${runInfo.taskQueueId || runInfo.commandId ? '' : 'disabled'}>${escapeHtml(state.t('viewInCtox', 'In CTOX ansehen'))}</button>
        </div>
      ` : `
        <p>${escapeHtml(state.t('noRunStarted', 'Kein Research-Lauf für dieses Dashboard gestartet.'))}</p>
      `}
    </section>
  `;
}

function computedDecisionNotes(source) {
  const top = state.sourceModels[0];
  const notes = [];
  if (top) {
    notes.push({ kind: 'opportunity', title: state.t('decisionNoteEv1', 'Use strongest evidence first'), body: state.t('decisionNoteEv1Body', `${top.title} ist aktuell der stärkste Dashboard-Anker.`, top.title) });
  }
  if (state.measurementRows.length) {
    notes.push({ kind: 'opportunity', title: state.t('decisionNoteQuant', 'Quantitative evidence available'), body: state.t('decisionNoteQuantBody', `${state.measurementRows.length} Messpunkte können in die aktiven Scoring-Kriterien einfließen.`, state.measurementRows.length) });
  }
  if (source && source.dimensions.reuse_readiness < 60) {
    notes.push({ kind: 'risk', title: state.t('decisionNoteGap', 'Reuse gap'), body: state.t('decisionNoteGapBody', 'Diese Quelle braucht weitere Extraktion, bevor sie als belastbare Dashboard-Kennzahl dient.') });
  }
  if (!notes.some((note) => note.kind === 'risk')) {
    notes.push({ kind: 'risk', title: state.t('decisionNoteScope', 'Scope control'), body: state.t('decisionNoteScopeBody', 'Dashboard-Scores bleiben nur so belastbar wie die verknüpften Knowledge-Tabellen und deren Provenance.') });
  }
  return notes;
}

function renderScoringModel(task) {
  if (!task) return '';
  const axes = scoringDimensionsForTask(task).filter((axis) => axis.id !== 'portfolio_priority');
  const pair = normalizedAxisPair(task);
  return `
    <section class="research-context-block">
      <div class="research-section-head flush"><strong>${escapeHtml(state.t('scoringModel', 'Scoring model'))}</strong><span>${axes.length} ${escapeHtml(state.t('kriterien', 'Kriterien'))}</span></div>
      <div class="research-scoring-list">
        ${axes.map((axis) => `
          <div class="${axis.id === pair.x || axis.id === pair.y ? 'is-active' : ''}">
            <strong>${escapeHtml(axis.label)}</strong>
            <span>${axis.id === pair.x ? escapeHtml(state.t('xAxis', 'X axis')) : axis.id === pair.y ? escapeHtml(state.t('yAxis', 'Y axis')) : escapeHtml(state.t('score', 'score'))}</span>
          </div>
        `).join('')}
      </div>
    </section>
  `;
}

function canRunResearchTask(task) {
  return validateSelectedResearchTask(task, state.knowledgeBases).valid;
}

function validateSelectedResearchTask(task, knowledgeBases = []) {
  if (!task?.id) return { valid: false, message: state.t('selectTaskFirst', 'Wähle zuerst eine Research-Aufgabe.') };
  if (!String(task.title || '').trim()) return { valid: false, message: state.t('missingTaskTitle', 'Die Research-Aufgabe hat keinen Titel.') };
  const domain = String(task.knowledge_domain || '').trim();
  if (!domain) return { valid: false, message: state.t('missingDomain', 'Die Research-Aufgabe hat keine Knowledge Domain.') };
  if (!knowledgeBases.some((base) => base.domain === domain)) {
    return { valid: false, message: state.t('domainNotLoaded', 'Die Knowledge Domain ist lokal nicht geladen.') };
  }
  return { valid: true, message: '' };
}

function runResearchHint(task, runInfo) {
  const validation = validateSelectedResearchTask(task, state.knowledgeBases);
  if (!validation.valid) return validation.message;
  return `${state.t('runHint', 'Systematic Research für dieses Dashboard')} ${runInfo.hasRun ? state.t('researchFortsetzen', 'fortsetzen') : state.t('researchStarten', 'starten')}`;
}

function runDisabledReason(task) {
  return validateSelectedResearchTask(task, state.knowledgeBases).message;
}

function validateResearchTaskInput(values, knowledgeBases = [], { isEdit = false } = {}) {
  const title = String(values?.title || '').trim();
  const domain = String(values?.domain || '').trim();
  const prompt = String(values?.prompt || '').trim();
  if (!title) return { valid: false, field: 'title', message: 'Titel ist erforderlich.' };
  if (!domain) return { valid: false, field: 'domain', message: 'Knowledge Domain ist erforderlich.' };
  if (!isEdit && !knowledgeBases.some((base) => base.domain === domain)) {
    return { valid: false, field: 'domain', message: 'Wähle eine lokal verfügbare Knowledge Domain.' };
  }
  if (!prompt) return { valid: false, field: 'prompt', message: 'Auftrag ist erforderlich.' };
  return { valid: true, field: '', message: '' };
}

function formValues(form) {
  const data = new FormData(form);
  return {
    title: data.get('title'),
    domain: data.get('domain'),
    prompt: data.get('prompt'),
  };
}

function domainSelectionNote(isEdit) {
  if (isEdit) return state.t('domainLockedEdit', 'Domain bleibt beim Bearbeiten an die bestehende Research-Aufgabe gebunden.');
  if (!state.knowledgeBases.length) return state.t('noLocalDomainsNote', 'Noch keine Knowledge Base verfügbar.');
  return state.t('localDomainsNote', `${state.knowledgeBases.length} lokale Knowledge Domains verfügbar.`, state.knowledgeBases.length);
}

function openTaskDialog(editTask = null) {
  closeTaskDialog();
  const root = state.ctx.host.querySelector('[data-research-root]');
  if (!root) return;
  const isEdit = Boolean(editTask?.id);
  const selectedDomain = editTask?.knowledge_domain || selectedTask()?.knowledge_domain || state.knowledgeBases[0]?.domain || '';
  const dimensionsText = formatDimensionLines(scoringDimensionsForTask(editTask));
  const domainOptions = state.knowledgeBases.map((base) => `
    <option value="${escapeHtml(base.domain)}" ${base.domain === selectedDomain ? 'selected' : ''}>
      ${escapeHtml(`${base.title || titleFromDomain(base.domain)} · ${base.domain}`)}
    </option>
  `).join('');
  const overlay = document.createElement('div');
  overlay.className = 'research-modal-backdrop';
  overlay.innerHTML = `
    <section class="research-modal" role="dialog" aria-modal="true" aria-labelledby="research-create-title">
      <header>
        <div>
          <span>${escapeHtml(state.t('webResearch', 'Web Research'))}</span>
          <strong id="research-create-title">${isEdit ? escapeHtml(state.t('editScoring', 'Scoring bearbeiten')) : escapeHtml(state.t('dashboardAnlegen', 'Dashboard anlegen'))}</strong>
        </div>
        <button type="button" data-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
      </header>
      <form data-research-task-form>
        <input type="hidden" name="task_id" value="${escapeHtml(editTask?.id || '')}">
        ${isEdit ? `<input type="hidden" name="domain" value="${escapeHtml(selectedDomain)}">` : ''}
        <label><span>${escapeHtml(state.t('titel', 'Titel'))}</span><input name="title" placeholder="${escapeHtml(state.t('neueResearch', 'Neue Research'))}" value="${escapeHtml(editTask?.title || '')}" required></label>
        <label>
          <span>Knowledge Domain</span>
          <select name="${isEdit ? 'domain_display' : 'domain'}" ${isEdit || !state.knowledgeBases.length ? 'disabled' : ''} required>
            <option value="" ${selectedDomain ? '' : 'selected'} disabled>${escapeHtml(state.t('selectKnowledgeDomain', 'Knowledge Domain auswählen'))}</option>
            ${domainOptions}
          </select>
          <small class="research-field-note">${escapeHtml(domainSelectionNote(isEdit))}</small>
        </label>
        <label><span>${escapeHtml(state.t('auftrag', 'Auftrag'))}</span><textarea name="prompt" placeholder="${escapeHtml(state.t('promptPlaceholder', 'Was soll das Dashboard auswerten?'))}" required>${escapeHtml(editTask?.prompt || '')}</textarea></label>
        <label><span>${escapeHtml(state.t('kriterien', 'Kriterien'))}</span><textarea name="criteria" placeholder="${escapeHtml(state.t('criteriaPlaceholder', 'Scope, Ausschlüsse, Scoring-Hinweise'))}">${escapeHtml(editTask?.criteria || '')}</textarea></label>
        <label><span>${escapeHtml(state.t('scoringDimensions', 'Scoring Dimensionen'))}</span><textarea name="scoring_dimensions" placeholder="${escapeHtml(state.t('scoringPlaceholder', 'overlap: Overlap\nbuyer_clarity: Buyer clarity'))}">${escapeHtml(dimensionsText)}</textarea></label>
        <p class="research-validation" data-validation-status aria-live="polite"></p>
        <footer>
          <button type="button" class="research-button" data-close>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
          <button type="submit" class="research-button primary" disabled>${isEdit ? escapeHtml(state.t('save', 'Speichern')) : escapeHtml(state.t('create', 'Anlegen'))}</button>
        </footer>
      </form>
    </section>
  `;
  const close = () => overlay.remove();
  overlay.addEventListener('click', (event) => {
    if (event.target === overlay || event.target.closest('[data-close]')) close();
  });
  const formEl = overlay.querySelector('[data-research-task-form]');
  const syncFormState = () => {
    const submit = formEl?.querySelector('button[type="submit"]');
    const status = formEl?.querySelector('[data-validation-status]');
    if (!submit || !status || !formEl) return;
    const validation = validateResearchTaskInput(formValues(formEl), state.knowledgeBases, { isEdit });
    submit.disabled = !validation.valid;
    status.textContent = validation.valid ? '' : validation.message;
  };
  formEl?.addEventListener('input', syncFormState);
  formEl?.addEventListener('change', syncFormState);
  formEl?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const submit = event.currentTarget.querySelector('button[type="submit"]');
    const status = event.currentTarget.querySelector('[data-validation-status]');
    const validation = validateResearchTaskInput(formValues(event.currentTarget), state.knowledgeBases, { isEdit });
    if (!validation.valid) {
      if (status) status.textContent = validation.message;
      submit.disabled = true;
      return;
    }
    submit.disabled = true;
    const form = new FormData(event.currentTarget);
    try {
      await createTaskFromForm(form);
      close();
    } catch (error) {
      if (status) status.textContent = errorMessage(error);
      submit.disabled = false;
    }
  });
  root.append(overlay);
  syncFormState();
  requestAnimationFrame(() => overlay.querySelector('input[name="title"]')?.focus());
}

function closeTaskDialog() {
  state.ctx.host.querySelector('.research-modal-backdrop')?.remove();
}

async function createTaskFromForm(form) {
  const taskId = String(form.get('task_id') || '').trim();
  const current = taskId ? state.tasks.find((item) => item.id === taskId) : null;
  const validation = validateResearchTaskInput({
    title: String(form.get('title') || ''),
    domain: String(form.get('domain') || current?.knowledge_domain || ''),
    prompt: String(form.get('prompt') || ''),
  }, state.knowledgeBases, { isEdit: Boolean(current) });
  if (!validation.valid) throw new Error(validation.message);
  const rawDomain = String(form.get('domain') || current?.knowledge_domain || '').trim();
  const rawTitle = String(form.get('title') || '').trim();
  const domain = normalizeResearchDomain(rawDomain || rawTitle || current?.title || 'research');
  const base = state.knowledgeBases.find((item) => item.domain === domain);
  const now = Date.now();
  const title = String(rawTitle || base?.title || titleFromDomain(domain) || 'Research').trim();
  const prompt = String(form.get('prompt') || defaultPromptForKnowledgeBase(base)).trim();
  const criteria = String(form.get('criteria') || '').trim();
  const scoringDimensions = parseDimensionLines(String(form.get('scoring_dimensions') || ''))
    || inferScoringDimensions({ knowledge_domain: domain, title, prompt, criteria });
  const axisPair = defaultAxisPairForTask({ knowledge_domain: domain, domain, title, prompt, criteria, payload: { scoring_dimensions: scoringDimensions } });
  const task = {
    ...(current || {}),
    id: current?.id || `research_${slugId(title)}_${now}`,
    title,
    prompt,
    criteria,
    status: current?.status || 'ready',
    knowledge_domain: domain,
    source_catalog_key: current?.source_catalog_key || tableKey(base, ['source_catalog', 'sources', 'curated_sources']) || 'source_catalog',
    curated_table_key: current?.curated_table_key || tableKey(base, ['evaluation_matrix', 'load_data_library', 'curated_sources', 'source_library']) || 'evaluation_matrix',
    measurements_table_key: current?.measurements_table_key || tableKey(base, ['evidence_points', 'measured_load_points', 'measurements']) || 'evidence_points',
    x_axis: safeAxis(current?.x_axis || axisPair.x, { payload: { scoring_dimensions: scoringDimensions } }, axisPair.x),
    y_axis: safeAxis(current?.y_axis || axisPair.y, { payload: { scoring_dimensions: scoringDimensions } }, axisPair.y),
    payload: {
      ...(current?.payload || {}),
      user_created: current?.payload?.user_created ?? true,
      scoring_dimensions: scoringDimensions,
      scoring_weights: scoringWeights(scoringDimensions),
      table_contract: RESEARCH_TABLE_CONTRACT,
    },
    created_at_ms: current?.created_at_ms || now,
    updated_at_ms: now,
  };
  await upsertDoc(state.ctx.db.research_tasks, task);
  await loadLocalState();
  state.selectedTaskId = task.id;
  await loadDashboardData();
  render();
}

async function runSelectedResearch() {
  const task = selectedTask();
  if (!canRunResearchTask(task)) {
    setStatus(runDisabledReason(task));
    renderRight();
    return;
  }
  const base = knowledgeBaseForTask(task);
  const now = Date.now();
  const scoringDimensions = scoringDimensionsForTask(task).filter((axis) => axis.id !== 'portfolio_priority');
  const tableContract = task.payload?.table_contract || RESEARCH_TABLE_CONTRACT;
  const existingTables = new Set((base?.tables || []).map((table) => table.table_key));
  const missingTables = Object.keys(tableContract).filter((key) => !existingTables.has(key));
  const instruction = [
    `Führe systematic-research für das Business-OS Web Research Dashboard "${task.title}" fort.`,
    `Research Task ID: ${task.id}`,
    `Knowledge domain: ${task.knowledge_domain}`,
    `Source catalog: ctox knowledge data describe --domain ${task.knowledge_domain} --key ${task.source_catalog_key || 'source_catalog'}`,
    `Evaluation matrix: ctox knowledge data describe --domain ${task.knowledge_domain} --key ${task.curated_table_key || 'evaluation_matrix'}`,
    `Evidence points: ctox knowledge data describe --domain ${task.knowledge_domain} --key ${task.measurements_table_key || 'evidence_points'}`,
    missingTables.length ? `Missing tables to create first: ${missingTables.join(', ')}` : 'Required Knowledge tables already exist in the catalog.',
    '',
    'Auftrag:',
    task.prompt || defaultPromptForKnowledgeBase(base),
    '',
    task.criteria ? `Kriterien:\n${task.criteria}` : null,
    '',
    `Scoring-Modell:\n${scoringDimensions.map((axis) => `- ${axis.id}: ${axis.label}; weight=${axis.weight || scoringWeights(scoringDimensions)[axis.id] || 1}`).join('\n')}`,
    `Portfolio axes: x=${normalizedAxisPair(task).x}, y=${normalizedAxisPair(task).y}`,
    '',
    'Nutze den systematic-research Skill. Starte mit ctox knowledge search, dann ctox web deep-research. Schreibe jede Discovery-Runde sofort nach source_catalog. Lies/prüfe Quellen, extrahiere Fakten nach evidence_points und schreibe nur belegte Optionen mit gewichteten Scores nach evaluation_matrix. Aktualisiere bestehende Zeilen, wenn sich Fokus oder Kriterien ändern, statt parallele Tabellen zu erzeugen.',
  ].filter(Boolean).join('\n');
  const commandId = `cmd_${crypto.randomUUID()}`;
  const title = `Research · ${task.title}`;
  const threadKey = `business-os/research/${task.id}`;
  const payload = {
    title,
    instruction,
    prompt: instruction,
    priority: 'high',
    required_skills: ['systematic-research'],
    research_mode: 'library+living_dashboard',
    thread_key: threadKey,
    knowledge_domain: task.knowledge_domain,
    source_catalog_key: task.source_catalog_key,
    curated_table_key: task.curated_table_key,
    measurements_table_key: task.measurements_table_key,
    web_stack_plan: {
      first_command: `ctox web deep-research --query ${JSON.stringify(task.prompt || task.title)} --depth standard --max-sources 24`,
      followups: [
        'ctox web scholarly search --query <refined topic> --with-oa-pdf --only-doi',
        'ctox web read --url <candidate-url> --query <research focus>',
        'ctox web search only as fallback for non-technical/vendor lookup gaps',
      ],
    },
    knowledge_contract: {
      domain: task.knowledge_domain,
      tables: tableContract,
      create_missing_tables: missingTables,
      provenance_required: true,
    },
    scoring_contract: {
      dimensions: scoringDimensions,
      weights: scoringWeights(scoringDimensions),
      total_field: 'weighted_total',
      rule: 'Only score facts supported by a read source or durable Knowledge row; raw discovery candidates stay unscored.',
    },
    writeback_contract: {
      collections: ['research_runs', 'research_tasks', 'knowledge_tables'],
      dashboard_tables: {
        source_catalog: task.source_catalog_key || 'source_catalog',
        evaluation_matrix: task.curated_table_key || 'evaluation_matrix',
        evidence_points: task.measurements_table_key || 'evidence_points',
      },
    },
  };
  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: `${runInfoActionLabel(task)}: ${task.title}`,
      module: 'research',
      source_title: 'Research',
      command_id: commandId,
      command_type: 'research.systematic.run',
      record_id: task.id,
      title,
      instruction,
      thread_key: threadKey,
      reuseActive: false,
      payload,
      client_context: {
        action: 'research-run-chat',
        module: 'research',
        source_module: 'research',
        inbound_channel: 'business_os.research',
        knowledge_domain: task.knowledge_domain,
        knowledge_tables: base?.tables || [],
      },
    },
  }));
  const result = {
    ok: true,
    command_id: commandId,
    status: 'queued',
    task_status: 'queued',
    title,
    thread_key: threadKey,
    transport: 'business-chat',
  };
  const run = {
    id: `research_run_${now}`,
    task_id: task.id,
    status: result.task_status,
    command_id: commandId,
    task_queue_id: '',
    identified_count: state.sourceRows.length,
    accepted_count: state.sourceModels.length,
    used_count: state.sourceModels.length,
    payload: { result },
    created_at_ms: now,
    updated_at_ms: now,
  };
  state.runs = [run, ...state.runs.filter((item) => item.id !== run.id)];
  await upsertDoc(state.ctx.db.research_runs, run).catch((error) => {
    console.warn('[research] could not persist run', error);
  });
  await patchDoc(state.ctx.db.research_tasks, task.id, { status: 'collecting', updated_at_ms: now }).catch((error) => {
    console.warn('[research] could not patch task status', error);
  });
  setStatus(state.t('researchChatQueued', 'Research-Aufgabe im Chat gestartet.'));
  render();
}

function runInfoActionLabel(task) {
  return researchRunInfo(task).hasRun
    ? state.t('researchFortsetzen', 'Research fortsetzen')
    : state.t('researchStarten', 'Research starten');
}

async function updateTaskAxis(axis, value) {
  const task = selectedTask();
  if (!task) return;
  const patch = axis === 'x' ? { x_axis: safeAxis(value, task) } : { y_axis: safeAxis(value, task) };
  await patchDoc(state.ctx.db.research_tasks, task.id, { ...patch, updated_at_ms: Date.now() });
  Object.assign(task, patch);
  renderCenter();
}

function openKnowledgeTable(tableId) {
  if (!tableId) return;
  sessionStorage.setItem('ctox.businessOs.knowledge.openId', tableId);
  location.hash = 'knowledge';
}

function openSourceDrawer(sourceId) {
  const source = state.sourceModels.find((item) => item.id === sourceId);
  if (!source) return;
  const body = document.createElement('div');
  body.className = 'research-drawer';
  body.innerHTML = `
    <header><strong>${escapeHtml(source.title)}</strong><button type="button" data-close>×</button></header>
    <div class="research-drawer-body">
      <span class="research-grade research-grade-${source.grade.toLowerCase()}">${source.grade} · ${(source.score / 10).toFixed(1)}</span>
      <p>${escapeHtml(source.note || '')}</p>
      <pre>${escapeHtml(JSON.stringify(source.row, null, 2))}</pre>
    </div>
  `;
  body.querySelector('[data-close]')?.addEventListener('click', state.ctx.closeDrawers);
  state.ctx.openRightDrawer(body);
}

function focusCtoxRun(taskQueueId, commandId) {
  if (!taskQueueId && !commandId) return;
  sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify({
    taskId: taskQueueId,
    commandId,
    sourceModule: 'research',
  }));
  window.dispatchEvent(new CustomEvent('ctox-business-os-focus-task', {
    detail: {
      taskId: taskQueueId,
      commandId,
      sourceModule: 'research',
    },
  }));
  location.hash = 'ctox';
}

function initResearchContextMenu() {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu research-context-menu';
  menu.hidden = true;
  const root = state.ctx.host.querySelector('[data-research-root]') || state.ctx.host;
  root.append(menu);
  state.contextMenu = menu;

  const onContext = (event) => {
    if (state.ctx.module?.id !== 'research') return;
    if (state.contextMenu?.contains(event.target)) return;
    event.preventDefault();
    event.stopPropagation();
    const context = researchContextFromTarget(event.target);
    renderContextMenu(context, event.clientX, event.clientY);
  };
  const hide = (event) => {
    if (menu.contains(event.target)) return;
    menu.hidden = true;
  };
  const esc = (event) => {
    if (event.key === 'Escape') menu.hidden = true;
  };
  state.ctx.host.addEventListener('contextmenu', onContext);
  window.addEventListener('click', hide, { capture: true });
  window.addEventListener('keydown', esc);
  return () => {
    state.ctx.host.removeEventListener('contextmenu', onContext);
    window.removeEventListener('click', hide, { capture: true });
    window.removeEventListener('keydown', esc);
  };
}

function researchContextFromTarget(target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  const record = element?.closest?.('[data-context-record-id]');
  const pane = element?.closest?.('.research-pane');
  const field = element?.closest?.('input, textarea, select, button');
  const task = selectedTask();
  return {
    module: 'research',
    column: pane?.classList.contains('research-left') ? 'ranking' : pane?.classList.contains('research-center') ? 'dashboard' : pane?.classList.contains('research-right') ? 'context' : 'module',
    field: field?.name || field?.dataset.action || field?.dataset.tab || field?.dataset.axisSelect || '',
    record_type: record?.dataset.contextRecordType || 'research_task',
    record_id: record?.dataset.contextRecordId || state.selectedSourceId || task?.id || '',
    label: record?.dataset.contextLabel || selectedSource()?.title || task?.title || 'Research',
    knowledge_domain: task?.knowledge_domain || '',
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderContextMenu(context, x, y) {
  const canModifyApp = canModifyResearchApp();
  state.contextMenu.innerHTML = `
    <form class="research-context-chat" data-research-context-chat-form>
      <header>
        <div>
          <strong>${escapeHtml(state.t('chatToCtox', 'Chat to CTOX'))}</strong>
          <span>${escapeHtml(researchContextSummary(context))}</span>
        </div>
        <button type="button" data-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">×</button>
      </header>
      ${canModifyApp ? `
        <div class="research-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label><input type="radio" name="mode" value="data" checked> ${escapeHtml(state.t('workWithResearch', 'Mit Research arbeiten'))}</label>
          <label><input type="radio" name="mode" value="app"> ${escapeHtml(state.t('modifyDashboard', 'Dashboard modifizieren'))}</label>
        </div>
      ` : ''}
      <textarea name="message" placeholder="${escapeHtml(state.t('chatPlaceholder', 'Was soll CTOX hier tun oder prüfen?'))}"></textarea>
      <footer><span data-status></span><button type="submit">${escapeHtml(state.t('send', 'Senden'))}</button></footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const rootRect = state.contextMenu.parentElement.getBoundingClientRect();
  const localX = x - rootRect.left;
  const localY = y - rootRect.top;
  const maxLeft = Math.max(8, rootRect.width - rect.width - 8);
  const maxTop = Math.max(8, rootRect.height - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(localX, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(localY, 8, maxTop)}px`;
  state.contextMenu.querySelector('[data-close]')?.addEventListener('click', () => {
    state.contextMenu.hidden = true;
  });
  state.contextMenu.querySelector('[data-research-context-chat-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const mode = canModifyApp && form.get('mode') === 'app' ? 'app' : 'data';
    const message = String(form.get('message') || '').trim();
    dispatchResearchContextChat(context, message, mode);
  });
  requestAnimationFrame(() => state.contextMenu.querySelector('textarea')?.focus());
}

function canModifyResearchApp() {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function researchContextSummary(context) {
  return [context.column || 'module', context.record_type || '', context.label || context.record_id || '']
    .filter(Boolean)
    .join(' · ') || 'Research';
}

function dispatchResearchContextChat(context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-status]');
  if (!trimmed) {
    if (status) status.textContent = state.t('messageMissing', 'Nachricht fehlt.');
    return;
  }
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = state.t('chatNotReady', 'Chat ist noch nicht bereit.');
    return;
  }

  const safeMode = mode === 'app' && canModifyResearchApp() ? 'app' : 'data';
  const task = selectedTask();
  const source = selectedSource();
  const title = `${safeMode === 'app' ? 'Web Research Dashboard modifizieren' : 'Research bearbeiten'} · ${context.label || task?.title || 'Research'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere das Business-OS Research Modul anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Knowledge-Daten selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : `Arbeite mit dem Web Research Dashboard und der verknuepften Knowledge Base.\n\n${trimmed}`;

  if (status) status.textContent = state.t('openChatting', 'Öffne Chat...');
  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'research',
      source_title: 'Research',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'research' : (context.record_id || task?.id || 'research'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        prompt: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : 'data',
        selected_task: task || null,
        selected_source: source || null,
        context,
        thread_key: `business-os/research/${task?.id || 'context'}`,
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        module: 'research',
        column: context.column,
        record_type: context.record_type,
        record_id: context.record_id,
        knowledge_domain: context.knowledge_domain || task?.knowledge_domain || '',
      },
    },
  }));
  state.contextMenu.hidden = true;
}

function pane(name) {
  return state.ctx.host.querySelector(`.research-${name}`);
}

function selectedTask() {
  return state.tasks.find((task) => task.id === state.selectedTaskId) || state.tasks[0] || null;
}

function selectedSource() {
  return state.sourceModels.find((source) => source.id === state.selectedSourceId) || state.sourceModels[0] || null;
}

function latestRunForTask(taskId) {
  if (!taskId) return null;
  return state.runs
    .filter((run) => run.task_id === taskId)
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function researchRunInfo(task) {
  const run = latestRunForTask(task?.id);
  const fallbackCommand = latestResearchCommandForTask(task?.id);
  const commandId = run?.command_id || run?.payload?.result?.command_id || fallbackCommand?.command_id || fallbackCommand?.id || '';
  const taskQueueId = run?.task_queue_id || run?.payload?.result?.task_id || '';
  const command = commandId
    ? state.commands.find((item) => item.command_id === commandId || item.id === commandId)
    : fallbackCommand;
  const queueTask = taskQueueId
    ? state.queueTasks.find((item) => item.id === taskQueueId)
    : commandId
      ? state.queueTasks.find((item) => item.command_id === commandId)
      : null;
  const status = queueTask?.status || command?.task_status || command?.status || run?.status || '';
  const statusKind = statusKindFor(status);
  return {
    run,
    command,
    queueTask,
    commandId,
    taskQueueId: queueTask?.id || taskQueueId,
    commandType: command?.command_type || queueTask?.command_type || '',
    title: queueTask?.title || command?.payload?.title || run?.payload?.result?.title || '',
    threadKey: queueTask?.thread_key || command?.payload?.thread_key || '',
    status,
    statusKind,
    statusLabel: statusLabel(status),
    hasRun: Boolean(run || command || queueTask),
    isActive: ['queued', 'running', 'accepted', 'blocked'].includes(statusKind),
    updatedLabel: relativeTime(queueTask?.updated_at_ms || command?.updated_at_ms || run?.updated_at_ms),
  };
}

function latestResearchCommandForTask(taskId) {
  if (!taskId) return null;
  return state.commands
    .filter((command) => command.record_id === taskId && String(command.command_type || '').startsWith('research.systematic.'))
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function statusKindFor(status) {
  const value = String(status || '').toLowerCase();
  if (['leased', 'running', 'in_progress', 'collecting'].includes(value)) return 'running';
  if (['accepted', 'queued', 'pending'].includes(value)) return 'queued';
  if (['handled', 'completed', 'done', 'ready'].includes(value)) return 'completed';
  if (['blocked'].includes(value)) return 'blocked';
  if (['failed', 'error'].includes(value)) return 'failed';
  if (['cancelled', 'canceled'].includes(value)) return 'cancelled';
  return value || 'idle';
}

function statusLabel(status) {
  const kind = statusKindFor(status);
  const key = `status${kind.charAt(0).toUpperCase()}${kind.slice(1)}`;
  return state.t(key, kind) || status || state.t('statusIdle', 'No active run');
}

function knowledgeBaseForTask(task) {
  return state.knowledgeBases.find((base) => base.domain === task?.knowledge_domain) || null;
}

function tableForKey(base, key) {
  if (!base || !key) return null;
  return base.tables.find((table) => table.table_key === key) || null;
}

function firstTableMatching(base, pattern) {
  return base?.tables?.find((table) => pattern.test(`${table.table_key} ${table.title} ${table.description}`)) || null;
}

function tableKey(base, keys) {
  return keys.map((key) => tableForKey(base, key)?.table_key).find(Boolean) || '';
}

function scoringDimensionsForTask(task) {
  const custom = Array.isArray(task?.payload?.scoring_dimensions)
    ? task.payload.scoring_dimensions
    : Array.isArray(task?.scoring_dimensions)
      ? task.scoring_dimensions
      : null;
  return dedupeDimensions((custom?.length ? custom : inferScoringDimensions(task)).concat({ id: 'portfolio_priority', label: 'Portfolio priority' }));
}

function inferScoringDimensions(task) {
  const kind = inferResearchKind(task);
  if (kind === 'bearing') return [...BEARING_AXES];
  if (kind === 'competitive_ai') return [...COMPETITIVE_AI_AXES];
  return [...BASE_AXES];
}

function inferResearchKind(task) {
  const text = [
    task?.knowledge_domain,
    task?.domain,
    task?.title,
    task?.prompt,
    task?.criteria,
  ].join(' ').toLowerCase();
  if (/bearing|propeller|uav|drone|load|rpm|thrust|torque/.test(text)) return 'bearing';
  if (/(competitive|competitor|wettbewerb|anbieter|unternehmen|market).*(agent|employee|worker|ki|ai)|agent.*(employee|worker|enterprise|platform)|ki[-\s]?mitarbeiter|ai employee/.test(text)) return 'competitive_ai';
  return 'generic';
}

function defaultAxisPairForTask(task) {
  const kind = inferResearchKind(task);
  if (kind === 'competitive_ai') return { x: 'overlap', y: 'buyer_clarity' };
  if (kind === 'bearing') return { x: 'evidence_strength', y: 'direct_load_relevance' };
  return { x: DEFAULT_AXIS_X, y: DEFAULT_AXIS_Y };
}

function normalizedAxisPair(task) {
  const defaults = defaultAxisPairForTask(task);
  const x = safeAxis(task?.x_axis, task, defaults.x);
  let y = safeAxis(task?.y_axis, task, defaults.y);
  if (x === y) {
    y = safeAxis(defaults.y, task, x === defaults.y ? 'topic_fit' : defaults.y);
    if (x === y) y = scoringDimensionsForTask(task).find((axis) => axis.id !== x)?.id || y;
  }
  return { x, y };
}

function dedupeDimensions(dimensions) {
  const seen = new Set();
  const result = [];
  for (const dimension of dimensions || []) {
    const id = normalizeAxisId(dimension?.id || dimension?.key || dimension?.name);
    if (!id || seen.has(id)) continue;
    seen.add(id);
    const weight = Number(dimension?.weight);
    result.push({
      id,
      label: String(dimension?.label || dimension?.title || groupLabel(id)).trim() || groupLabel(id),
      ...(Number.isFinite(weight) && weight > 0 ? { weight } : {}),
    });
  }
  return result.length ? result : [...BASE_AXES];
}

function parseDimensionLines(raw) {
  const dimensions = String(raw || '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const match = line.match(/^([^:=-]+)[:=-]\s*(.+)$/);
      if (match) return parseDimensionDefinition(match[1], match[2]);
      return parseDimensionDefinition(line, groupLabel(line));
    })
    .filter((dimension) => dimension.id);
  return dimensions.length ? dedupeDimensions(dimensions) : null;
}

function formatDimensionLines(dimensions) {
  return dedupeDimensions(dimensions)
    .filter((dimension) => dimension.id !== 'portfolio_priority')
    .map((dimension) => `${dimension.id}: ${dimension.label}${dimension.weight ? ` | ${dimension.weight}` : ''}`)
    .join('\n');
}

function parseDimensionDefinition(rawId, rawLabel) {
  const labelText = String(rawLabel || '').trim();
  const weightMatch = labelText.match(/^(.*?)\s*(?:\|\s*weight\s*=?|\|\s*|\((?:weight\s*=?\s*)?)(0?\.\d+|[1-9]\d*(?:\.\d+)?)\)?\s*$/i);
  const label = (weightMatch?.[1] || labelText).trim();
  const weight = weightMatch ? Number(weightMatch[2]) : NaN;
  return {
    id: normalizeAxisId(rawId),
    label: label || groupLabel(rawId),
    ...(Number.isFinite(weight) && weight > 0 ? { weight } : {}),
  };
}

function scoringWeights(dimensions) {
  const axes = dedupeDimensions(dimensions).filter((axis) => axis.id !== 'portfolio_priority');
  const explicit = axes.some((axis) => Number(axis.weight) > 0);
  if (explicit) {
    return Object.fromEntries(axes.map((axis) => [axis.id, Number(axis.weight || 1)]));
  }
  const weight = axes.length ? Number((1 / axes.length).toFixed(3)) : 1;
  return Object.fromEntries(axes.map((axis) => [axis.id, weight]));
}

function normalizeAxisId(value) {
  return slugId(value).slice(0, 72);
}

function axisSelect(axis, selected, variant = 'toolbar') {
  const task = selectedTask();
  const axes = scoringDimensionsForTask(task);
  const isMapAxis = variant === 'map';
  const axisName = axis === 'x' ? state.t('horizontalLabel', 'Horizontal') : state.t('verticalLabel', 'Vertical');
  const label = isMapAxis ? (axis === 'x' ? state.t('xAxisLabel', 'X Axis') : state.t('yAxisLabel', 'Y Axis')) : axisName;
  return `
    <label class="${isMapAxis ? `research-map-axis research-map-axis-${axis}` : 'research-axis-select'}">
      <span>${escapeHtml(label)}</span>
      <select data-axis-select="${axis}" aria-label="${escapeHtml(axisName)} axis">
        ${axes.map((item) => `<option value="${item.id}" ${item.id === selected ? 'selected' : ''}>${escapeHtml(item.label)}</option>`).join('')}
      </select>
    </label>
  `;
}

function tabButton(id, label) {
  return `<button type="button" data-action="tab" data-tab="${id}" aria-pressed="${state.activeTab === id}">${escapeHtml(label)}</button>`;
}

function disabledTabButton(id, label) {
  return `<button type="button" data-tab="${escapeHtml(id)}" aria-disabled="true" disabled>${escapeHtml(label)}</button>`;
}

function axisLabel(id, task = selectedTask()) {
  return scoringDimensionsForTask(task).find((axis) => axis.id === id)?.label || groupLabel(id);
}

function groupLabel(value) {
  return String(value || 'source')
    .replace(/[_-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function shortId(value) {
  const text = String(value || '').trim();
  if (!text) return '-';
  return text.length > 18 ? `${text.slice(0, 10)}…${text.slice(-5)}` : text;
}

function relativeTime(ms) {
  const value = Number(ms || 0);
  if (!value) return '';
  const diff = Math.max(0, Date.now() - value);
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diff < minute) return state.t('relativeJustNow', 'gerade eben');
  if (diff < hour) return state.t('relativeMin', `vor ${Math.round(diff / minute)} min`, Math.round(diff / minute));
  if (diff < day) return state.t('relativeHour', `vor ${Math.round(diff / hour)} h`, Math.round(diff / hour));
  const localeStr = state.lang === 'en' ? 'en-US' : 'de-DE';
  return new Date(value).toLocaleDateString(localeStr, { day: '2-digit', month: '2-digit' });
}

function iconSvg(name) {
  const paths = {
    refresh: '<path d="M21 12a9 9 0 0 1-15 6.7L3 16m0 0v5h5M3 12a9 9 0 0 1 15-6.7L21 8m0 0V3h-5"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    table: '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="3" y1="15" x2="21" y2="15"/><line x1="10" y1="3" x2="10" y2="21"/>',
    grid: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/>',
    eye: '<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>',
    eyeOff: '<path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/>',
  };
  return `<svg aria-hidden="true" viewBox="0 0 24 24" focusable="false">${paths[name] || ''}</svg>`;
}

function safeAxis(value, task = selectedTask(), fallback = DEFAULT_AXIS_X) {
  const axes = scoringDimensionsForTask(task);
  return axes.some((axis) => axis.id === value) ? value : (axes.some((axis) => axis.id === fallback) ? fallback : axes[0]?.id || DEFAULT_AXIS_X);
}

function pointJitter(source) {
  const seed = Array.from(String(source.id || source.title))
    .reduce((sum, char) => sum + char.charCodeAt(0), 0);
  return {
    x: ((seed % 13) - 6) * 1.6,
    y: (((Math.floor(seed / 13) % 13) - 6) * 1.6),
  };
}

function avgScore() {
  if (!state.sourceModels.length) return '0.0';
  return (state.sourceModels.reduce((sum, item) => sum + item.score, 0) / state.sourceModels.length / 10).toFixed(1);
}

async function findAll(collection, collectionName = '') {
  if (!collection?.find) {
    if (collectionName) markCollectionDiagnostic(collectionName, 'read', 'missing', state.t('collectionMissing', 'Daten nicht verfügbar'));
    return [];
  }
  try {
    const docs = await withTimeout(collection.find().exec(), COLLECTION_READ_TIMEOUT_MS, 'collection read timed out');
    if (collectionName) markCollectionDiagnostic(collectionName, 'read', 'ok', `${docs.length} rows`);
    return docs.map(toJson);
  } catch (error) {
    if (collectionName) markCollectionDiagnostic(collectionName, 'read', 'failed', errorMessage(error));
    return [];
  }
}

function markCollectionDiagnostic(collection, phase, kind, message = '') {
  const current = state.diagnostics.collections[collection] || {};
  state.diagnostics.collections[collection] = {
    ...current,
    collection,
    [phase]: {
      kind,
      message: String(message || ''),
      at: Date.now(),
    },
  };
}

function diagnosticRows() {
  return collectionDiagnosticRows(RESEARCH_COLLECTIONS, state.diagnostics.collections, state.t);
}

function collectionDiagnosticRows(collections, diagnostics = {}, t = (_key, fallback) => fallback) {
  return collections.map((collection) => {
    const diagnostic = diagnostics[collection] || {};
    const read = diagnostic.read || null;
    const sync = diagnostic.sync || null;
    const failed = [sync, read].find((entry) => entry?.kind === 'failed');
    if (failed) {
      return {
        collection,
        kind: 'failed',
        label: failed.message || t('failed', 'fehlgeschlagen'),
      };
    }
    if (read?.kind === 'ok') return { collection, kind: 'ok', label: read.message || t('loadedShort', 'geladen') };
    if (sync?.kind === 'ok') return { collection, kind: 'ok', label: t('syncReady', 'Sync bereit') };
    if (sync?.kind === 'local') return { collection, kind: 'local', label: t('localOnly', 'Lokaler Modus') };
    if (read?.kind === 'missing') return { collection, kind: 'missing', label: read.message };
    return { collection, kind: 'pending', label: t('pendingShort', 'wartet') };
  });
}

function diagnosticFailures() {
  return diagnosticRows()
    .filter((row) => row.kind === 'failed' || row.kind === 'missing')
    .map((row) => ({ collection: row.collection, message: row.label }));
}

function reloadStatusText() {
  const failures = diagnosticFailures();
  if (failures.length) return state.t('researchUnavailableTitle', 'Research ist gerade nicht verfügbar');
  if (!state.diagnostics.reloadFinishedAt) return state.t('loadingKnowledge', 'Knowledge wird geladen...');
  const domainCount = state.knowledgeBases.length;
  const taskCount = state.tasks.length;
  const sourceCount = state.sourceModels.length;
  if (!domainCount) return state.t('noKnowledgeDomains', 'Noch keine Knowledge Base verfügbar');
  return state.t('researchReadySummary', '{0} Aufgaben, {1} Knowledge Bases, {2} Quellen verfügbar.', taskCount, domainCount, sourceCount);
}

async function upsertDoc(collection, doc) {
  if (!collection) return null;
  if (typeof collection.upsert === 'function') return withTimeout(collection.upsert(doc), 1600, 'collection upsert timed out');
  const existing = await collection.findOne(doc.id).exec();
  if (existing) return existing.incrementalPatch(doc);
  return withTimeout(collection.insert(doc), 1600, 'collection insert timed out');
}

async function patchDoc(collection, id, patch) {
  const existing = await withTimeout(collection?.findOne(id).exec(), 1600, 'collection patch lookup timed out');
  if (existing?.incrementalPatch) return existing.incrementalPatch(patch);
  if (existing?.atomicPatch) return existing.atomicPatch(patch);
  return null;
}

function withTimeout(promise, timeoutMs, message) {
  let timer = null;
  const timeout = new Promise((_, reject) => {
    timer = window.setTimeout(() => reject(new Error(message)), timeoutMs);
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timer) window.clearTimeout(timer);
  });
}

function toJson(doc) {
  return typeof doc?.toJSON === 'function' ? doc.toJSON() : { ...(doc || {}) };
}

function firstArray(...values) {
  return values.find(Array.isArray) || [];
}

function sourceId(row) {
  return firstString(row, ['source_id', 'id', 'record_id', 'source_key']);
}

function firstString(row, keys) {
  for (const key of keys) {
    const value = row?.[key];
    if (value !== null && value !== undefined && String(value).trim()) return String(value).trim();
  }
  return '';
}

function defaultPromptForKnowledgeBase(base) {
  if (!base) return state.t('defaultPromptGeneric', 'Erstelle ein kompaktes Web Research Dashboard auf Basis der ausgewählten Knowledge Base.');
  return state.t('defaultPromptText', `Erzeuge ein übersichtliches Dashboard auf Basis der Knowledge Base ${base.domain}. Nutze source_catalog als Rohquellenbasis, kuratierte Tabellen als Auswertung und Score nur belegte Quellen.`, base.domain);
}

function topicFitScore(task, text, row) {
  const titleText = String(row?.title || '').toLowerCase();
  
  // High-fidelity keyword filter on Title to prevent crawler noise / cross-domain leakage!
  const hasDroneTopic = /propeller|rotor|uav|drone|bearing|load|force|moment|thrust|torque|rpm|vibration|spindel|motor|flight|telemetry|aerodynamic|blade|windtunnel|w\u00e4lzlager|lager|schub|drehmoment|last|messung|pr\u00fcfstand|spindle|vibrat|flight|telemetr|testing|bench|load cell|stanag|mil-std/i.test(titleText);
  
  if (!hasDroneTopic) {
    return 10;
  }

  const haystack = String(text || '').toLowerCase();
  const terms = [
    task?.title,
    task?.prompt,
    task?.criteria,
    task?.knowledge_domain,
    row?.source_class,
  ].join(' ')
    .toLowerCase()
    .split(/[^a-z0-9äöüß]+/i)
    .map((term) => term.trim())
    .filter((term) => term.length >= 4 && !STOP_TERMS.has(term))
    .slice(0, 32);
  const unique = [...new Set(terms)];
  const hits = unique.filter((term) => haystack.includes(term)).length;
  return clampScore(28 + Math.min(48, hits * 8) + (hasUrl(row) ? 6 : 0));
}

function titleFromDomain(domain) {
  return String(domain || 'Knowledge')
    .replace(/[_/-]+/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function normalizeResearchDomain(value) {
  const raw = String(value || '').trim();
  if (!raw) return 'research/general';
  if (raw.includes('/')) return raw.replace(/^\/+|\/+$/g, '').replace(/\s+/g, '-').toLowerCase();
  return `research/${slugId(raw).replace(/_/g, '-')}`;
}

function slugId(value) {
  return String(value || 'research')
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 90) || 'research';
}

function gradeForScore(score) {
  if (score >= 82) return 'A';
  if (score >= 66) return 'B';
  if (score >= 48) return 'C';
  return 'D';
}

function clampScore(value) {
  return Math.max(4, Math.min(96, Math.round(Number(value) || 0)));
}

function normalizeScoreScale(value) {
  const next = Number(value);
  if (!Number.isFinite(next)) return 0;
  if (next > 0 && next <= 1) return clampScore(next * 100);
  if (next > 0 && next <= 10) return clampScore(next * 10);
  return clampScore(next);
}

function numberValue(value) {
  const next = Number(value);
  return Number.isFinite(next) ? next : 0;
}

function weightedAverage(pairs) {
  let sum = 0;
  let weight = 0;
  for (const [value, itemWeight] of pairs) {
    const next = Number(value);
    const nextWeight = Number(itemWeight);
    if (!Number.isFinite(next) || !Number.isFinite(nextWeight)) continue;
    sum += next * nextWeight;
    weight += nextWeight;
  }
  return weight ? sum / weight : 0;
}

function hasUrl(row) {
  return Boolean(firstString(row, ['source_url', 'url', 'direct_url', 'homepage', 'website', 'doi']));
}

function clampNumber(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function formatNumber(value) {
  const next = Number(value);
  if (!Number.isFinite(next)) return '0';
  return next.toLocaleString('de-DE', { maximumFractionDigits: Math.abs(next) >= 100 ? 0 : 2 });
}

function shortLabel(value) {
  const text = String(value || '').replace(/\s+/g, ' ').trim();
  if (text.length <= 22) return text;
  return `${text.slice(0, 20).trim()}...`;
}

function isPresent(value) {
  return value !== null && value !== undefined && String(value).trim() !== '';
}

function setStatus(value) {
  state.status = value;
  const line = state.ctx.host.querySelector('.research-status-line');
  if (line) line.textContent = value;
}

function errorMessage(error) {
  return String(error?.message || error || '').trim() || 'unknown error';
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

const DOCUMENT_PROMPTS = {
  'doc_deep_research_report.md': `Generiere einen wissenschaftlichen, umfassenden Deep Research Word-Bericht auf Deutsch zum Thema "Wälzlagerauslegung für taktische UAVs unter militärischen Grenzlasten". 
Fokus: Detaillierte Übersicht aller 125 Wellen, wissenschaftliche Validierungsmethoden, mathematische Belastungsberechnungen (z.B. Hertzsche Pressung) und fundierte Zitate aus echten Forschungsdaten.`,
  'doc_decision_brief.md': `Generiere eine Entscheidungsvorlage zur Schmierstoff- und Dichtungsauswahl für arktische und chemische Einsatzbedingungen von Drohnen-Spindellagern.
Fokus: Vergleich arktischer Tieftemperatur-Fette (-60°C) und chemisch laugenresistenter Polymer-Dichtungssysteme für militärische Dekontaminations-Spülungen.`,
  'doc_feasibility_study.md': `Generiere eine Machbarkeitsstudie zur berührungslosen Früherkennung von EDM-Laufflächen-Pitting an Spindellagern unter elektromagnetischen Radarstörungen.
Fokus: Eignungsbewertung von Induktions-Thermografie (ECPT) und mmWave-Inline-Scannern zur zerstörungsfreien Zustandsüberwachung im Einsatz.`,
  'doc_market_research.md': `Generiere eine umfassende Marktanalyse für hochrobuste, zivil-militärische Outrunner-Motoren und Wälzlagerungen (< 25 kg MTOW).
Fokus: Marktsegmente, Verteidigungs-Barrieren, Analyse führender Lieferanten wie KDE Direct und T-Motor (inklusive Preispunkte und Dichtungsvarianten).`,
  'doc_project_description.md': `Generiere eine Projektbeschreibung / Fördervorhaben zur Entwicklung eines resonanzresistenten Spindellagersystems für FPV-Kampfdrohnen im aktiven Störumfeld.
Fokus: Begründungs- und Förderlogik, aktueller Stand der Technik, ESC-induzierte Resonanzschäden, innovative Technologiesprünge und eine strukturierte Arbeitspaket-Kostenmatrix.`,
  'doc_source_review.md': `Generiere ein Quellenreview und Datenabdeckungs-Kompendium der wissenschaftlichen, militärischen und industriellen Referenzen.
Fokus: Systematische Suchmethodik, Klassifikationstaxonomie nach Vertrauensgraden (Grade A bis D), Coverage-Analyse und Offenlegung verbleiberinger Datenlücken im Bereich kleiner Drohnen-Antriebe.`,
  'doc_literature_review.md': `Generiere einen wissenschaftlichen Stand der Technik zu aeroelastischem Flattern und dynamic-stall-induzierten Biegebewegungen im Sturzflug.
Fokus: Physikalischer Konsens über kreiselwirksame Momente, instationäre Aerodynamik und hochfrequente Lastspitzen an den Lagersitzflächen durch Dynamic Stall bei FPV-Drohnen.`,
  'doc_technology_screening.md': `Generiere ein Technologie-Screening von Wellen- und Gehäusewerkstoffen für ultraleichte Drohnen-Spindellagerungen.
Fokus: Strukturierter mechanischer Vergleich von Aluminium 7075-T6, Titan Grade 5 und Kohlefaser-Verbundwerkstoffen (CFK) hinsichtlich Steifigkeit, thermischer Dehnung und Gewichtsvorteil.`,
  'doc_competitive_analysis.md': `Generiere eine strukturierte Wettbewerberanalyse für Triebwerks- und Wälzlagerhersteller im Bereich Class 1-2 UAS.
Fokus: Detaillierte Bewertungsmatrix von T-Motor, KDE Direct und Tyto Robotics hinsichtlich Fertigungstoleranzen, IP-Schutzklassen (z.B. IP54 Lagerseals) und militärischer Tauglichkeit.`,
  'doc_whitepaper.md': `Generiere ein Whitepaper zum Thema "Cyber-physische Schutzstrategien gegen ESC-Resonanzangriffe auf Drohnen-Antriebslager".
Fokus: Argumentative Empfehlung kombinierter Schutzmaßnahmen durch Firmware-Notch-Filter in den Reglern (ESC) und mechanische Dämpfungsringe (Dämpfungs-O-Ringe) zur Verschleißminderung.`,
  'doc_requirements_extraction.md': `Generiere eine systematische Anforderungsextraktion aus militärischen STANAG-Lufttüchtigkeits- und MIL-STD-Härteprüfvorschriften.
Fokus: Detaillierte Extraktionstabelle für Schockzyklen, Sandsturm-Geschwindigkeiten, Vibrationsprofile und Salznebel-Testdauern gemäß STANAG 4671/4703 und MIL-STD-810H.`,
  'doc_risk_assessment.md': `Generiere eine Risikoanalyse und Fehlermöglichkeits- und Einflussanalyse (FMEA) für Spindellagerschäden unter Gefechtsbedingungen.
Fokus: Vollständige FMEA-Risikomatrix mit Risikoprioritätszahlen (RPZ) zu Schmierfilm-Washout durch Laugenwäschen, abrasivem Sandverschleiß und EDM-Laufflächen-Pitting.`
};

const GENERATED_REPORTS = [
  {
    id: 'doc_deep_research_report',
    filename: 'umfassender-deep-research-bericht-zur-waelzlagerauslegung.md',
    title: 'Umfassender Deep Research Bericht zur Wälzlagerauslegung für taktische UAVs unter militärischen Grenzlasten',
    category: 'Deep Research'
  },
  {
    id: 'doc_decision_brief',
    filename: 'entscheidungsvorlage-zur-schmierstoff-und-dichtungsauswahl.md',
    title: 'Entscheidungsvorlage zur Schmierstoff- und Dichtungsauswahl für arktische und chemische Einsatzbedingungen',
    category: 'Entscheidungsvorlage'
  },
  {
    id: 'doc_feasibility_study',
    filename: 'machbarkeitsstudie-zur-beruehrungslosen-frueherkennung-von-edm-pitting.md',
    title: 'Machbarkeitsstudie zur berührungslosen Früherkennung von EDM-Laufflächen-Pitting an Spindellagern unter Radarstörungen',
    category: 'Machbarkeitsstudie'
  },
  {
    id: 'doc_market_research',
    filename: 'marktanalyse-fuer-hochrobuste-outrunner-motoren.md',
    title: 'Marktanalyse für hochrobuste, zivil-militärische Outrunner-Motoren und Wälzlagerungen (< 25 kg MTOW)',
    category: 'Markt & Wettbewerb'
  },
  {
    id: 'doc_project_description',
    filename: 'projektbeschreibung-entwicklung-eines-resonanzresistenten-spindellagersystems.md',
    title: 'Projektbeschreibung – Entwicklung eines resonanzresistenten Spindellagersystems für FPV-Kampfdrohnen im aktiven Störumfeld',
    category: 'Projektbeschreibung'
  },
  {
    id: 'doc_source_review',
    filename: 'quellenreview-und-datenabdeckungs-kompendium-der-323-referenzen.md',
    title: 'Quellenreview und Datenabdeckungs-Kompendium der 323 wissenschaftlichen, militärischen und industriellen Referenzen',
    category: 'Quellenreview'
  },
  {
    id: 'doc_literature_review',
    filename: 'wissenschaftlicher-stand-der-technik-zu-aeroelastischem-flattern.md',
    title: 'Wissenschaftlicher Stand der Technik zu aeroelastischem Flattern und dynamic-stall-induzierten Biegebewegungen im Sturzflug',
    category: 'Stand der Technik'
  },
  {
    id: 'doc_technology_screening',
    filename: 'technologie-screening-von-wellen-und-gehaeusewerkstoffen.md',
    title: 'Technologie-Screening von Wellen- und Gehäusewerkstoffen für ultraleichte Drohnen-Spindellagerungen',
    category: 'Technologie-Screening'
  },
  {
    id: 'doc_competitive_analysis',
    filename: 'strukturierte-wettbewerberanalyse-triebwerks-waelzlagerhersteller.md',
    title: 'Strukturierte Wettbewerberanalyse für Triebwerks- und Wälzlagerhersteller im Bereich Class 1-2 UAS',
    category: 'Wettbewerberanalyse'
  },
  {
    id: 'doc_whitepaper',
    filename: 'whitepaper-cyber-physische-schutzstrategien-esc-resonanzangriffe.md',
    title: 'Whitepaper – Cyber-physische Schutzstrategien gegen ESC-Resonanzangriffe auf Drohnen-Antriebslager',
    category: 'Whitepaper'
  },
  {
    id: 'doc_requirements_extraction',
    filename: 'systematische-anforderungsextraktion-stanag-mil-std.md',
    title: 'Systematische Anforderungsextraktion aus militärischen STANAG-Lufttüchtigkeits- und MIL-STD-Härteprüfvorschriften',
    category: 'Spezifikation'
  },
  {
    id: 'doc_risk_assessment',
    filename: 'risikoanalyse-fmea-spindellager-gefechtsbedingungen.md',
    title: 'Risikoanalyse und Fehlermöglichkeits- und Einflussanalyse (FMEA) für Spindellagerschäden unter realen Gefechtsbedingungen',
    category: 'Risiko & FMEA'
  }
];

function getPromptForFilename(filename) {
  const f = filename.toLowerCase();
  if (f.includes('schmierstoff') || f.includes('entscheidung')) {
    return DOCUMENT_PROMPTS['doc_decision_brief.md'];
  }
  if (f.includes('edm') || f.includes('machbarkeit') || f.includes('pitting')) {
    return DOCUMENT_PROMPTS['doc_feasibility_study.md'];
  }
  if (f.includes('marktanalyse') || f.includes('outrunner')) {
    return DOCUMENT_PROMPTS['doc_market_research.md'];
  }
  if (f.includes('projektbeschreibung') || f.includes('resonanzresistenz')) {
    return DOCUMENT_PROMPTS['doc_project_description.md'];
  }
  if (f.includes('quellenreview') || f.includes('kompendium') || f.includes('323-referenzen')) {
    return DOCUMENT_PROMPTS['doc_source_review.md'];
  }
  if (f.includes('aeroelastisch') || f.includes('flattern') || f.includes('stand-der-technik') || f.includes('wissenschaftlicher-stand')) {
    return DOCUMENT_PROMPTS['doc_literature_review.md'];
  }
  if (f.includes('werkstoff') || f.includes('screening')) {
    return DOCUMENT_PROMPTS['doc_technology_screening.md'];
  }
  if (f.includes('wettbewerb') || f.includes('triebwerks-waelzlagerhersteller')) {
    return DOCUMENT_PROMPTS['doc_competitive_analysis.md'];
  }
  if (f.includes('whitepaper') || f.includes('cyber-physisch')) {
    return DOCUMENT_PROMPTS['doc_whitepaper.md'];
  }
  if (f.includes('anforderung') || f.includes('stanag')) {
    return DOCUMENT_PROMPTS['doc_requirements_extraction.md'];
  }
  if (f.includes('risiko') || f.includes('fmea')) {
    return DOCUMENT_PROMPTS['doc_risk_assessment.md'];
  }
  if (f.includes('umfassender-deep-research') || f.includes('waelzlagerauslegung')) {
    return DOCUMENT_PROMPTS['doc_deep_research_report.md'];
  }
  return 'Führe eine umfassende Recherche und Aggregation aller 322 wissenschaftlichen und empirischen Quellen durch. Konsolidiere die Datenpunkte (816 Messungen) zu einer detaillierten Systemanalyse zur Wälzlagerauslegung für Drohnenantriebe unter 25 kg MTOW.';
}

function showPromptViewer(filename) {
  const promptText = getPromptForFilename(filename);
  
  const backdrop = document.createElement("div");
  backdrop.className = "research-modal-backdrop";
  backdrop.style.zIndex = "10000";
  backdrop.innerHTML = `
    <div class="research-modal" style="position: relative;">
      <div style="display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid var(--research-line); padding-bottom: 10px;">
        <div>
          <span style="font-size: 10px; color: var(--research-accent); text-transform: uppercase; font-weight: 800; letter-spacing: 0.5px; display: block; margin-bottom: 2px;">KI-Generierung</span>
          <h3 style="font-size: 14px; font-weight: 700; margin: 0; color: var(--research-text);">System-Prompt des Fachberichts</h3>
        </div>
        <button type="button" class="research-map-reset" style="padding: 4px 8px; margin: 0;" onclick="this.closest('.research-modal-backdrop').remove()">Schließen</button>
      </div>
      <div style="font-family: monospace; font-size: 11px; line-height: 1.6; color: var(--research-text); max-height: 380px; overflow-y: auto; white-space: pre-wrap; background: var(--research-surface-2); padding: 12px; border-radius: 6px; border: 1px solid var(--research-line); box-shadow: inset 0 2px 6px rgba(0,0,0,0.15); text-align: left;">\${escapeHtml(promptText)}</div>
    </div>
  `;
  document.body.appendChild(backdrop);
}

function parseMarkdown(md) {
  if (window.marked && typeof window.marked.parse === 'function') {
    return window.marked.parse(md);
  }
  
  let html = md;
  // Basic escaping to avoid pure tag injections
  html = html
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  
  // Headers
  html = html.replace(/^# (.*$)/gim, '<h1>$1</h1>');
  html = html.replace(/^## (.*$)/gim, '<h2>$1</h2>');
  html = html.replace(/^### (.*$)/gim, '<h3>$1</h3>');
  
  // Bold & Italic
  html = html.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/\*(.*?)\*/g, '<em>$1</em>');
  
  // Pre blocks & Code blocks
  html = html.replace(/```([\s\S]*?)```/g, '<pre><code>$1</code></pre>');
  html = html.replace(/`([^`\n]+)`/g, '<code>$1</code>');
  
  // Basic Table support
  const lines = html.split('\n');
  let inTable = false;
  let tableRows = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (line.startsWith('|') && line.endsWith('|')) {
      if (!inTable) {
        inTable = true;
        tableRows = [];
      }
      if (line.includes('---')) continue;
      const cells = line.split('|').map(c => c.trim()).filter((c, idx, arr) => idx > 0 && idx < arr.length - 1);
      const isHeader = tableRows.length === 0;
      const cellTag = isHeader ? 'th' : 'td';
      const rowHtml = `<tr>${cells.map(c => `<${cellTag}>${c}</${cellTag}>`).join('')}</tr>`;
      tableRows.push(rowHtml);
      lines[i] = '';
    } else {
      if (inTable) {
        inTable = false;
        lines[i - 1] = `<table class="research-data-table">${tableRows.join('')}</table>`;
      }
    }
  }
  html = lines.filter(l => l !== '').join('\n');
  
  // Lists
  html = html.replace(/^\s*-\s+(.*$)/gim, '<li>$1</li>');
  html = html.replace(/(<li>.*<\/li>)/g, '<ul>$1</ul>');
  
  // Paragraphs
  html = html.replace(/^\s*([^<\n].*)$/gim, '<p>$1</p>');
  
  // Restore basic tags
  html = html
    .replace(/&lt;h1&gt;/gi, '<h1>').replace(/&lt;\/h1&gt;/gi, '</h1>')
    .replace(/&lt;h2&gt;/gi, '<h2>').replace(/&lt;\/h2&gt;/gi, '</h2>')
    .replace(/&lt;h3&gt;/gi, '<h3>').replace(/&lt;\/h3&gt;/gi, '</h3>')
    .replace(/&lt;p&gt;/gi, '<p>').replace(/&lt;\/p&gt;/gi, '</p>')
    .replace(/&lt;ul&gt;/gi, '<ul>').replace(/&lt;\/ul&gt;/gi, '</ul>')
    .replace(/&lt;li&gt;/gi, '<li>').replace(/&lt;\/li&gt;/gi, '</li>')
    .replace(/&lt;strong&gt;/gi, '<strong>').replace(/&lt;\/strong&gt;/gi, '</strong>')
    .replace(/&lt;em&gt;/gi, '<em>').replace(/&lt;\/em&gt;/gi, '</em>')
    .replace(/&lt;code&gt;/gi, '<code>').replace(/&lt;\/code&gt;/gi, '</code>')
    .replace(/&lt;pre&gt;/gi, '<pre>').replace(/&lt;\/pre&gt;/gi, '</pre>')
    .replace(/&lt;table class="research-data-table"&gt;/gi, '<table class="research-data-table">').replace(/&lt;\/table&gt;/gi, '</table>')
    .replace(/&lt;tr&gt;/gi, '<tr>').replace(/&lt;\/tr&gt;/gi, '</tr>')
    .replace(/&lt;th&gt;/gi, '<th>').replace(/&lt;\/th&gt;/gi, '</th>')
    .replace(/&lt;td&gt;/gi, '<td>').replace(/&lt;\/td&gt;/gi, '</td>');
  
  return html;
}

// Load a Fachbericht's content from RxDB (NO HTTP). The reports are the same
// documents that replicate into the `documents` collection over RxDB/WebRTC;
// `index_text` holds the document text, with a blob-chunk fallback — all RxDB.
async function loadReportContentFromRxdb(filename) {
  const raw = state.ctx && state.ctx.db;
  if (!raw || !raw.documents) {
    throw new Error('RxDB-Dokumente nicht verfügbar');
  }
  const matches = await raw.documents.find({ selector: { filename } }).exec();
  const rows = matches.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d));
  const json = rows.find((d) => !d.is_deleted) || rows[0];
  if (!json) {
    throw new Error(`Dokument ${filename} (noch) nicht synchronisiert`);
  }
  if (typeof json.index_text === 'string' && json.index_text.trim()) {
    return json.index_text;
  }
  // Fallback: reconstruct from the current version's blob chunks (RxDB only).
  const versionId = json.current_version_id;
  if (versionId && raw.document_versions && raw.document_blob_chunks) {
    const version = await raw.document_versions.findOne(versionId).exec();
    const blobId = version && typeof version.toJSON === 'function' ? version.toJSON().blob_id : null;
    if (blobId) {
      const chunkDocs = await raw.document_blob_chunks.find({ selector: { blob_id: blobId } }).exec();
      const chunks = chunkDocs
        .map((c) => (typeof c.toJSON === 'function' ? c.toJSON() : c))
        .sort((a, b) => (a.idx || 0) - (b.idx || 0));
      if (chunks.length) {
        const joined = chunks.map((c) => c.data || '').join('');
        try {
          const bytes = Uint8Array.from(atob(joined), (ch) => ch.charCodeAt(0));
          return new TextDecoder('utf-8').decode(bytes);
        } catch {
          return joined;
        }
      }
    }
  }
  throw new Error(`Kein Inhalt für ${filename}`);
}

function renderReportsWorkbench(task) {
  const reports = GENERATED_REPORTS;
  const selectedReportId = state.selectedReportId || reports[0].id;
  const selectedReport = reports.find(r => r.id === selectedReportId) || reports[0];
  
  if (!state.reportContents) {
    state.reportContents = {};
  }
  
  const content = state.reportContents[selectedReport.id];
  if (content === undefined) {
    state.reportContents[selectedReport.id] = null;
    // No HTTP: the Fachberichte ARE the documents that sync into the RxDB
    // `documents` collection over WebRTC. Read the content from RxDB.
    loadReportContentFromRxdb(selectedReport.filename)
      .then(text => {
        state.reportContents[selectedReport.id] = text;
        renderCenter();
      })
      .catch(err => {
        state.reportContents[selectedReport.id] = `Fehler beim Laden des Fachberichts: ${err.message}`;
        renderCenter();
      });
  }
  
  const viewerContent = content === null 
    ? `<div style="padding: 40px; text-align: center; color: var(--research-muted);"><span class="research-spinner" style="display:inline-block; width:18px; height:18px; border:2px solid var(--research-accent); border-top-color:transparent; border-radius:50%; animation:spin 1s linear infinite; margin-right:8px; vertical-align:middle;"></span>Lade Fachbericht...</div>`
    : content.startsWith('Fehler')
      ? `<div style="padding: 40px; text-align: center; color: var(--research-warn);">${escapeHtml(content)}</div>`
      : `
        <div class="ai-warning-banner" style="background: color-mix(in srgb, var(--research-accent) 6%, var(--research-surface)); border: 1px solid color-mix(in srgb, var(--research-accent) 25%, var(--research-line)); border-radius: 8px; padding: 14px 18px; margin-bottom: 20px; box-shadow: 0 4px 12px rgba(0,0,0,0.1);">
          <div style="display: flex; align-items: center; justify-content: space-between; gap: 16px; flex-wrap: wrap;">
            <div style="display: flex; align-items: center; gap: 12px;">
              <span style="font-size: 1.4rem; line-height: 1;">🤖</span>
              <div>
                <div style="font-weight: 700; font-size: 12px; color: var(--research-text);">KI-generierter Fachbericht</div>
                <div style="font-size: 11px; color: var(--research-muted); margin-top: 2px;">Erstellt auf Basis des aggregierten Wälzlager-Wissens (323 Referenzen, 816 Messpunkte).</div>
              </div>
            </div>
            <button type="button" class="research-button primary" onclick="window.showPromptViewer('${selectedReport.filename}')" style="margin: 0; background: color-mix(in srgb, var(--research-accent) 12%, var(--research-surface-2)); border-color: color-mix(in srgb, var(--research-accent) 25%, var(--research-line)); color: var(--research-accent); font-weight: 800; font-size: 11px; padding: 0 12px; height: 28px; border-radius: 6px;">
              Prompt im Modal ⚡
            </button>
          </div>
          <div style="border-top: 1px dashed var(--research-line); padding-top: 10px; margin-top: 10px; text-align: left;">
            <details style="width: 100%;">
              <summary style="font-size: 11px; color: var(--research-accent); font-weight: 700; cursor: pointer; user-select: none; display: inline-flex; align-items: center; gap: 6px;">
                <span>▶ System-Prompt der KI-Generierung einblenden</span>
              </summary>
              <div style="margin-top: 8px; background: var(--research-surface-2); border: 1px solid var(--research-line); border-radius: 6px; padding: 10px; font-family: monospace; font-size: 10px; line-height: 1.5; color: var(--research-text); max-height: 180px; overflow-y: auto; white-space: pre-wrap; box-shadow: inset 0 2px 4px rgba(0,0,0,0.1);">${escapeHtml(getPromptForFilename(selectedReport.filename))}</div>
            </details>
          </div>
        </div>
        <div class="markdown-body">${parseMarkdown(content)}</div>
      `;
      
  return `
    <div class="explorer-layout">
      <div class="explorer-sidebar">
        ${reports.map((report) => {
          const isActive = report.id === selectedReportId;
          return `
            <button type="button" class="doc-item${isActive ? ' active' : ''}" onclick="window.selectReport('${report.id}')">
              <div class="doc-item-cat">${escapeHtml(report.category)}</div>
              <div style="font-weight: 700;">${escapeHtml(report.title)}</div>
              <div style="font-size: 10px; color: var(--research-muted); margin-top: 2px;">
                ${escapeHtml(report.filename)}
              </div>
            </button>
          `;
        }).join('')}
      </div>
      <div class="explorer-viewer" id="markdown-viewer">
        ${viewerContent}
      </div>
    </div>
  `;
}

export const __researchTestHooks = {
  collectionDiagnosticRows,
  diagnosticRows,
  disabledTabButton,
  knowledgeBasesFromTables,
  renderNoTaskCenter,
  validateResearchTaskInput,
  validateSelectedResearchTask,
};
