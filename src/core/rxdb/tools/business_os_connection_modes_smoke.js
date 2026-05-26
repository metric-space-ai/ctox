#!/usr/bin/env node
/*
 * Live smoke for the supported Business OS launch modes.
 *
 * This expects the local CTOX Business OS peer/signaling stack to be running.
 * It verifies that each shell-delivery path still reaches the same RxDB/WebRTC
 * data plane and does not get stuck on the startup workspace loader.
 *
 *   node src/core/rxdb/tools/business_os_connection_modes_smoke.js
 *
 * Optional:
 *   BUSINESS_OS_DIRECT_URL=http://127.0.0.1:8765/#research \
 *   BUSINESS_OS_EXPECT_MODULE=research \
 *   BUSINESS_OS_EXPECT_TEXT="Web Research" \
 *   BUSINESS_OS_WEB_DEPLOY_URL=http://127.0.0.1:3000/business-os#ctox \
 *   CTOX_BIN=runtime/build/cargo-target/debug/ctox \
 *   node src/core/rxdb/tools/business_os_connection_modes_smoke.js
 */
const path = require('path');
const fs = require('fs');
const { spawnSync } = require('child_process');

const root = path.resolve(__dirname, '../../../..');
const playwrightModule =
  process.env.PLAYWRIGHT_MODULE_PATH ||
  (() => {
    const candidates = [
      'playwright',
      '/tmp/ctox-pw-smoke/node_modules/playwright',
    ];
    for (const candidate of candidates) {
      try {
        return require.resolve(candidate);
      } catch {
        // Try next known runtime.
      }
    }
    throw new Error('No Playwright runtime found. Install playwright or set PLAYWRIGHT_MODULE_PATH.');
  })();
const { chromium } = require(playwrightModule);

const ctoxBin = process.env.CTOX_BIN || path.join(root, 'runtime/build/cargo-target/debug/ctox');
const directUrl = process.env.BUSINESS_OS_DIRECT_URL || 'http://127.0.0.1:8765/#ctox';
const webDeployUrl = process.env.BUSINESS_OS_WEB_DEPLOY_URL || 'http://127.0.0.1:3000/business-os#ctox';
const expectedSignalingUrl = process.env.CTOX_BUSINESS_OS_SIGNALING_URLS || 'ws://127.0.0.1:18990';
const expectedModule = process.env.BUSINESS_OS_EXPECT_MODULE || 'ctox';
const expectedText = process.env.BUSINESS_OS_EXPECT_TEXT || (expectedModule === 'ctox' ? 'Rxdb Webrtc' : '');
const resultPath = process.env.BUSINESS_OS_CONNECTION_SMOKE_RESULT_PATH || '';
const shellVisibleBudgetMs = Number(process.env.BUSINESS_OS_SHELL_VISIBLE_BUDGET_MS || '3000');
const readinessPollMs = Number(process.env.BUSINESS_OS_CONNECTION_SMOKE_POLL_MS || '250');
const requiredAdvancedStatusVersion = 'business-os-advanced-status-v1';
const defaultConnectionModes = ['direct', 'web-deploy-local', 'web-deploy-packed'];
const requestedModes = (process.env.BUSINESS_OS_CONNECTION_SMOKE_MODES || defaultConnectionModes.join(','))
  .split(',')
  .map((mode) => mode.trim())
  .filter(Boolean);
const knownConnectionModes = new Set(defaultConnectionModes);
const summary = {
  ok: false,
  requestedModes,
  expectedSignalingUrl,
  shellVisibleBudgetMs,
  readinessPollMs,
  startedAt: new Date().toISOString(),
  endedAt: null,
  results: [],
};

const unknownModes = requestedModes.filter((mode) => !knownConnectionModes.has(mode));
if (!requestedModes.length) {
  failConfiguration('BUSINESS_OS_CONNECTION_SMOKE_MODES did not contain any modes');
}
if (unknownModes.length) {
  failConfiguration(`BUSINESS_OS_CONNECTION_SMOKE_MODES contains unsupported mode(s): ${unknownModes.join(', ')}`);
}

