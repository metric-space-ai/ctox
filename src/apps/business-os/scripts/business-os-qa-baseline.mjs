#!/usr/bin/env node
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadBusinessOsAppInventory } from './business-os-app-inventory.mjs';

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');

const appInventory = loadBusinessOsAppInventory();
const ALL_APP_BASELINE = appInventory.allApps;
const AUDIT_APP_BASELINE = Object.freeze([
  ...appInventory.coreApps,
  ...appInventory.compatibilityApps,
]);

const config = {
  url: process.env.BUSINESS_OS_QA_URL || 'http://127.0.0.1:18765/',
  outputDir: process.env.BUSINESS_OS_QA_OUTPUT_DIR || path.join(
    repoRoot,
    'output/playwright',
    `business-os-qa-baseline-${timestampForPath()}`
  ),
  headless: process.env.BUSINESS_OS_QA_HEADLESS !== '0',
  appWaitMs: parsePositiveInt(process.env.BUSINESS_OS_QA_APP_WAIT_MS || '1800', 'BUSINESS_OS_QA_APP_WAIT_MS'),
  readyTimeoutMs: parsePositiveInt(process.env.BUSINESS_OS_QA_READY_TIMEOUT_MS || '70000', 'BUSINESS_OS_QA_READY_TIMEOUT_MS'),
  failOnConsole: process.env.BUSINESS_OS_QA_FAIL_ON_CONSOLE !== '0',
  failOnWarnings: process.env.BUSINESS_OS_QA_FAIL_ON_WARNINGS !== '0',
  failOnRegistry: process.env.BUSINESS_OS_QA_FAIL_ON_REGISTRY !== '0',
  skipRegistry: process.env.BUSINESS_OS_QA_SKIP_REGISTRY === '1',
  loginUser: process.env.BUSINESS_OS_QA_LOGIN_USER || '',
  loginPassword: process.env.BUSINESS_OS_QA_LOGIN_PASSWORD || '',
  authBearer: process.env.BUSINESS_OS_QA_AUTH_BEARER || '',
  localAssets: process.env.BUSINESS_OS_QA_LOCAL_ASSETS === '1',
  localAssetPrefixes: String(process.env.BUSINESS_OS_QA_LOCAL_ASSET_PREFIXES || '')
    .split(',')
    .map((value) => value.trim().replace(/^\/+/, ''))
    .filter(Boolean),
  installedStrict: process.env.BUSINESS_OS_QA_INSTALLED_STRICT === '1',
  enforcePerformance: process.env.BUSINESS_OS_QA_ENFORCE_PERFORMANCE !== '0',
  warmMountBudgetMs: parsePositiveInt(process.env.BUSINESS_OS_QA_WARM_MOUNT_BUDGET_MS || '500', 'BUSINESS_OS_QA_WARM_MOUNT_BUDGET_MS'),
  interactionBudgetMs: parsePositiveInt(process.env.BUSINESS_OS_QA_INTERACTION_BUDGET_MS || '100', 'BUSINESS_OS_QA_INTERACTION_BUDGET_MS'),
  theme: ['light', 'dark'].includes(process.env.BUSINESS_OS_QA_THEME) ? process.env.BUSINESS_OS_QA_THEME : '',
  locale: ['de', 'en'].includes(process.env.BUSINESS_OS_QA_LOCALE) ? process.env.BUSINESS_OS_QA_LOCALE : '',
  appIds: String(process.env.BUSINESS_OS_QA_APP_IDS || '')
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean),
};
const appsToCheck = config.appIds.length
  ? ALL_APP_BASELINE.filter((app) => config.appIds.includes(app.id))
  : AUDIT_APP_BASELINE;
const unknownAppIds = config.appIds.filter((id) => !ALL_APP_BASELINE.some((app) => app.id === id));
if (unknownAppIds.length) {
  throw new Error(`BUSINESS_OS_QA_APP_IDS contains unknown app id(s): ${unknownAppIds.join(', ')}`);
}

const summary = {
  ok: false,
  startedAt: new Date().toISOString(),
  endedAt: null,
  config: {
    url: config.url,
    outputDir: config.outputDir,
    headless: config.headless,
    appWaitMs: config.appWaitMs,
    readyTimeoutMs: config.readyTimeoutMs,
    failOnConsole: config.failOnConsole,
    failOnWarnings: config.failOnWarnings,
    failOnRegistry: config.failOnRegistry,
    skipRegistry: config.skipRegistry,
    loginUser: config.loginUser || '',
    loginPasswordConfigured: !!config.loginPassword,
    authBearerConfigured: !!config.authBearer,
    localAssets: config.localAssets,
    localAssetPrefixes: config.localAssetPrefixes,
    installedStrict: config.installedStrict,
    enforcePerformance: config.enforcePerformance,
    warmMountBudgetMs: config.warmMountBudgetMs,
    interactionBudgetMs: config.interactionBudgetMs,
    theme: config.theme,
    locale: config.locale,
  },
  expectedApps: appsToCheck,
  expectedCoreApps: appsToCheck.filter((app) => app.cohort === 'core').length,
  expectedCompatibilityApps: appsToCheck.filter((app) => app.cohort === 'compatibility').length,
  staticRegistry: readStaticRegistry(),
  shell: null,
  surfaces: null,
  apps: [],
  console: [],
  failures: [],
};

const consoleEvents = [];
const localAssetEvents = [];
const playwrightModule = resolvePlaywrightModule();
const { chromium } = require(playwrightModule);

fs.mkdirSync(config.outputDir, { recursive: true });

