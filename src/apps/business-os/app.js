const SESSION_TOKEN_KEY = 'ctox.businessOs.sessionToken';
const AUTH_HEADER_KEY = 'ctox.businessOs.authHeader';
const LOGGED_OUT_KEY = 'ctox.businessOs.loggedOut';
const ACCOUNT_PREFS_KEY = 'ctox.businessOs.accountPreferences';
const PAIRING_CONFIG_KEY = 'ctox.businessOs.pairingConfig';
const RXDB_BOOTSTRAP_VERSION_KEY = 'ctox.businessOs.rxdbBootstrapVersion';
const RXDB_SCHEMA_REPAIR_KEY = 'ctox.businessOs.rxdbSchemaRepair';
const MODULE_LAYOUT_KEY = 'ctox.businessOs.moduleLayout';
const TASKBAR_PINS_KEY = 'ctox.businessOs.taskbarPins';
const SHELL_COLUMN_LAYOUT_KEY_PREFIX = 'ctox.businessOs.shellColumnLayout.';
const APP_BUILD = '20260528-runtime-form-fix1';
const MAX_TRANSIENT_MODULE_SYNC_RETRIES = 3;
const BUSINESS_DB_NAME = 'ctox_business_os_v10';
const RXDB_BOOTSTRAP_VERSION = '20260522-rxdb-db14';
const CTOX_HEALTH_POLL_MS = 10000;
const SYNC_RECOVERY_REPAIR_DELAY_MS = 15000;
const SHELL_IMPORT_TIMEOUT_MS = 45000;
const DEFAULT_TASKBAR_PIN_IDS = ['ctox', 'tickets', 'documents', 'spreadsheets', 'explorer', 'knowledge', 'app-store', 'research', 'calendar'];
let moduleLayoutSaveTimer = null;
let taskbarPinSaveTimer = null;
let shellColumnResizeSync = null;
let syncRecoveryRepairTimer = null;
let syncRecoveryRepairRunning = false;
let businessReporterModulePromise = null;
let businessChatModulePromise = null;
let shellUiModulesPromise = null;
let shellUiModules = null;
let shellDialogsModulePromise = null;
let shellIconsModulePromise = null;
let shellIconsModule = null;
let businessDbModulePromise = null;
let businessDbModule = null;
let syncModulePromise = null;
let syncModule = null;
let commandBusModulePromise = null;
let coreSchemaModulesPromise = null;
let reactSettingsModulePromise = null;

const SHELL_COL_MIN = {
  left: 210,
  center: 420,
  right: 260,
};

const SHELL_COL_SIDE_MAX = 620;

const state = {
  bootTimings: {
    startedAt: new Date().toISOString(),
    startedAtMs: performance.now(),
    shellVisibleMs: null,
    firstWebRtcConnectedMs: null,
    firstAdvancedStatusHealthyMs: null,
  },
  modules: [],
  activeModule: null,
  moduleRevisions: {},
  navHistory: [],
  navIndex: -1,
  navTransitioning: false,
  activeUnmount: null,
  db: null,
  dataPlaneReady: null,
  dataPlaneReadyStatus: 'idle',
  dataPlaneReadyReason: '',
  dataPlaneReadyResolve: null,
  dataPlaneReadyReject: null,
  dataPlaneGeneration: 0,
  sync: null,
  syncConfig: null,
  syncDiagnostics: null,
  advancedStatusEverHealthy: false,
  commandBus: null,
  session: null,
  governance: null,
  moduleLayout: null,
  taskbarPins: [],
  schemaRegistrations: new Map(),
  schemaRegistrationQueue: Promise.resolve(),
  schemaImportRetries: new Map(),
  schemaRetryTimers: new Map(),
  syncStartedModules: new Set(),
  backgroundModuleWorkScheduled: false,
  ctoxHealth: null,
  fileIntegrityDiagnostics: [],
  ctoxHealthTimer: null,
  eventBus: null,
  contextMenu: null,
  notifications: null,
  windowManager: null,
  taskbar: null,
  windowSwitcher: null,
  windowGeometryCache: new Map(),
  windowGeometrySaveTimers: new Map(),
  catalogSubscription: null,
  catalogRefreshTimer: null,
  catalogRefreshRunning: false,
  catalogRefreshQueued: false,
  moduleCatalogFingerprint: '',
  shellCatalogMergedIds: new Set(),
  initialModuleOpened: false,
};

function resetDataPlaneReady(reason = 'startup') {
  state.dataPlaneReadyStatus = 'pending';
  state.dataPlaneReadyReason = reason;
  state.dataPlaneReady = new Promise((resolve, reject) => {
    state.dataPlaneReadyResolve = resolve;
    state.dataPlaneReadyReject = reject;
  });
}

function resolveDataPlaneReady() {
  state.dataPlaneReadyStatus = 'ready';
  state.dataPlaneReadyReason = '';
  const resolve = state.dataPlaneReadyResolve;
  state.dataPlaneReadyResolve = null;
  state.dataPlaneReadyReject = null;
  if (resolve) resolve(true);
}

function rejectDataPlaneReady(error) {
  state.dataPlaneReadyStatus = 'failed';
  state.dataPlaneReadyReason = String(error?.message || error || 'Datenspeicher konnte nicht gestartet werden');
  const reject = state.dataPlaneReadyReject;
  state.dataPlaneReadyResolve = null;
  state.dataPlaneReadyReject = null;
  if (reject) reject(error);
}

async function waitForDataPlaneReady(timeoutMs = 30000) {
  if (state.db?.collection?.('ctox_runtime_settings') && state.sync && state.commandBus) {
    return true;
  }
  const readiness = state.dataPlaneReady;
  if (!readiness) {
    throw new Error('Business OS Datenspeicher wird noch vorbereitet.');
  }
  await Promise.race([
    readiness,
    new Promise((_, reject) => {
      window.setTimeout(() => reject(new Error('Business OS Datenspeicher ist noch nicht bereit.')), timeoutMs);
    }),
  ]);
  if (!state.db?.collection?.('ctox_runtime_settings')) {
    throw new Error('ctox_runtime_settings collection is not registered after data-plane startup');
  }
  return true;
}

function installAdvancedStatusInterface() {
  const api = {
    version: 'business-os-advanced-status-v1',
    async snapshot(options = {}) {
      return buildAdvancedStatusSnapshot(options);
    },
    async waitForHealthy(options = {}) {
      const timeoutMs = Number(options.timeoutMs || 30000);
      const intervalMs = Number(options.intervalMs || 500);
      const deadline = Date.now() + timeoutMs;
      let lastSnapshot = null;
      while (Date.now() < deadline) {
        lastSnapshot = await buildAdvancedStatusSnapshot({ ...options, includeCounts: false });
        if (lastSnapshot.ok) return lastSnapshot;
        await new Promise((resolve) => window.setTimeout(resolve, intervalMs));
      }
      const error = new Error(`Business OS advanced status did not become healthy: ${JSON.stringify(lastSnapshot)}`);
      error.status = lastSnapshot;
      throw error;
    },
  };
  window.CTOX_BUSINESS_OS_STATUS = api;
  window.CTOX_BUSINESS_OS_APP = state;
}

installAdvancedStatusInterface();

if (new URLSearchParams(window.location.search).has('rxdbSmoke')) {
  const smokeRoot = typeof globalThis === 'undefined' ? window : globalThis;
  const smokeApi = { state, openDesktopApp, reportFileIntegrityError };
  smokeRoot.ctoxBusinessOsSmoke = smokeApi;
  window.ctoxBusinessOsSmoke = smokeApi;
}

const moduleAliases = {
  notizen: 'notes',
};
const LEGACY_MODULE_ALIASES = new Map(Object.entries(moduleAliases));

async function loadShellUiModules() {
  if (!shellUiModulesPromise) {
    shellUiModulesPromise = Promise.all([
      importBusinessOsModule('./shared/event-bus.js?v=20260519-shell-os1', 'shell event bus'),
      importBusinessOsModule('./shared/notifications.js?v=20260519-shell-os1', 'shell notifications'),
      importBusinessOsModule('./shared/context-menu.js?v=20260519-shell-os1', 'shell context menu'),
      importBusinessOsModule('./shared/window-manager.js?v=20260527-rxdb-release1', 'shell window manager'),
      importBusinessOsModule('./shared/taskbar.js?v=20260527-rxdb-release1', 'shell taskbar'),
      importBusinessOsModule('./shared/window-switcher.js?v=20260519-shell-os1', 'shell window switcher'),
    ]).then(([
      eventBus,
      notifications,
      contextMenu,
      windowManager,
      taskbar,
      windowSwitcher,
    ]) => ({
      createEventBus: eventBus.createEventBus,
      createNotifications: notifications.createNotifications,
      createContextMenu: contextMenu.createContextMenu,
      createWindowManager: windowManager.createWindowManager,
      createTaskbar: taskbar.createTaskbar,
      createWindowSwitcher: windowSwitcher.createWindowSwitcher,
    }));
  }
  shellUiModules = await shellUiModulesPromise;
  return shellUiModules;
}

async function loadShellDialogsModule() {
  if (!shellDialogsModulePromise) {
    shellDialogsModulePromise = importBusinessOsModule('./shared/dialogs.js?v=20260527-rxdb-release1', 'shell dialogs');
  }
  return shellDialogsModulePromise;
}

async function loadShellIconsModule() {
  if (!shellIconsModulePromise) {
    shellIconsModulePromise = importBusinessOsModule('./shared/icons.js?v=20260520-svg-icons2', 'shell icons')
      .then((mod) => {
        shellIconsModule = mod;
        return mod;
      });
  }
  return shellIconsModulePromise;
}

function getRegisteredSvgIcon(id, size, strokeWidth) {
  return shellIconsModule?.getSvgIcon?.(id, size, strokeWidth) || '';
}

async function loadBusinessDbModule() {
  if (!businessDbModulePromise) {
    businessDbModulePromise = importBusinessOsModule('./shared/db.js?v=20260528-rxdb-native1', 'business db')
      .then((mod) => {
        businessDbModule = mod;
        return mod;
      });
  }
  return businessDbModulePromise;
}

async function loadSyncModule() {
  if (!syncModulePromise) {
    syncModulePromise = importBusinessOsModule('./shared/sync.js?v=20260528-rxdb-native1', 'business sync')
      .then((mod) => {
        syncModule = mod;
        return mod;
      });
  }
  return syncModulePromise;
}

async function loadCommandBusModule() {
  if (!commandBusModulePromise) {
    commandBusModulePromise = importBusinessOsModule('./shared/command-bus.js?v=20260521-rxdb-db32', 'command bus');
  }
  return commandBusModulePromise;
}

async function loadCoreSchemaModules() {
  if (!coreSchemaModulesPromise) {
    coreSchemaModulesPromise = Promise.all([
      importBusinessOsModule('./modules/ctox/schema.js?v=20260525-file-viewer-command-reuse1', 'ctox core schema'),
      importBusinessOsModule('./modules/desktop/schema.js?v=20260525-file-viewer-command-reuse1', 'desktop core schema'),
    ]).then(([ctox, desktop]) => ({ ctox, desktop }));
  }
  return coreSchemaModulesPromise;
}

async function loadReactSettingsModule() {
  if (!reactSettingsModulePromise) {
    reactSettingsModulePromise = importBusinessOsModule(`./shared/react-settings.js?v=${APP_BUILD}`, 'react settings');
  }
  return reactSettingsModulePromise;
}

async function importBusinessOsModule(url, label) {
  try {
    return await withImportTimeout(import(url), label, url);
  } catch (error) {
    console.warn(`[business-os] ${label} import stalled or failed; retrying once`, error);
    const separator = url.includes('?') ? '&' : '?';
    return withImportTimeout(
      import(`${url}${separator}retry=${Date.now().toString(36)}`),
      `${label} retry`,
      url,
    );
  }
}

function withImportTimeout(promise, label, url) {
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      window.setTimeout(() => {
        reject(new Error(`${label} import timed out after ${SHELL_IMPORT_TIMEOUT_MS}ms: ${url}`));
      }, SHELL_IMPORT_TIMEOUT_MS);
    }),
  ]);
}

const shellMessages = {
  de: {
    context: 'Kontext',
    topics: 'Themen',
    loadingWorkspace: 'Workspace wird geladen',
    loadingModule: 'Modul-Workspace wird geladen.',
    localWorkspace: 'Lokaler Workspace',
    loginRequired: 'Login erforderlich',
    startupChecking: 'CTOX-Sitzung prüfen',
    syncConnecting: 'Sync-Verbindungen starten',
    collection: 'Collection',
    activity: 'Aktivität',
    agentContext: 'Agent-Kontext',
    webrtcSync: 'WebRTC-Sync',
    ctoxNotWorking: 'CTOX ARBEITET NICHT',
    ctoxStopped: 'CTOX Service läuft nicht.',
    ctoxStatusUnavailable: 'CTOX Status nicht erreichbar.',
    ctoxLastError: 'Letzter Fehler',
    desktop: 'Desktop',
    showDesktop: 'Desktop anzeigen',
    closeModule: 'Schließen',
    selectVersion: 'Version...',
    windowDefaultTitle: 'Fenster',
    windowMaximize: 'Maximieren',
    windowRestore: 'Wiederherstellen',
    windowMinimize: 'Minimieren',
    windowClose: 'Schließen',
    windowSnapLeft: 'Links anheften',
    windowSnapRight: 'Rechts anheften',
    windowSnapTop: 'Oben anheften',
    windowSnapBottom: 'Unten anheften',
    windowAlwaysOnTop: 'Immer im Vordergrund',
    windowAlwaysOnTopOff: 'Immer im Vordergrund: aus',
    windowCloseOthers: 'Andere Fenster schließen',
    pinToTaskbar: 'An Bar anheften',
    unpinFromTaskbar: 'Von Bar lösen',
    pinned: 'Gepinnt',
    running: 'Läuft',
    openApp: 'Öffnen',
    chatToCtox: 'Mit CTOX chatten',
    chatWorkDataLabel: 'Mit Daten arbeiten',
    chatModifyAppLabel: 'App modifizieren',
    chatPlaceholder: 'Was soll CTOX hier tun oder prüfen?',
    chatOpening: 'Öffne Chat...',
    send: 'Senden',
    moduleTitles: {
      desktop: 'Desktop',
      ctox: 'CTOX',
      documents: 'Dokumente',
      spreadsheets: 'Tabellen',
      knowledge: 'Knowledge',
      'matching': 'Matching',
      reports: 'Bugs & Features',
      tickets: 'Tickets',
      research: 'Web Research',
      conversations: 'Conversations',
      calendar: 'Kalender',
    },
  },
  en: {
    context: 'Context',
    topics: 'Topics',
    loadingWorkspace: 'Loading workspace',
    loadingModule: 'Loading module workspace.',
    localWorkspace: 'Local workspace',
    loginRequired: 'Login required',
    startupChecking: 'Checking CTOX session',
    syncConnecting: 'Connecting sync peers',
    collection: 'Collection',
    activity: 'Activity',
    agentContext: 'Agent context',
    webrtcSync: 'WebRTC sync',
    ctoxNotWorking: 'CTOX NOT WORKING',
    ctoxStopped: 'CTOX service is not running.',
    ctoxStatusUnavailable: 'CTOX status is unavailable.',
    ctoxLastError: 'Last error',
    desktop: 'Desktop',
    showDesktop: 'Show desktop',
    closeModule: 'Close',
    selectVersion: 'Version...',
    windowDefaultTitle: 'Window',
    windowMaximize: 'Maximize',
    windowRestore: 'Restore',
    windowMinimize: 'Minimize',
    windowClose: 'Close',
    windowSnapLeft: 'Snap left',
    windowSnapRight: 'Snap right',
    windowSnapTop: 'Snap top',
    windowSnapBottom: 'Snap bottom',
    windowAlwaysOnTop: 'Always on top',
    windowAlwaysOnTopOff: 'Always on top: off',
    windowCloseOthers: 'Close other windows',
    pinToTaskbar: 'Pin to bar',
    unpinFromTaskbar: 'Unpin from bar',
    pinned: 'Pinned',
    running: 'Running',
    openApp: 'Open',
    chatToCtox: 'Chat to CTOX',
    chatWorkDataLabel: 'Work with data',
    chatModifyAppLabel: 'Modify app',
    chatPlaceholder: 'What should CTOX do or check here?',
    chatOpening: 'Opening Chat...',
    send: 'Send',
    moduleTitles: {
      desktop: 'Desktop',
      ctox: 'CTOX',
      documents: 'Documents',
      spreadsheets: 'Spreadsheets',
      knowledge: 'Knowledge',
      'matching': 'Matching',
      reports: 'Bugs & Features',
      tickets: 'Tickets',
      research: 'Web Research',
      conversations: 'Conversations',
      calendar: 'Calendar',
    },
  },
};

const els = {
  status: document.querySelector('[data-status-text]'),
  ctoxWarning: document.querySelector('[data-ctox-shell-warning]'),
  tabs: document.querySelector('[data-module-tabs]'),
  host: document.querySelector('[data-module-host]'),
  leftContent: document.querySelector('[data-left-content]'),
  rightContent: document.querySelector('[data-right-content]'),
  backdrop: document.querySelector('[data-backdrop]'),
  leftDrawer: document.querySelector('[data-drawer-left]'),
  rightDrawer: document.querySelector('[data-drawer-right]'),
  bottomDrawer: document.querySelector('[data-drawer-bottom]'),
  accountButton: document.querySelector('[data-open-account]'),
  languageSelect: document.querySelector('[data-language-select]'),
  themeSelect: document.querySelector('[data-theme-select]'),
  shellStyleSelect: document.querySelector('[data-shell-style-select]'),
  shellWindowLayer: document.querySelector('[data-shell-window-layer]'),
  shellNotifications: document.querySelector('[data-shell-notifications]'),
  shellTaskbar: document.querySelector('[data-shell-taskbar]'),
  shellSwitcherOverlay: document.querySelector('[data-shell-window-switcher]'),
  shellSwitcherPanel: document.querySelector('[data-shell-window-switcher-panel]'),
  showDesktop: document.querySelector('[data-show-desktop]'),
  backButton: document.querySelector('[data-shell-back]'),
  forwardButton: document.querySelector('[data-shell-forward]'),
};

let currentProgress = 0;
let progressTimer = null;

bootstrap().catch(async (error) => {
  if (await recoverFromLocalRxDbSchemaDrift(error)) return;
  console.error(error);
  showStartupError(error);
});

async function bootstrap() {
  resetDataPlaneReady('bootstrap');
  if (!globalThis.crypto?.subtle) {
    throw new Error('WebCrypto is missing (Insecure Origin on Safari 127.0.0.1). Please use http://localhost:8765/');
  }
  const { installBusinessDialogFallbacks } = await loadShellDialogsModule();
  installBusinessDialogFallbacks();
  const prefs = readAccountPrefs();
  applyShellTheme(prefs.theme || 'dark', { persist: false });
  applyShellLanguage(prefs.language || 'de', { persist: false });
  applyShellStyle(prefs.shellStyle || 'windows', { persist: false });
  syncHeaderControls();
  wireShellActions();

  setStartupProgress(10, 'System-Konfiguration wird geladen...');

  setStartupProgress(30, 'Anmeldesitzung wird überprüft...');
  const session = await loadSession();
  state.session = session;
  renderAccountButton(session);
  if (!session.authenticated) {
    state.dataPlaneReadyStatus = 'idle';
    state.dataPlaneReadyReason = 'login-required';
    const loginFailed = session.reason === 'invalid_credentials'
      || new URLSearchParams(location.search).has('loginFailed');
    clearStoredBrowserAuth();
    renderLoginGate(session, { loginFailed });
    setStatus(shellText('loginRequired'));
    return;
  }

  setStartupProgress(50, 'Lokaler Datenspeicher wird geladen...');
  const syncConfig = await loadSyncConfig();
  await resetBusinessDataPlaneForBuildIfNeeded(syncConfig);
  await openBusinessDataPlane(syncConfig);

  setStartupProgress(70, 'Workspace wird vorbereitet...');
  let modules;
  try {
    setStartupProgress(85, 'Ihre Anwendungen werden vorbereitet...');
    modules = await loadModules();
  } catch (error) {
    if (!isModuleCatalogSyncError(error)) throw error;
    console.warn('[business-os] module catalog sync stalled; resetting local RxDB cache and retrying WebRTC sync', error);
    setStartupProgress(80, 'Lokaler Datenspeicher wird optimiert...');
    await repairBusinessDataPlane(syncConfig);
    modules = await loadModules(20000);
  }
  state.modules = modules.modules || [];
  state.moduleCatalogFingerprint = modules.catalogFingerprint || state.moduleCatalogFingerprint;
  registerCustomModuleIcons().catch((error) => {
    console.warn('[business-os] deferred custom module icon registration failed:', error);
  });
  state.governance = modules.governance || null;
  state.moduleLayout = normalizeModuleLayout(await loadModuleLayout(), state.modules);
  state.taskbarPins = normalizeTaskbarPins(readTaskbarPins(), state.modules);
  persistModuleLayout();
  renderTabs();
  const shellUi = await loadShellUiModules();
  state.eventBus = shellUi.createEventBus();
  state.contextMenu = shellUi.createContextMenu({
    host: document.body,
    viewportEl: document.documentElement,
  });
  state.notifications = shellUi.createNotifications({
    container: els.shellNotifications,
    t: (key, fallback) => shellText(key) || fallback || key,
  });
  const snapPreviewEl = document.createElement('div');
  snapPreviewEl.className = 'shell-snap-preview';
  snapPreviewEl.hidden = true;
  els.shellWindowLayer.appendChild(snapPreviewEl);
  state.windowManager = shellUi.createWindowManager({
    windowLayer: els.shellWindowLayer,
    surfaceEl: document.querySelector('.workspace-frame') || document.body,
    rootEl: document.documentElement,
    snapPreviewEl,
    eventBus: state.eventBus,
    t: (key, fallback) => shellText(key) || fallback || key,
    persistence: createWindowGeometryPersistence(),
  });
  state.windowManager.setChromeLayout(
    document.documentElement.dataset.shellStyle === 'macos' ? 'macos' : 'windows'
  );
  state.windowManager.setInsets({ top: 0, bottom: els.shellTaskbar ? 58 : 0 });
  if (els.shellTaskbar) {
    state.taskbar = shellUi.createTaskbar({
      container: els.shellTaskbar,
      windowManager: state.windowManager,
      eventBus: state.eventBus,
      t: (key, fallback) => shellText(key) || fallback || key,
      ownerLabelFor: deriveOwnerLabel,
    });
  }
  if (els.shellSwitcherOverlay && els.shellSwitcherPanel) {
    state.windowSwitcher = shellUi.createWindowSwitcher({
      overlay: els.shellSwitcherOverlay,
      panel: els.shellSwitcherPanel,
      windowManager: state.windowManager,
      ownerLabelFor: deriveOwnerLabel,
      t: (key, fallback) => shellText(key) || fallback || key,
    });
  }
  wireShellWindowGestures();
  setStatus(shellText('localWorkspace'));

  // Initialize the global ctox context menu
  initGlobalCtoxContextMenu();

  setStartupProgress(95, 'Workspace ist bereit. CTOX wird gestartet...');
  try {
    await openModule(currentHashModuleId() || state.modules[0]?.id || 'ctox');
    markBootTiming('shellVisibleMs');
    setStatus(shellText('localWorkspace'));
    scheduleBusinessCompanions();
  } catch (error) {
    console.error('[business-os] module startup failed', error);
    setStatus(`Module startup failed: ${error.message || error}`);
  } finally {
    state.initialModuleOpened = Boolean(state.activeModule?.id);
    flushDeferredCatalogRefresh();
  }
  scheduleBackgroundModuleWork();
  refreshRemoteShellStateInBackground();
  scheduleCriticalSyncWarmup();
}

function businessDbName(syncConfig = state.syncConfig) {
  const instanceId = String(syncConfig?.instance_id || syncConfig?.instanceId || 'default')
    .replace(/[^a-zA-Z0-9_-]+/g, '_')
    .slice(0, 80) || 'default';
  const originId = String(location.host || location.hostname || 'local')
    .replace(/[^a-zA-Z0-9_-]+/g, '_')
    .slice(0, 80) || 'local';
  const params = new URLSearchParams(location.search);
  const smokeDbId = params.has('rxdbSmoke')
    ? String(params.get('smokeDbId') || '')
        .replace(/[^a-zA-Z0-9_-]+/g, '_')
        .slice(0, 80)
    : '';
  return [BUSINESS_DB_NAME, originId, instanceId, smokeDbId].filter(Boolean).join('_');
}

async function resetBusinessDataPlaneForBuildIfNeeded(syncConfig) {
  const dbName = businessDbName(syncConfig);
  const versionToken = `${RXDB_BOOTSTRAP_VERSION}:${dbName}`;
  const existingToken = localStorage.getItem(RXDB_BOOTSTRAP_VERSION_KEY);
  if (existingToken === versionToken) return;
  if (!existingToken) {
    localStorage.setItem(RXDB_BOOTSTRAP_VERSION_KEY, versionToken);
    return;
  }
  const { resetBusinessDb } = await loadBusinessDbModule();
  setStatus('Lokale RxDB wird neu synchronisiert');
  await resetBusinessDb({ name: dbName });
  localStorage.setItem(RXDB_BOOTSTRAP_VERSION_KEY, versionToken);
}

async function openBusinessDataPlane(syncConfig) {
  resetDataPlaneReady('open-business-data-plane');
  setStartupProgress(51, 'Datenspeicher-Konfiguration wird vorbereitet...');
  try {
    state.syncConfig = syncConfig;
    const dbName = businessDbName(syncConfig);

    setStartupProgress(54, 'Lokaler Speicher wird geöffnet...');
    const { createBusinessDb } = await loadBusinessDbModule();
    state.db = await createBusinessDb({ name: dbName });

    setStartupProgress(58, 'Systemdatenstrukturen werden aufgebaut...');
    await registerCoreCollections();

    setStartupProgress(62, 'Desktop-Layout wird geladen...');
    await hydrateTaskbarPinsFromDesktopLayout();
    renderTabs();

    setStartupProgress(66, 'Echtzeit-Synchronisierung wird gestartet...');
    const { createSyncRuntime } = await loadSyncModule();
    state.sync = createSyncRuntime({
      db: state.db,
      config: syncConfig,
      onDiagnostic: updateSyncDiagnostics,
    });

    setStartupProgress(69, 'Dienste werden gestartet...');
    const { createCommandBus } = await loadCommandBusModule();
    state.commandBus = createCommandBus({
      db: () => state.db,
      config: syncConfig,
    });
    startShellCtoxHealthMonitor();

    if (state.catalogSubscription) {
      try { state.catalogSubscription.unsubscribe(); } catch (e) {}
      state.catalogSubscription = null;
    }
    const catalogColl = state.db?.collection?.('business_module_catalog');
    if (catalogColl) {
      state.catalogSubscription = catalogColl.findOne('module-catalog').$.subscribe(async (doc) => {
        const data = doc?.toJSON?.();
        if (data && data._deleted !== true && data.is_deleted !== true) {
          const fingerprint = moduleCatalogFingerprint(data);
          if (fingerprint && fingerprint === state.moduleCatalogFingerprint) return;
          scheduleCatalogRefresh('database-sync');
        }
      });
    }
    resolveDataPlaneReady();
  } catch (error) {
    rejectDataPlaneReady(error);
    throw error;
  }
}

function scheduleCatalogRefresh(reason = 'database-sync') {
  state.catalogRefreshQueued = true;
  if (!state.initialModuleOpened) {
    console.log(`[business-os] Module catalog update queued until initial shell is visible (${reason}).`);
    return;
  }
  if (state.catalogRefreshTimer) return;
  state.catalogRefreshTimer = window.setTimeout(runQueuedCatalogRefresh, 100);
}

function flushDeferredCatalogRefresh() {
  if (!state.initialModuleOpened || !state.catalogRefreshQueued) return;
  if (state.catalogRefreshTimer) return;
  state.catalogRefreshTimer = window.setTimeout(runQueuedCatalogRefresh, 0);
}

async function runQueuedCatalogRefresh() {
  state.catalogRefreshTimer = null;
  if (!state.catalogRefreshQueued || state.catalogRefreshRunning) return;
  state.catalogRefreshQueued = false;
  state.catalogRefreshRunning = true;
  try {
    console.log('[business-os] Module catalog update detected in database sync.');
    await refreshModules();
  } catch (error) {
    console.warn('[business-os] Module catalog refresh failed:', error);
  } finally {
    state.catalogRefreshRunning = false;
    if (state.catalogRefreshQueued && state.initialModuleOpened && !state.catalogRefreshTimer) {
      state.catalogRefreshTimer = window.setTimeout(runQueuedCatalogRefresh, 100);
    }
  }
}

async function repairBusinessDataPlane(syncConfig) {
  state.dataPlaneGeneration += 1;
  resetDataPlaneReady('repair-business-data-plane');
  clearSyncRecoveryRepairTimer();
  if (state.catalogRefreshTimer) {
    window.clearTimeout(state.catalogRefreshTimer);
    state.catalogRefreshTimer = null;
  }
  state.catalogRefreshRunning = false;
  state.catalogRefreshQueued = false;
  state.moduleCatalogFingerprint = '';
  state.initialModuleOpened = false;
  if (state.ctoxHealthTimer) {
    window.clearInterval(state.ctoxHealthTimer);
    state.ctoxHealthTimer = null;
  }
  if (state.catalogSubscription) {
    try { state.catalogSubscription.unsubscribe(); } catch (e) {}
    state.catalogSubscription = null;
  }
  try { await state.sync?.stop?.(); } catch (error) { console.warn('[business-os] sync stop before cache reset failed', error); }
  try { await state.db?.close?.(); } catch (error) { console.warn('[business-os] db close before cache reset failed', error); }
  state.db = null;
  state.sync = null;
  updateSyncDiagnostics(null);
  state.commandBus = null;
  state.syncStartedModules.clear();
  state.schemaRegistrations.clear();
  for (const timer of state.schemaRetryTimers.values()) {
    window.clearTimeout(timer);
  }
  state.schemaRetryTimers.clear();
  state.schemaRegistrationQueue = Promise.resolve();
  const { resetBusinessDb } = await loadBusinessDbModule();
  await resetBusinessDb({ name: businessDbName(syncConfig) });
  await openBusinessDataPlane(syncConfig);
}

function isModuleCatalogSyncError(error) {
  const message = String(error?.message || error || '');
  return message.includes('Modulkatalog wurde noch nicht synchronisiert')
    || message.includes('business_module_catalog collection is required')
    || message.includes('module catalog');
}

async function registerCoreCollections() {
  const t0 = performance.now();
  setStartupProgress(58, 'Datenstrukturen werden vorbereitet...');

  const { ctox, desktop } = await loadCoreSchemaModules();
  const ctoxSchemes = withMigrationStrategies(ctox.collections, ctox.migrationStrategies);
  const desktopSchemes = withMigrationStrategies(desktop.collections, desktop.migrationStrategies);

  const consolidated = {
    ...ctoxSchemes,
    ...desktopSchemes,
  };

  setStartupProgress(59, 'Speicherstrukturen werden registriert...');
  await state.db.addCollections(consolidated);

  setStartupProgress(61, 'Speicherstrukturen erfolgreich geladen.');
  const t1 = performance.now();
  console.log(`[business-os] registerCoreCollections took ${(t1 - t0).toFixed(2)}ms`);
  await primeWindowGeometryCache();
}

async function primeWindowGeometryCache() {
  const coll = state.db?.collections?.desktop_windows;
  if (!coll) return;
  try {
    const docs = await coll.find().exec();
    state.windowGeometryCache.clear();
    for (const doc of docs) {
      const payload = doc.toJSON();
      if (!payload?.owner_id) continue;
      state.windowGeometryCache.set(payload.owner_id, payload);
    }
  } catch (error) {
    console.error('[business-os] primeWindowGeometryCache failed:', error);
  }
}

function createWindowGeometryPersistence() {
  return {
    load(ownerId) {
      if (!ownerId) return null;
      const cached = state.windowGeometryCache.get(ownerId);
      if (!cached) return null;
      return {
        x: numberOrNull(cached.x),
        y: numberOrNull(cached.y),
        width: numberOrNull(cached.width),
        height: numberOrNull(cached.height),
        state: cached.state || 'normal',
        snapZone: cached.snap_zone || '',
        alwaysOnTop: !!cached.always_on_top,
        stored: cached.stored_x != null || cached.stored_y != null || cached.stored_width != null || cached.stored_height != null
          ? {
              left: cached.stored_x != null ? `${cached.stored_x}px` : '',
              top: cached.stored_y != null ? `${cached.stored_y}px` : '',
              width: cached.stored_width != null ? `${cached.stored_width}px` : '',
              height: cached.stored_height != null ? `${cached.stored_height}px` : '',
            }
          : null,
      };
    },
    save(ownerId, snapshot) {
      if (!ownerId) return;
      const cached = state.windowGeometryCache.get(ownerId) || {};
      const next = {
        id: ownerId,
        owner_id: ownerId,
        title: snapshot.title || cached.title || '',
        icon: snapshot.icon || cached.icon || '',
        x: numberOrNull(snapshot.x),
        y: numberOrNull(snapshot.y),
        width: numberOrNull(snapshot.width),
        height: numberOrNull(snapshot.height),
        state: snapshot.state || 'normal',
        snap_zone: snapshot.snapZone || '',
        always_on_top: !!snapshot.alwaysOnTop,
        stored_x: parsePxOrNull(snapshot.stored?.left),
        stored_y: parsePxOrNull(snapshot.stored?.top),
        stored_width: parsePxOrNull(snapshot.stored?.width),
        stored_height: parsePxOrNull(snapshot.stored?.height),
        updated_at_ms: Date.now(),
      };
      state.windowGeometryCache.set(ownerId, next);
      scheduleGeometryPersist(ownerId, next);
    },
  };
}

