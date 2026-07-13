#!/usr/bin/env node
/*
 * Browser/Rust RxDB WebRTC smoke test for CTOX Business OS.
 *
 * Defaults to the isolated smoke page and browser-to-rust mode:
 *   node src/core/rxdb/tools/browser_rust_smoke.js
 *
 * Useful variants:
 *   SMOKE_MODE=rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-agent-artifacts-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-agent-artifacts-stress-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-agent-artifacts-churn-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-agent-artifacts-background-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-update-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-materialize-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-file-viewer-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=workspace-large-file-viewer-restart-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=tickets-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=tickets-clarification-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=outbound-active-ui SMOKE_PAGE_PATH=/index.html#outbound node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=coding-agents-ui SMOKE_PAGE_PATH=/index.html SMOKE_CODING_AGENT_PROVIDER=codex node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=business-os-roles-permissions-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=business-os-dynamic-apps-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=business-os-app-release-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=business-os-app-audience-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=business-os-agent-scope-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=business-os-threads-rightclick-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=business-os-auth-scope-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=business-os-fresh-profile-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=browser-lifecycle-ui SMOKE_PAGE_PATH=/index.html#browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=browser-input-runtime SMOKE_PAGE_PATH=/index.html#browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=browser-handoff-ui SMOKE_PAGE_PATH=/index.html#browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=migration-version-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-burst-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-reload-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-restart-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=command-midflight-restart-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=office-document-midflight-restart-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=office-spreadsheet-midflight-restart-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=rollover-native-peer-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=tab-freeze-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=network-flap-browser-to-rust SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=restart-signaling-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=signaling-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=peer-lifecycle-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=checkpoint-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=schema-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=native-schema-drift-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=replication-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=replication-push-contract-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=file-chunk-metadata-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=file-chunk-tombstone-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_MODE=file-chunk-stale-generation-error-browser-status SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-agent-artifacts-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-agent-artifacts-stress-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-agent-artifacts-churn-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-agent-artifacts-background-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-update-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-materialize-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-file-viewer-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=workspace-large-file-viewer-restart-rust-to-browser node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=tickets-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=tickets-clarification-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html#browser SMOKE_MODE=browser-lifecycle-ui node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html#browser SMOKE_MODE=browser-handoff-ui node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=migration-version-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-burst-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-reload-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=command-midflight-restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=rollover-native-peer-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=tab-freeze-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=network-flap-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=restart-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=restart-signaling-browser-to-rust node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=signaling-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=peer-lifecycle-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=checkpoint-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=schema-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=native-schema-drift-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=replication-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=replication-push-contract-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=file-chunk-metadata-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=file-chunk-tombstone-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 *   SMOKE_PAGE_PATH=/index.html SMOKE_MODE=file-chunk-stale-generation-error-browser-status node src/core/rxdb/tools/browser_rust_smoke.js
 */
const net = require('net');
const path = require('path');
const crypto = require('crypto');
const fs = require('fs');
const os = require('os');
const zlib = require('zlib');
const { spawn, spawnSync } = require('child_process');
const {
  businessOsProductionSmokeModes,
  businessOsProductionSmokeModeSet,
} = require('./business_os_production_smoke_registry');

const root = path.resolve(__dirname, '../../../..');
const runtimeRootProvided = !!process.env.CTOX_SMOKE_ROOT;
const runtimeRoot = process.env.CTOX_SMOKE_ROOT || fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-rxdb-smoke-'));
const keepSmokeArtifacts = process.env.CTOX_SMOKE_KEEP_ARTIFACTS === '1';
const smokeRunId = process.env.CTOX_SMOKE_RUN_ID || `smoke-${process.pid}-${crypto.randomBytes(6).toString('hex')}`;
const smokeProcessLifecyclePath = process.env.SMOKE_PROCESS_LIFECYCLE_PATH || '';
const smokeChildren = new Set();
const smokeProcessLifecycle = {
  schema: 'ctox.rxdb.smoke_process_lifecycle.v1',
  runId: smokeRunId,
  mode: process.env.SMOKE_MODE || 'rust-to-browser',
  parent: processIdentity(process.pid),
  startedAt: new Date().toISOString(),
  startupPhase: 'bootstrap',
  events: [],
};
recordSmokeProcessEvent('smoke_started');
const restoreProtectedBusinessOsSources = protectBusinessOsSourceFiles();
const smokeRootPrepareStartedAt = Date.now();
prepareSmokeRoot(runtimeRoot);
const smokeRootPrepareMs = Date.now() - smokeRootPrepareStartedAt;
const playwrightModule =
  process.env.PLAYWRIGHT_MODULE_PATH ||
  (() => {
    const candidates = [
      'playwright',
      path.join(root, 'src/apps/business-os/node_modules/playwright'),
      '/tmp/ctox-pw-smoke/node_modules/playwright',
    ];
    for (const candidate of candidates) {
      try {
        return require.resolve(candidate);
      } catch {
        // Try the next known browser automation runtime.
      }
    }
    throw new Error(
      'No Playwright runtime found. Install playwright in this checkout or set PLAYWRIGHT_MODULE_PATH.'
    );
  })();
const { chromium } = require(playwrightModule);

function existingChromeExecutable() {
  if (process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE) return process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE;
  const playwrightChromium = chromium.executablePath?.();
  const candidates = [
    playwrightChromium,
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome',
  ].filter(Boolean);
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function chromiumLaunchOptions() {
  const executablePath = existingChromeExecutable();
  const options = {
    headless: true,
    args: [
      '--disable-gpu',
      '--disable-features=WebRtcHideLocalIpsWithMdns',
    ],
  };
  if (executablePath) options.executablePath = executablePath;
  return options;
}

const ctoxBin = process.env.CTOX_BIN || path.join(root, 'runtime/build/core-rxdb-integration-target/debug/ctox');
const businessPort = parsePositiveIntegerEnv('BUSINESS_PORT', process.env.BUSINESS_PORT || '8877', { max: 65535 });
const signalingPort = parsePositiveIntegerEnv('SIGNALING_PORT', process.env.SIGNALING_PORT || '18876', { max: 65535 });
const signalingUrl = `ws://127.0.0.1:${signalingPort}`;
const signalingDebug = process.env.SIGNALING_DEBUG === '1';
const sqlitePath = process.env.CTOX_SQLITE || path.join(runtimeRoot, 'runtime/business-os-rxdb.sqlite3');
const nativeBusinessOsSqlitePath = path.join(runtimeRoot, 'runtime/business-os.sqlite3');
// OS-X3: the default is the Business OS app shell — the same default the
// soak matrix uses (browser_rust_smoke_matrix.js). The legacy synthetic page
// `/__rxdb_smoke__.html` no longer exists anywhere in the tree; defaulting to
// it produced a silent 404 boot and a misleading timeout 30s later.
const pagePath = process.env.SMOKE_PAGE_PATH || '/index.html';
const smokeMode = process.env.SMOKE_MODE || 'browser-to-rust';
const dynamicOpenModuleFixture = {
  module: {
    id: 'phase13-open-module-guard',
    title: 'Phase 13 OpenModule Guard',
    glyph: '13',
    version: '1.0.0',
    source: 'installed',
    install_scope: 'installed',
    entry: 'installed-modules/phase13-open-module-guard/index.js',
    collections: ['business_commands'],
    lifecycle: { runtime_installed: true },
  },
};
const releaseModuleFixture = {
  module: {
    id: 'phase10-release-app',
    title: 'Phase 10 Release App',
    description: 'Browser/Rust Smoke-App fuer Freigabe, Sichtbarkeit und Rollback.',
    category: 'Smoke',
    glyph: '10',
    version: '0.8.0',
    source: 'installed',
    install_scope: 'installed',
    entry: 'installed-modules/phase10-release-app/index.html',
    editable: true,
    deletable: false,
    default_installed: true,
    collections: ['business_commands'],
    lifecycle: {
      runtime_installed: true,
      visibility_state: 'private',
      audience: 'private',
    },
  },
};
const agentScopeModuleFixture = {
  module: {
    id: 'phase12-agent-scope-app',
    title: 'Phase 12 Agent Scope App',
    description: 'Browser/Rust Smoke-App fuer Agent Scope, App-Sichtbarkeit und Datenrechte.',
    category: 'Smoke',
    glyph: '12',
    version: '1.0.0',
    source: 'installed',
    install_scope: 'installed',
    entry: 'installed-modules/phase12-agent-scope-app/index.js',
    editable: true,
    deletable: false,
    default_installed: true,
    layout: { shell: 'full-workspace' },
    collections: ['business_commands'],
    lifecycle: { runtime_installed: true },
  },
};
if (smokeMode === 'business-os-dynamic-apps-ui') {
  prepareBusinessOsDynamicOpenModuleFixture(dynamicOpenModuleFixture);
}
if (smokeMode === 'business-os-app-release-ui') {
  prepareBusinessOsReleaseModuleFixture(releaseModuleFixture);
}
if (smokeMode === 'business-os-agent-scope-ui') {
  prepareBusinessOsAgentScopeModuleFixture(agentScopeModuleFixture);
}
const smokeDbId = process.env.SMOKE_DB_ID || `${smokeMode}_${Date.now()}_${token(8)}`;
const useAppDb = process.env.SMOKE_USE_APP_DB === '1'
  || /^\/index\.html(?:[?#]|$)/.test(pagePath)
  || /^\/business-os\/?(?:[?#]|$)/.test(pagePath);
const syncConfigWaitMs = parsePositiveIntegerEnv(
  'SMOKE_SYNC_CONFIG_WAIT_MS',
  process.env.SMOKE_SYNC_CONFIG_WAIT_MS || '60000',
  { max: 300000 }
);
const serverReadyTimeoutMs = parsePositiveIntegerEnv(
  'SMOKE_SERVER_READY_TIMEOUT_MS',
  process.env.SMOKE_SERVER_READY_TIMEOUT_MS || '90000',
  { max: 300000 }
);
const pageNavigationTimeoutMs = parsePositiveIntegerEnv(
  'SMOKE_PAGE_NAVIGATION_TIMEOUT_MS',
  process.env.SMOKE_PAGE_NAVIGATION_TIMEOUT_MS || '90000',
  { max: 120000 }
);
const smokeHookWaitTimeoutMs = parsePositiveIntegerEnv(
  'SMOKE_HOOK_WAIT_TIMEOUT_MS',
  process.env.SMOKE_HOOK_WAIT_TIMEOUT_MS || '60000',
  { max: 300000 }
);
const hasOwn = (object, key) => Object.prototype.hasOwnProperty.call(object, key);
const codingAgentSmoke = createCodingAgentSmokeConfig(smokeMode);

function createCodingAgentSmokeConfig(mode) {
  if (mode !== 'coding-agents-ui') return null;
  const provider = String(process.env.SMOKE_CODING_AGENT_PROVIDER || 'codex').trim().toLowerCase();
  if (!['codex', 'antigravity', 'claude'].includes(provider)) {
    throw new Error(`SMOKE_CODING_AGENT_PROVIDER must be one of codex, antigravity, claude; got ${JSON.stringify(provider)}`);
  }
  const providedWorkspace = hasOwn(process.env, 'SMOKE_CODING_AGENT_WORKSPACE');
  const workspaceRoot = providedWorkspace
    ? path.resolve(process.env.SMOKE_CODING_AGENT_WORKSPACE)
    : fs.mkdtempSync(path.join(os.tmpdir(), `ctox-coding-agent-ui-${provider}-`));
  fs.mkdirSync(workspaceRoot, { recursive: true });
  return {
    provider,
    workspaceRoot,
    cleanupWorkspace: !providedWorkspace,
    createMarker: `CTOX_CODING_AGENTS_UI_CREATE_${Date.now()}_${token(6)}`,
    followupMarker: `CTOX_CODING_AGENTS_UI_FOLLOWUP_${Date.now()}_${token(6)}`,
  };
}

const supportedSmokeModes = [
  'browser-to-rust',
  'rust-to-browser',
  'presence-merge-two-browsers',
  'workspace-rust-to-browser',
  'workspace-agent-artifacts-rust-to-browser',
  'workspace-agent-artifacts-stress-rust-to-browser',
  'workspace-agent-artifacts-churn-rust-to-browser',
  'workspace-agent-artifacts-background-rust-to-browser',
  'workspace-update-rust-to-browser',
  'workspace-large-materialize-rust-to-browser',
  'workspace-large-file-viewer-rust-to-browser',
  'workspace-large-file-viewer-restart-rust-to-browser',
  'command-browser-to-rust',
  'tickets-browser-to-rust',
  'tickets-clarification-browser-to-rust',
  'outbound-active-ui',
  'coding-agents-ui',
  'business-os-ui-regression',
  'business-os-roles-permissions-ui',
  'business-os-dynamic-apps-ui',
  'browser-lifecycle-ui',
  'browser-input-runtime',
  'browser-handoff-ui',
  'browser-responsive-ui',
  'migration-version-browser-to-rust',
  'command-burst-browser-to-rust',
  'command-reload-browser-to-rust',
  'command-restart-browser-to-rust',
  'command-midflight-restart-browser-to-rust',
  'office-document-midflight-restart-browser-to-rust',
  'office-spreadsheet-midflight-restart-browser-to-rust',
  'rollover-native-peer-browser-to-rust',
  'tab-freeze-browser-to-rust',
  'network-flap-browser-to-rust',
  'restart-browser-to-rust',
  'restart-signaling-browser-to-rust',
  'signaling-error-browser-status',
  'peer-lifecycle-browser-status',
  'checkpoint-error-browser-status',
  'rxdb-protocol-error-browser-status',
  'schema-error-browser-status',
  'native-schema-drift-browser-status',
  'replication-error-browser-status',
  'replication-push-contract-error-browser-status',
  'file-chunk-metadata-error-browser-status',
  'file-chunk-tombstone-error-browser-status',
  'file-chunk-stale-generation-error-browser-status',
  ...businessOsProductionSmokeModes,
];

if (!supportedSmokeModes.includes(smokeMode)) {
  throw new Error(`Unsupported SMOKE_MODE=${smokeMode}`);
}
if ([
  'tickets-browser-to-rust',
  'tickets-clarification-browser-to-rust',
  'outbound-active-ui',
  'signaling-error-browser-status',
  'peer-lifecycle-browser-status',
  'checkpoint-error-browser-status',
  'rxdb-protocol-error-browser-status',
  'schema-error-browser-status',
  'native-schema-drift-browser-status',
  'replication-error-browser-status',
  'replication-push-contract-error-browser-status',
  'file-chunk-metadata-error-browser-status',
  'file-chunk-tombstone-error-browser-status',
  'file-chunk-stale-generation-error-browser-status',
  'coding-agents-ui',
  'business-os-ui-regression',
  'business-os-roles-permissions-ui',
  'business-os-dynamic-apps-ui',
  'business-os-threads-rightclick-ui',
  'presence-merge-two-browsers',
  ...businessOsProductionSmokeModes,
].includes(smokeMode) && !useAppDb) {
  throw new Error(`SMOKE_MODE=${smokeMode} requires an app shell SMOKE_PAGE_PATH such as /index.html or /business-os#ctox`);
}
const implementedBusinessOsProductionSmokeModes = new Set([
  'business-os-app-release-ui',
  'business-os-app-audience-ui',
  'business-os-agent-scope-ui',
  'business-os-auth-scope-ui',
  'business-os-fresh-profile-ui',
  'business-os-threads-rightclick-ui',
  'business-os-threads-scale-ui',
  'business-os-restore-resync-ui',
]);
if (businessOsProductionSmokeModeSet.has(smokeMode) && !implementedBusinessOsProductionSmokeModes.has(smokeMode)) {
  throw new Error(`SMOKE_MODE=${smokeMode} is registered for Business OS production coverage but the browser story is not implemented yet. Complete the matching Phase 10-14 slice before using it as a release gate.`);
}

const BUSINESS_OS_CORE_STATUS_COLLECTIONS = Object.freeze([
  'business_module_catalog',
  'ctox_runtime_settings',
]);
const BUSINESS_OS_SHELL_STATUS_COLLECTIONS = Object.freeze([
  ...BUSINESS_OS_CORE_STATUS_COLLECTIONS,
  'business_commands',
  'ctox_queue_tasks',
  'desktop_files',
]);
const BUSINESS_OS_COMMAND_STATUS_COLLECTIONS = Object.freeze([
  ...BUSINESS_OS_CORE_STATUS_COLLECTIONS,
  'business_commands',
  'ctox_queue_tasks',
]);

function parsePositiveIntegerEnv(name, value, options = {}) {
  const parsed = Number(value);
  const min = options.min ?? 1;
  const max = options.max ?? Number.MAX_SAFE_INTEGER;
  if (!Number.isInteger(parsed) || parsed < min || parsed > max) {
    throw new Error(`${name} must be an integer between ${min} and ${max}; got ${JSON.stringify(String(value))}`);
  }
  return parsed;
}

function token(len = 12) {
  const alphabet = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  let out = '';
  for (let i = 0; i < len; i++) out += alphabet[Math.floor(Math.random() * alphabet.length)];
  return out;
}

function paethPredictor(left, up, upLeft) {
  const estimate = left + up - upLeft;
  const leftDistance = Math.abs(estimate - left);
  const upDistance = Math.abs(estimate - up);
  const upLeftDistance = Math.abs(estimate - upLeft);
  if (leftDistance <= upDistance && leftDistance <= upLeftDistance) return left;
  if (upDistance <= upLeftDistance) return up;
  return upLeft;
}

function analyzePngScreenshot(buffer) {
  const signature = '89504e470d0a1a0a';
  if (!Buffer.isBuffer(buffer) || buffer.subarray(0, 8).toString('hex') !== signature) {
    throw new Error('Business OS visual evidence screenshot is not a PNG');
  }
  let offset = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  const idatChunks = [];
  while (offset + 12 <= buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString('ascii');
    const dataStart = offset + 8;
    const dataEnd = dataStart + length;
    if (dataEnd + 4 > buffer.length) break;
    const data = buffer.subarray(dataStart, dataEnd);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === 'IDAT') {
      idatChunks.push(data);
    } else if (type === 'IEND') {
      break;
    }
    offset = dataEnd + 4;
  }
  const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 0;
  if (!width || !height || bitDepth !== 8 || !channels || idatChunks.length === 0) {
    throw new Error(`Unsupported PNG screenshot format: ${JSON.stringify({ width, height, bitDepth, colorType, idatChunks: idatChunks.length })}`);
  }
  const inflated = zlib.inflateSync(Buffer.concat(idatChunks));
  const stride = width * channels;
  const raw = Buffer.alloc(width * height * channels);
  let sourceOffset = 0;
  for (let y = 0; y < height; y++) {
    const filter = inflated[sourceOffset++];
    const rowOffset = y * stride;
    const prevRowOffset = (y - 1) * stride;
    for (let x = 0; x < stride; x++) {
      const value = inflated[sourceOffset++];
      const left = x >= channels ? raw[rowOffset + x - channels] : 0;
      const up = y > 0 ? raw[prevRowOffset + x] : 0;
      const upLeft = y > 0 && x >= channels ? raw[prevRowOffset + x - channels] : 0;
      if (filter === 0) {
        raw[rowOffset + x] = value;
      } else if (filter === 1) {
        raw[rowOffset + x] = (value + left) & 255;
      } else if (filter === 2) {
        raw[rowOffset + x] = (value + up) & 255;
      } else if (filter === 3) {
        raw[rowOffset + x] = (value + Math.floor((left + up) / 2)) & 255;
      } else if (filter === 4) {
        raw[rowOffset + x] = (value + paethPredictor(left, up, upLeft)) & 255;
      } else {
        throw new Error(`Unsupported PNG filter type ${filter}`);
      }
    }
  }

  const stepX = Math.max(1, Math.floor(width / 96));
  const stepY = Math.max(1, Math.floor(height / 54));
  const colors = new Map();
  let sampleCount = 0;
  let visibleSamples = 0;
  let lumaSum = 0;
  let lumaSquaredSum = 0;
  for (let y = 0; y < height; y += stepY) {
    for (let x = 0; x < width; x += stepX) {
      const index = (y * width + x) * channels;
      const red = raw[index];
      const green = raw[index + 1];
      const blue = raw[index + 2];
      const alpha = channels === 4 ? raw[index + 3] : 255;
      if (alpha > 8) visibleSamples++;
      const luma = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
      lumaSum += luma;
      lumaSquaredSum += luma * luma;
      const bucket = `${red >> 3},${green >> 3},${blue >> 3}`;
      colors.set(bucket, (colors.get(bucket) || 0) + 1);
      sampleCount++;
    }
  }
  const mean = sampleCount ? lumaSum / sampleCount : 0;
  const variance = sampleCount ? Math.max(0, (lumaSquaredSum / sampleCount) - (mean * mean)) : 0;
  const dominantCount = Math.max(0, ...colors.values());
  return {
    width,
    height,
    sampleCount,
    visibleSamples,
    uniqueSampledColors: colors.size,
    luminanceStdDev: Math.round(Math.sqrt(variance) * 10) / 10,
    dominantColorRatioPct: sampleCount ? Math.round((dominantCount / sampleCount) * 1000) / 10 : 100,
  };
}

async function captureBusinessOsVisualScreenshotEvidence(page) {
  const buffer = await page.screenshot({ fullPage: false });
  if (process.env.SMOKE_VISUAL_SCREENSHOT_PATH) {
    fs.writeFileSync(process.env.SMOKE_VISUAL_SCREENSHOT_PATH, buffer);
  }
  const evidence = analyzePngScreenshot(buffer);
  const problems = [];
  if (evidence.width < 900 || evidence.height < 600) {
    problems.push(`viewport too small: ${evidence.width}x${evidence.height}`);
  }
  if (evidence.uniqueSampledColors < 48) {
    problems.push(`too few sampled colors: ${evidence.uniqueSampledColors}`);
  }
  if (evidence.luminanceStdDev < 8) {
    problems.push(`low luminance variation: ${evidence.luminanceStdDev}`);
  }
  if (evidence.dominantColorRatioPct > 92) {
    problems.push(`dominant color ratio too high: ${evidence.dominantColorRatioPct}%`);
  }
  if (evidence.visibleSamples < Math.floor(evidence.sampleCount * 0.98)) {
    problems.push(`transparent sample ratio too high: ${evidence.visibleSamples}/${evidence.sampleCount}`);
  }
  if (problems.length) {
    throw new Error(`Business OS visual screenshot evidence failed: ${problems.join('; ')} ${JSON.stringify(evidence)}`);
  }
  return evidence;
}

function processIdentity(pid) {
  const identity = { pid: Number(pid) || null, ppid: null, pgid: null };
  if (!identity.pid || process.platform === 'win32') return identity;
  try {
    const result = spawnSync('ps', ['-o', 'ppid=', '-o', 'pgid=', '-p', String(identity.pid)], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const [ppid, pgid] = String(result.stdout || '').trim().split(/\s+/).map(Number);
    if (Number.isFinite(ppid)) identity.ppid = ppid;
    if (Number.isFinite(pgid)) identity.pgid = pgid;
  } catch {
    // Process-group diagnostics are best effort on non-POSIX hosts.
  }
  return identity;
}

function writeSmokeProcessLifecycle() {
  smokeProcessLifecycle.endedAt = new Date().toISOString();
  smokeProcessLifecycle.startupPhase = smokeProcessLifecycle.startupPhase || 'unknown';
  if (!smokeProcessLifecyclePath) return;
  try {
    fs.mkdirSync(path.dirname(smokeProcessLifecyclePath), { recursive: true });
    fs.writeFileSync(smokeProcessLifecyclePath, `${JSON.stringify(smokeProcessLifecycle, null, 2)}\n`);
  } catch (error) {
    process.stderr.write(`[smoke-process-lifecycle] write failed: ${error?.message || error}\n`);
  }
}

function recordSmokeProcessEvent(type, details = {}) {
  smokeProcessLifecycle.events.push({
    at: new Date().toISOString(),
    elapsedMs: Date.now() - Date.parse(smokeProcessLifecycle.startedAt),
    type,
    phase: smokeProcessLifecycle.startupPhase,
    ...details,
  });
  writeSmokeProcessLifecycle();
}

function setSmokeStartupPhase(phase) {
  if (!phase || smokeProcessLifecycle.startupPhase === phase) return;
  smokeProcessLifecycle.startupPhase = phase;
  recordSmokeProcessEvent('phase_changed', { phase });
}

function trackSmokeChild(child, kind = 'child') {
  if (!child) return child;
  child.__ctoxSmokeOwner = smokeRunId;
  child.__ctoxSmokeKind = kind;
  smokeChildren.add(child);
  recordSmokeProcessEvent('child_spawn_requested', {
    kind,
    child: processIdentity(child.pid),
  });
  child.once('spawn', () => {
    recordSmokeProcessEvent('child_spawned', {
      kind,
      child: processIdentity(child.pid),
    });
  });
  child.once('error', (error) => {
    recordSmokeProcessEvent('child_spawn_error', {
      kind,
      child: processIdentity(child.pid),
      error: error?.message || String(error),
    });
  });
  child.once('exit', (code, signal) => {
    smokeChildren.delete(child);
    recordSmokeProcessEvent('child_exited', {
      kind,
      child: processIdentity(child.pid),
      code,
      signal,
      signalSource: child.__ctoxTerminationRequest || (signal ? 'unknown_external_source' : null),
    });
  });
  return child;
}

function terminateOwnedSmokeChild(child, signal, cleanupOwner, reason) {
  if (!child || child.__ctoxSmokeOwner !== smokeRunId) return false;
  if (child.exitCode !== null || child.signalCode !== null) return false;
  child.__ctoxTerminationRequest = { cleanupOwner, reason, signal, requestedAt: new Date().toISOString() };
  recordSmokeProcessEvent('child_termination_requested', {
    kind: child.__ctoxSmokeKind || 'child',
    child: processIdentity(child.pid),
    cleanupOwner,
    reason,
    signal,
  });
  try {
    return child.kill(signal);
  } catch (error) {
    recordSmokeProcessEvent('child_termination_failed', {
      kind: child.__ctoxSmokeKind || 'child',
      child: processIdentity(child.pid),
      cleanupOwner,
      reason,
      signal,
      error: error?.message || String(error),
    });
    return false;
  }
}

function killTrackedSmokeChildren(signal = 'SIGTERM', cleanupOwner = 'smoke-parent', reason = 'shutdown') {
  for (const child of smokeChildren) {
    terminateOwnedSmokeChild(child, signal, cleanupOwner, reason);
  }
}

function protectBusinessOsSourceFiles() {
  const protectedPaths = [
    path.join(root, 'src/apps/business-os/app.js'),
    path.join(root, 'src/apps/business-os/index.html'),
  ];
  const snapshots = protectedPaths
    .filter((file) => fs.existsSync(file))
    .map((file) => ({ file, content: fs.readFileSync(file, 'utf8') }));
  let restored = false;
  const restore = () => {
    if (restored) return;
    restored = true;
    const restoredFiles = [];
    for (const snapshot of snapshots) {
      try {
        if (fs.existsSync(snapshot.file) && fs.readFileSync(snapshot.file, 'utf8') === snapshot.content) {
          continue;
        }
        fs.writeFileSync(snapshot.file, snapshot.content);
        restoredFiles.push(path.relative(root, snapshot.file));
      } catch (error) {
        process.stderr.write(`[business-os-source-guard] failed to restore ${snapshot.file}: ${error?.message || error}\n`);
      }
    }
    if (restoredFiles.length) {
      process.stderr.write(`business_os_source_restored=${restoredFiles.join(',')}\n`);
    }
  };
  process.once('exit', () => {
    killTrackedSmokeChildren('SIGKILL', 'process-exit-handler', 'parent-exit');
    restore();
    writeSmokeProcessLifecycle();
  });
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.once(signal, () => {
      recordSmokeProcessEvent('parent_signal_received', {
        signal,
        signalSource: 'unknown_external_source',
        parent: processIdentity(process.pid),
      });
      killTrackedSmokeChildren('SIGTERM', 'parent-signal-handler', `parent-received-${signal}`);
      restore();
      process.exit(signal === 'SIGINT' ? 130 : 143);
    });
  }
  return restore;
}

function prepareSmokeRoot(targetRoot) {
  fs.mkdirSync(path.join(targetRoot, 'runtime'), { recursive: true });
  for (const entry of ['Cargo.toml', 'contracts']) {
    const target = path.join(targetRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(root, entry), target, entry === 'Cargo.toml' ? 'file' : 'dir');
  }
  prepareSmokeSourceRoot(targetRoot);
}

function prepareSmokeSourceRoot(targetRoot) {
  const sourceRoot = path.join(root, 'src');
  const targetSourceRoot = path.join(targetRoot, 'src');
  const targetAppsRoot = path.join(targetSourceRoot, 'apps');
  fs.mkdirSync(targetAppsRoot, { recursive: true });

  for (const entry of fs.readdirSync(sourceRoot)) {
    if (entry === 'apps') continue;
    const target = path.join(targetSourceRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(sourceRoot, entry), target, 'dir');
  }

  const sourceAppsRoot = path.join(sourceRoot, 'apps');
  for (const entry of fs.readdirSync(sourceAppsRoot)) {
    if (entry === 'business-os') continue;
    const target = path.join(targetAppsRoot, entry);
    if (fs.existsSync(target)) continue;
    fs.symlinkSync(path.join(sourceAppsRoot, entry), target, 'dir');
  }

  const businessOsSource = path.join(sourceAppsRoot, 'business-os');
  const businessOsTarget = path.join(targetAppsRoot, 'business-os');
  if (fs.existsSync(businessOsTarget)) return;
  fs.mkdirSync(businessOsTarget, { recursive: true });
  for (const entry of fs.readdirSync(businessOsSource)) {
    const source = path.join(businessOsSource, entry);
    const target = path.join(businessOsTarget, entry);
    if (entry === 'app.js' || entry === 'index.html') {
      fs.copyFileSync(source, target);
      continue;
    }
    if (entry === 'installed-modules') {
      fs.mkdirSync(target, { recursive: true });
      for (const child of fs.readdirSync(source)) {
        const childSource = path.join(source, child);
        const childTarget = path.join(target, child);
        if (fs.existsSync(childTarget)) continue;
        const childType = fs.statSync(childSource).isDirectory() ? 'dir' : 'file';
        fs.symlinkSync(childSource, childTarget, childType);
      }
      continue;
    }
    const type = fs.statSync(source).isDirectory() ? 'dir' : 'file';
    fs.symlinkSync(source, target, type);
  }
}

function prepareBusinessOsDynamicOpenModuleFixture(fixture) {
  const module = fixture?.module || {};
  const id = String(module.id || '').trim();
  if (!id) throw new Error('dynamic openModule fixture needs a module id');
  const installedModulesRoot = path.join(runtimeRoot, 'runtime/business-os/installed-modules');
  fs.mkdirSync(installedModulesRoot, { recursive: true });
  const targetRealPath = fs.realpathSync(installedModulesRoot);
  const repoRealPath = fs.realpathSync(root);
  if (targetRealPath === repoRealPath || targetRealPath.startsWith(`${repoRealPath}${path.sep}`)) {
    throw new Error('dynamic openModule fixture would write into the real Business OS source tree');
  }

  const moduleRoot = path.join(installedModulesRoot, id);
  fs.mkdirSync(moduleRoot, { recursive: true });
  fs.writeFileSync(path.join(moduleRoot, 'index.html'), '<section data-phase13-open-module-template>Phase 13 openModule guarded DB fixture</section>\n');
  fs.writeFileSync(path.join(moduleRoot, 'index.css'), ':host { display: block; }\n');
  fs.writeFileSync(path.join(moduleRoot, 'schema.js'), 'export const collections = {};\n');
  fs.writeFileSync(path.join(moduleRoot, 'icon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="5" fill="#334155"/><path d="M7 12h10M12 7v10" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>\n');
  fs.writeFileSync(path.join(moduleRoot, 'index.js'), `export async function mount(ctx) {
  const collectionName = 'business_commands';
  const attempts = {};
  const record = async (key, action) => {
    try {
      const result = action();
      if (result && typeof result.exec === 'function') await result.exec();
      attempts[key] = 'allowed';
    } catch (error) {
      attempts[key] = error?.code || error?.name || String(error?.message || error);
    }
  };
  const cachedCollection = ctx.db.collection(collectionName);
  await record('collection', () => ctx.db.collection(collectionName).findOne('phase13_open_module_guard'));
  await record('property', () => ctx.db[collectionName].findOne('phase13_open_module_guard'));
  await record('cached', () => cachedCollection.findOne('phase13_open_module_guard'));
  await record('raw', () => ctx.db.raw[collectionName].findOne('phase13_open_module_guard'));
  const denied = {
    collection: attempts.collection === 'CTOX_BUSINESS_OS_PERMISSION_DENIED',
    property: attempts.property === 'CTOX_BUSINESS_OS_PERMISSION_DENIED',
    cached: attempts.cached === 'CTOX_BUSINESS_OS_PERMISSION_DENIED',
    raw: attempts.raw === 'CTOX_BUSINESS_OS_PERMISSION_DENIED',
  };
  const capabilities = ctx.runtimeCapabilities || {};
  const runtimeSafety = {
    contract: capabilities.version === 'business-os-runtime-capabilities-v1',
    trustModel: capabilities.trust_model === 'same-origin-trusted-generated-app',
    guardedDb: capabilities.database?.guarded === true
      && capabilities.database?.raw === 'guarded-deny-without-data-grant'
      && capabilities.database?.cached_handles === 'guarded-deny-without-data-grant',
    localAssetFetchOnly: capabilities.network?.fetch === 'local-module-assets-only'
      && capabilities.network?.http_business_data === 'forbidden',
    dynamicImportForbidden: capabilities.imports?.dynamic === 'forbidden'
      && capabilities.imports?.bare_package === 'forbidden'
      && capabilities.imports?.remote_url === 'forbidden',
    storageNonAuthoritative: capabilities.storage?.local_storage === 'forbidden'
      && capabilities.storage?.session_storage === 'forbidden'
      && capabilities.storage?.authoritative_permissions === false
      && capabilities.storage?.authoritative_lifecycle === false
      && capabilities.storage?.authoritative_audience === false
      && capabilities.storage?.authoritative_data_grants === false,
    shellGlobalsForbidden: capabilities.shell_state?.global_state_access === 'forbidden'
      && capabilities.shell_state?.global_shell_mutation === 'forbidden',
    workersForbidden: capabilities.workers?.worker === 'forbidden'
      && capabilities.workers?.service_worker === 'forbidden',
    externalEffectsChatOnly: capabilities.external_effects?.direct_control_commands === 'forbidden'
      && Array.isArray(capabilities.external_effects?.allowed_command_bus)
      && capabilities.external_effects.allowed_command_bus.length === 1
      && capabilities.external_effects.allowed_command_bus[0] === 'business_os.chat.task',
  };
  const host = ctx.host || document.body;
  const marker = document.createElement('section');
  marker.dataset.phase13OpenModuleGuard = ctx.module?.id || '';
  marker.textContent = 'Phase 13 openModule guarded DB fixture';
  host.replaceChildren(marker);
  globalThis.__ctoxPhase13OpenModuleGuard = {
    mounted: true,
    moduleId: ctx.module?.id || '',
    attempts,
    denied,
    capabilities,
    runtimeSafety,
  };
  return () => {
    if (globalThis.__ctoxPhase13OpenModuleGuard?.moduleId === ctx.module?.id) {
      delete globalThis.__ctoxPhase13OpenModuleGuard;
    }
  };
}
`);
}

function prepareBusinessOsAgentScopeModuleFixture(fixture) {
  const module = fixture?.module || {};
  const id = String(module.id || '').trim();
  if (!id) throw new Error('agent scope fixture needs a module id');
  const hiddenId = 'phase12-hidden-agent-scope-app';
  const installedModulesRoot = path.join(runtimeRoot, 'runtime/business-os/installed-modules');
  fs.mkdirSync(installedModulesRoot, { recursive: true });
  const targetRealPath = fs.realpathSync(installedModulesRoot);
  const repoRealPath = fs.realpathSync(root);
  if (targetRealPath === repoRealPath || targetRealPath.startsWith(`${repoRealPath}${path.sep}`)) {
    throw new Error('agent scope fixture would write into the real Business OS source tree');
  }

  const writeFixtureIcon = (moduleRoot, label) => {
    fs.writeFileSync(path.join(moduleRoot, 'icon.svg'), `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="${label}">
  <rect width="64" height="64" rx="14" fill="#23665f"/>
  <text x="32" y="39" text-anchor="middle" font-family="system-ui, sans-serif" font-size="22" font-weight="700" fill="#ffffff">${label}</text>
</svg>
`);
  };

  const moduleRoot = path.join(installedModulesRoot, id);
  fs.mkdirSync(moduleRoot, { recursive: true });
  fs.writeFileSync(path.join(moduleRoot, 'index.html'), '<section data-agent-scope-fixture>Phase 12 Agent Scope fixture</section>\n');
  fs.writeFileSync(path.join(moduleRoot, 'index.css'), ':host { display: block; }\n');
  fs.writeFileSync(path.join(moduleRoot, 'schema.js'), 'export const collections = {};\n');
  writeFixtureIcon(moduleRoot, '12');
  fs.writeFileSync(path.join(moduleRoot, 'index.js'), `export async function mount(ctx) {
  const host = ctx.host || document.body;
  const marker = document.createElement('section');
  marker.dataset.agentScopeFixture = ctx.module?.id || '';
  marker.dataset.moduleRoot = ctx.module?.id || '';
  marker.dataset.recordId = 'phase12_agent_scope_record';
  marker.dataset.recordType = 'smoke-record';
  marker.innerHTML = '<h2>Phase 12 Agent Scope App</h2><p data-agent-scope-copy>Agent scope browser fixture</p>';
  host.replaceChildren(marker);
  globalThis.__ctoxAgentScopeFixture = {
    mounted: true,
    moduleId: ctx.module?.id || '',
    recordId: marker.dataset.recordId,
  };
  return () => {
    if (globalThis.__ctoxAgentScopeFixture?.moduleId === ctx.module?.id) {
      delete globalThis.__ctoxAgentScopeFixture;
    }
  };
}
`);

  const hiddenModuleRoot = path.join(installedModulesRoot, hiddenId);
  fs.mkdirSync(hiddenModuleRoot, { recursive: true });
  fs.writeFileSync(path.join(hiddenModuleRoot, 'index.html'), '<section data-agent-scope-hidden-fixture>Phase 12 hidden Agent Scope fixture</section>\n');
  fs.writeFileSync(path.join(hiddenModuleRoot, 'index.css'), ':host { display: block; }\n');
  fs.writeFileSync(path.join(hiddenModuleRoot, 'schema.js'), 'export const collections = {};\n');
  writeFixtureIcon(hiddenModuleRoot, 'H12');
  fs.writeFileSync(path.join(hiddenModuleRoot, 'index.js'), `export async function mount(ctx) {
  const host = ctx.host || document.body;
  const marker = document.createElement('section');
  marker.dataset.agentScopeHiddenFixture = ctx.module?.id || '';
  marker.dataset.moduleRoot = ctx.module?.id || '';
  marker.innerHTML = '<h2>Phase 12 Hidden Agent Scope App</h2><p>Hidden agent scope browser fixture</p>';
  host.replaceChildren(marker);
  return () => {};
}
`);
}

function prepareBusinessOsReleaseModuleFixture(fixture) {
  const module = fixture?.module || {};
  const id = String(module.id || '').trim();
  if (!id) throw new Error('release fixture needs a module id');
  const roots = [
    path.join(runtimeRoot, 'runtime/business-os/installed-modules'),
  ];
  const seen = new Set();
  for (const installedModulesRoot of roots) {
    const normalizedRoot = path.resolve(installedModulesRoot);
    if (seen.has(normalizedRoot)) continue;
    seen.add(normalizedRoot);
    fs.mkdirSync(normalizedRoot, { recursive: true });
    const targetRealPath = fs.realpathSync(normalizedRoot);
    const repoRealPath = fs.realpathSync(root);
    if (targetRealPath === repoRealPath || targetRealPath.startsWith(`${repoRealPath}${path.sep}`)) {
      throw new Error('release fixture would write into the real Business OS source tree');
    }
    writeBusinessOsReleaseModuleFixture(normalizedRoot, id, module);
  }
}

function writeBusinessOsReleaseModuleFixture(installedModulesRoot, id, module) {
  const moduleRoot = path.join(installedModulesRoot, id);
  const collectionName = 'phase10_release_app_records';
  fs.mkdirSync(moduleRoot, { recursive: true });
  fs.mkdirSync(path.join(moduleRoot, 'core'), { recursive: true });
  fs.mkdirSync(path.join(moduleRoot, 'locales'), { recursive: true });
  fs.mkdirSync(path.join(moduleRoot, 'tests'), { recursive: true });
  const moduleRealPath = fs.realpathSync(moduleRoot);
  const repoRealPath = fs.realpathSync(root);
  if (moduleRealPath === repoRealPath || moduleRealPath.startsWith(`${repoRealPath}${path.sep}`)) {
    throw new Error('release fixture module root resolved inside the real Business OS source tree');
  }
  const manifest = {
    ...module,
    id,
    module_id: id,
    source: 'installed',
    install_scope: 'installed',
    entry: `installed-modules/${id}/index.html`,
    version: String(module.version || '0.8.0'),
    icon: 'icon.svg',
    collections: ['business_commands', collectionName],
    launch_kind: 'desktop-app',
    layout: {
      shell: 'windowed',
      left: 'Records',
      center: 'Details',
      default_width: 960,
      default_height: 680,
      min_width: 640,
      min_height: 480,
    },
    presentation: {
      default_mode: 'window',
      supported_modes: ['window', 'maximized', 'focus'],
      initial_size: { width: 960, height: 680 },
      minimum_size: { width: 640, height: 480 },
      multi_instance: false,
      auto_restore: false,
    },
    lifecycle: {
      ...(module.lifecycle || {}),
      runtime_installed: true,
      visibility_state: module.lifecycle?.visibility_state || 'private',
      audience: module.lifecycle?.audience || 'private',
    },
  };
  fs.writeFileSync(path.join(moduleRoot, 'module.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  fs.writeFileSync(path.join(moduleRoot, 'collections.schema.json'), `${JSON.stringify({
    schema_format: 'ctox-business-os-module-collections-v1',
    collections: {
      [collectionName]: {
        version: 0,
        primaryKey: 'id',
        type: 'object',
        properties: {
          id: { type: 'string', maxLength: 120 },
          title: { type: 'string' },
          updated_at_ms: { type: 'number' },
        },
        required: ['id', 'title', 'updated_at_ms'],
      },
    },
  }, null, 2)}\n`);
  fs.writeFileSync(path.join(moduleRoot, 'schema.js'), `export const collections = {
  ${collectionName}: {
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 120 },
      title: { type: 'string' },
      updated_at_ms: { type: 'number' },
    },
    required: ['id', 'title', 'updated_at_ms'],
  },
};
`);
  fs.writeFileSync(path.join(moduleRoot, 'core/automation.mjs'), `export function buildFollowUpCommand(record = {}) {
  return {
    module: '${id}',
    type: 'business_os.chat.task',
    command_type: 'business_os.chat.task',
    record_id: record.id || 'demo',
    payload: {
      title: \`Review \${record.title || 'record'}\`,
      instruction: \`Review \${record.title || 'record'} and continue the normal CTOX workflow.\`,
      record_snapshot: record,
    },
    client_context: { source: '${id}', collection: '${collectionName}' },
  };
}
`);
  fs.writeFileSync(path.join(moduleRoot, 'core/records.mjs'), `export function visibleRecords(records = []) {
  return records.filter((record) => !record.is_deleted);
}

export function summarizeRecords(records = []) {
  return { total: visibleRecords(records).length };
}
`);
  fs.writeFileSync(path.join(moduleRoot, 'index.html'), `<main class="phase10-release-app ctox-pane" data-phase10-release-app="${id}">
  <header class="ctox-pane-header">
    <span class="ctox-pane-icon" aria-hidden="true">10</span>
    <h1>Phase 10 Release App</h1>
  </header>
  <section class="ctox-fields" data-list></section>
  <button class="ctox-button" type="button" data-action="create-record">Create record</button>
  <button class="ctox-button" type="button" data-action="follow-up">Follow up</button>
</main>
`);
  fs.writeFileSync(path.join(moduleRoot, 'index.css'), `.phase10-release-app {
  display: grid;
  gap: 0.75rem;
  min-height: 100%;
  padding: 1rem;
  background: var(--surface);
  color: var(--text);
}
`);
  fs.writeFileSync(
    path.join(moduleRoot, 'icon.svg'),
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="5" fill="#0f766e"/><path d="M7 12.5 10.5 16 17 8" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>\n'
  );
  fs.writeFileSync(path.join(moduleRoot, 'locales/de.json'), `${JSON.stringify({ title: 'Phase 10 Release App' }, null, 2)}\n`);
  fs.writeFileSync(path.join(moduleRoot, 'locales/en.json'), `${JSON.stringify({ title: 'Phase 10 Release App' }, null, 2)}\n`);
  fs.writeFileSync(path.join(moduleRoot, 'index.js'), `import { buildFollowUpCommand } from './core/automation.mjs';

function attachStylesheetOnce() {
  if (document.querySelector('link[data-module-styles="${id}"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.moduleStyles = '${id}';
  document.head.append(link);
}

export async function mount(ctx) {
  attachStylesheetOnce();
  const host = ctx.host || document.body;
  host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const records = ctx.db.collection('${collectionName}');
  host.querySelector('[data-action="create-record"]')?.addEventListener('click', () => {
    records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() });
  });
  host.querySelector('[data-action="follow-up"]')?.addEventListener('click', () => {
    ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 }));
  });
  return () => { host.innerHTML = ''; };
}
`);
  fs.writeFileSync(path.join(moduleRoot, 'tests/basic.test.mjs'), `import assert from 'node:assert/strict';
import { buildFollowUpCommand } from '../core/automation.mjs';
import { summarizeRecords, visibleRecords } from '../core/records.mjs';

const record = { id: 'demo', title: 'Demo', updated_at_ms: 1 };
assert.equal(visibleRecords([record]).length, 1);
assert.equal(summarizeRecords([record]).total, 1);
const command = buildFollowUpCommand(record);
assert.equal(command.type, 'business_os.chat.task');
assert.equal(command.command_type, 'business_os.chat.task');
assert.deepEqual(command.payload.record_snapshot, record);
`);
}

async function seedBusinessOsReleaseNativeSetup() {
  const now = Date.now();
  const moduleRoot = path.join(runtimeRoot, 'runtime/business-os/installed-modules/phase10-release-app');
  const bundle = computeBusinessOsReleaseModuleBundle(moduleRoot);
  fs.mkdirSync(path.dirname(nativeBusinessOsSqlitePath), { recursive: true });
  sqlite(`
    CREATE TABLE IF NOT EXISTS business_users (
      user_id TEXT PRIMARY KEY,
      display_name TEXT NOT NULL,
      role TEXT NOT NULL CHECK(role IN ('chef', 'admin', 'founder', 'user')),
      active INTEGER NOT NULL DEFAULT 1,
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS business_module_acl (
      module_id TEXT NOT NULL,
      user_id TEXT NOT NULL,
      role TEXT NOT NULL CHECK(role IN ('founder')),
      active INTEGER NOT NULL DEFAULT 1,
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL,
      PRIMARY KEY(module_id, user_id, role)
    );
    CREATE INDEX IF NOT EXISTS idx_business_module_acl_user
      ON business_module_acl(user_id, active, module_id);

    CREATE TABLE IF NOT EXISTS business_permission_grants (
      grant_id TEXT PRIMARY KEY,
      subject_type TEXT NOT NULL CHECK(subject_type IN ('user', 'role')),
      subject_id TEXT NOT NULL,
      permission TEXT NOT NULL,
      scope_type TEXT NOT NULL CHECK(scope_type IN ('workspace', 'module', 'collection', 'record', 'task', 'approval', 'mcp')),
      scope_id TEXT NOT NULL DEFAULT '',
      active INTEGER NOT NULL DEFAULT 1,
      reason TEXT NOT NULL DEFAULT '',
      created_by TEXT NOT NULL DEFAULT '',
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_business_permission_grants_subject
      ON business_permission_grants(subject_type, subject_id, active);
    CREATE INDEX IF NOT EXISTS idx_business_permission_grants_scope
      ON business_permission_grants(permission, scope_type, scope_id, active);

    CREATE TABLE IF NOT EXISTS business_module_versions (
      version_id TEXT PRIMARY KEY,
      module_id TEXT NOT NULL,
      seq INTEGER NOT NULL,
      origin TEXT NOT NULL,
      label TEXT NOT NULL DEFAULT '',
      bundle_sha256 TEXT NOT NULL,
      files_json TEXT NOT NULL DEFAULT '[]',
      sealed INTEGER NOT NULL DEFAULT 0,
      created_by TEXT NOT NULL DEFAULT '',
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_business_module_versions_module
      ON business_module_versions(module_id, seq DESC);

    INSERT INTO business_users
      (user_id, display_name, role, active, created_at_ms, updated_at_ms)
    VALUES
      ('local-dev', 'Local CTOX', 'admin', 1, ${now}, ${now}),
      ('release_owner', 'Release Owner', 'founder', 1, ${now}, ${now}),
      ('team_member', 'Team Member', 'user', 1, ${now}, ${now})
    ON CONFLICT(user_id) DO UPDATE SET
      display_name = excluded.display_name,
      role = excluded.role,
      active = excluded.active,
      updated_at_ms = excluded.updated_at_ms;

    INSERT INTO business_module_acl
      (module_id, user_id, role, active, created_at_ms, updated_at_ms)
    VALUES
      ('phase10-release-app', 'release_owner', 'founder', 1, ${now}, ${now})
    ON CONFLICT(module_id, user_id, role) DO UPDATE SET
      active = excluded.active,
      updated_at_ms = excluded.updated_at_ms;

    INSERT INTO business_module_versions
      (version_id, module_id, seq, origin, label, bundle_sha256, files_json,
       sealed, created_by, created_at_ms, updated_at_ms)
    VALUES
      ('modver_phase10_release_app_install_1', 'phase10-release-app', 1, 'install', 'Installed',
       '${sqlString(bundle.sha256)}', '${sqlString(JSON.stringify(bundle.files))}',
       1, 'release_owner', ${now}, ${now})
    ON CONFLICT(version_id) DO UPDATE SET
      bundle_sha256 = excluded.bundle_sha256,
      files_json = excluded.files_json,
      sealed = excluded.sealed,
      updated_at_ms = excluded.updated_at_ms;
  `, nativeBusinessOsSqlitePath);
}

function seedBusinessOsFreshProfileScaleNativeSetup() {
  const now = Date.now();
  const moduleCount = 32;
  const versionsPerModule = 3;
  const auditEventCount = 128;
  const moduleIds = Array.from({ length: moduleCount }, (_, index) =>
    `phase14-scale-app-${String(index + 1).padStart(2, '0')}`);
  const statements = [];
  fs.mkdirSync(path.dirname(nativeBusinessOsSqlitePath), { recursive: true });
  statements.push(`
    CREATE TABLE IF NOT EXISTS business_events (
      event_id TEXT PRIMARY KEY,
      collection TEXT NOT NULL,
      record_id TEXT NOT NULL,
      command_type TEXT NOT NULL,
      payload_json TEXT NOT NULL,
      observed_at_ms INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_business_events_record
      ON business_events(collection, record_id, observed_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_business_events_type
      ON business_events(command_type, observed_at_ms DESC);

    CREATE TABLE IF NOT EXISTS business_users (
      user_id TEXT PRIMARY KEY,
      display_name TEXT NOT NULL,
      role TEXT NOT NULL CHECK(role IN ('chef', 'admin', 'founder', 'user')),
      active INTEGER NOT NULL DEFAULT 1,
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS business_module_acl (
      module_id TEXT NOT NULL,
      user_id TEXT NOT NULL,
      role TEXT NOT NULL CHECK(role IN ('founder')),
      active INTEGER NOT NULL DEFAULT 1,
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL,
      PRIMARY KEY(module_id, user_id, role)
    );
    CREATE INDEX IF NOT EXISTS idx_business_module_acl_user
      ON business_module_acl(user_id, active, module_id);

    CREATE TABLE IF NOT EXISTS business_permission_grants (
      grant_id TEXT PRIMARY KEY,
      subject_type TEXT NOT NULL CHECK(subject_type IN ('user', 'role')),
      subject_id TEXT NOT NULL,
      permission TEXT NOT NULL,
      scope_type TEXT NOT NULL CHECK(scope_type IN ('workspace', 'module', 'collection', 'record', 'task', 'approval', 'mcp')),
      scope_id TEXT NOT NULL DEFAULT '',
      active INTEGER NOT NULL DEFAULT 1,
      reason TEXT NOT NULL DEFAULT '',
      created_by TEXT NOT NULL DEFAULT '',
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_business_permission_grants_subject
      ON business_permission_grants(subject_type, subject_id, active);
    CREATE INDEX IF NOT EXISTS idx_business_permission_grants_scope
      ON business_permission_grants(permission, scope_type, scope_id, active);

    CREATE TABLE IF NOT EXISTS business_module_versions (
      version_id TEXT PRIMARY KEY,
      module_id TEXT NOT NULL,
      seq INTEGER NOT NULL,
      origin TEXT NOT NULL,
      label TEXT NOT NULL DEFAULT '',
      bundle_sha256 TEXT NOT NULL,
      files_json TEXT NOT NULL DEFAULT '[]',
      sealed INTEGER NOT NULL DEFAULT 0,
      created_by TEXT NOT NULL DEFAULT '',
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_business_module_versions_module
      ON business_module_versions(module_id, seq DESC);

    INSERT INTO business_users
      (user_id, display_name, role, active, created_at_ms, updated_at_ms)
    VALUES
      ('fresh_builder', 'Fresh Builder', 'founder', 1, ${now}, ${now}),
      ('fresh_team_member', 'Fresh Team', 'user', 1, ${now}, ${now})
    ON CONFLICT(user_id) DO UPDATE SET
      display_name = excluded.display_name,
      role = excluded.role,
      active = excluded.active,
      updated_at_ms = excluded.updated_at_ms;
  `);
  for (const [moduleIndex, moduleId] of moduleIds.entries()) {
    statements.push(`
      INSERT INTO business_module_acl
        (module_id, user_id, role, active, created_at_ms, updated_at_ms)
      VALUES
        ('${sqlString(moduleId)}', 'fresh_builder', 'founder', 1, ${now}, ${now})
      ON CONFLICT(module_id, user_id, role) DO UPDATE SET
        active = excluded.active,
        updated_at_ms = excluded.updated_at_ms;
    `);
    const grantRows = [
      [`phase14_scale_${moduleIndex + 1}_apps_view`, 'fresh_team_member', 'apps.view', 'module', moduleId],
      [`phase14_scale_${moduleIndex + 1}_data_read`, 'fresh_team_member', 'data.read', 'collection', 'business_commands'],
    ];
    for (const [grantId, subjectId, permission, scopeType, scopeId] of grantRows) {
      statements.push(`
        INSERT INTO business_permission_grants
          (grant_id, subject_type, subject_id, permission, scope_type, scope_id,
           active, reason, created_by, created_at_ms, updated_at_ms)
        VALUES
          ('${sqlString(grantId)}', 'user', '${sqlString(subjectId)}', '${sqlString(permission)}',
           '${sqlString(scopeType)}', '${sqlString(scopeId)}', 1,
           'Phase 14 scale fixture', 'fresh_builder', ${now}, ${now})
        ON CONFLICT(grant_id) DO UPDATE SET
          active = excluded.active,
          updated_at_ms = excluded.updated_at_ms;
      `);
    }
    for (let seq = 1; seq <= versionsPerModule; seq += 1) {
      const versionId = `${moduleId}-v${seq}`;
      const fileList = JSON.stringify([{ path: 'index.js', sha256: `scale-${moduleIndex + 1}-${seq}` }]);
      statements.push(`
        INSERT INTO business_module_versions
          (version_id, module_id, seq, origin, label, bundle_sha256, files_json,
           sealed, created_by, created_at_ms, updated_at_ms)
        VALUES
          ('${sqlString(versionId)}', '${sqlString(moduleId)}', ${seq}, '${seq === 1 ? 'install' : 'manual_release'}',
           'Scale Version ${seq}',
           '${'f'.repeat(56)}${String(moduleIndex + 1).padStart(4, '0')}${String(seq).padStart(4, '0')}',
           '${sqlString(fileList)}', 1, 'fresh_builder', ${now - ((versionsPerModule - seq) * 1000)}, ${now})
        ON CONFLICT(version_id) DO UPDATE SET
          bundle_sha256 = excluded.bundle_sha256,
          files_json = excluded.files_json,
          sealed = excluded.sealed,
          updated_at_ms = excluded.updated_at_ms;
      `);
    }
  }
  for (let index = 0; index < auditEventCount; index += 1) {
    const moduleId = moduleIds[index % moduleIds.length];
    const payload = JSON.stringify({
      schema: 'ctox.business_os.scale_audit_fixture.v1',
      module_id: moduleId,
      action: index % 2 === 0 ? 'release' : 'data-review',
    });
    statements.push(`
      INSERT INTO business_events
        (event_id, collection, record_id, command_type, payload_json, observed_at_ms)
      VALUES
        ('phase14_scale_event_${String(index + 1).padStart(3, '0')}',
         'business_module_catalog', '${sqlString(moduleId)}',
         'ctox.module.${index % 2 === 0 ? 'release' : 'review_data_access'}',
         '${sqlString(payload)}', ${now - index})
      ON CONFLICT(event_id) DO UPDATE SET
        payload_json = excluded.payload_json,
        observed_at_ms = excluded.observed_at_ms;
    `);
  }
  sqlite(`BEGIN IMMEDIATE;\n${statements.join('\n')}\nCOMMIT;`, nativeBusinessOsSqlitePath);
  return {
    moduleCount,
    versionsPerModule,
    permissionGrants: Number(sqlite(
      "SELECT COUNT(*) FROM business_permission_grants WHERE grant_id LIKE 'phase14_scale_%';",
      nativeBusinessOsSqlitePath,
    ).trim() || 0),
    moduleVersions: Number(sqlite(
      "SELECT COUNT(*) FROM business_module_versions WHERE version_id LIKE 'phase14-scale-app-%';",
      nativeBusinessOsSqlitePath,
    ).trim() || 0),
    auditEvents: Number(sqlite(
      "SELECT COUNT(*) FROM business_events WHERE event_id LIKE 'phase14_scale_event_%';",
      nativeBusinessOsSqlitePath,
    ).trim() || 0),
  };
}

async function seedBusinessOsRolesPermissionsNativeUsers() {
  const now = Date.now();
  fs.mkdirSync(path.dirname(nativeBusinessOsSqlitePath), { recursive: true });
  sqlite(`
    CREATE TABLE IF NOT EXISTS business_users (
      user_id TEXT PRIMARY KEY,
      display_name TEXT NOT NULL,
      role TEXT NOT NULL CHECK(role IN ('chef', 'admin', 'founder', 'user')),
      active INTEGER NOT NULL DEFAULT 1,
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );

    INSERT INTO business_users
      (user_id, display_name, role, active, created_at_ms, updated_at_ms)
    VALUES
      ('owner_ui', 'Owner UI', 'chef', 1, ${now}, ${now}),
      ('team_member', 'Team Member', 'user', 1, ${now}, ${now}),
      ('source_viewer', 'Source Viewer', 'user', 1, ${now}, ${now}),
      ('app_modifier', 'App Modifier', 'user', 1, ${now}, ${now}),
      ('founder_ui', 'Founder UI', 'founder', 1, ${now}, ${now})
    ON CONFLICT(user_id) DO UPDATE SET
      display_name = excluded.display_name,
      role = excluded.role,
      active = excluded.active,
      updated_at_ms = excluded.updated_at_ms;

  `, nativeBusinessOsSqlitePath);
}

async function seedBusinessOsThreadsRightClickNativeUsers() {
  const now = Date.now();
  fs.mkdirSync(path.dirname(nativeBusinessOsSqlitePath), { recursive: true });
  sqlite(`
    CREATE TABLE IF NOT EXISTS business_users (
      user_id TEXT PRIMARY KEY,
      display_name TEXT NOT NULL,
      role TEXT NOT NULL CHECK(role IN ('chef', 'admin', 'founder', 'user')),
      active INTEGER NOT NULL DEFAULT 1,
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS business_permission_grants (
      grant_id TEXT PRIMARY KEY,
      subject_type TEXT NOT NULL CHECK(subject_type IN ('user', 'role')),
      subject_id TEXT NOT NULL,
      permission TEXT NOT NULL,
      scope_type TEXT NOT NULL CHECK(scope_type IN ('workspace', 'module', 'collection', 'record', 'task', 'approval', 'mcp')),
      scope_id TEXT NOT NULL DEFAULT '',
      active INTEGER NOT NULL DEFAULT 1,
      reason TEXT NOT NULL DEFAULT '',
      created_by TEXT NOT NULL DEFAULT '',
      created_at_ms INTEGER NOT NULL,
      updated_at_ms INTEGER NOT NULL
    );

    INSERT INTO business_users
      (user_id, display_name, role, active, created_at_ms, updated_at_ms)
    VALUES
      ('local-dev', 'Local CTOX', 'admin', 1, ${now}, ${now}),
      ('threads-requester', 'Threads Requester', 'user', 1, ${now}, ${now}),
      ('threads-reviewer', 'Threads Reviewer', 'admin', 1, ${now}, ${now})
    ON CONFLICT(user_id) DO UPDATE SET
      display_name = excluded.display_name,
      role = excluded.role,
      active = excluded.active,
      updated_at_ms = excluded.updated_at_ms;

    INSERT INTO business_permission_grants
      (grant_id, subject_type, subject_id, permission, scope_type, scope_id,
       active, reason, created_by, created_at_ms, updated_at_ms)
    VALUES
      ('threads_rightclick_notes_read', 'user', 'threads-requester', 'data.read',
       'module', 'notes', 1, 'right-click smoke read-only source access',
       'browser-rust-smoke', ${now}, ${now})
    ON CONFLICT(grant_id) DO UPDATE SET
      subject_type = excluded.subject_type,
      subject_id = excluded.subject_id,
      permission = excluded.permission,
      scope_type = excluded.scope_type,
      scope_id = excluded.scope_id,
      active = excluded.active,
      reason = excluded.reason,
      updated_at_ms = excluded.updated_at_ms;
  `, nativeBusinessOsSqlitePath);
}

function issueBusinessOsSmokeCapability(userId) {
  const result = spawnSync(
    ctoxBin,
    ['business-os', 'auth', 'issue-capability', '--user', userId],
    {
      cwd: root,
      env: { ...process.env, CTOX_ROOT: runtimeRoot },
      encoding: 'utf8',
      timeout: 30000,
    },
  );
  if (result.error || result.status !== 0) {
    throw new Error(`failed to issue smoke capability for ${userId}: ${result.error?.message || result.stderr || result.status}`);
  }
  const payload = JSON.parse(String(result.stdout || '{}'));
  const token = String(payload.capability_token || '').trim();
  if (!token) throw new Error(`smoke capability response for ${userId} did not include a token`);
  return {
    token,
    expiresAtMs: Number(payload.expires_at_ms) || Date.now() + 60 * 60 * 1000,
  };
}

async function seedBusinessOsThreadsScaleNativeSetup() {
  const fixtureCount = 10000;
  const now = Date.now() - 60000;
  const tables = {
    commands: 'ctox_business_os__business_commands__v1',
    threads: 'ctox_business_os__user_threads__v0',
    messages: 'ctox_business_os__user_thread_messages__v0',
    notifications: 'ctox_business_os__user_notifications__v0',
  };
  await waitForSqliteTables(Object.values(tables), 60000);

  const fixtures = [
    {
      table: tables.commands,
      prefix: 'threads_scale_cmd_',
      make(index, id, timestamp) {
        return {
          id,
          command_id: id,
          module: 'threads-scale-history',
          command_type: 'ctox.scale.history',
          record_id: `threads_scale_record_${index}`,
          status: 'completed',
          inbound_channel: 'production-smoke',
          payload: { fixture: true, index },
          client_context: { actor: { id: 'local-dev', role: 'user' } },
          result: { fixture: true },
          updated_at_ms: timestamp,
        };
      },
    },
    {
      table: tables.threads,
      prefix: 'threads_scale_thread_',
      make(index, id, timestamp) {
        return {
          id,
          thread_id: id,
          title: `Historical thread ${index}`,
          kind: 'history',
          status: 'completed',
          participant_ids: ['local-dev', 'threads-requester'],
          watcher_user_ids: [],
          owner_user_id: 'local-dev',
          created_by_id: 'local-dev',
          source_module: 'threads-scale-history',
          source_record_type: 'scale-fixture',
          source_record_id: `threads_scale_record_${index}`,
          last_message_id: `threads_scale_message_${String(index).padStart(5, '0')}`,
          last_message_at_ms: timestamp,
          pending_approval_count: 0,
          created_at_ms: timestamp,
          updated_at_ms: timestamp,
        };
      },
    },
    {
      table: tables.messages,
      prefix: 'threads_scale_message_',
      make(index, id, timestamp) {
        return {
          id,
          message_id: id,
          thread_id: `threads_scale_thread_${String(index).padStart(5, '0')}`,
          kind: 'note',
          author_user_id: 'local-dev',
          author_display_name: 'Local CTOX',
          target_user_ids: [],
          body: `Historical message ${index}`,
          source_module: 'threads-scale-history',
          source_record_type: 'scale-fixture',
          source_record_id: `threads_scale_record_${index}`,
          command_id: `threads_scale_cmd_${String(index).padStart(5, '0')}`,
          created_at_ms: timestamp,
          updated_at_ms: timestamp,
        };
      },
    },
    {
      table: tables.notifications,
      prefix: 'threads_scale_notification_',
      make(index, id, timestamp) {
        return {
          id,
          notification_id: id,
          user_id: 'local-dev',
          thread_id: `threads_scale_thread_${String(index).padStart(5, '0')}`,
          message_id: `threads_scale_message_${String(index).padStart(5, '0')}`,
          notification_type: 'history',
          status: 'read',
          title: `Historical notification ${index}`,
          body_preview: `Historical message ${index}`,
          source_module: 'threads-scale-history',
          source_record_id: `threads_scale_record_${index}`,
          created_at_ms: timestamp,
          updated_at_ms: timestamp,
        };
      },
    },
  ];

  for (const fixture of fixtures) {
    for (let start = 1; start <= fixtureCount; start += 250) {
      const rows = [];
      const end = Math.min(fixtureCount, start + 249);
      for (let index = start; index <= end; index += 1) {
        const suffix = String(index).padStart(5, '0');
        const id = `${fixture.prefix}${suffix}`;
        const timestamp = now - (fixtureCount - index);
        const document = {
          ...fixture.make(index, id, timestamp),
          _deleted: false,
          is_deleted: false,
          _meta: { lwt: timestamp },
          _attachments: {},
          _rev: '1-threads-scale-smoke',
        };
        rows.push(`(
          '${sqlString(id)}',
          '1-threads-scale-smoke',
          0,
          ${timestamp},
          '${sqlString(JSON.stringify(document))}'
        )`);
      }
      sqlite(`
        BEGIN IMMEDIATE;
        INSERT OR REPLACE INTO ${quoteSqlIdentifier(fixture.table)}
          (id, revision, deleted, lastWriteTime, data)
        VALUES ${rows.join(',\n')};
        COMMIT;
      `);
    }
  }

  return {
    commands: sqliteRowCount(tables.commands, "id LIKE 'threads_scale_cmd_%'"),
    threads: sqliteRowCount(tables.threads, "id LIKE 'threads_scale_thread_%'"),
    messages: sqliteRowCount(tables.messages, "id LIKE 'threads_scale_message_%'"),
    notifications: sqliteRowCount(tables.notifications, "id LIKE 'threads_scale_notification_%'"),
  };
}

function computeBusinessOsReleaseModuleBundle(moduleRoot) {
  const files = [];
  const allowed = new Set(['css', 'html', 'js', 'json', 'md', 'mjs', 'ts', 'svg']);
  const walk = (dir, prefix = '') => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
      const abs = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(abs, rel);
        continue;
      }
      if (!entry.isFile()) continue;
      const ext = path.extname(entry.name).slice(1).toLowerCase();
      if (!allowed.has(ext)) continue;
      const content = fs.readFileSync(abs, 'utf8');
      files.push({
        path: rel,
        sha256: crypto.createHash('sha256').update(content).digest('hex'),
        content,
      });
    }
  };
  walk(moduleRoot);
  files.sort((left, right) => left.path.localeCompare(right.path));
  const digestInput = files.map((file) => `${file.path}\0${file.sha256}\n`).join('');
  return {
    files,
    sha256: crypto.createHash('sha256').update(digestInput).digest('hex'),
  };
}

function removeSmokePath(targetPath) {
  if (keepSmokeArtifacts || !targetPath) return;
  try {
    fs.rmSync(targetPath, {
      recursive: true,
      force: true,
      maxRetries: 3,
      retryDelay: 100,
    });
  } catch {
    // Best-effort cleanup only; smoke correctness is reported by the test assertions.
  }
}

function encodeFrame(text) {
  const payload = Buffer.from(text);
  let header;
  if (payload.length < 126) {
    header = Buffer.from([0x81, payload.length]);
  } else if (payload.length < 65536) {
    header = Buffer.alloc(4);
    header[0] = 0x81;
    header[1] = 126;
    header.writeUInt16BE(payload.length, 2);
  } else {
    header = Buffer.alloc(10);
    header[0] = 0x81;
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(payload.length), 2);
  }
  return Buffer.concat([header, payload]);
}

function tryDecodeFrame(buffer) {
  if (buffer.length < 2) return null;
  const opcode = buffer[0] & 0x0f;
  let len = buffer[1] & 0x7f;
  let offset = 2;
  if (len === 126) {
    if (buffer.length < 4) return null;
    len = buffer.readUInt16BE(2);
    offset = 4;
  } else if (len === 127) {
    if (buffer.length < 10) return null;
    const big = buffer.readBigUInt64BE(2);
    if (big > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('frame too large');
    len = Number(big);
    offset = 10;
  }
  const masked = (buffer[1] & 0x80) !== 0;
  const maskOffset = offset;
  if (masked) offset += 4;
  if (buffer.length < offset + len) return null;
  let payload = buffer.subarray(offset, offset + len);
  if (masked) {
    const mask = buffer.subarray(maskOffset, maskOffset + 4);
    const unmasked = Buffer.alloc(len);
    for (let i = 0; i < len; i++) unmasked[i] = payload[i] ^ mask[i % 4];
    payload = unmasked;
  }
  return { opcode, text: payload.toString('utf8'), rest: buffer.subarray(offset + len) };
}

async function startSignalingServer() {
  if (process.env.SMOKE_INLINE_SIGNALING !== '1' && smokeMode !== 'signaling-error-browser-status') {
    return startExternalSignalingServer();
  }
  return startInlineSignalingServer();
}

async function startExternalSignalingServer() {
  setSmokeStartupPhase('signaling-start');
  const script = path.join(root, 'src/core/rxdb/tools/local_signaling_server.js');
  const startupWaitMs = Number(process.env.SMOKE_SIGNALING_START_WAIT_MS || '20000');
  const child = trackSmokeChild(spawn(process.execPath, [script, String(signalingPort)], {
    cwd: root,
    env: {
      ...process.env,
      SIGNALING_HOST: '127.0.0.1',
      SIGNALING_PORT: String(signalingPort),
      CTOX_SMOKE_RUN_ID: smokeRunId,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  }), 'signaling-server');
  child.__ctoxExternalSignaling = true;
  let resolveListening;
  let rejectListening;
  let sawListening = false;
  child.__ctoxListening = new Promise((resolve, reject) => {
    resolveListening = resolve;
    rejectListening = reject;
  });
  child.stdout.on('data', (d) => {
    const text = d.toString();
    if (signalingDebug) process.stdout.write(`[signaling] ${d}`);
    if (text.includes('CTOX RxDB signaling listening')) {
      sawListening = true;
      resolveListening?.();
    }
  });
  child.stderr.on('data', (d) => process.stderr.write(`[signaling:err] ${d}`));
  child.on('exit', (code, signal) => {
    if (signalingDebug) console.error(`[signaling:exit] code=${code} signal=${signal}`);
    if (!sawListening) rejectListening?.(new Error(`signaling server exited before listening: code=${code} signal=${signal}`));
  });
  try {
    await waitForChildReady(child.__ctoxListening, startupWaitMs, 'signaling server');
    await waitForTcpPort('127.0.0.1', signalingPort, startupWaitMs, child);
    setSmokeStartupPhase('signaling-ready');
    return child;
  } catch (error) {
    terminateOwnedSmokeChild(child, 'SIGTERM', 'signaling-startup', 'startup-failed');
    await new Promise((resolve) => {
      const timeout = setTimeout(resolve, 1000);
      child.once('exit', () => {
        clearTimeout(timeout);
        resolve();
      });
    });
    if (child.exitCode === null && child.signalCode === null) {
      terminateOwnedSmokeChild(child, 'SIGKILL', 'signaling-startup', 'startup-failed-timeout');
    }
    throw error;
  }
}

function waitForChildReady(promise, timeoutMs, label) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout waiting for ${label} startup output`)), timeoutMs);
    Promise.resolve(promise)
      .then(() => {
        clearTimeout(timer);
        resolve();
      })
      .catch((error) => {
        clearTimeout(timer);
        reject(error);
      });
  });
}

function waitForTcpPort(host, port, timeoutMs, child) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const check = () => {
      if (child && (child.exitCode !== null || child.signalCode !== null)) {
        reject(new Error(`signaling server exited before listening: code=${child.exitCode} signal=${child.signalCode}`));
        return;
      }
      const socket = net.createConnection({ host, port });
      let settled = false;
      socket.once('connect', () => {
        settled = true;
        socket.end();
        resolve();
      });
      socket.once('error', () => {
        if (settled) return;
        socket.destroy();
        if (Date.now() >= deadline) {
          reject(new Error(`timeout waiting for signaling server on ${host}:${port}`));
        } else {
          setTimeout(check, 50);
        }
      });
    };
    check();
  });
}

function startInlineSignalingServer() {
  const peers = new Map();
  const rooms = new Map();
  const sockets = new Set();
  const knownRoles = new Set(['browser', 'ctox_instance', 'desktop_shell', 'desktop_terminal', 'ctox_desktop_app']);

  function metadataFromHandshake(header) {
    const requestLine = header.split('\r\n')[0] || '';
    const target = requestLine.split(' ')[1] || '/';
    try {
      const url = new URL(target, 'ws://local');
      const client = (url.searchParams.get('client') || '').trim();
      const role = normalizeRole(url.searchParams.get('role') || url.searchParams.get('peer_role') || '', client);
      return {
        client,
        role,
        instanceId: (url.searchParams.get('instance_id') || url.searchParams.get('instance') || '').trim(),
        protocol: (url.searchParams.get('protocol') || '').trim(),
        capabilities: parseCapabilities(url),
      };
    } catch {
      return { client: '', role: 'unknown', instanceId: '', protocol: '', capabilities: [] };
    }
  }

  function normalizeRole(value, client) {
    const role = String(value || '').trim();
    if (knownRoles.has(role)) return role;
    const normalizedClient = String(client || '').toLowerCase();
    if (normalizedClient.includes('business') || normalizedClient.includes('browser')) return 'browser';
    if (normalizedClient.includes('ctox')) return 'ctox_instance';
    if (normalizedClient.includes('desktop')) return 'desktop_shell';
    return 'unknown';
  }

  function parseCapabilities(url) {
    return [
      ...url.searchParams.getAll('cap'),
      ...url.searchParams.getAll('capability'),
      ...url.searchParams.getAll('capabilities'),
    ]
      .join(',')
      .split(/[,\s]+/)
      .map((entry) => entry.trim())
      .filter(Boolean);
  }

  function peerSummary(peer) {
    if (!peer) return null;
    return {
      peerId: peer.id,
      role: peer.role || 'unknown',
      protocol: peer.protocol || '',
      instanceId: peer.instanceId || '',
      client: peer.client || '',
      capabilities: Array.isArray(peer.capabilities) ? peer.capabilities : [],
    };
  }

  const server = net.createServer((socket) => {
    sockets.add(socket);
    let handshake = false;
    let buffer = Buffer.alloc(0);
    let peer = null;

    function send(message) {
      if (!socket.destroyed) socket.write(encodeFrame(JSON.stringify(message)));
    }

    function joined(roomId) {
      const room = rooms.get(roomId) || new Set();
      const otherPeerIds = Array.from(room);
      const peerDescriptors = otherPeerIds.map((id) => peerSummary(peers.get(id))).filter(Boolean);
      if (signalingDebug) console.error(`[smoke-signaling] joined room=${roomId} peers=${room.size}`);
      for (const id of room) {
        peers.get(id)?.send({ type: 'joined', otherPeerIds, peers: peerDescriptors });
      }
    }

    function disconnect() {
      if (!peer) return;
      for (const roomId of peer.rooms) {
        const room = rooms.get(roomId);
        room?.delete(peer.id);
        if (room && room.size === 0) rooms.delete(roomId);
        else joined(roomId);
      }
      peers.delete(peer.id);
      peer = null;
    }

    socket.on('data', (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);
      if (!handshake) {
        const headerEnd = buffer.indexOf('\r\n\r\n');
        if (headerEnd === -1) return;
        const header = buffer.subarray(0, headerEnd).toString('utf8');
        const key = /^Sec-WebSocket-Key: (.+)$/im.exec(header)?.[1]?.trim();
        if (!key) return socket.destroy();
        const accept = crypto
          .createHash('sha1')
          .update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11')
          .digest('base64');
        socket.write([
          'HTTP/1.1 101 Switching Protocols',
          'Upgrade: websocket',
          'Connection: Upgrade',
          `Sec-WebSocket-Accept: ${accept}`,
          '\r\n',
        ].join('\r\n'));
        handshake = true;
        buffer = buffer.subarray(headerEnd + 4);
        peer = {
          id: token(),
          ...metadataFromHandshake(header),
          rooms: new Set(),
          send,
          injectedControlPlaneError: false,
        };
        peers.set(peer.id, peer);
        if (signalingDebug) console.error(`[smoke-signaling] open peer=${peer.id} role=${peer.role} instance=${peer.instanceId || '-'} protocol=${peer.protocol || '-'} caps=${peer.capabilities.join(',') || '-'}`);
        send({ type: 'init', yourPeerId: peer.id, peer: peerSummary(peer) });
      }

      while (true) {
        const decoded = tryDecodeFrame(buffer);
        if (!decoded) break;
        buffer = decoded.rest;
        if (decoded.opcode === 8) {
          socket.end();
          break;
        }
        if (decoded.opcode !== 1) continue;
        let msg;
        try {
          msg = JSON.parse(decoded.text);
        } catch {
          socket.destroy();
          break;
        }
        if (msg.type === 'join') {
          if (typeof msg.room !== 'string' || msg.room.length <= 5 || msg.room.length >= 512) {
            socket.destroy();
            break;
          }
          peer.rooms.add(msg.room);
          if (!rooms.has(msg.room)) rooms.set(msg.room, new Set());
          rooms.get(msg.room).add(peer.id);
          if (signalingDebug) console.error(`[smoke-signaling] join peer=${peer.id} room=${msg.room} size=${rooms.get(msg.room).size}`);
          if (smokeMode === 'signaling-error-browser-status' && !peer.injectedControlPlaneError) {
            peer.injectedControlPlaneError = true;
            send({
              type: 'ctoxError',
              scope: 'control-plane',
              code: 'instance_mismatch',
              reason: 'smoke injected control-plane instance mismatch',
            });
          }
          joined(msg.room);
        } else if (msg.type === 'signal') {
          if (msg.senderPeerId !== peer.id) {
            socket.destroy();
            break;
          }
          if (signalingDebug) {
            const data = msg.data || msg.signal || {};
            const signalType = data?.type || (data?.sdp ? 'sdp' : (data?.candidate ? 'candidate' : 'unknown'));
            const candidateLine = typeof data?.candidate?.candidate === 'string'
              ? data.candidate.candidate
              : (typeof data?.candidate === 'string' ? data.candidate : '');
            const candidateType = /\styp\s+([a-z0-9-]+)/i.exec(candidateLine)?.[1] || '';
            const candidateAddress = candidateLine
              .replace(/^candidate:\S+\s+\S+\s+\S+\s+\S+\s+/i, '')
              .split(/\s+/)
              .slice(0, 2)
              .join(':')
              .slice(0, 80);
            const sdpBytes = typeof data?.sdp === 'string' ? Buffer.byteLength(data.sdp) : 0;
            console.error(`[smoke-signaling] signal from=${peer.id} to=${msg.receiverPeerId} receiver=${peers.has(msg.receiverPeerId) ? 'yes' : 'no'} room=${msg.room} type=${signalType} candidateType=${candidateType || '-'} candidate=${candidateAddress || '-'} sdpBytes=${sdpBytes}`);
          }
          peers.get(msg.receiverPeerId)?.send(msg);
        } else if (msg.type !== 'ping') {
          socket.destroy();
          break;
        }
      }
    });
    socket.on('close', () => {
      sockets.delete(socket);
      disconnect();
    });
    socket.on('error', () => {
      sockets.delete(socket);
      disconnect();
    });
  });
  server.closeAllSockets = () => {
    for (const socket of sockets) socket.destroy();
    sockets.clear();
  };

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(signalingPort, '127.0.0.1', () => resolve(server));
  });
}

async function stopSignalingServer(server) {
  if (!server) return;
  if (server.__ctoxExternalSignaling) {
    terminateOwnedSmokeChild(server, 'SIGINT', 'smoke-finalizer', 'graceful-signaling-stop');
    await withHostTimeout(new Promise((resolve) => server.once('exit', resolve)), 5000);
    if (server.exitCode === null && server.signalCode === null) {
      terminateOwnedSmokeChild(server, 'SIGKILL', 'smoke-finalizer', 'graceful-signaling-stop-timeout');
      await withHostTimeout(new Promise((resolve) => server.once('exit', resolve)), 2000);
    }
    return;
  }
  server.closeAllSockets?.();
  await withHostTimeout(new Promise((resolve) => server.close(() => resolve())), 5000);
}

function withHostTimeout(promise, ms) {
  return Promise.race([
    Promise.resolve(promise),
    new Promise((resolve) => setTimeout(resolve, ms)),
  ]);
}

async function waitForHttp(url, ms = 20000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (globalThis.__ctoxProcess
      && (globalThis.__ctoxProcess.exitCode !== null || globalThis.__ctoxProcess.signalCode !== null)) {
      throw new Error(`ctox exited before ${url}: code=${globalThis.__ctoxProcess.exitCode} signal=${globalThis.__ctoxProcess.signalCode}`);
    }
    try {
      const res = await fetch(url);
      if (res.ok) return await res.json();
    } catch {
      // Retry until deadline.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timeout waiting for ${url}`);
}

async function waitForLaunchSyncConfig(ms = 20000) {
  const url = `http://127.0.0.1:${businessPort}/index.html?rxdbSmoke=1`;
  const deadline = Date.now() + ms;
  let lastError = null;
  while (Date.now() < deadline) {
    if (globalThis.__ctoxProcess
      && (globalThis.__ctoxProcess.exitCode !== null || globalThis.__ctoxProcess.signalCode !== null)) {
      throw new Error(`ctox exited before launch sync config: code=${globalThis.__ctoxProcess.exitCode} signal=${globalThis.__ctoxProcess.signalCode}`);
    }
    try {
      const res = await fetch(url);
      if (res.ok) {
        const html = await res.text();
        const config = parseLaunchSyncConfig(html);
        if (config) return config;
      }
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timeout waiting for launch sync config${lastError ? `: ${lastError.message}` : ''}`);
}

function parseLaunchSyncConfig(html) {
  const marker = 'window.CTOX_BUSINESS_OS_CONFIG=';
  const start = html.indexOf(marker);
  if (start === -1) return null;
  const bodyStart = start + marker.length;
  const end = html.indexOf(';</script>', bodyStart);
  if (end === -1) return null;
  return JSON.parse(html.slice(bodyStart, end));
}

async function waitForNativePeerSyncConfig(ms = 60000) {
  const deadline = Date.now() + ms;
  let lastConfig = null;
  while (Date.now() < deadline) {
    lastConfig = await waitForLaunchSyncConfig(Math.min(2000, Math.max(500, deadline - Date.now())));
    const status = lastConfig?.native_rxdb_peer_status || {};
    if (lastConfig?.native_rxdb_peer_available === true && status.peer_session_id) {
      return lastConfig;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timeout waiting for native RxDB peer sync config: ${JSON.stringify(lastConfig)}`);
}

let cachedSqliteProgram = null;

function isExecutableFile(candidate) {
  try {
    const stat = fs.statSync(candidate);
    if (!stat.isFile()) return false;
    if (process.platform === 'win32') return true;
    fs.accessSync(candidate, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function resolveSqliteProgram() {
  if (cachedSqliteProgram) return cachedSqliteProgram;
  const candidates = process.platform === 'win32'
    ? []
    : ['/usr/bin/sqlite3', '/opt/homebrew/bin/sqlite3', '/usr/local/bin/sqlite3'];
  const pathExts = process.platform === 'win32'
    ? (process.env.PATHEXT || '.EXE;.CMD;.BAT;.COM').split(';').filter(Boolean)
    : [''];
  for (const dir of (process.env.PATH || '').split(path.delimiter).filter(Boolean)) {
    for (const ext of pathExts) {
      candidates.push(path.join(dir, `sqlite3${ext}`));
    }
  }
  for (const candidate of candidates) {
    if (isExecutableFile(candidate)) {
      cachedSqliteProgram = candidate;
      return cachedSqliteProgram;
    }
  }
  throw new Error('sqlite failed: sqlite3 executable not found in PATH or standard install locations');
}

function sqlite(statement, targetPath = sqlitePath) {
  const deadline = Date.now() + 20000;
  let lastOutput = '';
  const sqliteProgram = resolveSqliteProgram();
  while (Date.now() <= deadline) {
    const result = spawnSync(sqliteProgram, [
      '-cmd',
      '.timeout 10000',
      targetPath,
    ], {
      encoding: 'utf8',
      input: statement,
      maxBuffer: 16 * 1024 * 1024,
    });
    if (result.status === 0) return result.stdout;
    lastOutput = result.error?.message || result.stderr || result.stdout || '';
    if (!/database is locked|SQLITE_BUSY/i.test(lastOutput)) {
      const status = result.signal ? `signal ${result.signal}` : `status ${result.status}`;
      throw new Error(`sqlite failed (${sqliteProgram}, ${status}): ${lastOutput}`);
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
  }
  throw new Error(`sqlite failed (${sqliteProgram}): ${lastOutput}`);
}

async function waitForSqliteTables(tableNames, ms = 30000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    try {
      const rows = sqlite("SELECT name FROM sqlite_master WHERE type='table';")
        .split(/\r?\n/)
        .map((name) => name.trim())
        .filter(Boolean);
      if (tableNames.every((name) => rows.includes(name))) return;
    } catch {
      // The native peer may still be creating the SQLite database.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timeout waiting for sqlite tables: ${tableNames.join(', ')}`);
}

function pollSqliteFileAndChunk(id, ms = 30000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const fileRow = sqlite(`SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id='${sqlString(id)}' LIMIT 1;`).trim();
    const chunkRow = sqlite(`SELECT data FROM ctox_business_os__desktop_file_chunks__v0 WHERE id='${sqlString(`${id}_0`)}' LIMIT 1;`).trim();
    if (fileRow && chunkRow) {
      const file = JSON.parse(fileRow);
      const chunk = JSON.parse(chunkRow);
      const payload = Buffer.from(String(chunk.data || ''), 'base64').toString('utf8');
      return { file, chunk, payload };
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  throw new Error(`sqlite file/chunk rows not replicated for ${id}`);
}

function pollSqliteJson(tableName, id, ms = 30000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const row = sqlite(`SELECT data FROM ${quoteSqlIdentifier(tableName)} WHERE id='${sqlString(id)}' LIMIT 1;`).trim();
    if (row) return JSON.parse(row);
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  throw new Error(`sqlite row not replicated for ${tableName}.${id}`);
}

function sqliteTableExists(tableName) {
  return sqlite(`SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='${sqlString(tableName)}';`).trim() === '1';
}

function sqliteRowCount(tableName, whereClause = '1=1') {
  if (!sqliteTableExists(tableName)) return 0;
  const out = sqlite(`SELECT COUNT(*) FROM ${quoteSqlIdentifier(tableName)} WHERE ${whereClause};`).trim();
  return Number(out || 0);
}

function seedNativeOptionalSchemaDriftFixture() {
  const schemaPath = path.join(root, 'src/core/business_os/business_os_schema_contract.json');
  const contract = JSON.parse(fs.readFileSync(schemaPath, 'utf8'));
  const driftedSchema = typeof structuredClone === 'function'
    ? structuredClone(contract.outbound_messages)
    : JSON.parse(JSON.stringify(contract.outbound_messages));
  if (!driftedSchema || typeof driftedSchema !== 'object') {
    throw new Error('outbound_messages schema missing from Business OS schema contract');
  }
  driftedSchema.xLegacyDrift = 'legacy optional schema';
  const schemaVersion = Number.isInteger(Number(driftedSchema.version))
    ? Number(driftedSchema.version)
    : 0;
  const collectionKey = `outbound_messages-${schemaVersion}`;
  const internalTable = 'ctox_business_os___rxdb_internal__v0';
  const lwt = Date.now();
  const document = {
    id: `collection|${collectionKey}`,
    key: collectionKey,
    context: 'collection',
    data: {
      name: 'outbound_messages',
      schemaHash: 'legacy-outbound-messages-schema-hash',
      schema: driftedSchema,
      version: schemaVersion,
      connectedStorages: [],
    },
    _deleted: false,
    _meta: { lwt },
    _rev: '1-native-schema-drift-smoke',
    _attachments: {},
  };
  sqlite(`
    CREATE TABLE IF NOT EXISTS ${quoteSqlIdentifier(internalTable)}(
      id TEXT NOT NULL PRIMARY KEY UNIQUE,
      revision TEXT,
      deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
      lastWriteTime REAL NOT NULL,
      data TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS ${quoteSqlIdentifier(`${internalTable}_lwt_id_idx`)}
      ON ${quoteSqlIdentifier(internalTable)}(lastWriteTime, id);
    CREATE INDEX IF NOT EXISTS ${quoteSqlIdentifier(`${internalTable}_deleted_lwt_id_idx`)}
      ON ${quoteSqlIdentifier(internalTable)}(deleted, lastWriteTime, id);
    INSERT OR REPLACE INTO ${quoteSqlIdentifier(internalTable)}
      (id, revision, deleted, lastWriteTime, data)
    VALUES (
      '${sqlString(document.id)}',
      '${sqlString(document._rev)}',
      0,
      ${lwt},
      '${sqlString(JSON.stringify(document))}'
    );
  `);
  return { table: internalTable, collection: 'outbound_messages', documentId: document.id };
}

function sqlString(value) {
  return String(value).replaceAll("'", "''");
}

function quoteSqlIdentifier(identifier) {
  return `"${String(identifier).replaceAll('"', '""')}"`;
}

// waitForHealthy reports ok as soon as the SHELL is healthy. The strict
// contract requires COMPLETE initial sync for the requested collection set, so
// poll until the required tail finishes (bounded) before asserting. Demand-only
// chunk/blob collections must only be requested by callers that have explicitly
// leased them first.
async function waitForHealthyCompleteStatus(page, { timeoutMs = 60000, requiredCollections = null, allowRestart = true } = {}) {
  const deadline = Date.now() + timeoutMs;
  let status = null;
  for (;;) {
    status = await page.evaluate((options) => globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
      timeoutMs: options.timeoutMs,
      allowRestart: options.allowRestart === true,
      ...(options.requiredCollections ? { requiredCollections: options.requiredCollections } : {}),
    }), { timeoutMs, requiredCollections, allowRestart });
    const initialSync = status?.sync?.initialSync || {};
    const missing = Array.isArray(initialSync.missingInitialReplication)
      ? initialSync.missingInitialReplication
      : [];
    const incomplete = Array.isArray(initialSync.entries)
      && initialSync.entries.some((entry) => entry?.state !== 'complete');
    if (status?.ok && missing.length === 0 && !incomplete) return status;
    if (Date.now() > deadline) return status;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

function assertHealthyAdvancedStatusContract(status) {
  const problems = [];
  if (!status || typeof status !== 'object') {
    throw new Error(`Business OS advanced status missing: ${JSON.stringify(status)}`);
  }
  if (status.version !== 'business-os-advanced-status-v1') {
    problems.push(`unexpected version ${JSON.stringify(status.version)}`);
  }
  if (!status.ok) problems.push('status.ok is false');
  if (!isWebRtcStatusMode(status.sync?.mode)) problems.push(`sync.mode is ${JSON.stringify(status.sync?.mode)}`);
  if (status.sync?.protocol !== 'ctox-rxdb-protocol-v1') {
    problems.push(`sync.protocol is ${JSON.stringify(status.sync?.protocol)}`);
  }
  if (status.checks?.rxdbRuntimeAppLocal !== true) {
    problems.push(`checks.rxdbRuntimeAppLocal is ${JSON.stringify(status.checks?.rxdbRuntimeAppLocal)}`);
  }
  if (status.rxdbRuntime?.name !== 'ctox-rxdb-js'
    || status.rxdbRuntime?.publicName !== 'CTOX Sync Engine'
    || status.rxdbRuntime?.source !== 'app-local'
    || status.rxdbRuntime?.packageManager !== 'none'
    || status.rxdbRuntime?.apiContract !== 'ctox-db-business-os-v1'
    || status.rxdbRuntime?.upstreamCompatibility !== 'not-upstream-rxdb'
    || status.rxdbRuntime?.upstreamCompatible !== false) {
    problems.push(`rxdbRuntime is not app-local ctox-rxdb-js: ${JSON.stringify(status.rxdbRuntime)}`);
  }
  if (!Array.isArray(status.sync?.capabilities) || !status.sync.capabilities.includes('ctox-peer-session-v1')) {
    problems.push('sync.capabilities is missing ctox-peer-session-v1');
  }
  if (!Array.isArray(status.sync?.peerSessions)) {
    problems.push('sync.peerSessions is not an array');
  } else if (status.sync.peerSessions.length === 0) {
    problems.push('sync.peerSessions is empty');
  } else if (status.sync.peerSessions.some((session) => !Number.isFinite(Number(session?.generation)) || Number(session.generation) < 1)) {
    problems.push(`sync.peerSessions contains invalid generation: ${JSON.stringify(status.sync.peerSessions)}`);
  } else if (status.sync.peerSessions.some((session) => !session?.checkpoint?.epoch || session.checkpoint.state !== 'advertised')) {
    problems.push(`sync.peerSessions missing checkpoint epoch evidence: ${JSON.stringify(status.sync.peerSessions)}`);
  }
  if (!Array.isArray(status.sync?.collectionErrors)) {
    problems.push('sync.collectionErrors is not an array');
  } else if (status.sync.collectionErrors.length > 0) {
    problems.push(`sync.collectionErrors is not empty: ${JSON.stringify(status.sync.collectionErrors)}`);
  }
  if (!Array.isArray(status.sync?.checkpointErrors)) {
    problems.push('sync.checkpointErrors is not an array');
  } else if (status.sync.checkpointErrors.length > 0) {
    problems.push(`sync.checkpointErrors is not empty: ${JSON.stringify(status.sync.checkpointErrors)}`);
  }
  if (!Array.isArray(status.sync?.failedCollections)) {
    problems.push('sync.failedCollections is not an array');
  } else if (status.sync.failedCollections.length > 0) {
    problems.push(`sync.failedCollections is not empty: ${JSON.stringify(status.sync.failedCollections)}`);
  }
  if (!Array.isArray(status.sync?.missingRequiredCollections)) {
    problems.push('sync.missingRequiredCollections is not an array');
  } else if (status.sync.missingRequiredCollections.length > 0) {
    problems.push(`sync.missingRequiredCollections is not empty: ${JSON.stringify(status.sync.missingRequiredCollections)}`);
  }
  if (!status.sync?.initialSync || typeof status.sync.initialSync !== 'object') {
    problems.push('sync.initialSync is missing');
  } else {
    const initialSync = status.sync.initialSync;
    if (!Array.isArray(initialSync.missingInitialReplication)) {
      problems.push('sync.initialSync.missingInitialReplication is not an array');
    } else if (initialSync.missingInitialReplication.length > 0) {
      problems.push(`sync.initialSync.missingInitialReplication is not empty: ${JSON.stringify(initialSync.missingInitialReplication)}`);
    }
    if (!Array.isArray(initialSync.missingCheckpointEpoch)) {
      problems.push('sync.initialSync.missingCheckpointEpoch is not an array');
    } else if (initialSync.missingCheckpointEpoch.length > 0) {
      problems.push(`sync.initialSync.missingCheckpointEpoch is not empty: ${JSON.stringify(initialSync.missingCheckpointEpoch)}`);
    }
    if (!Array.isArray(initialSync.entries) || initialSync.entries.length === 0) {
      problems.push('sync.initialSync.entries is empty');
    } else if (initialSync.entries.some((entry) => entry?.state !== 'complete' || !entry?.initialReplicationAt)) {
      problems.push(`sync.initialSync.entries contains incomplete collection: ${JSON.stringify(initialSync.entries)}`);
    } else if (initialSync.entries.some((entry) => entry?.checkpointEpochAdvertised !== true || !entry?.checkpointEpoch)) {
      problems.push(`sync.initialSync.entries missing checkpoint epoch evidence: ${JSON.stringify(initialSync.entries)}`);
    }
  }
  if (status.checks?.frameTransportRealtimeHealthy !== true) {
    problems.push(`checks.frameTransportRealtimeHealthy is ${JSON.stringify(status.checks?.frameTransportRealtimeHealthy)}`);
  }
  const frameTransport = status.sync?.frameTransport;
  if (!frameTransport || typeof frameTransport !== 'object') {
    problems.push('sync.frameTransport is missing');
  } else {
    if (frameTransport.protocol !== 'ctox-rxdb-frame-v1') {
      problems.push(`sync.frameTransport.protocol is ${JSON.stringify(frameTransport.protocol)}`);
    }
    if (!Array.isArray(frameTransport.unhealthyCollections)) {
      problems.push('sync.frameTransport.unhealthyCollections is not an array');
    } else if (frameTransport.unhealthyCollections.length > 0) {
      problems.push(`sync.frameTransport.unhealthyCollections is not empty: ${JSON.stringify(frameTransport.unhealthyCollections)}`);
    }
    if (!Array.isArray(frameTransport.entries) || frameTransport.entries.length === 0) {
      problems.push('sync.frameTransport.entries is empty');
    } else if (frameTransport.entries.some((entry) => entry?.protocol !== 'ctox-rxdb-frame-v1')) {
      problems.push(`sync.frameTransport.entries contain non-frame protocol entries: ${JSON.stringify(frameTransport.entries)}`);
    } else if (frameTransport.entries.some((entry) => Number(entry?.maxInlineFrameBytes || 0) <= 0 || Number(entry?.maxChunkChars || 0) <= 0 || Number(entry?.ackWindow || 0) <= 0)) {
      problems.push(`sync.frameTransport.entries missing frame limits: ${JSON.stringify(frameTransport.entries)}`);
    }
    if (!frameTransport.totals || typeof frameTransport.totals !== 'object') {
      problems.push('sync.frameTransport.totals is missing');
    } else if (Number(frameTransport.totals.pendingAcks || 0) > 16 || Number(frameTransport.totals.priorityQueueDepth || 0) > 128) {
      problems.push(`sync.frameTransport.totals exceed realtime thresholds: ${JSON.stringify(frameTransport.totals)}`);
    }
  }
  if (problems.length) {
    throw new Error(`Business OS advanced status contract failed: ${problems.join('; ')}\n${JSON.stringify(status, null, 2)}`);
  }
}

function isWebRtcStatusMode(mode) {
  return typeof mode === 'string' && mode.split('+').includes('webrtc');
}

async function collectStartupState(page) {
  return page.evaluate(() => ({
    url: location.href,
    title: document.title,
    readyState: document.readyState,
    hasSmoke: Boolean(globalThis.ctoxBusinessOsSmoke),
    smokeBootstrap: globalThis.ctoxBusinessOsSmoke?.bootstrap || '',
    hasAdvancedStatus: Boolean(globalThis.CTOX_BUSINESS_OS_STATUS),
    search: location.search,
    scriptSrcs: [...document.scripts].map((script) => script.src || '[inline]').slice(0, 20),
    resources: performance.getEntriesByType('resource')
      .filter((entry) => /\/(?:app|shared|modules|vendor)\//.test(entry.name) || entry.name.includes('/app.js'))
      .map((entry) => ({
        name: entry.name,
        initiatorType: entry.initiatorType,
        duration: Math.round(entry.duration),
        transferSize: entry.transferSize,
        decodedBodySize: entry.decodedBodySize,
      }))
      .slice(-30),
    bodyDataset: { ...document.body?.dataset },
    bodyText: (document.body?.innerText || '').slice(0, 800),
  })).catch((evalError) => ({ evaluateError: String(evalError?.message || evalError) }));
}

function isPreHookModuleGraphStall(startupState) {
  if (!startupState || startupState.hasSmoke) return false;
  if (startupState.readyState !== 'interactive') return false;
  const scripts = Array.isArray(startupState.scriptSrcs) ? startupState.scriptSrcs : [];
  return scripts.some((src) => /\/app\.js\?/.test(src));
}

function addQueryParam(urlPath, key, value) {
  const [pathAndQuery, hash = ''] = urlPath.split('#');
  const [pathname, query = ''] = pathAndQuery.split('?');
  const params = new URLSearchParams(query);
  params.set(key, value);
  const nextQuery = params.toString();
  return `${pathname}${nextQuery ? `?${nextQuery}` : ''}${hash ? `#${hash}` : ''}`;
}

async function assertNoVisibleBrowserDebugText(page) {
  const forbidden = await page.evaluate(() => {
    const root = document.querySelector('[data-browser-root]');
    if (!root) return [];
    const terms = [
      'Waiting for the next RxDB frame',
      'pending_command',
      'browser.session',
      'RxDB',
      'Command',
      'Seq',
    ];
    const visibleText = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        const parent = node.parentElement;
        if (!parent) return NodeFilter.FILTER_REJECT;
        if (parent.closest('[hidden], .browser-diagnostics, script, style')) return NodeFilter.FILTER_REJECT;
        const style = getComputedStyle(parent);
        if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) {
          return NodeFilter.FILTER_REJECT;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    while (walker.nextNode()) {
      const text = walker.currentNode.nodeValue || '';
      if (text.trim()) visibleText.push(text);
    }
    const body = visibleText.join(' ').replace(/\s+/g, ' ');
    return terms.filter((term) => body.includes(term));
  });
  if (forbidden.length) {
    throw new Error(`Browser UI exposes debug text: ${forbidden.join(', ')}`);
  }
}

function seedRustSideFile(source) {
  const now = Date.now();
  const dir = path.join(runtimeRoot, 'runtime/business-os/notes/rxdb-smoke');
  fs.mkdirSync(dir, { recursive: true });
  const content = hasOwn(process.env, 'SMOKE_RUST_FILE_CONTENT')
    ? process.env.SMOKE_RUST_FILE_CONTENT
    : `hello from ${source} ${now}`;
  const filePath = path.join(dir, `${source}_${now}_${token(5)}.txt`);
  fs.writeFileSync(filePath, content);
  const canonicalPath = fs.realpathSync(filePath);
  const id = `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`;
  return { id, content, path: canonicalPath, syncMode: 'file' };
}

function seedRustWorkspaceFile(options = {}) {
  const now = Date.now();
  const workspaceName = `workspace_${now}_${token(5)}`;
  const workspacePath = path.join(runtimeRoot, 'runtime/business-os/workspaces/rxdb-smoke', workspaceName);
  const dir = path.join(workspacePath, 'reports');
  fs.mkdirSync(dir, { recursive: true });
  const largeContent = options.large
    ? `${'large workspace smoke block\n'.repeat(45000)}large workspace smoke ${now}\n`
    : null;
  const content = hasOwn(process.env, 'SMOKE_RUST_FILE_CONTENT')
    ? process.env.SMOKE_RUST_FILE_CONTENT
    : (largeContent || `hello from workspace_smoke ${now}`);
  const filePath = path.join(dir, 'brief.md');
  fs.writeFileSync(filePath, content);
  const canonicalPath = fs.realpathSync(filePath);
  const id = `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`;
  return {
    id,
    content,
    path: canonicalPath,
    syncMode: 'workspace',
    workspacePath: fs.realpathSync(workspacePath),
    expectedVirtualPath: `/CTOX/${workspaceName}/reports/brief.md`,
  };
}

function seedRustWorkspaceArtifacts() {
  const now = Date.now();
  const workspaceName = `agent_workspace_${now}_${token(5)}`;
  const workspacePath = path.join(runtimeRoot, 'runtime/business-os/workspaces/rxdb-smoke', workspaceName);
  const artifacts = [
    {
      relativePath: 'reports/brief.md',
      content: `# Agent Brief\n\nGenerated by CTOX smoke ${now}\n`,
    },
    {
      relativePath: 'plans/runtime-plan.json',
      content: JSON.stringify({
        id: `plan_${now}`,
        status: 'ready',
        steps: ['inspect', 'edit', 'verify'],
      }, null, 2),
    },
    {
      relativePath: 'logs/codex-output.txt',
      content: `ctox agent loop wrote a workspace artifact at ${now}\nstdout: ok\n`,
    },
    {
      relativePath: 'src/generated/module-note.md',
      content: `Generated source note ${now}\n\nThis file must appear in Business OS via RxDB.\n`,
    },
  ];
  const files = artifacts.map((artifact) => {
    const filePath = path.join(workspacePath, artifact.relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, artifact.content);
    const canonicalPath = fs.realpathSync(filePath);
    return {
      id: `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`,
      content: artifact.content,
      path: canonicalPath,
      relativePath: artifact.relativePath,
      expectedVirtualPath: `/CTOX/${workspaceName}/${artifact.relativePath}`,
    };
  });
  return {
    id: files[0].id,
    content: files[0].content,
    path: files[0].path,
    syncMode: 'workspace',
    workspacePath: fs.realpathSync(workspacePath),
    expectedVirtualPath: files[0].expectedVirtualPath,
    files,
  };
}

function seedRustWorkspaceArtifactStress() {
  const now = Date.now();
  const workspaceName = `agent_stress_${now}_${token(5)}`;
  const workspacePath = path.join(runtimeRoot, 'runtime/business-os/workspaces/rxdb-smoke', workspaceName);
  const largeBlock = (label) => Array.from({ length: 6200 }, (_, index) => `${label}:${index}:${now}:ctox-rxdb-stress\n`).join('');
  const artifacts = [
    { relativePath: 'reports/brief.md', content: `# Stress Brief\n\nGenerated ${now}\n` },
    { relativePath: 'reports/summary.md', content: `# Summary\n\nAll agent artifacts must sync over RxDB/WebRTC.\n${now}\n` },
    { relativePath: 'plans/runtime-plan.json', content: JSON.stringify({ id: `plan_${now}`, status: 'ready', steps: ['inspect', 'edit', 'verify', 'publish'] }, null, 2) },
    { relativePath: 'plans/retry-plan.json', content: JSON.stringify({ id: `retry_${now}`, retries: [0, 1, 2], policy: 'idempotent' }, null, 2) },
    { relativePath: 'logs/codex-output.txt', content: `ctox agent loop wrote artifacts at ${now}\nstdout: ok\n` },
    { relativePath: 'logs/tool-calls.jsonl', content: `${JSON.stringify({ t: now, tool: 'apply_patch', ok: true })}\n${JSON.stringify({ t: now + 1, tool: 'cargo test', ok: true })}\n` },
    { relativePath: 'src/generated/module-note.md', content: `Generated source note ${now}\n` },
    { relativePath: 'src/generated/config.toml', content: `generated_at = ${now}\nmode = "rxdb-webrtc"\n` },
    { relativePath: 'docs/checklist.md', content: `- [x] rxdb\n- [x] sqlite\n- [x] business-os\n${now}\n` },
    { relativePath: 'docs/notes/session.md', content: `Session ${now}\n\nNo HTTP data-plane fallback.\n` },
    { relativePath: 'artifacts/table.csv', content: `name,status,ts\nrxdb,ok,${now}\nwebrtc,ok,${now + 1}\n` },
    { relativePath: 'artifacts/state.json', content: JSON.stringify({ generatedAt: now, state: 'synced', collections: ['desktop_files', 'desktop_file_chunks'] }, null, 2) },
    { relativePath: 'large/transcript-a.txt', content: largeBlock('transcript-a') },
    { relativePath: 'large/transcript-b.txt', content: largeBlock('transcript-b') },
    { relativePath: 'large/report-c.txt', content: largeBlock('report-c') },
    { relativePath: 'large/report-d.txt', content: largeBlock('report-d') },
  ];
  const files = artifacts.map((artifact) => {
    const filePath = path.join(workspacePath, artifact.relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, artifact.content);
    const canonicalPath = fs.realpathSync(filePath);
    return {
      id: `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`,
      content: artifact.content,
      path: canonicalPath,
      relativePath: artifact.relativePath,
      expectedVirtualPath: `/CTOX/${workspaceName}/${artifact.relativePath}`,
    };
  });
  return {
    id: files[0].id,
    content: files[0].content,
    path: files[0].path,
    syncMode: 'workspace',
    workspacePath: fs.realpathSync(workspacePath),
    expectedVirtualPath: files[0].expectedVirtualPath,
    files,
    stress: true,
  };
}

function mutateRustWorkspaceArtifactStress(seed) {
  if (!seed?.workspacePath || !Array.isArray(seed.files)) {
    throw new Error('workspace artifact churn requires a stress seed');
  }
  const now = Date.now();
  const byRelativePath = new Map(seed.files.map((file) => [file.relativePath, file]));
  const updates = [
    ['reports/summary.md', `# Summary\n\nUpdated during churn ${now}\nNo stale chunk generation may be shown.\n`],
    ['plans/runtime-plan.json', JSON.stringify({ id: `plan_${now}`, status: 'updated', steps: ['inspect', 'edit', 'verify', 'publish', 'churn'] }, null, 2)],
    ['large/transcript-a.txt', Array.from({ length: 6900 }, (_, index) => `transcript-a-updated:${index}:${now}:ctox-rxdb-churn\n`).join('')],
    ['large/report-c.txt', Array.from({ length: 6700 }, (_, index) => `report-c-updated:${index}:${now}:ctox-rxdb-churn\n`).join('')],
  ];
  const added = [
    ['reports/churn-result.md', `# Churn Result\n\nAdded ${now}\n`],
    ['logs/churn-events.jsonl', `${JSON.stringify({ t: now, event: 'created' })}\n${JSON.stringify({ t: now + 1, event: 'synced' })}\n`],
    ['src/generated/churn.rs', `pub const CHURN_TS: u128 = ${now};\n`],
    ['large/churn-delta.txt', Array.from({ length: 5200 }, (_, index) => `delta:${index}:${now}:ctox-rxdb-churn\n`).join('')],
  ];

  for (const [relativePath, content] of updates) {
    const file = byRelativePath.get(relativePath);
    if (!file) throw new Error(`missing churn update target ${relativePath}`);
    fs.writeFileSync(file.path, content);
    file.content = content;
  }

  const workspaceName = path.basename(seed.workspacePath);
  const addedFiles = added.map(([relativePath, content]) => {
    const filePath = path.join(seed.workspacePath, relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, content);
    const canonicalPath = fs.realpathSync(filePath);
    return {
      id: `ctox_file_${crypto.createHash('sha256').update(canonicalPath).digest('hex')}`,
      content,
      path: canonicalPath,
      relativePath,
      expectedVirtualPath: `/CTOX/${workspaceName}/${relativePath}`,
      added: true,
    };
  });
  seed.files.push(...addedFiles);
  syncRustSeedFile(seed);
  return {
    files: seed.files,
    updatedRelativePaths: updates.map(([relativePath]) => relativePath),
    addedRelativePaths: added.map(([relativePath]) => relativePath),
  };
}

function createWorkspaceQueueTaskForBackgroundIndex(seed) {
  if (!seed?.workspacePath) throw new Error('background workspace index smoke requires a workspace seed');
  const result = spawnSync(ctoxBin, [
    'queue',
    'add',
    '--title',
    `RxDB background workspace index ${Date.now()}`,
    '--prompt',
    `Index this CTOX smoke workspace through the native Business OS background scanner.\nWorkspace root: ${seed.workspacePath}`,
    '--workspace-root',
    seed.workspacePath,
    '--priority',
    'normal',
  ], {
    cwd: root,
    env: {
      ...process.env,
      CTOX_ROOT: runtimeRoot,
      CARGO_TARGET_DIR: path.join(root, 'runtime/build/core-rxdb-integration-target'),
    },
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(`ctox queue add failed for background workspace index: ${result.stderr || result.stdout || 'no output'}`);
  }
  let parsed = null;
  try {
    parsed = JSON.parse(result.stdout || '{}');
  } catch {
    parsed = { raw: result.stdout || '' };
  }
  return {
    created: true,
    taskId: parsed?.task?.id || parsed?.task?.message_key || '',
    workspacePath: seed.workspacePath,
  };
}

function syncRustSeedFile(seed) {
  const deadline = Date.now() + 60000;
  let lastOutput = '';
  const args = seed.syncMode === 'workspace'
    ? ['business-os', 'files', 'sync-workspace', seed.workspacePath]
    : ['business-os', 'files', 'sync', seed.path];
  while (Date.now() < deadline) {
    const result = spawnSync(ctoxBin, args, {
      cwd: root,
      env: {
        ...process.env,
        CTOX_ROOT: runtimeRoot,
        CARGO_TARGET_DIR: path.join(root, 'runtime/build/core-rxdb-integration-target'),
      },
      encoding: 'utf8',
    });
    if (result.status === 0) return;
    lastOutput = result.stderr || result.stdout || '';
    if (!String(lastOutput).includes('database is locked')) break;
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  throw new Error(`ctox ${args.join(' ')} failed: ${lastOutput}`);
}

// Backlog OS-C3: two isolated browser peers on one CTOX room. Verifies the
// multi-user promise end to end: (1) presence entries propagate through the
// native in-memory hub (ctox-presence-v1) in both directions and disappear
// when a peer closes; (2) concurrent edits to DIFFERENT fields of the same
// field-merge document (notes: title vs content, docs/ctox-rxdb.md §8.2)
// converge on both peers AND in the native store without losing either edit.
async function runPresenceMergeTwoBrowsersMode(pageA) {
  const evidence = { mode: 'presence-merge-two-browsers' };
  const smokeHookReady = () => Boolean(
    globalThis.ctoxBusinessOsSmoke
      && globalThis.ctoxBusinessOsSmoke.bootstrap !== 'inline'
      && globalThis.CTOX_BUSINESS_OS_STATUS
  );
  const pollPage = async (page, label, fn, timeoutMs = 60000) => {
    const deadline = Date.now() + timeoutMs;
    let last = null;
    while (Date.now() < deadline) {
      last = await page.evaluate(fn);
      if (last && last.ok) return last;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
    throw new Error(`${label} did not converge within ${timeoutMs}ms: ${JSON.stringify(last)}`);
  };
  const setupNotes = (page) => page.evaluate(async () => {
    const state = globalThis.ctoxBusinessOsSmoke.state;
    const schemaMod = await import('/modules/notes/schema.js');
    await state.db.addCollections({ notes: schemaMod.collections.notes });
    await state.sync.startCollection('notes');
    return true;
  });

  const profileB = fs.mkdtempSync(path.join(runtimeRoot, 'browser-profile-b-'));
  const browserB = await chromium.launchPersistentContext(profileB, chromiumLaunchOptions());
  try {
    const pageB = await browserB.newPage();
    const urlB = new URL(pageA.url());
    urlB.searchParams.set('smokeDbId', `${smokeDbId}_peer_b`);
    await pageB.goto(urlB.toString(), { waitUntil: 'commit', timeout: pageNavigationTimeoutMs });
    await pageA.waitForFunction(smokeHookReady, null, { timeout: smokeHookWaitTimeoutMs });
    await pageB.waitForFunction(smokeHookReady, null, { timeout: smokeHookWaitTimeoutMs });

    // ---- Presence propagation A -> B ------------------------------------
    await pageA.evaluate(() => {
      const registry = globalThis.ctoxBusinessOsSmoke.state.db.rxdb.getPresenceRegistry();
      registry.setLocal('presence-smoke', [{
        collection: 'notes',
        recordId: 'presence-merge-smoke-note',
        actorId: 'smoke-user-a',
        actorName: 'Smoke User A',
        mode: 'editing',
      }]);
    });
    await pollPage(pageB, 'presence A->B', () => {
      const registry = globalThis.ctoxBusinessOsSmoke?.state?.db?.rxdb?.getPresenceRegistry?.();
      const entries = registry?.remoteEntries || [];
      return {
        ok: entries.some((entry) => entry.recordId === 'presence-merge-smoke-note' && entry.actorId === 'smoke-user-a'),
        entries,
      };
    });
    evidence.presencePropagated = true;

    // ---- Presence propagation B -> A (needed for the close test) --------
    await pageB.evaluate(() => {
      const registry = globalThis.ctoxBusinessOsSmoke.state.db.rxdb.getPresenceRegistry();
      registry.setLocal('presence-smoke', [{
        collection: 'notes',
        recordId: 'presence-merge-smoke-note',
        actorId: 'smoke-user-b',
        actorName: 'Smoke User B',
        mode: 'viewing',
      }]);
    });
    await pollPage(pageA, 'presence B->A', () => {
      const registry = globalThis.ctoxBusinessOsSmoke?.state?.db?.rxdb?.getPresenceRegistry?.();
      const entries = registry?.remoteEntries || [];
      return {
        ok: entries.some((entry) => entry.actorId === 'smoke-user-b'),
        entries,
      };
    });
    evidence.presenceBidirectional = true;

    // ---- Field-merge convergence -----------------------------------------
    await setupNotes(pageA);
    await setupNotes(pageB);
    await pageA.evaluate(async () => {
      const state = globalThis.ctoxBusinessOsSmoke.state;
      await state.db.raw.notes.insert({
        id: 'merge-smoke-note',
        title: 'title-base',
        content: 'content-base',
        folder: '',
        notebook: '',
        tags: '',
        is_favorite: false,
        is_trashed: false,
        is_locked: false,
        updated_at_ms: Date.now(),
      });
    });
    const readNote = async () => {
      const doc = await globalThis.ctoxBusinessOsSmoke.state.db.raw.notes.findOne('merge-smoke-note').exec();
      const note = doc?.toJSON?.() || doc || null;
      return note ? { ok: true, title: note.title, content: note.content } : { ok: false };
    };
    await pollPage(pageB, 'note replication A->B', readNote);

    // Both peers edit DIFFERENT fields at the same time.
    await Promise.all([
      pageA.evaluate(async () => {
        const doc = await globalThis.ctoxBusinessOsSmoke.state.db.raw.notes.findOne('merge-smoke-note').exec();
        await doc.patch({ title: 'title-from-a', updated_at_ms: Date.now() });
      }),
      pageB.evaluate(async () => {
        const doc = await globalThis.ctoxBusinessOsSmoke.state.db.raw.notes.findOne('merge-smoke-note').exec();
        await doc.patch({ content: 'content-from-b', updated_at_ms: Date.now() });
      }),
    ]);
    const converged = (label, page) => pollPage(page, label, async () => {
      const doc = await globalThis.ctoxBusinessOsSmoke.state.db.raw.notes.findOne('merge-smoke-note').exec();
      const note = doc?.toJSON?.() || doc || null;
      return {
        ok: Boolean(note && note.title === 'title-from-a' && note.content === 'content-from-b'),
        title: note?.title,
        content: note?.content,
      };
    }, 90000);
    await converged('merge convergence on A', pageA);
    await converged('merge convergence on B', pageB);
    evidence.mergeConverged = true;

    // The native store must hold BOTH edits too (the master accepted the
    // merged state, not one whole-doc winner).
    const nativeDeadline = Date.now() + 30000;
    let nativeRow = '';
    while (Date.now() < nativeDeadline) {
      nativeRow = sqlite(`
        SELECT json_extract(data, '$.title') || char(9) || json_extract(data, '$.content')
        FROM ctox_business_os__notes__v1
        WHERE id='merge-smoke-note';
      `).trim();
      if (nativeRow === `title-from-a\tcontent-from-b`) break;
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
    }
    if (nativeRow !== `title-from-a\tcontent-from-b`) {
      throw new Error(`native store did not converge to the merged note: ${JSON.stringify(nativeRow)}`);
    }
    evidence.nativeMergeConverged = true;

    // ---- Peer close clears presence --------------------------------------
    // Closing the whole browser is an ABRUPT disconnect: the native side may
    // see no clean DataChannel close, so clearing rides the ICE/DTLS timeout
    // + remove_peer dirty-mark + TTL sweep (45s) rather than an immediate
    // broadcast. Budget covers timeout + sweep + slack.
    await browserB.close();
    await pollPage(pageA, 'presence cleared after peer close', () => {
      const registry = globalThis.ctoxBusinessOsSmoke?.state?.db?.rxdb?.getPresenceRegistry?.();
      const entries = registry?.remoteEntries || [];
      return {
        ok: !entries.some((entry) => entry.actorId === 'smoke-user-b'),
        entries,
      };
    }, 120000);
    evidence.presenceClearedOnPeerClose = true;
  } finally {
    try { await browserB.close(); } catch {}
    removeSmokePath(profileB);
  }
  return evidence;
}

function corruptRustSeedChunkMetadata(seed) {
  const deadline = Date.now() + 60000;
  let rowId = '';
  let chunk = null;
  while (Date.now() < deadline) {
    const row = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_file_chunks__v0
      WHERE json_extract(data, '$.file_id')='${sqlString(seed.id)}'
      ORDER BY CAST(json_extract(data, '$.idx') AS INTEGER), id
      LIMIT 1;
    `).trim();
    if (row) {
      const splitAt = row.indexOf('\t');
      rowId = splitAt >= 0 ? row.slice(0, splitAt) : '';
      chunk = JSON.parse(splitAt >= 0 ? row.slice(splitAt + 1) : row);
      break;
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  if (!rowId || !chunk) {
    throw new Error(`no SQLite chunk row found for metadata corruption: ${seed.id}`);
  }
  const now = Date.now();
  const revisionHeight = Number(String(chunk._rev || '').split('-')[0] || 1) + 1;
  const revision = `${revisionHeight}-${token(10)}`;
  chunk.size_bytes = Number(chunk.size_bytes || 0) + 1;
  chunk._rev = revision;
  chunk._meta = { ...(chunk._meta || {}), lwt: now };
  sqlite(`
    UPDATE ctox_business_os__desktop_file_chunks__v0
    SET data='${sqlString(JSON.stringify(chunk))}',
        revision='${sqlString(revision)}',
        lastWriteTime=${now}
    WHERE id='${sqlString(rowId)}';
  `);
  return {
    rowId,
    fileId: seed.id,
    revision,
    expectedSizeBytes: chunk.size_bytes,
    actualSizeBytes: String(chunk.data || '').length,
  };
}

function tombstoneRustSeedChunk(seed) {
  const deadline = Date.now() + 60000;
  let fileRowId = '';
  let file = null;
  let chunkRowId = '';
  let chunk = null;
  while (Date.now() < deadline) {
    const fileRow = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_files__v0
      WHERE id='${sqlString(seed.id)}'
      LIMIT 1;
    `).trim();
    const chunkRow = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_file_chunks__v0
      WHERE json_extract(data, '$.file_id')='${sqlString(seed.id)}'
      ORDER BY CAST(json_extract(data, '$.idx') AS INTEGER), id
      LIMIT 1;
    `).trim();
    if (fileRow && chunkRow) {
      const fileSplitAt = fileRow.indexOf('\t');
      const chunkSplitAt = chunkRow.indexOf('\t');
      fileRowId = fileSplitAt >= 0 ? fileRow.slice(0, fileSplitAt) : '';
      chunkRowId = chunkSplitAt >= 0 ? chunkRow.slice(0, chunkSplitAt) : '';
      file = JSON.parse(fileSplitAt >= 0 ? fileRow.slice(fileSplitAt + 1) : fileRow);
      chunk = JSON.parse(chunkSplitAt >= 0 ? chunkRow.slice(chunkSplitAt + 1) : chunkRow);
      break;
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  if (!fileRowId || !file || !chunkRowId || !chunk) {
    throw new Error(`no SQLite file/chunk row found for tombstone corruption: ${seed.id}`);
  }
  const now = Date.now();
  const fileRevisionHeight = Number(String(file._rev || '').split('-')[0] || 1) + 1;
  const chunkRevisionHeight = Number(String(chunk._rev || '').split('-')[0] || 1) + 1;
  const fileRevision = `${fileRevisionHeight}-${token(10)}`;
  const chunkRevision = `${chunkRevisionHeight}-${token(10)}`;
  file.path = '';
  file.local_path = '';
  file.content_state = 'available';
  file._rev = fileRevision;
  file._meta = { ...(file._meta || {}), lwt: now };
  chunk._deleted = true;
  chunk.deleted = true;
  chunk.is_deleted = true;
  chunk._rev = chunkRevision;
  chunk._meta = { ...(chunk._meta || {}), lwt: now + 1 };
  sqlite(`
    UPDATE ctox_business_os__desktop_files__v0
    SET data='${sqlString(JSON.stringify(file))}',
        revision='${sqlString(fileRevision)}',
        lastWriteTime=${now}
    WHERE id='${sqlString(fileRowId)}';
  `);
  sqlite(`
    UPDATE ctox_business_os__desktop_file_chunks__v0
    SET data='${sqlString(JSON.stringify(chunk))}',
        revision='${sqlString(chunkRevision)}',
        lastWriteTime=${now + 1},
        deleted=1
    WHERE id='${sqlString(chunkRowId)}';
  `);
  return {
    fileRowId,
    chunkRowId,
    fileId: seed.id,
    fileRevision,
    chunkRevision,
  };
}

function staleRustSeedChunkGeneration(seed) {
  const deadline = Date.now() + 60000;
  let fileRowId = '';
  let file = null;
  let chunks = [];
  while (Date.now() < deadline) {
    const fileRow = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_files__v0
      WHERE id='${sqlString(seed.id)}'
      LIMIT 1;
    `).trim();
    const chunkRows = sqlite(`
      SELECT id || char(9) || data
      FROM ctox_business_os__desktop_file_chunks__v0
      WHERE json_extract(data, '$.file_id')='${sqlString(seed.id)}'
      ORDER BY CAST(json_extract(data, '$.idx') AS INTEGER), id;
    `).trim().split(/\n+/).filter(Boolean);
    if (fileRow && chunkRows.length) {
      const fileSplitAt = fileRow.indexOf('\t');
      fileRowId = fileSplitAt >= 0 ? fileRow.slice(0, fileSplitAt) : '';
      file = JSON.parse(fileSplitAt >= 0 ? fileRow.slice(fileSplitAt + 1) : fileRow);
      chunks = chunkRows.map((row) => {
        const splitAt = row.indexOf('\t');
        const rowId = splitAt >= 0 ? row.slice(0, splitAt) : '';
        const data = JSON.parse(splitAt >= 0 ? row.slice(splitAt + 1) : row);
        return { rowId, data };
      });
      break;
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  if (!fileRowId || !file || !chunks.length) {
    throw new Error(`no SQLite file/chunk rows found for stale generation corruption: ${seed.id}`);
  }
  const now = Date.now();
  const requestedGenerationId = `gen_missing_${token(16)}`;
  const fileRevisionHeight = Number(String(file._rev || '').split('-')[0] || 1) + 1;
  const fileRevision = `${fileRevisionHeight}-${token(10)}`;
  file.path = '';
  file.local_path = '';
  file.content_state = 'available';
  file.content_generation_id = requestedGenerationId;
  file._rev = fileRevision;
  file._meta = { ...(file._meta || {}), lwt: now };
  sqlite(`
    UPDATE ctox_business_os__desktop_files__v0
    SET data='${sqlString(JSON.stringify(file))}',
        revision='${sqlString(fileRevision)}',
        lastWriteTime=${now}
    WHERE id='${sqlString(fileRowId)}';
  `);
  return {
    fileRowId,
    fileId: seed.id,
    fileRevision,
    requestedGenerationId,
    availableGenerationIds: [...new Set(chunks.map((chunk) => chunk.data.generation_id || '').filter(Boolean))],
    liveChunkCount: chunks.length,
  };
}

async function stopChild(child) {
  if (!child || child.exitCode !== null) return;
  terminateOwnedSmokeChild(child, 'SIGINT', 'smoke-finalizer', 'graceful-stop');
  await new Promise((resolve) => {
    const timer = setTimeout(() => {
      if (child.exitCode === null) {
        terminateOwnedSmokeChild(child, 'SIGKILL', 'smoke-finalizer', 'graceful-stop-timeout');
      }
      resolve();
    }, 15000);
    child.once('exit', () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

function startCtoxServer() {
  setSmokeStartupPhase('ctox-start');
  const env = {
    ...process.env,
    CTOX_BUSINESS_OS_SIGNALING_URLS: signalingUrl,
    CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS: '1',
    CTOX_ROOT: runtimeRoot,
    CARGO_TARGET_DIR: path.join(root, 'runtime/build/core-rxdb-integration-target'),
    CTOX_BROWSER_AUTOMATION_MODULE: process.env.CTOX_BROWSER_AUTOMATION_MODULE || playwrightModule,
    CTOX_WEBRTC_UDP_BIND_ADDR: process.env.CTOX_WEBRTC_UDP_BIND_ADDR || '127.0.0.1:0',
    CTOX_SMOKE_RUN_ID: smokeRunId,
  };
  const browserExecutable = process.env.CTOX_BROWSER_EXECUTABLE || existingChromeExecutable();
  if (browserExecutable) env.CTOX_BROWSER_EXECUTABLE = browserExecutable;
  if (smokeMode !== 'workspace-agent-artifacts-background-rust-to-browser') {
    env.CTOX_BUSINESS_OS_DISABLE_BACKGROUND_FILE_INDEX = '1';
  }
  const child = trackSmokeChild(spawn(ctoxBin, ['business-os', 'serve', '--addr', `127.0.0.1:${businessPort}`], {
    cwd: root,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  }), 'ctox-business-os');
  let resolveListening;
  let rejectListening;
  let sawListening = false;
  child.__ctoxListening = new Promise((resolve, reject) => {
    resolveListening = resolve;
    rejectListening = reject;
  });
  child.stdout.on('data', (d) => {
    const text = d.toString();
    process.stdout.write(`[ctox] ${d}`);
    if (text.includes('CTOX Business OS listening')) {
      sawListening = true;
      setSmokeStartupPhase('ctox-listening-output');
      resolveListening?.();
    }
  });
  child.stderr.on('data', (d) => process.stderr.write(`[ctox:err] ${d}`));
  globalThis.__ctoxProcess = child;
  child.on('exit', (code, signal) => {
    if (!sawListening) rejectListening?.(new Error(`ctox exited before listening: code=${code} signal=${signal}`));
    console.log(`[ctox:exit] code=${code} signal=${signal}`);
  });
  return child;
}

async function waitForCtoxServerListening(child, ms = 60000) {
  const deadline = Date.now() + ms;
  await waitForChildReady(child?.__ctoxListening, ms, 'ctox business-os server');
  while (Date.now() < deadline) {
    if (child && child.exitCode !== null) {
      throw new Error(`ctox exited after startup output before HTTP readiness: code=${child.exitCode} signal=${child.signalCode}`);
    }
    const connected = await new Promise((resolve) => {
      const socket = net.createConnection({ host: '127.0.0.1', port: businessPort });
      const finish = (value) => {
        socket.destroy();
        resolve(value);
      };
      socket.setTimeout(500, () => finish(false));
      socket.once('connect', () => finish(true));
      socket.once('error', () => finish(false));
    });
    if (connected) {
      setSmokeStartupPhase('ctox-ready');
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timeout waiting for ctox TCP listener on 127.0.0.1:${businessPort}`);
}

function ensureCtoxSmokeBinary() {
  if (process.env.CTOX_BIN || process.env.CTOX_SKIP_SMOKE_BUILD === '1') return;
  const targetDir = path.join(root, 'runtime/build/core-rxdb-integration-target');
  const timeoutMs = parsePositiveIntegerEnv(
    'CTOX_SMOKE_BUILD_TIMEOUT_MS',
    process.env.CTOX_SMOKE_BUILD_TIMEOUT_MS || '1800000',
    { max: 7200000 },
  );
  const args = [
    'build',
    '--locked',
    '--bin',
    'ctox',
    '--no-default-features',
    '--target-dir',
    targetDir,
  ];
  console.log(`ctox_smoke_build_command=cargo ${args.join(' ')}`);
  console.log(`ctox_smoke_build_timeout_ms=${timeoutMs}`);
  const result = spawnSync('cargo', args, {
    cwd: root,
    env: {
      ...process.env,
      CARGO_TARGET_DIR: targetDir,
    },
    stdio: 'inherit',
    timeout: timeoutMs,
  });
  if (result.error) {
    throw new Error(`ctox smoke binary build failed: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`ctox smoke binary build failed with exit status ${result.status}`);
  }
  if (!fs.existsSync(ctoxBin)) {
    throw new Error(`ctox smoke binary was not produced at ${ctoxBin}`);
  }
}

(async () => {
  ensureCtoxSmokeBinary();
  let signaling = await startSignalingServer();
  console.log(`signaling=${signalingUrl}`);
  console.log(`smoke_root_prepare_ms=${smokeRootPrepareMs}`);
  const workspaceFileMode = smokeMode === 'workspace-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-background-rust-to-browser'
    || smokeMode === 'workspace-update-rust-to-browser'
    || smokeMode === 'workspace-large-materialize-rust-to-browser'
    || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
    || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser'
    || smokeMode === 'file-chunk-metadata-error-browser-status'
    || smokeMode === 'file-chunk-tombstone-error-browser-status'
    || smokeMode === 'file-chunk-stale-generation-error-browser-status';
  const workspaceArtifactMode = smokeMode === 'workspace-agent-artifacts-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-background-rust-to-browser';
  const rustSeed = smokeMode === 'workspace-agent-artifacts-rust-to-browser'
    || smokeMode === 'workspace-agent-artifacts-background-rust-to-browser'
    ? seedRustWorkspaceArtifacts()
    : (smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser'
        || smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser')
      ? seedRustWorkspaceArtifactStress()
      : workspaceFileMode
        ? seedRustWorkspaceFile({
          large: smokeMode === 'workspace-large-materialize-rust-to-browser'
            || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
            || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser',
        })
        : seedRustSideFile(smokeMode === 'rust-to-browser' ? 'rust_smoke' : 'rust_ready');
  const backgroundQueueTask = smokeMode === 'workspace-agent-artifacts-background-rust-to-browser'
    ? createWorkspaceQueueTaskForBackgroundIndex(rustSeed)
    : null;
  const nativeSchemaDriftFixture = smokeMode === 'native-schema-drift-browser-status'
    ? seedNativeOptionalSchemaDriftFixture()
    : null;
  if (nativeSchemaDriftFixture) {
    console.log(`native_schema_drift_seed=${JSON.stringify(nativeSchemaDriftFixture)}`);
  }
  if (smokeMode === 'business-os-app-release-ui') {
    await seedBusinessOsReleaseNativeSetup();
  }
  const freshProfileScaleSeed = smokeMode === 'business-os-fresh-profile-ui'
    ? seedBusinessOsFreshProfileScaleNativeSetup()
    : null;
  if (smokeMode === 'business-os-roles-permissions-ui') {
    await seedBusinessOsRolesPermissionsNativeUsers();
  }
  let threadsRightClickCapabilities = null;
  if (smokeMode === 'business-os-threads-rightclick-ui' || smokeMode === 'business-os-threads-scale-ui') {
    await seedBusinessOsThreadsRightClickNativeUsers();
  }
  let threadsScaleSeed = null;
  let ctox = startCtoxServer();
  const browserDiagnostics = {
    warnings: 0,
    websocketWarnings: 0,
    errors: 0,
    resource404Errors: 0,
    requestFailures: 0,
    assetResponseErrors: 0,
    smokeHookReloads: 0,
    smokeHookWaitMs: 0,
    cacheRepairs: 0,
    expectedNetworkFlapWarnings: 0,
    expectedNetworkFlapErrors: 0,
    expectedNetworkFlapRequestFailures: 0,
  };
  let browserDiagnosticsEmitted = false;
  function emitBrowserDiagnostics() {
    if (browserDiagnosticsEmitted) return;
    browserDiagnosticsEmitted = true;
    console.log(`browser_warning_count=${browserDiagnostics.warnings}`);
    console.log(`browser_websocket_warning_count=${browserDiagnostics.websocketWarnings}`);
    console.log(`browser_error_count=${browserDiagnostics.errors}`);
    console.log(`browser_resource_404_count=${browserDiagnostics.resource404Errors}`);
    console.log(`browser_request_failure_count=${browserDiagnostics.requestFailures}`);
    console.log(`browser_asset_response_error_count=${browserDiagnostics.assetResponseErrors}`);
    console.log(`browser_cache_repair_count=${browserDiagnostics.cacheRepairs}`);
    console.log(`expected_network_flap_warning_count=${browserDiagnostics.expectedNetworkFlapWarnings}`);
    console.log(`expected_network_flap_error_count=${browserDiagnostics.expectedNetworkFlapErrors}`);
    console.log(`expected_network_flap_request_failure_count=${browserDiagnostics.expectedNetworkFlapRequestFailures}`);
    console.log(`startup_smoke_hook_reload_count=${browserDiagnostics.smokeHookReloads}`);
    console.log(`startup_smoke_hook_wait_ms=${browserDiagnostics.smokeHookWaitMs}`);
    if (browserDiagnostics.cacheRepairs > 0) {
      throw new Error(`Business OS local RxDB cache repair was triggered during smoke: ${browserDiagnostics.cacheRepairs}`);
    }
  }
  function isExpectedNetworkFlapConsole(text) {
    return smokeMode === 'network-flap-browser-to-rust' && (
      /ERR_INTERNET_DISCONNECTED/i.test(text)
      || /ctox_signaling_socket_error/i.test(text)
      || /packaged module catalog seed unavailable/i.test(text)
    );
  }
  function isExpectedBusinessOsPermissionConsole(text) {
    return /BusinessOsPermissionError|CTOX_BUSINESS_OS_PERMISSION_DENIED|Kein Leserecht für/i.test(String(text || ''));
  }
  function isExpectedNetworkFlapRequestFailure(request) {
    const failureText = request.failure()?.errorText || '';
    return smokeMode === 'network-flap-browser-to-rust'
      && /ERR_INTERNET_DISCONNECTED/i.test(failureText)
      && request.url().includes('/business-os/modules/registry.json');
  }

  let browser;
  let browserUserDataDir = null;
  let freshProfileInitialStorage = null;
  const outerPhaseTimings = {};
  try {
    const ctoxServerWaitStartedAt = Date.now();
    await waitForCtoxServerListening(ctox, serverReadyTimeoutMs);
    outerPhaseTimings.ctoxServerWaitMs = Date.now() - ctoxServerWaitStartedAt;
    const configWaitStartedAt = Date.now();
    const config = await waitForLaunchSyncConfig(syncConfigWaitMs);
    outerPhaseTimings.syncConfigWaitMs = Date.now() - configWaitStartedAt;
    console.log(`ctox_sync_config_wait_ms=${outerPhaseTimings.syncConfigWaitMs}`);
    if (!config.native_rxdb_peer_available) {
      throw new Error(`native peer unavailable: ${JSON.stringify(config)}`);
    }
    if (smokeMode === 'business-os-threads-scale-ui') {
      const scaleSeedStartedAt = Date.now();
      threadsScaleSeed = await seedBusinessOsThreadsScaleNativeSetup();
      outerPhaseTimings.threadsScaleSeedMs = Date.now() - scaleSeedStartedAt;
      console.log(`business_os_threads_rightclick_scale_seed_ms=${outerPhaseTimings.threadsScaleSeedMs}`);
    }
    if (smokeMode === 'native-schema-drift-browser-status') {
      await waitForNativePeerSyncConfig(syncConfigWaitMs);
    }
    const browserLaunchStartedAt = Date.now();
    browserUserDataDir = fs.mkdtempSync(path.join(runtimeRoot, 'browser-profile-'));
    if (smokeMode === 'business-os-fresh-profile-ui') {
      const profileEntries = fs.readdirSync(browserUserDataDir);
      freshProfileInitialStorage = {
        cleanIndexedDb: !fs.existsSync(path.join(browserUserDataDir, 'Default', 'IndexedDB')),
        cleanLocalStorage: !fs.existsSync(path.join(browserUserDataDir, 'Default', 'Local Storage')),
        cleanSessionStorage: !fs.existsSync(path.join(browserUserDataDir, 'Default', 'Session Storage')),
        emptyProfile: profileEntries.length === 0,
      };
    }
    browser = await chromium.launchPersistentContext(browserUserDataDir, chromiumLaunchOptions());
    const page = await browser.newPage();
    outerPhaseTimings.browserLaunchMs = Date.now() - browserLaunchStartedAt;
    page.on('console', (msg) => {
      const type = msg.type();
      const text = msg.text();
      if (/local RxDB cache repair triggered/i.test(text)) {
        browserDiagnostics.cacheRepairs += 1;
      }
      if (type === 'warning') {
        if (isExpectedNetworkFlapConsole(text)) {
          browserDiagnostics.expectedNetworkFlapWarnings += 1;
        } else {
          browserDiagnostics.warnings += 1;
          if (/websocket/i.test(text)) browserDiagnostics.websocketWarnings += 1;
        }
      } else if (type === 'error') {
        if (/failed to load resource/i.test(text) && /status of 404/i.test(text)) {
          browserDiagnostics.resource404Errors += 1;
        } else if (isExpectedNetworkFlapConsole(text)) {
          browserDiagnostics.expectedNetworkFlapErrors += 1;
        } else if (smokeMode === 'business-os-ui-regression' && isExpectedBusinessOsPermissionConsole(text)) {
          browserDiagnostics.expectedNetworkFlapErrors += 1;
        } else {
          browserDiagnostics.errors += 1;
        }
      }
      console.log(`[browser:${type}] ${text}`);
    });
    page.on('pageerror', (err) => {
      if (smokeMode === 'business-os-ui-regression' && isExpectedBusinessOsPermissionConsole(err?.stack || err?.message || '')) {
        browserDiagnostics.expectedNetworkFlapErrors += 1;
      } else {
        browserDiagnostics.errors += 1;
      }
      console.error(`[browser:error] ${err.stack || err.message}`);
    });
    page.on('requestfailed', (request) => {
      const url = request.url();
      if (url.includes('/app.js') || url.includes('/shared/') || url.includes('/modules/') || url.includes('/installed-modules/') || url.includes('/vendor/')) {
        if (isExpectedNetworkFlapRequestFailure(request)) {
          browserDiagnostics.expectedNetworkFlapRequestFailures += 1;
        } else {
          browserDiagnostics.requestFailures += 1;
        }
        console.error(`[browser:requestfailed] ${request.method()} ${url} ${request.failure()?.errorText || ''}`);
      }
    });
    page.on('response', (response) => {
      const url = response.url();
      if (response.status() >= 400 && (url.includes('/app.js') || url.includes('/shared/') || url.includes('/modules/') || url.includes('/installed-modules/') || url.includes('/vendor/'))) {
        browserDiagnostics.assetResponseErrors += 1;
        console.error(`[browser:response] ${response.status()} ${url}`);
      }
    });
    const outboundScreenshotDir = process.env.SMOKE_OUTBOUND_SCREENSHOT_DIR
      ? path.resolve(process.env.SMOKE_OUTBOUND_SCREENSHOT_DIR)
      : '';
    if (outboundScreenshotDir) fs.mkdirSync(outboundScreenshotDir, { recursive: true });
    await page.exposeFunction('__ctoxCaptureSmokeScreenshot', async (name) => {
      if (!outboundScreenshotDir) return '';
      const safeName = String(name || 'screenshot').replace(/[^a-z0-9_.-]+/gi, '-').replace(/^-+|-+$/g, '') || 'screenshot';
      const target = path.join(outboundScreenshotDir, `${safeName}.png`);
      await page.screenshot({ path: target, fullPage: false });
      return target;
    });
    await page.exposeFunction('__ctoxSyncRustSeedFile', () => syncRustSeedFile(rustSeed));
    await page.exposeFunction('__ctoxUpdateRustSeedFile', (content) => {
      fs.writeFileSync(rustSeed.path, content);
      rustSeed.content = content;
      syncRustSeedFile(rustSeed);
      return {
        id: rustSeed.id,
        content,
        path: rustSeed.path,
        expectedVirtualPath: rustSeed.expectedVirtualPath || '',
      };
    });
    await page.exposeFunction('__ctoxMutateRustWorkspaceArtifacts', () => mutateRustWorkspaceArtifactStress(rustSeed));
    await page.exposeFunction('__ctoxCreateWorkspaceQueueTaskForBackgroundIndex', () => createWorkspaceQueueTaskForBackgroundIndex(rustSeed));
    await page.exposeFunction('__ctoxCorruptRustSeedChunkMetadata', () => corruptRustSeedChunkMetadata(rustSeed));
    await page.exposeFunction('__ctoxTombstoneRustSeedChunk', () => tombstoneRustSeedChunk(rustSeed));
    await page.exposeFunction('__ctoxStaleRustSeedChunkGeneration', () => staleRustSeedChunkGeneration(rustSeed));
    await page.exposeFunction('__ctoxRestartNativePeer', async () => {
      await stopChild(ctox);
      ctox = startCtoxServer();
      await waitForCtoxServerListening(ctox, serverReadyTimeoutMs);
      await waitForNativePeerSyncConfig(60000);
      await waitForSqliteTables([
        'ctox_business_os__desktop_files__v0',
        'ctox_business_os__desktop_file_chunks__v0',
      ]);
      return true;
    });
    await page.exposeFunction('__ctoxStopNativePeerForRestoreSmoke', async () => {
      await stopChild(ctox);
      ctox = null;
      return true;
    });
    await page.exposeFunction('__ctoxStartNativePeerForRestoreSmoke', async () => {
      if (ctox) await stopChild(ctox);
      ctox = startCtoxServer();
      await waitForCtoxServerListening(ctox, serverReadyTimeoutMs);
      await waitForNativePeerSyncConfig(60000);
      await waitForSqliteTables([
        'ctox_business_os__desktop_files__v0',
        'ctox_business_os__desktop_file_chunks__v0',
      ]);
      return true;
    });
    await page.exposeFunction('__ctoxSqliteFileExistsForRestoreSmoke', async (id) => {
      const safeId = sqlString(String(id || ''));
      const fileRows = Number(sqlite(`SELECT COUNT(*) FROM ctox_business_os__desktop_files__v0 WHERE id='${safeId}';`).trim() || '0');
      const chunkRows = Number(sqlite(`SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 WHERE id='${sqlString(`${id}_0`)}';`).trim() || '0');
      return fileRows > 0 || chunkRows > 0;
    });
    await page.exposeFunction('__ctoxRolloverNativePeerInProcess', async () => {
      const res = await fetch(`http://127.0.0.1:${businessPort}/api/business-os/sync/native-peer/restart`, {
        method: 'POST',
      });
      if (!res.ok) {
        throw new Error(`native peer in-process restart failed: ${res.status} ${await res.text()}`);
      }
      const status = await res.json();
      await waitForNativePeerSyncConfig(60000);
      await waitForSqliteTables([
        'ctox_business_os__desktop_files__v0',
        'ctox_business_os__desktop_file_chunks__v0',
      ]);
      return status;
    });
    await page.exposeFunction('__ctoxRestartSignalingAndNativePeer', async () => {
      await stopChild(ctox);
      await stopSignalingServer(signaling);
      signaling = await startSignalingServer();
      ctox = startCtoxServer();
      await waitForCtoxServerListening(ctox, serverReadyTimeoutMs);
      await waitForNativePeerSyncConfig(60000);
      await waitForSqliteTables([
        'ctox_business_os__desktop_files__v0',
        'ctox_business_os__desktop_file_chunks__v0',
      ]);
      return true;
    });
    const appPagePath = useAppDb
      && smokeMode === 'business-os-ui-regression'
      && !pagePath.includes('#')
      ? `${pagePath}#ctox`
      : pagePath;
    const browserPath = useAppDb
      ? addQueryParam(addQueryParam(appPagePath, 'rxdbSmoke', '1'), 'smokeDbId', smokeDbId)
      : pagePath;
    const smokeUrl = `http://127.0.0.1:${businessPort}${browserPath}`;
    const pageGotoStartedAt = Date.now();
    await page.goto(smokeUrl, { waitUntil: 'commit', timeout: pageNavigationTimeoutMs });
    outerPhaseTimings.pageGotoMs = Date.now() - pageGotoStartedAt;
    let advancedStatusEvidenceVersion = '';
    let advancedStatusEvidenceRuntime = null;
    const backgroundIndexerSmokeMode = smokeMode === 'workspace-agent-artifacts-background-rust-to-browser';
    const largeFileMaterializeSmokeMode = smokeMode === 'workspace-large-materialize-rust-to-browser'
      || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
      || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser';
    const deferredFileCollectionStartupMode = backgroundIndexerSmokeMode
      || smokeMode === 'file-chunk-tombstone-error-browser-status'
      || smokeMode === 'business-os-app-release-ui';
    if (useAppDb) {
      let startupState = null;
      const smokeHookWaitStartedAt = Date.now();
      for (let attempt = 0; attempt < 2; attempt += 1) {
        try {
          await page.waitForFunction(() => Boolean(
            globalThis.ctoxBusinessOsSmoke
              && globalThis.ctoxBusinessOsSmoke.bootstrap !== 'inline'
              && globalThis.CTOX_BUSINESS_OS_STATUS
          ), null, { timeout: smokeHookWaitTimeoutMs });
          startupState = null;
          break;
        } catch (error) {
          startupState = await collectStartupState(page);
          if (attempt === 0 && isPreHookModuleGraphStall(startupState)) {
            console.warn(`[smoke] Business OS module graph stalled before smoke hook; reloading once: ${JSON.stringify({
              url: startupState.url,
              search: startupState.search,
              readyState: startupState.readyState,
              hasSmoke: startupState.hasSmoke,
              smokeBootstrap: startupState.smokeBootstrap,
              hasAdvancedStatus: startupState.hasAdvancedStatus,
              scriptSrcs: startupState.scriptSrcs,
              resources: startupState.resources,
              bodyDataset: startupState.bodyDataset,
            })}`);
            browserDiagnostics.smokeHookReloads += 1;
            await page.goto('about:blank', { waitUntil: 'commit', timeout: pageNavigationTimeoutMs }).catch(() => {});
            await page.goto(smokeUrl, { waitUntil: 'commit', timeout: pageNavigationTimeoutMs });
            continue;
          }
          break;
        }
      }
      browserDiagnostics.smokeHookWaitMs = Date.now() - smokeHookWaitStartedAt;
      outerPhaseTimings.smokeHookWaitMs = browserDiagnostics.smokeHookWaitMs;
      if (startupState) {
        throw new Error(`Business OS smoke hook did not initialize: ${JSON.stringify(startupState, null, 2)}`);
      }
      if (smokeMode === 'signaling-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const deadline = Date.now() + 60000;
          let lastSnapshot = null;
          while (Date.now() < deadline) {
            lastSnapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: false });
            const collectionErrors = Array.isArray(lastSnapshot?.sync?.collectionErrors)
              ? lastSnapshot.sync.collectionErrors
              : [];
            const match = collectionErrors.find((error) => (
              error?.name === 'CtoxSignalingControlPlaneError' &&
              error?.code === 'instance_mismatch'
            ));
            if (match) return { ok: true, error: match, snapshot: lastSnapshot };
            await new Promise((resolve) => setTimeout(resolve, 250));
          }
          return { ok: false, snapshot: lastSnapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose injected signaling error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`signaling_error_collection=${errorStatus.error.collection}`);
        console.log(`signaling_error_code=${errorStatus.error.code}`);
        console.log(`signaling_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'peer-lifecycle-browser-status') {
        const lifecycleStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'reconnecting',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1'],
            collections: {
              peer_lifecycle_fixture: {
                collection: 'peer_lifecycle_fixture',
                status: 'reconnecting',
                connectionStatus: 'reconnecting',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1'],
                remotePeerSession: 'ctox_instance:lifecycle-fixture',
                remoteCheckpoint: {
                  source: 'ctox-rs',
                  state: 'advertised',
                  collection: 'peer_lifecycle_fixture',
                  epoch: 'peer-lifecycle-fixture-epoch',
                },
                peerGeneration: 2,
                previousPeerSession: 'ctox_instance:lifecycle-fixture-old',
                peerSessionSeenAt: new Date().toISOString(),
                reconnectingSince: new Date().toISOString(),
                lastError: null,
                lastLifecycleEvent: {
                  name: 'CtoxWebRtcPeerLifecycleEvent',
                  code: 'peer_connection_lost',
                  phase: 'peer-reconnect',
                  severity: 'recoverable',
                  retryable: true,
                  lifecycle: true,
                  message: 'WebRTC peer connection was lost; reconnect repair is scheduled.',
                },
              },
            },
            lastError: null,
            lastLifecycleEvent: {
              name: 'CtoxWebRtcPeerLifecycleEvent',
              code: 'peer_connection_lost',
              phase: 'peer-reconnect',
              severity: 'recoverable',
              retryable: true,
              lifecycle: true,
              message: 'WebRTC peer connection was lost; reconnect repair is scheduled.',
            },
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['peer_lifecycle_fixture'],
          });
          const lifecycleEvents = Array.isArray(snapshot?.sync?.lifecycleEvents)
            ? snapshot.sync.lifecycleEvents
            : [];
          const reconnectingCollections = Array.isArray(snapshot?.sync?.reconnectingCollections)
            ? snapshot.sync.reconnectingCollections
            : [];
          const match = lifecycleEvents.find((event) => (
            event?.name === 'CtoxWebRtcPeerLifecycleEvent' &&
            event?.code === 'peer_connection_lost' &&
            event?.phase === 'peer-reconnect' &&
            event?.severity === 'recoverable' &&
            event?.retryable === true
          ));
          return {
            ok: Boolean(
              match &&
              reconnectingCollections.includes('peer_lifecycle_fixture') &&
              snapshot?.checks?.noStalledReconnect === false &&
              snapshot?.checks?.noReplicationIoErrors === true
            ),
            error: match || null,
            snapshot,
          };
        });
        if (!lifecycleStatus?.ok) {
          throw new Error(`Business OS did not expose peer lifecycle event in advanced status: ${JSON.stringify(lifecycleStatus, null, 2)}`);
        }
        console.log(`peer_lifecycle_collection=${lifecycleStatus.error.collection}`);
        console.log(`peer_lifecycle_code=${lifecycleStatus.error.code}`);
        console.log(`peer_lifecycle_name=${lifecycleStatus.error.name}`);
        console.log(`peer_lifecycle_phase=${lifecycleStatus.error.phase}`);
        if (lifecycleStatus.snapshot?.version) console.log(`advanced_status=${lifecycleStatus.snapshot.version}`);
        if (lifecycleStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(lifecycleStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'checkpoint-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1', 'ctox-checkpoint-epoch-v1'],
            collections: {
              checkpoint_fixture: {
                collection: 'checkpoint_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1', 'ctox-checkpoint-epoch-v1'],
                remotePeerSession: 'ctox_instance:checkpoint-fixture',
                remoteCheckpoint: null,
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'CtoxCheckpointProtocolError',
                  code: 'ctox_checkpoint_epoch_missing',
                  phase: 'checkpoint-handshake',
                  severity: 'error',
                  retryable: false,
                  message: 'Remote RxDB peer did not provide advertised checkpoint epoch evidence.',
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['checkpoint_fixture'],
          });
          const checkpointErrors = Array.isArray(snapshot?.sync?.checkpointErrors)
            ? snapshot.sync.checkpointErrors
            : [];
          const match = checkpointErrors.find((error) => (
            error?.name === 'CtoxCheckpointProtocolError' &&
            error?.code === 'ctox_checkpoint_epoch_missing' &&
            error?.phase === 'checkpoint-handshake'
          ));
          return { ok: Boolean(match && snapshot?.checks?.noCheckpointProtocolErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose checkpoint protocol error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`checkpoint_error_collection=${errorStatus.error.collection}`);
        console.log(`checkpoint_error_code=${errorStatus.error.code}`);
        console.log(`checkpoint_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'schema-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1', 'ctox-schema-hash-v1'],
            collections: {
              schema_fixture: {
                collection: 'schema_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1', 'ctox-schema-hash-v1'],
                remotePeerSession: 'ctox_instance:schema-fixture',
                remoteCheckpoint: {
                  source: 'ctox-rs',
                  state: 'advertised',
                  collection: 'schema_fixture',
                  schemaHash: 'actual-schema-hash',
                  epoch: 'schema-fixture-epoch',
                },
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'CtoxSchemaProtocolError',
                  code: 'ctox_schema_hash_mismatch',
                  phase: 'schema-handshake',
                  severity: 'error',
                  retryable: false,
                  expected: 'expected-schema-hash',
                  actual: 'actual-schema-hash',
                  message: 'Remote RxDB peer collection schema hash does not match the Browser schema.',
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['schema_fixture'],
          });
          const schemaErrors = Array.isArray(snapshot?.sync?.schemaErrors)
            ? snapshot.sync.schemaErrors
            : [];
          const match = schemaErrors.find((error) => (
            error?.name === 'CtoxSchemaProtocolError' &&
            error?.code === 'ctox_schema_hash_mismatch' &&
            error?.phase === 'schema-handshake' &&
            error?.expected === 'expected-schema-hash' &&
            error?.actual === 'actual-schema-hash'
          ));
          return { ok: Boolean(match && snapshot?.checks?.noSchemaProtocolErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose schema protocol error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`schema_error_collection=${errorStatus.error.collection}`);
        console.log(`schema_error_code=${errorStatus.error.code}`);
        console.log(`schema_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'rxdb-protocol-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1', 'ctox-schema-hash-v1', 'ctox-checkpoint-epoch-v1'],
            collections: {
              protocol_fixture: {
                collection: 'protocol_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v0',
                remoteCapabilities: ['ctox-peer-session-v1', 'ctox-schema-hash-v1', 'ctox-checkpoint-epoch-v1'],
                remotePeerSession: 'ctox_instance:protocol-fixture',
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'CtoxRxdbProtocolError',
                  code: 'ctox_rxdb_protocol_mismatch',
                  phase: 'rxdb-protocol-handshake',
                  severity: 'error',
                  retryable: false,
                  expected: 'ctox-rxdb-protocol-v1',
                  actual: 'ctox-rxdb-protocol-v0',
                  message: 'Incompatible CTOX RxDB WebRTC protocol.',
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['protocol_fixture'],
          });
          const schemaErrors = Array.isArray(snapshot?.sync?.schemaErrors)
            ? snapshot.sync.schemaErrors
            : [];
          const match = schemaErrors.find((error) => (
            error?.name === 'CtoxSchemaProtocolError' &&
            error?.code === 'ctox_rxdb_protocol_mismatch' &&
            error?.phase === 'schema-handshake' &&
            error?.expected === 'ctox-rxdb-protocol-v1' &&
            error?.actual === 'ctox-rxdb-protocol-v0'
          ));
          return { ok: Boolean(match && snapshot?.checks?.noSchemaProtocolErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose RxDB protocol incompatibility in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`rxdb_protocol_error_collection=${errorStatus.error.collection}`);
        console.log(`rxdb_protocol_error_code=${errorStatus.error.code}`);
        console.log(`rxdb_protocol_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'replication-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1'],
            collections: {
              replication_fixture: {
                collection: 'replication_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1'],
                remotePeerSession: 'ctox_instance:replication-fixture',
                remoteCheckpoint: {
                  source: 'ctox-rs',
                  state: 'advertised',
                  collection: 'replication_fixture',
                  epoch: 'replication-fixture-epoch',
                },
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'RxError (RC_PULL)',
                  code: 'RC_PULL',
                  phase: 'replication-pull',
                  message: 'Rust WebRTC masterChangesSince failed.',
                  parameters: {
                    type: 'ctoxError',
                    scope: 'replication',
                    rxdb: true,
                    code: 'RC_PULL',
                    phase: 'replication-pull',
                    direction: 'pull',
                    checkpoint: { sequence: 1 },
                    batchSize: 20,
                    errors: [
                      {
                        rxdb: true,
                        code: 'TEST_PULL',
                        name: 'RxError (TEST_PULL)',
                        message: 'pull failed',
                        parameters: { attempt: 1 },
                      },
                    ],
                  },
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['replication_fixture'],
          });
          const replicationErrors = Array.isArray(snapshot?.sync?.replicationErrors)
            ? snapshot.sync.replicationErrors
            : [];
          const match = replicationErrors.find((error) => (
            error?.name === 'CtoxReplicationIoError' &&
            error?.code === 'ctox_replication_pull_failed' &&
            error?.phase === 'replication-pull' &&
            error?.direction === 'pull' &&
            error?.upstreamCode === 'RC_PULL' &&
            error?.batchSize === 20 &&
            error?.rowCount === null
          ));
          return { ok: Boolean(match && snapshot?.checks?.noReplicationIoErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose replication I/O error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`replication_error_collection=${errorStatus.error.collection}`);
        console.log(`replication_error_code=${errorStatus.error.code}`);
        console.log(`replication_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'replication-push-contract-error-browser-status') {
        const errorStatus = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state) return { ok: false, reason: 'missing smoke state' };
          state.session = { authenticated: true };
          state.sync = { mode: 'webrtc' };
          state.syncDiagnostics = {
            mode: 'webrtc',
            phase: 'collection-sync',
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1'],
            collections: {
              replication_push_fixture: {
                collection: 'replication_push_fixture',
                status: 'error',
                connectionStatus: 'error',
                remoteProtocol: 'ctox-rxdb-protocol-v1',
                remoteCapabilities: ['ctox-peer-session-v1'],
                remotePeerSession: 'ctox_instance:replication-push-fixture',
                remoteCheckpoint: {
                  source: 'ctox-rs',
                  state: 'advertised',
                  collection: 'replication_push_fixture',
                  epoch: 'replication-push-fixture-epoch',
                },
                peerGeneration: 1,
                peerSessionSeenAt: new Date().toISOString(),
                lastError: {
                  name: 'RxError (RC_PUSH_NO_AR)',
                  code: 'RC_PUSH_NO_AR',
                  phase: 'replication-push',
                  message: 'Rust WebRTC masterWrite returned an invalid push contract.',
                  parameters: {
                    type: 'ctoxError',
                    scope: 'replication',
                    rxdb: true,
                    code: 'RC_PUSH_NO_AR',
                    phase: 'replication-push',
                    direction: 'push',
                    pushRows: [
                      { newDocumentState: { id: 'push-a', _deleted: false } },
                      { newDocumentState: { id: 'push-b', _deleted: false } },
                    ],
                    message: 'fork masterWrite decode: invalid type: map, expected a sequence',
                  },
                },
              },
            },
            lastError: null,
          };
          const snapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['replication_push_fixture'],
          });
          const replicationErrors = Array.isArray(snapshot?.sync?.replicationErrors)
            ? snapshot.sync.replicationErrors
            : [];
          const match = replicationErrors.find((error) => (
            error?.name === 'CtoxReplicationIoError' &&
            error?.code === 'ctox_replication_push_contract_invalid' &&
            error?.phase === 'replication-push' &&
            error?.direction === 'push' &&
            error?.upstreamCode === 'RC_PUSH_NO_AR' &&
            error?.rowCount === 2 &&
            error?.retryable === false
          ));
          return { ok: Boolean(match && snapshot?.checks?.noReplicationIoErrors === false), error: match || null, snapshot };
        });
        if (!errorStatus?.ok) {
          throw new Error(`Business OS did not expose replication push contract error in advanced status: ${JSON.stringify(errorStatus, null, 2)}`);
        }
        console.log(`replication_push_error_collection=${errorStatus.error.collection}`);
        console.log(`replication_push_error_code=${errorStatus.error.code}`);
        console.log(`replication_push_error_name=${errorStatus.error.name}`);
        if (errorStatus.snapshot?.version) console.log(`advanced_status=${errorStatus.snapshot.version}`);
        if (errorStatus.snapshot?.rxdbRuntime) console.log(`rxdb_runtime=${JSON.stringify(errorStatus.snapshot.rxdbRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      try {
        const shellReadyWaitStartedAt = Date.now();
        if (deferredFileCollectionStartupMode) {
          await page.waitForFunction(() => {
            const state = globalThis.ctoxBusinessOsSmoke?.state;
            const modulesLoaded = Array.isArray(state?.modules) && state.modules.length > 0;
            return modulesLoaded && Boolean(state?.db?.raw) && Boolean(state?.sync);
          }, null, { timeout: 60000 });
        } else {
          await page.waitForFunction(() => {
            const state = globalThis.ctoxBusinessOsSmoke?.state;
            const modulesLoaded = Array.isArray(state?.modules) && state.modules.length > 0;
            const shellOpened = Boolean(document.body?.dataset?.moduleShell);
            const loading = Boolean(document.body?.dataset?.moduleLoading);
            return modulesLoaded && shellOpened && !loading;
          }, null, { timeout: 60000 });
        }
        outerPhaseTimings.shellReadyWaitMs = Date.now() - shellReadyWaitStartedAt;
      } catch (error) {
        const waitError = String(error?.message || error);
        const startupState = await page.evaluate(async (waitErrorMessage) => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: true }).catch((snapshotError) => ({
            snapshotError: String(snapshotError?.message || snapshotError),
          }));
          return {
            error: waitErrorMessage,
            hasSmoke: Boolean(globalThis.ctoxBusinessOsSmoke),
            authenticated: Boolean(state?.session?.authenticated),
            moduleCount: Array.isArray(state?.modules) ? state.modules.length : null,
            activeModule: state?.activeModule?.id || null,
            syncMode: state?.sync?.mode || null,
            syncDiagnostics: state?.syncDiagnostics || null,
            advancedStatus: status || null,
            bodyDataset: { ...document.body?.dataset },
            statusText: document.querySelector('[data-status]')?.textContent || '',
            visibleText: (document.body?.innerText || '').slice(0, 800),
          };
        }, waitError).catch((evalError) => ({ evaluateError: String(evalError?.message || evalError) }));
        throw new Error(`Business OS shell did not become ready: ${JSON.stringify(startupState, null, 2)}`);
      }
      const startupRequiredCollections = deferredFileCollectionStartupMode || largeFileMaterializeSmokeMode
        ? BUSINESS_OS_CORE_STATUS_COLLECTIONS
        : BUSINESS_OS_SHELL_STATUS_COLLECTIONS;
      const startupAdvancedStatusStartedAt = Date.now();
      // Startup readiness intentionally excludes demand-only chunk/blob
      // collections. File-focused smokes lease those collections explicitly
      // before adding them to strict advanced-status checks.
      let startupAdvancedStatusTimeoutMs = 60000;
      if (smokeMode === 'business-os-app-release-ui') {
        startupAdvancedStatusTimeoutMs = 240000;
      } else if (
        smokeMode === 'business-os-app-audience-ui'
        || smokeMode === 'business-os-threads-rightclick-ui'
      ) {
        startupAdvancedStatusTimeoutMs = 120000;
      }
      const advancedStatus = await waitForHealthyCompleteStatus(page, {
        timeoutMs: startupAdvancedStatusTimeoutMs,
        requiredCollections: startupRequiredCollections,
      });
      outerPhaseTimings.startupAdvancedStatusMs = Date.now() - startupAdvancedStatusStartedAt;
      if (!advancedStatus?.ok) {
        throw new Error(`Business OS advanced status unhealthy after startup: ${JSON.stringify(advancedStatus, null, 2)}`);
      }
      if (smokeMode !== 'business-os-app-release-ui') {
        assertHealthyAdvancedStatusContract(advancedStatus);
      }
      advancedStatusEvidenceVersion = advancedStatus.version || '';
      advancedStatusEvidenceRuntime = advancedStatus.rxdbRuntime || null;
      if (smokeMode === 'business-os-auth-scope-ui') {
        const requiredCollections = BUSINESS_OS_SHELL_STATUS_COLLECTIONS;
        const authSnapshot = async () => page.evaluate(async () => {
          const smoke = globalThis.ctoxBusinessOsSmoke;
          const state = smoke?.state || globalThis.CTOX_BUSINESS_OS_APP;
          let storageKeys = {};
          try {
            storageKeys = typeof smoke?.storageKeys === 'function' ? smoke.storageKeys() : {};
          } catch {}
          return {
            authState: document.body?.dataset?.authState || '',
            moduleShell: document.body?.dataset?.moduleShell || '',
            moduleLoading: document.body?.dataset?.moduleLoading || '',
            hasGate: Boolean(document.querySelector('[data-login-gate-form]')),
            hasSmoke: Boolean(smoke),
            authenticated: Boolean(state?.session?.authenticated),
            moduleCount: Array.isArray(state?.modules) ? state.modules.length : 0,
            activeModule: state?.activeModule?.id || '',
            instanceId: state?.syncConfig?.instance_id
              || state?.syncConfig?.instanceId
              || state?.sync?.config?.instance_id
              || state?.sync?.config?.instanceId
              || '',
            storageWorkspace: storageKeys.workspace || '',
            storageActor: storageKeys.actor || '',
            actorId: state?.session?.user?.id || '',
            actorRole: state?.session?.user?.role || '',
            legacySessionToken: localStorage.getItem('ctox.businessOs.sessionToken') || '',
            legacyAuthHeader: localStorage.getItem('ctox.businessOs.authHeader') || '',
            loggedOut: localStorage.getItem('ctox.businessOs.loggedOut') || '',
            pairingConfigKey: storageKeys.pairingConfig || 'ctox.businessOs.pairingConfig',
            visibleText: (document.body?.innerText || '').slice(0, 700),
          };
        });
        const waitForAuthenticatedShell = async (label) => {
          await page.waitForFunction(() => {
            const smoke = globalThis.ctoxBusinessOsSmoke;
            const state = smoke?.state || globalThis.CTOX_BUSINESS_OS_APP;
            const modulesLoaded = Array.isArray(state?.modules) && state.modules.length > 0;
            const shellOpened = Boolean(document.body?.dataset?.moduleShell);
            const loading = Boolean(document.body?.dataset?.moduleLoading);
            return Boolean(smoke)
              && Boolean(state?.session?.authenticated)
              && modulesLoaded
              && shellOpened
              && !loading;
          }, null, { timeout: 60000 });
          const status = await waitForHealthyCompleteStatus(page, {
            timeoutMs: 60000,
            requiredCollections,
          });
          if (!status?.ok) {
            throw new Error(`Business OS auth smoke ${label} advanced status unhealthy: ${JSON.stringify(status, null, 2)}`);
          }
          assertHealthyAdvancedStatusContract(status);
          return {
            snapshot: await authSnapshot(),
            status,
          };
        };
        const waitForLoginGate = async (label) => {
          await page.waitForFunction(() => (
            document.body?.dataset?.authState === 'locked'
              && Boolean(document.querySelector('[data-login-gate-form]'))
          ), null, { timeout: 30000 });
          const snapshot = await authSnapshot();
          if (!snapshot.hasGate || snapshot.authenticated || snapshot.moduleShell || snapshot.moduleCount > 0) {
            throw new Error(`Business OS auth smoke ${label} did not stay locked: ${JSON.stringify(snapshot, null, 2)}`);
          }
          return snapshot;
        };
        const clickAccountLogout = async () => {
          await page.locator('[data-open-account]').click({ timeout: 10000 });
          await page.locator('[data-logout]').click({ timeout: 10000 });
          await page.waitForLoadState('domcontentloaded', { timeout: 10000 }).catch(() => {});
        };
        const gotoInstrumentedSmokeUrl = async (label) => {
          let lastError = null;
          for (let attempt = 0; attempt < 3; attempt += 1) {
            try {
              await page.goto(smokeUrl, { waitUntil: 'commit', timeout: pageNavigationTimeoutMs });
              return;
            } catch (error) {
              lastError = error;
              if (!String(error?.message || error).includes('net::ERR_ABORTED')) break;
              await page.waitForLoadState('domcontentloaded', { timeout: 10000 }).catch(() => {});
              await new Promise((resolve) => setTimeout(resolve, 250));
            }
          }
          throw new Error(`Business OS auth smoke could not navigate to instrumented URL for ${label}: ${String(lastError?.message || lastError)}`);
        };
        const initial = await authSnapshot();
        if (!initial.authenticated || initial.moduleCount === 0 || !initial.moduleShell) {
          throw new Error(`Business OS auth smoke did not start authenticated: ${JSON.stringify(initial, null, 2)}`);
        }
        const browserContextClean = !initial.legacySessionToken
          && !initial.legacyAuthHeader
          && initial.loggedOut !== '1';
        const initialTenantScope = initial.storageWorkspace || initial.instanceId;
        const actorRole = initial.actorRole || 'unknown';
        await page.reload({ waitUntil: 'commit', timeout: pageNavigationTimeoutMs });
        const afterReload = await waitForAuthenticatedShell('authenticated reload');
        const reloadTenantScope = afterReload.snapshot.storageWorkspace || afterReload.snapshot.instanceId;
        const authenticatedReloadVerified = afterReload.snapshot.authenticated === true
          && afterReload.snapshot.moduleCount > 0
          && afterReload.snapshot.loggedOut !== '1';
        const forgedTenantScope = `${initialTenantScope || 'local-workspace'}-forged`;
        await page.evaluate(({ pairingConfigKey, forgedTenantScope: forged }) => {
          const forgedPairing = {
            ok: true,
            source: 'stored',
            app_hosting: 'web_deploy',
            sync_mode: 'p2p-first',
            instance_id: forged,
            peer_role: 'browser',
            sync_room: `ctox-business-os:${forged}:forged`,
            signaling_room_password: 'forged-room-password',
            signaling_urls: ['ws://127.0.0.1:9'],
            transport: 'webrtc',
            http_bridge_available: false,
            session: {
              authenticated: true,
              user: {
                id: 'forged-cross-scope-user',
                display_name: 'Forged Cross Scope User',
                role: 'admin',
              },
            },
          };
          localStorage.setItem(pairingConfigKey || 'ctox.businessOs.pairingConfig', JSON.stringify(forgedPairing));
          localStorage.setItem('ctox.businessOs.pairingConfig', JSON.stringify(forgedPairing));
          localStorage.setItem('ctox.businessOs.sessionToken', 'forged-cross-scope-token');
          localStorage.setItem('ctox.businessOs.authHeader', 'Basic forged-cross-scope-auth');
        }, {
          pairingConfigKey: afterReload.snapshot.pairingConfigKey,
          forgedTenantScope,
        });
        await page.reload({ waitUntil: 'commit', timeout: pageNavigationTimeoutMs });
        const crossScopeTampered = await waitForAuthenticatedShell('cross-scope storage tamper');
        const crossScopeTenantScope = crossScopeTampered.snapshot.storageWorkspace
          || crossScopeTampered.snapshot.instanceId;
        const crossScopeStorageDenied = Boolean(initialTenantScope)
          && crossScopeTenantScope === initialTenantScope
          && crossScopeTenantScope !== forgedTenantScope
          && crossScopeTampered.snapshot.actorId !== 'forged-cross-scope-user'
          && crossScopeTampered.snapshot.authenticated === true
          && crossScopeTampered.snapshot.moduleCount > 0;
        await clickAccountLogout();
        const logoutGate = await waitForLoginGate('logout');
        const logoutVerified = logoutGate.loggedOut === '1' && logoutGate.authState === 'locked';
        await gotoInstrumentedSmokeUrl('logged-out reload');
        const loggedOutReload = await waitForLoginGate('logged-out reload');
        const loggedOutReloadBlocked = loggedOutReload.loggedOut === '1'
          && loggedOutReload.authState === 'locked'
          && !loggedOutReload.authenticated;
        const protectedAccessBlocked = loggedOutReload.hasGate
          && !loggedOutReload.moduleShell
          && loggedOutReload.moduleCount === 0
          && !loggedOutReload.activeModule;
        await page.evaluate(() => {
          localStorage.setItem('ctox.businessOs.sessionToken', 'forged-smoke-token');
          localStorage.setItem('ctox.businessOs.authHeader', 'Basic forged-smoke-auth');
        });
        await page.reload({ waitUntil: 'commit', timeout: pageNavigationTimeoutMs });
        const tampered = await waitForLoginGate('tampered storage reload');
        const storageCopyDidNotWidenScope = tampered.loggedOut === '1'
          && !tampered.authenticated
          && tampered.moduleCount === 0
          && !tampered.moduleShell
          && !tampered.activeModule;
        await page.locator('[data-login-gate-form] input[name="user"]').fill('admin', { timeout: 10000 });
        await page.locator('[data-login-gate-form] input[name="password"]').fill('admin', { timeout: 10000 });
        await Promise.all([
          page.waitForURL((url) => url.pathname === '/' || url.pathname.endsWith('/index.html'), { timeout: 15000 }).catch(() => {}),
          page.locator('[data-login-gate-form] [data-gate-submit]').click({ timeout: 10000 }),
        ]);
        await page.waitForLoadState('domcontentloaded', { timeout: 15000 }).catch(() => {});
        await gotoInstrumentedSmokeUrl('login');
        const afterLogin = await waitForAuthenticatedShell('login');
        const loginTenantScope = afterLogin.snapshot.storageWorkspace || afterLogin.snapshot.instanceId;
        const loginVerified = afterLogin.snapshot.authenticated === true
          && afterLogin.snapshot.moduleCount > 0
          && afterLogin.snapshot.loggedOut !== '1'
          && !afterLogin.snapshot.legacySessionToken
          && !afterLogin.snapshot.legacyAuthHeader;
        const tenantScopeVerified = Boolean(initialTenantScope)
          && initialTenantScope === reloadTenantScope
          && initialTenantScope === loginTenantScope;
        advancedStatusEvidenceVersion = afterLogin.status.version || advancedStatusEvidenceVersion;
        advancedStatusEvidenceRuntime = afterLogin.status.rxdbRuntime || advancedStatusEvidenceRuntime;
        await clickAccountLogout();
        const finalGate = await waitForLoginGate('final logout');
        const finalState = finalGate.loggedOut === '1' && finalGate.authState === 'locked'
          ? 'logged_out'
          : 'unexpected';
        const checks = {
          loginVerified,
          authenticatedReloadVerified,
          logoutVerified,
          loggedOutReloadBlocked,
          protectedAccessBlocked,
          tenantScopeVerified,
          browserContextClean,
          crossScopeStorageDenied,
          storageCopyDidNotWidenScope,
          finalLoggedOut: finalState === 'logged_out',
        };
        const failed = Object.entries(checks).filter(([, value]) => value !== true);
        if (failed.length > 0) {
          throw new Error(`Business OS auth smoke failed: ${JSON.stringify({
            failed: failed.map(([key]) => key),
            initial,
            afterReload: afterReload.snapshot,
            crossScopeTampered: crossScopeTampered.snapshot,
            logoutGate,
            loggedOutReload,
            tampered,
            afterLogin: afterLogin.snapshot,
            finalGate,
            initialTenantScope,
            crossScopeTenantScope,
            forgedTenantScope,
            reloadTenantScope,
            loginTenantScope,
          }, null, 2)}`);
        }
        console.log(`business_os_auth_login_verified=${loginVerified ? 1 : 0}`);
        console.log(`business_os_auth_authenticated_reload_verified=${authenticatedReloadVerified ? 1 : 0}`);
        console.log(`business_os_auth_logout_verified=${logoutVerified ? 1 : 0}`);
        console.log(`business_os_auth_logged_out_reload_blocked=${loggedOutReloadBlocked ? 1 : 0}`);
        console.log(`business_os_auth_protected_access_blocked=${protectedAccessBlocked ? 1 : 0}`);
        console.log(`business_os_auth_tenant_scope_verified=${tenantScopeVerified ? 1 : 0}`);
        console.log(`business_os_auth_browser_context_clean=${browserContextClean ? 1 : 0}`);
        console.log(`business_os_auth_cross_scope_storage_denied=${crossScopeStorageDenied ? 1 : 0}`);
        console.log('business_os_auth_tenant_scope_claim=local-workspace-only');
        console.log(`business_os_auth_storage_copy_did_not_widen_scope=${storageCopyDidNotWidenScope ? 1 : 0}`);
        console.log(`business_os_auth_final_state=${finalState}`);
        console.log(`business_os_auth_auth_state=${finalState}`);
        console.log(`business_os_auth_actor_role=${actorRole}`);
        console.log(`business_os_auth_browser_context=${browserContextClean ? 'clean' : 'dirty'}`);
        console.log(`business_os_auth_tenant_scope=${initialTenantScope || ''}`);
        console.log(`advanced_status=${advancedStatusEvidenceVersion}`);
        if (advancedStatusEvidenceRuntime) console.log(`rxdb_runtime=${JSON.stringify(advancedStatusEvidenceRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'business-os-fresh-profile-ui') {
        const initialStorage = freshProfileInitialStorage || {};
        await page.setViewportSize({ width: 1280, height: 800 });
        const desktop = await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const waitFor = async (predicate, ms, label) => {
            const deadline = Date.now() + ms;
            let last = null;
            while (Date.now() < deadline) {
              last = await predicate();
              if (last?.ok) return last;
              await delay(100);
            }
            throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
          };
          const css = (value) => {
            if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
            return String(value).replace(/["\\]/g, '\\$&');
          };
          const smoke = globalThis.ctoxBusinessOsSmoke;
          const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state;
          if (!state) throw new Error('Business OS app state is unavailable for fresh-profile UI smoke');
          if (typeof smoke?.renderTabs !== 'function') throw new Error('Business OS smoke renderTabs hook is unavailable');
          if (typeof state.openModule !== 'function') throw new Error('Business OS state.openModule is unavailable for fresh-profile UI smoke');
          const [
            permissionsMod,
            lifecycleMod,
          ] = await Promise.all([
            import('/shared/permissions.js'),
            import('/shared/app-lifecycle.js'),
          ]);
          const { BusinessOsPermissions } = permissionsMod;
          const {
            appLifecycleBadge,
            canSeeModuleForAppVersion,
          } = lifecycleMod;
          const privateModule = {
            id: 'phase14-fresh-private-app',
            title: 'Phase 14 Fresh Private App',
            glyph: 'F14P',
            version: '0.5.0',
            source: 'installed',
            install_scope: 'installed',
            entry: 'installed-modules/phase14-fresh-private-app/index.js',
            collections: ['business_commands'],
            editable: true,
            deletable: true,
          };
          const teamModule = {
            id: 'phase14-fresh-team-app',
            title: 'Phase 14 Fresh Team App',
            glyph: 'F14T',
            version: '1.0.0',
            source: 'installed',
            install_scope: 'installed',
            entry: 'installed-modules/phase14-fresh-team-app/index.js',
            collections: ['business_commands'],
            editable: true,
            deletable: true,
          };
          const restrictedModule = {
            id: 'phase14-fresh-restricted-app',
            title: 'Phase 14 Fresh Restricted App',
            glyph: 'F14R',
            version: '1.2.0',
            source: 'installed',
            install_scope: 'installed',
            entry: 'installed-modules/phase14-fresh-restricted-app/index.js',
            collections: ['business_commands'],
            editable: true,
            deletable: true,
            lifecycle: {
              runtime_installed: true,
              visibility_state: 'restricted',
              audience: 'restricted',
            },
          };
          const moduleIds = [privateModule.id, teamModule.id, restrictedModule.id];
          const allPermissions = Object.values(BusinessOsPermissions);
          const scaleModuleCount = 32;
          const scaleModules = Array.from({ length: scaleModuleCount }, (_, index) => {
            const seq = index + 1;
            const version = `1.${Math.floor(index / 8)}.${index % 8}`;
            const moduleId = `phase14-scale-app-${String(seq).padStart(2, '0')}`;
            const versions = [3, 2, 1].map((versionSeq) => ({
              version_id: `${moduleId}-v${versionSeq}`,
              seq: versionSeq,
              origin: versionSeq === 1 ? 'install' : 'manual_release',
              label: `Scale Version ${versionSeq}`,
              sealed: true,
              file_count: 1,
              created_at_ms: Date.now() - ((3 - versionSeq) * 1000),
            }));
            return {
              id: moduleId,
              title: `Phase 14 Scale App ${String(seq).padStart(2, '0')}`,
              glyph: 'S14',
              version,
              source: 'installed',
              install_scope: 'installed',
              entry: `installed-modules/${moduleId}/index.js`,
              collections: ['business_commands'],
              editable: true,
              deletable: true,
              version_state: {
                version_count: versions.length,
                versions,
              },
              lifecycle: {
                runtime_installed: true,
                visibility_state: 'team',
                audience: 'team',
                current_semver: version,
                release_status: 'released',
                release_state: {
                  status: 'released',
                  current: {
                    version_id: `${moduleId}-v3`,
                    version: 3,
                    target_version: version,
                  },
                  rollback_target: {
                    version_id: `${moduleId}-v2`,
                    version: 2,
                    target_version: `1.${Math.floor(index / 8)}.${Math.max(0, (index % 8) - 1)}`,
                  },
                  history_count: versions.length,
                },
                data_access: {
                  status: 'reviewed',
                  completed: true,
                  areas: [
                    { collection: 'business_commands', read: 'granted', write: 'locked' },
                    { collection: 'ctox_queue_tasks', read: 'locked', write: 'not_requested' },
                  ],
                  granted_collection_ids: ['business_commands'],
                  locked_collection_ids: ['ctox_queue_tasks'],
                  review_is_evidence_only: true,
                  grants_implied: false,
                },
              },
            };
          });
          const scaleModuleIds = scaleModules.map((mod) => mod.id);
          const scaleExplicitGrants = scaleModules.flatMap((mod, index) => [
            {
              grant_id: `phase14_scale_${index + 1}_apps_view`,
              subject_type: 'user',
              subject_id: 'fresh_team_member',
              permission: BusinessOsPermissions.AppsView,
              scope_type: 'module',
              scope_id: mod.id,
              active: true,
            },
            {
              grant_id: `phase14_scale_${index + 1}_data_read`,
              subject_type: 'user',
              subject_id: 'fresh_team_member',
              permission: BusinessOsPermissions.DataRead,
              scope_type: 'collection',
              scope_id: 'business_commands',
              active: true,
            },
          ]);
          const scaleModuleAssignments = Object.fromEntries(scaleModules.map((mod) => [
            mod.id,
            {
              fresh_builder: [
                BusinessOsPermissions.AppsView,
                BusinessOsPermissions.AppsModify,
                BusinessOsPermissions.AppsSourceView,
                BusinessOsPermissions.AppsRelease,
              ],
            },
          ]));
          const governance = {
            founders: {
              [privateModule.id]: [{ user_id: 'fresh_builder', active: true }],
              [teamModule.id]: [{ user_id: 'fresh_builder', active: true }],
              [restrictedModule.id]: [{ user_id: 'fresh_builder', active: true }],
              ...Object.fromEntries(scaleModules.map((mod) => [
                mod.id,
                [{ user_id: 'fresh_builder', active: true }],
              ])),
            },
            permission_model: {
              version: 1,
              deny_supported: false,
              role_defaults: {
                chef: { workspace: allPermissions, module: allPermissions, assigned_module: allPermissions },
                admin: { workspace: allPermissions, module: allPermissions, assigned_module: allPermissions },
                founder: {
                  workspace: [],
                  module: [],
                  assigned_module: [
                    BusinessOsPermissions.AppsView,
                    BusinessOsPermissions.AppsModify,
                    BusinessOsPermissions.AppsSourceView,
                    BusinessOsPermissions.AppsRelease,
                    BusinessOsPermissions.DataRead,
                    BusinessOsPermissions.DataWrite,
                  ],
                },
                user: { workspace: [], module: [], assigned_module: [] },
              },
              module_assignments: {
                [privateModule.id]: {
                  fresh_builder: [
                    BusinessOsPermissions.AppsView,
                    BusinessOsPermissions.AppsModify,
                    BusinessOsPermissions.AppsSourceView,
                    BusinessOsPermissions.AppsRelease,
                  ],
                },
                [teamModule.id]: {
                  fresh_builder: [
                    BusinessOsPermissions.AppsView,
                    BusinessOsPermissions.AppsModify,
                    BusinessOsPermissions.AppsSourceView,
                    BusinessOsPermissions.AppsRelease,
                  ],
                },
                [restrictedModule.id]: {
                  fresh_builder: [
                    BusinessOsPermissions.AppsView,
                    BusinessOsPermissions.AppsModify,
                    BusinessOsPermissions.AppsSourceView,
                    BusinessOsPermissions.AppsRelease,
                  ],
                },
                ...scaleModuleAssignments,
              },
              explicit_grants: scaleExplicitGrants,
            },
          };
          const builderSession = { user: { id: 'fresh_builder', role: 'founder', name: 'Fresh Builder' } };
          const teamSession = { user: { id: 'fresh_team_member', role: 'user', name: 'Fresh Team' } };
          const originalState = {
            modules: Array.isArray(state.modules) ? [...state.modules] : [],
            taskbarPins: Array.isArray(state.taskbarPins) ? [...state.taskbarPins] : [],
            moduleAllowlist: Array.isArray(state.moduleAllowlist) ? [...state.moduleAllowlist] : state.moduleAllowlist,
            session: state.session,
            governance: state.governance,
            activeModule: state.activeModule,
            globalSession: globalThis.CTOX_BUSINESS_OS_SESSION,
            taskbarPinsGlobal: localStorage.getItem('ctox.businessOs.taskbarPins'),
          };
          let scopedTaskbarPinsKey = '';
          let scopedTaskbarPinsOriginal = null;
          globalThis.__ctoxFreshProfileRestore = () => {
            if (originalState.taskbarPinsGlobal === null) localStorage.removeItem('ctox.businessOs.taskbarPins');
            else localStorage.setItem('ctox.businessOs.taskbarPins', originalState.taskbarPinsGlobal);
            if (scopedTaskbarPinsKey) {
              if (scopedTaskbarPinsOriginal === null) localStorage.removeItem(scopedTaskbarPinsKey);
              else localStorage.setItem(scopedTaskbarPinsKey, scopedTaskbarPinsOriginal);
            }
            localStorage.removeItem('ctox.businessOs.freshProfileFakeLifecycle');
            state.modules = originalState.modules;
            state.taskbarPins = originalState.taskbarPins;
            state.moduleAllowlist = originalState.moduleAllowlist;
            state.session = originalState.session;
            state.governance = originalState.governance;
            state.activeModule = originalState.activeModule;
            globalThis.CTOX_BUSINESS_OS_SESSION = originalState.globalSession;
            smoke.renderTabs();
          };
          const authoritativeProjectionLoaded = Array.isArray(originalState.modules)
            && originalState.modules.length >= 10
            && Boolean(state.syncConfig?.instance_id || state.syncConfig?.instanceId)
            && Boolean(globalThis.CTOX_BUSINESS_OS_STATUS);
          const installModules = (session) => {
            const renderStartedAt = performance.now();
            state.session = session;
            state.governance = governance;
            globalThis.CTOX_BUSINESS_OS_SESSION = session;
            state.modules = [
              ...originalState.modules.filter((mod) => ![...moduleIds, ...scaleModuleIds].includes(mod?.id)),
              privateModule,
              teamModule,
              restrictedModule,
              ...scaleModules,
            ];
            state.moduleAllowlist = [...new Set([
              ...(Array.isArray(originalState.moduleAllowlist)
                ? originalState.moduleAllowlist.map((id) => String(id || '').trim()).filter(Boolean)
                : []),
              ...moduleIds,
              ...scaleModuleIds,
            ])];
            state.taskbarPins = [...moduleIds, ...scaleModuleIds];
            smoke.renderTabs();
            return Math.round(performance.now() - renderStartedAt);
          };
          globalThis.__ctoxFreshProfileNarrowSetup = async () => {
            installModules(teamSession);
            await state.openModule('app-store', { force: true, asModule: true });
            await waitFor(() => ({
              ok: Boolean(document.querySelector('[data-app-store-root]')),
              text: document.querySelector('[data-app-store-root]')?.innerText?.slice(0, 500) || '',
            }), 10000, 'fresh-profile narrow app store root');
            document.querySelector('[data-app-store-root] [data-scope="installed"]')?.click();
            return waitFor(() => {
              const card = document.querySelector(`[data-app-id="${css(teamModule.id)}"]`);
              const disabled = card?.querySelector('[data-disabled-reason]');
              const lifecycle = card?.querySelector('.app-lifecycle-badge');
              return {
                ok: Boolean(card && disabled && lifecycle),
                cardText: card?.innerText || '',
                disabledReason: disabled?.getAttribute('data-disabled-reason') || '',
                lifecycleText: lifecycle?.textContent?.trim() || '',
              };
            }, 10000, 'fresh-profile narrow app-store disabled reason');
          };
          const tabEvidence = () => {
            const privateBadge = document.querySelector(`[data-app-lifecycle-badge="${css(privateModule.id)}"]`);
            const teamBadge = document.querySelector(`[data-app-lifecycle-badge="${css(teamModule.id)}"]`);
            const restrictedBadge = document.querySelector(`[data-app-lifecycle-badge="${css(restrictedModule.id)}"]`);
            return {
              privateTab: Boolean(document.querySelector(`.module-tab[data-target="${css(privateModule.id)}"]`)),
              teamTab: Boolean(document.querySelector(`.module-tab[data-target="${css(teamModule.id)}"]`)),
              restrictedTab: Boolean(document.querySelector(`.module-tab[data-target="${css(restrictedModule.id)}"]`)),
              privateText: privateBadge?.textContent?.trim() || '',
              teamText: teamBadge?.textContent?.trim() || '',
              restrictedText: restrictedBadge?.textContent?.trim() || '',
              privateState: privateBadge?.getAttribute('data-state') || '',
              teamState: teamBadge?.getAttribute('data-state') || '',
              restrictedState: restrictedBadge?.getAttribute('data-state') || '',
            };
          };
          const openStartMenu = async () => {
            const startedAt = performance.now();
            const button = document.querySelector('[data-shell-start]');
            if (!button) throw new Error('Business OS start menu button is missing for fresh-profile smoke');
            document.querySelector('.shell-start-menu-panel')?.classList.remove('is-active');
            button.click();
            const result = await waitFor(() => {
              const panel = document.querySelector('.shell-start-menu-panel');
              const text = panel?.innerText || '';
              return {
                ok: Boolean(panel?.classList?.contains('is-active') && /Phase 14 Fresh/.test(text)),
                text,
              };
            }, 5000, 'fresh-profile start menu');
            return {
              ...result,
              ms: Math.round(performance.now() - startedAt),
            };
          };
          try {
            const renderTimings = [installModules(builderSession)];
            const builderTabs = await waitFor(() => {
              const tabs = tabEvidence();
              return {
                ok: tabs.privateTab
                  && tabs.teamTab
                  && tabs.restrictedTab
                  && tabs.privateState === 'private'
                  && tabs.teamState === 'team'
                  && tabs.restrictedState === 'restricted',
                ...tabs,
              };
            }, 8000, 'fresh-profile lifecycle tabs');
            const startMenu = await openStartMenu();
            const startMenuText = startMenu.text || document.querySelector('.shell-start-menu-panel')?.innerText || '';
            const privateLifecycle = appLifecycleBadge(privateModule, { session: builderSession, governance });
            const teamLifecycle = appLifecycleBadge(teamModule, { session: teamSession, governance });
            const restrictedLifecycle = appLifecycleBadge(restrictedModule, { session: builderSession, governance });
            smoke.openAppLifecycleDrawer(privateModule);
            const drawer = await waitFor(() => {
              const el = document.querySelector('.module-lifecycle-drawer');
              const text = el?.innerText || '';
              return {
                ok: Boolean(el && /App-Verantwortliche/.test(text) && /Verwalten erlaubt/.test(text)),
                text,
              };
            }, 5000, 'fresh-profile lifecycle drawer labels');
            document.querySelector('[data-close-lifecycle]')?.click();

            renderTimings.push(installModules(teamSession));
            const appStoreStartedAt = performance.now();
            await state.openModule('app-store', { force: true, asModule: true });
            await waitFor(() => ({
              ok: Boolean(document.querySelector('[data-app-store-root]')),
              text: document.querySelector('[data-app-store-root]')?.innerText?.slice(0, 500) || '',
            }), 10000, 'fresh-profile app store root');
            document.querySelector('[data-app-store-root] [data-scope="installed"]')?.click();
            const appStore = await waitFor(() => {
              const card = document.querySelector(`[data-app-id="${css(teamModule.id)}"]`);
              const disabled = card?.querySelector('[data-disabled-reason]');
              const lifecycle = card?.querySelector('.app-lifecycle-badge');
              const cards = [...document.querySelectorAll('[data-app-id]')];
              const scaleCardCount = scaleModuleIds
                .filter((id) => document.querySelector(`[data-app-id="${css(id)}"]`))
                .length;
              return {
                ok: Boolean(card && disabled && lifecycle),
                cardText: card?.innerText || '',
                disabledReason: disabled?.getAttribute('data-disabled-reason') || '',
                lifecycleText: lifecycle?.textContent?.trim() || '',
                cardCount: cards.length,
                scaleCardCount,
              };
            }, 10000, 'fresh-profile app-store disabled reason');
            appStore.ms = Math.round(performance.now() - appStoreStartedAt);

            scopedTaskbarPinsKey = typeof smoke.storageKeys === 'function'
              ? smoke.storageKeys()?.taskbarPins || ''
              : '';
            if (scopedTaskbarPinsKey) scopedTaskbarPinsOriginal = localStorage.getItem(scopedTaskbarPinsKey);
            const tamperedPins = JSON.stringify(moduleIds);
            localStorage.setItem('ctox.businessOs.taskbarPins', tamperedPins);
            if (scopedTaskbarPinsKey) localStorage.setItem(scopedTaskbarPinsKey, tamperedPins);
            localStorage.setItem('ctox.businessOs.freshProfileFakeLifecycle', JSON.stringify({
              [privateModule.id]: 'team',
              [restrictedModule.id]: 'team',
            }));
            renderTimings.push(installModules(teamSession));
            const tamperedTabs = await waitFor(() => {
              const tabs = tabEvidence();
              return {
                ok: tabs.teamTab && !tabs.privateTab && !tabs.restrictedTab,
                ...tabs,
              };
            }, 8000, 'fresh-profile storage tampering');
            localStorage.removeItem('ctox.businessOs.freshProfileFakeLifecycle');
            if (scopedTaskbarPinsKey) localStorage.removeItem(scopedTaskbarPinsKey);
            localStorage.removeItem('ctox.businessOs.taskbarPins');

            const lifecycleLabelsVisible = builderTabs.privateText === 'Privat'
              && builderTabs.teamText === 'Team'
              && builderTabs.restrictedText === 'Eingeschränkt'
              && privateLifecycle.text === 'Privat'
              && teamLifecycle.text === 'Team'
              && restrictedLifecycle.text === 'Eingeschränkt';
            const versionBadgesVisible = privateLifecycle.version === 'v0.5.0'
              && teamLifecycle.version === 'v1.0.0'
              && restrictedLifecycle.version === 'v1.2.0'
              && /v0\.5\.0\s+Privat/.test(startMenuText)
              && /v1\.0\.0\s+Team/.test(startMenuText)
              && /v1\.2\.0\s+Eingeschränkt/.test(startMenuText)
              && /v1\.0\.0\s*·\s*Team/.test(appStore.lifecycleText);
            const disabledReasonsVisible = /Nur Owner|Admins|App-Freigaberecht/.test(appStore.disabledReason)
              || /Nur Owner|Admins|App-Freigaberecht/.test(appStore.cardText);
            const desktopViewportVerified = window.innerWidth >= 1200
              && window.innerHeight >= 700
              && lifecycleLabelsVisible
              && versionBadgesVisible
              && disabledReasonsVisible;
            const noStorageWidening = tamperedTabs.teamTab
              && !tamperedTabs.privateTab
              && !tamperedTabs.restrictedTab;
            const authState = state.session?.user?.id ? 'authenticated' : 'unknown';
            const scaleReleaseVersions = scaleModules.reduce((total, mod) =>
              total + Number(mod.version_state?.version_count || 0), 0);
            const scaleCatalogModules = Array.isArray(state.modules) ? state.modules.length : 0;
            const scaleRenderMs = Math.max(...renderTimings);
            const scaleStartMenuMs = Number(startMenu.ms || 0);
            const scaleAppStoreMs = Number(appStore.ms || 0);
            const scaleBudgetPassed = scaleModules.length >= 32
              && scaleCatalogModules >= 50
              && scaleExplicitGrants.length >= 64
              && scaleReleaseVersions >= 96
              && Number(appStore.scaleCardCount || 0) >= 20
              && scaleRenderMs <= 5000
              && scaleStartMenuMs <= 5000
              && scaleAppStoreMs <= 15000;
            const checks = {
              authoritativeProjectionLoaded,
              lifecycleLabelsVisible,
              versionBadgesVisible,
              disabledReasonsVisible,
              desktopViewportVerified,
              noStorageWidening,
              authState: authState === 'authenticated',
              scaleBudgetPassed,
            };
            const failed = Object.entries(checks).filter(([, value]) => value !== true);
            if (failed.length) {
              throw new Error(`fresh-profile UI smoke failed: ${JSON.stringify({
                failed,
                builderTabs,
                startMenuText,
                drawer,
                appStore,
                tamperedTabs,
                privateLifecycle,
                teamLifecycle,
                restrictedLifecycle,
                authoritativeProjectionLoaded,
                scale: {
                  fixtureModules: scaleModules.length,
                  catalogModules: scaleCatalogModules,
                  explicitGrants: scaleExplicitGrants.length,
                  releaseVersions: scaleReleaseVersions,
                  appStoreCards: appStore.scaleCardCount,
                  renderMs: scaleRenderMs,
                  startMenuMs: scaleStartMenuMs,
                  appStoreMs: scaleAppStoreMs,
                },
              }, null, 2)}`);
            }
            return {
              authoritativeProjectionLoaded,
              lifecycleLabelsVisible,
              versionBadgesVisible,
              disabledReasonsVisible,
              desktopViewportVerified,
              noStorageWidening,
              authState,
              actorRole: state.session?.user?.role || '',
              tenantScope: state.syncConfig?.instance_id || state.syncConfig?.instanceId || '',
              targetModuleId: teamModule.id,
              scaleFixtureModules: scaleModules.length,
              scaleCatalogModules,
              scaleExplicitGrants: scaleExplicitGrants.length,
              scaleReleaseVersions,
              scaleAppStoreCards: appStore.scaleCardCount,
              scaleRenderMs,
              scaleStartMenuMs,
              scaleAppStoreMs,
              scaleBudgetPassed,
            };
          } finally {
            localStorage.removeItem('ctox.businessOs.freshProfileFakeLifecycle');
          }
        });
        await page.setViewportSize({ width: 390, height: 844 });
        let narrowViewport = null;
        try {
          narrowViewport = await page.evaluate(async () => {
            const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
            const waitFor = async (predicate, ms, label) => {
              const deadline = Date.now() + ms;
              let last = null;
              while (Date.now() < deadline) {
                last = await predicate();
                if (last?.ok) return last;
                await delay(100);
              }
              throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
            };
            await globalThis.__ctoxFreshProfileNarrowSetup?.();
            await waitFor(() => {
              const root = document.querySelector('[data-app-store-root]') || document.body;
              const targetCard = document.querySelector('[data-app-id="phase14-fresh-team-app"]');
              targetCard?.scrollIntoView?.({ block: 'center', inline: 'nearest' });
              const visibleLifecycle = [...document.querySelectorAll('.module-tab-lifecycle, .app-lifecycle-badge')]
                .filter((el) => {
                  const rect = el.getBoundingClientRect();
                  return rect.width > 0 && rect.height > 0 && rect.right > 0 && rect.left < window.innerWidth;
                });
              const disabled = targetCard?.querySelector('[data-disabled-reason]');
              const disabledReason = disabled?.getAttribute('data-disabled-reason') || '';
              return {
                ok: window.innerWidth <= 430
                  && Boolean(root)
                  && visibleLifecycle.length >= 1
                  && Boolean(targetCard && disabled && disabledReason),
                width: window.innerWidth,
                height: window.innerHeight,
                lifecycleCount: visibleLifecycle.length,
                disabledReason,
              };
            }, 5000, 'fresh-profile narrow viewport visible labels');
            return {
              width: window.innerWidth,
              height: window.innerHeight,
              ok: true,
            };
          });
        } finally {
          await page.evaluate(() => {
            globalThis.__ctoxFreshProfileRestore?.();
            delete globalThis.__ctoxFreshProfileRestore;
            delete globalThis.__ctoxFreshProfileNarrowSetup;
          }).catch(() => {});
        }
        const status = await waitForHealthyCompleteStatus(page, {
          timeoutMs: 60000,
          requiredCollections: BUSINESS_OS_SHELL_STATUS_COLLECTIONS,
        });
        if (!status?.ok) {
          throw new Error(`Business OS fresh-profile advanced status unhealthy: ${JSON.stringify(status, null, 2)}`);
        }
        assertHealthyAdvancedStatusContract(status);
        advancedStatusEvidenceVersion = status.version || advancedStatusEvidenceVersion;
        advancedStatusEvidenceRuntime = status.rxdbRuntime || advancedStatusEvidenceRuntime;
        const cleanIndexedDb = initialStorage.cleanIndexedDb === true && initialStorage.emptyProfile === true;
        const cleanLocalStorage = initialStorage.cleanLocalStorage === true && initialStorage.emptyProfile === true;
        const cleanSessionStorage = initialStorage.cleanSessionStorage === true && initialStorage.emptyProfile === true;
        const nativeScale = freshProfileScaleSeed || {};
        const nativeScalePermissionGrants = Number(nativeScale.permissionGrants || 0);
        const nativeScaleModuleVersions = Number(nativeScale.moduleVersions || 0);
        const nativeScaleAuditEvents = Number(nativeScale.auditEvents || 0);
        const scaleBudgetPassed = desktop.scaleBudgetPassed === true
          && Number(desktop.scaleFixtureModules || 0) >= 32
          && Number(desktop.scaleCatalogModules || 0) >= 50
          && Number(desktop.scaleExplicitGrants || 0) >= 64
          && Number(desktop.scaleReleaseVersions || 0) >= 96
          && nativeScalePermissionGrants >= 64
          && nativeScaleModuleVersions >= 96
          && nativeScaleAuditEvents >= 128
          && Number(desktop.scaleAppStoreCards || 0) >= 20
          && Number(desktop.scaleRenderMs || 0) <= 5000
          && Number(desktop.scaleStartMenuMs || 0) <= 5000
          && Number(desktop.scaleAppStoreMs || 0) <= 15000;
        const checks = {
          cleanIndexedDb,
          cleanLocalStorage,
          cleanSessionStorage,
          authoritativeProjectionLoaded: desktop.authoritativeProjectionLoaded === true,
          lifecycleLabelsVisible: desktop.lifecycleLabelsVisible === true,
          versionBadgesVisible: desktop.versionBadgesVisible === true,
          disabledReasonsVisible: desktop.disabledReasonsVisible === true,
          desktopViewportVerified: desktop.desktopViewportVerified === true,
          narrowViewportVerified: narrowViewport.ok === true,
          noStorageWidening: desktop.noStorageWidening === true,
          authState: desktop.authState === 'authenticated',
          scaleBudgetPassed,
        };
        const failed = Object.entries(checks).filter(([, value]) => value !== true);
        if (failed.length > 0) {
          throw new Error(`Business OS fresh-profile smoke failed: ${JSON.stringify({
            failed: failed.map(([key]) => key),
            initialStorage,
            desktop,
            nativeScale,
            narrowViewport,
          }, null, 2)}`);
        }
        console.log(`business_os_fresh_profile_clean_indexeddb=${cleanIndexedDb ? 1 : 0}`);
        console.log(`business_os_fresh_profile_clean_local_storage=${cleanLocalStorage ? 1 : 0}`);
        console.log(`business_os_fresh_profile_clean_session_storage=${cleanSessionStorage ? 1 : 0}`);
        console.log(`business_os_fresh_profile_authoritative_projection_loaded=${desktop.authoritativeProjectionLoaded ? 1 : 0}`);
        console.log(`business_os_fresh_profile_lifecycle_labels_visible=${desktop.lifecycleLabelsVisible ? 1 : 0}`);
        console.log(`business_os_fresh_profile_version_badges_visible=${desktop.versionBadgesVisible ? 1 : 0}`);
        console.log(`business_os_fresh_profile_disabled_reasons_visible=${desktop.disabledReasonsVisible ? 1 : 0}`);
        console.log(`business_os_fresh_profile_desktop_viewport_verified=${desktop.desktopViewportVerified ? 1 : 0}`);
        console.log(`business_os_fresh_profile_narrow_viewport_verified=${narrowViewport.ok ? 1 : 0}`);
        console.log(`business_os_fresh_profile_no_storage_widening=${desktop.noStorageWidening ? 1 : 0}`);
        console.log(`business_os_fresh_profile_auth_state=${desktop.authState || ''}`);
        console.log(`business_os_fresh_profile_actor_role=${desktop.actorRole || ''}`);
        console.log(`business_os_fresh_profile_browser_context=${initialStorage.emptyProfile === true ? 'clean' : 'dirty'}`);
        console.log(`business_os_fresh_profile_tenant_scope=${desktop.tenantScope || ''}`);
        console.log(`business_os_fresh_profile_scale_fixture_modules=${Number(desktop.scaleFixtureModules || 0)}`);
        console.log(`business_os_fresh_profile_scale_catalog_modules=${Number(desktop.scaleCatalogModules || 0)}`);
        console.log(`business_os_fresh_profile_scale_explicit_grants=${Number(desktop.scaleExplicitGrants || 0)}`);
        console.log(`business_os_fresh_profile_scale_release_versions=${Number(desktop.scaleReleaseVersions || 0)}`);
        console.log(`business_os_fresh_profile_scale_native_permission_grants=${nativeScalePermissionGrants}`);
        console.log(`business_os_fresh_profile_scale_native_module_versions=${nativeScaleModuleVersions}`);
        console.log(`business_os_fresh_profile_scale_native_audit_events=${nativeScaleAuditEvents}`);
        console.log(`business_os_fresh_profile_scale_app_store_cards=${Number(desktop.scaleAppStoreCards || 0)}`);
        console.log(`business_os_fresh_profile_scale_render_ms=${Number(desktop.scaleRenderMs || 0)}`);
        console.log(`business_os_fresh_profile_scale_start_menu_ms=${Number(desktop.scaleStartMenuMs || 0)}`);
        console.log(`business_os_fresh_profile_scale_app_store_ms=${Number(desktop.scaleAppStoreMs || 0)}`);
        console.log(`business_os_fresh_profile_scale_budget_passed=${scaleBudgetPassed ? 1 : 0}`);
        console.log(`advanced_status=${advancedStatusEvidenceVersion}`);
        if (advancedStatusEvidenceRuntime) console.log(`rxdb_runtime=${JSON.stringify(advancedStatusEvidenceRuntime)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'native-schema-drift-browser-status') {
        const driftStatus = await page.evaluate((requiredCollections) => globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections,
        }), Array.from(BUSINESS_OS_SHELL_STATUS_COLLECTIONS));
        assertHealthyAdvancedStatusContract(driftStatus);
        const nativePeer = driftStatus?.sync?.nativePeer || {};
        const degraded = Array.isArray(nativePeer.degradedOptionalCollections)
          ? nativePeer.degradedOptionalCollections
          : [];
        const outboundDrift = degraded.find((item) => item?.collection === 'outbound_messages');
        if (!outboundDrift || !String(outboundDrift.error || '').includes('DB6')) {
          throw new Error(`Native optional schema drift was not exposed as degraded outbound_messages collection: ${JSON.stringify(nativePeer, null, 2)}`);
        }
        if (nativePeer.requiredCollectionsRegistered !== true) {
          throw new Error(`Native peer required collections were not fully registered during optional schema drift: ${JSON.stringify(nativePeer, null, 2)}`);
        }
        if (Array.isArray(nativePeer.requiredMissingCollections) && nativePeer.requiredMissingCollections.length > 0) {
          throw new Error(`Native peer reports missing required collections during optional schema drift: ${JSON.stringify(nativePeer.requiredMissingCollections)}`);
        }
        const registered = Array.isArray(nativePeer.registeredCollections)
          ? nativePeer.registeredCollections
          : [];
        for (const required of ['business_commands', 'ctox_runtime_settings', 'desktop_files']) {
          if (!registered.includes(required)) {
            throw new Error(`Native peer did not register required collection ${required}: ${JSON.stringify(nativePeer, null, 2)}`);
          }
        }
        if (registered.includes('outbound_messages')) {
          throw new Error(`Native peer registered drifted optional outbound_messages collection instead of degrading it: ${JSON.stringify(nativePeer, null, 2)}`);
        }
        console.log(`native_schema_drift_collection=${outboundDrift.collection}`);
        console.log(`native_schema_drift_phase=${outboundDrift.phase || ''}`);
        console.log(`native_schema_drift_degraded_count=${nativePeer.degradedOptionalCount || degraded.length}`);
        console.log(`native_schema_drift_registered_count=${nativePeer.registeredCount || registered.length}`);
        console.log(`native_required_missing_count=${Array.isArray(nativePeer.requiredMissingCollections) ? nativePeer.requiredMissingCollections.length : -1}`);
        console.log(`advanced_status=${driftStatus.version}`);
        console.log(`rxdb_runtime=${JSON.stringify(driftStatus.rxdbRuntime || null)}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'tab-freeze-browser-to-rust') {
        const cdp = await page.context().newCDPSession(page);
        await cdp.send('Page.setWebLifecycleState', { state: 'frozen' });
        await new Promise((resolve) => setTimeout(resolve, 5000));
        await cdp.send('Page.setWebLifecycleState', { state: 'active' });
        await cdp.detach().catch(() => {});
        const resumedStatus = await waitForHealthyCompleteStatus(page, {
          timeoutMs: 60000,
          requiredCollections: BUSINESS_OS_SHELL_STATUS_COLLECTIONS,
        });
        if (!resumedStatus?.ok) {
          throw new Error(`Business OS advanced status unhealthy after tab freeze resume: ${JSON.stringify(resumedStatus, null, 2)}`);
        }
        assertHealthyAdvancedStatusContract(resumedStatus);
        advancedStatusEvidenceVersion = resumedStatus.version || advancedStatusEvidenceVersion;
        advancedStatusEvidenceRuntime = resumedStatus.rxdbRuntime || advancedStatusEvidenceRuntime;
      }
      if (smokeMode === 'network-flap-browser-to-rust') {
        await page.context().setOffline(true);
        await new Promise((resolve) => setTimeout(resolve, 5000));
        await page.context().setOffline(false);
        const resumedStatus = await waitForHealthyCompleteStatus(page, {
          timeoutMs: 90000,
          requiredCollections: BUSINESS_OS_SHELL_STATUS_COLLECTIONS,
        });
        if (!resumedStatus?.ok) {
          throw new Error(`Business OS advanced status unhealthy after browser network flap: ${JSON.stringify(resumedStatus, null, 2)}`);
        }
        assertHealthyAdvancedStatusContract(resumedStatus);
        advancedStatusEvidenceVersion = resumedStatus.version || advancedStatusEvidenceVersion;
        advancedStatusEvidenceRuntime = resumedStatus.rxdbRuntime || advancedStatusEvidenceRuntime;
      }
      if (smokeMode === 'browser-responsive-ui') {
        if (!/#browser(?:$|[?&])/.test(page.url())) {
          const browserUrl = new URL(page.url());
          browserUrl.pathname = '/index.html';
          browserUrl.searchParams.set('rxdbSmoke', '1');
          if (useAppDb && smokeDbId) browserUrl.searchParams.set('smokeDbId', smokeDbId);
          browserUrl.hash = '#browser';
          await page.goto(browserUrl.toString(), { waitUntil: 'domcontentloaded' });
        }
        await page.waitForFunction(() => Boolean(document.querySelector('[data-browser-root]')), null, { timeout: 60000 });
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          return Boolean(state?.sync?.startCollection && state?.db?.raw?.browser_frames);
        }, null, { timeout: 60000 });
        // Render a synthetic frame so the page area looks like a real browser viewport.
        await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          for (const collection of ['browser_sessions', 'browser_tabs', 'browser_frames', 'browser_input_events', 'business_commands']) {
            const bridge = await state.sync.startCollection(collection);
            await Promise.race([bridge?.state?.awaitInitialReplication?.().catch(() => {}) || delay(0), delay(8000)]);
          }
          const root = document.querySelector('[data-browser-root]');
          const address = root.querySelector('[data-browser-address]');
          if (address) {
            address.value = 'https://example.com';
            address.dispatchEvent(new Event('input', { bubbles: true }));
          }
          root.querySelector('[data-browser-seed]')?.click();
          await delay(1500);
          root.querySelector('[data-browser-refresh]')?.click();
          await delay(1200);
        });

        const measureChrome = async (label) => {
          const probe = await page.evaluate((forceAuth) => {
            const root = document.querySelector('[data-browser-root]');
            const rectOf = (sel) => {
              const el = root.querySelector(sel);
              if (!el) return null;
              const r = el.getBoundingClientRect();
              const visible = r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden';
              return { x: r.x, y: r.y, w: r.width, h: r.height, top: r.top, bottom: r.bottom, visible };
            };
            // Auth-assist must remain laid out (not display:none) at any width.
            const authEl = root.querySelector('[data-browser-auth-assist]');
            let authDisplayWhenShown = 'n/a';
            if (authEl) {
              const wasHidden = authEl.hidden;
              const prevHtml = authEl.innerHTML;
              authEl.hidden = false;
              if (!prevHtml.trim()) authEl.innerHTML = '<div><strong>probe</strong></div>';
              authDisplayWhenShown = getComputedStyle(authEl).display;
              authEl.hidden = wasHidden;
              authEl.innerHTML = prevHtml;
            }
            return {
              innerWidth: window.innerWidth,
              innerHeight: window.innerHeight,
              scrollWidth: document.documentElement.scrollWidth,
              clientWidth: document.documentElement.clientWidth,
              address: rectOf('[data-browser-address]'),
              back: rectOf('[data-browser-back]'),
              forward: rectOf('[data-browser-forward]'),
              reload: rectOf('[data-browser-reload]'),
              start: rectOf('[data-browser-start]'),
              canvas: rectOf('[data-browser-canvas]'),
              statusChip: rectOf('[data-browser-status-chip]'),
              authDisplayWhenShown,
              canvasHasFrame: !root.querySelector('[data-browser-empty]') || Boolean(root.querySelector('[data-browser-empty]')?.hidden),
            };
          });
          await assertNoVisibleBrowserDebugText(page);
          const problems = [];
          const need = ['address', 'back', 'forward', 'reload', 'start', 'canvas'];
          for (const key of need) {
            if (!probe[key] || !probe[key].visible) problems.push(`${key} not visible`);
          }
          if (probe.scrollWidth > probe.clientWidth + 2) {
            problems.push(`horizontal overflow scrollWidth=${probe.scrollWidth} clientWidth=${probe.clientWidth}`);
          }
          if (probe.address && probe.canvas && probe.address.bottom > probe.canvas.top + 4) {
            problems.push(`address bar not above page area (address.bottom=${probe.address.bottom} canvas.top=${probe.canvas.top})`);
          }
          if (probe.authDisplayWhenShown === 'none') {
            problems.push('auth-assist banner is display:none when shown');
          }
          if (problems.length) {
            throw new Error(`Browser responsive layout failed at ${label}: ${problems.join('; ')} ${JSON.stringify(probe)}`);
          }
          const saved = await page.evaluate((name) => globalThis.__ctoxCaptureSmokeScreenshot?.(name), `browser-responsive-${label}`);
          console.log(`browser_responsive_${label}_ok=1 innerWidth=${probe.innerWidth} canvasHasFrame=${probe.canvasHasFrame ? 1 : 0} screenshot=${saved || '-'}`);
          return probe;
        };

        await page.setViewportSize({ width: 1280, height: 800 });
        await page.waitForTimeout(400);
        await measureChrome('desktop');

        await page.setViewportSize({ width: 414, height: 896 });
        await page.waitForTimeout(500);
        await measureChrome('mobile');

        console.log('browser_responsive_ui_ok=1');
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'browser-lifecycle-ui') {
        if (!/#browser(?:$|[?&])/.test(page.url())) {
          const browserUrl = new URL(page.url());
          browserUrl.pathname = '/index.html';
          browserUrl.searchParams.set('rxdbSmoke', '1');
          if (useAppDb && smokeDbId) browserUrl.searchParams.set('smokeDbId', smokeDbId);
          browserUrl.hash = '#browser';
          await page.goto(browserUrl.toString(), { waitUntil: 'domcontentloaded' });
          await page.waitForFunction(() => Boolean(document.querySelector('[data-browser-root]')), null, { timeout: 60000 });
          await assertNoVisibleBrowserDebugText(page);
          await page.waitForFunction(() => {
            const state = globalThis.ctoxBusinessOsSmoke?.state;
            return Boolean(state?.sync?.startCollection && state?.db?.raw?.business_commands);
          }, null, { timeout: 60000 });
        }
        const lifecycle = await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          if (!state?.sync?.startCollection || !db?.business_commands) {
            throw new Error('Business OS command collections are not available for Browser lifecycle UI smoke');
          }
          for (const collection of [
            'business_commands',
            'browser_sessions',
            'browser_tabs',
            'browser_frames',
            'browser_input_events',
          ]) {
            const bridge = await state.sync.startCollection(collection);
            await bounded(bridge?.state?.awaitInitialReplication?.(), 20000);
            await bounded(bridge?.state?.awaitInSync?.(), 20000);
          }
          const waitForBrowserRoot = async () => {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const root = document.querySelector('[data-browser-root]');
              if (root) return root;
              location.hash = 'browser';
              await delay(250);
            }
            throw new Error(`Browser module root did not appear: ${document.body?.innerText?.slice(0, 500) || ''}`);
          };
          const root = await waitForBrowserRoot();
          const address = root.querySelector('[data-browser-address]');
          if (!address) throw new Error('Browser address input not found');
          address.value = 'https://example.com';
          address.dispatchEvent(new Event('input', { bubbles: true }));

          const startedAt = Date.now();
          const commandTypes = [
            ['browser.session.start', '[data-browser-start]'],
            ['browser.navigate', '[data-browser-address-form]'],
            ['browser.reload', '[data-browser-reload]'],
            ['browser.back', '[data-browser-back]'],
            ['browser.forward', '[data-browser-forward]'],
            ['browser.reset', '[data-browser-reset]'],
            ['browser.session.stop', '[data-browser-stop]'],
          ];
          const clickControl = async (type, selector) => {
            const target = root.querySelector(selector);
            if (!target) throw new Error(`Browser lifecycle control missing for ${type}: ${selector}`);
            if (selector === '[data-browser-address-form]') {
              target.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
            } else {
              target.click();
            }
            await delay(450);
          };
          const waitForAcceptedCommand = async (type, timeoutMs = 45000) => {
            const deadline = Date.now() + timeoutMs;
            let last = null;
            while (Date.now() < deadline) {
              const commands = (await db.business_commands.find().exec())
                .map((doc) => doc.toJSON?.() || doc)
                .filter((command) => {
                  const commandType = command.command_type || command.type || '';
                  return commandType === type
                    && command.module === 'browser'
                    && Number(command.created_at_ms || command.updated_at_ms || 0) >= startedAt - 1000;
                })
                .sort((a, b) => Number(b.created_at_ms || b.updated_at_ms || 0) - Number(a.created_at_ms || a.updated_at_ms || 0));
              last = commands[0] || null;
              if (last && String(last.status || '') !== 'pending_sync') return last;
              await delay(250);
            }
            throw new Error(`Browser lifecycle command ${type} was not accepted: ${JSON.stringify(last, null, 2)}`);
          };
          await clickControl(...commandTypes[0]);
          {
            const deadline = Date.now() + 45000;
            let sawSession = false;
            while (Date.now() < deadline) {
              const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
              if (session?.id === 'browser_session_default') {
                sawSession = true;
                break;
              }
              await delay(250);
            }
            if (!sawSession) throw new Error('Browser lifecycle UI smoke did not observe session after Start Remote');
          }
          root.querySelector('[data-browser-refresh]')?.click();
          {
            const deadline = Date.now() + 30000;
            let renderedSession = false;
            while (Date.now() < deadline) {
              const text = root.querySelector('[data-browser-session-card]')?.textContent || '';
              if (text.includes('Browser') && text.includes('https://example.com') && !text.includes('pending_command')) {
                renderedSession = true;
                break;
              }
              await delay(250);
            }
            if (!renderedSession) {
              throw new Error(`Browser lifecycle UI smoke did not render session after Start Remote: ${root.querySelector('[data-browser-session-card]')?.textContent || ''}`);
            }
          }
          await waitForAcceptedCommand(commandTypes[0][0]);
          for (const command of commandTypes.slice(1)) {
            await clickControl(...command);
            await waitForAcceptedCommand(command[0]);
          }

          const expected = commandTypes.map(([type]) => type);
          const deadline = Date.now() + 90000;
          let last = null;
          while (Date.now() < deadline) {
            const commands = (await db.business_commands.find().exec())
              .map((doc) => doc.toJSON?.() || doc)
              .filter((command) => {
                const type = command.command_type || command.type || '';
                return expected.includes(type)
                  && command.module === 'browser'
                  && Number(command.created_at_ms || command.updated_at_ms || 0) >= startedAt - 1000;
              });
            const acceptedTypes = new Set(commands
              .filter((command) => String(command.status || '') !== 'pending_sync')
              .map((command) => command.command_type || command.type || ''));
            const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
            const tab = (await db.browser_tabs?.findOne('browser_tab_default').exec())?.toJSON?.() || null;
            last = {
              commandCount: commands.length,
              acceptedTypes: [...acceptedTypes],
              commands: commands.map((command) => ({
                id: command.id,
                type: command.command_type || command.type || '',
                status: command.status || '',
                result: command.result || null,
                error: command.error || '',
              })),
              session,
              tab,
            };
            if (
              expected.every((type) => acceptedTypes.has(type)) &&
              session?.status === 'stopped' &&
              session?.runtime_status === 'stopped' &&
              session?.payload?.last_command_type === 'browser.session.stop' &&
              tab?.status === 'stopped'
            ) {
              return {
                commandTypes: expected,
                commandCount: commands.length,
                acceptedTypes: [...acceptedTypes],
                sessionStatus: session.status,
                runtimeStatus: session.runtime_status,
                tabStatus: tab.status,
                lastCommandType: session.payload?.last_command_type || '',
              };
            }
            await delay(500);
          }
          throw new Error(`Browser lifecycle UI smoke did not converge: ${JSON.stringify(last, null, 2)}`);
        });
        console.log(`browser_lifecycle_command_count=${lifecycle.commandCount}`);
        console.log(`browser_lifecycle_command_types=${lifecycle.commandTypes.join(',')}`);
        console.log(`browser_lifecycle_accepted_types=${lifecycle.acceptedTypes.join(',')}`);
        console.log(`browser_lifecycle_session_status=${lifecycle.sessionStatus}`);
        console.log(`browser_lifecycle_runtime_status=${lifecycle.runtimeStatus}`);
        console.log(`browser_lifecycle_tab_status=${lifecycle.tabStatus}`);
        console.log(`browser_lifecycle_last_command=${lifecycle.lastCommandType}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'browser-input-runtime') {
        if (!/#browser(?:$|[?&])/.test(page.url())) {
          const browserUrl = new URL(page.url());
          browserUrl.pathname = '/index.html';
          browserUrl.searchParams.set('rxdbSmoke', '1');
          if (useAppDb && smokeDbId) browserUrl.searchParams.set('smokeDbId', smokeDbId);
          browserUrl.hash = '#browser';
          await page.goto(browserUrl.toString(), { waitUntil: 'domcontentloaded' });
        }
        await page.waitForFunction(() => Boolean(document.querySelector('[data-browser-root]')), null, { timeout: 60000 });
        await assertNoVisibleBrowserDebugText(page);
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          return Boolean(state?.sync?.startCollection && state?.db?.raw?.business_commands);
        }, null, { timeout: 60000 });

        const phase1 = await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const docsToJson = (docs) => (docs || []).map((doc) => doc?.toJSON?.() || doc).filter(Boolean);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          if (!state?.sync?.startCollection || !db?.business_commands) {
            throw new Error('Business OS command collections are not available for Browser input runtime smoke');
          }
          for (const collection of [
            'business_commands',
            'browser_sessions',
            'browser_tabs',
            'browser_frames',
            'browser_input_events',
          ]) {
            const bridge = await state.sync.startCollection(collection);
            await bounded(bridge?.state?.awaitInitialReplication?.(), 20000);
            await bounded(bridge?.state?.awaitInSync?.(), 20000);
          }
          const waitForBrowserRoot = async () => {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const root = document.querySelector('[data-browser-root]');
              if (root) return root;
              location.hash = 'browser';
              await delay(250);
            }
            throw new Error(`Browser module root did not appear: ${document.body?.innerText?.slice(0, 500) || ''}`);
          };
          const root = await waitForBrowserRoot();
          const address = root.querySelector('[data-browser-address]');
          if (!address) throw new Error('Browser address input not found');

          // The local Business OS server is a deterministic, offline navigation
          // target. Two distinct query strings prove the real runtime actually
          // navigates and produces a fresh frame per navigation.
          const origin = location.origin;
          const url1 = `${origin}/index.html?probe=1`;
          const url2 = `${origin}/index.html?probe=2`;
          const setAddress = (value) => {
            address.value = value;
            address.dispatchEvent(new Event('input', { bubbles: true }));
          };
          const submitAddress = () => {
            const form = root.querySelector('[data-browser-address-form]');
            if (!form) throw new Error('Browser address form not found');
            form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
          };

          setAddress(url1);
          const startedAt = Date.now();
          root.querySelector('[data-browser-start]')?.click();

          const latestRealFrame = async () => docsToJson(await db.browser_frames?.find().exec())
            .filter((item) => item.session_id === 'browser_session_default' && item.data && Number(item.expires_at_ms || 0) > Date.now())
            .sort((a, b) => Number(b.seq || 0) - Number(a.seq || 0))[0] || null;
          const b64Len = (data) => {
            const clean = String(data || '').replace(/=+$/, '');
            return Math.floor((clean.length * 3) / 4);
          };

          let firstFrame = null;
          let firstState = null;
          {
            const deadline = Date.now() + 90000;
            while (Date.now() < deadline) {
              const frame = await latestRealFrame();
              const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
              const tab = (await db.browser_tabs?.findOne('browser_tab_default').exec())?.toJSON?.() || null;
              firstState = {
                frameSeq: frame?.seq ?? null,
                frameBytes: frame ? b64Len(frame.data) : 0,
                runtimeStatus: session?.runtime_status || '',
                tabUrl: tab?.url || '',
                sessionError: session?.error || '',
              };
              if (
                frame?.data &&
                Number(frame.seq || 0) >= 1 &&
                b64Len(frame.data) > 1000 &&
                session?.runtime_status === 'active' &&
                String(tab?.url || '').includes('probe=1')
              ) {
                firstFrame = frame;
                break;
              }
              await delay(500);
            }
          }
          if (!firstFrame) {
            throw new Error(`Browser input runtime smoke did not observe a real first frame: ${JSON.stringify(firstState, null, 2)}`);
          }

          // Navigate to a second URL; assert URL and frame both advance.
          setAddress(url2);
          submitAddress();
          let secondFrame = null;
          let secondState = null;
          {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const frame = await latestRealFrame();
              const tab = (await db.browser_tabs?.findOne('browser_tab_default').exec())?.toJSON?.() || null;
              secondState = {
                frameSeq: frame?.seq ?? null,
                tabUrl: tab?.url || '',
              };
              if (
                frame?.data &&
                Number(frame.seq || 0) > Number(firstFrame.seq || 0) &&
                String(tab?.url || '').includes('probe=2')
              ) {
                secondFrame = frame;
                break;
              }
              await delay(500);
            }
          }
          if (!secondFrame) {
            throw new Error(`Browser input runtime smoke did not advance frame after navigation: ${JSON.stringify({ firstSeq: firstFrame.seq, secondState }, null, 2)}`);
          }

          const canvas = root.querySelector('[data-browser-canvas]');
          if (!canvas) throw new Error('Browser canvas not found for input runtime smoke');
          const rect = canvas.getBoundingClientRect();
          if (!(rect.width > 0 && rect.height > 0)) {
            throw new Error(`Browser canvas has no layout box: ${JSON.stringify({ width: rect.width, height: rect.height })}`);
          }
          return {
            startedAt,
            firstSeq: Number(firstFrame.seq || 0),
            firstBytes: b64Len(firstFrame.data),
            secondSeq: Number(secondFrame.seq || 0),
            canvasRect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
          };
        });

        // Drive a trusted pointer click through Playwright so the canvas input
        // handler enqueues a real browser_input_events row.
        const clickX = phase1.canvasRect.x + Math.min(80, phase1.canvasRect.width / 2);
        const clickY = phase1.canvasRect.y + Math.min(80, phase1.canvasRect.height / 2);
        await page.mouse.move(clickX, clickY);
        await page.mouse.down();
        await page.mouse.up();

        const phase2 = await page.evaluate(async (prevSeq) => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const docsToJson = (docs) => (docs || []).map((doc) => doc?.toJSON?.() || doc).filter(Boolean);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          const root = document.querySelector('[data-browser-root]');

          let consumed = null;
          let inputState = null;
          {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const events = docsToJson(await db.browser_input_events?.find().exec())
                .filter((event) => event.session_id === 'browser_session_default');
              const frame = docsToJson(await db.browser_frames?.find().exec())
                .filter((item) => item.session_id === 'browser_session_default' && item.data && Number(item.expires_at_ms || 0) > Date.now())
                .sort((a, b) => Number(b.seq || 0) - Number(a.seq || 0))[0] || null;
              const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
              const consumedEvents = events.filter((event) => event.status === 'consumed');
              const failedEvents = events.filter((event) => event.status === 'failed');
              const confirmedInputSeq = Math.max(
                Number(session?.last_input_seq || 0),
                ...consumedEvents.map((event) => Number(event.seq || 0)),
              );
              inputState = {
                totalEvents: events.length,
                consumed: consumedEvents.length,
                failed: failedEvents.length,
                statuses: events.map((event) => `${event.type}:${event.status}`),
                frameSeq: frame?.seq ?? null,
                lastInputSeq: session?.last_input_seq ?? null,
                confirmedInputSeq,
                pendingInputCount: session?.pending_input_count ?? null,
              };
              if (failedEvents.length > 0) {
                throw new Error(`Browser input runtime smoke saw failed input events: ${JSON.stringify(inputState, null, 2)}`);
              }
              if (
                consumedEvents.length >= 1 &&
                frame?.data &&
                Number(frame.seq || 0) > Number(prevSeq || 0) &&
                confirmedInputSeq > 0 &&
                Number(session?.pending_input_count || 0) === 0
              ) {
                consumed = { event: consumedEvents[0], frame, session, confirmedInputSeq };
                break;
              }
              await delay(500);
            }
          }
          if (!consumed) {
            throw new Error(`Browser input runtime smoke did not consume the input event: ${JSON.stringify(inputState, null, 2)}`);
          }

          // Stop the session and assert it tears down cleanly.
          root.querySelector('[data-browser-stop]')?.click();
          let stopped = null;
          {
            const deadline = Date.now() + 45000;
            while (Date.now() < deadline) {
              const session = (await db.browser_sessions?.findOne('browser_session_default').exec())?.toJSON?.() || null;
              const tab = (await db.browser_tabs?.findOne('browser_tab_default').exec())?.toJSON?.() || null;
              stopped = {
                status: session?.status || '',
                runtimeStatus: session?.runtime_status || '',
                tabStatus: tab?.status || '',
              };
              if (session?.status === 'stopped' && session?.runtime_status === 'stopped' && tab?.status === 'stopped') {
                break;
              }
              await delay(500);
            }
          }
          if (!(stopped?.status === 'stopped' && stopped?.runtimeStatus === 'stopped')) {
            throw new Error(`Browser input runtime smoke session did not stop cleanly: ${JSON.stringify(stopped, null, 2)}`);
          }
          return {
            consumedSeq: Number(consumed.event.seq || 0),
            consumedType: consumed.event.type || '',
            frameSeqAfterInput: Number(consumed.frame.seq || 0),
            lastInputSeq: consumed.confirmedInputSeq,
            pendingInputCount: Number(consumed.session.pending_input_count || 0),
            sessionStatus: stopped.status,
            runtimeStatus: stopped.runtimeStatus,
            tabStatus: stopped.tabStatus,
          };
        }, phase1.secondSeq);

        console.log(`browser_input_first_frame_seq=${phase1.firstSeq}`);
        console.log(`browser_input_first_frame_bytes=${phase1.firstBytes}`);
        console.log(`browser_input_second_frame_seq=${phase1.secondSeq}`);
        console.log(`browser_input_consumed_type=${phase2.consumedType}`);
        console.log(`browser_input_consumed_seq=${phase2.consumedSeq}`);
        console.log(`browser_input_frame_seq_after_input=${phase2.frameSeqAfterInput}`);
        console.log(`browser_input_last_input_seq=${phase2.lastInputSeq}`);
        console.log(`browser_input_pending_count=${phase2.pendingInputCount}`);
        console.log(`browser_input_session_status=${phase2.sessionStatus}`);
        console.log(`browser_input_runtime_status=${phase2.runtimeStatus}`);
        console.log(`browser_input_tab_status=${phase2.tabStatus}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'browser-handoff-ui') {
        if (!/#browser(?:$|[?&])/.test(page.url())) {
          const browserUrl = new URL(page.url());
          browserUrl.pathname = '/index.html';
          browserUrl.searchParams.set('rxdbSmoke', '1');
          if (useAppDb && smokeDbId) browserUrl.searchParams.set('smokeDbId', smokeDbId);
          browserUrl.hash = '#browser';
          await page.goto(browserUrl.toString(), { waitUntil: 'domcontentloaded' });
        }
        await page.waitForFunction(() => Boolean(document.querySelector('[data-browser-root]')), null, { timeout: 60000 }).catch(async (error) => {
          const diagnostics = await page.evaluate(() => {
            const state = globalThis.ctoxBusinessOsSmoke?.state;
            return {
              url: location.href,
              hash: location.hash,
              title: document.title,
              bodyText: (document.body?.textContent || '').replace(/\s+/g, ' ').slice(0, 500),
              activeModule: state?.activeModule?.id || null,
              modules: (state?.modules || []).map((mod) => ({
                id: mod.id,
                launch_kind: mod.launch_kind || null,
                shell: mod.layout?.shell || null,
              })),
              windows: Array.from(document.querySelectorAll('.shell-window')).map((win) => ({
                title: win.querySelector('[data-window-title]')?.textContent?.trim() || '',
                owner: win.getAttribute('data-owner-id') || win.dataset.ownerId || '',
                text: (win.textContent || '').replace(/\s+/g, ' ').slice(0, 180),
              })),
              hasCtoxRoot: Boolean(document.querySelector('[data-ctox-root]')),
              hasBrowserRoot: Boolean(document.querySelector('[data-browser-root]')),
            };
          }).catch((diagError) => ({ diagnosticError: diagError?.message || String(diagError) }));
          throw new Error(`Browser handoff UI did not render Browser root: ${error.message}; diagnostics=${JSON.stringify(diagnostics)}`);
        });
        await assertNoVisibleBrowserDebugText(page);
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          return Boolean(state?.sync?.startCollection && state?.commandBus?.dispatch && state?.db?.raw?.business_commands);
        }, null, { timeout: 60000 });
        const handoff = await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const docsToJson = (docs) => (docs || []).map((doc) => doc?.toJSON?.() || doc).filter(Boolean);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          if (!state?.sync?.startCollection || !state?.commandBus?.dispatch || !db?.business_commands) {
            throw new Error('Business OS command runtime is not available for Browser handoff UI smoke');
          }
          for (const collection of [
            'business_commands',
            'ctox_queue_tasks',
            'browser_sessions',
            'browser_tabs',
            'browser_frames',
            'browser_input_events',
          ]) {
            const bridge = await state.sync.startCollection(collection);
            await bounded(bridge?.state?.awaitInitialReplication?.(), 20000);
            await bounded(bridge?.state?.awaitInSync?.(), 20000);
          }
          const waitForBrowserRoot = async () => {
            const deadline = Date.now() + 60000;
            while (Date.now() < deadline) {
              const root = document.querySelector('[data-browser-root]');
              if (root) return root;
              location.hash = 'browser';
              await delay(250);
            }
            throw new Error(`Browser module root did not appear: ${document.body?.innerText?.slice(0, 500) || ''}`);
          };
          const root = await waitForBrowserRoot();
          const address = root.querySelector('[data-browser-address]');
          if (!address) throw new Error('Browser address input not found');
          address.value = 'https://example.com';
          address.dispatchEvent(new Event('input', { bubbles: true }));

          const startedAt = Date.now();
          root.querySelector('[data-browser-seed]')?.click();
          let frame = null;
          let lastSeedState = null;
          {
            const deadline = Date.now() + 30000;
            while (Date.now() < deadline) {
              const frames = docsToJson(await db.browser_frames?.find().exec())
                .filter((item) => item.session_id === 'browser_session_synthetic' && item.data && Number(item.expires_at_ms || 0) > Date.now())
                .sort((a, b) => Number(b.seq || 0) - Number(a.seq || 0));
              const session = (await db.browser_sessions?.findOne('browser_session_synthetic').exec())?.toJSON?.() || null;
              lastSeedState = {
                frameCount: frames.length,
                sessionStatus: session?.status || '',
                runtimeStatus: session?.runtime_status || '',
                sessionError: session?.error || '',
              };
              if (frames[0]?.id) {
                frame = frames[0];
                break;
              }
              await delay(500);
            }
          }
          if (!frame?.id) {
            throw new Error(`Browser handoff UI smoke did not observe a synthetic RxDB frame: ${JSON.stringify(lastSeedState, null, 2)}`);
          }

          root.querySelector('[data-browser-refresh]')?.click();
          await delay(300);
          const send = root.querySelector('[data-browser-send-to-ctox]');
          if (!send) throw new Error('Browser Send to CTOX control not found');
          if (send.disabled) throw new Error('Browser Send to CTOX control is disabled despite an active session');
          send.click();

          const exerciseWebStackCapture = async () => {
            const webStartedAt = Date.now();
            const webSessionId = `browser_session_web_stack_auth_smoke_${webStartedAt}`;
            const webTabId = `browser_tab_web_stack_auth_smoke_${webStartedAt}`;
            const webFrameId = `browser_frame_web_stack_auth_smoke_${webStartedAt}`;
            const webFrameData = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=';
            const webSeq = webStartedAt + 1000000;
            const sourceId = 'linkedin.com';
            const captureScript = 'linkedin.profile_capture.v1';
            const verifySelector = 'a[href*="/in/"]';
            await db.browser_sessions.upsert({
              id: webSessionId,
              owner_user_id: state.user?.id || 'browser-smoke',
              controller_user_id: state.user?.id || 'browser-smoke',
              status: 'active',
              runtime_status: 'active',
              auth_status: 'authenticated',
              current_tab_id: webTabId,
              current_url: 'https://www.linkedin.com/in/smoke',
              title: 'LinkedIn Smoke Profile',
              viewport_w: 1280,
              viewport_h: 720,
              device_scale_factor: 1,
              frame_rate_target: 0,
              active_frame_id: webFrameId,
              last_frame_seq: webSeq,
              last_input_seq: 0,
              pending_input_count: 0,
              payload: {
                purpose: 'web_stack_auth',
                source_id: sourceId,
                secret_name: 'LINKEDIN_SALES_NAV_TOKEN',
                target_url: 'https://www.linkedin.com/login',
                allowed_domains: ['linkedin.com', 'www.linkedin.com', 'api.linkedin.com'],
                verify_selector: verifySelector,
                credential_selector: 'input[name="session_password"]',
                capture_script: captureScript,
                auth_assist_status: 'completed',
                authenticated: true,
                browser_stream: 'rxdb',
                secret_value_in_rxdb: false,
              },
              created_at_ms: webStartedAt,
              updated_at_ms: webStartedAt,
            });
            await db.browser_tabs.upsert({
              id: webTabId,
              session_id: webSessionId,
              title: 'LinkedIn Smoke Profile',
              url: 'https://www.linkedin.com/in/smoke',
              status: 'active',
              loading: false,
              active: true,
              can_go_back: false,
              can_go_forward: false,
              frame_seq: webSeq,
              last_frame_id: webFrameId,
              last_frame_at_ms: webStartedAt,
              payload: { source: 'browser-handoff-ui-smoke' },
              created_at_ms: webStartedAt,
              updated_at_ms: webStartedAt,
            });
            await db.browser_frames.upsert({
              id: webFrameId,
              session_id: webSessionId,
              tab_id: webTabId,
              seq: webSeq,
              mime_type: 'image/png',
              encoding: 'base64',
              data: webFrameData,
              width: 1280,
              height: 720,
              viewport_w: 1280,
              viewport_h: 720,
              quality: 100,
              size_bytes: webFrameData.length,
              frame_hash: 'browser-web-stack-smoke-frame',
              captured_at_ms: webStartedAt,
              expires_at_ms: webStartedAt + 300000,
              updated_at_ms: webStartedAt,
            });
            root.querySelector('[data-browser-refresh]')?.click();
            {
              const sessionDeadline = Date.now() + 30000;
              let sessionButton = null;
              while (Date.now() < sessionDeadline) {
                sessionButton = root.querySelector(`[data-browser-session-id="${webSessionId}"]`);
                if (sessionButton) break;
                root.querySelector('[data-browser-refresh]')?.click();
                await delay(250);
              }
              if (!sessionButton) {
                throw new Error(`Browser Web Stack session did not render in session list: ${root.querySelector('[data-browser-session-list]')?.textContent || ''}`);
              }
              sessionButton.click();
            }
            let captureButton = null;
            {
              const renderDeadline = Date.now() + 30000;
              while (Date.now() < renderDeadline) {
                captureButton = root.querySelector('[data-browser-web-stack-capture]');
                const authText = root.querySelector('[data-browser-auth-assist]')?.textContent || '';
                if (captureButton && !captureButton.disabled && authText.includes(captureScript)) break;
                await delay(250);
              }
            }
            if (!captureButton || captureButton.disabled) {
              throw new Error(`Browser Web Stack capture control did not become available: ${root.querySelector('[data-browser-auth-assist]')?.textContent || ''}`);
            }
            captureButton.click();
            const exerciseWebStackExtract = async () => {
              let extractButton = null;
              const renderDeadline = Date.now() + 30000;
              while (Date.now() < renderDeadline) {
                extractButton = root.querySelector('[data-browser-web-stack-extract]');
                if (extractButton && !extractButton.disabled) break;
                await delay(250);
              }
              if (!extractButton || extractButton.disabled) {
                throw new Error(`Browser Web Stack extract control did not become available: ${root.querySelector('[data-browser-auth-assist]')?.textContent || ''}`);
              }
              extractButton.click();
              const extractDeadline = Date.now() + 90000;
              let extractLast = null;
              while (Date.now() < extractDeadline) {
                const commands = docsToJson(await db.business_commands.find().exec())
                  .filter((command) => {
                    const type = command.command_type || command.type || '';
                    return type === 'browser.capture.extract'
                      && command.record_id === webSessionId
                      && Number(command.created_at_ms || command.updated_at_ms || 0) >= webStartedAt - 1000;
                  })
                  .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
                const command = commands[0] || null;
                const payloadJson = command ? JSON.stringify(command.payload || {}) : '';
                extractLast = { command, payloadJson };
                if (
                  command &&
                  ['pending_sync', 'accepted'].includes(command.status) &&
                  command.payload?.source_id === sourceId &&
                  command.payload?.capture_script === captureScript &&
                  command.payload?.frame_id === webFrameId &&
                  command.payload?.secret_value_in_payload === false &&
                  command.payload?.frame_data_in_payload === false &&
                  command.payload?.browser_context_artifact?.kind === 'browser_context' &&
                  command.payload?.browser_context_artifact?.source_id === sourceId &&
                  command.payload?.browser_context_artifact?.capture_script === captureScript &&
                  command.payload?.browser_context_artifact?.frame_data_in_payload === false &&
                  command.payload?.browser_context_artifact?.browser_context?.frame_id === webFrameId &&
                  command.payload?.browser_context_artifact?.browser_context?.frame_data_in_payload === false &&
                  !payloadJson.includes(webFrameData) &&
                  !payloadJson.includes('smoke-secret')
                ) {
                  return {
                    commandId: command.command_id || command.id,
                    status: command.status,
                    frameDataInPayload: command.payload.frame_data_in_payload,
                    artifactKind: command.payload.browser_context_artifact.kind,
                  };
                }
                await delay(500);
              }
              throw new Error(`Browser Web Stack extract UI smoke did not converge: ${JSON.stringify(extractLast, null, 2)}`);
            };
            const webDeadline = Date.now() + 90000;
            let webLast = null;
            while (Date.now() < webDeadline) {
              const commands = docsToJson(await db.business_commands.find().exec())
                .filter((command) => {
                  const type = command.command_type || command.type || '';
                  return type === 'ctox.browser_context.capture'
                    && command.module === 'ctox'
                    && command.record_id === webSessionId
                    && Number(command.created_at_ms || command.updated_at_ms || 0) >= webStartedAt - 1000;
                })
                .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
              const command = commands[0] || null;
              const payloadJson = command ? JSON.stringify(command.payload || {}) : '';
              const taskId = command?.task_id || '';
              const task = taskId
                ? ((await db.ctox_queue_tasks.findOne(taskId).exec())?.toJSON?.() || null)
                : null;
              webLast = { command, task, payloadJson };
              if (
                command?.status === 'accepted' &&
                command?.payload?.source_module === 'web_stack' &&
                command?.payload?.source_id === sourceId &&
                command?.payload?.capture_script === captureScript &&
                command?.payload?.secret_value_in_payload === false &&
                command?.payload?.browser_context?.session_id === webSessionId &&
                command?.payload?.browser_context?.frame_id === webFrameId &&
                command?.payload?.browser_context?.source_id === sourceId &&
                command?.payload?.browser_context?.capture_script === captureScript &&
                command?.payload?.browser_context?.verify_selector === verifySelector &&
                command?.payload?.browser_context?.frame_data_in_payload === false &&
                !payloadJson.includes(webFrameData) &&
                !payloadJson.includes('smoke-secret') &&
                task?.command_id === command.command_id &&
                task?.command_type === 'ctox.browser_context.capture' &&
                task?.inbound_channel === 'browser' &&
                task?.browser_context_artifact?.kind === 'browser_context' &&
                task?.browser_context_artifact?.source_id === sourceId &&
                task?.browser_context_artifact?.capture_script === captureScript &&
                task?.browser_context_artifact?.frame_data_in_payload === false &&
                task?.browser_context_artifact?.browser_context?.frame_id === webFrameId &&
                !JSON.stringify(task.browser_context_artifact).includes(webFrameData) &&
                !JSON.stringify(task.browser_context_artifact).includes('smoke-secret')
              ) {
                const webStackExtract = await exerciseWebStackExtract();
                return {
                  commandId: command.command_id || command.id,
                  sourceId,
                  captureScript,
                  frameDataInPayload: command.payload.browser_context.frame_data_in_payload,
                  artifactKind: task.browser_context_artifact.kind,
                  taskId: task.id,
                  extractCommandId: webStackExtract.commandId,
                  extractStatus: webStackExtract.status,
                  extractFrameDataInPayload: webStackExtract.frameDataInPayload,
                  extractArtifactKind: webStackExtract.artifactKind,
                };
              }
              await delay(500);
            }
            throw new Error(`Browser Web Stack capture UI smoke did not converge: ${JSON.stringify(webLast, null, 2)}`);
          };

          const deadline = Date.now() + 90000;
          let last = null;
          while (Date.now() < deadline) {
            const commands = docsToJson(await db.business_commands.find().exec())
              .filter((command) => {
                const type = command.command_type || command.type || '';
                return type === 'ctox.browser_context.capture'
                  && command.module === 'ctox'
                  && command.record_id === frame.session_id
                  && Number(command.created_at_ms || command.updated_at_ms || 0) >= startedAt - 1000;
              })
              .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0));
            const command = commands[0] || null;
            const taskId = command?.task_id || '';
            const task = taskId
              ? ((await db.ctox_queue_tasks.findOne(taskId).exec())?.toJSON?.() || null)
              : null;
            const capturedFrameId = command?.payload?.browser_context?.frame_id || '';
            const capturedFrame = capturedFrameId
              ? ((await db.browser_frames.findOne(capturedFrameId).exec())?.toJSON?.() || null)
              : null;
            const handoffText = root.querySelector('[data-browser-handoff-history]')?.textContent || '';
            last = {
              command,
              task,
              capturedFrame,
              handoffText,
              frameId: frame.id,
              frameSeq: frame.seq,
            };
            if (
              command?.status === 'accepted' &&
              command?.payload?.browser_context?.session_id === frame.session_id &&
              capturedFrame?.id === capturedFrameId &&
              capturedFrame?.session_id === frame.session_id &&
              capturedFrame?.data &&
              task?.command_id === command.command_id &&
              task?.command_type === 'ctox.browser_context.capture' &&
              task?.inbound_channel === 'browser' &&
              task?.browser_context_artifact?.kind === 'browser_context' &&
              task?.browser_context_artifact?.browser_context?.frame_id === capturedFrameId &&
              task?.browser_context_artifact?.browser_context?.frame_data_in_payload === false &&
              handoffText.includes(task.id)
            ) {
              const webStackCapture = await exerciseWebStackCapture();
              root.querySelector('[data-browser-stop]')?.click();
              return {
                commandId: command.command_id || command.id,
                commandStatus: command.status,
                commandType: command.command_type || command.type || '',
                taskId: task.id,
                taskStatus: task.status || task.route_status || '',
                taskInboundChannel: task.inbound_channel || '',
                frameId: capturedFrame.id,
                frameSeq: capturedFrame.seq,
                handoffVisible: 1,
                webStackCommandId: webStackCapture.commandId,
                webStackSourceId: webStackCapture.sourceId,
                webStackCaptureScript: webStackCapture.captureScript,
                webStackFrameDataInPayload: webStackCapture.frameDataInPayload,
                webStackArtifactKind: webStackCapture.artifactKind,
                webStackTaskId: webStackCapture.taskId,
                webStackExtractCommandId: webStackCapture.extractCommandId,
                webStackExtractStatus: webStackCapture.extractStatus,
                webStackExtractFrameDataInPayload: webStackCapture.extractFrameDataInPayload,
                webStackExtractArtifactKind: webStackCapture.extractArtifactKind,
              };
            }
            await delay(500);
          }
          throw new Error(`Browser handoff UI smoke did not converge: ${JSON.stringify(last, null, 2)}`);
        });
        console.log(`browser_handoff_command_id=${handoff.commandId}`);
        console.log(`browser_handoff_command_status=${handoff.commandStatus}`);
        console.log(`browser_handoff_command_type=${handoff.commandType}`);
        console.log(`browser_handoff_task_id=${handoff.taskId}`);
        console.log(`browser_handoff_task_status=${handoff.taskStatus}`);
        console.log(`browser_handoff_task_inbound_channel=${handoff.taskInboundChannel}`);
        console.log(`browser_handoff_frame_id=${handoff.frameId}`);
        console.log(`browser_handoff_frame_seq=${handoff.frameSeq}`);
        console.log(`browser_handoff_visible=${handoff.handoffVisible}`);
        console.log(`browser_handoff_web_stack_command_id=${handoff.webStackCommandId}`);
        console.log(`browser_handoff_web_stack_source_id=${handoff.webStackSourceId}`);
        console.log(`browser_handoff_web_stack_capture_script=${handoff.webStackCaptureScript}`);
        console.log(`browser_handoff_web_stack_frame_data_in_payload=${handoff.webStackFrameDataInPayload}`);
        console.log(`browser_handoff_web_stack_artifact_kind=${handoff.webStackArtifactKind}`);
        console.log(`browser_handoff_web_stack_task_id=${handoff.webStackTaskId}`);
        console.log(`browser_handoff_web_stack_extract_command_id=${handoff.webStackExtractCommandId}`);
        console.log(`browser_handoff_web_stack_extract_status=${handoff.webStackExtractStatus}`);
        console.log(`browser_handoff_web_stack_extract_frame_data_in_payload=${handoff.webStackExtractFrameDataInPayload}`);
        console.log(`browser_handoff_web_stack_extract_artifact_kind=${handoff.webStackExtractArtifactKind}`);
        emitBrowserDiagnostics();
        return;
      }
      if (smokeMode === 'command-reload-browser-to-rust') {
        const dispatched = await page.evaluate(async () => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          if (!state?.commandBus?.dispatch) throw new Error('Business OS command bus is not available for reload smoke');
          if (!state?.sync?.startCollection) throw new Error('Business OS sync runtime is not available for reload smoke');
          await state.sync.startCollection('business_commands');
          await state.sync.startCollection('ctox_queue_tasks');
          const id = `command_reload_smoke_${Date.now()}`;
          await state.commandBus.dispatch({
            id,
            module: 'ctox',
            type: 'business_os.smoke',
            record_id: '',
            inbound_channel: 'ctox',
            payload: { title: 'WebRTC command reload smoke', instruction: 'smoke test only' },
            client_context: { source: 'rxdb-smoke', reload: true },
          });
          return { id };
        });
        await page.reload({ waitUntil: 'domcontentloaded' });
        await page.waitForFunction(() => {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const modulesLoaded = Array.isArray(state?.modules) && state.modules.length > 0;
          const shellOpened = Boolean(document.body?.dataset?.moduleShell);
          const loading = Boolean(document.body?.dataset?.moduleLoading);
          return modulesLoaded && shellOpened && !loading;
        }, null, { timeout: 60000 });
        const reloadedStatus = await waitForHealthyCompleteStatus(page, {
          timeoutMs: 60000,
          requiredCollections: BUSINESS_OS_COMMAND_STATUS_COLLECTIONS,
        });
        if (!reloadedStatus?.ok) {
          throw new Error(`Business OS advanced status unhealthy after command reload: ${JSON.stringify(reloadedStatus, null, 2)}`);
        }
        assertHealthyAdvancedStatusContract(reloadedStatus);
        const result = await page.evaluate(async ({ id }) => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const db = state?.db?.raw;
          if (!db?.business_commands || !db?.ctox_queue_tasks) {
            throw new Error('Business OS command collections are not available after reload');
          }
          const commandBridge = await state.sync.startCollection('business_commands');
          const queueBridge = await state.sync.startCollection('ctox_queue_tasks');
          await bounded(commandBridge?.state?.awaitInitialReplication?.(), 20000);
          await bounded(queueBridge?.state?.awaitInitialReplication?.(), 20000);
          await bounded(commandBridge?.state?.awaitInSync?.(), 30000);
          await bounded(queueBridge?.state?.awaitInSync?.(), 30000);
          const deadline = Date.now() + 60000;
          while (Date.now() < deadline) {
            const commandDoc = await db.business_commands.findOne(id).exec();
            const command = commandDoc?.toJSON?.();
            const taskId = command?.task_id || '';
            if (command && command.status !== 'pending_sync' && taskId) {
              const taskDoc = await db.ctox_queue_tasks.findOne(taskId).exec();
              const task = taskDoc?.toJSON?.();
              if (task) {
                const queueTasksForCommand = (await db.ctox_queue_tasks.find().exec())
                  .map((doc) => doc.toJSON?.() || doc)
                  .filter((doc) => doc.command_id === id);
                if (queueTasksForCommand.length !== 1) {
                  throw new Error(`command ${id} produced ${queueTasksForCommand.length} queue tasks after reload: ${JSON.stringify(queueTasksForCommand)}`);
                }
                return {
                  id,
                  status: command.status,
                  taskId,
                  taskStatus: command.task_status || task.status || '',
                  taskCountForCommand: queueTasksForCommand.length,
                  reloaded: true,
                };
              }
            }
            await delay(500);
          }
          const commandDoc = await db.business_commands.findOne(id).exec();
          const queueDocs = await db.ctox_queue_tasks.find({ limit: 10 }).exec();
          throw new Error(`command ${id} was not accepted after Browser reload: ${JSON.stringify({
            command: commandDoc?.toJSON?.() || null,
            queueCount: queueDocs.length,
          })}`);
        }, dispatched);
        const commandTable = 'ctox_business_os__business_commands__v1';
        const taskTable = 'ctox_business_os__ctox_queue_tasks__v0';
        const commandRow = pollSqliteJson(commandTable, result.id);
        const taskRow = pollSqliteJson(taskTable, result.taskId);
        if (commandRow.command_id !== result.id || taskRow.command_id !== result.id) {
          throw new Error(`reload command/task rows mismatch: ${JSON.stringify({ commandRow, taskRow })}`);
        }
        console.log(`command_id=${result.id}`);
        console.log(`task_id=${result.taskId}`);
        console.log(`task_count_for_command=${result.taskCountForCommand}`);
        console.log(`status=${result.status}`);
        console.log(`task_status=${result.taskStatus}`);
        console.log(`reload_verified=${result.reloaded ? 1 : 0}`);
        emitBrowserDiagnostics();
        return;
      }
    }

    const browserPayload = hasOwn(process.env, 'SMOKE_BROWSER_FILE_CONTENT')
      ? process.env.SMOKE_BROWSER_FILE_CONTENT
      : 'hello';
    let rolesPermissionsReloadVerified = false;
    let dynamicAppsReloadVerified = false;
    let appReleaseReloadVerified = false;
    let appAudienceReloadVerified = false;
    if (smokeMode === 'business-os-roles-permissions-ui'
      || smokeMode === 'business-os-dynamic-apps-ui'
      || smokeMode === 'business-os-app-release-ui'
      || smokeMode === 'business-os-app-audience-ui') {
      if (smokeMode === 'business-os-dynamic-apps-ui') {
        await page.evaluate(async (fixtureModule) => {
          const state = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP;
          const collection = state?.db?.collection?.('business_module_catalog');
          if (!collection) throw new Error('business_module_catalog is unavailable before dynamic app reload fixture seed');
          const doc = await collection.findOne('module-catalog').exec();
          const existing = doc?.toJSON?.() || {
            id: 'module-catalog',
            ok: true,
            modules: [],
            templates: [],
            governance: null,
          };
          const {
            _rev,
            _attachments,
            ...catalog
          } = existing;
          void _rev;
          void _attachments;
          const modules = Array.isArray(catalog.modules) ? catalog.modules : [];
          const nextModules = [
            ...modules.filter((mod) => mod?.id !== fixtureModule.id),
            fixtureModule,
          ];
          const nextCatalog = {
            ...catalog,
            id: 'module-catalog',
            ok: catalog.ok !== false,
            modules: nextModules,
            updated_at_ms: Date.now(),
            source: catalog.source || 'business-os-dynamic-apps-smoke',
          };
          if (Array.isArray(catalog.allowed_module_ids)) {
            nextCatalog.allowed_module_ids = [...new Set([
              ...catalog.allowed_module_ids.map((id) => String(id || '').trim()).filter(Boolean),
              fixtureModule.id,
            ])];
          }
          if (typeof collection.upsert === 'function') {
            await collection.upsert(nextCatalog);
          } else if (doc && typeof doc.incrementalPatch === 'function') {
            await doc.incrementalPatch(nextCatalog);
          } else {
            await collection.insert(nextCatalog);
          }
        }, dynamicOpenModuleFixture.module);
      } else if (smokeMode === 'business-os-app-release-ui') {
        await page.evaluate(async () => {
          const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
          const bounded = (promise, ms) => promise
            ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
            : delay(0);
          const state = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP;
          const bridge = await (
            state?.sync?.restartCollection?.('business_module_catalog')
            || state?.sync?.startCollection?.('business_module_catalog')
          );
          await bounded(bridge?.state?.awaitInitialReplication?.(), 15000);
          await bounded(bridge?.state?.awaitInSync?.(), 15000);
        });
      }
      const appShellReloadStartedAt = Date.now();
      await page.reload({ waitUntil: 'commit', timeout: pageNavigationTimeoutMs });
      await page.waitForFunction(() => Boolean(
        globalThis.ctoxBusinessOsSmoke
          && globalThis.ctoxBusinessOsSmoke.bootstrap !== 'inline'
          && globalThis.CTOX_BUSINESS_OS_STATUS
      ), null, { timeout: smokeHookWaitTimeoutMs });
      const reloadMs = Date.now() - appShellReloadStartedAt;
      if (smokeMode === 'business-os-roles-permissions-ui') {
        outerPhaseTimings.rolesPermissionsReloadMs = reloadMs;
        rolesPermissionsReloadVerified = true;
      } else if (smokeMode === 'business-os-dynamic-apps-ui') {
        outerPhaseTimings.dynamicAppsReloadMs = reloadMs;
        dynamicAppsReloadVerified = true;
      } else if (smokeMode === 'business-os-app-release-ui') {
        outerPhaseTimings.appReleaseReloadMs = reloadMs;
        appReleaseReloadVerified = true;
      } else {
        outerPhaseTimings.appAudienceReloadMs = reloadMs;
        appAudienceReloadVerified = true;
      }
    }
    if (smokeMode === 'business-os-threads-rightclick-ui') {
      // Collection/grant bootstrap advances capability epochs. Issue the
      // requester/reviewer tokens only after the installed shell has completed
      // initial sync and all startup grants are materialized.
      threadsRightClickCapabilities = {
        requester: issueBusinessOsSmokeCapability('threads-requester'),
        reviewer: issueBusinessOsSmokeCapability('threads-reviewer'),
      };
    }
    const pageEvaluateStartedAt = Date.now();
    const officeRestartKind = smokeMode === 'office-document-midflight-restart-browser-to-rust'
      ? 'document'
      : smokeMode === 'office-spreadsheet-midflight-restart-browser-to-rust'
        ? 'spreadsheet'
        : '';
    const officeRestartFixtureBytes = officeRestartKind
      ? {
          kind: officeRestartKind,
          canonical: Array.from(fs.readFileSync(path.join(root, `tests/fixtures/office/${officeRestartKind}/edit-save.${officeRestartKind === 'document' ? 'docx' : 'xlsx'}`))),
          editor: Array.from(fs.readFileSync(path.join(root, `tests/fixtures/office/${officeRestartKind}/edit-save.editor.bin`))),
        }
      : null;
    // Backlog OS-C3: the two-browser mode drives a second isolated peer from
    // the node side instead of the single-page evaluate below.
    const result = smokeMode === 'presence-merge-two-browsers'
      ? await runPresenceMergeTwoBrowsersMode(page)
      : await page.evaluate(async ({ signalingUrl, smokeMode, rustSeed, useAppDb, browserPayload, backgroundQueueTask, advancedStatusEvidenceVersion, advancedStatusEvidenceRuntime, codingAgentSmoke, rolesPermissionsReloadVerified, dynamicAppsReloadVerified, appReleaseReloadVerified, appAudienceReloadVerified, threadsScaleSeed, threadsRightClickCapabilities, officeRestartFixtureBytes }) => {
      if (!globalThis.process) globalThis.process = {};
      if (typeof globalThis.process.nextTick !== 'function') {
        globalThis.process.nextTick = (callback, ...args) => Promise.resolve().then(() => callback(...args));
      }
      const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const bounded = (promise, ms) => promise
        ? Promise.race([promise.catch?.(() => undefined) || promise, delay(ms)])
        : delay(0);
      const describeReplicationPool = (state) => {
        const peerStates = state?.peerStates$?.getValue?.();
        const entries = peerStates && typeof peerStates.values === 'function'
          ? Array.from(peerStates.values())
          : [];
        return {
          peerCount: entries.length,
          forkPeerCount: entries.filter((entry) => entry?.replicationState).length,
          masterPeerCount: entries.filter((entry) => !entry?.replicationState).length,
        };
      };
      const waitForNativePeerOpen = async (state, label, timeoutMs = 60000) => {
        const deadline = Date.now() + timeoutMs;
        let lastSnapshot = null;
        while (Date.now() < deadline) {
          await bounded(state?.awaitInitialReplication?.(), 5000);
          await bounded(state?.awaitInSync?.(), 5000);
          const peerStates = state?.peerStates$?.getValue?.();
          const entries = peerStates && typeof peerStates.entries === 'function'
            ? Array.from(peerStates.entries())
            : [];
          const nativeEntry = entries.find(([, entry]) => entry?.remoteProtocol?.peerSession?.role === 'ctox_instance');
          const nativePeerId = nativeEntry?.[0] || '';
          const connection = nativePeerId ? state?.peer?.connections?.get?.(nativePeerId) : null;
          const channelState = connection?.channel?.readyState || '';
          const pcState = connection?.peer?.connectionState || '';
          lastSnapshot = {
            label,
            peerCount: entries.length,
            nativePeerId,
            channelState,
            pcState,
            peers: entries.map(([peerId, entry]) => ({
              peerId,
              role: entry?.remoteProtocol?.peerSession?.role || '',
              sessionId: entry?.remoteProtocol?.peerSession?.sessionId || '',
            })),
          };
          if (nativePeerId && channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(pcState)) {
            return lastSnapshot;
          }
          await delay(500);
        }
        throw new Error(`Timed out waiting for open native peer on ${label}: ${JSON.stringify(lastSnapshot)}`);
      };
      const isRecoverablePeerLifecycleError = (error) => {
        const haystack = [
          error?.code,
          error?.parameters?.error?.code,
          error?.message,
          (() => {
            try { return JSON.stringify(error?.parameters || null); } catch { return ''; }
          })(),
        ].filter(Boolean).join('\n');
        return haystack.includes('ERR_CONNECTION_FAILURE')
          || haystack.includes('ctox_data_channel_error')
          || haystack.includes('ERR_SET_LOCAL_DESCRIPTION')
          || haystack.includes('ERR_PC_CONSTRUCTOR')
          || haystack.includes('Cannot create so many PeerConnections')
          || haystack.includes('Still in CONNECTING state');
      };
      const logUnexpectedReplicationError = (label, error) => {
        if (isRecoverablePeerLifecycleError(error)) return;
        console.error(label, error);
      };
      const demandOnlyAppSyncCollections = new Set([
        'desktop_file_chunks',
        'document_blob_chunks',
        'spreadsheet_blob_chunks',
      ]);
      const syncBridgeFromHandle = (handle) => handle?.bridge || handle;
      const startAppSyncCollection = async (state, collection, reason = 'browser-rust-smoke') => {
        if (!state?.sync?.startCollection && !state?.sync?.leaseCollection) {
          throw new Error(`Business OS sync runtime is not available for ${collection}`);
        }
        if (demandOnlyAppSyncCollections.has(collection)) {
          if (typeof state.sync.leaseCollection !== 'function') {
            throw new Error(`${collection} is demand-only and requires sync.leaseCollection().`);
          }
          return syncBridgeFromHandle(await state.sync.leaseCollection(collection, reason));
        }
        return state.sync.startCollection(collection);
      };

      let db;
      let appFileReplicationState = null;
      let appChunkReplicationState = null;
      let appCommandReplicationState = null;
      let appQueueReplicationState = null;
      let appCodingAgentProjectionStates = [];
      let appTicketItemReplicationState = null;
      let appTicketEventReplicationState = null;
      let appTicketClarificationReplicationState = null;
      let ownsDb = false;
      let advancedStatusVersion = advancedStatusEvidenceVersion || '';
      let advancedStatusRuntime = advancedStatusEvidenceRuntime || null;
      const replicationStates = [];
      const ticketClarificationSmokeMode = smokeMode === 'tickets-clarification-browser-to-rust';
      const ticketSmokeMode = smokeMode === 'tickets-browser-to-rust' || ticketClarificationSmokeMode;
      const outboundActiveUiSmokeMode = smokeMode === 'outbound-active-ui';
      const codingAgentsUiSmokeMode = smokeMode === 'coding-agents-ui';
      const spreadsheetsActiveUiSmokeMode = smokeMode === 'spreadsheets-active-ui';
      const documentsActiveUiSmokeMode = smokeMode === 'documents-active-ui';
      const invoicesActiveUiSmokeMode = smokeMode === 'invoices-active-ui';
      const buchhaltungActiveUiSmokeMode = smokeMode === 'buchhaltung-active-ui';
      const businessOsAppReleaseUiSmokeMode = smokeMode === 'business-os-app-release-ui';
      // A cold release-policy fixture registers the full Business OS catalog
      // before command replication. Keep its native-peer deadline aligned
      // with the mode's 240 s startup deadline; this remains one attempt and
      // does not relax any warning, error or request-failure budget.
      const nativePeerOpenTimeoutMs = businessOsAppReleaseUiSmokeMode ? 240000 : 60000;
      const businessOsThreadsRightClickUiSmokeMode = smokeMode === 'business-os-threads-rightclick-ui';
      const businessOsThreadsScaleUiSmokeMode = smokeMode === 'business-os-threads-scale-ui';
      const commandSmokeMode = smokeMode === 'command-browser-to-rust'
        || smokeMode === 'migration-version-browser-to-rust'
        || smokeMode === 'command-burst-browser-to-rust'
        || smokeMode === 'command-reload-browser-to-rust'
        || smokeMode === 'command-restart-browser-to-rust'
        || smokeMode === 'command-midflight-restart-browser-to-rust'
        || smokeMode === 'office-document-midflight-restart-browser-to-rust'
        || smokeMode === 'office-spreadsheet-midflight-restart-browser-to-rust';
      const officeDocumentRestartSmokeMode = smokeMode === 'office-document-midflight-restart-browser-to-rust';
      const officeSpreadsheetRestartSmokeMode = smokeMode === 'office-spreadsheet-midflight-restart-browser-to-rust';
      const officeRestartSmokeMode = officeDocumentRestartSmokeMode || officeSpreadsheetRestartSmokeMode;
      const materializeSmokeMode = smokeMode === 'workspace-large-materialize-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser';
      const backgroundIndexerSmokeMode = smokeMode === 'workspace-agent-artifacts-background-rust-to-browser';
      const deferInitialFileCollections = smokeMode === 'file-chunk-tombstone-error-browser-status';
      const needsCommandCollections = commandSmokeMode
        || materializeSmokeMode
        || ticketSmokeMode
        || outboundActiveUiSmokeMode
        || codingAgentsUiSmokeMode
        || businessOsAppReleaseUiSmokeMode
        || businessOsThreadsRightClickUiSmokeMode
        || businessOsThreadsScaleUiSmokeMode;
      const needsCodingAgentCollections = codingAgentsUiSmokeMode;
      const needsTicketCollections = ticketSmokeMode;
      const needsFileCollections = (
        (!commandSmokeMode && !outboundActiveUiSmokeMode && !codingAgentsUiSmokeMode)
        || materializeSmokeMode
      )
        && !deferInitialFileCollections
        // Audience policy is evaluated from the module catalog, runtime
        // status and shell state. Requiring late file-collection negotiation
        // after its intentional page reload couples this policy gate to an
        // unrelated data plane and made the zero-retry release matrix flaky.
        && smokeMode !== 'business-os-app-audience-ui';
      const setupPhaseTimings = {};

      let appState = null;
      if (useAppDb) {
        const appDbReadyStartedAt = Date.now();
        const deadline = Date.now() + 30000;
        while (Date.now() < deadline) {
          const state = globalThis.ctoxBusinessOsSmoke?.state;
          const raw = state?.db?.raw;
          const hasCollections = backgroundIndexerSmokeMode
            ? Boolean(raw)
            : ((!needsFileCollections || (raw?.desktop_files && raw?.desktop_file_chunks))
              && (!needsCommandCollections || (raw?.business_commands && raw?.ctox_queue_tasks)));
          if (hasCollections && state?.sync) {
            appState = state;
            break;
          }
          await new Promise((resolve) => setTimeout(resolve, 250));
        }
        setupPhaseTimings.appDbReadyMs = Date.now() - appDbReadyStartedAt;
        if (!appState) {
          const smoke = globalThis.ctoxBusinessOsSmoke;
          const raw = smoke?.state?.db?.raw;
          throw new Error(`Business OS app DB did not become available for smoke test: ${JSON.stringify({
            hasSmoke: Boolean(smoke),
            hasDb: Boolean(smoke?.state?.db),
            hasSync: Boolean(smoke?.state?.sync),
            rawCollections: raw ? Object.keys(raw).slice(0, 20) : [],
            status: document.querySelector('[data-status]')?.textContent || '',
            bodyClass: document.body?.className || '',
          })}`);
        }
        if (needsTicketCollections) {
          const requiredTicketCollections = ['ctox_ticket_items', 'ctox_ticket_events'];
          if (ticketClarificationSmokeMode) requiredTicketCollections.push('ctox_ticket_clarification_requests');
          const missingTicketCollections = requiredTicketCollections.filter((name) => !appState.db.raw?.[name]);
          if (missingTicketCollections.length) {
            const ticketSchemaMod = await import('/modules/tickets/schema.js');
            const ticketCollections = {};
            for (const name of missingTicketCollections) {
              ticketCollections[name] = { schema: ticketSchemaMod.collections[name] };
            }
            await appState.db.raw.addCollections(ticketCollections);
          }
        }
        if (needsCodingAgentCollections) {
          const requiredCodingAgentCollections = [
            'coding_agent_workspace_grants',
            'coding_agent_sessions',
            'coding_agent_events',
          ];
          const missingCodingAgentCollections = requiredCodingAgentCollections.filter((name) => !appState.db.raw?.[name]);
          if (missingCodingAgentCollections.length) {
            const codingAgentsSchemaMod = await import('/modules/coding-agents/schema.js');
            const codingAgentCollections = {};
            for (const name of missingCodingAgentCollections) {
              codingAgentCollections[name] = { schema: codingAgentsSchemaMod.collections[name] };
            }
            await appState.db.raw.addCollections(codingAgentCollections);
          }
        }
        if (outboundActiveUiSmokeMode) {
          const outboundSchemaMod = await import('/modules/outbound/schema.js');
          const outboundCollections = {};
          for (const [name, schema] of Object.entries(outboundSchemaMod.collections || {})) {
            if (!appState.db.raw?.[name]) outboundCollections[name] = { schema };
          }
          if (Object.keys(outboundCollections).length) {
            await appState.db.raw.addCollections(outboundCollections);
          }
        }
        if (spreadsheetsActiveUiSmokeMode || officeSpreadsheetRestartSmokeMode) {
          const schemaMod = await import('/modules/spreadsheets/schema.js');
          const missing = {};
          for (const [name, schema] of Object.entries(schemaMod.collections || {})) {
            if (!appState.db.raw?.[name]) missing[name] = { schema };
          }
          if (Object.keys(missing).length) await appState.db.raw.addCollections(missing);
          const handles = await Promise.all([
            startAppSyncCollection(appState, 'spreadsheets', officeSpreadsheetRestartSmokeMode ? 'office-restart-smoke' : 'spreadsheets-active-ui'),
            startAppSyncCollection(appState, 'spreadsheet_versions', officeSpreadsheetRestartSmokeMode ? 'office-restart-smoke' : 'spreadsheets-active-ui'),
            startAppSyncCollection(appState, 'spreadsheet_blob_chunks', officeSpreadsheetRestartSmokeMode ? 'office-restart-smoke' : 'spreadsheets-active-ui'),
          ]);
          appSpreadsheetProjectionStates = handles.map((handle) => handle?.state || handle).filter(Boolean);
          await Promise.all(appSpreadsheetProjectionStates.map((state) => bounded(state?.awaitInitialReplication?.(), 15000)));
        }
        if (documentsActiveUiSmokeMode || officeDocumentRestartSmokeMode) {
          const schemaMod = await import('/modules/documents/schema.js');
          const missing = {};
          for (const [name, schema] of Object.entries(schemaMod.collections || {})) {
            if (!appState.db.raw?.[name]) missing[name] = { schema };
          }
          if (Object.keys(missing).length) await appState.db.raw.addCollections(missing);
          const handles = await Promise.all([
            startAppSyncCollection(appState, 'documents', officeDocumentRestartSmokeMode ? 'office-restart-smoke' : 'documents-active-ui'),
            startAppSyncCollection(appState, 'document_versions', officeDocumentRestartSmokeMode ? 'office-restart-smoke' : 'documents-active-ui'),
            startAppSyncCollection(appState, 'document_blob_chunks', officeDocumentRestartSmokeMode ? 'office-restart-smoke' : 'documents-active-ui'),
          ]);
          appDocumentProjectionStates = handles.map((handle) => handle?.state || handle).filter(Boolean);
          await Promise.all(appDocumentProjectionStates.map((state) => bounded(state?.awaitInitialReplication?.(), 15000)));
        }
        if (invoicesActiveUiSmokeMode) {
          const schemaMod = await import('/modules/invoices/schema.js');
          const required = ['customer_accounts', 'accounting_invoices'];
          const missing = {};
          for (const name of required) {
            if (!appState.db.raw?.[name]) {
              const definition = schemaMod.collections[name];
              missing[name] = definition?.schema ? definition : { schema: definition };
            }
          }
          if (Object.keys(missing).length) await appState.db.raw.addCollections(missing);
          const handles = await Promise.all(required.map((name) => startAppSyncCollection(appState, name, 'invoices-active-ui')));
          appInvoiceProjectionStates = handles.map((handle) => handle?.state || handle).filter(Boolean);
          await Promise.all(appInvoiceProjectionStates.map((state) => bounded(state?.awaitInitialReplication?.(), 15000)));
        }
        if (buchhaltungActiveUiSmokeMode) {
          const schemaMod = await import('/modules/buchhaltung/schema.js');
          const required = [
            'accounting_accounts',
            'accounting_journal_entries',
            'accounting_journal_entry_lines',
          ];
          const missing = {};
          for (const name of required) {
            if (!appState.db.raw?.[name]) missing[name] = { schema: schemaMod.collections[name] };
          }
          if (Object.keys(missing).length) await appState.db.raw.addCollections(missing);
          const handles = await Promise.all(required.map((name) => startAppSyncCollection(appState, name, 'buchhaltung-active-ui')));
          appAccountingProjectionStates = handles.map((handle) => handle?.state || handle).filter(Boolean);
          await Promise.all(appAccountingProjectionStates.map((state) => bounded(state?.awaitInitialReplication?.(), 15000)));
        }
        if (needsCommandCollections) {
          const commandCollectionsStartedAt = Date.now();
          const commandBridge = await appState.sync.startCollection('business_commands');
          const queueBridge = await appState.sync.startCollection('ctox_queue_tasks');
          commandBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app business_commands replication error', error));
          queueBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app ctox_queue_tasks replication error', error));
          appCommandReplicationState = commandBridge?.state || null;
          appQueueReplicationState = queueBridge?.state || null;
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 15000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 15000);
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands', nativePeerOpenTimeoutMs);
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks', nativePeerOpenTimeoutMs);
          setupPhaseTimings.commandCollectionsReadyMs = Date.now() - commandCollectionsStartedAt;
        }
        if (needsCodingAgentCollections) {
          const codingAgentCollectionsStartedAt = Date.now();
          const projectionCollections = [
            'coding_agent_workspace_grants',
            'coding_agent_sessions',
            'coding_agent_events',
          ];
          const bridges = [];
          for (const collection of projectionCollections) {
            const bridge = await appState.sync.startCollection(collection);
            bridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError(`app ${collection} replication error`, error));
            bridges.push(bridge);
          }
          appCodingAgentProjectionStates = bridges.map((bridge) => bridge?.state).filter(Boolean);
          await Promise.all(appCodingAgentProjectionStates.map((state) => bounded(state?.awaitInitialReplication?.(), 15000)));
          await Promise.all(appCodingAgentProjectionStates.map((state) => bounded(state?.awaitInSync?.(), 15000)));
          for (let index = 0; index < projectionCollections.length; index += 1) {
            await waitForNativePeerOpen(appCodingAgentProjectionStates[index], projectionCollections[index]);
          }
          setupPhaseTimings.codingAgentCollectionsReadyMs = Date.now() - codingAgentCollectionsStartedAt;
        }
        if (needsFileCollections) {
          const fileCollectionsStartedAt = Date.now();
          const fileBridge = await startAppSyncCollection(appState, 'desktop_files', 'browser-rust-smoke-file-sync');
          const chunkBridge = await startAppSyncCollection(appState, 'desktop_file_chunks', 'browser-rust-smoke-file-sync');
          fileBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app desktop_files replication error', error));
          chunkBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app desktop_file_chunks replication error', error));
          appFileReplicationState = fileBridge?.state || null;
          appChunkReplicationState = chunkBridge?.state || null;
          await bounded(appFileReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appFileReplicationState?.awaitInSync?.(), 15000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 15000);
          await waitForNativePeerOpen(appFileReplicationState, 'desktop_files');
          await waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks');
          setupPhaseTimings.fileCollectionsReadyMs = Date.now() - fileCollectionsStartedAt;
        }
        if (needsTicketCollections) {
          const ticketCollectionsStartedAt = Date.now();
          const ticketItemBridge = await appState.sync.startCollection('ctox_ticket_items');
          const ticketEventBridge = await appState.sync.startCollection('ctox_ticket_events');
          const ticketClarificationBridge = ticketClarificationSmokeMode
            ? await appState.sync.startCollection('ctox_ticket_clarification_requests')
            : null;
          ticketItemBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app ctox_ticket_items replication error', error));
          ticketEventBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app ctox_ticket_events replication error', error));
          ticketClarificationBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app ctox_ticket_clarification_requests replication error', error));
          appTicketItemReplicationState = ticketItemBridge?.state || null;
          appTicketEventReplicationState = ticketEventBridge?.state || null;
          appTicketClarificationReplicationState = ticketClarificationBridge?.state || null;
          await bounded(appTicketItemReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appTicketEventReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appTicketClarificationReplicationState?.awaitInitialReplication?.(), 15000);
          await bounded(appTicketItemReplicationState?.awaitInSync?.(), 15000);
          await bounded(appTicketEventReplicationState?.awaitInSync?.(), 15000);
          await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 15000);
          await waitForNativePeerOpen(appTicketItemReplicationState, 'ctox_ticket_items');
          await waitForNativePeerOpen(appTicketEventReplicationState, 'ctox_ticket_events');
          if (ticketClarificationSmokeMode) {
            await waitForNativePeerOpen(appTicketClarificationReplicationState, 'ctox_ticket_clarification_requests');
          }
          setupPhaseTimings.ticketCollectionsReadyMs = Date.now() - ticketCollectionsStartedAt;
        }
        db = appState.db.raw;
      } else {
        const config = globalThis.CTOX_BUSINESS_OS_CONFIG
          || await fetch('/index.html?rxdbSmoke=1')
            .then((res) => res.text())
            .then((html) => {
              const marker = 'window.CTOX_BUSINESS_OS_CONFIG=';
              const start = html.indexOf(marker);
              if (start === -1) throw new Error('launch sync config missing from index.html');
              const bodyStart = start + marker.length;
              const end = html.indexOf(';</script>', bodyStart);
              if (end === -1) throw new Error('launch sync config script is malformed');
              return JSON.parse(html.slice(bodyStart, end));
            });
        const rxdb = await import('/rxdb/dist/ctox-rxdb-js.mjs');
        registerRxdbPlugin(rxdb, rxdb.RxDBMigrationSchemaPlugin || rxdb.RxDBMigrationPlugin);
        const desktopSchemaMod = await import('/modules/desktop/schema.js');
        const ctoxSchemaMod = await import('/modules/ctox/schema.js');
        db = await rxdb.createRxDatabase({
          name: `ctox_smoke_${Date.now()}`,
          storage: rxdb.getCtoxIndexedDbStorage(),
          multiInstance: false,
          closeDuplicates: true,
        });
        ownsDb = true;
        const collections = {};
        if (needsCommandCollections) {
          collections.business_commands = { schema: ctoxSchemaMod.collections.business_commands };
          collections.ctox_queue_tasks = { schema: ctoxSchemaMod.collections.ctox_queue_tasks };
        }
        if (needsFileCollections) {
          collections.desktop_files = { schema: desktopSchemaMod.collections.desktop_files };
          collections.desktop_file_chunks = { schema: desktopSchemaMod.collections.desktop_file_chunks };
        }
        await db.addCollections(collections);

        async function startReplication(collectionName) {
          const batchSize = collectionName === 'desktop_file_chunks' ? 2 : 10;
          const replicationState = await rxdb.replicateWebRTC({
            collection: db[collectionName],
            topic: `${config.sync_room}:${collectionName}`,
            connectionHandlerCreator: rxdb.getConnectionHandlerSimplePeer({ signalingServerUrl: signalingUrl }),
            pull: { batchSize },
            push: { batchSize },
            retryTime: 1000,
          });
          replicationState.error$?.subscribe?.((error) => logUnexpectedReplicationError(`${collectionName} replication error`, error));
          replicationStates.push(replicationState);
        }

        if (needsCommandCollections) {
          await startReplication('business_commands');
          await startReplication('ctox_queue_tasks');
        }
        if (needsFileCollections) {
          await startReplication('desktop_files');
          await startReplication('desktop_file_chunks');
        }
        await Promise.all(replicationStates.map((state) => bounded(state?.awaitInitialReplication?.(), 15000)));
        await Promise.all(replicationStates.map((state) => bounded(state?.awaitInSync?.(), 15000)));
      }

      let smokeCapabilityToken = '';
      if (needsCommandCollections) {
        const capabilityResponse = await fetch('/api/business-os/auth/capability', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          credentials: 'same-origin',
          cache: 'no-store',
        });
        if (!capabilityResponse.ok) {
          throw new Error(`Business OS smoke capability request failed: ${capabilityResponse.status} ${await capabilityResponse.text()}`);
        }
        const capability = await capabilityResponse.json();
        smokeCapabilityToken = String(capability?.capability_token || '').trim();
        if (!smokeCapabilityToken) {
          throw new Error(`Business OS smoke capability response did not contain a token: ${JSON.stringify(capability)}`);
        }
      }
      const smokeClientContext = (context = {}) => ({
        ...context,
        ...(smokeCapabilityToken ? { capability_token: smokeCapabilityToken } : {}),
      });

      function selectActiveFileChunks(chunks, contentGenerationId) {
        const candidates = Array.isArray(chunks) ? chunks : [];
        let selected = [];
        if (contentGenerationId) {
          selected = candidates.filter((chunk) => chunk.generation_id === contentGenerationId);
        }
        if (selected.length === 0) {
          const newestCreatedAt = candidates.reduce((max, chunk) => Math.max(max, Number(chunk.created_at_ms || 0)), 0);
          selected = candidates.filter((chunk) => Number(chunk.created_at_ms || 0) === newestCreatedAt);
        }
        selected.sort((left, right) => Number(left.idx || 0) - Number(right.idx || 0));
        const expectedTotal = selected.length > 0 ? Number(selected[0].total || selected.length) : 0;
        return {
          chunks: selected,
          expectedTotal,
          complete: selected.length > 0 && selected.length === expectedTotal,
        };
      }

      async function waitForFile(id, ms = 30000, expectedPayload = null) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          const allChunks = (await db.desktop_file_chunks.find().exec())
            .map((doc) => doc.toJSON?.() || doc)
            .filter((doc) => doc.file_id === id);
          const active = selectActiveFileChunks(allChunks, file?.content_generation_id || '');
          if (file && active.complete) {
            const payload = atob(active.chunks.map((doc) => doc.data).join(''));
            lastSeen = {
              file,
              chunks: active.chunks,
              payload,
              generationId: file.content_generation_id || active.chunks[0]?.generation_id || '',
              allChunkCount: allChunks.length,
            };
            const payloadMatches = expectedPayload === null || payload === expectedPayload;
            const metadataReady = expectedPayload === null || file.content_state === 'available';
            if (payloadMatches && metadataReady) return lastSeen;
          }
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
        const fileDoc = await db.desktop_files.findOne(id).exec();
        const allFileDocs = await db.desktop_files.find().exec();
        const allChunkDocs = await db.desktop_file_chunks.find().exec();
        const fileDocs = allFileDocs.map((doc) => doc.toJSON?.() || doc);
        const chunkDocs = allChunkDocs
          .map((doc) => doc.toJSON?.() || doc)
          .filter((doc) => doc.file_id === id);
        throw new Error(`browser did not receive rust-side file ${id}: ${JSON.stringify({
          hasFile: Boolean(fileDoc),
          fileCount: fileDocs.length,
          fileIds: fileDocs.map((doc) => doc.id).slice(0, 8),
          chunkCount: chunkDocs.length,
          totalChunkCount: allChunkDocs.length,
          expectedPayload: expectedPayload === null ? null : {
            length: expectedPayload.length,
            prefix: expectedPayload.slice(0, 80),
          },
          lastSeen,
          syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
          syncConfig: globalThis.ctoxBusinessOsSmoke?.state?.sync?.config || null,
        })}`);
      }

      async function waitForFileViaDemandFetch(id, ms = 30000, expectedPayload = null, { notGenerationId = '' } = {}) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          // OS-X3: an update wait must not return while the replicated
          // metadata doc still carries the pre-update generation — the
          // demand fetch can serve the NEW bytes before the desktop_files
          // pull delivers the updated doc.
          if (notGenerationId && file && (file.content_generation_id || '') === notGenerationId) {
            lastSeen = { file, demandFetch: true, reason: 'stale-generation-metadata' };
            await delay(500);
            continue;
          }
          const loader = appFileReplicationState?.demandFileLoader || null;
          if (file && loader?.fetchFile) {
            try {
              const demandChunks = (await loader.fetchFile(id))
                .filter((chunk) => chunk && chunk.cancelled !== true)
                .filter((chunk) => typeof (chunk.bytesBase64 ?? chunk.bytes_base64) === 'string')
                .sort((left, right) => Number(left.sequence || 0) - Number(right.sequence || 0));
              const contiguous = demandChunks.length > 0
                && demandChunks.every((chunk, index) => Number(chunk.sequence) === index);
              if (contiguous) {
                const payload = atob(demandChunks.map((chunk) => chunk.bytesBase64 ?? chunk.bytes_base64 ?? '').join(''));
                lastSeen = {
                  file,
                  chunks: demandChunks,
                  payload,
                  generationId: file.content_generation_id || '',
                  demandFetch: true,
                };
                const payloadMatches = expectedPayload === null || payload === expectedPayload;
                const metadataReady = expectedPayload === null || file.content_state === 'available';
                if (payloadMatches && metadataReady) return lastSeen;
              } else {
                lastSeen = {
                  file,
                  demandChunkCount: demandChunks.length,
                  demandFetch: true,
                  reason: 'non-contiguous-demand-chunks',
                };
              }
            } catch (error) {
              lastSeen = {
                file,
                demandFetch: true,
                error: error?.message || String(error),
              };
            }
          } else {
            lastSeen = {
              hasFile: Boolean(file),
              hasDemandFileLoader: Boolean(loader?.fetchFile),
              demandFetch: false,
            };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive rust-side file through demand fetch ${id}: ${JSON.stringify({
          expectedPayload: expectedPayload === null ? null : {
            length: expectedPayload.length,
            prefix: expectedPayload.slice(0, 80),
          },
          lastSeen,
          syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
          syncConfig: globalThis.ctoxBusinessOsSmoke?.state?.sync?.config || null,
        })}`);
      }

      // OS-X3: product-semantics file wait. Under the app DB, chunk
      // collections are demand-only by design (§8.1 docs/ctox-rxdb.md:
      // isDemandOnlyPullCollection => pull disabled, leases included), so
      // waiting for background-pulled chunk DOCS can never succeed there —
      // file bytes must be read the way the product reads them, via
      // rxdb.file.fetch (waitForFileViaDemandFetch). The raw replication
      // wait (waitForFile) remains for explicit non-app pages, which run a
      // direct replicateWebRTC with pull enabled.
      function waitForFileProduct(id, ms, expectedPayload = null, options = {}) {
        return useAppDb
          ? waitForFileViaDemandFetch(id, ms, expectedPayload, options)
          : waitForFile(id, ms, expectedPayload);
      }

      // OS-X3b: product-semantics variant of the multi-file artifacts wait.
      // Under the app DB chunk collections never background-pull (§8.1), so
      // the chunk-doc based wait below can never see bytes — read them the
      // way the product does, via rxdb.file.fetch per expected file. A file
      // only counts once its payload matches AND the replicated metadata doc
      // caught up (size_bytes match) — the demand fetch can serve NEW bytes
      // before the desktop_files pull delivers the updated doc, which would
      // otherwise leak a stale generation id to churn assertions.
      async function waitForWorkspaceArtifactsProduct(expectedFiles, ms = 60000) {
        const expected = Array.isArray(expectedFiles) ? expectedFiles : [];
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const loader = appFileReplicationState?.demandFileLoader || null;
          const receivedFiles = [];
          const missing = [];
          const mismatched = [];
          for (const file of expected) {
            const fileDoc = await db.desktop_files.findOne(file.id).exec();
            const receivedFileDoc = fileDoc?.toJSON?.() || fileDoc;
            if (!receivedFileDoc || !loader?.fetchFile) {
              missing.push({ id: file.id, relativePath: file.relativePath || '', hasFile: Boolean(receivedFileDoc), hasLoader: Boolean(loader?.fetchFile) });
              continue;
            }
            const expectedContent = String(file.content || '');
            const metadataFresh = Number(receivedFileDoc.size_bytes || 0) === expectedContent.length;
            let payload = null;
            try {
              const demandChunks = (await loader.fetchFile(file.id))
                .filter((chunk) => chunk && chunk.cancelled !== true)
                .filter((chunk) => typeof (chunk.bytesBase64 ?? chunk.bytes_base64) === 'string')
                .sort((left, right) => Number(left.sequence || 0) - Number(right.sequence || 0));
              const contiguous = demandChunks.length > 0
                && demandChunks.every((chunk, index) => Number(chunk.sequence) === index);
              if (contiguous) {
                payload = atob(demandChunks.map((chunk) => chunk.bytesBase64 ?? chunk.bytes_base64 ?? '').join(''));
              }
              if (payload === null || payload !== expectedContent || !metadataFresh) {
                mismatched.push({
                  id: file.id,
                  relativePath: file.relativePath || '',
                  expectedLength: expectedContent.length,
                  actualLength: payload === null ? null : payload.length,
                  metadataFresh,
                });
                continue;
              }
              const actualPath = receivedFileDoc.virtual_path || receivedFileDoc.path || '';
              if (file.expectedVirtualPath && actualPath !== file.expectedVirtualPath) {
                throw new Error(`workspace artifact virtual path mismatch: ${JSON.stringify({
                  id: file.id,
                  expected: file.expectedVirtualPath,
                  actual: actualPath,
                })}`);
              }
              receivedFiles.push({
                file: receivedFileDoc,
                chunks: demandChunks,
                payload,
                generationId: receivedFileDoc.content_generation_id || '',
                allChunkCount: demandChunks.length,
                expectedVirtualPath: file.expectedVirtualPath,
                relativePath: file.relativePath || '',
              });
            } catch (error) {
              if (String(error?.message || '').includes('virtual path mismatch')) throw error;
              mismatched.push({ id: file.id, relativePath: file.relativePath || '', error: error?.message || String(error) });
            }
          }
          lastSeen = {
            expectedCount: expected.length,
            receivedCount: receivedFiles.length,
            missing: missing.slice(0, 8),
            mismatched: mismatched.slice(0, 8),
            demandFetch: true,
          };
          if (receivedFiles.length === expected.length) return receivedFiles;
          await delay(500);
        }
        throw new Error(`browser did not receive expected workspace artifacts through demand fetch: ${JSON.stringify(lastSeen)}`);
      }

      // Product semantics under the app DB, raw chunk replication otherwise
      // (mirrors waitForFileProduct).
      function waitForWorkspaceArtifactsAuto(expectedFiles, ms = 60000) {
        return useAppDb
          ? waitForWorkspaceArtifactsProduct(expectedFiles, ms)
          : waitForWorkspaceArtifacts(expectedFiles, ms);
      }

      async function waitForWorkspaceArtifacts(expectedFiles, ms = 60000) {
        const expected = Array.isArray(expectedFiles) ? expectedFiles : [];
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const allFileDocs = (await db.desktop_files.find().exec()).map((doc) => doc.toJSON?.() || doc);
          const allChunkDocs = (await db.desktop_file_chunks.find().exec()).map((doc) => doc.toJSON?.() || doc);
          const filesById = new Map(allFileDocs.map((doc) => [doc.id, doc]));
          const chunksByFileId = new Map();
          for (const chunk of allChunkDocs) {
            const fileId = chunk.file_id || '';
            if (!fileId) continue;
            const list = chunksByFileId.get(fileId) || [];
            list.push(chunk);
            chunksByFileId.set(fileId, list);
          }
          const receivedFiles = [];
          const missing = [];
          const mismatched = [];
          for (const file of expected) {
            const receivedFileDoc = filesById.get(file.id);
            const allChunks = chunksByFileId.get(file.id) || [];
            const active = selectActiveFileChunks(allChunks, receivedFileDoc?.content_generation_id || '');
            if (!receivedFileDoc || !active.complete) {
              missing.push({
                id: file.id,
                relativePath: file.relativePath || '',
                hasFile: Boolean(receivedFileDoc),
                chunkCount: allChunks.length,
                expectedTotal: active.expectedTotal || 0,
              });
              continue;
            }
            const payload = atob(active.chunks.map((doc) => doc.data).join(''));
            const actualPath = receivedFileDoc.virtual_path || receivedFileDoc.path || '';
            if (actualPath !== file.expectedVirtualPath) {
              throw new Error(`workspace artifact virtual path mismatch: ${JSON.stringify({
                id: file.id,
                expected: file.expectedVirtualPath,
                actual: actualPath,
                file: receivedFileDoc,
              })}`);
            }
            if (payload !== file.content) {
              mismatched.push({
                id: file.id,
                relativePath: file.relativePath || '',
                expectedLength: String(file.content || '').length,
                actualLength: payload.length,
                generationId: receivedFileDoc.content_generation_id || active.chunks[0]?.generation_id || '',
              });
              continue;
            }
            receivedFiles.push({
              file: receivedFileDoc,
              chunks: active.chunks,
              payload,
              generationId: receivedFileDoc.content_generation_id || active.chunks[0]?.generation_id || '',
              allChunkCount: allChunks.length,
              expectedVirtualPath: file.expectedVirtualPath,
              relativePath: file.relativePath || '',
            });
          }
          lastSeen = {
            expectedCount: expected.length,
            receivedCount: receivedFiles.length,
            totalFileCount: allFileDocs.length,
            totalChunkCount: allChunkDocs.length,
            missing: missing.slice(0, 8),
            mismatched: mismatched.slice(0, 8),
          };
          if (receivedFiles.length === expected.length) return receivedFiles;
          await delay(500);
        }
        throw new Error(`browser did not receive expected workspace artifacts: ${JSON.stringify(lastSeen)}`);
      }

      async function runBusinessOsUiRegression() {
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            last = await predicate();
            if (last?.ok) return last;
            await delay(100);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const failureText = () => {
          const text = document.body?.innerText || '';
          return /App-Start fehlgeschlagen|Module startup failed|System-Start fehlgeschlagen/i.test(text)
            ? text.slice(0, 1000)
            : '';
        };
        const expectedRequiredModules = ['ctox', 'documents', 'knowledge', 'research'];
        const expectedSecondaryModules = [
          'matching',
          'conversations',
          'outbound',
          'tickets',
          'shiftflow',
          'buchhaltung',
          'coding-agents',
          'app-store',
          'browser',
          'calendar',
          'creator',
          'notes',
          'reports',
          'spreadsheets',
          'appsec-pentest',
          'consent',
          'credentials',
          'customers',
          'cv-print-builder',
          'esign',
          'intake',
          'interviews',
          'iot',
          'nachweise',
          'placements',
          'submissions',
          'support',
          'threads',
        ];
        const moduleCatalog = await waitFor(() => {
          const moduleIds = Array.isArray(appState?.modules)
            ? appState.modules.map((mod) => mod?.id).filter(Boolean)
            : [];
          const requiredModules = expectedRequiredModules.filter((id) => moduleIds.includes(id));
          const secondaryModules = expectedSecondaryModules.filter((id) => moduleIds.includes(id));
          const expectedCatalogModules = [...expectedRequiredModules, ...expectedSecondaryModules];
          return {
            ok: requiredModules.length === expectedRequiredModules.length
              && secondaryModules.length === expectedSecondaryModules.length,
            moduleIds,
            requiredModules,
            secondaryModules,
            missingModules: expectedCatalogModules.filter((id) => !moduleIds.includes(id)),
          };
        }, 15000, 'required Business OS module catalog');
        const moduleIds = moduleCatalog.moduleIds;
        const requiredModules = moduleCatalog.requiredModules;
        const secondaryModules = moduleCatalog.secondaryModules;
        const openStartMenu = async () => {
          const startButton = document.querySelector('[data-shell-start]');
          if (!startButton) throw new Error('Business OS start menu button is missing');
          let panel = document.querySelector('.shell-start-menu-panel');
          if (!panel?.classList?.contains('is-active')) {
            startButton.click();
          }
          return waitFor(() => {
            panel = document.querySelector('.shell-start-menu-panel');
            const items = panel ? [...panel.querySelectorAll('.start-menu-item')] : [];
            return {
              ok: Boolean(panel?.classList?.contains('is-active') && items.length > 0),
              itemCount: items.length,
              labels: items.map((item) => item.textContent?.trim() || '').slice(0, 12),
            };
          }, 5000, 'start menu open');
        };
        const waitForOpenedModule = async (expectedModuleId, label) => {
          const opened = await waitFor(() => {
            const activeModule = document.body?.dataset?.activeModule || appState?.activeModule?.id || '';
            const loading = Boolean(document.body?.dataset?.moduleLoading);
            const errorText = failureText();
            return {
              ok: activeModule === expectedModuleId && !loading && !errorText,
              activeModule,
              loading,
              errorText,
            };
          }, 30000, label);
          return {
            expectedModuleId,
            activeModule: opened.activeModule,
          };
        };
        const openModuleByHash = async (expectedModuleId) => {
          const liveState = globalThis.CTOX_BUSINESS_OS_APP || globalThis.ctoxBusinessOsSmoke?.state || appState;
          if (liveState) appState = liveState;
          const nextHash = `#${expectedModuleId}`;
          if (location.hash !== nextHash) {
            history.replaceState(null, document.title, `${location.pathname}${location.search}${nextHash}`);
          }
          if (typeof liveState?.openModule === 'function') {
            await liveState.openModule(expectedModuleId, { force: true, asModule: true });
          } else {
            location.hash = expectedModuleId;
            window.dispatchEvent(new HashChangeEvent('hashchange'));
          }
          return waitForOpenedModule(expectedModuleId, `open module ${expectedModuleId}`);
        };
        const rectEvidence = (selector) => {
          const element = document.querySelector(selector);
          if (!element) return null;
          const rect = element.getBoundingClientRect();
          const style = getComputedStyle(element);
          return {
            selector,
            width: Math.round(rect.width),
            height: Math.round(rect.height),
            top: Math.round(rect.top),
            left: Math.round(rect.left),
            visible: rect.width > 0
              && rect.height > 0
              && style.display !== 'none'
              && style.visibility !== 'hidden'
              && Number(style.opacity || 1) > 0,
          };
        };
        const moduleRenderContracts = {
          ctox: {
            selectors: ['[data-ctox-harness]', '[data-ctox-left]', '[data-ctox-main]', '[data-flow-control]'],
            minTextLength: 40,
          },
          documents: {
            selectors: ['[data-documents-module]', '.documents-explorer', '[data-documents-list]', '[data-documents-editor]'],
            minTextLength: 40,
          },
          knowledge: {
            selectors: ['[data-knowledge-root]', '[data-knowledge-list]', '[data-markdown-view]'],
            minTextLength: 40,
          },
          research: {
            selectors: ['[data-research-root]', '.research-left', '.research-center', '.research-right'],
            minTextLength: 60,
          },
          matching: {
            selectors: ['.app', '#left', '#center', '#right'],
            minTextLength: 60,
          },
          conversations: {
            selectors: ['[data-conv-root]', '.conv-left', '.conv-center', '.conv-right'],
            minTextLength: 60,
          },
          outbound: {
            selectors: ['[data-outbound-root]', '.outbound-left', '.outbound-center'],
            minTextLength: 30,
          },
          tickets: {
            selectors: ['[data-tickets-root]', '[data-ticket-list]', '[data-ticket-detail]', '[data-ticket-context]'],
            minTextLength: 40,
          },
          shiftflow: {
            selectors: ['[data-shiftflow-root]', '#shiftflow-left', '#shiftflow-center', '#schedulerView'],
            minTextLength: 80,
          },
          buchhaltung: {
            selectors: ['[data-fibu-root]', '.fibu-left', '.fibu-center', '[data-fibu-nav]'],
            minTextLength: 80,
          },
          'coding-agents': {
            selectors: ['[data-coding-agents-root]', '.coding-agents-left', '.coding-agents-center', '#workbench-chat-feed'],
            minTextLength: 80,
          },
          'app-store': {
            selectors: ['[data-app-store-root]', '.store-left', '.store-center', '[data-apps-grid]'],
            minTextLength: 80,
          },
          browser: {
            selectors: ['[data-browser-root]', '.browser-workbench', '[data-browser-canvas]'],
            minTextLength: 70,
          },
          calendar: {
            selectors: ['[data-calendar-root]', '#calendar-left', '#calendar-center', '#calendar-right'],
            minTextLength: 80,
          },
          creator: {
            selectors: ['[data-creator-root]', '.creator-left', '.creator-center', '#expert-accordion-btn'],
            minTextLength: 120,
          },
          notes: {
            selectors: ['[data-notes-root]', '.notes-sidebar-pane', '.notes-list-pane', '.notes-center'],
            minTextLength: 120,
          },
          reports: {
            selectors: ['[data-reports-root]', '.reports-rail', '[data-reports-list]', '[data-reports-detail]'],
            minTextLength: 50,
          },
          spreadsheets: {
            selectors: ['[data-spreadsheets-module]', '[data-spreadsheets-editor]', '.spreadsheets-workbench'],
            minTextLength: 50,
          },
          'appsec-pentest': {
            selectors: ['[data-appsec-root]', '.appsec-rail', '.appsec-main-pane', '[data-appsec-tabs]'],
            minTextLength: 60,
          },
          consent: {
            selectors: ['.ats-consent[data-ats-root]', '.ats-consent .ctox-pane', '.ats-consent [data-ats-form]'],
            minTextLength: 40,
          },
          credentials: {
            selectors: ['[data-credentials-root]', '.credentials-module .ctox-pane', '.cred-body', '[data-cred-add]'],
            minTextLength: 60,
          },
          customers: {
            selectors: ['[data-customers-root]', '.customers-left', '[data-customers-center]', '[data-customers-right]'],
            minTextLength: 80,
          },
          'cv-print-builder': {
            selectors: ['[data-cv-print-builder]', '.cv-sidebar', '.cv-workbench', '[data-cv-search]'],
            minTextLength: 50,
          },
          esign: {
            selectors: ['.ats-esign[data-ats-root]', '.ats-esign .ats-head', '.ats-esign [data-ats-form]'],
            minTextLength: 40,
          },
          intake: {
            selectors: ['.ats-intake[data-ats-root]', '.ats-intake .ats-head', '.ats-intake [data-ats-form]'],
            minTextLength: 40,
          },
          interviews: {
            selectors: ['.ats-interviews[data-ats-root]', '.ats-interviews .ats-head', '.ats-interviews [data-ats-form]'],
            minTextLength: 40,
          },
          buchhaltung: {
            selectors: ['[data-fibu-root]', '[data-fibu-nav]', '[data-panel="skr"]', '[data-search-accounts]'],
            minTextLength: 50,
          },
          iot: {
            selectors: ['[data-iot-root]', '[data-iot-left]', '[data-iot-center]'],
            minTextLength: 50,
          },
          nachweise: {
            selectors: ['.ats-nachweise[data-ats-root]', '.ats-nachweise .ctox-pane', '.ats-nachweise [data-ats-form]'],
            minTextLength: 50,
          },
          placements: {
            selectors: ['.ats-placements[data-ats-root]', '.ats-placements .ats-head', '.ats-placements [data-ats-form]'],
            minTextLength: 40,
          },
          submissions: {
            selectors: ['.ats-submissions[data-ats-root]', '.ats-submissions .ctox-pane', '.ats-submissions [data-ats-form]'],
            minTextLength: 40,
          },
          support: {
            selectors: ['[data-support-root]', '.support-left', '.support-center', '[data-support-toggle-context]'],
            minTextLength: 60,
          },
          threads: {
            selectors: ['[data-threads-root]', '.threads-left', '.threads-center', '[data-thread-search]'],
            minTextLength: 60,
          },
        };
        const collectModuleRenderEvidence = (moduleId) => {
          const contract = moduleRenderContracts[moduleId];
          if (!contract) return { moduleId, ok: true, selectors: [], textLength: 0 };
          const text = document.body?.innerText || '';
          const activeModule = document.body?.dataset?.activeModule || appState?.activeModule?.id || '';
          const permissionDenied = document.querySelector('[data-module-permission-denied="true"]');
          if (activeModule === moduleId && permissionDenied) {
            return {
              moduleId,
              ok: true,
              permissionDenied: true,
              permission: permissionDenied.getAttribute('data-permission') || '',
              collection: permissionDenied.getAttribute('data-collection') || '',
              selectors: [],
              textLength: permissionDenied.innerText?.trim?.().length || 0,
              missing: [],
              errorText: '',
              loadingTextVisible: false,
            };
          }
          const selectorEvidence = contract.selectors.map((selector) => rectEvidence(selector) || { selector, missing: true });
          const missing = selectorEvidence
            .filter((entry) => !entry?.visible || entry.width < 24 || entry.height < 16);
          const errorText = failureText();
          const moduleTextLength = contract.selectors
            .map((selector) => document.querySelector(selector)?.innerText || document.querySelector(selector)?.textContent || '')
            .join('\n')
            .trim()
            .length;
          const loadingTextVisible = /Workspace wird geladen|Loading CTOX runtime|Loading tasks|Documents\s+Workspace wird geladen/i.test(text);
          const ok = missing.length === 0
            && !errorText
            && !loadingTextVisible
            && moduleTextLength >= contract.minTextLength;
          return {
            moduleId,
            ok,
            selectors: selectorEvidence,
            textLength: moduleTextLength,
            missing,
            errorText,
            loadingTextVisible,
          };
        };
        const waitForModuleRendered = (moduleId) => waitFor(() => collectModuleRenderEvidence(moduleId), 30000, `render module ${moduleId}`);
        const waitForElement = async (selector, label, ms = 5000) => waitFor(() => {
          const entry = rectEvidence(selector);
          return {
            ok: Boolean(entry?.visible),
            selector,
            entry,
          };
        }, ms, label || selector);
        const waitForAbsent = async (selector, label, ms = 5000) => waitFor(() => ({
          ok: !document.querySelector(selector),
          selector,
          exists: Boolean(document.querySelector(selector)),
        }), ms, label || `${selector} absent`);
        const runModuleInteraction = async (moduleId) => {
          const evidence = { moduleId, actions: [] };
          const exerciseInput = async (selector, value, label) => {
            const input = document.querySelector(selector);
            if (!input) throw new Error(`${label} input is missing: ${selector}`);
            const before = input.value;
            input.value = value;
            input.dispatchEvent(new Event('input', { bubbles: true }));
            await waitFor(() => ({
              ok: document.querySelector(selector)?.value === value,
              value: document.querySelector(selector)?.value || '',
            }), 5000, label);
            const live = document.querySelector(selector);
            live.value = before;
            live.dispatchEvent(new Event('input', { bubbles: true }));
          };
          if (moduleId === 'ctox') {
            const zoomLabel = document.querySelector('[data-flow-control] span');
            const zoomIn = document.querySelector('[data-flow-control] [data-zoom="+"]');
            const zoomReset = document.querySelector('[data-flow-control] [data-zoom="reset"]');
            if (!zoomLabel || !zoomIn || !zoomReset) {
              throw new Error(`CTOX zoom controls missing: ${JSON.stringify({
                zoomLabel: Boolean(zoomLabel),
                zoomIn: Boolean(zoomIn),
                zoomReset: Boolean(zoomReset),
              })}`);
            }
            const before = zoomLabel.textContent?.trim() || '';
            zoomIn.click();
            const changed = await waitFor(() => {
              const after = document.querySelector('[data-flow-control] span')?.textContent?.trim() || '';
              return { ok: after && after !== before, before, after };
            }, 5000, 'ctox zoom interaction');
            document.querySelector('[data-flow-control] [data-zoom="reset"]')?.click();
            evidence.actions.push(`ctox-zoom:${before}->${changed.after}`);
          } else if (moduleId === 'documents') {
            const newButton = document.querySelector('[data-documents-new-markdown]');
            if (!newButton) throw new Error('Documents new-document action is missing');
            newButton.click();
            await waitForElement('[data-documents-new-form]', 'documents new-document drawer');
            document.querySelector('[data-documents-drawer-close], [data-documents-drawer-cancel]')?.click();
            await waitForAbsent('[data-documents-new-form]', 'documents drawer close');
            evidence.actions.push('documents-new-drawer');
          } else if (moduleId === 'knowledge') {
            const root = document.querySelector('[data-knowledge-root]');
            if (!root) throw new Error('Knowledge root is missing');
            const selectableEvidence = () => {
              const selected = root.querySelector('.knowledge-item[aria-current="true"], .knowledge-bundle[aria-current="true"]');
              const firstItem = root.querySelector('.knowledge-item');
              return {
                ok: Boolean(selected || firstItem),
                selected: selected?.getAttribute('data-knowledge-id') || selected?.getAttribute('data-bundle-id') || '',
                firstItem: firstItem?.getAttribute('data-knowledge-id') || '',
              };
            };
            let selectionReady = selectableEvidence();
            if (!selectionReady.ok) {
              await seedBusinessOsUiRegressionFixtures();
              selectionReady = await waitFor(selectableEvidence, 15000, 'knowledge selectable item');
            }
            if (!root.querySelector('.knowledge-item[aria-current="true"]') && selectionReady.firstItem) {
              root.querySelector('.knowledge-item')?.click();
              await waitFor(() => {
                const selected = root.querySelector('.knowledge-item[aria-current="true"]');
                return {
                  ok: Boolean(selected),
                  selected: selected?.getAttribute('data-knowledge-id') || '',
                };
              }, 5000, 'knowledge selected item after click');
            }
            for (const tab of ['runbooks', 'data', 'skill']) {
              const ready = await waitFor(() => {
                const button = root.querySelector(`[data-tab="${tab}"]`);
                return {
                  ok: Boolean(button && !button.disabled && button.getAttribute('aria-disabled') !== 'true'),
                  tab,
                  exists: Boolean(button),
                  disabled: button?.disabled ?? null,
                  ariaDisabled: button?.getAttribute('aria-disabled') ?? null,
                };
              }, 10000, `knowledge tab ${tab} ready`);
              const button = root.querySelector(`[data-tab="${tab}"]`);
              if (!button || !ready.ok) throw new Error(`Knowledge tab button missing or disabled: ${tab}`);
              button.click();
              await waitFor(() => {
                const panel = root.querySelector(`[data-panel="${tab}"]`);
                const selected = root.querySelector(`[data-tab="${tab}"]`)?.getAttribute('aria-selected') === 'true';
                return {
                  ok: Boolean(panel && !panel.hidden && selected),
                  tab,
                  panelHidden: panel?.hidden ?? null,
                  selected,
                };
              }, 5000, `knowledge tab ${tab}`);
              evidence.actions.push(`knowledge-tab-${tab}`);
            }
          } else if (moduleId === 'research') {
            const newTask = document.querySelector('[data-action="new-task"]');
            if (!newTask) throw new Error('Research new-task action is missing');
            newTask.click();
            await waitForElement('[data-research-task-form]', 'research new-task modal');
            document.querySelector('.research-task-dialog [data-close]')?.click();
            await waitForAbsent('[data-research-task-form]', 'research modal close');
            evidence.actions.push('research-new-task-modal');
          } else if (moduleId === 'matching') {
            const tabMap = document.querySelector('#tabMap');
            const tabList = document.querySelector('#tabList');
            if (!tabMap || !tabList) throw new Error('Matching list/matrix tabs are missing');
            await waitFor(() => {
              const mapWrap = document.querySelector('#mapWrap');
              const requirementList = document.querySelector('#requirementList');
              if (!mapWrap?.classList?.contains('active')) tabMap.click();
              return {
                ok: Boolean(mapWrap?.classList?.contains('active')
                  && tabMap.classList.contains('active')
                  && !tabList.classList.contains('active')
                  && requirementList?.style?.display === 'none'),
                mapActive: Boolean(mapWrap?.classList?.contains('active')),
                tabMapActive: tabMap.classList.contains('active'),
                tabListActive: tabList.classList.contains('active'),
                requirementDisplay: requirementList?.style?.display || '',
              };
            }, 15000, 'matching matrix tab');
            await waitFor(() => {
              const mapWrap = document.querySelector('#mapWrap');
              const requirementList = document.querySelector('#requirementList');
              if (mapWrap?.classList?.contains('active')) tabList.click();
              return {
                ok: Boolean(!mapWrap?.classList?.contains('active')
                  && tabList.classList.contains('active')
                  && !tabMap.classList.contains('active')
                  && requirementList?.style?.display !== 'none'),
                mapActive: Boolean(mapWrap?.classList?.contains('active')),
                tabMapActive: tabMap.classList.contains('active'),
                tabListActive: tabList.classList.contains('active'),
                requirementDisplay: requirementList?.style?.display || '',
              };
            }, 15000, 'matching list tab');
            evidence.actions.push('matching-list-matrix-tabs');
          } else if (moduleId === 'conversations') {
            const whatsapp = document.querySelector('[data-conv-channel-filters] [data-channel="whatsapp"]');
            const all = document.querySelector('[data-conv-channel-filters] [data-channel="all"]');
            if (!whatsapp || !all) throw new Error('Conversations channel filters are missing');
            whatsapp.click();
            await waitFor(() => ({
              ok: whatsapp.classList.contains('is-active') && !all.classList.contains('is-active'),
              whatsappActive: whatsapp.classList.contains('is-active'),
              allActive: all.classList.contains('is-active'),
            }), 5000, 'conversations whatsapp filter');
            all.click();
            await waitFor(() => ({
              ok: all.classList.contains('is-active') && !whatsapp.classList.contains('is-active'),
              whatsappActive: whatsapp.classList.contains('is-active'),
              allActive: all.classList.contains('is-active'),
            }), 5000, 'conversations all filter');
            evidence.actions.push('conversations-channel-filter');
          } else if (moduleId === 'outbound') {
            const toggle = document.querySelector('[data-outbound-root] [data-action="toggle-outreach"]');
            if (!toggle) throw new Error('Outbound outreach view toggle is missing');
            const before = toggle.getAttribute('aria-pressed');
            toggle.click();
            await waitFor(() => {
              const current = document.querySelector('[data-outbound-root] [data-action="toggle-outreach"]');
              const after = current?.getAttribute('aria-pressed') || '';
              return {
                ok: after !== before,
                before,
                after,
              };
            }, 5000, 'outbound outreach view toggle');
            document.querySelector('[data-outbound-root] [data-action="toggle-outreach"]')?.click();
            evidence.actions.push('outbound-outreach-view-toggle');
          } else if (moduleId === 'tickets') {
            const search = document.querySelector('[data-ticket-search]');
            const stateFilter = document.querySelector('[data-ticket-state]');
            if (!search || !stateFilter) throw new Error('Tickets search or state filter controls are missing');
            search.value = 'regression-smoke';
            search.dispatchEvent(new Event('input', { bubbles: true }));
            stateFilter.value = 'open';
            stateFilter.dispatchEvent(new Event('change', { bubbles: true }));
            await waitFor(() => ({
              ok: document.querySelector('[data-ticket-search]')?.value === 'regression-smoke'
                && document.querySelector('[data-ticket-state]')?.value === 'open',
              search: document.querySelector('[data-ticket-search]')?.value || '',
              state: document.querySelector('[data-ticket-state]')?.value || '',
            }), 5000, 'tickets search and state filter');
            search.value = '';
            search.dispatchEvent(new Event('input', { bubbles: true }));
            stateFilter.value = 'all';
            stateFilter.dispatchEvent(new Event('change', { bubbles: true }));
            evidence.actions.push('tickets-search-status-filter');
          } else if (moduleId === 'shiftflow') {
            const scheduler = document.querySelector('#viewSchedulerTabBtn');
            const timesheets = document.querySelector('#viewTimesheetsTabBtn');
            const billing = document.querySelector('#viewBillingTabBtn');
            if (!scheduler || !timesheets || !billing) throw new Error('Shiftflow center tabs are missing');
            timesheets.click();
            await waitFor(() => ({
              ok: timesheets.classList.contains('active')
                && !document.querySelector('#timesheetsView')?.classList?.contains('hidden')
                && document.querySelector('#schedulerView')?.classList?.contains('hidden'),
              title: document.querySelector('#centerPaneTitle')?.textContent?.trim() || '',
            }), 5000, 'shiftflow timesheets tab');
            billing.click();
            await waitFor(() => ({
              ok: billing.classList.contains('active')
                && !document.querySelector('#billingView')?.classList?.contains('hidden')
                && document.querySelector('#timesheetsView')?.classList?.contains('hidden'),
              title: document.querySelector('#centerPaneTitle')?.textContent?.trim() || '',
            }), 5000, 'shiftflow billing tab');
            scheduler.click();
            await waitFor(() => ({
              ok: scheduler.classList.contains('active')
                && !document.querySelector('#schedulerView')?.classList?.contains('hidden')
                && document.querySelector('#billingView')?.classList?.contains('hidden'),
              title: document.querySelector('#centerPaneTitle')?.textContent?.trim() || '',
            }), 5000, 'shiftflow scheduler tab');
            evidence.actions.push('shiftflow-center-tabs');
          } else if (moduleId === 'buchhaltung') {
            for (const navId of ['journal', 'reports', 'skr']) {
              const button = document.querySelector(`[data-nav="${navId}"]`);
              if (!button) throw new Error(`Buchhaltung nav item missing: ${navId}`);
              button.click();
              await waitFor(() => {
                const panel = document.querySelector(`[data-panel="${navId}"]`);
                const title = document.querySelector('[data-active-title]')?.textContent?.trim() || '';
                return {
                  ok: Boolean(panel && !panel.hidden && button.classList.contains('active') && title),
                  navId,
                  panelHidden: panel?.hidden ?? null,
                  active: button.classList.contains('active'),
                  title,
                };
              }, 5000, `buchhaltung nav ${navId}`);
            }
            evidence.actions.push('buchhaltung-nav-switch');
          } else if (moduleId === 'coding-agents') {
            const openSettings = document.querySelector('#open-settings-btn');
            const closeSettings = document.querySelector('#close-settings-btn');
            const modal = document.querySelector('#settings-modal');
            if (!openSettings || !closeSettings || !modal) {
              throw new Error(`Coding Agents settings modal controls missing: ${JSON.stringify({
                openSettings: Boolean(openSettings),
                closeSettings: Boolean(closeSettings),
                modal: Boolean(modal),
              })}`);
            }
            openSettings.click();
            await waitFor(() => ({
              ok: !document.querySelector('#settings-modal')?.hasAttribute('hidden'),
              hidden: document.querySelector('#settings-modal')?.hasAttribute('hidden') ?? null,
            }), 5000, 'coding agents settings open');
            closeSettings.click();
            await waitFor(() => ({
              ok: document.querySelector('#settings-modal')?.hasAttribute('hidden') === true,
              hidden: document.querySelector('#settings-modal')?.hasAttribute('hidden') ?? null,
            }), 5000, 'coding agents settings close');
            evidence.actions.push('coding-agents-settings-modal');
          } else if (moduleId === 'app-store') {
            const listView = document.querySelector('[data-view="list"]');
            const gridView = document.querySelector('[data-view="grid"]');
            const installedScope = document.querySelector('[data-scope="installed"]');
            const marketplaceScope = document.querySelector('[data-scope="marketplace"]');
            if (!listView || !gridView || !installedScope || !marketplaceScope) {
              throw new Error('App Store view or scope controls are missing');
            }
            listView.click();
            await waitFor(() => ({
              ok: listView.classList.contains('is-active') && !gridView.classList.contains('is-active'),
              listActive: listView.classList.contains('is-active'),
              gridActive: gridView.classList.contains('is-active'),
            }), 5000, 'app-store list view');
            installedScope.click();
            await waitFor(() => ({
              ok: installedScope.classList.contains('active') && !marketplaceScope.classList.contains('active'),
              installedActive: installedScope.classList.contains('active'),
              marketplaceActive: marketplaceScope.classList.contains('active'),
            }), 5000, 'app-store installed scope');
            gridView.click();
            marketplaceScope.click();
            evidence.actions.push('app-store-view-scope');
          } else if (moduleId === 'browser') {
            const address = document.querySelector('[data-browser-address]');
            const refresh = document.querySelector('[data-browser-refresh]');
            if (!address || !refresh) throw new Error('Browser address or refresh controls are missing');
            address.value = 'https://example.com';
            address.dispatchEvent(new Event('input', { bubbles: true }));
            refresh.click();
            await waitFor(() => ({
              ok: document.querySelector('[data-browser-address]')?.value === 'https://example.com'
                && Boolean(document.querySelector('[data-browser-status-chip]')?.textContent?.trim()),
              address: document.querySelector('[data-browser-address]')?.value || '',
              status: document.querySelector('[data-browser-status-chip]')?.textContent?.trim() || '',
            }), 5000, 'browser address input');
            evidence.actions.push('browser-address-refresh');
          } else if (moduleId === 'calendar') {
            const newEvent = document.querySelector('#btnNewEvent');
            const closeDrawer = document.querySelector('#closeDrawerBtn');
            if (!newEvent || !closeDrawer) throw new Error('Calendar new-event drawer controls are missing');
            newEvent.click();
            await waitFor(() => ({
              ok: document.querySelector('#calendarInspectorDrawer')?.classList?.contains('open')
                || document.querySelector('#calendarInspectorDrawer')?.classList?.contains('is-open')
                || document.querySelector('#calendarInspectorDrawer')?.getAttribute('aria-hidden') === 'false'
                || Boolean(document.querySelector('#calendarInspectorDrawer form')),
              drawerClass: document.querySelector('#calendarInspectorDrawer')?.className || '',
              hasForm: Boolean(document.querySelector('#calendarInspectorDrawer form')),
            }), 5000, 'calendar new-event drawer');
            document.querySelector('#closeDrawerBtn')?.click();
            await delay(100);
            evidence.actions.push('calendar-new-event-drawer');
          } else if (moduleId === 'creator') {
            const trigger = document.querySelector('#expert-accordion-btn');
            const content = document.querySelector('#expert-accordion-content');
            if (!trigger || !content) throw new Error('Creator expert accordion controls are missing');
            const beforeCollapsed = content.classList.contains('is-collapsed');
            trigger.click();
            await waitFor(() => ({
              ok: content.classList.contains('is-collapsed') !== beforeCollapsed,
              beforeCollapsed,
              afterCollapsed: content.classList.contains('is-collapsed'),
            }), 5000, 'creator accordion toggle');
            trigger.click();
            evidence.actions.push('creator-expert-accordion');
          } else if (moduleId === 'notes') {
            const favorites = document.querySelector('[data-nav-category="favorites"]');
            const notes = document.querySelector('[data-nav-category="notes"]');
            const filter = document.querySelector('[data-action="toggle-filter"]');
            if (!favorites || !notes || !filter) throw new Error('Notes navigation/filter controls are missing');
            favorites.click();
            await waitFor(() => ({
              ok: favorites.classList.contains('active') && !notes.classList.contains('active'),
              favoritesActive: favorites.classList.contains('active'),
              notesActive: notes.classList.contains('active'),
            }), 5000, 'notes favorites nav');
            notes.click();
            filter.click();
            await delay(100);
            evidence.actions.push('notes-nav-filter');
          } else if (moduleId === 'reports') {
            const kind = document.querySelector('[data-report-kind]');
            const status = document.querySelector('[data-report-status]');
            if (!kind || !status) throw new Error('Reports filter controls are missing');
            kind.value = 'bug';
            kind.dispatchEvent(new Event('change', { bubbles: true }));
            await waitFor(() => ({
              ok: document.querySelector('[data-report-kind]')?.value === 'bug',
              kind: document.querySelector('[data-report-kind]')?.value || '',
            }), 5000, 'reports kind filter');
            status.value = 'open';
            status.dispatchEvent(new Event('change', { bubbles: true }));
            kind.value = 'all';
            kind.dispatchEvent(new Event('change', { bubbles: true }));
            status.value = 'all';
            status.dispatchEvent(new Event('change', { bubbles: true }));
            evidence.actions.push('reports-filter-controls');
          } else if (moduleId === 'spreadsheets') {
            const search = document.querySelector('[data-spreadsheets-search]');
            if (!search) throw new Error('Spreadsheets search control is missing');
            search.value = 'regression-smoke';
            search.dispatchEvent(new Event('input', { bubbles: true }));
            await waitFor(() => ({
              ok: document.querySelector('[data-spreadsheets-search]')?.value === 'regression-smoke',
              value: document.querySelector('[data-spreadsheets-search]')?.value || '',
            }), 5000, 'spreadsheets search input');
            document.querySelector('[data-spreadsheets-search]').value = '';
            document.querySelector('[data-spreadsheets-search]').dispatchEvent(new Event('input', { bubbles: true }));
            evidence.actions.push('spreadsheets-search-filter');
          } else if (moduleId === 'appsec-pentest') {
            const findings = document.querySelector('[data-appsec-tab="findings"]');
            const coverage = document.querySelector('[data-appsec-tab="coverage"]');
            if (!findings || !coverage) throw new Error('AppSec coverage/findings tabs are missing');
            findings.click();
            await waitFor(() => ({
              ok: document.querySelector('[data-appsec-tab="findings"]')?.getAttribute('aria-selected') === 'true',
            }), 5000, 'appsec findings tab');
            coverage.click();
            evidence.actions.push('appsec-coverage-findings-tabs');
          } else if (['consent', 'esign', 'intake', 'interviews', 'nachweise', 'placements', 'submissions'].includes(moduleId)) {
            await exerciseInput('[data-ats-form] input:not([type="hidden"])', 'ui-regression-smoke', `${moduleId} primary form`);
            evidence.actions.push(`${moduleId}-primary-form-input`);
          } else if (moduleId === 'credentials') {
            await exerciseInput('[data-add-key]', 'UI_REGRESSION_SMOKE', 'credentials custom key');
            evidence.actions.push('credentials-write-only-form-input');
          } else if (moduleId === 'customers') {
            await exerciseInput('[data-customers-search]', 'ui-regression-smoke', 'customers search');
            evidence.actions.push('customers-search-filter');
          } else if (moduleId === 'cv-print-builder') {
            await exerciseInput('[data-cv-search]', 'ui-regression-smoke', 'cv print search');
            evidence.actions.push('cv-print-search-filter');
          } else if (moduleId === 'buchhaltung') {
            const journal = document.querySelector('[data-fibu-nav] [data-nav="journal"]');
            const skr = document.querySelector('[data-fibu-nav] [data-nav="skr"]');
            if (!journal || !skr) throw new Error('Buchhaltung navigation controls are missing');
            journal.click();
            await waitFor(() => ({
              ok: document.querySelector('[data-panel="journal"]')?.hidden === false,
            }), 5000, 'buchhaltung journal panel');
            skr.click();
            evidence.actions.push('buchhaltung-journal-navigation');
          } else if (moduleId === 'iot') {
            const newAsset = document.querySelector('[data-iot-left] [data-act="new-asset"]');
            if (!newAsset) throw new Error('IoT new-asset action is missing');
            newAsset.click();
            await waitForElement('[data-iot-left] [data-form="create"]', 'iot create asset form');
            document.querySelector('[data-iot-left] [data-act="cancel-create"]')?.click();
            evidence.actions.push('iot-create-asset-form');
          } else if (moduleId === 'support') {
            await exerciseInput('[data-support-search]', 'ui-regression-smoke', 'support search');
            const contextToggle = document.querySelector('.support-center [data-support-toggle-context]');
            if (!contextToggle) throw new Error('Support context navigation is missing');
            contextToggle.click();
            await waitFor(() => ({
              ok: Boolean(rectEvidence('.support-right')?.visible),
              context: rectEvidence('.support-right'),
            }), 5000, 'support context open');
            document.querySelector('.support-right [data-support-toggle-context]')?.click();
            await waitFor(() => ({
              ok: Boolean(rectEvidence('.support-center')?.visible),
              center: rectEvidence('.support-center'),
            }), 5000, 'support conversation return');
            evidence.actions.push('support-search-and-context-navigation');
          } else if (moduleId === 'threads') {
            await exerciseInput('[data-thread-search]', 'ui-regression-smoke', 'threads search');
            const all = document.querySelector('[data-threads-root] [data-filter="all"]');
            if (!all) throw new Error('Threads all-filter is missing');
            all.click();
            evidence.actions.push('threads-search-and-filter');
          }
          return evidence.actions.length ? evidence : null;
        };
        const seedBusinessOsUiRegressionFixtures = async () => {
          const rawDb = appState?.db?.raw;
          if (!rawDb?.knowledge_items || !rawDb?.knowledge_runbooks || !rawDb?.knowledge_tables) return;
          const now = Date.now();
          const upsert = async (collection, document) => {
            if (collection.incrementalUpsert) {
              await collection.incrementalUpsert(document);
              return;
            }
            if (collection.upsert) {
              await collection.upsert(document);
              return;
            }
            try {
              await collection.insert(document);
            } catch (error) {
              throw new Error(`failed to seed Business OS UI regression fixture ${document.id}: ${error?.message || String(error)}`);
            }
          };
          await upsert(rawDb.knowledge_items, {
            id: 'ui_regression_skillbook',
            kind: 'skillbook',
            title: 'UI Regression Skillbook',
            subtitle: 'Business OS smoke fixture',
            summary: 'Deterministic fixture for Business OS UI regression tab coverage.',
            source_path: 'ui-regression',
            linked_runbook_ids: ['ui_regression_runbook'],
            updated_at: new Date(now).toISOString(),
            updated_at_ms: now,
          });
          await upsert(rawDb.knowledge_items, {
            id: 'ui_regression_skill',
            kind: 'skill',
            title: 'UI Regression Skill',
            subtitle: 'Business OS smoke fixture',
            summary: 'Allows deterministic Knowledge tab interactions in the UI regression harness.',
            source_path: 'ui-regression',
            skillbook_id: 'ui_regression_skillbook',
            linked_runbook_ids: ['ui_regression_runbook'],
            updated_at: new Date(now).toISOString(),
            updated_at_ms: now,
          });
          await upsert(rawDb.knowledge_runbooks, {
            id: 'ui_regression_runbook',
            kind: 'runbook',
            title: 'UI Regression Runbook',
            subtitle: 'Business OS smoke fixture',
            summary: 'Runbook fixture for tab coverage.',
            prompt: 'Verify the Knowledge module tab surface.',
            source_path: 'ui-regression',
            updated_at: new Date(now).toISOString(),
            updated_at_ms: now,
          });
          await upsert(rawDb.knowledge_tables, {
            id: 'ui_regression_table',
            kind: 'dataframe',
            title: 'UI Regression Data',
            subtitle: 'Business OS smoke fixture',
            summary: 'Table fixture for tab coverage.',
            row_count: 1,
            source_path: 'ui-regression',
            updated_at: new Date(now).toISOString(),
            updated_at_ms: now,
          });
        };
        const openAndVerifyModule = async (moduleId, opener, label) => {
          const opened = await opener();
          const renderEvidence = await waitForModuleRendered(moduleId);
          const interactionEvidence = renderEvidence.permissionDenied
            ? { moduleId, actions: ['policy-denied-render-skipped'], permissionDenied: true }
            : await runModuleInteraction(moduleId);
          return {
            ...opened,
            expectedModuleId: moduleId,
            renderEvidence,
            interactionEvidence,
            label,
          };
        };
        const collectVisualEvidence = () => {
          const workspace = rectEvidence('.workspace-frame');
          const moduleHost = rectEvidence('[data-module-host]');
          const startButton = rectEvidence('[data-shell-start]');
          const desktopIconCount = document.querySelectorAll('.desktop-icon').length;
          const desktopSurface = rectEvidence('[data-module-root], .desktop-root, [data-desktop-icons]');
          const bodyText = document.body?.innerText || '';
          const loadingTextVisible = /Loading workspace|waiting for module manifests|App-Start fehlgeschlagen|Module startup failed|System-Start fehlgeschlagen/i.test(bodyText);
          const evidence = {
            workspace,
            moduleHost,
            startButton,
            desktopSurface,
            desktopIconCount,
            bodyLoading: Boolean(document.body?.dataset?.moduleLoading),
            activeModule: document.body?.dataset?.activeModule || appState?.activeModule?.id || '',
            loadingTextVisible,
          };
          const problems = [];
          if (!workspace?.visible || workspace.width < 700 || workspace.height < 450) {
            problems.push(`workspace frame not visible enough: ${JSON.stringify(workspace)}`);
          }
          if (!moduleHost?.visible || moduleHost.width < 500 || moduleHost.height < 350) {
            problems.push(`module host not visible enough: ${JSON.stringify(moduleHost)}`);
          }
          if (!startButton?.visible) {
            problems.push(`start button not visible: ${JSON.stringify(startButton)}`);
          }
          if (!desktopSurface?.visible || desktopSurface.width < 400 || desktopSurface.height < 300) {
            problems.push(`desktop surface not visible enough: ${JSON.stringify(desktopSurface)}`);
          }
          if (desktopIconCount < 6) {
            problems.push(`desktop icon count too low: ${desktopIconCount}`);
          }
          if (evidence.bodyLoading || loadingTextVisible) {
            problems.push(`Business OS still shows loading or failure state: ${JSON.stringify({ bodyLoading: evidence.bodyLoading, loadingTextVisible })}`);
          }
          if (problems.length) {
            throw new Error(`Business OS visual layout evidence failed: ${problems.join('; ')}`);
          }
          return evidence;
        };

        await seedBusinessOsUiRegressionFixtures();
        const startMenu = await openStartMenu();
        if (startMenu.itemCount < 8) {
          throw new Error(`Business OS start menu rendered too few launch targets: ${JSON.stringify(startMenu)}`);
        }
        const openedModules = [];
        const ctoxStartItem = [...document.querySelectorAll('.shell-start-menu-panel .start-menu-item')]
          .find((item) => item.dataset?.target === 'ctox'
            || item.dataset?.moduleId === 'ctox'
            || /^CTOX\b/i.test(item.querySelector('.start-menu-item-label')?.textContent?.trim() || item.textContent?.trim() || ''));
        if (!ctoxStartItem) throw new Error('CTOX start-menu launch target is missing');
        ctoxStartItem.click();
        const ctoxWindowEvidence = await openAndVerifyModule(
          'ctox',
          () => waitFor(() => {
            const windowEntry = appState.windowManager?.listWindows?.()
              .find((entry) => entry.ownerId === 'desktop-app:ctox');
            const activeModule = document.body?.dataset?.activeModule || appState?.activeModule?.id || '';
            return {
              ok: Boolean(windowEntry) || activeModule === 'ctox',
              activeModule,
              windowId: windowEntry?.id || '',
            };
          }, 10000, 'open CTOX from start menu'),
          'start-menu',
        );
        openedModules.push(ctoxWindowEvidence);
        const ctoxWindowId = appState.windowManager?.listWindows?.()
          .find((entry) => entry.ownerId === 'desktop-app:ctox')?.id;
        if (ctoxWindowId) appState.windowManager.destroy(ctoxWindowId);
        for (const moduleId of requiredModules) {
          if (moduleId === 'ctox') continue;
          openedModules.push(await openAndVerifyModule(
            moduleId,
            () => openModuleByHash(moduleId),
            'hash',
          ));
        }
        const secondaryOpenedModules = [];
        for (const moduleId of secondaryModules) {
          secondaryOpenedModules.push(await openAndVerifyModule(
            moduleId,
            () => openModuleByHash(moduleId),
            'secondary-hash',
          ));
        }
        await openStartMenu();
        document.querySelector('.shell-start-menu-panel .show-desktop-btn')?.click();
        const desktop = await waitFor(() => {
          const activeModule = document.body?.dataset?.activeModule || appState?.activeModule?.id || '';
          const loading = Boolean(document.body?.dataset?.moduleLoading);
          const errorText = failureText();
          return {
            ok: activeModule === 'desktop' && !loading && !errorText,
            activeModule,
            loading,
            errorText,
          };
        }, 10000, 'show desktop');
        const visualEvidence = collectVisualEvidence();
        const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections: ['business_module_catalog', 'ctox_runtime_settings', 'desktop_files', 'desktop_file_chunks'],
        });
        if (status?.version !== 'business-os-advanced-status-v1') {
          throw new Error(`Business OS UI regression smoke lost advanced status evidence: ${JSON.stringify(status)}`);
        }
        return {
          mode: smokeMode,
          moduleCount: moduleIds.length,
          moduleIds,
          startMenuItemCount: startMenu.itemCount,
          openedModules,
          secondaryOpenedModules,
          desktopOpened: desktop.activeModule === 'desktop',
          activeModule: desktop.activeModule,
          visualEvidence,
          advancedStatusVersion: status.version || '',
          advancedStatusRuntime: status.rxdbRuntime || null,
        };
      }

      async function runBusinessOsRolesPermissionsUiSmoke() {
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            last = await predicate();
            if (last?.ok) return last;
            await delay(100);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/["\\]/g, '\\$&');
        };
        const smoke = globalThis.ctoxBusinessOsSmoke;
        const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state || appState;
        if (!state) throw new Error('Business OS app state is unavailable for roles/permissions UI smoke');
        if (typeof smoke?.openSettingsDrawer !== 'function') {
          throw new Error('Business OS smoke API does not expose openSettingsDrawer for roles/permissions UI smoke');
        }
        appState = state;
        const rawDb = state?.db?.raw;
        if (!rawDb?.business_commands) {
          throw new Error('business_commands collection is unavailable for roles/permissions UI smoke');
        }
        const [
          permissionsMod,
          shellPermissionsUiMod,
          rolesMod,
        ] = await Promise.all([
          import('/shared/permissions.js'),
          import('/shared/shell-permissions-ui.js'),
          import('/shared/roles.js'),
        ]);
        const {
          BusinessOsPermissions,
          canModifyBusinessModule,
          canViewBusinessModuleSource,
        } = permissionsMod;
        const {
          buildGlobalCtoxContextModes,
          buildModuleTargetContextItems,
          renderGlobalCtoxContextModeHtml,
          shouldRenderModuleSourceAction,
        } = shellPermissionsUiMod;
        const {
          assignableRolesForActor,
          roleDisplayName,
        } = rolesMod;
        const moduleCatalog = await waitFor(() => {
          const modules = Array.isArray(state.modules) ? state.modules.filter((mod) => mod?.id) : [];
          const targetModule = modules.find((mod) => mod.id === 'app-store') || modules.find((mod) => mod.id !== 'desktop') || null;
          const otherModule = modules.find((mod) => mod.id && mod.id !== targetModule?.id && mod.id !== 'desktop') || null;
          return {
            ok: Boolean(targetModule && otherModule),
            moduleIds: modules.map((mod) => mod.id),
            targetModule,
            otherModule,
          };
        }, 15000, 'roles/permissions module catalog');
        const targetModule = moduleCatalog.targetModule;
        const otherModule = moduleCatalog.otherModule;
        const allPermissions = Object.values(BusinessOsPermissions);
        const governance = {
          founders: {
            [targetModule.id]: [{ user_id: 'founder_ui', active: true }],
          },
          releases: {
            [targetModule.id]: [{
              version_id: 'roles-permissions-release-smoke',
              version: 1,
              status: 'released',
            }],
          },
          permission_model: {
            version: 1,
            deny_supported: false,
            role_defaults: {
              chef: {
                workspace: allPermissions,
                module: allPermissions,
                assigned_module: allPermissions,
              },
              admin: {
                workspace: allPermissions,
                module: allPermissions,
                assigned_module: allPermissions,
              },
              founder: {
                workspace: [],
                module: [],
                assigned_module: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                  BusinessOsPermissions.DataRead,
                  BusinessOsPermissions.DataWrite,
                ],
              },
              user: {
                workspace: [],
                module: [],
                assigned_module: [],
              },
            },
            module_assignments: {
              [targetModule.id]: {
                founder_ui: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                ],
              },
            },
            explicit_grants: [
              {
                grant_id: 'ui_source_target',
                subject_type: 'user',
                subject_id: 'source_viewer',
                permission: BusinessOsPermissions.AppsSourceView,
                scope_type: 'module',
                scope_id: targetModule.id,
                active: true,
              },
              {
                grant_id: 'ui_modify_target',
                subject_type: 'user',
                subject_id: 'app_modifier',
                permission: BusinessOsPermissions.AppsModify,
                scope_type: 'module',
                scope_id: targetModule.id,
                active: true,
              },
            ],
          },
        };
        const labels = {
          openApp: 'Öffnen',
          pinToTaskbar: 'An Bar anheften',
          unpinFromTaskbar: 'Von Bar lösen',
          openSource: 'Source öffnen',
          modifyApp: 'App ändern',
          workData: 'Mit Daten arbeiten',
          answer: 'Frage beantworten',
        };
        const sessionFor = (id, role) => ({ user: { id, role } });
        const contextLabelsFor = (module, session) => {
          const canModify = canModifyBusinessModule(module, { session, governance });
          const canOpenSource = canViewBusinessModuleSource(module, { session, governance });
          return {
            canModify,
            canOpenSource,
            sourceAction: shouldRenderModuleSourceAction({ module, canOpenSource }),
            labels: buildModuleTargetContextItems({
              target: {
                id: module.id,
                kind: 'module',
                title: module.title || module.id,
                glyph: module.glyph || '□',
                module,
              },
              pinned: false,
              canModify,
              canOpenSource,
              labels,
            })
              .filter((item) => item.label)
              .map((item) => item.label),
          };
        };
        const labelsContain = (items, label) => items.includes(label);
        const labelsAreBusinessFacing = (items) => items.every((label) => (
          !/App modifizieren|Modul bearbeiten|Founder/i.test(label)
        ));
        let currentSmokeSession = null;
        const applySmokeActorState = () => {
          if (!currentSmokeSession) return;
          state.session = currentSmokeSession;
          state.governance = governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = currentSmokeSession;
        };
        const applyActorAndOpenTarget = async (session) => {
          currentSmokeSession = session;
          const applyActorState = () => {
            applySmokeActorState();
          };
          applyActorState();
          await state.openModule?.(targetModule.id, { force: true, asModule: true });
          applyActorState();
          await waitFor(() => {
            const activeModule = document.body?.dataset?.activeModule || state.activeModule?.id || '';
            const loading = Boolean(document.body?.dataset?.moduleLoading);
            return {
              ok: activeModule === targetModule.id && !loading,
              activeModule,
              loading,
            };
          }, 30000, `open ${targetModule.id} for actor ${session.user.id}`);
          await delay(100);
        };
        const appbarSourceVisible = () => Boolean(document.querySelector(`[data-module-source="${css(targetModule.id)}"]`));
        const openShellContextMenu = async (label, expectedLabels = ['Öffnen']) => {
          state.contextMenu?.hide?.();
          await waitFor(() => {
            const menuCount = document.querySelectorAll('.shell-context-menu').length;
            return { ok: menuCount === 0, menuCount };
          }, 3000, `clear shell context menu before ${label}`);
          applySmokeActorState();
          const tab = document.querySelector(`.module-tab[data-module="${css(targetModule.id)}"], .module-tab[data-target="${css(targetModule.id)}"]`);
          if (!tab) {
            throw new Error(`Business OS shell tab for ${targetModule.id} is missing during ${label}`);
          }
          const rect = tab.getBoundingClientRect();
          tab.dispatchEvent(new MouseEvent('contextmenu', {
            bubbles: true,
            cancelable: true,
            button: 2,
            clientX: Math.max(20, Math.round(rect.left + 8)),
            clientY: Math.max(20, Math.round(rect.top + 8)),
          }));
          return waitFor(() => {
            const menus = [...document.querySelectorAll('.shell-context-menu')];
            const menu = menus[menus.length - 1] || null;
            const itemLabels = menu
              ? [...menu.querySelectorAll('.shell-context-menu-label')].map((item) => item.textContent?.trim() || '').filter(Boolean)
              : [];
            return {
              ok: expectedLabels.every((expectedLabel) => itemLabels.includes(expectedLabel)),
              labels: itemLabels,
              expectedLabels,
            };
          }, 5000, `shell context menu for ${label}`);
        };
        const teamSession = sessionFor('team_member', 'user');
        const sourceSession = sessionFor('source_viewer', 'user');
        const modifySession = sessionFor('app_modifier', 'user');
        const ownerSession = sessionFor('owner_ui', 'chef');

        await applyActorAndOpenTarget(teamSession);
        const teamDomContext = await openShellContextMenu('team member', ['Öffnen']);
        const teamAppbarSourceVisible = appbarSourceVisible();
        const teamHelper = contextLabelsFor(targetModule, teamSession);

        await applyActorAndOpenTarget(sourceSession);
        const sourceDomContext = await openShellContextMenu('source-view grant', ['Öffnen', 'Source öffnen']);
        const sourceAppbarSourceVisible = appbarSourceVisible();
        const sourceHelper = contextLabelsFor(targetModule, sourceSession);

        await applyActorAndOpenTarget(modifySession);
        const modifyDomContext = await openShellContextMenu('modify grant', ['Öffnen', 'App ändern']);
        const modifyAppbarSourceVisible = appbarSourceVisible();
        const modifyHelper = contextLabelsFor(targetModule, modifySession);

        await applyActorAndOpenTarget(ownerSession);
        const ownerDomContext = await openShellContextMenu('owner', ['Öffnen', 'Source öffnen', 'App ändern']);
        const ownerHelper = contextLabelsFor(targetModule, ownerSession);

        const sourceOtherHelper = contextLabelsFor(otherModule, sourceSession);
        const modifyOtherHelper = contextLabelsFor(otherModule, modifySession);
        const ownerAssignable = assignableRolesForActor('chef');
        const adminAssignable = assignableRolesForActor('admin');
        const deniedGlobalModes = buildGlobalCtoxContextModes({ canModify: false, labels });
        const allowedGlobalModes = buildGlobalCtoxContextModes({ canModify: true, labels });
        const deniedGlobalModeHtml = renderGlobalCtoxContextModeHtml({ canModify: false, labels });
        const allowedGlobalModeHtml = renderGlobalCtoxContextModeHtml({ canModify: true, labels });
        const deniedGlobalModeValues = deniedGlobalModes.map((mode) => mode.value);
        const allowedGlobalModeValues = allowedGlobalModes.map((mode) => mode.value);
        const deniedGlobalModeLabels = deniedGlobalModes.map((mode) => mode.label);
        const allowedGlobalModeLabels = allowedGlobalModes.map((mode) => mode.label);
        const deniedAppMode = deniedGlobalModes.find((mode) => mode.value === 'app') || null;
        const allowedAppMode = allowedGlobalModes.find((mode) => mode.value === 'app') || null;
        const allContextLabels = [
          ...teamDomContext.labels,
          ...sourceDomContext.labels,
          ...modifyDomContext.labels,
          ...ownerDomContext.labels,
          ...teamHelper.labels,
          ...sourceHelper.labels,
          ...modifyHelper.labels,
          ...ownerHelper.labels,
        ];
        const teamModifyHidden = !labelsContain(teamDomContext.labels, 'App ändern')
          && !labelsContain(teamHelper.labels, 'App ändern')
          && !teamHelper.canModify;
        const teamSourceHidden = !labelsContain(teamDomContext.labels, 'Source öffnen')
          && !labelsContain(teamHelper.labels, 'Source öffnen')
          && !teamAppbarSourceVisible
          && !teamHelper.canOpenSource;
        const sourceGrantVisible = labelsContain(sourceDomContext.labels, 'Source öffnen')
          && labelsContain(sourceHelper.labels, 'Source öffnen')
          && !labelsContain(sourceDomContext.labels, 'App ändern')
          && !sourceHelper.canModify
          && sourceHelper.canOpenSource;
        const modifyGrantVisible = labelsContain(modifyDomContext.labels, 'App ändern')
          && labelsContain(modifyHelper.labels, 'App ändern')
          && modifyHelper.canModify;
        const ownerContextVisible = labelsContain(ownerDomContext.labels, 'App ändern')
          && labelsContain(ownerDomContext.labels, 'Source öffnen')
          && ownerHelper.canModify
          && ownerHelper.canOpenSource;
        const appbarSourceGate = !teamAppbarSourceVisible
          && sourceAppbarSourceVisible
          && !modifyAppbarSourceVisible;
        const exactScopeIsolated = sourceOtherHelper.canOpenSource === false
          && modifyOtherHelper.canModify === false
          && !labelsContain(sourceOtherHelper.labels, 'Source öffnen')
          && !labelsContain(modifyOtherHelper.labels, 'App ändern');
        const ownerRoleOption = ownerAssignable.includes('chef');
        const adminOwnerOptionHidden = !adminAssignable.includes('chef');
        const businessLabels = roleDisplayName('chef') === 'Owner'
          && roleDisplayName('admin') === 'Admin'
          && roleDisplayName('founder') === 'App-Verantwortliche:r'
          && roleDisplayName('team') === 'Teammitglied'
          && labelsAreBusinessFacing(allContextLabels)
          && deniedGlobalModeValues.join(',') === 'data,ask,app'
          && allowedGlobalModeValues.join(',') === 'data,ask,app'
          && deniedAppMode?.approvalRequired === true
          && allowedAppMode?.approvalRequired === false
          && deniedGlobalModeLabels.includes('App ändern')
          && allowedGlobalModeValues.includes('app')
          && allowedGlobalModeLabels.includes('App ändern')
          && !/App modifizieren|Modul bearbeiten|Founder/i.test(deniedGlobalModeHtml)
          && !/App modifizieren|Modul bearbeiten|Founder/i.test(allowedGlobalModeHtml);
        applySmokeActorState();
        await smoke.openSettingsDrawer({ initialTab: 'admin' });
        const settingsFallback = await waitFor(() => {
          const drawer = document.querySelector('.settings-drawer');
          const rowText = drawer?.innerText || '';
          const diagnostics = drawer?.querySelector(`[data-module-release-diagnostics="${css(targetModule.id)}"]`) || null;
          const buttons = diagnostics
            ? [...diagnostics.querySelectorAll('button')].map((button) => ({
              text: button.textContent?.trim() || '',
              disabled: button.disabled === true || button.getAttribute('aria-disabled') === 'true',
            }))
            : [];
          const releaseButton = buttons.find((button) => /Freigabe im App Store/.test(button.text));
          const rollbackButton = buttons.find((button) => /Rollback nur Diagnose/.test(button.text));
          const rollbackSelect = diagnostics?.querySelector('select[disabled][aria-label="Rollback-Versionen nur Diagnose"]') || null;
          const activeControls = drawer
            ? drawer.querySelectorAll('[data-module-release], [data-module-rollback], [data-rollback-version]').length
            : -1;
          return {
            ok: Boolean(
              diagnostics
                && releaseButton?.disabled
                && rollbackButton?.disabled
                && rollbackSelect
                && activeControls === 0
                && /Settings zeigt Release und Rollback nur als Diagnose/.test(rowText)
            ),
            hasDrawer: Boolean(drawer),
            hasDiagnostics: Boolean(diagnostics),
            buttons,
            hasRollbackSelect: Boolean(rollbackSelect),
            activeControls,
            text: rowText.slice(0, 800),
          };
        }, 10000, 'settings read-only release fallback');
        const settingsReleaseFallbackReadOnly = settingsFallback.ok === true;
        const whyButton = document.querySelector(`.settings-drawer [data-module-why="${css(targetModule.id)}"]`);
        if (!whyButton) {
          throw new Error(`Settings why diagnostics button missing for ${targetModule.id}`);
        }
        whyButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
        const settingsWhyDiagnostics = await waitFor(() => {
          const drawer = document.querySelector('.settings-drawer');
          const root = drawer?.querySelector(`[data-why-diagnostics="${css(targetModule.id)}"]`) || null;
          const statusText = drawer?.querySelector('.settings-status')?.textContent?.trim() || '';
          const whyStatusText = drawer?.querySelector('[data-module-why-status]')?.textContent?.trim() || '';
          const button = drawer?.querySelector(`[data-module-why="${css(targetModule.id)}"]`) || null;
          const rows = root ? [...root.querySelectorAll('[data-why-row]')].map((node) => ({
            key: node.getAttribute('data-why-row') || '',
            state: node.getAttribute('data-decision-state') || '',
            text: node.textContent?.trim().replace(/\s+/g, ' ') || '',
          })) : [];
          const dataRows = root ? [...root.querySelectorAll('[data-why-data-row]')].map((node) => ({
            collection: node.getAttribute('data-why-data-row') || '',
            text: node.textContent?.trim().replace(/\s+/g, ' ') || '',
          })) : [];
          const rowKeys = rows.map((row) => row.key);
          const text = root?.textContent?.trim().replace(/\s+/g, ' ') || '';
          const redacted = !/(policy_decision|collection_decision|module_decision|reason_code|role_or_scope_denied|apps\.modify|Allowed\.|This role is not allowed|DO_NOT_LEAK|prompt|selected_text|token)/i.test(text);
          return {
            ok: Boolean(
              root
                && ['actor', 'visibility', 'open', 'modify', 'source', 'release', 'rollback', 'data']
                  .every((key) => rowKeys.includes(key))
                && redacted
                && /Warum\?/.test(text)
            ),
            visible: Boolean(root),
            rowKeys,
            rows,
            dataRows,
            redacted,
            statusText,
            whyStatusText,
            hasButton: Boolean(button),
            buttonDisabled: button?.disabled === true,
            text: text.slice(0, 1000),
          };
        }, 30000, 'settings why diagnostics command render');
        const settingsWhyDiagnosticsVisible = settingsWhyDiagnostics.visible === true;
        const settingsWhyDiagnosticsRows = Array.isArray(settingsWhyDiagnostics.rowKeys)
          && ['actor', 'visibility', 'open', 'modify', 'source', 'release', 'rollback', 'data']
            .every((key) => settingsWhyDiagnostics.rowKeys.includes(key));
        const settingsWhyDiagnosticsRedacted = settingsWhyDiagnostics.redacted === true;
        const supportButton = document.querySelector(`.settings-drawer [data-module-support-diagnostics="${css(targetModule.id)}"]`);
        if (!supportButton) {
          throw new Error(`Settings support diagnostics button missing for ${targetModule.id}`);
        }
        supportButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
        const settingsSupportDiagnostics = await waitFor(async () => {
          const drawer = document.querySelector('.settings-drawer');
          const root = drawer?.querySelector(`[data-support-diagnostics="${css(targetModule.id)}"]`) || null;
          const statusText = drawer?.querySelector('.settings-status')?.textContent?.trim() || '';
          const supportStatusText = drawer?.querySelector('[data-module-support-status]')?.textContent?.trim() || '';
          const button = drawer?.querySelector(`[data-module-support-diagnostics="${css(targetModule.id)}"]`) || null;
          const rows = root ? [...root.querySelectorAll('[data-support-row]')].map((node) => ({
            key: node.getAttribute('data-support-row') || '',
            text: node.textContent?.trim().replace(/\s+/g, ' ') || '',
          })) : [];
          const rowKeys = rows.map((row) => row.key);
          const text = root?.textContent?.trim().replace(/\s+/g, ' ') || '';
          const download = root?.querySelector(`[data-support-diagnostics-download="${css(targetModule.id)}"]`) || null;
          const commandDocs = (await rawDb.business_commands.find().exec())
            .map((doc) => doc.toJSON?.() || doc)
            .filter((doc) => doc.command_type === 'ctox.business_os.support.export_diagnostics'
              && doc.record_id === `support:${targetModule.id}`)
            .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
          const command = commandDocs[0] || null;
          const resultText = command?.result ? JSON.stringify(command.result) : '';
          const visibleRedacted = !/(policy_decision|collection_decision|module_decision|reason_code|role_or_scope_denied|apps\.modify|apps\.release|payload_json|record_payload|selected_text|message_body|\bprompt\b|\btoken\b|\bsecret\b|DO_NOT_LEAK)/i.test(text);
          const resultRedacted = Boolean(command?.result)
            && command.result.kind === 'business_os_support_diagnostics_artifact'
            && command.result.artifact_schema === 'ctox.business_os.support_diagnostics.v1'
            && !resultText.includes('DO_NOT_LEAK');
          return {
            ok: Boolean(
              root
                && ['schema', 'redaction', 'scope', 'activity', 'why'].every((key) => rowKeys.includes(key))
                && root.getAttribute('data-support-schema') === 'ctox.business_os.support_diagnostics.v1'
                && root.getAttribute('data-redaction-profile') === 'support-safe-v1'
                && visibleRedacted
                && resultRedacted
                && download
            ),
            visible: Boolean(root),
            rowKeys,
            rows,
            schema: root?.getAttribute('data-support-schema') || '',
            redactionProfile: root?.getAttribute('data-redaction-profile') || '',
            visibleRedacted,
            resultRedacted,
            hasDownload: Boolean(download),
            commandStatus: command?.status || '',
            commandId: command?.id || '',
            statusText,
            supportStatusText,
            hasButton: Boolean(button),
            buttonDisabled: button?.disabled === true,
            text: text.slice(0, 1000),
          };
        }, 45000, 'settings support diagnostics command render');
        const settingsSupportDiagnosticsVisible = settingsSupportDiagnostics.visible === true;
        const settingsSupportDiagnosticsRows = Array.isArray(settingsSupportDiagnostics.rowKeys)
          && ['schema', 'redaction', 'scope', 'activity', 'why']
            .every((key) => settingsSupportDiagnostics.rowKeys.includes(key));
        const settingsSupportDiagnosticsRedacted = settingsSupportDiagnostics.visibleRedacted === true
          && settingsSupportDiagnostics.resultRedacted === true;
        const settingsSupportDiagnosticsDownload = settingsSupportDiagnostics.hasDownload === true;
        const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections: ['business_module_catalog', 'business_commands', 'ctox_runtime_settings', 'desktop_files', 'desktop_file_chunks'],
        });
        if (status?.version !== 'business-os-advanced-status-v1') {
          throw new Error(`roles/permissions UI smoke lost advanced status evidence: ${JSON.stringify(status)}`);
        }
        const checks = {
          teamModifyHidden,
          teamSourceHidden,
          sourceGrantVisible,
          modifyGrantVisible,
          ownerContextVisible,
          appbarSourceGate,
          exactScopeIsolated,
          ownerRoleOption,
          adminOwnerOptionHidden,
          businessLabels,
          settingsReleaseFallbackReadOnly,
          settingsWhyDiagnosticsVisible,
          settingsWhyDiagnosticsRows,
          settingsWhyDiagnosticsRedacted,
          settingsSupportDiagnosticsVisible,
          settingsSupportDiagnosticsRows,
          settingsSupportDiagnosticsRedacted,
          settingsSupportDiagnosticsDownload,
          reloadVerified: Boolean(rolesPermissionsReloadVerified),
        };
        const failed = Object.entries(checks).filter(([, value]) => value !== true);
        if (failed.length) {
          throw new Error(`roles/permissions UI smoke failed: ${JSON.stringify({
            failed,
            targetModule: targetModule.id,
            otherModule: otherModule.id,
            teamDomContext,
            sourceDomContext,
            modifyDomContext,
            ownerDomContext,
            teamHelper,
            sourceHelper,
            modifyHelper,
            ownerHelper,
            sourceOtherHelper,
            modifyOtherHelper,
            ownerAssignable,
            adminAssignable,
            settingsFallback,
            settingsWhyDiagnostics,
            settingsSupportDiagnostics,
            appbar: {
              teamAppbarSourceVisible,
              sourceAppbarSourceVisible,
              modifyAppbarSourceVisible,
            },
          }, null, 2)}`);
        }
        return {
          mode: smokeMode,
          targetModuleId: targetModule.id,
          otherModuleId: otherModule.id,
          teamModifyHidden,
          teamSourceHidden,
          sourceGrantVisible,
          modifyGrantVisible,
          ownerContextVisible,
          appbarSourceGate,
          exactScopeIsolated,
          ownerRoleOption,
          adminOwnerOptionHidden,
          businessLabels,
          settingsReleaseFallbackReadOnly,
          settingsWhyDiagnosticsVisible,
          settingsWhyDiagnosticsRows,
          settingsWhyDiagnosticsRedacted,
          settingsSupportDiagnosticsVisible,
          settingsSupportDiagnosticsRows,
          settingsSupportDiagnosticsRedacted,
          settingsSupportDiagnosticsDownload,
          reloadVerified: Boolean(rolesPermissionsReloadVerified),
          authState: document.body?.dataset?.authState || (state.session?.user?.id ? 'local-session' : 'unknown'),
          advancedStatusVersion: status.version || '',
          advancedStatusRuntime: status.rxdbRuntime || null,
        };
      }

      async function runBusinessOsAgentScopeUiSmoke() {
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            last = await predicate();
            if (last?.ok) return last;
            await delay(100);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/["\\]/g, '\\$&');
        };
        const docsToJson = (docs) => (Array.isArray(docs) ? docs : [])
          .map((doc) => doc?.toJSON?.() || doc)
          .filter(Boolean);
        const smoke = globalThis.ctoxBusinessOsSmoke;
        const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state || appState;
        if (!state) throw new Error('Business OS app state is unavailable for agent scope UI smoke');
        if (typeof smoke?.renderTabs !== 'function') throw new Error('Business OS smoke renderTabs hook is unavailable');
        if (typeof state.openModule !== 'function') throw new Error('Business OS state.openModule is unavailable for agent scope UI smoke');
        if (typeof smoke?.createLiveDbFacade !== 'function') throw new Error('Business OS smoke DB facade hook is unavailable for agent scope UI smoke');
        appState = state;

        const { BusinessOsPermissions } = await import('/shared/permissions.js');
        const targetModule = {
          id: 'phase12-agent-scope-app',
          title: 'Phase 12 Agent Scope App',
          glyph: '12',
          version: '1.0.0',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase12-agent-scope-app/index.js',
          editable: true,
          layout: { shell: 'full-workspace' },
          collections: ['business_commands'],
          lifecycle: { runtime_installed: true },
        };
        const hiddenModule = {
          id: 'phase12-hidden-agent-scope-app',
          title: 'Phase 12 Hidden Agent Scope App',
          glyph: 'H12',
          version: '0.2.0',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase12-hidden-agent-scope-app/index.js',
          editable: true,
          layout: { shell: 'full-workspace' },
          collections: ['business_commands'],
          lifecycle: {
            runtime_installed: true,
            visibility_state: 'private',
            audience: 'private',
          },
        };
        const allPermissions = Object.values(BusinessOsPermissions);
        const actorSession = {
          authenticated: true,
          user: {
            id: 'agent_scope_team',
            display_name: 'Agent Scope Team',
            role: 'user',
          },
        };
        const governance = {
          founders: {},
          permission_model: {
            version: 1,
            deny_supported: false,
            role_defaults: {
              chef: {
                workspace: allPermissions,
                module: allPermissions,
                assigned_module: allPermissions,
              },
              admin: {
                workspace: allPermissions,
                module: allPermissions,
                assigned_module: allPermissions,
              },
              founder: {
                workspace: [],
                module: [],
                assigned_module: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                ],
              },
              user: {
                workspace: [],
                module: [],
                assigned_module: [],
              },
            },
            module_assignments: {},
            explicit_grants: [],
          },
        };
        const originalState = {
          modules: Array.isArray(state.modules) ? [...state.modules] : [],
          taskbarPins: Array.isArray(state.taskbarPins) ? [...state.taskbarPins] : [],
          moduleAllowlist: Array.isArray(state.moduleAllowlist) ? [...state.moduleAllowlist] : state.moduleAllowlist,
          session: state.session,
          governance: state.governance,
          activeModule: state.activeModule,
          globalSession: globalThis.CTOX_BUSINESS_OS_SESSION,
          bodyAuthState: document.body?.dataset?.authState || '',
        };
        const insertedIds = new Set([targetModule.id, hiddenModule.id]);
        let agentScopeCatalogSeedCount = 0;
        let lastAgentScopeCatalogSeedAt = 0;
        const seedAgentScopeModuleCatalog = async ({ force = false } = {}) => {
          const now = Date.now();
          if (!force && now - lastAgentScopeCatalogSeedAt < 500) return true;
          const collection = state.db?.collection?.('business_module_catalog');
          if (!collection) return false;
          const doc = await collection.findOne('module-catalog').exec().catch(() => null);
          const existing = doc?.toJSON?.() || {
            id: 'module-catalog',
            ok: true,
            modules: [],
            templates: [],
            governance: null,
          };
          const {
            _rev,
            _attachments,
            ...catalog
          } = existing;
          void _rev;
          void _attachments;
          const modules = Array.isArray(catalog.modules) ? catalog.modules : [];
          const nextCatalog = {
            ...catalog,
            id: 'module-catalog',
            ok: catalog.ok !== false,
            modules: [
              ...modules.filter((mod) => !insertedIds.has(mod?.id)),
              targetModule,
              hiddenModule,
            ],
            updated_at_ms: now,
            source: catalog.source || 'business-os-agent-scope-smoke',
          };
          if (Array.isArray(catalog.allowed_module_ids)) {
            nextCatalog.allowed_module_ids = [...new Set([
              ...catalog.allowed_module_ids.map((id) => String(id || '').trim()).filter(Boolean),
              targetModule.id,
              hiddenModule.id,
            ])];
          }
          if (typeof collection.upsert === 'function') {
            await collection.upsert(nextCatalog);
          } else if (doc && typeof doc.incrementalPatch === 'function') {
            await doc.incrementalPatch(nextCatalog);
          } else if (!doc) {
            await collection.insert(nextCatalog);
          }
          agentScopeCatalogSeedCount += 1;
          lastAgentScopeCatalogSeedAt = now;
          return true;
        };
        const applyAgentScopeState = () => {
          state.session = actorSession;
          state.governance = governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = actorSession;
          document.body.dataset.authState = 'authenticated';
          state.modules = [
            ...originalState.modules.filter((mod) => !insertedIds.has(mod?.id)),
            targetModule,
            hiddenModule,
          ];
          state.moduleAllowlist = [...new Set([
            ...(Array.isArray(originalState.moduleAllowlist)
              ? originalState.moduleAllowlist.map((id) => String(id || '').trim()).filter(Boolean)
              : []),
            targetModule.id,
            hiddenModule.id,
          ])];
          state.taskbarPins = [...new Set([
            ...(Array.isArray(originalState.taskbarPins) ? originalState.taskbarPins : []),
            targetModule.id,
            hiddenModule.id,
          ])];
          smoke.renderTabs();
        };
        const assertDenied = async (label, callback) => {
          try {
            const result = callback();
            if (result && typeof result.then === 'function') await result;
            if (result && typeof result.exec === 'function') await result.exec();
          } catch (error) {
            if (error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
              || error?.name === 'BusinessOsPermissionError'
              || /permission/i.test(String(error?.message || error))) {
              return true;
            }
            throw error;
          }
          throw new Error(`${label} unexpectedly allowed`);
        };
        const scopeRowsFromPanel = (panel) => (panel
          ? [...panel.querySelectorAll('[data-agent-scope-row]')].map((row) => ({
            key: row.getAttribute('data-agent-scope-row') || '',
            label: row.querySelector('dt')?.textContent?.trim() || '',
            value: row.querySelector('dd')?.textContent?.trim() || '',
          }))
          : []);
        const scopeRowsMatchVisibleScope = (rows, visibleScope) => {
          const scopeRows = Array.isArray(visibleScope?.rows) ? visibleScope.rows : [];
          return rows.length > 0 && rows.every((row) => {
            const matching = scopeRows.find((scopeRow) => scopeRow.key === row.key);
            return matching
              && String(matching.label || '') === row.label
              && String(matching.value || '') === row.value;
          });
        };
        const openGlobalContextMenu = async () => {
          let contextMenuDispatchCount = 0;
          let lastContextMenuEventPrevented = false;
          return waitFor(async () => {
            if (state.activeModule?.id !== targetModule.id
              || !document.querySelector(`[data-agent-scope-fixture="${css(targetModule.id)}"]`)) {
              applyAgentScopeState();
              await state.openModule(targetModule.id, { force: true, asModule: true });
            }
            let target = document.querySelector(`[data-agent-scope-fixture="${css(targetModule.id)}"]`)
              || document.querySelector('[data-module-content]')
              || document.querySelector('[data-module-root]');
            let menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
            let panel = menu?.querySelector('.ctox-agent-scope') || null;
            let rows = scopeRowsFromPanel(panel);
            if (!(menu && panel && rows.length >= 4 && /Phase 12 Agent Scope App/.test(panel.textContent || ''))) {
              if (!target) {
                return {
                  ok: false,
                  rows,
                  text: panel?.textContent?.trim() || '',
                  activeModule: state.activeModule?.id || document.body?.dataset?.activeModule || '',
                  fixture: Boolean(document.querySelector(`[data-agent-scope-fixture="${css(targetModule.id)}"]`)),
                  dispatchCount: contextMenuDispatchCount,
                  reason: 'agent scope fixture DOM target is missing',
                };
              }
              const rect = target.getBoundingClientRect();
              lastContextMenuEventPrevented = !target.dispatchEvent(new MouseEvent('contextmenu', {
                bubbles: true,
                cancelable: true,
                button: 2,
                buttons: 2,
                composed: true,
                clientX: Math.max(24, Math.round(rect.left + 12)),
                clientY: Math.max(24, Math.round(rect.top + 12)),
              }));
              contextMenuDispatchCount += 1;
              menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
              panel = menu?.querySelector('.ctox-agent-scope') || null;
              rows = scopeRowsFromPanel(panel);
            }
            menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
            panel = menu?.querySelector('.ctox-agent-scope') || null;
            rows = scopeRowsFromPanel(panel);
            return {
              ok: Boolean(menu && panel && rows.length >= 4 && /Phase 12 Agent Scope App/.test(panel.textContent || '')),
              rows,
              text: panel?.textContent?.trim() || '',
              eventPrevented: lastContextMenuEventPrevented,
              dispatchCount: contextMenuDispatchCount,
              activeModule: state.activeModule?.id || document.body?.dataset?.activeModule || '',
              fixture: Boolean(document.querySelector(`[data-agent-scope-fixture="${css(targetModule.id)}"]`)),
              hidden: menu?.hidden ?? null,
            };
          }, 20000, 'agent scope global context menu');
        };
        const submitGlobalContextMenu = async () => {
          let submittedDetail = null;
          const originalDispatch = state.commandBus?.dispatch;
          if (typeof originalDispatch !== 'function') throw new Error('agent scope command bus dispatch is missing');
          state.commandBus.dispatch = async function patchedAgentScopeDispatch(command, options) {
            submittedDetail = JSON.parse(JSON.stringify({
              text: command?.payload?.prompt || command?.payload?.user_message || command?.payload?.instruction || '',
              module: command?.module || '',
              source_title: targetModule.title,
              command_type: command?.command_type || command?.type || '',
              record_id: command?.record_id || '',
              title: command?.payload?.title || '',
              instruction: command?.payload?.instruction || '',
              payload: command?.payload || {},
              client_context: command?.client_context || {},
            }));
            return originalDispatch.call(this, command, options);
          };
          const menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
          const form = menu?.querySelector('form');
          const textarea = menu?.querySelector('textarea');
          if (!form || !textarea) throw new Error('agent scope context form is missing');
          const askInput = menu.querySelector('input[name="contextMode"][value="ask"]');
          askInput?.closest('label')?.dispatchEvent(new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
          }));
          textarea.value = 'Bitte prüfe den sichtbaren Agent Scope.';
          form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
          try {
            return await waitFor(() => ({
              ok: Boolean(submittedDetail),
              detail: submittedDetail,
            }), 5000, 'agent scope context submit detail');
          } finally {
            state.commandBus.dispatch = originalDispatch;
          }
        };
        const openAppStoreContextMenu = async () => {
          await seedAgentScopeModuleCatalog({ force: true });
          applyAgentScopeState();
          await state.openModule('app-store', { force: true, asModule: true });
          await waitFor(() => ({
            ok: Boolean(document.querySelector('[data-app-store-root]') && document.querySelector('[data-apps-grid]')),
            activeModule: state.activeModule?.id || document.body?.dataset?.activeModule || '',
            text: document.querySelector('[data-app-store-root]')?.innerText?.slice(0, 500) || '',
          }), 30000, 'agent scope App Store open');
          document.querySelector('[data-scope="installed"]')?.dispatchEvent(new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
          }));
          const cardState = await waitFor(() => {
            const card = document.querySelector(`[data-app-id="${css(targetModule.id)}"]`);
            return {
              ok: Boolean(card && /Phase 12 Agent Scope App/.test(card.textContent || '')),
              hasCard: Boolean(card),
              cardText: card?.innerText?.slice(0, 800) || '',
              activeModule: state.activeModule?.id || '',
            };
          }, 15000, 'agent scope App Store card');
          let contextMenuDispatchCount = 0;
          let lastContextMenuEventPrevented = false;
          return waitFor(async () => {
            let menu = document.querySelector('.app-store-context-menu:not([hidden])');
            let panel = menu?.querySelector('.ctox-agent-scope') || null;
            let rows = scopeRowsFromPanel(panel);
            if (!(menu && panel && rows.length >= 4 && /Phase 12 Agent Scope App/.test(panel.textContent || ''))) {
              let currentCard = document.querySelector(`[data-app-id="${css(targetModule.id)}"]`);
              if (!currentCard) {
                await seedAgentScopeModuleCatalog();
                document.querySelector('[data-app-store-root] [data-scope="installed"]')?.dispatchEvent(new MouseEvent('click', {
                  bubbles: true,
                  cancelable: true,
                }));
                currentCard = document.querySelector(`[data-app-id="${css(targetModule.id)}"]`);
              }
              if (currentCard) {
                const rect = currentCard.getBoundingClientRect();
                lastContextMenuEventPrevented = !currentCard.dispatchEvent(new MouseEvent('contextmenu', {
                  bubbles: true,
                  cancelable: true,
                  button: 2,
                  buttons: 2,
                  composed: true,
                  clientX: Math.max(24, Math.round(rect.left + 18)),
                  clientY: Math.max(24, Math.round(rect.top + 18)),
                }));
                contextMenuDispatchCount += 1;
                menu = document.querySelector('.app-store-context-menu:not([hidden])');
                panel = menu?.querySelector('.ctox-agent-scope') || null;
                rows = scopeRowsFromPanel(panel);
              }
            }
            const allMenus = [...document.querySelectorAll('.app-store-context-menu')];
            const currentCard = document.querySelector(`[data-app-id="${css(targetModule.id)}"]`);
            const host = document.querySelector('[data-module-content]') || document.querySelector('[data-module-root]');
            return {
              ok: Boolean(menu && panel && rows.length >= 4 && /Phase 12 Agent Scope App/.test(panel.textContent || '')),
              rows,
              text: panel?.textContent?.trim() || '',
              eventPrevented: lastContextMenuEventPrevented,
              dispatchCount: contextMenuDispatchCount,
              activeModule: state.activeModule?.id || document.body?.dataset?.activeModule || '',
              menuCount: allMenus.length,
              menuHidden: allMenus.map((entry) => entry.hidden),
              menuText: allMenus.map((entry) => entry.textContent?.trim?.().slice(0, 500) || ''),
              cardConnected: Boolean(currentCard?.isConnected),
              hostHasCard: Boolean(host && currentCard && host.contains(currentCard)),
              hostLocalContextMenu: host?.getAttribute?.('data-ctox-local-context-menu') || '',
              catalogSeedCount: agentScopeCatalogSeedCount,
            };
          }, 5000, 'agent scope App Store context menu');
        };
        const submitAppStoreContextMenu = async () => {
          let submittedDetail = null;
          const listener = (event) => {
            submittedDetail = JSON.parse(JSON.stringify(event.detail || {}));
          };
          window.addEventListener('ctox-business-os-chat-submit', listener, { capture: true, once: true });
          const menu = document.querySelector('.app-store-context-menu:not([hidden])');
          const form = menu?.querySelector('[data-app-store-context-chat-form]');
          const textarea = menu?.querySelector('[data-app-store-context-message]');
          if (!form || !textarea) throw new Error('agent scope App Store context form is missing');
          textarea.value = 'Bitte prüfe den App-Store-Scope im Chat.';
          textarea.dispatchEvent(new Event('input', { bubbles: true }));
          form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
          try {
            return await waitFor(() => ({
              ok: Boolean(submittedDetail),
              detail: submittedDetail,
            }), 5000, 'agent scope App Store context submit detail');
          } finally {
            window.removeEventListener('ctox-business-os-chat-submit', listener, { capture: true });
          }
        };
        const waitForBusinessChatScope = async (submittedDetail) => {
          const expectedScope = submittedDetail?.client_context?.visible_scope || null;
          return waitFor(() => {
            const panels = [...document.querySelectorAll('[data-ctox-chat-root] .ctox-chat-messages .ctox-agent-scope')];
            const matches = panels
              .map((panel) => ({
                panel,
                rows: scopeRowsFromPanel(panel),
                text: panel.textContent || '',
              }))
              .filter((entry) => /Phase 12 Agent Scope App/.test(entry.text));
            const match = matches.find((entry) => scopeRowsMatchVisibleScope(entry.rows, expectedScope)) || null;
            return {
              ok: Boolean(match),
              rows: match?.rows || [],
              panelCount: panels.length,
              matchingPanelCount: matches.length,
              text: match?.text?.trim?.() || '',
            };
          }, 15000, 'agent scope Business Chat visible scope');
        };
        const openAgentGrantBoundarySettings = async () => {
          applyAgentScopeState();
          state.session = {
            authenticated: true,
            user: {
              id: 'agent_scope_owner',
              display_name: 'Agent Scope Owner',
              role: 'chef',
              is_admin: true,
            },
          };
          globalThis.CTOX_BUSINESS_OS_SESSION = state.session;
          document.body.dataset.authState = 'authenticated';
          if (typeof smoke.openSettingsDrawer !== 'function') {
            throw new Error('Business OS smoke settings drawer hook is unavailable for agent scope UI smoke');
          }
          await smoke.openSettingsDrawer({ initialTab: 'admin' });
          return waitFor(() => {
            const panel = document.querySelector('[data-agent-grant-boundary]');
            const text = panel?.textContent || '';
            return {
              ok: Boolean(
	                panel
	                  && /Agent- und App-Zugriff/.test(text)
	                  && /agent_scope_team/.test(text)
	                  && /Daten lesen/.test(text)
                  && /Datenbereich Business Commands/.test(text)
                  && /Owner\/Admin-Policy/.test(text)
              ),
              text: text.trim().slice(0, 1000),
            };
          }, 15000, 'agent scope Settings grant boundary');
        };

        try {
          await seedAgentScopeModuleCatalog({ force: true });
          applyAgentScopeState();
          await state.openModule(targetModule.id, { force: true, asModule: true });
          await waitFor(() => ({
            ok: state.activeModule?.id === targetModule.id
              && Boolean(document.querySelector(`[data-agent-scope-fixture="${css(targetModule.id)}"]`)),
            activeModule: state.activeModule?.id || '',
            fixture: globalThis.__ctoxAgentScopeFixture || null,
          }), 30000, 'agent scope target module open');
          await waitFor(() => ({
            ok: Boolean(document.querySelector('[data-ctox-chat-root]')),
            hasChatRoot: Boolean(document.querySelector('[data-ctox-chat-root]')),
          }), 10000, 'agent scope business chat root');

          const menu = await openGlobalContextMenu();
          const submitted = await submitGlobalContextMenu();
          const detail = submitted.detail || {};
          const visibleScope = detail.client_context?.visible_scope || null;
          const clientContextMatchesUi = Boolean(
            visibleScope
              && visibleScope.app?.module_id === targetModule.id
              && detail.client_context?.module_id === targetModule.id
              && detail.client_context?.app_id === targetModule.id
              && detail.client_context?.actor?.id === actorSession.user.id
              && scopeRowsMatchVisibleScope(menu.rows, visibleScope)
          );

          const appStoreMenu = await openAppStoreContextMenu();
          const appStoreSubmitted = await submitAppStoreContextMenu();
          const appStoreDetail = appStoreSubmitted.detail || {};
          const appStoreVisibleScope = appStoreDetail.client_context?.visible_scope || null;
          const appStoreClientContextMatchesUi = Boolean(
            appStoreVisibleScope
              && appStoreVisibleScope.app?.module_id === targetModule.id
              && appStoreDetail.client_context?.module_id === targetModule.id
              && appStoreDetail.client_context?.app_id === targetModule.id
              && appStoreDetail.client_context?.actor?.id === actorSession.user.id
              && appStoreDetail.payload?.mode === 'data'
              && appStoreDetail.command_type === 'business_os.chat.task'
              && scopeRowsMatchVisibleScope(appStoreMenu.rows, appStoreVisibleScope)
          );
          const businessChatScope = await waitForBusinessChatScope(appStoreDetail);
          const businessChatScopeMatchesContext = scopeRowsMatchVisibleScope(
            businessChatScope.rows,
            appStoreVisibleScope,
          );

          await waitFor(() => {
            let collection = null;
            try {
              collection = state.db?.collection?.('business_commands') || state.db?.raw?.business_commands || null;
            } catch {
              collection = null;
            }
            return {
              ok: Boolean(collection),
              collections: Object.keys(state.db?.raw || {}).slice(0, 12),
            };
          }, 15000, 'agent scope business_commands collection');
          await state.sync?.startCollection?.('business_commands');
          const dataDeniedBeforeGrant = await assertDenied('agent scope read without data.read', () => {
            return smoke.createLiveDbFacade(targetModule)
              .collection('business_commands')
              .findOne('phase12_agent_scope_probe');
          });
          governance.permission_model.explicit_grants.push({
            grant_id: 'phase12_agent_scope_read_business_commands',
            subject_type: 'user',
            subject_id: actorSession.user.id,
            permission: BusinessOsPermissions.DataRead,
            scope_type: 'collection',
            scope_id: 'business_commands',
            active: true,
          });
          state.governance = governance;
          const readQuery = smoke.createLiveDbFacade(targetModule)
            .collection('business_commands')
            .findOne('phase12_agent_scope_probe');
          const readAllowedAfterGrant = Boolean(readQuery && typeof readQuery.exec === 'function');
          const writeDeniedWithoutGrant = await assertDenied('agent scope write without data.write', () => {
            return smoke.createLiveDbFacade(targetModule)
              .collection('business_commands')
              .insert({
                id: 'phase12_agent_scope_write_probe',
                command_id: 'phase12_agent_scope_write_probe',
                module: targetModule.id,
                command_type: 'phase12.agent_scope.write',
                status: 'pending_sync',
                payload: {},
                client_context: { source: 'phase12-agent-scope-ui-smoke' },
                updated_at_ms: Date.now(),
              });
          });
          const grantBoundary = await openAgentGrantBoundarySettings();

          const commandId = `cmd_phase12_agent_scope_${Date.now()}`;
          const dispatchResult = await state.commandBus.dispatch({
            id: commandId,
            wait_timeout_ms: 45000,
            module: targetModule.id,
            type: detail.command_type || 'business_os.chat.task',
            record_id: detail.record_id || targetModule.id,
            inbound_channel: 'business_os.agent_scope_smoke',
            payload: {
              ...(detail.payload || {}),
              title: detail.title || 'Agent scope smoke',
              instruction: detail.instruction || detail.text || 'Agent scope smoke',
            },
            client_context: {
              ...(detail.client_context || {}),
              source: 'business-os-agent-scope-smoke',
              audit_probe: true,
            },
          });
          const commandCollection = state.db?.raw?.business_commands || state.db?.collection?.('business_commands');
          const persistedCommand = await waitFor(async () => {
            const docs = docsToJson(await commandCollection.find().exec());
            const doc = docs.find((item) => item.id === commandId || item.command_id === commandId);
            return {
              ok: Boolean(doc),
              command: doc || null,
              count: docs.length,
            };
          }, 15000, 'agent scope persisted command');
          const persistedContext = persistedCommand.command?.client_context || {};
          const auditVisible = Boolean(
            persistedCommand.command
              && persistedContext.visible_scope?.app?.module_id === targetModule.id
              && (dispatchResult?.task_id || dispatchResult?.command_id || commandId)
          );

          applyAgentScopeState();
          smoke.renderTabs();
          const hiddenTab = document.querySelector(`.module-tab[data-module="${css(hiddenModule.id)}"], .module-tab[data-target="${css(hiddenModule.id)}"]`);
          await state.openModule(hiddenModule.id, { force: true, asModule: true });
          await delay(150);
          const statusText = document.body?.innerText || '';
          const appHiddenDenied = !hiddenTab && state.activeModule?.id !== hiddenModule.id;
          const deniedReasonVisible = /nicht sichtbar|not visible|Privat|private/i.test(statusText);

          const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['business_module_catalog', 'business_commands', 'ctox_runtime_settings'],
          });
          if (status?.version !== 'business-os-advanced-status-v1') {
            throw new Error(`agent scope UI smoke lost advanced status evidence: ${JSON.stringify(status)}`);
          }

          const checks = {
            panelVisible: menu.ok === true,
            clientContextMatchesUi,
            appStorePanelVisible: appStoreMenu.ok === true,
            appStoreClientContextMatchesUi,
            businessChatScopeMatchesContext,
            settingsGrantBoundaryVisible: grantBoundary.ok === true,
            appHiddenDenied,
            dataDeniedBeforeGrant,
            readAllowedAfterGrant,
            writeDeniedWithoutGrant,
            auditVisible,
            deniedReasonVisible,
          };
          const failed = Object.entries(checks).filter(([, value]) => value !== true);
          if (failed.length) {
            throw new Error(`agent scope UI smoke failed: ${JSON.stringify({
              failed,
              menu,
              detail,
              visibleScope,
              appStoreMenu,
              appStoreDetail,
              appStoreVisibleScope,
              businessChatScope,
              grantBoundary,
              persistedCommand: persistedCommand.command,
              dispatchResult,
              appHiddenDenied,
              deniedReasonVisible,
              activeModule: state.activeModule?.id || '',
            }, null, 2)}`);
          }

          return {
            mode: smokeMode,
            targetModuleId: targetModule.id,
            agentId: 'ctox',
            actorRole: actorSession.user.role,
            authState: 'authenticated',
            browserContext: 'clean',
            tenantScope: 'local-workspace',
            panelVisible: true,
            clientContextMatchesUi,
            appStorePanelVisible: true,
            appStoreClientContextMatchesUi,
            businessChatScopeMatchesContext,
            settingsGrantBoundaryVisible: grantBoundary.ok === true,
            appHiddenDenied,
            dataDeniedBeforeGrant,
            readAllowedAfterGrant,
            writeDeniedWithoutGrant,
            auditVisible,
            deniedReasonVisible,
            advancedStatusVersion: status.version || '',
            advancedStatusRuntime: status.rxdbRuntime || null,
          };
        } finally {
          state.session = originalState.session;
          state.governance = originalState.governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = originalState.globalSession;
          if (originalState.bodyAuthState) {
            document.body.dataset.authState = originalState.bodyAuthState;
          } else {
            delete document.body.dataset.authState;
          }
        }
      }

      async function runBusinessOsThreadsScaleUiSmoke() {
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            last = await predicate();
            if (last?.ok) return last;
            await delay(100);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const smoke = globalThis.ctoxBusinessOsSmoke;
        const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state || appState;
        if (!state?.openModule) throw new Error('Business OS app state is unavailable for threads scale UI smoke');
        const requiredCollections = [
          'business_commands',
          'user_threads',
          'user_thread_messages',
          'user_notifications',
          'ctox_task_approval_requests',
        ];
        const renderStartedAt = performance.now();
        await state.openModule('threads', { force: true, asModule: true });
        await Promise.all(requiredCollections.map((name) => state.sync?.startCollection?.(name).catch(() => null)));
        const rendered = await waitFor(() => {
          const root = document.querySelector('[data-threads-root]');
          const visibleThreadRows = root?.querySelectorAll?.('[data-thread-id]')?.length || 0;
          return {
            ok: Boolean(root && visibleThreadRows > 0 && visibleThreadRows <= 200),
            visibleThreadRows,
            renderMs: Math.round(performance.now() - renderStartedAt),
          };
        }, 30000, 'threads scale first bounded render');
        const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections,
        });
        if (status?.version !== 'business-os-advanced-status-v1' || status.ok !== true) {
          throw new Error(`threads scale advanced status unhealthy: ${JSON.stringify(status)}`);
        }
        const scale = threadsScaleSeed || {};
        const scaleBudgetPassed = Number(scale.commands || 0) >= 10000
          && Number(scale.threads || 0) >= 10000
          && Number(scale.messages || 0) >= 10000
          && Number(scale.notifications || 0) >= 10000
          && rendered.visibleThreadRows >= 1
          && rendered.visibleThreadRows <= 200
          && rendered.renderMs <= 30000;
        if (!scaleBudgetPassed) {
          throw new Error(`threads scale budget failed: ${JSON.stringify({ scale, rendered })}`);
        }
        return {
          mode: smokeMode,
          scaleCommands: Number(scale.commands || 0),
          scaleThreads: Number(scale.threads || 0),
          scaleMessages: Number(scale.messages || 0),
          scaleNotifications: Number(scale.notifications || 0),
          scaleVisibleThreadRows: rendered.visibleThreadRows,
          scaleFirstRenderMs: rendered.renderMs,
          scaleBudgetPassed,
          authState: 'authenticated',
          actorRole: 'admin',
          browserContext: 'clean',
          tenantScope: 'local-workspace',
          advancedStatusVersion: status.version || '',
          advancedStatusRuntime: status.rxdbRuntime || null,
        };
      }

      async function runBusinessOsThreadsRightClickUiSmoke() {
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            last = await predicate();
            if (last?.ok) return last;
            await delay(100);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/["\\]/g, '\\$&');
        };
        const docsToJson = (docs) => (Array.isArray(docs) ? docs : [])
          .map((doc) => doc?.toJSON?.() || doc)
          .filter((doc) => doc && doc._deleted !== true && doc.is_deleted !== true);
        const smoke = globalThis.ctoxBusinessOsSmoke;
        const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state || appState;
        if (!state) throw new Error('Business OS app state is unavailable for threads right-click UI smoke');
        if (typeof smoke?.renderTabs !== 'function') throw new Error('Business OS smoke renderTabs hook is unavailable');
        if (typeof state.openModule !== 'function') throw new Error('Business OS state.openModule is unavailable for threads right-click UI smoke');
        if (typeof state.commandBus?.dispatch !== 'function') throw new Error('Business OS command bus is unavailable for threads right-click UI smoke');
        appState = state;

        const targetModule = {
          id: 'notes',
          title: 'Notes',
          glyph: 'N',
        };
        const requesterSession = {
          authenticated: true,
          user: {
            id: 'threads-requester',
            display_name: 'Threads Requester',
            role: 'user',
            is_admin: false,
          },
        };
        const reviewerSession = {
          authenticated: true,
          user: {
            id: 'threads-reviewer',
            display_name: 'Threads Reviewer',
            role: 'admin',
            is_admin: true,
          },
        };
        const reviewerId = reviewerSession.user.id;
        const targetRecordId = 'notes_seed_ops_review';
        const appTargetRecordId = 'notes';
        const threadsCollections = [
          'user_threads',
          'user_thread_messages',
          'user_thread_links',
          'user_notifications',
          'ctox_task_approval_requests',
        ];
        const dataPrompt = `Threads right-click data change ${Date.now()}`;
        const askPrompt = `Threads right-click question ${Date.now()}`;
        const appPrompt = `Threads right-click app change ${Date.now()}`;
        const reviewerPickerEvidence = [];
        let scaleFirstRenderEvidence = null;
        const originalState = {
          session: state.session,
          governance: state.governance,
          activeModule: state.activeModule,
          modules: state.modules,
          taskbarPins: state.taskbarPins,
          moduleAllowlist: state.moduleAllowlist,
          globalSession: globalThis.CTOX_BUSINESS_OS_SESSION,
          bodyAuthState: document.body?.dataset?.authState || '',
        };
        const applyThreadsSmokeState = async (session, capability) => {
          const nextSession = {
            ...session,
            capability_token: capability?.token || '',
            capability_expires_at_ms: capability?.expiresAtMs || 0,
          };
          state.session = nextSession;
          globalThis.CTOX_BUSINESS_OS_SESSION = nextSession;
          document.body.dataset.authState = 'authenticated';
          const commandBusResource = performance.getEntriesByType('resource')
            .map((entry) => entry.name)
            .find((name) => /\/shared\/command-bus\.js\?v=/.test(name));
          if (!commandBusResource) throw new Error('loaded command-bus module URL is unavailable for capability switch');
          const commandBusModule = await import(commandBusResource);
          commandBusModule.resetBusinessOsCapabilityTokenCacheForTests?.();
        };
        const ensureThreadsModuleCollections = async () => {
          await applyThreadsSmokeState(requesterSession, threadsRightClickCapabilities?.requester);
          const renderStartedAt = performance.now();
          await state.openModule('threads', { force: true, asModule: true });
          await waitFor(() => {
            const raw = state.db?.raw || {};
            return {
              ok: threadsCollections.every((name) => Boolean(raw[name])),
              activeModule: state.activeModule?.id || '',
              missing: threadsCollections.filter((name) => !raw[name]),
            };
          }, 30000, 'threads module collections registered');
          await Promise.all(threadsCollections.map((name) => (
            state.sync?.startCollection?.(name).catch(() => null)
          )));
          if (threadsScaleSeed) {
            scaleFirstRenderEvidence = await waitFor(() => {
              const root = document.querySelector('[data-threads-root]');
              const visibleThreadRows = root?.querySelectorAll?.('[data-thread-id]')?.length || 0;
              return {
                ok: Boolean(root && visibleThreadRows > 0 && visibleThreadRows <= 200),
                visibleThreadRows,
                renderMs: Math.round(performance.now() - renderStartedAt),
              };
            }, 30000, 'threads scale first bounded render');
          }
        };
        const openTargetModule = async () => {
          await applyThreadsSmokeState(requesterSession, threadsRightClickCapabilities?.requester);
          await state.openModule(targetModule.id, { force: true, asModule: true });
          return waitFor(() => {
            const host = document.querySelector('[data-module-content], [data-module-root], [data-ctox-chat-root]')
              || document.querySelector('main')
              || document.body;
            let marker = document.querySelector('[data-threads-rightclick-fixture]');
            if (!marker && host) {
              marker = document.createElement('section');
              marker.dataset.threadsRightclickFixture = 'true';
              marker.dataset.moduleRoot = targetModule.id;
              marker.dataset.contextRecordId = targetRecordId;
              marker.dataset.contextRecordType = 'smoke-record';
              marker.dataset.contextLabel = 'Threads Right-Click Smoke Record';
              marker.style.position = 'relative';
              marker.style.padding = '8px';
              marker.style.margin = '8px';
              marker.style.border = '1px solid transparent';
              marker.textContent = 'Threads Right-Click Smoke Record';
              host.append(marker);
            }
            return {
              ok: state.activeModule?.id === targetModule.id && Boolean(marker),
              activeModule: state.activeModule?.id || '',
              hasMarker: Boolean(marker),
            };
          }, 30000, 'threads right-click target module open');
        };
        const openGlobalContextMenu = async () => {
          const target = document.querySelector('[data-threads-rightclick-fixture]');
          if (!target) throw new Error('threads right-click fixture DOM target is missing');
          const rect = target.getBoundingClientRect();
          target.dispatchEvent(new MouseEvent('contextmenu', {
            bubbles: true,
            cancelable: true,
            button: 2,
            clientX: Math.max(24, Math.round(rect.left + 12)),
            clientY: Math.max(24, Math.round(rect.top + 12)),
          }));
          return waitFor(() => {
            const menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
            const form = menu?.querySelector('form') || null;
            const modes = menu ? [...menu.querySelectorAll('input[name="contextMode"]')].map((input) => input.value) : [];
            return {
              ok: Boolean(menu && form && ['data', 'ask', 'app'].every((mode) => modes.includes(mode))),
              modes,
              text: menu?.textContent?.trim().slice(0, 1000) || '',
            };
          }, 5000, 'threads right-click global context menu');
        };
        const waitForReviewerOption = async () => waitFor(() => {
          const menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
          const options = menu
            ? [...menu.querySelectorAll('[data-ctox-context-user-options] option')].map((option) => ({
              value: option.getAttribute('value') || '',
              label: option.getAttribute('label') || '',
            }))
            : [];
          return {
            ok: options.some((option) => option.value === reviewerId && /Threads Reviewer/.test(option.label)),
            options,
          };
        }, 5000, 'threads right-click reviewer option');
        const submitContextMode = async ({ mode, message, userId, contextRecordId = targetRecordId }) => {
          await openTargetModule();
          const contextTarget = document.querySelector('[data-threads-rightclick-fixture]');
          contextTarget.dataset.contextRecordId = contextRecordId;
          contextTarget.dataset.contextLabel = contextRecordId === appTargetRecordId
            ? 'Threads Right-Click App Smoke Record'
            : 'Threads Right-Click Smoke Record';
          await openGlobalContextMenu();
          const reviewerOption = await waitForReviewerOption().catch((error) => ({
            ok: false,
            error: String(error?.message || error),
          }));
          reviewerPickerEvidence.push({
            mode,
            visible: reviewerOption.ok === true,
            optionCount: Array.isArray(reviewerOption.options) ? reviewerOption.options.length : 0,
            error: reviewerOption.error || '',
          });
          const menu = document.querySelector('.ctox-global-context-menu:not([hidden])');
          const form = menu?.querySelector('form');
          const input = menu?.querySelector(`input[name="contextMode"][value="${css(mode)}"]`);
          const label = input?.closest('label') || null;
          const textarea = menu?.querySelector('.ctox-context-textarea');
          const userInput = menu?.querySelector('.ctox-context-user-input');
          if (!form || !input || !label || !textarea || !userInput) {
            throw new Error(`threads right-click context form missing controls for ${mode}`);
          }
          label.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
          const needsApproval = mode === 'data' || mode === 'app';
          await waitFor(() => {
            const row = userInput.closest('.ctox-context-user-row');
            const visible = row?.hidden === false && getComputedStyle(row).display !== 'none';
            return {
              ok: visible === needsApproval,
              hidden: row?.hidden ?? null,
              display: getComputedStyle(row).display,
            };
          }, 5000, `threads right-click ${mode} delegation state`);
          if (needsApproval) {
            userInput.value = userId;
            userInput.dispatchEvent(new Event('input', { bubbles: true }));
          }
          textarea.value = message;
          textarea.dispatchEvent(new Event('input', { bubbles: true }));
          form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
          await waitFor(() => ({
            ok: !document.querySelector('.ctox-global-context-menu:not([hidden])'),
            status: document.querySelector('.ctox-global-context-menu .ctox-context-status')?.textContent?.trim() || '',
          }), 70000, `threads right-click ${mode} submit accepted`);
          const commandCollection = state.db?.raw?.business_commands || state.db?.collection?.('business_commands');
          return waitFor(async () => {
            const expectedCommandType = needsApproval
              ? 'threads.ctox_approval.request'
              : 'business_os.context.ask';
            const docs = docsToJson(await commandCollection.find({
              selector: { command_type: expectedCommandType },
              sort: [{ updated_at_ms: 'desc' }],
              limit: 50,
            }).exec());
            const command = docs.find((doc) => {
                if (needsApproval) {
                  return doc.command_type === 'threads.ctox_approval.request'
                    && doc.payload?.prompt === message
                    && doc.payload?.reviewer_user_id === userId
                    && doc.payload?.target_command_type === (mode === 'app'
                      ? 'ctox.business_os.app.modify'
                      : 'business_os.data.modify')
                    && doc.payload?.source_context?.record_id === contextRecordId;
                }
                return doc.command_type === 'business_os.context.ask'
                  && doc.record_id === contextRecordId;
              });
            return {
              ok: Boolean(command),
              command: command || null,
              commandCount: docs.length,
              sample: command ? null : (docs[0] || null),
            };
          }, 30000, `threads right-click ${mode} command persisted`);
        };

        try {
          await ensureThreadsModuleCollections();
          await waitFor(() => {
            const raw = state.db?.raw || {};
            const names = [
              'business_commands',
              'business_users',
              ...threadsCollections,
            ];
            return {
              ok: names.every((name) => Boolean(raw[name])),
              missing: names.filter((name) => !raw[name]),
            };
          }, 30000, 'threads right-click collections available');
          await Promise.all([
            'business_commands',
            'business_users',
            ...threadsCollections,
            'ctox_queue_tasks',
          ].map((name) => state.sync?.startCollection?.(name).catch(() => null)));

          await applyThreadsSmokeState(requesterSession, threadsRightClickCapabilities?.requester);
          const deniedCommandId = `cmd_${crypto.randomUUID()}`;
          let deniedDispatchError = '';
          try {
            await state.commandBus.dispatch({
              id: deniedCommandId,
              module: targetModule.id,
              command_type: 'business_os.data.modify',
              record_id: targetRecordId,
              inbound_channel: targetModule.id,
              payload: {
                prompt: `Denied direct data change ${Date.now()}`,
                instruction: 'This direct mutation must be denied before delegation.',
                context: {
                  module: targetModule.id,
                  record_type: 'smoke-record',
                  record_id: targetRecordId,
                  label: 'Threads Right-Click Smoke Record',
                },
              },
              client_context: {
                action: 'context-data-modify-direct-denial-smoke',
                module: targetModule.id,
                module_id: targetModule.id,
                app_id: targetModule.id,
                actor: requesterSession.user,
                record_id: targetRecordId,
              },
            }, { until: 'local' });
          } catch (error) {
            deniedDispatchError = String(error?.message || error);
          }
          const deniedDirectCommand = await waitFor(async () => {
            const docs = docsToJson(await state.db.raw.business_commands.find({
              selector: { command_id: deniedCommandId },
              limit: 5,
            }).exec());
            const command = docs.find((item) => (
              item.command_id === deniedCommandId || item.id === deniedCommandId
            )) || null;
            const serialized = JSON.stringify(command || {});
            return {
              ok: Boolean(command && command.status === 'failed' && serialized.includes('role_or_scope_denied')),
              command,
              deniedDispatchError,
            };
          }, 30000, 'threads right-click direct native denial');

          const dataCommand = await submitContextMode({ mode: 'data', message: dataPrompt, userId: reviewerId });
          const askCommand = await submitContextMode({ mode: 'ask', message: askPrompt, userId: reviewerId });
          const appCommand = await submitContextMode({
            mode: 'app',
            message: appPrompt,
            userId: reviewerId,
            contextRecordId: appTargetRecordId,
          });
          const contextCaptured = [
            [dataCommand, targetRecordId],
            [askCommand, targetRecordId],
            [appCommand, appTargetRecordId],
          ].every(([entry, expectedRecordId]) => {
            const command = entry.command || {};
            const context = command.payload?.source_context || {};
            const contextV2 = context.context_v2 || command.client_context?.context || {};
            const pointer = contextV2.pointer || {};
            return command.record_id === expectedRecordId
              && (context.record_id === expectedRecordId || contextV2.entity?.id === expectedRecordId)
              && Number.isFinite(pointer.x)
              && Number.isFinite(pointer.y);
          });
          if (!contextCaptured) {
            throw new Error('threads right-click commands lost their exact record or pointer context');
          }
          const commandStatus = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['business_commands', 'business_users'],
          });
          if (commandStatus?.version !== 'business-os-advanced-status-v1' || commandStatus.ok !== true) {
            throw new Error(`threads right-click command status unhealthy: ${JSON.stringify(commandStatus)}`);
          }
          const rawDb = state.db.raw;
          const expectedThreadId = `thread_${targetModule.id}_smoke-record_${targetRecordId}`;
          const projectionUpdatedAfterMs = Date.now() - 5 * 60 * 1000;
          let projectionPollAttempt = 0;
          let projectedThread = null;
          let projectedMessages = [];
          let projectedNotifications = [];
          let projectedApprovals = [];
          let projectedAppApprovals = [];
          const projections = await waitFor(async () => {
            const currentProjectionAttempt = projectionPollAttempt++;
            const threadDocs = projectedThread ? [projectedThread] : docsToJson(await rawDb.user_threads.find({
              // Demand-query windows are intentionally stale-while-revalidate.
              // Vary the lower bound so a previously completed empty window
              // cannot mask a projection created just after the first poll.
              selector: {
                id: expectedThreadId,
                updated_at_ms: { $gte: projectionUpdatedAfterMs + currentProjectionAttempt },
              },
              limit: 1,
            }).exec());
            const thread = threadDocs.find((item) => item.id === expectedThreadId) || null;
            if (thread) projectedThread = thread;
            const relatedUpdatedAfterMs = projectionUpdatedAfterMs + currentProjectionAttempt;
            const relatedQuery = thread ? {
              selector: {
                thread_id: thread.id,
                updated_at_ms: { $gte: relatedUpdatedAfterMs },
              },
              sort: [{ updated_at_ms: 'desc' }],
              limit: 50,
            } : null;
            const [messages, notifications, approvals, appApprovals] = relatedQuery
              ? await Promise.all([
                projectedMessages.length
                  ? projectedMessages
                  : rawDb.user_thread_messages.find(relatedQuery).exec().then(docsToJson),
                projectedNotifications.length
                  ? projectedNotifications
                  : rawDb.user_notifications.find(relatedQuery).exec().then(docsToJson),
                projectedApprovals.length
                  ? projectedApprovals
                  : rawDb.ctox_task_approval_requests.find(relatedQuery).exec().then(docsToJson),
                projectedAppApprovals.length
                  ? projectedAppApprovals
                  : rawDb.ctox_task_approval_requests.find({
                  selector: {
                    source_record_id: appTargetRecordId,
                    reviewer_user_id: reviewerId,
                    status: 'pending',
                    updated_at_ms: { $gte: relatedUpdatedAfterMs },
                  },
                  sort: [{ updated_at_ms: 'desc' }],
                  limit: 20,
                }).exec().then(docsToJson),
              ])
              : [[], [], [], []];
            if (messages.length) projectedMessages = messages;
            if (notifications.length) projectedNotifications = notifications;
            if (approvals.length) projectedApprovals = approvals;
            if (appApprovals.length) projectedAppApprovals = appApprovals;
            const threadMessages = thread
              ? messages.filter((item) => item.thread_id === thread.id)
              : [];
            const threadNotifications = thread
              ? notifications.filter((item) => item.thread_id === thread.id)
              : [];
            const threadApprovals = thread
              ? approvals.filter((item) => item.thread_id === thread.id)
              : [];
            const dataApproval = threadApprovals.find((item) => (
              item.prompt === dataPrompt
              && item.reviewer_user_id === reviewerId
              && item.status === 'pending'
            )) || null;
            const appApproval = appApprovals.find((item) => (
              item.prompt === appPrompt
              && item.reviewer_user_id === reviewerId
              && item.status === 'pending'
            )) || null;
            const reviewerNotification = threadNotifications.find((item) => item.user_id === reviewerId) || null;
            return {
              ok: Boolean(thread && dataApproval && reviewerNotification),
              thread,
              dataApproval,
              appApproval,
              reviewerNotification,
              counts: {
                threads: thread ? 1 : 0,
                messages: messages.length,
                notifications: notifications.length,
                approvals: approvals.length,
              },
            };
          }, 150000, 'threads right-click native projections');

          await applyThreadsSmokeState(reviewerSession, threadsRightClickCapabilities?.reviewer);
          await state.openModule('threads', { force: true, asModule: true });
          const rendered = await waitFor(() => {
            const root = document.querySelector('[data-threads-root]');
            const row = root?.querySelector(`[data-thread-id="${css(projections.thread.id)}"]`) || null;
            row?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
            const timeline = root?.querySelector('[data-thread-timeline]') || null;
            const dataApprovalCard = root?.querySelector(`[data-approval-id="${css(projections.dataApproval.id)}"]`) || null;
            const timelineText = timeline?.innerText || timeline?.textContent || '';
            const contextText = root?.querySelector('[data-thread-context]')?.innerText || '';
            return {
              ok: Boolean(
                root
                  && row
                  && timelineText.includes(dataPrompt)
                  && dataApprovalCard
                  && dataApprovalCard.querySelector('[data-approve-approval]')
                  && /Threads Reviewer|threads-reviewer/.test(timelineText)
              ),
              activeModule: state.activeModule?.id || '',
              hasRoot: Boolean(root),
              hasRow: Boolean(row),
              hasApprovalCard: Boolean(dataApprovalCard),
              hasApproveButton: Boolean(dataApprovalCard?.querySelector('[data-approve-approval]')),
              timelineText: timelineText.slice(0, 1200),
              contextText: contextText.slice(0, 600),
            };
          }, 30000, 'threads right-click hub render');

          const approvalButton = document.querySelector(
            `[data-approval-id="${css(projections.dataApproval.id)}"] [data-approve-approval]`,
          );
          if (!approvalButton) throw new Error('threads right-click approval button disappeared before reviewer decision');
          approvalButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
          const decisionUpdatedAfterMs = Date.now() - 5 * 60 * 1000;
          let approvalDecisionAttempt = 0;
          const approvalDecision = await waitFor(async () => {
            const approvalDocs = docsToJson(await rawDb.ctox_task_approval_requests.find({
              selector: {
                id: projections.dataApproval.id,
                updated_at_ms: { $gte: decisionUpdatedAfterMs + approvalDecisionAttempt++ },
              },
              limit: 1,
            }).exec());
            const approval = approvalDocs.find((item) => item.id === projections.dataApproval.id) || null;
            const approvedCommandId = approval?.approved_command_id || '';
            const commandDocs = approvedCommandId
              ? docsToJson(await rawDb.business_commands.find({
                selector: {
                  id: approvedCommandId,
                  updated_at_ms: { $gte: decisionUpdatedAfterMs + approvalDecisionAttempt },
                },
                limit: 1,
              }).exec())
              : [];
            const approvedCommand = commandDocs.find((item) => (
              item.command_id === approvedCommandId || item.id === approvedCommandId
            )) || null;
            const approvalLink = approvedCommand?.payload?.approval?.approval_request_id
              || approvedCommand?.client_context?.approval_request_id
              || '';
            return {
              ok: Boolean(
                approval?.status === 'approved'
                && approvedCommandId
                && approvedCommand
                && approvedCommand.status !== 'failed'
                && approvalLink === projections.dataApproval.id
              ),
              approval,
              approvedCommand,
              approvalLink,
            };
          }, 60000, 'threads reviewer decision and approved target reauthorization');

          const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: [
              'business_commands',
              'business_users',
              'user_threads',
              'user_thread_messages',
              'user_notifications',
              'ctox_task_approval_requests',
            ],
          });
          if (status?.version !== 'business-os-advanced-status-v1') {
            throw new Error(`threads right-click UI smoke lost advanced status evidence: ${JSON.stringify(status)}`);
          }
          const requiredInitialSyncEntries = Array.isArray(status?.sync?.initialSync?.entries)
            ? status.sync.initialSync.entries
            : [];
          const incompleteInitialSync = requiredInitialSyncEntries.filter((entry) => (
            entry?.state !== 'complete'
            || !entry?.initialReplicationAt
            || entry?.checkpointEpochAdvertised !== true
            || !entry?.checkpointEpoch
          ));
          const missingRequiredCollections = Array.isArray(status?.sync?.missingRequiredCollections)
            ? status.sync.missingRequiredCollections
            : [];
          const unhealthyFrameCollections = Array.isArray(status?.sync?.frameTransport?.unhealthyCollections)
            ? status.sync.frameTransport.unhealthyCollections
            : [];
          if (
            status.ok !== true
            || Number(status.health?.errorTotal || 0) !== 0
            || missingRequiredCollections.length
            || incompleteInitialSync.length
            || unhealthyFrameCollections.length
          ) {
            throw new Error(`threads right-click UI smoke advanced status target collections unhealthy: ${JSON.stringify({
              ok: status.ok,
              health: status.health || null,
              missingRequiredCollections,
              incompleteInitialSync,
              unhealthyFrameCollections,
              requiredCollections: status.sync?.requiredCollections || null,
            }, null, 2)}`);
          }

          const scale = threadsScaleSeed || {};
          const scaleBudgetPassed = !threadsScaleSeed || (Number(scale.commands || 0) >= 10000
            && Number(scale.threads || 0) >= 10000
            && Number(scale.messages || 0) >= 10000
            && Number(scale.notifications || 0) >= 10000
            && Number(scaleFirstRenderEvidence?.visibleThreadRows || 0) >= 1
            && Number(scaleFirstRenderEvidence?.visibleThreadRows || 0) <= 200
            && Number(scaleFirstRenderEvidence?.renderMs || 0) <= 30000);
          if (threadsScaleSeed && !scaleBudgetPassed) {
            throw new Error(`threads right-click scale budget failed: ${JSON.stringify({
              scale,
              scaleFirstRenderEvidence,
            }, null, 2)}`);
          }

          return {
            mode: smokeMode,
            targetModuleId: targetModule.id,
            reviewerId,
            threadId: projections.thread.id,
            dataCommandId: dataCommand.command?.command_id || dataCommand.command?.id || '',
            askCommandId: askCommand.command?.command_id || askCommand.command?.id || '',
            appCommandId: appCommand.command?.command_id || appCommand.command?.id || '',
            dataApprovalCommandPersisted: dataCommand.command?.command_type === 'threads.ctox_approval.request',
            directDenialCommandId: deniedCommandId,
            directDenialReason: deniedDirectCommand.command?.result?.reason_code
              || deniedDirectCommand.command?.result?.decision?.reason_code
              || 'role_or_scope_denied',
            dataApprovalId: projections.dataApproval.id || '',
            appApprovalId: projections.appApproval?.id || '',
            dataApprovalProjected: Boolean(projections.dataApproval),
            askCommandPersisted: Boolean(askCommand.command),
            appApprovalCommandPersisted: appCommand.command?.command_type === 'threads.ctox_approval.request',
            appApprovalProjected: Boolean(projections.appApproval),
            sourceContextCaptured: [
              [dataCommand, targetRecordId],
              [askCommand, targetRecordId],
              [appCommand, appTargetRecordId],
            ].every(([entry, expectedRecordId]) => {
              const command = entry.command || {};
              const context = command.payload?.source_context || {};
              const contextV2 = context.context_v2 || command.client_context?.context || {};
              const pointer = contextV2.pointer || {};
              return command.record_id === expectedRecordId
                && (context.record_id === expectedRecordId || contextV2.entity?.id === expectedRecordId)
                && Number.isFinite(pointer.x)
                && Number.isFinite(pointer.y);
            }),
            reviewerNotificationProjected: Boolean(projections.reviewerNotification),
            hubRendered: rendered.ok === true,
            approvalActionRendered: rendered.hasApproveButton === true,
            approvalDecision: approvalDecision.approval?.status || '',
            approvedCommandId: approvalDecision.approval?.approved_command_id || '',
            approvedCommandStatus: approvalDecision.approvedCommand?.status || '',
            reauthorizationLinked: approvalDecision.approvalLink === projections.dataApproval.id,
            authState: 'authenticated',
            browserContext: 'clean',
            tenantScope: 'local-workspace',
            actorRole: requesterSession.user.role,
            reviewerRole: reviewerSession.user.role,
            reviewerPickerVisible: reviewerPickerEvidence.some((item) => item.visible),
            reviewerPickerEvidence,
            scaleCommands: Number(scale.commands || 0),
            scaleThreads: Number(scale.threads || 0),
            scaleMessages: Number(scale.messages || 0),
            scaleNotifications: Number(scale.notifications || 0),
            scaleVisibleThreadRows: Number(scaleFirstRenderEvidence?.visibleThreadRows || 0),
            scaleFirstRenderMs: Number(scaleFirstRenderEvidence?.renderMs || 0),
            scaleBudgetPassed,
            advancedStatusVersion: status.version || '',
            advancedStatusRuntime: status.rxdbRuntime || null,
          };
        } finally {
          state.modules = originalState.modules;
          state.taskbarPins = originalState.taskbarPins;
          state.moduleAllowlist = originalState.moduleAllowlist;
          state.session = originalState.session;
          state.governance = originalState.governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = originalState.globalSession;
          if (originalState.bodyAuthState) {
            document.body.dataset.authState = originalState.bodyAuthState;
          } else {
            delete document.body.dataset.authState;
          }
          smoke.renderTabs();
        }
      }

      async function runBusinessOsDynamicAppsUiSmoke() {
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            last = await predicate();
            if (last?.ok) return last;
            await delay(100);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/["\\]/g, '\\$&');
        };
        const smoke = globalThis.ctoxBusinessOsSmoke;
        const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state || appState;
        if (!state) throw new Error('Business OS app state is unavailable for dynamic apps UI smoke');
        if (typeof smoke?.renderTabs !== 'function') throw new Error('Business OS smoke renderTabs hook is unavailable');
        if (typeof smoke?.openAppLifecycleDrawer !== 'function') throw new Error('Business OS smoke lifecycle drawer hook is unavailable');
        if (typeof smoke?.createLiveDbFacade !== 'function') throw new Error('Business OS smoke DB facade hook is unavailable');
        if (typeof smoke?.createModuleContext !== 'function') throw new Error('Business OS smoke module context hook is unavailable');
        appState = state;

        const [
          permissionsMod,
          lifecycleMod,
        ] = await Promise.all([
          import('/shared/permissions.js'),
          import('/shared/app-lifecycle.js'),
        ]);
        const { BusinessOsPermissions } = permissionsMod;
        const {
          appLifecycleBadge,
          appLifecycleState,
          canSeeModuleForAppVersion,
        } = lifecycleMod;
        const privateModule = {
          id: 'phase8-private-app',
          title: 'Phase 8 Private App',
          glyph: 'P8',
          version: '0.1.0',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase8-private-app/index.js',
          collections: ['business_commands'],
        };
        const teamModule = {
          id: 'phase8-team-app',
          title: 'Phase 8 Team App',
          glyph: 'T8',
          version: '1.0.0',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase8-team-app/index.js',
          collections: ['business_commands'],
        };
        const invalidModule = {
          id: 'phase8-invalid-app',
          title: 'Phase 8 Invalid Version App',
          glyph: 'I8',
          version: 'beta',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase8-invalid-app/index.js',
          collections: ['business_commands'],
        };
        const restrictedModule = {
          id: 'phase8-restricted-app',
          title: 'Phase 8 Restricted Team App',
          glyph: 'R8',
          version: '1.0.0',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase8-restricted-app/index.js',
          collections: ['business_commands'],
          lifecycle: { visibility_state: 'restricted' },
        };
        const openModuleFixtureId = 'phase13-open-module-guard';
        const allPermissions = Object.values(BusinessOsPermissions);
        const governance = {
          founders: {
            [privateModule.id]: [{ user_id: 'app_builder', active: true }],
            [teamModule.id]: [{ user_id: 'app_builder', active: true }],
            [invalidModule.id]: [{ user_id: 'app_builder', active: true }],
            [restrictedModule.id]: [{ user_id: 'app_builder', active: true }],
            [openModuleFixtureId]: [{ user_id: 'app_builder', active: true }],
          },
          permission_model: {
            version: 1,
            deny_supported: false,
            role_defaults: {
              chef: {
                workspace: allPermissions,
                module: allPermissions,
                assigned_module: allPermissions,
              },
              admin: {
                workspace: allPermissions,
                module: allPermissions,
                assigned_module: allPermissions,
              },
              founder: {
                workspace: [],
                module: [],
                assigned_module: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                  BusinessOsPermissions.AppsRelease,
                  BusinessOsPermissions.DataRead,
                  BusinessOsPermissions.DataWrite,
                ],
              },
              user: {
                workspace: [],
                module: [],
                assigned_module: [],
              },
            },
            module_assignments: {
              [privateModule.id]: {
                app_builder: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                  BusinessOsPermissions.DataRead,
                  BusinessOsPermissions.DataWrite,
                ],
              },
              [teamModule.id]: {
                app_builder: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                  BusinessOsPermissions.DataRead,
                  BusinessOsPermissions.DataWrite,
                ],
              },
              [invalidModule.id]: {
                app_builder: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                  BusinessOsPermissions.DataRead,
                  BusinessOsPermissions.DataWrite,
                ],
              },
              [restrictedModule.id]: {
                app_builder: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                  BusinessOsPermissions.DataRead,
                  BusinessOsPermissions.DataWrite,
                ],
              },
              [openModuleFixtureId]: {
                app_builder: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                  BusinessOsPermissions.DataRead,
                  BusinessOsPermissions.DataWrite,
                ],
              },
            },
            explicit_grants: [],
          },
        };
        const sessionFor = (id, role) => ({ user: { id, role } });
        const teamSession = sessionFor('team_member', 'user');
        const builderSession = sessionFor('app_builder', 'founder');
        const insertedIds = new Set([privateModule.id, teamModule.id, invalidModule.id, restrictedModule.id]);
        const temporaryVisibleIds = [privateModule.id, teamModule.id, invalidModule.id, restrictedModule.id, openModuleFixtureId];
        const originalState = {
          modules: Array.isArray(state.modules) ? [...state.modules] : [],
          taskbarPins: Array.isArray(state.taskbarPins) ? [...state.taskbarPins] : [],
          moduleAllowlist: Array.isArray(state.moduleAllowlist) ? [...state.moduleAllowlist] : state.moduleAllowlist,
          session: state.session,
          governance: state.governance,
          globalSession: globalThis.CTOX_BUSINESS_OS_SESSION,
        };
        const installSmokeModules = (session) => {
          state.session = session;
          state.governance = governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = session;
          state.modules = [
            ...originalState.modules.filter((mod) => !insertedIds.has(mod?.id)),
            privateModule,
            teamModule,
            invalidModule,
            restrictedModule,
          ];
          state.moduleAllowlist = [...new Set([
            ...(Array.isArray(state.moduleAllowlist)
              ? state.moduleAllowlist.map((id) => String(id || '').trim()).filter(Boolean)
              : []),
            ...temporaryVisibleIds,
          ])];
          state.taskbarPins = [privateModule.id, teamModule.id, invalidModule.id, restrictedModule.id];
          smoke.renderTabs();
        };
        const tabEvidence = () => {
          const privateTab = document.querySelector(`.module-tab[data-target="${css(privateModule.id)}"]`);
          const teamTab = document.querySelector(`.module-tab[data-target="${css(teamModule.id)}"]`);
          const invalidTab = document.querySelector(`.module-tab[data-target="${css(invalidModule.id)}"]`);
          const restrictedTab = document.querySelector(`.module-tab[data-target="${css(restrictedModule.id)}"]`);
          const privateBadge = document.querySelector(`[data-app-lifecycle-badge="${css(privateModule.id)}"]`);
          const teamBadge = document.querySelector(`[data-app-lifecycle-badge="${css(teamModule.id)}"]`);
          const invalidBadge = document.querySelector(`[data-app-lifecycle-badge="${css(invalidModule.id)}"]`);
          const restrictedBadge = document.querySelector(`[data-app-lifecycle-badge="${css(restrictedModule.id)}"]`);
          return {
            privateTabVisible: Boolean(privateTab),
            teamTabVisible: Boolean(teamTab),
            invalidTabVisible: Boolean(invalidTab),
            restrictedTabVisible: Boolean(restrictedTab),
            taskbarPins: Array.isArray(state.taskbarPins) ? [...state.taskbarPins] : [],
            launchTargetIds: typeof smoke.listLaunchTargets === 'function'
              ? smoke.listLaunchTargets().map((target) => target?.id).filter(Boolean)
              : [],
            privateBadgeState: privateBadge?.getAttribute('data-state') || '',
            privateBadgeText: privateBadge?.textContent?.trim() || '',
            teamBadgeState: teamBadge?.getAttribute('data-state') || '',
            teamBadgeText: teamBadge?.textContent?.trim() || '',
            invalidBadgeState: invalidBadge?.getAttribute('data-state') || '',
            invalidBadgeText: invalidBadge?.textContent?.trim() || '',
            restrictedBadgeState: restrictedBadge?.getAttribute('data-state') || '',
            restrictedBadgeText: restrictedBadge?.textContent?.trim() || '',
          };
        };
        const openStartMenu = async () => {
          const startButton = document.querySelector('[data-shell-start]');
          if (!startButton) throw new Error('Business OS start menu button is missing for dynamic app smoke');
          document.querySelector('.shell-start-menu-panel')?.classList.remove('is-active');
          startButton.click();
          return waitFor(() => {
            const panel = document.querySelector('.shell-start-menu-panel');
            const items = panel ? [...panel.querySelectorAll('.start-menu-item')] : [];
            return {
              ok: Boolean(panel?.classList?.contains('is-active') && items.length > 0),
              itemCount: items.length,
              labels: items.map((item) => item.querySelector('.start-menu-item-label')?.textContent?.trim() || '').slice(0, 12),
            };
          }, 5000, 'dynamic app start menu open');
        };
        const launcherEvidence = () => {
          const panel = document.querySelector('.shell-start-menu-panel');
          const privateBadge = panel?.querySelector(`.start-menu-lifecycle-badge[data-module-lifecycle="${css(privateModule.id)}"]`);
          const teamBadge = panel?.querySelector(`.start-menu-lifecycle-badge[data-module-lifecycle="${css(teamModule.id)}"]`);
          const invalidBadge = panel?.querySelector(`.start-menu-lifecycle-badge[data-module-lifecycle="${css(invalidModule.id)}"]`);
          const restrictedBadge = panel?.querySelector(`.start-menu-lifecycle-badge[data-module-lifecycle="${css(restrictedModule.id)}"]`);
          return {
            privateLauncherBadgeState: privateBadge?.getAttribute('data-state') || '',
            privateLauncherBadgeText: privateBadge?.textContent?.trim()?.replace(/\s+/g, ' ') || '',
            teamLauncherBadgeState: teamBadge?.getAttribute('data-state') || '',
            teamLauncherBadgeText: teamBadge?.textContent?.trim()?.replace(/\s+/g, ' ') || '',
            invalidLauncherBadgeState: invalidBadge?.getAttribute('data-state') || '',
            restrictedLauncherBadgeState: restrictedBadge?.getAttribute('data-state') || '',
            labels: panel ? [...panel.querySelectorAll('.start-menu-item-label')].map((node) => node.textContent?.trim() || '') : [],
          };
        };
        const whyDiagnosticsEvidence = (moduleId) => {
          const root = document.querySelector(`[data-why-diagnostics="${css(moduleId)}"]`);
          const rows = root ? [...root.querySelectorAll('[data-why-row]')].map((node) => ({
            key: node.getAttribute('data-why-row') || '',
            state: node.getAttribute('data-decision-state') || '',
            text: node.textContent?.trim().replace(/\s+/g, ' ') || '',
          })) : [];
          const dataRows = root ? [...root.querySelectorAll('[data-why-data-row]')].map((node) => ({
            collection: node.getAttribute('data-why-data-row') || '',
            text: node.textContent?.trim().replace(/\s+/g, ' ') || '',
          })) : [];
          return {
            visible: Boolean(root),
            rows,
            rowKeys: rows.map((row) => row.key),
            dataRows,
            dataCollections: dataRows.map((row) => row.collection),
            text: root?.textContent?.trim().replace(/\s+/g, ' ').slice(0, 600) || '',
          };
        };
        const installSmokeModulesAndWait = async (session, label, predicate) => waitFor(() => {
          installSmokeModules(session);
          const tabs = tabEvidence();
          return {
            ok: predicate(tabs),
            ...tabs,
          };
        }, 8000, `dynamic app ${label} tabs`);
        const assertDenied = async (label, callback) => {
          try {
            await callback();
          } catch (error) {
            if (error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED') return true;
            throw new Error(`${label} failed with unexpected error: ${error?.stack || error?.message || error}`);
          }
          throw new Error(`${label} unexpectedly allowed`);
        };
        let result = null;
        let tamperedScopedTaskbarPinsKey = '';
        let storedScopedTaskbarPins = null;
        try {
          const helperPrivateHiddenForTeam = !canSeeModuleForAppVersion(privateModule, {
            session: teamSession,
            governance,
          });
          const helperPrivateVisibleForBuilder = canSeeModuleForAppVersion(privateModule, {
            session: builderSession,
            governance,
          });
          const helperTeamVisibleForTeam = canSeeModuleForAppVersion(teamModule, {
            session: teamSession,
            governance,
          });
          const invalidLifecycle = appLifecycleState(invalidModule, {
            session: teamSession,
            governance,
          });
          const invalidVersionPrivate = invalidLifecycle.state === 'private'
            && invalidLifecycle.warning === true
            && !canSeeModuleForAppVersion(invalidModule, { session: teamSession, governance });
          const restrictedLifecycle = appLifecycleBadge(restrictedModule, {
            session: builderSession,
            governance,
          });
          const restrictedHiddenForTeam = !canSeeModuleForAppVersion(restrictedModule, {
            session: teamSession,
            governance,
          });
          const privateLifecycle = appLifecycleBadge(privateModule, {
            session: builderSession,
            governance,
          });
          const teamLifecycle = appLifecycleBadge(teamModule, {
            session: teamSession,
            governance,
          });

          const teamTabs = await installSmokeModulesAndWait(
            teamSession,
            'team actor',
            (tabs) => !tabs.privateTabVisible
              && tabs.teamTabVisible
              && tabs.teamBadgeState === 'team'
              && !tabs.restrictedTabVisible
              && !tabs.invalidTabVisible,
          );
          const builderTabs = await installSmokeModulesAndWait(
            builderSession,
            'builder actor',
            (tabs) => tabs.privateTabVisible
              && tabs.teamTabVisible
              && tabs.invalidTabVisible
              && tabs.restrictedTabVisible,
          );
          await openStartMenu();
          const builderLauncher = launcherEvidence();
          const privateLauncherBadge = document.querySelector(`.shell-start-menu-panel .start-menu-lifecycle-badge[data-module-lifecycle="${css(privateModule.id)}"]`);
          if (!privateLauncherBadge) throw new Error(`private launcher lifecycle badge missing for ${privateModule.id}`);
          privateLauncherBadge.click();
          const managerDrawer = await waitFor(() => {
            const element = document.querySelector('.module-lifecycle-drawer');
            const text = element?.innerText || '';
            const why = whyDiagnosticsEvidence(privateModule.id);
            return {
              ok: Boolean(
                element
                && element.dataset.lifecyclePermissionState === 'manager'
                && /Verwalten erlaubt/.test(text)
                && /App ändern/.test(text)
                && /Im App Store verwalten/.test(text)
                && why.visible
                && ['actor', 'visibility', 'open', 'modify', 'source', 'release', 'rollback', 'data']
                  .every((key) => why.rowKeys.includes(key))
                && why.dataCollections.includes('business_commands')
                && why.dataRows.some((row) => /Lesen: Nicht erlaubt/.test(row.text) && /Schreiben: Nicht erlaubt/.test(row.text))
              ),
              state: element?.dataset?.lifecyclePermissionState || '',
              text,
              why,
            };
          }, 5000, 'dynamic app manager lifecycle drawer');
          document.querySelector('[data-close-lifecycle]')?.dispatchEvent(new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
          }));
          const teamTabsForLauncher = await installSmokeModulesAndWait(
            teamSession,
            'team actor launcher',
            (tabs) => !tabs.privateTabVisible && tabs.teamTabVisible && tabs.teamBadgeState === 'team',
          );
          await openStartMenu();
          const teamLauncher = launcherEvidence();
          const teamLauncherBadge = document.querySelector(`.shell-start-menu-panel .start-menu-lifecycle-badge[data-module-lifecycle="${css(teamModule.id)}"]`);
          if (!teamLauncherBadge) throw new Error(`team launcher lifecycle badge missing for ${teamModule.id}`);
          teamLauncherBadge.click();
          const readonlyDrawer = await waitFor(() => {
            const element = document.querySelector('.module-lifecycle-drawer');
            const text = element?.innerText || '';
            const why = whyDiagnosticsEvidence(teamModule.id);
            const modifyRow = why.rows.find((row) => row.key === 'modify') || {};
            const releaseRow = why.rows.find((row) => row.key === 'release') || {};
            const hasEditAction = Boolean(element?.querySelector('[data-edit-lifecycle-app]'));
            return {
              ok: Boolean(
                element
                && element.dataset.lifecyclePermissionState === 'readonly'
                && /Nur Ansicht/.test(text)
                && /Details im App Store ansehen/.test(text)
                && !hasEditAction
                && why.visible
                && why.rowKeys.includes('visibility')
                && why.rowKeys.includes('data')
                && why.dataCollections.includes('business_commands')
                && modifyRow.state === 'blocked'
                && releaseRow.state === 'blocked'
              ),
              state: element?.dataset?.lifecyclePermissionState || '',
              text,
              why,
            };
          }, 5000, 'dynamic app readonly lifecycle drawer');
          document.querySelector('[data-close-lifecycle]')?.dispatchEvent(new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
          }));
          installSmokeModules(builderSession);
          const privateBadge = document.querySelector(`[data-app-lifecycle-badge="${css(privateModule.id)}"]`);
          if (!privateBadge) throw new Error(`private lifecycle badge missing for ${privateModule.id}`);
          privateBadge.dispatchEvent(new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
          }));
          const drawer = await waitFor(() => {
            const element = document.querySelector('.module-lifecycle-drawer');
            const text = element?.innerText || '';
            return {
              ok: Boolean(element && /Phase 8 Private App/.test(text) && /Privat/.test(text) && /v0\.1\.0/.test(text)),
              text,
            };
          }, 5000, 'dynamic app lifecycle drawer');

          const collectionName = 'business_commands';
          await waitFor(() => {
            let collection = null;
            try {
              collection = state.db?.collection?.(collectionName) || state.db?.raw?.[collectionName] || null;
            } catch {
              collection = null;
            }
            return {
              ok: Boolean(collection),
              dbMode: state.db?.mode || '',
              collections: Object.keys(state.db?.collections || state.db?.raw || {}).slice(0, 12),
            };
          }, 15000, 'dynamic app data collection');

          const persistedOpenModule = state.modules.find((mod) => mod?.id === openModuleFixtureId);
          if (!persistedOpenModule) {
            throw new Error(`dynamic app persisted openModule fixture missing after reload: ${JSON.stringify({
              expected: openModuleFixtureId,
              modules: state.modules.map((mod) => mod?.id).filter(Boolean),
            })}`);
          }
          if (typeof state.openModule !== 'function') {
            throw new Error('Business OS state.openModule is unavailable for dynamic app persisted fixture');
          }
          state.session = teamSession;
          state.governance = governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = teamSession;
          delete globalThis.__ctoxPhase13OpenModuleGuard;
          await state.openModule(persistedOpenModule.id, { force: true, asModule: true });
          const openModuleGuard = await waitFor(() => {
            const guard = globalThis.__ctoxPhase13OpenModuleGuard;
            return {
              ok: Boolean(
                guard?.mounted
                && guard.moduleId === persistedOpenModule.id
                && guard.denied?.collection === true
                && guard.denied?.property === true
                && guard.denied?.cached === true
                && guard.denied?.raw === true
                && Object.values(guard.runtimeSafety || {}).every((value) => value === true)
              ),
              guard: guard || null,
              activeModule: state.activeModule?.id || '',
              marker: document.querySelector('[data-phase13-open-module-guard]')?.getAttribute('data-phase13-open-module-guard') || '',
            };
          }, 8000, 'dynamic app persisted openModule guarded DB fixture');
          const openModuleReloadMounted = openModuleGuard.guard?.moduleId === persistedOpenModule.id
            && openModuleGuard.marker === persistedOpenModule.id;
          const openModuleCollectionDenied = openModuleGuard.guard?.denied?.collection === true;
          const openModulePropertyDenied = openModuleGuard.guard?.denied?.property === true;
          const openModuleCachedDenied = openModuleGuard.guard?.denied?.cached === true;
          const openModuleRawDenied = openModuleGuard.guard?.denied?.raw === true;
          const runtimeSafety = openModuleGuard.guard?.runtimeSafety || {};
          const openModuleRuntimeSafetyContract = runtimeSafety.contract === true
            && runtimeSafety.trustModel === true
            && runtimeSafety.guardedDb === true;
          const openModuleRuntimeSafetyCapabilities = runtimeSafety.localAssetFetchOnly === true
            && runtimeSafety.dynamicImportForbidden === true
            && runtimeSafety.storageNonAuthoritative === true
            && runtimeSafety.shellGlobalsForbidden === true
            && runtimeSafety.workersForbidden === true
            && runtimeSafety.externalEffectsChatOnly === true;

          state.session = teamSession;
          state.governance = governance;
          const teamDb = smoke.createLiveDbFacade(teamModule);
          const realContext = smoke.createModuleContext(teamModule);
          const realContextDb = realContext?.db;
          if (!realContextDb?.collection) {
            throw new Error('dynamic app real module context did not expose ctx.db.collection');
          }
          const realContextCachedCollection = realContextDb.collection(collectionName);
          const dbReadDenied = await assertDenied('dynamic app read without data.read', () => {
            teamDb.collection(collectionName).findOne('phase8_dynamic_apps_smoke');
          });
          const dbRawDenied = await assertDenied('dynamic app raw read without data.read', () => {
            teamDb.raw[collectionName].findOne('phase8_dynamic_apps_smoke');
          });
          const realContextCollectionDenied = await assertDenied('dynamic app real context collection read without data.read', () => {
            realContextDb.collection(collectionName).findOne('phase8_dynamic_apps_smoke');
          });
          const realContextPropertyDenied = await assertDenied('dynamic app real context property read without data.read', () => {
            realContextDb[collectionName].findOne('phase8_dynamic_apps_smoke');
          });
          const realContextCachedDenied = await assertDenied('dynamic app real context cached handle read without data.read', () => {
            realContextCachedCollection.findOne('phase8_dynamic_apps_smoke');
          });
          const realContextRawDenied = await assertDenied('dynamic app real context raw read without data.read', () => {
            realContextDb.raw[collectionName].findOne('phase8_dynamic_apps_smoke');
          });
          governance.permission_model.explicit_grants.push({
            grant_id: 'phase8_dynamic_read_business_commands',
            subject_type: 'user',
            subject_id: teamSession.user.id,
            permission: BusinessOsPermissions.DataRead,
            scope_type: 'collection',
            scope_id: collectionName,
            active: true,
          });
          state.governance = governance;
          const readQuery = smoke.createLiveDbFacade(teamModule)
            .collection(collectionName)
            .findOne('phase8_dynamic_apps_smoke');
          const dbReadGrantAllowed = Boolean(readQuery && typeof readQuery.exec === 'function');
          const realContextCachedReadQuery = realContextCachedCollection.findOne('phase8_dynamic_apps_smoke');
          const realContextCachedReadGrantAllowed = Boolean(
            realContextCachedReadQuery && typeof realContextCachedReadQuery.exec === 'function'
          );
          const dbWriteDeniedWithoutWrite = await assertDenied('dynamic app write without data.write', () => {
            return smoke.createLiveDbFacade(teamModule)
              .collection(collectionName)
              .insert({
                id: 'phase8_dynamic_apps_smoke',
                command_id: 'phase8_dynamic_apps_smoke',
                module: teamModule.id,
                command_type: 'phase8.dynamic.write',
                status: 'pending_sync',
                payload: {},
                client_context: { source: 'phase8-dynamic-apps-ui-smoke' },
                updated_at_ms: Date.now(),
              });
          });
          const permissionFacade = smoke.createModulePermissionFacade(teamModule);
          const facadeReadAllowed = permissionFacade.canReadCollection(collectionName) === true;
          const facadeWriteDenied = permissionFacade.canWriteCollection(collectionName) === false;

          const packagedGuardSpecs = Object.freeze([
            {
              id: 'coding-agents',
              title: 'Coding Agents',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/coding-agents/index.html',
              schema: '/modules/coding-agents/schema.js',
              collection: 'coding_agent_sessions',
              collections: [
                'business_commands',
                'coding_agent_workspace_grants',
                'coding_agent_sessions',
                'coding_agent_events',
              ],
            },
            {
              id: 'calendar',
              title: 'Kalender',
              source: 'packaged',
              install_scope: 'starter',
              entry: 'modules/calendar/index.html',
              schema: '/modules/calendar/schema.js',
              collection: 'calendar_events',
              collections: [
                'business_commands',
                'calendar_sources',
                'calendar_calendars',
                'calendar_events',
                'calendar_event_instances',
                'calendar_availability_rules',
                'calendar_booking_pages',
                'calendar_booking_holds',
                'calendar_bookings',
              ],
            },
            {
              id: 'buchhaltung',
              title: 'Buchhaltung',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/buchhaltung/index.html',
              schema: '/modules/buchhaltung/schema.js',
              collection: 'accounting_journal_entries',
              collections: [
                'business_commands',
                'accounting_accounts',
                'accounting_journal_entries',
                'accounting_journal_entry_lines',
                'accounting_ledger_entries',
                'accounting_receipts',
                'accounting_bank_statements',
                'accounting_bank_statement_lines',
                'accounting_number_series',
              ],
            },
            {
              id: 'conversations',
              title: 'Conversations',
              source: 'packaged',
              install_scope: 'store',
              entry: 'modules/conversations/index.html',
              collection: 'business_commands',
              collections: [
                'business_commands',
                'communication_accounts',
                'communication_threads',
                'communication_messages',
                'outbound_campaigns',
                'outbound_pipeline_items',
                'outbound_engagements',
                'outbound_messages',
                'outbound_approvals',
              ],
            },
            {
              id: 'customers',
              title: 'Kunden',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/customers/index.html',
              schema: '/modules/customers/schema.js',
              collection: 'customer_accounts',
              collections: [
                'business_commands',
                'customer_accounts',
                'customer_contacts',
                'customer_opportunities',
                'customer_tasks',
                'customer_notes',
                'customer_activities',
                'customer_files',
                'customer_views',
                'customer_view_filters',
                'customer_view_sorts',
                'customer_import_batches',
                'customer_dedupe_candidates',
              ],
            },
            {
              id: 'cv-print-builder',
              title: 'CV Print Builder',
              source: 'local',
              install_scope: 'local',
              entry: 'modules/cv-print-builder/index.html',
              collection: 'business_commands',
              collections: [
                'business_commands',
                'business_chats',
                'ctox_queue_tasks',
                'desktop_files',
                'desktop_file_chunks',
                'documents',
                'document_versions',
              ],
            },
            {
              id: 'documents',
              title: 'Documents',
              source: 'packaged',
              install_scope: 'starter',
              entry: 'modules/documents/index.html',
              collection: 'business_commands',
              collections: [
                'business_commands',
                'documents',
                'document_versions',
                'document_blob_chunks',
                'document_runbooks',
              ],
            },
            {
              id: 'invoices',
              title: 'Rechnungen',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/invoices/index.html',
              schema: '/modules/invoices/schema.js',
              collection: 'accounting_invoices',
              collections: [
                'business_commands',
                'customer_accounts',
                'customer_activities',
                'accounting_accounts',
                'accounting_journal_entries',
                'accounting_journal_entry_lines',
                'accounting_ledger_entries',
                'accounting_receipts',
                'accounting_bank_statement_lines',
                'accounting_number_series',
                'desktop_files',
                'desktop_file_chunks',
                'accounting_invoices',
                'accounting_invoice_lines',
                'accounting_payment_terms',
                'accounting_credit_notes',
                'accounting_payments',
                'accounting_payment_allocations',
                'accounting_dunning_runs',
                'accounting_dunning_letters',
                'accounting_recurring_invoices',
                'accounting_invoice_attachments',
                'accounting_invoice_approvals',
              ],
            },
            {
              id: 'iot',
              title: 'IoT',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/iot/index.html',
              schema: '/modules/iot/schema.js',
              collection: 'iot_widgets',
              collections: [
                'business_commands',
                'iot_realms',
                'iot_asset_types',
                'iot_assets',
                'iot_attributes',
                'iot_datapoints',
                'iot_alarms',
                'iot_agents',
                'iot_agent_status',
                'iot_rulesets',
                'iot_dashboards',
                'iot_widgets',
              ],
            },
            {
              id: 'notes',
              title: 'Notes',
              source: 'packaged',
              install_scope: 'starter',
              entry: 'modules/notes/index.html',
              schema: '/modules/notes/schema.js',
              collection: 'notes',
              collections: [
                'business_commands',
                'notes',
              ],
            },
            {
              id: 'outbound',
              title: 'Outbound',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/outbound/index.html',
              schema: '/modules/outbound/schema.js',
              collection: 'outbound_campaigns',
              collections: [
                'business_commands',
                'outbound_campaigns',
                'outbound_sources',
                'outbound_companies',
                'outbound_pipeline_items',
                'outbound_research_runs',
                'outbound_engagements',
                'outbound_messages',
                'outbound_approvals',
                'outbound_sequences',
                'outbound_sender_assignments',
                'outbound_meeting_requests',
                'outbound_suppression_entries',
                'outbound_account_limits',
                'outbound_skillbooks',
                'outbound_letter_templates',
              ],
            },
            {
              id: 'research',
              title: 'Web Research',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/research/index.html',
              schema: '/modules/research/schema.js',
              collection: 'research_tasks',
              collections: [
                'business_commands',
                'ctox_queue_tasks',
                'research_tasks',
                'research_runs',
                'research_notes',
                'knowledge_tables',
                'documents',
                'document_versions',
                'document_blob_chunks',
              ],
            },
            {
              id: 'matching',
              title: 'Matching',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/matching/index.html',
              schema: '/modules/matching/schema.js',
              collection: 'matching_requirements',
              collections: [
                'matching_requirements',
                'matching_objects',
                'matching_results',
              ],
            },
            {
              id: 'shiftflow',
              title: 'Einsatzplanung',
              source: 'local',
              install_scope: 'store',
              entry: 'modules/shiftflow/index.html',
              schema: '/modules/shiftflow/schema.js',
              collection: 'planning_shifts',
              collections: [
                'business_commands',
                'planning_employees',
                'planning_projects',
                'planning_shifts',
                'planning_time_records',
                'planning_absences',
              ],
            },
            {
              id: 'spreadsheets',
              title: 'Spreadsheets',
              source: 'packaged',
              install_scope: 'starter',
              entry: 'modules/spreadsheets/index.html',
              collection: 'business_commands',
              collections: [
                'business_commands',
                'spreadsheets',
                'spreadsheet_versions',
                'spreadsheet_blob_chunks',
                'spreadsheet_runbooks',
              ],
            },
            {
              id: 'support',
              title: 'Support',
              source: 'packaged',
              install_scope: 'store',
              entry: 'modules/support/index.html',
              collection: 'business_commands',
              collections: [
                'business_commands',
                'business_chats',
                'ctox_queue_tasks',
                'communication_threads',
                'communication_messages',
                'ctox_ticket_cases',
                'customer_accounts',
                'customer_contacts',
                'desktop_files',
                'desktop_file_chunks',
                'support_inboxes',
                'support_conversations',
                'support_thread_links',
                'support_identity_links',
                'support_notes',
                'support_conversation_events',
                'support_labels',
                'support_label_assignments',
                'support_views',
                'support_view_filters',
                'support_assignment_policies',
                'support_assignment_events',
                'support_macros',
                'support_automation_rules',
                'support_sla_policies',
                'support_applied_slas',
                'support_sla_events',
                'support_agent_requests',
                'support_agent_suggestions',
                'support_reporting_events',
                'support_reporting_rollups',
              ],
            },
          // Trusted system apps use the shell-owned scoped facade and are
          // covered by the system-scope contract/UI smoke, not this
          // grant-driven third-party app guard matrix.
          ].filter((spec) => !['documents', 'research'].includes(spec.id)));
          const launchTargets = typeof smoke.listLaunchTargets === 'function'
            ? smoke.listLaunchTargets()
            : [];
          const resolvePackagedGuardModule = (spec) => (Array.isArray(state.modules) ? state.modules : [])
            .find((module) => module?.id === spec.id)
            || launchTargets.find((module) => module?.id === spec.id)
            || {
              id: spec.id,
              title: spec.title,
              source: spec.source,
              install_scope: spec.install_scope,
              entry: spec.entry,
              collections: spec.collections,
            };
          const packagedGuardResults = [];
          for (const spec of packagedGuardSpecs) {
            const packagedGuardModule = resolvePackagedGuardModule(spec);
            const packagedGuardCollection = spec.collection;
            const subjectId = `phase13c_packaged_user_${spec.id.replace(/[^a-z0-9]+/gi, '_')}`;
            const recordId = `phase13c_packaged_guard_${spec.id.replace(/[^a-z0-9]+/gi, '_')}`;
            const packagedGuardSession = {
              user: { id: subjectId, role: 'user', name: `Phase 13C ${spec.title}` },
            };
            state.session = packagedGuardSession;
            state.governance = governance;
            if (spec.schema && !state.db?.collection?.(packagedGuardCollection)) {
              const schemaModule = await import(spec.schema);
              const missingCollections = {};
              for (const name of spec.collections) {
                if (state.db?.collection?.(name) || !schemaModule.collections?.[name]) continue;
                missingCollections[name] = { schema: schemaModule.collections[name] };
              }
              if (Object.keys(missingCollections).length) {
                await state.db?.raw?.addCollections?.(missingCollections);
              }
            }
            await waitFor(() => {
              let collection = null;
              try {
                collection = state.db?.collection?.(packagedGuardCollection) || null;
              } catch {
                collection = null;
              }
              return {
                ok: Boolean(collection),
                module_id: packagedGuardModule.id,
                collection: packagedGuardCollection,
                collections: Object.keys(state.db?.raw || {}).slice(0, 20),
              };
            }, 15000, `P13C packaged guard collection ${spec.id}`);
            await state.sync?.startCollection?.(packagedGuardCollection);
            const packagedGuardDb = smoke.createLiveDbFacade(packagedGuardModule);
            const packagedGuardContext = smoke.createModuleContext(packagedGuardModule);
            const packagedGuardContextDb = packagedGuardContext?.db;
            if (!packagedGuardContextDb?.collection) {
              throw new Error(`P13C packaged guard context did not expose ctx.db.collection for ${spec.id}`);
            }
            const packagedGuardContextPermissions = packagedGuardContext?.permissions || null;
            const packagedGuardCapabilities = packagedGuardContext?.runtimeCapabilities || {};
            const packagedGuardCapabilityContract = packagedGuardCapabilities.database?.guarded === true
              && packagedGuardCapabilities.database?.raw === 'guarded-deny-without-data-grant'
              && packagedGuardCapabilities.database?.collection_properties === 'guarded-deny-without-data-grant'
              && packagedGuardCapabilities.database?.cached_handles === 'guarded-deny-without-data-grant';
            const packagedGuardContextPermissionFacadeDenied =
              packagedGuardContextPermissions?.canReadCollection?.(packagedGuardCollection) === false
              && packagedGuardContextPermissions?.canWriteCollection?.(packagedGuardCollection) === false;
            const packagedGuardReadDenied = await assertDenied(`P13C ${spec.id} read without data.read`, () => {
              packagedGuardDb.collection(packagedGuardCollection).findOne(recordId);
            });
            const packagedGuardPropertyDenied = await assertDenied(`P13C ${spec.id} property read without data.read`, () => {
              packagedGuardDb[packagedGuardCollection].findOne(recordId);
            });
            const packagedGuardRawDenied = await assertDenied(`P13C ${spec.id} raw read without data.read`, () => {
              packagedGuardDb.raw[packagedGuardCollection].findOne(recordId);
            });
            const packagedGuardContextDenied = await assertDenied(`P13C ${spec.id} real context read without data.read`, () => {
              packagedGuardContextDb.collection(packagedGuardCollection).findOne(recordId);
            });
            const packagedGuardContextPropertyDenied = await assertDenied(`P13C ${spec.id} real context property read without data.read`, () => {
              packagedGuardContextDb[packagedGuardCollection].findOne(recordId);
            });
            governance.permission_model.explicit_grants.push({
              grant_id: `phase13c_${spec.id.replace(/[^a-z0-9]+/gi, '_')}_read_${packagedGuardCollection}`,
              subject_type: 'user',
              subject_id: packagedGuardSession.user.id,
              permission: BusinessOsPermissions.DataRead,
              scope_type: 'collection',
              scope_id: packagedGuardCollection,
              active: true,
            });
            state.governance = governance;
            const packagedGuardReadQuery = smoke.createLiveDbFacade(packagedGuardModule)
              .collection(packagedGuardCollection)
              .findOne(recordId);
            const packagedGuardReadAllowedAfterGrant = Boolean(
              packagedGuardReadQuery && typeof packagedGuardReadQuery.exec === 'function'
            );
            const packagedGuardContextPermissionFacadeReadAllowed =
              packagedGuardContextPermissions?.canReadCollection?.(packagedGuardCollection) === true
              && packagedGuardContextPermissions?.canWriteCollection?.(packagedGuardCollection) === false;
            const packagedGuardWriteDeniedWithoutGrant = await assertDenied(`P13C ${spec.id} write without data.write`, () => {
              return smoke.createLiveDbFacade(packagedGuardModule)
                .collection(packagedGuardCollection)
                .insert({
                  id: recordId,
                  command_id: recordId,
                  module: packagedGuardModule.id,
                  command_type: 'phase13c.packaged_guard.write',
                  status: 'pending_sync',
                  payload: {},
                  client_context: { source: 'phase13c-packaged-guard-smoke' },
                  updated_at_ms: Date.now(),
                });
            });
            packagedGuardResults.push({
              module: packagedGuardModule,
              moduleId: packagedGuardModule.id,
              collection: packagedGuardCollection,
              capabilityContract: packagedGuardCapabilityContract,
              readDenied: packagedGuardReadDenied,
              propertyDenied: packagedGuardPropertyDenied,
              rawDenied: packagedGuardRawDenied,
              contextDenied: packagedGuardContextDenied,
              contextPropertyDenied: packagedGuardContextPropertyDenied,
              readAllowedAfterGrant: packagedGuardReadAllowedAfterGrant,
              contextPermissionFacadeDenied: packagedGuardContextPermissionFacadeDenied,
              contextPermissionFacadeReadAllowed: packagedGuardContextPermissionFacadeReadAllowed,
              writeDeniedWithoutGrant: packagedGuardWriteDeniedWithoutGrant,
              capabilities: packagedGuardCapabilities,
            });
          }
          const firstPackagedGuardResult = packagedGuardResults[0] || {};
          const packagedGuardModule = firstPackagedGuardResult.module || {};
          const packagedGuardCollection = firstPackagedGuardResult.collection || '';
          const packagedGuardCapabilityContract = firstPackagedGuardResult.capabilityContract === true;
          const packagedGuardReadDenied = firstPackagedGuardResult.readDenied === true;
          const packagedGuardPropertyDenied = firstPackagedGuardResult.propertyDenied === true;
          const packagedGuardRawDenied = firstPackagedGuardResult.rawDenied === true;
          const packagedGuardContextDenied = firstPackagedGuardResult.contextDenied === true;
          const packagedGuardContextPropertyDenied = firstPackagedGuardResult.contextPropertyDenied === true;
          const packagedGuardReadAllowedAfterGrant = firstPackagedGuardResult.readAllowedAfterGrant === true;
          const packagedGuardContextPermissionFacade = firstPackagedGuardResult.contextPermissionFacadeDenied === true
            && firstPackagedGuardResult.contextPermissionFacadeReadAllowed === true;
          const packagedGuardWriteDeniedWithoutGrant = firstPackagedGuardResult.writeDeniedWithoutGrant === true;
          const packagedGuardBatchCoverage = packagedGuardResults.length === packagedGuardSpecs.length;
          const packagedGuardModules = packagedGuardResults.map((item) => item.moduleId).join(',');
          const packagedGuardCollections = packagedGuardResults.map((item) => item.collection).join(',');
          const packagedGuardAllCapabilityContracts = packagedGuardBatchCoverage
            && packagedGuardResults.every((item) => item.capabilityContract === true);
          const packagedGuardAllReadDenied = packagedGuardBatchCoverage
            && packagedGuardResults.every((item) => item.readDenied === true);
          const packagedGuardAllPropertyDenied = packagedGuardBatchCoverage
            && packagedGuardResults.every((item) => item.propertyDenied === true && item.contextPropertyDenied === true);
          const packagedGuardAllRawDenied = packagedGuardBatchCoverage
            && packagedGuardResults.every((item) => item.rawDenied === true);
          const packagedGuardAllContextDenied = packagedGuardBatchCoverage
            && packagedGuardResults.every((item) => item.contextDenied === true);
          const packagedGuardAllReadGrantsAllowed = packagedGuardBatchCoverage
            && packagedGuardResults.every((item) => item.readAllowedAfterGrant === true);
          const packagedGuardAllContextPermissionFacades = packagedGuardBatchCoverage
            && packagedGuardResults.every((item) => (
              item.contextPermissionFacadeDenied === true
              && item.contextPermissionFacadeReadAllowed === true
            ));
          const packagedGuardAllWritesDeniedWithoutWrite = packagedGuardBatchCoverage
            && packagedGuardResults.every((item) => item.writeDeniedWithoutGrant === true);
          const systemScopedSpecs = Object.freeze([
            { id: 'app-store', allowed: 'business_commands', foreign: 'ctox_runtime_settings' },
            { id: 'browser', allowed: 'business_commands', foreign: 'business_module_catalog' },
            { id: 'creator', allowed: 'business_commands', foreign: 'ctox_runtime_settings' },
            { id: 'ctox', allowed: 'business_commands', foreign: 'business_module_catalog' },
            { id: 'desktop', allowed: 'business_commands', foreign: 'business_module_catalog' },
            { id: 'documents', allowed: 'business_commands', foreign: 'ctox_runtime_settings' },
            { id: 'knowledge', allowed: 'business_commands', foreign: 'business_module_catalog' },
            { id: 'research', allowed: 'business_commands', foreign: 'ctox_runtime_settings' },
            { id: 'reports', allowed: 'business_commands', foreign: 'business_module_catalog' },
            { id: 'tickets', allowed: 'business_commands', foreign: 'business_module_catalog' },
          ]);
          const resolveSystemScopedModule = (spec) => (Array.isArray(state.modules) ? state.modules : [])
            .find((module) => module?.id === spec.id)
            || launchTargets.find((module) => module?.id === spec.id)
            || { id: spec.id, title: spec.id, collections: [spec.allowed] };
          const systemScopedResults = [];
          for (const spec of systemScopedSpecs) {
            const module = resolveSystemScopedModule(spec);
            const context = smoke.createModuleContext(module);
            const db = context?.db;
            const permissions = context?.permissions || {};
            const capabilities = context?.runtimeCapabilities || {};
            const allowedHandle = db?.collection?.(spec.allowed) || null;
            const foreignCollection = db?.collection?.(spec.foreign) || null;
            const foreignProperty = db?.[spec.foreign] || null;
            const foreignRaw = db?.raw?.[spec.foreign] || null;
            const foreignCollectionsProxy = db?.collections?.[spec.foreign] || null;
            const permissionFacade = permissions.canReadCollection?.(spec.allowed) === true
              && permissions.canWriteCollection?.(spec.allowed) === true
              && permissions.canReadCollection?.(spec.foreign) === false
              && permissions.canWriteCollection?.(spec.foreign) === false;
            const capabilityContract = capabilities.database?.scoped_system === true
              && capabilities.database?.guarded === false
              && capabilities.database?.raw === 'scoped-system-allowlist'
              && capabilities.database?.collection_properties === 'scoped-system-allowlist'
              && Array.isArray(capabilities.database?.allowed_collections)
              && capabilities.database.allowed_collections.includes(spec.allowed)
              && !capabilities.database.allowed_collections.includes(spec.foreign);
            systemScopedResults.push({
              moduleId: module?.id || spec.id,
              allowed: spec.allowed,
              foreign: spec.foreign,
              allowedAvailable: Boolean(allowedHandle),
              foreignDenied: !foreignCollection && !foreignProperty,
              rawForeignDenied: !foreignRaw && !foreignCollectionsProxy,
              permissionFacade,
              capabilityContract,
              capabilities: capabilities.database || null,
            });
          }
          const systemScopedModules = systemScopedResults.map((item) => item.moduleId).join(',');
          const systemScopedBatchCoverage = systemScopedResults.length === systemScopedSpecs.length;
          const systemScopedAllowed = systemScopedBatchCoverage
            && systemScopedResults.every((item) => item.allowedAvailable === true);
          const systemScopedForeignDenied = systemScopedBatchCoverage
            && systemScopedResults.every((item) => item.foreignDenied === true);
          const systemScopedRawForeignDenied = systemScopedBatchCoverage
            && systemScopedResults.every((item) => item.rawForeignDenied === true);
          const systemScopedPermissionFacade = systemScopedBatchCoverage
            && systemScopedResults.every((item) => item.permissionFacade === true);
          const systemScopedCapabilityContract = systemScopedBatchCoverage
            && systemScopedResults.every((item) => item.capabilityContract === true);
          state.session = teamSession;
          state.governance = governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = teamSession;
          const supportGuardSpec = packagedGuardSpecs.find((spec) => spec.id === 'support');
          const supportGuardModule = supportGuardSpec ? resolvePackagedGuardModule(supportGuardSpec) : null;
          if (supportGuardModule && !(Array.isArray(state.modules) ? state.modules : []).some((item) => item?.id === supportGuardModule.id)) {
            state.modules = [...(Array.isArray(state.modules) ? state.modules : []), supportGuardModule];
          }
          if (!supportGuardModule || typeof state.openModule !== 'function') {
            throw new Error('P13C packaged guard shell locked-state support module or openModule is unavailable');
          }
          await state.openModule(supportGuardModule.id, { force: true, asModule: true });
          const packagedGuardShellLockedState = await waitFor(() => {
            const locked = document.querySelector('[data-module-permission-denied="true"]');
            return {
              ok: Boolean(
                locked
                && state.activeModule?.id === supportGuardModule.id
                && locked.getAttribute('data-permission') === BusinessOsPermissions.DataRead
              ),
              module_id: state.activeModule?.id || '',
              permission: locked?.getAttribute('data-permission') || '',
              collection: locked?.getAttribute('data-collection') || '',
              text: locked?.textContent?.trim().replace(/\s+/g, ' ').slice(0, 220) || '',
            };
          }, 15000, 'P13C packaged guard shell locked state');

          const storageKeys = typeof smoke.storageKeys === 'function' ? smoke.storageKeys() : {};
          const expectedStorageKeys = {
            taskbarPins: 'ctox.businessOs.taskbarPins',
            moduleLayout: 'ctox.businessOs.moduleLayout',
            accountPreferences: 'ctox.businessOs.accountPreferences',
            pairingConfig: 'ctox.businessOs.pairingConfig',
          };
          const scopedStorageKeyValues = Object.entries(expectedStorageKeys)
            .map(([name, baseKey]) => ({ name, baseKey, key: storageKeys?.[name] || '' }));
          const actorScopedNames = new Set(['taskbarPins', 'moduleLayout', 'accountPreferences']);
          const storageKeysScoped = Boolean(storageKeys.workspace)
            && Boolean(storageKeys.actor)
            && scopedStorageKeyValues.every(({ name, baseKey, key }) => {
              if (typeof key !== 'string' || key === baseKey || !key.includes('.scope.')) return false;
              if (!key.includes(storageKeys.workspace)) return false;
              if (actorScopedNames.has(name) && !key.includes(storageKeys.actor)) return false;
              return true;
            });
          const realContextStorageScope = realContext?.storageScope || {};
          const moduleStorageScopeContract = realContextStorageScope.version === 'business-os-storage-scope-v1'
            && realContextStorageScope.module_id === teamModule.id
            && realContextStorageScope.workspace === storageKeys.workspace
            && realContextStorageScope.actor === storageKeys.actor
            && typeof realContextStorageScope.key === 'function'
            && realContextStorageScope.key('ctox.app-store.leftWidth').includes('.scope.')
            && realContextStorageScope.key('ctox.app-store.leftWidth').includes(teamModule.id);

          const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['business_module_catalog', 'ctox_runtime_settings', 'business_commands'],
          });
          if (status?.version !== 'business-os-advanced-status-v1') {
            throw new Error(`dynamic apps UI smoke lost advanced status evidence: ${JSON.stringify(status)}`);
          }

          const privateHiddenForTeam = helperPrivateHiddenForTeam
            && !teamTabs.privateTabVisible;
          const privateVisibleForBuilder = helperPrivateVisibleForBuilder
            && builderTabs.privateTabVisible
            && builderTabs.privateBadgeState === 'private'
            && builderTabs.privateBadgeText === 'Privat';
          const teamVisibleForReleased = helperTeamVisibleForTeam
            && teamTabs.teamTabVisible
            && teamTabs.teamBadgeState === 'team'
            && teamTabs.teamBadgeText === 'Team';
          const restrictedHiddenForTeamAndVisibleForBuilder = restrictedHiddenForTeam
            && !teamTabs.restrictedTabVisible
            && builderTabs.restrictedTabVisible
            && builderTabs.restrictedBadgeState === 'restricted'
            && builderTabs.restrictedBadgeText === 'Eingeschränkt';
          const lifecycleBadgesVisible = privateLifecycle.version === 'v0.1.0'
            && privateLifecycle.text === 'Privat'
            && teamLifecycle.version === 'v1.0.0'
            && teamLifecycle.text === 'Team'
            && restrictedLifecycle.version === 'v1.0.0'
            && restrictedLifecycle.text === 'Eingeschränkt'
            && builderTabs.invalidBadgeState === 'private';
          const launcherBadgesVisible = builderLauncher.privateLauncherBadgeState === 'private'
            && builderLauncher.privateLauncherBadgeText === 'v0.1.0 Privat'
            && builderLauncher.teamLauncherBadgeState === 'team'
            && builderLauncher.teamLauncherBadgeText === 'v1.0.0 Team'
            && builderLauncher.invalidLauncherBadgeState === 'private'
            && builderLauncher.restrictedLauncherBadgeState === 'restricted'
            && teamLauncher.teamLauncherBadgeState === 'team'
            && !teamLauncher.labels.includes(privateModule.title);
          const lifecycleDrawerManagerState = Boolean(managerDrawer?.ok);
          const lifecycleDrawerReadonlyState = Boolean(readonlyDrawer?.ok)
            && !teamTabsForLauncher.privateTabVisible
            && teamTabsForLauncher.teamTabVisible;
          const lifecycleDrawerVisible = Boolean(drawer?.ok);
          const lifecycleWhyDiagnosticsVisible = managerDrawer?.why?.visible === true
            && readonlyDrawer?.why?.visible === true;
          const lifecycleWhyDiagnosticsRows = ['actor', 'visibility', 'open', 'modify', 'source', 'release', 'rollback', 'data']
            .every((key) => managerDrawer?.why?.rowKeys?.includes(key))
            && ['visibility', 'modify', 'release', 'data']
              .every((key) => readonlyDrawer?.why?.rowKeys?.includes(key));
          const lifecycleWhyDiagnosticsData = managerDrawer?.why?.dataCollections?.includes('business_commands')
            && readonlyDrawer?.why?.dataCollections?.includes('business_commands');
          const checks = {
            privateHiddenForTeam,
            privateVisibleForBuilder,
            teamVisibleForReleased,
            restrictedHiddenForTeamAndVisibleForBuilder,
            lifecycleBadgesVisible,
            launcherBadgesVisible,
            lifecycleDrawerManagerState,
            lifecycleDrawerReadonlyState,
            lifecycleDrawerVisible,
            lifecycleWhyDiagnosticsVisible,
            lifecycleWhyDiagnosticsRows,
            lifecycleWhyDiagnosticsData,
            dbReadDenied,
            dbRawDenied,
            realContextCollectionDenied,
            realContextPropertyDenied,
            realContextCachedDenied,
            realContextRawDenied,
            openModuleReloadMounted,
            openModuleCollectionDenied,
            openModulePropertyDenied,
            openModuleCachedDenied,
            openModuleRawDenied,
            openModuleRuntimeSafetyContract,
            openModuleRuntimeSafetyCapabilities,
            dbReadGrantAllowed,
            realContextCachedReadGrantAllowed,
            dbWriteDeniedWithoutWrite,
            facadeReadAllowed,
            facadeWriteDenied,
            packagedGuardCapabilityContract,
            packagedGuardReadDenied,
            packagedGuardPropertyDenied,
            packagedGuardRawDenied,
            packagedGuardContextDenied,
            packagedGuardContextPropertyDenied,
            packagedGuardReadAllowedAfterGrant,
            packagedGuardContextPermissionFacade,
            packagedGuardWriteDeniedWithoutGrant,
            packagedGuardShellLockedState: Boolean(packagedGuardShellLockedState?.ok),
            packagedGuardBatchCoverage,
            packagedGuardAllCapabilityContracts,
            packagedGuardAllReadDenied,
            packagedGuardAllPropertyDenied,
            packagedGuardAllRawDenied,
            packagedGuardAllContextDenied,
            packagedGuardAllReadGrantsAllowed,
            packagedGuardAllContextPermissionFacades,
            packagedGuardAllWritesDeniedWithoutWrite,
            systemScopedAllowed,
            systemScopedForeignDenied,
            systemScopedRawForeignDenied,
            systemScopedPermissionFacade,
            systemScopedCapabilityContract,
            storageKeysScoped,
            moduleStorageScopeContract,
            invalidVersionPrivate,
            reloadVerified: Boolean(dynamicAppsReloadVerified),
          };
          const failed = Object.entries(checks).filter(([, value]) => value !== true);
          if (failed.length) {
            throw new Error(`dynamic apps UI smoke failed: ${JSON.stringify({
              failed,
              teamTabs,
              builderTabs,
              privateLifecycle,
              teamLifecycle,
              restrictedLifecycle,
              invalidLifecycle,
              builderLauncher,
              teamLauncher,
              managerDrawer,
              readonlyDrawer,
              drawerText: drawer?.text || '',
              openModuleGuard: openModuleGuard?.guard || null,
              packagedGuard: {
                module_id: packagedGuardModule?.id || '',
                collection: packagedGuardCollection,
                modules: packagedGuardModules,
                collections: packagedGuardCollections,
                results: packagedGuardResults.map((item) => ({
                  module_id: item.moduleId,
                  collection: item.collection,
                  capabilityContract: item.capabilityContract,
                  readDenied: item.readDenied,
                  propertyDenied: item.propertyDenied,
                  rawDenied: item.rawDenied,
                  contextDenied: item.contextDenied,
                  contextPropertyDenied: item.contextPropertyDenied,
                  readAllowedAfterGrant: item.readAllowedAfterGrant,
                  contextPermissionFacadeDenied: item.contextPermissionFacadeDenied,
                  contextPermissionFacadeReadAllowed: item.contextPermissionFacadeReadAllowed,
                  writeDeniedWithoutGrant: item.writeDeniedWithoutGrant,
                  capabilities: item.capabilities?.database || null,
                })),
                shellLockedState: packagedGuardShellLockedState || null,
              },
              systemScoped: {
                modules: systemScopedModules,
                results: systemScopedResults,
              },
              storageKeys,
              realContextStorageScope: {
                version: realContextStorageScope.version,
                workspace: realContextStorageScope.workspace,
                actor: realContextStorageScope.actor,
                module_id: realContextStorageScope.module_id,
                sampleKey: typeof realContextStorageScope.key === 'function'
                  ? realContextStorageScope.key('ctox.app-store.leftWidth')
                  : '',
              },
            }, null, 2)}`);
          }
          result = {
            mode: smokeMode,
            privateModuleId: privateModule.id,
            teamModuleId: teamModule.id,
            privateHiddenForTeam,
            privateVisibleForBuilder,
            teamVisibleForReleased,
            restrictedHiddenForTeamAndVisibleForBuilder,
            lifecycleBadgesVisible,
            launcherBadgesVisible,
            lifecycleDrawerManagerState,
            lifecycleDrawerReadonlyState,
            lifecycleDrawerVisible,
            lifecycleWhyDiagnosticsVisible,
            lifecycleWhyDiagnosticsRows,
            lifecycleWhyDiagnosticsData,
            dbReadDenied,
            dbRawDenied,
            realContextCollectionDenied,
            realContextPropertyDenied,
            realContextCachedDenied,
            realContextRawDenied,
            openModuleReloadMounted,
            openModuleCollectionDenied,
            openModulePropertyDenied,
            openModuleCachedDenied,
            openModuleRawDenied,
            openModuleRuntimeSafetyContract,
            openModuleRuntimeSafetyCapabilities,
            dbReadGrantAllowed,
            realContextCachedReadGrantAllowed,
            dbWriteDeniedWithoutWrite,
            facadeReadAllowed,
            facadeWriteDenied,
            packagedGuardModuleId: packagedGuardModule.id,
            packagedGuardCollection,
            packagedGuardCapabilityContract,
            packagedGuardReadDenied,
            packagedGuardPropertyDenied,
            packagedGuardRawDenied,
            packagedGuardContextDenied,
            packagedGuardContextPropertyDenied,
            packagedGuardReadAllowedAfterGrant,
            packagedGuardContextPermissionFacade,
            packagedGuardWriteDeniedWithoutGrant,
            packagedGuardShellLockedState: Boolean(packagedGuardShellLockedState?.ok),
            packagedGuardModules,
            packagedGuardCollections,
            packagedGuardBatchCount: packagedGuardResults.length,
            packagedGuardBatchCoverage,
            packagedGuardAllCapabilityContracts,
            packagedGuardAllReadDenied,
            packagedGuardAllPropertyDenied,
            packagedGuardAllRawDenied,
            packagedGuardAllContextDenied,
            packagedGuardAllReadGrantsAllowed,
            packagedGuardAllContextPermissionFacades,
            packagedGuardAllWritesDeniedWithoutWrite,
            systemScopedModules,
            systemScopedCount: systemScopedResults.length,
            systemScopedAllowed,
            systemScopedForeignDenied,
            systemScopedRawForeignDenied,
            systemScopedPermissionFacade,
            systemScopedCapabilityContract,
            storageKeysScoped,
            moduleStorageScopeContract,
            storageKeys,
            invalidVersionPrivate,
            reloadVerified: Boolean(dynamicAppsReloadVerified),
            authState: document.body?.dataset?.authState || (state.session?.user?.id ? 'local-session' : 'unknown'),
            advancedStatusVersion: status.version || '',
            advancedStatusRuntime: status.rxdbRuntime || null,
          };
        } finally {
          document.querySelector('[data-close-lifecycle]')?.dispatchEvent(new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
          }));
          state.modules = originalState.modules;
          state.taskbarPins = originalState.taskbarPins;
          state.moduleAllowlist = originalState.moduleAllowlist;
          state.session = originalState.session;
          state.governance = originalState.governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = originalState.globalSession;
          smoke.renderTabs();
        }
        return result;
      }

      async function runBusinessOsAppAudienceUiSmoke() {
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            last = await predicate();
            if (last?.ok) return last;
            await delay(100);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/["\\]/g, '\\$&');
        };
        const smoke = globalThis.ctoxBusinessOsSmoke;
        const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state || appState;
        if (!state) throw new Error('Business OS app state is unavailable for app audience UI smoke');
        if (typeof smoke?.renderTabs !== 'function') throw new Error('Business OS smoke renderTabs hook is unavailable');
        if (typeof state.openModule !== 'function') throw new Error('Business OS state.openModule is unavailable for app audience UI smoke');
        appState = state;

        const [
          permissionsMod,
          lifecycleMod,
        ] = await Promise.all([
          import('/shared/permissions.js'),
          import('/shared/app-lifecycle.js'),
        ]);
        const { BusinessOsPermissions } = permissionsMod;
        const {
          appLifecycleBadge,
          canSeeModuleForAppVersion,
        } = lifecycleMod;
        const privateModule = {
          id: 'phase11-private-audience-app',
          title: 'Phase 11 Private Audience App',
          glyph: 'P11',
          version: '0.2.0',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase11-private-audience-app/index.js',
          collections: ['business_commands'],
        };
        const previewModule = {
          id: 'phase11-preview-audience-app',
          title: 'Phase 11 Preview Audience App',
          glyph: 'V11',
          version: '0.4.0',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase11-preview-audience-app/index.js',
          collections: ['business_commands'],
          lifecycle: {
            runtime_installed: true,
            visibility_state: 'preview',
            audience: 'preview',
            preview_user_ids: ['preview_target'],
          },
        };
        const restrictedModule = {
          id: 'phase11-restricted-audience-app',
          title: 'Phase 11 Restricted Audience App',
          glyph: 'S11',
          version: '1.1.0',
          source: 'installed',
          install_scope: 'installed',
          entry: 'installed-modules/phase11-restricted-audience-app/index.js',
          collections: ['business_commands'],
          lifecycle: {
            runtime_installed: true,
            visibility_state: 'restricted',
            audience: 'restricted',
          },
        };
        const allPermissions = Object.values(BusinessOsPermissions);
        const governance = {
          founders: {},
          permission_model: {
            version: 1,
            deny_supported: false,
            role_defaults: {
              chef: {
                workspace: allPermissions,
                module: allPermissions,
                assigned_module: allPermissions,
              },
              admin: {
                workspace: allPermissions,
                module: allPermissions,
                assigned_module: allPermissions,
              },
              founder: {
                workspace: [],
                module: [],
                assigned_module: [
                  BusinessOsPermissions.AppsView,
                  BusinessOsPermissions.AppsModify,
                  BusinessOsPermissions.AppsSourceView,
                ],
              },
              user: {
                workspace: [],
                module: [],
                assigned_module: [],
              },
            },
            module_assignments: {},
            explicit_grants: [
              {
                grant_id: 'phase11_preview_target_view',
                subject_type: 'user',
                subject_id: 'preview_target',
                permission: BusinessOsPermissions.AppsView,
                scope_type: 'module',
                scope_id: previewModule.id,
                active: true,
              },
              {
                grant_id: 'phase11_restricted_target_view',
                subject_type: 'user',
                subject_id: 'restricted_target',
                permission: BusinessOsPermissions.AppsView,
                scope_type: 'module',
                scope_id: restrictedModule.id,
                active: true,
              },
            ],
          },
        };
        const sessionFor = (id, role = 'user') => ({ user: { id, role } });
        const outsideSession = sessionFor('outside_user', 'user');
        const previewSession = sessionFor('preview_target', 'user');
        const insertedIds = new Set([privateModule.id, previewModule.id, restrictedModule.id]);
        const originalState = {
          modules: Array.isArray(state.modules) ? [...state.modules] : [],
          taskbarPins: Array.isArray(state.taskbarPins) ? [...state.taskbarPins] : [],
          moduleAllowlist: Array.isArray(state.moduleAllowlist) ? [...state.moduleAllowlist] : state.moduleAllowlist,
          session: state.session,
          governance: state.governance,
          activeModule: state.activeModule,
          globalSession: globalThis.CTOX_BUSINESS_OS_SESSION,
          storedTaskbarPins: localStorage.getItem('ctox.businessOs.taskbarPins'),
          storedFakeAudience: localStorage.getItem('ctox.businessOs.fakeAudience'),
        };
        const temporaryVisibleIds = [privateModule.id, previewModule.id, restrictedModule.id];
        const installAudienceModules = (session) => {
          state.session = session;
          state.governance = governance;
          globalThis.CTOX_BUSINESS_OS_SESSION = session;
          state.modules = [
            ...originalState.modules.filter((mod) => !insertedIds.has(mod?.id)),
            privateModule,
            previewModule,
            restrictedModule,
          ];
          state.moduleAllowlist = [...new Set([
            ...(Array.isArray(state.moduleAllowlist)
              ? state.moduleAllowlist.map((id) => String(id || '').trim()).filter(Boolean)
              : []),
            ...temporaryVisibleIds,
          ])];
          state.taskbarPins = [...temporaryVisibleIds];
          smoke.renderTabs();
        };
        const tabEvidence = () => {
          const privateTab = document.querySelector(`.module-tab[data-target="${css(privateModule.id)}"]`);
          const previewTab = document.querySelector(`.module-tab[data-target="${css(previewModule.id)}"]`);
          const restrictedTab = document.querySelector(`.module-tab[data-target="${css(restrictedModule.id)}"]`);
          const previewBadge = document.querySelector(`[data-app-lifecycle-badge="${css(previewModule.id)}"]`);
          const restrictedBadge = document.querySelector(`[data-app-lifecycle-badge="${css(restrictedModule.id)}"]`);
          return {
            privateTabVisible: Boolean(privateTab),
            previewTabVisible: Boolean(previewTab),
            restrictedTabVisible: Boolean(restrictedTab),
            previewBadgeState: previewBadge?.getAttribute('data-state') || '',
            previewBadgeText: previewBadge?.textContent?.trim() || '',
            restrictedBadgeState: restrictedBadge?.getAttribute('data-state') || '',
            restrictedBadgeText: restrictedBadge?.textContent?.trim() || '',
            activeModule: state.activeModule?.id || '',
            bodyActiveModule: document.body?.dataset?.activeModule || '',
            statusText: document.querySelector('[data-status-text]')?.textContent || '',
          };
        };
        const installAndWait = async (session, label, predicate) => waitFor(() => {
          installAudienceModules(session);
          const tabs = tabEvidence();
          return {
            ok: predicate(tabs),
            ...tabs,
          };
        }, 8000, `app audience ${label} tabs`);

        let result = null;
        try {
          const helperPrivateHiddenForTeam = !canSeeModuleForAppVersion(privateModule, {
            session: outsideSession,
            governance,
          });
          const helperPreviewVisibleForTarget = canSeeModuleForAppVersion(previewModule, {
            session: previewSession,
            governance,
          });
          const helperPreviewHiddenForOutside = !canSeeModuleForAppVersion(previewModule, {
            session: outsideSession,
            governance,
          });
          const helperRestrictedHiddenForOutside = !canSeeModuleForAppVersion(restrictedModule, {
            session: outsideSession,
            governance,
          });
          const previewLifecycle = appLifecycleBadge(previewModule, {
            session: previewSession,
            governance,
          });
          const restrictedLifecycle = appLifecycleBadge(restrictedModule, {
            session: sessionFor('restricted_target', 'user'),
            governance,
          });

          const outsideTabs = await installAndWait(
            outsideSession,
            'outside actor',
            (tabs) => !tabs.privateTabVisible && !tabs.previewTabVisible && !tabs.restrictedTabVisible,
          );
          const previewTabs = await installAndWait(
            previewSession,
            'preview actor',
            (tabs) => !tabs.privateTabVisible
              && tabs.previewTabVisible
              && !tabs.restrictedTabVisible
              && tabs.previewBadgeState === 'preview'
              && tabs.previewBadgeText === 'Vorschau',
          );

          installAudienceModules(outsideSession);
          const fallbackBefore = state.activeModule?.id || '';
          await state.openModule(previewModule.id, { force: true });
          const deepLinkTabs = tabEvidence();
          const deepLinkLockedOutside = deepLinkTabs.activeModule !== previewModule.id
            && deepLinkTabs.bodyActiveModule !== previewModule.id
            && document.body?.dataset?.moduleLoading !== previewModule.id
            && /nicht sichtbar|not visible/i.test(deepLinkTabs.statusText);

          tamperedScopedTaskbarPinsKey = typeof smoke.storageKeys === 'function'
            ? smoke.storageKeys()?.taskbarPins || ''
            : '';
          if (tamperedScopedTaskbarPinsKey) {
            storedScopedTaskbarPins = localStorage.getItem(tamperedScopedTaskbarPinsKey);
          }
          const tamperedPins = JSON.stringify([
            privateModule.id,
            previewModule.id,
            restrictedModule.id,
          ]);
          localStorage.setItem('ctox.businessOs.taskbarPins', tamperedPins);
          if (tamperedScopedTaskbarPinsKey) localStorage.setItem(tamperedScopedTaskbarPinsKey, tamperedPins);
          localStorage.setItem('ctox.businessOs.fakeAudience', JSON.stringify({
            [privateModule.id]: ['outside_user'],
            [previewModule.id]: ['outside_user'],
            [restrictedModule.id]: ['outside_user'],
          }));
          const storageTamperedTabs = await installAndWait(
            outsideSession,
            'storage tampered outside actor',
            (tabs) => !tabs.privateTabVisible && !tabs.previewTabVisible && !tabs.restrictedTabVisible,
          );
          localStorage.removeItem('ctox.businessOs.taskbarPins');
          if (tamperedScopedTaskbarPinsKey) localStorage.removeItem(tamperedScopedTaskbarPinsKey);
          localStorage.removeItem('ctox.businessOs.fakeAudience');
          const freshProfileTabs = await installAndWait(
            previewSession,
            'fresh profile preview actor',
            (tabs) => tabs.previewTabVisible && !tabs.privateTabVisible && !tabs.restrictedTabVisible,
          );

          const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
            includeCounts: false,
            requiredCollections: ['business_module_catalog', 'ctox_runtime_settings'],
          });
          if (status?.version !== 'business-os-advanced-status-v1') {
            throw new Error(`app audience UI smoke lost advanced status evidence: ${JSON.stringify(status)}`);
          }

          const privateHiddenForTeam = helperPrivateHiddenForTeam
            && !outsideTabs.privateTabVisible;
          const previewVisibleForTarget = helperPreviewVisibleForTarget
            && previewTabs.previewTabVisible
            && previewLifecycle.text === 'Vorschau'
            && previewLifecycle.version === 'v0.4.0';
          const previewHiddenForOutside = helperPreviewHiddenForOutside
            && !outsideTabs.previewTabVisible;
          const restrictedHiddenForOutside = helperRestrictedHiddenForOutside
            && !outsideTabs.restrictedTabVisible
            && restrictedLifecycle.text === 'Eingeschränkt';
          const storageBoundaryChecked = !storageTamperedTabs.privateTabVisible
            && !storageTamperedTabs.previewTabVisible
            && !storageTamperedTabs.restrictedTabVisible;
          const freshProfileVerified = freshProfileTabs.previewTabVisible
            && freshProfileTabs.previewBadgeState === 'preview';
          const reloadVerified = Boolean(appAudienceReloadVerified);
          const checks = {
            privateHiddenForTeam,
            previewVisibleForTarget,
            previewHiddenForOutside,
            restrictedHiddenForOutside,
            deepLinkLockedOutside,
            reloadVerified,
            freshProfileVerified,
            storageBoundaryChecked,
          };
          const failed = Object.entries(checks).filter(([, value]) => value !== true);
          if (failed.length) {
            throw new Error(`app audience UI smoke failed: ${JSON.stringify({
              failed,
              outsideTabs,
              previewTabs,
              deepLinkTabs,
              storageTamperedTabs,
              freshProfileTabs,
              previewLifecycle,
              restrictedLifecycle,
              fallbackBefore,
              tamperedScopedTaskbarPinsKey,
            }, null, 2)}`);
          }
          result = {
            mode: smokeMode,
            targetModuleId: previewModule.id,
            actorRole: previewSession.user.role,
            authState: document.body?.dataset?.authState || (state.session?.user?.id ? 'authenticated' : 'unknown'),
            browserContext: 'clean',
            tenantScope: 'local-business-os-smoke',
            privateHiddenForTeam,
            previewVisibleForTarget,
            previewHiddenForOutside,
            restrictedHiddenForOutside,
            deepLinkLockedOutside,
            reloadVerified,
            freshProfileVerified,
            storageBoundaryChecked,
            advancedStatusVersion: status.version || '',
            advancedStatusRuntime: status.rxdbRuntime || null,
          };
        } finally {
          if (originalState.storedTaskbarPins === null) localStorage.removeItem('ctox.businessOs.taskbarPins');
          else localStorage.setItem('ctox.businessOs.taskbarPins', originalState.storedTaskbarPins);
          if (tamperedScopedTaskbarPinsKey) {
            if (storedScopedTaskbarPins === null) localStorage.removeItem(tamperedScopedTaskbarPinsKey);
            else localStorage.setItem(tamperedScopedTaskbarPinsKey, storedScopedTaskbarPins);
          }
          if (originalState.storedFakeAudience === null) localStorage.removeItem('ctox.businessOs.fakeAudience');
          else localStorage.setItem('ctox.businessOs.fakeAudience', originalState.storedFakeAudience);
          state.modules = originalState.modules;
          state.taskbarPins = originalState.taskbarPins;
          state.moduleAllowlist = originalState.moduleAllowlist;
          state.session = originalState.session;
          state.governance = originalState.governance;
          state.activeModule = originalState.activeModule;
          if (originalState.activeModule?.id) {
            document.body.dataset.activeModule = originalState.activeModule.id;
          } else {
            delete document.body.dataset.activeModule;
          }
          globalThis.CTOX_BUSINESS_OS_SESSION = originalState.globalSession;
          smoke.renderTabs();
        }
        return result;
      }

      async function runBusinessOsAppReleaseUiSmoke() {
        const isVolatileSyncError = (error) => {
          const message = String(error?.message || error || '');
          return /peer-close|peer .* closed|replication-cancel|database connection is closing|QUERY_CANCELLED|WebRTC peer/i.test(message)
            || /UNAUTHORIZED:\s*peer is not authorized for this collection/i.test(message);
        };
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            try {
              last = await predicate();
            } catch (error) {
              if (!isVolatileSyncError(error)) throw error;
              last = { ok: false, volatileSyncError: error?.message || String(error) };
            }
            if (last?.ok) return last;
            await delay(150);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/["\\]/g, '\\$&');
        };
        const textOf = (selector) => document.querySelector(selector)?.textContent?.trim() || '';
        const visible = (selector) => {
          const element = document.querySelector(selector);
          if (!element) return false;
          const rect = element.getBoundingClientRect();
          const style = getComputedStyle(element);
          return rect.width > 0
            && rect.height > 0
            && style.display !== 'none'
            && style.visibility !== 'hidden'
            && Number(style.opacity || 1) > 0;
        };
        const click = (selector, label) => {
          const element = document.querySelector(selector);
          if (!element) throw new Error(`${label || selector} is missing`);
          if (element.disabled || element.getAttribute('aria-disabled') === 'true') {
            throw new Error(`${label || selector} is disabled`);
          }
          element.click();
          return element;
        };
        const setValue = (selector, value, label) => {
          const element = document.querySelector(selector);
          if (!element) throw new Error(`${label || selector} is missing`);
          element.value = value;
          element.dispatchEvent(new Event('input', { bubbles: true }));
          element.dispatchEvent(new Event('change', { bubbles: true }));
          return element;
        };
        const smoke = globalThis.ctoxBusinessOsSmoke;
        const state = globalThis.CTOX_BUSINESS_OS_APP || smoke?.state || appState;
        if (!state) throw new Error('Business OS app state is unavailable for app release UI smoke');
        if (typeof state.openModule !== 'function') throw new Error('Business OS state.openModule is unavailable for app release UI smoke');
        appState = state;

        const [
          permissionsMod,
          lifecycleMod,
        ] = await Promise.all([
          import('/shared/permissions.js'),
          import('/shared/app-lifecycle.js'),
        ]);
        const { BusinessOsPermissions } = permissionsMod;
        const {
          appLifecycleBadge,
          appReleaseProjection,
          canSeeModuleForAppVersion,
        } = lifecycleMod;

        const moduleId = 'phase10-release-app';
        const releaseActorId = 'release_owner';
        const adminSession = {
          ok: true,
          authenticated: true,
          auth_required: false,
          source: 'business-os-app-release-smoke',
          user: {
            id: 'local-dev',
            display_name: 'Local CTOX',
            role: 'admin',
            is_admin: true,
          },
        };
        const releaseSession = {
          ok: true,
          authenticated: true,
          auth_required: false,
          source: 'business-os-app-release-smoke',
          user: {
            id: releaseActorId,
            display_name: 'Release Owner',
            role: 'founder',
            is_admin: false,
          },
        };
        const teamSession = {
          ok: true,
          authenticated: true,
          auth_required: false,
          source: 'business-os-app-release-smoke',
          user: {
            id: 'team_member',
            display_name: 'Team Member',
            role: 'user',
            is_admin: false,
          },
        };
        const syncBusinessCollections = async (timeoutMs = 30000) => {
          const catalogBridge = await bounded(state.sync?.startCollection?.('business_module_catalog'), 5000);
          const commandBridge = await bounded(state.sync?.startCollection?.('business_commands'), 5000);
          await bounded(catalogBridge?.state?.awaitInitialReplication?.(), timeoutMs);
          await bounded(commandBridge?.state?.awaitInitialReplication?.(), timeoutMs);
          await bounded(catalogBridge?.state?.awaitInSync?.(), timeoutMs);
          await bounded(commandBridge?.state?.awaitInSync?.(), timeoutMs);
          await bounded(appCommandReplicationState?.awaitInSync?.(), timeoutMs);
        };
        const catalogCollection = () => state.db?.collection?.('business_module_catalog');
        const commandCollection = () => state.db?.collection?.('business_commands');
        const catalogSnapshot = async () => {
          const doc = await catalogCollection()?.findOne('module-catalog').exec();
          return doc?.toJSON?.() || {};
        };
        const moduleFromCatalog = (catalog) => (Array.isArray(catalog?.modules) ? catalog.modules : [])
          .find((mod) => mod?.id === moduleId) || null;
        const commandDocs = async () => {
          const docs = await commandCollection()?.find?.().exec?.();
          return Array.isArray(docs) ? docs.map((doc) => doc?.toJSON?.() || doc) : [];
        };
        const latestCommand = (docs, type) => docs
          .filter((doc) => String(doc?.record_id || '') === moduleId
            && [doc?.type, doc?.command_type, doc?.payload?.type].map((value) => String(value || '')).includes(type))
          .sort((left, right) => Number(right.updated_at_ms || right.created_at_ms || 0) - Number(left.updated_at_ms || left.created_at_ms || 0))[0] || null;
        const dispatchBusinessCommand = async (commandType, payload, session, recordId = moduleId, timeoutMs = 60000, commandModule = 'app-store') => {
          const commandBus = state.commandBus || smoke?.state?.commandBus;
          if (!commandBus?.dispatch) throw new Error('Business OS CommandBus is unavailable for app release UI smoke');
          const previousSession = state.session;
          state.session = session;
          globalThis.CTOX_BUSINESS_OS_SESSION = session;
          const commandId = `cmd_app_release_${crypto.randomUUID()}`;
          let dispatchError = null;
          try {
            await commandBus.dispatch({
              id: commandId,
              wait_timeout_ms: timeoutMs,
              module: commandModule,
              type: commandType,
              command_type: commandType,
              record_id: recordId,
              inbound_channel: 'business_os.app_release_smoke',
              payload,
              client_context: {
                source: 'business-os-app-release-smoke',
                module_id: moduleId,
                actor: {
                  id: session?.user?.id || '',
                  display_name: session?.user?.display_name || session?.user?.id || '',
                  role: session?.user?.role || 'user',
                  is_admin: Boolean(session?.user?.is_admin),
                },
              },
            });
          } catch (error) {
            dispatchError = error;
          }
          const projected = await waitFor(async () => {
            await syncBusinessCollections(5000);
            const doc = await commandCollection()?.findOne(commandId).exec();
            const data = doc?.toJSON?.();
            return {
              ok: Boolean(data && data.status && data.status !== 'pending_sync'),
              command: data || null,
            };
          }, timeoutMs, `${commandType} command projection`);
          state.session = previousSession;
          if (dispatchError && projected.command?.status === 'failed') {
            throw new Error(`${commandType} failed after dispatch error: ${JSON.stringify({
              message: dispatchError?.message || String(dispatchError),
              command: projected.command,
            })}`);
          }
          return projected.command;
        };
        const applyReleaseSession = async () => {
          const catalog = await catalogSnapshot();
          state.session = releaseSession;
          state.governance = catalog.governance || state.governance || null;
          globalThis.CTOX_BUSINESS_OS_SESSION = releaseSession;
          document.body.dataset.authState = 'authenticated';
          return catalog;
        };

        await syncBusinessCollections();
        const initialCatalog = await applyReleaseSession();
        const initialModule = moduleFromCatalog(initialCatalog);
        if (!initialModule) {
          throw new Error(`release fixture module missing from catalog: ${JSON.stringify({
            expected: moduleId,
            modules: (initialCatalog.modules || []).map((mod) => mod?.id).filter(Boolean),
          })}`);
        }
        const initialLifecycle = appLifecycleBadge(initialModule, {
          session: releaseSession,
          governance: initialCatalog.governance,
        });
        const privateBeforeRelease = initialLifecycle.state === 'private'
          && /^0\./.test(String(initialModule.version || ''))
          && !canSeeModuleForAppVersion(initialModule, {
            session: teamSession,
            governance: initialCatalog.governance,
          })
          && canSeeModuleForAppVersion(initialModule, {
            session: releaseSession,
            governance: initialCatalog.governance,
          });

        await state.openModule('app-store', { force: true, asModule: true });
        await waitFor(() => ({
          ok: visible('[data-app-store-root]') && visible('[data-apps-grid]'),
          activeModule: state.activeModule?.id || document.body?.dataset?.activeModule || '',
          text: document.querySelector('[data-app-store-root]')?.innerText?.slice(0, 500) || '',
        }), 30000, 'App Store opened for release smoke');
        click('[data-scope="installed"]', 'installed scope');
        await waitFor(() => {
          const card = document.querySelector(`[data-apps-grid] [data-app-id="${css(moduleId)}"]`);
          const releaseButton = card?.querySelector('[data-card-action="release"]');
          const lifecycleBadge = card?.querySelector('.app-lifecycle-badge');
          return {
            ok: Boolean(card && releaseButton && !releaseButton.disabled && /Privat/.test(lifecycleBadge?.textContent || '')),
            hasCard: Boolean(card),
            hasReleaseButton: Boolean(releaseButton),
            releaseDisabled: releaseButton?.disabled ?? null,
            lifecycleText: lifecycleBadge?.textContent?.trim() || '',
            cardText: card?.innerText?.slice(0, 500) || '',
          };
        }, 15000, 'release candidate card rendered');
        click(`[data-app-id="${css(moduleId)}"] [data-card-action="release"]`, 'release action');
        const dataReviewDialog = await waitFor(() => {
          const dialog = document.querySelector('.app-release-dialog');
          const text = dialog?.innerText || '';
          return {
            ok: Boolean(dialog
              && /Zielversion/i.test(text)
              && /Datenzugriff Review/i.test(text)
              && /business_commands/.test(text)),
            text: text.slice(0, 800),
          };
        }, 10000, 'release dialog data review');
        setValue('.app-release-dialog [name="target_version"]', '1.0.0', 'target version input');
        setValue('.app-release-dialog [name="release_channel"]', 'team', 'release channel select');
        setValue('.app-release-dialog [name="notes"]', 'Browser/Rust release smoke: Team-Freigabe 1.0.0', 'release notes');
        click('.app-release-dialog button[type="submit"]', 'release submit');

        const releaseProjection = await waitFor(async () => {
          await syncBusinessCollections(5000);
          const [catalog, docs] = await Promise.all([catalogSnapshot(), commandDocs()]);
          const module = moduleFromCatalog(catalog);
          const releaseCommand = latestCommand(docs, 'ctox.module.release');
          const projection = appReleaseProjection(module || {});
          const lifecycle = appLifecycleBadge(module || {}, {
            session: teamSession,
            governance: catalog.governance,
          });
          const card = document.querySelector(`[data-app-id="${css(moduleId)}"]`);
          return {
            ok: Boolean(
              module
                && String(module.version || '') === '1.0.0'
                && lifecycle.state === 'team'
                && canSeeModuleForAppVersion(module, { session: teamSession, governance: catalog.governance })
                && releaseCommand?.status === 'completed'
                && projection.dataAccess?.hasReview === true
            ),
            moduleVersion: module?.version || '',
            lifecycleState: lifecycle.state || '',
            releaseCommandStatus: releaseCommand?.status || '',
            releaseCommandId: releaseCommand?.id || releaseCommand?.command_id || '',
            releaseCommandError: releaseCommand?.error || releaseCommand?.result?.error || '',
            releaseCommandResult: releaseCommand?.result || null,
            releaseVersionId: releaseCommand?.result?.version_id || '',
            projection,
            cardText: card?.innerText?.slice(0, 800) || '',
          };
        }, 60000, 'release command projection');
        const releasedCatalog = await catalogSnapshot();
        const releasedModule = moduleFromCatalog(releasedCatalog);
        const releasedLifecycle = appLifecycleBadge(releasedModule, {
          session: teamSession,
          governance: releasedCatalog.governance,
        });
        const teamVisibleAfterRelease = canSeeModuleForAppVersion(releasedModule, {
          session: teamSession,
          governance: releasedCatalog.governance,
        }) === true;
        const versionBadgeVisible = await waitFor(() => {
          const card = document.querySelector(`[data-app-id="${css(moduleId)}"]`);
          const lifecycleBadge = card?.querySelector('.app-lifecycle-badge');
          const releaseBadge = card?.querySelector('.app-release-state');
          const text = card?.innerText || '';
          return {
            ok: Boolean(card && /v1\.0\.0/.test(lifecycleBadge?.textContent || text) && /Team/.test(lifecycleBadge?.textContent || text)),
            lifecycleText: lifecycleBadge?.textContent?.trim() || '',
            releaseText: releaseBadge?.textContent?.trim() || '',
            cardText: text.slice(0, 800),
          };
        }, 15000, 'release version badge');
        click(`[data-app-id="${css(moduleId)}"] [data-card-action="details"]`, 'release details action');
        const dataReviewVisible = await waitFor(() => {
          const root = document.querySelector('[data-app-store-root]');
          const text = root?.innerText || '';
          return {
            ok: /Datenzugriff/.test(text)
              && /Gesperrt|Review ist Nachweis|business_commands|Business Commands/.test(text),
            text: text.slice(0, 1200),
          };
        }, 10000, 'release data review projection');

        const storageKey = `ctox_business_os_release_override_${moduleId}`;
        localStorage.setItem(storageKey, JSON.stringify({ version: '9.9.9', visibility_state: 'private' }));
        await syncBusinessCollections(5000);
        const storageCatalog = await catalogSnapshot();
        const storageModule = moduleFromCatalog(storageCatalog);
        const storageBoundaryChecked = String(storageModule?.version || '') === '1.0.0'
          && canSeeModuleForAppVersion(storageModule, {
            session: teamSession,
            governance: storageCatalog.governance,
          }) === true;
        localStorage.removeItem(storageKey);

        await state.openModule('app-store', { force: true, asModule: true });
        const versionStateReady = await waitFor(async () => {
          await syncBusinessCollections(5000);
          const catalog = await catalogSnapshot();
          const module = moduleFromCatalog(catalog);
          const installedScopeButton = document.querySelector('[data-scope="installed"]');
          if (!installedScopeButton
            || installedScopeButton.disabled
            || installedScopeButton.getAttribute('aria-disabled') === 'true') {
            return {
              ok: false,
              installedScopePresent: Boolean(installedScopeButton),
              installedScopeDisabled: installedScopeButton?.disabled ?? null,
              installedScopeAriaDisabled: installedScopeButton?.getAttribute('aria-disabled') || '',
              activeModule: state.activeModule?.id || '',
            };
          }
          installedScopeButton.click();
          const card = document.querySelector(`[data-app-id="${css(moduleId)}"]`);
          const versionsButton = card?.querySelector('[data-card-action="versions"]');
          const versions = Array.isArray(module?.version_state?.versions)
            ? module.version_state.versions
            : [];
          const versionButtonText = versionsButton?.textContent?.trim() || '';
          return {
            ok: Boolean(card && versionsButton && (versions.length >= 1 || /Versionen\s*\([1-9]\d*\)/i.test(versionButtonText))),
            versionCount: versions.length,
            moduleVersion: module?.version || '',
            versionButtonText,
            cardText: card?.innerText?.slice(0, 800) || '',
          };
        }, 30000, 'release version state ready');
        let stableSince = 0;
        await waitFor(async () => {
          try {
            await syncBusinessCollections(5000);
            const card = document.querySelector(`[data-apps-grid] [data-app-id="${css(moduleId)}"]`);
            const versionsButton = card?.querySelector('[data-card-action="versions"]');
            const versionButtonText = versionsButton?.textContent?.trim() || '';
            const ready = Boolean(card && versionsButton && /Versionen\s*\([1-9]\d*\)/i.test(versionButtonText));
            if (!ready) stableSince = 0;
            else if (!stableSince) stableSince = Date.now();
            return {
              ok: ready && Date.now() - stableSince >= 8000,
              stableMs: stableSince ? Date.now() - stableSince : 0,
              versionButtonText,
              cardText: card?.innerText?.slice(0, 800) || '',
            };
          } catch (error) {
            stableSince = 0;
            return { ok: false, error: error?.message || String(error) };
          }
        }, 45000, 'release sync stable after runtime schema respawn');
        let lastVersionsClickAt = 0;
        click(`[data-apps-grid] [data-app-id="${css(moduleId)}"] [data-card-action="versions"]`, 'versions action');
        lastVersionsClickAt = Date.now();
        const versionDialog = await waitFor(() => {
          const dialog = document.querySelector('.app-store-version-dialog');
          const rollbackButtons = dialog ? [...dialog.querySelectorAll('[data-rollback-version]')] : [];
          const card = document.querySelector(`[data-apps-grid] [data-app-id="${css(moduleId)}"]`);
          const versionsButton = card?.querySelector('[data-card-action="versions"]');
          if (!dialog && versionsButton && Date.now() - lastVersionsClickAt > 1500) {
            versionsButton.click();
            lastVersionsClickAt = Date.now();
          }
          return {
            ok: Boolean(dialog && rollbackButtons.length >= 1),
            text: dialog?.innerText?.slice(0, 800) || '',
            rollbackCount: rollbackButtons.length,
            statusText: document.querySelector('[data-app-store-root]')?.innerText?.match(/Keine Versionen[^\n]*/)?.[0] || '',
            versionButtonText: versionsButton?.textContent?.trim() || '',
            versionStateReady,
          };
        }, 30000, 'versions dialog rollback options');
        const originalConfirm = globalThis.confirm;
        globalThis.confirm = () => true;
        try {
          const rows = [...document.querySelectorAll('.app-store-version-dialog .app-version-row')];
          const baselineRow = rows.find((row) => /Install|Installation|#1\b/.test(row.innerText || '')) || rows.at(-1) || null;
          const rollbackButton = baselineRow?.querySelector?.('[data-rollback-version]')
            || document.querySelector('.app-store-version-dialog [data-rollback-version]');
          if (!rollbackButton) throw new Error('rollback version action is missing');
          rollbackButton.click();
        } finally {
          globalThis.confirm = originalConfirm;
        }
        const rollbackProjection = await waitFor(async () => {
          await syncBusinessCollections(5000);
          const docs = await commandDocs();
          const rollbackCommand = latestCommand(docs, 'ctox.module.rollback_version');
          return {
            ok: rollbackCommand?.status === 'completed',
            rollbackCommandStatus: rollbackCommand?.status || '',
            rollbackCommandId: rollbackCommand?.id || rollbackCommand?.command_id || '',
            rollbackCommandError: rollbackCommand?.error || rollbackCommand?.result?.error || '',
            rollbackCommandResult: rollbackCommand?.result || null,
          };
        }, 60000, 'rollback command projection');

        if (typeof smoke?.openSettingsDrawer !== 'function') {
          throw new Error('Business OS smoke API does not expose openSettingsDrawer for app release activity audit smoke');
        }
        const activityCatalog = await catalogSnapshot();
        state.session = adminSession;
        state.governance = activityCatalog.governance || state.governance || null;
        globalThis.CTOX_BUSINESS_OS_SESSION = adminSession;
        document.body.dataset.authState = 'authenticated';
        await syncBusinessCollections(5000);
        await smoke.openSettingsDrawer({ initialTab: 'activity' });
        const activityAudit = await waitFor(async () => {
          await syncBusinessCollections(5000);
          const drawer = document.querySelector('.settings-drawer');
          const rows = drawer
            ? [...drawer.querySelectorAll('.settings-table tbody tr')]
              .map((row) => row.innerText || '')
              .filter(Boolean)
            : [];
          const text = drawer?.innerText || '';
          const releaseRow = rows.find((row) => /App-Version veröffentlicht/.test(row)
            && /Version 1\.0\.0/.test(row)
            && /Team/.test(row));
          const rollbackRow = rows.find((row) => /App-Rollback angewendet/.test(row));
          const rawLeak = /business_os\.module|ctox\.module|data_access_review|locked_collection_ids|Browser\/Rust release smoke/i.test(text);
          return {
            ok: Boolean(releaseRow && rollbackRow && !rawLeak),
            releaseVisible: Boolean(releaseRow),
            rollbackVisible: Boolean(rollbackRow),
            redacted: !rawLeak,
            releaseRow: releaseRow || '',
            rollbackRow: rollbackRow || '',
            rowCount: rows.length,
            text: text.slice(0, 1400),
          };
        }, 30000, 'settings activity release and rollback audit');

        const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections: ['business_module_catalog', 'business_commands', 'ctox_runtime_settings'],
        });
        if (status?.version !== 'business-os-advanced-status-v1') {
          throw new Error(`app release UI smoke lost advanced status evidence: ${JSON.stringify(status)}`);
        }

        const checks = {
          privateBeforeRelease,
          publishSucceeded: releaseProjection.releaseCommandStatus === 'completed',
          teamVisibleAfterRelease,
          versionBadgeVisible: versionBadgeVisible.ok === true,
          dataReviewVisible: dataReviewVisible.ok === true && dataReviewDialog.ok === true,
          rollbackSucceeded: rollbackProjection.rollbackCommandStatus === 'completed',
          releaseAuditVisible: activityAudit.releaseVisible === true,
          rollbackAuditVisible: activityAudit.rollbackVisible === true,
          activityAuditRedacted: activityAudit.redacted === true,
          reloadVerified: Boolean(appReleaseReloadVerified),
          storageBoundaryChecked,
        };
        const failed = Object.entries(checks).filter(([, value]) => value !== true);
        if (failed.length) {
          throw new Error(`app release UI smoke failed: ${JSON.stringify({
            failed,
            initialLifecycle,
            releasedLifecycle,
            releaseProjection,
            versionBadgeVisible,
            dataReviewVisible,
            versionDialog,
            rollbackProjection,
            activityAudit,
            storageBoundaryChecked,
          }, null, 2)}`);
        }
        return {
          mode: smokeMode,
          targetModuleId: moduleId,
          actorRole: releaseSession.user.role,
          authState: document.body?.dataset?.authState || (state.session?.authenticated ? 'authenticated' : 'unknown'),
          browserContext: 'clean',
          tenantScope: 'local-workspace',
          privateBeforeRelease,
          publishSucceeded: true,
          teamVisibleAfterRelease,
          versionBadgeVisible: true,
          dataReviewVisible: true,
          rollbackSucceeded: true,
          releaseAuditVisible: true,
          rollbackAuditVisible: true,
          activityAuditRedacted: true,
          reloadVerified: Boolean(appReleaseReloadVerified),
          storageBoundaryChecked,
          releaseVersionId: releaseProjection.releaseVersionId || '',
          rollbackCommandId: rollbackProjection.rollbackCommandId || '',
          advancedStatusVersion: status.version || '',
          advancedStatusRuntime: status.rxdbRuntime || null,
        };
      }

      async function runCodingAgentsUiSmoke() {
        if (!codingAgentSmoke?.provider || !codingAgentSmoke?.workspaceRoot) {
          throw new Error(`Coding Agents UI smoke config missing: ${JSON.stringify(codingAgentSmoke)}`);
        }
        const provider = codingAgentSmoke.provider;
        const workspaceRoot = codingAgentSmoke.workspaceRoot;
        const createMarker = codingAgentSmoke.createMarker;
        const followupMarker = codingAgentSmoke.followupMarker;
        const alerts = [];
        const originalAlert = globalThis.alert;
        globalThis.alert = (message) => {
          alerts.push(String(message || ''));
        };
        const waitFor = async (predicate, ms, label) => {
          const deadline = Date.now() + ms;
          let last = null;
          while (Date.now() < deadline) {
            if (alerts.length) {
              throw new Error(`${label} interrupted by browser alert: ${alerts.join(' | ')}`);
            }
            last = await predicate();
            if (last?.ok) return last;
            await delay(250);
          }
          throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
        };
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/["\\]/g, '\\$&');
        };
        const visibleEvidence = (selector) => {
          const element = document.querySelector(selector);
          if (!element) return { selector, visible: false, missing: true };
          const rect = element.getBoundingClientRect();
          const style = getComputedStyle(element);
          return {
            selector,
            visible: rect.width > 0
              && rect.height > 0
              && style.display !== 'none'
              && style.visibility !== 'hidden'
              && Number(style.opacity || 1) > 0,
            width: Math.round(rect.width),
            height: Math.round(rect.height),
          };
        };
        const click = (selector, label) => {
          const element = document.querySelector(selector);
          if (!element) throw new Error(`${label || selector} is missing`);
          if (element.disabled) throw new Error(`${label || selector} is disabled`);
          element.click();
          return element;
        };
        const setInputValue = (selector, value, label) => {
          const element = document.querySelector(selector);
          if (!element) throw new Error(`${label || selector} is missing`);
          element.value = value;
          element.dispatchEvent(new Event('input', { bubbles: true }));
          element.dispatchEvent(new Event('change', { bubbles: true }));
          return element;
        };
        const outcomeFromDispatch = (result) => {
          if (!result) return null;
          if (result.payload?.outcome) return result.payload.outcome;
          if (result.result?.outcome) return result.result.outcome;
          if (result.result?.ok !== undefined || result.result?.exit_code !== undefined) return result.result;
          if (result.outcome?.ok !== undefined || result.outcome?.exit_code !== undefined) return result.outcome;
          return null;
        };
        const dispatchCodingAgentCommand = async (commandType, payload, timeoutMs = 60000) => {
          const commandBus = globalThis.ctoxBusinessOsSmoke?.state?.commandBus;
          if (!commandBus?.dispatch) throw new Error('Business OS CommandBus is unavailable for Coding Agents UI smoke');
          const commandId = `cmd_coding_agents_ui_${provider}_${crypto.randomUUID()}`;
          const dispatched = await commandBus.dispatch({
            id: commandId,
            module: 'coding-agents',
            command_type: commandType,
            record_id: commandId,
            inbound_channel: 'business_os.coding_agents.ui_smoke',
            payload,
            wait_timeout_ms: timeoutMs,
            client_context: { source_module: 'coding-agents-ui-smoke' },
          });
          const outcome = outcomeFromDispatch(dispatched);
          return { commandId, dispatched, outcome };
        };
        const syncCodingAgentCollections = async (timeoutMs = 30000) => {
          await bounded(appCommandReplicationState?.awaitInSync?.(), timeoutMs);
          await bounded(appQueueReplicationState?.awaitInSync?.(), timeoutMs);
          await Promise.all(appCodingAgentProjectionStates.map((state) => bounded(state?.awaitInSync?.(), timeoutMs)));
        };
        const collectionDocs = async (name) => {
          const collection = appState?.db?.collection?.(name)
            || appState?.db?.collections?.[name]
            || appState?.db?.raw?.[name];
          if (!collection?.find) return [];
          return (await collection.find().exec()).map((doc) => doc?.toJSON?.() || doc);
        };
        const waitForOpenedModule = async () => waitFor(() => {
          const activeModule = document.body?.dataset?.activeModule || appState?.activeModule?.id || '';
          const loading = Boolean(document.body?.dataset?.moduleLoading);
          return {
            ok: activeModule === 'coding-agents' && !loading,
            activeModule,
            loading,
            text: (document.body?.innerText || '').slice(0, 400),
          };
        }, 30000, 'open Coding Agents module');
        const waitForAssistantMarker = async (marker, label) => waitFor(async () => {
          await syncCodingAgentCollections(5000);
          const assistantBubbles = [...document.querySelectorAll('.feed-chat-bubble.assistant')];
          const assistantTexts = assistantBubbles.map((bubble) => bubble.textContent || '');
          const eventDocs = await collectionDocs('coding_agent_events');
          const eventMatch = eventDocs.find((doc) =>
            doc.session_id === document.querySelector('#workbench-session-select')?.value
            && String(doc.role || '').toLowerCase() === 'assistant'
            && String(doc.text || '').includes(marker)
          );
          const feedHasMarker = assistantTexts.some((text) => text.includes(marker));
          if (!feedHasMarker && eventMatch) {
            const select = document.querySelector('#workbench-session-select');
            select?.dispatchEvent?.(new Event('change', { bubbles: true }));
          }
          return {
            ok: feedHasMarker,
            marker,
            eventMatch: Boolean(eventMatch),
            assistantBubbleCount: assistantBubbles.length,
            eventCount: eventDocs.length,
            feedText: document.querySelector('#workbench-chat-feed')?.innerText?.slice(-600) || '',
          };
        }, 10 * 60 * 1000, label);

        let stopOutcome = null;
        let revokeOutcome = null;
        let sessionId = '';
        try {
          location.hash = 'coding-agents';
          window.dispatchEvent(new HashChangeEvent('hashchange'));
          await waitForOpenedModule();
          await waitFor(() => ({
            ok: [
              '[data-coding-agents-root]',
              '#add-workspace-btn',
              '#new-session-btn',
              '#workbench-chat-feed',
              '#workbench-prompt-form',
            ].every((selector) => visibleEvidence(selector).visible),
            root: visibleEvidence('[data-coding-agents-root]'),
            feed: visibleEvidence('#workbench-chat-feed'),
          }), 15000, 'Coding Agents workbench controls rendered');

          const status = await dispatchCodingAgentCommand('ctox.coding_agent.status', { provider }, 60000);
          if (!status.outcome?.ok || status.outcome?.data?.auth?.ready !== true) {
            throw new Error(`Coding agent provider status is not production-ready: ${JSON.stringify(status.outcome)}`);
          }

          localStorage.setItem(`workspace_agent_${workspaceRoot}`, provider);
          click(`#diag-dot-${provider}`, `switch provider ${provider}`);
          await waitFor(() => ({
            ok: document.querySelector('[data-coding-agents-root]')?.classList?.contains(`theme-${provider}`),
            className: document.querySelector('[data-coding-agents-root]')?.className || '',
          }), 15000, `provider theme ${provider}`);

          click('#add-workspace-btn', 'add workspace button');
          await waitFor(() => ({
            ok: document.querySelector('#add-workspace-modal')?.hidden === false,
            hidden: document.querySelector('#add-workspace-modal')?.hidden ?? null,
          }), 5000, 'add workspace modal open');
          setInputValue('#add-workspace-input', workspaceRoot, 'workspace path input');
          await waitFor(() => ({
            ok: document.querySelector('#add-workspace-submit')?.disabled === false,
            disabled: document.querySelector('#add-workspace-submit')?.disabled ?? null,
          }), 5000, 'add workspace submit enabled');
          click('#add-workspace-submit', 'add workspace submit');
          await waitFor(() => ({
            ok: document.querySelector('#add-workspace-modal')?.hidden === true,
            hidden: document.querySelector('#add-workspace-modal')?.hidden ?? null,
            text: document.querySelector('#add-workspace-error')?.textContent || '',
          }), 120000, 'workspace grant completed via UI');
          await syncCodingAgentCollections();
          await waitFor(async () => {
            const item = document.querySelector(`.workspace-item[data-workspace="${css(workspaceRoot)}"]`);
            const grants = await collectionDocs('coding_agent_workspace_grants');
            return {
              ok: Boolean(item) && grants.some((doc) =>
                doc.provider === provider
                && doc.path === workspaceRoot
                && doc.active !== false
                && doc.is_deleted !== true
              ),
              itemFound: Boolean(item),
              grantCount: grants.length,
            };
          }, 60000, 'workspace grant projection rendered');

          const workspaceItem = document.querySelector(`.workspace-item[data-workspace="${css(workspaceRoot)}"]`);
          const providerSelect = workspaceItem?.querySelector('.workspace-agent-select');
          if (providerSelect && providerSelect.value !== provider) {
            providerSelect.value = provider;
            providerSelect.dispatchEvent(new Event('change', { bubbles: true }));
          }
          workspaceItem?.click();
          await waitFor(() => ({
            ok: document.querySelector('#new-session-btn')?.disabled === false
              && document.querySelector('#active-app-desc')?.textContent?.toLowerCase()?.includes(provider),
            newSessionDisabled: document.querySelector('#new-session-btn')?.disabled ?? null,
            desc: document.querySelector('#active-app-desc')?.textContent || '',
          }), 15000, 'workspace selected with requested provider');

          click('#new-session-btn', 'new session button');
          await waitFor(() => ({
            ok: document.querySelector('#new-session-modal')?.hidden === false,
            hidden: document.querySelector('#new-session-modal')?.hidden ?? null,
          }), 5000, 'new session modal open');
          const createPrompt = `Reply with exactly ${createMarker}. Do not edit files.`;
          setInputValue('#new-session-prompt', createPrompt, 'new session prompt');
          await waitFor(() => ({
            ok: document.querySelector('#new-session-submit')?.disabled === false,
            disabled: document.querySelector('#new-session-submit')?.disabled ?? null,
          }), 5000, 'new session submit enabled');
          click('#new-session-submit', 'new session submit');
          await waitFor(() => ({
            ok: document.querySelector('#new-session-modal')?.hidden === true
              && document.querySelector('#workbench-session-select')?.disabled === false
              && Boolean(document.querySelector('#workbench-session-select')?.value),
            hidden: document.querySelector('#new-session-modal')?.hidden ?? null,
            selectDisabled: document.querySelector('#workbench-session-select')?.disabled ?? null,
            selectedSession: document.querySelector('#workbench-session-select')?.value || '',
            error: document.querySelector('#new-session-error')?.textContent || '',
          }), 10 * 60 * 1000, 'new coding session created via UI');
          sessionId = document.querySelector('#workbench-session-select')?.value || '';
          await waitForAssistantMarker(createMarker, 'initial assistant marker rendered');

          const followupPrompt = `Reply with exactly ${followupMarker}. Do not edit files.`;
          setInputValue('#workbench-prompt-input', followupPrompt, 'workbench prompt input');
          await waitFor(() => ({
            ok: document.querySelector('#workbench-prompt-submit')?.disabled === false,
            disabled: document.querySelector('#workbench-prompt-submit')?.disabled ?? null,
          }), 5000, 'workbench prompt submit enabled');
          click('#workbench-prompt-submit', 'workbench prompt submit');
          await waitForAssistantMarker(followupMarker, 'follow-up assistant marker rendered');

          await syncCodingAgentCollections();
          const sessionDocs = await collectionDocs('coding_agent_sessions');
          const eventDocs = await collectionDocs('coding_agent_events');
          const sessionProjection = sessionDocs.find((doc) => doc.session_id === sessionId || doc.id === sessionId);
          const sessionEvents = eventDocs.filter((doc) => doc.session_id === sessionId && doc.is_deleted !== true);
          if (!sessionProjection || sessionProjection.provider !== provider || sessionProjection.workspace_root !== workspaceRoot) {
            throw new Error(`Coding agent session projection mismatch: ${JSON.stringify({ sessionId, sessionProjection })}`);
          }
          if (!sessionEvents.some((doc) => String(doc.text || '').includes(createMarker))
            || !sessionEvents.some((doc) => String(doc.text || '').includes(followupMarker))) {
            throw new Error(`Coding agent event projection did not include both smoke markers: ${JSON.stringify({
              sessionId,
              createMarker,
              followupMarker,
              events: sessionEvents.map((doc) => ({ role: doc.role, text: String(doc.text || '').slice(0, 120) })),
            })}`);
          }

          const feedText = document.querySelector('#workbench-chat-feed')?.innerText || '';
          return {
            mode: smokeMode,
            provider,
            workspaceRoot,
            sessionId,
            createMarkerSeen: feedText.includes(createMarker),
            followupMarkerSeen: feedText.includes(followupMarker),
            statusReady: true,
            authStatus: status.outcome?.data?.auth?.status || status.outcome?.data?.auth?.state || 'ready',
            sessionProjectionStatus: sessionProjection.status || '',
            eventCount: sessionEvents.length,
            userEventCount: sessionEvents.filter((doc) => String(doc.role || '').toLowerCase() === 'user').length,
            assistantEventCount: sessionEvents.filter((doc) => String(doc.role || '').toLowerCase() === 'assistant').length,
            feedTextLength: feedText.length,
            activeModule: document.body?.dataset?.activeModule || appState?.activeModule?.id || '',
            advancedStatusVersion,
            advancedStatusRuntime,
          };
        } finally {
          if (sessionId) {
            stopOutcome = await dispatchCodingAgentCommand(
              'ctox.coding_agent.session.stop',
              { provider, session_id: sessionId },
              60000,
            ).then((result) => result.outcome).catch((error) => ({ ok: false, stderr: String(error?.message || error) }));
          }
          revokeOutcome = await dispatchCodingAgentCommand(
            'ctox.coding_agent.workspace.revoke',
            { provider, path: workspaceRoot },
            60000,
          ).then((result) => result.outcome).catch((error) => ({ ok: false, stderr: String(error?.message || error) }));
          await syncCodingAgentCollections().catch(() => null);
          globalThis.alert = originalAlert;
          if (stopOutcome?.ok === false) console.warn(`Coding Agents UI smoke cleanup stop failed: ${stopOutcome.stderr || ''}`);
          if (revokeOutcome?.ok === false) console.warn(`Coding Agents UI smoke cleanup revoke failed: ${revokeOutcome.stderr || ''}`);
        }
      }

      async function openDesktopFileViewerAndWait(fileArgs, expectedPayload) {
        const smoke = globalThis.ctoxBusinessOsSmoke;
        if (typeof smoke?.openDesktopApp !== 'function') {
          throw new Error('Business OS smoke API does not expose openDesktopApp for File Viewer UI regression');
        }
        const beforeWindows = document.querySelectorAll('.shell-window').length;
        const windowId = await smoke.openDesktopApp('file-viewer', {
          title: fileArgs.name || 'File Viewer',
          args: fileArgs,
        });
        const deadline = Date.now() + 60000;
        let last = null;
        while (Date.now() < deadline) {
          const windows = [...document.querySelectorAll('.shell-window')];
          const viewerWindows = windows
            .map((win) => {
              const text = win.querySelector('[data-file-text]')?.textContent || '';
              const errorText = win.querySelector('.file-viewer .is-error')?.textContent || '';
              const title = win.querySelector('[data-window-title]')?.textContent?.trim() || '';
              return { win, text, errorText, title };
            })
            .filter((entry) => entry.text || entry.errorText || /File Viewer|brief\.md|smoke/i.test(entry.title));
          last = {
            windowId,
            beforeWindows,
            windowCount: windows.length,
            viewerWindowCount: viewerWindows.length,
            titles: viewerWindows.map((entry) => entry.title).slice(0, 4),
            textLength: Math.max(0, ...viewerWindows.map((entry) => entry.text.length)),
            errorText: viewerWindows.find((entry) => entry.errorText)?.errorText || '',
          };
          if (last.errorText) {
            throw new Error(`Business OS File Viewer desktop app rendered an error: ${JSON.stringify(last)}`);
          }
          if (viewerWindows.some((entry) => entry.text === expectedPayload)) {
            return {
              windowId,
              beforeWindows,
              windowCount: windows.length,
              viewerWindowCount: viewerWindows.length,
              renderedLength: expectedPayload.length,
            };
          }
          await delay(500);
        }
        throw new Error(`Business OS File Viewer desktop app did not render the expected payload: ${JSON.stringify(last)}`);
      }

      async function waitForFileMetadata(id, ms = 30000) {
        const deadline = Date.now() + ms;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          if (file) {
            const chunks = (await db.desktop_file_chunks.find().exec())
              .map((doc) => doc.toJSON?.() || doc)
              .filter((doc) => doc.file_id === id);
            return { file, chunks };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive rust-side file metadata ${id}`);
      }

      async function waitForCorruptChunkMetadata(id, ms = 60000) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const chunks = (await db.desktop_file_chunks.find().exec())
            .map((doc) => doc.toJSON?.() || doc)
            .filter((doc) => doc.file_id === id)
            .sort((left, right) => Number(left.idx || 0) - Number(right.idx || 0));
          const match = chunks.find((chunk) => {
            const expectedSize = Number(chunk.size_bytes);
            return Number.isFinite(expectedSize) && expectedSize !== String(chunk.data || '').length;
          });
          lastSeen = chunks.map((chunk) => ({
            id: chunk.id,
            idx: chunk.idx,
            size_bytes: chunk.size_bytes,
            actualSizeBytes: String(chunk.data || '').length,
            rev: chunk._rev || '',
          }));
          if (match) {
            return {
              chunk: match,
              expectedSizeBytes: Number(match.size_bytes),
              actualSizeBytes: String(match.data || '').length,
            };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive corrupt chunk metadata ${id}: ${JSON.stringify(lastSeen)}`);
      }

      function isDeletedSmokeChunk(chunk) {
        return chunk?._deleted === true || chunk?.deleted === true || chunk?.is_deleted === true;
      }

      async function tombstoneLocalFileCache(id) {
        const chunkDocs = (await db.desktop_file_chunks.find().exec())
          .filter((doc) => {
            const chunk = doc?.toJSON?.() || doc;
            return chunk?.file_id === id && !isDeletedSmokeChunk(chunk);
          });
        for (const doc of chunkDocs) {
          await doc?.remove?.();
        }
        const fileDoc = await db.desktop_files.findOne(id).exec();
        if (fileDoc) {
          await fileDoc.incrementalPatch?.({
            path: '',
            local_path: '',
            content_state: 'available',
            updated_at_ms: Date.now(),
          });
        }
      }

      async function startDeferredAppFileCollections(label = 'desktop_files') {
        const state = appState || globalThis.ctoxBusinessOsSmoke?.state;
        if (!state?.sync?.startCollection && !state?.sync?.leaseCollection) {
          throw new Error(`Business OS sync runtime is not available for deferred ${label} replication`);
        }
        const fileBridge = await startAppSyncCollection(state, 'desktop_files', `deferred-${label}`);
        const chunkBridge = await startAppSyncCollection(state, 'desktop_file_chunks', `deferred-${label}`);
        fileBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app desktop_files replication error', error));
        chunkBridge?.state?.error$?.subscribe?.((error) => logUnexpectedReplicationError('app desktop_file_chunks replication error', error));
        appFileReplicationState = fileBridge?.state || null;
        appChunkReplicationState = chunkBridge?.state || null;
        await bounded(appFileReplicationState?.awaitInitialReplication?.(), 15000);
        await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 15000);
        await bounded(appFileReplicationState?.awaitInSync?.(), 15000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 15000);
        await waitForNativePeerOpen(appFileReplicationState, 'desktop_files');
        await waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks');
      }

      async function waitForTombstonedChunkState(id, ms = 60000) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          lastSeen = {
            file: file ? {
              id: file.id,
              path: file.path || '',
              local_path: file.local_path || '',
              content_state: file.content_state || '',
              deleted: isDeletedSmokeChunk(file),
              rev: file._rev || '',
            } : null,
          };
          // desktop_file_chunks deliberately has no query-demand loader. The
          // browser receives file metadata here; the viewer below proves the
          // tombstone through the bounded file-demand stream.
          if (file && (isDeletedSmokeChunk(file) || (!file.path && !file.local_path))) {
            return { file };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive tombstoned chunk state ${id}: ${JSON.stringify(lastSeen)}`);
      }

      async function waitForStaleGenerationState(id, requestedGenerationId, ms = 60000) {
        const deadline = Date.now() + ms;
        let lastSeen = null;
        while (Date.now() < deadline) {
          const fileDoc = await db.desktop_files.findOne(id).exec();
          const file = fileDoc?.toJSON?.() || fileDoc;
          lastSeen = {
            file: file ? {
              id: file.id,
              path: file.path || '',
              local_path: file.local_path || '',
              content_state: file.content_state || '',
              content_generation_id: file.content_generation_id || '',
              rev: file._rev || '',
            } : null,
            requestedGenerationId,
          };
          if (file
            && file.content_generation_id === requestedGenerationId
            && !file.path
            && !file.local_path) {
            return { file };
          }
          await delay(500);
        }
        throw new Error(`browser did not receive stale generation state ${id}: ${JSON.stringify(lastSeen)}`);
      }

      async function waitForFileIntegrityStatus(expectedCode, ms = 30000) {
        const deadline = Date.now() + ms;
        let lastSnapshot = null;
        while (Date.now() < deadline) {
          lastSnapshot = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: false });
          const errors = Array.isArray(lastSnapshot?.fileIntegrity?.errors)
            ? lastSnapshot.fileIntegrity.errors
            : [];
          const match = errors.find((error) => error?.name === 'CtoxFileChunkIntegrityError' && error?.code === expectedCode);
          if (match) return { error: match, snapshot: lastSnapshot };
          await delay(250);
        }
        throw new Error(`Business OS did not expose file integrity error ${expectedCode}: ${JSON.stringify(lastSnapshot, null, 2)}`);
      }

      function registerRxdbPlugin(target, plugin) {
        const add = target?.addRxPlugin;
        if (typeof add !== 'function' || !plugin) return;
        try {
          add(plugin);
        } catch (error) {
          const message = String(error?.message || error || '');
          if (!message.toLowerCase().includes('already')) throw error;
        }
      }

      async function repairAppFileAndCommandReplicationAfterNativeRestart() {
        const criticalCollections = [
          'business_module_catalog',
          'ctox_runtime_settings',
          'business_commands',
          'ctox_queue_tasks',
          ...(needsFileCollections ? ['desktop_files', 'desktop_file_chunks'] : []),
        ];
        const preRestartState = globalThis.ctoxBusinessOsSmoke?.state;
        const preRestartDiagnostics = preRestartState?.syncDiagnostics?.collections || {};
        const activeCollectionsBeforeRestart = Object.entries(preRestartDiagnostics)
          .filter(([, entry]) => {
            const status = entry?.connectionStatus || entry?.status || '';
            return status && status !== 'stopped' && status !== 'paused';
          })
          .map(([collection]) => collection);
        const suspendCollections = [...new Set([
          ...activeCollectionsBeforeRestart,
          ...criticalCollections,
        ])];
        const usedSuspend = typeof preRestartState?.sync?.suspendCollections === 'function';
        if (usedSuspend) {
          await preRestartState.sync.suspendCollections(suspendCollections, 'native-peer-controlled-restart');
        } else {
          for (const collection of suspendCollections) {
            await preRestartState?.sync?.stopCollection?.(collection).catch(() => null);
          }
        }
        await globalThis.__ctoxRestartNativePeer?.();
        const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
        if (!repairedState?.db?.raw?.desktop_files || !repairedState?.db?.raw?.desktop_file_chunks) {
          throw new Error('Business OS file collections were not available after native peer restart');
        }
        db = repairedState.db.raw;
        if (typeof repairedState.sync?.resumeCollections === 'function' && usedSuspend) {
          const repairedBridges = await repairedState.sync.resumeCollections(criticalCollections);
          const bridgeByCollection = Object.fromEntries(criticalCollections.map((collection, index) => [collection, repairedBridges[index]]));
          appCommandReplicationState = bridgeByCollection.business_commands?.state || appCommandReplicationState;
          appQueueReplicationState = bridgeByCollection.ctox_queue_tasks?.state || appQueueReplicationState;
          appFileReplicationState = bridgeByCollection.desktop_files?.state || appFileReplicationState;
          appChunkReplicationState = bridgeByCollection.desktop_file_chunks?.state || appChunkReplicationState;
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 30000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 30000);
          await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
          await waitForAppSyncCollections(criticalCollections);
          return repairedState;
        }
        if (typeof repairedState.sync?.restartCollections === 'function') {
          const repairedBridges = await repairedState.sync.restartCollections(criticalCollections);
          const bridgeByCollection = Object.fromEntries(criticalCollections.map((collection, index) => [collection, repairedBridges[index]]));
          appCommandReplicationState = bridgeByCollection.business_commands?.state || appCommandReplicationState;
          appQueueReplicationState = bridgeByCollection.ctox_queue_tasks?.state || appQueueReplicationState;
          appFileReplicationState = bridgeByCollection.desktop_files?.state || appFileReplicationState;
          appChunkReplicationState = bridgeByCollection.desktop_file_chunks?.state || appChunkReplicationState;
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 30000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 30000);
          await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
          await waitForAppSyncCollections(criticalCollections);
          return repairedState;
        }
        const startFresh = async (collection) => {
          if (typeof repairedState.sync?.restartCollection === 'function') {
            return repairedState.sync.restartCollection(collection);
          }
          return repairedState.sync?.startCollection?.(collection);
        };
        const repairedModuleBridge = await startFresh('business_module_catalog');
        const repairedRuntimeBridge = await startFresh('ctox_runtime_settings');
        const repairedCommandBridge = await startFresh('business_commands');
        const repairedQueueBridge = await startFresh('ctox_queue_tasks');
        const repairedFileBridge = await startFresh('desktop_files');
        const repairedChunkBridge = await startFresh('desktop_file_chunks');
        appCommandReplicationState = repairedCommandBridge?.state || appCommandReplicationState;
        appQueueReplicationState = repairedQueueBridge?.state || appQueueReplicationState;
        appFileReplicationState = repairedFileBridge?.state || appFileReplicationState;
        appChunkReplicationState = repairedChunkBridge?.state || appChunkReplicationState;
        await bounded(repairedModuleBridge?.state?.awaitInitialReplication?.(), 20000);
        await bounded(repairedRuntimeBridge?.state?.awaitInitialReplication?.(), 20000);
        await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
        await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
        await bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000);
        await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000);
        await bounded(repairedModuleBridge?.state?.awaitInSync?.(), 30000);
        await bounded(repairedRuntimeBridge?.state?.awaitInSync?.(), 30000);
        await bounded(appCommandReplicationState?.awaitInSync?.(), 30000);
        await bounded(appQueueReplicationState?.awaitInSync?.(), 30000);
        await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
        await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
        await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
        await waitForAppSyncCollections(criticalCollections);
        return repairedState;
      }

      async function waitForAppSyncCollections(collections, ms = 90000) {
        const deadline = Date.now() + ms;
        let lastDiagnostics = null;
        while (Date.now() < deadline) {
          lastDiagnostics = globalThis.ctoxBusinessOsSmoke?.state?.syncDiagnostics || null;
          const collectionDiagnostics = lastDiagnostics?.collections || {};
          const ready = collections.every((collection) => {
            const entry = collectionDiagnostics[collection] || {};
            return entry.connectionStatus === 'connected';
          });
          if (ready) return;
          await delay(500);
        }
        throw new Error(`Business OS sync collections did not reconnect: ${JSON.stringify({
          collections,
          diagnostics: lastDiagnostics,
        })}`);
      }

      async function runOutboundActiveUiSmoke() {
        const state = globalThis.ctoxBusinessOsSmoke?.state;
        const rawDb = state?.db?.raw;
        if (!state?.sync?.startCollection || !rawDb?.outbound_campaigns || !rawDb?.outbound_pipeline_items) {
          throw new Error('Outbound active UI smoke requires Business OS app DB and outbound collections');
        }
        const waitFor = async (probe, timeoutMs, label) => {
          const deadline = Date.now() + timeoutMs;
          let last = null;
          while (Date.now() < deadline) {
            last = await probe();
            if (last?.ok) return last;
            await delay(250);
          }
          throw new Error(`Timed out waiting for ${label}: ${JSON.stringify(last)}`);
        };
        const upsert = async (collection, document) => {
          if (collection.incrementalUpsert) return collection.incrementalUpsert(document);
          if (collection.upsert) return collection.upsert(document);
          const existing = await collection.findOne(document.id).exec();
          if (existing?.incrementalPatch) return existing.incrementalPatch(document);
          if (existing?.patch) return existing.patch(document);
          return collection.insert(document);
        };
        const jsonDocs = async (collection) => (await collection.find().exec()).map((doc) => doc.toJSON?.() || doc);
        const css = (value) => {
          if (globalThis.CSS?.escape) return globalThis.CSS.escape(String(value));
          return String(value).replace(/"/g, '\\"');
        };
        const click = (selector, label) => {
          const element = document.querySelector(selector);
          if (!element) throw new Error(`${label} not found: ${selector}`);
          element.click();
          return true;
        };
        const screenshotPaths = [];
        const capture = async (name) => {
          const path = await globalThis.__ctoxCaptureSmokeScreenshot?.(name);
          if (path) screenshotPaths.push(path);
        };
        const outboundCollections = [
          'business_commands',
          'ctox_queue_tasks',
          'outbound_campaigns',
          'outbound_pipeline_items',
          'outbound_engagements',
          'outbound_messages',
          'outbound_approvals',
          'outbound_sender_assignments',
          'outbound_account_limits',
          'outbound_meeting_requests',
          'outbound_suppression_entries',
          'outbound_sequences',
          'outbound_skillbooks',
          'outbound_letter_templates',
        ].filter((collection) => rawDb[collection]);
        const bridges = [];
        for (const collection of outboundCollections) {
          const bridge = await state.sync.startCollection(collection);
          bridges.push(bridge?.state);
          await bounded(bridge?.state?.awaitInitialReplication?.(), 20000);
          await bounded(bridge?.state?.awaitInSync?.(), 20000);
        }
        const commandBridge = bridges[outboundCollections.indexOf('business_commands')];
        const queueBridge = bridges[outboundCollections.indexOf('ctox_queue_tasks')];
        if (commandBridge) await waitForNativePeerOpen(commandBridge, 'business_commands');
        if (queueBridge) await waitForNativePeerOpen(queueBridge, 'ctox_queue_tasks');
        const projectCommandResult = async (result) => {
          const projections = [
            ['outbound_engagements', result?.engagement],
            ['outbound_messages', result?.message],
            ['outbound_approvals', result?.approval],
            ['outbound_sender_assignments', result?.assignment],
            ['outbound_sequences', result?.sequence],
            ['outbound_meeting_requests', result?.meeting_request || result?.request],
          ];
          for (const [collectionName, record] of projections) {
            if (!record?.id || !rawDb[collectionName]) continue;
            await upsert(rawDb[collectionName], record);
          }
        };
        const dispatchNativeOutboundCommand = async (type, recordId, payload, label) => {
          const commandId = `cmd_${type.replaceAll('.', '_')}_${Date.now()}_${Math.random().toString(16).slice(2)}`;
          await rawDb.business_commands.insert({
            id: commandId,
            command_id: commandId,
            module: 'outbound',
            command_type: type,
            record_id: recordId || '',
            status: 'pending_sync',
            inbound_channel: 'outbound',
            payload,
            client_context: smokeClientContext({ source: 'outbound-active-ui-smoke' }),
            updated_at_ms: Date.now(),
          });
          await bounded(commandBridge?.awaitInSync?.(), 25000);
          const command = await waitFor(async () => {
            const doc = (await rawDb.business_commands.findOne(commandId).exec())?.toJSON?.();
            return {
              ok: Boolean(doc && doc.status && doc.status !== 'pending_sync'),
              command: doc,
              status: doc?.status || '',
              error: doc?.error || '',
            };
          }, 60000, label || type);
          if (command.command?.status === 'failed') {
            throw new Error(`${label || type} failed: ${command.command.error || 'unknown error'}`);
          }
          await projectCommandResult(command.command?.result);
          return command.command;
        };

        if (!/#outbound(?:$|[?&])/.test(location.hash)) {
          location.hash = '#outbound';
        }
        await waitFor(async () => ({
          ok: Boolean(document.querySelector('[data-outbound-root]')),
          activeModule: state.activeModule?.id || '',
          body: document.body?.dataset?.activeModule || '',
        }), 60000, 'Outbound module root');

        const now = Date.now();
        const suffix = String(now).slice(-8);
        const campaignId = `outbound_active_ui_${suffix}`;
        const pipelineId = `outbound_active_ui_lead_${suffix}`;
        const senderAccount = 'email:outbound-smoke@example.com';
        const recipientEmail = `lead-${suffix}@example.com`;
        await upsert(rawDb.outbound_campaigns, {
          id: campaignId,
          name: 'Outbound Active UI Smoke',
          objective: 'Browser E2E approval-gated outbound communication',
          market: 'DACH',
          status: 'active',
          owner_id: 'browser-smoke',
          source_count: 1,
          company_count: 1,
          qualified_count: 1,
          pipeline_count: 1,
          communication_account_key: senderAccount,
          communication_account_address: 'outbound-smoke@example.com',
          payload: {
            subtitle: 'Browser smoke',
            scope: 'Approval gate and mailserver queue',
            communication_account_key: senderAccount,
            communication_account_address: 'outbound-smoke@example.com',
            active_outreach: {
              default_channel: 'email',
              strategy_text: 'Initial message, user approval, then provider queue.',
            },
          },
          created_at_ms: now,
          updated_at_ms: now,
        });
        await upsert(rawDb.outbound_account_limits, {
          id: senderAccount,
          sender_account_id: senderAccount,
          campaign_id: campaignId,
          daily_limit: 20,
          sent_today: 0,
          daily_sent_count: 0,
          status: 'ready',
          send_window: { timezone: 'Europe/Berlin', text: 'Mo-Fr 09:00-17:00' },
          payload: {},
          created_at_ms: now,
          updated_at_ms: now,
        });
        await upsert(rawDb.outbound_pipeline_items, {
          id: pipelineId,
          campaign_id: campaignId,
          company_id: `company_${suffix}`,
          company_name: 'Smoke GmbH',
          stage: 'lead_qualified',
          contact_research_status: 'qualified',
          outreach_status: 'qualified',
          priority: 'high',
          contacts: [{
            id: `contact_${suffix}`,
            name: 'Erika Smoke',
            email: recipientEmail,
          }],
          contact_id: `contact_${suffix}`,
          contact_name: 'Erika Smoke',
          contact_email: recipientEmail,
          lead_name: 'Erika Smoke',
          lead_status: 'qualified',
          payload: {
            company_id: `company_${suffix}`,
            contact_id: `contact_${suffix}`,
            contact_name: 'Erika Smoke',
            contact_email: recipientEmail,
            company_name: 'Smoke GmbH',
            lead_name: 'Erika Smoke',
          },
          created_at_ms: now,
          updated_at_ms: now,
        });

        await bounded(commandBridge?.awaitInSync?.(), 20000);
        await waitFor(async () => {
          const button = document.querySelector(`[data-action="select-campaign"][data-id="${css(campaignId)}"]`);
          return {
            ok: Boolean(button),
            campaignCards: document.querySelectorAll('.outbound-campaign-item').length,
            bodyText: document.body?.innerText?.slice(0, 300) || '',
          };
        }, 15000, 'seeded campaign in left pane');
        click(`[data-action="select-campaign"][data-id="${css(campaignId)}"]`, 'seeded campaign selector');
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="import-source"][data-id="${css(campaignId)}"]`)),
          selected: document.querySelector('.outbound-campaign-item[aria-current="true"] strong')?.textContent?.trim() || '',
        }), 10000, 'seeded campaign selected');

        const outreachToggle = document.querySelector('[data-action="toggle-outreach"]');
        if (!outreachToggle) throw new Error('Active Outreach toggle not found');
        if (outreachToggle.getAttribute('aria-pressed') !== 'true') outreachToggle.click();
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-auto-draft"][data-lead-id="${css(pipelineId)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 500) || '',
        }), 15000, 'lead queue auto-draft action');
        await capture('outbound-active-01-lead-queue');

        click(`[data-action="ao-auto-draft"][data-lead-id="${css(pipelineId)}"]`, 'auto-draft button');
        const draftReady = await waitFor(async () => {
          const messages = await jsonDocs(rawDb.outbound_messages);
          const message = messages.find((doc) => doc.campaign_id === campaignId && doc.engagement_id);
          return {
            ok: Boolean(message && message.approval_status === 'awaiting_approval' && message.send_status === 'awaiting_approval'),
            message: message ? {
              id: message.id,
              approval_status: message.approval_status,
              send_status: message.send_status,
              subject: message.subject || '',
              channel: message.channel || '',
            } : null,
            count: messages.length,
          };
        }, 60000, 'approval-gated auto draft');
        const messageId = draftReady.message.id;
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-approve"][data-message-id="${css(messageId)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 700) || '',
        }), 15000, 'approval card rendered');
        await capture('outbound-active-02-approval-inbox');

        const preApprovalMessage = (await rawDb.outbound_messages.findOne(messageId).exec())?.toJSON?.();
        if (!preApprovalMessage || preApprovalMessage.send_status !== 'awaiting_approval') {
          throw new Error(`approval gate was not enforced before approval: ${JSON.stringify(preApprovalMessage)}`);
        }

        click(`[data-action="ao-approve"][data-message-id="${css(messageId)}"]`, 'approve message button');
        await waitFor(async () => {
          const message = (await rawDb.outbound_messages.findOne(messageId).exec())?.toJSON?.();
          return {
            ok: message?.approval_status === 'approved' && message?.send_status === 'approved_not_sent',
            approval_status: message?.approval_status || '',
            send_status: message?.send_status || '',
          };
        }, 60000, 'approved message ready to send');
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-send-approved"][data-message-id="${css(messageId)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 700) || '',
        }), 15000, 'ready-to-send action rendered');
        await capture('outbound-active-03-ready-to-send');

        click(`[data-action="ao-send-approved"][data-message-id="${css(messageId)}"]`, 'send approved button');
        const queued = await waitFor(async () => {
          const message = (await rawDb.outbound_messages.findOne(messageId).exec())?.toJSON?.();
          const commandDocs = (await jsonDocs(rawDb.business_commands))
            .filter((doc) => doc.module === 'outbound')
            .slice(-8)
            .map((doc) => ({
              id: doc.id,
              command_type: doc.command_type,
              record_id: doc.record_id,
              status: doc.status,
              error: doc.error || '',
            }));
          return {
            ok: message?.send_status === 'queued_for_provider' && Boolean(message?.provider_message_id || message?.payload?.provider_queue_id),
            send_status: message?.send_status || '',
            provider_message_id: message?.provider_message_id || message?.payload?.provider_queue_id || '',
            communication_message_key: message?.communication_message_key || message?.payload?.communication_message_key || '',
            errorBanner: document.querySelector('.outbound-outreach-error')?.textContent?.trim() || '',
            commands: commandDocs,
          };
        }, 60000, 'approved message queued in mailserver');
        click('[data-action="ao-view"][data-view="engagements"]', 'engagements tab');
        await waitFor(async () => ({
          ok: /Im Versand|scheduled_to_send|Versand/.test(document.querySelector('[data-outbound-root]')?.innerText || ''),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 900) || '',
        }), 15000, 'queued engagement visible');
        await capture('outbound-active-04-queued-engagement');

        const finalQueuedMessage = (await rawDb.outbound_messages.findOne(messageId).exec())?.toJSON?.();
        const engagementId = finalQueuedMessage?.engagement_id || '';
        const engagement = engagementId
          ? (await rawDb.outbound_engagements.findOne(engagementId).exec())?.toJSON?.()
          : (await jsonDocs(rawDb.outbound_engagements))
            .find((doc) => doc.campaign_id === campaignId && doc.payload?.pipeline_id === pipelineId);
        const approval = (await jsonDocs(rawDb.outbound_approvals))
          .find((doc) => doc.message_id === messageId);
        if (!engagement?.id) {
          throw new Error('queued outbound message did not produce an engagement');
        }
        const replyMessageKey = `email:${recipientEmail}:inbound:${suffix}`;
        const replyCommand = await dispatchNativeOutboundCommand('outbound.reply.match', engagement.id, {
          engagement_id: engagement.id,
          reply_message_id: replyMessageKey,
          outbound_message_id: messageId,
          classification: 'positive',
        }, 'positive reply matched to engagement');
        await waitFor(async () => {
          const doc = (await rawDb.outbound_engagements.findOne(engagement.id).exec())?.toJSON?.();
          return {
            ok: doc?.status === 'reply_received' && doc?.payload?.reply_classification === 'positive',
            status: doc?.status || '',
            classification: doc?.payload?.reply_classification || '',
          };
        }, 60000, 'positive reply visible in outbound engagement');
        click('[data-action="ao-view"][data-view="replies"]', 'replies tab');
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-draft-scheduling"][data-id="${css(engagement.id)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 900) || '',
        }), 15000, 'positive reply scheduling action rendered');
        await capture('outbound-active-05-positive-reply');

        click(`[data-action="ao-draft-scheduling"][data-id="${css(engagement.id)}"]`, 'prepare scheduling draft button');
        const schedulingReady = await waitFor(async () => {
          const messages = await jsonDocs(rawDb.outbound_messages);
          const message = messages
            .filter((doc) => doc.engagement_id === engagement.id && doc.message_type === 'scheduling')
            .sort((a, b) => (b.created_at_ms || 0) - (a.created_at_ms || 0))[0];
          const slots = Array.isArray(message?.payload?.proposed_slots) ? message.payload.proposed_slots : [];
          return {
            ok: Boolean(message && message.approval_status === 'awaiting_approval' && slots.length >= 1 && message.payload?.meeting_request_id),
            message: message ? {
              id: message.id,
              approval_status: message.approval_status,
              send_status: message.send_status,
              meeting_request_id: message.payload?.meeting_request_id || '',
              slots: slots.length,
            } : null,
            count: messages.length,
          };
        }, 60000, 'scheduling draft with proposed slots');
        const schedulingMessageId = schedulingReady.message.id;
        const meetingRequestId = schedulingReady.message.meeting_request_id;
        await waitFor(async () => ({
          ok: Boolean(document.querySelector(`[data-action="ao-book-slot"][data-meeting-request-id="${css(meetingRequestId)}"]`))
            && Boolean(document.querySelector(`[data-action="ao-approve"][data-message-id="${css(schedulingMessageId)}"]`)),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 1000) || '',
        }), 15000, 'scheduling approval card with bookable slot');
        await capture('outbound-active-06-scheduling-draft');

        const originalPrompt = window.prompt;
        window.prompt = () => 'https://meet.example.com/outbound-smoke';
        let booked;
        try {
          click(`[data-action="ao-book-slot"][data-meeting-request-id="${css(meetingRequestId)}"]`, 'book proposed slot button');
          booked = await waitFor(async () => {
            const request = (await rawDb.outbound_meeting_requests.findOne(meetingRequestId).exec())?.toJSON?.();
            const updatedEngagement = (await rawDb.outbound_engagements.findOne(engagement.id).exec())?.toJSON?.();
            return {
              ok: request?.status === 'booked' && updatedEngagement?.status === 'meeting_booked',
              request_status: request?.status || '',
              meeting_url: request?.meeting_url || '',
              engagement_status: updatedEngagement?.status || '',
            };
          }, 60000, 'meeting request booked from proposed slot');
        } finally {
          window.prompt = originalPrompt;
        }
        click('[data-action="ao-view"][data-view="done"]', 'done tab');
        await waitFor(async () => ({
          ok: /Termin gebucht|meeting_booked/.test(document.querySelector('[data-outbound-root]')?.innerText || ''),
          text: document.querySelector('[data-outbound-root]')?.innerText?.slice(0, 900) || '',
        }), 15000, 'booked engagement visible as done');
        await capture('outbound-active-07-meeting-booked');
        const conversationLink = document.querySelector('.outbound-outreach-conv-link')?.getAttribute('href') || '';
        return {
          mode: smokeMode,
          campaignId,
          pipelineId,
          engagementId: engagement?.id || '',
          messageId,
          schedulingMessageId,
          meetingRequestId,
          approvalId: approval?.id || '',
          approvalGateVerified: true,
          finalSendStatus: queued.send_status,
          providerMessageId: queued.provider_message_id,
          communicationMessageKey: queued.communication_message_key,
          replyMessageKey,
          replyClassification: replyCommand?.result?.classification || '',
          meetingStatus: booked.request_status,
          meetingUrl: booked.meeting_url,
          conversationLink,
          screenshotPaths,
          advancedStatusVersion,
          advancedStatusRuntime,
        };
      }

      if (smokeMode === 'outbound-active-ui') {
        return await runOutboundActiveUiSmoke();
      }

      if (smokeMode === 'coding-agents-ui') {
        return await runCodingAgentsUiSmoke();
      }

      if (smokeMode === 'business-os-roles-permissions-ui') {
        return await runBusinessOsRolesPermissionsUiSmoke();
      }

      if (smokeMode === 'business-os-dynamic-apps-ui') {
        return await runBusinessOsDynamicAppsUiSmoke();
      }

      if (smokeMode === 'business-os-app-release-ui') {
        return await runBusinessOsAppReleaseUiSmoke();
      }

      if (smokeMode === 'business-os-app-audience-ui') {
        return await runBusinessOsAppAudienceUiSmoke();
      }

      if (smokeMode === 'business-os-agent-scope-ui') {
        return await runBusinessOsAgentScopeUiSmoke();
      }

      if (smokeMode === 'business-os-threads-rightclick-ui') {
        return await runBusinessOsThreadsRightClickUiSmoke();
      }

      if (smokeMode === 'business-os-threads-scale-ui') {
        return await runBusinessOsThreadsScaleUiSmoke();
      }

      if (smokeMode === 'business-os-ui-regression') {
        return await runBusinessOsUiRegression();
      }

      if (ticketSmokeMode) {
        const now = Date.now();
        const id = `ticket_command_smoke_${now}`;
        const title = `WebRTC ticket smoke ${now}`;
        await db.business_commands.insert({
          id,
          command_id: id,
          module: 'tickets',
          command_type: 'ctox.ticket.local.create',
          record_id: '',
          status: 'pending_sync',
          inbound_channel: 'tickets',
          payload: {
            title,
            body: 'created by Business OS RxDB/WebRTC smoke',
            priority: 'medium',
            labels: ['smoke'],
          },
          client_context: smokeClientContext({ source: 'rxdb-ticket-smoke' }),
          updated_at_ms: now,
        });
        await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
        await bounded(appTicketItemReplicationState?.awaitInSync?.(), 25000);
        await bounded(appTicketEventReplicationState?.awaitInSync?.(), 25000);
        const deadline = Date.now() + 60000;
        while (Date.now() < deadline) {
          const commandDoc = await db.business_commands.findOne(id).exec();
          const command = commandDoc?.toJSON?.();
          const ticketDocs = (await db.ctox_ticket_items.find().exec()).map((doc) => doc.toJSON?.() || doc);
          const ticket = ticketDocs.find((doc) => doc.title === title && doc.source_system === 'local');
          if (command && command.status === 'completed' && ticket) {
            if (ticketClarificationSmokeMode) {
              const clarificationNow = Date.now();
              const clarificationCommandId = `ticket_clarification_command_smoke_${clarificationNow}`;
              const question = `Welche Kundennummer gehoert zu ${ticket.ticket_key}?`;
              await db.business_commands.insert({
                id: clarificationCommandId,
                command_id: clarificationCommandId,
                module: 'tickets',
                command_type: 'ctox.ticket.request_clarification',
                record_id: ticket.ticket_key || '',
                status: 'pending_sync',
                inbound_channel: 'tickets',
                payload: {
                  ticket_key: ticket.ticket_key,
                  target_type: 'requester',
                  target_channel: 'ticket',
                  question,
                  missing_inputs: ['customer_id', 'approval_context'],
                  unblock_criteria: 'Requester supplies customer_id and approval_context.',
                },
                client_context: smokeClientContext({ source: 'rxdb-ticket-clarification-smoke' }),
                updated_at_ms: clarificationNow,
              });
              await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
              await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 25000);
              const clarificationDeadline = Date.now() + 60000;
              while (Date.now() < clarificationDeadline) {
                const clarificationCommandDoc = await db.business_commands.findOne(clarificationCommandId).exec();
                const clarificationCommand = clarificationCommandDoc?.toJSON?.();
                const clarificationDocs = (await db.ctox_ticket_clarification_requests.find().exec()).map((doc) => doc.toJSON?.() || doc);
                const clarification = clarificationDocs.find((doc) => doc.ticket_key === ticket.ticket_key && doc.question === question);
                if (clarificationCommand?.status === 'completed' && clarification?.status) {
                  if (clarification.status === 'waiting_for_response') {
                    await Promise.all(replicationStates.map((state) => state.cancel?.()));
                    if (ownsDb) await db.close();
                    return {
                      mode: smokeMode,
                      createCommandId: id,
                      clarificationCommandId,
                      createStatus: command.status,
                      clarificationStatus: clarificationCommand.status,
                      publishStatus: 'already_waiting',
                      ticketKey: ticket.ticket_key || '',
                      clarificationId: clarification.clarification_id || '',
                      clarificationRequestStatus: clarification.status || '',
                      missingInputCount: Array.isArray(clarification.missing_inputs)
                        ? clarification.missing_inputs.length
                        : 0,
                    };
                  }
                  if (clarification.status === 'draft') {
                    const publishNow = Date.now();
                    const publishCommandId = `ticket_clarification_publish_smoke_${publishNow}`;
                    await db.business_commands.insert({
                      id: publishCommandId,
                      command_id: publishCommandId,
                      module: 'tickets',
                      command_type: 'ctox.ticket.publish_clarification',
                      record_id: clarification.clarification_id || '',
                      status: 'pending_sync',
                      inbound_channel: 'tickets',
                      payload: {
                        clarification_id: clarification.clarification_id,
                        reviewed_by: 'rxdb-ticket-clarification-smoke',
                        review_summary: 'Clarification question reviewed by browser smoke.',
                      },
                      client_context: smokeClientContext({ source: 'rxdb-ticket-clarification-smoke' }),
                      updated_at_ms: publishNow,
                    });
                    await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
                    await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 25000);
                    const publishDeadline = Date.now() + 60000;
                    while (Date.now() < publishDeadline) {
                      await bounded(appCommandReplicationState?.awaitInSync?.(), 5000);
                      await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 5000);
                      const publishCommandDoc = await db.business_commands.findOne(publishCommandId).exec();
                      const publishCommand = publishCommandDoc?.toJSON?.();
                      let latestClarification = (await db.ctox_ticket_clarification_requests.findOne(clarification.clarification_id).exec())?.toJSON?.();
                      if (
                        publishCommand?.status === 'completed'
                        && latestClarification?.status !== 'waiting_for_response'
                        && globalThis.ctoxBusinessOsSmoke?.state?.sync?.restartCollection
                      ) {
                        const refreshedBridge = await globalThis.ctoxBusinessOsSmoke.state.sync.restartCollection('ctox_ticket_clarification_requests');
                        appTicketClarificationReplicationState = refreshedBridge?.state || appTicketClarificationReplicationState;
                        await bounded(appTicketClarificationReplicationState?.awaitInitialReplication?.(), 10000);
                        await bounded(appTicketClarificationReplicationState?.awaitInSync?.(), 10000);
                        latestClarification = (await db.ctox_ticket_clarification_requests.findOne(clarification.clarification_id).exec())?.toJSON?.();
                      }
                      const publishedClarification = publishCommand?.result?.clarification || latestClarification || null;
                      if (publishCommand?.status === 'completed' && publishedClarification?.status === 'waiting_for_response') {
                        await Promise.all(replicationStates.map((state) => state.cancel?.()));
                        if (ownsDb) await db.close();
                        return {
                          mode: smokeMode,
                          createCommandId: id,
                          clarificationCommandId,
                          publishCommandId,
                          createStatus: command.status,
                          clarificationStatus: clarificationCommand.status,
                          publishStatus: publishCommand.status,
                          ticketKey: ticket.ticket_key || '',
                          clarificationId: publishedClarification.clarification_id || '',
                          clarificationRequestStatus: publishedClarification.status || '',
                          projectionStatus: latestClarification?.status || '',
                          outboundMessageKey: publishedClarification.outbound_message_key || '',
                          missingInputCount: Array.isArray(publishedClarification.missing_inputs)
                            ? publishedClarification.missing_inputs.length
                            : 0,
                        };
                      }
                      await delay(500);
                    }
                    const publishCommandDoc = await db.business_commands.findOne(publishCommandId).exec();
                    const latestClarification = (await db.ctox_ticket_clarification_requests.findOne(clarification.clarification_id).exec())?.toJSON?.();
                    throw new Error(`ticket clarification publish command ${publishCommandId} was not completed via RxDB/WebRTC: ${JSON.stringify({
                      command: publishCommandDoc?.toJSON?.() || null,
                      clarification: latestClarification || null,
                      syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
                    })}`);
                  }
                }
                await delay(500);
              }
              const clarificationCommandDoc = await db.business_commands.findOne(clarificationCommandId).exec();
              const clarificationDocs = (await db.ctox_ticket_clarification_requests.find({ limit: 10 }).exec()).map((doc) => doc.toJSON?.() || doc);
              throw new Error(`ticket clarification command ${clarificationCommandId} was not completed via RxDB/WebRTC: ${JSON.stringify({
                command: clarificationCommandDoc?.toJSON?.() || null,
                clarificationCount: clarificationDocs.length,
                clarifications: clarificationDocs.map((doc) => ({
                  clarification_id: doc.clarification_id || '',
                  ticket_key: doc.ticket_key || '',
                  status: doc.status || '',
                  question: doc.question || '',
                })),
                syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
              })}`);
            }
            await Promise.all(replicationStates.map((state) => state.cancel?.()));
            if (ownsDb) await db.close();
            return {
              mode: smokeMode,
              id,
              status: command.status,
              taskId: command.task_id || '',
              ticketKey: ticket.ticket_key || '',
              ticketSource: ticket.source_system || '',
              ticketTitle: ticket.title || '',
            };
          }
          await delay(500);
        }
        const commandDoc = await db.business_commands.findOne(id).exec();
        const ticketDocs = (await db.ctox_ticket_items.find({ limit: 10 }).exec()).map((doc) => doc.toJSON?.() || doc);
        throw new Error(`ticket command ${id} was not completed via RxDB/WebRTC: ${JSON.stringify({
          command: commandDoc?.toJSON?.() || null,
          ticketCount: ticketDocs.length,
          tickets: ticketDocs.map((doc) => ({
            title: doc.title || '',
            ticket_key: doc.ticket_key || '',
            source_system: doc.source_system || '',
          })),
          syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
          syncConfig: globalThis.ctoxBusinessOsSmoke?.state?.sync?.config || null,
        })}`);
      }

      if (commandSmokeMode) {
        if (smokeMode === 'command-burst-browser-to-rust') {
          const now = Date.now();
          const commandCount = Math.max(2, Number(globalThis.__ctoxCommandBurstCount || 5));
          const ids = Array.from({ length: commandCount }, (_, index) => `command_burst_smoke_${now}_${index}`);
          await Promise.all(ids.map((id, index) => db.business_commands.insert({
            id,
            command_id: id,
            module: 'ctox',
            command_type: 'business_os.smoke',
            record_id: '',
            status: 'pending_sync',
            inbound_channel: 'ctox',
            payload: { title: `WebRTC command burst smoke ${index + 1}`, instruction: 'smoke test only' },
            client_context: smokeClientContext({ source: 'rxdb-smoke', burst: true, index }),
            updated_at_ms: now + index,
          })));
          await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 25000);
          const deadline = Date.now() + 60000;
          const accepted = new Map();
          while (Date.now() < deadline) {
            for (const id of ids) {
              if (accepted.has(id)) continue;
              const commandDoc = await db.business_commands.findOne(id).exec();
              const command = commandDoc?.toJSON?.();
              const taskId = command?.task_id || '';
              if (!command || command.status === 'pending_sync' || !taskId) continue;
              const taskDoc = await db.ctox_queue_tasks.findOne(taskId).exec();
              const task = taskDoc?.toJSON?.();
              if (!task) continue;
              const queueTasksForCommand = (await db.ctox_queue_tasks.find().exec())
                .map((doc) => doc.toJSON?.() || doc)
                .filter((doc) => doc.command_id === id);
              if (queueTasksForCommand.length !== 1) {
                throw new Error(`command ${id} produced ${queueTasksForCommand.length} queue tasks: ${JSON.stringify(queueTasksForCommand)}`);
              }
              accepted.set(id, { taskId, status: command.status, taskStatus: command.task_status || task.status || '' });
            }
            if (accepted.size === ids.length) {
              await Promise.all(replicationStates.map((state) => state.cancel?.()));
              if (ownsDb) await db.close();
              return {
                mode: smokeMode,
                commandCount: ids.length,
                taskCountForCommands: accepted.size,
                ids,
                taskIds: [...accepted.values()].map((item) => item.taskId),
              };
            }
            await delay(500);
          }
          const commandDocs = await Promise.all(ids.map(async (id) => (await db.business_commands.findOne(id).exec())?.toJSON?.() || null));
          const queueDocs = (await db.ctox_queue_tasks.find().exec()).map((doc) => doc.toJSON?.() || doc);
          throw new Error(`command burst was not accepted via RxDB/WebRTC: ${JSON.stringify({
            commandCount: ids.length,
            acceptedCount: accepted.size,
            commands: commandDocs,
            queueCount: queueDocs.length,
          })}`);
        }
        const now = Date.now();
        const id = `command_smoke_${now}`;
        let officeRestartFixture = null;
        if (officeRestartSmokeMode) {
          if (!officeRestartFixtureBytes?.canonical?.length || !officeRestartFixtureBytes?.editor?.length) {
            throw new Error('Office restart fixture bytes were not supplied by the smoke host');
          }
          const canonicalBytes = new Uint8Array(officeRestartFixtureBytes.canonical);
          const editorBytes = new Uint8Array(officeRestartFixtureBytes.editor);
          const sha256 = async (bytes) => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)))
            .map((value) => value.toString(16).padStart(2, '0')).join('');
          const encodeBase64 = (bytes) => {
            let binary = '';
            for (let offset = 0; offset < bytes.length; offset += 0x8000) {
              binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
            }
            return btoa(binary);
          };
          const kind = officeRestartFixtureBytes.kind;
          const isDocument = kind === 'document';
          const config = isDocument ? {
            records: 'documents', versions: 'document_versions', chunks: 'document_blob_chunks',
            recordIdField: 'document_id', extension: 'docx', sourceKind: 'docx',
            mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
            protocol: 'euro-office-document-binary-v10', feature: 'document.edit-save', module: 'documents',
          } : {
            records: 'spreadsheets', versions: 'spreadsheet_versions', chunks: 'spreadsheet_blob_chunks',
            recordIdField: 'spreadsheet_id', extension: 'xlsx', sourceKind: 'xlsx',
            mime: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
            protocol: 'euro-office-cell-binary-v10', feature: 'spreadsheet.edit-save', module: 'spreadsheets',
          };
          const recordId = `office_restart_${isDocument ? 'doc' : 'sheet'}_${now}`;
          const versionId = `${recordId}_v1`;
          const canonicalBlobId = `${recordId}_canonical`;
          const editorBlobId = `${recordId}_editor`;
          const changedBlobId = `${recordId}_changed`;
          const canonicalSha256 = await sha256(canonicalBytes);
          const editorSha256 = await sha256(editorBytes);
          await db[config.records].insert({
            id: recordId,
            title: 'Office restart smoke',
            filename: `office-restart-smoke.${config.extension}`,
            mime_type: config.mime,
            ...(isDocument ? { document_type: 'word_document' } : {}),
            status: 'Imported',
            current_version_id: versionId,
            source_sha256: canonicalSha256,
            index_text: 'Office restart smoke',
            is_deleted: false,
            created_at_ms: now,
            updated_at_ms: now,
          });
          await db[config.versions].insert({
            id: versionId,
            [config.recordIdField]: recordId,
            version: 1,
            source_kind: config.sourceKind,
            blob_id: canonicalBlobId,
            editor_blob_id: editorBlobId,
            source_sha256: canonicalSha256,
            editor_sha256: editorSha256,
            editor_protocol: config.protocol,
            editor_protocol_version: 10,
            conversion_state: 'prepared',
            model_json: {},
            diagnostics: [],
            created_at_ms: now,
            updated_at_ms: now,
          });
          const chunk = (blobId, bytes, mimeType) => ({
            id: `${blobId}_0000`, blob_id: blobId, [config.recordIdField]: recordId, version_id: versionId,
            idx: 0, total: 1, mime_type: mimeType, encoding: 'base64', data: encodeBase64(bytes), created_at_ms: now,
          });
          await db[config.chunks].bulkInsert([
            chunk(canonicalBlobId, canonicalBytes, config.mime),
            chunk(editorBlobId, editorBytes, 'application/octet-stream'),
            chunk(changedBlobId, editorBytes, 'application/octet-stream'),
          ]);
          const projectionStates = isDocument ? appDocumentProjectionStates : appSpreadsheetProjectionStates;
          await Promise.all(projectionStates.map((state) => bounded(state?.awaitInSync?.(), 30000)));
          officeRestartFixture = { recordId, versionId, changedBlobId, editorSha256, canonicalSha256, kind, config };
        }
        if (smokeMode === 'command-restart-browser-to-rust') {
          if (useAppDb) {
            const repairedState = await repairAppFileAndCommandReplicationAfterNativeRestart();
            if (!repairedState?.db?.raw?.business_commands || !repairedState?.db?.raw?.ctox_queue_tasks) {
              throw new Error('Business OS command collections were not available after native peer restart');
            }
            db = repairedState.db.raw;
          } else {
            await globalThis.__ctoxRestartNativePeer?.();
          }
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appCommandReplicationState?.awaitInSync?.(), 30000);
          await bounded(appQueueReplicationState?.awaitInSync?.(), 30000);
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
        }
        if (smokeMode === 'command-midflight-restart-browser-to-rust' || officeRestartSmokeMode) {
          const commandBus = globalThis.ctoxBusinessOsSmoke?.state?.commandBus;
          if (!commandBus?.dispatch) throw new Error('Business OS command bus is not available for mid-flight restart smoke');
          const restartPromise = globalThis.__ctoxRestartNativePeer?.();
          await delay(50);
          const midflightCommand = officeRestartSmokeMode ? {
            id,
            module: officeRestartFixture.config.module,
            type: `office.${officeRestartFixture.kind}.commit`,
            record_id: officeRestartFixture.recordId,
            inbound_channel: 'ctox',
            payload: {
              [`${officeRestartFixture.kind}_id`]: officeRestartFixture.recordId,
              version_id: officeRestartFixture.versionId,
              base_version_id: officeRestartFixture.versionId,
              editor_blob_id: officeRestartFixture.changedBlobId,
              editor_sha256: officeRestartFixture.editorSha256,
              editor_protocol: officeRestartFixture.config.protocol,
              editor_protocol_version: 10,
              implemented_features: [officeRestartFixture.config.feature],
              reason: 'native-peer-restart-smoke',
            },
            client_context: { source: 'ctox-office-esm', surface: `business-os-${officeRestartFixture.config.module}`, transport: 'rxdb-webrtc' },
          } : {
            id,
            module: 'ctox',
            type: 'business_os.smoke',
            record_id: '',
            inbound_channel: 'ctox',
            payload: { title: 'WebRTC command restart smoke', instruction: 'smoke test only' },
            client_context: { source: 'rxdb-smoke', restart: 'midflight' },
          };
          const firstDispatch = commandBus.dispatch(midflightCommand).catch((error) => ({ error }));
          await restartPromise;
          if (useAppDb) {
            const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
            db = repairedState?.db?.raw || db;
            let repairedCommandBridge = null;
            let repairedQueueBridge = null;
            if (typeof repairedState?.sync?.suspendCollections === 'function'
              && typeof repairedState?.sync?.resumeCollections === 'function') {
              // A process restart wakes every active collection at once. Stop
              // that reconnect storm before repairing the two command-path
              // collections; otherwise their peer-open deadline can expire
              // behind ~170 unrelated collection joins under soak load.
              const activeCollections = Object.entries(repairedState?.syncDiagnostics?.collections || {})
                .filter(([, entry]) => !['stopped', 'paused'].includes(entry?.connectionStatus || entry?.status || ''))
                .map(([collection]) => collection);
              await repairedState.sync.suspendCollections([
                ...new Set([...activeCollections, 'business_commands', 'ctox_queue_tasks']),
              ], 'midflight-command-restart-repair');
              [repairedCommandBridge, repairedQueueBridge] = await repairedState.sync.resumeCollections([
                'business_commands',
                'ctox_queue_tasks',
              ]);
            } else if (typeof repairedState?.sync?.restartCollections === 'function') {
              [repairedCommandBridge, repairedQueueBridge] = await repairedState.sync.restartCollections([
                'business_commands',
                'ctox_queue_tasks',
              ]);
            } else {
              repairedCommandBridge = await repairedState?.sync?.restartCollection?.('business_commands')
                || await repairedState?.sync?.startCollection?.('business_commands');
              repairedQueueBridge = await repairedState?.sync?.restartCollection?.('ctox_queue_tasks')
                || await repairedState?.sync?.startCollection?.('ctox_queue_tasks');
            }
            appCommandReplicationState = repairedCommandBridge?.state || appCommandReplicationState;
            appQueueReplicationState = repairedQueueBridge?.state || appQueueReplicationState;
          }
          await bounded(appCommandReplicationState?.awaitInitialReplication?.(), 20000);
          await bounded(appQueueReplicationState?.awaitInitialReplication?.(), 20000);
          await waitForNativePeerOpen(appCommandReplicationState, 'business_commands');
          await waitForNativePeerOpen(appQueueReplicationState, 'ctox_queue_tasks');
          const firstResult = await firstDispatch;
          const existing = await commandBus.getStatus(id).catch(() => null);
          if (!existing) {
            await commandBus.dispatch(midflightCommand, { until: 'accepted', timeoutMs: 60000 });
          } else if (firstResult?.error || existing.status === 'pending_sync') {
            await commandBus.resumeTracking(id, { until: 'accepted', timeoutMs: 60000 });
          }
        } else {
          await db.business_commands.insert({
            id,
            command_id: id,
            module: 'ctox',
            command_type: 'business_os.smoke',
            record_id: '',
            status: 'pending_sync',
            inbound_channel: 'ctox',
            payload: { title: 'WebRTC command smoke', instruction: 'smoke test only' },
            client_context: smokeClientContext({ source: 'rxdb-smoke' }),
            updated_at_ms: now,
          });
        }
        await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
        await bounded(appQueueReplicationState?.awaitInSync?.(), 25000);
        const deadline = Date.now() + 45000;
        while (Date.now() < deadline) {
          const commandDoc = await db.business_commands.findOne(id).exec();
          const command = commandDoc?.toJSON?.();
          const taskId = command?.task_id || '';
          const officeTerminal = officeRestartSmokeMode
            && command?.status === 'completed'
            && command?.result?.ok === true;
          if (command && command.status !== 'pending_sync' && (taskId || officeTerminal)) {
            const taskDoc = taskId ? await db.ctox_queue_tasks.findOne(taskId).exec() : null;
            const task = taskDoc?.toJSON?.() || null;
            if (task || officeTerminal) {
              const queueTasksForCommand = (await db.ctox_queue_tasks.find().exec())
                .map((doc) => doc.toJSON?.() || doc)
                .filter((doc) => doc.command_id === id);
              const expectedQueueTasks = officeRestartSmokeMode ? 0 : 1;
              if (queueTasksForCommand.length !== expectedQueueTasks) {
                throw new Error(`command ${id} produced ${queueTasksForCommand.length} queue tasks, expected ${expectedQueueTasks}: ${JSON.stringify(queueTasksForCommand)}`);
              }
              let officeCommit = null;
              if (officeRestartSmokeMode) {
                const state = globalThis.ctoxBusinessOsSmoke?.state;
                const { records, versions, chunks } = officeRestartFixture.config;
                const resumed = await state.sync.resumeCollections([records, versions, chunks]);
                const officeProjectionStates = resumed.map((handle) => handle?.state || handle).filter(Boolean);
                if (officeDocumentRestartSmokeMode) appDocumentProjectionStates = officeProjectionStates;
                else appSpreadsheetProjectionStates = officeProjectionStates;
                await Promise.all(officeProjectionStates.map((replication) => bounded(replication?.awaitInitialReplication?.(), 30000)));
                await Promise.all(officeProjectionStates.map((replication) => bounded(replication?.awaitInSync?.(), 30000)));
                const deadline = Date.now() + 30000;
                let record = null;
                while (Date.now() < deadline) {
                  record = (await db[records].findOne(officeRestartFixture.recordId).exec())?.toJSON?.() || null;
                  if (record?.current_version_id && record.current_version_id !== officeRestartFixture.versionId) break;
                  await delay(250);
                }
                if (!record?.current_version_id || record.current_version_id === officeRestartFixture.versionId) {
                  throw new Error(`Office commit did not advance the ${officeRestartFixture.kind} version after restart: ${JSON.stringify(record)}`);
                }
                const committedVersion = (await db[versions].findOne(record.current_version_id).exec())?.toJSON?.() || null;
                const nativeChunkCount = Number(command?.result?.chunks || 0);
                if (!committedVersion || nativeChunkCount < 1) {
                  throw new Error(`Office commit projection is incomplete after restart: ${JSON.stringify({ record, committedVersion, nativeChunkCount })}`);
                }
                officeCommit = {
                  recordId: officeRestartFixture.recordId,
                  kind: officeRestartFixture.kind,
                  baseVersionId: officeRestartFixture.versionId,
                  versionId: record.current_version_id,
                  blobId: committedVersion.blob_id,
                  blobChunkCount: nativeChunkCount,
                };
              }
              await Promise.all(replicationStates.map((state) => state.cancel?.()));
              if (ownsDb) await db.close();
              return {
                mode: smokeMode,
                id,
                status: command.status,
                taskId,
                taskStatus: command.task_status || task?.status || command.status || '',
                taskCountForCommand: queueTasksForCommand.length,
                officeCommit,
              };
            }
          }
          await delay(500);
        }
        const commandDoc = await db.business_commands.findOne(id).exec();
        const queueDocs = await db.ctox_queue_tasks.find({ limit: 5 }).exec();
        const commandReplicationDiagnostics = appCommandReplicationState ? {
          cancelled: Boolean(appCommandReplicationState.cancelled),
          periodicPullIntervalMs: appCommandReplicationState.periodicPullIntervalMs?.() || 0,
          periodicPullTimerActive: Boolean(appCommandReplicationState.periodicPullTimer),
          pullInProgress: Boolean(appCommandReplicationState.pullInProgress),
          openPeerIds: appCommandReplicationState.openPeerIds?.() || [],
          pullCheckpoints: Array.from(appCommandReplicationState.pullCheckpointsByPeer?.entries?.() || []),
          pushCheckpoints: Array.from(appCommandReplicationState.pushCheckpointsByPeer?.entries?.() || []),
        } : null;
        throw new Error(`command ${id} was not accepted via RxDB/WebRTC: ${JSON.stringify({
          command: commandDoc?.toJSON?.() || null,
          queueCount: queueDocs.length,
          commandReplicationDiagnostics,
          syncMode: globalThis.ctoxBusinessOsSmoke?.state?.sync?.mode || '',
          syncConfig: globalThis.ctoxBusinessOsSmoke?.state?.sync?.config || null,
        })}`);
      }

      let hostFileChunkMutation = null;
      const workspacePhaseTimings = {};
      const received = smokeMode === 'restart-browser-to-rust'
        || smokeMode === 'restart-signaling-browser-to-rust'
        || smokeMode === 'rollover-native-peer-browser-to-rust'
        ? { payload: rustSeed.content, file: {} }
        : await (async () => {
            if (smokeMode === 'file-chunk-tombstone-error-browser-status') {
              await globalThis.__ctoxSyncRustSeedFile?.();
              hostFileChunkMutation = await globalThis.__ctoxTombstoneRustSeedChunk?.();
              return { payload: '', file: { id: rustSeed.id, content_state: 'available' } };
            }
            if (smokeMode !== 'workspace-agent-artifacts-background-rust-to-browser') {
              await globalThis.__ctoxSyncRustSeedFile?.();
            }
            if (smokeMode === 'workspace-agent-artifacts-rust-to-browser'
              || smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser'
              || smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser'
              || smokeMode === 'workspace-agent-artifacts-background-rust-to-browser') {
              const waitStartedAt = Date.now();
              const artifacts = await waitForWorkspaceArtifactsAuto(
                rustSeed.files || [],
                smokeMode === 'workspace-agent-artifacts-background-rust-to-browser' ? 90000 : 60000,
              );
              if (smokeMode === 'workspace-agent-artifacts-background-rust-to-browser') {
                workspacePhaseTimings.waitBackgroundArtifactsMs = Date.now() - waitStartedAt;
              }
              artifacts.backgroundQueueTask = backgroundQueueTask;
              artifacts.phaseTimings = workspacePhaseTimings;
              return artifacts;
            }
            if (smokeMode === 'workspace-large-materialize-rust-to-browser'
              || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
              || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
              const initialMetadataStartedAt = Date.now();
              try {
                return await waitForFileMetadata(rustSeed.id, 60000);
              } finally {
                setupPhaseTimings.initialMetadataMs = Date.now() - initialMetadataStartedAt;
              }
            }
            if (smokeMode === 'business-os-restore-resync-ui') {
              return waitForFileViaDemandFetch(rustSeed.id, 60000, rustSeed.content);
            }
            return waitForFileProduct(rustSeed.id, 60000, rustSeed.content);
          })();
      if (smokeMode === 'workspace-agent-artifacts-churn-rust-to-browser') {
        const phaseTimings = {};
        const mark = async (name, action) => {
          const started = Date.now();
          try {
            return await action();
          } finally {
            phaseTimings[name] = Date.now() - started;
          }
        };
        const initialByRelativePath = new Map(received.map((file) => [file.relativePath, file]));
        const mutation = await mark('mutateAndSyncMs', () => globalThis.__ctoxMutateRustWorkspaceArtifacts?.());
        const updatedRelativePaths = Array.isArray(mutation?.updatedRelativePaths) ? mutation.updatedRelativePaths : [];
        const addedRelativePaths = Array.isArray(mutation?.addedRelativePaths) ? mutation.addedRelativePaths : [];
        // The mutation lands via the CLI — an EXTERNAL SQLite write with no
        // in-process hook and a possibly-standby external poll (same reason
        // as the workspace-update mode): pull explicitly before waiting.
        await appFileReplicationState?.pullFromRemotePeers?.();
        await appChunkReplicationState?.pullFromRemotePeers?.();
        await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
        const changed = await mark('waitChangedArtifactsMs', () => waitForWorkspaceArtifactsAuto(mutation?.files || rustSeed.files || [], 90000));
        const changedByRelativePath = new Map(changed.map((file) => [file.relativePath, file]));
        const staleGenerations = [];
        for (const relativePath of updatedRelativePaths) {
          const before = initialByRelativePath.get(relativePath);
          const after = changedByRelativePath.get(relativePath);
          if (!before || !after || before.generationId === after.generationId) {
            staleGenerations.push({
              relativePath,
              before: before?.generationId || '',
              after: after?.generationId || '',
            });
          }
        }
        if (staleGenerations.length) {
          throw new Error(`workspace churn did not advance updated file generations: ${JSON.stringify(staleGenerations)}`);
        }
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          files: changed.map((file) => ({
            id: file.file?.id || '',
            payloadLength: file.payload.length,
            chunkCount: file.chunks.length,
            generationId: file.generationId || '',
            virtualPath: file.file?.virtual_path || file.file?.path || '',
            relativePath: file.relativePath || '',
          })),
          updatedRelativePaths,
          addedRelativePaths,
          updatedGenerationChanges: updatedRelativePaths.length,
          addedCount: addedRelativePaths.length,
          advancedStatusVersion,
          advancedStatusRuntime,
          phaseTimings,
        };
      }
      if (smokeMode === 'workspace-agent-artifacts-background-rust-to-browser') {
        const phaseTimings = received.phaseTimings || {};
        const advancedStatusStartedAt = Date.now();
        const advancedStatus = await globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
          timeoutMs: 60000,
          requiredCollections: [
            'business_module_catalog',
            'ctox_runtime_settings',
            'desktop_files',
            'desktop_file_chunks',
          ],
        });
        phaseTimings.advancedStatusMs = Date.now() - advancedStatusStartedAt;
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          backgroundQueueTask: received.backgroundQueueTask || null,
          advancedStatus,
          files: received.map((file) => ({
            id: file.file?.id || '',
            payloadLength: file.payload.length,
            chunkCount: file.chunks.length,
            generationId: file.generationId || '',
            virtualPath: file.file?.virtual_path || file.file?.path || '',
            relativePath: file.relativePath || '',
          })),
          phaseTimings,
        };
      }
      if (smokeMode === 'workspace-agent-artifacts-rust-to-browser'
        || smokeMode === 'workspace-agent-artifacts-stress-rust-to-browser') {
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          files: received.map((file) => ({
            id: file.file?.id || '',
            payloadLength: file.payload.length,
            chunkCount: file.chunks.length,
            generationId: file.generationId || '',
            virtualPath: file.file?.virtual_path || file.file?.path || '',
            relativePath: file.relativePath || '',
          })),
          advancedStatusVersion,
          advancedStatusRuntime,
        };
      }
      if ((smokeMode === 'workspace-rust-to-browser'
        || smokeMode === 'workspace-update-rust-to-browser'
        || smokeMode === 'workspace-large-materialize-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser'
        || smokeMode === 'file-chunk-metadata-error-browser-status'
        || smokeMode === 'file-chunk-stale-generation-error-browser-status')
        && rustSeed.expectedVirtualPath) {
        const actualPath = received.file?.virtual_path || received.file?.path || '';
        if (actualPath !== rustSeed.expectedVirtualPath) {
          throw new Error(`workspace file virtual path mismatch: ${JSON.stringify({
            expected: rustSeed.expectedVirtualPath,
            actual: actualPath,
            file: received.file,
          })}`);
        }
      }
      if (smokeMode === 'file-chunk-stale-generation-error-browser-status') {
        const hostCorruption = await globalThis.__ctoxStaleRustSeedChunkGeneration?.();
        // The corruption is a raw SQLite write on the native side: no
        // change-stream event ever fires for it (browser pulls are
        // event-driven), and the native index scan SELF-HEALS the
        // inconsistent row within one scan interval. Pull explicitly so the
        // browser observes the stale-generation state deterministically
        // before the repair lands.
        await appFileReplicationState?.pullFromRemotePeers?.();
        await appChunkReplicationState?.pullFromRemotePeers?.();
        await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
        const stale = await waitForStaleGenerationState(rustSeed.id, hostCorruption?.requestedGenerationId || '', 60000);
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'file-chunk-stale-generation-error');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-stale-generation-smoke-${Date.now()}`);
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const teardown = await viewer.mount(mount, {
          db: smokeState?.db || db,
          sync: smokeState?.sync,
          commandBus: smokeState?.commandBus,
          session: smokeState?.session,
          reportFileIntegrityError: (error, details = {}) => globalThis.ctoxBusinessOsSmoke?.reportFileIntegrityError?.('desktop-app:file-viewer', error, {
            appId: 'file-viewer',
            ...details,
          }),
          setTitle: () => {},
          args: {
            fileId: rustSeed.id,
            name: stale.file?.name || received.file?.name || 'brief.md',
            mimeType: stale.file?.mime_type || received.file?.mime_type || 'text/markdown',
            sizeBytes: stale.file?.size_bytes || received.file?.size_bytes || rustSeed.content.length,
            path: stale.file?.local_path || stale.file?.path || '',
            contentState: stale.file?.content_state || 'available',
            contentGenerationId: stale.file?.content_generation_id || '',
            contentHash: stale.file?.content_hash || received.file?.content_hash || '',
            contentHashScheme: stale.file?.content_hash_scheme || received.file?.content_hash_scheme || '',
          },
        });
        const deadline = Date.now() + 30000;
        let errorText = '';
        while (Date.now() < deadline) {
          errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText.includes('Dateiinhalt ist unvollständig oder beschädigt.')) break;
          await delay(250);
        }
        try { teardown?.(); } catch {}
        mount.remove();
        if (!errorText.includes('Dateiinhalt ist unvollständig oder beschädigt.')) {
          throw new Error(`file viewer did not reject stale file chunk generation: ${JSON.stringify({
            errorText,
            hostCorruption,
            stale: {
              file: stale.file,
              liveChunkCount: hostCorruption?.liveChunkCount || 0,
              requestedGenerationChunkCount: 0,
              availableGenerationIds: hostCorruption?.availableGenerationIds || [],
            },
          })}`);
        }
        const status = await waitForFileIntegrityStatus('ctox_file_chunk_integrity_mismatch', 30000);
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          fileIntegrityName: status.error.name,
          fileIntegrityCode: status.error.code,
          fileIntegrityPhase: status.error.phase || '',
          fileIntegritySource: status.error.source || '',
          advancedStatusVersion,
          advancedStatusRuntime,
          requestedGenerationId: stale.file?.content_generation_id || hostCorruption?.requestedGenerationId || '',
          requestedGenerationChunkCount: 0,
          liveChunkCount: hostCorruption?.liveChunkCount || 0,
          availableGenerationIds: hostCorruption?.availableGenerationIds || [],
        };
      }
      if (smokeMode === 'file-chunk-tombstone-error-browser-status') {
        let hostCorruption = hostFileChunkMutation || await globalThis.__ctoxTombstoneRustSeedChunk?.();
        if (!appFileReplicationState || !appChunkReplicationState) {
          await startDeferredAppFileCollections('file-chunk-tombstone-error');
        }
        let tombstoned = null;
        let tombstoneWaitError = null;
        for (let attempt = 0; attempt < 3; attempt += 1) {
          if (attempt > 0) {
            hostCorruption = await globalThis.__ctoxTombstoneRustSeedChunk?.();
            if (typeof appFileReplicationState?.reSync === 'function') appFileReplicationState.reSync();
            if (typeof appChunkReplicationState?.reSync === 'function') appChunkReplicationState.reSync();
          }
          await tombstoneLocalFileCache(rustSeed.id);
          await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
          try {
            tombstoned = await waitForTombstonedChunkState(rustSeed.id, attempt === 0 ? 15000 : 30000);
            tombstoneWaitError = null;
            break;
          } catch (error) {
            tombstoneWaitError = error;
          }
        }
        if (!tombstoned) throw tombstoneWaitError || new Error('browser did not receive tombstoned chunk state');
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'file-chunk-tombstone-error');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-tombstone-smoke-${Date.now()}`);
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const teardown = await viewer.mount(mount, {
          db: smokeState?.db || db,
          sync: smokeState?.sync,
          commandBus: smokeState?.commandBus,
          session: smokeState?.session,
          reportFileIntegrityError: (error, details = {}) => globalThis.ctoxBusinessOsSmoke?.reportFileIntegrityError?.('desktop-app:file-viewer', error, {
            appId: 'file-viewer',
            ...details,
          }),
          setTitle: () => {},
          args: {
            fileId: rustSeed.id,
            name: tombstoned.file?.name || received.file?.name || 'brief.md',
            mimeType: tombstoned.file?.mime_type || received.file?.mime_type || 'text/markdown',
            sizeBytes: tombstoned.file?.size_bytes || received.file?.size_bytes || rustSeed.content.length,
            path: tombstoned.file?.local_path || tombstoned.file?.path || '',
            contentState: tombstoned.file?.content_state || 'available',
            contentGenerationId: tombstoned.file?.content_generation_id || received.file?.content_generation_id || '',
            contentHash: tombstoned.file?.content_hash || received.file?.content_hash || '',
            contentHashScheme: tombstoned.file?.content_hash_scheme || received.file?.content_hash_scheme || '',
          },
        });
        const deadline = Date.now() + 30000;
        let errorText = '';
        while (Date.now() < deadline) {
          errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText.includes('Dateiinhalt ist unvollständig oder beschädigt.')) break;
          await delay(250);
        }
        try { teardown?.(); } catch {}
        mount.remove();
        if (!errorText.includes('Dateiinhalt ist unvollständig oder beschädigt.')) {
          throw new Error(`file viewer did not reject tombstoned active chunk: ${JSON.stringify({
            errorText,
            hostCorruption,
            tombstoned: {
              file: tombstoned.file,
              liveChunkCount: 0,
              tombstonedChunkCount: 1,
            },
          })}`);
        }
        const status = await waitForFileIntegrityStatus('ctox_file_chunk_integrity_mismatch', 30000);
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          fileIntegrityName: status.error.name,
          fileIntegrityCode: status.error.code,
          fileIntegrityPhase: status.error.phase || '',
          fileIntegritySource: status.error.source || '',
          advancedStatusVersion,
          advancedStatusRuntime,
          liveChunkCount: 0,
          tombstonedChunkCount: 1,
          chunkId: hostCorruption?.chunkRowId || '',
        };
      }
      if (smokeMode === 'file-chunk-metadata-error-browser-status') {
        const hostCorruption = await globalThis.__ctoxCorruptRustSeedChunkMetadata?.();
        await bounded(appChunkReplicationState?.awaitInSync?.(), 30000);
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'file-chunk-metadata-error');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-integrity-smoke-${Date.now()}`);
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const teardown = await viewer.mount(mount, {
          db: smokeState?.db || db,
          sync: smokeState?.sync,
          commandBus: smokeState?.commandBus,
          session: smokeState?.session,
          reportFileIntegrityError: (error, details = {}) => globalThis.ctoxBusinessOsSmoke?.reportFileIntegrityError?.('desktop-app:file-viewer', error, {
            appId: 'file-viewer',
            ...details,
          }),
          setTitle: () => {},
          args: {
            fileId: rustSeed.id,
            name: received.file?.name || 'brief.md',
            mimeType: received.file?.mime_type || 'text/markdown',
            sizeBytes: received.file?.size_bytes || rustSeed.content.length,
            path: received.file?.local_path || received.file?.path || rustSeed.path,
            contentState: received.file?.content_state || '',
            contentGenerationId: received.file?.content_generation_id || '',
            contentHash: received.file?.content_hash || '',
            contentHashScheme: received.file?.content_hash_scheme || '',
          },
        });
        const deadline = Date.now() + 30000;
        let errorText = '';
        while (Date.now() < deadline) {
          errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText.includes('Dateiinhalt ist unvollständig oder beschädigt.')) break;
          await delay(250);
        }
        try { teardown?.(); } catch {}
        mount.remove();
        if (!errorText.includes('Dateiinhalt ist unvollständig oder beschädigt.')) {
          throw new Error(`file viewer did not reject corrupt chunk metadata: ${JSON.stringify({
            errorText,
            hostCorruption,
          })}`);
        }
        const status = await waitForFileIntegrityStatus('ctox_file_chunk_integrity_mismatch', 30000);
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          fileIntegrityName: status.error.name,
          fileIntegrityCode: status.error.code,
          fileIntegrityPhase: status.error.phase || '',
          fileIntegritySource: status.error.source || '',
          advancedStatusVersion,
          advancedStatusRuntime,
          chunkId: hostCorruption?.rowId || '',
          expectedSizeBytes: hostCorruption?.expectedSizeBytes,
          actualSizeBytes: hostCorruption?.actualSizeBytes,
        };
      }
      if (smokeMode === 'workspace-large-materialize-rust-to-browser') {
        if (received.file?.content_state !== 'lazy') {
          throw new Error(`large workspace file was not indexed lazily: ${JSON.stringify(received.file)}`);
        }
        if (received.chunks.length !== 0) {
          throw new Error(`large workspace file wrote eager chunks before materialize: ${received.chunks.length}`);
        }
        const commandId = `materialize_smoke_${Date.now()}`;
        const materializeCommand = {
          id: commandId,
          module: 'ctox',
          type: 'ctox.file.materialize',
          record_id: rustSeed.id,
          inbound_channel: 'ctox',
          payload: {
            file_id: rustSeed.id,
            path: received.file?.local_path || received.file?.path || rustSeed.path,
          },
          client_context: { source: 'rxdb-smoke', materialize: true },
        };
        if (useAppDb && appState?.commandBus?.dispatch) {
          await appState.commandBus.dispatch(materializeCommand, {
            until: 'terminal',
            timeoutMs: 90000,
          });
        } else {
          await db.business_commands.insert({
            ...materializeCommand,
            command_id: commandId,
            command_type: materializeCommand.type,
            status: 'pending_sync',
            client_context: smokeClientContext(materializeCommand.client_context),
            updated_at_ms: Date.now(),
          });
        }
        await bounded(appCommandReplicationState?.awaitInSync?.(), 25000);
        await bounded(appFileReplicationState?.awaitInSync?.(), 25000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 25000);
        const materialized = await waitForFileProduct(rustSeed.id, 90000, rustSeed.content);
        if (materialized.file?.content_state !== 'available') {
          throw new Error(`large workspace file did not become available after materialize: ${JSON.stringify(materialized.file)}`);
        }
        const commandDoc = await db.business_commands.findOne(commandId).exec();
        const command = commandDoc?.toJSON?.() || null;
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          commandId,
          commandStatus: command?.status || '',
          payloadLength: materialized.payload.length,
          chunkCount: materialized.chunks.length,
          generationId: materialized.generationId || '',
          virtualPath: materialized.file?.virtual_path || materialized.file?.path || '',
          phaseTimings: setupPhaseTimings,
        };
      }
      if (smokeMode === 'workspace-large-file-viewer-rust-to-browser'
        || smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
        const phaseTimings = { ...setupPhaseTimings };
        const mark = () => performance.now();
        if (received.file?.content_state !== 'lazy') {
          throw new Error(`large workspace file was not indexed lazily for file viewer: ${JSON.stringify(received.file)}`);
        }
        if (received.chunks.length !== 0) {
          throw new Error(`large workspace file wrote eager chunks before file viewer materialize: ${received.chunks.length}`);
        }
        if (smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser') {
          const restartStartedAt = mark();
          await repairAppFileAndCommandReplicationAfterNativeRestart();
          phaseTimings.restartMs = Math.round(mark() - restartStartedAt);
          const waitCollectionsStartedAt = mark();
          await waitForAppSyncCollections([
            'business_commands',
            'desktop_files',
            'desktop_file_chunks',
          ]);
          phaseTimings.waitCollectionsMs = Math.round(mark() - waitCollectionsStartedAt);
        }
        const mount = document.createElement('section');
        mount.setAttribute('data-file-viewer-smoke', 'true');
        mount.style.cssText = 'position:fixed;left:0;top:0;width:760px;height:560px;z-index:99999;background:#0b1117;';
        document.body.append(mount);
        const importStartedAt = mark();
        const viewer = await import(`/desktop-apps/file-viewer/app.js?v=file-viewer-smoke-${Date.now()}`);
        phaseTimings.importViewerMs = Math.round(mark() - importStartedAt);
        const viewerMimeType = received.file?.mime_type || 'text/markdown';
        const previewRange = viewer.__fileViewerTestHooks?.textPreviewRangeFor?.(
          viewerMimeType,
          received.file?.size_bytes || rustSeed.content.length,
        );
        const expectedPreview = previewRange
          ? rustSeed.content.slice(Number(previewRange.offset || 0), Number(previewRange.offset || 0) + Number(previewRange.length || 0))
          : rustSeed.content;
        const smokeState = globalThis.ctoxBusinessOsSmoke?.state;
        const mountStartedAt = mark();
        const teardown = await viewer.mount(mount, {
          db: smokeState?.db || db,
          sync: smokeState?.sync,
          commandBus: smokeState?.commandBus,
          session: smokeState?.session,
          setTitle: () => {},
          args: {
            fileId: rustSeed.id,
            name: rustSeed.name || 'brief.md',
            mimeType: viewerMimeType,
            sizeBytes: received.file?.size_bytes || rustSeed.content.length,
            path: received.file?.local_path || received.file?.path || rustSeed.path,
            contentState: received.file?.content_state || '',
            contentGenerationId: received.file?.content_generation_id || '',
          },
        });
        phaseTimings.mountViewerMs = Math.round(mark() - mountStartedAt);
        const deadline = Date.now() + 120000;
        const renderStartedAt = mark();
        let text = '';
        while (Date.now() < deadline) {
          const pre = mount.querySelector('[data-file-text]');
          text = pre?.textContent || '';
          if (text === expectedPreview) break;
          const errorText = mount.querySelector('.is-error')?.textContent || '';
          if (errorText) {
            throw new Error(`file viewer failed to materialize large file: ${JSON.stringify({
              errorText,
              diagnostics: await fileViewerMaterializeDiagnostics(rustSeed.id),
            }, null, 2)}`);
          }
          await delay(500);
        }
        phaseTimings.renderPayloadMs = Math.round(mark() - renderStartedAt);
        try { teardown?.(); } catch {}
        mount.remove();
        if (text !== expectedPreview) {
          throw new Error(`file viewer did not render materialized payload: ${JSON.stringify({
            expectedLength: expectedPreview.length,
            actualLength: text.length,
            prefix: text.slice(0, 80),
          })}`);
        }
        const waitForFileStartedAt = mark();
        const materialized = await waitForFileProduct(rustSeed.id, 30000, rustSeed.content);
        phaseTimings.waitForFileMs = Math.round(mark() - waitForFileStartedAt);
        const desktopViewerStartedAt = mark();
        const desktopViewer = await openDesktopFileViewerAndWait({
          fileId: rustSeed.id,
          name: rustSeed.name || 'brief.md',
          mimeType: materialized.file?.mime_type || received.file?.mime_type || 'text/markdown',
          sizeBytes: materialized.file?.size_bytes || received.file?.size_bytes || rustSeed.content.length,
          path: materialized.file?.local_path || materialized.file?.path || received.file?.local_path || received.file?.path || rustSeed.path,
          contentState: materialized.file?.content_state || 'available',
          contentGenerationId: materialized.file?.content_generation_id || materialized.generationId || '',
          contentHash: materialized.file?.content_hash || '',
          contentHashScheme: materialized.file?.content_hash_scheme || '',
        }, expectedPreview);
        phaseTimings.desktopViewerRenderMs = Math.round(mark() - desktopViewerStartedAt);
        const advancedStatusStartedAt = mark();
        const advancedStatus = smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser'
          ? await globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
              timeoutMs: 90000,
              requiredCollections: [
                'business_module_catalog',
                'ctox_runtime_settings',
                'business_commands',
                'ctox_queue_tasks',
                'desktop_files',
                'desktop_file_chunks',
              ],
            })
          : null;
        phaseTimings.advancedStatusMs = Math.round(mark() - advancedStatusStartedAt);
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          payloadLength: materialized.payload.length,
          previewLength: text.length,
          chunkCount: materialized.chunks.length,
          generationId: materialized.generationId || '',
          virtualPath: materialized.file?.virtual_path || materialized.file?.path || '',
          restarted: smokeMode === 'workspace-large-file-viewer-restart-rust-to-browser',
          desktopViewer,
          advancedStatus,
          phaseTimings,
        };
      }

      async function fileViewerMaterializeDiagnostics(fileId) {
        const docsToJson = (docs) => (docs || []).map((doc) => doc?.toJSON?.() || doc);
        const fileDoc = await db.desktop_files?.findOne(fileId).exec();
        const file = fileDoc?.toJSON?.() || null;
        const chunks = docsToJson(await db.desktop_file_chunks?.find().exec())
          .filter((chunk) => chunk?.file_id === fileId);
        const commands = docsToJson(await db.business_commands?.find().exec())
          .filter((command) => command?.record_id === fileId || command?.payload?.file_id === fileId)
          .map((command) => ({
            id: command.id || command.command_id || '',
            type: command.type || command.command_type || '',
            status: command.status || '',
            error: command.error || command.result?.error || '',
            updated_at_ms: command.updated_at_ms || null,
          }));
        const advancedStatus = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
          requiredCollections: ['business_commands', 'desktop_files', 'desktop_file_chunks'],
        }).catch((error) => ({ error: error?.message || String(error) }));
        return {
          file: file ? {
            id: file.id,
            content_state: file.content_state || '',
            content_generation_id: file.content_generation_id || '',
            local_path: file.local_path || '',
            path: file.path || '',
            updated_at_ms: file.updated_at_ms || null,
          } : null,
          liveChunkCount: chunks.filter((chunk) => !chunk?._deleted).length,
          chunkCount: chunks.length,
          commands,
          advancedStatus,
        };
      }
      if (smokeMode === 'workspace-update-rust-to-browser') {
        const updatedContent = `${rustSeed.content}\nupdated via workspace smoke ${Date.now()}`;
        const update = await globalThis.__ctoxUpdateRustSeedFile?.(updatedContent);
        // The update lands in the native store via the CLI — an EXTERNAL
        // SQLite write. In-process change hooks do not fire for it, and the
        // external write poll may be in idle standby, so the event-driven
        // relay is best-effort here. Pull explicitly (masterChangesSince
        // reads the native store at request time) so the browser observes
        // the update deterministically — same precedent as the
        // stale-generation mode below.
        await appFileReplicationState?.pullFromRemotePeers?.();
        await appChunkReplicationState?.pullFromRemotePeers?.();
        await bounded(appFileReplicationState?.awaitInSync?.(), 30000);
        const updated = await waitForFileProduct(rustSeed.id, 60000, updatedContent, {
          notGenerationId: received.generationId || '',
        });
        const updatedPath = updated.file?.virtual_path || updated.file?.path || '';
        if (rustSeed.expectedVirtualPath && updatedPath !== rustSeed.expectedVirtualPath) {
          throw new Error(`workspace updated file virtual path mismatch: ${JSON.stringify({
            expected: rustSeed.expectedVirtualPath,
            actual: updatedPath,
            file: updated.file,
          })}`);
        }
        if (received.generationId && updated.generationId && received.generationId === updated.generationId) {
          throw new Error(`workspace updated file reused content generation: ${JSON.stringify({
            previousGeneration: received.generationId,
            updatedGeneration: updated.generationId,
            update,
          })}`);
        }
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          previousPayload: received.payload,
          updatedPayload: updated.payload,
          previousGenerationId: received.generationId || '',
          updatedGenerationId: updated.generationId || '',
          virtualPath: updatedPath,
        };
      }
      if (smokeMode === 'rust-to-browser' || smokeMode === 'workspace-rust-to-browser') {
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id: rustSeed.id,
          payload: received.payload,
          virtualPath: received.file?.virtual_path || received.file?.path || '',
          advancedStatusVersion,
          advancedStatusRuntime,
        };
      }

      let peerCheckpointRefresh = null;
      if (smokeMode === 'business-os-restore-resync-ui') {
        if (!useAppDb) {
          throw new Error('business-os-restore-resync-ui requires the Business OS app DB');
        }
        const state = globalThis.ctoxBusinessOsSmoke?.state;
        if (!state?.sync || !appFileReplicationState || !appChunkReplicationState) {
          throw new Error('Business OS restore resync smoke requires active file replication states');
        }
        const peerSessionsBeforeStop = (await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
          includeCounts: false,
        }))?.sync?.peerSessions || [];
        const webrtcOnly = state.syncConfig?.transport === 'webrtc'
          && state.syncConfig?.http_bridge_available === false;
        if (!webrtcOnly) {
          throw new Error(`Business OS restore resync smoke requires WebRTC-only config: ${JSON.stringify(state.syncConfig)}`);
        }
        const moduleScriptBasePath = (mod) => {
          const entry = String(mod?.entry || `modules/${mod?.id}/index.html`)
            .replace(/^\.?\//, '')
            .split('?')[0]
            .split('#')[0];
          const slash = entry.lastIndexOf('/');
          return slash >= 0 ? entry.slice(0, slash) : `modules/${mod?.id}`;
        };
        const moduleRevisionQuery = (moduleId) => {
          const rev = state.moduleRevisions?.[moduleId];
          return rev ? `_${rev}` : '';
        };
        const appBuild = [...document.scripts]
          .map((script) => String(script.getAttribute('src') || script.src || ''))
          .map((src) => /(?:^|\/)app\.js\?v=([^&]+)/.exec(src)?.[1] || '')
          .find(Boolean);
        if (!appBuild) {
          throw new Error('restore-resync smoke could not derive Business OS app build cache buster');
        }
        const moduleScriptHrefs = [...new Set((state.modules || [])
          .filter((mod) => mod?.id)
          .map((mod) => `./${moduleScriptBasePath(mod)}/index.js?v=${appBuild}${moduleRevisionQuery(mod.id)}`))];
        await Promise.all(moduleScriptHrefs.map((href) => new Promise((resolve, reject) => {
          const existing = [...document.head.querySelectorAll('link[rel="modulepreload"]')]
            .find((link) => link.getAttribute('href') === href);
          if (existing?.dataset?.restoreResyncPreloaded === '1') {
            resolve();
            return;
          }
          const link = existing || document.createElement('link');
          const timeout = setTimeout(() => {
            reject(new Error(`restore-resync modulepreload timed out for ${href}`));
          }, 10000);
          link.addEventListener('load', () => {
            clearTimeout(timeout);
            link.dataset.restoreResyncPreloaded = '1';
            resolve();
          }, { once: true });
          link.addEventListener('error', () => {
            clearTimeout(timeout);
            reject(new Error(`restore-resync modulepreload failed for ${href}`));
          }, { once: true });
          if (!existing) {
            link.rel = 'modulepreload';
            link.setAttribute('href', href);
            document.head.append(link);
          } else {
            fetch(href, { cache: 'force-cache' })
              .then((response) => {
                if (!response.ok) throw new Error(`HTTP ${response.status}`);
                clearTimeout(timeout);
                existing.dataset.restoreResyncPreloaded = '1';
                resolve();
              })
              .catch((error) => {
                clearTimeout(timeout);
                reject(new Error(`restore-resync modulepreload refresh failed for ${href}: ${error?.message || error}`));
              });
          }
        })));

        await globalThis.__ctoxStopNativePeerForRestoreSmoke?.();
        const now = Date.now();
        const id = `business_os_restore_resync_${now}`;
        const encoded = btoa(browserPayload);
        await db.desktop_files.insert({
          id,
          path: `/browser/restore-resync/${id}.txt`,
          name: `${id}.txt`,
          kind: 'file',
          mime_type: 'text/plain',
          extension: 'txt',
          size_bytes: browserPayload.length,
          owner_id: 'business-os-restore-smoke',
          source: 'business-os-restore-resync-smoke',
          content_ref: id,
          sort_index: now,
          is_deleted: false,
          created_at_ms: now,
          updated_at_ms: now,
        });
        await db.desktop_file_chunks.insert({
          id: `${id}_0`,
          file_id: id,
          idx: 0,
          total: 1,
          encoding: 'base64',
          data: encoded,
          size_bytes: encoded.length,
          created_at_ms: now,
        });
        const nativePresentBeforeRestart = await globalThis.__ctoxSqliteFileExistsForRestoreSmoke?.(id);
        if (nativePresentBeforeRestart) {
          throw new Error(`restore-resync browser write reached native SQLite while native peer was stopped: ${id}`);
        }

        await globalThis.__ctoxStartNativePeerForRestoreSmoke?.();
        const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
        const repairedDb = repairedState?.db?.raw;
        if (!repairedDb?.desktop_files || !repairedDb?.desktop_file_chunks) {
          throw new Error('Business OS app DB was not available after restore resync peer restart');
        }
        db = repairedDb;
        const repairedBridges = typeof repairedState.sync.restartCollections === 'function'
          ? await repairedState.sync.restartCollections(['desktop_files', 'desktop_file_chunks'])
          : [
              await repairedState.sync.restartCollection('desktop_files'),
              await repairedState.sync.restartCollection('desktop_file_chunks'),
            ];
        appFileReplicationState = repairedBridges[0]?.state || null;
        appChunkReplicationState = repairedBridges[1]?.state || null;
        if (typeof appFileReplicationState?.reSync === 'function') appFileReplicationState.reSync();
        if (typeof appChunkReplicationState?.reSync === 'function') appChunkReplicationState.reSync();
        await bounded(appFileReplicationState?.awaitInitialReplication?.(), 30000);
        await bounded(appChunkReplicationState?.awaitInitialReplication?.(), 30000);
        await bounded(appFileReplicationState?.awaitInSync?.(), 60000);
        await bounded(appChunkReplicationState?.awaitInSync?.(), 60000);
        await waitForNativePeerOpen(appFileReplicationState, 'desktop_files restore resync');
        await waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks restore resync');
        const advancedStatusAfterRepair = await globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
          timeoutMs: 90000,
          requiredCollections: [
            'business_module_catalog',
            'ctox_runtime_settings',
            'desktop_files',
            'desktop_file_chunks',
          ],
        });
        const peerSessionsAfterRepair = advancedStatusAfterRepair?.sync?.peerSessions || [];
        const beforeByCollection = new Map(peerSessionsBeforeStop.map((session) => [session.collection, session]));
        const restartedSessions = peerSessionsAfterRepair.filter((session) => {
          const before = beforeByCollection.get(session.collection);
          return before
            && before.peerSession
            && session.peerSession
            && before.peerSession !== session.peerSession
            && Number(session.generation || 0) > Number(before.generation || 0);
        });
        const missingCheckpointEpoch = restartedSessions.filter((session) => (
          !session?.checkpoint
          || session.checkpoint.state !== 'advertised'
          || !session.checkpoint.epoch
          || session.checkpoint.collection !== session.collection
        ));
        if (!restartedSessions.length || missingCheckpointEpoch.length) {
          throw new Error(`Restore resync did not refresh peer checkpoint evidence: ${JSON.stringify({
            before: peerSessionsBeforeStop,
            after: peerSessionsAfterRepair,
            restartedSessions,
            missingCheckpointEpoch,
          }, null, 2)}`);
        }
        const nativeConvergenceDeadline = Date.now() + 90000;
        let nativeConverged = false;
        while (Date.now() < nativeConvergenceDeadline) {
          nativeConverged = await globalThis.__ctoxSqliteFileExistsForRestoreSmoke?.(id) === true;
          if (nativeConverged) break;
          if (typeof appFileReplicationState?.reSync === 'function') appFileReplicationState.reSync();
          if (typeof appChunkReplicationState?.reSync === 'function') appChunkReplicationState.reSync();
          await bounded(appFileReplicationState?.awaitInSync?.(), 5000);
          await bounded(appChunkReplicationState?.awaitInSync?.(), 5000);
          await delay(500);
        }
        if (!nativeConverged) {
          throw new Error(`restore resync browser-local write did not converge to native SQLite: ${id}`);
        }
        await Promise.all(replicationStates.map((state) => state.cancel?.()));
        if (ownsDb) await db.close();
        return {
          mode: smokeMode,
          id,
          readinessPayload: received.payload,
          browserPayload,
          authState: 'authenticated',
          actorRole: state.session?.user?.role || '',
          browserContext: 'clean',
          tenantScope: 'local-workspace',
          webrtcOnly,
          peerStopped: true,
          localOnlyBeforeRestart: nativePresentBeforeRestart === false,
          peerRestarted: true,
          preloadedModuleScripts: moduleScriptHrefs.length,
          nativeConvergedAfterRestart: nativeConverged === true,
          advancedStatusVersion: advancedStatusAfterRepair?.version || advancedStatusVersion,
          advancedStatusRuntime: advancedStatusAfterRepair?.rxdbRuntime || advancedStatusRuntime,
          peerCheckpointRefresh: {
            restartedCollections: restartedSessions.map((session) => session.collection),
            checkpointEpochs: restartedSessions.map((session) => session.checkpoint?.epoch || ''),
          },
          replicationDirections: {
            desktop_files: describeReplicationPool(appFileReplicationState),
            desktop_file_chunks: describeReplicationPool(appChunkReplicationState),
          },
        };
      }
      if (smokeMode === 'restart-browser-to-rust'
        || smokeMode === 'restart-signaling-browser-to-rust'
        || smokeMode === 'rollover-native-peer-browser-to-rust') {
        const requiredRestartCollections = [
          'business_module_catalog',
          'ctox_runtime_settings',
          'business_commands',
          'ctox_queue_tasks',
          'desktop_files',
          'desktop_file_chunks',
        ];
        let peerSessionsBeforeRestart = [];
        if (useAppDb) {
          const baselineDeadline = Date.now() + 60000;
          let baselineProblems = [];
          while (Date.now() < baselineDeadline) {
            const baselineStatus = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: false });
            peerSessionsBeforeRestart = baselineStatus?.sync?.peerSessions || [];
            const baselineByCollection = new Map(
              peerSessionsBeforeRestart.map((session) => [session.collection, session]),
            );
            baselineProblems = requiredRestartCollections.flatMap((collection) => {
              const session = baselineByCollection.get(collection);
              if (!session?.peerSession) return [`${collection}:missing_session`];
              if (!session.checkpoint
                || session.checkpoint.state !== 'advertised'
                || !session.checkpoint.epoch
                || session.checkpoint.collection !== collection) {
                return [`${collection}:checkpoint_not_advertised`];
              }
              return [];
            });
            if (baselineProblems.length === 0) break;
            await delay(500);
          }
          if (baselineProblems.length) {
            throw new Error(`Critical peer session baseline was incomplete before native peer restart: ${JSON.stringify({
              problems: baselineProblems,
              requiredRestartCollections,
              observedCollections: peerSessionsBeforeRestart.map((session) => session.collection),
              mode: smokeMode,
            }, null, 2)}`);
          }
        }
        if (smokeMode === 'rollover-native-peer-browser-to-rust') {
          await globalThis.__ctoxRolloverNativePeerInProcess?.();
        } else if (smokeMode === 'restart-signaling-browser-to-rust') {
          await globalThis.__ctoxRestartSignalingAndNativePeer?.();
        } else {
          await globalThis.__ctoxRestartNativePeer?.();
        }
        if (useAppDb) {
          const repairedState = globalThis.ctoxBusinessOsSmoke?.state;
          const repairedDb = repairedState?.db?.raw;
          if (!repairedDb?.desktop_files || !repairedDb?.desktop_file_chunks) {
            throw new Error('Business OS app DB was not available after reconnect repair');
          }
          db = repairedDb;
          const repairedBridges = typeof repairedState.sync.restartCollections === 'function'
            ? await repairedState.sync.restartCollections(['desktop_files', 'desktop_file_chunks'])
            : [
                await repairedState.sync.restartCollection('desktop_files'),
                await repairedState.sync.restartCollection('desktop_file_chunks'),
              ];
          const repairedFileBridge = repairedBridges[0];
          const repairedChunkBridge = repairedBridges[1];
          appFileReplicationState = repairedFileBridge?.state || null;
          appChunkReplicationState = repairedChunkBridge?.state || null;
          await Promise.all([
            bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000),
            bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000),
          ]);
          await Promise.all([
            bounded(appFileReplicationState?.awaitInSync?.(), 30000),
            bounded(appChunkReplicationState?.awaitInSync?.(), 30000),
          ]);
          await Promise.all([
            waitForNativePeerOpen(appFileReplicationState, 'desktop_files after native peer restart'),
            waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks after native peer restart'),
          ]);
          let advancedStatusAfterRepair = await globalThis.CTOX_BUSINESS_OS_STATUS?.waitForHealthy?.({
            timeoutMs: 60000,
            requiredCollections: requiredRestartCollections,
          });
          const beforeByCollection = new Map(peerSessionsBeforeRestart.map((session) => [session.collection, session]));
          const restartEvidenceDeadline = Date.now() + 60000;
          let restartedSessions = [];
          let restartEvidenceProblems = [];
          while (Date.now() < restartEvidenceDeadline) {
            const peerSessionsAfterRepair = advancedStatusAfterRepair?.sync?.peerSessions || [];
            const afterByCollection = new Map(peerSessionsAfterRepair.map((session) => [session.collection, session]));
            restartEvidenceProblems = [];
            restartedSessions = [];
            for (const collection of requiredRestartCollections) {
              const before = beforeByCollection.get(collection);
              const after = afterByCollection.get(collection);
              if (!before?.peerSession || !after?.peerSession) {
                restartEvidenceProblems.push(`${collection}:missing_session`);
                continue;
              }
              if (before.peerSession === after.peerSession
                || Number(after.generation || 0) <= Number(before.generation || 0)) {
                restartEvidenceProblems.push(`${collection}:generation_not_advanced`);
                continue;
              }
              if (!after.checkpoint
                || after.checkpoint.state !== 'advertised'
                || !after.checkpoint.epoch
                || after.checkpoint.collection !== collection) {
                restartEvidenceProblems.push(`${collection}:checkpoint_not_advertised`);
                continue;
              }
              restartedSessions.push(after);
            }
            if (restartEvidenceProblems.length === 0) break;
            await delay(500);
            advancedStatusAfterRepair = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: false });
          }
          if (restartEvidenceProblems.length) {
            const summarizeCriticalPeerSessions = (sessions) => requiredRestartCollections.map((collection) => {
              const session = (Array.isArray(sessions) ? sessions : [])
                .find((item) => item?.collection === collection);
              return {
                collection,
                peerSession: session?.peerSession || null,
                generation: Number(session?.generation || 0),
                previousPeerSession: session?.previousPeerSession || null,
                checkpoint: session?.checkpoint || null,
                seenAt: session?.seenAt || null,
              };
            });
            throw new Error(`Critical peer sessions did not converge after native peer restart: ${JSON.stringify({
              problems: restartEvidenceProblems,
              requiredRestartCollections,
              before: summarizeCriticalPeerSessions(peerSessionsBeforeRestart),
              after: summarizeCriticalPeerSessions(advancedStatusAfterRepair?.sync?.peerSessions || []),
              status: {
                version: advancedStatusAfterRepair?.version || null,
                phase: advancedStatusAfterRepair?.sync?.phase || null,
                replicationUp: advancedStatusAfterRepair?.sync?.replicationUp === true,
                reconnectingCollections: advancedStatusAfterRepair?.sync?.reconnectingCollections || [],
                missingRequiredCollections: advancedStatusAfterRepair?.sync?.missingRequiredCollections || [],
                lastError: advancedStatusAfterRepair?.sync?.lastError || null,
                lastLifecycleEvent: advancedStatusAfterRepair?.sync?.lastLifecycleEvent || null,
              },
              mode: smokeMode,
            }, null, 2)}`);
          }
          peerCheckpointRefresh = {
            restartedCollections: restartedSessions.map((session) => session.collection),
            checkpointEpochs: restartedSessions.map((session) => session.checkpoint?.epoch || ''),
          };
          advancedStatusVersion = advancedStatusAfterRepair?.version || advancedStatusVersion;
          advancedStatusRuntime = advancedStatusAfterRepair?.rxdbRuntime || advancedStatusRuntime;
          const stableFileBridges = typeof repairedState.sync.restartCollections === 'function'
            ? await repairedState.sync.restartCollections(['desktop_files', 'desktop_file_chunks'])
            : [
                await repairedState.sync.restartCollection('desktop_files'),
                await repairedState.sync.restartCollection('desktop_file_chunks'),
              ];
          appFileReplicationState = stableFileBridges[0]?.state || appFileReplicationState;
          appChunkReplicationState = stableFileBridges[1]?.state || appChunkReplicationState;
          await Promise.all([
            bounded(appFileReplicationState?.awaitInitialReplication?.(), 20000),
            bounded(appChunkReplicationState?.awaitInitialReplication?.(), 20000),
          ]);
          await Promise.all([
            bounded(appFileReplicationState?.awaitInSync?.(), 30000),
            bounded(appChunkReplicationState?.awaitInSync?.(), 30000),
          ]);
          await Promise.all([
            waitForNativePeerOpen(appFileReplicationState, 'desktop_files after stable restart'),
            waitForNativePeerOpen(appChunkReplicationState, 'desktop_file_chunks after stable restart'),
          ]);
        }
      }

      const now = Date.now();
      const idPrefixByMode = {
        'restart-signaling-browser-to-rust': 'browser_signaling_restart_smoke',
        'rollover-native-peer-browser-to-rust': 'browser_rollover_smoke',
        'tab-freeze-browser-to-rust': 'browser_tab_freeze_smoke',
        'network-flap-browser-to-rust': 'browser_network_flap_smoke',
        'restart-browser-to-rust': 'browser_restart_smoke',
      };
      const id = `${idPrefixByMode[smokeMode] || 'browser_smoke'}_${now}`;
      const encoded = btoa(browserPayload);
      await db.desktop_files.insert({
        id,
        path: `/browser/smoke/${id}.txt`,
        name: `${id}.txt`,
        kind: 'file',
        mime_type: 'text/plain',
        extension: 'txt',
        size_bytes: browserPayload.length,
        owner_id: 'browser-smoke',
        source: 'browser-webrtc-smoke',
        content_ref: id,
        sort_index: now,
        is_deleted: false,
        created_at_ms: now,
        updated_at_ms: now,
      });
      await db.desktop_file_chunks.insert({
        id: `${id}_0`,
        file_id: id,
        idx: 0,
        total: 1,
        encoding: 'base64',
        data: encoded,
        size_bytes: encoded.length,
        created_at_ms: now,
      });
      if (typeof appFileReplicationState?.reSync === 'function') appFileReplicationState.reSync();
      if (typeof appChunkReplicationState?.reSync === 'function') appChunkReplicationState.reSync();
      await bounded(appFileReplicationState?.awaitInSync?.(), 25000);
      await bounded(appChunkReplicationState?.awaitInSync?.(), 25000);
      return {
        mode: smokeMode,
        id,
        readinessPayload: received.payload,
        browserPayload,
        advancedStatusVersion,
        advancedStatusRuntime,
        peerCheckpointRefresh,
        replicationDirections: {
          desktop_files: describeReplicationPool(appFileReplicationState),
          desktop_file_chunks: describeReplicationPool(appChunkReplicationState),
        },
      };
    }, { signalingUrl, smokeMode, rustSeed, useAppDb, browserPayload, backgroundQueueTask, advancedStatusEvidenceVersion, advancedStatusEvidenceRuntime, codingAgentSmoke, rolesPermissionsReloadVerified, dynamicAppsReloadVerified, appReleaseReloadVerified, appAudienceReloadVerified, threadsScaleSeed, threadsRightClickCapabilities, officeRestartFixtureBytes });
    outerPhaseTimings.pageEvaluateMs = Date.now() - pageEvaluateStartedAt;

    if (result.mode === 'business-os-ui-regression') {
      result.screenshotEvidence = await captureBusinessOsVisualScreenshotEvidence(page);
    }

    if (result.mode === 'workspace-agent-artifacts-rust-to-browser'
      || result.mode === 'workspace-agent-artifacts-stress-rust-to-browser'
      || result.mode === 'workspace-agent-artifacts-churn-rust-to-browser'
      || result.mode === 'workspace-agent-artifacts-background-rust-to-browser') {
      const files = Array.isArray(result.files) ? result.files : [];
      const expectedFiles = Array.isArray(rustSeed.files) ? rustSeed.files : [];
      if (files.length !== expectedFiles.length) {
        throw new Error(`workspace artifact count mismatch: ${files.length} !== ${expectedFiles.length}`);
      }
      const totalChunkCount = files.reduce((sum, file) => sum + Number(file.chunkCount || 0), 0);
      const maxChunkCount = files.reduce((max, file) => Math.max(max, Number(file.chunkCount || 0)), 0);
      console.log(`replicated_count=${files.length}`);
      console.log(`replicated_ids=${files.map((file) => file.id).join(',')}`);
      console.log(`virtual_paths=${files.map((file) => file.virtualPath).join(',')}`);
      console.log(`payload_lengths=${files.map((file) => file.payloadLength).join(',')}`);
      console.log(`chunk_counts=${files.map((file) => file.chunkCount).join(',')}`);
      console.log(`total_chunk_count=${totalChunkCount}`);
      console.log(`max_chunk_count=${maxChunkCount}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
      if (result.mode === 'workspace-agent-artifacts-churn-rust-to-browser') {
        console.log(`updated_generation_changes=${Number(result.updatedGenerationChanges || 0)}`);
        console.log(`added_count=${Number(result.addedCount || 0)}`);
        console.log(`updated_relative_paths=${(result.updatedRelativePaths || []).join(',')}`);
        console.log(`added_relative_paths=${(result.addedRelativePaths || []).join(',')}`);
        if (result.phaseTimings) console.log(`phase_timings=${JSON.stringify(result.phaseTimings)}`);
      }
      if (result.mode === 'workspace-agent-artifacts-background-rust-to-browser') {
        if (result.advancedStatus) {
          assertHealthyAdvancedStatusContract(result.advancedStatus);
          console.log(`advanced_status=${result.advancedStatus.version}`);
          console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatus.rxdbRuntime || null)}`);
        }
        console.log(`background_indexer=1`);
        console.log(`background_queue_task_created=${result.backgroundQueueTask?.created ? 1 : 0}`);
        console.log(`background_queue_task_id=${result.backgroundQueueTask?.taskId || ''}`);
        if (result.phaseTimings) console.log(`phase_timings=${JSON.stringify(result.phaseTimings)}`);
      }
    } else if (result.mode === 'presence-merge-two-browsers') {
      for (const flag of [
        'presencePropagated',
        'presenceBidirectional',
        'mergeConverged',
        'nativeMergeConverged',
        'presenceClearedOnPeerClose',
      ]) {
        if (result[flag] !== true) throw new Error(`presence-merge evidence missing: ${flag}`);
        console.log(`${flag.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`)}=1`);
      }
    } else if (result.mode === 'rust-to-browser' || result.mode === 'workspace-rust-to-browser') {
      if (result.payload !== rustSeed.content) throw new Error(`browser payload mismatch: ${result.payload}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
      console.log(`replicated_id=${result.id}`);
      if (result.virtualPath) console.log(`virtual_path=${result.virtualPath}`);
      console.log(result.payload);
    } else if (result.mode === 'workspace-update-rust-to-browser') {
      if (result.updatedPayload !== rustSeed.content) {
        throw new Error(`browser updated payload mismatch: ${result.updatedPayload}`);
      }
      console.log(`replicated_id=${result.id}`);
      if (result.virtualPath) console.log(`virtual_path=${result.virtualPath}`);
      console.log(`previous_generation=${result.previousGenerationId}`);
      console.log(`updated_generation=${result.updatedGenerationId}`);
      console.log(result.updatedPayload);
    } else if (result.mode === 'workspace-large-materialize-rust-to-browser'
      || result.mode === 'workspace-large-file-viewer-rust-to-browser'
      || result.mode === 'workspace-large-file-viewer-restart-rust-to-browser') {
      if (result.payloadLength !== rustSeed.content.length) {
        throw new Error(`browser materialized payload length mismatch: ${result.payloadLength} !== ${rustSeed.content.length}`);
      }
      if (result.advancedStatus) {
        assertHealthyAdvancedStatusContract(result.advancedStatus);
        console.log(`advanced_status=${result.advancedStatus.version}`);
        console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatus.rxdbRuntime || null)}`);
      }
      console.log(`replicated_id=${result.id}`);
      if (result.commandId) console.log(`command_id=${result.commandId}`);
      if (result.commandStatus) console.log(`command_status=${result.commandStatus}`);
      if (result.virtualPath) console.log(`virtual_path=${result.virtualPath}`);
      console.log(`generation=${result.generationId}`);
      console.log(`chunk_count=${result.chunkCount}`);
      console.log(`payload_length=${result.payloadLength}`);
      if (Number(result.previewLength || 0) > 0) console.log(`preview_length=${result.previewLength}`);
      if (result.desktopViewer) {
        console.log(`file_viewer_desktop_window_count=${result.desktopViewer.viewerWindowCount || 0}`);
        console.log(`file_viewer_desktop_rendered_length=${result.desktopViewer.renderedLength || 0}`);
      }
      if (result.phaseTimings) console.log(`phase_timings=${JSON.stringify(result.phaseTimings)}`);
    } else if (result.mode === 'file-chunk-metadata-error-browser-status'
      || result.mode === 'file-chunk-tombstone-error-browser-status'
      || result.mode === 'file-chunk-stale-generation-error-browser-status') {
      console.log(`file_integrity_error_name=${result.fileIntegrityName}`);
      console.log(`file_integrity_error_code=${result.fileIntegrityCode}`);
      console.log(`file_integrity_error_phase=${result.fileIntegrityPhase}`);
      console.log(`file_integrity_error_source=${result.fileIntegritySource}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
      console.log(`replicated_id=${result.id}`);
      if (result.chunkId) console.log(`chunk_id=${result.chunkId}`);
      if (Number.isFinite(Number(result.expectedSizeBytes))) console.log(`expected_size_bytes=${result.expectedSizeBytes}`);
      if (Number.isFinite(Number(result.actualSizeBytes))) console.log(`actual_size_bytes=${result.actualSizeBytes}`);
      if (Number.isFinite(Number(result.liveChunkCount))) console.log(`live_chunk_count=${result.liveChunkCount}`);
      if (Number.isFinite(Number(result.tombstonedChunkCount))) console.log(`tombstoned_chunk_count=${result.tombstonedChunkCount}`);
      if (result.requestedGenerationId) console.log(`requested_generation=${result.requestedGenerationId}`);
      if (Number.isFinite(Number(result.requestedGenerationChunkCount))) console.log(`requested_generation_chunk_count=${result.requestedGenerationChunkCount}`);
      if (Array.isArray(result.availableGenerationIds)) console.log(`available_generations=${result.availableGenerationIds.join(',')}`);
    } else if (result.mode === 'business-os-ui-regression') {
      console.log(`business_os_ui_module_count=${result.moduleCount}`);
      console.log(`business_os_ui_start_menu_items=${result.startMenuItemCount}`);
      console.log(`business_os_ui_opened_modules=${result.openedModules.map((entry) => entry.activeModule).join(',')}`);
      console.log(`business_os_ui_rendered_modules=${result.openedModules.map((entry) => entry.renderEvidence?.moduleId).filter(Boolean).join(',')}`);
      console.log(`business_os_ui_interacted_modules=${result.openedModules.map((entry) => entry.interactionEvidence?.moduleId).filter(Boolean).join(',')}`);
      console.log(`business_os_ui_interaction_names=${result.openedModules.flatMap((entry) => (entry.interactionEvidence?.actions || []).map((action) => String(action).replace(/:.+$/, ''))).join(',')}`);
      console.log(`business_os_ui_interaction_actions=${result.openedModules.flatMap((entry) => entry.interactionEvidence?.actions || []).join(',')}`);
      console.log(`business_os_ui_min_module_text_length=${Math.min(...result.openedModules.map((entry) => Number(entry.renderEvidence?.textLength || 0)))}`);
      console.log(`business_os_ui_secondary_opened_modules=${(result.secondaryOpenedModules || []).map((entry) => entry.activeModule).join(',')}`);
      console.log(`business_os_ui_secondary_rendered_modules=${(result.secondaryOpenedModules || []).map((entry) => entry.renderEvidence?.moduleId).filter(Boolean).join(',')}`);
      console.log(`business_os_ui_secondary_interacted_modules=${(result.secondaryOpenedModules || []).map((entry) => entry.interactionEvidence?.moduleId).filter(Boolean).join(',')}`);
      console.log(`business_os_ui_secondary_interaction_names=${(result.secondaryOpenedModules || []).flatMap((entry) => (entry.interactionEvidence?.actions || []).map((action) => String(action).replace(/:.+$/, ''))).join(',')}`);
      console.log(`business_os_ui_min_secondary_text_length=${Math.min(...(result.secondaryOpenedModules || []).map((entry) => Number(entry.renderEvidence?.textLength || 0)))}`);
      console.log(`business_os_ui_desktop_opened=${result.desktopOpened ? 1 : 0}`);
      console.log(`business_os_ui_active_module=${result.activeModule || ''}`);
      console.log(`business_os_visual_workspace_visible=${result.visualEvidence?.workspace?.visible ? 1 : 0}`);
      console.log(`business_os_visual_desktop_icon_count=${result.visualEvidence?.desktopIconCount || 0}`);
      console.log(`business_os_visual_screenshot_unique_colors=${result.screenshotEvidence?.uniqueSampledColors || 0}`);
      console.log(`business_os_visual_screenshot_luma_stddev=${result.screenshotEvidence?.luminanceStdDev || 0}`);
      console.log(`business_os_visual_screenshot_dominant_ratio_pct=${result.screenshotEvidence?.dominantColorRatioPct || 0}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'business-os-roles-permissions-ui') {
      console.log(`business_os_roles_permissions_target_module=${result.targetModuleId || ''}`);
      console.log(`business_os_roles_permissions_other_module=${result.otherModuleId || ''}`);
      console.log(`business_os_roles_permissions_team_modify_hidden=${result.teamModifyHidden ? 1 : 0}`);
      console.log(`business_os_roles_permissions_team_source_hidden=${result.teamSourceHidden ? 1 : 0}`);
      console.log(`business_os_roles_permissions_source_grant_visible=${result.sourceGrantVisible ? 1 : 0}`);
      console.log(`business_os_roles_permissions_modify_grant_visible=${result.modifyGrantVisible ? 1 : 0}`);
      console.log(`business_os_roles_permissions_owner_context_visible=${result.ownerContextVisible ? 1 : 0}`);
      console.log(`business_os_roles_permissions_appbar_source_gate=${result.appbarSourceGate ? 1 : 0}`);
      console.log(`business_os_roles_permissions_exact_scope_isolated=${result.exactScopeIsolated ? 1 : 0}`);
      console.log(`business_os_roles_permissions_owner_role_option=${result.ownerRoleOption ? 1 : 0}`);
      console.log(`business_os_roles_permissions_admin_owner_option_hidden=${result.adminOwnerOptionHidden ? 1 : 0}`);
      console.log(`business_os_roles_permissions_business_labels=${result.businessLabels ? 1 : 0}`);
      console.log(`business_os_roles_permissions_settings_release_fallback_readonly=${result.settingsReleaseFallbackReadOnly ? 1 : 0}`);
      console.log(`business_os_roles_permissions_settings_why_diagnostics_visible=${result.settingsWhyDiagnosticsVisible ? 1 : 0}`);
      console.log(`business_os_roles_permissions_settings_why_diagnostics_rows=${result.settingsWhyDiagnosticsRows ? 1 : 0}`);
      console.log(`business_os_roles_permissions_settings_why_diagnostics_redacted=${result.settingsWhyDiagnosticsRedacted ? 1 : 0}`);
      console.log(`business_os_roles_permissions_settings_support_diagnostics_visible=${result.settingsSupportDiagnosticsVisible ? 1 : 0}`);
      console.log(`business_os_roles_permissions_settings_support_diagnostics_rows=${result.settingsSupportDiagnosticsRows ? 1 : 0}`);
      console.log(`business_os_roles_permissions_settings_support_diagnostics_redacted=${result.settingsSupportDiagnosticsRedacted ? 1 : 0}`);
      console.log(`business_os_roles_permissions_settings_support_diagnostics_download=${result.settingsSupportDiagnosticsDownload ? 1 : 0}`);
      console.log(`business_os_roles_permissions_reload_verified=${result.reloadVerified ? 1 : 0}`);
      console.log(`business_os_roles_permissions_auth_state=${result.authState || ''}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'business-os-dynamic-apps-ui') {
      console.log(`business_os_dynamic_private_module=${result.privateModuleId || ''}`);
      console.log(`business_os_dynamic_team_module=${result.teamModuleId || ''}`);
      console.log(`business_os_dynamic_private_hidden_for_team=${result.privateHiddenForTeam ? 1 : 0}`);
      console.log(`business_os_dynamic_private_visible_for_builder=${result.privateVisibleForBuilder ? 1 : 0}`);
      console.log(`business_os_dynamic_team_visible_for_released=${result.teamVisibleForReleased ? 1 : 0}`);
      console.log(`business_os_dynamic_restricted_hidden_for_team=${result.restrictedHiddenForTeamAndVisibleForBuilder ? 1 : 0}`);
      console.log(`business_os_dynamic_lifecycle_badges_visible=${result.lifecycleBadgesVisible ? 1 : 0}`);
      console.log(`business_os_dynamic_launcher_badges_visible=${result.launcherBadgesVisible ? 1 : 0}`);
      console.log(`business_os_dynamic_lifecycle_drawer_manager_state=${result.lifecycleDrawerManagerState ? 1 : 0}`);
      console.log(`business_os_dynamic_lifecycle_drawer_readonly_state=${result.lifecycleDrawerReadonlyState ? 1 : 0}`);
      console.log(`business_os_dynamic_lifecycle_drawer_visible=${result.lifecycleDrawerVisible ? 1 : 0}`);
      console.log(`business_os_dynamic_lifecycle_why_diagnostics_visible=${result.lifecycleWhyDiagnosticsVisible ? 1 : 0}`);
      console.log(`business_os_dynamic_lifecycle_why_diagnostics_rows=${result.lifecycleWhyDiagnosticsRows ? 1 : 0}`);
      console.log(`business_os_dynamic_lifecycle_why_diagnostics_data=${result.lifecycleWhyDiagnosticsData ? 1 : 0}`);
      console.log(`business_os_dynamic_db_read_denied=${result.dbReadDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_db_raw_denied=${result.dbRawDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_real_context_collection_denied=${result.realContextCollectionDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_real_context_property_denied=${result.realContextPropertyDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_real_context_cached_denied=${result.realContextCachedDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_real_context_raw_denied=${result.realContextRawDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_open_module_reload_mounted=${result.openModuleReloadMounted ? 1 : 0}`);
      console.log(`business_os_dynamic_open_module_collection_denied=${result.openModuleCollectionDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_open_module_property_denied=${result.openModulePropertyDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_open_module_cached_denied=${result.openModuleCachedDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_open_module_raw_denied=${result.openModuleRawDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_runtime_safety_contract=${result.openModuleRuntimeSafetyContract ? 1 : 0}`);
      console.log(`business_os_dynamic_runtime_safety_capabilities=${result.openModuleRuntimeSafetyCapabilities ? 1 : 0}`);
      console.log(`business_os_dynamic_storage_keys_scoped=${result.storageKeysScoped ? 1 : 0}`);
      console.log(`business_os_dynamic_storage_scope_contract=${result.moduleStorageScopeContract ? 1 : 0}`);
      console.log(`business_os_dynamic_db_read_grant_allowed=${result.dbReadGrantAllowed ? 1 : 0}`);
      console.log(`business_os_dynamic_real_context_cached_read_grant_allowed=${result.realContextCachedReadGrantAllowed ? 1 : 0}`);
      console.log(`business_os_dynamic_db_write_denied_without_write=${result.dbWriteDeniedWithoutWrite ? 1 : 0}`);
      console.log(`business_os_dynamic_permission_facade_read_allowed=${result.facadeReadAllowed ? 1 : 0}`);
      console.log(`business_os_dynamic_permission_facade_write_denied=${result.facadeWriteDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_module=${result.packagedGuardModuleId || ''}`);
      console.log(`business_os_dynamic_packaged_guard_collection=${result.packagedGuardCollection || ''}`);
      console.log(`business_os_dynamic_packaged_guard_capability_contract=${result.packagedGuardCapabilityContract ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_read_denied=${result.packagedGuardReadDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_property_denied=${result.packagedGuardPropertyDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_raw_denied=${result.packagedGuardRawDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_context_denied=${result.packagedGuardContextDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_context_property_denied=${result.packagedGuardContextPropertyDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_read_grant_allowed=${result.packagedGuardReadAllowedAfterGrant ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_context_permission_facade=${result.packagedGuardContextPermissionFacade ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_write_denied_without_write=${result.packagedGuardWriteDeniedWithoutGrant ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_shell_locked_state=${result.packagedGuardShellLockedState ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_modules=${result.packagedGuardModules || ''}`);
      console.log(`business_os_dynamic_packaged_guard_collections=${result.packagedGuardCollections || ''}`);
      console.log(`business_os_dynamic_packaged_guard_count=${Number(result.packagedGuardBatchCount || 0)}`);
      console.log(`business_os_dynamic_packaged_guard_batch_coverage=${result.packagedGuardBatchCoverage ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_all_capability_contracts=${result.packagedGuardAllCapabilityContracts ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_all_read_denied=${result.packagedGuardAllReadDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_all_property_denied=${result.packagedGuardAllPropertyDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_all_raw_denied=${result.packagedGuardAllRawDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_all_context_denied=${result.packagedGuardAllContextDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_all_read_grants_allowed=${result.packagedGuardAllReadGrantsAllowed ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_all_context_permission_facades=${result.packagedGuardAllContextPermissionFacades ? 1 : 0}`);
      console.log(`business_os_dynamic_packaged_guard_all_writes_denied_without_write=${result.packagedGuardAllWritesDeniedWithoutWrite ? 1 : 0}`);
      console.log(`business_os_dynamic_system_scope_modules=${result.systemScopedModules || ''}`);
      console.log(`business_os_dynamic_system_scope_count=${Number(result.systemScopedCount || 0)}`);
      console.log(`business_os_dynamic_system_scope_allowed=${result.systemScopedAllowed ? 1 : 0}`);
      console.log(`business_os_dynamic_system_scope_foreign_denied=${result.systemScopedForeignDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_system_scope_raw_foreign_denied=${result.systemScopedRawForeignDenied ? 1 : 0}`);
      console.log(`business_os_dynamic_system_scope_permission_facade=${result.systemScopedPermissionFacade ? 1 : 0}`);
      console.log(`business_os_dynamic_system_scope_capability_contract=${result.systemScopedCapabilityContract ? 1 : 0}`);
      console.log(`business_os_dynamic_invalid_version_private=${result.invalidVersionPrivate ? 1 : 0}`);
      console.log(`business_os_dynamic_reload_verified=${result.reloadVerified ? 1 : 0}`);
      console.log(`business_os_dynamic_auth_state=${result.authState || ''}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'business-os-app-release-ui') {
      console.log(`business_os_app_release_target_module=${result.targetModuleId || ''}`);
      console.log(`business_os_app_release_actor_role=${result.actorRole || ''}`);
      console.log(`business_os_app_release_auth_state=${result.authState || ''}`);
      console.log(`business_os_app_release_browser_context=${result.browserContext || ''}`);
      console.log(`business_os_app_release_tenant_scope=${result.tenantScope || ''}`);
      console.log(`business_os_app_release_private_before_release=${result.privateBeforeRelease ? 1 : 0}`);
      console.log(`business_os_app_release_publish_succeeded=${result.publishSucceeded ? 1 : 0}`);
      console.log(`business_os_app_release_team_visible_after_release=${result.teamVisibleAfterRelease ? 1 : 0}`);
      console.log(`business_os_app_release_version_badge_visible=${result.versionBadgeVisible ? 1 : 0}`);
      console.log(`business_os_app_release_data_review_visible=${result.dataReviewVisible ? 1 : 0}`);
      console.log(`business_os_app_release_rollback_succeeded=${result.rollbackSucceeded ? 1 : 0}`);
      console.log(`business_os_app_release_release_audit_visible=${result.releaseAuditVisible ? 1 : 0}`);
      console.log(`business_os_app_release_rollback_audit_visible=${result.rollbackAuditVisible ? 1 : 0}`);
      console.log(`business_os_app_release_activity_audit_redacted=${result.activityAuditRedacted ? 1 : 0}`);
      console.log(`business_os_app_release_reload_verified=${result.reloadVerified ? 1 : 0}`);
      console.log(`business_os_app_release_storage_boundary_checked=${result.storageBoundaryChecked ? 1 : 0}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'business-os-app-audience-ui') {
      console.log(`business_os_app_audience_target_module=${result.targetModuleId || ''}`);
      console.log(`business_os_app_audience_actor_role=${result.actorRole || ''}`);
      console.log(`business_os_app_audience_auth_state=${result.authState || ''}`);
      console.log(`business_os_app_audience_browser_context=${result.browserContext || ''}`);
      console.log(`business_os_app_audience_tenant_scope=${result.tenantScope || ''}`);
      console.log(`business_os_app_audience_private_hidden_for_team=${result.privateHiddenForTeam ? 1 : 0}`);
      console.log(`business_os_app_audience_preview_visible_for_target=${result.previewVisibleForTarget ? 1 : 0}`);
      console.log(`business_os_app_audience_preview_hidden_for_outside=${result.previewHiddenForOutside ? 1 : 0}`);
      console.log(`business_os_app_audience_restricted_hidden_for_outside=${result.restrictedHiddenForOutside ? 1 : 0}`);
      console.log(`business_os_app_audience_deep_link_locked_outside=${result.deepLinkLockedOutside ? 1 : 0}`);
      console.log(`business_os_app_audience_reload_verified=${result.reloadVerified ? 1 : 0}`);
      console.log(`business_os_app_audience_fresh_profile_verified=${result.freshProfileVerified ? 1 : 0}`);
      console.log(`business_os_app_audience_storage_boundary_checked=${result.storageBoundaryChecked ? 1 : 0}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'business-os-agent-scope-ui') {
      console.log(`business_os_agent_scope_target_module=${result.targetModuleId || ''}`);
      console.log(`business_os_agent_scope_agent_id=${result.agentId || ''}`);
      console.log(`business_os_agent_scope_actor_role=${result.actorRole || ''}`);
      console.log(`business_os_agent_scope_auth_state=${result.authState || ''}`);
      console.log(`business_os_agent_scope_browser_context=${result.browserContext || ''}`);
      console.log(`business_os_agent_scope_tenant_scope=${result.tenantScope || ''}`);
      console.log(`business_os_agent_scope_panel_visible=${result.panelVisible ? 1 : 0}`);
      console.log(`business_os_agent_scope_client_context_matches_ui=${result.clientContextMatchesUi ? 1 : 0}`);
      console.log(`business_os_agent_scope_app_store_panel_visible=${result.appStorePanelVisible ? 1 : 0}`);
      console.log(`business_os_agent_scope_app_store_context_matches_ui=${result.appStoreClientContextMatchesUi ? 1 : 0}`);
      console.log(`business_os_agent_scope_business_chat_scope_matches_context=${result.businessChatScopeMatchesContext ? 1 : 0}`);
      console.log(`business_os_agent_scope_settings_grant_boundary_visible=${result.settingsGrantBoundaryVisible ? 1 : 0}`);
      console.log(`business_os_agent_scope_app_hidden_denied=${result.appHiddenDenied ? 1 : 0}`);
      console.log(`business_os_agent_scope_data_denied_before_grant=${result.dataDeniedBeforeGrant ? 1 : 0}`);
      console.log(`business_os_agent_scope_read_allowed_after_grant=${result.readAllowedAfterGrant ? 1 : 0}`);
      console.log(`business_os_agent_scope_write_denied_without_grant=${result.writeDeniedWithoutGrant ? 1 : 0}`);
      console.log(`business_os_agent_scope_audit_visible=${result.auditVisible ? 1 : 0}`);
      console.log(`business_os_agent_scope_denied_reason_visible=${result.deniedReasonVisible ? 1 : 0}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'business-os-threads-rightclick-ui') {
      console.log(`business_os_threads_rightclick_target_module=${result.targetModuleId || ''}`);
      console.log(`business_os_threads_rightclick_reviewer_id=${result.reviewerId || ''}`);
      console.log(`business_os_threads_rightclick_thread_id=${result.threadId || ''}`);
      console.log(`business_os_threads_rightclick_data_command_id=${result.dataCommandId || ''}`);
      console.log(`business_os_threads_rightclick_ask_command_id=${result.askCommandId || ''}`);
      console.log(`business_os_threads_rightclick_app_command_id=${result.appCommandId || ''}`);
      console.log(`business_os_threads_rightclick_data_approval_command_persisted=${result.dataApprovalCommandPersisted ? 1 : 0}`);
      console.log(`business_os_threads_rightclick_direct_denial_command_id=${result.directDenialCommandId || ''}`);
      console.log(`business_os_threads_rightclick_direct_denial_reason=${result.directDenialReason || ''}`);
      console.log(`business_os_threads_rightclick_ask_command_persisted=${result.askCommandPersisted ? 1 : 0}`);
      console.log(`business_os_threads_rightclick_app_approval_command_persisted=${result.appApprovalCommandPersisted ? 1 : 0}`);
      console.log(`business_os_threads_rightclick_source_context_captured=${result.sourceContextCaptured ? 1 : 0}`);
      console.log(`business_os_threads_rightclick_reviewer_picker_visible=${result.reviewerPickerVisible ? 1 : 0}`);
      console.log(`business_os_threads_rightclick_approval_decision=${result.approvalDecision || ''}`);
      console.log(`business_os_threads_rightclick_approved_command_id=${result.approvedCommandId || ''}`);
      console.log(`business_os_threads_rightclick_approved_command_status=${result.approvedCommandStatus || ''}`);
      console.log(`business_os_threads_rightclick_reauthorization_linked=${result.reauthorizationLinked ? 1 : 0}`);
      console.log(`business_os_threads_rightclick_actor_role=${result.actorRole || ''}`);
      console.log(`business_os_threads_rightclick_reviewer_role=${result.reviewerRole || ''}`);
      console.log(`business_os_threads_rightclick_auth_state=${result.authState || ''}`);
      console.log(`business_os_threads_rightclick_browser_context=${result.browserContext || ''}`);
      console.log(`business_os_threads_rightclick_tenant_scope=${result.tenantScope || ''}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'business-os-threads-scale-ui') {
      console.log(`business_os_threads_scale_commands=${result.scaleCommands || 0}`);
      console.log(`business_os_threads_scale_threads=${result.scaleThreads || 0}`);
      console.log(`business_os_threads_scale_messages=${result.scaleMessages || 0}`);
      console.log(`business_os_threads_scale_notifications=${result.scaleNotifications || 0}`);
      console.log(`business_os_threads_scale_visible_thread_rows=${result.scaleVisibleThreadRows || 0}`);
      console.log(`business_os_threads_scale_first_render_ms=${result.scaleFirstRenderMs || 0}`);
      console.log(`business_os_threads_scale_budget_passed=${result.scaleBudgetPassed ? 1 : 0}`);
      console.log(`business_os_threads_scale_auth_state=${result.authState || ''}`);
      console.log(`business_os_threads_scale_actor_role=${result.actorRole || ''}`);
      console.log(`business_os_threads_scale_browser_context=${result.browserContext || ''}`);
      console.log(`business_os_threads_scale_tenant_scope=${result.tenantScope || ''}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'business-os-restore-resync-ui') {
      const replicated = pollSqliteFileAndChunk(result.id);
      if (replicated.payload !== result.browserPayload) {
        throw new Error(`restore-resync sqlite payload mismatch: ${replicated.payload}`);
      }
      console.log(`business_os_restore_resync_auth_state=${result.authState || ''}`);
      console.log(`business_os_restore_resync_actor_role=${result.actorRole || ''}`);
      console.log(`business_os_restore_resync_browser_context=${result.browserContext || ''}`);
      console.log(`business_os_restore_resync_tenant_scope=${result.tenantScope || ''}`);
      console.log(`business_os_restore_resync_webrtc_only=${result.webrtcOnly ? 1 : 0}`);
      console.log(`business_os_restore_resync_peer_stopped=${result.peerStopped ? 1 : 0}`);
      console.log(`business_os_restore_resync_local_only_before_restart=${result.localOnlyBeforeRestart ? 1 : 0}`);
      console.log(`business_os_restore_resync_peer_restarted=${result.peerRestarted ? 1 : 0}`);
      console.log(`business_os_restore_resync_checkpoint_epoch_count=${(result.peerCheckpointRefresh?.checkpointEpochs || []).filter(Boolean).length}`);
      console.log(`business_os_restore_resync_native_converged_after_restart=${result.nativeConvergedAfterRestart ? 1 : 0}`);
      console.log(`business_os_restore_resync_replicated_id=${result.id || ''}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'coding-agents-ui') {
      console.log(`coding_agents_ui_provider=${result.provider}`);
      console.log(`coding_agents_ui_workspace_root=${result.workspaceRoot}`);
      console.log(`coding_agents_ui_session_id=${result.sessionId}`);
      console.log(`coding_agents_ui_status_ready=${result.statusReady ? 1 : 0}`);
      console.log(`coding_agents_ui_auth_status=${result.authStatus || ''}`);
      console.log(`coding_agents_ui_create_marker_seen=${result.createMarkerSeen ? 1 : 0}`);
      console.log(`coding_agents_ui_followup_marker_seen=${result.followupMarkerSeen ? 1 : 0}`);
      console.log(`coding_agents_ui_session_projection_status=${result.sessionProjectionStatus || ''}`);
      console.log(`coding_agents_ui_event_count=${result.eventCount || 0}`);
      console.log(`coding_agents_ui_user_event_count=${result.userEventCount || 0}`);
      console.log(`coding_agents_ui_assistant_event_count=${result.assistantEventCount || 0}`);
      console.log(`coding_agents_ui_feed_text_length=${result.feedTextLength || 0}`);
      console.log(`coding_agents_ui_active_module=${result.activeModule || ''}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'outbound-active-ui') {
      console.log(`outbound_active_ui_campaign_id=${result.campaignId}`);
      console.log(`outbound_active_ui_pipeline_id=${result.pipelineId}`);
      console.log(`outbound_active_ui_engagement_id=${result.engagementId}`);
      console.log(`outbound_active_ui_message_id=${result.messageId}`);
      console.log(`outbound_active_ui_scheduling_message_id=${result.schedulingMessageId || ''}`);
      console.log(`outbound_active_ui_meeting_request_id=${result.meetingRequestId || ''}`);
      console.log(`outbound_active_ui_approval_id=${result.approvalId}`);
      console.log(`outbound_active_ui_approval_gate_verified=${result.approvalGateVerified ? 1 : 0}`);
      console.log(`outbound_active_ui_final_send_status=${result.finalSendStatus || ''}`);
      console.log(`outbound_active_ui_provider_message_id=${result.providerMessageId || ''}`);
      console.log(`outbound_active_ui_communication_message_key=${result.communicationMessageKey || ''}`);
      console.log(`outbound_active_ui_reply_message_key=${result.replyMessageKey || ''}`);
      console.log(`outbound_active_ui_reply_classification=${result.replyClassification || ''}`);
      console.log(`outbound_active_ui_meeting_status=${result.meetingStatus || ''}`);
      console.log(`outbound_active_ui_meeting_url=${result.meetingUrl || ''}`);
      console.log(`outbound_active_ui_conversation_link=${result.conversationLink || ''}`);
      console.log(`outbound_active_ui_screenshots=${(result.screenshotPaths || []).join(',')}`);
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
    } else if (result.mode === 'command-burst-browser-to-rust') {
      console.log(`command_count=${result.commandCount}`);
      console.log(`task_count_for_commands=${result.taskCountForCommands}`);
      console.log(`command_ids=${result.ids.join(',')}`);
      console.log(`task_ids=${result.taskIds.join(',')}`);
    } else if (result.mode === 'command-browser-to-rust'
      || result.mode === 'migration-version-browser-to-rust'
      || result.mode === 'command-restart-browser-to-rust'
      || result.mode === 'command-midflight-restart-browser-to-rust'
      || result.mode === 'office-document-midflight-restart-browser-to-rust'
      || result.mode === 'office-spreadsheet-midflight-restart-browser-to-rust') {
      if (result.mode === 'migration-version-browser-to-rust') {
        const commandTable = 'ctox_business_os__business_commands__v1';
        const staleCommandTable = 'ctox_business_os__business_commands__v0';
        const taskTable = 'ctox_business_os__ctox_queue_tasks__v0';
        const commandRow = pollSqliteJson(commandTable, result.id);
        const taskRow = pollSqliteJson(taskTable, result.taskId);
        const staleRows = sqliteRowCount(staleCommandTable, `id='${sqlString(result.id)}'`);
        if (staleRows !== 0) {
          throw new Error(`business_commands stale schema table received command rows: ${staleRows}`);
        }
        if (commandRow.command_id !== result.id || taskRow.command_id !== result.id) {
          throw new Error(`migration-version command/task rows mismatch: ${JSON.stringify({ commandRow, taskRow })}`);
        }
        console.log(`schema_collection=business_commands`);
        console.log(`schema_version=1`);
        console.log(`schema_table=${commandTable}`);
        console.log(`stale_schema_table=${staleCommandTable}`);
        console.log(`stale_schema_table_rows=${staleRows}`);
        console.log(`task_table=${taskTable}`);
      }
      console.log(`command_id=${result.id}`);
      console.log(`task_id=${result.taskId}`);
      console.log(`task_count_for_command=${result.taskCountForCommand}`);
      console.log(`status=${result.status}`);
      console.log(`task_status=${result.taskStatus}`);
      if (result.officeCommit) {
        console.log(`office_kind=${result.officeCommit.kind}`);
        console.log(`office_record_id=${result.officeCommit.recordId}`);
        console.log(`office_base_version_id=${result.officeCommit.baseVersionId}`);
        console.log(`office_committed_version_id=${result.officeCommit.versionId}`);
        console.log(`office_canonical_blob_id=${result.officeCommit.blobId}`);
        console.log(`office_canonical_blob_chunk_count=${result.officeCommit.blobChunkCount}`);
      }
    } else if (result.mode === 'tickets-browser-to-rust') {
      console.log(`command_id=${result.id}`);
      console.log(`task_id=${result.taskId}`);
      console.log(`status=${result.status}`);
      console.log(`ticket_key=${result.ticketKey}`);
      console.log(`ticket_source=${result.ticketSource}`);
      console.log(`ticket_title=${result.ticketTitle}`);
    } else if (result.mode === 'tickets-clarification-browser-to-rust') {
      console.log(`create_command_id=${result.createCommandId}`);
      console.log(`clarification_command_id=${result.clarificationCommandId}`);
      console.log(`publish_command_id=${result.publishCommandId || ''}`);
      console.log(`create_status=${result.createStatus}`);
      console.log(`clarification_status=${result.clarificationStatus}`);
      console.log(`publish_status=${result.publishStatus || ''}`);
      console.log(`ticket_key=${result.ticketKey}`);
      console.log(`clarification_id=${result.clarificationId}`);
      console.log(`clarification_request_status=${result.clarificationRequestStatus}`);
      console.log(`clarification_projection_status=${result.projectionStatus || ''}`);
      console.log(`outbound_message_key=${result.outboundMessageKey || ''}`);
      console.log(`missing_input_count=${result.missingInputCount}`);
    } else {
      if (result.readinessPayload !== rustSeed.content) {
        throw new Error(`browser readiness payload mismatch: ${result.readinessPayload}`);
      }
      if (result.replicationDirections) {
        console.log(`replication_directions=${JSON.stringify(result.replicationDirections)}`);
      }
      let replicated;
      try {
        replicated = pollSqliteFileAndChunk(result.id);
      } catch (error) {
        throw new Error(`${error.message}; replicationDirections=${JSON.stringify(result.replicationDirections || null)}`);
      }
      if (replicated.payload !== result.browserPayload) {
        throw new Error(`sqlite payload mismatch: ${replicated.payload}`);
      }
      if (result.advancedStatusVersion) console.log(`advanced_status=${result.advancedStatusVersion}`);
      if (result.advancedStatusRuntime) console.log(`rxdb_runtime=${JSON.stringify(result.advancedStatusRuntime)}`);
      if (result.peerCheckpointRefresh) {
        console.log(`checkpoint_restarted_collections=${result.peerCheckpointRefresh.restartedCollections.join(',')}`);
        console.log(`checkpoint_epoch_count=${result.peerCheckpointRefresh.checkpointEpochs.filter(Boolean).length}`);
      }
      console.log(`readiness_payload=${result.readinessPayload}`);
      console.log(`replicated_id=${result.id}`);
      console.log(JSON.stringify({
        file: replicated.file.id,
        chunk: replicated.chunk.id,
        payload: replicated.payload,
      }));
    }
    console.log(`outer_phase_timings=${JSON.stringify(outerPhaseTimings)}`);
    emitBrowserDiagnostics();
  } finally {
    setSmokeStartupPhase('cleanup');
    if (browser) await withHostTimeout(browser.close(), 5000).catch(() => {});
    if (browserUserDataDir) removeSmokePath(browserUserDataDir);
    if (codingAgentSmoke?.cleanupWorkspace) removeSmokePath(codingAgentSmoke.workspaceRoot);
    await stopChild(ctox);
    await stopSignalingServer(signaling);
    recordSmokeProcessEvent('smoke_cleanup_complete');
    const unknownSignals = smokeProcessLifecycle.events.filter((event) => (
      event.type === 'child_exited'
      && event.signal
      && event.signalSource === 'unknown_external_source'
    )).length;
    console.log(`smoke_process_lifecycle_run_id=${smokeRunId}`);
    console.log(`smoke_process_lifecycle_event_count=${smokeProcessLifecycle.events.length}`);
    console.log(`smoke_process_lifecycle_unknown_signals=${unknownSignals}`);
    if (!runtimeRootProvided && !keepSmokeArtifacts) removeSmokePath(runtimeRoot);
  }
})().then(() => {
  restoreProtectedBusinessOsSources();
  process.exit(0);
}).catch((error) => {
  console.error(error.stack || error.message || error);
  if (codingAgentSmoke?.cleanupWorkspace) removeSmokePath(codingAgentSmoke.workspaceRoot);
  if (!runtimeRootProvided && !keepSmokeArtifacts) removeSmokePath(runtimeRoot);
  process.exit(1);
});
