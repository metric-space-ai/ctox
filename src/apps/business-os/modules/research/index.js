import { loadModuleMessages } from '../../shared/i18n.js';
import { buildResearchGraphProjection } from './research-graph-data.mjs';

const BUILD = '20260713-semantic-graph-v1';
const DEFAULT_AXIS_X = 'evidence_strength';
const DEFAULT_AXIS_Y = 'topic_fit';
const ROW_LIMIT = 5000;
const COLLECTION_READ_TIMEOUT_MS = 10000;
const POST_SYNC_REFRESH_LIMIT = 3;
const KNOWLEDGE_TABLE_EMPTY_RETRY_DELAYS_MS = Object.freeze([250, 750, 1500]);
const RESEARCH_COLLECTIONS = Object.freeze([
  'business_commands',
  'ctox_queue_tasks',
  'research_tasks',
  'research_runs',
  'research_notes',
  'knowledge_tables',
  'documents',
  'document_versions',
  'document_blob_chunks',
]);
const RESEARCH_REQUIRED_COLLECTIONS = Object.freeze([
  'research_tasks',
  'research_runs',
  'research_notes',
  'knowledge_tables',
]);
const RESEARCH_OPTIONAL_COLLECTIONS = Object.freeze([
  'business_commands',
  'ctox_queue_tasks',
  'documents',
  'document_versions',
  'document_blob_chunks',
]);
const RESEARCH_DEMAND_ONLY_COLLECTIONS = new Set(['document_blob_chunks']);
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
  semantic_graph_nodes: {
    title: 'Semantic Graph Nodes',
    columns: [
      'node_id',
      'label',
      'kind',
      'cluster_id',
      'occurrences',
      'betweenness_centrality',
      'source_ids_json',
      'provenance_json',
      'updated_at',
    ],
  },
  semantic_graph_edges: {
    title: 'Semantic Graph Edges',
    columns: [
      'edge_id',
      'source_id',
      'target_id',
      'weight',
      'source_ids_json',
      'provenance_json',
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
  documents: [],
  commands: [],
  queueTasks: [],
  knowledgeBases: [],
  selectedTaskId: '',
  selectedSourceId: '',
  selectedReportId: '',
  reportContents: {},
  activeTab: 'sources',
  sourcesViewMode: 'shards',
  showDiagram: true,
  sourceSearchTerm: '',
  sourceActiveTag: 'all',
  mapMode: 'discovery',
  sourceRows: [],
  curatedRows: [],
  measurementRows: [],
  graphNodeRows: [],
  graphEdgeRows: [],
  sourceModels: [],
  graphProjection: null,
  graphSurface: null,
  graphMountToken: 0,
  selectedGraphNodeId: '',
  graph: {
    dimensions: 3,
    visibleLimit: 120,
    layer: 'concepts',
    panel: 'topics',
    query: '',
    autoRotate: true,
    busyAction: '',
    status: 'loading',
  },
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
  initialDataReady: false,
  refreshTimer: null,
  cleanup: [],
  contextMenu: null,
  mountToken: null,
  syncLeases: new Set(),
};

export async function mount(ctx) {
  const mountToken = Symbol('research-mount');
  state.mountToken = mountToken;
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  state.initialDataReady = false;

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
  startResearchCollections(mountToken)
    .then(async () => {
      if (state.mountToken !== mountToken) return;
      await refreshAll({ seed: true, mountToken });
      if (state.mountToken !== mountToken) return;
      state.initialDataReady = true;
      render();
      queueKnowledgeRefreshAfter(300);
      queueKnowledgeRefreshAfter(2500);
      queueKnowledgeRefreshAfter(6500);
    })
    .catch((error) => {
      console.warn('[research] background sync start failed', error);
    });
  // Paint the usable workbench before local queries or an empty-knowledge
  // retry can delay window activation. The background refresh is guarded by
  // this mount token so a late result from a closed instance cannot repaint a
  // subsequently opened window.
  render();
  setStatus(state.t('loadingKnowledge', 'Knowledge wird geladen...'));
  refreshAll({ seed: true, retryEmptyKnowledge: false, mountToken }).catch((error) => {
    if (state.mountToken === mountToken) {
      console.warn('[research] initial background refresh failed', error);
    }
  });
  schedulePostSyncRefresh(1200);
  return () => {
    if (state.mountToken === mountToken) state.mountToken = null;
    for (const lease of state.syncLeases) lease?.release?.().catch?.(() => null);
    state.syncLeases.clear();
    // Cleanup globals
    delete window.selectReport;
    delete window.showPromptViewer;
    
    state.cleanup.forEach((fn) => fn?.());
    state.cleanup = [];
    disposeResearchGraph();
    if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
    state.refreshTimer = null;
    state.contextMenu?.remove();
    state.contextMenu = null;
    ctx.host.replaceChildren();
    if (state.ctx === ctx) state.ctx = null;
  };
}

async function startResearchCollections(mountToken) {
  await Promise.all(RESEARCH_COLLECTIONS.map(async (collection) => {
    if (typeof state.ctx.sync?.startCollection !== 'function') {
      markCollectionDiagnostic(collection, 'sync', 'local', state.t('localOnly', 'Lokaler Modus'));
      return;
    }
    try {
      if (RESEARCH_DEMAND_ONLY_COLLECTIONS.has(collection)) {
        if (typeof state.ctx.sync.leaseCollection !== 'function') {
          throw new Error(`${collection} requires sync.leaseCollection().`);
        }
        const lease = await state.ctx.sync.leaseCollection(collection, 'research-document-blob-sync');
        if (state.mountToken !== mountToken) {
          await lease?.release?.().catch?.(() => null);
          return;
        }
        state.syncLeases.add(lease);
      } else {
        const bridge = await state.ctx.sync.startCollection(collection);
        if (RESEARCH_REQUIRED_COLLECTIONS.includes(collection) && bridge) {
          await waitForReplicationBridge(bridge, collection);
        }
      }
      markCollectionDiagnostic(collection, 'sync', 'ok', state.t('syncReady', 'Sync bereit'));
    } catch (error) {
      markCollectionDiagnostic(collection, 'sync', 'failed', errorMessage(error));
    }
  }));
}

async function waitForReplicationBridge(bridge, collection, timeoutMs = 20000) {
  const bridgeState = bridge?.state;
  const wait = typeof bridgeState?.awaitInSync === 'function'
    ? bridgeState.awaitInSync.bind(bridgeState)
    : typeof bridgeState?.awaitInitialReplication === 'function'
      ? bridgeState.awaitInitialReplication.bind(bridgeState)
      : null;
  if (!wait) return;
  await Promise.race([
    wait(),
    new Promise((_, reject) => {
      window.setTimeout(() => reject(new Error(`${collection} replication did not become ready in time`)), timeoutMs);
    }),
  ]);
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
    } else if (action === 'graph-dimension') {
      state.graph.dimensions = state.graph.dimensions === 3 ? 2 : 3;
      state.graphSurface?.setDimensions?.(state.graph.dimensions);
      target.textContent = `${state.graph.dimensions}D`;
      target.setAttribute('aria-label', state.graph.dimensions === 3 ? state.t('switch2d', 'Zu 2D wechseln') : state.t('switch3d', 'Zu 3D wechseln'));
    } else if (action === 'graph-command') {
      handleGraphCommand(target.dataset.graphCommand || '');
    } else if (action === 'graph-layer') {
      state.graph.layer = target.dataset.graphLayer || 'concepts';
      renderCenter();
    } else if (action === 'graph-panel') {
      state.graph.panel = target.dataset.graphPanel || 'topics';
      updateGraphInsights();
    } else if (action === 'graph-topic') {
      const nodeId = target.dataset.nodeId || '';
      const node = state.graphProjection?.nodes?.find((candidate) => candidate.id === nodeId);
      state.graphSurface?.select?.(nodeId, { focus: true });
      selectGraphNode(node);
    } else if (action === 'graph-ai') {
      await dispatchGraphAiAction(target.dataset.graphAi || 'research');
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
    } else if (action === 'build-knowledge') {
      await buildKnowledgeFromResearch();
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
    const graphLimit = event.target.closest('[data-action="graph-limit"]');
    if (graphLimit) {
      state.graph.visibleLimit = clampNumber(Number(graphLimit.value) || 120, 20, 500);
      renderCenter();
      return;
    }
    const axis = event.target.closest('[data-axis-select]');
    if (!axis) return;
    updateTaskAxis(axis.dataset.axisSelect, axis.value).catch((error) => {
      console.error('[research] axis update failed', error);
    });
  });
  root.addEventListener('input', (event) => {
    const graphSearch = event.target.closest('[data-action="graph-search"]');
    if (graphSearch) {
      state.graph.query = graphSearch.value;
      state.graphSurface?.search?.(graphSearch.value);
      return;
    }
    const graphLimit = event.target.closest('[data-action="graph-limit"]');
    if (graphLimit) {
      const label = graphLimit.closest('.research-graph-limit')?.querySelector('span');
      if (label) label.textContent = graphLimit.value;
      return;
    }
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

async function refreshAll({ seed = false, retryEmptyKnowledge = true, mountToken = null } = {}) {
  state.diagnostics.reloadStartedAt = Date.now();
  state.diagnostics.reloadFinishedAt = 0;
  state.diagnostics.reloadCount += 1;
  setStatus(state.t('loadingKnowledge', 'Knowledge wird geladen...'));
  const knowledgeBases = await loadKnowledgeBases({ retryEmpty: retryEmptyKnowledge });
  if (mountToken && state.mountToken !== mountToken) return;
  state.knowledgeBases = knowledgeBases;
  await loadLocalState({ mountToken });
  if (mountToken && state.mountToken !== mountToken) return;
  if (seed) await ensureTasksFromKnowledgeBases();
  if (mountToken && state.mountToken !== mountToken) return;
  if (!state.selectedTaskId || !state.tasks.some((task) => task.id === state.selectedTaskId)) {
    state.selectedTaskId = state.tasks[0]?.id || '';
  }
  await loadDashboardData();
  if (mountToken && state.mountToken !== mountToken) return;
  render();
  refreshOpenTaskDialogDomainOptions();
  state.diagnostics.reloadFinishedAt = Date.now();
  setStatus(reloadStatusText());
}

async function loadLocalState({ mountToken = null } = {}) {
  const [tasks, runs, notes, commands, queueTasks, documents] = await Promise.all([
    findAll(readableCollection('research_tasks'), 'research_tasks'),
    findAll(readableCollection('research_runs'), 'research_runs'),
    findAll(readableCollection('research_notes'), 'research_notes'),
    findAll(readableCollection('business_commands'), 'business_commands'),
    findAll(readableCollection('ctox_queue_tasks'), 'ctox_queue_tasks'),
    findAll(readableCollection('documents'), 'documents'),
  ]);
  if (mountToken && state.mountToken !== mountToken) return;
  if (tasks.length || !state.tasks.length) {
    state.tasks = tasks
      .filter((task) => isVisibleResearchTask(task))
      .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
  }
  if (runs.length || !state.runs.length) state.runs = runs;
  if (notes.length || !state.notes.length) state.notes = notes;
  if (commands.length || !state.commands.length) state.commands = commands;
  if (queueTasks.length || !state.queueTasks.length) state.queueTasks = queueTasks;
  if (documents.length || !state.documents.length) state.documents = documents;
}

function wireRealtime() {
  const collections = [
    ['research_tasks', readableCollection('research_tasks')],
    ['research_runs', readableCollection('research_runs')],
    ['research_notes', readableCollection('research_notes')],
    ['business_commands', readableCollection('business_commands')],
    ['ctox_queue_tasks', readableCollection('ctox_queue_tasks')],
    ['documents', readableCollection('documents')],
  ].filter(([, collection]) => collection);
  for (const [, collection] of collections) {
    const subscription = collection.$?.subscribe?.(() => scheduleLocalRefresh(80));
    if (subscription?.unsubscribe) state.cleanup.push(() => subscription.unsubscribe());
  }
  const knowledgeSubscription = readableCollection('knowledge_tables')?.$?.subscribe?.(() => scheduleKnowledgeRefresh(120));
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
  const mountToken = state.mountToken;
  state.refreshTimer = window.setTimeout(async () => {
    state.refreshTimer = null;
    if (!mountToken || state.mountToken !== mountToken) return;
    await loadLocalState({ mountToken });
    if (state.mountToken !== mountToken) return;
    render();
  }, delay);
}

function scheduleKnowledgeRefresh(delay = 120) {
  if (state.refreshTimer) window.clearTimeout(state.refreshTimer);
  const mountToken = state.mountToken;
  state.refreshTimer = window.setTimeout(async () => {
    state.refreshTimer = null;
    if (!mountToken || state.mountToken !== mountToken) return;
    const knowledgeBases = await loadKnowledgeBases();
    if (state.mountToken !== mountToken) return;
    state.knowledgeBases = knowledgeBases;
    await loadLocalState({ mountToken });
    if (state.mountToken !== mountToken) return;
    await ensureTasksFromKnowledgeBases();
    if (state.mountToken !== mountToken) return;
    if (!state.selectedTaskId || !state.tasks.some((task) => task.id === state.selectedTaskId)) {
      state.selectedTaskId = state.tasks[0]?.id || '';
    }
    await loadDashboardData();
    if (state.mountToken !== mountToken) return;
    render();
    refreshOpenTaskDialogDomainOptions();
  }, delay);
}

function queueKnowledgeRefreshAfter(delay) {
  const mountToken = state.mountToken;
  const timer = window.setTimeout(() => {
    if (!mountToken || state.mountToken !== mountToken) return;
    refreshAll({ seed: true, mountToken }).catch((error) => {
      console.warn('[research] deferred knowledge refresh failed', error);
    });
  }, delay);
  state.cleanup.push(() => window.clearTimeout(timer));
}

function isVisibleResearchTask(task) {
  if (/^outbound(?:_|$)/.test(String(task.knowledge_domain || ''))) return false;
  if (!task?.payload?.seeded_from_knowledge) return true;
  const base = state.knowledgeBases.find((item) => item.domain === task.knowledge_domain);
  return Boolean(base && isResearchKnowledgeBase(base));
}

async function ensureTasksFromKnowledgeBases() {
  if (!canWriteCollection('research_tasks')) return;
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
        graph_contract: semanticGraphContract(),
        source_table_ids: base.tables.map((table) => table.id),
      },
      created_at_ms: now,
      updated_at_ms: now,
    };
    await upsertDoc(writableCollection('research_tasks'), task).catch((error) => {
      console.warn('[research] could not persist seeded task', error);
    });
    state.tasks.push(task);
  }
}