function scheduleGeometryPersist(ownerId, payload) {
  const existing = state.windowGeometrySaveTimers.get(ownerId);
  if (existing) clearTimeout(existing);
  const handle = window.setTimeout(() => {
    state.windowGeometrySaveTimers.delete(ownerId);
    flushGeometryPersist(ownerId, payload).catch((error) => {
      console.error('[business-os] geometry persist failed:', error);
    });
  }, 250);
  state.windowGeometrySaveTimers.set(ownerId, handle);
}

async function flushGeometryPersist(ownerId, payload) {
  const coll = state.db?.collections?.desktop_windows;
  if (!coll) return;
  const existing = await coll.findOne(ownerId).exec();
  if (existing) {
    await existing.incrementalPatch({ ...payload, updated_at_ms: Date.now() });
  } else {
    try {
      await coll.insert(payload);
    } catch (error) {
      if (!String(error?.message || '').toLowerCase().includes('already')) throw error;
    }
  }
}

function deriveOwnerLabel(ownerId) {
  if (!ownerId) return '';
  if (ownerId.startsWith('desktop-app:')) {
    const id = ownerId.slice('desktop-app:'.length);
    const entry = DESKTOP_APPS.find((app) => app.id === id);
    return entry?.title || id;
  }
  if (ownerId.startsWith('module:')) {
    const id = ownerId.slice('module:'.length);
    return shellText('moduleTitles')?.[id] || moduleDisplayTitleFor(id) || id;
  }
  return ownerId;
}

function moduleDisplayTitleFor(moduleId) {
  if (!moduleId) return '';
  const mod = state.modules?.find((entry) => entry.id === moduleId);
  return mod?.title || '';
}

const SNAP_KEY_MAP = {
  ArrowLeft: 'left',
  ArrowRight: 'right',
  ArrowUp: 'top',
  ArrowDown: 'bottom',
};

function wireShellWindowGestures() {
  document.addEventListener('keydown', onShellKeyboardShortcut, true);
  if (state.eventBus) {
    state.eventBus.on('window:context_request', handleWindowContextRequest);
    state.eventBus.on('taskbar:item_context', handleTaskbarItemContext);
    [
      'window:opened',
      'window:closed',
      'window:focused',
      'window:minimized',
      'window:restored',
      'window:title_changed',
    ].forEach((eventName) => state.eventBus.on(eventName, renderTabs));
  }
}

function onShellKeyboardShortcut(event) {
  if (!state.windowManager) return;
  if (event.defaultPrevented) return;
  if (!(event.ctrlKey || event.metaKey)) return;
  if (!event.altKey) return;
  if (event.key === 'Tab') return;
  const zone = SNAP_KEY_MAP[event.key];
  if (!zone) return;
  const focused = state.windowManager.listWindows().find((w) => w.isFocused);
  if (!focused) return;
  event.preventDefault();
  event.stopPropagation();
  state.windowManager.snapTo(focused.id, zone);
}

function handleWindowContextRequest(data) {
  if (!state.contextMenu || !state.windowManager) return;
  const desc = state.windowManager.describe(data.id);
  if (!desc) return;
  const fakeEvent = {
    preventDefault() {},
    stopPropagation() {},
    clientX: data.clientX,
    clientY: data.clientY,
  };
  state.contextMenu.show(fakeEvent, buildWindowContextItems(desc));
}

function handleTaskbarItemContext(data) {
  if (!state.contextMenu || !state.windowManager) return;
  const desc = state.windowManager.describe(data.windowId);
  if (!desc) return;
  const fakeEvent = {
    preventDefault() {},
    stopPropagation() {},
    clientX: data.clientX,
    clientY: data.clientY,
  };
  state.contextMenu.show(fakeEvent, buildWindowContextItems(desc));
}

function buildWindowContextItems(desc) {
  const wm = state.windowManager;
  const isMax = desc.state === 'maximized';
  const ownerLabel = deriveOwnerLabel(desc.ownerId);
  const sameOwnerCount = desc.ownerId
    ? wm.listWindows().filter((w) => w.ownerId === desc.ownerId).length
    : 0;
  const items = [
    {
      label: isMax ? (shellText('windowRestore') || 'Wiederherstellen') : (shellText('windowMaximize') || 'Maximieren'),
      icon: isMax ? '❐' : '□',
      action: () => wm.toggleMaximize(desc.id),
    },
    {
      label: shellText('windowMinimize') || 'Minimieren',
      icon: '−',
      action: () => wm.minimize(desc.id),
    },
    { type: 'separator' },
    {
      label: shellText('windowSnapLeft') || 'Links anheften',
      icon: '◧',
      action: () => wm.snapTo(desc.id, 'left'),
    },
    {
      label: shellText('windowSnapRight') || 'Rechts anheften',
      icon: '◨',
      action: () => wm.snapTo(desc.id, 'right'),
    },
    {
      label: shellText('windowSnapTop') || 'Oben anheften',
      icon: '⬒',
      action: () => wm.snapTo(desc.id, 'top'),
    },
    {
      label: shellText('windowSnapBottom') || 'Unten anheften',
      icon: '⬓',
      action: () => wm.snapTo(desc.id, 'bottom'),
    },
    { type: 'separator' },
    {
      label: desc.alwaysOnTop
        ? (shellText('windowAlwaysOnTopOff') || 'Immer im Vordergrund: aus')
        : (shellText('windowAlwaysOnTop') || 'Immer im Vordergrund'),
      icon: desc.alwaysOnTop ? '✓' : '↑',
      action: () => wm.setAlwaysOnTop(desc.id, !desc.alwaysOnTop),
    },
    { type: 'separator' },
  ];
  if (sameOwnerCount > 1) {
    items.push({
      label: ownerLabel
        ? `${shellText('windowCloseOthers') || 'Andere Fenster schließen'} (${ownerLabel})`
        : (shellText('windowCloseOthers') || 'Andere Fenster schließen'),
      icon: '⊠',
      action: () => wm.closeOthersOfOwner(desc.id),
    });
  }
  items.push({
    label: shellText('windowClose') || 'Schließen',
    icon: '×',
    action: () => wm.destroy(desc.id),
  });
  return items;
}

function numberOrNull(value) {
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

function parsePxOrNull(value) {
  if (typeof value !== 'string') return null;
  const m = value.match(/^(-?\d+(?:\.\d+)?)px$/);
  if (!m) return null;
  const n = Number(m[1]);
  return Number.isFinite(n) ? n : null;
}

function wireShellActions() {
  window.addEventListener('unhandledrejection', (event) => {
    if (!isVolatileSyncTransportError(event.reason)) return;
    console.warn('[business-os] ignored volatile local sync transport error', event.reason);
    event.preventDefault();
  });
  window.addEventListener('error', (event) => {
    if (!isVolatileSyncTransportError(event.error || event.message)) return;
    console.warn('[business-os] ignored volatile local sync transport error', event.error || event.message);
    event.preventDefault();
  });
  window.addEventListener('message', (event) => {
    if (event.data?.type === 'ctox-business-os-command') {
      handleModuleCommand(event);
    }
  });
  window.addEventListener('ctox-business-os-modules-changed', async (event) => {
    console.log('[business-os] modules changed event received:', event.detail);
    await refreshModules();
  });
  window.addEventListener('hashchange', () => {
    if (state.navTransitioning) return;
    const id = currentHashModuleId();
    if (id) openModule(id);
  });
  document.querySelector('[data-open-settings]')?.addEventListener('click', () => {
    openSettingsDrawer();
  });
  document.querySelector('[data-shell-ctox]')?.addEventListener('click', (event) => {
    event.preventDefault();
    openModule('ctox');
  });
  document.querySelector('[data-shell-start]')?.addEventListener('click', (event) => {
    toggleStartMenu(event);
  });
  els.showDesktop?.addEventListener('click', () => openDesktop());
  els.backButton?.addEventListener('click', () => navigateHistory('back'));
  els.forwardButton?.addEventListener('click', () => navigateHistory('forward'));
  els.host?.addEventListener('click', (event) => {
    const homeButton = event.target.closest('[data-module-home]');
    if (homeButton) {
      event.preventDefault();
      openDesktop();
      return;
    }
    const sourceButton = event.target.closest('[data-module-source]');
    if (sourceButton) {
      event.preventDefault();
      const moduleId = sourceButton.dataset.moduleSource || state.activeModule?.id;
      if (moduleId) openModuleSourceEditor(moduleId);
    }
  });
  els.host?.addEventListener('change', async (event) => {
    const select = event.target.closest('[data-module-version-select]');
    if (select) {
      const moduleId = select.dataset.moduleVersionSelect;
      const snapshotId = select.value;
      if (!snapshotId) return;

      const moduleName = moduleDisplayTitleFor(moduleId);
      const confirmMsg = shellLang() === 'de'
        ? `Möchtest du das Modul "${moduleName}" wirklich auf diese Version zurücksetzen?`
        : `Do you really want to rollback module "${moduleName}" to this version?`;

      if (!confirm(confirmMsg)) {
        select.value = ''; // Reset select to placeholder
        return;
      }

      try {
        setStatus(shellLang() === 'de' ? 'Setze Version zurück...' : 'Rolling back version...');
        select.disabled = true;

        await dispatchShellModuleCommand({
          commandType: 'ctox.source.rollback_snapshot',
          moduleId,
          recordId: `${moduleId}:snapshots`,
          payload: { module_id: moduleId, snapshot_id: snapshotId },
          source: 'business-os-shell',
        });

        // Update module revision to bust cache
        if (!state.moduleRevisions) {
          state.moduleRevisions = {};
        }
        state.moduleRevisions[moduleId] = Date.now();

        // Remove from schemaRegistrations to force schema re-import
        state.schemaRegistrations.delete(moduleId);

        setStatus(shellLang() === 'de' ? 'Erfolgreich zurückgesetzt!' : 'Successfully rolled back!');

        // Force reload the module
        await openModule(moduleId, { force: true });
      } catch (error) {
        console.error('[business-os] rollback failed:', error);
        setStatus((shellLang() === 'de' ? 'Fehler beim Zurücksetzen: ' : 'Rollback failed: ') + (error?.message || error), true);
      } finally {
        select.disabled = false;
        select.value = ''; // Reset select to placeholder
      }
    }
  });
  els.ctoxWarning?.addEventListener('click', (event) => {
    event.preventDefault();
    event.stopPropagation();
    openSettingsDrawer({ initialTab: 'runtime' });
  });
  els.accountButton?.addEventListener('click', openAccountDrawer);
  els.languageSelect?.addEventListener('change', () => {
    applyShellLanguage(els.languageSelect.value);
    syncHeaderControls();
    renderShellCtoxWarning(state.ctoxHealth);
    postCurrentPreferencesToModule();
  });
  els.themeSelect?.addEventListener('change', () => {
    applyShellTheme(els.themeSelect.value);
    syncHeaderControls();
    postCurrentPreferencesToModule();
  });
  els.shellStyleSelect?.addEventListener('change', () => {
    applyShellStyle(els.shellStyleSelect.value);
    syncHeaderControls();
    postCurrentPreferencesToModule();
  });
  els.backdrop?.addEventListener('click', closeDrawers);
  els.tabs.addEventListener('dragover', (event) => {
    if (!draggedModuleId(event) && !draggedTaskbarPinId(event)) return;
    event.preventDefault();
    els.tabs.classList.add('is-drop-end');
  });
  els.tabs.addEventListener('dragleave', (event) => {
    if (!els.tabs.contains(event.relatedTarget)) els.tabs.classList.remove('is-drop-end');
  });
  els.tabs.addEventListener('drop', (event) => {
    const pinId = draggedTaskbarPinId(event);
    if (pinId) {
      event.preventDefault();
      els.tabs.classList.remove('is-drop-end');
      moveTaskbarPinBefore(pinId, null);
      return;
    }
    const moduleId = draggedModuleId(event);
    if (!moduleId || moduleId === 'ctox') return;
    event.preventDefault();
    els.tabs.classList.remove('is-drop-end');
    moveModuleToUngrouped(moduleId);
  });
  window.addEventListener('beforeunload', () => {
    if (state.ctoxHealthTimer) window.clearInterval(state.ctoxHealthTimer);
    state.db?.close?.();
  });
  shellColumnResizeSync = setupShellColumnResizing();
}

function openModuleSourceEditor(moduleId) {
  const mod = state.modules.find((entry) => entry.id === moduleId) || state.activeModule;
  if (!mod?.id) return;
  openDesktopApp('code-editor', {
    title: `${moduleDisplayTitle(mod)} Source`,
    width: 1040,
    height: 680,
    args: {
      moduleId: mod.id,
      moduleTitle: moduleDisplayTitle(mod),
    },
  });
}
window.openModuleSourceEditor = openModuleSourceEditor;


async function openSettingsDrawer(options = {}) {
  els.rightDrawer.classList.remove('account-popover');
  els.rightDrawer.classList.add('settings-drawer-open');
  showBackdrop();
  if (state.dataPlaneReadyStatus !== 'ready') {
    els.rightDrawer.replaceChildren();
    const loading = document.createElement('div');
    loading.className = 'drawer-body settings-drawer';
    loading.innerHTML = `
      <h2>CTOX Settings</h2>
      <p>Datenspeicher wird vorbereitet...</p>
      <button type="button" class="ghost" data-close-settings>Schließen</button>
    `;
    loading.querySelector('[data-close-settings]')?.addEventListener('click', closeDrawers);
    els.rightDrawer.append(loading);
  }
  try {
    await waitForDataPlaneReady();
  } catch (error) {
    els.rightDrawer.replaceChildren();
    const failed = document.createElement('div');
    failed.className = 'drawer-body settings-drawer';
    failed.innerHTML = `
      <h2>CTOX Settings</h2>
      <p>Datenspeicher ist noch nicht bereit.</p>
      <p class="muted">${escapeHtml(String(error?.message || error || 'Unbekannter Fehler'))}</p>
      <button type="button" class="ghost" data-close-settings>Schließen</button>
    `;
    failed.querySelector('[data-close-settings]')?.addEventListener('click', closeDrawers);
    els.rightDrawer.append(failed);
    return;
  }
  const { openReactSettings } = await loadReactSettingsModule();
  openReactSettings({
    mount: els.rightDrawer,
    modules: state.modules,
    session: state.session,
    governance: state.governance,
    syncConfig: state.sync?.config,
    sync: createLiveSyncFacade(),
    commandBus: createLiveCommandBusFacade(),
    db: createLiveDbFacade(),
    initialTab: options.initialTab || 'runtime',
    onAccount: openAccountDrawer,
    onClose: closeDrawers,
    onModulesChanged: refreshModules,
  });
}

function isVolatileSyncTransportError(error) {
  const text = String(error?.message || error || '');
  return /cannot send after peer is destroyed|ERR_DATA_CHANNEL|Failure to send data|User-Initiated Abort/i.test(text);
}

function setupShellColumnResizing() {
  const frame = document.querySelector('.workspace-frame');
  if (!frame) return null;

  const leftHandle = document.createElement('div');
  leftHandle.className = 'workspace-col-resizer workspace-col-resizer-left';
  leftHandle.dataset.resizer = 'left';
  leftHandle.setAttribute('role', 'separator');
  leftHandle.setAttribute('aria-orientation', 'vertical');
  leftHandle.setAttribute('aria-label', 'Linke und mittlere Spalte anpassen');
  leftHandle.setAttribute('tabindex', '0');

  const rightHandle = document.createElement('div');
  rightHandle.className = 'workspace-col-resizer workspace-col-resizer-right';
  rightHandle.dataset.resizer = 'right';
  rightHandle.setAttribute('role', 'separator');
  rightHandle.setAttribute('aria-orientation', 'vertical');
  rightHandle.setAttribute('aria-label', 'Mittlere und rechte Spalte anpassen');
  rightHandle.setAttribute('tabindex', '0');

  frame.append(leftHandle, rightHandle);

  let activeWidths = null;
  let persistedRatios = null;
  let dragState = null;
  let resizeRaf = 0;
  let currentLayoutKey = '';

  function layoutKey() {
    return state.activeModule?.id
      ? `${SHELL_COLUMN_LAYOUT_KEY_PREFIX}${state.activeModule.id}`
      : '';
  }

  function readPersistedRatios() {
    const key = layoutKey();
    if (!key || key === currentLayoutKey) return persistedRatios;
    currentLayoutKey = key;
    try {
      persistedRatios = sanitizeColumnLayoutRatios(JSON.parse(localStorage.getItem(key) || 'null'));
    } catch {
      persistedRatios = null;
    }
    return persistedRatios;
  }

  function isResizableLayout() {
    if (!state.activeModule || moduleUsesFullWorkspace(state.activeModule)) return false;
    if (document.body.dataset.moduleLoading) return false;
    if (document.body.dataset.authState === 'locked') return false;
    return Boolean(readGridTrackPixels(frame));
  }

  function hideHandles() {
    leftHandle.hidden = true;
    rightHandle.hidden = true;
    leftHandle.classList.remove('is-active');
    rightHandle.classList.remove('is-active');
  }

  function showHandles() {
    leftHandle.hidden = false;
    rightHandle.hidden = false;
  }

  function applyWidths(widths) {
    if (!widths) return;
    frame.style.gridTemplateColumns = `${widths.left}px ${widths.center}px ${widths.right}px`;
    updateHandleAria(widths);
  }

  function placeHandles(metrics, widths) {
    if (!metrics || !widths) return;
    leftHandle.style.left = `${Math.round(widths.left + (metrics.gap / 2))}px`;
    rightHandle.style.left = `${Math.round(widths.left + metrics.gap + widths.center + (metrics.gap / 2))}px`;
  }

  function updateHandleAria(widths) {
    if (!widths) return;
    leftHandle.setAttribute('aria-valuemin', String(SHELL_COL_MIN.left));
    leftHandle.setAttribute('aria-valuemax', String(SHELL_COL_SIDE_MAX));
    leftHandle.setAttribute('aria-valuenow', String(Math.round(widths.left)));
    leftHandle.setAttribute('aria-valuetext', `${Math.round(widths.left)} px`);
    rightHandle.setAttribute('aria-valuemin', String(SHELL_COL_MIN.right));
    rightHandle.setAttribute('aria-valuemax', String(SHELL_COL_SIDE_MAX));
    rightHandle.setAttribute('aria-valuenow', String(Math.round(widths.right)));
    rightHandle.setAttribute('aria-valuetext', `${Math.round(widths.right)} px`);
  }

  function persistCurrentLayout() {
    const key = layoutKey();
    const ratios = columnPixelsToRatios(activeWidths);
    if (!key || !ratios) return;
    persistedRatios = ratios;
    currentLayoutKey = key;
    localStorage.setItem(key, JSON.stringify(ratios));
  }

  function syncLayout() {
    if (!isResizableLayout()) {
      frame.style.removeProperty('grid-template-columns');
      hideHandles();
      activeWidths = null;
      return;
    }

    const metrics = getGridMetrics(frame);
    if (!metrics || metrics.trackTotal <= 0) return;

    let nextWidths = readPersistedRatios()
      ? columnRatiosToPixels(persistedRatios, metrics.trackTotal)
      : null;

    if (!nextWidths) {
      nextWidths = clampShellColumns(readGridTrackPixels(frame), metrics.trackTotal);
    }

    if (!nextWidths) return;
    activeWidths = nextWidths;
    applyWidths(activeWidths);
    placeHandles(metrics, activeWidths);
    showHandles();
  }

  function stopDrag() {
    if (!dragState) return;
    dragState = null;
    leftHandle.classList.remove('is-active');
    rightHandle.classList.remove('is-active');
    document.body.classList.remove('is-workspace-col-resizing');
    persistCurrentLayout();
  }

  function startDrag(which, event) {
    if (!isResizableLayout()) return;
    const metrics = getGridMetrics(frame);
    if (!metrics || metrics.trackTotal <= 0) return;
    const initial = activeWidths || clampShellColumns(readGridTrackPixels(frame), metrics.trackTotal);
    if (!initial) return;

    activeWidths = initial;
    dragState = {
      which,
      frameRect: frame.getBoundingClientRect(),
      metrics,
      widths: { ...initial },
    };

    if (which === 'left') leftHandle.classList.add('is-active');
    if (which === 'right') rightHandle.classList.add('is-active');
    document.body.classList.add('is-workspace-col-resizing');
    event.preventDefault();
  }

  function handleDragMove(event) {
    if (!dragState) return;
    const { which, frameRect, metrics, widths } = dragState;
    const pointerX = event.clientX - frameRect.left - metrics.padLeft;
    const boundedX = clampNumber(pointerX, 0, metrics.contentWidth);

    if (which === 'left') {
      const right = widths.right;
      const maxLeft = Math.max(
        SHELL_COL_MIN.left,
        Math.min(SHELL_COL_SIDE_MAX, metrics.trackTotal - right - SHELL_COL_MIN.center)
      );
      const left = clampNumber(boundedX - (metrics.gap / 2), SHELL_COL_MIN.left, maxLeft);
      activeWidths = clampShellColumns({ left, center: metrics.trackTotal - left - right, right }, metrics.trackTotal);
    } else {
      const left = widths.left;
      const maxRight = Math.max(
        SHELL_COL_MIN.right,
        Math.min(SHELL_COL_SIDE_MAX, metrics.trackTotal - left - SHELL_COL_MIN.center)
      );
      const right = clampNumber(metrics.contentWidth - boundedX - (metrics.gap / 2), SHELL_COL_MIN.right, maxRight);
      activeWidths = clampShellColumns({ left, center: metrics.trackTotal - left - right, right }, metrics.trackTotal);
    }

    if (!activeWidths) return;
    applyWidths(activeWidths);
    placeHandles(metrics, activeWidths);
  }

  function handleResize() {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0;
      syncLayout();
    });
  }

  function handleKeyboardResize(which, event) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    if (!isResizableLayout()) return;
    const metrics = getGridMetrics(frame);
    if (!metrics || metrics.trackTotal <= 0) return;
    const current = activeWidths || clampShellColumns(readGridTrackPixels(frame), metrics.trackTotal);
    if (!current) return;

    const step = event.shiftKey ? 64 : 24;
    let left = current.left;
    let right = current.right;
    const maxLeft = Math.max(
      SHELL_COL_MIN.left,
      Math.min(SHELL_COL_SIDE_MAX, metrics.trackTotal - right - SHELL_COL_MIN.center)
    );
    const maxRight = Math.max(
      SHELL_COL_MIN.right,
      Math.min(SHELL_COL_SIDE_MAX, metrics.trackTotal - left - SHELL_COL_MIN.center)
    );

    if (which === 'left') {
      if (event.key === 'Home') left = SHELL_COL_MIN.left;
      else if (event.key === 'End') left = maxLeft;
      else left += event.key === 'ArrowLeft' ? -step : step;
      left = clampNumber(left, SHELL_COL_MIN.left, maxLeft);
    } else {
      if (event.key === 'Home') right = SHELL_COL_MIN.right;
      else if (event.key === 'End') right = maxRight;
      else right += event.key === 'ArrowLeft' ? step : -step;
      right = clampNumber(right, SHELL_COL_MIN.right, maxRight);
    }

    activeWidths = clampShellColumns({ left, center: metrics.trackTotal - left - right, right }, metrics.trackTotal);
    if (!activeWidths) return;
    applyWidths(activeWidths);
    placeHandles(metrics, activeWidths);
    persistCurrentLayout();
    event.preventDefault();
  }

  leftHandle.addEventListener('pointerdown', (event) => startDrag('left', event));
  rightHandle.addEventListener('pointerdown', (event) => startDrag('right', event));
  leftHandle.addEventListener('keydown', (event) => handleKeyboardResize('left', event));
  rightHandle.addEventListener('keydown', (event) => handleKeyboardResize('right', event));
  window.addEventListener('pointermove', handleDragMove);
  window.addEventListener('pointerup', stopDrag);
  window.addEventListener('pointercancel', stopDrag);
  window.addEventListener('blur', stopDrag);
  window.addEventListener('resize', handleResize);

  syncLayout();
  return syncLayout;
}

function readGridTrackPixels(gridEl) {
  if (!gridEl) return null;
  const tracks = String(getComputedStyle(gridEl).gridTemplateColumns || '')
    .split(/\s+/)
    .map((part) => Number.parseFloat(part))
    .filter((value) => Number.isFinite(value) && value > 0);
  if (tracks.length < 3) return null;
  return {
    left: tracks[0],
    center: tracks[1],
    right: tracks[2],
  };
}

function getGridMetrics(gridEl) {
  if (!gridEl) return null;
  const styles = getComputedStyle(gridEl);
  const gap = Number.parseFloat(styles.columnGap || styles.gap || '0') || 0;
  const padLeft = Number.parseFloat(styles.paddingLeft || '0') || 0;
  const padRight = Number.parseFloat(styles.paddingRight || '0') || 0;
  const contentWidth = Math.max(0, gridEl.clientWidth - padLeft - padRight);
  return {
    gap,
    padLeft,
    contentWidth,
    trackTotal: Math.max(0, contentWidth - (gap * 2)),
  };
}

function clampShellColumns(widths, trackTotal) {
  if (!widths || !Number.isFinite(trackTotal) || trackTotal <= 0) return null;
  const maxLeft = Math.max(
    SHELL_COL_MIN.left,
    Math.min(SHELL_COL_SIDE_MAX, trackTotal - SHELL_COL_MIN.center - SHELL_COL_MIN.right)
  );
  const maxRight = Math.max(
    SHELL_COL_MIN.right,
    Math.min(SHELL_COL_SIDE_MAX, trackTotal - SHELL_COL_MIN.center - SHELL_COL_MIN.left)
  );

  let left = clampNumber(Number(widths.left) || SHELL_COL_MIN.left, SHELL_COL_MIN.left, maxLeft);
  let right = clampNumber(Number(widths.right) || SHELL_COL_MIN.right, SHELL_COL_MIN.right, maxRight);
  let center = trackTotal - left - right;

  if (center < SHELL_COL_MIN.center) {
    let shortage = SHELL_COL_MIN.center - center;
    const reduceLeft = Math.min(shortage, left - SHELL_COL_MIN.left);
    left -= reduceLeft;
    shortage -= reduceLeft;
    if (shortage > 0) {
      const reduceRight = Math.min(shortage, right - SHELL_COL_MIN.right);
      right -= reduceRight;
    }
    center = trackTotal - left - right;
  }

  if (center < SHELL_COL_MIN.center) return null;
  return {
    left: Math.round(left),
    center: Math.round(center),
    right: Math.round(right),
  };
}

function sanitizeColumnLayoutRatios(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const left = Number(raw.left);
  const center = Number(raw.center);
  const right = Number(raw.right);
  if (![left, center, right].every(Number.isFinite)) return null;
  if (left <= 0 || center <= 0 || right <= 0) return null;
  const sum = left + center + right;
  if (sum <= 0) return null;
  return {
    left: left / sum,
    center: center / sum,
    right: right / sum,
  };
}

function columnPixelsToRatios(widths) {
  if (!widths) return null;
  const left = Number(widths.left) || 0;
  const center = Number(widths.center) || 0;
  const right = Number(widths.right) || 0;
  const sum = left + center + right;
  if (sum <= 0) return null;
  return {
    left: Number((left / sum).toFixed(6)),
    center: Number((center / sum).toFixed(6)),
    right: Number((right / sum).toFixed(6)),
  };
}

function columnRatiosToPixels(ratios, trackTotal) {
  const safe = sanitizeColumnLayoutRatios(ratios);
  if (!safe) return null;
  return clampShellColumns({
    left: safe.left * trackTotal,
    center: safe.center * trackTotal,
    right: safe.right * trackTotal,
  }, trackTotal);
}

function clampNumber(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function shellLang() {
  return document.documentElement.lang === 'en' ? 'en' : 'de';
}

function shellText(key) {
  return shellMessages[shellLang()]?.[key] || shellMessages.en[key] || key;
}

function updateSyncDiagnostics(snapshot) {
  state.syncDiagnostics = snapshot;
  if (hasWebRtcConnectedCollection(snapshot)) markBootTiming('firstWebRtcConnectedMs');
  window.ctoxBusinessOsSyncDiagnostics = snapshot;
  scheduleSyncRecoveryRepairIfNeeded(snapshot);
  refreshOpenSyncDiagnosticsDrawer();
  window.dispatchEvent(new CustomEvent('ctox-business-os-sync-diagnostics', {
    detail: snapshot,
  }));
}

function hasWebRtcConnectedCollection(snapshot) {
  if (!snapshot || snapshot.mode !== 'webrtc') return false;
  return Object.values(snapshot.collections || {}).some((collection) => {
    return collection?.connectionStatus === 'connected'
      || collection?.status === 'connected'
      || Boolean(collection?.connectedAt)
      || Boolean(collection?.initialReplicationAt);
  });
}

function markBootTiming(key) {
  if (!Object.prototype.hasOwnProperty.call(state.bootTimings, key)) return;
  if (state.bootTimings[key] !== null) return;
  state.bootTimings[key] = Math.max(0, Math.round(performance.now() - state.bootTimings.startedAtMs));
}

function serializeBootTimings() {
  return {
    startedAt: state.bootTimings.startedAt,
    shellVisibleMs: state.bootTimings.shellVisibleMs,
    firstWebRtcConnectedMs: state.bootTimings.firstWebRtcConnectedMs,
    firstAdvancedStatusHealthyMs: state.bootTimings.firstAdvancedStatusHealthyMs,
  };
}

function scheduleSyncRecoveryRepairIfNeeded(snapshot) {
  if (!hasRecoverableWebRtcFailure(snapshot)) {
    clearSyncRecoveryRepairTimer();
    return;
  }
  if (syncRecoveryRepairTimer || syncRecoveryRepairRunning) return;
  syncRecoveryRepairTimer = window.setTimeout(() => {
    syncRecoveryRepairTimer = null;
    repairRecoveringDataPlane().catch((error) => {
      console.error('[business-os] automatic RxDB/WebRTC data-plane repair failed', error);
    });
  }, SYNC_RECOVERY_REPAIR_DELAY_MS);
}

function clearSyncRecoveryRepairTimer() {
  if (!syncRecoveryRepairTimer) return;
  window.clearTimeout(syncRecoveryRepairTimer);
  syncRecoveryRepairTimer = null;
}

function hasRecoverableWebRtcFailure(snapshot) {
  if (!snapshot || snapshot.mode !== 'webrtc') return false;
  const collections = Object.values(snapshot.collections || {});
  const hadEstablishedConnection = collections.some((collection) => collection?.connectedAt || collection?.initialReplicationAt);
  if (!hadEstablishedConnection && !state.advancedStatusEverHealthy) return false;
  const hasDataPlaneError = collections.some((collection) => collection?.lastError);
  if (!hasDataPlaneError) return false;
  if (snapshot.phase === 'reconnecting') return true;
  return collections.some((collection) => collection?.connectionStatus === 'reconnecting');
}

async function repairRecoveringDataPlane() {
  if (new URLSearchParams(window.location.search).has('rxdbSmoke')) {
    console.info('[business-os] smoke mode keeps the local RxDB cache intact; sync runtime handles reconnect');
    return;
  }
  if (syncRecoveryRepairRunning || !state.syncConfig || !state.db) return;
  syncRecoveryRepairRunning = true;
  try {
    console.warn('[business-os] repairing RxDB/WebRTC data plane after stalled reconnect');
    setStatus('RxDB/WebRTC wird neu verbunden');
    await repairBusinessDataPlane(state.syncConfig);
    await startCriticalSyncCollections();
    if (state.activeModule) startModuleSync(state.activeModule);
  } finally {
    syncRecoveryRepairRunning = false;
  }
}

async function startCriticalSyncCollections() {
  const collections = [
    'business_module_catalog',
    'ctox_runtime_settings',
    'business_commands',
    'ctox_queue_tasks',
    'desktop_files',
    'desktop_file_chunks',
  ];
  for (const collection of collections) {
    try {
      await state.sync?.startCollection?.(collection);
      await waitForCriticalSyncCollection(collection);
    } catch (error) {
      console.warn(`[business-os] critical sync collection ${collection} did not start during repair`, error);
    }
  }
}

async function waitForCriticalSyncCollection(collection, timeoutMs = 18000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    if (isCriticalSyncCollectionReady(collection)) return true;
    await delay(250);
  }
  const diagnostics = state.syncDiagnostics?.collections?.[collection] || null;
  console.warn('[business-os] critical sync collection did not become ready before continuing', {
    collection,
    connectionStatus: diagnostics?.connectionStatus || diagnostics?.status || null,
    activePeerCount: diagnostics?.frameTransport?.activePeerCount ?? null,
    sentFrames: diagnostics?.frameTransport?.sentFrames ?? null,
    receivedFrames: diagnostics?.frameTransport?.receivedFrames ?? null,
    lastLifecycleEvent: diagnostics?.lastLifecycleEvent || null,
    lastError: diagnostics?.lastError || null,
  });
  return false;
}

