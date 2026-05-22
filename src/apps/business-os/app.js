import { createBusinessDb, resetBusinessDb } from './shared/db.js?v=20260522-rxdb-fork1';
import { createSyncRuntime } from './shared/sync.js?v=20260522-replication-io1';
import { createCommandBus } from './shared/command-bus.js?v=20260521-rxdb-db32';
import { openReactSettings } from './shared/react-settings.js?v=20260518-runtime-auth-oauth3';
import { dispatchBusinessReport, initBusinessReporter } from './shared/business-reporter.js?v=20260520-rxdb-reports1';
import { initBusinessChat } from './shared/business-chat.js?v=20260520-chat-ux-theme1';
import { createEventBus } from './shared/event-bus.js?v=20260519-shell-os1';
import { createNotifications } from './shared/notifications.js?v=20260519-shell-os1';
import { createContextMenu } from './shared/context-menu.js?v=20260519-shell-os1';
import { createWindowManager } from './shared/window-manager.js?v=20260519-shell-os1';
import { createTaskbar } from './shared/taskbar.js?v=20260519-shell-os1';
import { createWindowSwitcher } from './shared/window-switcher.js?v=20260519-shell-os1';
import { installBusinessDialogFallbacks } from './shared/dialogs.js?v=20260519-dialogs1';
import { getSvgIcon, registerSvgIcon } from './shared/icons.js?v=20260520-svg-icons2';
import { collections as ctoxCollections, migrationStrategies as ctoxMigrationStrategies } from './modules/ctox/schema.js';
import { collections as desktopCollections, migrationStrategies as desktopMigrationStrategies } from './modules/desktop/schema.js';

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
const APP_BUILD = '20260522-replication-io1';
const BUSINESS_DB_NAME = 'ctox_business_os_v10';
const RXDB_BOOTSTRAP_VERSION = '20260521-rxdb-db13';
const CTOX_HEALTH_POLL_MS = 10000;
const SYNC_RECOVERY_REPAIR_DELAY_MS = 15000;
const DEFAULT_TASKBAR_PIN_IDS = ['ctox', 'documents', 'spreadsheets', 'explorer', 'knowledge', 'app-store', 'research'];
let moduleLayoutSaveTimer = null;
let taskbarPinSaveTimer = null;
let shellColumnResizeSync = null;
let syncRecoveryRepairTimer = null;
let syncRecoveryRepairRunning = false;

const SHELL_COL_MIN = {
  left: 210,
  center: 420,
  right: 260,
};

const SHELL_COL_SIDE_MAX = 620;

const state = {
  modules: [],
  activeModule: null,
  moduleRevisions: {},
  navHistory: [],
  navIndex: -1,
  navTransitioning: false,
  activeUnmount: null,
  db: null,
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
};

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
  smokeRoot.ctoxBusinessOsSmoke = { state };
  window.ctoxBusinessOsSmoke = { state };
}

const moduleAliases = {};

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
    moduleTitles: {
      desktop: 'Desktop',
      ctox: 'CTOX',
      documents: 'Dokumente',
      spreadsheets: 'Tabellen',
      knowledge: 'Knowledge',
      'matching': 'Matching',
      reports: 'Bugs & Features',
      research: 'Web Research',
      conversations: 'Conversations',
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
    moduleTitles: {
      desktop: 'Desktop',
      ctox: 'CTOX',
      documents: 'Documents',
      spreadsheets: 'Spreadsheets',
      knowledge: 'Knowledge',
      'matching': 'Matching',
      reports: 'Bugs & Features',
      research: 'Web Research',
      conversations: 'Conversations',
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
  console.error(error);
  if (await recoverFromLocalRxDbSchemaDrift(error)) return;
  showStartupError(error);
});