async function loadKnowledgeBases({ retryEmpty = true } = {}) {
  const tables = await loadKnowledgeTables({ retryEmpty });
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

async function loadKnowledgeTables({ retryEmpty = true } = {}) {
  const collection = readableCollection('knowledge_tables');
  const first = await findAll(collection, 'knowledge_tables');
  if (first.length || !collection?.find) return first;
  if (!retryEmpty || !shouldRetryEmptyKnowledgeTables()) return first;
  for (const delay of KNOWLEDGE_TABLE_EMPTY_RETRY_DELAYS_MS) {
    await sleep(delay);
    const retry = await findAll(collection, 'knowledge_tables');
    if (retry.length) return retry;
  }
  return first;
}

function shouldRetryEmptyKnowledgeTables() {
  const info = window.ctoxBusinessOsSyncDiagnostics?.collections?.knowledge_tables;
  if (!info) return true;
  return info.status === 'connected'
    || info.status === 'reused'
    || info.connectionStatus === 'connected'
    || info.initialReplicationState === 'complete'
    || info.active === true;
}

function isResearchKnowledgeBase(base) {
  if (!base?.tables?.length) return false;
  if (/^outbound(?:_|$)/.test(String(base.domain || ''))) return false;
  const text = [base.domain, base.title, base.description, ...base.tables.flatMap((table) => [table.table_key, table.title, table.description])].join(' ').toLowerCase();
  return /research|source|catalog|load|bearing|measurement|evidence|market|competitive|portfolio/.test(text);
}

function researchCollection(name) {
  const db = state.ctx?.db;
  if (!db || !name) return null;
  return db.collection?.(name) || null;
}

function readableCollection(name) {
  if (!name) return null;
  const permissionCheck = state.ctx?.permissions?.canReadCollection;
  if (typeof permissionCheck === 'function' && !permissionCheck(name)) {
    return null;
  }
  return researchCollection(name);
}

function writableCollection(name) {
  if (!canWriteCollection(name)) return null;
  return researchCollection(name);
}

function canReadCollection(name) {
  const permissionCheck = state.ctx?.permissions?.canReadCollection;
  return typeof permissionCheck !== 'function' || permissionCheck(name) === true;
}

function canWriteCollection(name) {
  const permissionCheck = state.ctx?.permissions?.canWriteCollection;
  return typeof permissionCheck !== 'function' || permissionCheck(name) === true;
}

function canWriteResearchState() {
  return canWriteCollection('research_tasks') && canWriteCollection('research_runs');
}

function researchWriteDeniedMessage() {
  return state.t('researchWriteDenied', 'Du kannst Research-Daten lesen, aber hier keine Research-Aufgaben ändern.');
}

function isOptionalResearchCollection(collectionName) {
  return RESEARCH_OPTIONAL_COLLECTIONS.includes(collectionName);
}

function isBusinessOsPermissionDenied(error) {
  return error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
    || error?.name === 'BusinessOsPermissionError';
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
  state.graphNodeRows = [];
  state.graphEdgeRows = [];
  state.sourceModels = [];
  state.graphProjection = null;
  if (!task) return;
  const base = knowledgeBaseForTask(task);
  const sourceTable = tableForKey(base, task.source_catalog_key) || firstTableMatching(base, /source|catalog|curated/i);
  const curatedTable = tableForKey(base, task.curated_table_key) || firstTableMatching(base, /library|curated/i);
  const measurementTable = tableForKey(base, task.measurements_table_key) || firstTableMatching(base, /measure|load|point/i);
  const graphNodeTable = tableForKey(base, task.payload?.graph_contract?.nodes_table_key || 'semantic_graph_nodes') || firstTableMatching(base, /semantic.*graph.*node|concept.*node/i);
  const graphEdgeTable = tableForKey(base, task.payload?.graph_contract?.edges_table_key || 'semantic_graph_edges') || firstTableMatching(base, /semantic.*graph.*edge|concept.*edge/i);
  const [sourceRows, curatedRows, measurementRows, graphNodeRows, graphEdgeRows] = await Promise.all([
    sourceTable ? fetchTableRows(sourceTable.id) : Promise.resolve([]),
    curatedTable && curatedTable.id !== sourceTable?.id ? fetchTableRows(curatedTable.id) : Promise.resolve([]),
    measurementTable && measurementTable.id !== sourceTable?.id && measurementTable.id !== curatedTable?.id ? fetchTableRows(measurementTable.id) : Promise.resolve([]),
    graphNodeTable ? fetchTableRows(graphNodeTable.id) : Promise.resolve([]),
    graphEdgeTable ? fetchTableRows(graphEdgeTable.id) : Promise.resolve([]),
  ]);
  state.sourceRows = sourceRows;
  state.curatedRows = curatedRows;
  state.measurementRows = measurementRows;
  state.graphNodeRows = graphNodeRows;
  state.graphEdgeRows = graphEdgeRows;
  state.sourceModels = buildSourceModels(task, sourceRows, curatedRows, measurementRows);
  const evidenceMeasurementRows = filterMeasurementRowsForEvidence(measurementRows, state.sourceModels);
  const evidenceGraphRows = filterGraphRowsForEvidence(graphNodeRows, graphEdgeRows, evidenceSourceIds(state.sourceModels));
  state.graphProjection = buildResearchGraphProjection({
    task,
    sourceModels: evidenceSourceModels(state.sourceModels),
    measurementRows: evidenceMeasurementRows,
    graphNodeRows: evidenceGraphRows.nodes,
    graphEdgeRows: evidenceGraphRows.edges,
    graphLayer: state.graph.layer,
    visibleLimit: state.graph.visibleLimit,
  });
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
  const curatedBySource = new Map();
  for (const row of curatedRows) {
    const id = sourceId(row);
    if (id) curatedBySource.set(id, row);
  }
  const raw = sourceRows.length ? sourceRows : curatedRows;
  const verifiedIds = new Set(raw.filter((row) => evidenceGate(row).eligible).map(sourceId).filter(Boolean));
  const measurementAgg = aggregateMeasurements((measurementRows || []).filter((row) => verifiedIds.has(sourceId(row))));
  return raw.map((row, index) => {
    const id = sourceId(row) || `source_${index + 1}`;
    const title = firstString(row, ['title', 'source_title', 'name']) || `Source ${index + 1}`;
    const sourceClass = firstString(row, ['source_class', 'type', 'bucket', 'record_type']) || 'source';
    const note = firstString(row, ['contribution_note', 'contribution', 'summary', 'relevance_to_bearing_design', 'use']) || '';
    const curated = curatedBySource.get(id);
    const agg = measurementAgg.get(id) || null;
    const axisDefs = scoringDimensionsForTask(task);
    const gate = evidenceGate(row);
    const dimensions = gate.eligible
      ? scoreDimensions(row, curated, agg, task, axisDefs)
      : emptyScoreDimensions(axisDefs);
    return {
      id,
      rank: index + 1,
      title,
      subtitle: sourceClass,
      url: firstString(row, ['source_url', 'url', 'direct_url', 'doi']) || '',
      canonicalUrl: firstString(row, ['canonical_url']) || '',
      sourceClass,
      note,
      row,
      curated,
      measurements: agg,
      evidenceEligible: gate.eligible,
      evidenceStatus: gate.status,
      evidenceStatusLabel: gate.label,
      dimensions,
      score: gate.eligible ? dimensions.portfolio_priority : null,
      grade: gate.eligible ? gradeForScore(dimensions.portfolio_priority) : '—',
    };
  }).sort((a, b) => {
    if (a.evidenceEligible !== b.evidenceEligible) return a.evidenceEligible ? -1 : 1;
    if (a.evidenceEligible) return b.score - a.score;
    return 0;
  }).map((item, index, items) => ({
    ...item,
    rank: item.evidenceEligible ? items.slice(0, index + 1).filter((candidate) => candidate.evidenceEligible).length : null,
  }));
}

function evidenceRankedSources() {
  return state.sourceModels.filter((source) => source.evidenceEligible);
}

function evidenceSourceModels(sourceModels = state.sourceModels) {
  return sourceModels.filter((source) => source.evidenceEligible);
}

function evidenceSourceIds(sourceModels = state.sourceModels) {
  return new Set(evidenceSourceModels(sourceModels).map((source) => source.id));
}

function filterMeasurementRowsForEvidence(rows, sourceModels = state.sourceModels) {
  const eligibleIds = evidenceSourceIds(sourceModels);
  return (rows || []).filter((row) => eligibleIds.has(sourceId(row)));
}

function filterGraphRowsForEvidence(nodeRows, edgeRows, eligibleIds) {
  const nodes = (nodeRows || []).map((row) => {
    const nodeId = firstString(row, ['node_id', 'id', 'concept_id', 'key']);
    const explicitSourceId = nodeId.startsWith('source:') ? nodeId.slice('source:'.length) : '';
    const sourceIds = graphSourceIds(row);
    const filteredSourceIds = sourceIds.filter((id) => eligibleIds.has(id));
    if (explicitSourceId && !eligibleIds.has(explicitSourceId)) return null;
    if (sourceIds.length && !filteredSourceIds.length) return null;
    if (!sourceIds.length || filteredSourceIds.length === sourceIds.length) return row;
    return { ...row, source_ids_json: JSON.stringify(filteredSourceIds) };
  }).filter(Boolean);
  const edges = (edgeRows || []).filter((row) => {
    const sourceIds = graphSourceIds(row);
    return !sourceIds.length || sourceIds.some((id) => eligibleIds.has(id));
  }).map((row) => {
    const sourceIds = graphSourceIds(row);
    if (!sourceIds.length || sourceIds.every((id) => eligibleIds.has(id))) return row;
    return { ...row, source_ids_json: JSON.stringify(sourceIds.filter((id) => eligibleIds.has(id))) };
  });
  return { nodes, edges };
}

function graphSourceIds(row) {
  const raw = row?.source_ids_json ?? row?.source_ids ?? row?.sources;
  if (Array.isArray(raw)) return raw.map(String).filter(Boolean);
  if (typeof raw !== 'string' || !raw.trim()) return [];
  try {
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) return parsed.map(String).filter(Boolean);
  } catch {}
  return raw.split(/[,;|]/).map((value) => value.trim()).filter(Boolean);
}

function evidenceGate(row) {
  const verificationStatus = firstString(row, ['verification_status']).toLowerCase();
  const httpStatus = Number(row?.http_status);
  const snapshotHash = firstString(row, ['snapshot_hash']);
  const canonicalUrl = firstString(row, ['canonical_url']);
  const sourceTier = firstString(row, ['source_tier']).toLowerCase();
  const sourceType = firstString(row, ['source_type', 'type']).toLowerCase();
  const rejectionReason = firstString(row, ['evidence_rejection_reason']);
  const relevanceScore = Number(row?.evidence_relevance_score);
  const validSnapshotHash = /^sha256:[0-9a-f]{64}$/i.test(snapshotHash);
  const actualSourceContent = row?.actual_full_text_or_data === true;
  const relevant = Number.isInteger(relevanceScore) && relevanceScore >= 8;
  const canonicalIsMetadata = isMetadataCanonicalUrl(canonicalUrl);
  const metadataOnly = row?.metadata_only === true
    || firstString(row, ['reading_status', 'source_status', 'review_status', 'status']).toLowerCase() === 'metadata_only'
    || firstString(row, ['source_type', 'type']).toLowerCase() === 'paper_metadata';
  const rejected = ['relevance_status', 'screening_status', 'review_status', 'source_status', 'status']
    .map((key) => firstString(row, [key]).toLowerCase())
    .some((value) => ['rejected', 'off_topic', 'off-topic', 'fachfremd', 'irrelevant'].includes(value));
  const aggregated = /aggregat|rollup|derived|synthes|summary/.test(sourceTier)
    || sourceType === 'aggregator';
  const eligible = verificationStatus === 'verified'
    && row?.transport_verified === true
    && row?.content_extracted === true
    && Number.isInteger(httpStatus)
    && httpStatus >= 200
    && httpStatus < 300
    && httpStatus !== 204
    && validSnapshotHash
    && Boolean(canonicalUrl)
    && !canonicalIsMetadata
    && row?.evidence_eligible === true
    && actualSourceContent
    && relevant
    && !rejectionReason
    && Boolean(sourceTier)
    && !aggregated
    && !metadataOnly
    && !rejected;

  if (eligible) return { eligible: true, status: 'verified', label: 'Verifiziert' };
  if (metadataOnly) return { eligible: false, status: 'metadata_only', label: 'Metadata only' };
  if (rejected) return { eligible: false, status: 'rejected', label: 'Rejected / off-topic' };
  if (Number.isFinite(httpStatus) && (httpStatus < 200 || httpStatus >= 300)) {
    return { eligible: false, status: 'http_error', label: `HTTP ${httpStatus}` };
  }
  if (aggregated) return { eligible: false, status: 'aggregated', label: 'Aggregated source' };
  if (canonicalIsMetadata) return { eligible: false, status: 'metadata_url', label: 'Metadata URL only' };
  if (verificationStatus !== 'verified') return { eligible: false, status: 'unverified', label: 'Not verified' };
  if (row?.transport_verified !== true) return { eligible: false, status: 'transport_unverified', label: 'Transport not verified' };
  if (row?.content_extracted !== true) return { eligible: false, status: 'empty_content', label: 'No source content extracted' };
  if (!validSnapshotHash) return { eligible: false, status: 'missing_snapshot', label: 'Valid snapshot missing' };
  if (!canonicalUrl) return { eligible: false, status: 'missing_canonical_url', label: 'Canonical source missing' };
  if (!actualSourceContent) return { eligible: false, status: 'no_primary_content', label: 'No full text or original data' };
  if (!relevant) return { eligible: false, status: 'insufficient_relevance', label: 'Relevance not verified' };
  if (rejectionReason) return { eligible: false, status: 'rejected', label: 'Evidence rejected' };
  if (row?.evidence_eligible !== true) return { eligible: false, status: 'not_eligible', label: 'Evidence not eligible' };
  if (!sourceTier) return { eligible: false, status: 'legacy', label: 'Legacy / not verified' };
  return { eligible: false, status: 'not_eligible', label: 'Evidence not eligible' };
}

