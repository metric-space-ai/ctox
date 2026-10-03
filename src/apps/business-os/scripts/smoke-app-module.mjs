#!/usr/bin/env node
import { existsSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const RELEASE_ROOT = resolve(SCRIPT_DIR, '../../../..');

function unique(values) {
  return Array.from(new Set(values));
}

function ensureParentDir(path) {
  const parent = dirname(path);
  if (parent && parent !== path) mkdirSync(parent, { recursive: true });
}

function findRuntimeChromiumExecutable(root) {
  const cacheDir = join(root, 'runtime/browser/interactive-reference/ms-playwright');
  if (!existsSync(cacheDir)) return null;
  for (const entry of readdirSync(cacheDir)) {
    if (!entry.startsWith('chromium-') && !entry.startsWith('chromium_headless_shell-')) continue;
    const base = join(cacheDir, entry);
    const candidates = [
      join(base, 'chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing'),
      join(base, 'chrome-mac/headless_shell'),
      join(base, 'chrome-linux/chrome'),
      join(base, 'chrome-linux64/chrome'),
      join(base, 'chrome-headless-shell-linux64/chrome-headless-shell'),
      join(base, 'chrome-win/chrome.exe'),
    ];
    const executable = candidates.find((candidate) => existsSync(candidate));
    if (executable) return executable;
  }
  return null;
}

function browserPackageJsonCandidates() {
  return unique([
    join(RELEASE_ROOT, 'runtime/browser/interactive-reference/package.json'),
    join(RELEASE_ROOT, 'src/apps/business-os/package.json'),
    join(SCRIPT_DIR, '../package.json'),
    join(process.cwd(), 'package.json'),
  ]).filter((candidate) => existsSync(candidate));
}

function loadBrowserRuntime() {
  const failures = [];
  for (const packageJson of browserPackageJsonCandidates()) {
    for (const packageName of ['playwright', 'patchright']) {
      try {
        const require = createRequire(packageJson);
        const runtime = require(packageName);
        if (!runtime?.chromium) throw new Error(`${packageName} did not expose chromium`);
        const executablePath = findRuntimeChromiumExecutable(RELEASE_ROOT);
        return {
          chromium: runtime.chromium,
          launchOptions: executablePath ? { executablePath } : {},
          evidence: {
            package: packageName,
            package_json: packageJson,
            executable_path: executablePath,
          },
        };
      } catch (error) {
        failures.push(`${packageName} from ${packageJson}: ${error.message}`);
      }
    }
  }
  throw new Error(`could not load CTOX browser runtime (${failures.join('; ')})`);
}

const browserRuntime = loadBrowserRuntime();

function usage() {
  return [
    'Usage: node src/apps/business-os/scripts/smoke-app-module.mjs <module-id> [--url <business-os-url>] [--json] [--create-action <action>] [--timeout-ms <n>] [--output <path>] [--screenshot <path>]',
    '',
    'Runs a real-browser smoke against a mounted CTOX Business OS app.',
  ].join('\n');
}

function parseArgs(argv) {
  const options = {
    moduleId: null,
    url: 'http://127.0.0.1:8765',
    json: false,
    createAction: null,
    timeoutMs: 90000,
    output: null,
    screenshot: null,
  };
  for (let idx = 0; idx < argv.length; idx += 1) {
    const arg = argv[idx];
    if (arg === '--url') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--url requires a value');
      options.url = value;
      idx += 1;
    } else if (arg === '--create-action') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--create-action requires a value');
      options.createAction = value;
      idx += 1;
    } else if (arg === '--timeout-ms') {
      const value = Number(argv[idx + 1]);
      if (!Number.isFinite(value) || value < 1000) throw new Error('--timeout-ms must be a number >= 1000');
      options.timeoutMs = value;
      idx += 1;
    } else if (arg === '--output') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--output requires a path');
      options.output = resolve(value);
      idx += 1;
    } else if (arg === '--screenshot') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--screenshot requires a path');
      options.screenshot = resolve(value);
      idx += 1;
    } else if (arg === '--json') {
      options.json = true;
    } else if (arg === '--installed' || arg === '--source') {
      // The browser smoke mounts through the shell. Source/installed mode is
      // expressed by the shell catalog entry, so the flag is accepted for CLI
      // symmetry with app validate/finalize.
    } else if (arg === '--help' || arg === '-h') {
      options.help = true;
    } else if (arg.startsWith('-')) {
      throw new Error(`unknown option: ${arg}`);
    } else if (!options.moduleId) {
      options.moduleId = arg;
    } else {
      throw new Error(`unexpected argument: ${arg}`);
    }
  }
  if (options.help) return options;
  if (!options.moduleId || /[\\/]/.test(options.moduleId) || options.moduleId === '.' || options.moduleId === '..') {
    throw new Error('module id is required and must be a single path segment');
  }
  return options;
}

