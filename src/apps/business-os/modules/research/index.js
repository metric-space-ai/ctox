import { loadModuleMessages } from '../../shared/i18n.js';
import { renderListOrState } from '../../shared/list-state.js';
import {
  buildResearchGraphProjection,
  GRAPH_DETAIL_LEVELS,
  GRAPH_NODE_KINDS,
  GRAPH_RELATION_TYPES,
  sliceResearchGraphProjection,
} from './research-graph-data.mjs';

// Modul-eigener Stempel: er bustet index.css und die Locale-Dateien, die
// sonst ohne Query-Parameter geladen und vom Edge bis zu vier Stunden alt
// ausgeliefert werden (Befund skf.ctox.dev 02.09.2026). Bei jeder Aenderung
// an index.css oder locales/ hochzaehlen.
const BUILD = '20260903-research-claims-evaluation-v98';
const DEFAULT_AXIS_X = 'evidence_strength';
const DEFAULT_AXIS_Y = 'topic_fit';
const ROW_LIMIT = 5000;
const KNOWLEDGE_CATALOG_FETCH_CONCURRENCY = 3;
// 10 s reichten nicht, sobald Knowledge-Chunks (bis 256 KB je Dokument) den
// selben WebRTC-Kanal fuellen: die Pflicht-Collections liefen dann in den
// Timeout und die Sicht kippte in den Fehlerzustand (skf.ctox.dev, 02.09.2026).
const COLLECTION_READ_TIMEOUT_MS = 30000;
const POST_SYNC_REFRESH_LIMIT = 1;
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
const RECEIPT_URL_ROLES = new Set(['original_content', 'original_data', 'publisher_full_text', 'dataset_archive']);
const RECEIPT_CONTENT_SCOPES = new Set(['full_text', 'original_data', 'data_file', 'full_dataset', 'dataset_archive']);

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
  source_candidates: {
    title: 'Discovery Candidates',
    columns: [
      'source_id',
      'title',
      'source_url',
      'source_type',
      'publisher',
      'discovery_query',
      'discovered_at',
      'canonical_url',
      'source_tier',
      'verification_status',
      'http_status',
      'snapshot_hash',
      'evidence_eligible',
      'evidence_rejection_reason',
      'review_status',
      'discovery_round',
      'discovery_method',
      'seed_source_id',
      'seed_identifier',
      'citation_hop',
      'citation_direction',
      'relation_type',
      'discovery_paths_json',
    ],
  },
  source_catalog: {
    title: 'Verified Source Registry',
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
      'canonical_url',
      'snapshot_id',
      'snapshot_path',
      'snapshot_hash',
      'evidence_id',
      'claim_id',
      'retrieved_at',
      'url_role',
      'content_scope',
      'verification_status',
      'transport_verified',
      'content_extracted',
      'actual_full_text_or_data',
      'evidence_relevance_score',
      'http_status',
      'evidence_eligible',
      'source_tier',
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
      'canonical_url',
      'snapshot_id',
      'snapshot_path',
      'snapshot_hash',
      'claim_id',
      'retrieved_at',
      'url_role',
      'content_scope',
      'verification_status',
      'transport_verified',
      'content_extracted',
      'actual_full_text_or_data',
      'evidence_relevance_score',
      'http_status',
      'evidence_eligible',
      'source_tier',
    ],
  },
  claims: {
    title: 'Claims',
    columns: [
      'claim_id',
      'claim_text',
      'statement_type',
      'evidence_id',
      'source_id',
      'exact_short_quote_or_table_ref',
      'confidence',
      'limitations',
      'knowledge_book',
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
      'description',
      'aliases_json',
      'cluster_id',
      'cluster_label',
      'occurrences',
      'evidence_count',
      'betweenness_centrality',
      'confidence',
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
      'relation_type',
      'label',
      'weight',
      'confidence',
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
  measurementMode: 'derived',
  mapMode: 'discovery',
  candidateRows: [],
  candidateModels: [],
  sourceRows: [],
  curatedRows: [],
  claimRows: [],
  evidenceRows: [],
  knowledgeTopic: '',
  knowledgeType: 'all',
  measurementRows: [],
  derivedMeasurementRows: [],
  graphNodeRows: [],
  graphEdgeRows: [],
  sourceModels: [],
  graphProjection: null,
  graphContractStatus: '',
  graphContractErrors: [],
  graphProjectionCache: new Map(),
  graphSurface: null,
  graphMountToken: 0,
  knowledgeRefreshInFlight: false,
  selectedGraphNodeId: '',
  graph: {
    dimensions: 3,
    detailLevel: 'standard',
    visibleLimit: GRAPH_DETAIL_LEVELS.standard,
    layer: 'all',
    panel: 'hidden',
    query: '',
    autoRotate: false,
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
    failureRetries: 0,
    failureRetryAt: 0,
    loadedOnce: false,
  },
  initialDataReady: false,
  readiness: {},
  refreshInFlight: null,
  refreshDirty: false,
  researchRefreshTimer: null,
  knowledgeRefreshTimer: null,
  refreshSequences: {
    research: 0,
    knowledge: 0,
  },
  rowLimitWarnings: [],
  chunkDiagnostics: [],
  knowledgeTableRowStates: {},
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
  const messages = await loadResearchMessages(ctx.locale);
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
  // `wireReadiness` + `schedulePostSyncRefresh` + `wireRealtime` refresh once
  // data lands.
  wireRealtime();
  wireReadiness();
  startResearchCollections(mountToken)
    .then(async () => {
      if (state.mountToken !== mountToken) return;
      const hadInitialData = state.initialDataReady;
      if (!hadInitialData) await refreshAll({ seed: true, mountToken });
      if (state.mountToken !== mountToken) return;
      state.initialDataReady = true;
      render();
      if (hadInitialData) scheduleKnowledgeRefresh(250);
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
  refreshAll({ seed: true, retryEmptyKnowledge: false, mountToken })
    .then(() => {
      if (state.mountToken === mountToken) state.initialDataReady = true;
    })
    .catch((error) => {
      if (state.mountToken === mountToken) console.warn('[research] initial background refresh failed', error);
    });
  schedulePostSyncRefresh(1200);
  return () => {
    if (state.mountToken === mountToken) state.mountToken = null;
    abortKnowledgeRowFetches('unmount');
    for (const lease of state.syncLeases) lease?.release?.().catch?.(() => null);
    state.syncLeases.clear();
    // Cleanup globals
    delete window.selectReport;
    delete window.showPromptViewer;
    
    state.cleanup.forEach((fn) => fn?.());
    state.cleanup = [];
    disposeResearchGraph();
    if (state.researchRefreshTimer) window.clearTimeout(state.researchRefreshTimer);
    if (state.knowledgeRefreshTimer) window.clearTimeout(state.knowledgeRefreshTimer);
    state.researchRefreshTimer = null;
    state.knowledgeRefreshTimer = null;
    state.refreshSequences.research += 1;
    state.refreshSequences.knowledge += 1;
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

// Die Sync-Phase wurde nur beim Mount bewertet: lief die Erstreplikation einer
// Pflicht-Collection laenger als 20 s (Daemon mit Recherche-Worker beschaeftigt),
// blieb "data sync did not become ready in time" fuer die ganze Sitzung stehen,
// obwohl die Daten laengst lokal lesbar waren (skf.ctox.dev, 02.09.2026).
// Jeder Reload prueft gescheiterte Collections erneut: erst ueber den
// Readiness-Schnappschuss der Shell, sonst ueber die Bridge selbst.
async function reprobeFailedSyncCollections(mountToken = state.mountToken) {
  const failed = RESEARCH_REQUIRED_COLLECTIONS.filter(
    (collection) => state.diagnostics.collections[collection]?.sync?.kind === 'failed',
  );
  if (!failed.length) return;
  await Promise.all(failed.map(async (collection) => {
    if (collectionReadiness(collection)?.ready === true) {
      markCollectionDiagnostic(collection, 'sync', 'ok', state.t('syncReady', 'Sync bereit'));
      return;
    }
    if (typeof state.ctx?.sync?.startCollection !== 'function' || RESEARCH_DEMAND_ONLY_COLLECTIONS.has(collection)) return;
    try {
      const bridge = await state.ctx.sync.startCollection(collection);
      if (mountToken && state.mountToken !== mountToken) return;
      if (bridge) await waitForReplicationBridge(bridge, collection);
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
      window.setTimeout(() => reject(new Error(`${collection} data sync did not become ready in time`)), timeoutMs);
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
      selectSourceFromUi(target.dataset.sourceId || '');
    } else if (action === 'tab') {
      state.activeTab = target.dataset.tab || 'sources';
      refreshWorkbenchForActiveTab();
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
      state.graph.layer = target.dataset.graphLayer || 'all';
      refreshGraphProjectionInPlace();
    } else if (action === 'graph-detail') {
      state.graph.detailLevel = target.dataset.graphDetail || 'standard';
      state.graph.visibleLimit = GRAPH_DETAIL_LEVELS[state.graph.detailLevel] || GRAPH_DETAIL_LEVELS.standard;
      refreshGraphProjectionInPlace();
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
    } else if (action === 'retry-knowledge-rows') {
      await retryKnowledgeTableRows(target.dataset.tableId || '');
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
      await focusCtoxRun(
        target.dataset.taskQueueId || '',
        target.dataset.commandId || '',
        target.dataset.taskStatus || '',
      );
    } else if (action === 'sources-view') {
      state.sourcesViewMode = target.dataset.viewMode || 'shards';
      refreshWorkbenchForActiveTab();
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
      refreshSourcesWorkbenchInPlace();
    } else if (action === 'measurement-mode') {
      state.measurementMode = target.dataset.measurementMode === 'direct' ? 'direct' : 'derived';
      refreshMeasurementWorkbenchInPlace();
    } else if (action === 'knowledge-book') {
      state.knowledgeTopic = target.dataset.knowledgeBook || '';
      renderCenter();
    } else if (action === 'knowledge-type') {
      state.knowledgeType = target.dataset.knowledgeType || 'all';
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
    const graphSearch = event.target.closest('[data-action="graph-search"]');
    if (graphSearch) {
      state.graph.query = graphSearch.value;
      state.graphSurface?.search?.(graphSearch.value);
      return;
    }
    const searchInput = event.target.closest('[data-action="source-search"]');
    if (searchInput) {
      const selectionStart = searchInput.selectionStart;
      const selectionEnd = searchInput.selectionEnd;
      state.sourceSearchTerm = searchInput.value;
      refreshSourcesWorkbenchInPlace();
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

function refreshAll(options = {}) {
  if (state.refreshInFlight) {
    state.refreshDirty = true;
    return state.refreshInFlight;
  }
  const run = refreshAllNow(options);
  state.refreshInFlight = run;
  return run.finally(() => {
    state.refreshInFlight = null;
    if (state.refreshDirty && state.mountToken) {
      state.refreshDirty = false;
      refreshAll({ ...options, seed: false }).catch((error) => console.warn('[research] deduplicated refresh failed', error));
    }
  });
}

async function refreshAllNow({ seed = false, retryEmptyKnowledge = true, mountToken = null } = {}) {
  state.diagnostics.reloadStartedAt = Date.now();
  state.diagnostics.reloadFinishedAt = 0;
  state.diagnostics.reloadCount += 1;
  setStatus(state.t('loadingKnowledge', 'Knowledge wird geladen...'));
  await reprobeFailedSyncCollections(mountToken);
  if (mountToken && state.mountToken !== mountToken) return;
  await loadLocalState({ mountToken });
  if (mountToken && state.mountToken !== mountToken) return;
  const knowledgeBases = await loadKnowledgeBases({
    retryEmpty: retryEmptyKnowledge,
    domains: activeResearchDomains(),
  });
  if (mountToken && state.mountToken !== mountToken) return;
  state.knowledgeBases = knowledgeBases;
  if (seed) await ensureTasksFromKnowledgeBases();
  if (mountToken && state.mountToken !== mountToken) return;
  if (!state.selectedTaskId || !state.tasks.some((task) => task.id === state.selectedTaskId)) {
    state.selectedTaskId = state.tasks[0]?.id || '';
  }
  await loadDashboardData();
  if (mountToken && state.mountToken !== mountToken) return;
  state.diagnostics.reloadFinishedAt = Date.now();
  state.diagnostics.loadedOnce = true;
  scheduleFailureRetry(mountToken);
  render();
  refreshOpenTaskDialogDomainOptions();
  setStatus(reloadStatusText());
}

// Ein Sync- oder Lesefehler (Timeout beim Demand-Fetch, Bridge nicht bereit)
// blieb bisher als "Research ist gerade nicht verfuegbar" stehen, bis jemand
// "Daten neu laden" drueckte - und die Sicht zeigte solange ueberall 0
// (skf.ctox.dev, 02.09.2026). Ein gestoerter Reload plant jetzt selbst den
// naechsten Versuch: 5 s, 10 s, 20 s, 40 s, dann jede Minute.
const FAILURE_RETRY_BASE_MS = 5000;
const FAILURE_RETRY_MAX_MS = 60000;

function failureRetryDelay(attempt) {
  return Math.min(FAILURE_RETRY_MAX_MS, FAILURE_RETRY_BASE_MS * (2 ** Math.max(0, attempt)));
}

function scheduleFailureRetry(mountToken = state.mountToken) {
  if (!diagnosticFailures().length) {
    state.diagnostics.failureRetries = 0;
    state.diagnostics.failureRetryAt = 0;
    return;
  }
  if (!mountToken || state.mountToken !== mountToken) return;
  const delay = failureRetryDelay(state.diagnostics.failureRetries);
  state.diagnostics.failureRetries += 1;
  state.diagnostics.failureRetryAt = Date.now() + delay;
  queueKnowledgeRefreshAfter(delay);
}

// Der Datenzustand des Moduls in einem Wort: "failed" (eine Pflicht-Collection
// meldet einen Fehler), "syncing" (noch kein abgeschlossener Reload oder die
// Knowledge-Collection ist nicht bereit) oder "ready". Zahlen werden nur im
// Zustand "ready" gezeigt - eine "0" waehrend der Synchronisation las sich als
// leere Knowledge Base.
// Ein Lesefehler bei vorhandenen lokalen Daten ist eine verzoegerte
// Aktualisierung, kein Datenverlust: die Sicht bleibt auf dem letzten Stand.
function hasLocalResearchData() {
  return state.tasks.length > 0 && state.knowledgeBases.length > 0;
}

function researchDataState() {
  if (diagnosticFailures().length && !hasLocalResearchData()) return 'failed';
  // Nur der allererste Reload zaehlt: jeder spaetere Reload setzt
  // reloadFinishedAt zurueck, und die Sicht wuerde bei jeder Hintergrund-
  // Aktualisierung von Zahlen auf Auslassungszeichen springen.
  if (!state.diagnostics.loadedOnce) return 'syncing';
  const readiness = collectionReadiness('knowledge_tables');
  if (readiness && readiness.ready === false && !state.knowledgeBases.length) return 'syncing';
  return 'ready';
}

function countText(value) {
  if (researchDataState() !== 'ready') return '…';
  return Number(value || 0).toLocaleString(state.lang === 'de' ? 'de-DE' : 'en-US');
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
    state.tasks = collapseResearchTaskLineages(tasks.filter((task) => isVisibleResearchTask(task)));
  }
  if (runs.length || !state.runs.length) state.runs = runs;
  if (notes.length || !state.notes.length) state.notes = notes;
  if (commands.length || !state.commands.length) state.commands = commands;
  if (queueTasks.length || !state.queueTasks.length) state.queueTasks = queueTasks;
  if (documents.length || !state.documents.length) state.documents = documents;
}

function wireRealtime() {
  const knowledgeLifecycleCollections = new Set([
    'research_tasks',
    'research_runs',
    'business_commands',
    'ctox_queue_tasks',
  ]);
  const collections = [
    ['research_tasks', readableCollection('research_tasks')],
    ['research_runs', readableCollection('research_runs')],
    ['research_notes', readableCollection('research_notes')],
    ['business_commands', readableCollection('business_commands')],
    ['ctox_queue_tasks', readableCollection('ctox_queue_tasks')],
    ['documents', readableCollection('documents')],
  ].filter(([, collection]) => collection);
  for (const [name, collection] of collections) {
    const subscription = collection.$?.subscribe?.(() => {
      scheduleLocalRefresh(80);
      if (knowledgeLifecycleCollections.has(name)) scheduleKnowledgeRefresh(250);
    });
    if (subscription?.unsubscribe) state.cleanup.push(() => subscription.unsubscribe());
  }
}

function collectionReadiness(name) {
  if (state.readiness[name]) return state.readiness[name];
  const read = state.ctx?.sync?.collectionReadiness;
  return typeof read === 'function' ? read.call(state.ctx.sync, name) : null;
}

// Shared gate for data-driven empties: only an empty, unfiltered replicated
// source may downgrade to the syncing shell, and only while its backing
// collection reports ready === false. Selection-, filter-, permission- and
// error-empties never call this.
function dataEmptyShowsSyncing(isEmpty, readiness) {
  return Boolean(isEmpty) && readiness?.ready === false;
}

// Canonical readiness subscription (replaces the former private
// 'ctox-business-os-sync-diagnostics' window-event probe): stores one
// snapshot per required collection as a render hint and refreshes the local
// knowledge projection once a collection finishes its initial replication.
function wireReadiness() {
  const subscribe = state.ctx?.sync?.subscribeCollectionReadiness;
  if (typeof subscribe !== 'function') return;
  for (const name of RESEARCH_REQUIRED_COLLECTIONS) {
    let wasReady = null;
    const unsubscribe = subscribe.call(state.ctx.sync, name, (snapshot) => {
      if (!snapshot) return;
      const becameReady = wasReady === false && snapshot.ready === true;
      wasReady = snapshot.ready === true;
      state.readiness[name] = snapshot;
      // Die Sync-Phase beim Mount wartet hoechstens 20 s auf die Bridge; laeuft
      // parallel der Knowledge-Fetch (bis 80 MB je Domain), verpasst sie das
      // "in sync" regelmaessig. Sobald die Shell die Collection bereit meldet,
      // ist der Mount-Fehler erledigt - ohne auf den naechsten Reload zu warten.
      if (snapshot.ready === true && state.diagnostics.collections[name]?.sync?.kind === 'failed') {
        markCollectionDiagnostic(name, 'sync', 'ok', state.t('syncReady', 'Sync bereit'));
        if (!diagnosticFailures().length) {
          state.diagnostics.failureRetries = 0;
          state.diagnostics.failureRetryAt = 0;
          setStatus(reloadStatusText());
        }
      }
      if (becameReady) scheduleKnowledgeRefresh(250);
      render();
    });
    if (typeof unsubscribe === 'function') state.cleanup.push(unsubscribe);
  }
}

function schedulePostSyncRefresh(delay = 250) {
  if (state.diagnostics.postSyncRefreshes >= POST_SYNC_REFRESH_LIMIT) return;
  state.diagnostics.postSyncRefreshes += 1;
  scheduleKnowledgeRefresh(delay);
}

function scheduleLocalRefresh(delay = 80) {
  if (state.researchRefreshTimer) window.clearTimeout(state.researchRefreshTimer);
  const sequence = ++state.refreshSequences.research;
  const mountToken = state.mountToken;
  state.researchRefreshTimer = window.setTimeout(async () => {
    if (sequence !== state.refreshSequences.research) return;
    state.researchRefreshTimer = null;
    if (!mountToken || state.mountToken !== mountToken) return;
    await loadLocalState({ mountToken });
    if (state.mountToken !== mountToken) return;
    render();
  }, delay);
}

function scheduleKnowledgeRefresh(delay = 120) {
  if (state.knowledgeRefreshTimer) window.clearTimeout(state.knowledgeRefreshTimer);
  const sequence = ++state.refreshSequences.knowledge;
  const mountToken = state.mountToken;
  state.knowledgeRefreshTimer = window.setTimeout(async () => {
    if (sequence !== state.refreshSequences.knowledge) return;
    state.knowledgeRefreshTimer = null;
    if (!mountToken || state.mountToken !== mountToken) return;
    // Demand-loading knowledge chunks writes them into IndexedDB and emits the
    // same collection stream that schedules this refresh. Ignore those
    // self-generated events while a complete snapshot is being assembled;
    // otherwise overlapping loads can publish an evicted, partial snapshot.
    if (state.knowledgeRefreshInFlight) return;
    state.knowledgeRefreshInFlight = true;
    try {
      await loadLocalState({ mountToken });
      if (state.mountToken !== mountToken) return;
      const knowledgeBases = await loadKnowledgeBases({ domains: activeResearchDomains() });
      if (state.mountToken !== mountToken) return;
      state.knowledgeBases = knowledgeBases;
      await ensureTasksFromKnowledgeBases();
      if (state.mountToken !== mountToken) return;
      if (!state.selectedTaskId || !state.tasks.some((task) => task.id === state.selectedTaskId)) {
        state.selectedTaskId = state.tasks[0]?.id || '';
      }
      await loadDashboardData();
      if (state.mountToken !== mountToken) return;
      render();
      refreshOpenTaskDialogDomainOptions();
    } finally {
      state.knowledgeRefreshInFlight = false;
    }
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
  if (isDeletedResearchTask(task)) return false;
  if (/^outbound(?:_|$)/.test(String(task.knowledge_domain || ''))) return false;
  if (!task?.payload?.seeded_from_knowledge) return true;
  const base = state.knowledgeBases.find((item) => item.domain === task.knowledge_domain);
  return Boolean(base && isResearchKnowledgeBase(base));
}

// Ein geloeschter Task (Soft-Delete aus dem Modul oder RxDB-Tombstone) ist
// kein Lineage-Kandidat: gruppiert nach knowledge_domain gewann sonst der
// zuletzt aktualisierte, aber geloeschte Task ueber die lebenden Tasks seiner
// Domain und stand mit fremdem Titel und fremdem Lauf als aktiv in der Liste
// (Befund skf.ctox.dev, 02.09.2026).
// Locale-Dateien mit dem Modul-Stempel laden: `loadModuleMessages` haengt
// keinen Cache-Buster an, und der Shell-Proxy cacht `locales/*.json` am Edge.
async function loadResearchMessages(locale) {
  const lang = locale === 'en' ? 'en' : 'de';
  try {
    const url = new URL(`locales/${lang}.json`, new URL('./', import.meta.url));
    url.searchParams.set('v', BUILD);
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    return await response.json();
  } catch {
    return loadModuleMessages(import.meta.url, locale, {});
  }
}

function isDeletedResearchTask(task) {
  if (!task || typeof task !== 'object') return true;
  if (task._deleted === true || task.is_deleted === true) return true;
  return String(task.status || '').trim().toLowerCase() === 'deleted';
}

function collapseResearchTaskLineages(tasks = []) {
  const byDomain = new Map();
  for (const task of tasks.filter((entry) => !isDeletedResearchTask(entry))) {
    const key = String(task.knowledge_domain || task.domain || task.id || '').trim();
    const bucket = byDomain.get(key) || [];
    bucket.push(task);
    byDomain.set(key, bucket);
  }
  return [...byDomain.values()]
    .map((bucket) => {
      const sorted = [...bucket].sort((a, b) => (
        Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0)
      ));
      return {
        ...sorted[0],
        lineage_task_ids: sorted.map((task) => task.id).filter(Boolean),
      };
    })
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
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
      criteria: state.t('defaultCriteriaText', 'Nutze die vorhandene Knowledge Base als Ausgangspunkt und trenne Rohkandidaten von belegten Quellen. Werte JEDE aufgenommene Quelle inhaltlich aus: Relevanzurteil (core/context/off_topic) und 3-10 belegte Aussagen mit wörtlichem Zitat, Fundstelle, Aussagenart und Grenzen in die Tabelle claims. Fasse anschließend gleiche Aussagen aus mehreren Quellen zu einem Claim mit mehreren Belegen zusammen und weise Widersprüche aus. Eine verifizierte, aber nicht ausgewertete Quelle gilt als offene Arbeit.'),
      status: 'ready',
      knowledge_domain: base.domain,
      candidate_catalog_key: tableKey(base, ['source_candidates']) || 'source_candidates',
      source_catalog_key: tableKey(base, ['source_catalog', 'sources', 'curated_sources']) || 'source_catalog',
      curated_table_key: tableKey(base, ['evaluation_matrix', 'load_data_library', 'curated_sources', 'source_library']) || 'evaluation_matrix',
      claims_table_key: tableKey(base, ['claims']) || 'claims',
      evidence_table_key: tableKey(base, ['evidence_points']) || 'evidence_points',
      measurements_table_key: defaultMeasurementsTableKey(base),
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

async function loadKnowledgeBases({ retryEmpty = true, domains = [] } = {}) {
  const tables = await loadKnowledgeTables({ retryEmpty, domains });
  return knowledgeBasesFromTables(tables);
}

function activeResearchDomains() {
  return [...new Set(state.tasks
    .map((task) => String(task?.knowledge_domain || '').trim())
    .filter(Boolean))];
}

function knowledgeBasesFromTables(tables = []) {
  const byDomain = new Map();
  for (const rawTable of mergeKnowledgeTableChunks(tables)) {
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
    if (isKnowledgeCatalogDocument(table)) attachCachedCatalogRows(table);
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

function mergeKnowledgeTableChunks(tables = []) {
  const groups = new Map();
  for (const rawTable of Array.isArray(tables) ? tables : []) {
    if (!rawTable || typeof rawTable !== 'object') continue;
    const source = rawTable.payload && typeof rawTable.payload === 'object' && !Array.isArray(rawTable.payload)
      ? rawTable.payload
      : rawTable;
    const logicalId = String(
      source.logical_table_id
      || rawTable.logical_table_id
      || source.id
      || rawTable.id
      || '',
    ).trim();
    if (!logicalId) continue;
    if (!groups.has(logicalId)) groups.set(logicalId, []);
    groups.get(logicalId).push({ rawTable, source });
  }

  return [...groups.entries()].map(([logicalId, parts]) => {
    if (parts.every(({ rawTable, source }) => isKnowledgeCatalogDocument(rawTable) || isKnowledgeCatalogDocument(source))) {
      return mergeKnowledgeCatalogParts(logicalId, parts);
    }
    parts.sort((left, right) => (
      Number(left.source.chunk_index ?? left.rawTable.chunk_index ?? 0)
      - Number(right.source.chunk_index ?? right.rawTable.chunk_index ?? 0)
    ));
    const first = parts[0];
    const rows = parts.flatMap(({ rawTable, source }) => firstArray(
      source.rows,
      source.records,
      source.data,
      rawTable.rows,
      rawTable.records,
      rawTable.data,
    ));
    const expectedChunks = Math.max(...parts.map(({ rawTable, source }) => (
      Number(source.chunk_count ?? rawTable.chunk_count ?? 1)
    )).filter(Number.isFinite), 1);
    const payload = {
      ...first.source,
      id: logicalId,
      logical_table_id: logicalId,
      chunk_index: 0,
      chunk_count: expectedChunks,
      chunk_row_offset: 0,
      chunk_row_count: rows.length,
      projected_row_count: rows.length,
      rows_complete: parts.length === expectedChunks && parts.every(({ rawTable, source }) => (
        (source.rows_complete ?? rawTable.rows_complete ?? true) !== false
      )),
      rows,
    };
    return {
      ...first.rawTable,
      ...payload,
      payload,
    };
  });
}

function isKnowledgeCatalogDocument(record) {
  if (!record || typeof record !== 'object' || Array.isArray(record)) return false;
  const payload = record.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : null;
  const version = record.projection_version ?? payload?.projection_version;
  const rowsSource = record.rows_source ?? payload?.rows_source;
  const marked = version === 2 || version === '2' || rowsSource === 'rxdb.rows.fetch';
  if (!marked) return false;
  return !hasEmbeddedKnowledgeRows(record);
}

function hasEmbeddedKnowledgeRows(record) {
  if (!record || typeof record !== 'object') return false;
  const payload = record.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : null;
  const candidates = [
    record.rows,
    record.records,
    record.data,
    record.dataframe?.rows,
    record.dataframe?.records,
    record.dataframe?.data,
    payload?.rows,
    payload?.records,
    payload?.data,
    payload?.dataframe?.rows,
    payload?.dataframe?.records,
    payload?.dataframe?.data,
  ];
  return candidates.some((value) => Array.isArray(value) && value.length > 0);
}

function catalogRowCount(record) {
  const payload = record?.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : null;
  const value = record?.row_count ?? payload?.row_count;
  const number = Number(value);
  if (!Number.isFinite(number) || number < 0) return null;
  return number;
}

function knowledgeContentHash(record) {
  const payload = record?.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : null;
  const value = record?.content_hash ?? payload?.content_hash ?? '';
  return value == null ? '' : String(value);
}

function logicalKnowledgeTableId(record) {
  const source = record?.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : record;
  return String(
    source?.logical_table_id
    || record?.logical_table_id
    || source?.id
    || record?.id
    || '',
  ).trim();
}

function omitEmbeddedKnowledgeRows(record) {
  if (!record || typeof record !== 'object' || Array.isArray(record)) return {};
  const next = { ...record };
  delete next.rows;
  delete next.records;
  delete next.data;
  delete next.rows_origin;
  delete next.rows_content_hash;
  if (next.dataframe && typeof next.dataframe === 'object' && !Array.isArray(next.dataframe)) {
    next.dataframe = { ...next.dataframe };
    delete next.dataframe.rows;
    delete next.dataframe.records;
    delete next.dataframe.data;
  }
  if (next.payload && next.payload !== record && typeof next.payload === 'object' && !Array.isArray(next.payload)) {
    next.payload = omitEmbeddedKnowledgeRows(next.payload);
  }
  return next;
}

function mergeKnowledgeCatalogParts(logicalId, parts) {
  const first = parts[0];
  const rowCount = parts
    .map(({ rawTable, source }) => catalogRowCount(source) ?? catalogRowCount(rawTable))
    .find((value) => value != null);
  const payload = omitEmbeddedKnowledgeRows({
    ...(first.source || {}),
    id: logicalId,
    logical_table_id: logicalId,
    rows_complete: true,
  });
  delete payload.chunk_index;
  delete payload.chunk_count;
  delete payload.chunk_row_offset;
  delete payload.chunk_row_count;
  delete payload.projected_row_count;
  if (rowCount != null) payload.row_count = rowCount;
  return {
    ...omitEmbeddedKnowledgeRows(first.rawTable || {}),
    ...payload,
    payload,
  };
}

function isKnowledgeTableBaseDocument(document) {
  if (isKnowledgeCatalogDocument(document)) return true;
  const source = document?.payload && typeof document.payload === 'object' && !Array.isArray(document.payload)
    ? document.payload
    : document;
  const chunkIndex = Number(source?.chunk_index ?? document?.chunk_index ?? 0);
  return !Number.isFinite(chunkIndex) || chunkIndex === 0;
}

const knowledgeTableLoads = new Map();
const knowledgeRowsCache = new Map();
let knowledgeRowsFetchController = null;
let knowledgeRowsFetchDomainKey = '';

function restartKnowledgeRowFetches(domains = []) {
  const domainKey = JSON.stringify([...new Set((domains || [])
    .map((domain) => String(domain || '').trim())
    .filter(Boolean))].sort());
  const reason = knowledgeRowsFetchDomainKey && knowledgeRowsFetchDomainKey !== domainKey
    ? 'domain-change'
    : 'reload';
  abortKnowledgeRowFetches(reason);
  knowledgeRowsFetchController = new AbortController();
  knowledgeRowsFetchDomainKey = domainKey;
  return knowledgeRowsFetchController.signal;
}

function abortKnowledgeRowFetches(reason = 'reload') {
  const current = knowledgeRowsFetchController;
  knowledgeRowsFetchController = null;
  if (!current || current.signal.aborted) return;
  try { current.abort(reason); } catch { /* already aborted */ }
}

function currentKnowledgeRowsSignal() {
  if (!knowledgeRowsFetchController || knowledgeRowsFetchController.signal.aborted) {
    knowledgeRowsFetchController = new AbortController();
  }
  return knowledgeRowsFetchController.signal;
}

function rowsCancelError(reason) {
  const text = typeof reason === 'string' && reason.trim() ? reason.trim() : 'client-abort';
  const error = new Error(`ROWS_CANCELLED: ${text}`);
  error.name = 'AbortError';
  error.code = 'ROWS_CANCELLED';
  error.retryable = false;
  return error;
}

function isRowsFetchCancelled(error) {
  return error?.name === 'AbortError' || error?.code === 'ROWS_CANCELLED';
}

async function loadKnowledgeTables(options = {}) {
  const domains = [...new Set((options.domains || [])
    .map((domain) => String(domain || '').trim())
    .filter(Boolean))]
    .sort();
  const key = JSON.stringify(domains);
  const active = knowledgeTableLoads.get(key);
  if (active) return active;
  const load = loadKnowledgeTablesOnce({ ...options, domains });
  knowledgeTableLoads.set(key, load);
  try {
    return await load;
  } finally {
    if (knowledgeTableLoads.get(key) === load) knowledgeTableLoads.delete(key);
  }
}

async function loadKnowledgeTablesOnce({ retryEmpty = true, domains = [] } = {}) {
  const signal = restartKnowledgeRowFetches(domains);
  const collection = readableCollection('knowledge_tables');
  const first = await loadKnowledgeTableChunks(collection, domains);
  if (signal.aborted) return first;
  if (first.length || !collection?.find) {
    await prefetchCatalogRows(first, signal);
    return first;
  }
  if (!retryEmpty || !shouldRetryEmptyKnowledgeTables()) return first;
  for (const delay of KNOWLEDGE_TABLE_EMPTY_RETRY_DELAYS_MS) {
    if (signal.aborted) return first;
    await sleep(delay);
    const retry = await loadKnowledgeTableChunks(collection, domains);
    if (retry.length) {
      await prefetchCatalogRows(retry, signal);
      return retry;
    }
  }
  return first;
}

async function loadKnowledgeTableChunks(collection, domains = []) {
  if (!collection?.find) return [];
  const normalizedDomains = [...new Set((domains || [])
    .map((domain) => String(domain || '').trim())
    .filter(Boolean))];
  const selectors = normalizedDomains.length
    ? normalizedDomains.flatMap((domain) => ([
        { domain, chunk_index: 0 },
        { domain, projection_version: 2 },
        { domain, rows_source: 'rxdb.rows.fetch' },
      ]))
    : [
        { chunk_index: 0 },
        { projection_version: 2 },
        { rows_source: 'rxdb.rows.fetch' },
      ];
  const baseDocuments = [];
  for (const selector of selectors) {
    const docs = await findBySelector(collection, selector, 'knowledge_tables');
    baseDocuments.push(...docs);
  }
  const bases = [...new Map(baseDocuments.map((document) => [document.id, document])).values()]
    .filter(isKnowledgeTableBaseDocument);
  const chunkIds = knowledgeTableChunkDocumentIds(bases);
  const chunks = [];
  for (let offset = 0; offset < chunkIds.length; offset += 3) {
    const batch = chunkIds.slice(offset, offset + 3);
    const loaded = await Promise.all(batch.map((id) => findOneDocument(collection, id, 'knowledge_tables')));
    chunks.push(...loaded.filter(Boolean));
  }
  const documents = [...bases, ...chunks];
  markCollectionDiagnostic('knowledge_tables', 'read', 'ok', `${documents.length} chunk rows`);
  return documents;
}

function knowledgeTableChunkDocumentIds(baseDocuments = []) {
  return baseDocuments.flatMap((document) => {
    const source = document?.payload && typeof document.payload === 'object'
      ? document.payload
      : document;
    if (isKnowledgeCatalogDocument(document) || isKnowledgeCatalogDocument(source)) return [];
    const logicalId = String(
      source?.logical_table_id
      || document?.logical_table_id
      || source?.id
      || document?.id
      || '',
    ).trim();
    const chunkCount = Math.max(1, Number(source?.chunk_count ?? document?.chunk_count ?? 1) || 1);
    if (!logicalId || chunkCount <= 1) return [];
    return Array.from(
      { length: chunkCount - 1 },
      (_, index) => `${logicalId}:chunk:${String(index + 1).padStart(4, '0')}`,
    );
  });
}

function catalogDocumentsByLogicalId(documents = []) {
  const groups = new Map();
  for (const document of Array.isArray(documents) ? documents : []) {
    if (!isKnowledgeCatalogDocument(document)) continue;
    const logicalId = logicalKnowledgeTableId(document);
    if (!logicalId || groups.has(logicalId)) continue;
    groups.set(logicalId, document);
  }
  return [...groups.values()];
}

async function prefetchCatalogRows(documents, signal) {
  const tables = catalogDocumentsByLogicalId(documents);
  await mapPool(tables, KNOWLEDGE_CATALOG_FETCH_CONCURRENCY, async (document) => {
    if (signal?.aborted) return null;
    try {
      return await fetchCatalogRowsIntoCache(document, { signal });
    } catch (error) {
      if (isRowsFetchCancelled(error) || signal?.aborted) return null;
      throw error;
    }
  });
}

async function mapPool(items, limit, worker) {
  const results = new Array(items.length);
  let cursor = 0;
  const workers = Math.min(Math.max(1, limit), items.length);
  if (!workers) return results;
  async function run() {
    while (cursor < items.length) {
      const index = cursor;
      cursor += 1;
      results[index] = await worker(items[index], index);
    }
  }
  await Promise.all(Array.from({ length: workers }, () => run()));
  return results;
}

async function fetchCatalogRowsIntoCache(record, { signal, force = false } = {}) {
  const tableId = knowledgeRowsTableId(record);
  const logicalId = logicalKnowledgeTableId(record) || tableId;
  const contentHash = knowledgeContentHash(record);
  if (!tableId) return null;
  if (!force && contentHash) {
    const cached = readKnowledgeRowsCache(tableId, contentHash);
    if (cached) {
      noteKnowledgeTableRowState(record, logicalId, 'ready');
      return cached.rows;
    }
  }
  const loader = await knowledgeRowsLoaderFor(state.ctx);
  if (typeof loader?.fetchAllRows !== 'function') return null;
  noteKnowledgeTableRowState(record, logicalId, 'loading');
  try {
    if (signal?.aborted) throw rowsCancelError(signal.reason);
    const result = await loader.fetchAllRows(tableId, { signal });
    if (signal?.aborted) throw rowsCancelError(signal.reason);
    const rows = (Array.isArray(result?.rows) ? result.rows : [])
      .map((row) => (row && typeof row === 'object' ? row : { value: row }));
    const expected = catalogRowCount(record);
    const declared = Number(result?.rowCount);
    const declaredCount = Number.isFinite(declared) && declared >= 0 ? declared : null;
    if ((expected != null && rows.length !== expected) || (declaredCount != null && rows.length !== declaredCount)) {
      const error = new Error(`unvollständig: ${rows.length} von ${expected ?? declaredCount} Zeilen`);
      error.retryable = true;
      error.code = 'ROWS_SOURCE_ERROR';
      throw error;
    }
    const hash = contentHash || String(result?.contentHash || '');
    if (hash) {
      writeKnowledgeRowsCache(tableId, hash, {
        rows,
        rowCount: rows.length,
        contentHash: hash,
        schemaHash: result?.schemaHash ?? null,
      });
    }
    noteKnowledgeTableRowState(record, logicalId, 'ready');
    return rows;
  } catch (error) {
    if (isRowsFetchCancelled(error) || signal?.aborted) {
      if (state.knowledgeTableRowStates?.[logicalId]?.phase === 'loading') {
        delete state.knowledgeTableRowStates[logicalId];
      }
      throw isRowsFetchCancelled(error) ? error : rowsCancelError(signal?.reason);
    }
    noteKnowledgeTableRowState(record, logicalId, 'error', error);
    return null;
  }
}

function readKnowledgeRowsCache(tableId, contentHash) {
  if (!tableId || !contentHash) return null;
  return knowledgeRowsCache.get(`${tableId}\0${contentHash}`) || null;
}

function writeKnowledgeRowsCache(tableId, contentHash, value) {
  if (!tableId || !contentHash) return;
  const prefix = `${tableId}\0`;
  const key = `${prefix}${contentHash}`;
  for (const existing of [...knowledgeRowsCache.keys()]) {
    if (existing.startsWith(prefix) && existing !== key) knowledgeRowsCache.delete(existing);
  }
  knowledgeRowsCache.set(key, value);
}

function forgetKnowledgeRowsCache(tableId) {
  if (!tableId) return;
  const prefix = `${tableId}\0`;
  for (const existing of [...knowledgeRowsCache.keys()]) {
    if (existing.startsWith(prefix)) knowledgeRowsCache.delete(existing);
  }
}

function attachCachedCatalogRows(table) {
  const cached = readKnowledgeRowsCache(knowledgeRowsTableId(table), knowledgeContentHash(table));
  if (!cached) return false;
  attachCatalogRows(table, cached.rows, cached);
  return true;
}

function attachCatalogRows(table, rows, meta = {}) {
  if (!table || typeof table !== 'object') return;
  const list = Array.isArray(rows) ? [...rows] : [];
  table.rows = list;
  table.rows_origin = 'rxdb.rows.fetch';
  table.rows_content_hash = meta.contentHash || knowledgeContentHash(table);
  if (!table.payload || typeof table.payload !== 'object' || Array.isArray(table.payload)) {
    table.payload = {};
  }
  table.payload = {
    ...table.payload,
    rows: list,
    rows_origin: 'rxdb.rows.fetch',
  };
}

function stripAttachedCatalogRows(table) {
  if (!table || typeof table !== 'object') return;
  delete table.rows;
  delete table.records;
  delete table.data;
  delete table.rows_origin;
  delete table.rows_content_hash;
  if (table.payload && typeof table.payload === 'object' && !Array.isArray(table.payload)) {
    delete table.payload.rows;
    delete table.payload.records;
    delete table.payload.data;
    delete table.payload.rows_origin;
  }
}

function usesCatalogRowWindow(table) {
  return table?.rows_origin === 'rxdb.rows.fetch' || table?.payload?.rows_origin === 'rxdb.rows.fetch';
}

function noteKnowledgeTableRowState(record, logicalId, phase, error = null) {
  const id = String(logicalId || logicalKnowledgeTableId(record) || '').trim();
  if (!id) return;
  if (!state.knowledgeTableRowStates || typeof state.knowledgeTableRowStates !== 'object') {
    state.knowledgeTableRowStates = {};
  }
  if (phase === 'ready') {
    delete state.knowledgeTableRowStates[id];
    return;
  }
  const source = record?.payload && typeof record.payload === 'object' && !Array.isArray(record.payload)
    ? record.payload
    : record;
  state.knowledgeTableRowStates[id] = {
    tableId: id,
    tableKey: String(source?.table_key || record?.table_key || ''),
    title: String(source?.title || record?.title || source?.table_key || record?.table_key || id),
    phase,
    message: phase === 'error'
      ? String(error?.message || state.t('rowsLoadFailed', 'Zeilen konnten nicht geladen werden'))
      : '',
    retryable: phase === 'error' && error?.retryable === true,
  };
}

async function knowledgeRowsLoaderFor(ctx) {
  try {
    if (typeof ctx?.sync?.startCollection !== 'function') return null;
    const bridge = await ctx.sync.startCollection('knowledge_tables');
    return bridge?.state?.knowledgeRowsLoader || null;
  } catch {
    return null;
  }
}

function knowledgeRowsTableId(table) {
  const explicit = table?.table_id || table?.payload?.table_id;
  if (explicit) return stripKnowledgeTablePrefix(explicit);
  return stripKnowledgeTablePrefix(table?.logical_table_id || table?.id || '');
}

function stripKnowledgeTablePrefix(value) {
  const text = String(value || '').trim();
  return text.startsWith('table:') ? text.slice('table:'.length) : text;
}

function findKnowledgeTable(tableId) {
  const wanted = String(tableId || '').trim();
  if (!wanted) return null;
  const bare = stripKnowledgeTablePrefix(wanted);
  return state.knowledgeBases
    .flatMap((base) => base.tables || [])
    .find((entry) => entry?.id === wanted
      || entry?.logical_table_id === wanted
      || entry?.table_id === wanted
      || stripKnowledgeTablePrefix(entry?.id) === bare
      || stripKnowledgeTablePrefix(entry?.table_id) === bare
      || stripKnowledgeTablePrefix(entry?.logical_table_id) === bare) || null;
}

async function retryKnowledgeTableRows(tableId) {
  const table = findKnowledgeTable(tableId);
  if (!table) return;
  const logicalId = logicalKnowledgeTableId(table) || table.id || tableId;
  forgetKnowledgeRowsCache(knowledgeRowsTableId(table));
  stripAttachedCatalogRows(table);
  delete state.knowledgeTableRowStates?.[logicalId];
  const signal = currentKnowledgeRowsSignal();
  try {
    const rows = await fetchCatalogRowsIntoCache(table, { signal, force: true });
    const cached = readKnowledgeRowsCache(knowledgeRowsTableId(table), knowledgeContentHash(table));
    if (cached) attachCatalogRows(table, cached.rows, cached);
    else if (rows) attachCatalogRows(table, rows, { contentHash: knowledgeContentHash(table) });
  } catch (error) {
    if (!isRowsFetchCancelled(error)) noteKnowledgeTableRowState(table, logicalId, 'error', error);
  }
  await loadDashboardData();
  if (state.ctx?.host) render();
}

function resetKnowledgeRowsForTest() {
  abortKnowledgeRowFetches('test-reset');
  knowledgeRowsCache.clear();
  knowledgeRowsFetchDomainKey = '';
  state.knowledgeTableRowStates = {};
  state.rowLimitWarnings = [];
  state.chunkDiagnostics = [];
}

async function findBySelector(collection, selector, collectionName = '') {
  try {
    const docs = await withTimeout(
      collection.find({ selector }).exec(),
      COLLECTION_READ_TIMEOUT_MS,
      'collection read timed out',
    );
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

async function findOneDocument(collection, id, collectionName = '') {
  try {
    const doc = await withTimeout(
      collection.findOne(id).exec(),
      COLLECTION_READ_TIMEOUT_MS,
      'collection read timed out',
    );
    return doc ? toJson(doc) : null;
  } catch (error) {
    if (collectionName) {
      markCollectionDiagnostic(
        collectionName,
        'read',
        isBusinessOsPermissionDenied(error) ? 'denied' : 'failed',
        isBusinessOsPermissionDenied(error) ? state.t('collectionLocked', 'Keine Datenfreigabe') : errorMessage(error),
      );
    }
    return null;
  }
}

function shouldRetryEmptyKnowledgeTables(readiness = collectionReadiness('knowledge_tables')) {
  // Canonical readiness replaces the private sync-diagnostics probe: retry an
  // empty knowledge_tables read while initial replication may still deliver
  // rows (catching-up) or when the channel is live (demand-loaded chunks can
  // land later). offline-pending / never-synced channels make retries
  // pointless. Without a readiness API keep the optimistic legacy default.
  if (!readiness) return true;
  return readiness.ready === true || readiness.state === 'catching-up';
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
  if (keys.has('source_candidates')) score += 2;
  if (keys.has('source_catalog')) score += 6;
  if (keys.has('curated_sources') || keys.has('load_data_library')) score += 4;
  if (keys.has('measured_load_points') || keys.has('measurements')) score += 3;
  if (/research|bearing|load|competitive/i.test(base.domain)) score += 2;
  return score;
}

async function loadDashboardData() {
  const task = selectedTask();
  state.candidateRows = [];
  state.candidateModels = [];
  state.sourceRows = [];
  state.curatedRows = [];
  state.claimRows = [];
  state.evidenceRows = [];
  state.measurementRows = [];
  state.derivedMeasurementRows = [];
  state.graphNodeRows = [];
  state.graphEdgeRows = [];
  state.sourceModels = [];
  state.graphProjection = null;
  state.graphContractStatus = '';
  state.graphContractErrors = [];
  state.rowLimitWarnings = [];
  state.chunkDiagnostics = [];
  if (!task) return;
  const base = knowledgeBaseForTask(task);
  const candidateTable = tableForKey(base, task.candidate_catalog_key || 'source_candidates');
  const sourceTable = tableForKey(
    base,
    task.source_catalog_key || tableKey(base, ['source_catalog', 'sources', 'curated_sources']),
  );
  const curatedTable = tableForKey(base, task.curated_table_key) || firstTableMatching(base, /library|curated/i);
  const measurementTable = tableForKey(base, task.measurements_table_key) || firstTableMatching(base, /measure|load|point/i);
  // Abgeleitete Kraefte/Momente: die Tabelle derived_propeller_load_points
  // traegt Schub, Drehmoment und Leistung aus CT/CP (T = CT*rho*n^2*D^4,
  // P = CP*rho*n^3*D^5, Q = P/(2*pi*n)). derived_bearing_loads ist seit der
  // Quarantaene der Legacy-Ableitung (26.07.2026) leer; die Sicht zeigte
  // deshalb 0 Zeilen, obwohl 3.925 verifizierte Ableitungen vorlagen
  // (skf.ctox.dev, 02.09.2026).
  const derivedMeasurementTable = tableForKey(base, 'derived_propeller_load_points')
    || tableForKey(base, 'derived_bearing_loads');
  const graphNodeTable = tableForKey(base, task.payload?.graph_contract?.nodes_table_key || 'semantic_graph_nodes') || firstTableMatching(base, /semantic.*graph.*node|concept.*node/i);
  const graphEdgeTable = tableForKey(base, task.payload?.graph_contract?.edges_table_key || 'semantic_graph_edges') || firstTableMatching(base, /semantic.*graph.*edge|concept.*edge/i);
  // Consolidated engineering claims (one row per statement, several sources per claim). Optional: bases
  // without a claims table fall back to the claim_support evidence rows below.
  const claimTable = tableForKey(base, task.claims_table_key || 'claims') || firstTableMatching(base, /^claims$/i);
  const evidenceTable = tableForKey(base, task.evidence_table_key || 'evidence_points') || firstTableMatching(base, /evidence.*point/i);
  const [candidateRows, sourceRows, curatedRows, measurementRows, derivedMeasurementRows, graphNodeRows, graphEdgeRows, claimRows, evidenceRows] = await Promise.all([
    fetchDashboardRows(candidateTable),
    fetchDashboardRows(sourceTable),
    curatedTable && curatedTable.id !== sourceTable?.id ? fetchDashboardRows(curatedTable) : Promise.resolve([]),
    measurementTable && measurementTable.id !== sourceTable?.id && measurementTable.id !== curatedTable?.id ? fetchDashboardRows(measurementTable) : Promise.resolve([]),
    fetchDashboardRows(derivedMeasurementTable),
    fetchDashboardRows(graphNodeTable),
    fetchDashboardRows(graphEdgeTable),
    fetchDashboardRows(claimTable),
    fetchDashboardRows(evidenceTable),
  ]);
  state.candidateRows = candidateRows;
  state.sourceRows = sourceRows;
  state.curatedRows = curatedRows;
  state.measurementRows = measurementRows;
  state.derivedMeasurementRows = derivedMeasurementRows;
  state.graphNodeRows = graphNodeRows;
  state.graphEdgeRows = graphEdgeRows;
  state.claimRows = claimRows;
  state.evidenceRows = evidenceRows;
  state.candidateModels = buildSourceModels(task, candidateRows, [], []);
  state.sourceModels = buildSourceModels(task, sourceRows, curatedRows, measurementRows);
  const evidenceMeasurementRows = filterMeasurementRowsForEvidence(measurementRows, state.sourceModels);
  const evidenceGraphRows = filterGraphRowsForEvidence(graphNodeRows, graphEdgeRows, evidenceSourceIds(state.sourceModels));
  state.graphContractStatus = evidenceGraphRows.status || '';
  state.graphContractErrors = evidenceGraphRows.errors || [];
  state.graphProjection = buildResearchGraphProjection({
    task,
    sourceModels: evidenceSourceModels(state.sourceModels),
    measurementRows: evidenceMeasurementRows,
    graphNodeRows: evidenceGraphRows.nodes,
    graphEdgeRows: evidenceGraphRows.edges,
    graphLayer: state.graph.layer,
    detailLevel: state.graph.detailLevel,
    visibleLimit: state.graph.visibleLimit,
    verifiedSourceIds: evidenceSourceIds(state.sourceModels),
    graphContractStatus: evidenceGraphRows.status,
    graphContractErrors: evidenceGraphRows.errors,
  });
  state.graphProjection = enrichGraphSemanticMetadata(state.graphProjection, graphNodeRows, graphEdgeRows, task);
  if (!state.selectedSourceId || !state.sourceModels.some((item) => item.id === state.selectedSourceId)) {
    state.selectedSourceId = state.sourceModels[0]?.id || '';
  }
}

async function fetchDashboardRows(table) {
  if (!table?.id) return [];
  try {
    return await fetchTableRows(table.id);
  } catch (error) {
    if (!isRowsFetchCancelled(error)) {
      noteKnowledgeTableRowState(table, logicalKnowledgeTableId(table) || table.id, 'error', error);
    }
    return [];
  }
}

async function fetchTableRows(tableId) {
  if (!tableId) return [];
  const table = findKnowledgeTable(tableId);
  try {
    if (table && isKnowledgeCatalogDocument(table) && !state.knowledgeTableRowStates?.[logicalKnowledgeTableId(table) || table.id]) {
      const signal = currentKnowledgeRowsSignal();
      const rows = await fetchCatalogRowsIntoCache(table, { signal });
      const cached = readKnowledgeRowsCache(knowledgeRowsTableId(table), knowledgeContentHash(table));
      if (cached) attachCatalogRows(table, cached.rows, cached);
      else if (rows) attachCatalogRows(table, rows, { contentHash: knowledgeContentHash(table) });
    }
  } catch (error) {
    if (isRowsFetchCancelled(error)) return [];
    noteKnowledgeTableRowState(table, logicalKnowledgeTableId(table) || table?.id || tableId, 'error', error);
    return [];
  }
  const normalized = normalizeKnowledgeTableRows(table, tableId);
  if (!normalized.valid) return [];
  if (usesCatalogRowWindow(table)) {
    const expected = catalogRowCount(table);
    if (expected != null && normalized.rows.length !== expected) {
      const error = new Error(`unvollständig: ${normalized.rows.length} von ${expected} Zeilen`);
      error.retryable = true;
      error.code = 'ROWS_SOURCE_ERROR';
      noteKnowledgeTableRowState(table, logicalKnowledgeTableId(table) || tableId, 'error', error);
      return [];
    }
    noteKnowledgeTableRowState(table, logicalKnowledgeTableId(table) || tableId, 'ready');
    return normalized.rows;
  }
  if (normalized.rows.length) return applyRowLimit(normalized.rows, table, tableId, normalized.rowCount);
  // Chunk fallback only: catalog tables never land in a collection. Embedded
  // rows stay the pre-S8 path, and a null rows loader leaves those rows in
  // place. There is no HTTP fallback.
  markCollectionDiagnostic('knowledge_tables', 'read', 'ok', `0 synced rows (${String(tableId || '')})`);
  return [];
}

function normalizeKnowledgeTableRows(table, tableId = '') {
  const source = table?.payload && typeof table.payload === 'object' ? table.payload : table;
  const chunks = usesCatalogRowWindow(table) ? [] : firstArray(
    table?.chunks,
    table?.row_chunks,
    table?.rows_chunks,
    source?.chunks,
    source?.row_chunks,
    source?.rows_chunks,
    source?.dataframe?.chunks,
  );
  if (chunks.length) {
    const result = validateChunkSequence(chunks, {
      expectedChunkCount: firstPositiveNumber(table, ['chunk_count', 'chunkCount', 'total_chunks', 'totalChunks'])
        || firstPositiveNumber(source, ['chunk_count', 'chunkCount', 'total_chunks', 'totalChunks']),
      expectedItemCount: firstPositiveNumber(table, ['row_count', 'rowCount', 'total_row_count', 'totalRows'])
        || firstPositiveNumber(source, ['row_count', 'rowCount', 'total_row_count', 'totalRows']),
      indexFields: ['index', 'idx', 'chunk_index', 'chunkIndex'],
      countFields: ['chunk_count', 'chunkCount', 'total_chunks', 'totalChunks'],
      offsetFields: ['offset', 'row_offset', 'rowOffset', 'start'],
      itemCountFields: ['row_count', 'rowCount', 'rows_count', 'rowsCount', 'count'],
      itemArrayFields: ['rows', 'records', 'dataframe.rows', 'payload.rows', 'payload.records'],
      itemLabel: 'rows',
    });
    if (!result.valid) {
      recordChunkDiagnostic(tableId, result.reason);
      return result;
    }
    return result;
  }
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
  return {
    valid: true,
    rows: rows.map((row) => row && typeof row === 'object' ? row : { value: row }),
    rowCount: rows.length,
  };
}

function applyRowLimit(rows, table, tableId, declaredRowCount = rows.length) {
  if (usesCatalogRowWindow(table)) return rows;
  const sourceRowCount = Math.max(rows.length, Number(declaredRowCount) || 0, Number(table?.row_count) || 0, Number(table?.payload?.row_count) || 0);
  if (sourceRowCount > ROW_LIMIT || rows.length > ROW_LIMIT) {
    state.rowLimitWarnings.push({
      tableId,
      cap: ROW_LIMIT,
      sourceRowCount,
      returnedRowCount: Math.min(rows.length, ROW_LIMIT),
    });
  }
  return rows.slice(0, ROW_LIMIT);
}

function recordChunkDiagnostic(tableId, reason) {
  const message = `knowledge_tables ${tableId || 'unknown'}: ${reason}`;
  state.chunkDiagnostics.push({ tableId, reason, message });
  markCollectionDiagnostic('knowledge_tables', 'read', 'failed', message);
}

function validateChunkSequence(chunks, options = {}) {
  if (!Array.isArray(chunks) || !chunks.length) return { valid: false, reason: 'Keine Chunks vorhanden.', rows: [], rowCount: 0, chunkCount: 0 };
  const indexFields = options.indexFields || ['idx', 'index', 'chunk_index', 'chunkIndex'];
  const countFields = options.countFields || ['total', 'chunk_count', 'chunkCount'];
  const offsetFields = options.offsetFields || ['offset', 'row_offset', 'rowOffset', 'start'];
  const itemCountFields = options.itemCountFields || ['row_count', 'rowCount', 'count'];
  const itemArrayFields = options.itemArrayFields || ['rows', 'records', 'data'];
  const itemValueFields = options.itemValueFields || [];
  const normalized = [];
  const indices = new Set();
  const declaredCounts = new Set();
  for (const [position, chunk] of chunks.entries()) {
    if (!chunk || typeof chunk !== 'object') return { valid: false, reason: `Chunk ${position} ist kein Objekt.`, rows: [], rowCount: 0, chunkCount: 0 };
    const index = finiteNonNegative(firstNumber(chunk, indexFields));
    if (index === null) return { valid: false, reason: `Chunk ${position} hat keinen gültigen Index.`, rows: [], rowCount: 0, chunkCount: 0 };
    if (indices.has(index)) return { valid: false, reason: `Chunk-Index ${index} ist doppelt vorhanden.`, rows: [], rowCount: 0, chunkCount: 0 };
    indices.add(index);
    const declaredCount = finitePositive(firstNumber(chunk, countFields));
    if (declaredCount !== null) declaredCounts.add(declaredCount);
    const rows = extractChunkItems(chunk, itemArrayFields, itemValueFields);
    if (options.requireItems && !rows.length) return { valid: false, reason: `Chunk ${index} enthält keine Daten.`, rows: [], rowCount: 0, chunkCount: 0 };
    const itemCount = finiteNonNegative(firstNumber(chunk, itemCountFields));
    if (itemCount !== null && itemCount !== rows.length) {
      return { valid: false, reason: `Chunk ${index} meldet ${itemCount} ${options.itemLabel || 'Elemente'}, enthält aber ${rows.length}.`, rows: [], rowCount: 0, chunkCount: 0 };
    }
    normalized.push({ index, rows, offset: finiteNonNegative(firstNumber(chunk, offsetFields)) });
  }
  const ordered = normalized.sort((left, right) => left.index - right.index);
  if (ordered.some((chunk, position) => chunk.index !== position)) {
    return { valid: false, reason: 'Chunk-Indizes sind nicht lückenlos von 0 bis N-1.', rows: [], rowCount: 0, chunkCount: 0 };
  }
  const expectedChunkCount = Number(options.expectedChunkCount) || null;
  const declaredChunkCount = declaredCounts.size ? [...declaredCounts] : [];
  if (declaredChunkCount.length > 1 || (declaredChunkCount.length && declaredChunkCount[0] !== ordered.length)) {
    return { valid: false, reason: 'chunk_count ist zwischen den Chunks nicht konsistent.', rows: [], rowCount: 0, chunkCount: 0 };
  }
  if (expectedChunkCount !== null && expectedChunkCount !== ordered.length) {
    return { valid: false, reason: `chunk_count ${expectedChunkCount} stimmt nicht mit ${ordered.length} Chunks überein.`, rows: [], rowCount: 0, chunkCount: 0 };
  }
  const rows = ordered.flatMap((chunk) => chunk.rows);
  let runningOffset = 0;
  for (const chunk of ordered) {
    if (chunk.offset !== null && chunk.offset !== runningOffset) {
      return { valid: false, reason: `Chunk ${chunk.index} beginnt bei Offset ${chunk.offset}, erwartet wurde ${runningOffset}.`, rows: [], rowCount: 0, chunkCount: 0 };
    }
    runningOffset += chunkAdvance(chunk.rows, options.offsetUnit);
  }
  const expectedItemCount = Number(options.expectedItemCount) || null;
  if (expectedItemCount !== null && expectedItemCount !== rows.length) {
    return { valid: false, reason: `Die Row-Summe ${rows.length} stimmt nicht mit ${expectedItemCount} überein.`, rows: [], rowCount: 0, chunkCount: 0 };
  }
  return { valid: true, rows, rowCount: rows.length, chunkCount: ordered.length };
}

function extractChunkItems(chunk, itemArrayFields, itemValueFields = []) {
  for (const field of itemArrayFields) {
    const value = field.split('.').reduce((current, key) => current?.[key], chunk);
    if (Array.isArray(value)) return value;
  }
  for (const field of itemValueFields) {
    const value = field.split('.').reduce((current, key) => current?.[key], chunk);
    if (typeof value === 'string' && value.length) return [value];
  }
  return [];
}

function chunkAdvance(items, offsetUnit) {
  if (offsetUnit === 'payload') return items.reduce((sum, item) => sum + String(item || '').length, 0);
  return items.length;
}

function firstNumber(value, keys) {
  for (const key of keys) {
    const candidate = value?.[key];
    if (candidate !== null && candidate !== undefined && String(candidate).trim() !== '') return Number(candidate);
  }
  return null;
}

function firstPositiveNumber(value, keys) {
  const number = firstNumber(value, keys);
  return finitePositive(number) || null;
}

function finiteNonNegative(value) {
  return Number.isInteger(value) && value >= 0 ? value : null;
}

function finitePositive(value) {
  return Number.isInteger(value) && value > 0 ? value : null;
}

function buildSourceModels(task, sourceRows, curatedRows, measurementRows) {
  const curatedBySource = new Map();
  for (const row of curatedRows) {
    const id = sourceId(row);
    if (id) curatedBySource.set(id, row);
  }
  const raw = (sourceRows.length ? sourceRows : curatedRows).filter((row) => sourceModelId(row));
  const initialModels = raw.map((row) => {
    const id = sourceModelId(row);
    const gate = evidenceGate(row);
    return { id, row, evidenceEligible: gate.eligible };
  });
  const measurementAgg = aggregateMeasurements(measurementRows || [], initialModels);
  return raw.map((row, index) => {
    const id = sourceModelId(row);
    const title = firstString(row, ['title', 'source_title', 'name']) || `Source ${index + 1}`;
    const sourceClass = firstString(row, ['source_class', 'source_type', 'type', 'bucket', 'record_type']) || 'source';
    const note = firstString(row, ['contribution_note', 'contribution', 'summary', 'relevance_to_bearing_design', 'use']) || '';
    const curated = curatedBySource.get(id);
    const agg = measurementAgg.get(id) || null;
    const axisDefs = scoringDimensionsForTask(task);
    const gate = evidenceGate(row);
    const dimensions = gate.eligible
      ? scoreDimensions(row, curated, agg, task, axisDefs)
      : emptyScoreDimensions(axisDefs);
    const auditedGrade = sourceTierGrade(row);
    return {
      id,
      rank: index + 1,
      title,
      subtitle: sourceClass,
      url: firstString(row, ['source_url', 'url', 'requested_url', 'pdf_url', 'direct_url', 'doi']) || '',
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
      grade: gate.eligible ? (auditedGrade || gradeForScore(dimensions.portfolio_priority)) : '—',
    };
  }).sort((a, b) => {
    if (a.evidenceEligible !== b.evidenceEligible) return a.evidenceEligible ? -1 : 1;
    if (a.evidenceEligible) {
      const gradeOrder = { A: 4, B: 3, C: 2, D: 1 };
      return (gradeOrder[b.grade] || 0) - (gradeOrder[a.grade] || 0) || b.score - a.score;
    }
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
  const modelsById = new Map((sourceModels || []).map((model) => [String(model.id), model]));
  return (rows || []).filter((row) => {
    const model = modelsById.get(firstString(row, ['source_id']));
    if (sourceModels?.length && (!model || !model.evidenceEligible)) return false;
    return measurementEvidenceBinding(row, model).eligible;
  });
}

function filterGraphRowsForEvidence(nodeRows, edgeRows, eligibleIds) {
  if (!(nodeRows || []).length && !(edgeRows || []).length) return { nodes: [], edges: [], status: '', errors: [] };
  const errors = [];
  const nodes = (nodeRows || []).map((row) => {
    const nodeId = firstString(row, ['node_id', 'id', 'concept_id', 'key']);
    const explicitSourceId = nodeId.startsWith('source:') ? nodeId.slice('source:'.length) : '';
    const sourceIds = graphSourceIds(row);
    const filteredSourceIds = sourceIds.filter((id) => eligibleIds.has(id));
    if (explicitSourceId && !eligibleIds.has(explicitSourceId)) errors.push(`${nodeId || 'node'}.source_id`);
    if (!sourceIds.length) errors.push(`${nodeId || 'node'}.source_ids`);
    if (filteredSourceIds.length !== sourceIds.length) errors.push(`${nodeId || 'node'}.unverified_source`);
    return row;
  });
  const edges = (edgeRows || []).map((row) => {
    const sourceIds = graphSourceIds(row);
    const edgeId = firstString(row, ['edge_id', 'id']) || 'edge';
    if (!sourceIds.length) errors.push(`${edgeId}.source_ids`);
    if (sourceIds.some((id) => !eligibleIds.has(id))) errors.push(`${edgeId}.unverified_source`);
    return row;
  });
  return errors.length
    ? { nodes: [], edges: [], status: 'invalid_graph_contract', errors }
    : { nodes, edges, status: '', errors: [] };
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

function measurementEvidenceBinding(row, sourceModel = null) {
  const sourceIdValue = firstString(row, ['source_id']);
  const snapshotId = firstString(row, ['snapshot_id', 'source_snapshot_id']);
  const snapshotHash = firstString(row, ['snapshot_hash', 'snapshot_sha256']);
  const canonicalUrl = firstString(row, ['canonical_url']);
  const evidenceId = firstString(row, ['evidence_id', 'claim_id']);
  const snapshotPath = firstString(row, ['snapshot_path', 'archive_path', 'local_snapshot_path']);
  const retrievedAt = firstString(row, ['retrieved_at', 'extracted_at']);
  const urlRole = firstString(row, ['url_role']).toLowerCase();
  const contentScope = firstString(row, ['content_scope']).toLowerCase();
  const validHash = /^sha256:[0-9a-f]{64}$/i.test(snapshotHash);
  const expectedRow = sourceModel?.row || {};
  const expectedSnapshotId = firstString(expectedRow, ['snapshot_id', 'source_snapshot_id']);
  const expectedHash = firstString(expectedRow, ['snapshot_hash', 'snapshot_sha256']);
  const expectedCanonicalUrl = firstString(expectedRow, ['canonical_url']);
  const eligible = Boolean(sourceIdValue)
    && Boolean(snapshotId)
    && validHash
    && Boolean(canonicalUrl)
    && Boolean(evidenceId)
    && Boolean(snapshotPath)
    && Boolean(retrievedAt)
    && RECEIPT_URL_ROLES.has(urlRole)
    && RECEIPT_CONTENT_SCOPES.has(contentScope)
    && (!sourceModel || (
      sourceModel.evidenceEligible === true
      && snapshotId === expectedSnapshotId
      && snapshotHash === expectedHash
      && canonicalUrl === expectedCanonicalUrl
    ));
  if (eligible) return { eligible: true, sourceId: sourceIdValue, snapshotId, snapshotHash, canonicalUrl };
  return {
    eligible: false,
    sourceId: sourceIdValue,
    reason: !sourceIdValue
      ? 'missing_source_id'
      : !snapshotId
        ? 'missing_snapshot_id'
        : !validHash
          ? 'invalid_snapshot_hash'
      : !canonicalUrl
        ? 'missing_canonical_url'
        : !evidenceId
          ? 'missing_evidence_id'
          : !snapshotPath
            ? 'missing_snapshot_path'
            : !retrievedAt
              ? 'missing_retrieved_at'
              : !RECEIPT_URL_ROLES.has(urlRole)
                ? 'invalid_url_role'
                : !RECEIPT_CONTENT_SCOPES.has(contentScope)
                  ? 'invalid_content_scope'
            : 'source_snapshot_lineage_mismatch',
  };
}

function sourceReceiptLineage(source) {
  const row = source?.row || {};
  const sourceIdValue = firstString(row, ['source_id']) || String(source?.id || '').trim();
  const sourceUrl = firstString(row, ['source_url', 'url', 'direct_url', 'doi']);
  const canonicalUrl = firstString(row, ['canonical_url']) || String(source?.canonicalUrl || '').trim();
  const snapshotId = firstString(row, ['snapshot_id', 'source_snapshot_id']);
  const snapshotHash = firstString(row, ['snapshot_hash', 'snapshot_sha256']);
  const receiptUrl = firstString(row, [
    'source_receipt_url',
    'source_receipt_link',
    'evidence_receipt_url',
    'receipt_url',
    'receipt_link',
    'snapshot_url',
  ]);
  const receiptId = firstString(row, ['source_receipt_id', 'evidence_receipt_id', 'receipt_id']);
  const evidenceId = firstString(row, ['evidence_id']);
  const claimId = firstString(row, ['claim_id']);
  const snapshotPath = firstString(row, ['snapshot_path', 'archive_path', 'local_snapshot_path']);
  const retrievedAt = firstString(row, ['retrieved_at']);
  const urlRole = firstString(row, ['url_role']).toLowerCase();
  const contentScope = firstString(row, ['content_scope']).toLowerCase();
  const valid = Boolean(sourceIdValue)
    && Boolean(canonicalUrl)
    && Boolean(receiptUrl || receiptId)
    && Boolean(snapshotId)
    && Boolean(snapshotPath)
    && Boolean(evidenceId || claimId)
    && Boolean(retrievedAt)
    && RECEIPT_URL_ROLES.has(urlRole)
    && RECEIPT_CONTENT_SCOPES.has(contentScope)
    && /^sha256:[0-9a-f]{64}$/i.test(snapshotHash);
  return {
    valid,
    source_id: sourceIdValue,
    source_url: sourceUrl,
    url: receiptUrl,
    receipt_url: receiptUrl,
    source_receipt_url: receiptUrl,
    receipt_id: receiptId,
    evidence_id: evidenceId,
    claim_id: claimId,
    snapshot_path: snapshotPath,
    retrieved_at: retrievedAt,
    url_role: urlRole,
    content_scope: contentScope,
    snapshot_id: snapshotId,
    snapshot_hash: snapshotHash,
    snapshot_sha256: snapshotHash,
    canonical_url: canonicalUrl,
  };
}

function evidenceGate(row) {
  const sourceIdValue = firstString(row, ['source_id']);
  const verificationStatus = firstString(row, ['verification_status']).toLowerCase();
  const httpStatus = Number(row?.http_status);
  const snapshotHash = firstString(row, ['snapshot_hash']);
  const canonicalUrl = firstString(row, ['canonical_url']);
  const snapshotId = firstString(row, ['snapshot_id', 'source_snapshot_id']);
  const snapshotPath = firstString(row, ['snapshot_path', 'archive_path', 'local_snapshot_path']);
  const evidenceId = firstString(row, ['evidence_id']);
  const claimId = firstString(row, ['claim_id']);
  const retrievedAt = firstString(row, ['retrieved_at']);
  const urlRole = firstString(row, ['url_role']).toLowerCase();
  const contentScope = firstString(row, ['content_scope']).toLowerCase();
  const sourceTier = firstString(row, ['source_tier']).toLowerCase();
  const sourceType = firstString(row, ['source_type', 'type']).toLowerCase();
  const rejectionReason = firstString(row, ['evidence_rejection_reason']);
  const relevanceScore = Number(row?.evidence_relevance_score);
  const validSnapshotHash = /^sha256:[0-9a-f]{64}$/i.test(snapshotHash);
  const actualSourceContent = booleanField(row, 'actual_full_text_or_data');
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
    && Boolean(sourceIdValue)
    && booleanField(row, 'transport_verified')
    && booleanField(row, 'content_extracted')
    && Number.isInteger(httpStatus)
    && httpStatus >= 200
    && httpStatus < 300
    && httpStatus !== 204
    && validSnapshotHash
    && Boolean(snapshotId)
    && Boolean(snapshotPath)
    && Boolean(evidenceId || claimId)
    && Boolean(retrievedAt)
    && RECEIPT_URL_ROLES.has(urlRole)
    && RECEIPT_CONTENT_SCOPES.has(contentScope)
    && Boolean(canonicalUrl)
    && !canonicalIsMetadata
    && booleanField(row, 'evidence_eligible')
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
  if (!sourceIdValue) return { eligible: false, status: 'missing_source_id', label: 'Source ID missing' };
  if (!booleanField(row, 'transport_verified')) return { eligible: false, status: 'transport_unverified', label: 'Transport not verified' };
  if (!booleanField(row, 'content_extracted')) return { eligible: false, status: 'empty_content', label: 'No source content extracted' };
  if (!validSnapshotHash) return { eligible: false, status: 'missing_snapshot', label: 'Valid snapshot missing' };
  if (!snapshotId) return { eligible: false, status: 'missing_snapshot_id', label: 'Snapshot ID missing' };
  if (!snapshotPath) return { eligible: false, status: 'missing_snapshot_path', label: 'Snapshot path missing' };
  if (!evidenceId && !claimId) return { eligible: false, status: 'missing_evidence_id', label: 'Evidence or claim ID missing' };
  if (!retrievedAt) return { eligible: false, status: 'missing_retrieved_at', label: 'Retrieval time missing' };
  if (!RECEIPT_URL_ROLES.has(urlRole)) return { eligible: false, status: 'invalid_url_role', label: 'URL role not receipt-bound' };
  if (!RECEIPT_CONTENT_SCOPES.has(contentScope)) return { eligible: false, status: 'invalid_content_scope', label: 'Content scope not receipt-bound' };
  if (!canonicalUrl) return { eligible: false, status: 'missing_canonical_url', label: 'Canonical source missing' };
  if (!actualSourceContent) return { eligible: false, status: 'no_primary_content', label: 'No full text or original data' };
  if (!relevant) return { eligible: false, status: 'insufficient_relevance', label: 'Relevance not verified' };
  if (rejectionReason) return { eligible: false, status: 'rejected', label: 'Evidence rejected' };
  if (!booleanField(row, 'evidence_eligible')) return { eligible: false, status: 'not_eligible', label: 'Evidence not eligible' };
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
  return pickAxisScores({}, axisDefs);
}

function aggregateMeasurements(rows, sourceModels = null) {
  const bySource = new Map();
  const eligibleRows = sourceModels
    ? filterMeasurementRowsForEvidence(rows, sourceModels)
    : (rows || []).filter((row) => measurementEvidenceBinding(row).eligible);
  for (const row of eligibleRows) {
    const id = firstString(row, ['source_id']);
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
  const sourceClass = String(row.source_type || row.type || row.source_class || row.bucket || curated?.record_type || '').toLowerCase();
  const auditedGrade = sourceTierGrade(row);
  let evidence = ({ A: 94, B: 82, C: 64, D: 42 })[auditedGrade] || 38;
  if (!auditedGrade) {
    if (/dataset|repository|zenodo|figshare|dataverse|csv|xlsx|parquet/.test(text)) evidence = 88;
    else if (/agency|standard|regulatory|nasa|faa|easa|dod|osti|dtic/.test(text)) evidence = 78;
    else if (/scholarly|article|journal|conference|proceedings|dissertation|thesis|preprint|paper|doi|springer|ieee|aiaa|semantic/.test(sourceClass + text)) evidence = 78;
    else if (/web|manufacturer|vendor|datasheet/.test(sourceClass + text)) evidence = 52;
  }
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
  const auditedRelevance = Number(row?.evidence_relevance_score);
  const topicFit = Number.isFinite(auditedRelevance)
    ? normalizeScoreScale(auditedRelevance)
    : topicFitScore(task, text, row);
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
  
  // Bearing relevance is a domain rule, not a global filter. Other research
  // domains use their own inferred terms and are never penalized for lacking
  // rotor vocabulary.
  const titleText = String(row.title || row.name || '').toLowerCase();
  const hasBearingTopic = /propeller|rotor|uav|drone|bearing|load|force|moment|thrust|torque|rpm|vibration|spindel|motor|flight|telemetry|aerodynamic|blade|windtunnel|w\u00e4lzlager|lager|schub|drehmoment|last|messung|pr\u00fcfstand|spindle|vibrat|flight|telemetr|testing|bench|load cell|stanag|mil-std/i.test(titleText);
  if (inferResearchKind(task) === 'bearing' && !hasBearingTopic) {
    scores.topic_fit = 10;
  }

  const weightedCriteria = axisDefs
    .filter((axis) => axis.id !== 'portfolio_priority')
    .map((axis) => [scores[axis.id] ?? topicFitScore(task, text, row), Number(axis.weight || 1)]);
  if (weightedCriteria.length) scores.portfolio_priority = clampScore(weightedAverage(weightedCriteria));
  // Only the criteria this task is scored on are part of the source model. The other entries of the internal
  // catalogue (buyer clarity, pricing clarity, … for competitive research) are meaningless for an engineering
  // base and must never reach the UI, the export or the evaluation drawer.
  return pickAxisScores(scores, axisDefs);
}

function pickAxisScores(scores, axisDefs = BASE_AXES) {
  const ids = new Set([...(axisDefs || []).map((axis) => axis.id), 'portfolio_priority']);
  return Object.fromEntries([...ids].map((id) => [id, scores[id] ?? null]));
}

function render() {
  renderLeft();
  renderCenter();
  renderRight();
}

function renderLeft() {
  const root = pane('left');
  if (!root) return;
  const scrollState = capturePaneScroll(root);
  const task = selectedTask();
  const rankedSources = evidenceRankedSources();
  root.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('webResearch', 'Web Research'))}</span>
          <h2 class="ctox-pane-title">${escapeHtml(state.t('knowledgeDashboards', 'Dashboards'))}</h2>
        </div>
        <div class="ctox-pane-actions">
          <button type="button" class="ctox-pane-icon" data-action="refresh" aria-label="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}" title="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}">${iconSvg('refresh')}</button>
          <button type="button" class="ctox-pane-icon is-primary" data-action="new-task" aria-label="${escapeHtml(state.t('createResearch', 'Research anlegen'))}" title="${escapeHtml(state.t('createResearch', 'Research anlegen'))}">${iconSvg('plus')}</button>
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
          <span>${countText(rankedSources.length)} ${escapeHtml(state.t('verified', 'verifiziert'))}</span>
        </div>
        <div class="research-ranking-list">
          ${rankedSources.map(renderRankingRow).join('') || `<div class="research-empty">${escapeHtml(state.t('noVerifiedSources', 'Keine verifizierten Quellen verfügbar. Discovery-Kandidaten bleiben ohne Evidence-Score.'))}</div>`}
        </div>
      </section>
    </div>
  `;
  restorePaneScroll(root, scrollState);
}

// Die Aufgabenliste nennt Quellen, nicht Rohzeilen: fuer die gewaehlte Aufgabe
// die verifizierten Quellen des Evidence-Rankings, fuer die uebrigen die
// Zeilen des Quellenkatalogs ihrer Domain. Waehrend der Synchronisation steht
// dort keine Zahl.
function taskSourceSummary(task) {
  const dataState = researchDataState();
  if (dataState !== 'ready') {
    return dataState === 'failed'
      ? state.t('taskSourcesUnavailable', 'Quellen nicht verfügbar')
      : state.t('taskSourcesSyncing', 'Quellen werden synchronisiert …');
  }
  if (task.id === state.selectedTaskId) {
    const verified = evidenceRankedSources().length;
    return `${countText(verified)} ${state.t('verifiedSources', 'verifizierte Quellen')}`;
  }
  const base = knowledgeBaseForTask(task);
  if (!base) return state.t('noKnowledgeYet', 'noch keine Knowledge Base');
  const catalog = tableForKey(base, task.source_catalog_key || 'source_catalog')
    || base.tables.find((table) => /source_catalog/.test(String(table.table_key || '')))
    || null;
  const sources = Number(catalog?.row_count || 0);
  return `${countText(sources)} ${state.t('sourcesLabel', 'Quellen')}`;
}

function renderTaskButton(task) {
  const isActive = task.id === state.selectedTaskId;
  return `
    <button type="button" class="research-task-item${isActive ? ' is-active' : ''}" data-action="select-task" data-task-id="${escapeHtml(task.id)}">
      <strong>${escapeHtml(task.title)}</strong>
      <span>${escapeHtml(task.knowledge_domain)} · ${escapeHtml(taskSourceSummary(task))}</span>
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
  if (empty.kind === 'syncing') {
    return `
      <div class="ctox-syncing research-empty-card" role="status" aria-live="polite">
        <strong>${escapeHtml(empty.title)}</strong>
        <span>${escapeHtml(empty.body)}</span>
      </div>
    `;
  }
  return `
    <div class="ctox-empty research-empty research-empty-card">
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
  // Sync readiness is the second input of this data-driven empty: while a
  // backing collection has not finished its initial replication
  // (ready === false), an empty local result means "syncing", never
  // "no data".
  const tasksReadiness = collectionReadiness('research_tasks');
  const knowledgeReadiness = collectionReadiness('knowledge_tables');
  if (dataEmptyShowsSyncing(true, tasksReadiness) || dataEmptyShowsSyncing(true, knowledgeReadiness)) {
    return {
      kind: 'syncing',
      title: state.t('loadingKnowledge', 'Knowledge wird geladen...'),
      body: state.t('syncingResearchData', 'Research-Daten werden mit dieser Instanz synchronisiert.'),
    };
  }
  if (!tasksReadiness && !knowledgeReadiness && !state.initialDataReady) {
    // No readiness API available (standalone/test harness): keep the legacy
    // loading placeholder until the first local refresh finished.
    return {
      kind: 'loading',
      title: state.t('loadingKnowledge', 'Knowledge wird geladen...'),
      body: state.t('syncingResearchData', 'Research-Daten werden mit dieser Instanz synchronisiert.'),
    };
  }
  const failure = diagnosticFailures()[0];
  if (failure) {
    return {
      kind: 'empty',
      title: state.t('researchUnavailableTitle', 'Research ist gerade nicht verfügbar'),
      body: state.t('researchUnavailableBody', 'Dashboards erscheinen automatisch, sobald die Knowledge Base verfügbar ist.'),
    };
  }
  if (!state.knowledgeBases.length) {
    return {
      kind: 'empty',
      title: state.t('noKnowledgeDomains', 'Noch keine Knowledge Base verfügbar'),
      body: state.t('noKnowledgeDomainsBody', 'Lege eine Knowledge Base an oder lade Inhalte, um ein Research-Dashboard zu starten.'),
    };
  }
  return {
    kind: 'empty',
    title: state.t('noResearchTask', 'Keine Research-Aufgabe'),
    body: state.t('createTaskBase', 'Lege eine Aufgabe auf Basis einer Knowledge Base an.'),
  };
}

function renderCenter() {
  const root = pane('center');
  if (!root) return;
  const scrollState = capturePaneScroll(root);
  const task = selectedTask();
  if (!task) {
    disposeResearchGraph();
    root.innerHTML = renderNoTaskCenter();
    return;
  }
  const projection = currentGraphProjection(task);
  const visibleStatus = visibleResearchStatus();
  // Der Graph ueberlebt den Neuaufbau der Mittelspalte: jeder render() nach
  // einem Realtime-Ereignis (Queue-Tick, Befehlsstatus, Notiz) schrieb den
  // Mittelbereich per innerHTML neu und montierte die 3D-Szene von vorn -
  // alle paar Sekunden ein neuer Aufbau, waehrend ein Lauf lief
  // (skf.ctox.dev, 02.09.2026). Die bestehende Buehne wird ausgehaengt, in
  // den neuen Rahmen zurueckgesetzt und nur bei geaenderter Projektion mit
  // setData() aktualisiert.
  const preserved = state.showDiagram && state.graphSurface && state.graphSurfaceTaskId === task.id
    ? root.querySelector('[data-research-graph-host]')
    : null;
  root.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band research-center-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(task.knowledge_domain)}</span>
          <h2 class="ctox-pane-title">${escapeHtml(task.title)}</h2>
        </div>
        <div class="ctox-pane-actions">
          ${state.showDiagram ? `<span class="research-map-hint">${escapeHtml(state.t('graphNavigationHint', 'Ziehen: drehen · Scrollen: zoomen'))}</span>` : ''}
          <!-- Dominante Flussaktion der Mittelspalte. Sie steht hier nicht
               doppelt: die gleichnamige Schaltflaeche liegt IM Graphen und ist
               weg, sobald das Diagramm ausgeblendet wird. -->
          <button type="button"
                  class="ctox-pane-icon is-primary"
                  data-action="graph-ai"
                  data-graph-ai="research"
                  ${state.graph.busyAction ? 'disabled aria-disabled="true"' : ''}
                  title="${escapeHtml(state.t('targetedResearch', 'Nachrecherche'))}"
                  aria-label="${escapeHtml(state.t('targetedResearch', 'Nachrecherche'))}">
            ${iconSvg('search')}
          </button>
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
    ${visibleStatus ? `<div class="research-status-line" role="status" aria-live="polite">${escapeHtml(visibleStatus)}</div>` : ''}
    <div class="research-center-body${state.showDiagram ? '' : ' has-hidden-map'}">
      ${renderSemanticGraph(task, projection)}
      <section class="research-workbench">
        ${renderKnowledgeTableRowStates()}
        <div class="research-tabs-container">
          <div class="ctox-pane-tabs" role="tablist" aria-label="Research views">
            ${countedTabButton('sources', state.t('sources', 'Sources'), evidenceRankedSources().length)}
            ${countedTabButton('candidates', state.t('candidates', 'Candidates'), state.candidateModels.length)}
            ${countedTabButton('measurements', state.t('measurements', 'Measurements'), filterMeasurementRowsForEvidence(state.measurementRows, state.sourceModels).length)}
            ${countedTabButton('knowledge', state.t('knowledge', 'Knowledge'), knowledgeClaims().length)}
            ${countedTabButton('reports', state.t('reports', 'Fachberichte'), researchReportsForTask(task).length)}
          </div>
          ${state.activeTab === 'sources' ? `
            <div class="ctox-pane-tabs research-view-toggle">
              ${sourcesViewToggleButton()}
            </div>
          ` : ''}
        </div>
        <div class="research-table-host">
          ${renderActiveTable(task)}
        </div>
      </section>
    </div>
  `;
  if (!state.showDiagram) {
    disposeResearchGraph();
  } else if (preserved) {
    const placeholder = root.querySelector('[data-research-graph-host]');
    if (placeholder) placeholder.replaceWith(preserved);
    root.querySelector('[data-research-graph-loading]')?.remove();
    const key = projection.fingerprint || graphProjectionKey(projection);
    if (key !== state.graphSurfaceKey) {
      state.graphSurfaceKey = key;
      state.graphSurface.setData(projection);
    }
  } else {
    scheduleResearchGraphMount(task, projection);
  }
  restorePaneScroll(root, scrollState);
}

// Stabile Kennung einer Projektion fuer den In-Place-Vergleich: Knoten,
// Kanten, Ebene und Detailstufe. Metriken oder Laufstatus aendern die Szene
// nicht und loesen deshalb kein setData() aus.
function graphProjectionKey(projection) {
  const nodes = (projection.nodes || []).map((node) => `${node.id}:${node.size ?? ''}:${node.group ?? ''}`).join('|');
  const links = (projection.links || []).map((link) => `${graphLinkNodeId(link.source)}>${graphLinkNodeId(link.target)}:${link.weight ?? ''}`).join('|');
  return `${projection.layer || ''}#${projection.detailLevel || state.graph.detailLevel || ''}#${nodes.length}#${links.length}#${simpleHash(nodes)}#${simpleHash(links)}`;
}

function simpleHash(text) {
  let hash = 0;
  for (let index = 0; index < text.length; index += 1) {
    hash = ((hash << 5) - hash + text.charCodeAt(index)) | 0;
  }
  return (hash >>> 0).toString(36);
}

function renderSemanticGraph(task, projection) {
  const metrics = projection.metrics || {};
  const runInfo = researchRunInfo(task);
  const live = ['queued', 'running'].includes(runInfo.statusKind);
  return `
    <section class="research-graph-shell" aria-label="${escapeHtml(state.t('semanticResearchGraph', 'Semantischer Research-Graph'))}">
      <div class="research-graph-stage${state.graph.panel === 'hidden' ? '' : ' has-insights'}">
        <div class="research-graph-canvas" data-research-graph-host role="img" aria-label="${escapeHtml(state.t('graphCanvasLabel', 'Interaktiver semantischer Graph. Ziehen dreht die Szene, Scrollen zoomt.'))}"></div>
        <div class="research-graph-loading" data-research-graph-loading role="status">
          <span class="research-spinner" aria-hidden="true"></span>
          <span>${escapeHtml(state.t('graphLoading', 'Semantischen Graph aufbauen …'))}</span>
        </div>
        <div class="research-graph-meta">
          <div>
            <strong>${escapeHtml(state.t('semanticResearchGraph', 'Semantic Research Graph'))}</strong>
            <span data-graph-summary>${graphSummary(metrics)}</span>
          </div>
          <span class="research-graph-live${live ? ' is-live' : ''}">${live ? escapeHtml(state.t('liveRun', 'LIVE · CTOX aktualisiert')) : escapeHtml(projection.origin === 'persisted' ? state.t('persistedGraph', 'Knowledge Graph') : projection.status === 'invalid_graph_contract' ? 'invalid_graph_contract' : state.t('derivedGraph', 'Live projection'))}</span>
        </div>
        <label class="research-graph-search">
          <span class="research-sr-only">${escapeHtml(state.t('searchGraph', 'Graph durchsuchen'))}</span>
          ${iconSvg('search')}
          <input type="search" data-action="graph-search" value="${escapeHtml(state.graph.query)}" placeholder="${escapeHtml(state.t('searchConcepts', 'Begriff suchen …'))}" autocomplete="off" />
        </label>
        <div class="research-graph-layer-switch" role="group" aria-label="${escapeHtml(state.t('graphLayer', 'Graph-Ebene'))}">
          ${graphLayerButton('all', state.t('all', 'Alle'))}
          ${graphLayerButton('concepts', state.t('concepts', 'Themen'))}
          ${graphLayerButton('sources', state.t('sources', 'Quellen'))}
          ${graphLayerButton('evidence', state.t('evidence', 'Belege'))}
        </div>
        ${state.graph.panel === 'hidden' ? '' : renderGraphInsights(projection)}
        ${projection.status === 'invalid_graph_contract' ? `<div class="research-graph-contract-error" data-graph-contract-status="invalid_graph_contract" data-graph-contract-errors="${escapeHtml((projection.errors || state.graphContractErrors || []).join(','))}">invalid_graph_contract</div>` : ''}
        <!-- Untere Leiste: Aktionszeile (links) und Graph-Steuerung (rechts)
             teilen sich EINE Reihe. Beide lagen frueher getrennt absolut am
             unteren Rand und ueberdeckten sich, sobald die Buehne schmal wurde
             (Dreispaltenansicht ab ~1180px: 34px Ueberlappung, gemessen
             31.08.). Als Flex-Zeile ist die Ueberdeckung strukturell
             ausgeschlossen. -->
        <div class="research-graph-bottombar">
          <div class="research-graph-actions">
            <!-- title/aria-label sind Pflicht: in der schmalen Buehne rendert
                 die Zeile nur die Glyphe (siehe index.css), der Knopf braucht
                 dann trotzdem einen Namen. -->
            <button type="button" class="research-graph-action" data-action="graph-ai" data-graph-ai="research" title="${escapeHtml(state.t('targetedResearch', 'Nachrecherche'))}" aria-label="${escapeHtml(state.t('targetedResearch', 'Nachrecherche'))}" ${state.graph.busyAction ? 'disabled' : ''}>${iconSvg('search')}<span>${escapeHtml(state.t('targetedResearch', 'Nachrecherche'))}</span></button>
            <button type="button" class="research-graph-action" data-action="graph-ai" data-graph-ai="document" title="${escapeHtml(evidenceRankedSources().length ? state.t('createDocument', 'Dokument erstellen') : state.t('reportUnavailable', 'Report nicht verfügbar'))}" aria-label="${escapeHtml(evidenceRankedSources().length ? state.t('createDocument', 'Dokument erstellen') : state.t('reportUnavailable', 'Report nicht verfügbar'))}" ${state.graph.busyAction || !evidenceRankedSources().length ? 'disabled' : ''}>${iconSvg('file')}<span>${escapeHtml(evidenceRankedSources().length ? state.t('createDocument', 'Dokument erstellen') : state.t('reportUnavailable', 'Report nicht verfügbar'))}</span></button>
          </div>
          <div class="research-graph-rail" aria-label="${escapeHtml(state.t('graphControls', 'Graph-Steuerung'))}">
            <button type="button" data-action="graph-command" data-graph-command="panel" class="research-graph-tool${state.graph.panel !== 'hidden' ? ' is-active' : ''}" aria-label="${escapeHtml(state.t('toggleInsights', 'Insights ein-/ausblenden'))}" title="${escapeHtml(state.t('toggleInsights', 'Insights ein-/ausblenden'))}">${iconSvg('layers')}</button>
            <div class="research-graph-detail" role="group" aria-label="${escapeHtml(state.t('graphDetail', 'Detailstufe'))}">
              ${graphDetailButton('overview', state.t('detailOverview', 'Übersicht'))}
              ${graphDetailButton('standard', state.t('detailStandard', 'Standard'))}
              ${graphDetailButton('deep', state.t('detailDeep', 'Tief'))}
            </div>
            <button type="button" data-action="graph-dimension" class="research-graph-tool research-graph-dimension" aria-label="${state.graph.dimensions === 3 ? escapeHtml(state.t('switch2d', 'Zu 2D wechseln')) : escapeHtml(state.t('switch3d', 'Zu 3D wechseln'))}" title="${state.graph.dimensions === 3 ? escapeHtml(state.t('switch2d', 'Zu 2D wechseln')) : escapeHtml(state.t('switch3d', 'Zu 3D wechseln'))}">${state.graph.dimensions}D</button>
            <button type="button" data-action="graph-command" data-graph-command="fit" class="research-graph-tool" aria-label="${escapeHtml(state.t('fitGraph', 'Graph einpassen'))}" title="${escapeHtml(state.t('fitGraph', 'Graph einpassen'))}">${iconSvg('focus')}</button>
          </div>
        </div>
      </div>
    </section>
  `;
}

function graphLayerButton(id, label) {
  return `<button type="button" data-action="graph-layer" data-graph-layer="${id}" class="${state.graph.layer === id ? 'is-active' : ''}" aria-pressed="${state.graph.layer === id}">${escapeHtml(label)}</button>`;
}

function graphDetailButton(id, label) {
  return `<button type="button" data-action="graph-detail" data-graph-detail="${id}" class="${state.graph.detailLevel === id ? 'is-active' : ''}" aria-pressed="${state.graph.detailLevel === id}" title="${escapeHtml(label)}"><span>${escapeHtml(label)}</span></button>`;
}

function graphSummary(metrics = {}) {
  const nodeLabel = state.graph.layer === 'concepts'
    ? state.t('concepts', 'Themen')
    : state.t('nodes', 'Elemente');
  return `${metrics.nodeCount || 0} ${escapeHtml(nodeLabel)} · ${metrics.linkCount || 0} ${escapeHtml(state.t('relations', 'Verknüpfungen'))} · ${metrics.clusterCount || 0} ${escapeHtml(state.t('clusters', 'Themenfelder'))}`;
}

function renderGraphInsights(projection) {
  const metrics = projection.metrics || {};
  const body = state.graph.panel === 'analytics'
    ? `
      <dl class="research-graph-metrics">
        <div><dt>${escapeHtml(state.t('nodes', 'Elemente'))}</dt><dd>${metrics.nodeCount || 0}</dd></div>
        <div><dt>${escapeHtml(state.t('relations', 'Verknüpfungen'))}</dt><dd>${metrics.linkCount || 0}</dd></div>
        <div><dt>${escapeHtml(state.t('clusters', 'Themenfelder'))}</dt><dd>${metrics.clusterCount || 0}</dd></div>
        <div><dt>${escapeHtml(state.t('sources', 'Quellen'))}</dt><dd>${metrics.sourceCount || 0}</dd></div>
      </dl>
      <p>${escapeHtml(state.t('graphMethod', 'Größe zeigt fachliche Vernetzung, Farben gruppieren Themenfelder, Relationen zeigen Typ, Konfidenz und Provenienz.'))}</p>
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
        <button type="button" data-action="graph-panel" data-graph-panel="topics" class="${state.graph.panel === 'topics' ? 'is-active' : ''}" role="tab" aria-selected="${state.graph.panel === 'topics'}">${escapeHtml(state.t('topics', 'Themenfelder'))}</button>
        <button type="button" data-action="graph-panel" data-graph-panel="analytics" class="${state.graph.panel === 'analytics' ? 'is-active' : ''}" role="tab" aria-selected="${state.graph.panel === 'analytics'}">${escapeHtml(state.t('analytics', 'Qualität'))}</button>
      </div>
      ${body}
      ${renderGraphInspector(projection)}
    </aside>
  `;
}

function renderGraphInspector(projection) {
  const node = projection.nodes.find((candidate) => candidate.id === state.selectedGraphNodeId);
  if (!node) return '<div class="research-graph-inspector" data-graph-inspector>Node auswählen, um Relation und Provenienz zu prüfen.</div>';
  const relations = projection.links
    .filter((link) => graphLinkNodeId(link.source) === node.id || graphLinkNodeId(link.target) === node.id)
    .slice(0, 6)
    .map((link) => `<li><strong>${escapeHtml(link.label || link.relationType || 'co_occurs')}</strong><span>${escapeHtml(formatGraphConfidence(link.confidence))}</span><small>${escapeHtml(formatGraphProvenance(link.provenance))}</small></li>`)
    .join('');
  return `<div class="research-graph-inspector" data-graph-inspector><strong>${escapeHtml(node.label)}</strong><span>Confidence ${escapeHtml(formatGraphConfidence(node.confidence))}</span><small>Provenienz: ${escapeHtml(formatGraphProvenance(node.provenance))}</small>${relations ? `<ul>${relations}</ul>` : ''}</div>`;
}

function formatGraphConfidence(value) {
  const number = Number(value);
  return Number.isFinite(number) ? `${Math.round((number > 1 ? number / 100 : number) * 100)}%` : 'n/a';
}

function formatGraphProvenance(value) {
  if (!value) return 'nicht vorhanden';
  if (typeof value === 'string') return value;
  return [value.method, value.table, value.evidenceId, value.kind].filter(Boolean).join(' · ') || 'verifiziert';
}

function currentGraphProjection(task = selectedTask()) {
  const evidenceGraphRows = filterGraphRowsForEvidence(state.graphNodeRows, state.graphEdgeRows, evidenceSourceIds(state.sourceModels));
  const key = graphProjectionFingerprint(task, evidenceGraphRows, state.graph.layer, state.sourceModels, state.measurementRows);
  const cached = state.graphProjectionCache.get(key);
  const baseProjection = cached || enrichGraphSemanticMetadata(buildResearchGraphProjection({
      task,
      sourceModels: evidenceSourceModels(state.sourceModels),
      measurementRows: filterMeasurementRowsForEvidence(state.measurementRows),
      graphNodeRows: evidenceGraphRows.nodes,
      graphEdgeRows: evidenceGraphRows.edges,
      graphLayer: state.graph.layer,
      detailLevel: 'deep',
      visibleLimit: GRAPH_DETAIL_LEVELS.deep,
      verifiedSourceIds: evidenceSourceIds(state.sourceModels),
      graphContractStatus: evidenceGraphRows.status,
      graphContractErrors: evidenceGraphRows.errors,
    }), evidenceGraphRows.nodes, evidenceGraphRows.edges, task);
  const projection = baseProjection.status === 'invalid_graph_contract'
    ? baseProjection
    : sliceResearchGraphProjection(baseProjection, state.graph.detailLevel, state.graph.visibleLimit, state.graph.layer);
  state.graphProjection = projection;
  if (!cached) state.graphProjectionCache.set(key, baseProjection);
  while (state.graphProjectionCache.size > 8) state.graphProjectionCache.delete(state.graphProjectionCache.keys().next().value);
  return projection;
}

function enrichGraphSemanticMetadata(projection, graphNodeRows = [], graphEdgeRows = [], task = null) {
  const nodeMetadata = new Map(graphNodeRows.map((row) => [firstString(row, ['node_id', 'id', 'concept_id', 'key']), graphSemanticMetadata(row)]));
  const edgeMetadata = new Map(graphEdgeRows.map((row) => [firstString(row, ['edge_id', 'id']), graphSemanticMetadata(row)]));
  return {
    ...projection,
    metadata: {
      ...(projection?.metadata || {}),
      task: task ? { id: task.id || '', title: task.title || '', knowledge_domain: task.knowledge_domain || '' } : null,
      provenance: 'research-graph-projection',
    },
    nodes: (projection?.nodes || []).map((node) => ({
      ...node,
      ...nodeMetadata.get(node.id),
    })),
    links: (projection?.links || []).map((link) => ({
      ...link,
      ...edgeMetadata.get(link.id),
    })),
  };
}

function graphSemanticMetadata(row) {
  if (!row || typeof row !== 'object') return {};
  const metadata = parseObject(row.metadata_json ?? row.metadata) || {};
  return compactDefined({
    label: firstString(row, ['label', 'title', 'concept', 'term']) || undefined,
    tags: parseStringList(row.tags_json ?? row.tags ?? row.labels),
    description: firstString(row, ['description', 'summary', 'note']) || undefined,
    clusterHint: firstString(row, ['cluster_id', 'cluster', 'community']) || undefined,
    provenance: parseObject(row.provenance_json ?? row.provenance) || undefined,
    metadata: Object.keys(metadata).length ? metadata : undefined,
  });
}

function compactDefined(value) {
  return Object.fromEntries(Object.entries(value).filter(([, item]) => item !== undefined));
}

function graphProjectionFingerprint(task, graphRows, layer, sourceModels, measurementRows) {
  return JSON.stringify({
    task: task?.id || task?.knowledge_domain || '',
    layer,
    graphStatus: graphRows.status || '',
    nodes: (graphRows.nodes || []).map((row) => [row.node_id || row.id, row.updated_at, row.confidence, row.provenance_json, row.source_ids_json]),
    edges: (graphRows.edges || []).map((row) => [row.edge_id || row.id, row.updated_at, row.relation_type, row.confidence, row.provenance_json, row.source_ids_json]),
    sources: (sourceModels || []).map((source) => [source.id, source.evidenceEligible, source.score, source.row?.snapshot_id, source.row?.snapshot_hash]),
    evidence: (measurementRows || []).map((row) => [row.evidence_id || row.claim_id || row.id, row.source_id, row.snapshot_id, row.snapshot_hash, row.updated_at]),
  });
}

function refreshGraphProjectionInPlace() {
  if (!state.graphSurface) {
    renderCenter();
    return;
  }
  const projection = currentGraphProjection();
  const center = pane('center');
  center?.querySelectorAll('[data-action="graph-layer"]').forEach((button) => {
    const active = button.dataset.graphLayer === state.graph.layer;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  center?.querySelectorAll('[data-action="graph-detail"]').forEach((button) => {
    const active = button.dataset.graphDetail === state.graph.detailLevel;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  const summary = center?.querySelector('[data-graph-summary]');
  if (summary) summary.innerHTML = graphSummary(projection.metrics);
  updateGraphInsights();
  state.graphSurfaceKey = projection.fingerprint || graphProjectionKey(projection);
  state.graphSurface.setData(projection);
}

async function scheduleResearchGraphMount(task, projection) {
  const root = pane('center');
  const host = root?.querySelector('[data-research-graph-host]');
  if (!host || !state.showDiagram) return;
  disposeResearchGraph();
  const token = ++state.graphMountToken;
  const loading = root.querySelector('[data-research-graph-loading]');
  if (!projection.nodes.length) {
    if (loading) loading.innerHTML = projection.status === 'invalid_graph_contract'
      ? '<strong>invalid_graph_contract</strong><span>Persistierte Graphdaten erfüllen den Evidence-/Provenienzvertrag nicht.</span>'
      : `<span>${escapeHtml(state.t('graphNoData', 'Noch keine Begriffe. Starte eine Nachrecherche oder füge Quellen hinzu.'))}</span>`;
    return;
  }
  try {
    const moduleUrl = new URL('./research-graph.mjs', import.meta.url);
    moduleUrl.search = new URL(import.meta.url).search;
    const graphModule = await import(moduleUrl.href);
    if (token !== state.graphMountToken || !host.isConnected || selectedTask()?.id !== task.id) return;
    state.graphSurfaceTaskId = task.id;
    state.graphSurfaceKey = projection.fingerprint || graphProjectionKey(projection);
    state.graphSurface = graphModule.createResearchGraph(host, {
      projection,
      dimensions: state.graph.dimensions,
      autoRotate: state.graph.autoRotate,
      sourceCountLabel: state.t('sources', 'Quellen'),
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
  state.graphSurfaceTaskId = '';
  state.graphSurfaceKey = '';
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
  updateGraphInsights();
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
  stage.classList.toggle('has-insights', state.graph.panel !== 'hidden');
  if (state.graph.panel === 'hidden') {
    existing?.remove();
    toggle?.classList.remove('is-active');
    return;
  }
  const markup = renderGraphInsights(state.graphProjection);
  if (existing) existing.outerHTML = markup;
  // Anker ist die untere Leiste (sie traegt Aktionszeile UND Steuerung); ein
  // Einhaengen vor `.research-graph-actions` landete sonst IN der Leiste.
  else stage.querySelector('.research-graph-bottombar')?.insertAdjacentHTML('beforebegin', markup);
  toggle?.classList.add('is-active');
}

async function dispatchGraphAiAction(action) {
  const task = selectedTask();
  if (!task || state.graph.busyAction) return;
  if (action !== 'document') {
    const selectedNode = state.graphProjection?.nodes?.find((node) => node.id === state.selectedGraphNodeId) || null;
    if (!selectedNode) {
      await runSelectedResearch();
      return;
    }
  }
  if (action === 'document' && !evidenceRankedSources().length) {
    setStatus(state.t('reportRequiresVerifiedSources', 'Reports sind ohne verifizierte Quellen nicht verfügbar.'));
    renderCenter();
    return;
  }
  if (action === 'document') {
    const base = knowledgeBaseForTask(task);
    const latestRun = latestEvidenceRunForTask(task.id, state.runs);
    const selectedNode = state.graphProjection?.nodes?.find((node) => node.id === state.selectedGraphNodeId) || null;
    const selectedIds = eligibleGraphFocusSourceIds(selectedNode, state.sourceModels);
    const lineage = graphDocumentLineage(task, base, latestRun, state.sourceModels, selectedIds);
    if (!lineage.ok) {
      setStatus(`${state.t('graphDocumentUnavailable', 'Dokument nicht erstellt: belastbare Knowledge-/Quellen-Provenienz fehlt')}. ${lineage.reason}`);
      renderCenter();
      return;
    }
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
  const commandId = `cmd_${crypto.randomUUID()}`;
  const researchRunId = `research_run_${crypto.randomUUID()}`;
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
    `Research Run ID: ${researchRunId}`,
    `Research Command ID: ${commandId}`,
    `Fokusbegriff: ${focus}`,
    related.length ? `Benachbarte Begriffe: ${related.join(', ')}` : '',
    `Knowledge domain: ${task.knowledge_domain}`,
    '',
    task.prompt || '',
    '',
    'Nutze systematic-research und die CTOX Web-Research-Tools. Prüfe die vorhandenen Belege, schließe erkennbare Lücken und schreibe neue Quellen und Belege sofort in die bestehenden Knowledge-Tabellen.',
    `Schreibe auf jede erzeugte oder aktualisierte Knowledge-Zeile research_run_id=${researchRunId} und research_command_id=${commandId}.`,
    'Aktualisiere semantic_graph_nodes und semantic_graph_edges inkrementell aus verifizierten Evidenzzeilen. Verwende kanonische fachliche Mehrwort-Begriffe, klare Clusterbezeichnungen und typisierte Relationen. Technische Feldnamen, IDs, Hashes, URLs und generische Metadaten dürfen keine Konzepte werden. Jeder Knoten und jede Kante braucht Source-IDs, Konfidenz und Provenienz; überschreibe keine belegten Daten ohne neuen Nachweis.',
  ].filter(Boolean).join('\n');
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
      research_run_id: researchRunId,
      research_command_id: commandId,
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
        row_lineage_required: {
          research_run_id: researchRunId,
          research_command_id: commandId,
        },
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
      research_run_id: researchRunId,
      research_command_id: commandId,
      graph_node_id: selectedNode?.id || '',
    },
  });
  const run = {
    id: researchRunId,
    task_id: task.id,
    status: result?.task_status || result?.status || 'queued',
    command_id: commandId,
    task_queue_id: result?.task_id || '',
    identified_count: state.candidateRows.length + state.sourceRows.length,
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
  const base = knowledgeBaseForTask(task);
  const latestRun = latestEvidenceRunForTask(task.id, state.runs);
  const selectedNode = state.graphProjection?.nodes?.find((node) => node.id === state.selectedGraphNodeId) || null;
  const graphFocusSourceIds = eligibleGraphFocusSourceIds(selectedNode, state.sourceModels);
  const lineage = graphDocumentLineage(task, base, latestRun, state.sourceModels, graphFocusSourceIds);
  if (!lineage.ok) throw new Error(lineage.reason);
  const focus = selectedNode?.label || task.title;
  const title = `${task.title} · ${focus}`.slice(0, 120);
  const filename = `${slugId(title).slice(0, 82) || 'research-graph-report'}.docx`;
  const outputPath = `runtime/business-os/documents/generated/${filename}`;
  const commandId = `cmd_${crypto.randomUUID()}`;
  const instruction = [
    `Erstelle ein belastbares Word-Dokument aus dem Research-Graph "${task.title}".`,
    `Research Run ID: ${latestRun?.id || ''}`,
    `Research Command ID: ${commandId}`,
    `Fokus: ${focus}`,
    `Knowledge domain: ${task.knowledge_domain}`,
    `Knowledge version: ${lineage.knowledge_version_id}`,
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
      knowledge_version_id: lineage.knowledge_version_id,
      knowledge_version: lineage.knowledge_version,
      source_receipts: lineage.source_receipts,
      requested_snapshot_hashes: lineage.requested_snapshot_hashes,
      evidence_lineage: lineage.evidence_lineage,
      graph_focus: {
        node_id: selectedNode?.id || '',
        label: focus,
        source_ids: graphFocusSourceIds,
        snapshot_hashes: lineage.source_receipts
          .filter((receipt) => !graphFocusSourceIds.length || graphFocusSourceIds.includes(receipt.source_id))
          .map((receipt) => receipt.snapshot_hash),
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
          { kind: 'knowledge_version', id: lineage.knowledge_version_id },
        ],
        knowledge_version_id: lineage.knowledge_version_id,
        source_receipts: lineage.source_receipts,
        requested_snapshot_hashes: lineage.requested_snapshot_hashes,
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
    extraction: 'evidence_grounded_domain_concepts',
    node_kinds: [...GRAPH_NODE_KINDS],
    node_fields: ['node_id', 'label', 'kind', 'description', 'aliases_json', 'cluster_id', 'cluster_label', 'occurrences', 'evidence_count', 'betweenness_centrality', 'confidence', 'source_ids_json', 'provenance_json'],
    edge_fields: ['edge_id', 'source_id', 'target_id', 'relation_type', 'label', 'weight', 'confidence', 'source_ids_json', 'provenance_json'],
    allowed_relation_types: [...GRAPH_RELATION_TYPES],
    semantic_labels_required: true,
    technical_metadata_keys_forbidden_as_concepts: true,
    community_detection: 'automatic_modularity',
    node_importance: 'betweenness_centrality',
    incremental_writeback: true,
    provenance_required: true,
    verified_source_required: true,
    confidence_range: [0, 1],
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
          <button type="button" class="ctox-pane-icon is-primary" data-action="refresh" aria-label="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}" title="${escapeHtml(state.t('refreshData', 'Daten neu laden'))}">${iconSvg('refresh')}</button>
          <button type="button" class="ctox-pane-icon" data-action="new-task" aria-label="${escapeHtml(state.t('createResearch', 'Research anlegen'))}" title="${escapeHtml(state.t('createResearch', 'Research anlegen'))}">${iconSvg('plus')}</button>
        </div>
      </div>
    </header>
    <div class="research-center-empty-body">
      <section class="${empty.kind === 'syncing' ? 'ctox-syncing' : 'ctox-empty'} research-empty-state-panel"${empty.kind === 'syncing' ? ' role="status" aria-live="polite"' : ''}>
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
            <input type="text" class="ctox-input" disabled placeholder="${escapeHtml(state.t('searchSourcesPlaceholder', 'Quelle suchen: Titel, Autor, DOI, Kennung …'))}" />
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

  if (inferResearchKind(selectedTask()) !== 'bearing') {
    const taxonomy = domainTaxonomy(selectedTask());
    return taxonomy.clusters.find((cluster) => cluster.pattern.test(text))?.id || taxonomy.fallback;
  }

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
  return 'other';
}

function discoveryGraph(task) {
  const persisted = persistedCitationDiscoveryGraph(task);
  if (persisted) return persisted;

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

function persistedCitationDiscoveryGraph(task) {
  const rows = Array.isArray(state.candidateRows) ? state.candidateRows : [];
  const candidates = rows.map((row, index) => {
    const rawPaths = firstString(row, ['discovery_paths_json']);
    let paths = [];
    if (rawPaths) {
      try {
        const parsed = JSON.parse(rawPaths);
        if (Array.isArray(parsed)) paths = parsed.filter((item) => item && typeof item === 'object');
      } catch {
        paths = [];
      }
    }
    if (!paths.length && firstString(row, ['discovery_round', 'seed_source_id', 'citation_hop', 'citation_direction', 'relation_type'])) {
      paths = [{
        round: firstString(row, ['discovery_round']),
        method: firstString(row, ['discovery_method']),
        seed_source_id: firstString(row, ['seed_source_id']),
        seed_identifier: firstString(row, ['seed_identifier']),
        hop: firstString(row, ['citation_hop']) || '0',
        direction: firstString(row, ['citation_direction']) || 'seed',
        relation_type: firstString(row, ['relation_type']) || 'search_seed',
      }];
    }
    if (!paths.length) return null;
    const key = firstString(row, ['candidate_key', 'source_id', 'doi', 'canonical_url', 'url']) || `candidate-${index}`;
    const canonicalUrl = firstString(row, ['canonical_url', 'url', 'source_url']);
    const source = state.sourceModels.find((item) => (
      item.id === firstString(row, ['source_id'])
      || (canonicalUrl && [item.canonicalUrl, item.url].includes(canonicalUrl))
    ));
    return {
      row,
      key,
      title: firstString(row, ['title', 'source_title', 'name']) || key,
      source,
      paths,
      hop: Math.max(0, ...paths.map((path) => Number(path.hop) || 0)),
      admitted: /admitted|eligible|verified/.test(firstString(row, ['verification_state', 'verification_status']).toLowerCase()),
    };
  }).filter(Boolean);
  if (!candidates.length) return null;

  const visibleLimit = clampNumber(state.graph.visibleLimit || 60, 20, 100);
  const visible = candidates
    .sort((a, b) => Number(b.admitted) - Number(a.admitted) || a.hop - b.hop || a.title.localeCompare(b.title))
    .slice(0, visibleLimit);
  const visibleKeys = new Set(visible.map((candidate) => candidate.key));
  const sourceKeyToCandidate = new Map();
  for (const candidate of visible) {
    for (const value of [
      candidate.key,
      firstString(candidate.row, ['source_id']),
      firstString(candidate.row, ['doi', 'doi_or_stable_id']),
    ]) {
      if (value) sourceKeyToCandidate.set(value, candidate);
    }
  }

  const nodes = [{
    id: 'knowledge',
    kind: 'knowledge',
    label: task.title,
    title: task.knowledge_domain || task.title,
    meta: `${candidates.length} Kandidaten · ${visible.length} sichtbar`,
    x: 8,
    y: 50,
  }];
  const edges = [];
  const byHop = new Map();
  for (const candidate of visible) {
    const hop = Math.min(4, candidate.hop);
    const bucket = byHop.get(hop) || [];
    bucket.push(candidate);
    byHop.set(hop, bucket);
  }
  const nodeIdByKey = new Map();
  for (const [hop, bucket] of [...byHop.entries()].sort(([a], [b]) => a - b)) {
    bucket.forEach((candidate, index) => {
      const nodeId = `candidate_${nodes.length}`;
      nodeIdByKey.set(candidate.key, nodeId);
      nodes.push({
        id: nodeId,
        kind: candidate.admitted ? 'source-strong' : 'source',
        label: shortLabel(candidate.title),
        title: candidate.title,
        meta: `Hop ${candidate.hop} · ${candidate.paths.length} Pfad${candidate.paths.length === 1 ? '' : 'e'}`,
        sourceId: candidate.source?.id || '',
        x: 24 + hop * 17,
        y: 8 + ((index + 1) * 84) / (bucket.length + 1),
      });
    });
  }
  for (const candidate of visible) {
    const to = nodeIdByKey.get(candidate.key);
    for (const path of candidate.paths) {
      const seedKey = String(path.seed_source_id || path.seed_identifier || '').trim();
      const seedCandidate = sourceKeyToCandidate.get(seedKey);
      const from = seedCandidate && visibleKeys.has(seedCandidate.key)
        ? nodeIdByKey.get(seedCandidate.key)
        : 'knowledge';
      if (!from || !to || from === to) continue;
      const kind = String(path.direction || path.relation_type || 'seed').toLowerCase();
      if (!edges.some((edge) => edge.from === from && edge.to === to && edge.kind === kind)) {
        edges.push({ from, to, kind });
      }
    }
  }
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
  if (state.activeTab === 'candidates') return renderSourcesWorkbench(state.candidateModels, { candidates: true });
  if (state.activeTab === 'measurements') return renderMeasurementsTable();
  if (state.activeTab === 'knowledge') return renderKnowledgeTables(task);
  if (state.activeTab === 'reports') return renderReportsWorkbench(task);
  return renderSourcesWorkbench(evidenceRankedSources());
}

/* Listenansicht (Betreiber-Direktive 31.08.2026): genau EINE kompakte Zeile
   je Quelle — Titel links, ein Kurz-Meta rechts (Grade · Score), enge
   Zeilenhoehe, maximale Dichte. Die ausfuehrlichen Detailfelder gehoeren der
   Kartenansicht; Achsen- und Klassenwerte stehen weiterhin in der
   Quellen-Schublade. Der Kurz-Meta traegt den kanonischen Link, und zwar nur
   fuer belegfaehige Quellen — Discovery-URLs erscheinen nie. */
function renderSourcesList(filteredList = state.sourceModels) {
  if (!filteredList.length) {
    return `<div class="research-empty">${escapeHtml(sourcesEmptyText())}</div>`;
  }
  return `
    <div class="research-source-list">
      ${filteredList.map((source) => {
        const selected = source.id === state.selectedSourceId;
        const meta = `${escapeHtml(source.grade)}${source.evidenceEligible ? ` · ${formatPortfolioScore(source.score)}` : ''}`;
        const openable = source.evidenceEligible && source.canonicalUrl;
        return `
        <div class="research-source-row${selected ? ' is-selected' : ''}" data-source-id="${escapeHtml(source.id)}" data-evidence-status="${escapeHtml(source.evidenceStatus)}">
          <button type="button"
                  class="research-source-row-title"
                  data-action="select-source"
                  data-source-id="${escapeHtml(source.id)}"
                  aria-current="${selected}"
                  title="${escapeHtml(source.title)}">${escapeHtml(source.title)}</button>
          ${openable
            ? `<a class="research-source-row-meta ${gradeBadgeClass(source.grade)}"
                  href="${escapeHtml(source.canonicalUrl)}"
                  target="_blank"
                  rel="noreferrer"
                  title="${escapeHtml(state.t('openLabel', 'Öffnen'))}">${meta}</a>`
            : `<span class="research-source-row-meta ${gradeBadgeClass(source.grade)}">${meta}</span>`}
        </div>`;
      }).join('')}
    </div>
  `;
}

/* Ein Bedienelement statt zweier nebeneinanderliegender Knoepfe
   (Betreiber-Direktive 31.08.2026): der Knopf zeigt die Ansicht, in die er
   wechselt. Er ist damit eine Aktion und kein Zustand — deshalb kein
   aria-pressed und kein is-active. */
function sourcesViewToggleButton() {
  const showsCards = state.sourcesViewMode !== 'table';
  const nextMode = showsCards ? 'table' : 'shards';
  const label = showsCards
    ? state.t('showAsList', 'Als Liste anzeigen')
    : state.t('showAsCards', 'Als Karten anzeigen');
  return `<button type="button"
          class="ctox-pane-tab"
          data-action="sources-view"
          data-view-mode="${nextMode}"
          aria-label="${escapeHtml(label)}"
          title="${escapeHtml(label)}">${iconSvg(showsCards ? 'list' : 'grid')}</button>`;
}

function sourcesEmptyText() {
  return state.activeTab === 'candidates'
    ? state.t('noCandidates', 'Keine Kandidaten vorhanden.')
    : state.t('noSources', 'Keine Quellen vorhanden.');
}

// Ein Themen-Chip ohne Treffer ist ein toter Filter: er stammt aus der festen
// Domain-Taxonomie, nicht aus den Daten, und fuehrte auf Instanzen mit anderem
// Quellenbestand zu leeren Listen hinter jedem Chip. Angeboten werden nur
// Cluster, die mindestens eine Quelle der aktuellen Liste treffen; der
// aktive Chip bleibt sichtbar, damit er sich abwaehlen laesst.
function availableSubthemes(sourceModels = [], activeTag = 'all') {
  const clusters = domainTaxonomy(selectedTask()).clusters;
  const hit = new Set(sourceModels.map((source) => getSearchCluster(source)));
  return [
    { id: 'all', label: state.t('subthemeAll', 'Alle') },
    ...clusters.filter((cluster) => hit.has(cluster.id) || cluster.id === activeTag),
  ];
}

function renderSourcesWorkbench(sourceModels = evidenceRankedSources(), { candidates = false } = {}) {
  const activeTag = state.sourceActiveTag || 'all';
  const subthemes = availableSubthemes(sourceModels, activeTag);

  const filtered = filteredSources(sourceModels);

  return `
    <div class="research-sources-shards-wrapper">
      <div class="research-sources-shards-toolbar">
        <input type="text"
               class="ctox-input research-sources-shards-search"
               id="research-source-search-input"
               data-action="source-search"
               placeholder="${escapeHtml(candidates
                 ? state.t('searchCandidatesPlaceholder', 'Kandidat suchen: DOI, Titel, Publisher ...')
                 : state.t('searchSourcesPlaceholder', 'Quelle suchen: Titel, Autor, DOI, Kennung …'))}"
               value="${escapeHtml(state.sourceSearchTerm || '')}"
               autocomplete="off" />
        <div class="research-sources-shards-filters">
          ${subthemes.map((theme) => `
            <button type="button"
                    class="ctox-chip${activeTag === theme.id ? ' is-active' : ''}"
                    data-action="source-tag-filter"
                    data-tag-id="${theme.id}"
                    aria-pressed="${activeTag === theme.id}">
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
                ${escapeHtml(sourcesEmptyText())}
              </div>
            `}
          </div>
        ` : renderSourcesList(filtered)}
      </div>
    </div>
  `;
}

function filteredSources(sourceModels = state.sourceModels) {
  const activeTag = state.sourceActiveTag || 'all';
  const searchTerm = (state.sourceSearchTerm || '').trim().toLowerCase();

  return sourceModels.filter((source) => {
    if (activeTag !== 'all') {
      if (getSearchCluster(source) !== activeTag) return false;
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

function refreshSourcesWorkbenchInPlace() {
  const center = pane('center');
  if (!center || !['sources', 'candidates'].includes(state.activeTab)) return;
  center.querySelectorAll('[data-action="source-tag-filter"]').forEach((button) => {
    const active = button.dataset.tagId === (state.sourceActiveTag || 'all');
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  const host = center.querySelector('.research-table-host');
  if (!host) return;
  const scrollTop = host.scrollTop;
  const scrollLeft = host.scrollLeft;
  host.innerHTML = renderActiveTable(selectedTask());
  host.scrollTop = scrollTop;
  host.scrollLeft = scrollLeft;
}

function refreshWorkbenchForActiveTab() {
  const center = pane('center');
  const host = center?.querySelector('.research-table-host');
  const tabs = center?.querySelector('.research-tabs-container');
  if (!host || !tabs) {
    renderCenter();
    return;
  }
  center.querySelectorAll('[data-action="tab"]').forEach((button) => {
    const active = button.dataset.tab === state.activeTab;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-selected', String(active));
  });
  const previousToggle = tabs.querySelector('.research-view-toggle');
  if (state.activeTab === 'sources') {
    const toggle = document.createElement('div');
    toggle.className = 'ctox-pane-tabs research-view-toggle';
    toggle.innerHTML = sourcesViewToggleButton();
    previousToggle?.replaceWith(toggle);
    if (!previousToggle) tabs.append(toggle);
  } else {
    previousToggle?.remove();
  }
  const scrollTop = host.scrollTop;
  const scrollLeft = host.scrollLeft;
  host.innerHTML = renderActiveTable(selectedTask());
  host.scrollTop = scrollTop;
  host.scrollLeft = scrollLeft;
}

function refreshMeasurementWorkbenchInPlace() {
  const host = pane('center')?.querySelector('.research-table-host');
  if (!host || state.activeTab !== 'measurements') return;
  const scrollTop = host.scrollTop;
  const scrollLeft = host.scrollLeft;
  host.innerHTML = renderMeasurementsTable();
  host.scrollTop = scrollTop;
  host.scrollLeft = scrollLeft;
}

function selectSourceFromUi(sourceId) {
  const nextId = String(sourceId || '');
  if (!nextId) return;
  state.selectedSourceId = nextId;

  for (const root of [pane('left'), pane('center')]) {
    root?.querySelectorAll('[data-source-id]').forEach((node) => {
      const selected = node.dataset.sourceId === nextId;
      node.classList.toggle('is-selected', selected);
      node.setAttribute('aria-selected', String(selected));
    });
  }

  const centerMatch = [...(pane('center')?.querySelectorAll('[data-source-id]') || [])]
    .find((node) => node.dataset.sourceId === nextId && node.closest('.research-table-host'));
  centerMatch?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });

  renderRight();
  pane('right')?.querySelector('[data-selected-source-section]')?.scrollIntoView?.({
    block: 'start',
    inline: 'nearest',
  });
}

function capturePaneScroll(root) {
  if (!root) return [];
  const selectors = [
    '.research-left-scroll',
    '.research-right-scroll',
    '.research-center-body',
    '.research-table-host',
    '.research-sources-shards-scroll',
  ];
  return selectors.flatMap((selector) => [...root.querySelectorAll(selector)].map((node, index) => ({
    selector,
    index,
    top: node.scrollTop,
    left: node.scrollLeft,
  })));
}

function restorePaneScroll(root, entries = []) {
  for (const entry of entries) {
    const node = root?.querySelectorAll(entry.selector)?.[entry.index];
    if (!node) continue;
    node.scrollTop = entry.top;
    node.scrollLeft = entry.left;
  }
}

/* Kartenansicht (Betreiber-Direktive 31.08.2026): drei Zeilen je Quelle —
   fetter Titel, eine Meta-Zeile mit Klasse, Belegstatus und Score, darunter
   eine Zeile mit dem wichtigsten Detailfeld. Der vollstaendige Datensatz
   (Nutzen, Luecke, Tags) bleibt der Quellen-Schublade vorbehalten. */
function renderSourceCard(source) {
  const isSelected = source.id === state.selectedSourceId;
  const kind = source.sourceClass || 'Quelle';
  const fields = sourceDataSummary(source);
  const canonicalUrl = firstString(source.row, ['canonical_url']);
  const meta = [
    kind,
    source.evidenceStatusLabel,
    `${state.t('scoreLabel', 'Score')} ${source.evidenceEligible ? formatPortfolioScore(source.score) : '—'}`,
  ].filter(Boolean);

  return `
    <div class="research-source-card${isSelected ? ' is-selected' : ''}"
         data-action="select-source"
         data-source-id="${escapeHtml(source.id)}"
         data-evidence-status="${escapeHtml(source.evidenceStatus)}">
      <div class="research-source-card-top">
        <h3 class="research-source-card-title">${escapeHtml(source.title)}</h3>
        <span class="research-source-card-badge ${source.grade.toLowerCase()}">
          ${escapeHtml(gradeFullText(source.grade))}
        </span>
      </div>
      <div class="research-source-card-meta ${source.evidenceEligible ? 'is-verified' : 'is-discovery'}">
        ${meta.map((part) => `<span>${escapeHtml(part)}</span>`).join('<span class="research-source-card-dot" aria-hidden="true">·</span>')}
        ${source.evidenceEligible && canonicalUrl ? `<a href="${escapeHtml(canonicalUrl)}"
             class="research-source-card-open"
             target="_blank"
             rel="noreferrer"
             onclick="event.stopPropagation();">${escapeHtml(state.t('openLabel', 'Öffnen'))}</a>` : ''}
      </div>
      ${fields ? `<div class="research-source-card-detail" title="${escapeHtml(fields)}">${escapeHtml(fields)}</div>` : ''}
    </div>
  `;
}

function sourceDataSummary(source) {
  const row = source?.row || {};
  const direct = firstString(row, ['data_fields', 'fields', 'measurement_fields', 'summary', 'abstract']);
  if (direct) return direct;
  const bibliographic = [
    firstString(row, ['authors_or_institution', 'authors', 'institution', 'publisher']),
    firstString(row, ['publication_year', 'year']),
    firstString(row, ['source_type', 'content_type']),
  ].filter(Boolean);
  const type = firstString(row, ['source_type', 'content_type']) || source?.sourceClass || 'Quelle';
  return bibliographic.length
    ? bibliographic.join(' · ')
    : `${type}; Originalinhalt und bibliografische Metadaten verfügbar.`;
}

function sourceTags(source) {
  const raw = source?.row?.tags ?? source?.row?.source_tags ?? source?.sourceClass ?? '';
  const values = Array.isArray(raw)
    ? raw
    : typeof raw === 'string' && raw.trim().startsWith('[')
      ? (() => { try { return JSON.parse(raw); } catch { return raw.split(/[,;|]/); } })()
      : String(raw).split(/[,;|]/);
  const taxonomy = domainTaxonomy(selectedTask());
  const allowed = new Set(taxonomy.clusters.map((cluster) => cluster.id));
  const aliases = new Map([
    ['autonomous agents', 'agent'], ['agent orchestration', 'agent'], ['agentic workflow', 'agent'],
    ['enterprise readiness', 'enterprise'], ['trust compliance', 'enterprise'], ['customer proof', 'market'],
    ['integration api', 'integration'], ['research quality', 'research-quality'],
    ['rotor load', 'rotorload'], ['wind tunnel', 'rotorload'], ['flight log', 'flightlog'],
  ]);
  return [...new Set(values.map((value) => String(value).trim().toLowerCase())
    .map((value) => aliases.get(value) || value.replace(/\s+/g, '-'))
    .filter((value) => value && value !== 'source' && allowed.has(value)))];
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
  const directRows = filterMeasurementRowsForEvidence(state.measurementRows, state.sourceModels);
  const derivedRows = filterMeasurementRowsForEvidence(state.derivedMeasurementRows, state.sourceModels);
  const mode = state.measurementMode === 'direct' ? 'direct' : 'derived';
  const rows = mode === 'direct' ? directRows : derivedRows;
  return `
    <div class="research-measurement-mode ctox-pane-tabs" role="tablist" aria-label="Messdatenart">
      <button type="button" class="ctox-pane-tab${mode === 'direct' ? ' is-active' : ''}" data-action="measurement-mode" data-measurement-mode="direct" role="tab" aria-selected="${mode === 'direct'}">
        Direkte Messwerte <span>${countText(directRows.length)}</span>
      </button>
      <button type="button" class="ctox-pane-tab${mode === 'derived' ? ' is-active' : ''}" data-action="measurement-mode" data-measurement-mode="derived" role="tab" aria-selected="${mode === 'derived'}">
        Abgeleitete Kräfte &amp; Momente <span>${countText(derivedRows.length)}</span>
      </button>
    </div>
    <p class="research-measurement-note">${mode === 'direct'
      ? 'Direkt publizierte dimensionslose UIUC-Koeffizienten. Kraft und Moment werden hier bewusst nicht als direkt gemessen ausgegeben.'
      : 'Aus CT/CP mit dokumentierter Luftdichte und Propellergeometrie abgeleitet. Diese Werte sind keine direkt gemessenen Lagerkräfte.'}</p>
    ${mode === 'direct' ? renderDirectMeasurements(rows) : renderDerivedMeasurements(rows)}
  `;
}

function renderDirectMeasurements(rows) {
  return `
    <table class="ctox-table" style="table-layout: fixed; width: 100%;">
      <colgroup>
        <col style="width: 17%;" /><col style="width: 12%;" /><col style="width: 12%;" />
        <col style="width: 12%;" /><col style="width: 10%;" /><col style="width: 10%;" />
        <col style="width: 10%;" /><col style="width: 17%;" />
      </colgroup>
      <thead>
        <tr>
          ${measurementHeader('Quelle', 'Quell-ID der Messreihe oder des extrahierten Datensatzes.')}
          ${measurementHeader('Propeller', 'Originale Propellerangabe als Durchmesser x Steigung. 9x5 bedeutet 9 Zoll Durchmesser und 5 Zoll Steigung.')}
          ${measurementHeader('Durchmesser (mm)', 'Propeller-Durchmesser metrisch in Millimetern, aus Angaben wie 9x5 separat extrahiert.', true)}
          ${measurementHeader('Steigung (mm)', 'Propeller-Steigung metrisch in Millimetern, aus Angaben wie 9x5 separat extrahiert.', true)}
          ${measurementHeader('RPM', 'Drehzahl in Umdrehungen pro Minute, ohne Tausendertrennzeichen formatiert.', true)}
          ${measurementHeader('CT', 'Direkt publizierter dimensionsloser Schubbeiwert.', true)}
          ${measurementHeader('CP', 'Direkt publizierter dimensionsloser Leistungsbeiwert.', true)}
          ${measurementHeader('Methode', 'Konfidenz oder Ableitungsverfahren der Messzeile.')}
        </tr>
      </thead>
      <tbody>
        ${rows.slice(0, 120).map((row) => `
          <tr>
            <td>${escapeHtml(row.source_id || '')}</td>
            <td>${escapeHtml(propellerSize(row))}</td>
            <td class="is-num">${formatMeasurementNumber(metricPropellerLength(row, 'prop_diameter'))}</td>
            <td class="is-num">${formatMeasurementNumber(metricPropellerLength(row, 'prop_pitch'))}</td>
            <td class="is-num">${formatMeasurementNumber(row.rpm, 0)}</td>
            <td class="is-num">${formatMeasurementNumber(row.thrust_coefficient_CT)}</td>
            <td class="is-num">${formatMeasurementNumber(row.power_coefficient_CP)}</td>
            <td>${escapeHtml(firstString(row, ['confidence', 'derivation_method']).slice(0, 90))}</td>
          </tr>
        `).join('') || `<tr><td colspan="8">${escapeHtml(state.t('noMeasurements', 'Keine verifizierten Messpunkte vorhanden.'))}</td></tr>`}
      </tbody>
    </table>
  `;
}

function renderDerivedMeasurements(rows) {
  return `
    <table class="ctox-table research-derived-measurements" style="table-layout: fixed; width: 100%;">
      <colgroup>
        <col style="width: 15%;" /><col style="width: 11%;" /><col style="width: 11%;" />
        <col style="width: 11%;" /><col style="width: 10%;" /><col style="width: 12%;" />
        <col style="width: 12%;" /><col style="width: 10%;" /><col style="width: 8%;" />
      </colgroup>
      <thead><tr>
        ${measurementHeader('Quelle', 'Quell-ID der zugrunde liegenden Messreihe.')}
        ${measurementHeader('Propeller', 'Originale Propellerangabe Durchmesser x Steigung.')}
        ${measurementHeader('Durchmesser (mm)', 'Metrischer Propellerdurchmesser.', true)}
        ${measurementHeader('Steigung (mm)', 'Metrische Propellersteigung.', true)}
        ${measurementHeader('RPM', 'Eingangs-Drehzahl.', true)}
        ${measurementHeader('Schub/Force (N)', 'Aus CT, Luftdichte, Drehzahl und Durchmesser abgeleiteter Schub.', true)}
        ${measurementHeader('Moment/Torque (N m)', 'Aus CP und Wellenleistung abgeleitetes Drehmoment.', true)}
        ${measurementHeader('Leistung (W)', 'Aus CP abgeleitete Wellenleistung.', true)}
        ${measurementHeader('ρ (kg/m³)', 'Für die Ableitung verwendete Luftdichte.', true)}
      </tr></thead>
      <tbody>
        ${rows.slice(0, 120).map((row) => `
          <tr>
            <td>${escapeHtml(row.source_id || '')}</td>
            <td>${escapeHtml(firstString(row, ['propeller_size_original', 'propeller_size']))}</td>
            <td class="is-num">${formatMeasurementNumber(metricPropellerLength(row, 'prop_diameter'))}</td>
            <td class="is-num">${formatMeasurementNumber(metricPropellerLength(row, 'prop_pitch'))}</td>
            <td class="is-num">${formatMeasurementNumber(row.rpm_input ?? row.rpm, 0)}</td>
            <td class="is-num">${formatMeasurementNumber(row.thrust_N_derived ?? row.thrust_N)}</td>
            <td class="is-num">${formatMeasurementNumber(row.torque_Nm_derived ?? row.torque_Nm)}</td>
            <td class="is-num">${formatMeasurementNumber(row.shaft_power_W_derived ?? row.shaft_power_W)}</td>
            <td class="is-num">${formatMeasurementNumber(row.air_density_kg_m3_input ?? row.air_density_kg_m3)}</td>
          </tr>
        `).join('') || `<tr><td colspan="9">Keine verifizierten abgeleiteten Kraft-/Momentzeilen vorhanden.</td></tr>`}
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
  const explicit = firstString(row, ['propeller_size_original', 'propeller_size', 'prop_size', 'prop']);
  if (explicit) return explicit.replace(/\s*[xX×]\s*/g, ' x ');
  const diameter = formatMeasurementNumber(row.prop_diameter_in);
  const pitch = formatMeasurementNumber(row.prop_pitch_in);
  return [diameter, pitch].filter(isPresent).join(' x ');
}

function metricPropellerLength(row, stem) {
  const metric = optionalNumberValue(row[`${stem}_mm`]);
  if (metric !== null) return metric;
  const inches = optionalNumberValue(row[`${stem}_in`]);
  return inches === null ? '' : inches * 25.4;
}

function tangentialEquivalentForce(row) {
  const explicit = optionalNumberValue(row.tangential_equivalent_force_N);
  return explicit === null ? '' : explicit;
}

/* Knowledge = the consolidated engineering claims of the base (table `claims`), each with its knowledge
   book, statement type, confidence, contributing sources and the verbatim evidence behind it. Bases without
   a claims table fall back to the claim_support evidence rows so older domains keep working. */
function clipText(value, max) {
  const text = String(value || '');
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

const CLAIM_TYPE_LABELS = Object.freeze({
  direct_measurement: 'Messung',
  normative: 'Norm/Hersteller',
  analytical: 'Analyse/Modell',
  observation: 'Beobachtung',
  assumption: 'Annahme',
});

function knowledgeClaims() {
  if (state.claimRows.length) {
    return state.claimRows.map((row) => ({
      id: firstString(row, ['claim_id', 'id']),
      text: firstString(row, ['claim_text', 'claim', 'statement']),
      type: firstString(row, ['statement_type']),
      confidence: firstString(row, ['confidence']),
      book: firstString(row, ['knowledge_book', 'topic']),
      limitations: firstString(row, ['limitations']),
      sources: String(firstString(row, ['source_id']) || '').split(';').map((part) => part.trim()).filter(Boolean),
      evidenceId: firstString(row, ['evidence_id']),
    })).filter((claim) => claim.id && claim.text);
  }
  return state.evidenceRows
    .filter((row) => firstString(row, ['evidence_kind']) === 'claim_support')
    .map((row) => ({
      id: firstString(row, ['claim_id', 'evidence_id']),
      text: firstString(row, ['fact_value', 'exact_quote_or_value', 'quote']),
      type: firstString(row, ['statement_type']),
      confidence: firstString(row, ['confidence']),
      book: '',
      limitations: firstString(row, ['limitations']),
      sources: [firstString(row, ['source_id'])].filter(Boolean),
      evidenceId: firstString(row, ['evidence_id']),
    })).filter((claim) => claim.id && claim.text);
}

function claimEvidence(claim) {
  return state.evidenceRows.filter((row) => firstString(row, ['evidence_kind']) === 'claim_support'
    && firstString(row, ['claim_id']) === claim.id);
}

function knowledgeBooks(claims) {
  const counts = new Map();
  for (const claim of claims) {
    if (!claim.book) continue;
    counts.set(claim.book, (counts.get(claim.book) || 0) + 1);
  }
  return [...counts.entries()].sort((a, b) => b[1] - a[1]);
}

function renderKnowledgeTables(task) {
  const all = knowledgeClaims();
  if (!all.length) {
    const base = knowledgeBaseForTask(task);
    const tables = base?.tables || [];
    return `<div class="research-knowledge-list">${renderDataQualityNotices()}${renderListOrState(tables, collectionReadiness('knowledge_tables'), {
      renderRows: (rows) => rows.map((table) => `
        <button type="button" data-action="open-knowledge" data-table-id="${escapeHtml(table.id)}">
          <strong>${escapeHtml(table.title || table.table_key)}</strong>
          <span>${escapeHtml(table.table_key)} · ${Number(table.row_count || 0).toLocaleString(state.lang === 'de' ? 'de-DE' : 'en-US')} ${escapeHtml(state.t('rows', 'rows'))}</span>
        </button>`).join(''),
      empty: state.t('noKnowledgeClaims', 'Noch keine belegten Aussagen in dieser Knowledge Base.'),
      syncing: state.t('syncingKnowledgeTables', 'Knowledge-Tabellen werden synchronisiert.'),
    })}</div>`;
  }
  const books = knowledgeBooks(all);
  const multi = all.filter((claim) => claim.sources.length > 1).length;
  const types = new Map();
  for (const claim of all) types.set(claim.type, (types.get(claim.type) || 0) + 1);
  const book = state.knowledgeTopic;
  const type = state.knowledgeType;
  const rows = all.filter((claim) => (!book || claim.book === book)
    && (type === 'all' || (type === 'multi' ? claim.sources.length > 1 : claim.type === type)));
  const shown = rows.slice(0, ROW_LIMIT);
  return `
    ${renderDataQualityNotices()}
    <div class="research-claim-filters ctox-pane-tabs" role="tablist" aria-label="Knowledge-Filter">
      <button type="button" class="ctox-pane-tab${!book ? ' is-active' : ''}" data-action="knowledge-book" data-knowledge-book="" role="tab" aria-selected="${!book}">Alle Themen <span>${all.length}</span></button>
      ${books.map(([name, count]) => `<button type="button" class="ctox-pane-tab${book === name ? ' is-active' : ''}" data-action="knowledge-book" data-knowledge-book="${escapeHtml(name)}" role="tab" aria-selected="${book === name}">${escapeHtml(name)} <span>${count}</span></button>`).join('')}
    </div>
    <div class="research-claim-filters ctox-pane-tabs" role="tablist" aria-label="Aussagenart">
      <button type="button" class="ctox-pane-tab${type === 'all' ? ' is-active' : ''}" data-action="knowledge-type" data-knowledge-type="all" role="tab" aria-selected="${type === 'all'}">Alle <span>${all.length}</span></button>
      <button type="button" class="ctox-pane-tab${type === 'multi' ? ' is-active' : ''}" data-action="knowledge-type" data-knowledge-type="multi" role="tab" aria-selected="${type === 'multi'}">Mehrere Quellen <span>${multi}</span></button>
      ${[...types.entries()].sort((a, b) => b[1] - a[1]).map(([name, count]) => `<button type="button" class="ctox-pane-tab${type === name ? ' is-active' : ''}" data-action="knowledge-type" data-knowledge-type="${escapeHtml(name)}" role="tab" aria-selected="${type === name}">${escapeHtml(CLAIM_TYPE_LABELS[name] || name || '—')} <span>${count}</span></button>`).join('')}
    </div>
    <div class="research-claim-list">
      ${shown.map((claim) => {
        const evidence = claimEvidence(claim);
        return `
        <article class="research-claim">
          <header>
            <b>${escapeHtml(claim.id)}</b>
            <span>${escapeHtml(CLAIM_TYPE_LABELS[claim.type] || claim.type || '—')}</span>
            <span>${escapeHtml(state.t('confidence', 'Konfidenz'))} ${escapeHtml(claim.confidence || '—')}</span>
            ${!book && claim.book ? `<span>${escapeHtml(claim.book)}</span>` : ''}
            <span>${claim.sources.length} ${escapeHtml(claim.sources.length === 1 ? 'Quelle' : 'Quellen')}: ${claim.sources.map((id) => `<button type="button" class="research-claim-source" data-action="select-source" data-source-id="${escapeHtml(id)}">${escapeHtml(id)}</button>`).join(' ')}</span>
          </header>
          <p>${escapeHtml(claim.text)}</p>
          ${claim.limitations ? `<p class="research-claim-limits">${escapeHtml(claim.limitations)}</p>` : ''}
          ${evidence.length ? `<details><summary>${evidence.length} ${escapeHtml(evidence.length === 1 ? 'Beleg' : 'Belege')}</summary>${evidence.map((row) => `
            <div class="research-claim-evidence">
              <b>${escapeHtml(firstString(row, ['source_id']) || '')}</b>
              <span>${escapeHtml(firstString(row, ['source_locator', 'table_file_column_row']) || '')}</span>
              <q>${escapeHtml(clipText(firstString(row, ['quote', 'exact_quote_or_value']) || '', 600))}</q>
            </div>`).join('')}</details>` : ''}
        </article>`;
      }).join('')}
      ${rows.length > shown.length ? `<p class="research-claim-more">… ${rows.length - shown.length} weitere Aussagen.</p>` : ''}
      ${rows.length ? '' : `<div class="research-empty">${escapeHtml(state.t('noKnowledgeClaims', 'Keine Aussagen für diesen Filter.'))}</div>`}
    </div>
  `;
}

function renderKnowledgeTableRowStates() {
  const entries = Object.values(state.knowledgeTableRowStates || {})
    .filter((entry) => entry?.phase === 'loading' || entry?.phase === 'error');
  if (!entries.length) return '';
  return `<div class="research-table-row-states">${entries.map((entry) => {
    const label = entry.title || entry.tableKey || entry.tableId;
    if (entry.phase === 'loading') {
      return `<div class="research-table-row-state" role="status" data-table-id="${escapeHtml(entry.tableId)}" data-table-key="${escapeHtml(entry.tableKey)}" data-row-state="loading"><span class="research-spinner" aria-hidden="true"></span><span>${escapeHtml(label)}: ${escapeHtml(state.t('rowsLoading', 'Zeilen werden geladen …'))}</span></div>`;
    }
    const retry = entry.retryable
      ? `<button type="button" class="ctox-button" data-action="retry-knowledge-rows" data-table-id="${escapeHtml(entry.tableId)}">${escapeHtml(state.t('rowsRetry', 'Erneut versuchen'))}</button>`
      : '';
    return `<div class="research-table-row-state" role="alert" data-table-id="${escapeHtml(entry.tableId)}" data-table-key="${escapeHtml(entry.tableKey)}" data-row-state="error"><strong>${escapeHtml(label)}</strong><span>${escapeHtml(entry.message || state.t('rowsLoadFailed', 'Zeilen konnten nicht geladen werden'))}</span>${retry}</div>`;
  }).join('')}</div>`;
}

function renderDataQualityNotices() {
  const notices = [
    ...state.rowLimitWarnings.map((warning) => `Anzeige auf ${ROW_LIMIT.toLocaleString(state.lang === 'de' ? 'de-DE' : 'en-US')} Zeilen begrenzt (${warning.sourceRowCount.toLocaleString(state.lang === 'de' ? 'de-DE' : 'en-US')} vorhanden).`),
    ...state.chunkDiagnostics.map((diagnostic) => `Tabelle ${diagnostic.tableId || 'unbekannt'} wurde wegen unvollständiger Chunk-Metadaten nicht geladen: ${diagnostic.reason}`),
  ];
  if (!notices.length) return '';
  return `<div class="research-data-quality-notices" role="status">${notices.map((notice) => `<span>${escapeHtml(notice)}</span>`).join('')}</div>`;
}

function renderRight() {
  const root = pane('right');
  if (!root) return;
  const scrollState = capturePaneScroll(root);
  const task = selectedTask();
  const source = selectedSource();
  const runInfo = researchRunInfo(task);
  const canBuildKnowledge = canBuildKnowledgeFromResearch(task);
  root.innerHTML = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${escapeHtml(state.t('context', 'Context'))}</span>
          <!-- Statischer Spaltentitel wie im Skelett: der Aufgabentitel steht
               bereits im Mittelkopf; die Dublette ellipsierte hier nur frueh
               (Betreiber-Nachtrag 31.08.2026). -->
          <h2 class="ctox-pane-title">Research</h2>
        </div>
        <!-- Dominante Flussaktion der Kontextspalte; ohne waehlbare Aufgabe
             ehrlich deaktiviert (gleicher Grund wie am Textknopf unten). -->
        <div class="ctox-pane-actions">
          <button type="button"
                  class="ctox-pane-icon is-primary"
                  data-action="build-knowledge"
                  ${canBuildKnowledge ? '' : 'disabled aria-disabled="true"'}
                  title="${escapeHtml(canBuildKnowledge ? (task?.payload?.knowledge_refresh?.command_id ? state.t('updateKnowledge', 'Knowledge aktualisieren') : state.t('buildKnowledge', 'Knowledge aufbauen')) : knowledgeUnavailableReason())}"
                  aria-label="${escapeHtml(task?.payload?.knowledge_refresh?.command_id ? state.t('updateKnowledge', 'Knowledge aktualisieren') : state.t('buildKnowledge', 'Knowledge aufbauen'))}">
            ${iconSvg('book')}
          </button>
        </div>
      </div>
    </header>
    <div class="research-right-scroll">
      <section class="research-context-block">
        <span class="ctox-pane-kicker">Knowledge Base</span>
        <strong${task?.knowledge_domain ? '' : ' class="research-context-empty"'}>${escapeHtml(task?.knowledge_domain || state.t('noDomain', 'Keine Domain'))}</strong>
        <p>${escapeHtml(state.t('defaultTaskDesc', 'Research, Knowledge und Berichte teilen eine gemeinsame, nachvollziehbare Lineage.'))}</p>
        ${task ? `<div class="research-context-actions">
          <button type="button" class="ctox-button" data-action="build-knowledge" ${canBuildKnowledge ? '' : 'disabled aria-disabled="true"'} title="${escapeHtml(canBuildKnowledge ? '' : knowledgeUnavailableReason())}">${escapeHtml(task.payload?.knowledge_refresh?.command_id ? state.t('updateKnowledge', 'Knowledge aktualisieren') : state.t('buildKnowledge', 'Knowledge aufbauen'))}</button>
          <button type="button" class="ctox-button" data-action="edit-task">${escapeHtml(state.t('editScoring', 'Methode & Scoring'))}</button>
        </div>` : ''}
      </section>
      <section class="research-metric-grid">
        <div><strong>${countText(state.candidateModels.length)}</strong><span>${escapeHtml(state.t('candidates', 'Candidates'))}</span></div>
        <div><strong>${countText(evidenceRankedSources().length)}</strong><span>${escapeHtml(state.t('sources', 'Sources'))}</span></div>
        <div><strong>${countText(filterMeasurementRowsForEvidence(state.measurementRows, state.sourceModels).length)}</strong><span>${escapeHtml(state.t('measurements', 'Measurements'))}</span></div>
        <div><strong>${countText(researchReportsForTask(task).length)}</strong><span>${escapeHtml(state.t('reports', 'Fachberichte'))}</span></div>
      </section>
      ${renderRunPanel(runInfo)}
      <section class="research-context-block" data-selected-source-section>
        <span class="ctox-pane-kicker">${escapeHtml(state.t('selectedSource', 'Selected Source'))}</span>
        ${source ? `
          <strong>${escapeHtml(source.title)}</strong>
          <p>${escapeHtml(source.note || state.t('noSummaryAvailable', 'Keine Zusammenfassung vorhanden.'))}</p>
          <div class="research-source-card-status ${source.evidenceEligible ? 'is-verified' : 'is-discovery'}" data-evidence-status="${escapeHtml(source.evidenceStatus)}">${escapeHtml(source.evidenceStatusLabel)}</div>
          <dl class="ctox-fields">
            <dt>${escapeHtml(state.t('quality', 'Qualität'))}</dt><dd>${escapeHtml(source.grade)} · ${escapeHtml(formatPortfolioScore(source.score))}</dd>
            <dt>${escapeHtml(state.t('sourceType', 'Quellentyp'))}</dt><dd>${escapeHtml(source.sourceClass)}</dd>
          </dl>
          <div class="research-context-actions">
            <button type="button" class="ctox-button" data-action="source-detail" data-source-id="${escapeHtml(source.id)}">${escapeHtml(state.t('details', 'Details'))}</button>
            ${source.evidenceEligible && source.canonicalUrl ? `<a class="ctox-button" href="${escapeHtml(source.canonicalUrl)}" target="_blank" rel="noreferrer">${escapeHtml(state.t('openOriginal', 'Original öffnen'))}</a>` : ''}
          </div>
        ` : `<p>${escapeHtml(state.t('selectSourcePrompt', 'Wähle eine Quelle aus.'))}</p>`}
      </section>
    </div>
  `;
  restorePaneScroll(root, scrollState);
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
          <dt>${escapeHtml(state.t('thread', 'Thread'))}</dt><dd title="${escapeHtml(runInfo.threadKey || '-')}">${escapeHtml(runInfo.threadKey || '-')}</dd>
        </dl>
        <div class="research-run-actions">
          <button type="button" class="ctox-button" data-action="focus-ctox-run" data-command-id="${escapeHtml(runInfo.commandId)}" data-task-queue-id="${escapeHtml(runInfo.taskQueueId)}" data-task-status="${escapeHtml(runInfo.status)}" ${runInfo.taskQueueId || runInfo.commandId ? '' : 'disabled'}>${escapeHtml(state.t('viewInCtox', 'In CTOX ansehen'))}</button>
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
  const verifiedMeasurementCount = filterMeasurementRowsForEvidence(state.measurementRows, state.sourceModels).length;
  if (verifiedMeasurementCount) {
    notes.push({ kind: 'opportunity', title: state.t('decisionNoteQuant', 'Quantitative evidence available'), body: state.t('decisionNoteQuantBody', `${verifiedMeasurementCount} Messpunkte können in die aktiven Scoring-Kriterien einfließen.`, verifiedMeasurementCount) });
  }
  if (!top) {
    notes.push({ kind: 'risk', title: state.t('decisionNoteGate', 'Evidence gate active'), body: state.t('decisionNoteGateBody', 'Discovery-Kandidaten bleiben sichtbar, bis Verifizierung, Snapshot und HTTP-Erfolg vollständig vorliegen.') });
  }
  if (source && Number.isFinite(Number(source.dimensions.reuse_readiness)) && source.dimensions.reuse_readiness < 60) {
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
  return validateSelectedResearchTask(task, state.knowledgeBases).valid
    && canWriteResearchState()
    && !researchRunInfo(task).isActive;
}

function canBuildKnowledgeFromResearch(task = selectedTask()) {
  const base = task ? knowledgeBaseForTask(task) : null;
  const latestRun = task ? latestEvidenceRunForTask(task.id, state.runs) : null;
  return Boolean(task?.id
    && evidenceRankedSources().length
    && knowledgeVersionContext(base, latestRun).available
    && canWriteResearchState());
}

function knowledgeUnavailableReason() {
  if (!evidenceRankedSources().length) return state.t('knowledgeRequiresVerifiedSources', 'Knowledge ist ohne verifizierte Quellen nicht verfügbar.');
  const task = selectedTask();
  const version = knowledgeVersionContext(task ? knowledgeBaseForTask(task) : null, task ? latestEvidenceRunForTask(task.id, state.runs) : null);
  if (!version.available) return `${state.t('knowledgeVersionUnavailable', 'Knowledge ist ohne eine immutable Version nicht verfügbar')}: ${version.reason}`;
  if (!canWriteResearchState()) return researchWriteDeniedMessage();
  return '';
}

function validateSelectedResearchTask(task, knowledgeBases = []) {
  if (!task?.id) return { valid: false, message: state.t('selectTaskFirst', 'Wähle zuerst eine Research-Aufgabe.') };
  if (!String(task.title || '').trim()) return { valid: false, message: state.t('missingTaskTitle', 'Die Research-Aufgabe hat keinen Titel.') };
  const domain = String(task.knowledge_domain || '').trim();
  if (!domain) return { valid: false, message: state.t('missingDomain', 'Die Research-Aufgabe hat keine Knowledge Domain.') };
  if (!knowledgeBases.some((base) => base.domain === domain)) {
    // A declared-new domain has no local tables until the first server-side
    // writeback creates them (knowledge_tables is pull-only, the browser never
    // seeds it). Blocking those runs makes new-topic research impossible - it
    // stranded both SKF test dashboards. Every other task keeps the guard.
    if (task?.payload?.new_domain !== true) {
      return { valid: false, message: state.t('domainNotLoaded', 'Die Knowledge Domain ist lokal nicht geladen.') };
    }
  }
  return { valid: true, message: '' };
}

function runResearchHint(task, runInfo) {
  const validation = validateSelectedResearchTask(task, state.knowledgeBases);
  if (!validation.valid) return validation.message;
  if (runInfo.isActive) {
    return state.t('researchAlreadyActive', 'Research läuft bereits. Öffne den aktiven CTOX Task.');
  }
  return `${state.t('runHint', 'Systematic Research für dieses Dashboard')} ${runInfo.hasRun ? state.t('researchFortsetzen', 'fortsetzen') : state.t('researchStarten', 'starten')}`;
}

function runDisabledReason(task) {
  const validation = validateSelectedResearchTask(task, state.knowledgeBases);
  if (!validation.valid) return validation.message;
  if (!canWriteResearchState()) return researchWriteDeniedMessage();
  if (researchRunInfo(task).isActive) {
    return state.t('researchAlreadyActive', 'Research läuft bereits. Öffne den aktiven CTOX Task.');
  }
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
  // Modul-Overlay: der Dialog gehoert in den Modul-Host, nicht auf
  // document.body — `.research-module-overlay` haelt ihn im Fenster.
  overlay.className = 'ctox-modal research-task-dialog research-module-overlay';
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
  const onKeydown = (event) => {
    if (event.key !== 'Escape' || !overlay.isConnected) return;
    event.preventDefault();
    close();
  };
  const close = () => {
    window.removeEventListener('keydown', onKeydown);
    overlay.remove();
  };
  window.addEventListener('keydown', onKeydown);
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
  // Beim Bearbeiten ist die Domain gesperrt und muss die Domain der Aufgabe
  // zeigen — auch wenn dafuer (noch) keine Knowledge-Tabelle lokal liegt.
  // Vorher zeigte das gesperrte Feld die erste verfuegbare Fremd-Domain.
  const bases = selectedDomain && !state.knowledgeBases.some((base) => base.domain === selectedDomain)
    ? [{ domain: selectedDomain, title: titleFromDomain(selectedDomain) }, ...state.knowledgeBases]
    : state.knowledgeBases;
  return bases.map((base) => `
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
    candidate_catalog_key: current?.candidate_catalog_key || tableKey(base, ['source_candidates']) || 'source_candidates',
    source_catalog_key: current?.source_catalog_key || tableKey(base, ['source_catalog', 'sources', 'curated_sources']) || 'source_catalog',
    curated_table_key: current?.curated_table_key || tableKey(base, ['evaluation_matrix', 'load_data_library', 'curated_sources', 'source_library']) || 'evaluation_matrix',
    measurements_table_key: current?.measurements_table_key || defaultMeasurementsTableKey(base),
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
  const commandId = `cmd_${crypto.randomUUID()}`;
  const researchRunId = `research_run_${crypto.randomUUID()}`;
  const scoringDimensions = scoringDimensionsForTask(task).filter((axis) => axis.id !== 'portfolio_priority');
  const tableContract = task.payload?.table_contract || RESEARCH_TABLE_CONTRACT;
  const existingTables = new Set((base?.tables || []).map((table) => table.table_key));
  const missingTables = Object.keys(tableContract).filter((key) => !existingTables.has(key));
  const requireMeasuredLoadPoints = task.measurements_table_key === 'measured_load_points'
    || existingTables.has('measured_load_points')
    || Object.hasOwn(tableContract, 'measured_load_points');
  const requireDerivedBearingLoads = existingTables.has('derived_bearing_loads')
    || Object.hasOwn(tableContract, 'derived_bearing_loads');
  const candidateTable = tableForKey(base, task.candidate_catalog_key || 'source_candidates');
  const sourceTable = tableForKey(base, task.source_catalog_key || 'source_catalog');
  const [candidateRows, authoritativeSourceRows] = await Promise.all([
    candidateTable ? fetchTableRows(candidateTable.id) : Promise.resolve([]),
    sourceTable ? fetchTableRows(sourceTable.id) : Promise.resolve([]),
  ]);
  const declaredSourceRows = firstPositiveNumber(sourceTable, [
    'row_count',
    'rowCount',
    'total_row_count',
    'totalRows',
    'projected_row_count',
  ]);
  if (sourceTable && declaredSourceRows && authoritativeSourceRows.length < declaredSourceRows) {
    setStatus(`Der Quellenkatalog wird noch synchronisiert (${authoritativeSourceRows.length}/${declaredSourceRows}). Research wurde nicht gestartet.`);
    renderRight();
    return;
  }
  const knowledgeTableRefs = compactKnowledgeTableReferences(base?.tables || []);
  const launchSourceModels = sourceTable
    ? buildSourceModels(task, authoritativeSourceRows, [], state.measurementRows || [])
    : state.sourceModels || [];
  const rawVerifiedSourceModels = launchSourceModels
    .filter((source) => source.evidenceEligible);
  const verifiedSourceModels = uniqueSourceModels(rawVerifiedSourceModels);
  const verifiedSourceCount = boundedVerifiedSourceCount(verifiedSourceModels, sourceTable);
  const verifiedSourceUrls = [...new Set(sourceUrlsFromRows(
    verifiedSourceModels.map((source) => source.row),
  ))];
  const excludedSourceUrls = [...new Set([
    ...sourceUrlsFromRows(candidateRows),
    ...sourceUrlsFromRows(launchSourceModels.map((source) => source.row)),
  ])];
  // An explicitly configured target is a decision, not a projection artefact.
  // The anti-gaming heuristic below cannot tell them apart - it once rejected a
  // deliberate 40 purely because the run happened to hold 20 verified sources
  // at that moment - so an explicit flag bypasses the guessing entirely.
  const targetVerifiedSources = task?.payload?.target_verified_sources_explicit === true
    ? Math.max(20, Number(task?.payload?.target_verified_sources || 0) || 100)
    : effectiveTargetVerifiedSources(
      task?.payload?.target_verified_sources,
      rawVerifiedSourceModels.length,
      verifiedSourceModels.length,
      verifiedSourceCount,
    );
  const minimumCandidateSources = Math.max(
    targetVerifiedSources * 2,
    Number(task?.payload?.minimum_candidate_sources || 0),
  );
  // Depth and round counts decide whether a run fits inside one model session.
  // These were hard-wired to the exhaustive profile, so a dashboard configured
  // for a smaller sweep still dispatched a multi-hour run and died against the
  // session ceiling before it could ever validate. Honor the configuration and
  // keep the exhaustive profile as the default for unconfigured dashboards.
  const discoveryDepth = researchDiscoveryDepth(task?.payload?.discovery_depth);
  const minimumDiscoveryRounds = Math.max(
    2,
    Number(task?.payload?.minimum_discovery_rounds || 0) || 6,
  );
  const minimumScholarlyRounds = Math.max(
    1,
    Number(task?.payload?.minimum_scholarly_rounds || 0) || 2,
  );
  const instruction = [
    `Führe die systematische Recherche für "${task.title}" mit dem System-Skill systematic-research fort.`,
    `Research Task ID: ${task.id}`,
    `Research Run ID: ${researchRunId}`,
    `Research Command ID: ${commandId}`,
    `Requested Discovery Depth: ${discoveryDepth}`,
    `Target Verified Sources: ${targetVerifiedSources}`,
    `Minimum Candidate Sources: ${minimumCandidateSources}`,
    `Minimum Discovery Rounds: ${minimumDiscoveryRounds}`,
    `Minimum Scholarly Rounds: ${minimumScholarlyRounds}`,
    `Knowledge domain: ${task.knowledge_domain}`,
    '',
    task.prompt || defaultPromptForKnowledgeBase(base),
    task.criteria ? `Kriterien:\n${task.criteria}` : null,
    `Scoring-Modell:\n${scoringDimensions.map((axis) => `- ${axis.id}: ${axis.label}; weight=${axis.weight || scoringWeights(scoringDimensions)[axis.id] || 1}`).join('\n')}`,
    `Portfolio axes: x=${normalizedAxisPair(task).x}, y=${normalizedAxisPair(task).y}`,
    '',
    `Bekannte Quellen: ${excludedSourceUrls.length}; davon bereits evidence-eligible: ${verifiedSourceCount}. Die vollständige kanonische Exclude-Liste steht in web_stack_plan.exclude_urls.`,
    'Arbeite iterativ mit den typisierten Web-Stack-Werkzeugen, folge bei wissenschaftlichen Quellen den Referenzen und führe zwei orthogonale Nulltreffer-Runden durch. Behandle Discovery nur als Kandidatenmenge. Evidence, Knowledge, Graph und Reports dürfen ausschließlich aus gelesenen, gesnapshotpten und vom Evidence-Gate zugelassenen Originalquellen entstehen.',
    'Nutze die vom System materialisierte Skill-Anleitung und den serverseitigen Writeback-Vertrag. Erzeuge keine parallelen Tabellen, schreibe nicht direkt in Business-OS-Datenbanken und starte keine Child Agents.',
  ].filter(Boolean).join('\n');
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
    research_run_id: researchRunId,
    research_command_id: commandId,
    knowledge_domain: task.knowledge_domain,
    candidate_catalog_key: task.candidate_catalog_key || 'source_candidates',
    source_catalog_key: task.source_catalog_key,
    curated_table_key: task.curated_table_key,
    measurements_table_key: task.measurements_table_key,
    web_stack_plan: {
      strategy: 'agentic_iterative_systematic_research',
      seed_query: task.prompt || task.title,
      depth: discoveryDepth,
      target_verified_sources: targetVerifiedSources,
      minimum_candidate_sources: minimumCandidateSources,
      minimum_discovery_rounds: minimumDiscoveryRounds,
      minimum_scholarly_rounds: minimumScholarlyRounds,
      exclude_urls: excludedSourceUrls,
      verified_source_urls: verifiedSourceUrls,
      available_rounds: [
        'ctox_scholarly_search',
        'ctox_web_search',
        'ctox_deep_research',
        'ctox_web_read',
      ],
      saturation_rule: 'stop only after two consecutive orthogonal facet or citation rounds add no new eligible source',
    },
    knowledge_contract: {
      domain: task.knowledge_domain,
      tables: tableContract,
      create_missing_tables: missingTables,
      provenance_required: true,
      row_lineage_required: {
        research_run_id: researchRunId,
        research_command_id: commandId,
      },
    },
    graph_contract: semanticGraphContract(),
    scoring_contract: researchScoringContract(scoringDimensions),
    writeback_contract: {
      collections: ['research_runs', 'research_tasks', 'knowledge_tables'],
      dashboard_tables: {
        source_candidates: task.candidate_catalog_key || 'source_candidates',
        source_catalog: task.source_catalog_key || 'source_catalog',
        evaluation_matrix: task.curated_table_key || 'evaluation_matrix',
        evidence_points: 'evidence_points',
        semantic_graph_nodes: 'semantic_graph_nodes',
        semantic_graph_edges: 'semantic_graph_edges',
        ...(requireMeasuredLoadPoints ? { measured_load_points: 'measured_load_points' } : {}),
        ...(requireDerivedBearingLoads ? { derived_bearing_loads: 'derived_bearing_loads' } : {}),
      },
    },
  };
  const clientContext = {
    action: 'research-run-chat',
    module: 'research',
    source_module: 'research',
    inbound_channel: 'business_os.research',
    knowledge_domain: task.knowledge_domain,
    research_run_id: researchRunId,
    research_command_id: commandId,
    knowledge_table_refs: knowledgeTableRefs,
  };
  // The run reaches the harness through the Business Chat, exactly like the
  // Outbound module: the operator sees the full systematic-research prompt,
  // can adjust it, and sends it themselves. Before this the module dispatched
  // straight into the queue while still calling itself `business-chat` in
  // status text and transport - no chat ever opened.
  const openBusinessChat = state.ctx?.openBusinessChat;
  const useChat = typeof openBusinessChat === 'function';
  let dispatched = null;
  if (useChat) {
    openBusinessChat({
      module: 'research',
      source_module: 'research',
      source_title: 'Web Research',
      action: 'context-chat',
      reuseActive: false,
      command_id: commandId,
      command_type: 'research.systematic.run',
      record_id: task.id,
      title,
      command_title: title,
      thread_key: threadKey,
      instruction,
      // `draft` prefills the composer; the operator presses send.
      draft: instruction,
      text: instruction,
      payload,
      client_context: clientContext,
    });
  } else {
    // No chat surface available (embedded/QA hosts): keep the direct path so
    // the run is still dispatchable rather than silently doing nothing.
    dispatched = await state.ctx.commandBus.dispatch({
      id: commandId,
      command_id: commandId,
      module: 'research',
      command_type: 'research.systematic.run',
      record_id: task.id,
      payload,
      client_context: clientContext,
    });
  }
  const result = {
    ...(dispatched || {}),
    ok: true,
    command_id: commandId,
    status: useChat ? 'chat' : (dispatched?.status || 'queued'),
    task_status: useChat ? 'chat' : (dispatched?.task_status || dispatched?.status || 'queued'),
    title,
    thread_key: threadKey,
    transport: useChat ? 'business-chat' : 'command-bus',
  };
  const run = {
    id: researchRunId,
    task_id: task.id,
    status: result.task_status,
    command_id: commandId,
    task_queue_id: '',
    identified_count: state.candidateRows.length + state.sourceRows.length,
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
  // `collecting` means the harness is working. In the chat path nothing runs
  // until the operator sends, so claiming it here would show a dashboard that
  // is busy with a run that was never started.
  if (!useChat) {
    await patchDoc(writableCollection('research_tasks'), task.id, { status: 'collecting', updated_at_ms: now }).catch((error) => {
      console.warn('[research] could not patch task status', error);
    });
  }
  setStatus(useChat
    ? state.t('researchChatOpened', 'Research-Aufgabe im Chat vorbereitet - zum Starten im Chat senden.')
    : state.t('researchChatQueued', 'Research-Aufgabe an CTOX uebergeben.'));
  render();
  // Only a dispatched command has a queue task to focus. In the chat path the
  // queue entry appears when the operator sends, so focusing here would jump
  // to a run that does not exist yet.
  if (!useChat) {
    await focusCtoxRun(
      result.task_id || result.task_queue_id || '',
      commandId,
      result.task_status || result.status || 'queued',
    );
  }
}

function compactKnowledgeTableReferences(tables = []) {
  return (tables || []).slice(0, 100).map((table) => ({
    id: String(table?.id || ''),
    table_key: String(table?.table_key || table?.key || ''),
    domain: String(table?.domain || table?.knowledge_domain || ''),
    row_count: Number(table?.row_count ?? table?.total_rows ?? table?.rows?.length ?? 0),
    knowledge_version_id: String(table?.knowledge_version_id || table?.knowledge_version?.version_id || ''),
  }));
}

function sourceUrlsFromRows(rows = []) {
  return (rows || [])
    .flatMap((row) => [
      firstString(row, ['canonical_url']),
      firstString(row, ['source_url', 'url', 'direct_url', 'doi']),
    ])
    .map((url) => String(url || '').trim())
    .filter((url) => /^https?:\/\//i.test(url));
}

function normalizedSourceIdentityUrl(value) {
  const raw = String(value || '').trim();
  if (!/^https?:\/\//i.test(raw)) return '';
  try {
    const url = new URL(raw);
    url.hash = '';
    url.protocol = url.protocol.toLowerCase();
    url.hostname = url.hostname.toLowerCase();
    if (url.pathname !== '/') url.pathname = url.pathname.replace(/\/+$/, '');
    return url.toString();
  } catch {
    return raw.replace(/#.*$/, '').replace(/\/+$/, '').toLowerCase();
  }
}

function sourceModelIdentities(source) {
  const row = source?.row || {};
  const identities = [];
  for (const value of [
    firstString(row, ['canonical_url']),
    firstString(row, ['source_url', 'url', 'direct_url']),
  ]) {
    const url = normalizedSourceIdentityUrl(value);
    if (url) identities.push(`url:${url}`);
  }
  const doi = firstString(row, ['doi'])
    .replace(/^https?:\/\/(?:dx\.)?doi\.org\//i, '')
    .trim()
    .toLowerCase();
  if (doi) identities.push(`doi:${doi}`);
  const contentHash = firstString(row, ['content_hash', 'snapshot_hash', 'snapshot_sha256'])
    .replace(/^sha256:/i, '')
    .trim()
    .toLowerCase();
  if (/^[0-9a-f]{64}$/.test(contentHash)) identities.push(`sha256:${contentHash}`);
  const id = String(sourceId(row) || source?.id || '').trim();
  if (id) identities.push(`id:${id}`);
  return [...new Set(identities)];
}

function uniqueSourceModels(sourceModels = []) {
  const seen = new Set();
  return (sourceModels || []).filter((source, index) => {
    const identities = sourceModelIdentities(source);
    if (identities.some((identity) => seen.has(identity))) return false;
    for (const identity of identities.length ? identities : [`anonymous:${index}`]) seen.add(identity);
    return true;
  });
}

function boundedVerifiedSourceCount(sourceModels = [], sourceTable = null) {
  const modelCount = sourceModels.length;
  const declaredCount = firstPositiveNumber(sourceTable, [
    'row_count',
    'rowCount',
    'total_row_count',
    'totalRows',
    'projected_row_count',
  ]);
  return declaredCount ? Math.min(modelCount, declaredCount) : modelCount;
}

// Only the three profiles the systematic-research skill understands; anything
// else falls back to the strict default rather than reaching the skill as an
// unknown token.
const RESEARCH_DISCOVERY_DEPTHS = new Set(['standard', 'deep', 'exhaustive']);

function researchDiscoveryDepth(configured) {
  const value = String(configured || '').trim().toLowerCase();
  return RESEARCH_DISCOVERY_DEPTHS.has(value) ? value : 'exhaustive';
}

function effectiveTargetVerifiedSources(
  configuredTarget,
  rawVerifiedCount,
  uniqueVerifiedCount,
  authoritativeVerifiedCount = uniqueVerifiedCount,
) {
  const configured = Number(configuredTarget || 0);
  const verifiedCount = Math.min(
    Number(uniqueVerifiedCount || 0),
    Number(authoritativeVerifiedCount || uniqueVerifiedCount || 0),
  );
  const duplicatedProjectionTarget = rawVerifiedCount > verifiedCount
    && configured === rawVerifiedCount;
  const duplicatedAliasTarget = verifiedCount > 0
    && configured === verifiedCount * 2;
  return Math.max(100, duplicatedProjectionTarget || duplicatedAliasTarget ? 0 : configured);
}

function researchScoringContract(scoringDimensions) {
  return {
    dimensions: scoringDimensions,
    weights: scoringWeights(scoringDimensions),
    total_field: 'weighted_total',
    rule: 'Only score rows passing the UI evidence gate and durable receipt lineage: source_id, verification flags, HTTP 2xx, snapshot_id, snapshot_path, byte-hash-shaped snapshot_hash, canonical_url, evidence_id or claim_id, retrieved_at, allowed url_role/content_scope, evidence_eligible=true, and non-aggregated source_tier. Raw, legacy, metadata-only, off-topic, rejected, empty, or aggregated discovery candidates stay unscored.',
    required_source_fields: ['source_id', 'verification_status', 'transport_verified', 'content_extracted', 'actual_full_text_or_data', 'evidence_relevance_score', 'http_status', 'snapshot_id', 'snapshot_path', 'snapshot_hash', 'canonical_url', 'evidence_id_or_claim_id', 'retrieved_at', 'url_role', 'content_scope', 'evidence_eligible', 'source_tier'],
    required_audits: ['source', 'data', 'claim'],
  };
}

function knowledgeVersionContext(base, latestRun = null) {
  const candidates = [];
  const addCandidate = (value, origin) => {
    if (value === null || value === undefined) return;
    if (typeof value === 'string') {
      const id = value.trim();
      if (id) candidates.push({ id, origin, record: null });
      return;
    }
    if (typeof value !== 'object' || Array.isArray(value)) return;
    const id = firstString(value, ['current_version_id', 'knowledge_version_id', 'version_id', 'id']);
    if (id) candidates.push({ id, origin, record: value });
  };
  const visit = (value, origin, table = false) => {
    if (!value || typeof value !== 'object') return;
    const keys = table
      ? ['knowledge_version_id', 'current_version_id', 'knowledge_version']
      : ['knowledge_version_id', 'current_version_id', 'knowledge_version'];
    for (const key of keys) addCandidate(value[key], `${origin}.${key}`);
    if (value.knowledge && typeof value.knowledge === 'object') visit(value.knowledge, `${origin}.knowledge`);
    if (value.knowledge?.version && typeof value.knowledge.version === 'object') addCandidate(value.knowledge.version, `${origin}.knowledge.version`);
    if (value.versions && Array.isArray(value.versions)) {
      const current = value.versions.find((item) => item?.status === 'current');
      if (current) addCandidate(current, `${origin}.versions[current]`);
    }
  };
  visit(base, 'knowledge_base');
  for (const table of base?.tables || []) visit(table, `knowledge_table:${table.table_key || table.id || 'unknown'}`, true);
  visit(latestRun, 'research_run');
  visit(latestRun?.payload, 'research_run.payload');
  visit(latestRun?.payload?.result, 'research_run.payload.result');

  const ids = [...new Set(candidates.map((candidate) => candidate.id))];
  if (!ids.length) return { available: false, reason: 'authoritative Knowledge version is missing' };
  if (ids.length > 1) return { available: false, reason: `authoritative Knowledge version is inconsistent (${ids.join(', ')})` };
  const record = candidates.find((candidate) => candidate.record && candidate.id === ids[0])?.record || null;
  if (record?.status && String(record.status).toLowerCase() !== 'current') {
    return { available: false, reason: `Knowledge version ${ids[0]} is not current` };
  }
  return {
    available: true,
    id: ids[0],
    record,
    origin: candidates.filter((candidate) => candidate.id === ids[0]).map((candidate) => candidate.origin),
  };
}

function tableRowsForLineage(table) {
  return firstArray(
    table?.rows,
    table?.records,
    table?.data,
    table?.payload?.rows,
    table?.payload?.records,
    table?.payload?.data,
    table?.dataframe?.rows,
    table?.payload?.dataframe?.rows,
  ).filter((row) => row && typeof row === 'object');
}

function knowledgeLineageForPayload(base, latestRun = null, sourceModels = []) {
  const version = knowledgeVersionContext(base, latestRun);
  const models = sourceModels.length
    ? sourceModels
    : (tableForKey(base, 'source_catalog') ? tableRowsForLineage(tableForKey(base, 'source_catalog')).map((row) => ({ id: sourceId(row), row, evidenceEligible: evidenceGate(row).eligible })) : []);
  let sourceReceipts = models
    .filter((source) => source?.evidenceEligible)
    .map(sourceReceiptLineage)
    .filter((receipt) => receipt.valid)
    .map(({ valid, ...receipt }) => receipt);
  let sourceById = new Map(sourceReceipts.map((receipt) => [receipt.source_id, receipt]));
  const evidenceLineage = [];
  const claimLineage = [];
  for (const table of base?.tables || []) {
    for (const row of tableRowsForLineage(table)) {
      const source = sourceById.get(firstString(row, ['source_id']));
      if (!source) continue;
      const snapshotId = firstString(row, ['snapshot_id', 'source_snapshot_id']) || source.snapshot_id;
      const snapshotHash = firstString(row, ['snapshot_hash', 'snapshot_sha256']) || source.snapshot_hash;
      const canonicalUrl = firstString(row, ['canonical_url']) || source.canonical_url;
      if (!snapshotId || snapshotHash !== source.snapshot_hash || canonicalUrl !== source.canonical_url) continue;
      const evidenceId = firstString(row, ['evidence_id']);
      const claimId = firstString(row, ['claim_id']);
      const lineage = {
        source_id: source.source_id,
        canonical_url: source.canonical_url,
        snapshot_id: snapshotId,
        snapshot_hash: snapshotHash,
        evidence_id: evidenceId,
        claim_id: claimId,
        claim_text: firstString(row, ['claim_text', 'fact_label', 'fact_value']),
        lineage_sha256: firstString(row, ['lineage_sha256']),
        table_key: table.table_key || '',
        row_id: firstString(row, ['row_id', 'id', 'record_id']),
      };
      if (evidenceId) evidenceLineage.push(lineage);
      if (claimId) claimLineage.push(lineage);
    }
  }
  if (!sourceModels.length) {
    const runLineage = [
      latestRun?.payload?.evidence_lineage,
      latestRun?.payload?.lineage,
      latestRun?.evidence_lineage,
      latestRun?.lineage,
    ].filter((value) => value && typeof value === 'object');
    const runReceipts = runLineage.flatMap((lineage) => firstArray(lineage.source_receipts, lineage.source_lineage, lineage.sources))
      .map((receipt) => ({
        ...receipt,
        receipt_url: firstString(receipt, ['receipt_url', 'source_receipt_url', 'receipt_link', 'url']),
        receipt_id: firstString(receipt, ['source_receipt_id', 'evidence_receipt_id', 'receipt_id']),
        canonical_url: firstString(receipt, ['canonical_url', 'source_url']),
        snapshot_id: firstString(receipt, ['snapshot_id', 'source_snapshot_id']),
        snapshot_hash: firstString(receipt, ['snapshot_hash', 'snapshot_sha256']),
      }))
      .filter((receipt) => receipt.source_id && (receipt.receipt_url || receipt.receipt_id) && receipt.canonical_url && receipt.snapshot_id && /^sha256:[0-9a-f]{64}$/i.test(receipt.snapshot_hash));
    sourceReceipts = [...sourceReceipts, ...runReceipts].filter((receipt, index, all) => all.findIndex((candidate) => candidate.source_id === receipt.source_id && candidate.snapshot_hash === receipt.snapshot_hash) === index);
    sourceById = new Map(sourceReceipts.map((receipt) => [receipt.source_id, receipt]));
    for (const lineage of runLineage) {
      for (const row of firstArray(lineage.evidence, lineage.evidence_lineage, lineage.items)) {
        if (row?.evidence_id) evidenceLineage.push(row);
      }
      for (const row of firstArray(lineage.claims, lineage.claim_lineage)) {
        if (row?.claim_id) claimLineage.push(row);
      }
    }
  }
  const dedupe = (rows) => {
    const seen = new Set();
    return rows.filter((row) => {
      const key = JSON.stringify(row);
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  };
  const snapshots = dedupe(sourceReceipts.map((receipt) => ({
    source_id: receipt.source_id,
    canonical_url: receipt.canonical_url,
    snapshot_id: receipt.snapshot_id,
    snapshot_hash: receipt.snapshot_hash,
  })));
  return {
    available: version.available,
    reason: version.reason || '',
    knowledge_version_id: version.id || '',
    knowledge_version: version.record,
    source_receipts: sourceReceipts,
    snapshots,
    evidence: dedupe(evidenceLineage),
    claims: dedupe(claimLineage),
    requested_snapshot_hashes: [...new Set(sourceReceipts.map((receipt) => receipt.snapshot_hash))],
  };
}

function graphDocumentLineage(task, base, latestRun, sourceModels, selectedSourceIds = []) {
  const lineage = knowledgeLineageForPayload(base, latestRun, sourceModels);
  const eligibleSources = (sourceModels || []).filter((source) => source?.evidenceEligible);
  if (!lineage.available) return { ok: false, reason: lineage.reason || 'authoritative Knowledge version is missing' };
  if (!eligibleSources.length) return { ok: false, reason: 'no eligible source receipts are loaded' };
  if (lineage.source_receipts.length !== eligibleSources.length) {
    return { ok: false, reason: 'a verified source is missing its immutable receipt lineage' };
  }
  const selected = new Set((selectedSourceIds || []).map(String));
  const requested = lineage.source_receipts.filter((receipt) => !selected.size || selected.has(receipt.source_id));
  return {
    ok: true,
    task_id: task?.id || '',
    knowledge_version_id: lineage.knowledge_version_id,
    knowledge_version: lineage.knowledge_version,
    source_receipts: lineage.source_receipts,
    requested_snapshot_hashes: [...new Set(requested.map((receipt) => receipt.snapshot_hash))],
    evidence_lineage: {
      knowledge_version_id: lineage.knowledge_version_id,
      source_receipts: lineage.source_receipts,
      snapshots: lineage.snapshots,
      evidence: lineage.evidence,
      claims: lineage.claims,
    },
  };
}

function knowledgeRefreshPayload(task, base, latestRun, commandId) {
  const lineage = knowledgeLineageForPayload(base, latestRun);
  const tables = base?.tables || [];
  const tableRefs = Object.fromEntries(tables
    .filter((table) => table.table_key)
    .map((table) => [table.table_key, table.id || table.table_id || table.table_key]));
  const instruction = [
    `Baue oder aktualisiere die Knowledge Base fuer das abgeschlossene Research "${task.title}".`,
    `Research Task ID: ${task.id}`,
    `Research Run ID: ${latestRun?.id || ''}`,
    `Research Command ID: ${commandId}`,
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
    research_command_id: commandId,
    knowledge_domain: task.knowledge_domain,
    knowledge_version_id: lineage.knowledge_version_id,
    knowledge_version: lineage.knowledge_version,
    immutable_knowledge_version: true,
    source_tables: tableRefs,
    source_lineage: lineage.source_receipts,
    snapshot_lineage: lineage.snapshots,
    evidence_lineage: lineage.evidence,
    claim_lineage: lineage.claims,
    requested_snapshot_hashes: lineage.requested_snapshot_hashes,
    lineage_status: lineage.available ? 'complete' : 'incomplete',
    knowledge_contract: {
      domain: task.knowledge_domain,
      knowledge_version_id: lineage.knowledge_version_id,
      immutable_version_required: true,
      create_or_update: ['skillbook', 'skills', 'runbooks', 'resources'],
      stable_identity: true,
      provenance_required: true,
      source_of_truth: 'original_sources',
      citations: ['claim_id', 'evidence_id', 'snapshot_id', 'source_id', 'canonical_url', 'lineage_sha256'],
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
        knowledge_version_id: lineage.knowledge_version_id,
        source_lineage: lineage.source_receipts,
        snapshot_lineage: lineage.snapshots,
        evidence_lineage: lineage.evidence,
        claim_lineage: lineage.claims,
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
  const payload = knowledgeRefreshPayload(task, base, latestRun, commandId);
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

async function focusCtoxRun(taskQueueId, commandId, taskStatus = '') {
  if (!taskQueueId && !commandId) return;
  const focus = {
    taskId: taskQueueId,
    commandId,
    taskStatus,
    sourceModule: 'research',
    openDrawer: true,
  };
  try {
    sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
  } catch {}
  const params = new URLSearchParams();
  if (taskQueueId) params.set('task_id', taskQueueId);
  if (commandId) params.set('command_id', commandId);
  if (taskStatus) params.set('task_status', taskStatus);
  params.set('source', 'research');
  params.set('drawer', '1');
  location.hash = `#ctox?${params.toString()}`;
  const app = window.CTOX_BUSINESS_OS_APP;
  if (typeof app?.openModule === 'function' && app.activeModule?.id !== 'ctox') {
    await app.openModule('ctox');
  }
  window.dispatchEvent(new CustomEvent('ctox-business-os-focus-task', { detail: focus }));
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
  const visibleModels = state.activeTab === 'candidates' ? state.candidateModels : evidenceRankedSources();
  return visibleModels.find((source) => source.id === state.selectedSourceId)
    || state.sourceModels.find((source) => source.id === state.selectedSourceId)
    || state.candidateModels.find((source) => source.id === state.selectedSourceId)
    || visibleModels[0]
    || null;
}

function latestRunForTask(taskId) {
  if (!taskId) return null;
  const task = state.tasks.find((entry) => entry.id === taskId);
  const lineageIds = new Set(task?.lineage_task_ids?.length ? task.lineage_task_ids : [taskId]);
  return state.runs
    .filter((run) => lineageIds.has(run.task_id))
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
  const status = resolveRunStatus(queueTask, command, run);
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

// Die Queue-Projektion (ctox_queue_tasks) hinkt einem nativen Abbruch nach:
// `ctox queue cancel` setzte den Befehl auf cancelled, die Projektion blieb
// auf queued, und die Sicht hielt den Lauf fuer aktiv - "Research fortsetzen"
// blieb gesperrt (skf.ctox.dev, 02.09.2026). Ein terminaler Befehlsstatus
// ueberstimmt deshalb eine noch offene Projektion.
function resolveRunStatus(queueTask, command, run) {
  const queueStatus = queueTask?.status || '';
  const commandStatus = command?.task_status || command?.status || '';
  const terminalCommand = ['completed', 'failed', 'cancelled'].includes(statusKindFor(commandStatus));
  const openQueue = ['queued', 'running', 'blocked'].includes(statusKindFor(queueStatus));
  if (terminalCommand && openQueue) return commandStatus;
  return queueStatus || commandStatus || run?.status || '';
}

function latestResearchCommandForTask(taskId) {
  if (!taskId) return null;
  return state.commands
    .filter((command) => command.record_id === taskId && String(command.command_type || '').startsWith('research.systematic.'))
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function statusKindFor(status) {
  const value = String(status || '').toLowerCase();
  if (['leased', 'running', 'in_progress', 'collecting', 'review_rework'].includes(value)) return 'running';
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

function defaultMeasurementsTableKey(base) {
  return tableKey(base, ['measured_load_points', 'measurements']) || 'measured_load_points';
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

function domainTaxonomy(task) {
  const kind = inferResearchKind(task);
  if (kind === 'bearing') {
    return {
      fallback: 'other',
      clusters: [
        { id: 'rotorload', label: state.t('subthemeRotorload', 'Rotorlast'), pattern: /rotor|propeller|thrust|force|moment|aerodynamic|windtunnel|windkanal/i },
        { id: 'bench', label: state.t('subthemeBench', 'Prüfstand'), pattern: /bench|pr\u00fcfstand|motor|esc|spindel|dynamometer|dyno|messstand/i },
        { id: 'flightlog', label: state.t('subthemeFlightlog', 'Fluglog'), pattern: /flight|flug|telemetry|telemetrie|mission|ulog|blackbox/i },
        { id: 'vibration', label: state.t('subthemeVibration', 'Vibration'), pattern: /vibration|unwucht|schaden|fault|pitting|edm|abrasiv|sand/i },
        { id: 'simulation', label: state.t('subthemeSimulation', 'Simulation'), pattern: /simulation|modell|gazebo|sih|virtuell|cfd|ansys|numerical/i },
      ],
    };
  }
  if (kind === 'competitive_ai') {
    return {
      fallback: 'other',
      clusters: [
        { id: 'agent', label: 'Agent depth', pattern: /agent|autonomous|orchestrat|workflow|tool use|delegat/i },
        { id: 'enterprise', label: 'Enterprise readiness', pattern: /enterprise|governance|security|compliance|sso|privacy|audit/i },
        { id: 'integration', label: 'Integration/API', pattern: /api|integration|connector|webhook|sdk|slack|salesforce|jira/i },
        { id: 'market', label: 'Market evidence', pattern: /buyer|pricing|customer|market|category|competitor|adoption/i },
        { id: 'research-quality', label: 'Research quality', pattern: /evidence|source|study|benchmark|method|reproduc/i },
      ],
    };
  }
  return {
    fallback: 'other',
    clusters: [
      { id: 'domain', label: 'Domain relevance', pattern: new RegExp(String(task?.knowledge_domain || task?.title || '').split(/[^a-z0-9]+/i).filter((term) => term.length >= 4).slice(0, 6).join('|') || 'a^', 'i') },
      { id: 'evidence', label: 'Evidence quality', pattern: /evidence|source|study|dataset|benchmark|method/i },
    ],
  };
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

function countedTabButton(id, label, rawCount) {
  const count = countText(rawCount);
  const accessibleLabel = `${label} (${count})`;
  return `<button type="button" class="ctox-pane-tab research-counted-tab${state.activeTab === id ? ' is-active' : ''}" role="tab" data-action="tab" data-tab="${id}" aria-selected="${state.activeTab === id}" aria-label="${escapeHtml(accessibleLabel)}" title="${escapeHtml(accessibleLabel)}"><span class="research-tab-label">${escapeHtml(label)}</span><span class="research-tab-count">${escapeHtml(count)}</span></button>`;
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
// Modul-eigene Icon-Pfade. `ctx.getActionIcon` ist die erste Quelle; fehlt sie
// (Modul ausserhalb der Shell-Icon-Kit-Version, unbekannter Name), lieferte
// iconSvg bisher einen leeren String und die Knoepfe standen ohne Glyphe da.
// Referenz: modules/knowledge/index.js (ACTION_ICON_FALLBACK_PATHS).
const RESEARCH_ICON_FALLBACK_PATHS = Object.freeze({
  book: 'M4 5.5A2.5 2.5 0 0 1 6.5 3H20v15H6.5A2.5 2.5 0 0 0 4 20.5v-15ZM4 18a2.5 2.5 0 0 1 2.5-2.5H20M8 7h8',
  close: 'M6 6l12 12M18 6L6 18',
  eye: 'M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Zm9.5 2.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z',
  file: 'M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8l-5-5Zm0 0v5h5M9 13h6M9 17h6',
  focus: 'M4 9V5.5A1.5 1.5 0 0 1 5.5 4H9M15 4h3.5A1.5 1.5 0 0 1 20 5.5V9M20 15v3.5a1.5 1.5 0 0 1-1.5 1.5H15M9 20H5.5A1.5 1.5 0 0 1 4 18.5V15M12 9.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Z',
  knowledge: 'M4 5.5A2.5 2.5 0 0 1 6.5 3H20v15H6.5A2.5 2.5 0 0 0 4 20.5v-15ZM4 18a2.5 2.5 0 0 1 2.5-2.5H20M8 7h8',
  layers: 'M12 3.5 3.5 8l8.5 4.5L20.5 8 12 3.5ZM3.5 12.5 12 17l8.5-4.5M3.5 17 12 21.5l8.5-4.5',
  plus: 'M12 5v14M5 12h14',
  refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4',
  search: 'M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14ZM20 20l-4-4',
});

function iconSvg(name) {
  const kitNames = { plus: 'add', knowledge: 'knowledge' };
  const fromShell = state.ctx?.getActionIcon?.(kitNames[name] || name, 16, 1.8);
  if (fromShell) return fromShell;
  const path = RESEARCH_ICON_FALLBACK_PATHS[name];
  if (!path) return '';
  return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${path}"></path></svg>`;
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
  if (failures.length) {
    const seconds = Math.max(1, Math.round((state.diagnostics.failureRetryAt - Date.now()) / 1000));
    if (hasLocalResearchData()) {
      return state.t('researchRefreshDelayed', `Aktualisierung verzögert – neuer Versuch in ${seconds} s`, seconds);
    }
    return state.diagnostics.failureRetryAt
      ? state.t('researchSyncRetry', `Synchronisation gestört – neuer Versuch in ${seconds} s`, seconds)
      : state.t('researchUnavailableTitle', 'Research ist gerade nicht verfügbar');
  }
  if (!state.diagnostics.reloadFinishedAt) return state.t('loadingKnowledge', 'Knowledge wird geladen...');
  const domainCount = state.knowledgeBases.length;
  const taskCount = state.tasks.length;
  const sourceCount = state.sourceModels.length;
  if (!domainCount) return state.t('noKnowledgeDomains', 'Noch keine Knowledge Base verfügbar');
  return state.t('researchReadySummary', '{0} Aufgaben, {1} Knowledge Bases, {2} Quellen verfügbar.', taskCount, domainCount, sourceCount);
}

function visibleResearchStatus() {
  if (diagnosticFailures().length) return reloadStatusText();
  if (state.initialDataReady && state.tasks.length) return '';
  return state.status;
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
  const value = typeof doc?.toJSON === 'function' ? doc.toJSON() : doc;
  if (!value || typeof value !== 'object') return value;
  if (typeof structuredClone === 'function') return structuredClone(value);
  return JSON.parse(JSON.stringify(value));
}

function firstArray(...values) {
  return values.find(Array.isArray) || [];
}

function parseStringList(value) {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (typeof value !== 'string' || !value.trim()) return [];
  try {
    const parsed = JSON.parse(value);
    if (Array.isArray(parsed)) return parsed.map(String).filter(Boolean);
  } catch {}
  return value.split(/[,;|]/).map((item) => item.trim()).filter(Boolean);
}

function parseObject(value) {
  if (value && typeof value === 'object' && !Array.isArray(value)) return value;
  if (typeof value !== 'string' || !value.trim()) return null;
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function sourceId(row) {
  return firstString(row, ['source_id', 'id', 'record_id', 'source_key']);
}

function sourceModelId(row) {
  return sourceId(row) || firstString(row, ['candidate_id', 'candidate_key']);
}

function booleanField(row, key) {
  const value = row?.[key];
  if (value === true) return true;
  if (value === false || value === null || value === undefined) return false;
  if (typeof value === 'number') return value === 1;
  const normalized = String(value).trim().toLowerCase();
  return normalized === 'true' || normalized === '1' || normalized === 'yes' || normalized === 'ja';
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
  return state.t('defaultPromptText', `Erzeuge ein belegtes Research-Dashboard auf Basis der Knowledge Base ${base.domain}. Finde und verifiziere Quellen (source_candidates → source_catalog), werte danach JEDE aufgenommene Quelle inhaltlich aus und schreibe ihre belegten Aussagen mit wörtlichem Zitat und Fundstelle nach claims. Konsolidiere gleiche Aussagen quellenübergreifend, halte Widersprüche fest und verbinde jede Quelle im semantischen Graphen.`, base.domain);
}

function topicFitScore(task, text, row) {
  const titleText = String(row?.title || '').toLowerCase();
  
  const hasBearingTopic = /propeller|rotor|uav|drone|bearing|load|force|moment|thrust|torque|rpm|vibration|spindel|motor|flight|telemetry|aerodynamic|blade|windtunnel|w\u00e4lzlager|lager|schub|drehmoment|last|messung|pr\u00fcfstand|spindle|vibrat|flight|telemetr|testing|bench|load cell|stanag|mil-std/i.test(titleText);

  if (inferResearchKind(task) === 'bearing' && !hasBearingTopic) {
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

function sourceTierGrade(row) {
  const tier = firstString(row, ['source_tier', 'evidence_tier', 'quality_tier']).trim().toUpperCase();
  const match = tier.match(/(?:^|[^A-D])([A-D])(?:$|[^A-D])/);
  return match?.[1] || (/^[A-D]$/.test(tier) ? tier : '');
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
  return optionalNumberValue(value) ?? 0;
}

function optionalNumberValue(value) {
  if (value === null || value === undefined || (typeof value === 'string' && value.trim() === '')) return null;
  const next = Number(value);
  return Number.isFinite(next) ? next : null;
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
  // Projektregel "nichts ausserhalb der App": der Viewer rendert im
  // Modul-Host, nicht auf document.body. Ohne Host wird er nicht gezeigt.
  const mountTarget = state.ctx?.host?.querySelector('[data-research-root]') || state.ctx?.host || null;
  mountTarget?.querySelector('.research-prompt-viewer')?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "ctox-modal research-prompt-viewer research-module-overlay";
  backdrop.innerHTML = `
    <div class="ctox-modal-card">
      <header class="ctox-modal-header">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">KI-Generierung</span>
          <h3 class="ctox-modal-title">System-Prompt des Fachberichts</h3>
        </div>
        <button type="button" class="ctox-button" data-close>Schließen</button>
      </header>
      <div class="ctox-modal-body">
        <div class="research-ai-prompt-pre research-prompt-viewer-text">${escapeHtml(promptText)}</div>
      </div>
    </div>
  `;
  backdrop.addEventListener('click', (event) => {
    if (event.target === backdrop || event.target.closest('[data-close]')) backdrop.remove();
  });
  if (mountTarget) mountTarget.appendChild(backdrop);
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
      ? 'Dokumente sind noch nicht verfügbar'
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
      ? 'Dokumentversionen sind noch nicht verfügbar'
      : 'Keine Datenfreigabe für Dokumentinhalte');
  }
  if (versionId) {
    const version = await versions.findOne(versionId).exec();
    const blobId = version && typeof version.toJSON === 'function' ? version.toJSON().blob_id : null;
    if (blobId) {
      const chunkDocs = await blobChunks.find({ selector: { blob_id: blobId } }).exec();
      const chunks = chunkDocs.map((c) => (typeof c.toJSON === 'function' ? c.toJSON() : c));
      const validation = validateChunkSequence(chunks, {
        indexFields: ['idx', 'index', 'chunk_index', 'chunkIndex'],
        countFields: ['total', 'chunk_count', 'chunkCount'],
        offsetFields: ['offset', 'byte_offset', 'byteOffset', 'start'],
        itemCountFields: [],
        itemArrayFields: [],
        itemValueFields: ['data'],
        itemLabel: 'Daten',
        requireItems: true,
        offsetUnit: 'payload',
      });
      if (validation.valid) {
        const joined = validation.rows.join('');
        try {
          const bytes = Uint8Array.from(atob(joined), (ch) => ch.charCodeAt(0));
          return new TextDecoder('utf-8').decode(bytes);
        } catch {
          return joined;
        }
      }
      throw new Error(`Dokument-Chunks sind unvollständig: ${validation.reason}`);
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
  const taskIds = new Set([task.id, ...(task.lineage_task_ids || [])].filter(Boolean).map(String));
  return (document.linked_records || []).some((record) => {
    const kind = String(record?.kind || record?.type || record?.record_type || '').toLowerCase();
    const id = String(record?.id || record?.record_id || record?.value || '');
    return (kind === 'research_task' && taskIds.has(id))
      || (kind === 'knowledge_domain' && id === task.knowledge_domain);
  });
}

function setCollectionReadinessForTest(name, snapshot) {
  if (snapshot == null) delete state.readiness[name];
  else state.readiness[name] = snapshot;
}

export const __researchTestHooks = {
  availableSubthemes,
  graphProjectionKey,
  resolveRunStatus,
  countText,
  failureRetryDelay,
  researchDataState,
  taskSourceSummary,
  RESEARCH_TABLE_CONTRACT,
  defaultPromptForKnowledgeBase,
  buildSourceModels,
  knowledgeClaims,
  renderKnowledgeTables,
  scoringDimensionsForTask,
  setStateForTest: (patch) => Object.assign(state, patch),
  collapseResearchTaskLineages,
  isDeletedResearchTask,
  collectionDiagnosticRows,
  dataEmptyShowsSyncing,
  diagnosticRows,
  disabledTabButton,
  emptyStateForNoTask,
  evidenceGate,
  defaultMeasurementsTableKey,
  eligibleGraphFocusSourceIds,
  filterGraphRowsForEvidence,
  filterMeasurementRowsForEvidence,
  formatDimensionScore,
  formatPortfolioScore,
  aggregateMeasurements,
  hasVerifiedEvidence: () => evidenceRankedSources().length > 0,
  knowledgeBasesFromTables,
  knowledgeTableChunkDocumentIds,
  knowledgeTableRowStates: () => ({ ...(state.knowledgeTableRowStates || {}) }),
  loadDashboardData,
  loadKnowledgeBases,
  mergeKnowledgeTableChunks,
  renderKnowledgeTableRowStates,
  renderMeasurementsTable,
  resetKnowledgeRowsForTest,
  setLoadedOnceForTest: () => { state.diagnostics.loadedOnce = true; },
  knowledgeLineageForPayload,
  knowledgeRefreshPayload,
  compactKnowledgeTableReferences,
  graphDocumentLineage,
  latestEvidenceRunForTask,
  researchScoringContract,
  researchReportsForTask,
  // Der Haken behaelt seinen Namen; die Ansicht ist seit dem 31.08.2026 eine
  // kompakte Liste statt einer Tabelle.
  renderSourcesTable: renderSourcesList,
  renderSourceCard,
  sourcesViewToggleButton,
  normalizeKnowledgeTableRows,
  renderNoTaskCenter,
  renderNoTasksEmpty,
  researchDomainFromFormValue,
  setCollectionReadinessForTest,
  sourceTierGrade,
  metricPropellerLength,
  boundedVerifiedSourceCount,
  effectiveTargetVerifiedSources,
  shouldRetryEmptyKnowledgeTables,
  tangentialEquivalentForce,
  toJson,
  uniqueSourceModels,
  validateChunkSequence,
  validateResearchTaskInput,
  validateSelectedResearchTask,
};