function isCriticalSyncCollectionReady(collection) {
  const diagnostics = state.syncDiagnostics?.collections?.[collection];
  if (!diagnostics) return false;
  const status = diagnostics.connectionStatus || diagnostics.status || '';
  if (['connected', 'running', 'reused'].includes(status)) return true;
  if (diagnostics.connectedAt || diagnostics.initialReplicationAt) return true;
  if (diagnostics.initialReplicationState === 'complete') return true;
  const transport = diagnostics.frameTransport || {};
  return Number(transport.activePeerCount || 0) > 0
    && (Number(transport.sentFrames || 0) > 0 || Number(transport.receivedFrames || 0) > 0);
}

function scheduleCriticalSyncWarmup() {
  const run = () => {
    startCriticalSyncCollections().catch((error) => {
      console.warn('[business-os] critical sync warmup failed', error);
    });
  };
  if ('requestIdleCallback' in window) {
    window.requestIdleCallback(run, { timeout: 1000 });
  } else {
    window.setTimeout(run, 0);
  }
}

async function buildAdvancedStatusSnapshot(options = {}) {
  const diagnostics = state.syncDiagnostics || null;
  const collections = diagnostics?.collections || {};
  const collectionValues = Object.values(collections);
  if (!syncModule && collectionValues.some((item) => item?.lastError)) {
    try {
      await loadSyncModule();
    } catch (error) {
      console.warn('[business-os] advanced status sync classifier unavailable', error);
    }
  }
  const requiredCollections = Array.isArray(options.requiredCollections) && options.requiredCollections.length
    ? options.requiredCollections.filter((collection) => typeof collection === 'string' && collection.trim())
    : [
        'business_module_catalog',
        'ctox_runtime_settings',
        'business_commands',
        'ctox_queue_tasks',
        'desktop_files',
        'desktop_file_chunks',
      ];
  const failedCollections = collectionValues
    .filter((item) => ['failed', 'error', 'stopped'].includes(item?.connectionStatus || item?.status))
    .map((item) => item.collection)
    .filter(Boolean);
  const collectionErrors = collectionValues
    .filter((item) => item?.lastError)
    .map((item) => serializeAdvancedStatusCollectionError(item))
    .filter(Boolean);
  const checkpointErrors = collectionErrors
    .filter((error) => error?.name === 'CtoxCheckpointProtocolError');
  const schemaErrors = collectionErrors
    .filter((error) => error?.name === 'CtoxSchemaProtocolError');
  const replicationErrors = collectionErrors
    .filter((error) => error?.name === 'CtoxReplicationIoError');
  const serviceErrors = serializeAdvancedStatusServiceErrors(diagnostics?.serviceErrors || diagnostics?.health?.errors || []);
  const lifecycleEvents = collectionValues
    .filter((item) => item?.lastLifecycleEvent)
    .map((item) => serializeAdvancedStatusLifecycleEvent(item))
    .filter(Boolean);
  const fileIntegrityErrors = state.fileIntegrityDiagnostics
    .map((item) => serializeAdvancedStatusFileIntegrityError(item))
    .filter(Boolean);
  const reconnectingCollections = collectionValues
    .filter((item) => item?.connectionStatus === 'reconnecting' || item?.status === 'reconnecting')
    .map((item) => item.collection)
    .filter(Boolean);
  const requiredCollectionSet = new Set(requiredCollections);
  const requiredReconnectingCollections = reconnectingCollections
    .filter((collection) => requiredCollectionSet.has(collection));
  const optionalReconnectingCollections = reconnectingCollections
    .filter((collection) => !requiredCollectionSet.has(collection));
  const frameTransport = buildAdvancedStatusFrameTransport(collectionValues, requiredCollections);
  const peerSessions = collectionValues
    .filter((item) => item?.remotePeerSession)
    .map((item) => ({
      collection: item.collection || null,
      protocol: item.remoteProtocol || null,
      capabilities: Array.isArray(item.remoteCapabilities) ? item.remoteCapabilities : [],
      peerSession: item.remotePeerSession,
      generation: Number(item.peerGeneration || 0),
      previousPeerSession: item.previousPeerSession || null,
      checkpoint: sanitizeAdvancedStatusRemoteCheckpoint(item.remoteCheckpoint || null),
      generationChangedAt: item.peerGenerationChangedAt || null,
      seenAt: item.peerSessionSeenAt || null,
    }));
  const bodyDataset = { ...document.body?.dataset };
  const counts = options.includeCounts === false ? null : await collectAdvancedStatusCounts();
  const requiredCollectionEvidence = await collectAdvancedStatusRequiredEvidence(requiredCollections);
  const initialSync = buildAdvancedStatusInitialSync(requiredCollections, collections);
  const missingRequiredCollections = requiredCollections.filter((collection) => !isRequiredCollectionReady({
    collection,
    diagnostics: collections[collection] || null,
    evidence: requiredCollectionEvidence[collection] || null,
  }));
  const checks = {
    authenticated: Boolean(state.session?.authenticated),
    shellLoaded: state.modules.length > 0,
    activeModuleLoaded: Boolean(state.activeModule?.id),
    workspaceNotLoading: !bodyDataset.moduleLoading,
    dataPlaneWebrtc: state.sync?.mode === 'webrtc' && diagnostics?.mode === 'webrtc',
    rxdbRuntimeAppLocal: state.db?.runtime?.name === 'ctox-rxdb-js'
      && state.db?.runtime?.source === 'app-local'
      && state.db?.runtime?.packageManager === 'none',
    moduleCatalogAvailable: state.modules.length > 0 && (counts === null || Number(counts.business_module_catalog || 0) > 0),
    requiredCollectionsConnected: missingRequiredCollections.length === 0,
    requiredCollectionsInitialSyncComplete: initialSync.missingInitialReplication.length === 0,
    requiredCollectionsCheckpointEpochAdvertised: initialSync.missingCheckpointEpoch.length === 0,
    noCheckpointProtocolErrors: checkpointErrors.length === 0,
    noSchemaProtocolErrors: schemaErrors.length === 0,
    noReplicationIoErrors: replicationErrors.length === 0,
    noFailedCollections: failedCollections.length === 0,
    noStalledReconnect: requiredReconnectingCollections.length === 0,
    frameTransportRealtimeHealthy: frameTransport.healthy,
    noAutomaticRepairRunning: !syncRecoveryRepairRunning,
  };
  const ok = Object.values(checks).every(Boolean);
  if (ok) markBootTiming('firstAdvancedStatusHealthyMs');
  if (ok) state.advancedStatusEverHealthy = true;
  return {
    version: 'business-os-advanced-status-v1',
    build: APP_BUILD,
    ok,
    checkedAt: new Date().toISOString(),
    checks,
    failures: Object.entries(checks).filter(([, passed]) => !passed).map(([name]) => name),
    shell: {
      readyState: document.readyState,
      bodyDataset,
      activeModule: state.activeModule?.id || null,
      moduleCount: state.modules.length,
      moduleIds: state.modules.map((mod) => mod.id).filter(Boolean),
      statusText: document.querySelector('[data-status]')?.textContent || '',
      visibleTextSample: (document.body?.innerText || '').slice(0, 500),
      bootTimings: serializeBootTimings(),
    },
    rxdbRuntime: sanitizeRxdbRuntime(state.db?.runtime || state.db?.rxdb?.__ctoxRuntime || null),
    sync: {
      mode: state.sync?.mode || null,
      phase: diagnostics?.phase || null,
      syncRoom: diagnostics?.syncRoom || null,
      signalingUrls: diagnostics?.signalingUrls || [],
      iceServersConfigured: diagnostics?.iceServersConfigured || 0,
      protocol: diagnostics?.protocol || null,
      capabilities: Array.isArray(diagnostics?.capabilities) ? diagnostics.capabilities : [],
      peerSessions,
      collectionTotal: collectionValues.length,
      failedCollections,
      collectionErrors,
      checkpointErrors,
      schemaErrors,
      replicationErrors,
      reconnectingCollections,
      requiredReconnectingCollections,
      optionalReconnectingCollections,
      frameTransport,
      lifecycleEvents,
      nativePeerRecovery: sanitizeAdvancedStatusNativePeerRecovery(diagnostics?.nativePeerRecovery || diagnostics?.recovery || null),
      requiredCollections,
      requiredCollectionEvidence,
      missingRequiredCollections,
      initialSync,
      lastError: diagnostics?.lastError || null,
      lastLifecycleEvent: diagnostics?.lastLifecycleEvent || null,
    },
    health: {
      errorTotal: collectionErrors.length + fileIntegrityErrors.length + serviceErrors.length,
      collectionErrors,
      fileIntegrityErrors,
      serviceErrors,
      lastError: collectionErrors[0] || fileIntegrityErrors[0] || serviceErrors[0] || null,
    },
    fileIntegrity: {
      errorTotal: fileIntegrityErrors.length,
      errors: fileIntegrityErrors,
      lastError: fileIntegrityErrors[0] || null,
    },
    data: { counts },
  };
}

function buildAdvancedStatusFrameTransport(collectionValues, requiredCollections = []) {
  const requiredSet = new Set(requiredCollections);
  const entries = collectionValues
    .map((item) => sanitizeAdvancedStatusFrameTransportEntry(item?.collection, item?.frameTransport))
    .filter(Boolean);
  const byCollection = new Map(entries.map((entry) => [entry.collection, entry]));
  const missingCollections = requiredCollections.filter((collection) => !byCollection.has(collection));
  const unhealthyCollections = [];
  const thresholds = {
    maxAckLagMs: 5000,
    maxPendingAcks: 16,
    maxActiveTransfers: 32,
    maxQueueDepth: 128,
    maxHighPriorityQueueDepth: 32,
  };
  for (const entry of entries) {
    const reasons = [];
    if (entry.protocol !== 'ctox-rxdb-frame-v1') reasons.push('protocol');
    if (requiredSet.has(entry.collection) && entry.activePeerCount < 1) reasons.push('no-active-peer');
    if (entry.pendingAcks > thresholds.maxPendingAcks) reasons.push('pending-acks');
    if (entry.activeTransfers > thresholds.maxActiveTransfers) reasons.push('active-transfers');
    if (entry.priorityQueueDepth > thresholds.maxQueueDepth) reasons.push('queue-depth');
    if (entry.highPriorityQueueDepth > thresholds.maxHighPriorityQueueDepth) reasons.push('high-priority-queue-depth');
    if (entry.lastAckLagMs > thresholds.maxAckLagMs) reasons.push('ack-lag');
    if (entry.sendBufferHighWater > 0 && entry.lastBufferedAmount >= entry.sendBufferHighWater) reasons.push('datachannel-backpressure');
    if (reasons.length > 0) {
      unhealthyCollections.push({
        collection: entry.collection,
        reasons,
        required: requiredSet.has(entry.collection),
      });
    }
  }
  for (const collection of missingCollections) {
    unhealthyCollections.push({
      collection,
      reasons: ['missing-frame-transport-status'],
      required: true,
    });
  }
  const totals = entries.reduce((acc, entry) => {
    acc.activePeerCount += entry.activePeerCount;
    acc.activeTransfers += entry.activeTransfers;
    acc.pendingAcks += entry.pendingAcks;
    acc.incomingTransfers += entry.incomingTransfers;
    acc.sentFrames += entry.sentFrames;
    acc.sentBytes += entry.sentBytes;
    acc.receivedFrames += entry.receivedFrames;
    acc.receivedBytes += entry.receivedBytes;
    acc.retryCount += entry.retryCount;
    acc.resumeRequestCount += entry.resumeRequestCount;
    acc.resumeAckCount += entry.resumeAckCount;
    acc.backpressureWaitCount += entry.backpressureWaitCount;
    acc.queuedFrames += entry.queuedFrames;
    acc.priorityQueueDepth += entry.priorityQueueDepth;
    acc.highPriorityQueueDepth += entry.highPriorityQueueDepth;
    acc.normalPriorityQueueDepth += entry.normalPriorityQueueDepth;
    acc.lowPriorityQueueDepth += entry.lowPriorityQueueDepth;
    acc.lastAckLagMs = Math.max(acc.lastAckLagMs, entry.lastAckLagMs);
    acc.lastBufferedAmount = Math.max(acc.lastBufferedAmount, entry.lastBufferedAmount);
    return acc;
  }, {
    activePeerCount: 0,
    activeTransfers: 0,
    pendingAcks: 0,
    incomingTransfers: 0,
    sentFrames: 0,
    sentBytes: 0,
    receivedFrames: 0,
    receivedBytes: 0,
    retryCount: 0,
    resumeRequestCount: 0,
    resumeAckCount: 0,
    backpressureWaitCount: 0,
    queuedFrames: 0,
    priorityQueueDepth: 0,
    highPriorityQueueDepth: 0,
    normalPriorityQueueDepth: 0,
    lowPriorityQueueDepth: 0,
    lastAckLagMs: 0,
    lastBufferedAmount: 0,
  });
  return {
    protocol: 'ctox-rxdb-frame-v1',
    healthy: unhealthyCollections.length === 0,
    thresholds,
    collectionTotal: entries.length,
    requiredCollectionTotal: requiredCollections.length,
    missingCollections,
    unhealthyCollections,
    totals,
    entries,
    collections: entries,
  };
}

function sanitizeAdvancedStatusFrameTransportEntry(collection, value) {
  if (!value || typeof value !== 'object') return null;
  const numberField = (key) => Number.isFinite(Number(value[key])) ? Number(value[key]) : 0;
  const stringField = (key, fallback = null, maxLength = 120) => {
    const raw = value[key];
    return typeof raw === 'string' && raw.trim() ? raw.slice(0, maxLength) : fallback;
  };
  return {
    collection: stringField('collection', collection || null, 120),
    topic: stringField('topic', null, 180),
    protocol: stringField('protocol', 'ctox-rxdb-frame-v1', 80),
    maxInlineFrameBytes: numberField('maxInlineFrameBytes'),
    maxChunkChars: numberField('maxChunkChars'),
    maxTransferBytes: numberField('maxTransferBytes'),
    ackWindow: numberField('ackWindow'),
    sendBufferHighWater: numberField('sendBufferHighWater'),
    sendBufferLowWater: numberField('sendBufferLowWater'),
    activePeerCount: numberField('activePeerCount'),
    activeTransfers: numberField('activeTransfers'),
    pendingAcks: numberField('pendingAcks'),
    incomingTransfers: numberField('incomingTransfers'),
    completedAckCacheSize: numberField('completedAckCacheSize'),
    sentFrames: numberField('sentFrames'),
    sentBytes: numberField('sentBytes'),
    receivedFrames: numberField('receivedFrames'),
    receivedBytes: numberField('receivedBytes'),
    retryCount: numberField('retryCount'),
    resumeRequestCount: numberField('resumeRequestCount'),
    resumeAckCount: numberField('resumeAckCount'),
    backpressureWaitCount: numberField('backpressureWaitCount'),
    queuedFrames: numberField('queuedFrames'),
    sentScheduledFrames: numberField('sentScheduledFrames'),
    priorityQueueDepth: numberField('priorityQueueDepth'),
    highPriorityQueueDepth: numberField('highPriorityQueueDepth'),
    normalPriorityQueueDepth: numberField('normalPriorityQueueDepth'),
    lowPriorityQueueDepth: numberField('lowPriorityQueueDepth'),
    lastSendPriority: stringField('lastSendPriority', 'normal', 20),
    lastAckLagMs: numberField('lastAckLagMs'),
    lastBufferedAmount: numberField('lastBufferedAmount'),
    pullInProgress: value.pullInProgress === true,
    pushInProgress: value.pushInProgress === true,
    rtcConnections: sanitizeAdvancedStatusRtcConnections(value.rtcConnections),
    recentRtcEvents: sanitizeAdvancedStatusRtcEvents(value.recentRtcEvents),
    connectionStates: sanitizeAdvancedStatusConnectionStates(value.connectionStates),
    rtcConnectionPool: sanitizeAdvancedStatusRtcPool(value.rtcConnectionPool),
    updatedAtMs: numberField('updatedAtMs'),
    observedAt: stringField('observedAt', null, 80),
  };
}

function sanitizeAdvancedStatusRtcConnections(value) {
  if (!Array.isArray(value)) return [];
  return value.slice(-8).map((entry) => ({
    peerId: advancedStatusString(entry?.peerId, 80),
    collection: advancedStatusString(entry?.collection, 120),
    ageMs: advancedStatusNumber(entry?.ageMs),
    signalingState: advancedStatusString(entry?.signalingState, 40),
    iceConnectionState: advancedStatusString(entry?.iceConnectionState, 40),
    iceGatheringState: advancedStatusString(entry?.iceGatheringState, 40),
    connectionState: advancedStatusString(entry?.connectionState, 40),
    channelReadyState: advancedStatusString(entry?.channelReadyState, 40),
    pendingCandidates: advancedStatusNumber(entry?.pendingCandidates),
    hasLocalDescription: entry?.hasLocalDescription === true,
    hasRemoteDescription: entry?.hasRemoteDescription === true,
    localCandidateTypes: sanitizeAdvancedStatusCountMap(entry?.localCandidateTypes),
    remoteCandidateTypes: sanitizeAdvancedStatusCountMap(entry?.remoteCandidateTypes),
    signal: sanitizeAdvancedStatusSignalStats(entry?.signal),
    lastError: entry?.lastError ? sanitizeAdvancedStatusTypedError(entry.lastError) : null,
  }));
}

function sanitizeAdvancedStatusRtcEvents(value) {
  if (!Array.isArray(value)) return [];
  return value.slice(-16).map((entry) => ({
    atMs: advancedStatusNumber(entry?.atMs),
    event: advancedStatusString(entry?.event, 80),
    peerId: advancedStatusString(entry?.peerId, 80),
    collection: advancedStatusString(entry?.collection, 120),
    state: advancedStatusString(entry?.state, 80),
    signalingState: advancedStatusString(entry?.signalingState, 80),
    connectionState: advancedStatusString(entry?.connectionState, 80),
    iceConnectionState: advancedStatusString(entry?.iceConnectionState, 80),
    iceGatheringState: advancedStatusString(entry?.iceGatheringState, 80),
    pendingCandidates: advancedStatusNumber(entry?.pendingCandidates),
    ageMs: advancedStatusNumber(entry?.ageMs),
  }));
}

function sanitizeAdvancedStatusConnectionStates(value) {
  if (!Array.isArray(value)) return [];
  return value.slice(-8).map((entry) => ({
    peerId: advancedStatusString(entry?.peerId, 80),
    peerConnectionState: advancedStatusString(entry?.peerConnectionState, 40),
    iceConnectionState: advancedStatusString(entry?.iceConnectionState, 40),
    iceGatheringState: advancedStatusString(entry?.iceGatheringState, 40),
    signalingState: advancedStatusString(entry?.signalingState, 40),
    channelState: advancedStatusString(entry?.channelState, 40),
    channelLabel: advancedStatusString(entry?.channelLabel, 80),
    pendingCandidates: advancedStatusNumber(entry?.pendingCandidates),
  }));
}

function sanitizeAdvancedStatusRtcPool(value) {
  if (!value || typeof value !== 'object') return null;
  return {
    maxConnections: advancedStatusNumber(value.maxConnections),
    activeConnections: advancedStatusNumber(value.activeConnections),
    queuedConnections: advancedStatusNumber(value.queuedConnections),
    criticalActiveConnections: advancedStatusNumber(value.criticalActiveConnections),
    criticalQueuedConnections: advancedStatusNumber(value.criticalQueuedConnections),
  };
}

function sanitizeAdvancedStatusSignalStats(value) {
  if (!value || typeof value !== 'object') return {};
  return {
    offerSent: advancedStatusNumber(value.offerSent),
    offerReceived: advancedStatusNumber(value.offerReceived),
    answerSent: advancedStatusNumber(value.answerSent),
    answerReceived: advancedStatusNumber(value.answerReceived),
    candidateSent: advancedStatusNumber(value.candidateSent),
    candidateReceived: advancedStatusNumber(value.candidateReceived),
    localCandidateComplete: value.localCandidateComplete === true,
    lastLocalCandidateType: advancedStatusString(value.lastLocalCandidateType, 40),
    lastRemoteCandidateType: advancedStatusString(value.lastRemoteCandidateType, 40),
    lastSignalAtMs: advancedStatusNumber(value.lastSignalAtMs),
  };
}

function sanitizeAdvancedStatusCountMap(value) {
  if (!value || typeof value !== 'object') return {};
  const result = {};
  for (const [key, count] of Object.entries(value)) {
    const normalized = advancedStatusString(key, 40);
    if (!normalized) continue;
    result[normalized] = advancedStatusNumber(count);
  }
  return result;
}

function advancedStatusString(value, maxLength = 120) {
  return typeof value === 'string' && value.trim() ? value.slice(0, maxLength) : '';
}

function advancedStatusNumber(value) {
  return Number.isFinite(Number(value)) ? Number(value) : 0;
}

function serializeAdvancedStatusCollectionError(item) {
  const error = item?.lastError;
  if (!error) return null;
  const normalizedError = syncModule?.classifySchemaProtocolError?.(item.collection || null, error)
    || syncModule?.classifyReplicationIoError?.(item.collection || null, error)
    || error;
  const rawCode = typeof normalizedError.code === 'string' ? normalizedError.code.trim() : '';
  const rawName = typeof normalizedError.name === 'string' ? normalizedError.name.trim() : '';
  const rawMessage = typeof normalizedError.message === 'string' ? normalizedError.message.trim() : '';
  const rawPhase = typeof normalizedError.phase === 'string' ? normalizedError.phase.trim() : '';
  const rawSeverity = typeof normalizedError.severity === 'string' ? normalizedError.severity.trim() : '';
  return {
    collection: item.collection || null,
    status: item.connectionStatus || item.status || null,
    name: rawName || 'Error',
    code: rawCode || null,
    phase: rawPhase || null,
    severity: rawSeverity || null,
    retryable: typeof normalizedError.retryable === 'boolean' ? normalizedError.retryable : null,
    expected: typeof normalizedError.expected === 'string' ? normalizedError.expected.slice(0, 120) : null,
    actual: typeof normalizedError.actual === 'string' ? normalizedError.actual.slice(0, 120) : null,
    direction: typeof normalizedError.direction === 'string' ? normalizedError.direction.slice(0, 20) : null,
    upstreamCode: typeof normalizedError.upstreamCode === 'string' ? normalizedError.upstreamCode.slice(0, 40) : null,
    batchSize: normalizedError.batchSize !== null && Number.isFinite(Number(normalizedError.batchSize)) ? Number(normalizedError.batchSize) : null,
    rowCount: normalizedError.rowCount !== null && Number.isFinite(Number(normalizedError.rowCount)) ? Number(normalizedError.rowCount) : null,
    message: rawMessage.slice(0, 240),
  };
}

function sanitizeAdvancedStatusTypedError(error) {
  if (!error) return null;
  const name = typeof error.name === 'string' && error.name.trim() ? error.name.trim() : 'Error';
  const code = typeof error.code === 'string' && error.code.trim() ? error.code.trim() : null;
  const message = typeof error.message === 'string' ? error.message : String(error);
  return {
    name,
    code,
    phase: typeof error.phase === 'string' ? error.phase.slice(0, 80) : null,
    severity: typeof error.severity === 'string' ? error.severity.slice(0, 40) : null,
    retryable: typeof error.retryable === 'boolean' ? error.retryable : null,
    message: message.slice(0, 240),
  };
}

function serializeAdvancedStatusServiceErrors(errors) {
  const list = Array.isArray(errors) ? errors : [errors].filter(Boolean);
  return list
    .map((error) => sanitizeAdvancedStatusTypedError(error))
    .filter(Boolean)
    .slice(0, 20);
}

function sanitizeAdvancedStatusNativePeerRecovery(value) {
  if (!value || typeof value !== 'object') return null;
  return {
    code: typeof value.code === 'string' ? value.code.slice(0, 80) : null,
    action: typeof value.action === 'string' ? value.action.slice(0, 80) : null,
    status: typeof value.status === 'string' ? value.status.slice(0, 80) : null,
    collection: typeof value.collection === 'string' ? value.collection.slice(0, 120) : null,
    message: typeof value.message === 'string' ? value.message.slice(0, 240) : null,
    updatedAt: typeof value.updatedAt === 'string' ? value.updatedAt : null,
  };
}

function serializeAdvancedStatusLifecycleEvent(item) {
  const event = item?.lastLifecycleEvent;
  if (!event) return null;
  return {
    collection: item.collection || null,
    status: item.connectionStatus || item.status || null,
    name: typeof event.name === 'string' && event.name.trim() ? event.name.trim() : 'CtoxWebRtcPeerLifecycleEvent',
    code: typeof event.code === 'string' ? event.code : null,
    phase: typeof event.phase === 'string' ? event.phase : null,
    severity: typeof event.severity === 'string' ? event.severity : null,
    retryable: event.retryable === true,
    message: String(event.message || '').slice(0, 240),
    reconnectingSince: item.reconnectingSince || null,
  };
}

function reportFileIntegrityError(source, error, details = {}) {
  const code = typeof error?.code === 'string' ? error.code : '';
  const phase = typeof error?.phase === 'string' ? error.phase : '';
  const name = typeof error?.name === 'string' ? error.name : 'Error';
  if (!code && name !== 'CtoxFileChunkIntegrityError') return;
  state.fileIntegrityDiagnostics.unshift({
    source: String(source || 'business-os').slice(0, 80),
    name,
    code: code || null,
    phase: phase || null,
    message: String(error?.message || error || '').slice(0, 240),
    details: sanitizeFileIntegrityDetails(details),
    observedAt: new Date().toISOString(),
  });
  state.fileIntegrityDiagnostics = state.fileIntegrityDiagnostics.slice(0, 10);
}

function serializeAdvancedStatusFileIntegrityError(item) {
  if (!item) return null;
  return {
    source: item.source || null,
    name: item.name || 'CtoxFileChunkIntegrityError',
    code: item.code || null,
    phase: item.phase || null,
    message: String(item.message || '').slice(0, 240),
    details: sanitizeFileIntegrityDetails(item.details || {}),
    observedAt: item.observedAt || null,
  };
}

function sanitizeFileIntegrityDetails(details = {}) {
  const clean = {};
  for (const [key, value] of Object.entries(details || {})) {
    if (!['appId', 'fileId', 'mimeType', 'contentState', 'contentGenerationId', 'contentHashScheme'].includes(key)) continue;
    clean[key] = String(value || '').slice(0, 160);
  }
  return clean;
}

function sanitizeAdvancedStatusRemoteCheckpoint(value) {
  if (!value || typeof value !== 'object') return null;
  return {
    source: typeof value.source === 'string' ? value.source.slice(0, 80) : null,
    state: typeof value.state === 'string' ? value.state.slice(0, 40) : null,
    collection: typeof value.collection === 'string' ? value.collection.slice(0, 120) : null,
    schemaHash: typeof value.schemaHash === 'string' ? value.schemaHash.slice(0, 96) : null,
    latestLwt: Number.isFinite(Number(value.latestLwt)) ? Number(value.latestLwt) : null,
    latestIdHash: typeof value.latestIdHash === 'string' ? value.latestIdHash.slice(0, 96) : null,
    epoch: typeof value.epoch === 'string' ? value.epoch.slice(0, 96) : null,
  };
}

function sanitizeRxdbRuntime(value) {
  if (!value || typeof value !== 'object') return null;
  return {
    name: typeof value.name === 'string' ? value.name.slice(0, 80) : null,
    publicName: typeof value.publicName === 'string' ? value.publicName.slice(0, 80) : null,
    source: typeof value.source === 'string' ? value.source.slice(0, 80) : null,
    importPath: typeof value.importPath === 'string' ? value.importPath.slice(0, 200) : null,
    packageManager: typeof value.packageManager === 'string' ? value.packageManager.slice(0, 40) : null,
    compatibility: typeof value.compatibility === 'string' ? value.compatibility.slice(0, 80) : null,
    upstreamCompatible: value.upstreamCompatible === true ? true : value.upstreamCompatible === false ? false : null,
    upstreamCompatibility: typeof value.upstreamCompatibility === 'string' ? value.upstreamCompatibility.slice(0, 80) : null,
    apiContract: typeof value.apiContract === 'string' ? value.apiContract.slice(0, 120) : null,
    protocolVersion: typeof value.protocolVersion === 'string' ? value.protocolVersion.slice(0, 80) : null,
  };
}

function buildAdvancedStatusInitialSync(requiredCollections, collections) {
  const now = Date.now();
  const stallAfterMs = 45000;
  const entries = requiredCollections.map((collection) => {
    const diagnostics = collections?.[collection] || null;
    const initialReplicationAt = diagnostics?.initialReplicationAt || null;
    const startedAt = diagnostics?.initialReplicationStartedAt || null;
    const startedMs = startedAt ? Date.parse(startedAt) : NaN;
    const state = initialReplicationAt
      ? 'complete'
      : (diagnostics?.initialReplicationState || (diagnostics ? 'pending' : 'missing-diagnostics'));
    const remoteCapabilities = Array.isArray(diagnostics?.remoteCapabilities)
      ? diagnostics.remoteCapabilities
      : [];
    const checkpoint = sanitizeAdvancedStatusRemoteCheckpoint(diagnostics?.remoteCheckpoint || null);
    const checkpointEpochAdvertised = hasAdvertisedCheckpointEpoch(diagnostics);
    const stalledForMs = !initialReplicationAt && Number.isFinite(startedMs)
      ? Math.max(0, now - startedMs)
      : 0;
    return {
      collection,
      state,
      status: diagnostics?.status || null,
      connectionStatus: diagnostics?.connectionStatus || null,
      source: diagnostics?.initialReplicationSource || null,
      initialReplicationStartedAt: startedAt,
      initialReplicationAt,
      checkpointState: checkpoint?.state || null,
      checkpointEpoch: checkpoint?.epoch || null,
      checkpointEpochAdvertised,
      checkpointCapabilityAdvertised: remoteCapabilities.includes('ctox-checkpoint-epoch-v1'),
      stalled: !initialReplicationAt && stalledForMs >= stallAfterMs,
      stalledForMs,
    };
  });
  return {
    requiredTotal: entries.length,
    completedTotal: entries.filter((entry) => entry.state === 'complete').length,
    missingInitialReplication: entries
      .filter((entry) => entry.state !== 'complete')
      .map((entry) => entry.collection),
    missingCheckpointEpoch: entries
      .filter((entry) => !entry.checkpointEpochAdvertised)
      .map((entry) => entry.collection),
    pendingCollections: entries
      .filter((entry) => !['complete', 'failed'].includes(entry.state))
      .map((entry) => entry.collection),
    stalledCollections: entries
      .filter((entry) => entry.stalled)
      .map((entry) => entry.collection),
    entries,
  };
}

function isRequiredCollectionReady({ collection, diagnostics, evidence }) {
  const status = diagnostics?.connectionStatus || diagnostics?.status || '';
  if (evidence?.hasCollection !== true || !diagnostics) return false;
  const initialReplicationComplete = Boolean(diagnostics.initialReplicationAt || diagnostics.initialReplicationState === 'complete');
  if (!initialReplicationComplete) return false;
  if (!hasAdvertisedCheckpointEpoch(diagnostics)) return false;
  if (['failed', 'error', 'stopped', 'pending'].includes(status)) return false;
  if (['connected', 'running', 'reused'].includes(status)) return true;
  if (evidence?.hasData === true) return true;
  if (![
    'business_commands',
    'ctox_queue_tasks',
    'desktop_files',
    'desktop_file_chunks',
  ].includes(collection)) return false;
  return true;
}

function hasAdvertisedCheckpointEpoch(diagnostics) {
  if (!diagnostics) return false;
  const capabilities = Array.isArray(diagnostics.remoteCapabilities) ? diagnostics.remoteCapabilities : [];
  if (!capabilities.includes('ctox-checkpoint-epoch-v1')) return false;
  const checkpoint = sanitizeAdvancedStatusRemoteCheckpoint(diagnostics.remoteCheckpoint || null);
  return Boolean(checkpoint?.state === 'advertised' && checkpoint.epoch);
}