async function bootstrap() {
  installBusinessDialogFallbacks();
  const prefs = readAccountPrefs();
  applyShellTheme(prefs.theme || 'dark', { persist: false });
  applyShellLanguage(prefs.language || 'de', { persist: false });
  applyShellStyle(prefs.shellStyle || 'windows', { persist: false });
  syncHeaderControls();
  wireShellActions();

  setStartupProgress(10, 'Initialisiere Systemeinstellungen...');

  setStartupProgress(30, 'Sitzungsdaten werden überprüft...');
  const session = await loadSession();
  state.session = session;
  renderAccountButton(session);
  if (!session.authenticated) {
    const loginFailed = session.reason === 'invalid_credentials'
      || new URLSearchParams(location.search).has('loginFailed');
    clearStoredBrowserAuth();
    renderLoginGate(session, { loginFailed });
    setStatus(shellText('loginRequired'));
    return;
  }

  setStartupProgress(50, 'Lokale Datenbank wird geladen...');
  const syncConfig = await loadSyncConfig();
  await resetBusinessDataPlaneForBuildIfNeeded();
  await openBusinessDataPlane(syncConfig);

  setStartupProgress(70, 'Verbindung zu Sync-Peers wird hergestellt...');
  let modules;
  try {
    setStartupProgress(85, 'Lade verfügbare Module & Anwendungsmanifeste...');
    modules = await loadModules();
  } catch (error) {
    if (!isModuleCatalogSyncError(error)) throw error;
    console.warn('[business-os] module catalog sync stalled; resetting local RxDB cache and retrying WebRTC sync', error);
    setStartupProgress(80, 'Lokale RxDB wird neu synchronisiert...');
    await repairBusinessDataPlane(syncConfig);
    modules = await loadModules(20000);
  }
  state.modules = modules.modules || [];
  registerCustomModuleIcons();
  state.governance = modules.governance || null;
  state.moduleLayout = normalizeModuleLayout(await loadModuleLayout(), state.modules);
  state.taskbarPins = normalizeTaskbarPins(readTaskbarPins(), state.modules);
  persistModuleLayout();
  renderTabs();
  state.eventBus = createEventBus();
  state.contextMenu = createContextMenu({
    host: document.body,
    viewportEl: document.documentElement,
  });
  state.notifications = createNotifications({
    container: els.shellNotifications,
    t: (key, fallback) => shellText(key) || fallback || key,
  });
  const snapPreviewEl = document.createElement('div');
  snapPreviewEl.className = 'shell-snap-preview';
  snapPreviewEl.hidden = true;
  els.shellWindowLayer.appendChild(snapPreviewEl);
  state.windowManager = createWindowManager({
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
  state.windowManager.setInsets({ top: 0, bottom: els.shellTaskbar ? 54 : 0 });
  if (els.shellTaskbar) {
    state.taskbar = createTaskbar({
      container: els.shellTaskbar,
      windowManager: state.windowManager,
      eventBus: state.eventBus,
      t: (key, fallback) => shellText(key) || fallback || key,
      ownerLabelFor: deriveOwnerLabel,
    });
  }
  if (els.shellSwitcherOverlay && els.shellSwitcherPanel) {
    state.windowSwitcher = createWindowSwitcher({
      overlay: els.shellSwitcherOverlay,
      panel: els.shellSwitcherPanel,
      windowManager: state.windowManager,
      ownerLabelFor: deriveOwnerLabel,
      t: (key, fallback) => shellText(key) || fallback || key,
    });
  }
  wireShellWindowGestures();
  setStatus(shellText('localWorkspace'));
  initBusinessReporter({
    session: state.session,
    getActiveModule: () => state.activeModule,
    commandBus: createLiveCommandBusFacade(),
    db: createLiveDbFacade(),
  });
  initBusinessChat({
    session: state.session,
    commandBus: createLiveCommandBusFacade(),
    db: createLiveDbFacade(),
    getActiveModule: () => state.activeModule,
  });

  setStartupProgress(95, 'Workspace ist bereit. Öffne Standardmodul...');
  try {
    await openModule(currentHashModuleId() || state.modules[0]?.id || 'ctox');
    setStatus(shellText('localWorkspace'));
  } catch (error) {
    console.error('[business-os] module startup failed', error);
    setStatus(`Module startup failed: ${error.message || error}`);
  }
  scheduleBackgroundModuleWork();
  refreshRemoteShellStateInBackground();
}

async function resetBusinessDataPlaneForBuildIfNeeded() {
  if (localStorage.getItem(RXDB_BOOTSTRAP_VERSION_KEY) === RXDB_BOOTSTRAP_VERSION) return;
  setStatus('Lokale RxDB wird neu synchronisiert');
  await resetBusinessDb({ name: BUSINESS_DB_NAME });
  localStorage.setItem(RXDB_BOOTSTRAP_VERSION_KEY, RXDB_BOOTSTRAP_VERSION);
}

async function openBusinessDataPlane(syncConfig) {
  setStartupProgress(51, 'Lokale Datenbank-Konfiguration wird vorbereitet...');
  state.syncConfig = syncConfig;

  setStartupProgress(54, 'Verbindung zur lokalen IndexedDB-Instanz wird geöffnet...');
  state.db = await createBusinessDb({ name: BUSINESS_DB_NAME });

  setStartupProgress(58, 'Systemtabellen und reaktive Schemata werden registriert...');
  await registerCoreCollections();

  setStartupProgress(62, 'Desktop-Layout und Fensterkonfiguration werden geladen...');
  await hydrateTaskbarPinsFromDesktopLayout();
  renderTabs();

  setStartupProgress(66, 'Reaktive Daten-Synchronisation (WebRTC) wird initialisiert...');
  state.sync = createSyncRuntime({
    db: state.db,
    config: syncConfig,
    onDiagnostic: updateSyncDiagnostics,
  });

  setStartupProgress(69, 'Offline-First Befehls-Bus wird gestartet...');
  state.commandBus = createCommandBus({
    db: () => state.db,
    config: syncConfig,
  });
  startShellCtoxHealthMonitor();
}

async function repairBusinessDataPlane(syncConfig) {
  state.dataPlaneGeneration += 1;
  clearSyncRecoveryRepairTimer();
  if (state.ctoxHealthTimer) {
    window.clearInterval(state.ctoxHealthTimer);
    state.ctoxHealthTimer = null;
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
  await resetBusinessDb({ name: BUSINESS_DB_NAME });
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
  setStartupProgress(58, 'Systemtabellen werden vorbereitet...');

  const ctoxSchemes = withMigrationStrategies(ctoxCollections, ctoxMigrationStrategies);
  const desktopSchemes = withMigrationStrategies(desktopCollections, desktopMigrationStrategies);

  const consolidated = {
    ...ctoxSchemes,
    ...desktopSchemes,
  };

  setStartupProgress(59, 'Registriere Systemdaten-Struktur in IndexedDB...');
  await state.db.addCollections(consolidated);

  setStartupProgress(61, 'Systemtabellen erfolgreich initialisiert.');
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
    if (!state.contextMenu || !state.modules?.length) {
      location.hash = '#desktop';
      return;
    }
    event.preventDefault();
    const moduleItems = listLaunchTargets('module').map((target) => startMenuItemForTarget(target));
    const appItems = listLaunchTargets('app').map((target) => startMenuItemForTarget(target));
    const items = [...moduleItems];
    if (appItems.length) items.push({ type: 'separator' }, ...appItems);
    items.push({ type: 'separator' });
    items.push({
      label: shellText('settings') || 'Einstellungen',
      icon: '⚙',
      action: () => openSettingsDrawer(),
    });
    state.contextMenu.show(event, items);
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

function openSettingsDrawer(options = {}) {
  els.rightDrawer.classList.remove('account-popover');
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
  showBackdrop();
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
  leftHandle.setAttribute('role', 'separator');
  leftHandle.setAttribute('aria-orientation', 'vertical');
  leftHandle.setAttribute('aria-label', 'Linke und mittlere Spalte anpassen');

  const rightHandle = document.createElement('div');
  rightHandle.className = 'workspace-col-resizer workspace-col-resizer-right';
  rightHandle.setAttribute('role', 'separator');
  rightHandle.setAttribute('aria-orientation', 'vertical');
  rightHandle.setAttribute('aria-label', 'Mittlere und rechte Spalte anpassen');

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
  }

  function placeHandles(metrics, widths) {
    if (!metrics || !widths) return;
    leftHandle.style.left = `${Math.round(widths.left + (metrics.gap / 2))}px`;
    rightHandle.style.left = `${Math.round(widths.left + metrics.gap + widths.center + (metrics.gap / 2))}px`;
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

  leftHandle.addEventListener('pointerdown', (event) => startDrag('left', event));
  rightHandle.addEventListener('pointerdown', (event) => startDrag('right', event));
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
  window.ctoxBusinessOsSyncDiagnostics = snapshot;
  scheduleSyncRecoveryRepairIfNeeded(snapshot);
  refreshOpenSyncDiagnosticsDrawer();
  window.dispatchEvent(new CustomEvent('ctox-business-os-sync-diagnostics', {
    detail: snapshot,
  }));
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
  if (snapshot.phase === 'reconnecting') return true;
  return collections.some((collection) => collection?.connectionStatus === 'reconnecting');
}

async function repairRecoveringDataPlane() {
  if (syncRecoveryRepairRunning || !state.syncConfig || !state.db) return;
  syncRecoveryRepairRunning = true;
  try {
    console.warn('[business-os] repairing RxDB/WebRTC data plane after stalled reconnect');
    setStatus('RxDB/WebRTC wird neu verbunden');
    await repairBusinessDataPlane(state.syncConfig);
    await startCriticalSyncCollections();
    if (state.activeModule) startModuleSync(state.activeModule);
    window.setTimeout(() => startAllModuleSync(), 5000);
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
    } catch (error) {
      console.warn(`[business-os] critical sync collection ${collection} did not start during repair`, error);
    }
  }
}

async function buildAdvancedStatusSnapshot(options = {}) {
  const diagnostics = state.syncDiagnostics || null;
  const collections = diagnostics?.collections || {};
  const collectionValues = Object.values(collections);
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
    moduleCatalogAvailable: state.modules.length > 0 && (counts === null || Number(counts.business_module_catalog || 0) > 0),
    requiredCollectionsConnected: missingRequiredCollections.length === 0,
    requiredCollectionsInitialSyncComplete: initialSync.missingInitialReplication.length === 0,
    noCheckpointProtocolErrors: checkpointErrors.length === 0,
    noSchemaProtocolErrors: schemaErrors.length === 0,
    noReplicationIoErrors: replicationErrors.length === 0,
    noFailedCollections: failedCollections.length === 0,
    noStalledReconnect: reconnectingCollections.length === 0,
    noAutomaticRepairRunning: !syncRecoveryRepairRunning,
  };
  const ok = Object.values(checks).every(Boolean);
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
    },
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
      lifecycleEvents,
      requiredCollections,
      requiredCollectionEvidence,
      missingRequiredCollections,
      initialSync,
      lastError: diagnostics?.lastError || null,
      lastLifecycleEvent: diagnostics?.lastLifecycleEvent || null,
    },
    fileIntegrity: {
      errorTotal: fileIntegrityErrors.length,
      errors: fileIntegrityErrors,
      lastError: fileIntegrityErrors[0] || null,
    },
    data: { counts },
  };
}

