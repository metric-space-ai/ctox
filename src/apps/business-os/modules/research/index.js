import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';

const BUILD = '20260519-research-dynamic-scoring1';
const DEFAULT_AXIS_X = 'evidence_strength';
const DEFAULT_AXIS_Y = 'topic_fit';
const ROW_LIMIT = 320;
const RESEARCH_LAYOUT_KEY = 'ctox.businessOs.research.columnLayout';
const RESEARCH_COL_MIN = Object.freeze({ left: 260, center: 420, right: 240 });
const RESEARCH_COL_MAX = Object.freeze({ left: 680, right: 520 });
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
  activeTab: 'sources',
  sourcesViewMode: 'shards',
  showDiagram: true,
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
  await startResearchCollections();
  const resizeCleanup = setupResearchColumnResizing();
  if (resizeCleanup) state.cleanup.push(resizeCleanup);
  wireRealtime();
  state.cleanup.push(initResearchContextMenu());
  await refreshAll({ seed: true });
  return () => {
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
  const collections = [
    'business_commands',
    'ctox_queue_tasks',
    'research_tasks',
    'research_runs',
    'research_notes',
    'knowledge_tables',
  ];
  await Promise.allSettled(collections.map((collection) => state.ctx.sync?.startCollection?.(collection)));
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
      resetMapView();
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
  setStatus(state.t('loadingKnowledge', 'Knowledge wird geladen...'));
  state.knowledgeBases = await loadKnowledgeBases();
  await loadLocalState();
  if (seed) await ensureTasksFromKnowledgeBases();
  if (!state.selectedTaskId || !state.tasks.some((task) => task.id === state.selectedTaskId)) {
    state.selectedTaskId = state.tasks[0]?.id || '';
  }
  await loadDashboardData();
  render();
  setStatus('');
}