try {
  const browser = await chromium.launch(chromiumLaunchOptions());
  try {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 980 },
      deviceScaleFactor: 1,
    });
    const page = await context.newPage();
    let smokeUrlLoaded = false;
    if (config.authBearer) await attachSameOriginBearer(page);
    attachConsoleCapture(page);

    if (config.localAssets) {
      const bootstrapUrl = withQuery(config.url, 'rxdbSmoke', '1');
      if (config.localAssetPrefixes.length) await attachLocalAssetRouting(page);
      await page.goto(bootstrapUrl, { waitUntil: 'domcontentloaded', timeout: config.readyTimeoutMs });
      smokeUrlLoaded = true;
      await authenticateIfNeeded(page, bootstrapUrl);
      await page.waitForFunction(
        () => document.body?.dataset?.authState !== 'locked',
        null,
        { timeout: config.readyTimeoutMs },
      );
      if (!config.localAssetPrefixes.length) await attachLocalAssetRouting(page);
    }

    const smokeUrl = withQuery(config.url, 'rxdbSmoke', '1');
    if (!smokeUrlLoaded) {
      await page.goto(smokeUrl, { waitUntil: 'domcontentloaded', timeout: config.readyTimeoutMs });
    }
    await authenticateIfNeeded(page, smokeUrl);
    summary.shell = await waitForShellReady(page);
    await applyQAPreferences(page);

    if (!config.skipRegistry) {
      summary.surfaces = await collectRegistrySurfaces(page);
      await safeScreenshot(page, '00-registry-surfaces.png');
      pushRegistryFailures(summary.failures, summary.surfaces, summary.staticRegistry);
      await closeAppWindow(page, { id: 'app-store', kind: 'module' });
    }

    for (let index = 0; index < appsToCheck.length; index += 1) {
      const app = appsToCheck[index];
      let appResult;
      try {
        appResult = await withHostTimeout(
          captureApp(page, app, index + 1),
          Math.max(config.readyTimeoutMs, 45000),
          `App capture timed out outside the renderer: ${app.id}`,
        );
      } catch (error) {
        summary.failures.push({
          scope: 'app-open',
          app: app.id,
          message: error?.message || String(error),
          stack: error?.stack || '',
        });
        await withHostTimeout(
          closeAppWindow(page, app),
          5000,
          `Failed app cleanup timed out: ${app.id}`,
        ).catch(() => {});
        // An in-page timer cannot fire when Chromium's renderer event loop is
        // starved. Abort this browser session on a host-side timeout instead of
        // sending more work to a renderer that can no longer answer.
        if (error?.code === 'BUSINESS_OS_QA_HOST_TIMEOUT') throw error;
        continue;
      }
      summary.apps.push(appResult);
      if (appResult.dom.horizontalOverflow || appResult.dom.unnamedVisibleButtons > 0) {
        summary.failures.push({
          scope: 'app-contract',
          app: app.id,
          message: `${app.title} violates the responsive/accessibility contract`,
          horizontalOverflow: appResult.dom.horizontalOverflow,
          unnamedVisibleButtons: appResult.dom.unnamedVisibleButtons,
        });
      }
      if (appResult.presentation?.applicable && !appResult.presentation.ok) {
        summary.failures.push({
          scope: 'presentation-contract',
          app: app.id,
          message: `${app.title} failed mode continuity or responsive overflow checks`,
          presentation: appResult.presentation,
        });
      }
      if (config.failOnConsole && appResult.consoleErrors.length > 0) {
        summary.failures.push({
          scope: 'console',
          app: app.id,
          message: `${app.title} emitted ${appResult.consoleErrors.length} disallowed console/page event(s)`,
          errors: appResult.consoleErrors,
        });
      }
    }

    summary.performance = summarizePerformance(summary.apps);
    if (config.enforcePerformance && summary.performance.warmMountP95Ms > config.warmMountBudgetMs) {
      summary.failures.push({
        scope: 'performance',
        message: `Warm mount p95 ${summary.performance.warmMountP95Ms}ms exceeds ${config.warmMountBudgetMs}ms`,
      });
    }
    if (config.enforcePerformance && summary.performance.interactionP95Ms > config.interactionBudgetMs) {
      summary.failures.push({
        scope: 'performance',
        message: `Visible interaction p95 ${summary.performance.interactionP95Ms}ms exceeds ${config.interactionBudgetMs}ms`,
      });
    }
    if (config.failOnConsole) {
      const attributed = new Set(summary.failures
        .flatMap((failure) => failure.errors || [])
        .map(consoleEventKey));
      const unattributed = consoleEvents.filter((entry) => (
        (entry.level === 'error' || entry.level === 'pageerror' || (config.failOnWarnings && entry.level === 'warning'))
        && !attributed.has(consoleEventKey(entry))
      ));
      if (unattributed.length) {
        summary.failures.push({
          scope: 'console-global',
          message: `Business OS emitted ${unattributed.length} unattributed disallowed console/page event(s)`,
          errors: unattributed,
        });
      }
    }

    summary.console = consoleEvents;
    summary.ok = summary.failures.length === 0;
    await withHostTimeout(context.close(), 5000, 'Browser context close timed out').catch(() => {});
  } finally {
    await withHostTimeout(browser.close(), 5000, 'Browser close timed out').catch(() => {});
  }
} catch (error) {
  summary.failures.push({
    scope: 'harness',
    message: error?.message || String(error),
    stack: error?.stack || '',
  });
} finally {
  summary.endedAt = new Date().toISOString();
  summary.console = consoleEvents;
  summary.localAssetEvents = localAssetEvents;
  summary.ok = summary.failures.length === 0;
  writeJson('business-os-qa-baseline.json', summary);
  writeMarkdownReport(summary);
}

if (!summary.ok) {
  console.error(`Business OS QA baseline failed with ${summary.failures.length} issue(s).`);
  console.error(`Report: ${path.join(config.outputDir, 'business-os-qa-baseline.md')}`);
  process.exit(1);
}

console.log(`Business OS QA baseline OK: ${appsToCheck.length} apps checked.`);
console.log(`Report: ${path.join(config.outputDir, 'business-os-qa-baseline.md')}`);

async function authenticateIfNeeded(page, smokeUrl) {
  const authState = await page.evaluate(() => document.body?.dataset?.authState || '');
  if (authState !== 'locked') return;
  if (config.authBearer) {
    await page.goto(smokeUrl, { waitUntil: 'domcontentloaded', timeout: config.readyTimeoutMs });
    const bearerAuthState = await page.evaluate(() => document.body?.dataset?.authState || '');
    if (bearerAuthState !== 'locked') return;
    throw new Error('Business OS capability bearer was rejected by the server');
  }
  if (!config.loginUser || !config.loginPassword) {
    throw new Error('Business OS login gate is locked and BUSINESS_OS_QA_LOGIN_USER/PASSWORD are not configured');
  }

  const loginUrl = new URL('/login', smokeUrl).toString();
  const response = await page.context().request.post(loginUrl, {
    form: {
      user: config.loginUser,
      password: config.loginPassword,
    },
    maxRedirects: 0,
  });
  if (![200, 302, 303].includes(response.status())) {
    throw new Error(`Business OS login request failed with HTTP ${response.status()}`);
  }
  const location = response.headers().location || '';
  if (/loginFailed=1/.test(location)) {
    throw new Error('Business OS login request was rejected by the server');
  }
  const setCookie = response.headers()['set-cookie'] || '';
  const sessionCookie = setCookie.match(/(?:^|,\s*)ctox_business_os_session=([^;]+)/);
  if (sessionCookie?.[1]) {
    await page.context().addCookies([{
      name: 'ctox_business_os_session',
      value: sessionCookie[1],
      domain: new URL(smokeUrl).hostname,
      path: '/',
      httpOnly: true,
      secure: new URL(smokeUrl).protocol === 'https:',
      sameSite: 'Lax',
      expires: Math.floor(Date.now() / 1000) + 30 * 24 * 60 * 60,
    }]);
  }
  await page.goto(smokeUrl, { waitUntil: 'domcontentloaded', timeout: config.readyTimeoutMs });
}