function existingChromeExecutable() {
  if (process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE) return process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE;
  const candidates = [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
  ];
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function chromiumLaunchOptions() {
  const executablePath = existingChromeExecutable();
  return executablePath
    ? { headless: true, executablePath }
    : { headless: true };
}

function base64UrlJson(value) {
  return Buffer.from(JSON.stringify(value), 'utf8').toString('base64url');
}

function writeSummary(ok) {
  summary.ok = ok;
  summary.endedAt = new Date().toISOString();
  if (!resultPath) return;
  fs.mkdirSync(path.dirname(resultPath), { recursive: true });
  fs.writeFileSync(resultPath, `${JSON.stringify(summary, null, 2)}\n`);
}

function failConfiguration(message) {
  summary.configurationError = message;
  console.error(`business-os connection smoke configuration error: ${message}`);
  writeSummary(false);
  process.exit(1);
}

function parseJson(output) {
  const text = String(output || '').trim();
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    for (let start = text.lastIndexOf('{'); start >= 0; start = text.lastIndexOf('{', start - 1)) {
      try {
        return JSON.parse(text.slice(start));
      } catch {
        // Keep scanning older JSON starts.
      }
    }
  }
  return null;
}

function withQuery(url, key, value) {
  const next = new URL(url);
  next.searchParams.set(key, value);
  return next.toString();
}

function packedWebDeployUrl() {
  const result = spawnSync(ctoxBin, ['business-os', 'peer', 'status'], {
    cwd: root,
    env: {
      ...process.env,
      CTOX_BUSINESS_OS_SIGNALING_URLS: expectedSignalingUrl,
    },
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(`ctox business-os peer status failed: ${result.stderr || result.stdout}`);
  }
  const config = parseJson(result.stdout || `${result.stdout}\n${result.stderr}`);
  if (!config || typeof config !== 'object') {
    throw new Error(`ctox business-os peer status did not return JSON: ${result.stdout || result.stderr}`);
  }
  const pairingConfig = {
    ...config,
    app_hosting: 'ctox_dev_web_deploy',
    transport: 'webrtc',
    http_bridge_available: false,
    ctox_instance_required: true,
  };
  return withQuery(webDeployUrl, 'ctox_config', base64UrlJson(pairingConfig));
}

async function checkMode(browser, mode, url) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const consoleIssues = [];
  page.on('console', (msg) => {
    if (['error', 'warning'].includes(msg.type())) {
      const text = msg.text();
      if (!/favicon|ERR_ABORTED/i.test(text)) {
        consoleIssues.push(`[${msg.type()}] ${text}`);
      }
    }
  });
  page.on('pageerror', (error) => consoleIssues.push(`[pageerror] ${error.stack || error.message}`));
  try {
    const smokeUrl = withQuery(url, 'rxdbSmoke', '1');
    const serverStatus = await fetchServerStatus(smokeUrl);
    await page.goto(smokeUrl, { waitUntil: 'domcontentloaded', timeout: 20000 });
    const state = await waitForReady(page, mode);
    if (!state.config || state.config.transport !== 'webrtc') {
      throw new Error(`${mode}: missing WebRTC config`);
    }
    if (state.config.http_bridge_available !== false) {
      throw new Error(`${mode}: HTTP bridge unexpectedly enabled`);
    }
    if (!Array.isArray(state.config.signaling_urls) || state.config.signaling_urls.length === 0) {
      throw new Error(`${mode}: missing signaling URLs`);
    }
    if (!state.config.signaling_urls.includes(expectedSignalingUrl)) {
      throw new Error(`${mode}: expected signaling ${expectedSignalingUrl}, got ${JSON.stringify(state.config.signaling_urls)}`);
    }
    if (serverStatus?.data_plane && serverStatus.data_plane.ok === false) {
      throw new Error(`${mode}: native RxDB data plane is unhealthy: ${JSON.stringify(serverStatus.data_plane)}`);
    }
    return {
      mode,
      url: smokeUrl,
      appHosting: state.config.app_hosting || '',
      activeModule: state.activeModule || '',
      moduleCount: state.moduleCount,
      timings: state.timings || null,
      advancedStatusVersion: state.advancedStatus?.version || '',
      advancedStatusBootTimings: state.advancedStatus?.shell?.bootTimings || null,
      advancedStatusChecks: state.advancedStatus?.checks || null,
      signalingUrls: state.config.signaling_urls,
      signalingSource: state.config.signaling_urls_source || serverStatus?.sync?.signaling_urls_source || '',
      moduleCatalog: serverStatus?.module_catalog || null,
      dataPlane: serverStatus?.data_plane || null,
      consoleIssues,
    };
  } finally {
    await context.close().catch(() => {});
  }
}