async function collectAdvancedStatusCounts() {
  const names = [
    'business_module_catalog',
    'ctox_runtime_settings',
    'desktop_files',
    'desktop_file_chunks',
    'business_commands',
    'ctox_queue_tasks',
  ];
  const counts = {};
  await Promise.all(names.map(async (name) => {
    counts[name] = await countCollectionDocs(name);
  }));
  return counts;
}

async function collectAdvancedStatusRequiredEvidence(names) {
  const evidence = {};
  await Promise.all(names.map(async (name) => {
    const collection = state.db?.raw?.[name];
    if (!collection?.find) {
      evidence[name] = { hasCollection: false, hasData: false };
      return;
    }
    try {
      const docs = await collection.find({ limit: 1 }).exec();
      const hasData = docs
        .map((doc) => doc?.toJSON?.() || doc)
        .some((doc) => !doc?._deleted && !doc?.is_deleted);
      evidence[name] = { hasCollection: true, hasData };
    } catch (error) {
      evidence[name] = { hasCollection: true, hasData: false, error: String(error?.message || error) };
    }
  }));
  return evidence;
}

async function countCollectionDocs(name) {
  const collection = state.db?.raw?.[name];
  if (!collection?.find) return null;
  try {
    const docs = await collection.find({ limit: 20 }).exec();
    return docs
      .map((doc) => doc?.toJSON?.() || doc)
      .filter((doc) => !doc?._deleted && !doc?.is_deleted)
      .length;
  } catch (error) {
    console.warn(`[business-os] advanced status count failed for ${name}`, error);
    return null;
  }
}

function refreshOpenSyncDiagnosticsDrawer() {
  if (!els.rightDrawer || els.rightDrawer.hidden) return;
  if (els.rightDrawer.firstElementChild?.dataset?.drawerKind !== 'sync-diagnostics') return;
  els.rightDrawer.replaceChildren(renderSyncDiagnosticsDrawer());
}

const MODULE_GLYPHS = {
  desktop: '🖥',
  ctox: '◆',
  documents: '📄',
  spreadsheets: '📊',
  knowledge: '📚',
  matching: '🔗',
  outbound: '📣',
  reports: '🐞',
  tickets: '▤',
  research: '🔬',
  conversations: '💬',
  notes: '📝',
  'app-store': '🛍',
  'coding-agents': '🤖',
};

function glyphForModule(moduleId) {
  return MODULE_GLYPHS[moduleId] || '◻︎';
}

const DESKTOP_APPS = [
  {
    id: 'explorer',
    title: 'Files',
    glyph: '📁',
    defaultWidth: 720,
    defaultHeight: 460,
    loader: () => import('./desktop-apps/explorer/app.js?v=20260522-file-chunk-integrity4'),
  },
  {
    id: 'code-editor',
    title: 'Source Editor',
    glyph: '⌘',
    defaultWidth: 980,
    defaultHeight: 640,
    loader: () => import('./desktop-apps/code-editor/app.js?v=20260519-monaco2'),
  },
  {
    id: 'file-viewer',
    title: 'File Viewer',
    glyph: '◫',
    defaultWidth: 760,
    defaultHeight: 560,
    loader: () => import('./desktop-apps/file-viewer/app.js?v=20260525-file-viewer-command-reuse1'),
  },
  {
    id: 'creator',
    title: 'App Creator',
    glyph: '⚙️',
    defaultWidth: 1200,
    defaultHeight: 800,
    loader: () => import('./desktop-apps/creator/app.js?v=20260521-app-creator1'),
  },
];

function listDesktopApps() {
  const moduleIds = new Set((state.modules || []).map((mod) => mod?.id).filter(Boolean));
  return DESKTOP_APPS
    .filter((app) => app.id !== 'file-viewer' && !moduleIds.has(app.id))
    .map(({ id, title, glyph, defaultWidth, defaultHeight }) => ({
      id,
      title,
      glyph,
      defaultWidth,
      defaultHeight,
    }));
}

async function openDesktopApp(appId, options = {}) {
  if (!state.windowManager) return null;
  const entry = DESKTOP_APPS.find((app) => app.id === appId);
  if (!entry) {
    console.warn(`[desktop-app] unknown app: ${appId}`);
    return null;
  }
  const win = state.windowManager.create({
    title: options.title || entry.title,
    icon: entry.glyph,
    width: options.width || entry.defaultWidth,
    height: options.height || entry.defaultHeight,
    ownerId: `desktop-app:${entry.id}`,
  });
  let teardown = null;
  try {
    const mod = await entry.loader();
    teardown = await mod.mount(win.container, {
      db: createLiveDbFacade(),
      sync: createLiveSyncFacade(),
      commandBus: createLiveCommandBusFacade(),
      session: state.session,
      contextMenu: state.contextMenu,
      notifications: state.notifications,
      locale: shellLang(),
      args: options.args || {},
      openDesktopApp,
      openBusinessChat,
      reportFileIntegrityError: (error, details = {}) => reportFileIntegrityError(`desktop-app:${entry.id}`, error, {
        appId: entry.id,
        ...details,
      }),
      isTaskbarPinned,
      pinToTaskbar: pinTaskbarTarget,
      unpinFromTaskbar: unpinTaskbarTarget,
      toggleTaskbarPin,
      onClose: () => win.close(),
      setTitle: win.setTitle,
    });
  } catch (error) {
    console.error(`[desktop-app:${appId}] mount failed:`, error);
    win.container.innerHTML = `<p style="padding:16px;color:var(--danger);font-size:12px;">App-Start fehlgeschlagen: ${escapeHtml(String(error?.message || error))}</p>`;
  }
  if (teardown && state.eventBus) {
    const token = state.eventBus.on('window:closed', (data) => {
      if (data?.id !== win.id) return;
      state.eventBus.off('window:closed', token);
      try {
        teardown();
      } catch (error) {
        console.error(`[desktop-app:${appId}] teardown failed:`, error);
      }
    });
  }
  return win.id;
}

function openBusinessChat(detail = {}) {
  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-open', { detail }));
}

function applyShellTheme(theme, options = {}) {
  const value = theme === 'light' ? 'light' : 'dark';
  document.documentElement.dataset.theme = value;
  if (options.persist !== false) {
    writeAccountPrefs({ theme: value });
  }
}

function applyShellStyle(style, options = {}) {
  const value = style === 'macos' ? 'macos' : 'windows';
  document.documentElement.dataset.shellStyle = value;
  state.windowManager?.setChromeLayout(value);
  if (options.persist !== false) {
    writeAccountPrefs({ shellStyle: value });
  }
}

function syncHeaderControls() {
  if (els.languageSelect) {
    els.languageSelect.value = shellLang();
  }
  if (els.themeSelect) {
    els.themeSelect.value = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
  }
  if (els.shellStyleSelect) {
    els.shellStyleSelect.value = document.documentElement.dataset.shellStyle === 'macos' ? 'macos' : 'windows';
  }
}

function localizeShellChrome() {
  document.querySelector('[data-left-pane] > .pane-title')?.replaceChildren(document.createTextNode(shellText('context')));
  document.querySelector('[data-right-pane] > .pane-title')?.replaceChildren(document.createTextNode(shellText('topics')));
}

async function handleModuleCommand(event) {
  const requestId = event.data?.requestId || `cmdreq_${crypto.randomUUID()}`;
  try {
    if (!state.commandBus) throw new Error('Command bus is not ready');
    const command = event.data.command || {};
    const result = await state.commandBus.dispatch({
      ...command,
      client_context: {
        ...(command.client_context || {}),
        module_surface: event.data.surface || 'module-frame',
        shell_module: state.activeModule?.id || '',
      },
    });
    event.source?.postMessage({
      type: 'ctox-business-os-command-result',
      requestId,
      ok: true,
      result,
    }, '*');
  } catch (error) {
    event.source?.postMessage({
      type: 'ctox-business-os-command-result',
      requestId,
      ok: false,
      error: String(error?.message || error),
    }, '*');
  }
}

function applyShellLanguage(lang, options = {}) {
  const value = lang === 'en' ? 'en' : 'de';
  document.documentElement.lang = value;
  if (options.persist !== false) {
    writeAccountPrefs({ language: value });
  }
}

function postCurrentPreferencesToModule() {
  const detail = {
    theme: document.documentElement.dataset.theme === 'light' ? 'light' : 'dark',
    language: document.documentElement.lang === 'en' ? 'en' : 'de',
  };
  window.dispatchEvent(new CustomEvent('ctox-business-os-preferences', { detail }));
  window.postMessage({ type: 'ctox-business-os-language', lang: detail.language }, '*');
  for (const frame of els.host?.querySelectorAll?.('iframe') || []) {
    frame.contentWindow?.postMessage({ type: 'ctox-business-os-preferences', ...detail }, '*');
  }
}

function renderTabs() {
  els.tabs.replaceChildren();
  state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
  state.taskbarPins = normalizeTaskbarPins(state.taskbarPins, state.modules);
  const rendered = new Set();
  for (const id of state.taskbarPins) {
    const target = launchTargetForId(id);
    if (!target) continue;
    els.tabs.append(renderModuleTab(target, { pinned: true }));
    rendered.add(target.id);
  }
  const active = state.activeModule && moduleAppearsInSwitcher(state.activeModule)
    ? launchTargetForId(state.activeModule.id)
    : null;
  if (active && !rendered.has(active.id)) {
    els.tabs.append(renderModuleTab(active, { temporary: true }));
    rendered.add(active.id);
  }
  for (const target of runningDesktopAppTargets()) {
    if (rendered.has(target.id)) continue;
    els.tabs.append(renderModuleTab(target, { temporary: true, running: true }));
    rendered.add(target.id);
  }
}

function renderModuleTab(target, options = {}) {
  const button = document.createElement('button');
  button.className = 'module-tab';
  button.type = 'button';
  button.dataset.module = target.kind === 'module' ? target.id : '';
  button.dataset.target = target.id;
  button.dataset.targetKind = target.kind;
  if (options.pinned) button.dataset.pinned = 'true';
  if (options.temporary) button.dataset.temporary = 'true';
  if (target.kind === 'app' && desktopAppIsFocused(target.id)) button.dataset.running = 'focused';
  else if (target.kind === 'app' && desktopAppIsRunning(target.id)) button.dataset.running = 'true';
  else if (target.kind === 'module' && state.activeModule?.id === target.id) button.dataset.running = 'focused';
  const status = options.pinned
    ? shellText('pinned')
    : (button.dataset.running ? shellText('running') : '');
  const svgHtml = getRegisteredSvgIcon(target.id, 16, 1.8);
  button.innerHTML = `
    <span class="module-tab-icon" aria-hidden="true">${svgHtml || escapeHtml(target.glyph || '◻︎')}</span>
    <span class="module-tab-label">${escapeHtml(target.title || target.id)}</span>
    ${status ? `<span class="module-tab-state">${escapeHtml(status)}</span>` : ''}
  `;
  button.setAttribute('aria-current', state.activeModule?.id === target.id ? 'page' : 'false');
  button.title = target.title || target.id;
  if (options.pinned) {
    button.draggable = true;
    button.addEventListener('dragstart', (event) => {
      event.dataTransfer.effectAllowed = 'move';
      event.dataTransfer.setData('application/x-ctox-taskbar-pin', target.id);
      event.dataTransfer.setData('text/plain', target.id);
      button.classList.add('is-dragging');
    });
    button.addEventListener('dragend', () => {
      button.classList.remove('is-dragging');
    });
  } else {
    button.draggable = false;
  }
  button.addEventListener('dragover', (event) => {
    if (!draggedTaskbarPinId(event)) return;
    event.preventDefault();
    button.classList.add('is-drop-before');
  });
  button.addEventListener('dragleave', () => button.classList.remove('is-drop-before'));
  button.addEventListener('drop', (event) => {
    const pinId = draggedTaskbarPinId(event);
    if (!pinId) return;
    event.preventDefault();
    event.stopPropagation();
    button.classList.remove('is-drop-before');
    moveTaskbarPinBefore(pinId, target.id);
  });
  button.addEventListener('contextmenu', (event) => {
    event.preventDefault();
    showTargetContextMenu(event, target);
  });
  button.addEventListener('dblclick', () => {
    if (target.kind === 'module') openModuleSourceEditor(target.id);
  });
  button.addEventListener('click', () => openLaunchTarget(target));
  return button;
}

function renderLegacyModuleTab(mod, options = {}) {
  const button = document.createElement('button');
  button.className = 'module-tab';
  button.type = 'button';
  button.dataset.module = mod.id;
  const svgHtml = getRegisteredSvgIcon(mod.id, 16, 1.8);
  button.innerHTML = `
    <span class="module-tab-icon" aria-hidden="true">${svgHtml || escapeHtml(taskbarMarkForModule(mod))}</span>
    <span class="module-tab-label">${escapeHtml(moduleDisplayTitle(mod))}</span>
  `;
  if (!options.locked) {
    button.addEventListener('dragover', (event) => {
      if (!draggedModuleId(event)) return;
      event.preventDefault();
      button.classList.add('is-drop-before');
    });
    button.addEventListener('dragleave', () => button.classList.remove('is-drop-before'));
    button.addEventListener('drop', (event) => {
      const moduleId = draggedModuleId(event);
      if (!moduleId || moduleId === 'ctox') return;
      event.preventDefault();
      event.stopPropagation();
      button.classList.remove('is-drop-before');
      moveModuleBefore(moduleId, mod.id);
    });
    button.addEventListener('contextmenu', (event) => {
      event.preventDefault();
      if (canModifyModule(mod)) openModuleEditDrawer(mod);
    });
    button.addEventListener('dblclick', () => {
      if (canModifyModule(mod)) openModuleEditDrawer(mod);
    });
  }
  button.addEventListener('click', () => {
    location.hash = mod.id;
    openModule(mod.id);
  });
  return button;
}

function moduleAppearsInSwitcher(mod) {
  return mod?.id && mod.id !== 'desktop' && mod.id !== 'notizen' && mod.install_scope !== 'internal';
}

function listLaunchTargets(kind = '') {
  const moduleIds = new Set((state.modules || []).map((mod) => mod?.id).filter(Boolean));
  const moduleTargets = state.modules
    .filter(moduleAppearsInSwitcher)
    .map((mod) => ({
      id: mod.id,
      kind: 'module',
      title: moduleDisplayTitle(mod),
      glyph: taskbarMarkForModule(mod),
      module: mod,
    }));
  const appTargets = DESKTOP_APPS
    .filter((app) => app.id !== 'file-viewer' && !moduleIds.has(app.id))
    .map((app) => ({
      id: app.id,
      kind: 'app',
      title: app.title,
      glyph: app.glyph,
      app,
    }));
  const targetsById = new Map();
  for (const target of moduleTargets) {
    if (!target?.id || targetsById.has(target.id)) continue;
    targetsById.set(target.id, target);
  }
  for (const target of appTargets) {
    if (!target?.id) continue;
    if (targetsById.has(target.id)) continue;
    targetsById.set(target.id, target);
  }
  const all = Array.from(targetsById.values());
  return kind ? all.filter((target) => target.kind === kind) : all;
}

function launchTargetForId(id) {
  return listLaunchTargets().find((target) => target.id === id) || null;
}

function startMenuItemForTarget(target) {
  const pinned = isTaskbarPinned(target.id);
  return {
    label: target.title || target.id,
    icon: target.glyph,
    trailingIcon: pinned ? '−' : '+',
    trailingLabel: pinned ? shellText('unpinFromTaskbar') : shellText('pinToTaskbar'),
    trailingAction: () => toggleTaskbarPin(target.id, !pinned),
    action: () => openLaunchTarget(target),
  };
}

function showTargetContextMenu(event, target) {
  if (!state.contextMenu) return;
  const pinned = isTaskbarPinned(target.id);
  const items = [
    {
      label: shellText('openApp') || 'Öffnen',
      icon: target.glyph || '↗',
      action: () => openLaunchTarget(target),
    },
    {
      label: pinned ? shellText('unpinFromTaskbar') : shellText('pinToTaskbar'),
      icon: pinned ? '−' : '+',
      action: () => toggleTaskbarPin(target.id, !pinned),
    },
  ];
  if (target.kind === 'module') {
    items.push({ type: 'separator' });
    items.push({
      label: 'Source öffnen',
      icon: '⌘',
      action: () => openModuleSourceEditor(target.id),
    });
    if (canModifyModule(target.module)) {
      items.push({
        label: 'Modul bearbeiten',
        icon: '✎',
        action: () => openModuleEditDrawer(target.module),
      });
    }
  }
  state.contextMenu.show(event, items);
}

function openLaunchTarget(targetOrId) {
  const target = typeof targetOrId === 'string' ? launchTargetForId(targetOrId) : targetOrId;
  if (!target) return;
  if (target.kind === 'app') {
    const existing = state.windowManager?.listWindows?.()
      .find((win) => win.ownerId === `desktop-app:${target.id}`);
    if (existing) {
      if (existing.state === 'minimized') state.windowManager.restore(existing.id);
      state.windowManager.focus(existing.id);
      return;
    }
    openDesktopApp(target.id);
    return;
  }
  location.hash = target.id;
  openModule(target.id);
}

function runningDesktopAppTargets() {
  const ownerIds = new Set(
    (state.windowManager?.listWindows?.() || [])
      .map((win) => win.ownerId || '')
      .filter((ownerId) => ownerId.startsWith('desktop-app:'))
      .map((ownerId) => ownerId.slice('desktop-app:'.length))
  );
  return Array.from(ownerIds)
    .map((id) => launchTargetForId(id))
    .filter(Boolean);
}

function desktopAppIsRunning(appId) {
  return (state.windowManager?.listWindows?.() || [])
    .some((win) => win.ownerId === `desktop-app:${appId}`);
}

function desktopAppIsFocused(appId) {
  return (state.windowManager?.listWindows?.() || [])
    .some((win) => win.ownerId === `desktop-app:${appId}` && win.isFocused);
}

function isTaskbarPinned(targetId) {
  return state.taskbarPins.includes(targetId);
}

function pinTaskbarTarget(targetId) {
  return toggleTaskbarPin(targetId, true);
}

function unpinTaskbarTarget(targetId) {
  return toggleTaskbarPin(targetId, false);
}

function toggleTaskbarPin(targetId, shouldPin = !isTaskbarPinned(targetId)) {
  if (!launchTargetForId(targetId)) return;
  const pins = state.taskbarPins.filter((id) => id !== targetId);
  if (shouldPin) pins.push(targetId);
  state.taskbarPins = normalizeTaskbarPins(pins, state.modules);
  persistTaskbarPins();
  renderTabs();
}

function moveTaskbarPinBefore(targetId, beforeTargetId) {
  if (!targetId || targetId === beforeTargetId || !isTaskbarPinned(targetId)) return;
  const pins = state.taskbarPins.filter((id) => id !== targetId);
  const index = pins.indexOf(beforeTargetId);
  if (index >= 0) pins.splice(index, 0, targetId);
  else pins.push(targetId);
  state.taskbarPins = normalizeTaskbarPins(pins, state.modules);
  persistTaskbarPins();
  renderTabs();
}

function draggedTaskbarPinId(event) {
  return event.dataTransfer?.getData('application/x-ctox-taskbar-pin')
    || event.dataTransfer?.getData('text/plain')
    || '';
}