function isMetadataCanonicalUrl(raw) {
  const normalized = String(raw || '').trim().toLowerCase();
  return [
    'https://doi.org/',
    'http://doi.org/',
    'https://api.crossref.org/',
    'https://api.openalex.org/',
    'https://api.semanticscholar.org/',
    'https://www.semanticscholar.org/',
    'https://scholar.google.',
    'https://www.researchgate.net/',
    'https://www.academia.edu/',
  ].some((prefix) => normalized.startsWith(prefix));
}

function emptyScoreDimensions(axisDefs = BASE_AXES) {
  return Object.fromEntries([...new Set([
    ...BASE_AXES,
    ...BEARING_AXES,
    ...COMPETITIVE_AI_AXES,
    ...(axisDefs || []),
  ].map((axis) => axis.id))].map((id) => [id, null]));
}

function aggregateMeasurements(rows) {
  const bySource = new Map();
  for (const row of rows || []) {
    const id = sourceId(row);
    if (!id) continue;
    const current = bySource.get(id) || {
      count: 0,
      maxAxial: 0,
      maxTangentialEquivalent: 0,
      maxRpm: 0,
      files: new Set(),
    };
    current.count += 1;
    current.maxAxial = Math.max(current.maxAxial, numberValue(row.force_N ?? row.axial_load_N ?? row.thrust_N));
    current.maxTangentialEquivalent = Math.max(current.maxTangentialEquivalent, numberValue(tangentialEquivalentForce(row)));
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
  const rankedSources = evidenceRankedSources();
  root.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('webResearch', 'Web Research'))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(state.t('knowledgeDashboards', 'Knowledge Dashboards'))}</h2>
        </div>
        <div class="ctox-pane-actions">
          <button type="button" class="ctox-pane-icon" data-action="refresh" aria-label="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}" title="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}">${iconSvg('refresh')}</button>
          <button type="button" class="ctox-pane-icon" data-action="new-task" aria-label="${escapeHtml(state.t('createResearch', 'Research anlegen'))}" title="${escapeHtml(state.t('createResearch', 'Research anlegen'))}">${iconSvg('plus')}</button>
        </div>
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
      <section class="research-section">
        <div class="research-section-head">
          <strong>${escapeHtml(state.t('evidenceRanking', 'Evidence-Ranking'))}</strong>
          <span>${rankedSources.length} ${escapeHtml(state.t('verified', 'verifiziert'))}</span>
        </div>
        <div class="research-ranking-list">
          ${rankedSources.map(renderRankingRow).join('') || `<div class="research-empty">${escapeHtml(state.t('noVerifiedSources', 'Keine verifizierten Quellen verfügbar. Discovery-Kandidaten bleiben ohne Evidence-Score.'))}</div>`}
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
      <span class="ctox-badge ${gradeBadgeClass(source.grade)}">${source.grade}</span>
      <span class="research-score">${formatPortfolioScore(source.score)}</span>
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
  if (!state.initialDataReady) {
    return {
      title: state.t('loadingKnowledge', 'Knowledge wird geladen...'),
      body: state.t('syncingResearchData', 'Research-Daten werden mit dieser Instanz synchronisiert.'),
    };
  }
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
    disposeResearchGraph();
    root.innerHTML = renderNoTaskCenter();
    return;
  }
  const projection = currentGraphProjection(task);
  root.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band research-center-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(task.knowledge_domain)}</span>
          <h2 class="ctox-pane-title">${escapeHtml(task.title)}</h2>
        </div>
        <div class="ctox-pane-actions">
          ${state.showDiagram ? `<span class="research-map-hint">${escapeHtml(state.t('graphNavigationHint', 'Ziehen: drehen · Scrollen: zoomen'))}</span>` : ''}
          <button type="button"
                  class="ctox-pane-icon${state.showDiagram ? ' is-active' : ''}"
                  data-action="toggle-diagram"
                  title="${state.showDiagram ? 'Diagramm ausblenden' : 'Diagramm einblenden'}"
                  aria-label="${state.showDiagram ? 'Diagramm ausblenden' : 'Diagramm einblenden'}"
                  aria-pressed="${!state.showDiagram}">
            ${iconSvg('eye')}
          </button>
        </div>
      </div>
    </header>
    <div class="research-center-body${state.showDiagram ? '' : ' has-hidden-map'}">
      ${renderSemanticGraph(task, projection)}
      <section class="research-workbench">
        <div class="research-tabs-container">
          <div class="ctox-pane-tabs" role="tablist" aria-label="Research views">
            ${tabButton('sources', `${state.t('sources', 'Sources')} (${state.sourceModels.length})`)}
            ${tabButton('measurements', `${state.t('measurements', 'Measurements')} (${state.measurementRows.length})`)}
            ${tabButton('knowledge', `${state.t('knowledge', 'Knowledge')} (${state.curatedRows.length})`)}
            ${tabButton('reports', `${state.t('reports', 'Fachberichte')} (${researchReportsForTask(task).length})`)}
          </div>
          ${state.activeTab === 'sources' ? `
            <div class="ctox-pane-tabs research-view-toggle">
              <button type="button"
                      class="ctox-pane-tab${state.sourcesViewMode === 'table' ? ' is-active' : ''}"
                      data-action="sources-view"
                      data-view-mode="table"
                      aria-label="${escapeHtml(state.t('tableView', 'Tabelle'))}"
                      title="${escapeHtml(state.t('tableView', 'Tabelle'))}">
                ${iconSvg('table')}
              </button>
              <button type="button"
                      class="ctox-pane-tab${state.sourcesViewMode === 'shards' ? ' is-active' : ''}"
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
  if (state.showDiagram) scheduleResearchGraphMount(task, projection);
  else disposeResearchGraph();
}

function renderSemanticGraph(task, projection) {
  const metrics = projection.metrics || {};
  const maximum = Math.max(20, Math.min(500, projection.availableNodeCount || state.graph.visibleLimit));
  const runInfo = researchRunInfo(task);
  const live = ['queued', 'running'].includes(runInfo.statusKind);
  return `
    <section class="research-graph-shell" aria-label="${escapeHtml(state.t('semanticResearchGraph', 'Semantischer Research-Graph'))}">
      <div class="research-graph-stage">
        <div class="research-graph-canvas" data-research-graph-host role="img" aria-label="${escapeHtml(state.t('graphCanvasLabel', 'Interaktiver semantischer Graph. Ziehen dreht die Szene, Scrollen zoomt.'))}"></div>
        <div class="research-graph-loading" data-research-graph-loading role="status">
          <span class="research-spinner" aria-hidden="true"></span>
          <span>${escapeHtml(state.t('graphLoading', 'Semantischen Graph aufbauen …'))}</span>
        </div>
        <div class="research-graph-meta">
          <div>
            <strong>${escapeHtml(state.t('semanticResearchGraph', 'Semantic Research Graph'))}</strong>
            <span>${metrics.nodeCount || 0} ${escapeHtml(state.t('concepts', 'Begriffe'))} · ${metrics.linkCount || 0} ${escapeHtml(state.t('relations', 'Beziehungen'))} · ${metrics.clusterCount || 0} ${escapeHtml(state.t('clusters', 'Cluster'))}</span>
          </div>
          <span class="research-graph-live${live ? ' is-live' : ''}">${live ? escapeHtml(state.t('liveRun', 'LIVE · CTOX aktualisiert')) : escapeHtml(projection.origin === 'persisted' ? state.t('persistedGraph', 'Knowledge Graph') : state.t('derivedGraph', 'Live projection'))}</span>
        </div>
        <label class="research-graph-search">
          <span class="research-sr-only">${escapeHtml(state.t('searchGraph', 'Graph durchsuchen'))}</span>
          ${iconSvg('search')}
          <input type="search" data-action="graph-search" value="${escapeHtml(state.graph.query)}" placeholder="${escapeHtml(state.t('searchConcepts', 'Begriff suchen …'))}" autocomplete="off" />
        </label>
        <div class="research-graph-rail" aria-label="${escapeHtml(state.t('graphControls', 'Graph-Steuerung'))}">
          <button type="button" data-action="graph-command" data-graph-command="panel" class="research-graph-tool${state.graph.panel !== 'hidden' ? ' is-active' : ''}" aria-label="${escapeHtml(state.t('toggleInsights', 'Insights ein-/ausblenden'))}" title="${escapeHtml(state.t('toggleInsights', 'Insights ein-/ausblenden'))}">${iconSvg('layers')}</button>
          <button type="button" data-action="graph-command" data-graph-command="reset" class="research-graph-tool" aria-label="${escapeHtml(state.t('resetGraph', 'Ansicht zurücksetzen'))}" title="${escapeHtml(state.t('resetGraph', 'Ansicht zurücksetzen'))}">${iconSvg('refresh')}</button>
          <label class="research-graph-limit" title="${escapeHtml(state.t('topWordsShown', 'Angezeigte Top-Begriffe'))}">
            <span>${state.graph.visibleLimit}</span>
            <input type="range" data-action="graph-limit" min="20" max="${maximum}" step="10" value="${Math.min(maximum, state.graph.visibleLimit)}" aria-label="${escapeHtml(state.t('topWordsShown', 'Angezeigte Top-Begriffe'))}" />
          </label>
          <button type="button" data-action="graph-dimension" class="research-graph-tool research-graph-dimension" aria-label="${state.graph.dimensions === 3 ? escapeHtml(state.t('switch2d', 'Zu 2D wechseln')) : escapeHtml(state.t('switch3d', 'Zu 3D wechseln'))}" title="${state.graph.dimensions === 3 ? escapeHtml(state.t('switch2d', 'Zu 2D wechseln')) : escapeHtml(state.t('switch3d', 'Zu 3D wechseln'))}">${state.graph.dimensions}D</button>
          <button type="button" data-action="graph-command" data-graph-command="rotate" class="research-graph-tool${state.graph.autoRotate ? ' is-active' : ''}" aria-pressed="${state.graph.autoRotate}" aria-label="${escapeHtml(state.t('autoRotate', 'Automatische Rotation'))}" title="${escapeHtml(state.t('autoRotate', 'Automatische Rotation'))}">${iconSvg('refresh')}</button>
          <button type="button" data-action="graph-command" data-graph-command="zoom-in" class="research-graph-tool" aria-label="${escapeHtml(state.t('zoomIn', 'Vergrößern'))}" title="${escapeHtml(state.t('zoomIn', 'Vergrößern'))}">+</button>
          <button type="button" data-action="graph-command" data-graph-command="zoom-out" class="research-graph-tool" aria-label="${escapeHtml(state.t('zoomOut', 'Verkleinern'))}" title="${escapeHtml(state.t('zoomOut', 'Verkleinern'))}">−</button>
          <button type="button" data-action="graph-command" data-graph-command="fit" class="research-graph-tool" aria-label="${escapeHtml(state.t('fitGraph', 'Graph einpassen'))}" title="${escapeHtml(state.t('fitGraph', 'Graph einpassen'))}">${iconSvg('focus')}</button>
        </div>
        <div class="research-graph-layer-switch" role="group" aria-label="${escapeHtml(state.t('graphLayer', 'Graph-Ebene'))}">
          ${graphLayerButton('concepts', state.t('concepts', 'Begriffe'))}
          ${graphLayerButton('sources', state.t('sources', 'Quellen'))}
          ${graphLayerButton('evidence', state.t('evidence', 'Belege'))}
        </div>
        ${state.graph.panel === 'hidden' ? '' : renderGraphInsights(projection)}
        <div class="research-graph-actions">
          <button type="button" class="research-graph-action" data-action="graph-ai" data-graph-ai="research" ${state.graph.busyAction ? 'disabled' : ''}>${iconSvg('search')}<span>${escapeHtml(state.t('targetedResearch', 'Nachrecherche'))}</span></button>
          <button type="button" class="research-graph-action" data-action="graph-ai" data-graph-ai="document" ${state.graph.busyAction || !evidenceRankedSources().length ? 'disabled' : ''}>${iconSvg('file')}<span>${escapeHtml(evidenceRankedSources().length ? state.t('createDocument', 'Dokument erstellen') : state.t('reportUnavailable', 'Report nicht verfügbar'))}</span></button>
        </div>
      </div>
    </section>
  `;
}

function graphLayerButton(id, label) {
  return `<button type="button" data-action="graph-layer" data-graph-layer="${id}" class="${state.graph.layer === id ? 'is-active' : ''}" aria-pressed="${state.graph.layer === id}">${escapeHtml(label)}</button>`;
}

function renderGraphInsights(projection) {
  const metrics = projection.metrics || {};
  const body = state.graph.panel === 'analytics'
    ? `
      <dl class="research-graph-metrics">
        <div><dt>${escapeHtml(state.t('nodes', 'Knoten'))}</dt><dd>${metrics.nodeCount || 0}</dd></div>
        <div><dt>${escapeHtml(state.t('relations', 'Beziehungen'))}</dt><dd>${metrics.linkCount || 0}</dd></div>
        <div><dt>${escapeHtml(state.t('clusters', 'Cluster'))}</dt><dd>${metrics.clusterCount || 0}</dd></div>
        <div><dt>${escapeHtml(state.t('sources', 'Quellen'))}</dt><dd>${metrics.sourceCount || 0}</dd></div>
      </dl>
      <p>${escapeHtml(state.t('graphMethod', 'Größe: Betweenness-Zentralität · Farbe: automatische Community · Kante: gemeinsame Nennung'))}</p>
    `
    : `<ol class="research-graph-topics">${(projection.topics || []).slice(0, 6).map((topic, index) => `
        <li>
          <button type="button" data-action="graph-topic" data-node-id="${escapeHtml(topic.nodeId || '')}" style="--topic-color:${escapeHtml(topic.color)}">
            <span>${String(index + 1).padStart(2, '0')}</span><strong>${escapeHtml(topic.label)}</strong><small>${topic.nodeCount}</small>
          </button>
        </li>
      `).join('')}</ol>`;
  return `
    <aside class="research-graph-insights">
      <div class="research-graph-insights-tabs" role="tablist" aria-label="Graph insights">
        <button type="button" data-action="graph-panel" data-graph-panel="topics" class="${state.graph.panel === 'topics' ? 'is-active' : ''}" role="tab" aria-selected="${state.graph.panel === 'topics'}">${escapeHtml(state.t('topics', 'Topics'))}</button>
        <button type="button" data-action="graph-panel" data-graph-panel="analytics" class="${state.graph.panel === 'analytics' ? 'is-active' : ''}" role="tab" aria-selected="${state.graph.panel === 'analytics'}">${escapeHtml(state.t('analytics', 'Analytics'))}</button>
      </div>
      ${body}
    </aside>
  `;
}

function currentGraphProjection(task = selectedTask()) {
  const evidenceGraphRows = filterGraphRowsForEvidence(state.graphNodeRows, state.graphEdgeRows, evidenceSourceIds(state.sourceModels));
  const projection = buildResearchGraphProjection({
    task,
    sourceModels: evidenceSourceModels(state.sourceModels),
    measurementRows: filterMeasurementRowsForEvidence(state.measurementRows),
    graphNodeRows: evidenceGraphRows.nodes,
    graphEdgeRows: evidenceGraphRows.edges,
    graphLayer: state.graph.layer,
    visibleLimit: state.graph.visibleLimit,
  });
  state.graphProjection = projection;
  return projection;
}

async function scheduleResearchGraphMount(task, projection) {
  const root = pane('center');
  const host = root?.querySelector('[data-research-graph-host]');
  if (!host || !state.showDiagram) return;
  disposeResearchGraph();
  const token = ++state.graphMountToken;
  const loading = root.querySelector('[data-research-graph-loading]');
  if (!projection.nodes.length) {
    if (loading) loading.innerHTML = `<span>${escapeHtml(state.t('graphNoData', 'Noch keine Begriffe. Starte eine Nachrecherche oder füge Quellen hinzu.'))}</span>`;
    return;
  }
  try {
    const moduleUrl = new URL('./research-graph.mjs', import.meta.url);
    moduleUrl.search = new URL(import.meta.url).search;
    const graphModule = await import(moduleUrl.href);
    if (token !== state.graphMountToken || !host.isConnected || selectedTask()?.id !== task.id) return;
    state.graphSurface = graphModule.createResearchGraph(host, {
      projection,
      dimensions: state.graph.dimensions,
      autoRotate: state.graph.autoRotate,
      onNodeClick(node) {
        selectGraphNode(node);
      },
      onBackgroundClick() {
        state.selectedGraphNodeId = '';
      },
      onSettled() {
        state.graph.status = 'ready';
        loading?.remove();
      },
    });
    state.graph.status = 'ready';
    loading?.remove();
  } catch (error) {
    if (token !== state.graphMountToken || !host.isConnected) return;
    state.graph.status = 'failed';
    console.error('[research] semantic graph mount failed', error);
    const message = errorMessage(error);
    if (loading) {
      loading.classList.add('is-error');
      loading.innerHTML = `
        <strong>${escapeHtml(state.t('graphUnavailable', '3D-Graph nicht verfügbar'))}</strong>
        <span>${escapeHtml(message)}</span>
        <button type="button" class="ctox-button" data-action="graph-command" data-graph-command="retry">${escapeHtml(state.t('retry', 'Erneut versuchen'))}</button>
      `;
    }
  }
}

function disposeResearchGraph() {
  state.graphMountToken += 1;
  state.graphSurface?.dispose?.();
  state.graphSurface = null;
}

function selectGraphNode(node) {
  if (!node) return;
  state.selectedGraphNodeId = node.id || '';
  const sourceId = (node.sourceIds || []).find((id) => state.sourceModels.some((source) => source.id === id));
  if (sourceId) {
    state.selectedSourceId = sourceId;
    renderLeft();
    renderRight();
  }
}

function handleGraphCommand(command) {
  if (command === 'panel') {
    state.graph.panel = state.graph.panel === 'hidden' ? 'topics' : 'hidden';
    updateGraphInsights();
    return;
  }
  if (command === 'retry') {
    renderCenter();
    return;
  }
  if (command === 'rotate') {
    state.graph.autoRotate = !state.graph.autoRotate;
    state.graphSurface?.setAutoRotate?.(state.graph.autoRotate);
    const button = pane('center')?.querySelector('[data-graph-command="rotate"]');
    button?.classList.toggle('is-active', state.graph.autoRotate);
    button?.setAttribute('aria-pressed', String(state.graph.autoRotate));
    return;
  }
  if (command === 'zoom-in') state.graphSurface?.zoomIn?.();
  else if (command === 'zoom-out') state.graphSurface?.zoomOut?.();
  else if (command === 'fit') state.graphSurface?.fit?.();
  else if (command === 'reset') state.graphSurface?.reset?.();
}

function updateGraphInsights() {
  const center = pane('center');
  const stage = center?.querySelector('.research-graph-stage');
  const existing = stage?.querySelector('.research-graph-insights');
  const toggle = stage?.querySelector('[data-graph-command="panel"]');
  if (!stage || !state.graphProjection) return;
  if (state.graph.panel === 'hidden') {
    existing?.remove();
    toggle?.classList.remove('is-active');
    return;
  }
  const markup = renderGraphInsights(state.graphProjection);
  if (existing) existing.outerHTML = markup;
  else stage.querySelector('.research-graph-actions')?.insertAdjacentHTML('beforebegin', markup);
  toggle?.classList.add('is-active');
}

async function dispatchGraphAiAction(action) {
  const task = selectedTask();
  if (!task || state.graph.busyAction) return;
  if (action === 'document' && !evidenceRankedSources().length) {
    setStatus(state.t('reportRequiresVerifiedSources', 'Reports sind ohne verifizierte Quellen nicht verfügbar.'));
    renderCenter();
    return;
  }
  if (!canWriteResearchState()) {
    setStatus(researchWriteDeniedMessage());
    return;
  }
  state.graph.busyAction = action;
  renderCenter();
  try {
    if (action === 'document') await dispatchGraphDocumentTask(task);
    else await dispatchTargetedGraphResearch(task);
  } catch (error) {
    console.error('[research] graph action failed', error);
    setStatus(`${state.t('actionFailed', 'Aktion fehlgeschlagen')}: ${errorMessage(error)}`);
  } finally {
    state.graph.busyAction = '';
    renderCenter();
    renderRight();
  }
}

async function dispatchTargetedGraphResearch(task) {
  const selectedNode = state.graphProjection?.nodes?.find((node) => node.id === state.selectedGraphNodeId) || null;
  const graphFocusSourceIds = eligibleGraphFocusSourceIds(selectedNode, state.sourceModels);
  const focus = selectedNode?.label || task.title;
  const related = selectedNode
    ? state.graphProjection.links
      .filter((link) => graphLinkNodeId(link.source) === selectedNode.id || graphLinkNodeId(link.target) === selectedNode.id)
      .slice(0, 12)
      .map((link) => {
        const peerId = graphLinkNodeId(link.source) === selectedNode.id ? graphLinkNodeId(link.target) : graphLinkNodeId(link.source);
        return state.graphProjection.nodes.find((node) => node.id === peerId)?.label;
      })
      .filter(Boolean)
    : [];
  const instruction = [
    `Führe eine gezielte Nachrecherche für den Research-Graph "${task.title}" durch.`,
    `Fokusbegriff: ${focus}`,
    related.length ? `Benachbarte Begriffe: ${related.join(', ')}` : '',
    `Knowledge domain: ${task.knowledge_domain}`,
    '',
    task.prompt || '',
    '',
    'Nutze systematic-research und die CTOX Web-Research-Tools. Prüfe die vorhandenen Belege, schließe erkennbare Lücken und schreibe neue Quellen und Belege sofort in die bestehenden Knowledge-Tabellen.',
    'Aktualisiere semantic_graph_nodes und semantic_graph_edges inkrementell. Erzeuge Kanten aus gemeinsamer Nennung in einem 4-Token-Fenster, behalte Provenienz und Source-IDs und überschreibe keine belegten Daten ohne neuen Nachweis.',
  ].filter(Boolean).join('\n');
  const commandId = `cmd_${crypto.randomUUID()}`;
  const now = Date.now();
  const result = await state.ctx.commandBus.dispatch({
    id: commandId,
    command_id: commandId,
    module: 'research',
    command_type: 'research.systematic.run',
    record_id: task.id,
    payload: {
      title: `Nachrecherche · ${focus}`,
      instruction,
      prompt: instruction,
      priority: 'high',
      required_skills: ['systematic-research'],
      research_mode: 'targeted_graph_gap',
      thread_key: `business-os/research/${task.id}`,
      knowledge_domain: task.knowledge_domain,
      graph_focus: {
        node_id: selectedNode?.id || '',
        label: focus,
        related_terms: related,
        source_ids: graphFocusSourceIds,
      },
      knowledge_contract: {
        domain: task.knowledge_domain,
        tables: task.payload?.table_contract || RESEARCH_TABLE_CONTRACT,
        provenance_required: true,
      },
      graph_contract: semanticGraphContract(),
      writeback_contract: {
        collections: ['research_runs', 'research_tasks', 'knowledge_tables'],
        graph_tables: { nodes: 'semantic_graph_nodes', edges: 'semantic_graph_edges' },
      },
    },
    client_context: {
      action: 'research-graph-targeted-research',
      module: 'research',
      source_module: 'research',
      inbound_channel: 'business_os.research',
      knowledge_domain: task.knowledge_domain,
      graph_node_id: selectedNode?.id || '',
    },
  });
  const run = {
    id: `research_run_${now}`,
    task_id: task.id,
    status: result?.task_status || result?.status || 'queued',
    command_id: commandId,
    task_queue_id: result?.task_id || '',
    identified_count: state.sourceRows.length,
    accepted_count: evidenceRankedSources().length,
    used_count: evidenceRankedSources().length,
    payload: { result, graph_focus: focus },
    created_at_ms: now,
    updated_at_ms: now,
  };
  state.runs = [run, ...state.runs];
  await upsertDoc(writableCollection('research_runs'), run);
  setStatus(state.t('targetedResearchQueued', 'Gezielte Nachrecherche wurde an CTOX übergeben.'));
}

function eligibleGraphFocusSourceIds(selectedNode, sourceModels = state.sourceModels) {
  const eligibleIds = evidenceSourceIds(sourceModels);
  return [...new Set((selectedNode?.sourceIds || []).map(String).filter((id) => eligibleIds.has(id)))];
}

async function dispatchGraphDocumentTask(task) {
  if (!evidenceRankedSources().length) return;
  const selectedNode = state.graphProjection?.nodes?.find((node) => node.id === state.selectedGraphNodeId) || null;
  const graphFocusSourceIds = eligibleGraphFocusSourceIds(selectedNode, state.sourceModels);
  const focus = selectedNode?.label || task.title;
  const title = `${task.title} · ${focus}`.slice(0, 120);
  const filename = `${slugId(title).slice(0, 82) || 'research-graph-report'}.docx`;
  const outputPath = `runtime/business-os/documents/generated/${filename}`;
  const commandId = `cmd_${crypto.randomUUID()}`;
  const instruction = [
    `Erstelle ein belastbares Word-Dokument aus dem Research-Graph "${task.title}".`,
    `Fokus: ${focus}`,
    `Knowledge domain: ${task.knowledge_domain}`,
    graphFocusSourceIds.length ? `Bevorzugte Source-IDs: ${graphFocusSourceIds.join(', ')}` : '',
    '',
    'Nutze systematic-research für die Knowledge-Lookup-Pflicht und den doc-Skill für Produktion, Rendering und visuelle Qualitätsprüfung.',
    'Strukturiere Kernaussagen, Cluster, Zusammenhänge, Evidenzlücken und Handlungsempfehlungen. Zitiere nur nachweisbare Quellen aus der Knowledge Base.',
    `Speichere das finale DOCX unter ${outputPath}. Kein Markdown als Endartefakt.`,
  ].filter(Boolean).join('\n');
  await state.ctx.commandBus.dispatch({
    id: commandId,
    command_id: commandId,
    module: 'documents',
    command_type: 'research.systematic.report.create',
    record_id: task.id,
    inbound_channel: 'business_os.documents',
    payload: {
      title,
      instruction,
      prompt: instruction,
      report_type_id: 'research-brief',
      selected_runbook_id: 'research.report.auto',
      desired_format: 'docx',
      output_filename: filename,
      output_path: outputPath,
      required_skills: ['systematic-research', 'doc'],
      required_artifacts: [outputPath],
      thread_key: `business-os/research/${task.id}`,
      knowledge_domain: task.knowledge_domain,
      graph_focus: {
        node_id: selectedNode?.id || '',
        label: focus,
        source_ids: graphFocusSourceIds,
      },
      document_quality_contract: {
        use_documents_skill: true,
        final_artifact_format: 'docx',
        require_real_word_styles: true,
        require_tables_and_figures_when_useful: true,
        require_render_or_structural_qa: true,
      },
      writeback_contract: {
        module: 'documents',
        collection: 'documents',
        desired_format: 'docx',
        document_type: 'word_document',
        title,
        filename,
        output_path: outputPath,
        linked_records: [
          { kind: 'research_task', id: task.id },
          { kind: 'knowledge_domain', id: task.knowledge_domain },
        ],
      },
    },
    client_context: {
      module: 'documents',
      surface: 'research-semantic-graph',
      action: 'create_word_document',
      source_module: 'research',
      inbound_channel: 'business_os.documents',
      document_type: 'word_document',
      filename,
      output_path: outputPath,
    },
  });
  setStatus(state.t('graphDocumentQueued', 'Word-Dokument wurde an CTOX übergeben.'));
}

function semanticGraphContract() {
  return {
    nodes_table_key: 'semantic_graph_nodes',
    edges_table_key: 'semantic_graph_edges',
    extraction: 'concept_cooccurrence',
    cooccurrence_window_tokens: 4,
    community_detection: 'automatic_modularity',
    node_importance: 'betweenness_centrality',
    incremental_writeback: true,
    provenance_required: true,
  };
}

function graphLinkNodeId(value) {
  return typeof value === 'object' && value ? value.id : String(value || '');
}

function renderNoTaskCenter() {
  const empty = emptyStateForNoTask();
  return `
    <header class="ctox-pane-header ctox-pane-band research-center-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('webResearch', 'Web Research'))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(state.t('evidenceWorkbench', 'Portfolio Map & Evidence Workbench'))}</h2>
        </div>
        <div class="ctox-pane-actions">
          <button type="button" class="ctox-pane-icon" data-action="refresh" aria-label="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}" title="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}">${iconSvg('refresh')}</button>
          <button type="button" class="ctox-pane-icon" data-action="new-task" aria-label="${escapeHtml(state.t('createResearch', 'Research anlegen'))}" title="${escapeHtml(state.t('createResearch', 'Research anlegen'))}">${iconSvg('plus')}</button>
        </div>
      </div>
    </header>
    <div class="research-center-empty-body">
      <section class="ctox-empty research-empty-state-panel">
        <strong>${escapeHtml(empty.title)}</strong>
        <span>${escapeHtml(empty.body)}</span>
      </section>
      <section class="research-workbench research-empty-workbench" aria-label="${escapeHtml(state.t('sources', 'Sources'))}">
        <div class="research-tabs-container">
          <div class="ctox-pane-tabs" role="tablist" aria-label="Research views">
            ${disabledTabButton('sources', state.t('sources', 'Sources'))}
            ${disabledTabButton('measurements', state.t('measurements', 'Measurements'))}
            ${disabledTabButton('knowledge', state.t('knowledge', 'Knowledge'))}
          </div>
        </div>
        <div class="research-empty-workbench-body">
          <label class="research-empty-search-row">
            <span>${escapeHtml(state.t('sourceSearch', 'Quellensuche'))}</span>
            <input type="text" class="ctox-input" disabled placeholder="${escapeHtml(state.t('searchSourcesPlaceholder', 'Quelle suchen: NASA, UIUC, Tyto, PX4, Vibration ...'))}" />
          </label>
          <p>${escapeHtml(state.t('noTaskControlsHint', 'Suche, Filter, Portfolio Map und Tabellen werden aktiv, sobald mindestens eine lokale Knowledge Domain mit Quellen geladen ist.'))}</p>
        </div>
      </section>
    </div>
  `;
}

function mapModeToggle() {
  return `
    <div class="ctox-pane-tabs" role="group" aria-label="Research map view">
      <button type="button" class="ctox-pane-tab${state.mapMode !== 'discovery' ? ' is-active' : ''}" data-action="map-mode" data-map-mode="portfolio" aria-pressed="${state.mapMode !== 'discovery'}">${escapeHtml(state.t('map', 'Map'))}</button>
      <button type="button" class="ctox-pane-tab${state.mapMode === 'discovery' ? ' is-active' : ''}" data-action="map-mode" data-map-mode="discovery" aria-pressed="${state.mapMode === 'discovery'}">${escapeHtml(state.t('graph', 'Graph'))}</button>
    </div>
  `;
}

function renderMapPoint(source, xAxis, yAxis) {
  if (!source?.evidenceEligible) return '';
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
  const tags = sourceTags(source);
  const text = [source.id, source.title, source.sourceClass, source.note, ...tags].join(' ').toLowerCase();

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
    const clusterSources = evidenceRankedSources()
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
    <table class="ctox-table" style="table-layout: fixed; width: 100%;">
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
          <th class="is-num">${escapeHtml(state.t('scoreLabel', 'Score'))}</th>
          <th class="is-num">${escapeHtml(axisLabel(yAxis, task))}</th>
          <th class="is-num">${escapeHtml(axisLabel(xAxis, task))}</th>
          <th class="is-num"></th>
        </tr>
      </thead>
      <tbody>
        ${filteredList.map((source) => `
          <tr class="${source.id === state.selectedSourceId ? 'is-selected' : ''}" data-evidence-status="${escapeHtml(source.evidenceStatus)}">
            <td><button type="button" data-action="select-source" data-source-id="${escapeHtml(source.id)}"><strong>${escapeHtml(source.title)}</strong><span>${escapeHtml(source.id)} · ${escapeHtml(source.evidenceStatusLabel)}</span></button></td>
            <td>${escapeHtml(source.sourceClass)}</td>
            <td class="is-num"><span class="ctox-badge ${gradeBadgeClass(source.grade)}">${escapeHtml(source.grade)}${source.evidenceEligible ? ` · ${formatPortfolioScore(source.score)}` : ''}</span></td>
            <td class="is-num">${formatDimensionScore(source.dimensions[yAxis])}</td>
            <td class="is-num">${formatDimensionScore(source.dimensions[xAxis])}</td>
            <td class="is-num">${source.evidenceEligible && source.canonicalUrl ? `<a href="${escapeHtml(source.canonicalUrl)}" target="_blank" rel="noreferrer">${escapeHtml(state.t('openLabel', 'Open'))}</a>` : ''}</td>
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
               class="ctox-input research-sources-shards-search"
               id="research-source-search-input"
               data-action="source-search"
               placeholder="${escapeHtml(state.t('searchSourcesPlaceholder', 'Quelle suchen: NASA, UIUC, Tyto, PX4, Vibration ...'))}"
               value="${escapeHtml(state.sourceSearchTerm || '')}"
               autocomplete="off" />
        <div class="research-sources-shards-filters">
          ${subthemes.map((theme) => `
            <button type="button"
                    class="ctox-chip${activeTag === theme.id ? ' is-active' : ''}"
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
      const tags = sourceTags(source);
      if (!tags.includes(activeTag)) return false;
    }
    if (searchTerm) {
      const titleMatch = (source.title || '').toLowerCase().includes(searchTerm);
      const idMatch = (source.id || '').toLowerCase().includes(searchTerm);
      const classMatch = (source.sourceClass || '').toLowerCase().includes(searchTerm);

      const kindMatch = source.subtitle.toLowerCase().includes(searchTerm);
      const fieldsMatch = firstString(source.row, ['data_fields', 'fields', 'measurement_fields']).toLowerCase().includes(searchTerm);
      const useMatch = firstString(source.row, ['contribution_note', 'contribution', 'use']).toLowerCase().includes(searchTerm);
      const missingMatch = firstString(source.row, ['evidence_gap', 'gap', 'limitations']).toLowerCase().includes(searchTerm);
      const tagMatch = sourceTags(source).some((tag) => tag.toLowerCase().includes(searchTerm));

      if (!titleMatch && !idMatch && !classMatch && !kindMatch && !fieldsMatch && !useMatch && !missingMatch && !tagMatch) {
        return false;
      }
    }
    return true;
  });
}

function renderSourceCard(source) {
  const isSelected = source.id === state.selectedSourceId;
  const kind = source.sourceClass || 'Quelle';
  const tags = sourceTags(source);
  const fields = firstString(source.row, ['data_fields', 'fields', 'measurement_fields', 'summary']) || state.t('noSummaryAvailable', 'Keine Zusammenfassung.');
  const use = firstString(source.row, ['contribution_note', 'contribution', 'use']) || state.t('contributionMissing', 'Beitrag nicht dokumentiert.');
  const missing = firstString(source.row, ['evidence_gap', 'gap', 'limitations']) || state.t('limitationsMissing', 'Grenzen nicht dokumentiert.');
  const canonicalUrl = firstString(source.row, ['canonical_url']);

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
      <div class="research-source-card-status ${source.evidenceEligible ? 'is-verified' : 'is-discovery'}" data-evidence-status="${escapeHtml(source.evidenceStatus)}">
        ${escapeHtml(source.evidenceStatusLabel)}${source.evidenceEligible ? ` · ${escapeHtml(formatPortfolioScore(source.score))}` : ' · Score —'}
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
      ${source.evidenceEligible && canonicalUrl ? `
        <div class="research-source-card-actions">
          <a href="${escapeHtml(canonicalUrl)}"
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

function sourceTags(source) {
  const raw = source?.row?.tags ?? source?.row?.source_tags ?? source?.sourceClass ?? '';
  const values = Array.isArray(raw)
    ? raw
    : typeof raw === 'string' && raw.trim().startsWith('[')
      ? (() => { try { return JSON.parse(raw); } catch { return raw.split(/[,;|]/); } })()
      : String(raw).split(/[,;|]/);
  return [...new Set(values.map((value) => String(value).trim().toLowerCase()).filter(Boolean))];
}

function gradeFullText(grade) {
  const g = String(grade || '').toUpperCase();
  if (g === 'A') return 'A · Ausgezeichnet';
  if (g === 'B') return 'B · Gut';
  if (g === 'C') return 'C · Ergänzend';
  if (g === 'D') return 'D · Risiko';
  return g;
}

function formatPortfolioScore(value) {
  if (value === null || value === undefined || value === '') return '—';
  const score = Number(value);
  return Number.isFinite(score) ? (score / 10).toFixed(1) : '—';
}

function formatDimensionScore(value) {
  if (value === null || value === undefined || value === '') return '—';
  const score = Number(value);
  return Number.isFinite(score) ? String(Math.round(score)) : '—';
}

function renderMeasurementsTable() {
  return `
    <table class="ctox-table" style="table-layout: fixed; width: 100%;">
      <colgroup>
        <col style="width: 15%;" />
        <col style="width: 11%;" />
        <col style="width: 11%;" />
        <col style="width: 10%;" />
        <col style="width: 10%;" />
        <col style="width: 11%;" />
        <col style="width: 11%;" />
        <col style="width: 11%;" />
        <col style="width: 10%;" />
      </colgroup>
      <thead>
        <tr>
          ${measurementHeader('Quelle', 'Quell-ID der Messreihe oder des extrahierten Datensatzes.')}
          ${measurementHeader('Propeller', 'Originale Propellerangabe als Durchmesser x Steigung. 9x5 bedeutet 9 Zoll Durchmesser und 5 Zoll Steigung.')}
          ${measurementHeader('Durchmesser (mm)', 'Propeller-Durchmesser metrisch in Millimetern, aus Angaben wie 9x5 separat extrahiert.', true)}
          ${measurementHeader('Steigung (mm)', 'Propeller-Steigung metrisch in Millimetern, aus Angaben wie 9x5 separat extrahiert.', true)}
          ${measurementHeader('RPM', 'Drehzahl in Umdrehungen pro Minute, ohne Tausendertrennzeichen formatiert.', true)}
          ${measurementHeader('Force (N)', 'Axiale Kraft in Newton. Bei Propellerdaten ist dies in der Regel der gemessene Schub.', true)}
          ${measurementHeader('Tangentiale Ersatzkraft (N)', 'Aus Drehmoment und Radius abgeleitete tangentiale Ersatzkraft. Das ist keine gemessene Lager-Radiallast.', true)}
          ${measurementHeader('Moment/Torque (N m)', 'Drehmoment beziehungsweise Moment in Newtonmeter aus dem Messdatensatz.', true)}
          ${measurementHeader('Methode', 'Konfidenz oder Ableitungsverfahren der Messzeile.')}
        </tr>
      </thead>
      <tbody>
        ${state.measurementRows.slice(0, 120).map((row) => `
          <tr>
            <td>${escapeHtml(row.source_id || '')}</td>
            <td>${escapeHtml(propellerSize(row))}</td>
            <td class="is-num">${formatMeasurementNumber(metricPropellerLength(row, 'prop_diameter'))}</td>
            <td class="is-num">${formatMeasurementNumber(metricPropellerLength(row, 'prop_pitch'))}</td>
            <td class="is-num">${formatMeasurementNumber(row.rpm, 0)}</td>
            <td class="is-num">${formatMeasurementNumber(row.force_N ?? row.axial_load_N ?? row.thrust_N)}</td>
            <td class="is-num">${formatMeasurementNumber(tangentialEquivalentForce(row))}</td>
            <td class="is-num">${formatMeasurementNumber(row.torque_Nm)}</td>
            <td>${escapeHtml(firstString(row, ['confidence', 'derivation_method']).slice(0, 90))}</td>
          </tr>
        `).join('') || `<tr><td colspan="9">${escapeHtml(state.t('noMeasurements', 'Keine Messpunkte vorhanden.'))}</td></tr>`}
      </tbody>
    </table>
  `;
}

function measurementHeader(label, help, numeric = false) {
  return `
    <th class="${numeric ? 'is-num' : ''}" title="${escapeHtml(help)}">
      <span>${escapeHtml(label)}</span>
    </th>
  `;
}

function propellerSize(row) {
  const explicit = firstString(row, ['propeller_size', 'prop_size', 'prop']);
  if (explicit) return explicit.replace(/\s*[xX×]\s*/g, ' x ');
  const diameter = formatMeasurementNumber(row.prop_diameter_in);
  const pitch = formatMeasurementNumber(row.prop_pitch_in);
  return [diameter, pitch].filter(isPresent).join(' x ');
}

function metricPropellerLength(row, stem) {
  const metric = numberValue(row[`${stem}_mm`]);
  if (metric) return metric;
  const inches = numberValue(row[`${stem}_in`]);
  return inches ? inches * 25.4 : '';
}

function tangentialEquivalentForce(row) {
  const explicit = numberValue(row.tangential_equivalent_force_N);
  if (explicit) return explicit;
  const legacy = numberValue(row.radial_load_N);
  return legacy ? Math.abs(legacy) : '';
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
  const canBuildKnowledge = canBuildKnowledgeFromResearch(task);
  root.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('context', 'Context'))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(task?.title || 'Research')}</h2>
        </div>
      </div>
    </header>
    <div class="research-right-scroll">
      <section class="research-context-block">
        <span class="ctox-pane-kicker">Knowledge Base</span>
        <strong>${escapeHtml(task?.knowledge_domain || state.t('noDomain', 'Keine Domain'))}</strong>
        <p>${escapeHtml(task?.prompt || state.t('defaultTaskDesc', 'Research-Dashboard auf Basis einer vorhandenen Knowledge Base.'))}</p>
        ${task?.criteria ? `<small>${escapeHtml(task.criteria)}</small>` : ''}
        ${task ? `<button type="button" class="ctox-button" data-action="edit-task">${escapeHtml(state.t('editScoring', 'Scoring bearbeiten'))}</button>` : ''}
        ${task ? `<button type="button" class="ctox-button" data-action="build-knowledge" ${canBuildKnowledge ? '' : 'disabled aria-disabled="true"'} title="${escapeHtml(canBuildKnowledge ? '' : knowledgeUnavailableReason())}">${escapeHtml(task.payload?.knowledge_refresh?.command_id ? state.t('updateKnowledge', 'Knowledge aktualisieren') : state.t('buildKnowledge', 'Knowledge aufbauen'))}</button>` : ''}
      </section>
      ${renderScoringModel(task)}
      <section class="research-metric-grid">
        <div><strong>${state.sourceModels.length}</strong><span>${escapeHtml(state.t('sources', 'Sources'))}</span></div>
        <div><strong>${evidenceRankedSources().length}</strong><span>${escapeHtml(state.t('verified', 'Verified'))}</span></div>
        <div><strong>${state.measurementRows.length}</strong><span>${escapeHtml(state.t('measurements', 'Measurements'))}</span></div>
        <div><strong>${avgScore()}</strong><span>${escapeHtml(state.t('avgScore', 'Avg score'))}</span></div>
        <div><strong>${runInfo.status || latestRun?.status || task?.status || 'ready'}</strong><span>${escapeHtml(state.t('status', 'Status'))}</span></div>
      </section>
      ${renderRunPanel(runInfo)}
      <section class="research-context-block">
        <span class="ctox-pane-kicker">${escapeHtml(state.t('selectedSource', 'Selected Source'))}</span>
        ${source ? `
          <strong style="font-size: 13px; display: block; margin-bottom: 8px; color: var(--research-text);">${escapeHtml(source.title)}</strong>
          <p style="font-size: 11.5px; line-height: 1.4; color: var(--research-muted); margin-bottom: 12px;">${escapeHtml(source.note || state.t('noSummaryAvailable', 'Keine Zusammenfassung vorhanden.'))}</p>
          <div class="research-source-card-status ${source.evidenceEligible ? 'is-verified' : 'is-discovery'}" data-evidence-status="${escapeHtml(source.evidenceStatus)}">${escapeHtml(source.evidenceStatusLabel)} · ${source.evidenceEligible ? 'Score verfügbar' : 'Score —'}</div>
          
          <div class="research-metric-profile" style="margin-bottom: 16px; display: flex; flex-direction: column; gap: 8px;">
            <!-- Overall Score Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Overall Score</span>
                <span>${formatPortfolioScore(source.score)}${source.evidenceEligible ? '%' : ''}</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.evidenceEligible && source.score / 10 > 75 ? 'good' : source.evidenceEligible && source.score / 10 > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${source.evidenceEligible ? (source.score / 10).toFixed(1) : '0'}%;"></div>
              </div>
            </div>
            
            <!-- Source Quality Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Source Quality</span>
                <span>${formatDimensionScore(source.dimensions.source_quality)}${source.evidenceEligible ? '%' : ''}</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.evidenceEligible && source.dimensions.source_quality > 75 ? 'good' : source.evidenceEligible && source.dimensions.source_quality > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${source.evidenceEligible ? Math.round(source.dimensions.source_quality) : 0}%;"></div>
              </div>
            </div>
            
            <!-- Evidence Strength Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Evidence Strength</span>
                <span>${formatDimensionScore(source.dimensions.evidence_strength)}${source.evidenceEligible ? '%' : ''}</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.evidenceEligible && source.dimensions.evidence_strength > 75 ? 'good' : source.evidenceEligible && source.dimensions.evidence_strength > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${source.evidenceEligible ? Math.round(source.dimensions.evidence_strength) : 0}%;"></div>
              </div>
            </div>
            
            <!-- Topic Fit Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Topic Fit</span>
                <span>${formatDimensionScore(source.dimensions.topic_fit)}${source.evidenceEligible ? '%' : ''}</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.evidenceEligible && source.dimensions.topic_fit > 75 ? 'good' : source.evidenceEligible && source.dimensions.topic_fit > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${source.evidenceEligible ? Math.round(source.dimensions.topic_fit) : 0}%;"></div>
              </div>
            </div>
            
            <!-- Actionability Progress -->
            <div class="research-metric-progress-wrapper" style="margin-bottom: 4px;">
              <div class="research-metric-progress-label" style="display: flex; justify-content: space-between; font-size: 11px; font-weight: 700; margin-bottom: 4px;">
                <span>Actionability</span>
                <span>${formatDimensionScore(source.dimensions.actionability)}${source.evidenceEligible ? '%' : ''}</span>
              </div>
              <div class="research-metric-progress-bar-bg" style="height: 6px; background: var(--research-surface-2); border-radius: 3px; overflow: hidden;">
                <div class="research-metric-progress-bar-fill ${source.evidenceEligible && source.dimensions.actionability > 75 ? 'good' : source.evidenceEligible && source.dimensions.actionability > 45 ? 'accent' : 'warn'}" style="height: 100%; border-radius: 3px; transition: width 0.3s ease; width: ${source.evidenceEligible ? Math.round(source.dimensions.actionability) : 0}%;"></div>
              </div>
            </div>
          </div>
          
          <button type="button" class="ctox-button" data-action="source-detail" data-source-id="${escapeHtml(source.id)}" style="width: 100%; text-align: center;">${escapeHtml(state.t('details', 'Details'))}</button>
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
  const task = selectedTask();
  const canRun = canRunResearchTask(task);
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
        <dl class="ctox-fields">
          <dt>${escapeHtml(state.t('command', 'Command'))}</dt><dd>${escapeHtml(shortId(runInfo.commandId))}</dd>
          <dt>${escapeHtml(state.t('queue', 'Queue'))}</dt><dd>${escapeHtml(shortId(runInfo.taskQueueId))}</dd>
          <dt>${escapeHtml(state.t('thread', 'Thread'))}</dt><dd>${escapeHtml(runInfo.threadKey || '-')}</dd>
        </dl>
        <div class="research-run-actions">
          <button type="button" class="ctox-button" data-action="focus-ctox-run" data-command-id="${escapeHtml(runInfo.commandId)}" data-task-queue-id="${escapeHtml(runInfo.taskQueueId)}" ${runInfo.taskQueueId || runInfo.commandId ? '' : 'disabled'}>${escapeHtml(state.t('viewInCtox', 'In CTOX ansehen'))}</button>
        </div>
      ` : `
        <p>${escapeHtml(state.t('noRunStarted', 'Kein Research-Lauf für dieses Dashboard gestartet.'))}</p>
      `}
      <button type="button" class="ctox-button ctox-run-control research-run-control" data-action="run-research" ${canRun ? '' : 'disabled aria-disabled="true"'} aria-label="${escapeHtml(runInfoActionLabel(task))}" title="${escapeHtml(runResearchHint(task, runInfo))}" ${['queued', 'running'].includes(runInfo.statusKind) ? 'aria-busy="true"' : ''}>
        <span aria-hidden="true">▶</span>${escapeHtml(runInfoActionLabel(task))}
      </button>
    </section>
  `;
}

function computedDecisionNotes(source) {
  const top = evidenceRankedSources()[0];
  const notes = [];
  if (top) {
    notes.push({ kind: 'opportunity', title: state.t('decisionNoteEv1', 'Use strongest evidence first'), body: state.t('decisionNoteEv1Body', `${top.title} ist aktuell der stärkste Dashboard-Anker.`, top.title) });
  }
  if (state.measurementRows.length) {
    notes.push({ kind: 'opportunity', title: state.t('decisionNoteQuant', 'Quantitative evidence available'), body: state.t('decisionNoteQuantBody', `${state.measurementRows.length} Messpunkte können in die aktiven Scoring-Kriterien einfließen.`, state.measurementRows.length) });
  }
  if (!top) {
    notes.push({ kind: 'risk', title: state.t('decisionNoteGate', 'Evidence gate active'), body: state.t('decisionNoteGateBody', 'Discovery-Kandidaten bleiben sichtbar, bis Verifizierung, Snapshot und HTTP-Erfolg vollständig vorliegen.') });
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
  return validateSelectedResearchTask(task, state.knowledgeBases).valid && canWriteResearchState();
}

function canBuildKnowledgeFromResearch(task = selectedTask()) {
  return Boolean(task?.id && evidenceRankedSources().length && canWriteResearchState());
}

function knowledgeUnavailableReason() {
  if (!evidenceRankedSources().length) return state.t('knowledgeRequiresVerifiedSources', 'Knowledge ist ohne verifizierte Quellen nicht verfügbar.');
  if (!canWriteResearchState()) return researchWriteDeniedMessage();
  return '';
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
  const validation = validateSelectedResearchTask(task, state.knowledgeBases);
  if (!validation.valid) return validation.message;
  if (!canWriteResearchState()) return researchWriteDeniedMessage();
  return '';
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
  const domainOptions = knowledgeDomainOptionsMarkup(selectedDomain);
  const overlay = document.createElement('div');
  overlay.className = 'ctox-modal research-task-dialog';
  overlay.innerHTML = `
    <section class="ctox-modal-card" role="dialog" aria-modal="true" aria-labelledby="research-create-title">
      <header class="ctox-modal-header">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('webResearch', 'Web Research'))}</span>
          <h3 class="ctox-modal-title" id="research-create-title">${isEdit ? escapeHtml(state.t('editScoring', 'Scoring bearbeiten')) : escapeHtml(state.t('dashboardAnlegen', 'Dashboard anlegen'))}</h3>
        </div>
        <button type="button" class="ctox-pane-icon" data-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">${iconSvg('close')}</button>
      </header>
      <form class="ctox-modal-body" data-research-task-form>
        <input type="hidden" name="task_id" value="${escapeHtml(editTask?.id || '')}">
        ${isEdit ? `<input type="hidden" name="domain" value="${escapeHtml(selectedDomain)}">` : ''}
        <label><span class="ctox-field-label">${escapeHtml(state.t('titel', 'Titel'))}</span><input class="ctox-input" name="title" placeholder="${escapeHtml(state.t('neueResearch', 'Neue Research'))}" value="${escapeHtml(editTask?.title || '')}" required></label>
        <label>
          <span class="ctox-field-label">Knowledge Domain</span>
          <select class="ctox-select" name="${isEdit ? 'domain_display' : 'domain'}" ${isEdit || !state.knowledgeBases.length ? 'disabled' : ''} required>
            <option value="" ${selectedDomain ? '' : 'selected'} disabled>${escapeHtml(state.t('selectKnowledgeDomain', 'Knowledge Domain auswählen'))}</option>
            ${domainOptions}
          </select>
          <small class="research-field-note">${escapeHtml(domainSelectionNote(isEdit))}</small>
        </label>
        <label><span class="ctox-field-label">${escapeHtml(state.t('auftrag', 'Auftrag'))}</span><textarea class="ctox-textarea" name="prompt" placeholder="${escapeHtml(state.t('promptPlaceholder', 'Was soll das Dashboard auswerten?'))}" required>${escapeHtml(editTask?.prompt || '')}</textarea></label>
        <label><span class="ctox-field-label">${escapeHtml(state.t('kriterien', 'Kriterien'))}</span><textarea class="ctox-textarea" name="criteria" placeholder="${escapeHtml(state.t('criteriaPlaceholder', 'Scope, Ausschlüsse, Scoring-Hinweise'))}">${escapeHtml(editTask?.criteria || '')}</textarea></label>
        <label><span class="ctox-field-label">${escapeHtml(state.t('scoringDimensions', 'Scoring Dimensionen'))}</span><textarea class="ctox-textarea" name="scoring_dimensions" placeholder="${escapeHtml(state.t('scoringPlaceholder', 'overlap: Overlap\nbuyer_clarity: Buyer clarity'))}">${escapeHtml(dimensionsText)}</textarea></label>
        <p class="research-validation" data-validation-status aria-live="polite"></p>
        <footer class="ctox-modal-footer">
          <button type="button" class="ctox-button" data-close>${escapeHtml(state.t('cancel', 'Abbrechen'))}</button>
          <button type="submit" class="ctox-button is-primary" disabled>${isEdit ? escapeHtml(state.t('save', 'Speichern')) : escapeHtml(state.t('create', 'Anlegen'))}</button>
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
  if (!isEdit && !state.knowledgeBases.length) {
    refreshTaskDialogKnowledgeOptions().catch((error) => {
      console.warn('[research] task dialog knowledge refresh failed', error);
    });
  }
  requestAnimationFrame(() => overlay.querySelector('input[name="title"]')?.focus());
}