async function waitForReady(page, mode) {
  const deadline = Date.now() + 70000;
  const startedAt = Date.now();
  let firstShellVisibleMs = null;
  let firstWebRtcConnectedMs = null;
  let lastState = null;
  while (Date.now() < deadline) {
    lastState = await page.evaluate(async ({ expectedText }) => {
      const text = document.body?.innerText || '';
      const app = globalThis.CTOX_BUSINESS_OS_APP || globalThis.ctoxBusinessOsSmoke?.state || null;
      const inlineConfig = (() => {
        const marker = 'window.CTOX_BUSINESS_OS_CONFIG=';
        const script = [...document.scripts]
          .map((entry) => entry.textContent || '')
          .find((entry) => entry.includes(marker));
        if (!script) return null;
        const start = script.indexOf(marker) + marker.length;
        const end = script.indexOf(';', start);
        if (end <= start) return null;
        try {
          return JSON.parse(script.slice(start, end));
        } catch {
          return null;
        }
      })();
      const modules = Array.isArray(app?.modules)
        ? app.modules
        : Array.isArray(app?.moduleCatalog)
          ? app.moduleCatalog
          : [];
      const storedConfig = (() => {
        try {
          return JSON.parse(localStorage.getItem('ctox.businessOs.pairingConfig') || 'null');
        } catch {
          return null;
        }
      })();
      const scripts = [...document.scripts].map((script) => script.src).filter(Boolean);
      const advancedStatus = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({
        includeCounts: false,
        requiredCollections: ['business_module_catalog', 'ctox_runtime_settings', 'desktop_files', 'desktop_file_chunks'],
      }).catch((error) => ({ ok: false, error: String(error?.message || error) }));
      return {
        title: document.title,
        config: globalThis.CTOX_BUSINESS_OS_CONFIG || app?.sync?.config || inlineConfig || storedConfig,
        activeModule: document.body?.dataset?.activeModule || app?.activeModule?.id || app?.state?.activeModuleId || '',
        loading: text.includes('Loading workspace') || document.body?.dataset?.moduleLoading === 'startup',
        ctoxVisible: text.includes('CTOX LIVE FLOW') || text.includes('Was CTOX gerade tut') || text.includes('Rxdb Webrtc'),
        connected: text.includes('Rxdb Webrtc') && text.includes('VERBUNDEN'),
        expectedTextFound: !expectedText || text.includes(expectedText),
        moduleCount: modules.length,
        advancedStatus,
        dbBuild: scripts.find((src) => src.includes('app.js?v=')) || '',
        textSample: text.slice(0, 500),
      };
    }, { expectedText });
    const moduleOk = expectedModule ? lastState.activeModule === expectedModule : Boolean(lastState.activeModule);
    const connectionOk = expectedModule === 'ctox' ? lastState.connected : true;
    let statusEvidenceOk = false;

    if (firstShellVisibleMs === null && !lastState.loading && moduleOk && lastState.moduleCount > 0) {
      firstShellVisibleMs = Date.now() - startedAt;
    }
    if (firstWebRtcConnectedMs === null && lastState.connected) {
      firstWebRtcConnectedMs = Date.now() - startedAt;
    }
    lastState.timings = {
      firstShellVisibleMs,
      firstWebRtcConnectedMs,
      elapsedMs: Date.now() - startedAt,
      shellVisibleBudgetMs,
    };
    if (firstShellVisibleMs === null && Date.now() - startedAt > shellVisibleBudgetMs) {
      throw new Error(`${mode}: Business OS shell did not become visible within ${shellVisibleBudgetMs}ms: ${JSON.stringify(lastState, null, 2)}`);
    }
    if (firstShellVisibleMs !== null && firstShellVisibleMs > shellVisibleBudgetMs) {
      throw new Error(`${mode}: Business OS shell became visible after ${firstShellVisibleMs}ms, exceeding ${shellVisibleBudgetMs}ms budget: ${JSON.stringify(lastState, null, 2)}`);
    }
    if (firstShellVisibleMs !== null) {
      const statusBootTimings = lastState.advancedStatus?.shell?.bootTimings || null;
      const statusShellVisibleMs = Number(statusBootTimings?.shellVisibleMs);
      if (!lastState.advancedStatus || lastState.advancedStatus.version !== requiredAdvancedStatusVersion) {
        throw new Error(`${mode}: missing ${requiredAdvancedStatusVersion} evidence: ${JSON.stringify(lastState, null, 2)}`);
      }
      if (!Number.isFinite(statusShellVisibleMs)) {
        throw new Error(`${mode}: advanced status missing shellVisibleMs boot timing: ${JSON.stringify(lastState, null, 2)}`);
      }
      if (statusShellVisibleMs > shellVisibleBudgetMs) {
        throw new Error(`${mode}: advanced status shellVisibleMs exceeded budget: shellVisibleMs=${statusShellVisibleMs}, budget=${shellVisibleBudgetMs}, state=${JSON.stringify(lastState, null, 2)}`);
      }
      statusEvidenceOk = true;
    }
    if (!lastState.loading && lastState.moduleCount > 0 && moduleOk && connectionOk && lastState.expectedTextFound && statusEvidenceOk) {
      return lastState;
    }
    await new Promise((resolve) => setTimeout(resolve, readinessPollMs));
  }
  throw new Error(`${mode}: Business OS did not become ready: ${JSON.stringify(lastState, null, 2)}`);
}