function readTaskbarPins() {
  try {
    const parsed = JSON.parse(localStorage.getItem(TASKBAR_PINS_KEY) || 'null');
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function persistTaskbarPins() {
  localStorage.setItem(TASKBAR_PINS_KEY, JSON.stringify(state.taskbarPins));
  clearTimeout(taskbarPinSaveTimer);
  taskbarPinSaveTimer = window.setTimeout(() => {
    taskbarPinSaveTimer = null;
    syncTaskbarPinsToDesktopLayout().catch((error) => {
      console.error('[business-os] taskbar pin sync failed:', error);
    });
  }, 180);
}

function normalizeTaskbarPins(rawPins, modules, options = {}) {
  const valid = new Set(listLaunchTargets().map((target) => target.id));
  const raw = Array.isArray(rawPins) ? rawPins : [];
  let pins = raw
    .map((id) => String(id || '').trim())
    .filter((id, index, arr) => id && valid.has(id) && arr.indexOf(id) === index);
  if (options.compactLegacyAllPins && looksLikeLegacyAllPins(pins, valid)) pins = [];
  if (!pins.length) {
    pins = DEFAULT_TASKBAR_PIN_IDS.filter((id) => valid.has(id));
    if (!pins.length) pins = listLaunchTargets('module').slice(0, 4).map((target) => target.id);
  }
  return pins;
}

function looksLikeLegacyAllPins(pins, valid) {
  if (pins.length <= DEFAULT_TASKBAR_PIN_IDS.length + 2) return false;
  const coverage = pins.filter((id) => valid.has(id)).length / Math.max(1, valid.size);
  return coverage >= 0.75;
}

async function hydrateTaskbarPinsFromDesktopLayout() {
  const collection = state.db?.collection?.('desktop_layout');
  if (!collection) {
    state.taskbarPins = normalizeTaskbarPins(state.taskbarPins, state.modules);
    return;
  }
  const doc = await collection.findOne('layout').exec();
  const layout = doc?.toJSON?.() || null;
  if (Array.isArray(layout?.taskbar_pins)) {
    state.taskbarPins = normalizeTaskbarPins(layout.taskbar_pins, state.modules, { compactLegacyAllPins: true });
  } else {
    state.taskbarPins = normalizeTaskbarPins(state.taskbarPins, state.modules);
  }
  localStorage.setItem(TASKBAR_PINS_KEY, JSON.stringify(state.taskbarPins));
  await syncTaskbarPinsToDesktopLayout();
}

async function syncTaskbarPinsToDesktopLayout() {
  const collection = state.db?.collection?.('desktop_layout');
  if (!collection) return;
  const existing = await collection.findOne('layout').exec();
  const patch = {
    taskbar_pins: state.taskbarPins,
    updated_at_ms: Date.now(),
  };
  if (existing) {
    await existing.incrementalPatch(patch);
  } else {
    await collection.insert({ id: 'layout', ...patch });
  }
}

function renderModuleGroup(group, modulesById) {
  const wrap = document.createElement('details');
  wrap.className = 'module-group';
  wrap.dataset.groupId = group.id;
  const activeInside = group.items.includes(state.activeModule?.id);
  if (activeInside) wrap.dataset.active = 'true';
  wrap.addEventListener('dragover', (event) => {
    if (!draggedModuleId(event)) return;
    event.preventDefault();
    wrap.classList.add('is-drop-target');
  });
  wrap.addEventListener('dragleave', () => wrap.classList.remove('is-drop-target'));
  wrap.addEventListener('drop', (event) => {
    const moduleId = draggedModuleId(event);
    if (!moduleId || moduleId === 'ctox') return;
    event.preventDefault();
    event.stopPropagation();
    wrap.classList.remove('is-drop-target');
    moveModuleToGroup(moduleId, group.id);
  });

  const summary = document.createElement('summary');
  summary.className = 'module-group-summary';
  summary.innerHTML = `<span>${escapeHtml(group.title || 'Gruppe')}</span><small>${group.items.length}</small>`;
  summary.addEventListener('contextmenu', (event) => {
    event.preventDefault();
    openGroupEditDrawer(group.id);
  });
  summary.addEventListener('dblclick', (event) => {
    event.preventDefault();
    openGroupEditDrawer(group.id);
  });
  wrap.append(summary);

  const menu = document.createElement('div');
  menu.className = 'module-group-menu';
  for (const moduleId of group.items) {
    const mod = modulesById.get(moduleId);
    if (mod) menu.append(renderModuleTab(mod));
  }
  if (!menu.childElementCount) {
    const empty = document.createElement('span');
    empty.className = 'module-group-empty';
    empty.textContent = 'Leere Gruppe';
    menu.append(empty);
  }
  wrap.append(menu);
  return wrap;
}

async function openModule(moduleId, options = {}) {
  moduleId = String(moduleId || '').split('?')[0];
  const requestedId = moduleAliases[moduleId] || moduleId;
  if (requestedId !== moduleId && currentHashModuleId() === moduleId) {
    history.replaceState(null, '', `#${requestedId}`);
  }
  const mod = state.modules.find((item) => item.id === requestedId) || state.modules[0];
  if (!mod) return;
  if (state.activeModule?.id === mod.id && !options.force) return;

  // Track history stack
  if (!options.isNavHistory) {
    if (state.navIndex < state.navHistory.length - 1) {
      state.navHistory = state.navHistory.slice(0, state.navIndex + 1);
    }
    if (state.navIndex === -1 || state.navHistory[state.navIndex] !== mod.id) {
      state.navHistory.push(mod.id);
      state.navIndex = state.navHistory.length - 1;
    }
  }

  if (typeof state.activeUnmount === 'function') {
    await state.activeUnmount();
  }
  state.activeModule = mod;
  state.activeUnmount = null;
  document.body.dataset.activeModule = mod.id;
  document.body.dataset.moduleShell = moduleUsesFullWorkspace(mod) ? 'full' : 'pane';
  document.body.dataset.moduleLoading = mod.id;
  updateActiveAppChrome(mod);
  renderTabs();
  shellColumnResizeSync?.();
  for (const button of els.tabs.querySelectorAll('[data-module]')) {
    button.setAttribute('aria-current', button.dataset.module === mod.id ? 'page' : 'false');
  }
  els.host.replaceChildren(renderModuleFrame(mod));
  els.leftContent.replaceChildren(renderLeftContext(mod));
  els.rightContent.replaceChildren(renderRightContext(mod));
  loadModuleVersionsDropdown(mod.id);
  try {
    await registerModuleSchemas(mod);
  } catch (error) {
    console.error(`[business-os] Schema registration failed for ${mod.id}`, error);
    setStatus(`Schema warning: ${error.message || error}`);
  }
  try {
    const moduleScript = await importBusinessOsModule(
      `./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}${moduleRevisionQuery(mod.id)}`,
      `${mod.id} module`,
    );
    if (typeof moduleScript.mount === 'function') {
      state.activeUnmount = await moduleScript.mount(createModuleContext(mod));
    }
  } finally {
    delete document.body.dataset.moduleLoading;
    shellColumnResizeSync?.();
  }
  postCurrentPreferencesToModule();
  startModuleSync(mod);
  updateNavButtons();
}

function updateNavButtons() {
  if (els.backButton) {
    els.backButton.disabled = state.navIndex <= 0;
  }
  if (els.forwardButton) {
    els.forwardButton.disabled = state.navIndex === -1 || state.navIndex >= state.navHistory.length - 1;
  }
}

async function navigateHistory(direction) {
  if (direction === 'back' && state.navIndex > 0) {
    state.navIndex--;
  } else if (direction === 'forward' && state.navIndex < state.navHistory.length - 1) {
    state.navIndex++;
  } else {
    return;
  }
  const nextModuleId = state.navHistory[state.navIndex];
  state.navTransitioning = true;
  location.hash = nextModuleId;
  await openModule(nextModuleId, { isNavHistory: true });
  state.navTransitioning = false;
  updateNavButtons();
}

function openDesktop() {
  location.hash = '#desktop';
  return openModule('desktop');
}

function moduleUsesFullWorkspace(mod) {
  return mod?.id === 'ctox'
    || mod.layout?.shell === 'full-workspace'
    || mod.layout?.full_workspace === true
    || mod.layout?.fullFrame === true;
}

function currentHashModuleId() {
  return location.hash.replace(/^#/, '').split('?')[0];
}

async function registerModuleSchemas(mod) {
  if (!mod?.id || !state.db) return;
  if (state.schemaRegistrations.has(mod.id)) {
    return state.schemaRegistrations.get(mod.id);
  }
  const generation = state.dataPlaneGeneration;
  const db = state.db;
  const registration = (async () => {
    const retry = Number(state.schemaImportRetries.get(mod.id) || 0);
    const retryQuery = retry > 0 ? `_schemaRetry${retry}` : '';
    const schemaModule = await importBusinessOsModule(
      `./${moduleBasePath(mod)}/schema.js?v=${APP_BUILD}${moduleRevisionQuery(mod.id)}${retryQuery}`,
      `${mod.id} schema`,
    );
    if (isStaleDataPlaneGeneration(generation)) return;
    if (schemaModule.collections) {
      const collections = withMigrationStrategies(
        schemaModule.collections,
        schemaModule.migrationStrategies
      );
      const nextRegistration = state.schemaRegistrationQueue
        .catch(() => {})
        .then(() => {
          if (isStaleDataPlaneGeneration(generation)) return null;
          return db.addCollections(collections);
        });
      state.schemaRegistrationQueue = nextRegistration.catch(() => {});
      await nextRegistration;
    }
  })().catch((error) => {
    state.schemaRegistrations.delete(mod.id);
    throw error;
  });
  state.schemaRegistrations.set(mod.id, registration);
  return registration;
}

function withMigrationStrategies(collections, migrationStrategies = {}) {
  if (!collections || !migrationStrategies || !Object.keys(migrationStrategies).length) {
    return collections;
  }
  const next = {};
  for (const [name, definition] of Object.entries(collections)) {
    const strategies = migrationStrategies[name];
    if (!strategies) {
      next[name] = definition;
    } else if (definition?.schema) {
      next[name] = { ...definition, migrationStrategies: strategies };
    } else {
      next[name] = { schema: definition, migrationStrategies: strategies };
    }
  }
  return next;
}

function startAllModuleSync() {
  const modules = state.modules.filter((mod) => mod.id !== state.activeModule?.id);
  modules.forEach((mod, index) => {
    window.setTimeout(() => startModuleSync(mod), index * 350);
  });
}

function startModuleSync(mod) {
  if (!mod?.id || !state.sync || state.syncStartedModules.has(mod.id)) return;
  if (state.schemaRetryTimers.has(mod.id)) return;
  state.syncStartedModules.add(mod.id);
  registerModuleSchemas(mod)
    .then(() => {
      state.schemaImportRetries.delete(mod.id);
      return state.sync.startModule(mod);
    })
    .catch(async (error) => {
      state.syncStartedModules.delete(mod.id);
      if (isRecoverableDataPlaneAbort(error)) return;
      if (await recoverFromLocalRxDbSchemaDrift(error)) return;
      if (isTransientModuleLoadError(error)) {
        scheduleTransientModuleSyncRetry(mod, error);
        return;
      }
      console.error(`[business-os] Sync startup failed for ${mod.id}`, error);
      setStatus(`Sync failed: ${error.message || error}`);
    });
}

function scheduleTransientModuleSyncRetry(mod, error) {
  const retry = Number(state.schemaImportRetries.get(mod.id) || 0) + 1;
  if (retry > MAX_TRANSIENT_MODULE_SYNC_RETRIES) {
    state.schemaImportRetries.delete(mod.id);
    console.warn(`[business-os] schema import unavailable for ${mod.id}; module sync disabled`, error);
    return;
  }
  state.schemaImportRetries.set(mod.id, retry);
  const delayMs = Math.min(15000, 1000 * Math.max(1, Math.min(retry, 8)));
  if (retry === 1 || retry % 5 === 0) {
    console.warn(`[business-os] transient schema import failed for ${mod.id}; retrying`, error);
  }
  const timer = window.setTimeout(() => {
    state.schemaRetryTimers.delete(mod.id);
    startModuleSync(mod);
  }, delayMs);
  state.schemaRetryTimers.set(mod.id, timer);
}

async function recoverFromLocalRxDbSchemaDrift(error) {
  if (!isRxDbSchemaDriftError(error)) return false;
  const repairToken = `${businessDbName()}:${RXDB_BOOTSTRAP_VERSION}`;
  try {
    if (sessionStorage.getItem(RXDB_SCHEMA_REPAIR_KEY) === repairToken) return false;
    sessionStorage.setItem(RXDB_SCHEMA_REPAIR_KEY, repairToken);
  } catch {}
  const log = isRxDbOpenTimeoutError(error) ? console.info : console.warn;
  log('[business-os] local RxDB cache repair triggered; rebuilding browser cache', error);
  setStatus('Lokale RxDB wird neu aufgebaut');
  try { await state.sync?.stop?.(); } catch (stopError) { console.warn('[business-os] sync stop before schema repair failed', stopError); }
  try { await state.db?.close?.(); } catch (closeError) { console.warn('[business-os] db close before schema repair failed', closeError); }
  try {
    const { resetBusinessDb } = await loadBusinessDbModule();
    await resetBusinessDb({ name: businessDbName() });
  } catch (resetError) { console.warn('[business-os] RxDB schema repair reset failed', resetError); }
  window.setTimeout(() => window.location.reload(), 250);
  return true;
}

function isRxDbSchemaDriftError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: DB6')
    || message.includes('previousSchemaHash')
    || message.includes('schemaHash')
    || message.includes('timed out')
    || message.includes('IndexedDB lock');
}

function isRxDbOpenTimeoutError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB database creation timed out')
    || message.includes('IndexedDB lock');
}

function preloadModuleScripts() {
  const modules = state.modules.filter((mod) => mod.id !== state.activeModule?.id);
  for (const [index, mod] of modules.entries()) {
    const href = `./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}${moduleRevisionQuery(mod.id)}`;
    if (document.head.querySelector(`link[rel="modulepreload"][href="${href}"]`)) continue;
    window.setTimeout(() => {
      if (document.head.querySelector(`link[rel="modulepreload"][href="${href}"]`)) return;
      const link = document.createElement('link');
      link.rel = 'modulepreload';
      link.href = href;
      document.head.append(link);
    }, index * 250);
  }
}

function scheduleBackgroundModuleWork() {
  if (state.backgroundModuleWorkScheduled) return;
  state.backgroundModuleWorkScheduled = true;
  const run = () => {
    preloadModuleScripts();
  };
  if ('requestIdleCallback' in window) {
    window.requestIdleCallback(run, { timeout: 3000 });
  } else {
    window.setTimeout(run, 1200);
  }
}

function moduleBasePath(mod) {
  const entry = String(mod.entry || `modules/${mod.id}/index.html`)
    .replace(/^\.?\//, '')
    .split('?')[0]
    .split('#')[0];
  const slash = entry.lastIndexOf('/');
  return slash >= 0 ? entry.slice(0, slash) : `modules/${mod.id}`;
}

function createModuleContext(mod) {
  return {
    module: mod,
    modules: state.modules,
    locale: document.documentElement.lang === 'en' ? 'en' : 'de',
    shellStyle: document.documentElement.dataset.shellStyle === 'macos' ? 'macos' : 'windows',
    host: els.host.querySelector('[data-module-content]') || els.host.querySelector('[data-module-root]'),
    left: els.leftContent,
    right: els.rightContent,
    db: createLiveDbFacade(),
    sync: createLiveSyncFacade(),
    commandBus: createLiveCommandBusFacade(),
    syncConfig: state.sync?.config,
    session: state.session,
    governance: state.governance,
    eventBus: state.eventBus,
    contextMenu: state.contextMenu,
    notifications: state.notifications,
    windowManager: state.windowManager,
    desktopApps: listDesktopApps(),
    openDesktopApp,
    openBusinessChat,
    reportFileIntegrityError: (error, details = {}) => reportFileIntegrityError(`module:${mod.id}`, error, {
      appId: mod.id,
      ...details,
    }),
    isTaskbarPinned,
    pinToTaskbar: pinTaskbarTarget,
    unpinFromTaskbar: unpinTaskbarTarget,
    toggleTaskbarPin,
    canModifyModule: () => canModifyModule(mod),
    reportIssue: (details = {}) => reportCurrentModule({ module: mod, ...details }),
    openLeftDrawer: (content) => openDrawer('left', content),
    openRightDrawer: (content) => openDrawer('right', content),
    openBottomDrawer: (content) => openDrawer('bottom', content),
    closeDrawers,
  };
}

function createLiveDbFacade() {
  return {
    get mode() { return state.db?.mode; },
    get rxdb() { return state.db?.rxdb; },
    get raw() { return state.db?.raw; },
    get collections() { return state.db?.collections || {}; },
    addCollections: (...args) => state.db?.addCollections?.(...args),
    collection: (...args) => state.db?.collection?.(...args),
    close: (...args) => state.db?.close?.(...args),
  };
}

function createLiveSyncFacade() {
  return {
    get mode() { return state.sync?.mode; },
    get config() { return state.sync?.config; },
    get diagnostics() { return state.sync?.diagnostics; },
    startCollection: (...args) => state.sync?.startCollection?.(...args),
    stopCollection: (...args) => state.sync?.stopCollection?.(...args),
    restartCollection: (...args) => state.sync?.restartCollection?.(...args),
    restartCollections: (...args) => state.sync?.restartCollections?.(...args),
    suspendCollections: (...args) => state.sync?.suspendCollections?.(...args),
    resumeCollections: (...args) => state.sync?.resumeCollections?.(...args),
    stop: (...args) => state.sync?.stop?.(...args),
  };
}

function createLiveCommandBusFacade() {
  return {
    dispatch: (...args) => state.commandBus?.dispatch?.(...args),
  };
}

function renderModuleFrame(mod) {
  const root = document.createElement('div');
  root.className = 'module-root';
  root.dataset.moduleRoot = mod.id;
  root.innerHTML = `
    ${renderModuleAppBar(mod)}
    <div class="module-content" data-module-content>
      ${renderModuleLoadingShell(mod, moduleDisplayTitle(mod), shellText('loadingModule'))}
    </div>
  `;
  return root;
}

function moduleRevisionQuery(moduleId) {
  const rev = state.moduleRevisions?.[moduleId];
  return rev ? `_${rev}` : '';
}

async function loadModuleVersionsDropdown(moduleId) {
  const select = els.host.querySelector(`[data-module-version-select="${moduleId}"]`);
  if (!select) return;
  const generation = state.dataPlaneGeneration;
  try {
    const response = await dispatchShellModuleCommand({
      commandType: 'ctox.source.list_snapshots',
      moduleId,
      recordId: `${moduleId}:snapshots`,
      payload: { module_id: moduleId },
      source: 'business-os-shell',
    });
    const snapshots = response?.result || [];
    if (snapshots.length > 0) {
      // Clear all but first option
      while (select.options.length > 1) {
        select.remove(1);
      }
      // Populate select
      snapshots.forEach((snap) => {
        const date = new Date(snap.created_at_ms);
        const dateStr = date.toLocaleString(shellLang() === 'de' ? 'de-DE' : 'en-US', {
          month: 'short',
          day: 'numeric',
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
        });
        const option = document.createElement('option');
        option.value = snap.snapshot_id;
        option.textContent = `${snap.path} (${dateStr})`;
        select.appendChild(option);
      });
      select.style.display = 'inline-block';
    } else {
      select.style.display = 'none';
    }
  } catch (error) {
    if (isRecoverableDataPlaneAbort(error) || isStaleDataPlaneGeneration(generation)) return;
    console.error('[business-os] failed to load module versions:', error);
  }
}

function renderModuleAppBar(mod) {
  if (mod?.id === 'desktop') return '';
  const title = escapeHtml(moduleDisplayTitle(mod));
  const svgHtml = getRegisteredSvgIcon(mod.id, 16, 1.8);
  return `
    <header class="module-appbar" data-module-appbar>
      <div class="module-appbar-title">
        <span class="module-appbar-icon" aria-hidden="true">${svgHtml || escapeHtml(glyphForModule(mod.id))}</span>
        <span>${title}</span>
      </div>
      <div class="module-appbar-actions">
        <select class="header-select module-appbar-select" style="display: none; width: auto; max-width: 140px; margin-right: 4px;" data-module-version-select="${escapeHtml(mod.id)}" aria-label="${escapeHtml(shellText('selectVersion') || 'Version auswählen')}">
          <option value="" disabled selected>${escapeHtml(shellText('selectVersion') || 'Version...')}</option>
        </select>
        <button class="module-appbar-button" type="button" data-module-source="${escapeHtml(mod.id)}" aria-label="Source von ${title} öffnen" title="Source öffnen">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 8l-4 4 4 4"></path><path d="M16 8l4 4-4 4"></path><path d="M14 5l-4 14"></path></svg>
        </button>
        <button class="module-appbar-button" type="button" data-module-home aria-label="${escapeHtml(shellText('showDesktop'))}" title="${escapeHtml(shellText('showDesktop'))}">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5.5h16v13H4z"></path><path d="M8 9h8M8 12h8M8 15h5"></path></svg>
        </button>
      </div>
    </header>
  `;
}

function updateActiveAppChrome(mod) {
  const instanz = getInstanceName();
  document.title = `${moduleDisplayTitle(mod)} · CTOX Business OS (${instanz})`;
}

function taskbarMarkForModule(mod) {
  const marks = {
    ctox: '◆',
    desktop: '⌂',
    documents: 'D',
    spreadsheets: 'S',
    knowledge: 'K',
    matching: 'M',
    outbound: 'O',
    reports: '🐞',
    tickets: 'T',
    research: 'R',
    'coding-agents': '🤖',
  };
  return marks[mod?.id] || String(mod?.title || mod?.id || 'A').trim().slice(0, 1).toUpperCase();
}

function renderModuleLoadingShell(mod, title, subtitle) {
  const moduleId = String(mod?.id || 'generic').replace(/[^a-z0-9_-]/gi, '').toLowerCase() || 'generic';
  const safeTitle = escapeHtml(title || 'Documents');
  const safeSubtitle = escapeHtml(subtitle || shellText('loadingModule'));
  const panels = moduleLoadingPanels(moduleId, safeTitle, safeSubtitle);
  return `
    <div class="module-loading-shell module-loading-shell-${moduleId}" aria-busy="true">
      ${panels}
    </div>
  `;
}

function moduleLoadingPanels(moduleId, title, subtitle) {
  if (moduleId === 'matching') {
    return `
      <section class="module-loading-pane module-loading-matching-source" aria-hidden="true">
        ${loadingHead()}
        <div class="module-loading-control-stack"><b></b><b></b></div>
        <div class="module-loading-source-list"><b></b><b></b><b></b></div>
      </section>
      <section class="module-loading-pane module-loading-matching-center">
        ${loadingHead()}
        ${loadingCopy(title, subtitle)}
        <div class="module-loading-match-workbench" aria-hidden="true">
          <div class="module-loading-match-toolbar"><b></b><b></b><b></b></div>
          <div class="module-loading-match-grid"><b></b><b></b><b></b><b></b><b></b><b></b></div>
        </div>
      </section>
      <section class="module-loading-pane module-loading-matching-object" aria-hidden="true">
        ${loadingHead()}
        <div class="module-loading-control-stack"><b></b><b></b></div>
        <div class="module-loading-object-list"><b></b><b></b><b></b><b></b></div>
      </section>
    `;
  }
  if (moduleId === 'knowledge') {
    return `
      <section class="module-loading-pane module-loading-knowledge-left" aria-hidden="true">
        ${loadingHead()}
        <div class="module-loading-segments"><b></b><b></b><b></b></div>
        <div class="module-loading-search"></div>
        <div class="module-loading-tree"><b></b><b></b><b></b><b></b><b></b></div>
      </section>
      <section class="module-loading-pane module-loading-knowledge-reader">
        ${loadingHead()}
        ${loadingCopy(title, subtitle)}
        <div class="module-loading-article" aria-hidden="true"><b></b><b></b><b></b><b></b><b></b><b></b></div>
      </section>
    `;
  }
  if (moduleId === 'ctox') {
    return `
      <section class="module-loading-pane module-loading-ctox-left" aria-hidden="true">
        <div class="module-loading-kpi"><b></b><b></b></div>
        <div class="module-loading-channel"><b></b><b></b></div>
        <div class="module-loading-task-card"><b></b><b></b><b></b></div>
      </section>
      <section class="module-loading-pane module-loading-ctox-flow">
        ${loadingHead()}
        ${loadingCopy(title, subtitle)}
        <div class="module-loading-stats" aria-hidden="true"><b></b><b></b><b></b><b></b></div>
        <div class="module-loading-flow-canvas" aria-hidden="true"><b></b><b></b><b></b><b></b><i></i><i></i><i></i></div>
        <div class="module-loading-timeline" aria-hidden="true"><b></b><b></b></div>
      </section>
    `;
  }
  if (moduleId === 'outbound') {
    return `
      <section class="module-loading-pane module-loading-outbound-left" aria-hidden="true">
        ${loadingHead()}
        <div class="module-loading-campaign-list"><b></b><b></b><b></b><b></b></div>
      </section>
      <section class="module-loading-pane module-loading-outbound-center">
        ${loadingHead()}
        ${loadingCopy(title, subtitle)}
        <div class="module-loading-pipeline" aria-hidden="true"><b></b><b></b><b></b><b></b><b></b><b></b></div>
      </section>
    `;
  }
  return `
    <section class="module-loading-pane module-loading-doc-list" aria-hidden="true">
      ${loadingHead()}
      <div class="module-loading-control-stack"><b></b><b></b></div>
      <div class="module-loading-document-list"><b></b><b></b><b></b></div>
    </section>
    <section class="module-loading-pane module-loading-doc-editor">
      ${loadingHead()}
      ${loadingCopy(title, subtitle)}
      <div class="module-loading-document" aria-hidden="true"><b></b><b></b><b></b><b></b></div>
    </section>
    <section class="module-loading-pane module-loading-doc-runbooks" aria-hidden="true">
      ${loadingHead()}
      <div class="module-loading-runbook-list"><b></b><b></b><b></b></div>
    </section>
  `;
}

function loadingHead() {
  return '<div class="module-loading-panel-head"><span></span><i></i></div>';
}

function loadingCopy(title, subtitle) {
  return `
    <div class="module-loading-copy">
      <strong>${title}</strong>
      <span>${subtitle}</span>
    </div>
  `;
}

function readModuleLayout() {
  try {
    return JSON.parse(localStorage.getItem(MODULE_LAYOUT_KEY) || '{}') || {};
  } catch {
    return {};
  }
}

function persistModuleLayout() {
  const layout = state.moduleLayout || {
    version: 1,
    labels: {},
    ungrouped: [],
    groups: [],
  };
  localStorage.setItem(MODULE_LAYOUT_KEY, JSON.stringify(layout));
  clearTimeout(moduleLayoutSaveTimer);
  moduleLayoutSaveTimer = null;
}

function normalizeModuleLayout(layout, modules) {
  const movableIds = modules
    .map((mod) => mod.id)
    .filter((id) => id && id !== 'ctox');
  const movable = new Set(movableIds);
  const next = {
    version: 1,
    labels: Object.fromEntries(
      Object.entries(layout?.labels || {}).filter(([id]) => movable.has(id))
    ),
    ungrouped: [],
    groups: [],
  };
  const seen = new Set();

  for (const rawGroup of layout?.groups || []) {
    const id = sanitizeClientId(rawGroup.id || `group-${crypto.randomUUID()}`);
    const items = [];
    for (const moduleId of rawGroup.items || []) {
      if (!movable.has(moduleId) || seen.has(moduleId)) continue;
      seen.add(moduleId);
      items.push(moduleId);
    }
    next.groups.push({
      id,
      title: String(rawGroup.title || 'Gruppe').trim() || 'Gruppe',
      open: rawGroup.open !== false,
      items,
    });
  }
  for (const moduleId of layout?.ungrouped || []) {
    if (!movable.has(moduleId) || seen.has(moduleId)) continue;
    seen.add(moduleId);
    next.ungrouped.push(moduleId);
  }
  for (const moduleId of movableIds) {
    if (!seen.has(moduleId)) {
      seen.add(moduleId);
      next.ungrouped.push(moduleId);
    }
  }
  return next;
}

function moduleDisplayTitle(mod) {
  return state.moduleLayout?.labels?.[mod.id] || mod.title || mod.id;
}

function draggedModuleId(event) {
  const moduleId = event.dataTransfer?.getData('application/x-ctox-module')
    || event.dataTransfer?.getData('text/plain');
  return moduleId && moduleId !== 'ctox' ? moduleId : '';
}

function removeModuleFromLayout(moduleId) {
  state.moduleLayout.ungrouped = state.moduleLayout.ungrouped.filter((id) => id !== moduleId);
  for (const group of state.moduleLayout.groups) {
    group.items = group.items.filter((id) => id !== moduleId);
  }
}

function moveModuleBefore(moduleId, beforeModuleId) {
  if (moduleId === beforeModuleId || moduleId === 'ctox') return;
  removeModuleFromLayout(moduleId);
  for (const group of state.moduleLayout.groups) {
    const index = group.items.indexOf(beforeModuleId);
    if (index >= 0) {
      group.items.splice(index, 0, moduleId);
      persistModuleLayout();
      renderTabs();
      return;
    }
  }
  const index = state.moduleLayout.ungrouped.indexOf(beforeModuleId);
  if (index >= 0) {
    state.moduleLayout.ungrouped.splice(index, 0, moduleId);
  } else {
    state.moduleLayout.ungrouped.push(moduleId);
  }
  persistModuleLayout();
  renderTabs();
}

function moveModuleToGroup(moduleId, groupId) {
  if (moduleId === 'ctox') return;
  removeModuleFromLayout(moduleId);
  const group = state.moduleLayout.groups.find((item) => item.id === groupId);
  if (!group) return;
  group.items.push(moduleId);
  group.open = true;
  persistModuleLayout();
  renderTabs();
}

function moveModuleToUngrouped(moduleId) {
  if (moduleId === 'ctox') return;
  removeModuleFromLayout(moduleId);
  state.moduleLayout.ungrouped.push(moduleId);
  persistModuleLayout();
  renderTabs();
}

function createModuleGroup(title = 'Neue Gruppe') {
  const group = {
    id: `group-${crypto.randomUUID()}`,
    title,
    open: true,
    items: [],
  };
  state.moduleLayout.groups.push(group);
  persistModuleLayout();
  renderTabs();
  return group;
}

function renameModule(moduleId, label) {
  if (moduleId === 'ctox') return;
  const mod = state.modules.find((item) => item.id === moduleId);
  const fallback = mod?.title || mod?.id || '';
  const trimmed = label.trim();
  if (!trimmed || trimmed === fallback) {
    delete state.moduleLayout.labels[moduleId];
  } else {
    state.moduleLayout.labels[moduleId] = trimmed;
  }
  persistModuleLayout();
  renderTabs();
  if (state.activeModule?.id === moduleId) {
    els.host.querySelector('.empty-state strong')?.replaceChildren(document.createTextNode(trimmed || fallback));
  }
}

function openModuleEditDrawer(mod) {
  if (!canModifyModule(mod)) return;
  const body = document.createElement('div');
  body.className = 'drawer-body module-organizer-drawer';
  const groups = state.moduleLayout.groups;
  const currentGroup = groups.find((group) => group.items.includes(mod.id))?.id || '';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Modul organisieren</h2>
        <p>${escapeHtml(mod.id)}</p>
      </div>
      <button class="icon-button" type="button" data-close-organizer aria-label="Schließen">×</button>
    </header>
    <form class="module-organizer-form" data-module-organizer-form>
      <label>
        <span>Anzeigename</span>
        <input name="label" value="${escapeHtml(moduleDisplayTitle(mod))}" />
      </label>
      <label>
        <span>Gruppe</span>
        <select name="group">
          <option value="">Oberste Modulebene</option>
          ${groups.map((group) => `<option value="${escapeHtml(group.id)}"${group.id === currentGroup ? ' selected' : ''}>${escapeHtml(group.title)}</option>`).join('')}
          <option value="__new__">Neue Gruppe...</option>
        </select>
      </label>
      <label data-new-group-row hidden>
        <span>Neue Gruppe</span>
        <input name="newGroupTitle" value="Neue Gruppe" />
      </label>
      <div class="module-organizer-actions">
        <button class="text-button account-primary" type="submit">Speichern</button>
        <button class="text-button" type="button" data-ungroup-module>Aus Gruppe lösen</button>
      </div>
      <small>Drag and Drop in der oberen Navigation ändert Reihenfolge und Gruppenzuordnung direkt. CTOX bleibt fix.</small>
    </form>
  `;
  body.querySelector('[data-close-organizer]')?.addEventListener('click', closeDrawers);
  const groupSelect = body.querySelector('select[name="group"]');
  const newGroupRow = body.querySelector('[data-new-group-row]');
  groupSelect.addEventListener('change', () => {
    newGroupRow.hidden = groupSelect.value !== '__new__';
  });
  body.querySelector('[data-ungroup-module]')?.addEventListener('click', () => {
    moveModuleToUngrouped(mod.id);
    closeDrawers();
  });
  body.querySelector('[data-module-organizer-form]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    renameModule(mod.id, form.get('label')?.toString() || '');
    const groupValue = form.get('group')?.toString() || '';
    if (groupValue === '__new__') {
      const group = createModuleGroup(form.get('newGroupTitle')?.toString().trim() || 'Neue Gruppe');
      moveModuleToGroup(mod.id, group.id);
    } else if (groupValue) {
      moveModuleToGroup(mod.id, groupValue);
    } else {
      moveModuleToUngrouped(mod.id);
    }
    closeDrawers();
  });
  openDrawer('right', body);
}

function openGroupEditDrawer(groupId) {
  const group = state.moduleLayout.groups.find((item) => item.id === groupId);
  if (!group) return;
  const body = document.createElement('div');
  body.className = 'drawer-body module-organizer-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Gruppe organisieren</h2>
        <p>${escapeHtml(group.items.length)} Module</p>
      </div>
      <button class="icon-button" type="button" data-close-organizer aria-label="Schließen">×</button>
    </header>
    <form class="module-organizer-form" data-group-organizer-form>
      <label>
        <span>Gruppenname</span>
        <input name="title" value="${escapeHtml(group.title)}" />
      </label>
      <div class="module-organizer-actions">
        <button class="text-button account-primary" type="submit">Speichern</button>
        <button class="text-button" type="button" data-dissolve-group>Gruppe auflösen</button>
      </div>
    </form>
  `;
  body.querySelector('[data-close-organizer]')?.addEventListener('click', closeDrawers);
  body.querySelector('[data-group-organizer-form]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const title = new FormData(event.currentTarget).get('title')?.toString().trim();
    group.title = title || 'Gruppe';
    persistModuleLayout();
    renderTabs();
    closeDrawers();
  });
  body.querySelector('[data-dissolve-group]')?.addEventListener('click', () => {
    state.moduleLayout.ungrouped.push(...group.items);
    state.moduleLayout.groups = state.moduleLayout.groups.filter((item) => item.id !== group.id);
    persistModuleLayout();
    renderTabs();
    closeDrawers();
  });
  openDrawer('right', body);
}

function sanitizeClientId(value) {
  return String(value || '')
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '') || `group-${crypto.randomUUID()}`;
}

function renderLoginGate(session, options = {}) {
  document.body.dataset.authState = 'locked';
  delete document.body.dataset.moduleShell;
  delete document.body.dataset.moduleLoading;
  state.modules = [];
  els.tabs.replaceChildren();
  els.leftContent.replaceChildren();
  els.rightContent.replaceChildren();

  const container = document.createElement('div');
  container.className = 'auth-gate';

  const savedUser = readAccountPrefs().loginUser || '';
  const loginUrl = session.login_url || '';
  const pairingMissing = false;

  container.innerHTML = `
    <div class="auth-gate-panel${options.loginFailed ? ' has-error' : ''}">
      <header class="auth-gate-header">
        <div class="auth-gate-logo">
          <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2">
            <polygon points="12,2 2,7 12,12 22,7" fill="var(--accent)" fill-opacity="0.25" stroke="var(--accent)" stroke-width="2" stroke-linejoin="round" />
            <polygon points="2,7 2,17 12,22 12,12" fill="var(--accent)" fill-opacity="0.15" stroke="var(--accent)" stroke-width="2" stroke-linejoin="round" />
            <polygon points="12,12 12,22 22,17 22,7" fill="var(--accent)" fill-opacity="0.15" stroke="var(--accent)" stroke-width="2" stroke-linejoin="round" />
            <circle cx="12" cy="12" r="3" fill="var(--accent)" />
          </svg>
        </div>
        <div class="auth-gate-title">
          <h1>CTOX Business OS</h1>
          <p>Melden Sie sich an, um eine Verbindung zur ctox-Instanz herzustellen.</p>
        </div>
      </header>

      ${pairingMissing ? `
        <div class="auth-gate-actions">
          <div class="auth-gate-error" data-gate-error>
            Business OS benötigt eine Pairing-Konfiguration mit sync_room und Signaling-URL.
          </div>
        </div>
      ` : `
      <form class="auth-gate-form" data-login-gate-form method="post" action="/login">
        <div class="auth-gate-field">
          <label for="gate-user">Benutzer</label>
          <div class="auth-gate-input-wrapper">
            <input
              id="gate-user"
              name="user"
              autocomplete="username"
              value="${escapeHtml(savedUser)}"
              placeholder="E-Mail oder Benutzername"
              class="auth-gate-input"
              required
            />
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path>
              <circle cx="12" cy="7" r="4"></circle>
            </svg>
          </div>
        </div>

        <div class="auth-gate-field">
          <label for="gate-password">Passwort</label>
          <div class="auth-gate-input-wrapper">
            <input
              id="gate-password"
              type="password"
              name="password"
              autocomplete="current-password"
              placeholder="Passwort"
              class="auth-gate-input"
              required
            />
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect>
              <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
            </svg>
          </div>
        </div>

        <div class="auth-gate-actions">
          <div class="auth-gate-error" data-gate-error hidden></div>
          <button class="auth-gate-button" type="submit" data-gate-submit>Einloggen &amp; Verbinden</button>
          ${loginUrl ? `<a class="auth-gate-external" href="${escapeHtml(loginUrl)}">Mit SSO einloggen</a>` : ''}
        </div>
      </form>
      `}

      <footer class="auth-gate-footer">
        <small>CTOX Business OS · Sichere Ende-zu-Ende verschlüsselte Verbindung.</small>
      </footer>
    </div>
  `;

  const form = container.querySelector('[data-login-gate-form]');
  if (!form) {
    els.host.replaceChildren(container);
    return;
  }
  const userInput = form.querySelector('input[name="user"]');
  const passwordInput = form.querySelector('input[name="password"]');
  const errorEl = form.querySelector('[data-gate-error]');
  const submitBtn = form.querySelector('[data-gate-submit]');

  const showGateError = (msg) => {
    errorEl.textContent = msg;
    errorEl.hidden = false;
  };

  if (options.loginFailed) {
    clearStoredBrowserAuth();
    showGateError("Ungültiger Benutzername oder Passwort.");
  }

  form.addEventListener('submit', (event) => {
    errorEl.hidden = true;

    const user = userInput.value.trim();
    const password = passwordInput.value;

    if (!user || !password) {
      event.preventDefault();
      showGateError("Bitte Benutzername und Passwort eingeben.");
      return;
    }

    clearStoredBrowserAuth();
    localStorage.removeItem(LOGGED_OUT_KEY);
    writeAccountPrefs({ loginUser: user });
    submitBtn.disabled = true;
    submitBtn.textContent = "Verbindung wird hergestellt...";
  });

  els.host.replaceChildren(container);

  // Autofocus handling: if username is prefilled, focus password, otherwise username.
  setTimeout(() => {
    if (userInput.value) {
      passwordInput.focus();
    } else {
      userInput.focus();
    }
  }, 50);
}

function getInstanceName() {
  const hostname = window.location.hostname;
  const hostLabel = hostname
    .replace(/\.ctox\.dev$/i, '')
    .replace(/\.localhost$/i, '')
    .trim();
  if (hostLabel && !['localhost', '127.0.0.1', '::1'].includes(hostLabel)) {
    return hostLabel.toUpperCase();
  }
  try {
    const injected = globalThis.CTOX_BUSINESS_OS_CONFIG || globalThis.ctoxBusinessOsLaunch?.config;
    if (injected?.instance_id) {
      return injected.instance_id.startsWith('biz_') ? injected.instance_id.substring(4, 10).toUpperCase() : injected.instance_id.substring(0, 6).toUpperCase();
    }
  }
  catch (e) {}
  try {
    const params = new URLSearchParams(window.location.search);
    const packed = params.get('ctox_config') || params.get('ctoxConfig');
    if (packed) {
      const decoded = JSON.parse(atob(packed));
      if (decoded && decoded.instance_id) {
        if (decoded.instance_id === 'biz_6ca27fe1-0186-49e8-8e30-24ac67b5e9bd') {
          return 'A6000';
        }
        return decoded.instance_id.startsWith('biz_') ? decoded.instance_id.substring(4, 10).toUpperCase() : decoded.instance_id.substring(0, 6).toUpperCase();
      }
    }
    const syncRoom = params.get('sync_room') || params.get('syncRoom');
    if (syncRoom) {
      const inst = syncRoom.replace(/^ctox-business-os:/, '').split(':')[0];
      if (inst === 'biz_6ca27fe1-0186-49e8-8e30-24ac67b5e9bd') {
        return 'A6000';
      }
      return inst.startsWith('biz_') ? inst.substring(4, 10).toUpperCase() : inst.substring(0, 6).toUpperCase();
    }
  } catch (e) {}
  try {
    const stored = localStorage.getItem('ctox.businessOs.pairingConfig');
    if (stored) {
      const parsed = JSON.parse(stored);
      if (parsed && parsed.instance_id) {
        if (parsed.instance_id === 'biz_6ca27fe1-0186-49e8-8e30-24ac67b5e9bd') {
          return 'A6000';
        }
        return parsed.instance_id.startsWith('biz_') ? parsed.instance_id.substring(4, 10).toUpperCase() : parsed.instance_id.substring(0, 6).toUpperCase();
      }
    }
  } catch (e) {}
  if (state.syncConfig && state.syncConfig.instance_id) {
    if (state.syncConfig.instance_id === 'biz_6ca27fe1-0186-49e8-8e30-24ac67b5e9bd') {
      return 'A6000';
    }
    return state.syncConfig.instance_id.startsWith('biz_') ? state.syncConfig.instance_id.substring(4, 10).toUpperCase() : state.syncConfig.instance_id.substring(0, 6).toUpperCase();
  }
  return 'A6000';
}

function renderAccountButton(session = state.session) {
  if (!els.accountButton) return;
  const labelNode = els.accountButton.querySelector('[data-account-label]');
  const user = session?.user || {};
  const instanz = getInstanceName();
  if (session?.authenticated) {
    const prefs = readAccountPrefs();
    const label = prefs.displayName || user.display_name || user.id || 'Account';
    const role = roleDisplayName(user.role || (user.is_admin ? 'admin' : 'user'));
    const userAtInstance = `${label}@${instanz}`;
    if (labelNode) labelNode.textContent = userAtInstance;
    els.accountButton.setAttribute('aria-label', `Account: ${label}, Rolle: ${role}, Instanz: ${instanz}`);
    els.accountButton.title = `Account: ${label} · Rolle: ${role} · Instanz: ${instanz}`;
    els.accountButton.dataset.authenticated = 'true';
  } else {
    if (labelNode) labelNode.textContent = `Login@${instanz}`;
    els.accountButton.setAttribute('aria-label', `Login öffnen für ${instanz}`);
    els.accountButton.title = `Login öffnen für ${instanz}`;
    els.accountButton.dataset.authenticated = 'false';
  }
}

function openAccountDrawer() {
  const content = state.session?.authenticated
    ? renderProfileDrawer()
    : renderLoginDrawer(state.session || {});
  els.rightDrawer.classList.add('account-popover');
  openDrawer('right', content);
}