function withModuleHash(baseUrl, moduleId) {
  const url = new URL(baseUrl);
  url.searchParams.set('rxdbSmoke', '1');
  url.searchParams.set('qaInstalledModule', moduleId);
  url.hash = moduleId;
  return url.href;
}

function withoutModuleHash(baseUrl, moduleId) {
  const url = new URL(baseUrl);
  url.searchParams.set('rxdbSmoke', '1');
  url.searchParams.set('qaInstalledModule', moduleId);
  url.hash = '';
  return url.href;
}

function redactedRequestUrl(value) {
  try {
    const url = new URL(value);
    return `${url.origin}${url.pathname}`;
  } catch {
    return '<invalid-url>';
  }
}

function printResult(result, json) {
  if (json) {
    console.log(JSON.stringify(result, null, 2));
  } else if (result.ok) {
    console.log(`Business OS app browser smoke OK: ${result.module_id}`);
  } else {
    console.error(`Business OS app browser smoke failed for ${result.module_id}:`);
    for (const failure of result.failures) console.error(`- ${failure}`);
  }
}

async function waitForPrimaryCreateAction(page, moduleId, timeoutMs) {
  try {
    const handle = await page.waitForFunction((id) => {
      function isVisible(el) {
        const box = el.getBoundingClientRect();
        const style = window.getComputedStyle(el);
        return box.width > 0
          && box.height > 0
          && style.visibility !== 'hidden'
          && style.display !== 'none'
          && !el.disabled;
      }
      function isPrimaryCreateAction(value) {
        const action = String(value || '').trim().toLowerCase();
        if (!action) return false;
        if (/(^|[-_:])(follow-?up|review|save|submit|cancel|close|edit|archive|delete|remove)([-_:]|$)/.test(action)) return false;
        return /^(add|new|create)([-_:]|$)/.test(action);
      }
      const root = document.querySelector(`[data-module-root="${id}"]`);
      if (!root) return false;
      const actions = Array.from(root.querySelectorAll('[data-action]'))
        .map((el) => ({
          action: el.getAttribute('data-action'),
          visible: isVisible(el),
        }))
        .filter((item) => item.visible && isPrimaryCreateAction(item.action));
      return actions[0]?.action || false;
    }, moduleId, { timeout: Math.min(timeoutMs, 3000), polling: 250 });
    return handle.jsonValue();
  } catch (error) {
    if (error?.name === 'TimeoutError' || /timeout/i.test(String(error?.message || error))) return null;
    throw error;
  }
}