function closeTaskDialog() {
  state.ctx.host.querySelector('.research-task-dialog')?.remove();
}

function knowledgeDomainOptionsMarkup(selectedDomain = '') {
  return state.knowledgeBases.map((base) => `
    <option value="${escapeHtml(base.domain)}" ${base.domain === selectedDomain ? 'selected' : ''}>
      ${escapeHtml(`${base.title || titleFromDomain(base.domain)} · ${base.domain}`)}
    </option>
  `).join('');
}

function refreshOpenTaskDialogDomainOptions() {
  const overlay = state.ctx.host.querySelector('.research-task-dialog');
  const form = overlay?.querySelector('[data-research-task-form]');
  const select = form?.querySelector('select[name="domain"]');
  if (!overlay || !form || !select) return;
  const currentValue = select.value || selectedTask()?.knowledge_domain || state.knowledgeBases[0]?.domain || '';
  const selectedDomain = state.knowledgeBases.some((base) => base.domain === currentValue)
    ? currentValue
    : state.knowledgeBases[0]?.domain || '';
  select.disabled = !state.knowledgeBases.length;
  select.innerHTML = `
    <option value="" ${selectedDomain ? '' : 'selected'} disabled>${escapeHtml(state.t('selectKnowledgeDomain', 'Knowledge Domain auswählen'))}</option>
    ${knowledgeDomainOptionsMarkup(selectedDomain)}
  `;
  if (selectedDomain) select.value = selectedDomain;
  const note = form.querySelector('.research-field-note');
  if (note) note.textContent = domainSelectionNote(false);
  const status = form.querySelector('[data-validation-status]');
  const submit = form.querySelector('button[type="submit"]');
  const validation = validateResearchTaskInput(formValues(form), state.knowledgeBases, { isEdit: false });
  if (submit) submit.disabled = !validation.valid;
  if (status) status.textContent = validation.valid ? '' : validation.message;
}