function renderLoginDrawer(session) {
  const body = document.createElement('div');
  body.className = 'drawer-body account-drawer';
  const savedUser = readAccountPrefs().loginUser || '';
  const loginUrl = session.login_url || '';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Login</h2>
        <p>Bei Desktop-Start wird die CTOX-Instanz automatisch übernommen.</p>
      </div>
      <button class="icon-button" type="button" data-close-account aria-label="Schließen">×</button>
    </header>
    <form class="account-form" data-login-form method="post" action="/login">
      <label>
        <span>Benutzer</span>
        <input name="user" autocomplete="username" value="${escapeHtml(savedUser)}" placeholder="E-Mail oder Benutzername" />
      </label>
      <label>
        <span>Passwort</span>
        <input type="password" name="password" autocomplete="current-password" placeholder="Passwort" />
      </label>
      <button class="text-button account-primary" type="submit">Einloggen</button>
      ${loginUrl ? `<a class="text-button" href="${escapeHtml(loginUrl)}">Extern einloggen</a>` : ''}
    </form>
  `;
  body.querySelector('[data-close-account]')?.addEventListener('click', closeDrawers);
  body.querySelector('[data-login-form]')?.addEventListener('submit', (event) => {
    const form = new FormData(event.currentTarget);
    const user = form.get('user')?.toString().trim() || '';
    const password = form.get('password')?.toString() || '';
    if (!user || !password) {
      event.preventDefault();
      return;
    }
    clearStoredBrowserAuth();
    localStorage.removeItem(LOGGED_OUT_KEY);
    writeAccountPrefs({ loginUser: user });
  });
  return body;
}

function renderProfileDrawer() {
  const body = document.createElement('div');
  body.className = 'drawer-body account-drawer';
  const user = state.session?.user || {};
  const prefs = readAccountPrefs();
  const role = user.role || (user.is_admin ? 'admin' : 'user');
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Account</h2>
        <p>${escapeHtml(user.display_name || user.id || 'CTOX User')} · ${escapeHtml(roleDisplayName(role))}</p>
      </div>
      <button class="icon-button" type="button" data-close-account aria-label="Schließen">×</button>
    </header>
    <section class="account-role-card">
      <span>Rolle</span>
      <strong>${escapeHtml(roleDisplayName(role))}</strong>
      <small>${escapeHtml(roleDescription(role))}</small>
    </section>
    <form class="account-form" data-profile-form>
      <label>
        <span>Anzeigename</span>
        <input name="displayName" value="${escapeHtml(prefs.displayName || user.display_name || '')}" placeholder="Name" />
      </label>
      <label>
        <span>Standard-Sprache</span>
        <select name="language">
          <option value="de"${(prefs.language || document.documentElement.lang) === 'de' ? ' selected' : ''}>Deutsch</option>
          <option value="en"${(prefs.language || document.documentElement.lang) === 'en' ? ' selected' : ''}>English</option>
        </select>
      </label>
      <div class="account-actions">
        <button class="text-button account-primary" type="submit">Speichern</button>
        <button class="text-button" type="button" data-logout>Logout</button>
      </div>
      <small>Persönliche Einstellungen bleiben lokal und werden beim Laden der Module angewendet.</small>
    </form>
    <form class="account-form account-password-form" data-password-form>
      <label>
        <span>Aktuelles Passwort</span>
        <input type="password" name="currentPassword" autocomplete="current-password" />
      </label>
      <label>
        <span>Neues Passwort</span>
        <input type="password" name="newPassword" autocomplete="new-password" minlength="8" />
      </label>
      <label>
        <span>Neues Passwort wiederholen</span>
        <input type="password" name="confirmPassword" autocomplete="new-password" minlength="8" />
      </label>
      <div class="account-actions">
        <button class="text-button account-primary" type="submit">Passwort ändern</button>
      </div>
      <small data-password-status>Mindestens 8 Zeichen.</small>
    </form>
  `;
  body.querySelector('[data-close-account]')?.addEventListener('click', closeDrawers);
  body.querySelector('[data-profile-form]')?.addEventListener('submit', (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const prefs = {
      ...readAccountPrefs(),
      displayName: form.get('displayName')?.toString().trim() || '',
      language: form.get('language')?.toString() || 'de',
    };
    writeAccountPrefs(prefs);
    applyShellLanguage(prefs.language);
    syncHeaderControls();
    postCurrentPreferencesToModule();
    renderAccountButton({
      ...state.session,
      user: {
        ...(state.session?.user || {}),
        display_name: prefs.displayName || state.session?.user?.display_name || 'CTOX User',
      },
    });
    closeDrawers();
  });
  body.querySelector('[data-logout]')?.addEventListener('click', () => {
    clearStoredBrowserAuth();
    localStorage.removeItem(LOGGED_OUT_KEY);
    location.href = '/logout';
  });
  body.querySelector('[data-password-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const formEl = event.currentTarget;
    const statusEl = formEl.querySelector('[data-password-status]');
    const submit = formEl.querySelector('button[type="submit"]');
    const form = new FormData(formEl);
    const currentPassword = form.get('currentPassword')?.toString() || '';
    const newPassword = form.get('newPassword')?.toString() || '';
    const confirmPassword = form.get('confirmPassword')?.toString() || '';
    if (!currentPassword || !newPassword || !confirmPassword) {
      statusEl.textContent = 'Bitte alle Passwortfelder ausfüllen.';
      statusEl.dataset.state = 'error';
      return;
    }
    if (newPassword !== confirmPassword) {
      statusEl.textContent = 'Die neuen Passwörter stimmen nicht überein.';
      statusEl.dataset.state = 'error';
      return;
    }
    if (newPassword.length < 8) {
      statusEl.textContent = 'Das neue Passwort muss mindestens 8 Zeichen haben.';
      statusEl.dataset.state = 'error';
      return;
    }
    submit.disabled = true;
    statusEl.textContent = 'Passwort wird geändert...';
    statusEl.dataset.state = '';
    try {
      const response = await fetch('/account/password', {
        method: 'POST',
        body: form,
        credentials: 'same-origin',
      });
      if (!response.ok) {
        const payload = await response.json().catch(() => ({}));
        const messages = {
          invalid_current_password: 'Das aktuelle Passwort ist falsch.',
          password_too_short: 'Das neue Passwort muss mindestens 8 Zeichen haben.',
          invalid_input: 'Bitte prüfe die Passwortfelder.',
          auth_required: 'Bitte neu einloggen.',
        };
        throw new Error(messages[payload.error] || 'Passwort konnte nicht geändert werden.');
      }
      formEl.reset();
      statusEl.textContent = 'Passwort geändert.';
      statusEl.dataset.state = 'ok';
    } catch (error) {
      statusEl.textContent = error?.message || 'Passwort konnte nicht geändert werden.';
      statusEl.dataset.state = 'error';
    } finally {
      submit.disabled = false;
    }
  });
  return body;
}

function readAccountPrefs() {
  try {
    return JSON.parse(localStorage.getItem(ACCOUNT_PREFS_KEY) || '{}') || {};
  } catch {
    return {};
  }
}

function writeAccountPrefs(nextPrefs) {
  const prefs = { ...readAccountPrefs(), ...(nextPrefs || {}) };
  localStorage.setItem(ACCOUNT_PREFS_KEY, JSON.stringify(prefs));
  return prefs;
}

function clearStoredBrowserAuth() {
  localStorage.removeItem(SESSION_TOKEN_KEY);
  localStorage.removeItem(AUTH_HEADER_KEY);
}

function roleDisplayName(role) {
  const value = normalizeRole(role);
  return { chef: 'Chef', admin: 'Admin', founder: 'Founder', user: 'User' }[value] || value;
}

function roleDescription(role) {
  const value = normalizeRole(role);
  return {
    chef: 'Voller Zugriff auf Instanz, Nutzer, Policies und CTOX Steuerung.',
    admin: 'Verwaltet Module, Nutzer, Runtime und operative Einstellungen.',
    founder: 'Founder-Sicht mit Zugriff auf Review, Entscheidungen und Business-Kontext.',
    user: 'Normale Nutzung der freigegebenen Business-OS Module.',
  }[value] || 'Rolle dieser Business-OS Sitzung.';
}

function normalizeRole(role) {
  const value = String(role || '').trim().toLowerCase().replace(/^business_os_/, '');
  if (value === 'owner') return 'chef';
  if (['chef', 'admin', 'founder', 'user'].includes(value)) return value;
  return 'user';
}

function roleCanAdmin(role) {
  return ['chef', 'admin'].includes(normalizeRole(role));
}

function canModifyModule(mod) {
  if (!mod?.id) return false;
  const role = normalizeRole(state.session?.user?.role || (state.session?.user?.is_admin ? 'admin' : 'user'));
  if (['chef', 'admin'].includes(role)) return true;
  if (role !== 'founder') return false;
  const userId = state.session?.user?.id || '';
  const assignments = state.governance?.founders?.[mod.id] || [];
  return assignments.some((item) => item.user_id === userId && item.active !== false);
}

async function reportCurrentModule(details = {}) {
  const mod = details.module || state.activeModule;
  const { dispatchBusinessReport } = await loadBusinessReporterModule();
  const result = await dispatchBusinessReport({
    commandBus: createLiveCommandBusFacade(),
    session: state.session,
    module: mod,
    kind: details.kind || 'bug',
    severity: details.severity || 'medium',
    title: details.title || 'Business OS report',
    summary: details.summary || '',
    expected: details.expected || '',
    clientContext: {
      url: location.href,
      module_id: mod?.id || '',
      viewport: { width: innerWidth, height: innerHeight },
      user_agent: navigator.userAgent,
      source: 'business-os-shell',
    },
  });
  window.dispatchEvent(new CustomEvent('ctox-business-os-reports-updated', {
    detail: { reportId: result.report_id || '', moduleId: mod?.id || '' },
  }));
  return result;
}

function loadBusinessReporterModule() {
  if (!businessReporterModulePromise) {
    businessReporterModulePromise = import('./shared/business-reporter.js?v=20260520-rxdb-reports1');
  }
  return businessReporterModulePromise;
}

function loadBusinessChatModule() {
  if (!businessChatModulePromise) {
    businessChatModulePromise = import('./shared/business-chat.js?v=20260520-chat-ux-theme1');
  }
  return businessChatModulePromise;
}

function scheduleBusinessCompanions() {
  loadBusinessReporterModule()
    .then(({ initBusinessReporter }) => {
      initBusinessReporter({
        session: state.session,
        getActiveModule: () => state.activeModule,
        commandBus: createLiveCommandBusFacade(),
        db: createLiveDbFacade(),
      });
    })
    .catch((error) => {
      console.warn('[business-os] reporter surface lazy init failed', error);
    });
  loadBusinessChatModule()
    .then(({ initBusinessChat }) => {
      initBusinessChat({
        session: state.session,
        commandBus: createLiveCommandBusFacade(),
        db: createLiveDbFacade(),
        getActiveModule: () => state.activeModule,
      });
    })
    .catch((error) => {
      console.warn('[business-os] chat surface lazy init failed', error);
    });
}

function renderLeftContext(mod) {
  const wrap = document.createElement('div');
  wrap.className = 'list';
  for (const collection of mod.collections || []) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = collection;
    button.addEventListener('click', () => {
      openDrawer('left', drawerContent('Collection', collection));
    });
    wrap.append(button);
  }
  return wrap;
}

function renderRightContext(mod) {
  const wrap = document.createElement('div');
  wrap.className = 'list';
  for (const topic of ['Activity', 'Agent context', 'WebRTC sync']) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = topic;
    button.addEventListener('click', () => {
      if (topic === 'WebRTC sync') {
        openDrawer('right', renderSyncDiagnosticsDrawer());
        return;
      }
      openDrawer('right', drawerContent(topic, `${mod.title || mod.id} topic context`));
    });
    wrap.append(button);
  }
  return wrap;
}

function renderSyncDiagnosticsDrawer() {
  const diagnostics = state.syncDiagnostics || {};
  const config = state.sync?.config || {};
  const collections = Object.values(diagnostics.collections || {})
    .sort((a, b) => String(a.collection || '').localeCompare(String(b.collection || '')));
  const lastError = diagnostics.lastError || null;
  const body = document.createElement('div');
  body.className = 'drawer-body sync-diagnostics-drawer';
  body.dataset.drawerKind = 'sync-diagnostics';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>WebRTC Sync</h2>
        <p>${escapeHtml(syncDiagnosticSummary(diagnostics))}</p>
      </div>
      <button class="icon-button" type="button" data-close-sync-diagnostics aria-label="Schließen">×</button>
    </header>
    <section class="sync-diagnostics-grid">
      <div><span>Phase</span><strong>${escapeHtml(diagnostics.phase || 'unknown')}</strong></div>
      <div><span>Modus</span><strong>${escapeHtml(diagnostics.mode || 'webrtc')}</strong></div>
      <div><span>Role</span><strong>${escapeHtml(config.peer_role || 'browser')}</strong></div>
      <div><span>ICE</span><strong>${Number(diagnostics.iceServersConfigured || 0)}</strong></div>
    </section>
    <section class="sync-diagnostics-section">
      <span>Sync Room</span>
      <code>${escapeHtml(diagnostics.syncRoom || config.sync_room || 'unknown')}</code>
    </section>
    <section class="sync-diagnostics-section">
      <span>Signaling</span>
      <div class="sync-diagnostics-list">
        ${(diagnostics.signalingUrls || []).map((url) => `<code>${escapeHtml(url)}</code>`).join('') || '<small>Keine Signaling-URL konfiguriert.</small>'}
      </div>
    </section>
    ${lastError ? `
      <section class="sync-diagnostics-error">
        <span>Letzter Fehler</span>
        <strong>${escapeHtml(lastError.name || 'Error')}</strong>
        <p>${escapeHtml(lastError.message || String(lastError))}</p>
      </section>
    ` : ''}
    <section class="sync-diagnostics-section">
      <span>Collections</span>
      <div class="sync-diagnostics-collections">
        ${collections.map((item) => `
          <div class="sync-diagnostics-collection">
            <strong>${escapeHtml(item.collection || 'collection')}</strong>
            <span>${escapeHtml(item.status || 'unknown')}${item.active !== undefined ? ` · active=${Boolean(item.active)}` : ''}</span>
            ${item.reason ? `<small>${escapeHtml(item.reason)}</small>` : ''}
            ${item.lastError?.message ? `<small class="is-error">${escapeHtml(item.lastError.message)}</small>` : ''}
          </div>
        `).join('') || '<small>Noch keine Collection-Synchronisation gestartet.</small>'}
      </div>
    </section>
  `;
  body.querySelector('[data-close-sync-diagnostics]')?.addEventListener('click', closeDrawers);
  return body;
}

function syncDiagnosticSummary(diagnostics) {
  if (!diagnostics) return 'Noch keine Sync-Diagnostik verfügbar.';
  const collectionValues = Object.values(diagnostics.collections || {});
  const failed = collectionValues.filter((item) => ['failed', 'error'].includes(item.status)).length;
  const running = collectionValues.filter((item) => item.status === 'running' || item.status === 'connected').length;
  const reconnecting = collectionValues.filter((item) => item.status === 'reconnecting').length;
  const pending = collectionValues.filter((item) => item.status === 'pending' || item.status === 'starting' || item.status === 'connecting').length;
  if (failed) return `${failed} Collection${failed === 1 ? '' : 's'} mit Fehlern.`;
  if (reconnecting) return `${running} Collection${running === 1 ? '' : 's'} aktiv, ${reconnecting} im Reconnect.`;
  if (running) return `${running} Collection${running === 1 ? '' : 's'} aktiv, ${pending} im Aufbau.`;
  return `Phase ${diagnostics.phase || 'unknown'}, ${pending} Collection${pending === 1 ? '' : 's'} im Aufbau.`;
}

async function openTemplateStoreDrawer() {
  const body = document.createElement('div');
  body.className = 'drawer-body template-store-drawer';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Template Store</h2>
        <p>Füge eine kopierte Modulinstanz hinzu. CTOX kann diese Kopie danach frei verändern.</p>
      </div>
      <button class="icon-button" type="button" data-close-template-store aria-label="Schließen">×</button>
    </header>
    <div class="template-store-list" data-template-store-list>
      <div class="empty-state"><strong>Templates werden geladen</strong></div>
    </div>
  `;
  body.querySelector('[data-close-template-store]')?.addEventListener('click', closeDrawers);
  openDrawer('right', body);
  const list = body.querySelector('[data-template-store-list]');
  try {
    const payload = await loadTemplates();
    const templates = payload.templates || [];
    list.replaceChildren(...templates.map((template) => renderTemplateStoreItem(template)));
    if (!templates.length) {
      list.innerHTML = '<div class="empty-state"><strong>Keine Templates gefunden</strong><span>Lege Templates unter business-os/template-store an.</span></div>';
    }
  } catch (error) {
    list.innerHTML = `
      <div class="auth-gate-panel">
        <strong>Template Store nicht verfügbar</strong>
        <span>${escapeHtml(error.message || error)}</span>
      </div>
    `;
  }
}

function renderTemplateStoreItem(template) {
  const card = document.createElement('article');
  card.className = 'template-store-card';
  const installedCount = state.modules.filter((mod) => mod.id === template.id || mod.id.startsWith(`${template.id}-`)).length;
  const defaultTitle = template.default_title || template.title || template.id;
  const suggestedTitle = installedCount ? `${defaultTitle} ${installedCount + 1}` : defaultTitle;
  card.innerHTML = `
    <div>
      <strong>${escapeHtml(template.title || template.id)}</strong>
      <span>${escapeHtml(template.category || 'Template')}</span>
    </div>
    <p>${escapeHtml(template.description || '')}</p>
    <label>
      <span>Name der Kopie</span>
      <input value="${escapeHtml(suggestedTitle)}" data-template-title />
    </label>
    <div class="template-store-tags">
      ${(template.tags || []).map((tag) => `<span>${escapeHtml(tag)}</span>`).join('')}
    </div>
    <button class="text-button account-primary" type="button" data-install-template>Hinzufügen</button>
  `;
  card.querySelector('[data-install-template]')?.addEventListener('click', async (event) => {
    const button = event.currentTarget;
    button.disabled = true;
    button.textContent = 'Kopiere...';
    try {
      const title = card.querySelector('[data-template-title]')?.value?.trim() || suggestedTitle;
      const result = await installTemplate({ templateId: template.id, title });
      await refreshModules();
      closeDrawers();
      location.hash = result.module?.id || '';
      await openModule(result.module?.id || state.modules[0]?.id || 'ctox');
    } catch (error) {
      button.disabled = false;
      button.textContent = 'Fehlgeschlagen';
      const message = document.createElement('small');
      message.className = 'template-store-error';
      message.textContent = error.message || String(error);
      card.append(message);
    }
  });
  return card;
}

async function registerCustomModuleIcons() {
  const { registerSvgIcon } = await loadShellIconsModule();
  if (!Array.isArray(state.modules)) return;
  for (const mod of state.modules) {
    if (mod.layout?.icon_svg) {
      registerSvgIcon(mod.id, mod.layout.icon_svg);
    }
  }
}

async function refreshModules() {
  const modules = await loadModules();
  const nextModules = modules.modules || [];
  const currentIds = state.modules.map(m => m.id).join(',');
  const nextIds = nextModules.map(m => m.id).join(',');
  const nextFingerprint = modules.catalogFingerprint || '';
  if (nextFingerprint && nextFingerprint === state.moduleCatalogFingerprint) {
    return;
  }
  if (!nextFingerprint && currentIds === nextIds && state.governance === modules.governance) {
    return; // No actual change in module list or governance
  }
  state.modules = nextModules;
  state.moduleCatalogFingerprint = nextFingerprint || state.moduleCatalogFingerprint;
  await registerCustomModuleIcons();
  state.governance = modules.governance || state.governance;
  state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
  persistModuleLayout();
  renderTabs();
  state.backgroundModuleWorkScheduled = false;
  scheduleBackgroundModuleWork();
  refreshRemoteShellStateInBackground();

  // If the URL hash requests a module that wasn't previously loaded, but is now available, open it!
  const hashId = currentHashModuleId();
  if (hashId && hashId !== state.activeModule?.id) {
    const matched = state.modules.find((m) => m.id === hashId);
    if (matched) {
      console.log(`[business-os] URL hash #${hashId} is now available after catalog refresh. Opening module.`);
      await openModule(hashId);
    }
  }
}

function renderNavigationDrawer() {
  const body = document.createElement('div');
  body.className = 'drawer-body';
  body.innerHTML = '<h2>Modules</h2>';
  const list = document.createElement('div');
  list.className = 'list';
  for (const mod of state.modules) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = mod.title || mod.id;
    button.addEventListener('click', () => {
      closeDrawers();
      location.hash = mod.id;
      openModule(mod.id);
    });
    list.append(button);
  }
  body.append(list);
  return body;
}

function drawerContent(title, text) {
  const body = document.createElement('div');
  body.className = 'drawer-body';
  body.innerHTML = `<h2>${escapeHtml(title)}</h2><p>${escapeHtml(text)}</p>`;
  return body;
}

function openDrawer(side, content) {
  const target = side === 'left' ? els.leftDrawer : side === 'right' ? els.rightDrawer : els.bottomDrawer;
  if (side === 'right') {
    target.classList.remove('settings-drawer-open');
    if (!target.classList.contains('account-popover')) {
      target.classList.remove('account-popover');
    }
  }
  if (typeof content === 'string') {
    const temp = document.createElement('div');
    temp.innerHTML = content;
    target.replaceChildren(...temp.childNodes);
  } else {
    target.replaceChildren(content);
  }
  target.hidden = false;
  showBackdrop();
}

function showBackdrop() {
  els.backdrop.hidden = false;
}

function closeDrawers() {
  els.backdrop.hidden = true;
  els.leftDrawer.hidden = true;
  els.rightDrawer.hidden = true;
  els.bottomDrawer.hidden = true;
  els.rightDrawer.classList.remove('account-popover');
  els.rightDrawer.classList.remove('settings-drawer-open');
  els.leftDrawer.replaceChildren();
  els.rightDrawer.replaceChildren();
  els.bottomDrawer.replaceChildren();
}

function startShellCtoxHealthMonitor() {
  if (state.ctoxHealthTimer) window.clearInterval(state.ctoxHealthTimer);
  refreshShellCtoxHealth();
  state.ctoxHealthTimer = window.setInterval(refreshShellCtoxHealth, CTOX_HEALTH_POLL_MS);
}

async function refreshShellCtoxHealth() {
  try {
    const status = await loadShellCtoxHealth();
    state.ctoxHealth = status;
    renderShellCtoxWarning(status);
  } catch (error) {
    const status = {
      ok: false,
      pending: isPendingCtoxHealthError(error),
      error: error?.message || String(error),
    };
    state.ctoxHealth = status;
    renderShellCtoxWarning(status);
  }
}

function isPendingCtoxHealthError(error) {
  const message = String(error?.message || error || '');
  return message.includes('Runtime-Status wurde noch nicht synchronisiert')
    || message.includes('ctox_runtime_settings collection is required');
}

async function loadShellCtoxHealth() {
  const coll = state.db?.collection?.('ctox_runtime_settings');
  if (!coll) throw new Error('ctox_runtime_settings collection is required for shell health');
  await state.sync?.startCollection?.('ctox_runtime_settings');
  const doc = await coll.findOne('runtime-settings').exec();
  const runtime = doc?.toJSON?.();
  if (!runtime || runtime._deleted === true || runtime.is_deleted === true) {
    throw new Error('Runtime-Status wurde noch nicht synchronisiert.');
  }
  return {
    ok: runtime.ok !== false,
    source: 'rxdb',
    ctox_service: runtime.service || null,
    runtime_settings: runtime,
  };
}

function renderShellCtoxWarning(status) {
  if (!els.ctoxWarning) return;
  const problem = shellCtoxHealthProblem(status);
  if (!problem) {
    els.ctoxWarning.hidden = true;
    els.ctoxWarning.removeAttribute('title');
    document.body.dataset.ctoxOperational = 'ok';
    return;
  }
  els.ctoxWarning.hidden = false;
  els.ctoxWarning.textContent = shellText('ctoxNotWorking');
  els.ctoxWarning.title = problem;
  document.body.dataset.ctoxOperational = 'blocked';
}

function shellCtoxHealthProblem(status) {
  if (status?.pending) {
    return [shellText('ctoxStatusUnavailable'), status?.error].filter(Boolean).join(' ');
  }
  if (status?.source === 'rxdb' && status.ok === false) {
    return '';
  }
  if (!status || status.ok === false) {
    return [shellText('ctoxStatusUnavailable'), status?.error].filter(Boolean).join(' ');
  }
  const service = status.ctox_service;
  if (!service) return shellText('ctoxStatusUnavailable');
  if (service.running === false) {
    if (status.source === 'rxdb') return '';
    return shellText('ctoxStopped');
  }
  const lastError = String(service.last_error || '').trim();
  if (lastError) return `${shellText('ctoxLastError')}: ${lastError}`;
  return '';
}

async function loadSession() {
  const explicitLogout = localStorage.getItem(LOGGED_OUT_KEY) === '1';
  if (explicitLogout) {
    return {
      ok: true,
      authenticated: false,
      auth_required: true,
      reason: 'logged_out',
    };
  }

  const injected = readInjectedDesktopSession();
  if (injected) return injected;

  const pairedConfig = await readBusinessOsLaunchConfig();
  if (pairedConfig && allowsPairingConfigSession()) {
    const user = pairedConfig.session?.user || pairedConfig.user || {};
    const role = normalizeRole(user.role || 'user');
    return {
      ok: true,
      authenticated: true,
      auth_required: false,
      source: 'webrtc_pairing',
      user: {
        id: user.id || 'paired-user',
        display_name: user.display_name || user.name || user.id || 'Paired User',
        role,
        is_admin: roleCanAdmin(role),
        ...user,
      },
      reason: null,
    };
  }

  clearStoredBrowserAuth();

  return {
    ok: false,
    authenticated: false,
    auth_required: true,
    reason: 'pairing_config_missing',
  };
}

function allowsPairingConfigSession() {
  return isLocalBusinessOsSurface() || location.protocol === 'file:';
}

function readInjectedDesktopSession() {
  const candidates = [
    globalThis.CTOX_BUSINESS_OS_SESSION,
    globalThis.ctoxBusinessOsSession,
    globalThis.ctoxBusinessOsLaunch?.session,
    globalThis.CTOX_DESKTOP_SESSION,
    globalThis.ctoxDesktop?.session,
  ];
  const session = candidates.find((item) => item && typeof item === 'object');
  if (!session) return null;
  const user = session.user || {};
  const role = normalizeRole(user.role || (user.is_admin ? 'admin' : 'user'));
  return {
    ok: true,
    authenticated: session.authenticated !== false,
    auth_required: false,
    ...session,
    user: {
      id: user.id || 'ctox-desktop',
      display_name: user.display_name || user.name || user.id || 'CTOX Desktop',
      role,
      is_admin: roleCanAdmin(role),
      ...user,
    },
    reason: null,
  };
}

function isLocalBusinessOsSurface() {
  return ['127.0.0.1', 'localhost', '::1'].includes(location.hostname);
}

async function loadModules(options = {}) {
  const normalized = typeof options === 'number' ? { timeoutMs: options } : (options || {});
  const catalog = await loadModuleCatalog(normalized.timeoutMs, {
    allowShellSeed: normalized.allowShellSeed !== false,
  });
  const modules = await ensurePackagedModuleList(
    normalizeModuleList(catalog.modules),
    { allowShellSeed: normalized.allowShellSeed !== false }
  );
  return {
    ok: catalog.ok !== false,
    modules,
    governance: catalog.governance || null,
    catalogFingerprint: moduleCatalogFingerprint({ ...catalog, modules }),
  };
}

function moduleCatalogFingerprint(catalog) {
  if (!catalog || typeof catalog !== 'object') return '';
  try {
    return JSON.stringify({
      ok: catalog.ok !== false,
      modules: normalizeModuleList(catalog.modules),
      templates: Array.isArray(catalog.templates) ? catalog.templates : [],
      governance: catalog.governance || null,
    });
  } catch (error) {
    console.warn('[business-os] failed to fingerprint module catalog:', error);
    return '';
  }
}

async function loadModuleLayout() {
  return readModuleLayout();
}

async function loadTemplates() {
  const catalog = await loadModuleCatalog();
  return {
    ok: catalog.ok !== false,
    templates: Array.isArray(catalog.templates) ? catalog.templates : [],
  };
}

async function loadModuleCatalog(timeoutMs = 60000, options = {}) {
  const coll = state.db?.collection?.('business_module_catalog');
  if (!coll) throw new Error('business_module_catalog collection is required for shell module metadata');

  const cachedCatalog = await readModuleCatalogProjection(coll);
  const shellCatalog = options.allowShellSeed === false ? null : await loadPackagedModuleCatalog();

  if (cachedCatalog) {
    state.sync?.startCollection?.('business_module_catalog').catch((error) => {
      console.warn('[business-os] module catalog sync warmup failed after cached startup', error);
    });

    if (shellCatalog && Array.isArray(shellCatalog.modules)) {
      let changed = false;
      const mergedModules = [...(cachedCatalog.modules || [])];
      for (const shellMod of shellCatalog.modules) {
        if (!mergedModules.some(m => m.id === shellMod.id)) {
          mergedModules.push(shellMod);
          changed = true;
          if (!state.shellCatalogMergedIds.has(shellMod.id)) {
            state.shellCatalogMergedIds.add(shellMod.id);
            console.log(`[business-os] Merging missing packaged module locally: ${shellMod.id}`);
          }
        }
      }
    if (changed) {
        return normalizeModuleCatalog({ ...cachedCatalog, modules: mergedModules });
      }
    }
    return normalizeModuleCatalog(cachedCatalog);
  }

  const syncStart = state.sync?.startCollection?.('business_module_catalog');
  syncStart?.catch((error) => {
    console.warn('[business-os] module catalog sync start failed during shell seed startup', error);
  });

  if (shellCatalog) {
    try {
      await coll.insert(shellCatalog);
    } catch (err) {
      console.warn('[business-os] failed to insert initial packaged catalog into RxDB', err);
    }
    return normalizeModuleCatalog(shellCatalog);
  }

  await syncStart;
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const data = await readModuleCatalogProjection(coll);
      if (data) return normalizeModuleCatalog(data);
    } catch (error) {
      lastError = error;
    }
    await delay(300);
  }
  throw lastError || new Error('Modulkatalog wurde noch nicht synchronisiert.');
}

function normalizeModuleCatalog(catalog) {
  if (!catalog || typeof catalog !== 'object') return catalog;
  return {
    ...catalog,
    modules: normalizeModuleList(catalog.modules),
  };
}

function normalizeModuleList(modules) {
  if (!Array.isArray(modules)) return [];
  const seen = new Set();
  const normalized = [];
  for (const mod of modules) {
    const id = String(mod?.id || '').trim();
    if (!id) continue;
    const aliasTarget = LEGACY_MODULE_ALIASES.get(id);
    if (aliasTarget) {
      if (!modules.some((candidate) => candidate?.id === aliasTarget)) {
        normalized.push({ ...mod, id: aliasTarget, entry: 'modules/notes/index.html', collections: ['business_commands', 'notes'] });
      }
      continue;
    }
    if (seen.has(id)) continue;
    seen.add(id);
    normalized.push(mod);
  }
  return normalized;
}

async function ensurePackagedModuleList(modules, options = {}) {
  const normalized = normalizeModuleList(modules);
  if (options.allowShellSeed === false) return normalized;
  const shellCatalog = await loadPackagedModuleCatalog();
  if (!Array.isArray(shellCatalog?.modules) || shellCatalog.modules.length === 0) return normalized;
  const merged = [...normalized];
  for (const shellMod of normalizeModuleList(shellCatalog.modules)) {
    if (!merged.some((mod) => mod.id === shellMod.id)) merged.push(shellMod);
  }
  return normalizeModuleList(merged);
}

async function readModuleCatalogProjection(coll) {
  const doc = await coll.findOne('module-catalog').exec();
  const data = doc?.toJSON?.();
  if (data && data._deleted !== true && data.is_deleted !== true) return data;
  return null;
}