async function openModule(page, moduleId, url, timeoutMs) {
  const rootSelector = `[data-module-root="${moduleId}"]`;
  await page.goto(withoutModuleHash(url, moduleId), { waitUntil: 'domcontentloaded', timeout: timeoutMs });
  const deadline = Date.now() + timeoutMs;
  let opened = false;
  while (!opened && Date.now() <= deadline) {
    try {
      opened = await page.evaluate(async (id) => {
        const app = window.CTOX_BUSINESS_OS_APP;
        if (typeof app?.openModule === 'function'
          && app.modules?.find?.((module) => module.id === id)) {
          const target = new URL(location.href);
          target.hash = id;
          history.replaceState(history.state, '', target.href);
          await app.openModule(id, { force: true });
          return true;
        }
        const selector = [
          `.desktop-icon[data-target="${CSS.escape(id)}"]`,
          `.module-tab[data-target="${CSS.escape(id)}"]`,
          `.start-menu-item[data-target="${CSS.escape(id)}"]`,
        ].join(',');
        const launcher = Array.from(document.querySelectorAll(selector)).find((element) => {
          const box = element.getBoundingClientRect();
          const style = window.getComputedStyle(element);
          return box.width > 0 && box.height > 0
            && style.visibility !== 'hidden' && style.display !== 'none';
        });
        if (!launcher) return false;
        launcher.click();
        return true;
      }, moduleId);
    } catch (error) {
      if (!/execution context was destroyed|cannot find context/i.test(String(error?.message || error))) throw error;
    }
    if (!opened) await page.waitForTimeout(250);
  }
  if (!opened) throw new Error(`Business OS shell did not expose module ${moduleId} before timeout`);
  await page.waitForFunction((selector) => {
    const root = document.querySelector(selector);
    return root?.dataset.moduleReady === 'true';
  }, rootSelector, { timeout: timeoutMs, polling: 100 });
  const mount = await page.evaluate(({ id, selector }) => {
    const root = document.querySelector(selector);
    return {
      failed: root?.dataset.moduleLoadFailed === 'true',
      failure: globalThis.ctoxBusinessOsSmoke?.state?.qaModuleMountFailures?.[id]
        || window.CTOX_BUSINESS_OS_APP?.qaModuleMountFailures?.[id]
        || null,
      recovery_text: root?.querySelector('.shell-app-recovery')?.textContent?.trim() || '',
    };
  }, { id: moduleId, selector: rootSelector });
  if (mount.failed) {
    throw new Error(`module mount failed: ${mount.failure?.message || mount.recovery_text || moduleId}`);
  }
  await page.waitForSelector(rootSelector, { state: 'visible', timeout: timeoutMs });
  return rootSelector;
}

// Exercise the mounted module and shell pane grammar together. This deliberately
// does not stand in for populated-data, authorization, or native-sync acceptance.
async function verifyReportsChrome(page, rootSelector, timeoutMs) {
  const root = page.locator(rootSelector);
  const toggle = root.locator('[data-reports-view-toggle]');
  const trayToggle = root.locator('[data-pg-tray-toggle]');
  const tray = root.locator('[data-pg-tray]');
  const originalView = await toggle.getAttribute('data-pg-view');
  const originalLabel = await toggle.getAttribute('aria-label');
  if (!['cards', 'list'].includes(originalView) || !originalLabel) {
    throw new Error('Reports view toggle has no valid initial state or accessible label');
  }
  const timings = [];
  for (const expected of [originalView === 'cards' ? 'list' : 'cards', originalView]) {
    const start = performance.now();
    await toggle.click({ timeout: timeoutMs });
    await page.waitForFunction(({ selector, expectedView }) => {
      const button = document.querySelector(selector)?.querySelector('[data-reports-view-toggle]');
      return button?.getAttribute('data-pg-view') === expectedView;
    }, { selector: rootSelector, expectedView: expected }, { timeout: timeoutMs });
    const label = await toggle.getAttribute('aria-label');
    if (!label || (expected !== originalView && label === originalLabel)) {
      throw new Error('Reports view toggle did not update its accessible action label');
    }
    if (await toggle.getAttribute('aria-pressed') !== null) {
      throw new Error('Reports single-action view toggle unexpectedly exposes aria-pressed');
    }
    timings.push({ view: expected, click_to_state_ms: performance.now() - start });
  }
  const wasExpanded = await trayToggle.getAttribute('aria-expanded') === 'true';
  for (const expanded of [!wasExpanded, wasExpanded]) {
    await trayToggle.click({ timeout: timeoutMs });
    await page.waitForFunction(({ selector, expectedExpanded }) => {
      const host = document.querySelector(selector);
      const button = host?.querySelector('[data-pg-tray-toggle]');
      const panel = host?.querySelector('[data-pg-tray]');
      return button?.getAttribute('aria-expanded') === String(expectedExpanded)
        && panel?.hidden === !expectedExpanded;
    }, { selector: rootSelector, expectedExpanded: expanded }, { timeout: timeoutMs });
    if (await tray.isVisible() !== expanded) {
      throw new Error('Reports filter tray CSS visibility disagrees with shell grammar state');
    }
  }
  if (await toggle.getAttribute('data-pg-view') !== originalView) {
    throw new Error('Reports filter tray changed the selected view');
  }
  return { passed: true, view_round_trip: true, tray_round_trip: true,
    timings, scope: 'mounted-shell-chrome; no data, permissions, or sync acceptance' };
}

