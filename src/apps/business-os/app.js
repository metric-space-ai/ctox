import { CtoxResizer } from './shared/resizer.js';
import { createAppActions } from './shared/app-actions.js?v=20260711-runtime-v1';
import {
  appLifecycleBadge,
  appLifecycleState,
  appReleaseProjection,
  canSeeModuleForAppVersion as lifecycleCanSeeModuleForAppVersion,
  isRuntimeInstalledModule,
} from './shared/app-lifecycle.js?v=20260623-role-session';
import {
  BusinessOsPermissions,
  canModifyBusinessModule,
  canSelfExecuteBusinessData,
  canUseBusinessPermission,
  canViewBusinessModuleSource,
} from './shared/permissions.js?v=20260701-branding-v1';
import {
  applyWorkspaceBranding,
  brandingForPreferencePayload,
  WORKSPACE_BRANDING_COLLECTION,
  WORKSPACE_BRANDING_DOCUMENT_ID,
} from './shared/branding.js?v=20260701-branding-v1';
import { normalizeRole, roleCanManage, roleDescription, roleDisplayName } from './shared/roles.js';
import {
  launchesInWindow,
  resolvePresentation,
  usesLegacyWorkspace,
} from './shared/presentation.js?v=20260711-presentation-v3';
import {
  buildLifecyclePermissionView,
  buildGlobalCtoxAgentScopeView,
  buildModuleWhyDiagnosticsView,
  buildModuleTargetContextItems,
  renderBusinessUserDatalistOptions,
  renderGlobalCtoxAgentScopeHtml,
  renderModuleWhyDiagnosticsHtml,
  renderGlobalCtoxContextModeHtml,
  shouldRenderModuleSourceAction,
} from './shared/shell-permissions-ui.js?v=20260701-runtime-module-rev1';

const SESSION_TOKEN_KEY = 'ctox.businessOs.sessionToken';
const AUTH_HEADER_KEY = 'ctox.businessOs.authHeader';
const LOGGED_OUT_KEY = 'ctox.businessOs.loggedOut';
const ACCOUNT_PREFS_KEY = 'ctox.businessOs.accountPreferences';
const PAIRING_CONFIG_KEY = 'ctox.businessOs.pairingConfig';
const RXDB_BOOTSTRAP_VERSION_KEY = 'ctox.businessOs.rxdbBootstrapVersion';
const RXDB_SCHEMA_REPAIR_KEY = 'ctox.businessOs.rxdbSchemaRepair';
const MODULE_LAYOUT_KEY = 'ctox.businessOs.moduleLayout';
const TASKBAR_PINS_KEY = 'ctox.businessOs.taskbarPins';
const WINDOW_GEOMETRY_KEY = 'ctox.businessOs.windowGeometry';
const SHELL_COLUMN_LAYOUT_KEY_PREFIX = 'ctox.businessOs.shellColumnLayout.';
const SHELL_MODULE_RESIZER_KEY_PREFIX = 'ctox.businessOs.moduleColumns.';
const APP_BUILD = '20260713-peer-protocol-status-v1';

ensureShellStylesheets();

// Monotonic token so a slow loading-shadow fetch from a previous module open
// cannot paint over a newer one (rapid module switching).
let activeLoadToken = 0;
const MAX_TRANSIENT_MODULE_SYNC_RETRIES = 3;
// After the fast-retry budget, a module's sync falls back to this slow periodic
// retry instead of being permanently disabled, so a longer transient failure
// still recovers on its own without a full app reload.
const SLOW_MODULE_SYNC_RETRY_MS = 60000;
const BUSINESS_DB_NAME = 'ctox_business_os_v11';
const RXDB_BOOTSTRAP_VERSION = `${BUSINESS_DB_NAME}:storage-v1`;
const CTOX_HEALTH_POLL_MS = 10000;
const CTOX_UPDATE_CHECK_POLL_MS = 30 * 60 * 1000;
const SYNC_RECOVERY_REPAIR_DELAY_MS = 15000;
const SHELL_IMPORT_TIMEOUT_MS = 45000;
const MODULE_SCRIPT_PRELOAD_STABLE_HEALTH_MS = 10000;
const MODULE_SCRIPT_PRELOAD_INTERVAL_MS = 250;
const DEFAULT_TASKBAR_PIN_IDS = ['ctox', 'tickets', 'documents', 'spreadsheets', 'explorer', 'knowledge', 'app-store', 'research', 'calendar'];
// Shell-critical collections this app eagerly warms at boot. This MUST stay a
// subset of SHELL_CRITICAL_COLLECTIONS, the single source of truth exported by
// the ctox-rxdb-js bundle (rxdb/src/webrtc-native.mjs). The browser_* shell
// criticals are intentionally omitted here because they only register when the
// Browser module is active; warming them eagerly is not this app's job.
// desktop_file_chunks is intentionally not shell-critical: it can contain very
// large file bodies and must be pulled lazily by file/document views instead of
// blocking login, module navigation, or command dispatch at shell startup.
// assertCriticalSyncCollectionsMatchBundle() runs once the bundle loads and
// throws if this list ever drifts out of that source-of-truth set.
const CRITICAL_SYNC_COLLECTIONS = [
  'business_module_catalog',
  'ctox_runtime_settings',
  'business_commands',
  'ctox_queue_tasks',
];

let criticalSyncCollectionsBundleChecked = false;

function ensureShellStylesheets() {
  if (typeof document === 'undefined') return;
  for (const href of [
    `app.css?v=${APP_BUILD}`,
    'shared/base.css?v=20260609-base1',
  ]) {
    const absoluteHref = new URL(href, import.meta.url).href;
    const alreadyLoaded = Array.from(document.querySelectorAll('link[rel="stylesheet"]'))
      .some((link) => {
        try {
          return new URL(link.getAttribute('href') || link.href, document.baseURI).pathname
            === new URL(absoluteHref).pathname;
        } catch {
          return false;
        }
      });
    if (alreadyLoaded) continue;
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = absoluteHref;
    link.dataset.shellRequiredStylesheet = 'true';
    document.head.appendChild(link);
  }
}

function assertCriticalSyncCollectionsMatchBundle(rxdb) {
  if (criticalSyncCollectionsBundleChecked) return;
  const canonical = rxdb?.SHELL_CRITICAL_COLLECTIONS;
  if (!canonical || typeof canonical.has !== 'function') return;
  criticalSyncCollectionsBundleChecked = true;
  const drifted = CRITICAL_SYNC_COLLECTIONS.filter((collection) => !canonical.has(collection));
  if (drifted.length) {
    throw new Error(
      `[business-os] CRITICAL_SYNC_COLLECTIONS drifted from the ctox-rxdb-js `
      + `SHELL_CRITICAL_COLLECTIONS source of truth: ${drifted.join(', ')}`,
    );
  }
}
let moduleLayoutSaveTimer = null;
let taskbarPinSaveTimer = null;
let shellColumnResizeSync = null;
let syncToastRefresh = null;
let syncToastWatchdog = 0;
let moduleResizers = [];
let syncRecoveryRepairTimer = null;
let syncRecoveryRepairRunning = false;
let moduleScriptPreloadPending = false;
let moduleScriptPreloadHealthySinceMs = 0;
let moduleScriptPreloadResumeTimer = null;
let moduleScriptPreloadIdleHandle = null;
let moduleScriptPreloadGeneration = 0;
let moduleScriptPreloadPauseReason = '';
const moduleScriptPreloadTimers = new Set();
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
let launchConfigForPageSession = null;

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
  packagedModuleAssetRevisions: new Map(),
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
  // Phase 2: sync orchestration (critical-warmup ordering, module-sync
  // deferral, background scheduling) was removed from app.js. Replication now
  // starts lazily inside the RxDB layer and the foreground collection is
  // prioritized from real reactive subscriptions (see active-collections.mjs).
  // `deferredSyncModules` / `criticalSyncWarmupPromise` /
  // `backgroundModuleWorkScheduled` are intentionally gone.
  ctoxHealth: null,
  ctoxUpdateCheck: null,
  ctoxUpdateCheckRunning: false,
  ctoxUpdateCheckedAtMs: 0,
  ctoxUpdateInstallRunning: false,
  ctoxUpdateInstallStatus: '',
  fileIntegrityDiagnostics: [],
  ctoxHealthTimer: null,
  eventBus: null,
  contextMenu: null,
  notifications: null,
  windowManager: null,
  taskbar: null,
  windowSwitcher: null,
  windowGeometryCache: new Map(),
  windowGeometryWriteChains: new Map(),
  catalogSubscription: null,
  catalogRefreshTimer: null,
  catalogRefreshRunning: false,
  catalogRefreshQueued: false,
  moduleCatalogFingerprint: '',
  workspaceBranding: null,
  workspaceBrandingSubscription: null,
  moduleAllowlist: [],
  shellCatalogMergedIds: new Set(),
  moduleIconSvgCache: new Map(),
  initialModuleOpened: false,
  advancedStatusRequiredRestarts: new Map(),
};

function resetDataPlaneReady(reason = 'startup') {
  if (state.dataPlaneReadyStatus === 'pending' && state.dataPlaneReady) {
    state.dataPlaneReadyReason = reason;
    return state.dataPlaneReady;
  }
  state.dataPlaneReadyStatus = 'pending';
  state.dataPlaneReadyReason = reason;
  state.dataPlaneReady = new Promise((resolve, reject) => {
    state.dataPlaneReadyResolve = resolve;
    state.dataPlaneReadyReject = reject;
  });
  return state.dataPlaneReady;
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
        await ensureAdvancedStatusRequiredCollections(options.requiredCollections, {
          allowRestart: options.allowRestart === true,
        });
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
  state.openModule = (moduleId, options = {}) => openModule(moduleId, options);
}

async function ensureAdvancedStatusRequiredCollections(requiredCollections, options = {}) {
  if (!Array.isArray(requiredCollections) || !state.sync?.startCollection) return;
  const names = requiredCollections
    .filter((collection) => typeof collection === 'string' && collection.trim())
    .filter((collection) => state.db?.raw?.[collection]);
  await Promise.all(names.map((collection) => state.sync.startCollection(collection).catch(() => null)));
  if (options.allowRestart !== true) return;
  for (const collection of names) {
    if (!shouldRestartAdvancedStatusRequiredCollection(collection)) continue;
    const lastRestartAt = Number(state.advancedStatusRequiredRestarts?.get(collection) || 0);
    if (Date.now() - lastRestartAt < 15000) continue;
    state.advancedStatusRequiredRestarts.set(collection, Date.now());
    state.sync.restartCollection?.(collection).catch((error) => {
      console.warn(`[business-os] advanced status required collection restart failed for ${collection}`, error);
    });
  }
}

function shouldRestartAdvancedStatusRequiredCollection(collection) {
  const diagnostics = state.syncDiagnostics?.collections?.[collection] || null;
  if (!diagnostics) return false;
  if (diagnostics.initialReplicationState === 'complete' && diagnostics.remoteCheckpoint?.epoch) return false;
  const status = diagnostics.connectionStatus || diagnostics.status || '';
  const transport = diagnostics.frameTransport || {};
  const activePeerCount = Number(transport.activePeerCount || 0);
  const startedAt = Date.parse(
    diagnostics.initialReplicationStartedAt
      || diagnostics.reconnectingSince
      || diagnostics.updatedAt
      || ''
  );
  const ageMs = Number.isFinite(startedAt) ? Date.now() - startedAt : 0;
  if (ageMs < 12000) return false;
  if (diagnostics.lastLifecycleEvent?.code === 'peer_connect_timeout') return true;
  return ['connecting', 'running', 'reconnecting'].includes(status) && activePeerCount < 1;
}

function isAdvancedStatusWebRtcMode(mode) {
  return typeof mode === 'string' && mode.split('+').includes('webrtc');
}

installAdvancedStatusInterface();

if (new URLSearchParams(window.location.search).has('rxdbSmoke')) {
  const smokeRoot = typeof globalThis === 'undefined' ? window : globalThis;
  const smokeApi = {
    state,
    openDesktopApp,
    reportFileIntegrityError,
    createLiveDbFacade,
    createModuleContext,
    createModulePermissionFacade,
    storageKeys: businessOsStorageKeys,
    renderTabs,
    listLaunchTargets,
    openAppLifecycleDrawer,
    openSettingsDrawer,
  };
  smokeRoot.ctoxBusinessOsSmoke = smokeApi;
  window.ctoxBusinessOsSmoke = smokeApi;
}

const moduleAliases = {
  notizen: 'notes',
};
const LEGACY_MODULE_ALIASES = new Map(Object.entries(moduleAliases));

function storageScopeSegment(value, fallback = 'default') {
  const normalized = String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_.-]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 96);
  return normalized || fallback;
}

function currentWorkspaceStorageScope() {
  const root = typeof globalThis === 'undefined' ? window : globalThis;
  const urlConfig = (() => {
    try { return readUrlPairingConfig(); } catch { return null; }
  })();
  const candidates = [
    state.syncConfig?.instance_id,
    state.syncConfig?.instanceId,
    state.sync?.config?.instance_id,
    state.sync?.config?.instanceId,
    launchConfigForPageSession?.instance_id,
    launchConfigForPageSession?.instanceId,
    urlConfig?.instance_id,
    urlConfig?.instanceId,
    root.CTOX_BUSINESS_OS_CONFIG?.instance_id,
    root.CTOX_BUSINESS_OS_CONFIG?.instanceId,
    root.ctoxBusinessOsLaunch?.config?.instance_id,
    root.ctoxBusinessOsLaunch?.config?.instanceId,
    root.ctoxBusinessOsLaunch?.instance_id,
    root.ctoxBusinessOsLaunch?.instanceId,
    location.host || location.origin,
  ];
  return storageScopeSegment(candidates.find((value) => String(value || '').trim()), 'local');
}

function currentActorStorageScope() {
  const user = state.session?.user || {};
  return storageScopeSegment(user.id || user.email || user.login || (state.session?.authenticated ? 'authenticated' : 'browser'), 'browser');
}

function scopedStorageKey(baseKey, options = {}) {
  const workspace = options.workspace === false ? '' : currentWorkspaceStorageScope();
  const actor = options.actor === false ? '' : currentActorStorageScope();
  const moduleId = options.moduleId ? storageScopeSegment(options.moduleId, '') : '';
  return [
    String(baseKey || '').trim(),
    'scope',
    workspace,
    actor,
    moduleId,
  ].filter(Boolean).join('.');
}

function readScopedLocalStorage(baseKey, options = {}) {
  const key = scopedStorageKey(baseKey, options);
  try {
    const scoped = localStorage.getItem(key);
    if (scoped !== null) return scoped;
    if (options.legacyFallback) return localStorage.getItem(baseKey);
  } catch {}
  return null;
}

function writeScopedLocalStorage(baseKey, value, options = {}) {
  const key = scopedStorageKey(baseKey, options);
  try { localStorage.setItem(key, value); } catch {}
  return key;
}

function removeScopedLocalStorage(baseKey, options = {}) {
  try { localStorage.removeItem(scopedStorageKey(baseKey, options)); } catch {}
}

function businessOsStorageKeys() {
  return {
    workspace: currentWorkspaceStorageScope(),
    actor: currentActorStorageScope(),
    taskbarPins: scopedStorageKey(TASKBAR_PINS_KEY),
    moduleLayout: scopedStorageKey(MODULE_LAYOUT_KEY),
    accountPreferences: scopedStorageKey(ACCOUNT_PREFS_KEY),
    pairingConfig: scopedStorageKey(PAIRING_CONFIG_KEY, { actor: false }),
  };
}

function createStorageScopeFacade(mod) {
  const moduleId = mod?.id || '';
  const optionsFor = (options = {}) => ({ moduleId, ...options });
  return Object.freeze({
    version: 'business-os-storage-scope-v1',
    workspace: currentWorkspaceStorageScope(),
    actor: currentActorStorageScope(),
    module_id: moduleId,
    key: (baseKey, options = {}) => scopedStorageKey(baseKey, optionsFor(options)),
    get: (baseKey, options = {}) => readScopedLocalStorage(baseKey, optionsFor(options)),
    set: (baseKey, value, options = {}) => writeScopedLocalStorage(baseKey, value, optionsFor(options)),
    remove: (baseKey, options = {}) => removeScopedLocalStorage(baseKey, optionsFor(options)),
  });
}