async function refreshTaskDialogKnowledgeOptions() {
  const overlay = state.ctx.host.querySelector('.research-task-dialog');
  if (!overlay || overlay.dataset.knowledgeRefresh === 'running') return;
  overlay.dataset.knowledgeRefresh = 'running';
  const note = overlay.querySelector('.research-field-note');
  if (note) note.textContent = state.t('loadingKnowledge', 'Knowledge wird geladen...');
  try {
    const knowledgeBases = await loadKnowledgeBases();
    if (knowledgeBases.length) {
      state.knowledgeBases = knowledgeBases;
      await loadLocalState();
      await ensureTasksFromKnowledgeBases();
      refreshOpenTaskDialogDomainOptions();
      return;
    }
    if (note) note.textContent = domainSelectionNote(false);
  } finally {
    delete overlay.dataset.knowledgeRefresh;
  }
}

async function createTaskFromForm(form) {
  if (!canWriteCollection('research_tasks')) throw new Error(researchWriteDeniedMessage());
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
  const domain = researchDomainFromFormValue(rawDomain, state.knowledgeBases, rawTitle || current?.title || 'research');
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
      graph_contract: semanticGraphContract(),
    },
    created_at_ms: current?.created_at_ms || now,
    updated_at_ms: now,
  };
  await upsertDoc(writableCollection('research_tasks'), task);
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
  if (!canWriteResearchState()) {
    setStatus(researchWriteDeniedMessage());
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
    'Nutze den systematic-research Skill. Starte mit ctox knowledge search, dann ctox web deep-research. Schreibe jede Discovery-Runde sofort nach source_catalog. Lies/prüfe jede kanonische Quelle, extrahiere Fakten nach evidence_points und schreibe nur belegte Optionen mit gewichteten Scores nach evaluation_matrix. Aktualisiere bestehende Zeilen, wenn sich Fokus oder Kriterien ändern, statt parallele Tabellen zu erzeugen. Die UI-Evidence-Gate-Felder verification_status=verified, transport_verified=true, content_extracted=true, actual_full_text_or_data=true, evidence_relevance_score>=8, http_status 2xx (nicht 204), snapshot_hash als SHA-256, canonical_url auf die Originalquelle, evidence_eligible=true und ein nicht-aggregierter source_tier sind zwingend; Metadaten-URLs, alte, fehlende, metadata_only, fachfremde oder rejected Zeilen bleiben ungescored.',
    'Vor Abschluss sind drei voneinander getrennte Audits auszuführen: Source-Audit (URL, Autorität, Inhalt, Snapshot), Data-Audit (Originaldatei, Zeile/Spalte, Einheit, Parsing, Umrechnung, Row-Count) und Claim-Audit (jede Knowledge- und Report-Aussage gegen freigegebene Evidence). Nicht bestandene Aussagen oder Quellen dürfen nicht in Knowledge, Scores oder Reports gelangen.',
    'Pflege parallel semantic_graph_nodes und semantic_graph_edges: Konzepte aus Titel, Zusammenfassung und Evidenz; gemeinsame Nennung im 4-Token-Fenster; automatische Communities; Betweenness-Zentralität; Source-IDs und Provenienz an jedem Graph-Datensatz. Schreibe inkrementell, damit die laufende Research-App über RxDB/WebRTC live aktualisiert wird.',
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
    graph_contract: semanticGraphContract(),
    scoring_contract: researchScoringContract(scoringDimensions),
    writeback_contract: {
      collections: ['research_runs', 'research_tasks', 'knowledge_tables'],
      dashboard_tables: {
        source_catalog: task.source_catalog_key || 'source_catalog',
        evaluation_matrix: task.curated_table_key || 'evaluation_matrix',
        evidence_points: task.measurements_table_key || 'evidence_points',
        semantic_graph_nodes: 'semantic_graph_nodes',
        semantic_graph_edges: 'semantic_graph_edges',
      },
    },
  };
  const dispatched = await state.ctx.commandBus.dispatch({
      id: commandId,
      command_id: commandId,
      module: 'research',
      command_type: 'research.systematic.run',
      record_id: task.id,
      payload,
      client_context: {
        action: 'research-run-chat',
        module: 'research',
        source_module: 'research',
        inbound_channel: 'business_os.research',
        knowledge_domain: task.knowledge_domain,
        knowledge_tables: base?.tables || [],
      },
  });
  const result = {
    ...(dispatched || {}),
    ok: true,
    command_id: commandId,
    status: dispatched?.status || 'queued',
    task_status: dispatched?.task_status || dispatched?.status || 'queued',
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
    accepted_count: evidenceRankedSources().length,
    used_count: evidenceRankedSources().length,
    payload: { result },
    created_at_ms: now,
    updated_at_ms: now,
  };
  state.runs = [run, ...state.runs.filter((item) => item.id !== run.id)];
  await upsertDoc(writableCollection('research_runs'), run).catch((error) => {
    console.warn('[research] could not persist run', error);
  });
  await patchDoc(writableCollection('research_tasks'), task.id, { status: 'collecting', updated_at_ms: now }).catch((error) => {
    console.warn('[research] could not patch task status', error);
  });
  setStatus(state.t('researchChatQueued', 'Research-Aufgabe im Chat gestartet.'));
  render();
}