function serializeAdvancedStatusCollectionError(item) {
  const error = item?.lastError;
  if (!error) return null;
  const rawCode = typeof error.code === 'string' ? error.code.trim() : '';
  const rawName = typeof error.name === 'string' ? error.name.trim() : '';
  const rawMessage = typeof error.message === 'string' ? error.message.trim() : '';
  const rawPhase = typeof error.phase === 'string' ? error.phase.trim() : '';
  const rawSeverity = typeof error.severity === 'string' ? error.severity.trim() : '';
  return {
    collection: item.collection || null,
    status: item.connectionStatus || item.status || null,
    name: rawName || 'Error',
    code: rawCode || null,
    phase: rawPhase || null,
    severity: rawSeverity || null,
    retryable: typeof error.retryable === 'boolean' ? error.retryable : null,
    expected: typeof error.expected === 'string' ? error.expected.slice(0, 120) : null,
    actual: typeof error.actual === 'string' ? error.actual.slice(0, 120) : null,
    direction: typeof error.direction === 'string' ? error.direction.slice(0, 20) : null,
    upstreamCode: typeof error.upstreamCode === 'string' ? error.upstreamCode.slice(0, 40) : null,
    batchSize: Number.isFinite(Number(error.batchSize)) ? Number(error.batchSize) : null,
    rowCount: Number.isFinite(Number(error.rowCount)) ? Number(error.rowCount) : null,
    message: rawMessage.slice(0, 240),
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
  research: '🔬',
  conversations: '💬',
  notes: '📝',
  'app-store': '🛍',
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
    loader: () => import('./desktop-apps/file-viewer/app.js?v=20260522-file-chunk-integrity4'),
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
  return DESKTOP_APPS.map(({ id, title, glyph, defaultWidth, defaultHeight }) => ({
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
  const svgHtml = getSvgIcon(target.id, 16, 1.8);
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
  const svgHtml = getSvgIcon(mod.id, 16, 1.8);
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
  return mod?.id && mod.id !== 'desktop';
}

function listLaunchTargets(kind = '') {
  const moduleTargets = state.modules
    .filter(moduleAppearsInSwitcher)
    .map((mod) => ({
      id: mod.id,
      kind: 'module',
      title: moduleDisplayTitle(mod),
      glyph: taskbarMarkForModule(mod),
      module: mod,
    }));
  const appTargets = DESKTOP_APPS.map((app) => ({
    id: app.id,
    kind: 'app',
    title: app.title,
    glyph: app.glyph,
    app,
  }));
  const all = [...moduleTargets, ...appTargets];
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
    const moduleScript = await import(`./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}${moduleRevisionQuery(mod.id)}`);
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
    const schemaModule = await import(`./${moduleBasePath(mod)}/schema.js?v=${APP_BUILD}${moduleRevisionQuery(mod.id)}${retryQuery}`);
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
  const repairToken = `${BUSINESS_DB_NAME}:${RXDB_BOOTSTRAP_VERSION}`;
  try {
    if (sessionStorage.getItem(RXDB_SCHEMA_REPAIR_KEY) === repairToken) return false;
    sessionStorage.setItem(RXDB_SCHEMA_REPAIR_KEY, repairToken);
  } catch {}
  console.warn('[business-os] local RxDB schema drift detected; rebuilding browser cache', error);
  setStatus('Lokale RxDB wird neu aufgebaut');
  try { await state.sync?.stop?.(); } catch (stopError) { console.warn('[business-os] sync stop before schema repair failed', stopError); }
  try { await state.db?.close?.(); } catch (closeError) { console.warn('[business-os] db close before schema repair failed', closeError); }
  try { await resetBusinessDb({ name: BUSINESS_DB_NAME }); } catch (resetError) { console.warn('[business-os] RxDB schema repair reset failed', resetError); }
  window.setTimeout(() => window.location.reload(), 250);
  return true;
}

function isRxDbSchemaDriftError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: DB6')
    || message.includes('previousSchemaHash')
    || message.includes('schemaHash');
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
    startAllModuleSync();
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
  const svgHtml = getSvgIcon(mod.id, 16, 1.8);
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
  document.title = `${moduleDisplayTitle(mod)} · CTOX Business OS`;
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
    research: 'R',
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

  const savedUser = readAccountPrefs().loginUser || 'admin';
  const loginUrl = session.login_url || '';
  const pairingMissing = session.reason === 'pairing_config_missing'
    || session.reason === 'session_launch_context_missing';

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
              placeholder="admin"
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

  // Autofocus handling: if username is prefilled, focus password, otherwise username
  setTimeout(() => {
    if (userInput.value && userInput.value !== 'admin') {
      passwordInput.focus();
    } else {
      userInput.focus();
    }
  }, 50);
}

function renderAccountButton(session = state.session) {
  if (!els.accountButton) return;
  const labelNode = els.accountButton.querySelector('[data-account-label]');
  const user = session?.user || {};
  if (session?.authenticated) {
    const prefs = readAccountPrefs();
    const label = prefs.displayName || user.display_name || user.id || 'Account';
    const role = roleDisplayName(user.role || (user.is_admin ? 'admin' : 'user'));
    if (labelNode) labelNode.textContent = role;
    els.accountButton.setAttribute('aria-label', `Account: ${label}, Rolle: ${role}`);
    els.accountButton.title = `Account: ${label} · Rolle: ${role}`;
    els.accountButton.dataset.authenticated = 'true';
  } else {
    if (labelNode) labelNode.textContent = 'Login';
    els.accountButton.setAttribute('aria-label', 'Login öffnen');
    els.accountButton.title = 'Login öffnen';
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
  const savedUser = readAccountPrefs().loginUser || 'admin';
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
        <input name="user" autocomplete="username" value="${escapeHtml(savedUser)}" placeholder="admin" />
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

function registerCustomModuleIcons() {
  if (!Array.isArray(state.modules)) return;
  for (const mod of state.modules) {
    if (mod.layout?.icon_svg) {
      registerSvgIcon(mod.id, mod.layout.icon_svg);
    }
  }
}

async function refreshModules() {
  const modules = await loadModules();
  state.modules = modules.modules || [];
  registerCustomModuleIcons();
  state.governance = modules.governance || state.governance;
  state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
  persistModuleLayout();
  renderTabs();
  state.backgroundModuleWorkScheduled = false;
  scheduleBackgroundModuleWork();
  refreshRemoteShellStateInBackground();
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
  if (side === 'right' && !target.classList.contains('account-popover')) {
    target.classList.remove('account-popover');
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
    const status = isPendingCtoxHealthError(error)
      ? { ok: true, pending: true, error: error?.message || String(error) }
      : { ok: false, error: error?.message || String(error) };
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
  if (status?.pending) return '';
  if (!status || status.ok === false) {
    return [shellText('ctoxStatusUnavailable'), status?.error].filter(Boolean).join(' ');
  }
  const service = status.ctox_service;
  if (!service) return shellText('ctoxStatusUnavailable');
  if (service.running === false) return shellText('ctoxStopped');
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

  const pairedConfig = readBusinessOsLaunchConfig();
  if (pairedConfig) {
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

async function loadModules() {
  const catalog = await loadModuleCatalog();
  return {
    ok: catalog.ok !== false,
    modules: Array.isArray(catalog.modules) ? catalog.modules : [],
    governance: catalog.governance || null,
  };
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

async function loadModuleCatalog(timeoutMs = 60000) {
  const coll = state.db?.collection?.('business_module_catalog');
  if (!coll) throw new Error('business_module_catalog collection is required for shell module metadata');
  await state.sync?.startCollection?.('business_module_catalog');
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const doc = await coll.findOne('module-catalog').exec();
      const data = doc?.toJSON?.();
      if (data && data._deleted !== true && data.is_deleted !== true) return data;
    } catch (error) {
      lastError = error;
    }
    await delay(300);
  }
  throw lastError || new Error('Modulkatalog wurde noch nicht synchronisiert.');
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
  await state.sync?.startCollection?.('business_commands');
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
  const launch = firstObject(
    readUrlPairingConfig(),
    root.CTOX_BUSINESS_OS_CONFIG,
    root.ctoxBusinessOsLaunch?.config,
    root.ctoxBusinessOsLaunch,
    window.CTOX_BUSINESS_OS_CONFIG,
    window.ctoxBusinessOsLaunch?.config,
    window.ctoxBusinessOsLaunch,
    readStoredPairingConfig(),
  );
  const config = await normalizeBusinessOsLaunchConfig(launch);
  if (config && config.source === 'url') {
    writeStoredPairingConfig(config);
    scrubPairingConfigFromUrl();
  }
  return config;
}

function readUrlPairingConfig() {
  const params = new URLSearchParams(location.search);
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
    ];
    let changed = false;
    for (const key of sensitiveKeys) {
      if (!url.searchParams.has(key)) continue;
      url.searchParams.delete(key);
      changed = true;
    }
    if (!changed) return;
    const next = `${url.pathname}${url.search}${url.hash}`;
    history.replaceState(history.state, document.title, next);
  } catch {}
}

function parsePackedConfig(value) {
  try {
    const normalized = value.replace(/-/g, '+').replace(/_/g, '/');
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, '=');
    const bytes = Uint8Array.from(atob(padded), (char) => char.charCodeAt(0));
    return JSON.parse(new TextDecoder().decode(bytes));
  } catch {
    try {
      return JSON.parse(value);
    } catch {
      return null;
    }
  }
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
  return {
    ok: config.ok !== false,
    app_hosting: config.app_hosting || config.appHosting || 'web_deploy',
    sync_mode: config.sync_mode || config.syncMode || 'p2p-first',
    instance_id: instanceId || syncRoom.replace(/^ctox-business-os:/, '').split(':')[0],
    peer_id: config.peer_id || config.peerId || '',
    peer_role: config.peer_role || config.peerRole || 'browser',
    sync_room: syncRoom,
    signaling_room_password: roomPassword,
    signaling_urls: urls,
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
    loadModules()
      .then((modules) => {
        if (!Array.isArray(modules?.modules) || !modules.modules.length) return;
        const currentIds = state.modules.map((mod) => mod.id).join('\n');
        const nextIds = modules.modules.map((mod) => mod.id).join('\n');
        if (currentIds === nextIds) return;
        state.modules = modules.modules;
        registerCustomModuleIcons();
        state.governance = modules.governance || state.governance;
        state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
        persistModuleLayout();
        renderTabs();
      })
      .catch(() => {});
  }, 2000);
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

  if (msg.includes('pairing') || msg.includes('sync config is missing') || msg.includes('Pair this browser')) {
    title = 'Keine Kopplung vorhanden';
    description = 'Dieser Browser ist noch nicht mit einer aktiven CTOX-Instanz verbunden.';
    advice = 'Bitte öffnen Sie Business OS über den bereitgestellten Link aus Ihrer CTOX-Schnittstelle oder koppeln Sie die Instanz erneut.';
  } else if (msg.includes('IndexedDB lock') || msg.includes('timed out after 25000ms') || msg.includes('database creation timed out')) {
    title = 'Datenbank-Zugriff blockiert';
    description = 'Die lokale Datenbankverbindung konnte nicht rechtzeitig hergestellt werden (IndexedDB Timeout).';
    advice = 'Möglicherweise ist die Anwendung in einem anderen Tab geöffnet. Schließen Sie bitte alle anderen Tabs von Business OS und versuchen Sie es erneut.';
  } else if (msg.includes('Schema-Drift') || msg.includes('DB6') || msg.includes('previousSchemaHash') || msg.includes('schemaHash') || msg.includes('drift')) {
    title = 'Datenbank-Drift erkannt';
    description = 'Die Tabellenstruktur der lokalen Datenbank ist inkompatibel mit dieser Version.';
    advice = 'Wir versuchen, den Cache automatisch neu aufzubauen. Klicken Sie auf "Erneut versuchen", um den Vorgang abzuschließen.';
  } else if (msg.includes('modulkatalog') || msg.includes('business_module_catalog') || msg.includes('module catalog')) {
    title = 'Module konnten nicht geladen werden';
    description = 'Die Synchronisation des Modulkatalogs über das WebRTC-Netzwerk ist fehlgeschlagen oder wartet auf Verbindung.';
    advice = 'Bitte überprüfen Sie, ob die CTOX-Instanz im Terminal aktiv ist. Eine stabile WebRTC-Verbindung ist für den Start zwingend erforderlich.';
  } else if (msg.includes('Cannot access') && msg.includes('before initialization')) {
    title = 'Fehler in Skript-Reihenfolge';
    description = 'Eine Systemvariable wurde vor ihrer Initialisierung aufgerufen (Temporal Dead Zone).';
    advice = 'Dieses Ladeproblem wurde behoben. Bitte leeren Sie den Browser-Cache und klicken Sie auf "Erneut versuchen".';
  } else if (msg.includes('NetworkError') || msg.includes('Failed to fetch') || msg.includes('signaling')) {
    title = 'Verbindung zum Netzwerk fehlgeschlagen';
    description = 'Der Signalisierungs-Server oder die Peer-Verbindungen konnten nicht erreicht werden.';
    advice = 'Bitte überprüfen Sie Ihre Internetverbindung und stellen Sie sicher, dass keine Firewall oder restriktive Antiviren-Software WebRTC-Verbindungen blockiert.';
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