async function attachSameOriginBearer(page) {
  const allowedOrigin = new URL(config.url).origin;
  await page.route('**/*', async (route) => {
    const request = route.request();
    if (new URL(request.url()).origin !== allowedOrigin) {
      await route.fallback();
      return;
    }
    await route.fallback({
      headers: {
        ...request.headers(),
        authorization: `Bearer ${config.authBearer}`,
      },
    });
  });
}

async function applyQAPreferences(page) {
  if (!config.theme && !config.locale) return;
  await page.evaluate(({ theme, locale }) => {
    if (theme) document.documentElement.dataset.theme = theme;
    if (locale) document.documentElement.lang = locale;
    const detail = {
      theme: document.documentElement.dataset.theme === 'light' ? 'light' : 'dark',
      language: document.documentElement.lang === 'en' ? 'en' : 'de',
    };
    window.dispatchEvent(new CustomEvent('ctox-business-os-preferences', { detail }));
    window.postMessage({ type: 'ctox-business-os-language', lang: detail.language }, '*');
  }, { theme: config.theme, locale: config.locale });
}

async function captureApp(page, app, ordinal) {
  const consoleStart = consoleEvents.length;
  const openStartedAt = Date.now();
  const opened = await openApp(page, app);
  const openDurationMs = Date.now() - openStartedAt;
  await page.waitForTimeout(config.appWaitMs);
  const healthAfterOpen = await captureCompactRuntimeHealth(page);
  const presentation = await exercisePresentationContract(page, app);
  const dom = await collectDomCounts(page, app);
  const screenshot = `${String(ordinal).padStart(2, '0')}-${slug(app.id)}.png`;
  await safeScreenshot(page, screenshot);
  const consoleErrors = consoleEvents
    .slice(consoleStart)
    .filter((entry) => entry.level === 'error' || entry.level === 'pageerror' || (config.failOnWarnings && entry.level === 'warning'));
  const result = {
    id: app.id,
    title: app.title,
    kind: app.kind,
    cohort: app.cohort,
    opened,
    openDurationMs,
    screenshot,
    dom,
    consoleErrors,
    healthAfterOpen,
    presentation,
  };
  await closeAppWindow(page, app);
  await page.waitForTimeout(250);
  result.healthAfterClose = await captureCompactRuntimeHealth(page);
  if (app.kind !== 'shell-surface') {
    const warmOpen = await openApp(page, app);
    result.warmMountMs = warmOpen.visibleMountMs;
    result.warmReadyMs = warmOpen.readyMountMs;
    result.warmLoadingShadowObserved = warmOpen.loadingShadowObserved;
    await closeAppWindow(page, app);
  }
  return result;
}