function researchScoringContract(scoringDimensions) {
  return {
    dimensions: scoringDimensions,
    weights: scoringWeights(scoringDimensions),
    total_field: 'weighted_total',
    rule: 'Only score rows passing the UI evidence gate: verification_status=verified, transport_verified=true, content_extracted=true, HTTP 2xx, non-empty snapshot_hash and canonical_url, evidence_eligible=true, and non-aggregated source_tier. Raw, legacy, metadata-only, off-topic, rejected, empty, or aggregated discovery candidates stay unscored.',
    required_source_fields: ['verification_status', 'transport_verified', 'content_extracted', 'actual_full_text_or_data', 'evidence_relevance_score', 'http_status', 'snapshot_hash', 'canonical_url', 'evidence_eligible', 'source_tier'],
    required_audits: ['source', 'data', 'claim'],
  };
}

function knowledgeRefreshPayload(task, base, latestRun) {
  const tables = base?.tables || [];
  const tableRefs = Object.fromEntries(tables
    .filter((table) => table.table_key)
    .map((table) => [table.table_key, table.id || table.table_id || table.table_key]));
  const instruction = [
    `Baue oder aktualisiere die Knowledge Base fuer das abgeschlossene Research "${task.title}".`,
    `Research Task ID: ${task.id}`,
    `Research Run ID: ${latestRun?.id || 'latest'}`,
    `Knowledge domain: ${task.knowledge_domain}`,
    '',
    'Erzeuge bzw. aktualisiere einen fachlichen Skill/Skillbook und die dazugehoerigen Runbooks und Ressourcen.',
    'Verwende die bestehenden stabilen IDs und aktualisiere vorhandene Elemente per Upsert; erzeuge keine parallelen Kopien derselben Knowledge Base.',
    'Der Skill ist der Wissens- und Arbeits-Hub, aber keine Ersatzquelle: Jede faktische Aussage muss auf die originalen source_id/source_url-Eintraege aus source_catalog und evidence_points zurueckverweisen.',
    'Uebernimm keine unbelegten Aussagen. Halte Quellen, Evidenz, Tabellen und Ableitungen getrennt nachvollziehbar.',
    'Erzeuge Runbooks fuer wiederkehrende Analysen und Dokumenttypen, die dieses Knowledge und bei Bedarf die Originalquellen erneut lesen.',
    'Bewahre die Verbindung zu Research Task, Research Run und Knowledge-Tabellen, damit spaetere Research-Laeufe dieselben Elemente aktualisieren koennen.',
  ].join('\n');
  return {
    title: `Knowledge aktualisieren · ${task.title}`,
    instruction,
    prompt: instruction,
    priority: 'high',
    required_skills: ['systematic-research', 'knowledge'],
    update_mode: 'upsert',
    thread_key: `business-os/research/${task.id}/knowledge`,
    research_task_id: task.id,
    research_run_id: latestRun?.id || '',
    knowledge_domain: task.knowledge_domain,
    source_tables: tableRefs,
    knowledge_contract: {
      domain: task.knowledge_domain,
      create_or_update: ['skillbook', 'skills', 'runbooks', 'resources'],
      stable_identity: true,
      provenance_required: true,
      source_of_truth: 'original_sources',
      citations: ['source_id', 'source_url'],
      refresh_policy: 'update_existing_elements_from_latest_research_run',
    },
    writeback_contract: {
      collections: ['knowledge_items', 'knowledge_runbooks', 'knowledge_tables'],
      mode: 'upsert',
      preserve_lineage: true,
      lineage: {
        research_task_id: task.id,
        research_run_id: latestRun?.id || '',
        knowledge_domain: task.knowledge_domain,
        table_ids: Object.values(tableRefs),
      },
    },
  };
}

