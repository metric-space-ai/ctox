#!/usr/bin/env node
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');

const AUDIT_APP_BASELINE = [
  { id: 'ctox', title: 'CTOX', kind: 'module' },
  { id: 'reports', title: 'Bugs & Features', kind: 'module' },
  { id: 'documents', title: 'Documents', kind: 'module' },
  { id: 'knowledge', title: 'Knowledge', kind: 'module' },
  { id: 'research', title: 'Web Research', kind: 'module' },
  { id: 'matching', title: 'Matching', kind: 'module' },
  { id: 'conversations', title: 'Conversations', kind: 'module' },
  { id: 'outbound', title: 'Outbound', kind: 'module' },
  { id: 'shiftflow', title: 'Einsatzplanung', kind: 'module' },
  { id: 'spreadsheets', title: 'Spreadsheets', kind: 'module' },
  { id: 'notes', title: 'Notizen', kind: 'module', aliases: ['notizen'] },
  { id: 'app-store', title: 'App Store', kind: 'module' },
  { id: 'buchhaltung', title: 'Buchhaltung', kind: 'module' },
  { id: 'calendar', title: 'Kalender', kind: 'module' },
  { id: 'coding-agents', title: 'Coding Agents', kind: 'module' },
  { id: 'explorer', title: 'Files', kind: 'desktop-app' },
  { id: 'code-editor', title: 'Source Editor', kind: 'desktop-app' },
  { id: 'creator', title: 'App Creator', kind: 'desktop-app' },
];

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
  failOnRegistry: process.env.BUSINESS_OS_QA_FAIL_ON_REGISTRY !== '0',
};

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
    failOnRegistry: config.failOnRegistry,
  },
  expectedApps: AUDIT_APP_BASELINE,
  staticRegistry: readStaticRegistry(),
  shell: null,
  surfaces: null,
  apps: [],
  console: [],
  failures: [],
};

const consoleEvents = [];
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
    attachConsoleCapture(page);

    const smokeUrl = withQuery(config.url, 'rxdbSmoke', '1');
    await page.goto(smokeUrl, { waitUntil: 'domcontentloaded', timeout: 30000 });
    summary.shell = await waitForShellReady(page);

    summary.surfaces = await collectRegistrySurfaces(page);
    await page.screenshot({ path: path.join(config.outputDir, '00-registry-surfaces.png'), fullPage: false });
    pushRegistryFailures(summary.failures, summary.surfaces, summary.staticRegistry);

    for (let index = 0; index < AUDIT_APP_BASELINE.length; index += 1) {
      const app = AUDIT_APP_BASELINE[index];
      const appResult = await captureApp(page, app, index + 1);
      summary.apps.push(appResult);
      if (config.failOnConsole && appResult.consoleErrors.length > 0) {
        summary.failures.push({
          scope: 'console',
          app: app.id,
          message: `${app.title} emitted ${appResult.consoleErrors.length} console/page error(s)`,
          errors: appResult.consoleErrors,
        });
      }
    }

    summary.console = consoleEvents;
    summary.ok = summary.failures.length === 0;
    await context.close().catch(() => {});
  } finally {
    await browser.close().catch(() => {});
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
  summary.ok = summary.failures.length === 0;
  writeJson('business-os-qa-baseline.json', summary);
  writeMarkdownReport(summary);
}

if (!summary.ok) {
  console.error(`Business OS QA baseline failed with ${summary.failures.length} issue(s).`);
  console.error(`Report: ${path.join(config.outputDir, 'business-os-qa-baseline.md')}`);
  process.exit(1);
}

console.log(`Business OS QA baseline OK: ${AUDIT_APP_BASELINE.length} apps checked.`);
console.log(`Report: ${path.join(config.outputDir, 'business-os-qa-baseline.md')}`);

async function captureApp(page, app, ordinal) {
  const consoleStart = consoleEvents.length;
  const opened = await openApp(page, app);
  await page.waitForTimeout(config.appWaitMs);
  const dom = await collectDomCounts(page, app);
  const screenshot = `${String(ordinal).padStart(2, '0')}-${slug(app.id)}.png`;
  await page.screenshot({ path: path.join(config.outputDir, screenshot), fullPage: false });
  const consoleErrors = consoleEvents
    .slice(consoleStart)
    .filter((entry) => entry.level === 'error' || entry.level === 'pageerror');
  return {
    id: app.id,
    title: app.title,
    kind: app.kind,
    opened,
    screenshot,
    dom,
    consoleErrors,
  };
}