async function exercisePresentationContract(page, app) {
  if (app.kind === 'shell-surface') {
    return { applicable: false, reason: 'shell-surface' };
  }
  return page.evaluate(async (appId) => {
    const state = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP || null;
    const manager = state?.windowManager;
    const win = manager?.listWindows?.().find((entry) => entry.ownerId === `desktop-app:${appId}`);
    const element = win ? document.getElementById(win.id) : null;
    const root = element?.querySelector('[data-window-content]');
    if (!win || !element || !root || typeof manager?.setAppMode !== 'function') {
      return { applicable: true, ok: false, error: 'window presentation surface unavailable' };
    }
    const marker = `qa-${appId}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    root.dataset.qaMountIdentity = marker;
    const modes = [];
    for (const mode of ['maximized', 'focus', 'window']) {
      const started = performance.now();
      manager.setAppMode(win.id, mode);
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      modes.push({
        mode,
        durationMs: Number((performance.now() - started).toFixed(2)),
        renderedMode: element.dataset.appMode || '',
        sameMount: root.isConnected && root.dataset.qaMountIdentity === marker,
      });
    }
    const original = {
      width: element.style.width,
      height: element.style.height,
      left: element.style.left,
      top: element.style.top,
    };
    const responsive = [];
    for (const width of [640, 960, 1180]) {
      element.style.width = `${width}px`;
      element.style.height = '760px';
      element.style.left = '0px';
      element.style.top = '0px';
      await new Promise((resolve) => requestAnimationFrame(resolve));
      responsive.push({
        width,
        contentWidth: root.clientWidth,
        scrollWidth: root.scrollWidth,
        horizontalOverflow: root.scrollWidth > root.clientWidth + 2,
        layoutSamples: ['.app-explorer-toolbar', '.app-explorer-body', '.source-editor', '.source-editor-toolbar']
          .flatMap((selector) => {
            const node = root.querySelector(selector);
            if (!node) return [];
            const style = getComputedStyle(node);
            return [{ selector, display: style.display, gridTemplateColumns: style.gridTemplateColumns, flexWrap: style.flexWrap }];
          }),
        overflowElements: [...root.querySelectorAll('*')].flatMap((node) => {
          const rootRect = root.getBoundingClientRect();
          const rect = node.getBoundingClientRect();
          const overflow = Math.max(0, rect.right - rootRect.right, node.scrollWidth - node.clientWidth);
          return overflow > 2 ? [{
            node: `${node.tagName.toLowerCase()}${node.id ? `#${node.id}` : ''}${node.classList.length ? `.${[...node.classList].slice(0, 3).join('.')}` : ''}`,
            overflow: Number(overflow.toFixed(1)),
            clientWidth: node.clientWidth,
            scrollWidth: node.scrollWidth,
          }] : [];
        }).sort((a, b) => b.overflow - a.overflow).slice(0, 8),
      });
    }
    Object.assign(element.style, original);
    delete root.dataset.qaMountIdentity;
    return {
      applicable: true,
      ok: modes.every((entry) => entry.renderedMode === entry.mode && entry.sameMount)
        && responsive.every((entry) => !entry.horizontalOverflow),
      modes,
      responsive,
      interactionMaxMs: Math.max(...modes.map((entry) => entry.durationMs)),
      styleCapabilities: {
        explorerContainerRules: document.getElementById('app-explorer-styles')?.textContent.includes('@container business-app-window') || false,
        editorContainerRules: document.getElementById('source-editor-styles')?.textContent.includes('@container business-app-window') || false,
      },
    };
  }, app.id);
}

async function closeAppWindow(page, app) {
  await page.evaluate((targetId) => {
    const state = globalThis.ctoxBusinessOsSmoke?.state;
    const ownerId = `desktop-app:${targetId.id}`;
    const windows = state?.windowManager?.listWindows?.()
      .filter((item) => item.ownerId === ownerId) || [];
    for (const win of windows) state.windowManager.destroy?.(win.id);
  }, app);
  await page.waitForFunction((targetId) => {
    const state = globalThis.ctoxBusinessOsSmoke?.state;
    const ownerId = `desktop-app:${targetId.id}`;
    return !(state?.windowManager?.listWindows?.() || [])
      .some((item) => item.ownerId === ownerId);
  }, app, { timeout: 3000 }).catch(() => {});
}

async function captureCompactRuntimeHealth(page) {
  return page.evaluate(() => {
    // Use the synchronous runtime diagnostic object here. Calling the full
    // advanced-status snapshot after every app also performs IndexedDB
    // evidence reads and can itself block the lifecycle stress test.
    const state = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP || null;
    const diagnostics = state?.sync?.diagnostics || {};
    const entries = Object.values(diagnostics.collections || {});
    const failedCollections = entries
      .filter((entry) => ['failed', 'error'].includes(String(entry?.connectionStatus || entry?.status || '')))
      .map((entry) => entry.collection)
      .filter(Boolean);
    const reconnectingCollections = entries
      .filter((entry) => String(entry?.connectionStatus || entry?.status || '') === 'reconnecting')
      .map((entry) => entry.collection)
      .filter(Boolean);
    const replicationErrors = entries
      .filter((entry) => entry?.lastError)
      .slice(0, 20)
      .map((entry) => ({ collection: entry.collection, error: entry.lastError }));
    const frameEntries = entries.map((entry) => entry?.frameTransport).filter(Boolean);
    const demand = frameEntries
      .map((entry) => entry?.demandTransport)
      .filter(Boolean)
      .reduce((acc, entry) => ({
        pendingQueryCollectors: Math.max(acc.pendingQueryCollectors, Number(entry.pendingQueryCollectors || 0)),
        queuedQueryRequests: Math.max(acc.queuedQueryRequests, Number(entry.queuedQueryRequests || 0)),
        activeQueryStreams: Math.max(acc.activeQueryStreams, Number(entry.activeQueryStreams || 0)),
        queryCollectorTimeouts: Math.max(acc.queryCollectorTimeouts, Number(entry.queryCollectorTimeouts || 0)),
        queryCollectorsRejected: Math.max(acc.queryCollectorsRejected, Number(entry.queryCollectorsRejected || 0)),
      }), {
        pendingQueryCollectors: 0,
        queuedQueryRequests: 0,
        activeQueryStreams: 0,
        queryCollectorTimeouts: 0,
        queryCollectorsRejected: 0,
      });
    const frameTotals = frameEntries.reduce((totals, entry) => {
      for (const key of ['activeTransfers', 'pendingAcks', 'incomingTransfers', 'priorityQueueDepth', 'highPriorityQueueDepth']) {
        totals[key] = Math.max(totals[key] || 0, Number(entry?.[key] || 0));
      }
      return totals;
    }, {});
    const ok = failedCollections.length === 0
      && reconnectingCollections.length === 0
      && replicationErrors.length === 0;
    return {
      checkedAt: new Date().toISOString(),
      ok,
      failures: ok ? [] : ['runtime_diagnostics_unhealthy'],
      failedCollections,
      reconnectingCollections,
      replicationErrors,
      frameTransport: {
        healthy: ok,
        unhealthyCollections: [...new Set([...failedCollections, ...reconnectingCollections])],
        totals: frameTotals,
      },
      demandTransport: demand,
    };
  });
}

async function openApp(page, app) {
  if (app.kind === 'shell-surface') {
    await openDesktopSurface(page);
    return page.evaluate(() => ({
      activeModule: document.body.dataset.activeModule || '',
      hash: location.hash,
      windows: globalThis.ctoxBusinessOsSmoke?.state?.windowManager?.listWindows?.() || [],
      shellSurface: 'desktop',
    }));
  }
  const sourceDefinition = app.kind === 'module' && !config.installedStrict
    ? JSON.parse(fs.readFileSync(
      path.join(repoRoot, 'src/apps/business-os/modules', app.id, 'module.json'),
      'utf8',
    ))
    : null;
  const launchKey = `launch-${app.id}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const startedAt = Date.now();
  await page.evaluate(({ appId, sourceDefinition, installedStrict, launchKey }) => {
    const api = globalThis.ctoxBusinessOsSmoke;
    const state = api?.state || globalThis.CTOX_BUSINESS_OS_APP || null;
    if (!api?.openDesktopApp || !state?.windowManager) throw new Error('Business OS desktop launcher is unavailable');
    const current = state.modules?.find((item) => item.id === appId);
    if (sourceDefinition && !current) throw new Error(`Installed Business OS registry is missing module: ${appId}`);
    if (!installedStrict && sourceDefinition) Object.assign(current, sourceDefinition);
    const existing = state.windowManager.listWindows?.().find((win) => win.ownerId === `desktop-app:${appId}`);
    let promise;
    if (existing) {
      state.windowManager.restore?.(existing.id);
      state.windowManager.focus?.(existing.id);
      promise = Promise.resolve(existing.id);
    } else {
      promise = Promise.resolve(api.openDesktopApp(appId, { mode: 'window' }));
    }
    globalThis.__CTOX_QA_APP_LAUNCHES ||= new Map();
    globalThis.__CTOX_QA_APP_LAUNCHES.set(launchKey, promise);
  }, { appId: app.id, sourceDefinition, installedStrict: config.installedStrict, launchKey });
  const ownerSelector = `.shell-window[data-owner-id="desktop-app:${app.id}"]`;
  await page.waitForSelector(ownerSelector, { timeout: 25000 });
  const visibleMountMs = Date.now() - startedAt;
  const loadingShadowObserved = await page.locator(ownerSelector).evaluate((windowElement) => (
    Boolean(windowElement.querySelector('[data-loading-shadow]'))
  ));
  await page.evaluate(async ({ launchKey, appId }) => {
    const promise = globalThis.__CTOX_QA_APP_LAUNCHES?.get(launchKey);
    if (!promise) throw new Error(`Business OS QA launch promise missing: ${appId}`);
    try {
      await Promise.race([
        promise,
        new Promise((_, reject) => setTimeout(() => reject(new Error(`App ready timed out: ${appId}`)), 25000)),
      ]);
    } finally {
      globalThis.__CTOX_QA_APP_LAUNCHES.delete(launchKey);
    }
  }, { launchKey, appId: app.id });
  const readyMountMs = Date.now() - startedAt;
  return page.evaluate(({ appId, visibleMountMs, readyMountMs, loadingShadowObserved }) => ({
    activeModule: document.body.dataset.activeModule || '',
    hash: location.hash,
    windows: globalThis.ctoxBusinessOsSmoke?.state?.windowManager?.listWindows?.() || [],
    windowedModule: appId,
    visibleMountMs,
    readyMountMs,
    loadingShadowObserved,
  }), { appId: app.id, visibleMountMs, readyMountMs, loadingShadowObserved });
}

async function waitForShellReady(page) {
  const deadline = Date.now() + config.readyTimeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await page.evaluate(async () => {
      const state = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP || null;
      const text = document.body?.innerText || '';
      const status = await globalThis.CTOX_BUSINESS_OS_STATUS?.snapshot?.({ includeCounts: false })
        .catch((error) => ({ ok: false, error: String(error?.message || error) }));
      return {
        title: document.title,
        url: location.href,
        activeModule: document.body.dataset.activeModule || state?.activeModule?.id || '',
        moduleLoading: document.body.dataset.moduleLoading || '',
        authState: document.body.dataset.authState || '',
        moduleCount: Array.isArray(state?.modules) ? state.modules.length : 0,
        taskbarPins: Array.isArray(state?.taskbarPins) ? state.taskbarPins : [],
        hasSmokeApi: !!globalThis.ctoxBusinessOsSmoke,
        hasStatusApi: !!globalThis.CTOX_BUSINESS_OS_STATUS,
        status,
        textSample: text.slice(0, 600),
      };
    });
    if (last.authState === 'locked') {
      throw new Error(`Business OS login gate is locked: ${JSON.stringify(last, null, 2)}`);
    }
    const installedRuntimeReady = !config.installedStrict || last.status?.ok === true;
    if (
      last.hasSmokeApi
      && last.moduleCount > 0
      && last.activeModule
      && !last.moduleLoading
      && installedRuntimeReady
    ) {
      return last;
    }
    await page.waitForTimeout(300);
  }
  throw new Error(`Business OS shell did not become ready: ${JSON.stringify(last, null, 2)}`);
}

async function collectRegistrySurfaces(page) {
  await openDesktopSurface(page);
  await safeScreenshot(page, '00a-desktop.png');

  const desktop = await page.evaluate(() => ({
    activeModule: document.body.dataset.activeModule || '',
    icons: [...document.querySelectorAll('.desktop-icon')].map((node) => ({
      id: node.dataset.iconId || '',
      target: node.dataset.target || '',
      label: node.querySelector('.desktop-icon-label')?.textContent?.trim() || node.textContent.trim(),
      visible: !!(node.offsetWidth || node.offsetHeight || node.getClientRects().length),
    })),
  }));

  const startMenu = await page.evaluate(() => {
    globalThis.toggleStartMenu?.({ preventDefault() {}, stopPropagation() {} });
    return {
      items: [...document.querySelectorAll('.shell-start-menu-panel .start-menu-item')].map((node) => ({
        label: node.querySelector('.start-menu-item-label')?.textContent?.trim() || node.textContent.trim(),
        pinned: !!node.querySelector('.start-menu-item-pin-btn.is-pinned'),
      })),
      categories: [...document.querySelectorAll('.shell-start-menu-panel .start-menu-category-title')].map((node) => node.textContent.trim()),
    };
  });
  await safeScreenshot(page, '00b-start-menu.png');
  await page.evaluate(() => document.querySelector('.shell-start-menu-panel')?.classList.remove('is-active'));

  await openModuleSurface(page, 'app-store');
  await page.waitForTimeout(Math.max(config.appWaitMs, 1200));
  await page.evaluate(() => document.querySelector('[data-scope="all"]')?.click());
  await page.waitForSelector('[data-apps-grid] .app-card', { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(500);
  const appStore = await page.evaluate(() => ({
    activeModule: document.body.dataset.activeModule || '',
    cards: [...document.querySelectorAll('[data-apps-grid] .app-card')].map((node) => ({
      id: node.dataset.appId || '',
      title: node.querySelector('.app-card-title')?.textContent?.trim() || '',
      category: node.querySelector('.app-card-category')?.textContent?.trim() || '',
      status: node.querySelector('.app-status-badge')?.textContent?.trim() || '',
    })),
    countText: document.querySelector('[data-apps-count]')?.textContent?.trim() || '',
  }));
  await safeScreenshot(page, '00c-app-store.png');

  const runtime = await page.evaluate(() => {
    const state = globalThis.ctoxBusinessOsSmoke?.state || globalThis.CTOX_BUSINESS_OS_APP || null;
    return {
      activeModule: document.body.dataset.activeModule || '',
      modules: (state?.modules || []).map((mod) => ({
        id: mod.id,
        title: mod.title || mod.id,
        entry: mod.entry || '',
        source: mod.source || '',
        install_scope: mod.install_scope || '',
        core: !!mod.core,
      })),
      taskbarPins: Array.isArray(state?.taskbarPins) ? state.taskbarPins : [],
      taskbarTabs: [...document.querySelectorAll('[data-module-tabs] .module-tab')].map((node) => ({
        target: node.dataset.target || node.dataset.module || '',
        label: node.querySelector('.module-tab-label')?.textContent?.trim() || node.textContent.trim(),
        pinned: node.dataset.pinned === 'true',
        running: node.dataset.running || '',
      })),
      windows: state?.windowManager?.listWindows?.() || [],
    };
  });

  return { runtime, desktop, startMenu, appStore };
}

async function openDesktopSurface(page) {
  await page.evaluate(() => {
    const button = document.querySelector('[data-show-desktop]');
    if (button) button.click();
    else location.hash = '#desktop';
  });
  try {
    await page.waitForFunction(
      () => document.body.dataset.activeModule === 'desktop' && !document.body.dataset.moduleLoading,
      undefined,
      { timeout: Math.min(config.readyTimeoutMs, 25000) }
    );
  } catch (error) {
    console.warn(`[business-os-qa] Desktop surface did not become active: ${error?.message || error}`);
  }
  await page.waitForTimeout(1000);
}

async function safeScreenshot(page, fileName) {
  try {
    await page.screenshot({
      path: path.join(config.outputDir, fileName),
      fullPage: false,
      animations: 'disabled',
      caret: 'hide',
      timeout: Math.min(config.readyTimeoutMs, 15000),
    });
  } catch (error) {
    console.warn(`[business-os-qa] Screenshot ${fileName} failed: ${error?.message || error}`);
  }
}

async function openModuleSurface(page, id) {
  const launchKind = await page.evaluate(async ({ moduleId, installedStrict }) => {
    const api = globalThis.ctoxBusinessOsSmoke;
    const state = api?.state || globalThis.CTOX_BUSINESS_OS_APP || null;
    const current = state?.modules?.find?.((item) => item.id === moduleId) || null;
    const windowed = installedStrict
      || current?.launch_kind === 'desktop-app'
      || current?.layout?.shell === 'windowed';
    if (windowed) {
      if (!api?.openDesktopApp) throw new Error('Business OS windowed module launcher is unavailable');
      await api.openDesktopApp(moduleId, { mode: 'window' });
      return 'window';
    }
    const hash = `#${moduleId}`;
    if (location.hash === hash) window.dispatchEvent(new HashChangeEvent('hashchange'));
    else location.hash = hash;
    return 'module';
  }, { moduleId: id, installedStrict: config.installedStrict });
  if (launchKind === 'window') {
    await page.waitForSelector(`.shell-window [data-module-root="${id}"]`, {
      timeout: config.readyTimeoutMs,
    });
    return;
  }
  await page.waitForFunction(
    (moduleId) => document.body.dataset.activeModule === moduleId && !document.body.dataset.moduleLoading,
    id,
    { timeout: config.readyTimeoutMs }
  );
}

async function collectDomCounts(page, app) {
  return page.evaluate((appInfo) => {
    const activeWindow = document.querySelector(
      `.shell-window[data-owner-id="desktop-app:${CSS.escape(appInfo.id)}"]`,
    ) || [...document.querySelectorAll('.shell-window')]
      .find((node) => node.classList.contains('is-focused'));
    const root = activeWindow?.querySelector('[data-window-content]')
      || document.querySelector('[data-module-host]')
      || document.body;
    const visible = (node) => !!(node.offsetWidth || node.offsetHeight || node.getClientRects().length);
    const text = root.innerText || '';
    const all = [...root.querySelectorAll('*')];
    const visibleButtons = [...root.querySelectorAll('button')].filter(visible);
    return {
      activeModule: document.body.dataset.activeModule || '',
      rootClass: root.className || '',
      title: activeWindow?.querySelector('[data-window-title]')?.textContent?.trim()
        || document.querySelector('.module-appbar-title')?.textContent?.trim()
        || document.title,
      totalElements: all.length,
      visibleElements: all.filter(visible).length,
      textLength: text.length,
      textSample: text.slice(0, 500),
      buttons: root.querySelectorAll('button').length,
      inputs: root.querySelectorAll('input').length,
      selects: root.querySelectorAll('select').length,
      textareas: root.querySelectorAll('textarea').length,
      links: root.querySelectorAll('a[href]').length,
      tables: root.querySelectorAll('table').length,
      rows: root.querySelectorAll('tr,[role="row"]').length,
      cards: root.querySelectorAll('.app-card,.card,[class*="card"]').length,
      dialogs: document.querySelectorAll('[role="dialog"],dialog').length,
      separators: root.querySelectorAll('[role="separator"],[data-resizer]').length,
      horizontalOverflow: root.scrollWidth > root.clientWidth + 2,
      unnamedVisibleButtons: visibleButtons.filter((button) => !String(
        button.getAttribute('aria-label') || button.title || button.textContent || '',
      ).trim()).length,
      emptyStateMentions: countMatches(text, [
        'Keine',
        'No ',
        'No documents',
        'No matching',
        'Keine Reports',
        'Keine Tabellen',
        'Keine Notizen',
      ]),
      syncErrorMentions: countMatches(text, [
        'WebRTC replication failed',
        'wurde nicht synchronisiert',
        'Sync',
        'Verbindung',
        'failed',
        'Fehler',
      ]),
    };

    function countMatches(haystack, needles) {
      return needles.reduce((count, needle) => count + (haystack.includes(needle) ? 1 : 0), 0);
    }
  }, app);
}

function pushRegistryFailures(failures, surfaces, staticRegistry) {
  if (!config.failOnRegistry) return;
  const coreExpected = new Set(appInventory.coreApps.map((app) => app.id));
  const launchableExpected = new Set(AUDIT_APP_BASELINE
    .filter((app) => app.kind !== 'shell-surface')
    .map((app) => app.id));
  const storeExpected = new Set(ALL_APP_BASELINE
    .filter((app) => app.kind !== 'shell-surface')
    .map((app) => app.id));
  const expectedTitleMap = new Map(ALL_APP_BASELINE.map((app) => [app.id, app.title]));
  const runtimeIds = new Set((surfaces.runtime.modules || []).map((item) => item.id));
  const desktopTargets = new Set((surfaces.desktop.icons || []).map((item) => item.target).filter(Boolean));
  const startLabels = new Set((surfaces.startMenu.items || []).map((item) => normalizeLabel(item.label)));
  const appStoreIds = new Set((surfaces.appStore.cards || []).map((item) => item.id).filter(Boolean));
  const taskbarTargets = new Set((surfaces.runtime.taskbarTabs || []).map((item) => item.target).filter(Boolean));
  const staticIds = new Set((staticRegistry.modules || []).map((item) => item.id));

  const checks = [
    {
      surface: 'runtime modules',
      actual: runtimeIds,
      expected: coreExpected,
      expectedLabel: (id) => id,
    },
    {
      surface: 'desktop icons',
      actual: desktopTargets,
      expected: launchableExpected,
      expectedLabel: (id) => id,
    },
    {
      surface: 'app store',
      actual: appStoreIds,
      expected: storeExpected,
      expectedLabel: (id) => id,
    },
    {
      surface: 'taskbar tabs',
      actual: taskbarTargets,
      expected: new Set(surfaces.runtime.taskbarPins || []),
      expectedLabel: (id) => id,
    },
    {
      surface: 'static modules/registry.json',
      actual: staticIds,
      expected: coreExpected,
      expectedLabel: (id) => id,
    },
  ];

  const missingStartMenuApps = AUDIT_APP_BASELINE
    .filter((app) => app.kind !== 'shell-surface')
    .filter((app) => ![app.title, ...(app.aliases || [])]
      .map(normalizeLabel)
      .some((label) => startLabels.has(label)))
    .map((app) => app.id);
  const duplicateStartMenuLabels = duplicateLabelsForSurface('start menu', surfaces, expectedTitleMap);
  if (missingStartMenuApps.length || duplicateStartMenuLabels.length) {
    failures.push({
      scope: 'registry',
      surface: 'start menu',
      message: 'start menu does not match the Business OS QA baseline',
      missing: missingStartMenuApps,
      duplicates: duplicateStartMenuLabels,
    });
  }

  for (const check of checks) {
    const missing = [...check.expected].filter((id) => !check.actual.has(check.expectedLabel(id)));
    const duplicates = duplicateLabelsForSurface(check.surface, surfaces, expectedTitleMap);
    if (missing.length || duplicates.length) {
      failures.push({
        scope: 'registry',
        surface: check.surface,
        message: `${check.surface} does not match the Business OS QA baseline`,
        missing,
        duplicates,
      });
    }
  }
}

function duplicateLabelsForSurface(surface, surfaces) {
  let values = [];
  if (surface === 'start menu') {
    values = (surfaces.startMenu.items || []).map((item) => normalizeLabel(item.label));
  } else if (surface === 'desktop icons') {
    values = (surfaces.desktop.icons || []).map((item) => item.target || normalizeLabel(item.label));
  } else if (surface === 'app store') {
    values = (surfaces.appStore.cards || []).map((item) => item.id || normalizeLabel(item.title));
  }
  const seen = new Set();
  const duplicates = new Set();
  for (const value of values.filter(Boolean)) {
    if (seen.has(value)) duplicates.add(value);
    else seen.add(value);
  }
  return [...duplicates];
}

function readStaticRegistry() {
  const registryPath = path.join(repoRoot, 'src/apps/business-os/modules/registry.json');
  try {
    const parsed = JSON.parse(fs.readFileSync(registryPath, 'utf8'));
    return {
      path: registryPath,
      ok: parsed?.ok !== false,
      modules: Array.isArray(parsed?.modules) ? parsed.modules.map((mod) => ({
        id: mod.id,
        title: mod.title || mod.id,
        entry: mod.entry || '',
        source: mod.source || '',
        install_scope: mod.install_scope || '',
        core: !!mod.core,
      })) : [],
    };
  } catch (error) {
    return { path: registryPath, ok: false, modules: [], error: error?.message || String(error) };
  }
}

function attachConsoleCapture(page) {
  page.on('console', (message) => {
    if (message.type() !== 'error' && message.type() !== 'warning') return;
    const text = message.text();
    if (/favicon|ERR_ABORTED/i.test(text)) return;
    consoleEvents.push({
      at: new Date().toISOString(),
      level: message.type(),
      text,
      location: message.location(),
    });
  });
  page.on('pageerror', (error) => {
    consoleEvents.push({
      at: new Date().toISOString(),
      level: 'pageerror',
      text: error?.stack || error?.message || String(error),
    });
  });
  page.on('requestfailed', (request) => {
    const failure = request.failure();
    if (!failure || /favicon/i.test(request.url()) || /ERR_ABORTED/i.test(failure.errorText || '')) return;
    consoleEvents.push({
      at: new Date().toISOString(),
      level: 'error',
      text: `requestfailed ${request.method()} ${request.url()} ${failure.errorText}`,
    });
  });
}

async function attachLocalAssetRouting(page) {
  const assetRoot = path.join(repoRoot, 'src/apps/business-os');
  await page.route('**/*', async (route) => {
    const url = new URL(route.request().url());
    const relativePath = decodeURIComponent(url.pathname === '/' ? 'index.html' : url.pathname.slice(1));
    const servedRelativePath = relativePath.replace(/^business-os\//, '');
    if (/desktop-apps\/(explorer|code-editor)/.test(relativePath)) {
      localAssetEvents.push({ url: route.request().url(), relativePath, action: 'seen' });
    }
    if (config.localAssetPrefixes.length
      && !config.localAssetPrefixes.some((prefix) => servedRelativePath === prefix || servedRelativePath.startsWith(`${prefix}/`))) {
      await route.fallback();
      return;
    }
    const candidate = path.resolve(assetRoot, servedRelativePath);
    if (!candidate.startsWith(`${assetRoot}${path.sep}`) && candidate !== path.join(assetRoot, 'index.html')) {
      await route.fallback();
      return;
    }
    try {
      if (!fs.statSync(candidate).isFile()) throw new Error('not a file');
      const ext = path.extname(candidate).toLowerCase();
      const contentType = {
        '.html': 'text/html; charset=utf-8',
        '.js': 'text/javascript; charset=utf-8',
        '.mjs': 'text/javascript; charset=utf-8',
        '.css': 'text/css; charset=utf-8',
        '.json': 'application/json; charset=utf-8',
        '.svg': 'image/svg+xml',
      }[ext] || 'application/octet-stream';
      await route.fulfill({ status: 200, contentType, body: fs.readFileSync(candidate) });
      localAssetEvents.push({ url: route.request().url(), relativePath: servedRelativePath, action: 'fulfilled' });
    } catch {
      await route.fallback();
    }
  });
}

function resolvePlaywrightModule() {
  const candidates = [
    process.env.PLAYWRIGHT_MODULE_PATH,
    'playwright',
    '/tmp/ctox-pw-smoke/node_modules/playwright',
  ].filter(Boolean);
  for (const candidate of candidates) {
    try {
      return require.resolve(candidate);
    } catch {
      // Try next candidate.
    }
  }
  throw new Error('No Playwright runtime found. Install playwright or set PLAYWRIGHT_MODULE_PATH.');
}

function chromiumLaunchOptions() {
  const executablePath = existingChromeExecutable();
  const options = {
    headless: config.headless,
  };
  if (executablePath) options.executablePath = executablePath;
  return options;
}

function existingChromeExecutable() {
  if (process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE) return process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE;
  const playwrightPath = chromium.executablePath?.();
  const candidates = [
    playwrightPath,
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
  ].filter(Boolean);
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function withQuery(rawUrl, key, value) {
  const url = new URL(rawUrl);
  url.searchParams.set(key, value);
  return url.toString();
}

function parsePositiveInt(value, name) {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer; got ${JSON.stringify(String(value))}`);
  }
  return parsed;
}

function summarizePerformance(apps) {
  const warmMounts = apps.map((app) => Number(app.warmMountMs)).filter(Number.isFinite);
  const warmReady = apps.map((app) => Number(app.warmReadyMs)).filter(Number.isFinite);
  const interactions = apps
    .map((app) => Number(app.presentation?.interactionMaxMs))
    .filter(Number.isFinite);
  return {
    warmMountSamples: warmMounts.length,
    warmMountP95Ms: percentile(warmMounts, 0.95),
    warmMountMaxMs: warmMounts.length ? Math.max(...warmMounts) : 0,
    warmReadyP95Ms: percentile(warmReady, 0.95),
    warmReadyMaxMs: warmReady.length ? Math.max(...warmReady) : 0,
    loadingShadowObserved: apps.filter((app) => app.warmLoadingShadowObserved).length,
    interactionSamples: interactions.length,
    interactionP95Ms: percentile(interactions, 0.95),
    interactionMaxMs: interactions.length ? Math.max(...interactions) : 0,
  };
}

function percentile(values, quantile) {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * quantile) - 1)];
}

function consoleEventKey(entry) {
  return `${entry?.at || ''}\u0000${entry?.level || ''}\u0000${entry?.text || ''}`;
}

function withHostTimeout(promise, timeoutMs, message) {
  let timer = null;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timer = setTimeout(() => {
        const error = new Error(message);
        error.code = 'BUSINESS_OS_QA_HOST_TIMEOUT';
        reject(error);
      }, timeoutMs);
    }),
  ]).finally(() => clearTimeout(timer));
}

function writeJson(name, value) {
  fs.writeFileSync(path.join(config.outputDir, name), `${JSON.stringify(value, null, 2)}\n`);
}

function writeMarkdownReport(data) {
  const lines = [];
  lines.push('# Business OS QA Baseline');
  lines.push('');
  lines.push(`- Status: ${data.ok ? 'OK' : 'FAILED'}`);
  lines.push(`- URL: ${data.config.url}`);
  lines.push(`- Started: ${data.startedAt}`);
  lines.push(`- Ended: ${data.endedAt}`);
  lines.push(`- Apps checked: ${data.apps.length}/${data.expectedApps.length}`);
  lines.push(`- Core matrix: ${data.apps.filter((app) => app.cohort === 'core').length}/${data.expectedCoreApps}`);
  lines.push(`- Compatibility surfaces: ${data.apps.filter((app) => app.cohort === 'compatibility').length}/${data.expectedCompatibilityApps}`);
  lines.push(`- Console events captured: ${data.console.length}`);
  lines.push(`- Warm mount p95: ${data.performance?.warmMountP95Ms ?? 'n/a'} ms (budget ${data.config.warmMountBudgetMs} ms)`);
  lines.push(`- Warm ready p95: ${data.performance?.warmReadyP95Ms ?? 'n/a'} ms (diagnostic, includes app data work)`);
  lines.push(`- Loading shadow observed: ${data.performance?.loadingShadowObserved ?? 0} mounts`);
  lines.push(`- Visible interaction p95: ${data.performance?.interactionP95Ms ?? 'n/a'} ms (budget ${data.config.interactionBudgetMs} ms)`);
  lines.push('');
  lines.push('## Registry Surfaces');
  lines.push('');
  const surface = data.surfaces || {};
  lines.push(`- Runtime modules: ${(surface.runtime?.modules || []).map((item) => item.id).join(', ')}`);
  lines.push(`- Desktop icons: ${(surface.desktop?.icons || []).map((item) => item.target || item.label).join(', ')}`);
  lines.push(`- Start menu: ${(surface.startMenu?.items || []).map((item) => item.label).join(', ')}`);
  lines.push(`- App Store cards: ${(surface.appStore?.cards || []).map((item) => item.id || item.title).join(', ')}`);
  lines.push(`- Taskbar tabs: ${(surface.runtime?.taskbarTabs || []).map((item) => item.target || item.label).join(', ')}`);
  lines.push('');
  lines.push('## App Smoke');
  lines.push('');
  lines.push('| App | Kind | Ready ms | Warm visible ms | Warm ready ms | Interaction ms | Presentation | Health after open/close | Screenshot | Elements | Buttons | Inputs | Tables | Rows | Console Errors |');
  lines.push('|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const app of data.apps) {
    const health = `${app.healthAfterOpen?.ok ? 'OK' : 'FAIL'}/${app.healthAfterClose?.ok ? 'OK' : 'FAIL'}`;
    lines.push(`| ${escapeMd(app.title)} | ${app.kind} | ${app.openDurationMs} | ${app.warmMountMs ?? 'n/a'} | ${app.warmReadyMs ?? 'n/a'} | ${app.presentation?.interactionMaxMs ?? 'n/a'} | ${app.presentation?.applicable === false ? 'N/A' : app.presentation?.ok ? 'OK' : 'FAIL'} | ${health} | ${app.screenshot} | ${app.dom.totalElements} | ${app.dom.buttons} | ${app.dom.inputs + app.dom.selects + app.dom.textareas} | ${app.dom.tables} | ${app.dom.rows} | ${app.consoleErrors.length} |`);
  }
  lines.push('');
  if (data.failures.length) {
    lines.push('## Failures');
    lines.push('');
    for (const failure of data.failures) {
      lines.push(`- ${escapeMd(failure.scope || 'unknown')}: ${escapeMd(failure.message || '')}`);
      if (failure.surface) lines.push(`  - Surface: ${escapeMd(failure.surface)}`);
      if (failure.app) lines.push(`  - App: ${escapeMd(failure.app)}`);
      if (failure.missing?.length) lines.push(`  - Missing: ${failure.missing.map(escapeMd).join(', ')}`);
      if (failure.duplicates?.length) lines.push(`  - Duplicates: ${failure.duplicates.map(escapeMd).join(', ')}`);
    }
    lines.push('');
  }
  lines.push('## Artifacts');
  lines.push('');
  lines.push(`- JSON: ${path.join(config.outputDir, 'business-os-qa-baseline.json')}`);
  lines.push(`- Screenshots: ${config.outputDir}`);
  fs.writeFileSync(path.join(config.outputDir, 'business-os-qa-baseline.md'), `${lines.join('\n')}\n`);
}

function normalizeLabel(value) {
  return String(value || '')
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .trim()
    .toLowerCase();
}

function timestampForPath() {
  return new Date().toISOString().replace(/[:.]/g, '-');
}

function slug(value) {
  return String(value || 'app').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}

function escapeMd(value) {
  return String(value ?? '').replace(/\|/g, '\\|').replace(/\n/g, ' ');
}