async function loadShellUiModules() {
  if (!shellUiModulesPromise) {
    shellUiModulesPromise = Promise.all([
      importBusinessOsModule(`./shared/event-bus.js?v=${APP_BUILD}`, 'shell event bus'),
      importBusinessOsModule(`./shared/notifications.js?v=${APP_BUILD}`, 'shell notifications'),
      importBusinessOsModule(`./shared/context-menu.js?v=${APP_BUILD}`, 'shell context menu'),
      importBusinessOsModule(`./shared/window-manager.js?v=${APP_BUILD}`, 'shell window manager'),
      importBusinessOsModule(`./shared/taskbar.js?v=${APP_BUILD}`, 'shell taskbar'),
      importBusinessOsModule(`./shared/window-switcher.js?v=${APP_BUILD}`, 'shell window switcher'),
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
    shellDialogsModulePromise = importBusinessOsModule(`./shared/dialogs.js?v=${APP_BUILD}`, 'shell dialogs');
  }
  return shellDialogsModulePromise;
}

async function loadShellIconsModule() {
  if (!shellIconsModulePromise) {
    shellIconsModulePromise = importBusinessOsModule(`./shared/icons.js?v=${APP_BUILD}`, 'shell icons')
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

function getRegisteredActionIcon(name, size, strokeWidth) {
  return shellIconsModule?.getActionIcon?.(name, size, strokeWidth) || '';
}

async function loadBusinessDbModule() {
  if (!businessDbModulePromise) {
    businessDbModulePromise = importBusinessOsModule(`./shared/db.js?v=${APP_BUILD}`, 'business db')
      .then((mod) => {
        businessDbModule = mod;
        return mod;
      });
  }
  return businessDbModulePromise;
}

async function loadSyncModule() {
  if (!syncModulePromise) {
    syncModulePromise = importBusinessOsModule(`./shared/sync.js?v=${APP_BUILD}`, 'business sync')
      .then((mod) => {
        syncModule = mod;
        return mod;
      });
  }
  return syncModulePromise;
}

async function loadCommandBusModule() {
  if (!commandBusModulePromise) {
    commandBusModulePromise = importBusinessOsModule(`./shared/command-bus.js?v=${APP_BUILD}`, 'command bus');
  }
  return commandBusModulePromise;
}

async function loadCoreSchemaModules() {
  if (!coreSchemaModulesPromise) {
    coreSchemaModulesPromise = Promise.all([
      importBusinessOsModule(`./modules/ctox/schema.js?v=${APP_BUILD}`, 'ctox core schema'),
      importBusinessOsModule(`./modules/desktop/schema.js?v=${APP_BUILD}`, 'desktop core schema'),
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
    syncingContent: 'Inhalte werden synchronisiert',
    syncComplete: 'Inhalte synchronisiert',
    syncDismiss: 'Ausblenden',
    loginRequired: 'Login erforderlich',
    startupChecking: 'CTOX-Sitzung prüfen',
    syncConnecting: 'Sync-Verbindungen starten',
    collection: 'Collection',
    activity: 'Aktivität',
    agentContext: 'Agent-Kontext',
    webrtcSync: 'WebRTC-Sync',
    ctoxNotWorking: 'CTOX Verbindung prüfen',
    ctoxStopped: 'CTOX Service ist gerade nicht verfügbar.',
    ctoxStatusUnavailable: 'CTOX Status ist gerade nicht verfügbar.',
    ctoxLastError: 'Letzter Fehler',
    ctoxUpdateAvailable: 'Update verfügbar',
    ctoxUpdateInstall: 'Update installieren',
    ctoxUpdateChecking: 'Update wird geprüft',
    ctoxUpdateInstalling: 'Update läuft',
    ctoxUpdateConfirm: 'CTOX Update jetzt installieren? Der lokale Dienst wird währenddessen neu gestartet.',
    ctoxUpdateStarted: 'CTOX Update wurde gestartet.',
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
    chatWorkDataLabel: 'Daten ändern',
    chatAnswerLabel: 'Frage stellen',
    chatModifyAppLabel: 'App ändern',
    chatPlaceholder: 'Was soll CTOX hier tun oder prüfen?',
    chatOpening: 'Öffne Chat...',
    send: 'Senden',
    startMenuOpen: 'Startmenü öffnen',
    ctoxCoreOpen: 'CTOX AI Core öffnen',
    navBack: 'Zurück',
    navForward: 'Vorwärts',
    openApps: 'Geöffnete Apps',
    shellStyleAria: 'Stil',
    languageAria: 'Sprache',
    themeAria: 'Design Theme',
    appearanceSettings: 'Darstellung und Sprache',
    shellStyleLabel: 'Fenster',
    languageLabel: 'Sprache',
    themeLabel: 'Schema',
    loginOpen: 'Login öffnen',
    appMenuOpen: 'App-Menü öffnen',
    notificationsAria: 'Benachrichtigungen',
    openWindowsAria: 'Offene Fenster',
    startupStarting: 'System wird gestartet...',
    startupFailedTitle: 'System-Start fehlgeschlagen',
    startupFailedBody: 'Bei der Verbindung zum lokalen Daten-Netzwerk ist ein schwerwiegender Fehler aufgetreten. Der lokale RxDB-Catalog-Sync meldet:',
    startupRetry: 'Erneut versuchen',
    gateSubtitle: 'Melden Sie sich an, um eine Verbindung zur ctox-Instanz herzustellen.',
    gateUser: 'Benutzer',
    gateUserPlaceholder: 'E-Mail oder Benutzername',
    gatePassword: 'Passwort',
    gateSubmit: 'Einloggen & Verbinden',
    gateSso: 'Mit SSO einloggen',
    gateFooter: 'Sichere Ende-zu-Ende verschlüsselte Verbindung.',
    gateInvalidCredentials: 'Ungültiger Benutzername oder Passwort.',
    drawerLoginHint: 'Bei Desktop-Start wird die CTOX-Instanz automatisch übernommen.',
    drawerLoginSubmit: 'Einloggen',
    drawerLoginExternal: 'Extern einloggen',
    bootConfig: 'System-Konfiguration wird geladen...',
    bootSession: 'Anmeldesitzung wird überprüft...',
    bootDatastore: 'Lokaler Datenspeicher wird geladen...',
    bootWorkspace: 'Workspace wird vorbereitet...',
    bootApps: 'Ihre Anwendungen werden vorbereitet...',
    bootCatalog: 'Modulkatalog wird synchronisiert...',
    bootOptimize: 'Lokaler Datenspeicher wird optimiert...',
    bootReady: 'Workspace ist bereit. CTOX wird gestartet...',
    bootDbConfig: 'Datenspeicher-Konfiguration wird vorbereitet...',
    bootDbOpen: 'Lokaler Speicher wird geöffnet...',
    bootDbStructures: 'Systemdatenstrukturen werden aufgebaut...',
    bootDesktopLayout: 'Desktop-Layout wird geladen...',
    bootSyncStart: 'Echtzeit-Synchronisierung wird gestartet...',
    bootServices: 'Dienste werden gestartet...',
    bootSchemas: 'Datenstrukturen werden vorbereitet...',
    bootSchemasRegister: 'Speicherstrukturen werden registriert...',
    bootSchemasDone: 'Speicherstrukturen erfolgreich geladen.',
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
    syncingContent: 'Syncing content',
    syncComplete: 'Content synced',
    syncDismiss: 'Dismiss',
    loginRequired: 'Login required',
    startupChecking: 'Checking CTOX session',
    syncConnecting: 'Connecting sync peers',
    collection: 'Collection',
    activity: 'Activity',
    agentContext: 'Agent context',
    webrtcSync: 'WebRTC sync',
    ctoxNotWorking: 'Check CTOX connection',
    ctoxStopped: 'CTOX service is unavailable right now.',
    ctoxStatusUnavailable: 'CTOX status is unavailable right now.',
    ctoxLastError: 'Last error',
    ctoxUpdateAvailable: 'Update available',
    ctoxUpdateInstall: 'Install update',
    ctoxUpdateChecking: 'Checking update',
    ctoxUpdateInstalling: 'Update running',
    ctoxUpdateConfirm: 'Install the CTOX update now? The local service will restart during the update.',
    ctoxUpdateStarted: 'CTOX update started.',
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
    chatWorkDataLabel: 'Change data',
    chatAnswerLabel: 'Ask question',
    chatModifyAppLabel: 'Change app',
    chatPlaceholder: 'What should CTOX do or check here?',
    chatOpening: 'Opening Chat...',
    send: 'Send',
    startMenuOpen: 'Open start menu',
    ctoxCoreOpen: 'Open CTOX AI Core',
    navBack: 'Back',
    navForward: 'Forward',
    openApps: 'Open apps',
    shellStyleAria: 'Style',
    languageAria: 'Language',
    themeAria: 'Theme',
    appearanceSettings: 'Appearance and language',
    shellStyleLabel: 'Window',
    languageLabel: 'Language',
    themeLabel: 'Scheme',
    loginOpen: 'Open login',
    appMenuOpen: 'Open app menu',
    notificationsAria: 'Notifications',
    openWindowsAria: 'Open windows',
    startupStarting: 'Starting system...',
    startupFailedTitle: 'System startup failed',
    startupFailedBody: 'A fatal error occurred while connecting to the local data network. The local RxDB catalog sync reports:',
    startupRetry: 'Retry',
    gateSubtitle: 'Sign in to connect to the ctox instance.',
    gateUser: 'User',
    gateUserPlaceholder: 'Email or username',
    gatePassword: 'Password',
    gateSubmit: 'Sign in & connect',
    gateSso: 'Sign in with SSO',
    gateFooter: 'Secure end-to-end encrypted connection.',
    gateInvalidCredentials: 'Invalid username or password.',
    drawerLoginHint: 'On desktop start the CTOX instance is taken over automatically.',
    drawerLoginSubmit: 'Sign in',
    drawerLoginExternal: 'External sign-in',
    bootConfig: 'Loading system configuration...',
    bootSession: 'Checking sign-in session...',
    bootDatastore: 'Loading local datastore...',
    bootWorkspace: 'Preparing workspace...',
    bootApps: 'Preparing your applications...',
    bootCatalog: 'Syncing module catalog...',
    bootOptimize: 'Optimizing local datastore...',
    bootReady: 'Workspace ready. Starting CTOX...',
    bootDbConfig: 'Preparing datastore configuration...',
    bootDbOpen: 'Opening local storage...',
    bootDbStructures: 'Building system data structures...',
    bootDesktopLayout: 'Loading desktop layout...',
    bootSyncStart: 'Starting realtime sync...',
    bootServices: 'Starting services...',
    bootSchemas: 'Preparing data structures...',
    bootSchemasRegister: 'Registering storage structures...',
    bootSchemasDone: 'Storage structures loaded.',
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
  ctoxVersion: document.querySelector('[data-ctox-version]'),
  tabs: document.querySelector('[data-module-tabs]'),
  host: document.querySelector('[data-module-host]'),
  leftContent: document.querySelector('[data-left-content]'),
  rightContent: document.querySelector('[data-right-content]'),
  backdrop: document.querySelector('[data-backdrop]'),
  leftDrawer: document.querySelector('[data-drawer-left]'),
  rightDrawer: document.querySelector('[data-drawer-right]'),
  bottomDrawer: document.querySelector('[data-drawer-bottom]'),
  accountButton: document.querySelector('[data-open-account]'),
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

  // Resolve the session before showing any "loading" UI. An unauthenticated
  // request must never see the workspace startup loader — that falsely implies
  // the system is loading data when nothing past the auth gate runs.
  const session = await loadSession();
  state.session = session;
  renderAccountButton(session);
  if (!session.authenticated) {
    document.documentElement.dataset.authState = 'locked';
    state.dataPlaneReadyStatus = 'idle';
    state.dataPlaneReadyReason = 'login-required';
    const loginFailed = session.reason === 'invalid_credentials'
      || new URLSearchParams(location.search).has('loginFailed');
    clearStoredBrowserAuth();
    renderLoginGate(session, { loginFailed });
    setStatus(shellText('loginRequired'));
    return;
  }

  setStartupProgress(10, shellText('bootConfig'));
  setStartupProgress(30, shellText('bootSession'));
  setStartupProgress(50, shellText('bootDatastore'));
  const syncConfig = await loadSyncConfig();
  await resetBusinessDataPlaneForBuildIfNeeded(syncConfig);
  await openBusinessDataPlane(syncConfig);

  setStartupProgress(70, shellText('bootWorkspace'));
  let modules;
  try {
    setStartupProgress(85, shellText('bootApps'));
    modules = await loadModules();
  } catch (error) {
    if (!isModuleCatalogSyncError(error)) throw error;
    console.warn('[business-os] module catalog sync stalled; extending WebRTC wait before local cache repair', error);
    setStartupProgress(82, shellText('bootCatalog'));
    try {
      modules = await loadModules({ timeoutMs: 180000, allowShellSeed: false });
    } catch (retryError) {
      if (!isModuleCatalogSyncError(retryError)) throw retryError;
      console.warn('[business-os] module catalog still unavailable; resetting local RxDB cache and retrying WebRTC sync', retryError);
      setStartupProgress(80, shellText('bootOptimize'));
      await repairBusinessDataPlane(syncConfig);
      modules = await loadModules({ timeoutMs: 180000, allowShellSeed: false });
    }
  }
  modules = await waitForRequestedHashModule(modules);
  state.modules = modules.modules || [];
  state.moduleCatalogFingerprint = modules.catalogFingerprint || state.moduleCatalogFingerprint;
  try {
    await registerCustomModuleIcons();
  } catch (error) {
    console.warn('[business-os] custom module icon registration failed:', error);
  }
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
    getSvgIcon: getRegisteredSvgIcon,
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
      getSvgIcon: getRegisteredSvgIcon,
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
  setWorkspaceStatus();

  // Initialize the global ctox context menu
  initGlobalCtoxContextMenu();

  setStartupProgress(95, shellText('bootReady'));
  try {
    await openModule(currentHashModuleId() || state.modules[0]?.id || 'ctox');
    markBootTiming('shellVisibleMs');
    setWorkspaceStatus();
    scheduleBusinessCompanions();
  } catch (error) {
    console.error('[business-os] module startup failed', error);
    setStatus(`Module startup failed: ${error.message || error}`);
  } finally {
    state.initialModuleOpened = Boolean(state.activeModule?.id);
    flushDeferredCatalogRefresh();
  }
  // Phase 2: no critical-sync warmup choreography here anymore — replication
  // starts lazily inside RxDB when a collection is first subscribed/read.
  // Module-script preloading is a render concern (not sync orchestration) and
  // stays, scheduled off the idle path.
  scheduleModuleScriptPreload();
  refreshRemoteShellStateInBackground();
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
  localStorage.setItem(RXDB_BOOTSTRAP_VERSION_KEY, versionToken);
  if (existingToken) {
    console.info('[business-os] RxDB bootstrap token updated without deleting local cache', {
      previous: existingToken,
      current: versionToken,
    });
  }
}

async function openBusinessDataPlane(syncConfig) {
  resetDataPlaneReady('open-business-data-plane');
  setStartupProgress(51, shellText('bootDbConfig'));
  try {
    state.syncConfig = syncConfig;
    const dbName = businessDbName(syncConfig);

    await openBusinessDbAndRegisterCoreCollections(dbName);

    setStartupProgress(62, shellText('bootDesktopLayout'));
    await hydrateTaskbarPinsFromDesktopLayout();
    renderTabs();

    setStartupProgress(66, shellText('bootSyncStart'));
    // Resolve the native-signed actor capability before the first multiplexed
    // WebRTC peer handshake. Protected demand collections authorize the peer
    // from that initial protocol payload; fetching the token only after a core
    // collection has opened leaves the whole room at least privilege until a
    // reconnect even though command submission itself is already authorized.
    const commandBusModule = await loadCommandBusModule();
    await commandBusModule.getBusinessOsCapabilityToken?.();
    const { createSyncRuntime } = await loadSyncModule();
    state.sync = createSyncRuntime({
      db: state.db,
      config: syncConfig,
      onDiagnostic: updateSyncDiagnostics,
    });

    setStartupProgress(69, shellText('bootServices'));
    state.commandBus = commandBusModule.createCommandBus({
      db: () => state.db,
      sync: () => state.sync,
      session: () => state.session,
      config: syncConfig,
    });
    startShellCtoxHealthMonitor();
    startWorkspaceBrandingMonitor();

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

async function openBusinessDbAndRegisterCoreCollections(dbName) {
  const { createBusinessDb } = await loadBusinessDbModule();
  const maxAttempts = 3;
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    setStartupProgress(54, shellText('bootDbOpen'));
    state.db = await createBusinessDb({ name: dbName });
    assertCriticalSyncCollectionsMatchBundle(state.db?.rxdb);

    try {
      setStartupProgress(58, shellText('bootDbStructures'));
      await registerCoreCollections({ timeoutMs: 12000 });
      return;
    } catch (error) {
      const retryable = (isIndexedDbConnectionClosingError(error) || isCoreCollectionRegistrationTimeout(error))
        && attempt < maxAttempts;
      try { await state.db?.close?.(); } catch (closeError) {
        console.debug('[business-os] stale IndexedDB close failed during startup retry', closeError);
      }
      state.db = null;
      if (!retryable) throw error;
      console.warn(`[business-os] Core schema registration did not complete; reopening IndexedDB (${attempt}/${maxAttempts - 1})`, error);
      await new Promise((resolve) => window.setTimeout(resolve, attempt * 150));
    }
  }
}

function isIndexedDbConnectionClosingError(error) {
  const message = String(error?.message || error || '');
  return error?.name === 'InvalidStateError'
    && /IDBDatabase.*closing|database connection is closing/i.test(message);
}

function isCoreCollectionRegistrationTimeout(error) {
  return error?.name === 'CtoxCoreCollectionRegistrationTimeout';
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
    if (isVolatileSyncTransportError(error) || isClosedRxDbCollectionError(error)) {
      console.debug('[business-os] Module catalog refresh skipped during data-plane shutdown:', error?.message || error);
      return;
    }
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
  if (state.workspaceBrandingSubscription) {
    try { state.workspaceBrandingSubscription.unsubscribe(); } catch (e) {}
    state.workspaceBrandingSubscription = null;
  }
  state.workspaceBranding = applyWorkspaceBranding(null);
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

async function registerCoreCollections({ timeoutMs = 12000 } = {}) {
  const t0 = performance.now();
  setStartupProgress(58, shellText('bootSchemas'));

  const { ctox, desktop } = await loadCoreSchemaModules();
  const ctoxSchemes = withMigrationStrategies(ctox.collections, ctox.migrationStrategies);
  const desktopSchemes = withMigrationStrategies(desktop.collections, desktop.migrationStrategies);

  const consolidated = {
    ...ctoxSchemes,
    ...desktopSchemes,
  };

  setStartupProgress(59, shellText('bootSchemasRegister'));
  await withCoreCollectionRegistrationTimeout(
    state.db.addCollections(consolidated),
    timeoutMs
  );

  setStartupProgress(61, shellText('bootSchemasDone'));
  const t1 = performance.now();
  console.log(`[business-os] registerCoreCollections took ${(t1 - t0).toFixed(2)}ms`);
  await primeWindowGeometryCache();
}

function withCoreCollectionRegistrationTimeout(promise, timeoutMs) {
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) return promise;
  let timer = null;
  const timeout = new Promise((_, reject) => {
    timer = window.setTimeout(() => {
      const error = new Error(`Core collection registration did not finish within ${timeoutMs}ms.`);
      error.name = 'CtoxCoreCollectionRegistrationTimeout';
      reject(error);
    }, timeoutMs);
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timer) window.clearTimeout(timer);
  });
}

async function primeWindowGeometryCache() {
  const coll = state.db?.collections?.desktop_windows;
  state.windowGeometryCache.clear();
  for (const [ownerId, payload] of readWindowGeometryLocalCache()) {
    state.windowGeometryCache.set(ownerId, payload);
  }
  if (!coll) return;
  try {
    const docs = await coll.find().exec();
    for (const doc of docs) {
      const payload = doc.toJSON();
      if (!payload?.owner_id) continue;
      if (windowGeometryDocumentMatchesCurrentScope(payload)) {
        mergeWindowGeometryCache(payload.owner_id, payload);
      } else if (isLegacyWindowGeometryDocument(payload) && !state.windowGeometryCache.has(payload.owner_id)) {
        mergeWindowGeometryCache(payload.owner_id, payload);
      }
    }
    persistWindowGeometryLocalCache();
  } catch (error) {
    console.error('[business-os] primeWindowGeometryCache failed:', error);
  }
}

function readWindowGeometryLocalCache() {
  const entries = new Map();
  try {
    const parsed = JSON.parse(readScopedLocalStorage(WINDOW_GEOMETRY_KEY) || 'null');
    const rawEntries = parsed?.entries && typeof parsed.entries === 'object' ? parsed.entries : {};
    for (const [ownerId, payload] of Object.entries(rawEntries)) {
      if (!ownerId || !payload || typeof payload !== 'object') continue;
      entries.set(ownerId, { ...payload, id: payload.id || ownerId, owner_id: payload.owner_id || ownerId });
    }
  } catch {}
  return entries;
}

function persistWindowGeometryLocalCache() {
  const entries = {};
  for (const [ownerId, payload] of state.windowGeometryCache) {
    if (!ownerId || !payload) continue;
    entries[ownerId] = payload;
  }
  writeScopedLocalStorage(WINDOW_GEOMETRY_KEY, JSON.stringify({
    version: 1,
    entries,
  }));
}

function mergeWindowGeometryCache(ownerId, payload) {
  if (!ownerId || !payload) return;
  const current = state.windowGeometryCache.get(ownerId);
  const currentUpdatedAt = Number(current?.updated_at_ms || 0);
  const nextUpdatedAt = Number(payload.updated_at_ms || 0);
  if (current && currentUpdatedAt > nextUpdatedAt) return;
  state.windowGeometryCache.set(ownerId, payload);
}

function currentWindowGeometryScope() {
  return {
    workspace_scope: currentWorkspaceStorageScope(),
    actor_scope: currentActorStorageScope(),
  };
}

function windowGeometryRecordId(ownerId) {
  const scope = currentWindowGeometryScope();
  const owner = storageScopeSegment(ownerId, 'window').slice(0, 80);
  const workspace = scope.workspace_scope.slice(0, 48);
  const actor = scope.actor_scope.slice(0, 48);
  return `shellwin_${workspace}_${actor}_${owner}_${stableShortHash(`${workspace}|${actor}|${ownerId}`)}`;
}

function windowGeometryDocumentMatchesCurrentScope(payload) {
  const scope = currentWindowGeometryScope();
  return payload?.workspace_scope === scope.workspace_scope
    && payload?.actor_scope === scope.actor_scope;
}

function isLegacyWindowGeometryDocument(payload) {
  return payload
    && !payload.workspace_scope
    && !payload.actor_scope
    && payload.id === payload.owner_id;
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
      const scope = currentWindowGeometryScope();
      const next = {
        id: windowGeometryRecordId(ownerId),
        owner_id: ownerId,
        workspace_scope: scope.workspace_scope,
        actor_scope: scope.actor_scope,
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
      persistWindowGeometryLocalCache();
      queueGeometryPersist(ownerId, next);
    },
  };
}

function queueGeometryPersist(ownerId, payload) {
  const previous = state.windowGeometryWriteChains.get(ownerId) || Promise.resolve();
  const write = previous
    .catch(() => {})
    .then(() => flushGeometryPersist(ownerId, state.windowGeometryCache.get(ownerId) || payload));
  state.windowGeometryWriteChains.set(ownerId, write);
  write
    .catch((error) => {
      console.error('[business-os] geometry persist failed:', error);
    })
    .finally(() => {
      if (state.windowGeometryWriteChains.get(ownerId) === write) {
        state.windowGeometryWriteChains.delete(ownerId);
      }
    });
}

async function flushGeometryPersist(ownerId, payload) {
  const coll = state.db?.collections?.desktop_windows;
  if (!coll) return;
  if (!payload) return;
  const recordId = payload.id || windowGeometryRecordId(ownerId);
  const existing = await coll.findOne(recordId).exec();
  if (existing) {
    await existing.incrementalPatch(payload);
  } else {
    try {
      await coll.insert(payload);
    } catch (error) {
      if (!isRxConflictError(error)) throw error;
      const conflicted = await coll.findOne(recordId).exec();
      if (!conflicted) throw error;
      await conflicted.incrementalPatch(payload);
    }
  }
}

function isRxConflictError(error) {
  const status = error?.status || error?.parameters?.writeError?.status;
  if (status === 409) return true;
  const code = String(error?.code || error?.rxdb || '').toUpperCase();
  if (code === 'CONFLICT') return true;
  const message = String(error?.message || error || '').toLowerCase();
  return message.includes('conflict') || message.includes('already');
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

function stableShortHash(value) {
  let hash = 2166136261;
  const text = String(value || '');
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
}

function wireShellActions() {
  window.addEventListener('unhandledrejection', (event) => {
    if (!isVolatileSyncTransportError(event.reason)) return;
    console.debug('[business-os] ignored volatile local sync transport error');
    event.preventDefault();
  });
  window.addEventListener('error', (event) => {
    if (!isVolatileSyncTransportError(event.error || event.message)) return;
    console.debug('[business-os] ignored volatile local sync transport error');
    event.preventDefault();
  });
  window.addEventListener('message', (event) => {
    if (event.origin !== window.location.origin) return;
    if (!isTrustedBusinessOsMessageSource(event)) return;
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
      return;
    }
    const lifecycleButton = event.target.closest('[data-module-lifecycle]');
    if (lifecycleButton) {
      event.preventDefault();
      const moduleId = lifecycleButton.dataset.moduleLifecycle || state.activeModule?.id;
      const mod = state.modules.find((item) => item.id === moduleId) || state.activeModule;
      if (mod) openAppLifecycleDrawer(mod);
    }
  });
  els.host?.addEventListener('change', async (event) => {
    const select = event.target.closest('[data-module-version-select]');
    if (select) {
      const moduleId = select.dataset.moduleVersionSelect;
      const selected = select.value;
      if (!selected) return;
      const isBundleVersion = selected.startsWith('modver:');

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

        if (isBundleVersion) {
          await dispatchShellModuleCommand({
            commandType: 'ctox.module.rollback_version',
            moduleId,
            recordId: `${moduleId}:versions`,
            payload: { module_id: moduleId, version_id: selected.slice('modver:'.length) },
            source: 'business-os-shell',
          });
        } else {
          await dispatchShellModuleCommand({
            commandType: 'ctox.source.rollback_snapshot',
            moduleId,
            recordId: `${moduleId}:snapshots`,
            payload: { module_id: moduleId, snapshot_id: selected },
            source: 'business-os-shell',
          });
        }

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
  els.ctoxVersion?.querySelector('[data-ctox-update-button]')?.addEventListener('click', installCtoxUpdateFromShell);
  els.accountButton?.addEventListener('click', openAccountDrawer);
  document.addEventListener('change', (event) => {
    const control = event.target;
    if (!control?.matches) return;
    if (control.matches('[data-language-select]')) {
      applyShellLanguage(control.value);
      syncHeaderControls();
      renderShellCtoxWarning(state.ctoxHealth);
      renderShellCtoxVersion(state.ctoxHealth);
      postCurrentPreferencesToModule();
    } else if (control.matches('[data-theme-select]')) {
      applyShellTheme(control.value);
      syncHeaderControls();
      postCurrentPreferencesToModule();
    } else if (control.matches('[data-shell-style-select]')) {
      applyShellStyle(control.value);
      syncHeaderControls();
      postCurrentPreferencesToModule();
    }
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
  setupSyncToast();
}

function isTrustedBusinessOsMessageSource(event) {
  if (!event) return false;
  if (event.source === window) return true;
  for (const frame of els.host?.querySelectorAll?.('iframe') || []) {
    if (frame.contentWindow === event.source) return true;
  }
  return false;
}

// The old floating sync toast is intentionally disabled. Sync state is surfaced
// inline by product surfaces such as the Desktop status widget, while the shell
// still emits `ctox-business-os-sync-diagnostics` for those views.
function setupSyncToast() {
  document.querySelector('[data-sync-toast]')?.remove();
  syncToastRefresh = () => {};
}

function teardownModuleResizers() {
  for (const resizer of moduleResizers) {
    try { resizer.destroy?.(); } catch {}
  }
  moduleResizers = [];
}

// Shell-owned column resizing for module-provided frames. Any module that ships
// resizer handles declaratively — a `.ctox-column-resizer` with `data-resizer-var`
// (the CSS custom property to drive) plus `data-resizer` (left|right) and optional
// `data-resizer-min`/`data-resizer-max`, inside a `[data-resize-frame]` root — gets
// drag/keyboard resizing AND per-module width persistence for free. Modules no
// longer hand-wire CtoxResizer or their own localStorage layout code.
function setupModuleResizers(mod, options = {}) {
  const managedList = Array.isArray(options.resizers) ? options.resizers : moduleResizers;
  if (managedList === moduleResizers) teardownModuleResizers();
  const scope = options.scope || els.host?.querySelector('[data-module-content]');
  if (!scope || !mod?.id) return () => {};
  for (const handle of scope.querySelectorAll('.ctox-column-resizer[data-resizer-var]')) {
    const cssVar = handle.getAttribute('data-resizer-var');
    const container = handle.closest('[data-resize-frame]');
    if (!cssVar || !container) continue;
    const side = handle.getAttribute('data-resizer') === 'right' ? 'right' : 'left';
    const minWidth = Number.parseFloat(handle.getAttribute('data-resizer-min')) || 200;
    const maxWidth = Number.parseFloat(handle.getAttribute('data-resizer-max')) || 600;
    const storageKey = scopedStorageKey(`${SHELL_MODULE_RESIZER_KEY_PREFIX}${mod.id}:${cssVar}`, {
      moduleId: mod.id,
    });

    // Restore persisted width synchronously (before paint) to avoid a flash.
    let savedWidth = NaN;
    try { savedWidth = Number.parseFloat(localStorage.getItem(storageKey) || ''); } catch {}
    if (Number.isFinite(savedWidth) && savedWidth > 0) {
      container.style.setProperty(cssVar, `${Math.round(clampNumber(savedWidth, minWidth, maxWidth))}px`);
    }

    const resizer = new CtoxResizer({
      resizerEl: handle,
      containerEl: container,
      cssVar,
      side,
      minWidth,
      maxWidth,
      onResize: (width) => {
        try { localStorage.setItem(storageKey, String(Math.round(width))); } catch {}
      },
    });
    managedList.push(resizer);
  }
  return () => {
    for (const resizer of managedList.splice(0)) {
      try { resizer.destroy?.(); } catch {}
    }
  };
}

function openModuleSourceEditor(moduleId) {
  const mod = state.modules.find((entry) => entry.id === moduleId) || state.activeModule;
  if (!mod?.id) return;
  if (!canViewModuleSource(mod)) {
    setStatus(shellLang() === 'de'
      ? 'Source ist fuer diese App nicht freigegeben.'
      : 'Source is not available for this app.', true);
    return;
  }
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

function shellPreferenceControlsTemplate() {
  const shellStyle = document.documentElement.dataset.shellStyle === 'macos' ? 'macos' : 'windows';
  const language = shellLang();
  const theme = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
  return `
    <div class="settings-preferences" aria-label="${escapeHtml(shellText('appearanceSettings') || 'Appearance and language')}" data-shell-t-aria="appearanceSettings">
      <label class="settings-preference-control">
        <span data-shell-t="shellStyleLabel">${escapeHtml(shellText('shellStyleLabel') || 'Window')}</span>
        <select class="header-select" data-shell-style-select aria-label="${escapeHtml(shellText('shellStyleAria') || 'Style')}" data-shell-t-aria="shellStyleAria">
          ${preferenceOption('windows', 'Windows', shellStyle)}
          ${preferenceOption('macos', 'macOS', shellStyle)}
        </select>
      </label>
      <label class="settings-preference-control">
        <span data-shell-t="languageLabel">${escapeHtml(shellText('languageLabel') || 'Language')}</span>
        <select class="header-select" data-language-select aria-label="${escapeHtml(shellText('languageAria') || 'Language')}" data-shell-t-aria="languageAria">
          ${preferenceOption('de', 'DE', language)}
          ${preferenceOption('en', 'EN', language)}
        </select>
      </label>
      <label class="settings-preference-control">
        <span data-shell-t="themeLabel">${escapeHtml(shellText('themeLabel') || 'Scheme')}</span>
        <select class="header-select" data-theme-select aria-label="${escapeHtml(shellText('themeAria') || 'Theme')}" data-shell-t-aria="themeAria">
          ${preferenceOption('dark', 'Dark', theme)}
          ${preferenceOption('light', 'Light', theme)}
        </select>
      </label>
    </div>
  `;
}

function preferenceOption(value, label, selected) {
  return `<option value="${escapeHtml(value)}" ${selected === value ? 'selected' : ''}>${escapeHtml(label)}</option>`;
}


async function openSettingsDrawer(options = {}) {
  els.rightDrawer.classList.remove('account-popover');
  els.rightDrawer.classList.add('settings-drawer-open');
  els.rightDrawer.hidden = false;
  showBackdrop();
  if (state.dataPlaneReadyStatus !== 'ready') {
    els.rightDrawer.replaceChildren();
    const loading = document.createElement('div');
    loading.className = 'drawer-body settings-drawer';
    loading.innerHTML = `
      <header class="drawer-header-row settings-head">
        ${shellPreferenceControlsTemplate()}
        <button class="icon-button" type="button" data-close-settings aria-label="Schließen">×</button>
      </header>
      <section class="settings-section">
        <header><h3>Datenspeicher</h3><span>Wird vorbereitet.</span></header>
      </section>
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
      <header class="drawer-header-row settings-head">
        ${shellPreferenceControlsTemplate()}
        <button class="icon-button" type="button" data-close-settings aria-label="Schließen">×</button>
      </header>
      <section class="settings-section">
        <header><h3>Datenspeicher</h3><span>Nicht bereit.</span></header>
        <p class="muted">${escapeHtml(String(error?.message || error || 'Unbekannter Fehler'))}</p>
      </section>
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
    db: createScopedSystemDbFacade('settings-drawer-react-settings', SETTINGS_DB_COLLECTIONS),
    initialTab: options.initialTab || 'runtime',
    onAccount: openAccountDrawer,
    onClose: closeDrawers,
    onModulesChanged: refreshModules,
  });
}

function isVolatileSyncTransportError(error) {
  const text = String(error?.message || error || '');
  return /cannot send after peer is destroyed|ERR_DATA_CHANNEL|Failure to send data|User-Initiated Abort|QUERY_CANCELLED|replication-cancel|WebRTC replication cancelled|IDBDatabase.*closing|database connection is closing/i.test(text);
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
  let syncRetries = 0;

  function layoutKey() {
    return state.activeModule?.id
      ? scopedStorageKey(`${SHELL_COLUMN_LAYOUT_KEY_PREFIX}${state.activeModule.id}`, {
          moduleId: state.activeModule.id,
        })
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
    if (!metrics || metrics.trackTotal <= 0) {
      // Frame not measured yet (called mid-transition right after mount). Retry
      // on the next frame so handles still appear without waiting for a resize.
      if (syncRetries < 5) {
        syncRetries += 1;
        requestAnimationFrame(syncLayout);
      }
      return;
    }
    syncRetries = 0;

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
  updateModuleScriptPreloadAvailability(snapshot);
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
    // Phase 2: no critical-warmup choreography on reconnect — re-arm the active
    // module's collections directly; RxDB drives replication + priority lazily.
    if (state.activeModule) startModuleSync(state.activeModule);
  } finally {
    syncRecoveryRepairRunning = false;
  }
}

// Phase 2: the critical-sync warmup choreography
// (`runCriticalSyncWarmup` / `startCriticalSyncCollections` /
// `scheduleCriticalSyncWarmup` / `waitForCriticalSyncCollection` /
// `isCriticalSyncCollectionReady` / `areCriticalSyncCollectionsReady`) was
// REMOVED. Apps no longer choreograph which collections sync first or wait for
// "critical" collections to be ready before opening a module. Replication
// starts lazily inside the RxDB layer the first time a collection is
// subscribed/read, and the foreground collection is prioritized from real
// reactive subscriptions (active-collections.mjs → `rxdb.activeCollections`).
//
// TODO(phase2-cleanup): once every app reads/writes purely through RxDB
// reactive queries, the thin `state.sync.startModule(mod)` call in
// `startModuleSync` can also move into RxDB's lazy first-subscription path so
// app.js stops touching sync entirely. `CRITICAL_SYNC_COLLECTIONS` is retained
// ONLY for the schema-hash drift guard near the top of this file, not for
// ordering.

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
  const requiredCollectionsStreamingReady = initialSync.missingInitialReplication.length === 0
    || initialSync.missingStreamingReady.length === 0;
  const checks = {
    authenticated: Boolean(state.session?.authenticated),
    shellLoaded: state.modules.length > 0,
    activeModuleLoaded: Boolean(state.activeModule?.id),
    workspaceNotLoading: !bodyDataset.moduleLoading,
    dataPlaneWebrtc: isAdvancedStatusWebRtcMode(state.sync?.mode) && isAdvancedStatusWebRtcMode(diagnostics?.mode),
    rxdbRuntimeAppLocal: state.db?.runtime?.name === 'ctox-rxdb-js'
      && state.db?.runtime?.source === 'app-local'
      && state.db?.runtime?.packageManager === 'none',
    moduleCatalogAvailable: state.modules.length > 0 && (counts === null || Number(counts.business_module_catalog || 0) > 0),
    requiredCollectionsConnected: missingRequiredCollections.length === 0,
    requiredCollectionsInitialSyncComplete: initialSync.missingInitialReplication.length === 0
      || requiredCollectionsStreamingReady,
    requiredCollectionsStreamingReady,
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
      iceServersHaveTurn: diagnostics?.iceServersHaveTurn === true,
      iceServersHaveCredentialedTurn: diagnostics?.iceServersHaveCredentialedTurn === true,
      protocol: diagnostics?.protocol || null,
      capabilities: Array.isArray(diagnostics?.capabilities) ? diagnostics.capabilities : [],
      commandPlane: diagnostics?.commandPlane || null,
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
    const httpBridgeReady = isHttpBridgeReady(diagnostics);
    const initialReplicationAt = diagnostics?.initialReplicationAt || (httpBridgeReady ? diagnostics?.httpBridgePulledAt : null) || null;
    const startedAt = diagnostics?.initialReplicationStartedAt || null;
    const startedMs = startedAt ? Date.parse(startedAt) : NaN;
    const state = initialReplicationAt
      ? 'complete'
      : (diagnostics?.initialReplicationState || (diagnostics ? 'pending' : 'missing-diagnostics'));
    const remoteCapabilities = Array.isArray(diagnostics?.remoteCapabilities)
      ? diagnostics.remoteCapabilities
      : [];
    const checkpoint = sanitizeAdvancedStatusRemoteCheckpoint(diagnostics?.remoteCheckpoint || null);
    const checkpointEpochAdvertised = httpBridgeReady || hasAdvertisedCheckpointEpoch(diagnostics);
    const streamingReady = isRequiredCollectionStreamingReady(diagnostics, checkpointEpochAdvertised);
    const stalledForMs = !initialReplicationAt && Number.isFinite(startedMs)
      ? Math.max(0, now - startedMs)
      : 0;
    return {
      collection,
      state,
      status: diagnostics?.status || null,
      connectionStatus: diagnostics?.connectionStatus || null,
      source: httpBridgeReady ? 'http-bridge' : (diagnostics?.initialReplicationSource || null),
      initialReplicationStartedAt: startedAt,
      initialReplicationAt,
      checkpointState: checkpoint?.state || null,
      checkpointEpoch: checkpoint?.epoch || null,
      checkpointEpochAdvertised,
      streamingReady,
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
    missingStreamingReady: entries
      .filter((entry) => !entry.streamingReady)
      .map((entry) => entry.collection),
    streamingReadyCollections: entries
      .filter((entry) => entry.streamingReady)
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
  if (isHttpBridgeReady(diagnostics)) return true;
  const initialReplicationComplete = Boolean(diagnostics.initialReplicationAt || diagnostics.initialReplicationState === 'complete');
  if (!hasAdvertisedCheckpointEpoch(diagnostics)) return false;
  if (['failed', 'error', 'stopped', 'pending'].includes(status)) return false;
  if (initialReplicationComplete) return true;
  if (isRequiredCollectionStreamingReady(diagnostics, true)) return true;
  if (['connected', 'running', 'reused'].includes(status)) return true;
  if (evidence?.hasData === true) return true;
  if (![
    'business_commands',
    'ctox_queue_tasks',
  ].includes(collection)) return false;
  return true;
}

function isRequiredCollectionStreamingReady(diagnostics, checkpointEpochAdvertised = hasAdvertisedCheckpointEpoch(diagnostics)) {
  if (!diagnostics) return false;
  if (isHttpBridgeReady(diagnostics)) return true;
  if (!checkpointEpochAdvertised) return false;
  const status = diagnostics.connectionStatus || diagnostics.status || '';
  if (['failed', 'error', 'stopped', 'pending'].includes(status)) return false;
  const transport = diagnostics.frameTransport || {};
  const activePeerCount = Number(transport.activePeerCount || 0);
  if (activePeerCount < 1) return false;
  if (diagnostics.connectedAt || diagnostics.initialReplicationAt) return true;
  if (diagnostics.initialReplicationState === 'complete') return true;
  if (['connected', 'running', 'reused'].includes(status)) return true;
  return Number(transport.sentFrames || 0) > 0
    || Number(transport.receivedFrames || 0) > 0
    || transport.pullInProgress === true
    || transport.pushInProgress === true;
}

function hasAdvertisedCheckpointEpoch(diagnostics) {
  if (!diagnostics) return false;
  if (isHttpBridgeReady(diagnostics)) return true;
  const capabilities = Array.isArray(diagnostics.remoteCapabilities) ? diagnostics.remoteCapabilities : [];
  if (!capabilities.includes('ctox-checkpoint-epoch-v1')) return false;
  const checkpoint = sanitizeAdvancedStatusRemoteCheckpoint(diagnostics.remoteCheckpoint || null);
  return Boolean(checkpoint?.state === 'advertised' && checkpoint.epoch);
}

function isHttpBridgeReady(diagnostics) {
  return Boolean(diagnostics?.httpBridgeStatus === 'ready' && diagnostics?.httpBridgePulledAt);
}

async function collectAdvancedStatusCounts() {
  const names = [
    'business_module_catalog',
    'ctox_runtime_settings',
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
    loader: () => import(`./desktop-apps/explorer/app.js?v=${APP_BUILD}`),
  },
  {
    id: 'browser',
    title: 'Browser',
    glyph: '🌐',
    defaultWidth: 1120,
    defaultHeight: 760,
    loader: () => import(`./desktop-apps/browser/app.js?v=${APP_BUILD}`),
  },
  {
    id: 'code-editor',
    title: 'Source Editor',
    glyph: '⌘',
    defaultWidth: 980,
    defaultHeight: 640,
    loader: () => import(`./desktop-apps/code-editor/app.js?v=${APP_BUILD}`),
  },
  {
    id: 'file-viewer',
    title: 'File Viewer',
    glyph: '◫',
    defaultWidth: 760,
    defaultHeight: 560,
    loader: () => import(`./desktop-apps/file-viewer/app.js?v=${APP_BUILD}`),
  },
  {
    id: 'creator',
    title: 'App Creator',
    glyph: '⚙️',
    defaultWidth: 1200,
    defaultHeight: 800,
    loader: () => import(`./desktop-apps/creator/app.js?v=${APP_BUILD}`),
  },
];

// Companion viewers remain available internally under a module allowlist, but
// launchable desktop apps like Files must be explicitly allowlisted per tenant.
const DESKTOP_APP_ALWAYS_ALLOWED = new Set();

function listDesktopApps() {
  const nonWindowedModuleIds = new Set((state.modules || [])
    .filter((mod) => mod?.id && !moduleLaunchesAsDesktopApp(mod))
    .map((mod) => mod.id));
  const allow = resolveModuleAllowlist(state.moduleAllowlist);
  const allowActive = allow.size > 0;
  const targetsById = new Map();
  for (const mod of state.modules || []) {
    if (!moduleAppearsAsWindowTarget(mod)) continue;
    targetsById.set(mod.id, desktopAppDescriptorForModule(mod));
  }
  for (const app of DESKTOP_APPS) {
    if (app.id === 'file-viewer' || targetsById.has(app.id)) continue;
    if (nonWindowedModuleIds.has(app.id)) continue;
    // Under an active allowlist, only surface allowlisted apps plus explicitly
    // always-available file tools.
    if (allowActive && !DESKTOP_APP_ALWAYS_ALLOWED.has(app.id) && !allow.has(app.id)) continue;
    targetsById.set(app.id, {
      id: app.id,
      title: app.title,
      glyph: app.glyph,
      defaultWidth: app.defaultWidth,
      defaultHeight: app.defaultHeight,
      minWidth: app.minWidth,
      minHeight: app.minHeight,
    });
  }
  return Array.from(targetsById.values());
}

function moduleLaunchesAsDesktopApp(mod) {
  return launchesInWindow(mod);
}

function moduleAppearsAsWindowTarget(mod) {
  return mod?.id
    && mod.id !== 'desktop'
    && mod.install_scope !== 'internal'
    && mod.instance_visible !== false
    && moduleLaunchesAsDesktopApp(mod)
    && canSeeModuleForAppVersion(mod);
}

function desktopAppDescriptorForModule(mod) {
  const presentation = resolvePresentation(mod);
  return {
    id: mod.id,
    title: moduleDisplayTitle(mod),
    glyph: taskbarMarkForModule(mod),
    defaultWidth: presentation.initialSize.width,
    defaultHeight: presentation.initialSize.height,
    minWidth: presentation.minimumSize.width,
    minHeight: presentation.minimumSize.height,
    defaultMode: presentation.defaultMode,
    multiInstance: presentation.multiInstance,
  };
}

async function openDesktopApp(appId, options = {}) {
  if (!state.windowManager) return null;
  const moduleDef = state.modules.find((item) => item.id === appId);
  if (moduleDef && moduleLaunchesAsDesktopApp(moduleDef)) {
    return openWindowedModule(moduleDef, options);
  }
  const entry = DESKTOP_APPS.find((app) => app.id === appId);
  if (!entry) {
    console.warn(`[desktop-app] unknown app: ${appId}`);
    return null;
  }
  const existing = findDesktopWindow(appId);
  if (existing) {
    restoreAndFocusWindow(existing);
    return existing.id;
  }
  const win = state.windowManager.create({
    title: options.title || entry.title,
    icon: entry.glyph,
    width: options.width || entry.defaultWidth,
    height: options.height || entry.defaultHeight,
    minWidth: options.minWidth || entry.minWidth,
    minHeight: options.minHeight || entry.minHeight,
    ownerId: `desktop-app:${entry.id}`,
  });
  let teardown = null;
  try {
    const moduleDef = state.modules.find((item) => item.id === appId);
    if (moduleDef) await registerModuleSchemas(moduleDef);
    const appModule = await entry.loader();
    teardown = await appModule.mount(win.container, {
      db: createScopedSystemDbFacade(`desktop-app:${entry.id}`, DESKTOP_APP_DB_COLLECTIONS[entry.id] || []),
      sync: createLiveSyncFacade(),
      commandBus: createLiveCommandBusFacade(),
      session: state.session,
      governance: state.governance,
      modules: state.modules,
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

async function openWindowedModule(mod, options = {}) {
  if (!state.windowManager || !mod?.id) return null;
  const descriptor = desktopAppDescriptorForModule(mod);
  const existing = descriptor.multiInstance ? null : findDesktopWindow(mod.id);
  if (existing) {
    restoreAndFocusWindow(existing);
    return existing.id;
  }
  const win = state.windowManager.create({
    title: options.title || descriptor.title,
    icon: descriptor.glyph,
    width: options.width || descriptor.defaultWidth,
    height: options.height || descriptor.defaultHeight,
    minWidth: options.minWidth || descriptor.minWidth,
    minHeight: options.minHeight || descriptor.minHeight,
    ownerId: `desktop-app:${mod.id}`,
  });
  const { root, content, left, right } = createWindowedModuleHost(mod);
  win.container.replaceChildren(root);

  let teardown = null;
  let cleanupWindowResizers = null;
  let moduleSyncStart = null;
  try {
    await registerModuleSchemas(mod);
    moduleSyncStart = startModuleSync(mod);
    const moduleScript = await importBusinessOsModule(
      `./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}${moduleRevisionQuery(mod)}`,
      `${mod.id} windowed module`,
    );
    if (typeof moduleScript.mount === 'function') {
      teardown = await moduleScript.mount(createModuleContext(mod, {
        host: content,
        left,
        right,
        ownerKey: `desktop-app:${mod.id}`,
        args: options.args || {},
      }));
    }
    const windowResizers = [];
    cleanupWindowResizers = setupModuleResizers(mod, {
      scope: content,
      resizers: windowResizers,
    });
  } catch (error) {
    console.error(`[module-window:${mod.id}] mount failed:`, error);
    content.innerHTML = `<p style="padding:16px;color:var(--danger);font-size:12px;">App-Start fehlgeschlagen: ${escapeHtml(String(error?.message || error))}</p>`;
  }
  state.windowManager?.setAppMode?.(win.id, options.mode || descriptor.defaultMode);
  moduleSyncStart?.catch?.(() => {});
  if ((teardown || cleanupWindowResizers) && state.eventBus) {
    const token = state.eventBus.on('window:closed', (data) => {
      if (data?.id !== win.id) return;
      state.eventBus.off('window:closed', token);
      try {
        cleanupWindowResizers?.();
      } catch (error) {
        console.error(`[module-window:${mod.id}] resizer cleanup failed:`, error);
      }
      try {
        teardown?.();
      } catch (error) {
        console.error(`[module-window:${mod.id}] teardown failed:`, error);
      }
    });
  }
  return win.id;
}

function createWindowedModuleHost(mod) {
  const root = document.createElement('div');
  root.className = 'module-root shell-window-module-root';
  root.dataset.moduleRoot = mod.id;
  const left = document.createElement('aside');
  left.className = 'module-context shell-window-module-pane shell-window-module-pane--left';
  left.dataset.moduleLeft = '';
  const content = document.createElement('main');
  content.className = 'module-content';
  content.dataset.moduleContent = '';
  const right = document.createElement('aside');
  right.className = 'module-context shell-window-module-pane shell-window-module-pane--right';
  right.dataset.moduleRight = '';
  root.append(left, content, right);
  return { root, content, left, right };
}

function findDesktopWindow(targetId) {
  return state.windowManager?.listWindows?.()
    .find((win) => win.ownerId === `desktop-app:${targetId}`) || null;
}

function restoreAndFocusWindow(win) {
  if (win.state === 'minimized') state.windowManager?.restore?.(win.id);
  state.windowManager?.focus?.(win.id);
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
  document.querySelectorAll('[data-language-select]').forEach((select) => {
    select.value = shellLang();
  });
  document.querySelectorAll('[data-theme-select]').forEach((select) => {
    select.value = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
  });
  document.querySelectorAll('[data-shell-style-select]').forEach((select) => {
    select.value = document.documentElement.dataset.shellStyle === 'macos' ? 'macos' : 'windows';
  });
}

function localizeShellChrome() {
  applyShellStaticTranslations();
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
    }, event.origin);
  } catch (error) {
    event.source?.postMessage({
      type: 'ctox-business-os-command-result',
      requestId,
      ok: false,
      error: String(error?.message || error),
    }, event.origin);
  }
}

function applyShellLanguage(lang, options = {}) {
  const value = lang === 'en' ? 'en' : 'de';
  document.documentElement.lang = value;
  applyShellStaticTranslations();
  if (options.persist !== false) {
    writeAccountPrefs({ language: value });
  }
}

// Translate the static shell chrome markup (index.html ships German defaults).
// Scoped to elements carrying data-shell-t* attributes — module content inside
// [data-module-content] never carries them, so module markup stays untouched.
// Runs at boot and on every language switch.
function applyShellStaticTranslations() {
  document.querySelectorAll('[data-shell-t]').forEach((el) => {
    el.textContent = shellText(el.dataset.shellT);
  });
  document.querySelectorAll('[data-shell-t-aria]').forEach((el) => {
    el.setAttribute('aria-label', shellText(el.dataset.shellTAria));
  });
  document.querySelectorAll('[data-shell-t-title]').forEach((el) => {
    el.setAttribute('title', shellText(el.dataset.shellTTitle));
  });
}

function postCurrentPreferencesToModule() {
  const detail = {
    theme: document.documentElement.dataset.theme === 'light' ? 'light' : 'dark',
    language: document.documentElement.lang === 'en' ? 'en' : 'de',
    branding: brandingForPreferencePayload(state.workspaceBranding),
  };
  window.dispatchEvent(new CustomEvent('ctox-business-os-preferences', { detail }));
  window.postMessage({ type: 'ctox-business-os-language', lang: detail.language }, window.location.origin);
  for (const frame of els.host?.querySelectorAll?.('iframe') || []) {
    if (isSameOriginFrame(frame)) {
      frame.contentWindow?.postMessage(
        { type: 'ctox-business-os-preferences', ...detail },
        window.location.origin,
      );
    }
  }
}

function isSameOriginFrame(frame) {
  try {
    const src = frame.getAttribute('src') || frame.src || '';
    if (!src || frame.hasAttribute('srcdoc')) return false;
    return new URL(src, window.location.href).origin === window.location.origin;
  } catch {
    return false;
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
  // Toggle the trailing-edge fade only when the pinned-app row actually
  // overflows, so a row that fits shows no faded last tab. Measured after
  // layout settles.
  requestAnimationFrame(() => {
    if (!els.tabs) return;
    els.tabs.classList.toggle('is-scrollable', els.tabs.scrollWidth > els.tabs.clientWidth + 1);
  });
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
  const lifecycle = target.kind === 'module'
    ? appLifecycleBadge(target.module, {
      session: state.session,
      governance: state.governance,
    })
    : null;
  button.innerHTML = `
    <span class="module-tab-icon" aria-hidden="true">${svgHtml || escapeHtml(target.glyph || '◻︎')}</span>
    <span class="module-tab-label">${escapeHtml(target.title || target.id)}</span>
    ${lifecycle?.runtimeInstalled ? `<span class="module-tab-lifecycle" data-app-lifecycle-badge="${escapeHtml(target.id)}" data-state="${escapeHtml(lifecycle.state)}" title="${escapeHtml(lifecycle.title)}" aria-label="${escapeHtml(`${target.title || target.id}: ${lifecycle.version} ${lifecycle.text}`)}">${escapeHtml(lifecycle.text)}</span>` : ''}
    ${lifecycle?.version && !lifecycle.runtimeInstalled ? `<span class="module-tab-version">${escapeHtml(lifecycle.version)}</span>` : ''}
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
  button.querySelector('[data-app-lifecycle-badge]')?.addEventListener('click', (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (target.kind === 'module') openAppLifecycleDrawer(target.module);
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
  return mod?.id
    && mod.id !== 'desktop'
    && mod.id !== 'notizen'
    && mod.install_scope !== 'internal'
    && mod.instance_visible !== false
    && canSeeModuleForAppVersion(mod)
    && !moduleLaunchesAsDesktopApp(mod);
}

function canSeeModuleForAppVersion(mod, governance = state.governance) {
  return lifecycleCanSeeModuleForAppVersion(mod, {
    session: state.session,
    governance,
  });
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
  const appTargets = listDesktopApps()
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
  const items = buildModuleTargetContextItems({
    target,
    pinned,
    canModify: target.kind === 'module' && canModifyModule(target.module),
    canOpenSource: target.kind === 'module' && canViewBusinessModuleSource(target.module, {
      session: state.session,
      governance: state.governance,
    }),
    labels: {
      openApp: shellText('openApp') || 'Öffnen',
      pinToTaskbar: shellText('pinToTaskbar'),
      unpinFromTaskbar: shellText('unpinFromTaskbar'),
      openSource: 'Source öffnen',
      modifyApp: shellText('chatModifyAppLabel') || 'App ändern',
    },
    actions: {
      open: () => openLaunchTarget(target),
      togglePin: () => toggleTaskbarPin(target.id, !pinned),
      openSource: () => openModuleSourceEditor(target.id),
      modify: () => openModuleEditDrawer(target.module),
    },
  });
  state.contextMenu.show(event, items);
}

function openLaunchTarget(targetOrId) {
  const target = typeof targetOrId === 'string' ? launchTargetForId(targetOrId) : targetOrId;
  if (!target) return;
  if (target.kind === 'app') {
    const existing = findDesktopWindow(target.id);
    if (existing) {
      restoreAndFocusWindow(existing);
      return;
    }
    openDesktopApp(target.id);
    return;
  }
  location.hash = target.id;
  openModule(target.id);
}

function visibleModuleFallbackId(blockedModuleId = '') {
  const activeId = state.activeModule?.id || '';
  if (activeId && activeId !== blockedModuleId) {
    const active = state.modules.find((item) => item.id === activeId);
    if (active && !moduleLaunchesAsDesktopApp(active) && canSeeModuleForAppVersion(active)) return active.id;
  }
  const desktop = state.modules.find((item) => item.id === 'desktop');
  if (desktop && desktop.id !== blockedModuleId && !moduleLaunchesAsDesktopApp(desktop) && canSeeModuleForAppVersion(desktop)) return desktop.id;
  const ctox = state.modules.find((item) => item.id === 'ctox');
  if (ctox && ctox.id !== blockedModuleId && !moduleLaunchesAsDesktopApp(ctox) && canSeeModuleForAppVersion(ctox)) return ctox.id;
  const firstVisible = state.modules.find((item) => (
    item?.id
    && item.id !== blockedModuleId
    && !moduleLaunchesAsDesktopApp(item)
    && canSeeModuleForAppVersion(item)
  ));
  return firstVisible?.id || '';
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
    const parsed = JSON.parse(readScopedLocalStorage(TASKBAR_PINS_KEY) || 'null');
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function persistTaskbarPins() {
  writeScopedLocalStorage(TASKBAR_PINS_KEY, JSON.stringify(state.taskbarPins));
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
  const doc = await withStartupTimeout(
    collection.findOne('layout').exec(),
    1500,
    null,
    'desktop_layout read',
  );
  const layout = doc?.toJSON?.() || null;
  if (Array.isArray(layout?.taskbar_pins)) {
    state.taskbarPins = normalizeTaskbarPins(layout.taskbar_pins, state.modules, { compactLegacyAllPins: true });
  } else {
    state.taskbarPins = normalizeTaskbarPins(state.taskbarPins, state.modules);
  }
  writeScopedLocalStorage(TASKBAR_PINS_KEY, JSON.stringify(state.taskbarPins));
  await withStartupTimeout(syncTaskbarPinsToDesktopLayout(), 1500, null, 'desktop_layout write');
}

async function withStartupTimeout(promise, timeoutMs, fallback, label) {
  let timer = null;
  try {
    return await Promise.race([
      promise,
      new Promise((resolve) => {
        timer = window.setTimeout(() => {
          console.warn(`[business-os] Startup ${label} timed out after ${timeoutMs}ms; continuing with fallback.`);
          resolve(fallback);
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timer) window.clearTimeout(timer);
  }
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
  const rawModuleRef = String(moduleId || '');
  const parsedModuleRef = parseHashWithParams(rawModuleRef);
  moduleId = parsedModuleRef.name || rawModuleRef.split('?')[0];
  const refArgs = parsedModuleRef.params ? searchParamsToObject(parsedModuleRef.params) : {};
  const requestedId = moduleAliases[moduleId] || moduleId;
  if (requestedId !== moduleId && currentHashModuleId() === moduleId) {
    history.replaceState(null, '', `#${requestedId}`);
  }
  const mod = state.modules.find((item) => item.id === requestedId) || state.modules[0];
  if (!mod) return;
  if (!canSeeModuleForAppVersion(mod)) {
    const lifecycle = appLifecycleState(mod, {
      session: state.session,
      governance: state.governance,
    });
    const fallbackId = visibleModuleFallbackId(mod.id);
    setStatus(`${moduleDisplayTitle(mod)} ist für diesen Account nicht sichtbar. ${lifecycle.reason || ''}`.trim());
    if (currentHashModuleId() === mod.id && fallbackId) {
      history.replaceState(null, '', `#${fallbackId}`);
    }
    if (fallbackId && fallbackId !== mod.id) {
      await openModule(fallbackId, {
        isNavHistory: true,
        force: options.force,
      });
    }
    return;
  }
  if (moduleLaunchesAsDesktopApp(mod) && !options.asModule) {
    const fallbackId = visibleModuleFallbackId(mod.id);
    const launchArgs = {
      ...currentHashArgsForModule(mod.id),
      ...refArgs,
      ...(options.args || {}),
    };
    if (fallbackId && state.activeModule?.id !== fallbackId) {
      if (currentHashModuleId() === mod.id) history.replaceState(null, '', `#${fallbackId}`);
      await openModule(fallbackId, { isNavHistory: true });
    } else if (currentHashModuleId() === mod.id && fallbackId) {
      history.replaceState(null, '', `#${fallbackId}`);
    }
    await openDesktopApp(mod.id, {
      title: moduleDisplayTitle(mod),
      width: resolvePresentation(mod).initialSize.width,
      height: resolvePresentation(mod).initialSize.height,
      minWidth: resolvePresentation(mod).minimumSize.width,
      minHeight: resolvePresentation(mod).minimumSize.height,
      mode: resolvePresentation(mod).defaultMode,
      args: launchArgs,
    });
    return;
  }
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
  teardownModuleResizers();
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
  const loadToken = ++activeLoadToken;
  els.host.replaceChildren(renderModuleFrame(mod));
  applyLoadingShadow(mod, loadToken);
  els.leftContent.replaceChildren(renderLeftContext(mod));
  els.rightContent.replaceChildren(renderRightContext(mod));
  // Fetch the module script while the schemas register — the two are
  // independent until mount() runs (mount needs the collections, the import
  // only needs the network/module cache).
  const moduleScriptPromise = importBusinessOsModule(
    `./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}${moduleRevisionQuery(mod)}`,
    `${mod.id} module`,
  );
  let moduleSyncStart = null;
  try {
    await registerModuleSchemas(mod);
    moduleSyncStart = startModuleSync(mod);
  } catch (error) {
    console.error(`[business-os] Schema registration failed for ${mod.id}`, error);
    setStatus(`Schema warning: ${error.message || error}`);
  }
  try {
    const moduleScript = await moduleScriptPromise;
    if (typeof moduleScript.mount === 'function') {
      try {
        state.activeUnmount = await moduleScript.mount(createModuleContext(mod));
      } catch (error) {
        // A failing module mount must not take the shell down with it: the
        // module usually rendered its markup before the error (data wiring is
        // what tends to fail), and shell-owned wiring below — column resizers,
        // chrome — must still run. Surface the error instead of letting it
        // escape as an unhandled rejection that silently skips the rest.
        if (isBusinessOsPermissionError(error)) {
          console.log(`[business-os] mount locked for ${mod.id}: ${error?.message || error}`);
          renderModulePermissionDeniedState(mod, error);
        } else {
          console.error(`[business-os] mount failed for ${mod.id}`, error);
        }
        setStatus(`${moduleDisplayTitle(mod)}: ${error?.message || error}`);
      }
    }
    // Wire shell-owned column resizing for any declarative handles the module
    // rendered. Runs before paint so restored widths apply without a flash.
    setupModuleResizers(mod);
  } finally {
    delete document.body.dataset.moduleLoading;
    // If a module renders no own markup (no/short-circuited mount), drop the
    // leftover shadow so we never leave a permanent fake skeleton on screen.
    els.host?.querySelector('[data-loading-shadow]')?.remove();
    els.host?.querySelector('.module-loading-note')?.remove();
    shellColumnResizeSync?.();
  }
  postCurrentPreferencesToModule();
  moduleSyncStart?.catch?.(() => {});
  window.setTimeout(() => {
    loadModuleVersionsDropdown(mod.id).catch((error) => {
      if (isRecoverableDataPlaneAbort(error) || isStaleDataPlaneGeneration(state.dataPlaneGeneration)) return;
      console.warn('[business-os] module versions unavailable:', error);
    });
  }, 0);
  syncToastRefresh?.();
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
  return usesLegacyWorkspace(mod);
}

function currentHashModuleId() {
  return location.hash.replace(/^#/, '').split('?')[0];
}

function currentHashArgsForModule(moduleId) {
  const hash = parseHashWithParams(location.hash);
  if (hash.name !== moduleId || !hash.params) return {};
  return searchParamsToObject(hash.params);
}

function searchParamsToObject(params) {
  const result = {};
  for (const [key, value] of params.entries()) {
    result[key] = value;
  }
  return result;
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
      `./${moduleBasePath(mod)}/schema.js?v=${APP_BUILD}${moduleRevisionQuery(mod)}${retryQuery}`,
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

// Phase 2: `startModuleSync` is now a thin RxDB-layer trigger — it registers
// the module's schemas (so RxDB knows the collections) and asks the sync
// runtime to begin replication for them. It NO LONGER choreographs ordering:
// the old critical-collections-ready gate and the `deferModuleSyncUntilCriticalReady`
// deferral are gone. Replication begins as soon as a module's schemas are
// registered, and which collection gets bandwidth first is decided by real
// reactive subscriptions (active-collections.mjs), not by app.js.
//
// TODO(phase2-cleanup): fold this last `state.sync.startModule` call into RxDB
// so it fires on first subscription to a collection — then app.js no longer
// touches sync at all (apps just read/write). Kept thin (not fully removed)
// because the sync runtime still owns connection-handler + signaling config,
// and moving that into RxDB is a larger, separately-shippable refactor.
function startModuleSync(mod) {
  if (!mod?.id || !state.sync || state.syncStartedModules.has(mod.id)) return Promise.resolve(null);
  if (state.schemaRetryTimers.has(mod.id)) return Promise.resolve(null);
  state.syncStartedModules.add(mod.id);
  return registerModuleSchemas(mod)
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
  // Never permanently disable a module's sync. Use fast exponential retries up to
  // the budget, then fall back to a slow PERIODIC retry — a transient failure
  // that outlasts the fast attempts (e.g. a signaling/network blip) must still
  // recover on its own without a full app reload. The counter is cleared on the
  // next successful startModuleSync (see registerModuleSchemas().then).
  const fast = retry <= MAX_TRANSIENT_MODULE_SYNC_RETRIES;
  const delayMs = fast
    ? Math.min(15000, 1000 * Math.max(1, Math.min(retry, 8)))
    : SLOW_MODULE_SYNC_RETRY_MS;
  if (retry === 1 || (fast && retry % 5 === 0) || (!fast && retry % 10 === 0)) {
    const mode = fast
      ? 'retrying'
      : `slow-retrying every ${Math.round(SLOW_MODULE_SYNC_RETRY_MS / 1000)}s`;
    console.warn(`[business-os] schema import failed for ${mod.id}; ${mode}`, error);
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
    || message.includes('RxDB database retry timed out')
    || message.includes('IndexedDB lock')
    || message.includes('IndexedDB open timed out')
    || message.includes('IndexedDB open blocked');
}

function hasLiveModulePreloadDataPlane(snapshot = state.syncDiagnostics) {
  if (typeof navigator !== 'undefined' && navigator.onLine === false) return false;
  if (!snapshot || snapshot.mode !== 'webrtc') return false;
  return Object.values(snapshot.collections || {}).some((collection) => {
    const status = collection?.connectionStatus || collection?.status || '';
    const activePeerCount = Number(collection?.frameTransport?.activePeerCount || 0);
    return activePeerCount > 0 && ['connected', 'running', 'reused'].includes(status);
  });
}

function hasModulePreloadLink(href) {
  return Array.from(document.head.querySelectorAll('link[rel="modulepreload"]'))
    .some((link) => link.getAttribute('href') === href);
}

function hasPendingModuleScriptPreloads() {
  return state.modules
    .filter((mod) => mod.id !== state.activeModule?.id)
    .some((mod) => {
      const href = `./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}${moduleRevisionQuery(mod)}`;
      return !hasModulePreloadLink(href);
    });
}

function clearModuleScriptPreloadScheduling({ resetHealth = false } = {}) {
  moduleScriptPreloadGeneration += 1;
  for (const timer of moduleScriptPreloadTimers) window.clearTimeout(timer);
  moduleScriptPreloadTimers.clear();
  if (moduleScriptPreloadResumeTimer) {
    window.clearTimeout(moduleScriptPreloadResumeTimer);
    moduleScriptPreloadResumeTimer = null;
  }
  if (moduleScriptPreloadIdleHandle !== null && 'cancelIdleCallback' in window) {
    window.cancelIdleCallback(moduleScriptPreloadIdleHandle);
  }
  moduleScriptPreloadIdleHandle = null;
  if (resetHealth) moduleScriptPreloadHealthySinceMs = 0;
}

function pauseModuleScriptPreloads(reason = 'data-plane-unavailable') {
  clearModuleScriptPreloadScheduling({ resetHealth: true });
  moduleScriptPreloadPending = moduleScriptPreloadPending || hasPendingModuleScriptPreloads();
  if (moduleScriptPreloadPending && moduleScriptPreloadPauseReason !== reason) {
    console.debug(`[business-os] module preloads paused (${reason})`);
  }
  moduleScriptPreloadPauseReason = reason;
}

function updateModuleScriptPreloadAvailability(snapshot = state.syncDiagnostics) {
  if (!hasLiveModulePreloadDataPlane(snapshot)) {
    pauseModuleScriptPreloads(
      typeof navigator !== 'undefined' && navigator.onLine === false
        ? 'browser-offline'
        : 'webrtc-peer-unavailable',
    );
    return;
  }
  if (!moduleScriptPreloadPending) return;
  moduleScriptPreloadPauseReason = '';
  if (!moduleScriptPreloadHealthySinceMs) moduleScriptPreloadHealthySinceMs = Date.now();
  armModuleScriptPreloadAfterStableHealth();
}

function armModuleScriptPreloadAfterStableHealth() {
  if (!moduleScriptPreloadPending || !hasLiveModulePreloadDataPlane()) return;
  const healthyForMs = Math.max(0, Date.now() - moduleScriptPreloadHealthySinceMs);
  const waitMs = Math.max(0, MODULE_SCRIPT_PRELOAD_STABLE_HEALTH_MS - healthyForMs);
  if (moduleScriptPreloadResumeTimer) window.clearTimeout(moduleScriptPreloadResumeTimer);
  moduleScriptPreloadResumeTimer = window.setTimeout(() => {
    moduleScriptPreloadResumeTimer = null;
    if (!hasLiveModulePreloadDataPlane()) {
      pauseModuleScriptPreloads('health-lost-before-preload');
      return;
    }
    const run = async () => {
      moduleScriptPreloadIdleHandle = null;
      await registerCustomModuleIcons().catch((error) => {
        console.debug('[business-os] deferred module icon preload skipped:', error?.message || error);
      });
      preloadModuleScripts();
    };
    if ('requestIdleCallback' in window) {
      moduleScriptPreloadIdleHandle = window.requestIdleCallback(run, { timeout: 3000 });
    } else {
      const timer = window.setTimeout(() => {
        moduleScriptPreloadTimers.delete(timer);
        run();
      }, 0);
      moduleScriptPreloadTimers.add(timer);
    }
  }, waitMs);
}

function preloadModuleScripts() {
  if (!hasLiveModulePreloadDataPlane()) {
    pauseModuleScriptPreloads('health-lost-at-preload');
    return;
  }
  clearModuleScriptPreloadScheduling();
  const generation = moduleScriptPreloadGeneration;
  const modules = state.modules.filter((mod) => mod.id !== state.activeModule?.id);
  for (const [index, mod] of modules.entries()) {
    const href = `./${moduleBasePath(mod)}/index.js?v=${APP_BUILD}${moduleRevisionQuery(mod)}`;
    if (hasModulePreloadLink(href)) continue;
    const timer = window.setTimeout(() => {
      moduleScriptPreloadTimers.delete(timer);
      if (generation !== moduleScriptPreloadGeneration) return;
      if (!hasLiveModulePreloadDataPlane()) {
        pauseModuleScriptPreloads('health-lost-during-preload');
        return;
      }
      if (hasModulePreloadLink(href)) return;
      const link = document.createElement('link');
      link.rel = 'modulepreload';
      link.href = href;
      document.head.append(link);
    }, index * MODULE_SCRIPT_PRELOAD_INTERVAL_MS);
    moduleScriptPreloadTimers.add(timer);
  }
  moduleScriptPreloadPending = false;
}

function hasStableLiveModulePreloadDataPlane() {
  return hasLiveModulePreloadDataPlane()
    && moduleScriptPreloadHealthySinceMs > 0
    && Date.now() - moduleScriptPreloadHealthySinceMs >= MODULE_SCRIPT_PRELOAD_STABLE_HEALTH_MS;
}

// Phase 2: renamed from `scheduleBackgroundModuleWork` and stripped of the
// sync-orchestration flag (`backgroundModuleWorkScheduled`). This is purely a
// render concern now — it warms the module-script HTTP cache so navigation is
// snappy. It does NOT touch sync; replication is lazy in RxDB.
function scheduleModuleScriptPreload() {
  clearModuleScriptPreloadScheduling({ resetHealth: true });
  moduleScriptPreloadPending = hasPendingModuleScriptPreloads();
  if (!moduleScriptPreloadPending) return;
  updateModuleScriptPreloadAvailability(state.syncDiagnostics);
}

window.addEventListener('offline', () => pauseModuleScriptPreloads('browser-offline'));
window.addEventListener('online', () => {
  // Do not trust the pre-offline diagnostics snapshot. The next live WebRTC
  // diagnostic resumes preloading after a new stable-health window.
  moduleScriptPreloadHealthySinceMs = 0;
});

function moduleBasePath(mod) {
  const entry = String(mod.entry || `modules/${mod.id}/index.html`)
    .replace(/^\.?\//, '')
    .split('?')[0]
    .split('#')[0];
  const slash = entry.lastIndexOf('/');
  return slash >= 0 ? entry.slice(0, slash) : `modules/${mod.id}`;
}

// The object literal below IS the platform API every Business OS module (and
// every agent-generated app) programs against — `mount(ctx)` receives it.
// The field list is pinned by docs/business-os-module-context.md and
// scripts/assert-module-context-contract.mjs: adding a field means updating
// the contract doc in the same change; removing or renaming one is a
// BREAKING module-API change and needs an explicit decision. The markers
// below are load-bearing for the contract scan — do not remove them.
function createModuleContext(mod, overrides = {}) {
  const actor = actorContext(state.session);
  const sessionUser = state.session?.user && typeof state.session.user === 'object'
    ? state.session.user
    : {};
  const hostEl = overrides.host
    || els.host.querySelector('[data-module-content]')
    || els.host.querySelector('[data-module-root]');
  const ownerKey = overrides.ownerKey || `module:${mod.id}`;
  // CTX-CONTRACT-BEGIN business-os-module-context-v1
  return {
    module: mod,
    modules: state.modules,
    getModules: () => state.modules,
    getDesktopApps: () => listDesktopApps(),
    locale: document.documentElement.lang === 'en' ? 'en' : 'de',
    shellStyle: document.documentElement.dataset.shellStyle === 'macos' ? 'macos' : 'windows',
    host: hostEl,
    left: overrides.left || els.leftContent,
    right: overrides.right || els.rightContent,
    db: createLiveDbFacade(mod),
    permissions: createModulePermissionFacade(mod),
    runtimeCapabilities: createRuntimeCapabilityFacade(mod),
    storageScope: createStorageScopeFacade(mod),
    sync: createLiveSyncFacade(),
    commandBus: createLiveCommandBusFacade(),
    actions: createAppActions({
      module: mod,
      commandBus: createLiveCommandBusFacade(),
      ensureRuntimeReady: async () => {
        const collections = Array.isArray(mod.collections) ? mod.collections.filter(Boolean) : [];
        const bridges = await Promise.all(collections.map((collection) => state.sync?.startCollection?.(collection)));
        if (collections.length && bridges.some((bridge) => !bridge)) {
          throw new Error('app collection replication is unavailable');
        }
        const readiness = bridges
          .map((bridge, index) => ({ replication: bridge?.state, collection: collections[index] }))
          .filter(({ replication }) => Boolean(replication))
          .map(async ({ replication, collection }) => {
            await replication.awaitInitialReplication?.();
            await replication.awaitInSync?.();
            const deadline = Date.now() + 30000;
            while (Date.now() < deadline) {
              const peers = replication.openPeerIds?.() || [];
              const visible = peers.some((peerId) => {
                const protocol = replication.remoteProtocolForPeer?.(peerId) || null;
                const schemas = protocol?.collectionSchemas;
                if (schemas && typeof schemas === 'object') {
                  return Boolean(schemas[collection]);
                }
                return protocol?.collection?.name === collection;
              });
              if (visible) return;
              await new Promise((resolve) => window.setTimeout(resolve, 100));
            }
            throw new Error(`native app collection ${collection} did not become visible`);
          });
        if (!readiness.length) return;
        let timeoutId = 0;
        try {
          await Promise.race([
            Promise.all(readiness),
            new Promise((_, reject) => {
              timeoutId = window.setTimeout(
                () => reject(new Error('app collection readiness timed out after 30 seconds')),
                30000,
              );
            }),
          ]);
        } finally {
          if (timeoutId) window.clearTimeout(timeoutId);
        }
      },
      hasCapability: (capability) => {
        const capabilities = state.syncDiagnostics?.remoteCapabilities;
        return Array.isArray(capabilities) ? capabilities.includes(capability) : null;
      },
    }),
    contextActions: createContextActionsFacade(mod),
    businessChat: createLiveBusinessChatFacade(mod),
    presence: createModulePresenceFacade(mod),
    syncConfig: state.sync?.config,
    session: state.session,
    actor,
    user: {
      ...actor,
      ...sessionUser,
      id: sessionUser.id || actor.id,
      display_name: sessionUser.display_name || sessionUser.name || actor.display_name,
      role: sessionUser.role || actor.role,
      is_admin: Boolean(sessionUser.is_admin || actor.is_admin),
    },
    governance: state.governance,
    eventBus: state.eventBus,
    contextMenu: state.contextMenu,
    notifications: state.notifications,
    windowManager: state.windowManager,
    desktopApps: listDesktopApps(),
    args: overrides.args || {},
    getSvgIcon: getRegisteredSvgIcon,
    getActionIcon: getRegisteredActionIcon,
    openDesktopApp,
    openBusinessChat,
    reportFileIntegrityError: (error, details = {}) => reportFileIntegrityError(ownerKey, error, {
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
  // CTX-CONTRACT-END business-os-module-context-v1
}

function createRuntimeCapabilityFacade(mod) {
  const runtimeInstalled = isRuntimeInstalledModule(mod);
  const scopedSystemCollections = scopedSystemCollectionsForModule(mod);
  const scopedSystemFacade = Boolean(scopedSystemCollections);
  const guardedDataFacade = !scopedSystemFacade && Boolean(createDynamicAppDataGuard(mod));
  return Object.freeze({
    version: 'business-os-runtime-capabilities-v1',
    module_id: mod?.id || '',
    trust_model: runtimeInstalled
      ? 'same-origin-trusted-generated-app'
      : 'packaged-core-module',
    code_origin: runtimeInstalled ? 'runtime-installed-module' : 'packaged-module',
    database: Object.freeze({
      facade: 'ctx.db',
      guarded: guardedDataFacade,
      scoped_system: scopedSystemFacade,
      allowed_collections: scopedSystemFacade ? Object.freeze([...scopedSystemCollections]) : Object.freeze([]),
      raw: guardedDataFacade
        ? 'guarded-deny-without-data-grant'
        : (scopedSystemFacade ? 'scoped-system-allowlist' : 'compatibility'),
      collection_properties: guardedDataFacade
        ? 'guarded-deny-without-data-grant'
        : (scopedSystemFacade ? 'scoped-system-allowlist' : 'compatibility'),
      cached_handles: guardedDataFacade
        ? 'guarded-deny-without-data-grant'
        : (scopedSystemFacade ? 'scoped-system-allowlist' : 'compatibility'),
    }),
    network: Object.freeze({
      fetch: runtimeInstalled ? 'local-module-assets-only' : 'packaged-module-compatibility',
      http_business_data: 'forbidden',
      remote_origin: runtimeInstalled ? 'forbidden' : 'packaged-module-compatibility',
    }),
    imports: Object.freeze({
      static_relative: 'allowed',
      dynamic: runtimeInstalled ? 'forbidden' : 'packaged-module-compatibility',
      bare_package: 'forbidden',
      remote_url: 'forbidden',
    }),
    storage: Object.freeze({
      local_storage: runtimeInstalled ? 'forbidden' : 'shell-owned-hints-only',
      session_storage: runtimeInstalled ? 'forbidden' : 'shell-owned-hints-only',
      indexed_db: 'forbidden',
      authoritative_permissions: false,
      authoritative_lifecycle: false,
      authoritative_audience: false,
      authoritative_data_grants: false,
    }),
    shell_state: Object.freeze({
      global_state_access: runtimeInstalled ? 'forbidden' : 'shell-owned',
      direct_navigation: runtimeInstalled ? 'forbidden' : 'shell-owned',
      global_shell_mutation: runtimeInstalled ? 'forbidden' : 'shell-owned',
    }),
    workers: Object.freeze({
      worker: runtimeInstalled ? 'forbidden' : 'packaged-module-compatibility',
      service_worker: 'forbidden',
    }),
    external_effects: Object.freeze({
      command_bus: 'allowed',
      app_actions: 'ctox-app-runtime-v1',
      allowed_command_bus: runtimeInstalled ? Object.freeze(['business_os.chat.task']) : Object.freeze([]),
      direct_control_commands: runtimeInstalled ? 'forbidden' : 'packaged-module-compatibility',
      approval_boundary: 'server-policy',
    }),
  });
}

// Live DB facade handed to modules as ctx.db. A Proxy forwards unknown
// property names to the live RxDB collections, so modules get the ergonomic
// `ctx.db.notes.find()` style WITHOUT unwrapping ctx.db.raw. The indirection
// through state.db is the point: the data plane can be torn down and rebuilt
// (schema-drift recovery bumps state.dataPlaneGeneration) and the facade keeps
// pointing at the live database, while an unwrapped raw handle goes stale.
// The module conformance guard (scripts/assert-module-conformance.mjs)
// forbids ctx.db.raw in modules.
const READ_COLLECTION_METHODS = new Set([
  'find',
  'findOne',
  'count',
  'exportJSON',
]);

const WRITE_COLLECTION_METHODS = new Set([
  'insert',
  'bulkInsert',
  'upsert',
  'atomicUpsert',
  'bulkUpsert',
  'bulkRemove',
  'remove',
]);

const WRITE_QUERY_METHODS = new Set([
  'remove',
]);

const WRITE_DOCUMENT_METHODS = new Set([
  'patch',
  'incrementalPatch',
  'atomicPatch',
  'atomicUpdate',
  'update',
  'remove',
  'incrementalRemove',
]);

const GUARDED_PACKAGED_DATA_MODULE_IDS = new Set([
  'buchhaltung',
  'calendar',
  'coding-agents',
  'conversations',
  'customers',
  'cv-print-builder',
  'documents',
  'invoices',
  'iot',
  'matching',
  'notes',
  'outbound',
  'research',
  'shiftflow',
  'spreadsheets',
  'support',
]);

const SETTINGS_DB_COLLECTIONS = [
  'business_commands',
  'business_module_catalog',
  'business_users',
  'channel_pairing_state',
  'communication_accounts',
  'ctox_runtime_settings',
  WORKSPACE_BRANDING_COLLECTION,
];

const BUSINESS_CHAT_DB_COLLECTIONS = [
  'business_chats',
  'business_commands',
  'ctox_queue_tasks',
  'desktop_file_chunks',
  'desktop_files',
];

const BUSINESS_REPORTER_DB_COLLECTIONS = [
  'business_module_reports',
  'ctox_bug_reports',
];

const DESKTOP_APP_DB_COLLECTIONS = {
  browser: [],
  'code-editor': [
    'business_commands',
    'business_module_source_files',
  ],
  creator: [],
  explorer: [
    'desktop_file_chunks',
    'desktop_files',
  ],
  'file-viewer': [
    'business_commands',
    'desktop_files',
  ],
};

const SCOPED_SYSTEM_MODULE_DB_COLLECTIONS = Object.freeze({
  'app-store': Object.freeze([
    'business_commands',
    'business_module_catalog',
  ]),
  'appsec-pentest': Object.freeze([
    'appsec_approvals',
    'appsec_artifacts',
    'appsec_assessments',
    'appsec_coverage',
    'appsec_findings',
    'appsec_pipeline_stages',
    'appsec_runs',
    'appsec_scanner_inventory',
    'business_commands',
  ]),
  browser: Object.freeze([
    'browser_frames',
    'browser_input_events',
    'browser_sessions',
    'browser_tabs',
    'business_commands',
    'ctox_queue_tasks',
  ]),
  creator: Object.freeze([
    'business_commands',
    'business_module_catalog',
  ]),
  ctox: Object.freeze([
    'business_commands',
    WORKSPACE_BRANDING_COLLECTION,
    'ctox_bug_reports',
    'ctox_queue_tasks',
    'ctox_runtime_settings',
  ]),
  desktop: Object.freeze([
    'business_commands',
    'desktop_icons',
    'desktop_layout',
  ]),
  documents: Object.freeze([
    'business_commands',
    'documents',
    'document_versions',
    'document_blob_chunks',
    'document_runbooks',
    'knowledge_items',
    'knowledge_runbooks',
    'knowledge_tables',
  ]),
  knowledge: Object.freeze([
    'business_commands',
    'knowledge_items',
    'knowledge_runbooks',
    'knowledge_tables',
  ]),
  research: Object.freeze([
    'business_commands',
    'business_chats',
    'ctox_queue_tasks',
    'research_tasks',
    'research_runs',
    'research_notes',
    'knowledge_tables',
    'documents',
    'document_versions',
    'document_blob_chunks',
  ]),
  reports: Object.freeze([
    'business_commands',
    'business_module_releases',
    'business_module_reports',
    'ctox_bug_reports',
    'ctox_queue_tasks',
  ]),
  threads: Object.freeze([
    'business_commands',
    'ctox_queue_tasks',
    'ctox_task_approval_requests',
    'user_notifications',
    'user_thread_links',
    'user_thread_messages',
    'user_threads',
  ]),
  tickets: Object.freeze([
    'business_commands',
    'ctox_ticket_approvals',
    'ctox_ticket_cases',
    'ctox_ticket_clarification_requests',
    'ctox_ticket_control_bundles',
    'ctox_ticket_event_routing_state',
    'ctox_ticket_events',
    'ctox_ticket_items',
    'ctox_ticket_label_assignments',
    'ctox_ticket_self_work_items',
    'ctox_ticket_self_work_notes',
    'ctox_ticket_verifications',
    'ctox_ticket_writebacks',
  ]),
});

function moduleUsesGuardedDataFacade(moduleLike = null) {
  const moduleId = String(moduleLike?.id || moduleLike?.module_id || '').trim();
  if (!moduleId) return false;
  return isRuntimeInstalledModule(moduleLike) || GUARDED_PACKAGED_DATA_MODULE_IDS.has(moduleId);
}

function scopedSystemCollectionsForModule(moduleLike = null) {
  const moduleId = String(moduleLike?.id || moduleLike?.module_id || '').trim();
  const collections = moduleId ? SCOPED_SYSTEM_MODULE_DB_COLLECTIONS[moduleId] : null;
  return Array.isArray(collections) ? collections : null;
}

function createLiveDbFacade(contextModule = null) {
  const scopedSystemCollections = scopedSystemCollectionsForModule(contextModule);
  if (scopedSystemCollections) {
    return createScopedSystemDbFacade(`module:${contextModule.id}`, scopedSystemCollections);
  }
  const guard = createDynamicAppDataGuard(contextModule);
  const base = {
    get mode() { return state.db?.mode; },
    get rxdb() { return state.db?.rxdb; },
    get raw() { return guard ? createGuardedRawDbProxy(guard) : state.db?.raw; },
    get collections() { return guard ? createGuardedCollectionsProxy(guard) : (state.db?.collections || {}); },
    addCollections: (...args) => state.db?.addCollections?.(...args),
    collection: (name) => guardedCollectionFor(guard, name),
    close: (...args) => state.db?.close?.(...args),
  };
  return new Proxy(base, {
    get(target, prop, receiver) {
      if (prop in target) return Reflect.get(target, prop, receiver);
      if (typeof prop !== 'string') return undefined;
      return guardedCollectionFor(guard, prop);
    },
    has(target, prop) {
      if (prop in target) return true;
      return typeof prop === 'string' && Boolean(state.db?.collection?.(prop));
    },
  });
}

function createScopedSystemDbFacade(scopeName, collectionNames = []) {
  const allowed = new Set(
    (Array.isArray(collectionNames) ? collectionNames : [])
      .map((name) => String(name || '').trim())
      .filter(Boolean)
  );
  const collectionFor = (name, collection = undefined) => {
    const normalized = String(name || '').trim();
    if (!normalized || !allowed.has(normalized)) return null;
    return collection === undefined ? (state.db?.collection?.(normalized) || null) : (collection || null);
  };
  const rawProxy = new Proxy({}, {
    get(_target, prop) {
      if (typeof prop !== 'string') return undefined;
      return collectionFor(prop, state.db?.raw?.[prop]);
    },
    has(_target, prop) {
      return typeof prop === 'string' && allowed.has(prop) && Boolean(state.db?.raw?.[prop]);
    },
    ownKeys() {
      return [...allowed].filter((name) => Boolean(state.db?.raw?.[name]));
    },
    getOwnPropertyDescriptor(_target, prop) {
      if (typeof prop !== 'string' || !allowed.has(prop) || !state.db?.raw?.[prop]) return undefined;
      return { enumerable: true, configurable: true };
    },
  });
  const collectionsProxy = new Proxy({}, {
    get(_target, prop) {
      if (typeof prop !== 'string') return undefined;
      return collectionFor(prop, state.db?.collections?.[prop]);
    },
    has(_target, prop) {
      return typeof prop === 'string' && allowed.has(prop) && Boolean(state.db?.collections?.[prop]);
    },
    ownKeys() {
      return [...allowed].filter((name) => Boolean(state.db?.collections?.[name]));
    },
    getOwnPropertyDescriptor(_target, prop) {
      if (typeof prop !== 'string' || !allowed.has(prop) || !state.db?.collections?.[prop]) return undefined;
      return { enumerable: true, configurable: true };
    },
  });
  const base = {
    scope: scopeName,
    get mode() { return state.db?.mode; },
    get rxdb() { return state.db?.rxdb; },
    get raw() { return rawProxy; },
    get collections() { return collectionsProxy; },
    collection: (name) => collectionFor(name),
  };
  return new Proxy(base, {
    get(target, prop, receiver) {
      if (prop in target) return Reflect.get(target, prop, receiver);
      if (typeof prop !== 'string') return undefined;
      return collectionFor(prop);
    },
    has(target, prop) {
      if (prop in target) return true;
      return typeof prop === 'string' && allowed.has(prop) && Boolean(state.db?.collection?.(prop));
    },
  });
}

function createModulePermissionFacade(moduleLike = null) {
  const guard = createDynamicAppDataGuard(moduleLike);
  const scopedSystemCollections = scopedSystemCollectionsForModule(moduleLike);
  return {
    canReadCollection: (collectionName) => {
      if (scopedSystemCollections) return scopedSystemCollections.includes(String(collectionName || '').trim());
      return !guard || guardAllowsCollectionPermission(
        guard,
        collectionName,
        BusinessOsPermissions.DataRead,
      );
    },
    canWriteCollection: (collectionName) => {
      if (scopedSystemCollections) return scopedSystemCollections.includes(String(collectionName || '').trim());
      return !guard || guardAllowsCollectionPermission(
        guard,
        collectionName,
        BusinessOsPermissions.DataWrite,
      );
    },
    canModifyApp: () => canModifyModule(moduleLike),
    canViewSource: () => canViewModuleSource(moduleLike),
    lifecycle: () => appLifecycleState(moduleLike, {
      session: state.session,
      governance: state.governance,
    }),
  };
}

function createDynamicAppDataGuard(moduleLike = null) {
  if (!moduleLike?.id || !moduleUsesGuardedDataFacade(moduleLike)) return null;
  const collections = new Set(
    (Array.isArray(moduleLike.collections) ? moduleLike.collections : [])
      .map((name) => String(name || '').trim())
      .filter(Boolean)
  );
  return {
    moduleId: moduleLike.id,
    moduleTitle: moduleDisplayTitle(moduleLike),
    collections,
  };
}

function createGuardedRawDbProxy(guard) {
  const raw = state.db?.raw || {};
  return new Proxy(raw, {
    get(target, prop, receiver) {
      if (typeof prop !== 'string') return Reflect.get(target, prop, receiver);
      return guardedCollectionFor(guard, prop, target[prop]);
    },
    has(target, prop) {
      return typeof prop === 'string' && prop in target;
    },
  });
}

function createGuardedCollectionsProxy(guard) {
  const collections = state.db?.collections || {};
  return new Proxy(collections, {
    get(target, prop, receiver) {
      if (typeof prop !== 'string') return Reflect.get(target, prop, receiver);
      return guardedCollectionFor(guard, prop, target[prop]);
    },
    has(target, prop) {
      return typeof prop === 'string' && prop in target;
    },
  });
}

function guardedCollectionFor(guard, collectionName, collection = undefined) {
  const name = String(collectionName || '').trim();
  const realCollection = collection === undefined ? state.db?.collection?.(name) : collection;
  if (!guard || !name || !realCollection) return realCollection;
  return createGuardedCollectionProxy(guard, name, realCollection);
}

function createGuardedCollectionProxy(guard, collectionName, collection) {
  return new Proxy(collection, {
    get(target, prop, receiver) {
      if (typeof prop !== 'string') return Reflect.get(target, prop, receiver);
      if (prop === '$') {
        assertGuardedCollectionPermission(guard, collectionName, BusinessOsPermissions.DataRead);
        return Reflect.get(target, prop, receiver);
      }
      if (READ_COLLECTION_METHODS.has(prop)) {
        return (...args) => {
          assertGuardedCollectionPermission(guard, collectionName, BusinessOsPermissions.DataRead);
          return wrapGuardedQueryLike(guard, collectionName, target[prop]?.apply(target, args));
        };
      }
      if (WRITE_COLLECTION_METHODS.has(prop)) {
        return (...args) => {
          assertGuardedCollectionPermission(guard, collectionName, BusinessOsPermissions.DataWrite);
          return wrapGuardedResult(guard, collectionName, target[prop]?.apply(target, args));
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === 'function' ? value.bind(target) : value;
    },
  });
}

function wrapGuardedQueryLike(guard, collectionName, query) {
  if (!query || typeof query !== 'object') return query;
  return new Proxy(query, {
    get(target, prop, receiver) {
      if (prop === '$') {
        assertGuardedCollectionPermission(guard, collectionName, BusinessOsPermissions.DataRead);
        return Reflect.get(target, prop, receiver);
      }
      if (prop === 'exec') {
        return (...args) => {
          assertGuardedCollectionPermission(guard, collectionName, BusinessOsPermissions.DataRead);
          return wrapGuardedResult(guard, collectionName, target.exec.apply(target, args));
        };
      }
      if (typeof prop === 'string' && WRITE_QUERY_METHODS.has(prop)) {
        return (...args) => {
          assertGuardedCollectionPermission(guard, collectionName, BusinessOsPermissions.DataWrite);
          return target[prop]?.apply(target, args);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === 'function' ? value.bind(target) : value;
    },
  });
}

function wrapGuardedResult(guard, collectionName, result) {
  if (result && typeof result.then === 'function') {
    return result.then((value) => wrapGuardedResult(guard, collectionName, value));
  }
  if (Array.isArray(result)) {
    return result.map((item) => wrapGuardedDocumentLike(guard, collectionName, item));
  }
  return wrapGuardedDocumentLike(guard, collectionName, result);
}

function wrapGuardedDocumentLike(guard, collectionName, doc) {
  if (!doc || typeof doc !== 'object') return doc;
  return new Proxy(doc, {
    get(target, prop, receiver) {
      if (typeof prop === 'string' && WRITE_DOCUMENT_METHODS.has(prop)) {
        return (...args) => {
          assertGuardedCollectionPermission(guard, collectionName, BusinessOsPermissions.DataWrite);
          return target[prop]?.apply(target, args);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === 'function' ? value.bind(target) : value;
    },
  });
}

function assertGuardedCollectionPermission(guard, collectionName, permission) {
  if (guardAllowsCollectionPermission(guard, collectionName, permission)) return;
  throw createBusinessOsPermissionError({
    message: `Kein ${permission === BusinessOsPermissions.DataWrite ? 'Schreib' : 'Lese'}recht für ${collectionName}.`,
    moduleId: guard.moduleId,
    collectionName,
    permission,
  });
}

function guardAllowsCollectionPermission(guard, collectionName, permission) {
  const name = String(collectionName || '').trim();
  if (!guard || !name) return true;
  // Runtime-installed app code never inherits the signed-in operator's
  // ambient collection authority. Its shell-delivered facade is confined to
  // the collections declared by that app, and data handles require a concrete
  // collection grant. Module-scope permissions decide lifecycle/open/modify
  // authority; they must not unlock arbitrary declared data collections.
  if (!guard.collections.has(name)) return false;
  return hasReviewedCollectionDataGrant(name, permission);
}

function hasReviewedCollectionDataGrant(collectionName, permission) {
  const collection = String(collectionName || '').trim();
  if (!collection || !permission) return false;
  const actor = actorContext(state.session);
  const grants = state.governance?.permission_model?.explicit_grants
    || state.governance?.governance?.permission_model?.explicit_grants
    || [];
  return (Array.isArray(grants) ? grants : []).some((grant) => {
    if (!grant || grant.active === false) return false;
    const grantId = String(grant.grant_id || '').trim();
    if (grantId.startsWith('migration.sync.')) return false;
    if (String(grant.permission || '') !== permission) return false;
    if (String(grant.scope_type || '') !== 'collection') return false;
    if (String(grant.scope_id || '').trim() !== collection) return false;
    const subjectType = String(grant.subject_type || '').trim();
    const subjectId = String(grant.subject_id || '').trim();
    if (subjectType === 'user') return Boolean(actor.id) && subjectId === actor.id;
    if (subjectType === 'role') return normalizeRole(subjectId) === actor.role;
    return false;
  });
}

function createBusinessOsPermissionError({ message, moduleId, collectionName, permission }) {
  const error = new Error(message);
  error.name = 'BusinessOsPermissionError';
  error.code = 'CTOX_BUSINESS_OS_PERMISSION_DENIED';
  error.details = {
    module_id: moduleId,
    collection: collectionName,
    permission,
  };
  return error;
}

function isBusinessOsPermissionError(error) {
  return error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
    || error?.name === 'BusinessOsPermissionError';
}

function renderModulePermissionDeniedState(mod, error) {
  const host = els.host?.querySelector('[data-module-content]') || els.host;
  if (!host) return;
  const de = shellLang() === 'de';
  const details = error?.details || {};
  const permission = String(details.permission || '').trim();
  const collection = String(details.collection || '').trim();
  const locked = document.createElement('div');
  locked.className = 'empty-state module-permission-denied-state';
  locked.dataset.modulePermissionDenied = 'true';
  if (permission) locked.dataset.permission = permission;
  if (collection) locked.dataset.collection = collection;
  locked.innerHTML = `
    <strong>${escapeHtml(de ? 'Datenzugriff fehlt' : 'Data access required')}</strong>
    <span>${escapeHtml(de
      ? 'Diese App ist sichtbar, aber die freigegebenen Daten reichen fuer diesen Bereich noch nicht aus.'
      : 'This app is visible, but the shared data access is not enough for this area.')}</span>
    <button class="text-button" type="button" data-open-app-permissions>${escapeHtml(de ? 'App-Rechte ansehen' : 'View app permissions')}</button>
  `;
  locked.querySelector('[data-open-app-permissions]')?.addEventListener('click', () => {
    openAppLifecycleDrawer(mod);
  });
  host.replaceChildren(locked);
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
    getStatus: (...args) => state.commandBus?.getStatus?.(...args),
    subscribe: (...args) => state.commandBus?.subscribe?.(...args),
  };
}

const contextActionTargets = new WeakMap();

function registeredContextActionTarget(target) {
  let current = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  while (current) {
    const descriptor = contextActionTargets.get(current);
    if (descriptor) return { element: current, descriptor };
    current = current.parentElement;
  }
  return null;
}

function createContextActionsFacade(moduleLike) {
  return Object.freeze({
    register: (element, descriptor = {}) => {
      if (!element || element.nodeType !== Node.ELEMENT_NODE) {
        throw new TypeError('Context action target must be an Element.');
      }
      const registration = Object.freeze({ ...descriptor });
      contextActionTargets.set(element, registration);
      return () => {
        if (contextActionTargets.get(element) === registration) contextActionTargets.delete(element);
      };
    },
    capture: (target, pointer = {}) => extractGlobalCtoxContext(moduleLike, target, pointer),
    dispatch: async (action, options = {}) => {
      if (!state.commandBus?.dispatch) {
        throw new Error('Business OS command bus is not available.');
      }
      const context = options.context || extractGlobalCtoxContext(moduleLike, options.target || null);
      const prompt = String(options.prompt || options.instruction || '').trim();
      if (!prompt) throw new Error('Context action instruction is required.');
      const commandType = {
        ask: 'business_os.context.ask',
        'context.ask': 'business_os.context.ask',
        data: 'business_os.data.modify',
        'data.modify': 'business_os.data.modify',
        app: 'ctox.business_os.app.modify',
        'app.modify': 'ctox.business_os.app.modify',
      }[action];
      if (!commandType) throw new Error(`Unsupported context action: ${action}`);
      const commandId = options.command_id || `cmd_${crypto.randomUUID()}`;
      const moduleId = moduleLike?.id || context.module || 'ctox';
      const extraClientContext = options.client_context && typeof options.client_context === 'object'
        ? options.client_context
        : {};
      return state.commandBus.dispatch({
        id: commandId,
        command_id: commandId,
        module: moduleId,
        command_type: commandType,
        record_id: commandType === 'ctox.business_os.app.modify'
          ? moduleId
          : (context.record_id || moduleId),
        inbound_channel: moduleId,
        payload: {
          title: options.title || prompt.slice(0, 120),
          instruction: prompt,
          prompt,
          user_message: prompt,
          mode: action,
          target: commandType === 'ctox.business_os.app.modify' ? 'app' : (action === 'ask' ? 'read' : 'data'),
          context: context.context_v2 || context,
          source_context: context,
          thread_key: `business-os/${moduleId}/${context.record_id || 'module'}`,
          response_channel: 'business_os_chat',
          outbound_channel: 'business_os_chat',
        },
        client_context: {
          action: `context-${action}`,
          module: moduleId,
          module_id: moduleId,
          app_id: moduleId,
          source_module: moduleId,
          context_version: 2,
          context: context.context_v2 || context,
          ...extraClientContext,
        },
      }, { until: 'local' });
    },
  });
}

function createLiveBusinessChatFacade(moduleLike = null) {
  return {
    open: (detail = {}) => openBusinessChat(detail),
    submitTask: (options = {}) => submitBusinessChatTask(moduleLike, options),
  };
}

// Presence (ctox-presence-v1): advisory "who is viewing/editing what" hints
// between browser peers, relayed in-memory through the native CTOX peer.
// NEVER authoritative — it must not gate any action; policy stays server-side.
// Each module publishes under its own owner key; the actor identity is
// stamped from the session so apps cannot impersonate other users' hints.
function createModulePresenceFacade(moduleLike = null) {
  const ownerKey = moduleLike?.id || 'shell';
  const registry = () => state.db?.rxdb?.getPresenceRegistry?.() || null;
  const stampEntries = (entries) => {
    const actor = actorContext(state.session);
    return (Array.isArray(entries) ? entries : [])
      .filter((entry) => entry && typeof entry === 'object' && !Array.isArray(entry))
      .map((entry) => ({
        ...entry,
        module: entry.module || ownerKey,
        actorId: actor.id,
        actorName: actor.display_name,
      }));
  };
  return {
    // Replace this module's presence hints, e.g.
    // `ctx.presence.set([{ collection, recordId, mode: 'editing' }])`.
    set(entries) {
      registry()?.setLocal(ownerKey, stampEntries(entries));
    },
    clear() {
      registry()?.clearLocal(ownerKey);
    },
    // Listener receives the OTHER peers' entries (this tab's own are not
    // echoed back). Fires immediately; returns an unsubscribe function.
    subscribe(listener) {
      return registry()?.onRemoteChange(listener) || (() => {});
    },
  };
}

async function submitBusinessChatTask(moduleLike, options = {}) {
  if (!state.commandBus?.dispatch) {
    throw new Error('Business OS command bus is not available.');
  }
  const moduleId = cleanBusinessChatValue(
    options.module || options.source_module || options.sourceModule || moduleLike?.id || 'ctox',
    'ctox',
  );
  const recordId = cleanBusinessChatValue(
    options.record_id || options.recordId || options.conversationId || options.conversation_id || '',
    '',
  );
  const commandId = cleanBusinessChatValue(
    options.id || options.command_id || options.commandId || `cmd_${crypto.randomUUID()}`,
    `cmd_${crypto.randomUUID()}`,
  );
  const title = cleanBusinessChatValue(
    options.title || options.payload?.title || `${moduleDisplayTitle(moduleLike || { id: moduleId })} task`,
    'CTOX task',
  );
  const instruction = cleanBusinessChatValue(
    options.instruction || options.payload?.instruction || options.prompt || options.user_message || title,
    title,
  );
  const prompt = cleanBusinessChatValue(
    options.prompt || options.payload?.prompt || options.user_message || instruction,
    instruction,
  );
  const threadKey = cleanBusinessChatValue(
    options.thread_key || options.threadKey || options.payload?.thread_key || (recordId ? `business-os/${moduleId}/${recordId}` : `business-os/${moduleId}/${commandId}`),
    `business-os/${moduleId}/${commandId}`,
  );
  const payload = options.payload && typeof options.payload === 'object' ? options.payload : {};
  const clientContext = options.client_context && typeof options.client_context === 'object'
    ? options.client_context
    : {};
  const command = {
    id: commandId,
    module: moduleId,
    type: 'business_os.chat.task',
    command_type: 'business_os.chat.task',
    record_id: recordId,
    inbound_channel: cleanBusinessChatValue(options.inbound_channel || options.inboundChannel || moduleId, moduleId),
    payload: {
      ...payload,
      title,
      instruction,
      prompt,
      user_message: cleanBusinessChatValue(options.user_message || options.userMessage || payload.user_message || prompt, prompt),
      mode: cleanBusinessChatValue(options.mode || payload.mode || 'data', 'data'),
      target: cleanBusinessChatValue(options.target || payload.target || 'data', 'data'),
      priority: cleanBusinessChatValue(options.priority || payload.priority || 'normal', 'normal'),
      source_module: moduleId,
      thread_key: threadKey,
      required_skills: Array.isArray(options.required_skills || options.requiredSkills || payload.required_skills)
        ? [...(options.required_skills || options.requiredSkills || payload.required_skills)]
        : [],
      record_snapshot: options.record_snapshot || options.recordSnapshot || payload.record_snapshot || {},
      writeback_contract: options.writeback_contract || options.writebackContract || payload.writeback_contract || {},
      response_channel: 'business_os_chat',
      outbound_channel: 'business_os_chat',
    },
    client_context: {
      ...clientContext,
      source: clientContext.source || 'business-os-business-chat-facade',
      module: moduleId,
      source_module: moduleId,
      surface: clientContext.surface || options.surface || `${moduleId}.business_chat.submit_task`,
      record_id: recordId,
      thread_key: threadKey,
      url: location.href,
      language: document.documentElement.lang || 'de',
    },
  };
  if (options.open !== false) {
    openBusinessChat({
      title,
      module: moduleId,
      source_module: moduleId,
      record_id: recordId,
      thread_key: threadKey,
      reuseActive: false,
    });
  }
  return state.commandBus.dispatch(command);
}

function cleanBusinessChatValue(value, fallback) {
  const text = String(value || '').trim();
  return text || fallback;
}

function renderModuleFrame(mod) {
  const root = document.createElement('div');
  root.className = 'module-root';
  root.dataset.moduleRoot = mod.id;
  root.innerHTML = `
    ${renderModuleAppBar(mod)}
    <div class="module-content" data-module-content>
      <div class="module-loading-shadow is-pending" data-loading-shadow aria-busy="true" aria-hidden="true">
        ${renderLoadingShadowFallback(mod)}
      </div>
      <div class="module-loading-note" aria-hidden="true">
        <strong>${escapeHtml(moduleDisplayTitle(mod))}</strong>
        <span>${escapeHtml(shellText('loadingModule'))}</span>
      </div>
    </div>
  `;
  return root;
}

function moduleRevisionQuery(moduleLike) {
  const moduleId = typeof moduleLike === 'string'
    ? moduleLike
    : String(moduleLike?.id || moduleLike?.module_id || '').trim();
  const mod = typeof moduleLike === 'object' && moduleLike ? moduleLike : null;
  const lifecycle = mod?.lifecycle && typeof mod.lifecycle === 'object' ? mod.lifecycle : {};
  const candidates = [
    state.moduleRevisions?.[moduleId],
    mod?.asset_revision,
    mod?.assetRevision,
    mod?.source_revision,
    mod?.sourceRevision,
    mod?.source_sha256,
    mod?.sourceSha256,
    mod?.manifest_sha256,
    mod?.manifestSha256,
    mod?.updated_at_ms,
    mod?.updatedAtMs,
    mod?.modified_at_ms,
    mod?.modifiedAtMs,
    mod?.version,
    lifecycle.last_released_at_ms,
    lifecycle.last_reviewed_at_ms,
    lifecycle.last_release_id,
  ];
  const rev = candidates
    .map((value) => String(value || '').trim())
    .find(Boolean);
  return rev ? `_${encodeURIComponent(rev).replace(/%/g, '')}` : '';
}

function moduleVersionOriginLabel(origin) {
  const de = shellLang() === 'de';
  return {
    install: de ? 'Installation' : 'Install',
    manual_release: de ? 'Release' : 'Release',
    rollback: 'Rollback',
    edit: de ? 'Bearbeitung' : 'Edit',
    creator_deploy: 'Creator',
  }[origin] || origin || 'Version';
}

async function moduleBundleVersionsFor(moduleId) {
  try {
    const doc = await state.db?.collection?.('business_module_catalog')?.findOne('module-catalog').exec();
    const data = doc?.toJSON?.();
    const versions = data?.version_states?.[moduleId]?.versions;
    return Array.isArray(versions) ? versions : [];
  } catch {
    return [];
  }
}

async function loadModuleVersionsDropdown(moduleId) {
  const select = els.host.querySelector(`[data-module-version-select="${moduleId}"]`);
  if (!select) return;
  const generation = state.dataPlaneGeneration;
  try {
    const bundleVersions = await moduleBundleVersionsFor(moduleId);
    if (isStaleDataPlaneGeneration(generation)) return;

    // Clear all but first (placeholder) option
    while (select.options.length > 1) {
      select.remove(1);
    }

    const fmtDate = (ms) => new Date(ms).toLocaleString(shellLang() === 'de' ? 'de-DE' : 'en-US', {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });

    if (bundleVersions.length > 0) {
      const group = document.createElement('optgroup');
      group.label = shellLang() === 'de' ? 'Vollständige Versionen' : 'Full versions';
      bundleVersions.forEach((version) => {
        const option = document.createElement('option');
        option.value = `modver:${version.version_id}`;
        const label = version.label || moduleVersionOriginLabel(version.origin);
        option.textContent = `#${version.seq} ${label} (${fmtDate(version.created_at_ms)})`;
        group.appendChild(option);
      });
      select.appendChild(group);
    }

    select.style.display = bundleVersions.length > 0 ? 'inline-block' : 'none';
  } catch (error) {
    if (isRecoverableDataPlaneAbort(error) || isStaleDataPlaneGeneration(generation)) return;
    console.warn('[business-os] failed to load module versions:', error);
  }
}

function renderModuleAppBar(mod) {
  if (mod?.id === 'desktop') return '';
  const title = escapeHtml(moduleDisplayTitle(mod));
  const svgHtml = getRegisteredSvgIcon(mod.id, 16, 1.8);
  const lifecycle = appLifecycleBadge(mod, {
    session: state.session,
    governance: state.governance,
  });
  const canOpenSource = shouldRenderModuleSourceAction({
    module: mod,
    canOpenSource: canViewModuleSource(mod),
  });
  return `
    <header class="module-appbar" data-module-appbar>
      <div class="module-appbar-title">
        <span class="module-appbar-icon" aria-hidden="true">${svgHtml || escapeHtml(glyphForModule(mod.id))}</span>
        <span>${title}</span>
        ${lifecycle?.version ? `<button class="module-lifecycle-chip" type="button" data-module-lifecycle="${escapeHtml(mod.id)}" data-state="${escapeHtml(lifecycle.state)}" title="${escapeHtml(lifecycle.title)}" aria-label="${escapeHtml(`${title}: ${lifecycle.version} ${lifecycle.text}`)}"><b>${escapeHtml(lifecycle.version)}</b><span>${escapeHtml(lifecycle.text)}</span></button>` : ''}
      </div>
      <div class="module-appbar-actions">
        <select class="header-select module-appbar-select" style="display: none; width: auto; max-width: 140px; margin-right: 4px;" data-module-version-select="${escapeHtml(mod.id)}" aria-label="${escapeHtml(shellText('selectVersion') || 'Version auswählen')}">
          <option value="" disabled selected>${escapeHtml(shellText('selectVersion') || 'Version...')}</option>
        </select>
        ${canOpenSource ? `
        <button class="module-appbar-button" type="button" data-module-source="${escapeHtml(mod.id)}" aria-label="Source von ${title} öffnen" title="Source öffnen">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 8l-4 4 4 4"></path><path d="M16 8l4 4-4 4"></path><path d="M14 5l-4 14"></path></svg>
        </button>
        ` : ''}
        <button class="module-appbar-button" type="button" data-module-home aria-label="${escapeHtml(shellText('showDesktop'))}" title="${escapeHtml(shellText('showDesktop'))}">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5.5h16v13H4z"></path><path d="M8 9h8M8 12h8M8 15h5"></path></svg>
        </button>
      </div>
    </header>
  `;
}

function lifecycleBadgeAriaLabel(title, lifecycle) {
  const state = [lifecycle?.version, lifecycle?.text].filter(Boolean).join(' ');
  return [title, state].filter(Boolean).join(': ');
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

// Generic instant placeholder shown the moment a module opens, before its real
// markup has been fetched. Also the fallback when the markup fetch fails.
// Column-shaped for workbench modules; the desktop is an icon surface and gets
// an icon grid instead (a 2-pane skeleton would promise columns that never
// appear). With the CSS reveal delay this is only ever seen on cold/slow loads.
function renderLoadingShadowFallback(mod) {
  if (mod?.id === 'desktop') {
    const tile = '<div class="module-loading-shadow-icon-tile"><b class="mls-icon"></b><b class="mls-icon-label"></b></div>';
    return `<div class="module-loading-shadow-icons">${tile.repeat(8)}</div>`;
  }
  return `
    <div class="module-loading-shadow-frame">
      <div class="module-loading-shadow-pane">${loadingFillRows(1, 4)}</div>
      <div class="module-loading-shadow-pane is-wide">${loadingFillRows(1, 3)}</div>
    </div>
  `;
}

function loadingFillRows(head, rows) {
  const h = head ? '<b class="mls-head"></b>' : '';
  const r = Array.from({ length: Math.max(0, rows) }, () => '<b class="mls-row"></b>').join('');
  return `<div class="module-loading-shadow-fill">${h}${r}</div>`;
}

// Derive the loading shell automatically from the module's own static layout
// (index.html + index.css) instead of a hand-authored per-module skeleton. The
// real (empty) frame is injected and a single global CSS rule turns it into a
// shimmer shadow; truly empty panes get generic shimmer fillers. `token` guards
// against races when the user switches modules quickly: a stale fetch must not
// paint over a freshly mounted (or different) module.
async function applyLoadingShadow(mod, token) {
  // The desktop's derived shadow would be an empty JS-filled stub; its icon
  // grid fallback already shows the right shape.
  if (mod?.id === 'desktop') return;
  const base = moduleBasePath(mod);
  ensureModuleStylesheet(base);
  let markup = '';
  try {
    const res = await fetch(
      `./${base}/index.html?v=${APP_BUILD}${moduleRevisionQuery(mod)}`,
      // Versioned URL: the server marks it immutable; revisits must not refetch.
      { cache: 'force-cache' },
    );
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    markup = await res.text();
  } catch (error) {
    console.warn(`[business-os] loading shadow markup failed for ${mod.id}; keeping generic placeholder`, error);
    return;
  }
  if (token !== activeLoadToken) return;
  if (document.body.dataset.moduleLoading !== mod.id) return;
  const shadow = els.host?.querySelector('[data-loading-shadow].is-pending');
  if (!shadow || !shadow.isConnected) return;

  let frag;
  try {
    const doc = new DOMParser().parseFromString(markup, 'text/html');
    doc.querySelectorAll('script, link, style, template, noscript').forEach((el) => el.remove());
    // Avoid duplicate-id / form collisions during the brief overlap with mount.
    doc.querySelectorAll('[id]').forEach((el) => el.removeAttribute('id'));
    doc.querySelectorAll('input, textarea, select, button').forEach((el) => {
      el.setAttribute('disabled', '');
      el.setAttribute('tabindex', '-1');
    });
    fillEmptyPanes(doc.body);
    frag = doc.body.innerHTML;
  } catch (error) {
    console.warn(`[business-os] loading shadow parse failed for ${mod.id}`, error);
    return;
  }
  if (token !== activeLoadToken || !shadow.isConnected) return;
  shadow.innerHTML = frag;
  shadow.classList.remove('is-pending');
}

// Inject the module's stylesheet ahead of mount so the derived shadow is styled.
// Matches the module's own `ensureStyles()` href shape; a duplicate <link> to an
// identical sheet is harmless (the browser dedupes the fetch) and doubles as a
// preload for the real mount.
function ensureModuleStylesheet(base) {
  const already = Array.from(document.querySelectorAll('link[rel="stylesheet"]'))
    .some((l) => l.href.includes(`/${base}/index.css`));
  if (already) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = `${base}/index.css?v=${APP_BUILD}`;
  link.dataset.loadingShadowCss = base;
  document.head.append(link);
}

// Most modules ship a frame in index.html but fill its panes from JS at mount
// (e.g. outbound/ctox have empty <section> shells). Those panes would render as
// empty boxes in the shadow, so drop generic shimmer rows into any leaf pane
// that has no content of its own.
function fillEmptyPanes(scope) {
  const panes = scope.querySelectorAll(
    'section, aside, [class*="pane"], [class*="-left"], [class*="-center"], [class*="-right"], [class*="sidebar"], [class*="column"]',
  );
  const SKIP = 'button, a, input, textarea, select, hr, [class*="resizer"], [class*="splitter"], [class*="handle"], [class*="divider"]';
  panes.forEach((pane) => {
    if (pane.closest('[data-loading-filled]')) return;
    if (pane.matches(SKIP)) return;
    const hasContent = pane.querySelector('*') || (pane.textContent || '').trim().length > 0;
    if (hasContent) return;
    pane.setAttribute('data-loading-filled', '');
    pane.innerHTML = loadingFillRows(1, 4);
  });
}

function readModuleLayout() {
  try {
    return JSON.parse(readScopedLocalStorage(MODULE_LAYOUT_KEY) || '{}') || {};
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
  writeScopedLocalStorage(MODULE_LAYOUT_KEY, JSON.stringify(layout));
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

function openAppLifecycleDrawer(mod) {
  if (!mod?.id) return;
  const lifecycle = appLifecycleState(mod, {
    session: state.session,
    governance: state.governance,
  });
  const releaseProjection = appReleaseProjection(mod);
  const canModify = canModifyModule(mod);
  const canOpenSource = canViewModuleSource(mod);
  const canRelease = canUseModulePermission(mod, BusinessOsPermissions.AppsRelease);
  const canRollback = canUseModulePermission(mod, BusinessOsPermissions.AppsRollback);
  const permissionView = buildLifecyclePermissionView({
    canManage: Boolean(lifecycle.canManage || canModify),
    canOpenSource,
  });
  const dataAccess = releaseProjection.dataAccess;
  const whyDiagnostics = buildModuleWhyDiagnosticsView({
    actor: state.session?.user,
    module: mod,
    lifecycle,
    releaseProjection,
    dataAccess,
    permissionView,
    permissions: {
      canSee: lifecycle.public === true || lifecycle.canAccessNonPublic === true || lifecycle.state === 'packaged',
      canOpen: lifecycle.public === true || lifecycle.canAccessNonPublic === true || lifecycle.state === 'packaged',
      canModify,
      canOpenSource,
      canRelease,
      canRollback,
    },
    dataPermissions: buildLifecycleDataPermissionDiagnostics(mod, dataAccess),
  });
  const releaseFact = releaseProjection.hasReleaseState
    ? releaseProjection.releaseLine
    : 'Noch kein Release projiziert';
  const rollbackFact = releaseProjection.rollbackLine || 'Noch kein Rollback-Ziel projiziert';
  const dataAccessFact = dataAccess?.summary || 'Keine Datenbereiche deklariert';
  const body = document.createElement('div');
  body.className = 'drawer-body module-lifecycle-drawer';
  body.dataset.lifecyclePermissionState = permissionView.state;
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>${escapeHtml(moduleDisplayTitle(mod))}</h2>
        <p>${escapeHtml(mod.id)}</p>
      </div>
      <button class="icon-button" type="button" data-close-lifecycle aria-label="Schließen">×</button>
    </header>
    <section class="module-lifecycle-summary" data-state="${escapeHtml(lifecycle.state)}">
      <div class="module-lifecycle-mark" aria-hidden="true">${escapeHtml(lifecycle.state === 'team' ? 'T' : (lifecycle.state === 'preview' ? 'V' : (lifecycle.state === 'restricted' ? 'S' : 'P')))}</div>
      <div>
        <strong>${escapeHtml(lifecycle.label)}</strong>
        <span>${escapeHtml(lifecycle.versionLabel || 'Version fehlt')}</span>
      </div>
    </section>
    <section class="module-lifecycle-access" data-lifecycle-permission-state="${escapeHtml(permissionView.state)}" aria-label="App-Rechte">
      <strong>${escapeHtml(permissionView.label)}</strong>
      <span>${escapeHtml(permissionView.description)}</span>
    </section>
    <dl class="module-lifecycle-facts">
      <div>
        <dt>Sichtbarkeit</dt>
        <dd>${escapeHtml(lifecycle.reason)}</dd>
      </div>
      <div>
        <dt>Standard nach Version</dt>
        <dd>${lifecycle.state === 'restricted' ? 'Eingeschränkte Team-Version' : (lifecycle.public ? 'Team-sichtbar' : 'Privat bis zur Team-Version')}</dd>
      </div>
      <div>
        <dt>Freigabe</dt>
        <dd>${escapeHtml(releaseFact)}</dd>
      </div>
      <div>
        <dt>Rollback</dt>
        <dd>${escapeHtml(rollbackFact)}</dd>
      </div>
      <div>
        <dt>Datenzugriff</dt>
        <dd>${escapeHtml(dataAccessFact)}</dd>
      </div>
      ${dataAccess?.reviewNote ? `
      <div>
        <dt>Review</dt>
        <dd>${escapeHtml(dataAccess.reviewNote)}</dd>
      </div>
      ` : ''}
    </dl>
    <div class="module-lifecycle-actions">
      <button class="text-button account-primary" type="button" data-open-lifecycle-app>App öffnen</button>
      ${canModify ? '<button class="text-button" type="button" data-edit-lifecycle-app>App ändern</button>' : ''}
      ${canOpenSource ? '<button class="text-button" type="button" data-open-lifecycle-source>Source öffnen</button>' : ''}
      <button class="text-button" type="button" data-open-lifecycle-store>${escapeHtml(permissionView.storeActionLabel)}</button>
    </div>
    ${renderModuleWhyDiagnosticsHtml({ view: whyDiagnostics })}
    <p class="module-lifecycle-note">App-Sichtbarkeit entscheidet, wer die App sieht. Daten bleiben separat über Datenrechte geschützt.</p>
  `;
  body.querySelector('[data-close-lifecycle]')?.addEventListener('click', closeDrawers);
  body.querySelector('[data-open-lifecycle-app]')?.addEventListener('click', () => {
    closeDrawers();
    openModule(mod.id);
  });
  body.querySelector('[data-edit-lifecycle-app]')?.addEventListener('click', () => {
    closeDrawers();
    openModuleEditDrawer(mod);
  });
  body.querySelector('[data-open-lifecycle-source]')?.addEventListener('click', () => {
    closeDrawers();
    openModuleSourceEditor(mod.id);
  });
  body.querySelector('[data-open-lifecycle-store]')?.addEventListener('click', () => {
    closeDrawers();
    location.hash = 'app-store';
    openModule('app-store');
  });
  openDrawer('right', body);
}

function buildLifecycleDataPermissionDiagnostics(mod, dataAccess = {}) {
  const moduleId = String(mod?.id || mod?.module_id || '').trim();
  const areas = Array.isArray(dataAccess?.areas) ? dataAccess.areas : [];
  const collections = new Set([
    ...cleanStringList(dataAccess?.declared),
    ...cleanStringList(dataAccess?.granted),
    ...cleanStringList(dataAccess?.locked),
    ...areas.map((area) => String(area?.collection || '').trim()).filter(Boolean),
  ]);
  if (!moduleId || !collections.size) return [];
  return [...collections].map((collection) => {
    const area = areas.find((item) => String(item?.collection || '').trim() === collection) || {};
    return {
      collection,
      readAllowed: canUseModuleDataPermission(mod, collection, BusinessOsPermissions.DataRead),
      writeAllowed: canUseModuleDataPermission(mod, collection, BusinessOsPermissions.DataWrite),
      readReviewState: String(area.read || '').trim(),
      writeReviewState: String(area.write || '').trim(),
    };
  });
}

function cleanStringList(value) {
  if (!Array.isArray(value)) return [];
  return value.map((item) => String(item || '').trim()).filter(Boolean);
}

function canUseModulePermission(mod, permission) {
  const moduleId = String(mod?.id || mod?.module_id || '').trim();
  if (!moduleId || !permission) return false;
  return canUseBusinessPermission({
    session: state.session,
    governance: state.governance,
    permission,
    scopeType: 'module',
    scopeId: moduleId,
  });
}

function canUseModuleDataPermission(mod, collectionName, permission) {
  const collection = String(collectionName || '').trim();
  if (!collection || !permission) return false;
  return hasReviewedCollectionDataGrant(collection, permission);
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
          <p>${escapeHtml(shellText('gateSubtitle'))}</p>
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
          <label for="gate-user">${escapeHtml(shellText('gateUser'))}</label>
          <div class="auth-gate-input-wrapper">
            <input
              id="gate-user"
              name="user"
              autocomplete="username"
              value="${escapeHtml(savedUser)}"
              placeholder="${escapeHtml(shellText('gateUserPlaceholder'))}"
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
          <label for="gate-password">${escapeHtml(shellText('gatePassword'))}</label>
          <div class="auth-gate-input-wrapper">
            <input
              id="gate-password"
              type="password"
              name="password"
              autocomplete="current-password"
              placeholder="${escapeHtml(shellText('gatePassword'))}"
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
          <button class="auth-gate-button" type="submit" data-gate-submit>${escapeHtml(shellText('gateSubmit'))}</button>
          ${loginUrl ? `<a class="auth-gate-external" href="${escapeHtml(loginUrl)}">${escapeHtml(shellText('gateSso'))}</a>` : ''}
        </div>
      </form>
      `}

      <footer class="auth-gate-footer">
        <small>CTOX Business OS · ${escapeHtml(shellText('gateFooter'))}</small>
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
    showGateError(shellText('gateInvalidCredentials'));
  }

  form.addEventListener('submit', async (event) => {
    // Submit in-page so a failed attempt shows the inline error without a
    // full-page reload — a reload re-paints the workspace startup loader and
    // makes it look like data is loading before the auth error appears.
    event.preventDefault();
    errorEl.hidden = true;

    const user = userInput.value.trim();
    const password = passwordInput.value;

    if (!user || !password) {
      showGateError("Bitte Benutzername und Passwort eingeben.");
      return;
    }

    clearStoredBrowserAuth();
    localStorage.removeItem(LOGGED_OUT_KEY);
    writeAccountPrefs({ loginUser: user });
    const originalLabel = submitBtn.textContent;
    submitBtn.disabled = true;
    submitBtn.textContent = "Verbindung wird hergestellt...";

    const restoreSubmit = () => {
      submitBtn.disabled = false;
      submitBtn.textContent = originalLabel;
    };

    try {
      const params = new URLSearchParams();
      params.set('user', user);
      params.set('password', password);
      const response = await fetch('/login', {
        method: 'POST',
        headers: {
          Accept: 'application/json',
          'Content-Type': 'application/x-www-form-urlencoded',
        },
        body: params.toString(),
      });
      if (response.ok) {
        // Auth cookie is set server-side; boot the authenticated workspace.
        window.location.assign('/');
        return;
      }
      restoreSubmit();
      passwordInput.value = '';
      showGateError(shellText('gateInvalidCredentials'));
      passwordInput.focus();
    } catch (error) {
      restoreSubmit();
      showGateError("Verbindung fehlgeschlagen. Bitte erneut versuchen.");
    }
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
    const parsed = readStoredPairingConfig();
    if (parsed) {
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
    const label = user.display_name || user.name || user.id || prefs.displayName || 'Account';
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
        <p>${escapeHtml(shellText('drawerLoginHint'))}</p>
      </div>
      <button class="icon-button" type="button" data-close-account aria-label="${escapeHtml(shellText('windowClose'))}">×</button>
    </header>
    <form class="account-form" data-login-form method="post" action="/login">
      <label>
        <span>${escapeHtml(shellText('gateUser'))}</span>
        <input name="user" autocomplete="username" value="${escapeHtml(savedUser)}" placeholder="${escapeHtml(shellText('gateUserPlaceholder'))}" />
      </label>
      <label>
        <span>${escapeHtml(shellText('gatePassword'))}</span>
        <input type="password" name="password" autocomplete="current-password" placeholder="${escapeHtml(shellText('gatePassword'))}" />
      </label>
      <button class="text-button account-primary" type="submit">${escapeHtml(shellText('drawerLoginSubmit'))}</button>
      ${loginUrl ? `<a class="text-button" href="${escapeHtml(loginUrl)}">${escapeHtml(shellText('drawerLoginExternal'))}</a>` : ''}
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
  const displayName = user.display_name || user.name || prefs.displayName || '';
  body.innerHTML = `
    <header class="drawer-header-row">
      <div>
        <h2>Account</h2>
        <p>${escapeHtml(user.display_name || user.id || 'CTOX User')} · ${escapeHtml(roleDisplayName(role))}</p>
      </div>
      <button class="icon-button" type="button" data-close-account aria-label="${escapeHtml(shellText('windowClose'))}">×</button>
    </header>
    <section class="account-role-card">
      <span>Rolle</span>
      <strong>${escapeHtml(roleDisplayName(role))}</strong>
      <small>${escapeHtml(roleDescription(role))}</small>
    </section>
    <form class="account-form" data-profile-form>
      <label>
        <span>Anzeigename</span>
        <input name="displayName" value="${escapeHtml(displayName)}" placeholder="Name" />
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
      <small data-profile-status>Anzeigename wird im Team gespeichert. Sprache bleibt lokal und wird beim Laden der Module angewendet.</small>
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
  body.querySelector('[data-profile-form]')?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const formEl = event.currentTarget;
    const submit = formEl.querySelector('button[type="submit"]');
    const statusEl = formEl.querySelector('[data-profile-status]');
    const form = new FormData(formEl);
    const nextDisplayName = form.get('displayName')?.toString().trim() || '';
    const language = form.get('language')?.toString() || 'de';
    if (!nextDisplayName) {
      statusEl.textContent = 'Bitte Anzeigenamen eingeben.';
      statusEl.dataset.state = 'error';
      return;
    }
    const prefs = writeAccountPrefs({
      ...readAccountPrefs(),
      displayName: '',
      language,
    });
    applyShellLanguage(prefs.language);
    syncHeaderControls();
    postCurrentPreferencesToModule();
    submit.disabled = true;
    statusEl.textContent = 'Account wird gespeichert...';
    statusEl.dataset.state = '';
    try {
      await saveCurrentSessionUserProfile(nextDisplayName);
      statusEl.textContent = 'Account gespeichert.';
      statusEl.dataset.state = 'ok';
      closeDrawers();
    } catch (error) {
      statusEl.textContent = error?.message || 'Account konnte nicht gespeichert werden.';
      statusEl.dataset.state = 'error';
    } finally {
      submit.disabled = false;
    }
  });
  body.querySelector('[data-logout]')?.addEventListener('click', () => {
    clearStoredBrowserAuth();
    localStorage.setItem(LOGGED_OUT_KEY, '1');
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

async function saveCurrentSessionUserProfile(displayName) {
  const currentUser = state.session?.user || {};
  const userId = String(
    currentUser.id || currentUser.user_id || currentUser.email || currentUser.login || '',
  ).trim();
  if (!userId) {
    throw new Error('Benutzer-ID fehlt.');
  }
  const role = normalizeRole(currentUser.role || (currentUser.is_admin ? 'admin' : 'user'));
  if (!roleCanManage(role)) {
    throw new Error('Nur Admins können Accounts bearbeiten.');
  }
  const command = await dispatchShellModuleCommand({
    commandType: 'ctox.business_os.user.upsert',
    moduleId: 'ctox',
    recordId: userId,
    payload: {
      id: userId,
      display_name: displayName,
      role,
      active: currentUser.active !== false,
    },
    source: 'business-os-account',
  });
  const payload = command?.result || command || {};
  if (command?.status === 'failed' || payload?.ok === false) {
    throw new Error(payload?.error || command?.error || 'Account konnte nicht gespeichert werden.');
  }
  const users = Array.isArray(payload.users) ? payload.users : [];
  const savedUser = users.find((candidate) => {
    const candidateId = String(candidate?.id || candidate?.user_id || '').trim();
    return candidateId === userId;
  }) || {};
  const savedRole = normalizeRole(savedUser.role || role);
  state.session = {
    ...state.session,
    user: {
      ...currentUser,
      ...savedUser,
      id: savedUser.id || savedUser.user_id || currentUser.id || userId,
      user_id: savedUser.user_id || savedUser.id || currentUser.user_id || userId,
      display_name: savedUser.display_name || displayName,
      role: savedRole,
      is_admin: roleCanAdmin(savedRole),
      active: savedUser.active !== false,
    },
  };
  renderAccountButton(state.session);
  return payload;
}

function readAccountPrefs() {
  try {
    return JSON.parse(readScopedLocalStorage(ACCOUNT_PREFS_KEY) || '{}') || {};
  } catch {
    return {};
  }
}

function writeAccountPrefs(nextPrefs) {
  const prefs = { ...readAccountPrefs(), ...(nextPrefs || {}) };
  writeScopedLocalStorage(ACCOUNT_PREFS_KEY, JSON.stringify(prefs));
  return prefs;
}

function clearStoredBrowserAuth() {
  localStorage.removeItem(SESSION_TOKEN_KEY);
  localStorage.removeItem(AUTH_HEADER_KEY);
}

function roleCanAdmin(role) {
  return roleCanManage(role);
}

function canModifyModule(mod, governance = state.governance) {
  return canModifyBusinessModule(mod, {
    session: state.session,
    governance,
  });
}

function canViewModuleSource(mod, governance = state.governance) {
  return canViewBusinessModuleSource(mod, {
    session: state.session,
    governance,
  });
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
    businessReporterModulePromise = import(`./shared/business-reporter.js?v=${APP_BUILD}`);
  }
  return businessReporterModulePromise;
}

function loadBusinessChatModule() {
  if (!businessChatModulePromise) {
    businessChatModulePromise = import(`./shared/business-chat.js?v=${APP_BUILD}`);
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
        db: createScopedSystemDbFacade('business-reporter-companion', BUSINESS_REPORTER_DB_COLLECTIONS),
        sync: createLiveSyncFacade(),
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
        db: createScopedSystemDbFacade('business-chat-companion', BUSINESS_CHAT_DB_COLLECTIONS),
        sync: createLiveSyncFacade(),
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
    const svgIcon = await resolveModuleIconSvg(mod);
    if (svgIcon) {
      registerSvgIcon(mod.id, svgIcon);
    }
  }
}

async function resolveModuleIconSvg(mod) {
  if (!mod?.id) return '';
  const inlineSvg = inlineModuleIconSvg(mod);
  if (inlineSvg) return inlineSvg;
  if (state.moduleIconSvgCache.has(mod.id)) return state.moduleIconSvgCache.get(mod.id);
  const assetPath = moduleIconAssetPath(mod);
  if (!assetPath) return '';
  // External icon files are optional render assets. Do not start them while
  // the daemon/network is down or before the WebRTC peer has been stable long
  // enough; inline manifest icons remain available immediately.
  if (!hasStableLiveModulePreloadDataPlane()) return '';
  try {
    const response = await fetch(`./${assetPath}?v=${APP_BUILD}${moduleRevisionQuery(mod)}`, { cache: 'force-cache' });
    if (!response.ok) {
      state.moduleIconSvgCache.set(mod.id, '');
      return '';
    }
    const svg = (await response.text()).trim();
    if (!svg.includes('<svg')) {
      state.moduleIconSvgCache.set(mod.id, '');
      return '';
    }
    state.moduleIconSvgCache.set(mod.id, svg);
    return svg;
  } catch (error) {
    state.moduleIconSvgCache.set(mod.id, '');
    return '';
  }
}

function inlineModuleIconSvg(mod) {
  const candidates = [
    mod?.layout?.icon_svg,
    mod?.icon_svg,
    mod?.iconSvg,
  ];
  for (const candidate of candidates) {
    const svg = typeof candidate === 'string' ? candidate.trim() : '';
    if (svg.includes('<svg')) return svg;
  }
  return '';
}

function moduleIconAssetPath(mod) {
  const iconPath = String(mod?.icon || mod?.icon_path || 'icon.svg').trim();
  if (!iconPath || iconPath.includes('..') || /^[a-z][a-z0-9+.-]*:/i.test(iconPath)) return '';
  const cleanPath = iconPath.replace(/^\.?\//, '').split('?')[0].split('#')[0];
  if (!cleanPath || cleanPath.includes('..')) return '';
  if (cleanPath.startsWith('modules/') || cleanPath.startsWith('installed-modules/')) {
    return cleanPath;
  }
  return `${moduleBasePath(mod)}/${cleanPath}`;
}

async function refreshModules() {
  const activeModuleId = state.activeModule?.id || '';
  const activeModuleRevisionBefore = activeModuleId ? moduleRevisionQuery(state.activeModule) : '';
  const activeModuleSignatureBefore = activeModuleId ? moduleActivationSignature(state.activeModule) : '';
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
  state.governance = modules.governance || state.governance;
  await registerCustomModuleIcons();
  state.moduleLayout = normalizeModuleLayout(state.moduleLayout || readModuleLayout(), state.modules);
  persistModuleLayout();
  renderTabs();
  state.eventBus?.emitAsync?.('modules:changed', {
    modules: state.modules,
    governance: state.governance,
    moduleAllowlist: state.moduleAllowlist,
    catalogFingerprint: state.moduleCatalogFingerprint,
  });
  // Phase 2: re-warm the module-script cache after a catalog change. Pure
  // render concern — no sync orchestration.
  scheduleModuleScriptPreload();
  refreshRemoteShellStateInBackground();

  // If the URL hash requests a module that wasn't previously loaded, but is now available, open it!
  const hashId = currentHashModuleId();
  if (hashId && hashId !== state.activeModule?.id) {
    const matched = state.modules.find((m) => m.id === hashId);
    if (matched) {
      console.log(`[business-os] URL hash #${hashId} is now available after catalog refresh. Opening module.`);
      await openModule(hashId);
    }
  } else if (activeModuleId) {
    const refreshedActiveModule = state.modules.find((m) => m.id === activeModuleId);
    const activeModuleRevisionAfter = refreshedActiveModule ? moduleRevisionQuery(refreshedActiveModule) : '';
    const activeModuleSignatureAfter = refreshedActiveModule ? moduleActivationSignature(refreshedActiveModule) : '';
    if (
      refreshedActiveModule
      && (
        activeModuleRevisionAfter !== activeModuleRevisionBefore
        || activeModuleSignatureAfter !== activeModuleSignatureBefore
      )
    ) {
      console.info('[business-os] active module catalog changed; remounting module', {
        module_id: activeModuleId,
        revision_before: activeModuleRevisionBefore,
        revision_after: activeModuleRevisionAfter,
      });
      await openModule(activeModuleId, { force: true });
    }
  }
}

function moduleActivationSignature(mod) {
  if (!mod || typeof mod !== 'object') return '';
  try {
    return JSON.stringify({
      id: mod.id || '',
      entry: mod.entry || '',
      version: mod.version || '',
      source: mod.source || '',
      core: Boolean(mod.core),
      install_scope: mod.install_scope || '',
      launch_kind: mod.launch_kind || mod.layout?.launch_kind || '',
      shell: mod.layout?.shell || '',
      collections: Array.isArray(mod.collections)
        ? mod.collections.map((name) => String(name || '').trim()).filter(Boolean).sort()
        : [],
    });
  } catch {
    return `${mod.id || ''}:${mod.entry || ''}:${mod.version || ''}`;
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

function startWorkspaceBrandingMonitor() {
  if (state.workspaceBrandingSubscription) {
    try { state.workspaceBrandingSubscription.unsubscribe(); } catch (error) {}
    state.workspaceBrandingSubscription = null;
  }
  state.workspaceBranding = applyWorkspaceBranding(null);
  const coll = state.db?.collection?.(WORKSPACE_BRANDING_COLLECTION);
  if (!coll?.findOne) return;
  const applyBrandingDocument = (doc) => {
    const previousStatus = els.status?.textContent?.trim() || '';
    const previousWorkspaceName = workspaceStatusText();
    state.workspaceBranding = applyWorkspaceBranding(doc?.toJSON?.() || null);
    if (isWorkspaceStatusText(previousStatus) || previousStatus === previousWorkspaceName) {
      setWorkspaceStatus();
    }
    postCurrentPreferencesToModule();
  };
  const loadCurrentBrandingDocument = () => coll
    .findOne(WORKSPACE_BRANDING_DOCUMENT_ID)
    .exec()
    .then(applyBrandingDocument)
    .catch((error) => {
      console.debug('[business-os] workspace branding read skipped:', error?.message || error);
    });
  loadCurrentBrandingDocument();
  state.sync?.startCollection?.(WORKSPACE_BRANDING_COLLECTION)
    ?.then?.(loadCurrentBrandingDocument)
    ?.catch?.((error) => {
      console.debug('[business-os] workspace branding sync start skipped:', error?.message || error);
    });
  state.workspaceBrandingSubscription = coll
    .findOne(WORKSPACE_BRANDING_DOCUMENT_ID)
    .$
    .subscribe(applyBrandingDocument);
}

async function refreshShellCtoxHealth() {
  try {
    const status = await loadShellCtoxHealth();
    state.ctoxHealth = status;
    renderShellCtoxWarning(status);
    renderShellCtoxVersion(status);
  } catch (error) {
    const status = {
      ok: false,
      pending: isPendingCtoxHealthError(error),
      error: error?.message || String(error),
    };
    state.ctoxHealth = status;
    renderShellCtoxWarning(status);
    renderShellCtoxVersion(status);
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

function renderShellCtoxVersion(status = state.ctoxHealth) {
  const container = els.ctoxVersion;
  if (!container) return;
  if (!sessionCanManageCtoxPlatform()) {
    container.hidden = true;
    container.removeAttribute('title');
    return;
  }
  const platform = status?.runtime_settings?.platform || null;
  const version = platformDisplayVersion(platform?.version || platform?.release_tag || '');
  if (!version) {
    container.hidden = true;
    container.removeAttribute('title');
    return;
  }
  maybeRefreshCtoxUpdateCheck(platform);
  const check = currentCtoxUpdateCheck();
  const updateAvailable = check?.update_available === true;
  const latest = platformDisplayVersion(check?.latest_release || '');
  const labelEl = container.querySelector('[data-ctox-version-label]');
  const button = container.querySelector('[data-ctox-update-button]');
  const parts = [`CTOX ${version}`];
  if (state.ctoxUpdateInstallRunning) {
    parts.push(shellText('ctoxUpdateInstalling'));
  } else if (updateAvailable) {
    parts.push(latest ? `${latest} ${shellText('ctoxUpdateAvailable')}` : shellText('ctoxUpdateAvailable'));
  } else if (state.ctoxUpdateCheckRunning) {
    parts.push(shellText('ctoxUpdateChecking'));
  }
  if (labelEl) labelEl.textContent = parts.join(' · ');
  container.title = ctoxVersionTitle(platform, check);
  container.hidden = false;
  if (button) {
    button.hidden = !updateAvailable;
    button.disabled = state.ctoxUpdateInstallRunning;
    button.textContent = state.ctoxUpdateInstallRunning
      ? shellText('ctoxUpdateInstalling')
      : shellText('ctoxUpdateInstall');
    button.title = latest
      ? `${shellText('ctoxUpdateInstall')}: ${latest}`
      : shellText('ctoxUpdateInstall');
  }
}

function sessionCanManageCtoxPlatform(session = state.session) {
  const user = session?.user || {};
  return Boolean(
    session?.authenticated
    && (user.is_admin || roleCanManage(user.role || '')),
  );
}

function platformDisplayVersion(value) {
  const raw = String(value || '').trim();
  if (!raw) return '';
  return raw.startsWith('v') ? raw : `v${raw}`;
}

function currentCtoxUpdateCheck() {
  return state.ctoxUpdateCheck?.check || null;
}

function ctoxVersionTitle(platform, check) {
  const lines = [];
  const version = platformDisplayVersion(platform?.version || platform?.release_tag || '');
  if (version) lines.push(`Version: ${version}`);
  if (platform?.current_release) lines.push(`Release: ${platform.current_release}`);
  if (platform?.install_mode) lines.push(`Install: ${platform.install_mode}`);
  if (check?.latest_release) lines.push(`Latest: ${check.latest_release}`);
  if (check?.published_at) lines.push(`Published: ${check.published_at}`);
  if (state.ctoxUpdateInstallStatus) lines.push(state.ctoxUpdateInstallStatus);
  return lines.join('\n');
}

function shouldPollCtoxUpdateCheck(platform) {
  return sessionCanManageCtoxPlatform()
    && Boolean(platform?.release_channel_configured)
    && businessOsHttpControlPlaneAvailableForUpdates();
}

function businessOsHttpControlPlaneAvailableForUpdates() {
  const host = String(window.location?.hostname || '').trim().toLowerCase();
  if (!host) return false;
  if (host === 'localhost' || host === '127.0.0.1' || host === '0.0.0.0' || host === '::1') {
    return true;
  }
  if (host.endsWith('.localhost')) return true;
  // ctox.dev instance subdomains intentionally do not expose Business OS HTTP
  // API paths. Browser data and operational state come through RxDB/WebRTC
  // there, so polling the admin update endpoint only creates a visible 410.
  if (host === 'ctox.dev' || host.endsWith('.ctox.dev')) return false;
  return true;
}

function maybeRefreshCtoxUpdateCheck(platform) {
  if (!shouldPollCtoxUpdateCheck(platform)) return;
  if (state.ctoxUpdateCheckRunning || state.ctoxUpdateInstallRunning) return;
  const now = Date.now();
  if (now - state.ctoxUpdateCheckedAtMs < CTOX_UPDATE_CHECK_POLL_MS) return;
  state.ctoxUpdateCheckRunning = true;
  state.ctoxUpdateCheckedAtMs = now;
  fetchBusinessOsControlJson('/api/business-os/ctox/update/check')
    .then((payload) => {
      state.ctoxUpdateCheck = payload;
      state.ctoxUpdateCheckedAtMs = Date.now();
    })
    .catch((error) => {
      state.ctoxUpdateCheck = {
        ok: false,
        error: error?.message || String(error),
        check: null,
      };
    })
    .finally(() => {
      state.ctoxUpdateCheckRunning = false;
      renderShellCtoxVersion(state.ctoxHealth);
    });
}

async function installCtoxUpdateFromShell(event) {
  event?.preventDefault?.();
  event?.stopPropagation?.();
  if (!sessionCanManageCtoxPlatform() || state.ctoxUpdateInstallRunning) return;
  if (!confirm(shellText('ctoxUpdateConfirm'))) return;
  state.ctoxUpdateInstallRunning = true;
  state.ctoxUpdateInstallStatus = shellText('ctoxUpdateInstalling');
  renderShellCtoxVersion(state.ctoxHealth);
  try {
    const payload = await fetchBusinessOsControlJson('/api/business-os/ctox/update/apply', {
      method: 'POST',
      body: '{}',
    });
    state.ctoxUpdateInstallStatus = payload?.status === 'started'
      ? shellText('ctoxUpdateStarted')
      : (payload?.status || shellText('ctoxUpdateStarted'));
    setStatus(state.ctoxUpdateInstallStatus);
  } catch (error) {
    state.ctoxUpdateInstallRunning = false;
    state.ctoxUpdateInstallStatus = error?.message || String(error);
    setStatus(state.ctoxUpdateInstallStatus, true);
  } finally {
    renderShellCtoxVersion(state.ctoxHealth);
  }
}

async function fetchBusinessOsControlJson(url, options = {}) {
  const headers = {
    Accept: 'application/json',
    ...(options.body ? { 'Content-Type': 'application/json' } : {}),
    ...(options.headers || {}),
  };
  const response = await fetch(url, {
    method: options.method || 'GET',
    headers,
    body: options.body,
    credentials: 'same-origin',
    cache: 'no-store',
  });
  const text = await response.text();
  let payload = null;
  try {
    payload = text ? JSON.parse(text) : null;
  } catch {
    payload = null;
  }
  if (!response.ok) {
    throw new Error(payload?.message || payload?.error || text || `HTTP ${response.status}`);
  }
  if (payload && payload.ok === false) {
    throw new Error(payload.error || payload.message || 'CTOX control-plane request failed.');
  }
  return payload || { ok: true };
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
  const pairedConfig = await readBusinessOsLaunchConfig();
  const explicitLogout = localStorage.getItem(LOGGED_OUT_KEY) === '1';
  const freshUrlPairingLaunch = pairedConfig?.source === 'url' && allowsPairingConfigSession(pairedConfig);
  if (explicitLogout && !freshUrlPairingLaunch) {
    return {
      ok: true,
      authenticated: false,
      auth_required: true,
      reason: 'logged_out',
    };
  }
  if (explicitLogout && freshUrlPairingLaunch) {
    localStorage.removeItem(LOGGED_OUT_KEY);
  }

  const injected = readInjectedDesktopSession();
  if (injected?.authenticated) return injected;

  if (pairedConfig && allowsPairingConfigSession(pairedConfig)) {
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

  if (injected) return injected;

  clearStoredBrowserAuth();

  return {
    ok: false,
    authenticated: false,
    auth_required: true,
    reason: 'pairing_config_missing',
  };
}

function allowsPairingConfigSession(config = null) {
  if (isLocalBusinessOsSurface() || location.protocol === 'file:') return true;
  const source = String(config?.source || '').trim().toLowerCase();
  // A public web-deploy launch URL carries a short-lived pairing credential in
  // `ctox_config` or explicit URL parameters. That URL is the auth handoff for
  // RxDB/WebRTC-only instances; stored browser pairing remains restricted to
  // local/private surfaces.
  return source === 'url';
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
  return location.protocol === 'ctox-business-os:'
    || ['127.0.0.1', 'localhost', '::1'].includes(location.hostname);
}

async function loadModules(options = {}) {
  const normalized = typeof options === 'number' ? { timeoutMs: options } : (options || {});
  const allowShellSeed = normalized.allowShellSeed !== false && allowsPackagedModuleCatalogSeed();
  const catalog = await loadModuleCatalog(normalized.timeoutMs, {
    allowShellSeed,
  });
  const merged = await ensurePackagedModuleList(
    normalizeModuleList(catalog.modules),
    { allowShellSeed }
  );
  // Remember the catalog-provided allowlist so desktop-app gating (listDesktopApps)
  // stays in sync with the tab list. Only overwrite when the synced catalog actually
  // carries it — the packaged shell seed has no allowed_module_ids and must not clear
  // a previously-synced restriction.
  if (Array.isArray(catalog.allowed_module_ids)) {
    state.moduleAllowlist = catalog.allowed_module_ids;
  }
  // Per-instance app allowlist: when the instance scopes its visible apps, the
  // server projects `allowed_module_ids` into the catalog doc (RxDB data plane)
  // and `module_allowlist` into the injected launch config (instant at startup,
  // so there is no flash of disallowed apps before the catalog syncs). Empty/unset
  // means no restriction — every packaged module is surfaced.
  const governance = catalog.governance || state.governance || null;
  const modules = filterModulesForAppVersionVisibility(
    applyModuleAllowlist(merged, catalog.allowed_module_ids),
    governance,
  );
  return {
    ok: catalog.ok !== false,
    modules,
    governance,
    catalogFingerprint: moduleCatalogFingerprint({ ...catalog, modules }),
  };
}

async function waitForRequestedHashModule(modules, timeoutMs = 45000) {
  const hashId = currentHashModuleId();
  if (!hashId) return modules;
  const hasRequestedModule = (candidate) => Array.isArray(candidate?.modules)
    && candidate.modules.some((mod) => mod.id === hashId);
  if (hasRequestedModule(modules)) return modules;
  if (!state.db?.collection?.('business_module_catalog')) return modules;

  console.log(`[business-os] Waiting for requested runtime module #${hashId} in RxDB module catalog.`);
  state.sync?.startCollection?.('business_module_catalog').catch((error) => {
    console.warn('[business-os] requested module catalog sync start failed:', error);
  });

  const deadline = Date.now() + timeoutMs;
  let latest = modules;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const next = await loadModules({ timeoutMs: 5000, allowShellSeed: false });
      latest = next;
      if (hasRequestedModule(next)) {
        console.log(`[business-os] Requested runtime module #${hashId} arrived in RxDB module catalog.`);
        return next;
      }
    } catch (error) {
      lastError = error;
    }
    await delay(500);
  }
  console.warn(
    `[business-os] Requested runtime module #${hashId} did not arrive before initial shell open; `
      + `continuing with available modules.`,
    lastError || '',
  );
  return latest || modules;
}

function resolveModuleAllowlist(catalogAllowlist) {
  const fromCatalog = Array.isArray(catalogAllowlist) ? catalogAllowlist : [];
  const cfg = (typeof window !== 'undefined' && window.CTOX_BUSINESS_OS_CONFIG) || null;
  const fromConfig = Array.isArray(cfg?.module_allowlist) ? cfg.module_allowlist : [];
  const allow = new Set();
  for (const id of [...fromCatalog, ...fromConfig]) {
    const trimmed = String(id || '').trim();
    if (trimmed) allow.add(trimmed);
  }
  if (allow.has('app-store')) {
    allow.add('creator');
  }
  return allow;
}

function applyModuleAllowlist(modules, catalogAllowlist) {
  const allow = resolveModuleAllowlist(catalogAllowlist);
  if (allow.size === 0) return modules; // no restriction configured
  return normalizeModuleList(modules)
    .filter((mod) => allow.has(String(mod?.id || '').trim()) || moduleBypassesInstanceAllowlist(mod));
}

function moduleBypassesInstanceAllowlist(mod) {
  // Tenant allowlists scope packaged apps; native runtime-installed apps and
  // operator-placed local modules (runtime local-modules/, git-ignored) still
  // need lifecycle/policy filtering so freshly created apps can open.
  return mod?.instance_visible === true
    && (isRuntimeInstalledModule(mod) || mod?.source === 'local');
}

function filterModulesForAppVersionVisibility(modules, governance = state.governance) {
  return normalizeModuleList(modules)
    .filter((mod) => canSeeModuleForAppVersion(mod, governance));
}

function allowsPackagedModuleCatalogSeed() {
  const config = (typeof window !== 'undefined' && window.CTOX_BUSINESS_OS_CONFIG) || null;
  const hosting = String(config?.app_hosting || config?.appHosting || '').trim();
  if (config?.ctox_instance_required === true || config?.ctoxInstanceRequired === true) return true;
  if (String(config?.sync_mode || config?.syncMode || '').trim() === 'p2p-first') return true;
  // Public web deployments must render the server-projected RxDB catalog. The
  // packaged registry is code metadata only there; inserting it locally widens
  // tenant-scoped shells before the real projection arrives.
  if (hosting === 'web_deploy' || hosting === 'ctox_dev_web_deploy' || hosting === 'desktop_web_deploy') return false;
  if (hosting === 'ctox_instance_webserver' || hosting === 'ctox_instance') return true;
  return isLocalBusinessOsSurface();
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
      const merged = mergePackagedCatalogModules(cachedCatalog.modules, shellCatalog.modules);
      for (const id of merged.changedIds) {
        if (!state.shellCatalogMergedIds.has(id)) {
          state.shellCatalogMergedIds.add(id);
          console.log(`[business-os] Merging packaged module metadata locally: ${id}`);
        }
      }
      if (merged.changed) {
        const mergedCatalog = {
          ...cachedCatalog,
          modules: merged.modules,
          updated_at_ms: Date.now(),
          source: cachedCatalog.source || 'business-os-shell',
        };
        return normalizeModuleCatalog(mergedCatalog);
      }
    }
    return normalizeModuleCatalog(cachedCatalog);
  }

  const syncStart = state.sync?.startCollection?.('business_module_catalog');
  syncStart?.catch((error) => {
    console.warn('[business-os] module catalog sync start failed during shell seed startup', error);
  });

  if (shellCatalog) {
    // The packaged catalog is only a cold-start UI fallback. The persisted
    // business_module_catalog is owned by the native CTOX runtime so freshly
    // created installed modules cannot be shadowed by the shell seed.
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
  return mergePackagedCatalogModules(normalized, shellCatalog.modules).modules;
}

function mergePackagedCatalogModules(cachedModules, packagedModules) {
  const merged = normalizeModuleList(cachedModules);
  const changedIds = [];
  for (const shellMod of normalizeModuleList(packagedModules)) {
    const index = merged.findIndex((mod) => mod.id === shellMod.id);
    if (index < 0) {
      merged.push(shellMod);
      changedIds.push(shellMod.id);
      continue;
    }
    const current = merged[index];
    const next = {
      ...current,
      ...shellMod,
      layout: {
        ...(current.layout || {}),
        ...(shellMod.layout || {}),
      },
      store: {
        ...(current.store || {}),
        ...(shellMod.store || {}),
      },
    };
    if (JSON.stringify(current) !== JSON.stringify(next)) {
      merged[index] = next;
      changedIds.push(shellMod.id);
    }
  }
  return {
    modules: normalizeModuleList(merged),
    changed: changedIds.length > 0,
    changedIds,
  };
}

async function readModuleCatalogProjection(coll) {
  const doc = await coll.findOne('module-catalog').exec();
  const data = doc?.toJSON?.();
  if (data && data._deleted !== true && data.is_deleted !== true) return data;
  return null;
}

function getOfflineFallbackCatalog() {
  return {
    "ok": true,
    "modules": [
        {
            "id": "desktop",
            "title": "Desktop",
            "description": "Workspace landing surface with switchable Windows/macOS chrome, draggable icons, taskbar/dock, and live CTOX activity notifications.",
            "entry": "modules/desktop/index.html",
            "collections": [
                "business_commands",
                "desktop_icons",
                "desktop_layout",
                "desktop_notifications"
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
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Business OS workspace shell with file access, app launching, desktop icons, and taskbar state.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/desktop",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "category": "Workspace",
            "tags": [
                "desktop",
                "files",
                "launcher",
                "notifications"
            ]
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-app-store\"><defs><linearGradient id=\"grad-app-store\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#f59e0b\" /><stop offset=\"100%\" stop-color=\"#ec4899\" /></linearGradient></defs><path d=\"M21 8H3a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h18a2 2 0 0 0 2-2V10a2 2 0 0 0-2-2z\" fill=\"url(#grad-app-store)\" fill-opacity=\"0.12\" stroke=\"url(#grad-app-store)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M16 8A4 4 0 0 0 8 8\" stroke=\"url(#grad-app-store)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><rect x=\"5\" y=\"12\" width=\"5\" height=\"5\" rx=\"1\" fill=\"url(#grad-app-store)\" fill-opacity=\"0.25\" stroke=\"url(#grad-app-store)\" stroke-width=\"1.2\"></rect><rect x=\"14\" y=\"12\" width=\"5\" height=\"5\" rx=\"1\" fill=\"url(#grad-app-store)\" fill-opacity=\"0.25\" stroke=\"url(#grad-app-store)\" stroke-width=\"1.2\"></rect></svg>",
                "left": "Categories and Search",
                "center": "Available Applications Catalog",
                "right": "Application Details and Actions",
                "default_width": 1120,
                "default_height": 760,
                "min_width": 640,
                "min_height": 480
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Discover CTOX repository modules, create apps from templates, and manage installed Business OS modules.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/app-store",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1120,
                    "height": 760
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Development",
            "tags": [
                "marketplace",
                "github",
                "modules",
                "governance"
            ]
        },
        {
            "id": "appsec-pentest",
            "title": "AppSec Pentest",
            "description": "CTOX-native AppSec assessment console for scanner readiness, coverage, findings, evidence metadata, and active-scan approvals.",
            "entry": "modules/appsec-pentest/index.html",
            "collections": [
                "business_commands",
                "appsec_assessments",
                "appsec_runs",
                "appsec_artifacts",
                "appsec_findings",
                "appsec_coverage",
                "appsec_pipeline_stages",
                "appsec_scanner_inventory",
                "appsec_approvals"
            ],
            "source": "core",
            "core": true,
            "editable": true,
            "deletable": false,
            "layout": {
                "shell": "windowed",
                "left": "assessment and coverage navigation",
                "center": "selected assessment evidence workbench",
                "right": "scanner readiness, active approvals, and command status",
                "default_width": 1280,
                "default_height": 820,
                "min_width": 640,
                "min_height": 480
            },
            "category": "Security",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "appsec",
                "pentest",
                "security",
                "scanners",
                "approvals"
            ],
            "store": {
                "summary": "Native AppSec/Pentest console over CTOX durable AppSec projections and WebRTC-only Business OS data.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/appsec-pentest",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1280,
                    "height": 820
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "browser",
            "title": "Browser",
            "description": "Browser window for opening web pages through the CTOX computer.",
            "entry": "modules/browser/index.html",
            "collections": [
                "business_commands",
                "browser_sessions",
                "browser_tabs",
                "browser_frames",
                "browser_input_events",
                "ctox_queue_tasks"
            ],
            "source": "core",
            "core": true,
            "editable": false,
            "deletable": false,
            "launch_kind": "desktop-app",
            "install_scope": "core",
            "default_installed": true,
            "category": "Workspace",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "browser",
                "remote",
                "playwright"
            ],
            "store": {
                "summary": "Open websites through the CTOX computer from Business OS.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/browser",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "layout": {
                "shell": "windowed",
                "default_width": 1120,
                "default_height": 760,
                "min_width": 640,
                "min_height": 480,
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-browser\"><defs><linearGradient id=\"grad-browser\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0ea5e9\" /><stop offset=\"100%\" stop-color=\"#22c55e\" /></linearGradient></defs><rect x=\"3\" y=\"4\" width=\"18\" height=\"16\" rx=\"3\" fill=\"url(#grad-browser)\" fill-opacity=\"0.12\" stroke=\"url(#grad-browser)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><path d=\"M3 9h18\" stroke=\"url(#grad-browser)\" stroke-width=\"2\" stroke-linecap=\"round\"></path><circle cx=\"7\" cy=\"6.5\" r=\"0.8\" fill=\"url(#grad-browser)\"></circle><circle cx=\"10\" cy=\"6.5\" r=\"0.8\" fill=\"url(#grad-browser)\"></circle><path d=\"M8 15h8M12 11v8\" stroke=\"url(#grad-browser)\" stroke-width=\"1.7\" stroke-linecap=\"round\"></path></svg>",
                "top": "browser tabs and address bar",
                "center": "web page"
            },
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1120,
                    "height": 760
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
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
                "accounting_bank_statement_lines",
                "accounting_number_series"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-buchhaltung\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-buchhaltung\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#818cf8\" /><stop offset=\"100%\" stop-color=\"#db2777\" /></linearGradient></defs><path d=\"M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z\" fill=\"url(#grad-buchhaltung)\" fill-opacity=\"0.12\" stroke=\"url(#grad-buchhaltung)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M8 11h8M8 15h5M9 7h6\" stroke=\"url(#grad-buchhaltung)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path></svg>",
                "left": "Fibu-Navigationsstruktur & Kontenrahmen-Wähler",
                "center": "Aktiver Arbeitsbereich & Journale",
                "right": "Zugeordnete Belege, AI-Vorschläge & Begleitaktionen"
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Professionelle deutsche Finanzbuchhaltung mit SKR03/SKR04, DATEV-Exporten und GoBD-Unveränderbarkeit.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/buchhaltung",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1120,
                    "height": 760
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Finance",
            "tags": [
                "buchhaltung",
                "fibu",
                "datev",
                "elster",
                "hgb"
            ]
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-calendar\"><defs><linearGradient id=\"grad-calendar\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#3b82f6\" /><stop offset=\"100%\" stop-color=\"#8b5cf6\" /></linearGradient></defs><rect x=\"3\" y=\"4\" width=\"18\" height=\"16\" rx=\"3\" ry=\"3\" fill=\"url(#grad-calendar)\" fill-opacity=\"0.12\" stroke=\"url(#grad-calendar)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><line x1=\"3\" y1=\"9\" x2=\"21\" y2=\"9\" stroke=\"url(#grad-calendar)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"9\" y1=\"9\" x2=\"9\" y2=\"20\" stroke=\"url(#grad-calendar)\" stroke-width=\"1.2\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></line><line x1=\"15\" y1=\"9\" x2=\"15\" y2=\"20\" stroke=\"url(#grad-calendar)\" stroke-width=\"1.2\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></line><path d=\"M8 2v3M16 2v3\" stroke=\"url(#grad-calendar)\" stroke-width=\"2\" stroke-linecap=\"round\"></path><rect x=\"5\" y=\"12\" width=\"3\" height=\"3\" rx=\"0.5\" fill=\"url(#grad-calendar)\" fill-opacity=\"0.3\" stroke=\"url(#grad-calendar)\" stroke-width=\"1\"></rect><rect x=\"10\" y=\"12\" width=\"4\" height=\"5\" rx=\"1\" fill=\"url(#grad-calendar)\" fill-opacity=\"0.3\" stroke=\"url(#grad-calendar)\" stroke-width=\"1\"></rect></svg>",
                "left": "Mini-Calendar & Lists",
                "center": "Calendar Grid",
                "right": "Inspector & Booking Pages"
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Local-first spatial calendar app with native booking pages, slot reservations, and recurrence rules.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/calendar",
                "installable": false,
                "editable_after_install": true,
                "distribution": "starter-module"
            },
            "install_scope": "starter",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1120,
                    "height": 760
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Productivity",
            "tags": [
                "calendar",
                "booking",
                "local-first",
                "scheduler"
            ]
        },
        {
            "id": "coding-agents",
            "title": "Coding Agents",
            "description": "Unified dashboard to manage, configure, license, and remotely run Antigravity, Claude, and Codex agents.",
            "entry": "modules/coding-agents/index.html",
            "collections": [
                "business_commands",
                "coding_agent_workspace_grants",
                "coding_agent_sessions",
                "coding_agent_events"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-coding-agents\"><defs><linearGradient id=\"grad-coding-agents\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#8b5cf6\" /><stop offset=\"100%\" stop-color=\"#3b82f6\" /></linearGradient></defs><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\" ry=\"2\" fill=\"url(#grad-coding-agents)\" fill-opacity=\"0.12\" stroke=\"url(#grad-coding-agents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><path d=\"M9 16l-3-3 3-3M15 10l3 3-3 3\" stroke=\"url(#grad-coding-agents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><line x1=\"13\" y1=\"8\" x2=\"11\" y2=\"16\" stroke=\"url(#grad-coding-agents)\" stroke-width=\"2\" stroke-linecap=\"round\"></line></svg>",
                "left": "Agent selector and remote connection status",
                "center": "Unified Agent Control, subscriptions, permission bypasses, and active terminal",
                "right": "Sessions, remote jobs logs, and CLI runner",
                "third_pane_justification": "Coding-agent work needs persistent workspace selection, active session workbench, and session/history inspection visible together so long-running provider tasks can be monitored without hiding the active prompt surface."
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Unified dashboard to manage, configure, license, and remotely run Antigravity, Claude, and Codex agents.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/coding-agents",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "store",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Development",
            "tags": [
                "coding",
                "agents",
                "automation",
                "dev-tools"
            ]
        },
        {
            "id": "consent",
            "title": "Einwilligungen",
            "description": "Generisches DSGVO-Einwilligungs-/Rechtsgrundlagen-Register mit Zweck und Löschfrist.",
            "entry": "modules/consent/index.html",
            "collections": [
                "business_commands",
                "business_consents"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "left": "Filter",
                "center": "Einwilligungen"
            },
            "category": "Operations",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "consent",
                "ats"
            ],
            "store": {
                "summary": "Generisches DSGVO-Einwilligungs-/Rechtsgrundlagen-Register mit Zweck und Löschfrist.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/consent",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 960,
                    "height": 680
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
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
                "communication_messages",
                "outbound_campaigns",
                "outbound_pipeline_items",
                "outbound_engagements",
                "outbound_messages",
                "outbound_approvals"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-conversations\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-conversations\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#4f46e5\" /><stop offset=\"100%\" stop-color=\"#7c3aed\" /></linearGradient></defs><path d=\"M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z\" fill=\"url(#grad-conversations)\" fill-opacity=\"0.12\" stroke=\"url(#grad-conversations)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"9\" cy=\"11\" r=\"1.5\" fill=\"url(#grad-conversations)\"></circle><circle cx=\"13\" cy=\"11\" r=\"1.5\" fill=\"url(#grad-conversations)\"></circle><circle cx=\"17\" cy=\"11\" r=\"1.5\" fill=\"url(#grad-conversations)\"></circle></svg>",
                "left": "Conversation list filtered by channel and search",
                "center": "Selected conversation timeline with channel-aware messages",
                "right": "Contact card, related business records, and CTOX agent attribution",
                "third_pane_justification": "The contact and linked-record inspector must stay visible beside long communication timelines in wide mode; compact mode moves it into the shared drawer."
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Read-only communication timeline across connected channels with business record cross-links.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/conversations",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "store",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Collaboration",
            "tags": [
                "communication",
                "audit",
                "threads",
                "channels"
            ]
        },
        {
            "id": "creator",
            "title": "App Creator",
            "description": "Business OS app request workspace for handing app creation and modification tasks to CTOX agents.",
            "entry": "modules/creator/index.html",
            "collections": [
                "business_commands",
                "business_module_catalog"
            ],
            "source": "core",
            "core": true,
            "editable": true,
            "deletable": false,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-creator\"><defs><linearGradient id=\"grad-creator\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#06b6d4\" /><stop offset=\"100%\" stop-color=\"#0891b2\" /></linearGradient></defs><polyline points=\"7 8 3 12 7 16\" stroke=\"url(#grad-creator)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><polyline points=\"17 8 21 12 17 16\" stroke=\"url(#grad-creator)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><line x1=\"14\" y1=\"6\" x2=\"10\" y2=\"18\" stroke=\"url(#grad-creator)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><path d=\"M18 4l.5 1.5L20 6l-1.5.5L18 8l-.5-1.5L16 6l1.5-.5z\" fill=\"url(#grad-creator)\"></path><path d=\"M6 18l.25.75L7 19l-.75.25L6 20l-.25-.75L5 19l.75-.25z\" fill=\"url(#grad-creator)\"></path></svg>",
                "left": "App request and metadata inputs",
                "center": "App request status, installed apps, and CTOX task handoff",
                "default_width": 1200,
                "default_height": 800,
                "min_width": 640,
                "min_height": 480
            },
            "version": "0.1.0",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Local-first workspace for creating Business-OS app requests and tracking installed apps.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/creator",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1200,
                    "height": 800
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Development",
            "tags": [
                "creator",
                "developer-tools",
                "app-creation",
                "local-first",
                "architecture"
            ]
        },
        {
            "id": "credentials",
            "title": "Zugangsdaten",
            "description": "Write-only manager for provider credentials and API keys. Values are stored in the encrypted CTOX secret store and are never read back into the browser.",
            "entry": "modules/credentials/index.html",
            "collections": [
                "business_commands"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-credentials\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-credentials\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#14b8a6\" /><stop offset=\"100%\" stop-color=\"#6366f1\" /></linearGradient></defs><path d=\"M12 2l8 3v6c0 5-3.5 8-8 11-4.5-3-8-6-8-11V5l8-3z\" fill=\"url(#grad-credentials)\" fill-opacity=\"0.12\" stroke=\"url(#grad-credentials)\" stroke-width=\"2\" stroke-linejoin=\"round\"></path><circle cx=\"12\" cy=\"10\" r=\"2.4\" stroke=\"url(#grad-credentials)\" stroke-width=\"2\"></circle><path d=\"M12 12.4V16\" stroke=\"url(#grad-credentials)\" stroke-width=\"2\" stroke-linecap=\"round\"></path></svg>",
                "left": "Credential catalog and status",
                "center": "Set, rotate and remove credentials",
                "right": "Security notes"
            },
            "category": "Security",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Write-only credentials manager backed by the encrypted CTOX secret store. Set, rotate and remove provider credentials; values never leave the daemon.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/credentials",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 960,
                    "height": 680
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "tags": [
                "secrets",
                "credentials",
                "api-keys",
                "security"
            ]
        },
        {
            "id": "ctox",
            "title": "CTOX",
            "description": "Native control surface for queues, runs, sync state, and agent context.",
            "entry": "modules/ctox/index.html",
            "collections": [
                "business_commands",
                "business_chats",
                "ctox_runtime_settings",
                "business_workspace_branding",
                "ctox_queue_tasks",
                "ctox_runs",
                "ctox_bug_reports",
                "business_module_acl",
                "business_module_releases",
                "business_module_reports",
                "business_module_source_files"
            ],
            "source": "core",
            "core": true,
            "editable": true,
            "deletable": false,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-ctox\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-ctox\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#10b981\" /><stop offset=\"100%\" stop-color=\"#06b6d4\" /></linearGradient></defs><polygon points=\"12 2 22 8 22 16 12 22 2 16 2 8\" fill=\"url(#grad-ctox)\" fill-opacity=\"0.12\" stroke=\"url(#grad-ctox)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polygon><polyline points=\"12 22 12 12 22 8\" stroke=\"url(#grad-ctox)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><polyline points=\"12 12 2 8\" stroke=\"url(#grad-ctox)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><polyline points=\"12 2 12 12\" stroke=\"url(#grad-ctox)\" stroke-width=\"1.5\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polyline><circle cx=\"12\" cy=\"12\" r=\"3.5\" fill=\"url(#grad-ctox)\" stroke=\"#ffffff\" stroke-width=\"1\"></circle></svg>",
                "left": "runtime scopes",
                "center": "active workbench",
                "right": "agent context",
                "default_width": 1320,
                "default_height": 860,
                "min_width": 640,
                "min_height": 480
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Native CTOX control surface for queue tasks, runs, module reports, releases, and source evidence.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/ctox",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1320,
                    "height": 860
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "System",
            "tags": [
                "control-plane",
                "queue",
                "runs",
                "governance"
            ]
        },
        {
            "id": "customers",
            "title": "Kunden",
            "description": "Native CRM-App für Bestandskunden, Kontakte, Opportunities, Aufgaben, Notizen, Aktivitäten und Outbound-Handoffs.",
            "entry": "modules/customers/index.html",
            "collections": [
                "business_commands",
                "customer_accounts",
                "customer_contacts",
                "customer_opportunities",
                "customer_tasks",
                "customer_notes",
                "customer_activities",
                "customer_files",
                "customer_views",
                "customer_view_filters",
                "customer_view_sorts",
                "customer_import_batches",
                "customer_dedupe_candidates"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-customers\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-customers\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0f766e\" /><stop offset=\"100%\" stop-color=\"#2563eb\" /></linearGradient></defs><path d=\"M4 20V7a3 3 0 0 1 3-3h10a3 3 0 0 1 3 3v13\" fill=\"url(#grad-customers)\" fill-opacity=\"0.12\" stroke=\"url(#grad-customers)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M8 20v-5a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v5\" stroke=\"url(#grad-customers)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M8 8h2M14 8h2M8 11h8\" stroke=\"url(#grad-customers)\" stroke-width=\"1.8\" stroke-linecap=\"round\"></path><circle cx=\"18\" cy=\"6\" r=\"3\" fill=\"#ffffff\" stroke=\"url(#grad-customers)\" stroke-width=\"1.5\"></circle><path d=\"M16.9 6h2.2M18 4.9v2.2\" stroke=\"url(#grad-customers)\" stroke-width=\"1.3\" stroke-linecap=\"round\"></path></svg>",
                "left": "Customer segments, saved views and review inbox",
                "center": "Customer account list, opportunity pipeline and task workbench",
                "right": "Selected customer inspector, contacts, open tasks and activity timeline"
            },
            "category": "Sales",
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "crm",
                "customers",
                "sales",
                "accounts",
                "pipeline"
            ],
            "store": {
                "summary": "Bestandskunden-CRM mit Accounts, Kontakten, Opportunities, Aufgaben, Notizen, Aktivitäten und Outbound-Handoff.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/customers",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "cv-print-builder",
            "title": "CV Print Builder",
            "description": "CV-PDFs strukturiert parsen, korrigieren und in einheitliche Druckansichten mit Template-System freigeben.",
            "entry": "modules/cv-print-builder/index.html",
            "collections": [
                "business_commands",
                "business_chats",
                "ctox_queue_tasks",
                "desktop_files",
                "desktop_file_chunks",
                "documents",
                "document_versions"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-cv-print-builder\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-cv-print-builder\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0f766e\"/><stop offset=\"100%\" stop-color=\"#64748b\"/></linearGradient></defs><path d=\"M6 3h8l4 4v14H6z\" fill=\"url(#grad-cv-print-builder)\" fill-opacity=\"0.12\" stroke=\"url(#grad-cv-print-builder)\" stroke-width=\"2\" stroke-linejoin=\"round\"/><path d=\"M14 3v5h4\" stroke=\"url(#grad-cv-print-builder)\" stroke-width=\"2\" stroke-linejoin=\"round\"/><path d=\"M8.5 12h7M8.5 15h7M8.5 18h4\" stroke=\"url(#grad-cv-print-builder)\" stroke-width=\"1.8\" stroke-linecap=\"round\"/></svg>",
                "left": "Kandidaten-Shards und CV-Import",
                "center": "Original-PDF, Split-Ansicht und Druckansicht"
            },
            "category": "Recruiting",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "cv",
                "pdf",
                "print",
                "parser"
            ],
            "store": {
                "summary": "Business-OS CV-Parser mit PDF-Original, strukturierter Druckansicht, Freigabeprozess und Template-System.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/cv-print-builder",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "focus",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 800
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-documents\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-documents\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#3b82f6\" /><stop offset=\"100%\" stop-color=\"#6366f1\" /></linearGradient></defs><path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z\" fill=\"url(#grad-documents)\" fill-opacity=\"0.12\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M14 2v4a2 2 0 0 0 2 2h4\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><line x1=\"8\" y1=\"12\" x2=\"16\" y2=\"12\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"8\" y1=\"16\" x2=\"16\" y2=\"16\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"8\" y1=\"8\" x2=\"10\" y2=\"8\" stroke=\"url(#grad-documents)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line></svg>",
                "left": "document navigation and explorer",
                "center": "DOCX viewer/editor workbench",
                "right": "document runbooks and automation prompts",
                "drawers": {
                    "left": "document metadata and import settings",
                    "right": "runbook details and generated commands",
                    "bottom": "diagnostics, export evidence, and selected document context"
                }
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "DOCX and Markdown document workspace with document versions, blob chunks, and automation runbooks.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/documents",
                "installable": false,
                "editable_after_install": true,
                "distribution": "starter-module"
            },
            "install_scope": "starter",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "focus",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 800
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Knowledge",
            "tags": [
                "documents",
                "docx",
                "markdown",
                "runbooks"
            ]
        },
        {
            "id": "esign",
            "title": "E-Signatur",
            "description": "Generische Signatur-Anfragen: Dokument an Unterzeichner routen und Status verfolgen.",
            "entry": "modules/esign/index.html",
            "collections": [
                "business_commands",
                "signature_requests"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "left": "Filter",
                "center": "E-Signatur"
            },
            "category": "Operations",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "esign",
                "ats"
            ],
            "store": {
                "summary": "Generische Signatur-Anfragen: Dokument an Unterzeichner routen und Status verfolgen.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/esign",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 960,
                    "height": 680
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "intake",
            "title": "Bewerbungseingang",
            "description": "Generischer Mehrkanal-Eingang: normalisierte Bewerbungen aus Karriereseite/Jobbörse/E-Mail/QR.",
            "entry": "modules/intake/index.html",
            "collections": [
                "business_commands",
                "applications"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "left": "Filter",
                "center": "Bewerbungseingang"
            },
            "category": "Operations",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "intake",
                "ats"
            ],
            "store": {
                "summary": "Generischer Mehrkanal-Eingang: normalisierte Bewerbungen aus Karriereseite/Jobbörse/E-Mail/QR.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/intake",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 960,
                    "height": 680
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "interviews",
            "title": "Interviews",
            "description": "Mehr-Parteien-Interview-Koordination mit strukturierten Scorecards.",
            "entry": "modules/interviews/index.html",
            "collections": [
                "business_commands",
                "interview_scorecards",
                "interview_meetings"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "left": "Filter",
                "center": "Interviews"
            },
            "category": "Recruiting",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "interviews",
                "ats"
            ],
            "store": {
                "summary": "Mehr-Parteien-Interview-Koordination mit strukturierten Scorecards.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/interviews",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 960,
                    "height": 680
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "invoices",
            "title": "Rechnungen",
            "description": "Ausgangs- und Eingangsrechnungen mit Post, Skonto/Allocation, Mahnwesen (Level 1) und XRechnung-2.0-XML-Export. DATEV-Anbindung ueber das Buchhaltung-Modul. Storno, Gutschriften und Recurring sind als Command-Typen erfasst, aber noch nicht implementiert (siehe README 'Out of Scope').",
            "entry": "modules/invoices/index.html",
            "collections": [
                "business_commands",
                "customer_accounts",
                "customer_activities",
                "accounting_accounts",
                "accounting_journal_entries",
                "accounting_journal_entry_lines",
                "accounting_ledger_entries",
                "accounting_receipts",
                "accounting_bank_statement_lines",
                "accounting_number_series",
                "desktop_files",
                "desktop_file_chunks",
                "accounting_invoices",
                "accounting_invoice_lines",
                "accounting_payment_terms",
                "accounting_credit_notes",
                "accounting_payments",
                "accounting_payment_allocations",
                "accounting_dunning_runs",
                "accounting_dunning_letters",
                "accounting_recurring_invoices",
                "accounting_invoice_attachments",
                "accounting_invoice_approvals"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-invoices\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0f766e\" /><stop offset=\"100%\" stop-color=\"#f97316\" /></linearGradient></defs><path d=\"M6 2h9l5 5v13a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z\" fill=\"url(#grad-invoices)\" fill-opacity=\"0.12\" stroke=\"url(#grad-invoices)\" stroke-width=\"2\" stroke-linejoin=\"round\" /><path d=\"M15 2v6h6\" stroke=\"url(#grad-invoices)\" stroke-width=\"2\" stroke-linejoin=\"round\" /><line x1=\"9\" y1=\"12\" x2=\"15\" y2=\"12\" stroke=\"url(#grad-invoices)\" stroke-width=\"1.8\" stroke-linecap=\"round\" /><line x1=\"9\" y1=\"15\" x2=\"15\" y2=\"15\" stroke=\"url(#grad-invoices)\" stroke-width=\"1.8\" stroke-linecap=\"round\" /><line x1=\"9\" y1=\"18\" x2=\"13\" y2=\"18\" stroke=\"url(#grad-invoices)\" stroke-width=\"1.8\" stroke-linecap=\"round\" /></svg>",
                "left": "Rechnungs-Scopes, Status-Chips, Schnellfilter (ueberfaellig, Mahnstufe 1, offene Posten)",
                "center": "Editor und Detail mit Tabs (Stammdaten, Positionen, Steuern, Zahlungen, PDF, Verlauf)",
                "right": "Kunden-Inspector, offene Posten, AI-Aktionen (Mahnlauf, Skonto, Gutschrift)",
                "third_pane_justification": "The financial inspector provides approval, open-item and customer context alongside the invoice editor; compact mode moves it into the shared drawer."
            },
            "category": "Finance",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "invoices",
                "billing",
                "fibu",
                "skonto",
                "dunning",
                "xrechnung"
            ],
            "store": {
                "summary": "Rechnungsstellung mit Lebenszyklus, Gutschriften, Skonto, Mahnwesen, XRechnung und GoBD-Archivierung auf Basis der Buchhaltung.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/invoices",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "store",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1120,
                    "height": 760
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "iot",
            "title": "IoT",
            "description": "CTOX IoT delegation app (RFC 0011): a 2-pane workspace where you task CTOX in free text (Wenn/Dann) to watch your signals and act. LEFT = asset/signal tree (right-click a signal to create an order or a webhook source). CENTER = dashboards of automation widgets; each widget is one standing order CTOX programs in three editable parts — ① Trigger-Logik (a sandboxed Rhai watcher that fires per datapoint in the backend), ② Widget-Code (render_code in a sandboxed iframe), ③ Auftrags-Prompt (spawns a chat on fire). Reads iot_* projections only and writes via business_commands; webhooks in & out. The actual trigger/render code is generated by CTOX (model-driven), never a heuristic template.",
            "entry": "modules/iot/index.html",
            "collections": [
                "business_commands",
                "iot_realms",
                "iot_asset_types",
                "iot_assets",
                "iot_attributes",
                "iot_datapoints",
                "iot_alarms",
                "iot_agents",
                "iot_agent_status",
                "iot_rulesets",
                "iot_dashboards",
                "iot_widgets"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-iot\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-iot\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0f766e\" /><stop offset=\"100%\" stop-color=\"#2563eb\" /></linearGradient></defs><circle cx=\"12\" cy=\"12\" r=\"3\" fill=\"url(#grad-iot)\" fill-opacity=\"0.18\" stroke=\"url(#grad-iot)\" stroke-width=\"2\"></circle><path d=\"M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4\" stroke=\"url(#grad-iot)\" stroke-width=\"2\" stroke-linecap=\"round\"></path><path d=\"M5 5a10 10 0 0 0 0 14M19 5a10 10 0 0 1 0 14\" stroke=\"url(#grad-iot)\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-opacity=\"0.7\"></path></svg>",
                "left": "Realm scope, asset/signal tree; right-click a signal to create an order or a webhook source",
                "center": "Dashboards of automation widgets (the three CTOX-programmed parts: trigger logic, widget code, order prompt), Karten ⇄ Liste",
                "right": ""
            },
            "category": "Operations",
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "iot",
                "sensors",
                "assets",
                "alarms",
                "telemetry"
            ],
            "store": {
                "summary": "Task CTOX in plain text (Wenn/Dann) to watch your signals and act. Each dashboard widget is a standing order CTOX programs in three parts (Rhai watcher · sandboxed render code · order prompt that spawns a chat on fire). Webhooks in & out; MQTT/HTTP/WS ingest; multi-realm isolation; sandboxed generated code.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/iot",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "store",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-knowledge\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-knowledge\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#8b5cf6\" /><stop offset=\"100%\" stop-color=\"#d946ef\" /></linearGradient></defs><path d=\"M4 19.5A2.5 2.5 0 0 1 6.5 17H20\" stroke=\"url(#grad-knowledge)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z\" fill=\"url(#grad-knowledge)\" fill-opacity=\"0.12\" stroke=\"url(#grad-knowledge)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M12 2v10l2.5-2 2.5 2V2z\" fill=\"url(#grad-knowledge)\" fill-opacity=\"0.25\" stroke=\"url(#grad-knowledge)\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"9\" cy=\"12\" r=\"1.5\" fill=\"url(#grad-knowledge)\"></circle><circle cx=\"14\" cy=\"15\" r=\"1\" fill=\"url(#grad-knowledge)\"></circle></svg>",
                "left": "Knowledge selection and source groups",
                "center": "Markdown reader/editor and dataframe table tabs",
                "right": "Runbooks as operational knowledge layer",
                "drawers": {
                    "left": "Knowledge source and import configuration",
                    "right": "Runbook configuration, modification, and execution",
                    "bottom": "Selected rows, dataframe diagnostics, and CTOX task evidence"
                },
                "default_width": 1180,
                "default_height": 780,
                "min_width": 640,
                "min_height": 480
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Knowledge workspace for operational runbooks, markdown assets, skills, and structured data tables.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/knowledge",
                "installable": false,
                "editable_after_install": true,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Knowledge",
            "tags": [
                "knowledge-base",
                "runbooks",
                "dataframes",
                "skills"
            ]
        },
        {
            "id": "matching",
            "title": "Matching",
            "description": "Generic CTOX matching workspace for configurable source, match, and object columns.",
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-matching\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-matching\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#f59e0b\" /><stop offset=\"100%\" stop-color=\"#ea580c\" /></linearGradient></defs><path d=\"M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71\" fill=\"url(#grad-matching)\" fill-opacity=\"0.12\" stroke=\"url(#grad-matching)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71\" fill=\"url(#grad-matching)\" fill-opacity=\"0.12\" stroke=\"url(#grad-matching)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"12\" cy=\"12\" r=\"2.5\" fill=\"#ffffff\" stroke=\"url(#grad-matching)\" stroke-width=\"1\"></circle></svg>",
                "left": "Requirement/source records and import task prompts",
                "center": "Configured matching tasks, queue state, and match results",
                "right": "Object pool records and import task prompts",
                "drawers": {
                    "left": "Column import and source configuration",
                    "right": "Object pool configuration and selected record detail",
                    "bottom": "Matching prompt, schema, task status, and evidence"
                }
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Configurable matching workspace for requirements, object pools, scoring runs, and result review.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/matching",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1280,
                    "height": 820
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Operations",
            "tags": [
                "matching",
                "scoring",
                "imports",
                "workflow"
            ]
        },
        {
            "id": "nachweise",
            "title": "Nachweise",
            "description": "Generischer Nachweis-/Zertifikats-Tresor: ablaufende, verifizierte Artefakte (Zertifikate, Lizenzen, Arbeitserlaubnis) je Subjekt mit Ablauf-Warnung und Einsatz-Gate.",
            "entry": "modules/nachweise/index.html",
            "collections": [
                "business_commands",
                "business_credentials"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "left": "Subjekt- und Statusfilter",
                "center": "Nachweise mit Ablauf und Verifikationsstatus"
            },
            "category": "Operations",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "credentials",
                "expiry",
                "compliance",
                "vault"
            ],
            "store": {
                "summary": "Generischer Nachweis-/Ablauf-Tresor mit Einsatz-Gate.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/nachweise",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 960,
                    "height": 680
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "notes",
            "title": "Notes",
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-notes\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-notes\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#eab308\" /><stop offset=\"100%\" stop-color=\"#d97706\" /></linearGradient></defs><path d=\"M16 2H4a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2z\" fill=\"url(#grad-notes)\" fill-opacity=\"0.12\" stroke=\"url(#grad-notes)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><path d=\"M2 6h2M2 10h2M2 14h2M2 18h2\" stroke=\"url(#grad-notes)\" stroke-width=\"1.5\" stroke-linecap=\"round\"></path><path d=\"M18.5 2.5a2.121 2.121 0 0 1 3 3L11 16l-4 1 1-4 10.5-10.5z\" fill=\"url(#grad-notes)\" fill-opacity=\"0.3\" stroke=\"url(#grad-notes)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path></svg>",
                "left": "Folders and note list",
                "center": "Markdown editor and rich text live preview",
                "right": "Command dashboard and formatting shortcuts"
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Local-first markdown notes workspace with folder navigation and live preview.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/notes",
                "installable": false,
                "editable_after_install": true,
                "distribution": "starter-module"
            },
            "install_scope": "starter",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "focus",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1120,
                    "height": 760
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Productivity",
            "tags": [
                "notes",
                "markdown",
                "local-first",
                "writing"
            ]
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
                "outbound_research_adapters",
                "outbound_engagements",
                "outbound_messages",
                "outbound_approvals",
                "outbound_sequences",
                "outbound_sender_assignments",
                "outbound_meeting_requests",
                "outbound_suppression_entries",
                "outbound_account_limits",
                "outbound_skillbooks",
                "outbound_letter_templates"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-outbound\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-outbound\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#ec4899\" /><stop offset=\"100%\" stop-color=\"#f43f5e\" /></linearGradient></defs><line x1=\"22\" y1=\"2\" x2=\"11\" y2=\"13\" stroke=\"url(#grad-outbound)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><polygon points=\"22 2 15 22 11 13 2 9 22 2\" fill=\"url(#grad-outbound)\" fill-opacity=\"0.12\" stroke=\"url(#grad-outbound)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></polygon><path d=\"M6 19c3-1 6-1 9-3\" stroke=\"url(#grad-outbound)\" stroke-width=\"1.5\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></path></svg>",
                "left": "campaign selection and source import",
                "center": "company qualification and pipeline workbench"
            },
            "version": "0.1.0",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Campaign sourcing, company qualification, research handoff, and pipeline preparation.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/outbound",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "starter",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Sales",
            "tags": [
                "sales",
                "outbound",
                "research",
                "pipeline"
            ]
        },
        {
            "id": "placements",
            "title": "Vermittlungen",
            "description": "Angebots-/Vermittlungs-Lifecycle mit Garantie-Uhr und Honorar.",
            "entry": "modules/placements/index.html",
            "collections": [
                "business_commands",
                "offers",
                "placements"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "left": "Filter",
                "center": "Vermittlungen"
            },
            "category": "Recruiting",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "placements",
                "ats"
            ],
            "store": {
                "summary": "Angebots-/Vermittlungs-Lifecycle mit Garantie-Uhr und Honorar.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/placements",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 960,
                    "height": 680
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-reports\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-reports\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#ef4444\" /><stop offset=\"100%\" stop-color=\"#f97316\" /></linearGradient></defs><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\" fill=\"url(#grad-reports)\" fill-opacity=\"0.12\" stroke=\"url(#grad-reports)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><path d=\"M18 17V10M12 17V6M6 17v-4\" stroke=\"url(#grad-reports)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"12\" cy=\"6\" r=\"2\" fill=\"#ffffff\" stroke=\"url(#grad-reports)\" stroke-width=\"1.2\"></circle></svg>",
                "left": "Bug and feature filters and history",
                "center": "Selected report evidence, CTOX change log, and rollback",
                "default_width": 1120,
                "default_height": 760,
                "min_width": 640,
                "min_height": 480
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Bug and feature report tracker with CTOX acceptance, evidence, release, and rollback state.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/reports",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1120,
                    "height": 760
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Governance",
            "tags": [
                "bugs",
                "features",
                "reports",
                "rollback"
            ]
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
                "research_notes",
                "knowledge_tables",
                "documents",
                "document_versions",
                "document_blob_chunks"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-research\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-research\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0891b2\" /><stop offset=\"100%\" stop-color=\"#10b981\" /></linearGradient></defs><path d=\"M6 3h12\" stroke=\"url(#grad-research)\" stroke-width=\"2\" stroke-linecap=\"round\"></path><path d=\"M8 3v4c0 1.66-1.34 3-3 3v0a7 7 0 0 0-2 4.9V20a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5.1a7 7 0 0 0-2-4.9v0c-1.66 0-3-1.34-3-3V3\" fill=\"url(#grad-research)\" fill-opacity=\"0.12\" stroke=\"url(#grad-research)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><line x1=\"8.5\" y1=\"11\" x2=\"15.5\" y2=\"11\" stroke=\"url(#grad-research)\" stroke-width=\"2\"></line><circle cx=\"12\" cy=\"16\" r=\"2.5\" fill=\"url(#grad-research)\"></circle><circle cx=\"9\" cy=\"18\" r=\"1\" fill=\"#ffffff\"></circle><circle cx=\"15\" cy=\"15\" r=\"1\" fill=\"#ffffff\"></circle></svg>",
                "left": "research tasks and scored source ranking",
                "center": "portfolio map and source evidence workbench",
                "right": "research task context, decisions, and CTOX handoff",
                "third_pane_justification": "The task and evidence inspector remains visible beside source ranking and the portfolio workbench in wide mode; compact mode uses the shared right drawer.",
                "drawers": {
                    "right": "task setup, scoring model, and selected source detail",
                    "bottom": "Knowledge table diagnostics and raw row evidence"
                }
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Knowledge-backed web research dashboards with source scoring and CTOX systematic-research handoff.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/research",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "store",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1280,
                    "height": 820
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Research",
            "tags": [
                "research",
                "sources",
                "scoring",
                "knowledge"
            ]
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-shiftflow\"><defs><linearGradient id=\"grad-shiftflow\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#8b5cf6\" /><stop offset=\"100%\" stop-color=\"#7c3aed\" /></linearGradient></defs><rect x=\"3\" y=\"4\" width=\"18\" height=\"16\" rx=\"3\" fill=\"url(#grad-shiftflow)\" fill-opacity=\"0.12\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><line x1=\"3\" y1=\"9\" x2=\"21\" y2=\"9\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"2\" stroke-linecap=\"round\"></line><line x1=\"9\" y1=\"9\" x2=\"9\" y2=\"20\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></line><line x1=\"15\" y1=\"9\" x2=\"15\" y2=\"20\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1\" stroke-dasharray=\"2 2\" stroke-linecap=\"round\"></line><rect x=\"5\" y=\"12\" width=\"8\" height=\"4\" rx=\"1.5\" fill=\"url(#grad-shiftflow)\" fill-opacity=\"0.3\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1\"></rect><circle cx=\"17\" cy=\"15\" r=\"2.5\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1.2\"></circle><polyline points=\"17 13.5 17 15 18 15\" stroke=\"url(#grad-shiftflow)\" stroke-width=\"1\" stroke-linecap=\"round\"></polyline></svg>",
                "left": "team status, absence scopes and department selection",
                "center": "interactive scheduler timeline and timesheet grid",
                "right": "AI roster planner, conflict alerts and timesheet inspector",
                "drawers": {
                    "left": "Arbeitszeitmodelle und Einstellungen",
                    "right": "Dienstplaner und Detail-Inspektor",
                    "bottom": "Schnelleingabe & Massenaktionen"
                }
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "Team scheduling, time records, absence planning, and AI-assisted roster conflict handling.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/shiftflow",
                "installable": true,
                "editable_after_install": true,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Operations",
            "tags": [
                "planning",
                "scheduling",
                "time-tracking",
                "absences"
            ]
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
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-spreadsheets\"><defs><linearGradient id=\"grad-spreadsheets\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#10b981\" /><stop offset=\"100%\" stop-color=\"#059669\" /></linearGradient></defs><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\" fill=\"url(#grad-spreadsheets)\" fill-opacity=\"0.12\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></rect><line x1=\"9\" y1=\"3\" x2=\"9\" y2=\"21\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"3\" y1=\"9\" x2=\"21\" y2=\"9\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><line x1=\"3\" y1=\"15\" x2=\"21\" y2=\"15\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></line><path d=\"M5 17l3-3 4 2 4-4\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path><circle cx=\"16\" cy=\"12\" r=\"1.5\" fill=\"#ffffff\" stroke=\"url(#grad-spreadsheets)\" stroke-width=\"1\"></circle></svg>",
                "left": "spreadsheet navigation and explorer",
                "center": "Spreadsheet viewer/editor workbench",
                "right": "spreadsheet runbooks and automation prompts",
                "drawers": {
                    "left": "spreadsheet metadata and import settings",
                    "right": "runbook details and generated commands",
                    "bottom": "diagnostics, export evidence, and selected spreadsheet context"
                }
            },
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "store": {
                "summary": "XLSX spreadsheet workspace with explorer, JSpreadsheet editor, versions, and automation runbooks.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/spreadsheets",
                "installable": false,
                "editable_after_install": true,
                "distribution": "starter-module"
            },
            "install_scope": "starter",
            "default_installed": true,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "focus",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1280,
                    "height": 820
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            },
            "category": "Analytics",
            "tags": [
                "spreadsheets",
                "xlsx",
                "tables",
                "runbooks"
            ]
        },
        {
            "id": "submissions",
            "title": "Vorstellungen",
            "description": "Kandidaten-Vorstellung an Kunden mit Doppel-Vorstellungs- und Consent-Schutz.",
            "entry": "modules/submissions/index.html",
            "collections": [
                "business_commands",
                "submissions"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "left": "Filter",
                "center": "Vorstellungen"
            },
            "category": "Recruiting",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "submissions",
                "ats"
            ],
            "store": {
                "summary": "Kandidaten-Vorstellung an Kunden mit Doppel-Vorstellungs- und Consent-Schutz.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/submissions",
                "installable": true,
                "editable_after_install": true,
                "distribution": "catalog-module"
            },
            "install_scope": "starter",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 960,
                    "height": 680
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "support",
            "title": "Support",
            "description": "Native CTOX Support Desk for omnichannel support queues, customer context, ticket links, SLA, macros, and CTOX Harness-assisted drafts.",
            "entry": "modules/support/index.html",
            "collections": [
                "business_commands",
                "business_chats",
                "ctox_queue_tasks",
                "communication_threads",
                "communication_messages",
                "ctox_ticket_cases",
                "customer_accounts",
                "customer_contacts",
                "desktop_files",
                "desktop_file_chunks",
                "support_inboxes",
                "support_conversations",
                "support_thread_links",
                "support_identity_links",
                "support_notes",
                "support_conversation_events",
                "support_labels",
                "support_label_assignments",
                "support_views",
                "support_view_filters",
                "support_assignment_policies",
                "support_assignment_events",
                "support_macros",
                "support_automation_rules",
                "support_sla_policies",
                "support_applied_slas",
                "support_sla_events",
                "support_agent_requests",
                "support_agent_suggestions",
                "support_reporting_events",
                "support_reporting_rollups"
            ],
            "source": "local",
            "core": false,
            "editable": true,
            "deletable": true,
            "layout": {
                "shell": "windowed",
                "left": "Support queue, saved views, and channel filters",
                "center": "Selected support conversation timeline and composer",
                "right": "Customer context, ticket links, SLA, macros, and CTOX Agent suggestions",
                "third_pane_justification": "Support operators need customer identity, linked tickets, SLA state, and CTOX suggestions visible while reading and replying; moving this context into drawers would force repeated context switches during every conversation."
            },
            "category": "Operations",
            "version": "0.1.0",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "support",
                "inbox",
                "customers",
                "tickets",
                "agent"
            ],
            "store": {
                "summary": "CTOX-native support desk layered over communication, ticket, customer, and harness projections.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/support",
                "installable": true,
                "editable_after_install": false,
                "distribution": "ctox-repo-module"
            },
            "install_scope": "store",
            "default_installed": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "threads",
            "title": "Threads",
            "description": "User-focused Business OS hub for app-linked notes, mentions, handoffs, and CTOX approval requests across durable work lifecycle records.",
            "entry": "modules/threads/index.html",
            "collections": [
                "business_commands",
                "ctox_queue_tasks",
                "user_threads",
                "user_thread_messages",
                "user_thread_links",
                "user_notifications",
                "ctox_task_approval_requests"
            ],
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-threads\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-threads\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0f766e\" /><stop offset=\"100%\" stop-color=\"#7c3aed\" /></linearGradient></defs><path d=\"M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v7A2.5 2.5 0 0 1 17.5 15H10l-5 4v-4.2A2.5 2.5 0 0 1 4 12.5z\" fill=\"url(#grad-threads)\" fill-opacity=\"0.12\" stroke=\"url(#grad-threads)\" stroke-width=\"2\" stroke-linejoin=\"round\"></path><path d=\"M8 8h8M8 11h5\" stroke=\"url(#grad-threads)\" stroke-width=\"2\" stroke-linecap=\"round\"></path><circle cx=\"18\" cy=\"18\" r=\"3\" fill=\"#fff\" stroke=\"url(#grad-threads)\" stroke-width=\"1.5\"></circle><path d=\"M18 16.6v1.7l1.2.8\" stroke=\"url(#grad-threads)\" stroke-width=\"1.4\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></path></svg>",
                "left": "personal inbox, approvals, and source filters",
                "center": "durable thread timeline tied to app records",
                "right": "new notes, CTOX approval requests, and lifecycle context",
                "default_width": 1120,
                "default_height": 760,
                "min_width": 640,
                "min_height": 480
            },
            "category": "System",
            "version": "v0.1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "threads",
                "mentions",
                "approvals",
                "handoff",
                "ctox"
            ],
            "store": {
                "summary": "Business OS user-space hub for lifecycle-aware collaboration and CTOX approval workflows.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/threads",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "source": "core",
            "core": true,
            "editable": true,
            "deletable": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1120,
                    "height": 760
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        },
        {
            "id": "tickets",
            "title": "Tickets",
            "description": "Native CTOX ticket operations surface for synchronized tickets, routed cases, self-work, approvals, verification, and writeback evidence.",
            "entry": "modules/tickets/index.html",
            "collections": [
                "ctox_ticket_items",
                "ctox_ticket_events",
                "ctox_ticket_event_routing_state",
                "ctox_ticket_cases",
                "ctox_ticket_self_work_items",
                "ctox_ticket_self_work_notes",
                "ctox_ticket_label_assignments",
                "ctox_ticket_control_bundles",
                "ctox_ticket_approvals",
                "ctox_ticket_verifications",
                "ctox_ticket_writebacks",
                "ctox_ticket_clarification_requests"
            ],
            "layout": {
                "shell": "windowed",
                "icon_svg": "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"svg-icon svg-tickets\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad-tickets\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\"><stop offset=\"0%\" stop-color=\"#0f766e\" /><stop offset=\"100%\" stop-color=\"#2563eb\" /></linearGradient></defs><path d=\"M4 5a2 2 0 0 1 2-2h9l5 5v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z\" fill=\"url(#grad-tickets)\" fill-opacity=\"0.12\" stroke=\"url(#grad-tickets)\" stroke-width=\"2\" stroke-linejoin=\"round\"></path><path d=\"M15 3v5h5\" stroke=\"url(#grad-tickets)\" stroke-width=\"2\" stroke-linejoin=\"round\"></path><path d=\"M8 12h8M8 16h6\" stroke=\"url(#grad-tickets)\" stroke-width=\"2\" stroke-linecap=\"round\"></path></svg>",
                "left": "ticket inbox, source and state filters",
                "center": "selected ticket timeline and case evidence",
                "right": "case controls, self-work, approval, verification and writeback context",
                "default_width": 1180,
                "default_height": 780,
                "min_width": 640,
                "min_height": 480
            },
            "category": "Operations",
            "version": "v1",
            "developer": "CTOX",
            "license": "AGPL-3.0-only",
            "tags": [
                "tickets",
                "cases",
                "approvals",
                "support"
            ],
            "store": {
                "summary": "Read-only CTOX ticket operations app over native RxDB/WebRTC ticket projections.",
                "repository": "metric-space-ai/ctox",
                "source_path": "modules/tickets",
                "installable": false,
                "editable_after_install": false,
                "distribution": "system-module"
            },
            "install_scope": "core",
            "default_installed": true,
            "source": "core",
            "core": true,
            "editable": true,
            "deletable": false,
            "launch_kind": "desktop-app",
            "presentation": {
                "default_mode": "window",
                "supported_modes": [
                    "window",
                    "maximized",
                    "focus"
                ],
                "initial_size": {
                    "width": 1180,
                    "height": 780
                },
                "minimum_size": {
                    "width": 640,
                    "height": 480
                },
                "multi_instance": false,
                "auto_restore": false
            }
        }
    ],
    "id": "module-catalog",
    "updated_at_ms": Date.now(),
    "templates": [],
    "governance": null,
    "source": "business-os-shell-embedded-catalog"
};
}

async function loadPackagedModuleCatalog() {
  try {
    const response = await fetch(`modules/registry.json?v=${APP_BUILD}`, { cache: 'force-cache' });
    if (response.ok) {
      const catalog = await response.json();
      if (Array.isArray(catalog?.modules) && catalog.modules.length) {
        return {
          id: 'module-catalog',
          updated_at_ms: Date.now(),
          ok: catalog.ok !== false,
          modules: await withPackagedModuleAssetRevisions(catalog.modules),
          templates: Array.isArray(catalog.templates) ? catalog.templates : [],
          governance: catalog.governance || null,
          source: 'business-os-shell',
        };
      }
    }
  } catch (error) {
    console.warn('[business-os] packaged module catalog seed unavailable; using embedded shell catalog', error);
  }
  const fallback = getOfflineFallbackCatalog();
  return {
    ...fallback,
    modules: await withPackagedModuleAssetRevisions(fallback.modules),
  };
}

async function withPackagedModuleAssetRevisions(modules) {
  if (!Array.isArray(modules) || !modules.length) return modules;
  return Promise.all(modules.map(async (mod) => {
    const existing = String(mod?.asset_revision || mod?.assetRevision || '').trim();
    if (existing) return mod;
    const revision = await packagedModuleAssetRevision(mod);
    return revision ? { ...mod, asset_revision: revision } : mod;
  }));
}

async function packagedModuleAssetRevision(mod) {
  const base = moduleBasePath(mod);
  if (!base || !base.startsWith('modules/')) return '';
  const cacheKey = `${base}:${APP_BUILD}`;
  if (state.packagedModuleAssetRevisions.has(cacheKey)) {
    return state.packagedModuleAssetRevisions.get(cacheKey);
  }
  const assets = ['module.json', 'index.js', 'schema.js', 'index.css', 'icon.svg'];
  const parts = [];
  for (const asset of assets) {
    try {
      const response = await fetch(`./${base}/${asset}?v=${APP_BUILD}`, { cache: 'no-store' });
      if (!response.ok) continue;
      const content = await response.text();
      parts.push(`${asset}\0${content.length}\0${content}`);
    } catch {
      // Optional module assets are expected to be absent for many apps.
    }
  }
  if (!parts.length || !crypto?.subtle) {
    state.packagedModuleAssetRevisions.set(cacheKey, '');
    return '';
  }
  const bytes = new TextEncoder().encode(parts.join('\n'));
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  const revision = Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
  state.packagedModuleAssetRevisions.set(cacheKey, revision);
  return revision;
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
    throw new Error('Aktionen sind gerade nicht verfügbar.');
  }
  const generation = state.dataPlaneGeneration;
  const db = state.db;
  const commandBridge = await state.sync?.startCollection?.('business_commands');
  await waitForSyncBridgeReady(commandBridge, 15000);
  if (isStaleDataPlaneGeneration(generation)) {
    throw createRecoverableDataPlaneAbort('Business OS data plane was rebuilt before command dispatch.');
  }
  const commandId = `cmd_${newId()}`;
  return state.commandBus.dispatch({
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
  }, { until: 'accepted' });
}

async function waitForSyncBridgeReady(bridge, timeoutMs = 15000) {
  const state = bridge?.state;
  if (!state) return;
  let timer = null;
  try {
    await Promise.race([
      Promise.resolve()
        .then(() => state.awaitInSync?.() || state.awaitInitialReplication?.())
        .catch(() => {}),
      new Promise((resolve) => {
        timer = setTimeout(resolve, timeoutMs);
        timer?.unref?.();
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
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
  return error?.code === 'CTOX_DATA_PLANE_REBUILT'
    || isClosedRxDbCollectionError(error)
    || isVolatileSyncTransportError(error);
}

function isClosedRxDbCollectionError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: COL21')
    || message.includes('collection is closed')
    || message.includes('closed collection')
    || /IDBDatabase.*closing|database connection is closing/i.test(message);
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
    email: user.email || '',
    login: user.login || '',
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
  if (launchConfigForPageSession) return launchConfigForPageSession;

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
  if (config) {
    launchConfigForPageSession = config;
  }
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
    const raw = readScopedLocalStorage(PAIRING_CONFIG_KEY, {
      actor: false,
      legacyFallback: true,
    });
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function writeStoredPairingConfig(config) {
  try {
    writeScopedLocalStorage(PAIRING_CONFIG_KEY, JSON.stringify({ ...config, source: 'stored' }), {
      actor: false,
    });
  } catch {}
}

function clearStoredPairingConfig() {
  try {
    removeScopedLocalStorage(PAIRING_CONFIG_KEY, { actor: false });
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

function workspaceStatusText() {
  const brandingName = state.workspaceBranding?.custom === true
    ? String(state.workspaceBranding?.name || '').trim()
    : '';
  if (brandingName) return brandingName;
  const instanceName = getInstanceName();
  if (instanceName && instanceName !== 'A6000' && !isLocalBusinessOsSurface()) {
    return instanceName;
  }
  return shellText('localWorkspace');
}

function setWorkspaceStatus() {
  setStatus(workspaceStatusText());
}

function isWorkspaceStatusText(text) {
  const value = String(text || '').trim();
  if (!value) return true;
  if (value === shellMessages.de.localWorkspace || value === shellMessages.en.localWorkspace) return true;
  const brandingName = String(state.workspaceBranding?.name || '').trim();
  return Boolean(brandingName && value === brandingName);
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
        const remainingCap = (endVal + 4) - currentProgress;
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
    description = 'Der lokale Browsercache hat nicht rechtzeitig geantwortet.';
    advice = 'Die Anwendung versucht automatisch einen frischen lokalen Cache zu öffnen und neu zu synchronisieren. Falls diese Meldung erneut erscheint, bitte die Seite neu laden; die technischen Details nennen den konkreten Timeout.';
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
  } else if (msg.includes('Failed to fetch dynamically imported module') || msg.includes('ctox-rxdb-js.mjs') || msg.includes('RxDB bundle import')) {
    title = 'Business-OS-Dateien konnten nicht geladen werden';
    description = 'Ein benötigtes CTOX-DB-Bundle fehlt, ist veraltet oder wurde vom Cache mit einer alten Version geladen.';
    advice = 'Bitte versuchen Sie es erneut. Wenn die Meldung bleibt, muss die Instanz mit den aktuellen Business-OS-Assets synchronisiert werden.';
  } else if (msg.includes('NetworkError') || msg.includes('Failed to fetch') || msg.includes('signaling')) {
    title = 'Netzwerkverbindung fehlgeschlagen';
    description = 'Eine Netzwerk- oder WebRTC-Verbindung konnte nicht aufgebaut werden.';
    advice = 'Bitte versuchen Sie es erneut. Wenn die Meldung bleibt, prüfen Sie die Instanzdienste und die Erreichbarkeit des Signaling-Servers.';
  }

  return { title, description, advice };
}

function isLocalRxDbStartupError(error) {
  const msg = String(error?.message || error || '');
  return msg.includes('IndexedDB lock')
    || msg.includes('IndexedDB open blocked')
    || msg.includes('RxDB database creation timed out')
    || msg.includes('RxDB database retry timed out')
    || msg.includes('RxDB createRxDatabase timed out')
    || msg.includes('RxDB database reset timed out');
}

async function resetLocalRxDbBeforeStartupRetry(error) {
  if (!isLocalRxDbStartupError(error)) return false;
  setStatus('Lokale RxDB wird neu synchronisiert');
  try { sessionStorage.removeItem(RXDB_SCHEMA_REPAIR_KEY); } catch {}
  try { await state.sync?.stop?.(); } catch (stopError) { console.warn('[business-os] sync stop before startup retry reset failed', stopError); }
  try { await state.db?.close?.(); } catch (closeError) { console.warn('[business-os] db close before startup retry reset failed', closeError); }
  if (state.workspaceBrandingSubscription) {
    try { state.workspaceBrandingSubscription.unsubscribe(); } catch (error) {}
    state.workspaceBrandingSubscription = null;
  }
  state.workspaceBranding = applyWorkspaceBranding(null);
  state.sync = null;
  state.db = null;
  try {
    const { resetBusinessDb } = await loadBusinessDbModule();
    await resetBusinessDb({ name: businessDbName() });
    return true;
  } catch (resetError) {
    console.warn('[business-os] local RxDB startup retry reset failed', resetError);
    return false;
  }
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
    errorMsgBlock.textContent = formatStartupErrorDetails(error, errMsg);
  }

  const errorCard = document.getElementById('startup-error-card');
  if (errorCard) {
    errorCard.removeAttribute('hidden');
  }

  const retryBtn = document.getElementById('startup-retry-btn');
  if (retryBtn) {
    retryBtn.onclick = async () => {
      retryBtn.disabled = true;
      retryBtn.textContent = isLocalRxDbStartupError(error)
        ? 'Lokale RxDB wird neu synchronisiert...'
        : 'Wird neu geladen...';
      await resetLocalRxDbBeforeStartupRetry(error);
      window.location.reload();
    };
  }
}
window.showStartupError = showStartupError;

function formatStartupErrorDetails(error, errMsg = '') {
  const message = String(error?.message || errMsg || error || 'Unbekannter Fehler');
  const stack = String(error?.stack || '').trim();
  if (!stack || stack === message) return message;
  if (stack.startsWith(message)) return stack;
  return `${message}\n\n${stack}`;
}

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

function renderStartMenuLifecycleBadge(target) {
  if (target?.kind !== 'module' || !target.module) return '';
  const lifecycle = appLifecycleBadge(target.module, {
    session: state.session,
    governance: state.governance,
  });
  if (!lifecycle?.runtimeInstalled) return '';
  const title = target.title || target.id;
  const updateDot = lifecycle.updateAvailable
    ? `<i class="start-menu-update-dot" aria-hidden="true"></i>`
    : '';
  return `
    <button
      class="start-menu-lifecycle-badge${lifecycle.updateAvailable ? ' has-update' : ''}"
      type="button"
      data-module-lifecycle="${escapeHtml(target.id)}"
      data-state="${escapeHtml(lifecycle.state)}"
      title="${escapeHtml(lifecycle.title)}"
      aria-label="${escapeHtml(lifecycleBadgeAriaLabel(title, lifecycle))}"
    >
      ${updateDot}
      ${lifecycle.version ? `<b>${escapeHtml(lifecycle.version)}</b>` : ''}
      <span>${escapeHtml(lifecycle.text || lifecycle.label || '')}</span>
    </button>
  `;
}

function buildStartMenuItem(target) {
  const el = document.createElement('div');
  el.className = 'start-menu-item';

  const pinned = isTaskbarPinned(target.id);
  const iconMarkup = getLauncherIconSvg(target);
  const lifecycleBadge = renderStartMenuLifecycleBadge(target);

  el.innerHTML = `
    <div class="start-menu-item-left">
      <div class="start-menu-item-icon">
        ${iconMarkup}
      </div>
      <div class="start-menu-item-copy">
        <span class="start-menu-item-label">${escapeHtml(target.title || target.id)}</span>
        ${lifecycleBadge ? `<span class="start-menu-item-meta">${lifecycleBadge}</span>` : ''}
      </div>
    </div>
    <button class="start-menu-item-pin-btn ${pinned ? 'is-pinned' : ''}" type="button" title="${pinned ? (shellLang() === 'de' ? 'Von Bar lösen' : 'Unpin') : (shellLang() === 'de' ? 'An Bar anheften' : 'Pin')}">
      ${pinned ? '−' : '+'}
    </button>
  `;

  // Clicks
  el.addEventListener('click', (e) => {
    if (e.target.closest('.start-menu-item-pin-btn') || e.target.closest('[data-module-lifecycle]')) return;
    openLaunchTarget(target);
    hideStartMenu();
  });

  el.querySelector('[data-module-lifecycle]')?.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (target.kind === 'module') openAppLifecycleDrawer(target.module);
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
      return;
    }
    if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
      const target = document.activeElement;
      if (!target || !isGlobalCtoxContextSurface(target) || isCtoxContextMenuBypassTarget(target)) return;
      e.preventDefault();
      e.stopPropagation();
      const rect = target.getBoundingClientRect?.() || { left: 8, bottom: 8, width: 0 };
      openGlobalCtoxContextMenuForTarget(target, rect.left + Math.min(rect.width / 2, 24), rect.bottom + 4);
    }
  });

  // Global capture phase listener
  document.addEventListener('contextmenu', handleGlobalContextMenu, true);
}

function handleGlobalContextMenu(event) {
  const target = event.target?.nodeType === Node.ELEMENT_NODE
    ? event.target
    : event.target?.parentElement;

  if (!target || !isGlobalCtoxContextSurface(target) || isCtoxContextMenuBypassTarget(target)) {
    return;
  }

  // Intercept the click!
  event.preventDefault();
  event.stopPropagation();
  event.stopImmediatePropagation?.();

  openGlobalCtoxContextMenuForTarget(target, event.clientX, event.clientY);
}

function openGlobalCtoxContextMenuForTarget(target, clientX, clientY) {
  state.contextMenu?.hide?.();
  removeLegacyCtoxContextMenus();
  const moduleId = target.closest('[data-module-root]')?.dataset?.moduleRoot || state.activeModule?.id;
  const mod = state.modules.find((item) => item.id === moduleId) || state.activeModule;
  if (!mod) return;
  const context = extractGlobalCtoxContext(mod, target, {
    clientX,
    clientY,
  });
  showGlobalCtoxContextMenu(context, clientX, clientY);
}

function isGlobalCtoxContextSurface(target) {
  if (!target?.closest) return false;
  if (target.closest([
    '.ctox-global-context-menu',
    '.shell-context-menu',
    '[data-ctox-chat-root]',
    '[data-ctox-local-context-menu]',
    '[data-shell-taskbar]',
    '.shell-taskbar',
    '.topbar',
    '.module-nav',
    '.drawer',
    '.drawer-backdrop',
    '[data-backdrop]'
  ].join(', '))) {
    return false;
  }

  return Boolean(target.closest([
    '[data-module-host]',
    '[data-module-content]',
    '[data-module-root]',
    '[data-left-content]',
    '[data-right-content]'
  ].join(', ')));
}

function isCtoxContextMenuBypassTarget(target) {
  if (!target?.closest) return false;
  return Boolean(target.closest([
    '[contenteditable="true"]',
    '.monaco-editor',
    '.no-ctox-context'
  ].join(', ')));
}

function extractGlobalCtoxContext(mod, target, pointer = {}) {
  const registration = registeredContextActionTarget(target);
  const descriptor = registration?.descriptor || {};
  const column = detectColumnFromElement(mod?.id, target);
  const record = detectRecordFromElement(mod?.id, target);
  const selectedText = String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000);
  const clickedText = String(target?.innerText || target?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500);

  const moduleId = mod?.id || '';
  const windowEl = target?.closest?.('.shell-window');
  const registeredEntity = descriptor.entity || {};
  const registeredSelection = typeof descriptor.selection === 'function'
    ? descriptor.selection()
    : (descriptor.selection || {});
  const entity = {
    collection: registeredEntity.collection
      || target?.closest?.('[data-context-collection]')?.getAttribute('data-context-collection')
      || '',
    type: registeredEntity.type || record?.type || 'module',
    id: registeredEntity.id || record?.id || '',
    label: registeredEntity.label || record?.label || mod?.title || mod?.id || '',
  };
  const fieldPath = descriptor.field?.path
    || target?.closest?.('[data-context-field]')?.getAttribute('data-context-field')
    || '';
  const selectionIds = Array.isArray(registeredSelection.ids)
    ? registeredSelection.ids.map((id) => String(id)).filter(Boolean)
    : (entity.id ? [entity.id] : []);
  const paneId = descriptor.pane || column;
  const surfaceId = descriptor.surface
    || target?.closest?.('[data-context-surface]')?.getAttribute('data-context-surface')
    || (windowEl ? 'window-content' : 'workspace');
  const windowInstanceId = windowEl?.dataset?.ownerId || windowEl?.id || `${moduleId}:workspace`;
  const presentationMode = windowEl?.dataset?.appMode || (windowEl ? 'window' : 'workspace');
  const legacy = {
    module: moduleId,
    column: paneId,
    record_type: entity.type,
    record_id: entity.id,
    label: entity.label,
    deep_link: buildGlobalCtoxContextDeepLink(moduleId, entity),
    selected_text: selectedText,
    clicked_text: clickedText,
  };
  return {
    ...legacy,
    context_v2: {
      schema_version: 'business-os-context-v2',
      schema_version_number: 2,
      app_id: moduleId,
      module_id: moduleId,
      window_instance_id: windowInstanceId,
      surface_id: surfaceId,
      pane_id: paneId,
      presentation_mode: presentationMode,
      entity,
      field: { path: fieldPath },
      selection: {
        ids: selectionIds,
        text: registeredSelection.text || selectedText,
        selected_text: selectedText,
        clicked_text: clickedText,
      },
      pointer: {
        x: Number.isFinite(pointer.clientX) ? pointer.clientX : null,
        y: Number.isFinite(pointer.clientY) ? pointer.clientY : null,
        client_x: Number.isFinite(pointer.clientX) ? pointer.clientX : null,
        client_y: Number.isFinite(pointer.clientY) ? pointer.clientY : null,
      },
      deep_link: legacy.deep_link,
      surface: {
        kind: windowEl ? 'window' : 'workspace',
        window_id: windowEl?.id || '',
        owner_id: windowEl?.dataset?.ownerId || '',
      },
      location: {
        column: paneId,
        record: {
          type: legacy.record_type,
          id: legacy.record_id,
          label: legacy.label,
        },
        field: fieldPath,
        deep_link: legacy.deep_link,
      },
    },
  };
}

function buildGlobalCtoxContextDeepLink(moduleId, record) {
  const cleanModule = String(moduleId || '').trim();
  if (!cleanModule) return '';
  const params = new URLSearchParams();
  const recordId = String(record?.id || '').trim();
  const recordType = String(record?.type || '').trim();
  if (recordId) params.set('record', recordId);
  if (recordType) params.set('record_type', recordType);
  const query = params.toString();
  return `#${encodeURIComponent(cleanModule)}${query ? `?${query}` : ''}`;
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

  // `data-*-id` attributes that are layout/UI hooks, never a record handle.
  const NON_RECORD_ID_ATTRS = new Set([
    'data-context-id', 'data-context-record-id', 'data-tab-id', 'data-grad-id',
    'data-gradient-id', 'data-loading-id', 'data-drawer-id',
  ]);
  // Trailing tokens that describe an interaction (`data-account-click-id`), not the type.
  const ACTION_SUFFIXES = new Set(['click', 'select', 'open', 'toggle', 'manage', 'expand', 'edit', 'view']);
  const deriveTypeFromAttr = (attrName) => {
    const parts = attrName.slice(5, -3).split('-').filter(Boolean); // strip 'data-' + '-id'
    if (parts.length > 1 && ACTION_SUFFIXES.has(parts[parts.length - 1])) parts.pop();
    return parts[0] || '';
  };

  while (current && current !== document.body) {
    const contextRecordId = current.getAttribute('data-context-record-id') || current.getAttribute('data-context-id');
    if (contextRecordId) {
      return {
        type: current.getAttribute('data-context-record-type') || current.getAttribute('data-record-type') || moduleId || 'item',
        id: contextRecordId,
        label: current.getAttribute('data-context-label') || deriveLabelFromElement(current)
      };
    }

    // 1. Any `data-*-id` attribute is treated as a record handle, so a module can
    //    expose a record to the agent without registering its attribute name here.
    //    Canonical generic ids (data-id / data-record-id) win when several are present;
    //    otherwise the first domain-specific data-<thing>-id (e.g. data-customer-id) wins.
    if (current.attributes && current.attributes.length) {
      let chosen = null;
      for (const attr of Array.from(current.attributes)) {
        const name = attr.name;
        const isIdAttr = name === 'data-id' || /^data-[a-z][\w-]*-id$/.test(name);
        if (!isIdAttr || NON_RECORD_ID_ATTRS.has(name) || !attr.value) continue;
        if (name === 'data-id' || name === 'data-record-id') { chosen = attr; break; }
        if (!chosen) chosen = attr;
      }
      if (chosen) {
        let type = (chosen.name === 'data-id' || chosen.name === 'data-record-id')
          ? ''
          : deriveTypeFromAttr(chosen.name);
        if (!type || type === 'record') {
          const recordTypeAttr = current.closest('[data-record-type]');
          type = recordTypeAttr ? recordTypeAttr.getAttribute('data-record-type') : (moduleId || 'item');
        }
        return {
          type: type || moduleId || 'item',
          id: chosen.value,
          label: deriveLabelFromElement(current),
        };
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
  removeLegacyCtoxContextMenus();

  const mod = state.modules.find((item) => item.id === context.module)
    || state.activeModule
    || { id: 'ctox', title: 'CTOX' };
  const canModify = canModifyModule(mod);
  // Whether this actor may run a data change here themselves. If not, the menu
  // hides the self-execute modes and steers them to delegate the change to a
  // reviewer via an approval request. Native policy stays authoritative.
  const canSelfExecute = canSelfExecuteBusinessData(mod, {
    session: state.session,
    governance: state.governance,
  });
  const lifecycle = appLifecycleState(mod, {
    session: state.session,
    governance: state.governance,
  });
  const dataAccess = appReleaseProjection(mod).dataAccess;
  const agentScope = buildGlobalCtoxAgentScopeView({
    actor: actorContext(state.session),
    module: mod,
    lifecycle,
    dataAccess,
    context,
    canModify,
    externalActions: 'none',
  });
  const lang = shellLang();

  const titleText = shellText('chatToCtox') || (lang === 'de' ? 'Mit CTOX chatten' : 'Chat to CTOX');
  const workDataLabel = shellText('chatWorkDataLabel') || (lang === 'de' ? 'Daten ändern' : 'Change data');
  const answerLabel = shellText('chatAnswerLabel') || (lang === 'de' ? 'Frage stellen' : 'Ask question');
  const modifyAppLabel = shellText('chatModifyAppLabel') || (lang === 'de' ? 'App ändern' : 'Change app');
  const approvalLabel = lang === 'de' ? 'Freigabe einholen' : 'Request approval';
  const placeholderText = shellText('chatPlaceholder') || (lang === 'de' ? 'Was soll CTOX hier tun oder prüfen?' : 'What should CTOX do or check here?');
  const dataPlaceholderText = lang === 'de' ? 'Welche Daten sollen geändert werden?' : 'What data should change?';
  const askPlaceholderText = lang === 'de' ? 'Welche Frage soll beantwortet werden?' : 'What question should be answered?';
  const appPlaceholderText = lang === 'de' ? 'Was soll an der App geändert werden?' : 'What should change in the app?';
  const approvalPlaceholderText = lang === 'de' ? 'Was soll nach Freigabe passieren?' : 'What should happen after approval?';
  const sendLabel = shellText('send') || (lang === 'de' ? 'Senden' : 'Send');
  const closeLabel = lang === 'de' ? 'Schließen' : 'Close';
  const missingMsgLabel = lang === 'de' ? 'Nachricht fehlt.' : 'Message is missing.';
  const missingReviewerLabel = lang === 'de' ? 'Reviewer fehlt.' : 'Reviewer is missing.';
  const chatNotReadyLabel = lang === 'de' ? 'Chat ist noch nicht bereit.' : 'Chat is not ready.';
  const chatOpeningLabel = shellText('chatOpening') || (lang === 'de' ? 'Öffne Chat...' : 'Opening Chat...');
  const commandOpeningLabel = lang === 'de' ? 'Sende an Threads...' : 'Sending to Threads...';
  const reviewerLabel = lang === 'de' ? 'Reviewer' : 'Reviewer';
  const reviewerPlaceholder = lang === 'de' ? 'reviewer-user-id' : 'reviewer-user-id';
  const initialUserOptions = renderBusinessUserDatalistOptions([], { session: state.session });

  const subtitle = context.label || shellText('moduleTitles')?.[mod.id] || mod.title || mod.id;

  globalCtoxContextMenuEl.innerHTML = `
    <form class="ctox-context-chat-form" novalidate>
      <header class="ctox-context-header">
        <div class="ctox-context-heading">
          <strong>${escapeHtml(titleText)}</strong>
          <span>${escapeHtml(subtitle)}</span>
        </div>
        <button type="button" class="ctox-context-close-btn" aria-label="${escapeHtml(closeLabel)}">×</button>
      </header>
      <div class="ctox-context-mode" role="radiogroup" aria-label="Aktion">
        ${renderGlobalCtoxContextModeHtml({
          canModify,
          canSelfExecute,
          labels: {
            workData: workDataLabel,
            answer: answerLabel,
            modifyApp: modifyAppLabel,
            dataApprovalDescription: lang === 'de'
              ? 'Braucht Freigabe: Daten werden erst nach Review geändert.'
              : 'Data changes require approval. Pick a reviewer.',
            appApprovalDescription: lang === 'de'
              ? 'Braucht Freigabe: Die App wird erst nach Review geändert.'
              : 'App changes require approval. Pick a reviewer.',
          },
        })}
      </div>
      <p class="ctox-context-mode-help" data-ctox-context-mode-help></p>
      ${renderGlobalCtoxAgentScopeHtml({ view: agentScope })}
      <label class="ctox-context-user-row" hidden>
        <span class="ctox-context-user-label">${escapeHtml(reviewerLabel)}</span>
        <input class="ctox-context-user-input" type="text" autocomplete="off" list="ctox-context-user-options" placeholder="${escapeHtml(reviewerPlaceholder)}">
        <datalist id="ctox-context-user-options" data-ctox-context-user-options>${initialUserOptions}</datalist>
      </label>
      <textarea class="ctox-context-textarea" placeholder="${escapeHtml(placeholderText)}"></textarea>
      <footer class="ctox-context-footer">
        <span class="ctox-context-status"></span>
        <button type="submit" class="ctox-context-submit-btn">${escapeHtml(sendLabel)}</button>
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
  const userRow = globalCtoxContextMenuEl.querySelector('.ctox-context-user-row');
  const userInput = globalCtoxContextMenuEl.querySelector('.ctox-context-user-input');
  const userOptionsEl = globalCtoxContextMenuEl.querySelector('[data-ctox-context-user-options]');
  const userLabel = globalCtoxContextMenuEl.querySelector('.ctox-context-user-label');
  const modeHelp = globalCtoxContextMenuEl.querySelector('[data-ctox-context-mode-help]');
  const statusEl = globalCtoxContextMenuEl.querySelector('.ctox-context-status');
  const closeBtn = globalCtoxContextMenuEl.querySelector('.ctox-context-close-btn');

  closeBtn.addEventListener('click', () => {
    hideGlobalCtoxContextMenu();
  });

  const modeLabels = globalCtoxContextMenuEl.querySelectorAll('.ctox-context-mode label');
  const syncModeInputs = () => {
    const mode = new FormData(form).get('contextMode') || 'data';
    const selectedModeLabel = globalCtoxContextMenuEl.querySelector('.ctox-context-mode label.is-selected')
      || Array.from(modeLabels).find((label) => label.querySelector('input')?.checked);
    const needsApproval = selectedModeLabel?.dataset.approvalRequired === 'true';
    if (userRow) {
      userRow.hidden = !needsApproval;
      userRow.style.display = needsApproval ? 'grid' : 'none';
    }
    if (userInput) {
      userInput.placeholder = reviewerPlaceholder;
      userInput.value = needsApproval ? userInput.value : '';
    }
    if (userLabel) {
      userLabel.textContent = reviewerLabel;
    }
    if (modeHelp) {
      modeHelp.textContent = selectedModeLabel?.dataset.description || '';
    }
    if (statusEl) {
      statusEl.textContent = needsApproval
        ? (lang === 'de' ? 'Freigabe nötig.' : 'Approval required.')
        : '';
    }
    textarea.placeholder = needsApproval
      ? approvalPlaceholderText
      : mode === 'ask'
        ? askPlaceholderText
        : mode === 'app'
          ? appPlaceholderText
          : mode === 'data'
            ? dataPlaceholderText
            : placeholderText;
  };
  modeLabels.forEach(label => {
    label.addEventListener('click', () => {
      modeLabels.forEach(l => l.classList.remove('is-selected'));
      label.classList.add('is-selected');
      const input = label.querySelector('input');
      if (input) input.checked = true;
      syncModeInputs();
    });
  });
  syncModeInputs();
  populateGlobalCtoxUserOptions(userOptionsEl).catch((error) => {
    console.warn('[business-os] failed to populate context user picker:', error);
  });

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

    const mode = new FormData(form).get('contextMode') || 'data';
    const needsApproval = (mode === 'data' && !canSelfExecute) || (mode === 'app' && !canModify);

    if (needsApproval) {
      const reviewerUserId = String(userInput?.value || '').trim();
      if (!reviewerUserId) {
        if (statusEl) statusEl.textContent = missingReviewerLabel;
        userInput?.focus?.();
        return;
      }
      if (!state.commandBus?.dispatch) {
        if (statusEl) statusEl.textContent = chatNotReadyLabel;
        return;
      }
      if (statusEl) statusEl.textContent = commandOpeningLabel;
      const sourceContext = {
        module: mod.id,
        column: context.column,
        record_type: context.record_type,
        record_id: context.record_id,
        label: context.label || mod.title || mod.id,
        deep_link: context.deep_link,
        selected_text: context.selected_text,
        clicked_text: context.clicked_text,
        context_v2: context.context_v2,
      };
      const recordId = context.record_id || mod.id;
      const modeLabel = mode === 'app' ? modifyAppLabel : workDataLabel;
      const title = `${approvalLabel}: ${modeLabel} · ${subtitle}`;
      const targetCommandType = mode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.data.modify';
      const targetPayload = {
        title,
        instruction: prompt,
        prompt,
        user_message: prompt,
        mode,
        target: mode === 'app' ? 'app' : 'data',
        context: sourceContext,
        thread_key: `business-os/${mod.id}/${recordId || 'module'}`,
      };
      const payload = {
        prompt,
        instruction: prompt,
        reviewer_user_id: reviewerUserId,
        title,
        target_command_type: targetCommandType,
        target_module: mod.id,
        target_record_id: mode === 'app' ? mod.id : recordId,
        source_context: sourceContext,
        target_payload: targetPayload,
      };
      try {
        await state.commandBus.dispatch({
          id: `cmd_${crypto.randomUUID()}`,
          module: 'threads',
          command_type: 'threads.ctox_approval.request',
          record_id: recordId,
          inbound_channel: mod.id,
          payload,
          client_context: {
            action: 'context-approval-request',
            mode,
            module: 'threads',
            module_id: 'threads',
            app_id: 'threads',
            source_module: mod.id,
            actor: agentScope.actor,
            visible_scope: agentScope,
            column: context.column,
            record_type: context.record_type,
            record_id: context.record_id,
          },
        });
        hideGlobalCtoxContextMenu();
      } catch (error) {
        if (statusEl) statusEl.textContent = error?.message || chatNotReadyLabel;
      }
      return;
    }

    if (!state.commandBus?.dispatch) {
      if (statusEl) statusEl.textContent = chatNotReadyLabel;
      return;
    }

    if (statusEl) statusEl.textContent = chatOpeningLabel;

    let title;
    let instruction;
    if (mode === 'app') {
      title = `${mod.title || mod.id} App ändern`;
      instruction = `Ändere die ${mod.title || mod.id}-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, App-Daten selbst nicht als primäres Ziel verändern.\n\n${prompt}`;
    } else if (mode === 'ask') {
      title = `Frage · ${subtitle}`;
      instruction = `Beantworte die folgende Frage ausschließlich lesend. Nutze nur vorhandene Daten und Kontext; führe keine Änderungen an Daten, Records, Dateien oder der App aus. Antworte knapp und direkt.\n\n${prompt}`;
    } else {
      title = `Kontext-Aufgabe · ${subtitle}`;
      instruction = prompt;
    }

    try {
      const result = await createContextActionsFacade(mod).dispatch(mode, {
        context,
        prompt: instruction,
        title,
        client_context: {
          source: 'business-os-global-context',
          action: 'context-chat',
          mode,
          target: mode === 'app' ? 'app' : (mode === 'ask' ? 'read' : 'data'),
          column: context.column,
          record_type: context.record_type,
          record_id: context.record_id || mod.id,
          module_id: mod.id,
          app_id: mod.id,
          actor: agentScope.actor,
          visible_scope: agentScope,
        },
      });
      openBusinessChat({
        title,
        module: mod.id,
        source_module: mod.id,
        record_id: context.record_id || mod.id,
        command_id: result?.command_id || result?.id || '',
        thread_key: `business-os/${mod.id}/${context.record_id || 'module'}`,
        reuseActive: false,
      });
      hideGlobalCtoxContextMenu();
    } catch (error) {
      if (statusEl) statusEl.textContent = error?.message || chatNotReadyLabel;
    }
  });

  requestAnimationFrame(() => {
    textarea.focus();
  });
}

function hideGlobalCtoxContextMenu() {
  if (globalCtoxContextMenuEl) {
    globalCtoxContextMenuEl.hidden = true;
  }
  removeLegacyCtoxContextMenus();
}

function removeLegacyCtoxContextMenus() {
  document.querySelectorAll('.ctox-context-menu:not(.ctox-global-context-menu)').forEach((menu) => {
    menu.remove();
  });
}

async function populateGlobalCtoxUserOptions(datalistEl) {
  if (!datalistEl) return;
  const users = await loadGlobalCtoxContextUsers();
  datalistEl.innerHTML = renderBusinessUserDatalistOptions(users, { session: state.session });
}

async function loadGlobalCtoxContextUsers() {
  const sessionUser = state.session?.user ? [state.session.user] : [];
  try {
    await state.sync?.startCollection?.('business_users')?.catch?.(() => null);
    const coll = state.db?.collection?.('business_users');
    if (!coll?.find) return sessionUser;
    const docs = await coll.find().exec();
    return docs.map((doc) => doc.toJSON?.() || doc).filter(Boolean);
  } catch (error) {
    console.warn('[business-os] business_users picker fallback:', error);
    return sessionUser;
  }
}