async function openApp(page, app) {
  if (app.kind === 'desktop-app') {
    return page.evaluate(async (appId) => {
      const api = globalThis.ctoxBusinessOsSmoke;
      const state = api?.state || globalThis.CTOX_BUSINESS_OS_APP || null;
      if (!api?.openDesktopApp || !state?.windowManager) {
        throw new Error('Business OS smoke desktop launcher is unavailable');
      }
      const existing = state.windowManager.listWindows?.()
        .find((win) => win.ownerId === `desktop-app:${appId}`);
      if (existing) {
        state.windowManager.restore?.(existing.id);
        state.windowManager.focus?.(existing.id);
      } else {
        await api.openDesktopApp(appId);
      }
      return {
        activeModule: document.body.dataset.activeModule || '',
        windows: state.windowManager.listWindows?.() || [],
      };
    }, app.id);
  }

  const targetHash = `#${app.id}`;
  await page.evaluate((hash) => {
    if (location.hash === hash) {
      window.dispatchEvent(new HashChangeEvent('hashchange'));
    } else {
      location.hash = hash;
    }
  }, targetHash);
  await page.waitForFunction(
    (id) => document.body.dataset.activeModule === id && !document.body.dataset.moduleLoading,
    app.id,
    { timeout: 25000 }
  );
  return page.evaluate(() => ({
    activeModule: document.body.dataset.activeModule || '',
    hash: location.hash,
    windows: globalThis.CTOX_BUSINESS_OS_APP?.windowManager?.listWindows?.() || [],
  }));
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
    if (last.hasSmokeApi && last.moduleCount > 0 && last.activeModule && !last.moduleLoading) {
      return last;
    }
    await page.waitForTimeout(300);
  }
  throw new Error(`Business OS shell did not become ready: ${JSON.stringify(last, null, 2)}`);
}

async function collectRegistrySurfaces(page) {
  await openDesktopSurface(page);
  await page.screenshot({ path: path.join(config.outputDir, '00a-desktop.png'), fullPage: false });

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
  await page.screenshot({ path: path.join(config.outputDir, '00b-start-menu.png'), fullPage: false });
  await page.evaluate(() => document.querySelector('.shell-start-menu-panel')?.classList.remove('is-active'));

  await openModuleSurface(page, 'app-store');
  await page.waitForTimeout(config.appWaitMs);
  await page.evaluate(() => document.querySelector('[data-scope="all"]')?.click());
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
  await page.screenshot({ path: path.join(config.outputDir, '00c-app-store.png'), fullPage: false });

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
  await page.waitForFunction(
    () => document.body.dataset.activeModule === 'desktop' && !document.body.dataset.moduleLoading,
    undefined,
    { timeout: 25000 }
  );
  await page.waitForTimeout(1000);
}

async function openModuleSurface(page, id) {
  await page.evaluate((moduleId) => {
    const hash = `#${moduleId}`;
    if (location.hash === hash) window.dispatchEvent(new HashChangeEvent('hashchange'));
    else location.hash = hash;
  }, id);
  await page.waitForFunction(
    (moduleId) => document.body.dataset.activeModule === moduleId && !document.body.dataset.moduleLoading,
    id,
    { timeout: 25000 }
  );
}

async function collectDomCounts(page, app) {
  return page.evaluate((appInfo) => {
    const activeWindow = appInfo.kind === 'desktop-app'
      ? [...document.querySelectorAll('.shell-window')]
        .find((node) => node.classList.contains('is-focused'))
      : null;
    const root = activeWindow?.querySelector('[data-window-content]')
      || document.querySelector('[data-module-host]')
      || document.body;
    const visible = (node) => !!(node.offsetWidth || node.offsetHeight || node.getClientRects().length);
    const text = root.innerText || '';
    const all = [...root.querySelectorAll('*')];
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
  const expected = new Set(AUDIT_APP_BASELINE.map((app) => app.id));
  const moduleExpected = new Set(AUDIT_APP_BASELINE.filter((app) => app.kind === 'module').map((app) => app.id));
  const expectedTitleMap = new Map(AUDIT_APP_BASELINE.map((app) => [app.id, app.title]));
  const normalizedExpectedTitles = new Set(AUDIT_APP_BASELINE.flatMap((app) => [app.title, ...(app.aliases || [])]).map(normalizeLabel));

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
      expected: moduleExpected,
      expectedLabel: (id) => id,
    },
    {
      surface: 'desktop icons',
      actual: desktopTargets,
      expected,
      expectedLabel: (id) => id,
    },
    {
      surface: 'start menu',
      actual: startLabels,
      expected: normalizedExpectedTitles,
      expectedLabel: (label) => label,
    },
    {
      surface: 'app store',
      actual: appStoreIds,
      expected,
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
      expected: moduleExpected,
      expectedLabel: (id) => id,
    },
  ];

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
    if (!failure || /favicon/i.test(request.url())) return;
    consoleEvents.push({
      at: new Date().toISOString(),
      level: 'error',
      text: `requestfailed ${request.method()} ${request.url()} ${failure.errorText}`,
    });
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
    args: ['--disable-gpu'],
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
  lines.push(`- Apps checked: ${data.apps.length}/${AUDIT_APP_BASELINE.length}`);
  lines.push(`- Console events captured: ${data.console.length}`);
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
  lines.push('| App | Kind | Screenshot | Elements | Buttons | Inputs | Tables | Rows | Console Errors |');
  lines.push('|---|---|---:|---:|---:|---:|---:|---:|---:|');
  for (const app of data.apps) {
    lines.push(`| ${escapeMd(app.title)} | ${app.kind} | ${app.screenshot} | ${app.dom.totalElements} | ${app.dom.buttons} | ${app.dom.inputs + app.dom.selects + app.dom.textareas} | ${app.dom.tables} | ${app.dom.rows} | ${app.consoleErrors.length} |`);
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