function getOfflineFallbackCatalog() {
  return {
    ok: true,
    modules: [
      {
        "id": "desktop",
        "title": "Desktop",
        "description": "Workspace landing surface with switchable Windows/macOS chrome, draggable icons, taskbar/dock, and live CTOX activity notifications.",
        "entry": "modules/desktop/index.html",
        "collections": [
          "business_commands",
          "desktop_icons",
          "desktop_layout",
          "desktop_notifications",
          "desktop_windows",
          "channel_pairing_state"
        ],
        "source": "core",
        "core": true,
        "editable": true,
        "deletable": false,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-desktop\"><defs><linearGradient id=\"grad-desktop\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#94a3b8\" /><stop offset=\"100%\" stop-color=\"#3b82f6\" /></linearGradient></defs><rect x=\"2\" y=\"3\" width=\"20\" height=\"14\" rx=\"3\" ry=\"3\" fill=\"url(#grad-desktop)\" fill-opacity=\"0.12\" stroke=\"url(#grad-desktop)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><path d=\"M12 17v4M8 21h8\" stroke=\"url(#grad-desktop)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><rect x=\"5\" y=\"6\" width=\"6\" height=\"4\" rx=\"1\" fill=\"url(#grad-desktop)\" fill-opacity=\"0.2\" stroke=\"url(#grad-desktop)\" stroke-width=\"1\"></rect><rect x=\"13\" y=\"6\" width=\"6\" height=\"8\" rx=\"1\" fill=\"url(#grad-desktop)\" fill-opacity=\"0.2\" stroke=\"url(#grad-desktop)\" stroke-width=\"1\"></rect><rect x=\"5\" y=\"12\" width=\"6\" height=\"2\" rx=\"0.5\" fill=\"url(#grad-desktop)\" fill-opacity=\"0.2\" stroke=\"url(#grad-desktop)\" stroke-width=\"1\"></rect></svg>",
          "left": "desktop scopes",
          "center": "desktop surface",
          "right": "agent context"
        }
      },
      {
        "id": "ctox",
        "title": "CTOX",
        "description": "Native control surface for queues, runs, sync state, and agent context.",
        "entry": "modules/ctox/index.html",
        "collections": [
          "business_commands",
          "business_chats",
          "ctox_queue_tasks",
          "ctox_runs",
          "ctox_bug_reports",
          "business_module_acl",
          "business_module_releases",
          "business_module_reports"
        ],
        "source": "core",
        "core": true,
        "editable": true,
        "deletable": false,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-ctox\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-ctox\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#10b981\" /><stop offset=\"100%\" stop-color=\"#06b6d4\" /></linearGradient></defs><polygon points=\"12 2 22 8 22 16 12 22 2 16 2 8\" fill=\"url(#grad-ctox)\" fill-opacity=\"0.12\" stroke=\"url(#grad-ctox)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polygon><polyline points=\"12 22 12 12 22 8\" stroke=\"url(#grad-ctox)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><polyline points=\"12 12 2 8\" stroke=\"url(#grad-ctox)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><polyline points=\"12 2 12 12\" stroke=\"url(#grad-ctox)\" stroke-width=\"1.5\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><circle cx=\"12\" cy=\"12\" r=\"3.5\" fill=\"url(#grad-ctox)\" stroke=\"#ffffff\" stroke-width=\"1\"></circle></svg>",
          "left": "runtime scopes",
          "center": "active workbench",
          "right": "agent context"
        }
      },
      {
        "id": "reports",
        "title": "Bugs & Features",
        "description": "Historical bug and feature request tracker with CTOX acceptance, change evidence, screenshots, and module rollback actions.",
        "entry": "modules/reports/index.html",
        "collections": [
          "business_module_reports",
          "ctox_bug_reports",
          "business_module_releases",
          "business_commands",
          "ctox_queue_tasks"
        ],
        "source": "core",
        "core": true,
        "editable": true,
        "deletable": false,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-reports\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-reports\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#ef4444\" /><stop offset=\"100%\" stop-color=\"#f97316\" /></linearGradient></defs><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\" fill=\"url(#grad-reports)\" fill-opacity=\"0.12\" stroke=\"url(#grad-reports)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><path d=\"M18 17V10M12 17V6M6 17v-4\" stroke=\"url(#grad-reports)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"12\" cy=\"6\" r=\"2\" fill=\"#ffffff\" stroke=\"url(#grad-reports)\" stroke-width=\"1.2\"></circle></svg>",
          "left": "bug and feature filters and history",
          "center": "report evidence, CTOX change log, and rollback"
        }
      },
      {
        "id": "documents",
        "title": "Documents",
        "description": "Native DOCX document workspace with document explorer, editor surface, and CTOX runbooks.",
        "entry": "modules/documents/index.html",
        "collections": [
          "business_commands",
          "documents",
          "document_versions",
          "document_blob_chunks",
          "document_runbooks"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-documents\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-documents\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#3b82f6\" /><stop offset=\"100%\" stop-color=\"#6366f1\" /></linearGradient></defs><path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z\" fill=\"url(#grad-documents)\" fill-opacity=\"0.12\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M14 2v4a2 2 0 0 0 2 2h4\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><line x1=\"8\" y1=\"12\" x2=\"16\" y2=\"12\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"8\" y1=\"16\" x2=\"16\" y2=\"16\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"8\" y1=\"8\" x2=\"10\" y2=\"8\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line></svg>",
          "left": "document navigation and explorer",
          "center": "DOCX viewer/editor workbench",
          "right": "document runbooks and automation prompts"
        }
      },
      {
        "id": "knowledge",
        "title": "Knowledge",
        "description": "Native CTOX Knowledge workspace for skillbooks, runbooks, markdown assets, and Polars-backed dataframes.",
        "entry": "modules/knowledge/index.html",
        "collections": [
          "business_commands",
          "knowledge_items",
          "knowledge_runbooks",
          "knowledge_tables"
        ],
        "source": "core",
        "core": true,
        "editable": true,
        "deletable": false,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-knowledge\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-knowledge\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#8b5cf6\" /><stop offset=\"100%\" stop-color=\"#d946ef\" /></linearGradient></defs><path d=\"M4 19.5A2.5 2.5 0 0 1 6.5 17H20\" stroke=\"url(#grad-knowledge)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z\" fill=\"url(#grad-knowledge)\" fill-opacity=\"0.12\" stroke=\"url(#grad-knowledge)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M12 2v10l2.5-2 2.5 2V2z\" fill=\"url(#grad-knowledge)\" fill-opacity=\"0.25\" stroke=\"url(#grad-knowledge)\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"9\" cy=\"12\" r=\"1.5\" fill=\"url(#grad-knowledge)\"></circle><circle cx=\"14\" cy=\"15\" r=\"1\" fill=\"url(#grad-knowledge)\"></circle></svg>",
          "left": "Knowledge selection and source groups",
          "center": "Markdown reader/editor and dataframe table tabs",
          "right": "Runbooks as operational knowledge layer"
        }
      },
      {
        "id": "research",
        "title": "Web Research",
        "description": "Knowledge-backed research dashboards with source scoring, portfolio maps, and CTOX systematic-research handoff.",
        "entry": "modules/research/index.html",
        "collections": [
          "business_commands",
          "business_chats",
          "ctox_queue_tasks",
          "research_tasks",
          "research_runs",
          "research_notes"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-research\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-research\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0891b2\" /><stop offset=\"100%\" stop-color=\"#10b981\" /></linearGradient></defs><path d=\"M6 3h12\" stroke=\"url(#grad-research)\" stroke-width=\"2\" stroke-linecap=\"round\"></path><path d=\"M8 3v4c0 1.66-1.34 3-3 3v0a7 7 0 0 0-2 4.9V20a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5.1a7 7 0 0 0-2-4.9v0c-1.66 0-3-1.34-3-3V3\" fill=\"url(#grad-research)\" fill-opacity=\"0.12\" stroke=\"url(#grad-research)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><line x1=\"8.5\" y1=\"11\" x2=\"15.5\" y2=\"11\" stroke=\"url(#grad-research)\" stroke-width=\"2\"></line><circle cx=\"12\" cy=\"16\" r=\"2.5\" fill=\"url(#grad-research)\"></circle><circle cx=\"9\" cy=\"18\" r=\"1\" fill=\"#ffffff\"></circle><circle cx=\"15\" cy=\"15\" r=\"1\" fill=\"#ffffff\"></circle></svg>",
          "left": "research tasks and scored source ranking",
          "center": "portfolio map and source evidence workbench",
          "right": "research task context, decisions, and CTOX handoff"
        }
      },
      {
        "id": "matching",
        "title": "Matching",
        "description": "Generic matching workspace with configurable source parsing, object parsing, and CTOX match tasks.",
        "entry": "modules/matching/index.html",
        "collections": [
          "matching_requirements",
          "matching_objects",
          "matching_results"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-matching\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-matching\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#f59e0b\" /><stop offset=\"100%\" stop-color=\"#ea580c\" /></linearGradient></defs><path d=\"M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71\" fill=\"url(#grad-matching)\" fill-opacity=\"0.12\" stroke=\"url(#grad-matching)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71\" fill=\"url(#grad-matching)\" fill-opacity=\"0.12\" stroke=\"url(#grad-matching)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"12\" cy=\"12\" r=\"2.5\" fill=\"#ffffff\" stroke=\"url(#grad-matching)\" stroke-width=\"1\"></circle></svg>",
          "left": "Requirement/source records and import task prompts",
          "center": "Configured matching task, queue state, and match results",
          "right": "Object pool records and import task prompts"
        }
      },
      {
        "id": "conversations",
        "title": "Conversations",
        "description": "Read-only audit surface for all CTOX communication across WhatsApp, Jami, Email, and MS Teams. Contact-centric timeline with channel-aware rendering and cross-links into Outbound, Matching, and Reports.",
        "entry": "modules/conversations/index.html",
        "collections": [
          "business_commands",
          "communication_accounts",
          "communication_threads",
          "communication_messages"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-conversations\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-conversations\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#4f46e5\" /><stop offset=\"100%\" stop-color=\"#7c3aed\" /></linearGradient></defs><path d=\"M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z\" fill=\"url(#grad-conversations)\" fill-opacity=\"0.12\" stroke=\"url(#grad-conversations)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"9\" cy=\"11\" r=\"1.5\" fill=\"url(#grad-conversations)\"></circle><circle cx=\"13\" cy=\"11\" r=\"1.5\" fill=\"url(#grad-conversations)\"></circle><circle cx=\"17\" cy=\"11\" r=\"1.5\" fill=\"url(#grad-conversations)\"></circle></svg>",
          "left": "Conversation list filtered by channel and search",
          "center": "Selected conversation timeline with channel-aware messages",
          "right": "Contact card, related business records, and CTOX agent attribution"
        }
      },
      {
        "id": "outbound",
        "title": "Outbound",
        "description": "Campaign source import, company qualification, and pipeline handoff for outbound sales workflows.",
        "entry": "modules/outbound/index.html",
        "collections": [
          "business_commands",
          "outbound_campaigns",
          "outbound_sources",
          "outbound_companies",
          "outbound_pipeline_items",
          "outbound_research_runs",
          "outbound_engagements",
          "outbound_messages",
          "outbound_approvals",
          "outbound_sequences",
          "outbound_sender_assignments",
          "outbound_meeting_requests",
          "outbound_suppression_entries",
          "outbound_account_limits"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-outbound\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-outbound\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#ec4899\" /><stop offset=\"100%\" stop-color=\"#f43f5e\" /></linearGradient></defs><line x1=\"22\" y1=\"2\" x2=\"11\" y2=\"13\" stroke=\"url(#grad-outbound)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><polygon points=\"22 2 15 22 11 13 2 9 22 2\" fill=\"url(#grad-outbound)\" fill-opacity=\"0.12\" stroke=\"url(#grad-outbound)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polygon><path d=\"M6 19c3-1 6-1 9-3\" stroke=\"url(#grad-outbound)\" stroke-width=\"1.5\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></path></svg>",
          "left": "campaign selection and source import",
          "center": "company qualification and pipeline workbench"
        }
      },
      {
        "id": "shiftflow",
        "title": "Einsatzplanung",
        "description": "Agentenunterstützte Einsatzplanung, Arbeitszeiterfassung und Urlaubsverwaltung für Teams mit Echtzeit-Synchronisation.",
        "entry": "modules/shiftflow/index.html",
        "collections": [
          "business_commands",
          "planning_employees",
          "planning_projects",
          "planning_shifts",
          "planning_time_records",
          "planning_absences"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-shiftflow\"><defs><linearGradient id=\"grad-shiftflow\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#8b5cf6\" /><stop offset=\"100%\" stop-color=\"#7c3aed\" /></linearGradient></defs><rect x=\"3\" y=\"4\" width=\"18\" height=\"16\" rx=\"3\" fill=\"url(#grad-shiftflow)\" fill-opacity=\"0.12\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><line x1=\"3\" y1=\"9\" x2=\"21\" y2=\"9\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"2\" stroke-linecap=\"round\"></line><line x1=\"9\" y1=\"9\" x2=\"9\" y2=\"20\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></line><line x1=\"15\" y1=\"9\" x2=\"15\" y2=\"20\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></line><rect x=\"5\" y=\"12\" width=\"8\" height=\"4\" rx=\"1.5\" fill=\"url(#grad-shiftflow)\" fill-opacity=\"0.3\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1\"></rect><circle cx=\"17\" cy=\"15\" r=\"2.5\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1.2\"></circle><polyline points=\"17 13.5 17 15 18 15\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1\" stroke-linecap=\"round\"></polyline></svg>",
          "left": "team status, absence scopes and department selection",
          "center": "interactive scheduler timeline and timesheet grid",
          "right": "AI roster planner, conflict alerts and timesheet inspector"
        }
      },
      {
        "id": "spreadsheets",
        "title": "Spreadsheets",
        "description": "Native XLSX spreadsheet workspace with spreadsheet explorer, spreadsheet editor surface based on JSpreadsheet, and CTOX runbooks.",
        "entry": "modules/spreadsheets/index.html",
        "collections": [
          "business_commands",
          "spreadsheets",
          "spreadsheet_versions",
          "spreadsheet_blob_chunks",
          "spreadsheet_runbooks"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-spreadsheets\"><defs><linearGradient id=\"grad-spreadsheets\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#10b981\" /><stop offset=\"100%\" stop-color=\"#059669\" /></linearGradient></defs><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\" fill=\"url(#grad-spreadsheets)\" fill-opacity=\"0.12\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><line x1=\"9\" y1=\"3\" x2=\"9\" y2=\"21\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"3\" y1=\"9\" x2=\"21\" y2=\"9\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"3\" y1=\"15\" x2=\"21\" y2=\"15\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><path d=\"M5 17l3-3 4 2 4-4\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"16\" cy=\"12\" r=\"1.5\" fill=\"#ffffff\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"1\"></circle></svg>",
          "left": "spreadsheet navigation and explorer",
          "center": "Spreadsheet viewer/editor workbench",
          "right": "spreadsheet runbooks and automation prompts"
        }
      },
      {
        "id": "notes",
        "title": "Notizen",
        "description": "Premium local-first markdown note workspace matching macOS Notes aesthetic.",
        "entry": "modules/notes/index.html",
        "collections": [
          "business_commands",
          "notes"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-notes\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-notes\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#eab308\" /><stop offset=\"100%\" stop-color=\"#d97706\" /></linearGradient></defs><path d=\"M16 2H4a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2z\" fill=\"url(#grad-notes)\" fill-opacity=\"0.12\" stroke=\"url(#grad-notes)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M2 6h2M2 10h2M2 14h2M2 18h2\" stroke=\"url(#grad-notes)\" stroke-width=\"1.5\" stroke-linecap=\"round\"></path><path d=\"M18.5 2.5a2.121 2.121 0 0 1 3 3L11 16l-4 1 1-4 10.5-10.5z\" fill=\"url(#grad-notes)\" fill-opacity=\"0.3\" stroke=\"url(#grad-notes)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path></svg>",
          "left": "Folders and note list",
          "center": "Markdown editor and rich text live preview",
          "right": "Command dashboard and formatting shortcuts"
        }
      },
      {
        "id": "creator",
        "title": "App Creator",
        "description": "Native standalone code-generator & harness workbench to visualize and test custom Business-OS modules.",
        "entry": "modules/creator/index.html",
        "collections": [
          "business_commands"
        ],
        "source": "core",
        "core": true,
        "editable": true,
        "deletable": false,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-creator\"><defs><linearGradient id=\"grad-creator\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#06b6d4\" /><stop offset=\"100%\" stop-color=\"#0891b2\" /></linearGradient></defs><polyline points=\"7 8 3 12 7 16\" stroke=\"url(#grad-creator)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><polyline points=\"17 8 21 12 17 16\" stroke=\"url(#grad-creator)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><line x1=\"14\" y1=\"6\" x2=\"10\" y2=\"18\" stroke=\"url(#grad-creator)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><path d=\"M18 4l.5 1.5L20 6l-1.5.5L18 8l-.5-1.5L16 6l1.5-.5z\" fill=\"url(#grad-creator)\"></path><path d=\"M6 18l.25.75L7 19l-.75.25L6 20l-.25-.75L5 19l.75-.25z\" fill=\"url(#grad-creator)\"></path></svg>",
          "left": "Harness configuration and parameter inputs",
          "center": "Architectural simulation flow, visual graphs, and code projections"
        }
      },
      {
        "id": "app-store",
        "title": "App Store",
        "description": "CTOX GitHub module catalog to discover repository apps, create apps from templates, and manage local Business OS installations.",
        "entry": "modules/app-store/index.html",
        "collections": [
          "business_commands",
          "business_module_catalog"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-app-store\"><defs><linearGradient id=\"grad-app-store\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#f59e0b\" /><stop offset=\"100%\" stop-color=\"#ec4899\" /></linearGradient></defs><path d=\"M21 8H3a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h18a2 2 0 0 0 2-2V10a2 2 0 0 0-2-2z\" fill=\"url(#grad-app-store)\" fill-opacity=\"0.12\" stroke=\"url(#grad-app-store)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M16 8A4 4 0 0 0 8 8\" stroke=\"url(#grad-app-store)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><rect x=\"5\" y=\"12\" width=\"5\" height=\"5\" rx=\"1\" fill=\"url(#grad-app-store)\" fill-opacity=\"0.25\" stroke=\"url(#grad-app-store)\" stroke-width=\"1.2\"></rect><rect x=\"14\" y=\"12\" width=\"5\" height=\"5\" rx=\"1\" fill=\"url(#grad-app-store)\" fill-opacity=\"0.25\" stroke=\"url(#grad-app-store)\" stroke-width=\"1.2\"></rect></svg>",
          "left": "Categories and Search",
          "center": "Available Applications Catalog",
          "right": "Application Details and Actions"
        }
      },
      {
        "id": "buchhaltung",
        "title": "Buchhaltung",
        "description": "Premium deutsches doppeltes Buchführungsmodul nach HGB mit SKR03/SKR04, UStVA/ELSTER, DATEV EXTF-Export und automatisiertem Bankabgleich.",
        "entry": "modules/buchhaltung/index.html",
        "collections": [
          "business_commands",
          "accounting_accounts",
          "accounting_journal_entries",
          "accounting_journal_entry_lines",
          "accounting_ledger_entries",
          "accounting_receipts",
          "accounting_bank_statements",
          "accounting_bank_statement_lines"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-buchhaltung\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-buchhaltung\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#818cf8\" /><stop offset=\"100%\" stop-color=\"#db2777\" /></linearGradient></defs><path d=\"M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z\" fill=\"url(#grad-buchhaltung)\" fill-opacity=\"0.12\" stroke=\"url(#grad-buchhaltung)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M8 11h8M8 15h5M9 7h6\" stroke=\"url(#grad-buchhaltung)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path>",
          "left": "Fibu-Navigationsstruktur & Kontenrahmen-Wähler",
          "center": "Aktiver Arbeitsbereich & Journale",
          "right": "Zugeordnete Belege, AI-Vorschläge & Begleitaktionen"
        }
      },
      {
        "id": "calendar",
        "title": "Kalender",
        "description": "Mac/Outlook-style calendar with native booking links and availability scheduling.",
        "entry": "modules/calendar/index.html",
        "collections": [
          "business_commands",
          "calendar_sources",
          "calendar_calendars",
          "calendar_events",
          "calendar_event_instances",
          "calendar_availability_rules",
          "calendar_booking_pages",
          "calendar_booking_holds",
          "calendar_bookings"
        ],
        "source": "local",
        "core": false,
        "editable": true,
        "deletable": true,
        "layout": {
          "shell": "full-workspace",
          "left": "Mini-Calendar & Lists",
          "center": "Calendar Grid",
          "right": "Inspector & Booking Pages",
          "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-calendar\"><defs><linearGradient id=\"grad-calendar\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#3b82f6\" /><stop offset=\"100%\" stop-color=\"#8b5cf6\" /></linearGradient></defs><rect x=\"3\" y=\"4\" width=\"18\" height=\"16\" rx=\"3\" ry=\"3\" fill=\"url(#grad-calendar)\" fill-opacity=\"0.12\" stroke=\"url(#grad-calendar)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><line x1=\"3\" y1=\"9\" x2=\"21\" y2=\"9\" stroke=\"url(#grad-calendar)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"9\" y1=\"9\" x2=\"9\" y2=\"20\" stroke=\"url(#grad-calendar)\" stroke-width=\"1.2\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></line><line x1=\"15\" y1=\"9\" x2=\"15\" y2=\"20\" stroke=\"url(#grad-calendar)\" stroke-width=\"1.2\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></line><path d=\"M8 2v3M16 2v3\" stroke=\"url(#grad-calendar)\" stroke-width=\"2\" stroke-linecap=\"round\"></path><rect x=\"5\" y=\"12\" width=\"3\" height=\"3\" rx=\"0.5\" fill=\"url(#grad-calendar)\" fill-opacity=\"0.3\" stroke=\"url(#grad-calendar)\" stroke-width=\"1\"></rect><rect x=\"10\" y=\"12\" width=\"4\" height=\"5\" rx=\"1\" fill=\"url(#grad-calendar)\" fill-opacity=\"0.3\" stroke=\"url(#grad-calendar)\" stroke-width=\"1\"></rect></svg>"
        }
      }
    ],
    id: 'module-catalog',
    updated_at_ms: Date.now(),
    templates: [],
    governance: null,
    source: 'business-os-shell-embedded-catalog',
  };
}

async function loadPackagedModuleCatalog() {
  try {
    const response = await fetch(`modules/registry.json?v=${APP_BUILD}`, { cache: 'no-store' });
    if (response.ok) {
      const catalog = await response.json();
      if (Array.isArray(catalog?.modules) && catalog.modules.length) {
        return {
          id: 'module-catalog',
          updated_at_ms: Date.now(),
          ok: catalog.ok !== false,
          modules: catalog.modules,
          templates: Array.isArray(catalog.templates) ? catalog.templates : [],
          governance: catalog.governance || null,
          source: 'business-os-shell',
        };
      }
    }
  } catch (error) {
    console.warn('[business-os] packaged module catalog seed unavailable; using embedded shell catalog', error);
  }
  return getOfflineFallbackCatalog();
}

async function installTemplate({ templateId, title }) {
  const command = await dispatchShellModuleCommand({
    commandType: 'ctox.module.install_template',
    moduleId: templateId,
    recordId: templateId,
    payload: {
      template_id: templateId,
      title,
    },
    source: 'business-os-shell-template-store',
  });
  return command.result || command;
}

async function dispatchShellModuleCommand({
  commandType,
  moduleId,
  recordId,
  payload,
  source,
}) {
  if (!state.commandBus?.dispatch || !state.db?.collection?.('business_commands')) {
    throw new Error('business_commands collection is required for module commands');
  }
  const generation = state.dataPlaneGeneration;
  const db = state.db;
  const commandBridge = await state.sync?.startCollection?.('business_commands');
  await waitForSyncBridgeReady(commandBridge, 15000);
  if (isStaleDataPlaneGeneration(generation)) {
    throw createRecoverableDataPlaneAbort('Business OS data plane was rebuilt before command dispatch.');
  }
  const commandId = `cmd_${newId()}`;
  await state.commandBus.dispatch({
    id: commandId,
    module: 'ctox',
    type: commandType,
    record_id: recordId || moduleId,
    inbound_channel: moduleId,
    payload,
    client_context: {
      source,
      module_id: moduleId,
      actor: actorContext(state.session),
    },
  });
  return waitForCommandProjection(db, commandId, 45000, generation);
}

async function waitForCommandProjection(db, commandId, timeoutMs = 45000, generation = state.dataPlaneGeneration) {
  const collection = db?.collection?.('business_commands');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (isStaleDataPlaneGeneration(generation)) {
      throw createRecoverableDataPlaneAbort(`Command ${commandId} was superseded by an RxDB/WebRTC repair.`);
    }
    let doc = null;
    try {
      doc = await collection?.findOne(commandId).exec();
    } catch (error) {
      if (isClosedRxDbCollectionError(error)) {
        throw createRecoverableDataPlaneAbort(`Command ${commandId} collection was closed by an RxDB/WebRTC repair.`);
      }
      throw error;
    }
    const data = doc?.toJSON?.();
    if (data && data.status && data.status !== 'pending_sync') {
      if (data.status === 'failed') throw new Error(data.error || `Command ${commandId} failed`);
      return data;
    }
    await delay(300);
  }
  throw new Error(`Command ${commandId} wurde nicht synchronisiert.`);
}

async function waitForSyncBridgeReady(bridge, timeoutMs = 15000) {
  const state = bridge?.state;
  if (!state) return;
  await Promise.race([
    Promise.resolve()
      .then(() => state.awaitInSync?.() || state.awaitInitialReplication?.())
      .catch(() => {}),
    delay(timeoutMs),
  ]);
}

function isStaleDataPlaneGeneration(generation) {
  return Number.isFinite(generation) && generation !== state.dataPlaneGeneration;
}

function createRecoverableDataPlaneAbort(message) {
  const error = new Error(message);
  error.code = 'CTOX_DATA_PLANE_REBUILT';
  return error;
}

function isRecoverableDataPlaneAbort(error) {
  return error?.code === 'CTOX_DATA_PLANE_REBUILT' || isClosedRxDbCollectionError(error);
}

function isClosedRxDbCollectionError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: COL21')
    || message.includes('collection is closed')
    || message.includes('closed collection');
}

function isTransientModuleLoadError(error) {
  const message = String(error?.message || error || '');
  return message.includes('Failed to fetch dynamically imported module')
    || message.includes('Importing a module script failed')
    || message.includes('net::ERR_CONNECTION_REFUSED')
    || message.includes('net::ERR_CONNECTION_RESET');
}

function actorContext(session) {
  const user = session?.user || {};
  return {
    id: user.id || '',
    display_name: user.display_name || user.name || user.id || '',
    role: user.role || 'user',
    is_admin: Boolean(user.is_admin),
  };
}