async function runSmoke(options) {

  const result = {
    ok: false,
    module_id: options.moduleId,
    url: withModuleHash(options.url, options.moduleId),
    create_action: options.createAction,
    failures: [],
    evidence: {},
  };

  result.evidence.browser_runtime = browserRuntime.evidence;
  const browser = await browserRuntime.chromium.launch({
    headless: true,
    ...browserRuntime.launchOptions,
  });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage();
  const consoleErrors = [];
  const pageErrors = [];
  const failedRequests = [];

  page.on('console', (message) => {
    if (message.type() === 'error') {
      consoleErrors.push(message.text().slice(0, 1200));
    }
  });
  page.on('pageerror', (error) => {
    pageErrors.push(String(error.stack || error.message || error).slice(0, 1200));
  });
  page.on('requestfailed', (request) => {
    failedRequests.push({
      url: redactedRequestUrl(request.url()),
      error: request.failure()?.errorText || '',
    });
  });

  await page.addInitScript(() => {
    window.__ctoxAppSmoke = { showModalCalls: [], docClicks: [] };
    const originalShowModal = HTMLDialogElement.prototype.showModal;
    HTMLDialogElement.prototype.showModal = function patchedShowModal(...args) {
      window.__ctoxAppSmoke.showModalCalls.push({
        id: this.id || '',
        classes: this.className || '',
        time: Date.now(),
      });
      return originalShowModal.apply(this, args);
    };
    document.addEventListener('click', (event) => {
      const target = event.target?.closest?.('[data-action]');
      if (target) {
        window.__ctoxAppSmoke.docClicks.push({
          action: target.getAttribute('data-action'),
          text: target.textContent.trim().slice(0, 120),
          time: Date.now(),
        });
      }
    }, true);
  });

  try {
    const rootSelector = await openModule(page, options.moduleId, options.url, options.timeoutMs);
    result.evidence.mount = await page.evaluate((selector) => {
      const root = document.querySelector(selector);
      if (!root) return { visible: false };
      const box = root.getBoundingClientRect();
      const style = window.getComputedStyle(root);
      return {
        visible: box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none',
        width: Math.round(box.width),
        height: Math.round(box.height),
        text_present: String(root.textContent || '').trim().length > 0,
      };
    }, rootSelector);
    if (!result.evidence.mount.visible) {
      result.failures.push('module root mounted but is not visible');
      return result;
    }

    if (options.moduleId === "reports") {
      result.evidence.reports_chrome = await verifyReportsChrome(page, rootSelector, options.timeoutMs);
    }

    const action = options.createAction || await waitForPrimaryCreateAction(page, options.moduleId, options.timeoutMs);
    result.create_action = action;
    if (!action) {
      result.evidence.interaction = {
        status: 'not-declared',
        reason: 'No primary create action is required for a generic mount smoke; release/full workflow evidence is provided by tests/audit-scenarios.json.',
      };
      return result;
    }
    result.evidence.interaction = { status: 'checking', action };

    const actionSelector = `${rootSelector} [data-action="${action}"]`;
    await page.waitForSelector(actionSelector, { state: 'visible', timeout: options.timeoutMs });
    result.evidence.before = await page.evaluate(() => {
      function visible(el) {
        const box = el.getBoundingClientRect();
        const style = window.getComputedStyle(el);
        return box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
      }
      return {
        open_dialogs: document.querySelectorAll('dialog[open]').length,
        visible_forms: Array.from(document.querySelectorAll('form')).filter(visible).length,
        visible_save_submit_controls: Array.from(document.querySelectorAll('button, input[type="submit"], [data-action]'))
          .filter(visible)
          .filter((el) => {
            const action = el.getAttribute('data-action') || '';
            const text = el.textContent || el.value || '';
            return /\bsave\b|\bsubmit\b|speichern/i.test(`${action} ${text}`);
          })
          .length,
      };
    });

    const actionLocator = page.locator(actionSelector);
    await actionLocator.scrollIntoViewIfNeeded({ timeout: options.timeoutMs });
    const actionBox = await actionLocator.boundingBox({ timeout: options.timeoutMs });
    if (!actionBox) throw new Error(`primary create action ${action} has no clickable bounds`);
    await page.mouse.click(
      actionBox.x + (actionBox.width / 2),
      actionBox.y + (actionBox.height / 2),
    );
    await page.waitForTimeout(600);

    result.evidence.after = await page.evaluate(() => {
      function visible(el) {
        const box = el.getBoundingClientRect();
        const style = window.getComputedStyle(el);
        return box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
      }
      const openDialogs = Array.from(document.querySelectorAll('dialog[open]')).map((dialog) => ({
        id: dialog.id || '',
        text: dialog.textContent.trim().slice(0, 160),
      }));
      const visibleForms = Array.from(document.querySelectorAll('form')).filter(visible).map((form) => ({
        id: form.id || '',
        text: form.textContent.trim().slice(0, 160),
      }));
      const saveControls = Array.from(document.querySelectorAll('button, input[type="submit"], [data-action]'))
        .filter(visible)
        .filter((el) => {
          const action = el.getAttribute('data-action') || '';
          const text = el.textContent || el.value || '';
          return /\bsave\b|\bsubmit\b|speichern/i.test(`${action} ${text}`);
        })
        .map((el) => ({
          action: el.getAttribute('data-action') || '',
          text: (el.textContent || el.value || '').trim().slice(0, 120),
        }));
      return {
        open_dialogs: openDialogs.length,
        open_dialog_details: openDialogs,
        visible_forms: visibleForms.length,
        visible_form_details: visibleForms,
        visible_save_submit_controls: saveControls.length,
        visible_save_submit_details: saveControls,
        probe: window.__ctoxAppSmoke,
      };
    });

    const probe = result.evidence.after.probe || { docClicks: [], showModalCalls: [] };
    const clicked = Array.isArray(probe.docClicks) && probe.docClicks.some((entry) => entry.action === action);
    const openedDialog = result.evidence.after.open_dialogs > result.evidence.before.open_dialogs;
    const revealedForm = result.evidence.after.visible_forms > result.evidence.before.visible_forms;
    const revealedSave = result.evidence.after.visible_save_submit_controls > result.evidence.before.visible_save_submit_controls;
    const calledShowModal = Array.isArray(probe.showModalCalls) && probe.showModalCalls.length > 0;
    if (Array.isArray(probe.docClicks) && probe.docClicks.length > 0 && !clicked) {
      result.failures.push(`primary create action ${action} was not observed by the browser click probe`);
    }
    if (!(openedDialog || revealedForm || revealedSave || calledShowModal)) {
      result.failures.push(`primary create action ${action} did not reveal an open dialog, visible form, or save/submit control`);
    } else {
      result.evidence.interaction.status = 'passed';
    }
  } catch (error) {
    result.failures.push(String(error.stack || error.message || error).split('\n')[0]);
  } finally {
    const moduleErrors = consoleErrors.filter((message) => message.includes(options.moduleId) || message.includes('[business-os] mount failed'));
    if (moduleErrors.length > 0) {
      result.failures.push(...moduleErrors.map((message) => `browser console error: ${message}`));
    }
    if (pageErrors.length > 0) {
      result.failures.push(...pageErrors.map((message) => `browser page error: ${message}`));
    }
    result.evidence.console_errors = consoleErrors;
    result.evidence.page_errors = pageErrors;
    result.evidence.failed_requests = failedRequests;
    const pageOrigin = new URL(options.url).origin;
    const blockingFailedRequests = failedRequests.filter((request) => {
      try {
        const url = new URL(request.url);
        return url.origin === pageOrigin && ['http:', 'https:'].includes(url.protocol);
      } catch {
        return false;
      }
    });
    if (blockingFailedRequests.length > 0) {
      result.failures.push(...blockingFailedRequests.map((request) => `same-origin request failed: ${request.url} (${request.error || 'unknown error'})`));
    }
    result.ok = result.failures.length === 0;
    if (options.screenshot && !result.ok) {
      ensureParentDir(options.screenshot);
      await page.screenshot({ path: options.screenshot, fullPage: true }).catch(() => {});
      result.evidence.screenshot = options.screenshot;
    }
    await context.close().catch(() => {});
    await browser.close().catch(() => {});
  }

  return result;
}

let options;
try {
  options = parseArgs(process.argv.slice(2));
} catch (error) {
  console.error(error.message);
  console.error(usage());
  process.exit(2);
}

if (options.help) {
  console.log(usage());
  process.exit(0);
}

const result = await runSmoke(options);
if (options.output) {
  ensureParentDir(options.output);
  writeFileSync(options.output, `${JSON.stringify(result, null, 2)}\n`);
}
printResult(result, options.json);
process.exit(result.ok ? 0 : 1);