async function fetchServerStatus(smokeUrl) {
  const statusUrl = new URL('/api/business-os/status', smokeUrl);
  try {
    const response = await fetch(statusUrl);
    if (!response.ok) return null;
    return await response.json();
  } catch {
    return null;
  }
}

(async () => {
  const cases = [];
  if (requestedModes.includes('direct')) {
    cases.push({ mode: 'direct', url: directUrl });
  }
  if (requestedModes.includes('web-deploy-local')) {
    cases.push({ mode: 'web-deploy-local', url: webDeployUrl });
  }
  if (requestedModes.includes('web-deploy-packed')) {
    cases.push({ mode: 'web-deploy-packed', url: packedWebDeployUrl() });
  }
  if (!cases.length) throw new Error('No Business OS connection modes selected.');

  const browser = await chromium.launch(chromiumLaunchOptions());
  try {
    const results = [];
    for (const entry of cases) {
      results.push(await checkMode(browser, entry.mode, entry.url));
    }
    summary.results = results;
    writeSummary(true);
    console.log(JSON.stringify(summary, null, 2));
  } finally {
    await browser.close().catch(() => {});
  }
})().catch((error) => {
  summary.error = error.stack || error.message || String(error);
  writeSummary(false);
  console.error(error.stack || error.message || error);
  process.exit(1);
});