async function loadLocalState() {
  const [tasks, runs, notes, commands, queueTasks] = await Promise.all([
    findAll(state.ctx.db.raw.research_tasks),
    findAll(state.ctx.db.raw.research_runs),
    findAll(state.ctx.db.raw.research_notes),
    findAll(state.ctx.db.raw.business_commands),
    findAll(state.ctx.db.raw.ctox_queue_tasks),
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
  const raw = state.ctx?.db?.raw || {};
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
    await upsertDoc(state.ctx.db.raw.research_tasks, task).catch((error) => {
      console.warn('[research] could not persist seeded task', error);
    });
    state.tasks.push(task);
  }
}

async function loadKnowledgeBases() {
  const tables = await loadKnowledgeTables();
  const byDomain = new Map();
  for (const rawTable of tables) {
    const table = rawTable?.payload && typeof rawTable.payload === 'object' ? rawTable.payload : rawTable;
    const domain = table.domain || 'knowledge';
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
  return findAll(state.ctx?.db?.raw?.knowledge_tables);
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
          ${state.tasks.map(renderTaskButton).join('') || `<div class="research-empty">${escapeHtml(state.t('noTasksFound', 'Keine Knowledge-basierte Research-Aufgabe gefunden.'))}</div>`}
        </div>
      </section>
      <section class="research-section">
        <div class="research-section-head">
          <strong>${escapeHtml(state.t('ranking', 'Ranking'))}</strong>
          <span>${escapeHtml(axisLabel('portfolio_priority'))}</span>
        </div>
        <div class="research-ranking-list">
          ${state.sourceModels.map(renderRankingRow).join('') || `<div class="research-empty">${escapeHtml(state.t('noSourcesLoaded', 'Noch keine Quellen geladen.'))}</div>`}
        </div>
      </section>
      <section class="research-status-line">${escapeHtml(state.status || task?.knowledge_domain || '')}</section>
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

function renderCenter() {
  const root = pane('center');
  if (!root) return;
  const task = selectedTask();
  if (!task) {
    root.innerHTML = `<div class="research-empty-state"><strong>${escapeHtml(state.t('noResearchTask', 'Keine Research-Aufgabe'))}</strong><span>${escapeHtml(state.t('createTaskBase', 'Lege eine Aufgabe auf Basis einer Knowledge Base an.'))}</span></div>`;
    return;
  }
  const axisPair = normalizedAxisPair(task);
  const xAxis = axisPair.x;
  const yAxis = axisPair.y;
  const isGraphMode = state.mapMode === 'discovery';
  root.innerHTML = `
    <header class="research-pane-header research-center-header">
      <div><span>${escapeHtml(task.knowledge_domain)}</span><h2>${escapeHtml(task.title)}</h2></div>
      <div style="display: flex; align-items: center; gap: 10px;">
        ${state.showDiagram ? `<span class="research-map-hint" style="margin-right: 6px;">Scroll zoom · drag pan</span>` : ''}
        <button type="button"
                class="research-icon-button"
                data-action="toggle-diagram"
                title="${state.showDiagram ? 'Diagramm ausblenden' : 'Diagramm einblenden'}"
                aria-pressed="${!state.showDiagram}">
          ${iconSvg(state.showDiagram ? 'eyeOff' : 'eye')}
        </button>
      </div>
    </header>
    <div class="research-center-body${state.showDiagram ? '' : ' has-hidden-map'}">
      <section class="research-map-panel">
        <div class="research-map-head">
          <div><strong>${isGraphMode ? escapeHtml(state.t('discoveryGraph', 'Discovery Graph')) : escapeHtml(state.t('portfolioMap', 'Portfolio Map'))}</strong><span>${isGraphMode ? escapeHtml(state.t('discoverySub', 'Knowledge, Quellen, Messpunkte')) : `${escapeHtml(axisLabel(yAxis))} ${escapeHtml(state.t('portfolioSub', 'gegen'))} ${escapeHtml(axisLabel(xAxis))}`}</span></div>
          ${mapModeToggle()}
          <button type="button" class="research-map-reset" data-action="reset-map">${escapeHtml(state.t('reset', 'Reset'))}</button>
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
            ${tabButton('sources', state.t('sources', 'Sources'))}
            ${tabButton('measurements', state.t('measurements', 'Measurements'))}
            ${tabButton('knowledge', state.t('knowledge', 'Knowledge'))}
          </div>
          ${state.activeTab === 'sources' ? `
            <div class="research-view-toggle">
              <button type="button"
                      class="research-view-btn${state.sourcesViewMode === 'table' ? ' is-active' : ''}"
                      data-action="sources-view"
                      data-view-mode="table"
                      title="${escapeHtml(state.t('tableView', 'Tabelle'))}">
                ${iconSvg('table')}
              </button>
              <button type="button"
                      class="research-view-btn${state.sourcesViewMode === 'shards' ? ' is-active' : ''}"
                      data-action="sources-view"
                      data-view-mode="shards"
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

function discoveryGraph(task) {
  const base = knowledgeBaseForTask(task);
  const nodes = [];
  const edges = [];
  const pushNode = (node) => {
    if (nodes.some((item) => item.id === node.id)) return;
    nodes.push(node);
  };
  const topSources = state.sourceModels.slice(0, 12);
  const sourceGroups = [...new Set(topSources.map((source) => source.sourceClass || 'source'))].slice(0, 7);
  const sourceLayout = new Map(topSources.map((source, index) => {
    const span = Math.max(topSources.length - 1, 1);
    return [source.id, {
      x: 66 + ((index % 2) * 14),
      y: 12 + (index * (76 / span)),
    }];
  }));
  pushNode({
    id: 'knowledge',
    kind: 'knowledge',
    label: base?.title || task.title,
    title: task.knowledge_domain || task.title,
    meta: `${base?.tables?.length || 0} tables`,
    x: 14,
    y: 50,
  });
  sourceGroups.forEach((group, index) => {
    const groupSources = topSources.filter((source) => source.sourceClass === group);
    const y = groupSources.length
      ? groupSources.reduce((sum, source) => sum + (sourceLayout.get(source.id)?.y || 50), 0) / groupSources.length
      : 18 + (index * (64 / Math.max(sourceGroups.length - 1, 1)));
    const id = `class_${slugId(group)}`;
    pushNode({ id, kind: 'class', label: groupLabel(group), title: group, meta: `${groupSources.length} ${state.t('sourcesLabel', 'Quellen')}`, x: 36, y });
    edges.push({ from: 'knowledge', to: id, kind: 'class' });
  });
  topSources.forEach((source, index) => {
    const group = source.sourceClass || 'source';
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
    edges.push({ from: `class_${slugId(group)}`, to: id, kind: 'source' });
    if (source.measurements?.count && index < 5) {
      const measureId = `measurement_${source.id}`;
      pushNode({
        id: measureId,
        kind: 'measurement',
        label: `${source.measurements.count} ${state.t('measurementsLabel', 'Messpunkte')}`,
        title: `${source.title}: ${source.measurements.count} ${state.t('measurementsLabel', 'Messpunkte')}`,
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
  return renderSourcesWorkbench();
}

function renderSourcesTable(filteredList = state.sourceModels) {
  const task = selectedTask();
  const axisPair = normalizedAxisPair(task);
  const xAxis = axisPair.x;
  const yAxis = axisPair.y;
  return `
    <table class="research-data-table">
      <thead><tr><th>${escapeHtml(state.t('sourceLabel', 'Source'))}</th><th>${escapeHtml(state.t('classLabel', 'Class'))}</th><th>${escapeHtml(state.t('scoreLabel', 'Score'))}</th><th>${escapeHtml(axisLabel(yAxis, task))}</th><th>${escapeHtml(axisLabel(xAxis, task))}</th><th></th></tr></thead>
      <tbody>
        ${filteredList.map((source) => `
          <tr class="${source.id === state.selectedSourceId ? 'is-selected' : ''}">
            <td><button type="button" data-action="select-source" data-source-id="${escapeHtml(source.id)}"><strong>${escapeHtml(source.title)}</strong><span>${escapeHtml(source.id)}</span></button></td>
            <td>${escapeHtml(source.sourceClass)}</td>
            <td><span class="research-score-pill research-grade-${source.grade.toLowerCase()}">${source.grade} · ${(source.score / 10).toFixed(1)}</span></td>
            <td>${Math.round(source.dimensions[yAxis] ?? 0)}</td>
            <td>${Math.round(source.dimensions[xAxis] ?? 0)}</td>
            <td>${source.url ? `<a href="${escapeHtml(source.url)}" target="_blank" rel="noreferrer">${escapeHtml(state.t('openLabel', 'Open'))}</a>` : ''}</td>
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
    <table class="research-data-table">
      <thead><tr><th>${escapeHtml(state.t('sourceLabel', 'Source'))}</th><th>Prop</th><th>RPM</th><th>Axial N</th><th>Radial N</th><th>Method</th></tr></thead>
      <tbody>
        ${state.measurementRows.slice(0, 120).map((row) => `
          <tr>
            <td>${escapeHtml(row.source_id || '')}</td>
            <td>${escapeHtml([row.prop_diameter_in, row.prop_pitch_in].filter(isPresent).join(' x '))}</td>
            <td>${formatNumber(row.rpm)}</td>
            <td>${formatNumber(row.axial_load_N ?? row.thrust_N)}</td>
            <td>${formatNumber(row.radial_load_N)}</td>
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
  root.innerHTML = `
    <header class="research-pane-header">
      <div><span>${escapeHtml(state.t('context', 'Context'))}</span><h2>${escapeHtml(task?.title || 'Research')}</h2></div>
      <button type="button" class="research-button primary" data-action="run-research" ${task ? '' : 'disabled'} title="${escapeHtml(state.t('runHint', 'Systematic Research für dieses Dashboard'))} ${runInfo.hasRun ? escapeHtml(state.t('researchFortsetzen', 'fortsetzen')) : escapeHtml(state.t('researchStarten', 'starten'))}">${runInfo.hasRun ? escapeHtml(state.t('researchFortsetzen', 'Research fortsetzen')) : escapeHtml(state.t('researchStarten', 'Research starten'))}</button>
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
          <strong>${escapeHtml(source.title)}</strong>
          <p>${escapeHtml(source.note || state.t('noSummaryAvailable', 'Keine Zusammenfassung vorhanden.'))}</p>
          <dl class="research-facts">
            <div><dt>${escapeHtml(state.t('gradeLabel', 'Grade'))}</dt><dd>${source.grade} · ${(source.score / 10).toFixed(1)}</dd></div>
            <div><dt>${escapeHtml(axisLabel(axisPair.y, task))}</dt><dd>${Math.round(source.dimensions[axisPair.y] || 0)}</dd></div>
            <div><dt>${escapeHtml(axisLabel(axisPair.x, task))}</dt><dd>${Math.round(source.dimensions[axisPair.x] || 0)}</dd></div>
            <div><dt>${escapeHtml(state.t('evidenceLabel', 'Evidence'))}</dt><dd>${Math.round(source.dimensions.evidence_strength || 0)}</dd></div>
          </dl>
          <button type="button" class="research-button" data-action="source-detail" data-source-id="${escapeHtml(source.id)}">${escapeHtml(state.t('details', 'Details'))}</button>
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

function openTaskDialog(editTask = null) {
  closeTaskDialog();
  const root = state.ctx.host.querySelector('[data-research-root]');
  if (!root) return;
  const isEdit = Boolean(editTask?.id);
  const selectedDomain = editTask?.knowledge_domain || state.knowledgeBases[0]?.domain || '';
  const dimensionsText = formatDimensionLines(scoringDimensionsForTask(editTask));
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
        <label><span>${escapeHtml(state.t('titel', 'Titel'))}</span><input name="title" placeholder="${escapeHtml(state.t('neueResearch', 'Neue Research'))}" value="${escapeHtml(editTask?.title || '')}" required></label>
        <label><span>Knowledge Domain</span><input name="domain" list="research-knowledge-domains" placeholder="research/vendor-ai-agents" value="${escapeHtml(selectedDomain)}" ${isEdit ? 'readonly' : ''} required><datalist id="research-knowledge-domains">${state.knowledgeBases.map((base) => `<option value="${escapeHtml(base.domain)}">${escapeHtml(base.title)}</option>`).join('')}</datalist></label>
        <label><span>${escapeHtml(state.t('auftrag', 'Auftrag'))}</span><textarea name="prompt" placeholder="${escapeHtml(state.t('promptPlaceholder', 'Was soll das Dashboard auswerten?'))}">${escapeHtml(editTask?.prompt || '')}</textarea></label>
        <label><span>${escapeHtml(state.t('kriterien', 'Kriterien'))}</span><textarea name="criteria" placeholder="${escapeHtml(state.t('criteriaPlaceholder', 'Scope, Ausschlüsse, Scoring-Hinweise'))}">${escapeHtml(editTask?.criteria || '')}</textarea></label>
        <label><span>${escapeHtml(state.t('scoringDimensions', 'Scoring Dimensionen'))}</span><textarea name="scoring_dimensions" placeholder="${escapeHtml(state.t('scoringPlaceholder', 'overlap: Overlap\nbuyer_clarity: Buyer clarity'))}">${escapeHtml(dimensionsText)}</textarea></label>
        <footer>
          <button type="button" class="research-button" data-close>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
          <button type="submit" class="research-button primary">${isEdit ? escapeHtml(state.t('save', 'Speichern')) : escapeHtml(state.t('create', 'Anlegen'))}</button>
        </footer>
      </form>
    </section>
  `;
  const close = () => overlay.remove();
  overlay.addEventListener('click', (event) => {
    if (event.target === overlay || event.target.closest('[data-close]')) close();
  });
  overlay.querySelector('[data-research-task-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const submit = event.currentTarget.querySelector('button[type="submit"]');
    submit.disabled = true;
    const form = new FormData(event.currentTarget);
    await createTaskFromForm(form);
    close();
  });
  root.append(overlay);
  requestAnimationFrame(() => overlay.querySelector('input[name="title"]')?.focus());
}

function closeTaskDialog() {
  state.ctx.host.querySelector('.research-modal-backdrop')?.remove();
}

async function createTaskFromForm(form) {
  const taskId = String(form.get('task_id') || '').trim();
  const current = taskId ? state.tasks.find((item) => item.id === taskId) : null;
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
  await upsertDoc(state.ctx.db.raw.research_tasks, task);
  await loadLocalState();
  state.selectedTaskId = task.id;
  await loadDashboardData();
  render();
}

async function runSelectedResearch() {
  const task = selectedTask();
  if (!task) return;
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
  const result = await state.ctx.commandBus.dispatch({
    module: 'research',
    type: 'research.systematic.run',
    record_id: task.id,
    payload: {
      title: `Research · ${task.title}`,
      instruction,
      prompt: instruction,
      priority: 'high',
      required_skills: ['systematic-research'],
      research_mode: 'library+living_dashboard',
      thread_key: `business-os/research/${task.id}`,
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
    },
    client_context: {
      module: 'research',
      source_module: 'research',
      inbound_channel: 'business_os.research',
      knowledge_tables: base?.tables || [],
    },
  });
  const run = {
    id: `research_run_${now}`,
    task_id: task.id,
    status: result?.task_status || result?.status || 'queued',
    command_id: result?.command_id || '',
    task_queue_id: result?.task_id || '',
    identified_count: state.sourceRows.length,
    accepted_count: state.sourceModels.length,
    used_count: state.sourceModels.length,
    payload: { result },
    created_at_ms: now,
    updated_at_ms: now,
  };
  state.runs = [run, ...state.runs.filter((item) => item.id !== run.id)];
  if (result?.task_id) {
    state.queueTasks = [{
      id: result.task_id,
      command_id: result.command_id || '',
      title: `Research · ${task.title}`,
      status: result.task_status || 'queued',
      source_module: 'research',
      command_type: 'research.systematic.run',
      thread_key: `business-os/research/${task.id}`,
      updated_at_ms: now,
    }, ...state.queueTasks.filter((item) => item.id !== result.task_id)];
  }
  await upsertDoc(state.ctx.db.raw.research_runs, run).catch((error) => {
    console.warn('[research] could not persist run', error);
  });
  await patchDoc(state.ctx.db.raw.research_tasks, task.id, { status: 'collecting', updated_at_ms: now }).catch((error) => {
    console.warn('[research] could not patch task status', error);
  });
  render();
}

async function updateTaskAxis(axis, value) {
  const task = selectedTask();
  if (!task) return;
  const patch = axis === 'x' ? { x_axis: safeAxis(value, task) } : { y_axis: safeAxis(value, task) };
  await patchDoc(state.ctx.db.raw.research_tasks, task.id, { ...patch, updated_at_ms: Date.now() });
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

function setupResearchColumnResizing() {
  const root = state.ctx.host.querySelector('[data-research-root]');
  if (!root) return null;

  const leftHandle = root.querySelector('[data-resizer="left"]');
  const rightHandle = root.querySelector('[data-resizer="right"]');
  if (!leftHandle || !rightHandle) return null;

  const widths = readResearchColumnLayout();
  const leftPx = clampNumber(widths.left, RESEARCH_COL_MIN.left, RESEARCH_COL_MAX.left);
  const rightPx = clampNumber(widths.right, RESEARCH_COL_MIN.right, RESEARCH_COL_MAX.right);
  root.style.setProperty('--research-left-width', `${Math.round(leftPx)}px`);
  root.style.setProperty('--research-right-width', `${Math.round(rightPx)}px`);

  const persistWidths = () => {
    try {
      const leftStr = root.style.getPropertyValue('--research-left-width') || `${leftPx}px`;
      const rightStr = root.style.getPropertyValue('--research-right-width') || `${rightPx}px`;
      window.localStorage.setItem(RESEARCH_LAYOUT_KEY, JSON.stringify({
        left: Number.parseFloat(leftStr),
        right: Number.parseFloat(rightStr),
      }));
    } catch (_) {
      /* storage unavailable */
    }
  };

  const resizerL = new CtoxResizer({
    resizerEl: leftHandle,
    containerEl: root,
    cssVar: '--research-left-width',
    side: 'left',
    minWidth: RESEARCH_COL_MIN.left,
    maxWidth: RESEARCH_COL_MAX.left,
    onResize: persistWidths,
  });

  const resizerR = new CtoxResizer({
    resizerEl: rightHandle,
    containerEl: root,
    cssVar: '--research-right-width',
    side: 'right',
    minWidth: RESEARCH_COL_MIN.right,
    maxWidth: RESEARCH_COL_MAX.right,
    onResize: persistWidths,
  });

  return () => {
    resizerL.destroy();
    resizerR.destroy();
  };
}

function readResearchColumnLayout() {
  try {
    const raw = JSON.parse(window.localStorage.getItem(RESEARCH_LAYOUT_KEY) || 'null');
    if (raw && typeof raw === 'object') {
      const left = Number(raw.left);
      const right = Number(raw.right);
      return {
        left: Number.isFinite(left) && left > 0 ? left : 320,
        right: Number.isFinite(right) && right > 0 ? right : 300,
      };
    }
  } catch (_) {
    /* storage unavailable */
  }
  return { left: 320, right: 300 };
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
  const seed = Array.from(String(source.id || source.title || source.rank))
    .reduce((sum, char) => sum + char.charCodeAt(0), 0);
  return {
    x: ((seed % 7) - 3) * 1.4,
    y: (((Math.floor(seed / 7) % 7) - 3) * 1.4),
  };
}

function avgScore() {
  if (!state.sourceModels.length) return '0.0';
  return (state.sourceModels.reduce((sum, item) => sum + item.score, 0) / state.sourceModels.length / 10).toFixed(1);
}

async function findAll(collection) {
  if (!collection?.find) return [];
  try {
    const docs = await withTimeout(collection.find().exec(), 1600, 'collection read timed out');
    return docs.map(toJson);
  } catch (error) {
    console.warn('[research] collection read skipped', error);
    return [];
  }
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

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