function newId() {
  return globalThis.crypto?.randomUUID?.() || `${Date.now()}_${Math.random().toString(36).slice(2)}`;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function loadSyncConfig() {
  const config = await readBusinessOsLaunchConfig();
  if (config) return config;

  throw new Error('Business OS WebRTC sync config is missing. Pair this browser with a CTOX instance first.');
}

async function readBusinessOsLaunchConfig() {
  const root = globalRoot();
  const storedPairingConfig = allowsStoredPairingConfig() ? readStoredPairingConfig() : null;
  if (!allowsStoredPairingConfig()) {
    clearStoredPairingConfig();
  }
  const launch = firstObject(
    readUrlPairingConfig(),
    root.CTOX_BUSINESS_OS_CONFIG,
    root.ctoxBusinessOsLaunch?.config,
    root.ctoxBusinessOsLaunch,
    window.CTOX_BUSINESS_OS_CONFIG,
    window.ctoxBusinessOsLaunch?.config,
    window.ctoxBusinessOsLaunch,
    storedPairingConfig,
  );
  const config = await normalizeBusinessOsLaunchConfig(launch);
  if (config && config.source === 'url' && allowsStoredPairingConfig()) {
    writeStoredPairingConfig(config);
    scrubPairingConfigFromUrl();
  } else if (config && config.source === 'url') {
    scrubPairingConfigFromUrl();
  }
  return config;
}

function allowsStoredPairingConfig() {
  return isLocalBusinessOsSurface() || location.protocol === 'file:';
}

function readUrlPairingConfig() {
  const params = launchUrlParams();
  const packed = params.get('ctox_config') || params.get('ctoxConfig');
  if (packed) {
    const parsed = parsePackedConfig(packed);
    if (parsed) return { ...parsed, source: 'url' };
  }
  const syncRoom = params.get('sync_room') || params.get('syncRoom');
  const signaling = params.get('signaling_url') || params.get('signalingUrl');
  const instanceId = params.get('instance_id') || params.get('instanceId');
  const roomPassword = params.get('room_password')
    || params.get('roomPassword')
    || params.get('signaling_room_password')
    || params.get('signalingRoomPassword');
  if ((!syncRoom && (!instanceId || !roomPassword)) || !signaling) return null;
  return {
    ok: true,
    source: 'url',
    app_hosting: 'web_deploy',
    sync_mode: 'p2p-first',
    instance_id: instanceId || syncRoom.replace(/^ctox-business-os:/, '').split(':')[0],
    peer_id: params.get('peer_id') || params.get('peerId') || '',
    native_peer_id: params.get('native_peer_id') || params.get('nativePeerId') || '',
    peer_role: 'browser',
    sync_room: syncRoom,
    signaling_room_password: roomPassword || '',
    signaling_urls: signaling.split(',').map((item) => item.trim()).filter(Boolean),
    transport: 'webrtc',
    http_bridge_available: false,
    ctox_instance_required: true,
    native_rxdb_peer_available: true,
    native_rxdb_peer_reason: '',
  };
}

function readStoredPairingConfig() {
  try {
    const raw = localStorage.getItem(PAIRING_CONFIG_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function writeStoredPairingConfig(config) {
  try {
    localStorage.setItem(PAIRING_CONFIG_KEY, JSON.stringify({ ...config, source: 'stored' }));
  } catch {}
}

function clearStoredPairingConfig() {
  try {
    localStorage.removeItem(PAIRING_CONFIG_KEY);
  } catch {}
}

function scrubPairingConfigFromUrl() {
  try {
    const url = new URL(location.href);
    const sensitiveKeys = [
      'ctox_config',
      'ctoxConfig',
      'sync_room',
      'syncRoom',
      'signaling_url',
      'signalingUrl',
      'instance_id',
      'instanceId',
      'room_password',
      'roomPassword',
      'signaling_room_password',
      'signalingRoomPassword',
      'peer_id',
      'peerId',
      'native_peer_id',
      'nativePeerId',
    ];
    let changed = false;
    for (const key of sensitiveKeys) {
      if (!url.searchParams.has(key)) continue;
      url.searchParams.delete(key);
      changed = true;
    }
    const hash = parseHashWithParams(url.hash);
    if (hash.params) {
      for (const key of sensitiveKeys) {
        if (!hash.params.has(key)) continue;
        hash.params.delete(key);
        changed = true;
      }
      url.hash = buildHashWithParams(hash.name, hash.params);
    }
    if (!changed) return;
    const next = `${url.pathname}${url.search}${url.hash}`;
    history.replaceState(history.state, document.title, next);
  } catch {}
}

function launchUrlParams() {
  const params = new URLSearchParams(location.search);
  const hash = parseHashWithParams(location.hash);
  if (hash.params) {
    for (const [key, value] of hash.params.entries()) {
      if (!params.has(key)) params.set(key, value);
    }
  }
  return params;
}

function parseHashWithParams(hashValue) {
  const raw = String(hashValue || '').replace(/^#/, '');
  const split = raw.indexOf('?');
  if (split < 0) return { name: raw, params: null };
  return {
    name: raw.slice(0, split),
    params: new URLSearchParams(raw.slice(split + 1)),
  };
}

function buildHashWithParams(name, params) {
  const query = params.toString();
  return query ? `${name}?${query}` : name;
}

function parsePackedConfig(value) {
  try {
    return JSON.parse(decodeBase64UrlJson(value));
  } catch {
    try {
      return JSON.parse(value);
    } catch {
      return null;
    }
  }
}

function decodeBase64UrlJson(value) {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  const normalized = String(value || '').replace(/-/g, '+').replace(/_/g, '/').replace(/\s/g, '');
  let buffer = 0;
  let bits = 0;
  const bytes = [];
  for (const char of normalized) {
    if (char === '=') break;
    const index = alphabet.indexOf(char);
    if (index < 0) throw new Error('Invalid base64url config payload.');
    buffer = (buffer << 6) | index;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      bytes.push((buffer >> bits) & 0xff);
    }
  }
  return decodeUtf8Bytes(bytes);
}

function decodeUtf8Bytes(bytes) {
  if (typeof TextDecoder !== 'undefined') {
    return new TextDecoder().decode(new Uint8Array(bytes));
  }
  let output = '';
  for (let i = 0; i < bytes.length; i += 1) {
    const byte = bytes[i];
    if (byte < 0x80) {
      output += String.fromCharCode(byte);
    } else if ((byte & 0xe0) === 0xc0) {
      const next = bytes[++i] || 0;
      output += String.fromCharCode(((byte & 0x1f) << 6) | (next & 0x3f));
    } else if ((byte & 0xf0) === 0xe0) {
      const next = bytes[++i] || 0;
      const last = bytes[++i] || 0;
      output += String.fromCharCode(((byte & 0x0f) << 12) | ((next & 0x3f) << 6) | (last & 0x3f));
    } else {
      const second = bytes[++i] || 0;
      const third = bytes[++i] || 0;
      const fourth = bytes[++i] || 0;
      const codePoint = ((byte & 0x07) << 18) | ((second & 0x3f) << 12) | ((third & 0x3f) << 6) | (fourth & 0x3f);
      const offset = codePoint - 0x10000;
      output += String.fromCharCode(0xd800 + (offset >> 10), 0xdc00 + (offset & 0x3ff));
    }
  }
  return output;
}

async function normalizeBusinessOsLaunchConfig(config) {
  if (!config || typeof config !== 'object') return null;
  const signalingUrls = Array.isArray(config.signaling_urls)
    ? config.signaling_urls
    : (Array.isArray(config.signalingUrls) ? config.signalingUrls : []);
  const instanceId = String(config.instance_id || config.instanceId || '').trim();
  const roomPassword = String(
    config.signaling_room_password
      || config.signalingRoomPassword
      || config.room_password
      || config.roomPassword
      || ''
  ).trim();
  const explicitSyncRoom = String(config.sync_room || config.syncRoom || '').trim();
  const syncRoom = explicitSyncRoom || await deriveSyncRoomFromPassword(instanceId, roomPassword);
  const urls = signalingUrls.map((url) => String(url || '').trim()).filter(Boolean);
  if (!syncRoom || !urls.length) return null;
  const iceServers = Array.isArray(config.ice_servers)
    ? config.ice_servers
    : (Array.isArray(config.iceServers) ? config.iceServers : []);
  return {
    ok: config.ok !== false,
    app_hosting: config.app_hosting || config.appHosting || 'web_deploy',
    sync_mode: config.sync_mode || config.syncMode || 'p2p-first',
    instance_id: instanceId || syncRoom.replace(/^ctox-business-os:/, '').split(':')[0],
    peer_id: config.peer_id || config.peerId || '',
    native_peer_id: config.native_peer_id || config.nativePeerId || '',
    peer_role: config.peer_role || config.peerRole || 'browser',
    sync_room: syncRoom,
    signaling_room_password: roomPassword,
    signaling_urls: urls,
    ice_servers: iceServers,
    iceServers,
    transport: 'webrtc',
    http_bridge_available: false,
    ctox_instance_required: config.ctox_instance_required !== false,
    native_rxdb_peer_available: config.native_rxdb_peer_available !== false,
    native_rxdb_peer_reason: config.native_rxdb_peer_reason || '',
    session: config.session || null,
    user: config.user || null,
    source: config.source || 'injected',
  };
}

async function deriveSyncRoomFromPassword(instanceId, roomPassword) {
  if (!instanceId || !roomPassword) return '';
  if (!globalThis.crypto?.subtle || typeof TextEncoder === 'undefined') {
    throw new Error('Business OS pairing requires WebCrypto to derive the WebRTC room from the room password.');
  }
  const bytes = new TextEncoder().encode(roomPassword);
  const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes);
  const secretId = base64UrlEncode(new Uint8Array(digest)).slice(0, 22);
  return `ctox-business-os:${instanceId}:${secretId}`;
}

function base64UrlEncode(bytes) {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '');
}

function firstObject(...items) {
  return items.find((item) => item && typeof item === 'object') || null;
}

function globalRoot() {
  return typeof globalThis === 'undefined' ? window : globalThis;
}

function refreshRemoteShellStateInBackground() {
  if (!state.session?.authenticated) return;
  window.setTimeout(() => {
    loadModules({ timeoutMs: 20000, allowShellSeed: false })
      .then((modules) => {
        if (!Array.isArray(modules?.modules) || !modules.modules.length) return;
        const nextModules = preserveCurrentShellModules(modules.modules, state.modules);
        const currentIds = state.modules.map((mod) => mod.id).join('\n');
        const nextIds = nextModules.map((mod) => mod.id).join('\n');
        const nextFingerprint = moduleCatalogFingerprint({ ok: modules.ok !== false, modules: nextModules, governance: modules.governance || null });
        if ((nextFingerprint && nextFingerprint === state.moduleCatalogFingerprint)
          || (!nextFingerprint && currentIds === nextIds)) return;
        state.modules = nextModules;
        state.moduleCatalogFingerprint = nextFingerprint || state.moduleCatalogFingerprint;
        registerCustomModuleIcons();
        state.governance = modules.governance || state.governance;
        state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
        persistModuleLayout();
        renderTabs();
      })
      .catch(() => {});
  }, 2000);
}

function preserveCurrentShellModules(remoteModules, currentModules) {
  const merged = normalizeModuleList(remoteModules);
  const seen = new Set(merged.map((mod) => mod.id));
  for (const mod of normalizeModuleList(currentModules)) {
    if (!isShellPackagedModule(mod) || seen.has(mod.id)) continue;
    merged.push(mod);
    seen.add(mod.id);
  }
  return normalizeModuleList(merged);
}

function isShellPackagedModule(mod) {
  const entry = String(mod?.entry || '');
  const source = String(mod?.source || '');
  return entry.startsWith('modules/')
    || source === 'business-os-shell'
    || source === 'business-os-shell-embedded-catalog';
}

function setStatus(text) {
  if (els.status) els.status.textContent = text;
}

function setStartupProgress(percent, statusText) {
  setStatus(statusText);

  const statusLabel = document.getElementById('startup-status-text');
  if (statusLabel) {
    statusLabel.textContent = statusText;
  }

  if (progressTimer) {
    clearInterval(progressTimer);
    progressTimer = null;
  }

  const progressBar = document.getElementById('startup-progress-bar');
  if (!progressBar) return;

  const startVal = currentProgress;
  const endVal = percent;

  // Choose duration proportional to step size to feel natural
  const stepDiff = Math.abs(endVal - startVal);
  const duration = Math.min(800, Math.max(250, stepDiff * 25));
  const intervalTime = 16; // ~60fps
  const totalSteps = Math.max(1, duration / intervalTime);
  let step = 0;

  progressTimer = setInterval(() => {
    step++;
    if (step <= totalSteps) {
      const t = step / totalSteps;
      const easeT = t * (2 - t); // Quadratic ease-out transition
      currentProgress = startVal + (endVal - startVal) * easeT;
      progressBar.style.width = `${currentProgress.toFixed(2)}%`;
    } else {
      // Creeping phase: asymptotic advance beyond target step to keep progress indicator active and alive
      if (endVal < 95) {
        const remainingCap = (endVal + 12) - currentProgress;
        if (remainingCap > 0) {
          currentProgress += remainingCap * 0.003;
          progressBar.style.width = `${currentProgress.toFixed(2)}%`;
        }
      } else {
        currentProgress = endVal;
        progressBar.style.width = `${currentProgress}%`;
        clearInterval(progressTimer);
        progressTimer = null;
      }
    }
  }, intervalTime);
}
window.setStartupProgress = setStartupProgress;

function getFriendlyErrorMessage(error) {
  const msg = error ? String(error.message || error) : '';

  let title = 'Unerwartetes Systemproblem';
  let description = 'Das Business OS konnte nicht vollständig geladen werden.';
  let advice = 'Bitte versuchen Sie die Seite neu zu laden. Falls das Problem weiterhin besteht, vergewissern Sie sich, dass der CTOX-Dienst im Hintergrund läuft.';

  if (msg.includes('WebCrypto') || msg.includes('subtle') || !globalThis.crypto?.subtle) {
    title = 'Sicherer Kontext erforderlich (WebCrypto fehlt)';
    description = 'Safari blockiert notwendige Verschlüsselungsfunktionen, wenn die Seite über die IP-Adresse "127.0.0.1" geladen wird.';
    advice = 'Bitte öffnen Sie die Anwendung über http://localhost:8765/ anstelle von http://127.0.0.1:8765/. Safari stuft "localhost" als sichere Herkunft ein und schaltet die benötigten Verschlüsselungsfunktionen (WebCrypto) frei.';
  } else if (msg.includes('pairing') || msg.includes('sync config is missing') || msg.includes('Pair this browser')) {
    title = 'Keine Kopplung vorhanden';
    description = 'Dieser Browser ist noch nicht mit einer aktiven CTOX-Instanz verbunden.';
    advice = 'Bitte öffnen Sie Business OS über den bereitgestellten Link aus Ihrer CTOX-Schnittstelle oder koppeln Sie die Instanz erneut.';
  } else if (msg.includes('IndexedDB lock') || msg.includes('timed out')) {
    title = 'Lokaler Speicher blockiert';
    description = 'Die Verbindung zum lokalen Datenspeicher konnte nicht rechtzeitig hergestellt werden.';
    advice = 'Möglicherweise ist das Business OS bereits in einem anderen Browser-Tab geöffnet. Bitte schließen Sie alle anderen geöffneten Tabs dieser Anwendung und versuchen Sie es erneut.';
  } else if (msg.includes('Schema-Drift') || msg.includes('DB6') || msg.includes('previousSchemaHash') || msg.includes('schemaHash') || msg.includes('drift')) {
    title = 'Datenstruktur-Aktualisierung';
    description = 'Die Struktur des lokalen Datenspeichers wird an die neue Version angepasst.';
    advice = 'Der lokale Speicher wird automatisch zurückgesetzt und neu synchronisiert. Bitte klicken Sie auf "Erneut versuchen", um fortzufahren.';
  } else if (msg.includes('modulkatalog') || msg.includes('business_module_catalog') || msg.includes('module catalog')) {
    title = 'Systemmodule konnten nicht geladen werden';
    description = 'Die Synchronisation der Systemmodule mit der CTOX-Hintergrundinstanz konnte nicht abgeschlossen werden.';
    advice = 'Bitte stellen Sie sicher, dass der CTOX-Hintergrunddienst aktiv läuft und eine stabile Netzwerkverbindung besteht.';
  } else if (msg.includes('Cannot access') && msg.includes('before initialization')) {
    title = 'Fehler in Skript-Reihenfolge';
    description = 'Eine Systemvariable wurde vor ihrer Initialisierung aufgerufen (Temporal Dead Zone).';
    advice = 'Dieses Ladeproblem wurde behoben. Bitte leeren Sie den Browser-Cache und klicken Sie auf "Erneut versuchen".';
  } else if (msg.includes('NetworkError') || msg.includes('Failed to fetch') || msg.includes('signaling')) {
    title = 'Netzwerkverbindung fehlgeschlagen';
    description = 'Der Signalisierungs-Server für die Echtzeit-Synchronisation konnte nicht erreicht werden.';
    advice = 'Bitte überprüfen Sie Ihre Netzwerkverbindung und stellen Sie sicher, dass die CTOX-Hintergrunddienste aktiv sind.';
  }

  return { title, description, advice };
}

function showStartupError(error) {
  console.error('[business-os] bootstrap error caught:', error);
  const errMsg = error ? (error.message || String(error)) : 'Unbekannter Fehler';

  if (progressTimer) {
    clearInterval(progressTimer);
    progressTimer = null;
  }
  currentProgress = 100;

  const statusLabel = document.getElementById('startup-status-text');
  if (statusLabel) {
    statusLabel.textContent = 'System-Start fehlgeschlagen.';
    statusLabel.classList.add('is-error');
  }

  const progressBar = document.getElementById('startup-progress-bar');
  if (progressBar) {
    progressBar.style.width = '100%';
    progressBar.classList.add('is-error');
  }

  const errorBody = document.querySelector('.error-body');
  if (errorBody) {
    const friendly = getFriendlyErrorMessage(error);
    errorBody.innerHTML = `
      <div class="friendly-error-info">
        <h4 class="friendly-error-title">${escapeHtml(friendly.title)}</h4>
        <p class="friendly-error-description">${escapeHtml(friendly.description)}</p>
        <div class="friendly-error-advice">
          <strong>Empfohlene Aktion:</strong>
          ${escapeHtml(friendly.advice)}
        </div>
      </div>
      <details class="technical-details-toggle">
        <summary>Technische Details (für Entwickler)</summary>
        <code class="error-msg-block" id="startup-error-msg"></code>
      </details>
    `;
  }

  const errorMsgBlock = document.getElementById('startup-error-msg');
  if (errorMsgBlock) {
    errorMsgBlock.textContent = error && error.stack ? error.stack : errMsg;
  }

  const errorCard = document.getElementById('startup-error-card');
  if (errorCard) {
    errorCard.removeAttribute('hidden');
  }

  const retryBtn = document.getElementById('startup-retry-btn');
  if (retryBtn) {
    retryBtn.onclick = () => {
      window.location.reload();
    };
  }
}
window.showStartupError = showStartupError;

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/* ==========================================================================
   PREMIUM APP LAUNCHER OVERLAY HELPERS & LOGIC
   ========================================================================== */

const DESKTOP_APP_SVGS = {
  explorer: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="svg-icon svg-explorer"><defs><linearGradient id="grad-explorer" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="#3b82f6" /><stop offset="100%" stop-color="#1d4ed8" /></linearGradient></defs><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" fill="url(#grad-explorer)" fill-opacity="0.15" stroke="url(#grad-explorer)"></path></svg>`,
  'code-editor': `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="svg-icon svg-code-editor"><defs><linearGradient id="grad-code-editor" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="#06b6d4" /><stop offset="100%" stop-color="#0891b2" /></linearGradient></defs><polyline points="16 18 22 12 16 6" stroke="url(#grad-code-editor)"></polyline><polyline points="8 6 2 12 8 18" stroke="url(#grad-code-editor)"></polyline><line x1="14" y1="4" x2="10" y2="20" stroke="url(#grad-code-editor)"></line></svg>`,
  'file-viewer': `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="svg-icon svg-file-viewer"><defs><linearGradient id="grad-file-viewer" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="#10b981" /><stop offset="100%" stop-color="#047857" /></linearGradient></defs><rect x="3" y="3" width="18" height="18" rx="2" fill="url(#grad-file-viewer)" fill-opacity="0.15" stroke="url(#grad-file-viewer)"></rect><line x1="9" y1="3" x2="9" y2="21" stroke="url(#grad-file-viewer)"></line></svg>`,
  creator: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="svg-icon svg-creator"><defs><linearGradient id="grad-creator-start" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="#f59e0b" /><stop offset="100%" stop-color="#ea580c" /></linearGradient></defs><circle cx="12" cy="12" r="3" stroke="url(#grad-creator-start)"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" stroke="url(#grad-creator-start)"></path></svg>`
};

const LAUNCHER_CATEGORIES = [
  {
    id: 'system',
    name: '🧠 System',
    matchIds: ['ctox', 'tickets', 'app-store', 'coding-agents']
  },
  {
    id: 'productivity',
    name: shellLang() === 'de' ? '⚡ Produktivität' : '⚡ Productivity',
    matchIds: ['explorer', 'notizen', 'notes', 'spreadsheets', 'documents', 'calendar', 'conversations']
  },
  {
    id: 'management',
    name: '📋 Management',
    matchIds: ['reports', 'shiftflow', 'buchhaltung', 'outbound']
  },
  {
    id: 'recherche',
    name: shellLang() === 'de' ? '🔍 Recherche & Daten' : '🔍 Web & Data',
    matchIds: ['research', 'matching', 'knowledge']
  },
  {
    id: 'development',
    name: shellLang() === 'de' ? '🛠️ Entwicklung' : '🛠️ Development',
    matchIds: ['code-editor', 'creator']
  }
];

function toggleStartMenu(event) {
  if (event) {
    event.preventDefault();
    event.stopPropagation();
  }
  let panel = document.querySelector('.shell-start-menu-panel');
  if (panel) {
    const isVisible = panel.classList.contains('is-active');
    if (isVisible) {
      hideStartMenu();
    } else {
      showStartMenu(panel);
    }
  } else {
    panel = createStartMenuElement();
    showStartMenu(panel);
  }
}
window.toggleStartMenu = toggleStartMenu;

function showStartMenu(panel) {
  // Hide default context menu if active
  state.contextMenu?.hide?.();
  panel.classList.add('is-active');
  
  const searchInput = panel.querySelector('.start-menu-search-input');
  if (searchInput) {
    searchInput.value = '';
    setTimeout(() => searchInput.focus(), 20);
  }
  filterStartMenu(panel, '');

  // Close when clicking outside
  const outsideClickListener = (evt) => {
    const startBtn = document.querySelector('[data-shell-start]');
    if (!panel.contains(evt.target) && (!startBtn || !startBtn.contains(evt.target))) {
      hideStartMenu();
      document.removeEventListener('mousedown', outsideClickListener, true);
    }
  };
  document.addEventListener('mousedown', outsideClickListener, true);
}

function hideStartMenu() {
  const panel = document.querySelector('.shell-start-menu-panel');
  if (panel) {
    panel.classList.remove('is-active');
  }
}

function createStartMenuElement() {
  const panel = document.createElement('div');
  panel.className = 'shell-start-menu-panel';
  panel.innerHTML = `
    <header class="start-menu-header">
      <div class="start-menu-search-wrapper">
        <svg class="start-menu-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
        </svg>
        <input type="text" class="start-menu-search-input" placeholder="${shellLang() === 'de' ? 'Suche nach Apps...' : 'Search apps...'}" />
      </div>
    </header>
    <div class="start-menu-body"></div>
    <footer class="start-menu-footer">
      <button class="start-menu-footer-btn show-desktop-btn" type="button">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke-width="2.2" stroke-linejoin="round" />
          <path d="M8 9h8M8 12h8M8 15h5" stroke-linecap="round" />
        </svg>
        <span>Desktop</span>
      </button>
      <button class="start-menu-footer-btn settings-btn" type="button">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3"></circle>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83 2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
        </svg>
        <span>${shellLang() === 'de' ? 'Einstellungen' : 'Settings'}</span>
      </button>
    </footer>
  `;

  // Footer actions
  panel.querySelector('.show-desktop-btn').addEventListener('click', () => {
    openDesktop();
    hideStartMenu();
  });
  panel.querySelector('.settings-btn').addEventListener('click', () => {
    openSettingsDrawer();
    hideStartMenu();
  });

  // Search events
  const searchInput = panel.querySelector('.start-menu-search-input');
  searchInput.addEventListener('input', (e) => {
    filterStartMenu(panel, e.target.value);
  });
  
  // Keyboard navigation in search
  searchInput.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      hideStartMenu();
    } else if (e.key === 'Enter') {
      const firstItem = panel.querySelector('.start-menu-item');
      if (firstItem) {
        firstItem.click();
      }
    }
  });

  document.body.appendChild(panel);
  return panel;
}

function filterStartMenu(panel, query) {
  const body = panel.querySelector('.start-menu-body');
  body.innerHTML = '';
  
  const targets = listLaunchTargets();
  const cleanQuery = query.trim().toLowerCase();
  
  const filtered = targets.filter(target => {
    if (!cleanQuery) return true;
    return target.title.toLowerCase().includes(cleanQuery) || target.id.toLowerCase().includes(cleanQuery);
  });

  if (filtered.length === 0) {
    body.innerHTML = `
      <div class="start-menu-empty">
        ${shellLang() === 'de' ? 'Keine passenden Apps gefunden.' : 'No matching apps found.'}
      </div>
    `;
    return;
  }

  // If query is active, render flat list of matches
  if (cleanQuery) {
    const categoryContainer = document.createElement('div');
    categoryContainer.className = 'start-menu-category';
    categoryContainer.innerHTML = `
      <div class="start-menu-category-title">
        <span>${shellLang() === 'de' ? 'Suchergebnisse' : 'Search Results'}</span>
      </div>
    `;
    filtered.forEach(target => {
      categoryContainer.appendChild(buildStartMenuItem(target));
    });
    body.appendChild(categoryContainer);
    return;
  }

  // Otherwise, render categorized layout
  const renderedIds = new Set();
  LAUNCHER_CATEGORIES.forEach(cat => {
    const catTargets = filtered.filter(target => cat.matchIds.includes(target.id));
    if (catTargets.length === 0) return;

    const categoryContainer = document.createElement('div');
    categoryContainer.className = 'start-menu-category';
    categoryContainer.innerHTML = `
      <div class="start-menu-category-title">
        <span>${cat.name}</span>
      </div>
    `;
    catTargets.forEach(target => {
      categoryContainer.appendChild(buildStartMenuItem(target));
      renderedIds.add(target.id);
    });
    body.appendChild(categoryContainer);
  });

  const uncategorized = filtered.filter((target) => !renderedIds.has(target.id));
  if (uncategorized.length) {
    const categoryContainer = document.createElement('div');
    categoryContainer.className = 'start-menu-category';
    categoryContainer.innerHTML = `
      <div class="start-menu-category-title">
        <span>${shellLang() === 'de' ? 'Weitere Apps' : 'More Apps'}</span>
      </div>
    `;
    uncategorized.forEach(target => {
      categoryContainer.appendChild(buildStartMenuItem(target));
    });
    body.appendChild(categoryContainer);
  }
}

function buildStartMenuItem(target) {
  const el = document.createElement('div');
  el.className = 'start-menu-item';
  
  const pinned = isTaskbarPinned(target.id);
  const iconMarkup = getLauncherIconSvg(target);
  
  el.innerHTML = `
    <div class="start-menu-item-left">
      <div class="start-menu-item-icon">
        ${iconMarkup}
      </div>
      <span class="start-menu-item-label">${target.title || target.id}</span>
    </div>
    <button class="start-menu-item-pin-btn ${pinned ? 'is-pinned' : ''}" type="button" title="${pinned ? (shellLang() === 'de' ? 'Von Bar lösen' : 'Unpin') : (shellLang() === 'de' ? 'An Bar anheften' : 'Pin')}">
      ${pinned ? '−' : '+'}
    </button>
  `;

  // Clicks
  el.addEventListener('click', (e) => {
    if (e.target.closest('.start-menu-item-pin-btn')) return;
    openLaunchTarget(target);
    hideStartMenu();
  });

  el.querySelector('.start-menu-item-pin-btn').addEventListener('click', (e) => {
    e.stopPropagation();
    toggleTaskbarPin(target.id, !pinned);
    // Re-render
    const panel = document.querySelector('.shell-start-menu-panel');
    const searchInput = panel?.querySelector('.start-menu-search-input');
    filterStartMenu(panel, searchInput?.value || '');
  });

  return el;
}

function getLauncherIconSvg(target) {
  if (target.kind === 'module' && target.module?.layout?.icon_svg) {
    return target.module.layout.icon_svg;
  }
  if (target.kind === 'app' && DESKTOP_APP_SVGS[target.id]) {
    return DESKTOP_APP_SVGS[target.id];
  }
  return `<span>${target.glyph || target.title.charAt(0)}</span>`;
}

let globalCtoxContextMenuEl = null;

function initGlobalCtoxContextMenu() {
  if (globalCtoxContextMenuEl) return;
  globalCtoxContextMenuEl = document.createElement('div');
  globalCtoxContextMenuEl.className = 'ctox-context-menu ctox-global-context-menu';
  globalCtoxContextMenuEl.hidden = true;
  document.body.appendChild(globalCtoxContextMenuEl);

  // Close when clicking outside
  document.addEventListener('click', (e) => {
    if (globalCtoxContextMenuEl && !globalCtoxContextMenuEl.contains(e.target)) {
      hideGlobalCtoxContextMenu();
    }
  }, { capture: true });

  // Close when pressing Escape
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      hideGlobalCtoxContextMenu();
    }
  });

  // Global capture phase listener
  document.addEventListener('contextmenu', handleGlobalContextMenu, true);
}

function handleGlobalContextMenu(event) {
  // Check if a full-screen module is active
  if (!state.activeModule || !moduleUsesFullWorkspace(state.activeModule)) {
    return;
  }

  const target = event.target;

  // Preserve native context menus for fields, links, editable divs, Monaco, etc.
  if (
    target.closest('input') ||
    target.closest('textarea') ||
    target.closest('select') ||
    target.closest('button') ||
    target.closest('a') ||
    target.closest('[contenteditable="true"]') ||
    target.closest('.monaco-editor') ||
    target.closest('.no-ctox-context')
  ) {
    return;
  }

  // Intercept the click!
  event.preventDefault();
  event.stopPropagation();

  const mod = state.activeModule;
  const context = extractGlobalCtoxContext(mod, target);
  
  showGlobalCtoxContextMenu(context, event.clientX, event.clientY);
}

function extractGlobalCtoxContext(mod, target) {
  const column = detectColumnFromElement(mod?.id, target);
  const record = detectRecordFromElement(mod?.id, target);
  const selectedText = String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000);
  const clickedText = String(target.innerText || target.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500);

  return {
    module: mod?.id || '',
    column,
    record_type: record?.type || 'module',
    record_id: record?.id || '',
    label: record?.label || mod?.title || mod?.id || '',
    selected_text: selectedText,
    clicked_text: clickedText
  };
}

function detectColumnFromElement(moduleId, element) {
  if (!element) return 'center';
  const el = element.nodeType === Node.ELEMENT_NODE ? element : element.parentElement;
  if (!el) return 'center';

  const leftSelector = '[class*="-left"], [class*="-sidebar"], [class*="-navigation"], [class*="-nav"], [class*="list-pane"], [class*="master-panel"], [id*="left"], [id*="sidebar"], .sidebar, .left-content, [data-left-content], [data-drawer-left]';
  const rightSelector = '[class*="-right"], [class*="-companion"], [class*="-auxiliary"], [class*="aside"], [class*="detail-pane"][class*="right"], [class*="preview"], [id*="right"], .right-content, [data-right-content], [data-drawer-right]';
  
  if (el.closest(leftSelector)) {
    return 'left';
  }
  if (el.closest(rightSelector)) {
    return 'right';
  }
  return 'center';
}

function detectRecordFromElement(moduleId, element) {
  if (!element) return null;
  let current = element.nodeType === Node.ELEMENT_NODE ? element : element.parentElement;
  
  const idAttributePatterns = [
    'data-id', 'data-note-id', 'data-report-id', 'data-account-id', 'data-booking-id',
    'data-document-id', 'data-folder-id', 'data-record-id', 'data-conversation-id',
    'data-node-id', 'data-sheet-id', 'data-task-id', 'data-event-id', 'data-project-id',
    'data-item-id', 'data-entity-id'
  ];

  while (current && current !== document.body) {
    // 1. Check ID attributes
    for (const attr of idAttributePatterns) {
      if (current.hasAttribute(attr)) {
        const val = current.getAttribute(attr);
        if (val) {
          let type = 'item';
          if (attr.startsWith('data-') && attr.endsWith('-id')) {
            const potentialType = attr.slice(5, -3);
            if (potentialType && potentialType !== 'id' && potentialType !== 'record') {
              type = potentialType;
            }
          }
          if (type === 'item') {
            const recordTypeAttr = current.closest('[data-record-type]');
            if (recordTypeAttr) {
              type = recordTypeAttr.getAttribute('data-record-type');
            } else {
              type = moduleId || 'item';
            }
          }
          return {
            type,
            id: val,
            label: deriveLabelFromElement(current)
          };
        }
      }
    }
    
    // 2. Fallback to ID with pattern
    const elementId = current.id || '';
    if (elementId && (elementId.includes('_') || elementId.length > 20)) {
      const parts = elementId.split('_');
      if (parts.length > 1 && parts[0].length > 2) {
        return {
          type: parts[0],
          id: elementId,
          label: deriveLabelFromElement(current)
        };
      }
    }
    
    current = current.parentElement;
  }
  return null;
}

function deriveLabelFromElement(el) {
  if (!el) return '';
  if (el.hasAttribute('data-title')) return el.getAttribute('data-title');
  if (el.hasAttribute('data-label')) return el.getAttribute('data-label');
  if (el.hasAttribute('data-name')) return el.getAttribute('data-name');
  
  const sub = el.querySelector('.title, .name, .label, .header, strong, h1, h2, h3, h4, h5, h6');
  if (sub) {
    const text = String(sub.textContent || sub.innerText).trim();
    if (text) return text;
  }
  
  const text = String(el.innerText || el.textContent).trim();
  if (text) {
    return text.split('\n')[0].slice(0, 60).trim();
  }
  return '';
}

function showGlobalCtoxContextMenu(context, x, y) {
  if (!globalCtoxContextMenuEl) return;
  
  const mod = state.activeModule || { id: 'ctox', title: 'CTOX' };
  const canModify = canModifyModule(mod);
  const lang = shellLang();
  
  const titleText = shellText('chatToCtox') || (lang === 'de' ? 'Mit CTOX chatten' : 'Chat to CTOX');
  const workDataLabel = shellText('chatWorkDataLabel') || (lang === 'de' ? 'Mit Daten arbeiten' : 'Work with data');
  const modifyAppLabel = shellText('chatModifyAppLabel') || (lang === 'de' ? 'App modifizieren' : 'Modify app');
  const placeholderText = shellText('chatPlaceholder') || (lang === 'de' ? 'Was soll CTOX hier tun oder prüfen?' : 'What should CTOX do or check here?');
  const sendLabel = shellText('send') || (lang === 'de' ? 'Senden' : 'Send');
  const closeLabel = lang === 'de' ? 'Schließen' : 'Close';
  const missingMsgLabel = lang === 'de' ? 'Nachricht fehlt.' : 'Message is missing.';
  const chatNotReadyLabel = lang === 'de' ? 'Chat ist noch nicht bereit.' : 'Chat is not ready.';
  const chatOpeningLabel = shellText('chatOpening') || (lang === 'de' ? 'Öffne Chat...' : 'Opening Chat...');

  const subtitle = context.label || shellText('moduleTitles')?.[mod.id] || mod.title || mod.id;
  
  globalCtoxContextMenuEl.innerHTML = `
    <form class="ctox-context-chat-form" novalidate>
      <header style="display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 2px;">
        <div style="min-width: 0; flex: 1;">
          <strong style="display: block; color: var(--text-strong, var(--text, #18222d)); font-size: 13px; font-weight: 800; line-height: 1.4; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">${escapeHtml(titleText)}</strong>
          <span style="display: block; color: var(--text-muted, var(--muted, #64747c)); font-size: 11px; font-weight: 600; line-height: 1.4; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">${escapeHtml(subtitle)}</span>
        </div>
        <button type="button" class="ctox-context-close-btn" aria-label="${escapeHtml(closeLabel)}" style="width: 28px; height: 28px; line-height: 24px; text-align: center; font-size: 20px; border: none; background: none; color: var(--text-muted, var(--muted, #64747c)); cursor: pointer; transition: color 0.2s ease; padding: 0;">×</button>
      </header>
      ${canModify ? `
        <div class="ctox-context-mode" role="radiogroup" aria-label="Aktion" style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px;">
          <label class="is-selected"><input type="radio" name="contextMode" value="data" checked style="display:none;" /><span>${escapeHtml(workDataLabel)}</span></label>
          <label><input type="radio" name="contextMode" value="app" style="display:none;" /><span>${escapeHtml(modifyAppLabel)}</span></label>
        </div>
      ` : ''}
      <textarea class="ctox-context-textarea" placeholder="${escapeHtml(placeholderText)}" style="width: 100%; box-sizing: border-box; min-height: 96px; max-height: 180px; border: 1px solid var(--line, #d8e1e5); border-radius: 8px; background: var(--surface-2, #eef3f7); color: var(--text, #18222d); font-family: inherit; font-size: 12.5px; line-height: 1.4; padding: 10px; resize: vertical; outline: none; transition: border-color 0.2s ease;"></textarea>
      <footer style="display: flex; align-items: center; justify-content: space-between; gap: 10px;">
        <span class="ctox-context-status" style="font-size: 11px; color: var(--text-muted, var(--muted, #64747c)); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;"></span>
        <button type="submit" class="ctox-context-submit-btn" style="flex: 0 0 auto; height: 32px; border: 1px solid var(--accent, #23665f); border-radius: 8px; background: color-mix(in srgb, var(--accent, #23665f) 10%, var(--surface, #fff)); color: var(--accent, #23665f); font-size: 12px; font-weight: 700; cursor: pointer; padding: 0 16px; transition: all 0.2s ease;">${escapeHtml(sendLabel)}</button>
      </footer>
    </form>
  `;

  globalCtoxContextMenuEl.hidden = false;
  
  // Clamp positioning
  globalCtoxContextMenuEl.style.left = '0px';
  globalCtoxContextMenuEl.style.top = '0px';
  const rect = globalCtoxContextMenuEl.getBoundingClientRect();
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  globalCtoxContextMenuEl.style.left = `${Math.min(maxLeft, Math.max(8, x))}px`;
  globalCtoxContextMenuEl.style.top = `${Math.min(maxTop, Math.max(8, y))}px`;

  const form = globalCtoxContextMenuEl.querySelector('form');
  const textarea = globalCtoxContextMenuEl.querySelector('.ctox-context-textarea');
  const statusEl = globalCtoxContextMenuEl.querySelector('.ctox-context-status');
  const closeBtn = globalCtoxContextMenuEl.querySelector('.ctox-context-close-btn');
  
  closeBtn.addEventListener('click', () => {
    hideGlobalCtoxContextMenu();
  });

  if (canModify) {
    const labels = globalCtoxContextMenuEl.querySelectorAll('.ctox-context-mode label');
    labels.forEach(label => {
      label.addEventListener('click', () => {
        labels.forEach(l => l.classList.remove('is-selected'));
        label.classList.add('is-selected');
        const input = label.querySelector('input');
        if (input) input.checked = true;
      });
    });
  }

  const closeBtnHover = () => { closeBtn.style.color = 'var(--text-strong)'; };
  const closeBtnOut = () => { closeBtn.style.color = 'var(--text-muted)'; };
  closeBtn.addEventListener('mouseenter', closeBtnHover);
  closeBtn.addEventListener('mouseleave', closeBtnOut);

  textarea.addEventListener('focus', () => {
    textarea.style.borderColor = 'var(--accent, #23665f)';
    textarea.style.boxShadow = '0 0 0 2px color-mix(in srgb, var(--accent, #23665f) 20%, transparent)';
  });
  textarea.addEventListener('blur', () => {
    textarea.style.borderColor = 'var(--line, #d8e1e5)';
    textarea.style.boxShadow = 'none';
  });

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const prompt = textarea.value.trim();
    if (!prompt) {
      if (statusEl) statusEl.textContent = missingMsgLabel;
      return;
    }

    if (!document.querySelector('[data-ctox-chat-root]')) {
      if (statusEl) statusEl.textContent = chatNotReadyLabel;
      return;
    }

    if (statusEl) statusEl.textContent = chatOpeningLabel;

    const mode = canModify ? (new FormData(form).get('contextMode') || 'data') : 'data';
    const title = mode === 'app' ? `${mod.title || mod.id} App modifizieren` : `Kontext-Aufgabe · ${subtitle}`;
    const instruction = mode === 'app' 
      ? `Modifiziere die ${mod.title || mod.id}-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, App-Daten selbst nicht als primäres Ziel verändern.\n\n${prompt}`
      : prompt;

    window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
      detail: {
        text: prompt,
        module: mod.id,
        source_title: mod.title || mod.id,
        command_type: mode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
        record_id: mode === 'app' ? mod.id : (context.record_id || mod.id),
        title,
        instruction,
        payload: {
          title,
          instruction,
          prompt,
          user_message: prompt,
          mode,
          target: mode === 'app' ? 'app' : 'data',
          context: {
            module: mod.id,
            column: context.column,
            record_type: context.record_type,
            record_id: context.record_id,
            label: context.label || mod.title || mod.id,
            selected_text: context.selected_text,
            clicked_text: context.clicked_text,
          },
          thread_key: `business-os/${mod.id}`,
        },
        client_context: {
          action: 'context-chat',
          mode,
          column: context.column,
          record_type: context.record_type,
          record_id: context.record_id,
        }
      }
    }));

    hideGlobalCtoxContextMenu();
  });

  requestAnimationFrame(() => {
    textarea.focus();
  });
}

function hideGlobalCtoxContextMenu() {
  if (globalCtoxContextMenuEl) {
    globalCtoxContextMenuEl.hidden = true;
  }
}