async function buildKnowledgeFromResearch() {
  const task = selectedTask();
  if (!canBuildKnowledgeFromResearch(task)) {
    setStatus(knowledgeUnavailableReason());
    renderRight();
    return;
  }
  if (!canRunResearchTask(task)) {
    setStatus(runDisabledReason(task));
    renderRight();
    return;
  }
  if (!canWriteResearchState()) {
    setStatus(researchWriteDeniedMessage());
    renderRight();
    return;
  }
  const base = knowledgeBaseForTask(task);
  const latestRun = latestEvidenceRunForTask(task.id, state.runs);
  const commandId = `cmd_${crypto.randomUUID()}`;
  const payload = knowledgeRefreshPayload(task, base, latestRun);
  const result = await state.ctx.commandBus.dispatch({
    id: commandId,
    command_id: commandId,
    module: 'research',
    command_type: 'research.knowledge.refresh',
    record_id: task.id,
    payload,
    client_context: {
      action: 'build-or-update-knowledge',
      module: 'research',
      source_module: 'research',
      inbound_channel: 'business_os.research',
      research_task_id: task.id,
      research_run_id: latestRun?.id || '',
      knowledge_domain: task.knowledge_domain,
    },
  });
  const now = Date.now();
  const knowledgeRefresh = {
    command_id: result?.command_id || commandId,
    task_id: result?.task_id || '',
    status: result?.task_status || result?.status || 'queued',
    research_run_id: latestRun?.id || '',
    requested_at_ms: now,
  };
  await patchDoc(writableCollection('research_tasks'), task.id, {
    payload: { ...(task.payload || {}), knowledge_refresh: knowledgeRefresh },
    updated_at_ms: now,
  });
  task.payload = { ...(task.payload || {}), knowledge_refresh: knowledgeRefresh };
  state.ctx.storageScope.set('ctox.businessOs.knowledge.openDomain', task.knowledge_domain);
  setStatus(state.t('knowledgeQueued', 'Knowledge-Aufbau wurde an CTOX uebergeben.'));
  render();
}

function latestEvidenceRunForTask(taskId, runs = state.runs) {
  return [...(runs || [])]
    .filter((run) => run.task_id === taskId)
    .filter((run) => Number(run.used_count) > 0 || Number(run.accepted_count) > 0)
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))[0] || null;
}

function runInfoActionLabel(task) {
  return researchRunInfo(task).hasRun
    ? state.t('researchFortsetzen', 'Research fortsetzen')
    : state.t('researchStarten', 'Research starten');
}

async function updateTaskAxis(axis, value) {
  const task = selectedTask();
  if (!task) return;
  if (!canWriteCollection('research_tasks')) {
    setStatus(researchWriteDeniedMessage());
    renderRight();
    return;
  }
  const patch = axis === 'x' ? { x_axis: safeAxis(value, task) } : { y_axis: safeAxis(value, task) };
  await patchDoc(writableCollection('research_tasks'), task.id, { ...patch, updated_at_ms: Date.now() });
  Object.assign(task, patch);
  renderCenter();
}

function openKnowledgeTable(tableId) {
  if (!tableId) return;
  state.ctx.storageScope.set('ctox.businessOs.knowledge.openId', tableId);
  location.hash = 'knowledge';
}

function openSourceDrawer(sourceId) {
  const source = state.sourceModels.find((item) => item.id === sourceId);
  if (!source) return;
  const body = document.createElement('div');
  body.className = 'research-drawer';
  body.innerHTML = `
    <header><strong>${escapeHtml(source.title)}</strong><button type="button" class="ctox-pane-icon" data-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">${iconSvg('close')}</button></header>
    <div class="research-drawer-body">
      <span class="ctox-badge ${gradeBadgeClass(source.grade)}">${escapeHtml(source.grade)}${source.evidenceEligible ? ` · ${formatPortfolioScore(source.score)}` : ' · Score —'}</span>
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
  window.addEventListener('click', hide, { capture: true });
  window.addEventListener('keydown', esc);
  return () => {
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
        <button type="button" class="ctox-pane-icon" data-close aria-label="${escapeHtml(state.t('close', 'Schließen'))}">${iconSvg('close')}</button>
      </header>
      ${canModifyApp ? `
        <div class="ctox-choice-group research-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label class="ctox-choice"><input type="radio" name="mode" value="data" checked> <span>${escapeHtml(state.t('workWithResearch', 'Mit Research arbeiten'))}</span></label>
          <label class="ctox-choice"><input type="radio" name="mode" value="app"> <span>${escapeHtml(state.t('modifyDashboard', 'Dashboard modifizieren'))}</span></label>
        </div>
      ` : ''}
      <textarea class="ctox-textarea" name="message" placeholder="${escapeHtml(state.t('chatPlaceholder', 'Was soll CTOX hier tun oder prüfen?'))}"></textarea>
      <footer><span data-status></span><button type="submit" class="ctox-button is-primary">${escapeHtml(state.t('send', 'Senden'))}</button></footer>
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

async function dispatchResearchContextChat(context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-status]');
  if (!trimmed) {
    if (status) status.textContent = state.t('messageMissing', 'Nachricht fehlt.');
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
  await state.ctx.contextActions.dispatch(safeMode, {
    title,
    prompt: instruction,
    context,
  });
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
  return `<button type="button" class="ctox-pane-tab${state.activeTab === id ? ' is-active' : ''}" role="tab" data-action="tab" data-tab="${id}" aria-selected="${state.activeTab === id}">${escapeHtml(label)}</button>`;
}

function disabledTabButton(id, label) {
  return `<button type="button" class="ctox-pane-tab" data-tab="${escapeHtml(id)}" aria-disabled="true" disabled>${escapeHtml(label)}</button>`;
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

// Standard action icons come from the shell icon set (shared/icons.js via
// ctx.getActionIcon): monochrome stroke glyphs that inherit currentColor.
// Legacy local names are mapped onto the shared glyph names.
function iconSvg(name) {
  const kitNames = { plus: 'add', table: 'columns', knowledge: 'knowledge' };
  return state.ctx?.getActionIcon?.(kitNames[name] || name, 16, 1.8) || '';
}

// Grade → kit badge state (A=success, B=info, C=warning, D=danger).
function gradeBadgeClass(grade) {
  const g = String(grade || '').toUpperCase();
  if (g === 'A') return 'is-success';
  if (g === 'B') return 'is-info';
  if (g === 'C') return 'is-warning';
  if (g === 'D') return 'is-danger';
  return '';
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
  const ranked = evidenceRankedSources();
  if (!ranked.length) return '—';
  return (ranked.reduce((sum, item) => sum + item.score, 0) / ranked.length / 10).toFixed(1);
}

async function findAll(collection, collectionName = '') {
  if (!collection?.find) {
    if (collectionName) {
      markCollectionDiagnostic(
        collectionName,
        'read',
        canReadCollection(collectionName) ? 'missing' : 'denied',
        canReadCollection(collectionName)
          ? state.t('collectionMissing', 'Daten nicht verfügbar')
          : state.t('collectionLocked', 'Keine Datenfreigabe'),
      );
    }
    return [];
  }
  try {
    const docs = await withTimeout(collection.find().exec(), COLLECTION_READ_TIMEOUT_MS, 'collection read timed out');
    if (collectionName) markCollectionDiagnostic(collectionName, 'read', 'ok', `${docs.length} rows`);
    return docs.map(toJson);
  } catch (error) {
    if (collectionName) {
      markCollectionDiagnostic(
        collectionName,
        'read',
        isBusinessOsPermissionDenied(error) ? 'denied' : 'failed',
        isBusinessOsPermissionDenied(error) ? state.t('collectionLocked', 'Keine Datenfreigabe') : errorMessage(error),
      );
    }
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
    if (read?.kind === 'denied') return { collection, kind: isOptionalResearchCollection(collection) ? 'locked' : 'missing', label: read.message };
    if (read?.kind === 'missing') return { collection, kind: isOptionalResearchCollection(collection) ? 'pending' : 'missing', label: read.message };
    return { collection, kind: 'pending', label: t('pendingShort', 'wartet') };
  });
}

function diagnosticFailures() {
  return diagnosticRows()
    .filter((row) => RESEARCH_REQUIRED_COLLECTIONS.includes(row.collection))
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

function sleep(ms) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
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

function researchDomainFromFormValue(rawDomain, knowledgeBases = [], fallback = 'research') {
  const selected = String(rawDomain || '').trim();
  if (selected && knowledgeBases.some((base) => base.domain === selected)) return selected;
  return normalizeResearchDomain(selected || fallback);
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

function formatMeasurementNumber(value, maximumFractionDigits = 2) {
  if (!isPresent(value)) return '';
  const next = Number(value);
  if (!Number.isFinite(next)) return '';
  return next.toLocaleString('de-DE', {
    useGrouping: false,
    maximumFractionDigits,
  });
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
  backdrop.className = "ctox-modal";
  // Above module modals (240), below shell notifications (260).
  backdrop.style.zIndex = "250";
  backdrop.innerHTML = `
    <div class="ctox-modal-card">
      <header class="ctox-modal-header">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">KI-Generierung</span>
          <h3 class="ctox-modal-title">System-Prompt des Fachberichts</h3>
        </div>
        <button type="button" class="ctox-button" onclick="this.closest('.ctox-modal').remove()">Schließen</button>
      </header>
      <div class="ctox-modal-body">
        <div style="font-family: var(--font-mono, monospace); font-size: 11px; line-height: 1.6; color: var(--research-text); max-height: 380px; overflow-y: auto; white-space: pre-wrap; background: var(--research-surface-2); padding: 12px; border-radius: 6px; border: 1px solid var(--research-line); text-align: left;">\${escapeHtml(promptText)}</div>
      </div>
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
        lines[i - 1] = `<table>${tableRows.join('')}</table>`;
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
    .replace(/&lt;table&gt;/gi, '<table>').replace(/&lt;\/table&gt;/gi, '</table>')
    .replace(/&lt;tr&gt;/gi, '<tr>').replace(/&lt;\/tr&gt;/gi, '</tr>')
    .replace(/&lt;th&gt;/gi, '<th>').replace(/&lt;\/th&gt;/gi, '</th>')
    .replace(/&lt;td&gt;/gi, '<td>').replace(/&lt;\/td&gt;/gi, '</td>');
  
  return html;
}

// Load a Fachbericht's content from RxDB (NO HTTP). The reports are the same
// documents that replicate into the `documents` collection over RxDB/WebRTC;
// `index_text` holds the document text, with a blob-chunk fallback — all RxDB.
async function loadReportContentFromRxdb(filename) {
  const documents = readableCollection('documents');
  if (!documents) {
    throw new Error(canReadCollection('documents')
      ? 'RxDB-Dokumente nicht verfügbar'
      : 'Keine Datenfreigabe für Dokumente');
  }
  const matches = await documents.find({ selector: { filename } }).exec();
  const rows = matches.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d));
  const json = rows.find((d) => !d.is_deleted) || rows[0];
  if (!json) {
    throw new Error(`Dokument ${filename} (noch) nicht synchronisiert`);
  }
  if (typeof json.index_text === 'string' && json.index_text.trim()) {
    return json.index_text;
  }
  // Fallback: reconstruct from the current version's blob chunks (RxDB only).
  const versions = readableCollection('document_versions');
  const blobChunks = readableCollection('document_blob_chunks');
  const versionId = json.current_version_id;
  if (!versions || !blobChunks) {
    throw new Error(canReadCollection('document_versions') && canReadCollection('document_blob_chunks')
      ? 'RxDB-Dokumentversionen nicht verfügbar'
      : 'Keine Datenfreigabe für Dokumentinhalte');
  }
  if (versionId) {
    const version = await versions.findOne(versionId).exec();
    const blobId = version && typeof version.toJSON === 'function' ? version.toJSON().blob_id : null;
    if (blobId) {
      const chunkDocs = await blobChunks.find({ selector: { blob_id: blobId } }).exec();
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
  if (!evidenceRankedSources().length) {
    return `
      <section class="research-empty research-empty-card" data-report-gate="blocked">
        <strong>${escapeHtml(state.t('reportUnavailable', 'Reports nicht verfügbar'))}</strong>
        <span>${escapeHtml(state.t('reportRequiresVerifiedSources', 'Erst verifizierte Quellen mit vollständigem Evidence-Gate machen Reports verfügbar.'))}</span>
      </section>
    `;
  }
  const reports = researchReportsForTask(task);
  if (!reports.length) {
    return `
      <section class="research-empty research-empty-card" data-report-gate="ready-empty">
        <strong>${escapeHtml(state.t('noResearchReports', 'Noch keine verknüpften Fachberichte'))}</strong>
        <span>${escapeHtml(state.t('createResearchReportHint', 'Erstelle einen Bericht aus dem verifizierten Research-Graph. Er erscheint nach der Documents-Synchronisierung hier.'))}</span>
      </section>
    `;
  }
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
    ? `<div class="research-report-loading"><span class="research-spinner"></span>Lade Fachbericht...</div>`
    : content.startsWith('Fehler')
      ? `<div class="research-report-error">${escapeHtml(content)}</div>`
      : `
        <div class="ai-warning-banner">
          <div class="research-ai-banner-row">
            <div class="research-ai-banner-title">
              <div>
                <strong>${escapeHtml(state.t('evidenceBackedReport', 'Evidence-basierter Fachbericht'))}</strong>
                <span>${evidenceRankedSources().length} ${escapeHtml(state.t('verifiedSources', 'verifizierte Quellen'))} · ${filterMeasurementRowsForEvidence(state.measurementRows, state.sourceModels).length} ${escapeHtml(state.t('traceableMeasurements', 'nachverfolgbare Messpunkte'))}</span>
              </div>
            </div>
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

function researchReportsForTask(task, documents = state.documents) {
  if (!task?.id || !task.knowledge_domain) return [];
  return (documents || [])
    .filter((document) => !document.is_deleted && document.filename)
    .filter((document) => documentLinksToResearch(document, task))
    .map((document) => ({
      id: String(document.id),
      filename: String(document.filename),
      title: String(document.title || document.filename),
      category: String(document.document_type || state.t('report', 'Fachbericht')),
      updated_at_ms: Number(document.updated_at_ms || document.created_at_ms || 0),
    }))
    .sort((a, b) => b.updated_at_ms - a.updated_at_ms || a.title.localeCompare(b.title));
}

function documentLinksToResearch(document, task) {
  return (document.linked_records || []).some((record) => {
    const kind = String(record?.kind || record?.type || record?.record_type || '').toLowerCase();
    const id = String(record?.id || record?.record_id || record?.value || '');
    return (kind === 'research_task' && id === task.id)
      || (kind === 'knowledge_domain' && id === task.knowledge_domain);
  });
}

export const __researchTestHooks = {
  buildSourceModels,
  collectionDiagnosticRows,
  diagnosticRows,
  disabledTabButton,
  evidenceGate,
  eligibleGraphFocusSourceIds,
  filterGraphRowsForEvidence,
  filterMeasurementRowsForEvidence,
  formatDimensionScore,
  formatPortfolioScore,
  hasVerifiedEvidence: () => evidenceRankedSources().length > 0,
  knowledgeBasesFromTables,
  knowledgeRefreshPayload,
  latestEvidenceRunForTask,
  researchScoringContract,
  researchReportsForTask,
  renderSourcesTable,
  renderNoTaskCenter,
  researchDomainFromFormValue,
  shouldRetryEmptyKnowledgeTables,
  validateResearchTaskInput,
  validateSelectedResearchTask,
};
