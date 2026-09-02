#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  loadAppAuditScenarios,
  renderScenarioValue,
  scenarioTargetSelector,
  selectAppAuditScenarios,
} from './app-audit-scenarios.mjs';

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
    'Usage: node src/apps/business-os/scripts/e2e-app-module.mjs <module-id> [--installed|--source] [--url <business-os-url>] [--json] [--timeout-ms <n>] [--output <path>] [--screenshot <path>] [--marker <value>] [--profile quick|release|full] [--scenario <id>] [--require-scenario]',
    '',
    'Runs a real-browser save/reload/command-bus E2E against a runtime-installed CTOX Business OS app.',
  ].join('\n');
}

function parseArgs(argv) {
  const options = {
    moduleId: null,
    url: 'http://127.0.0.1:8765',
    json: false,
    timeoutMs: 120000,
    output: null,
    screenshot: null,
    marker: null,
    mode: 'installed',
    scenarioId: null,
    requireScenario: false,
    profile: 'release',
  };
  for (let idx = 0; idx < argv.length; idx += 1) {
    const arg = argv[idx];
    if (arg === '--url') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--url requires a value');
      options.url = value;
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
    } else if (arg === '--marker') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--marker requires a value');
      options.marker = value;
      idx += 1;
    } else if (arg === '--scenario') {
      const value = argv[idx + 1];
      if (!value) throw new Error('--scenario requires a value');
      options.scenarioId = value;
      idx += 1;
    } else if (arg === '--profile') {
      const value = argv[idx + 1];
      if (!['quick', 'release', 'full'].includes(value)) throw new Error('--profile must be quick, release, or full');
      options.profile = value;
      idx += 1;
    } else if (arg === '--require-scenario') {
      options.requireScenario = true;
    } else if (arg === '--json') {
      options.json = true;
    } else if (arg === '--installed' || arg === '--source') {
      options.mode = arg.slice(2);
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
  if (!options.marker) {
    options.marker = `CTOX_E2E_${options.moduleId}_${Date.now()}`;
  }
  return options;
}

function withModuleHash(baseUrl, moduleId) {
  const url = new URL(baseUrl);
  url.searchParams.set('rxdbSmoke', '1');
  url.hash = moduleId;
  return url.href;
}

function withoutModuleHash(baseUrl) {
  const url = new URL(baseUrl);
  url.searchParams.set('rxdbSmoke', '1');
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
    console.log(`Business OS app E2E OK: ${result.module_id}`);
  } else {
    console.error(`Business OS app E2E failed for ${result.module_id}:`);
    for (const failure of result.failures) console.error(`- ${failure}`);
  }
}

function moduleDir(moduleId, mode = 'installed') {
  return mode === 'source'
    ? join(RELEASE_ROOT, 'src/apps/business-os/modules', moduleId)
    : join(RELEASE_ROOT, 'runtime/business-os/installed-modules', moduleId);
}

function readModuleCollections(moduleId, mode) {
  const path = join(moduleDir(moduleId, mode), 'collections.schema.json');
  if (!existsSync(path)) return [];
  const parsed = JSON.parse(readFileSync(path, 'utf8'));
  return Object.entries(parsed.collections || {}).map(([name, schema]) => ({
    name,
    version: Number(schema?.version || 0),
  }));
}

function quoteSqlString(value) {
  return `'${String(value).replace(/'/g, "''")}'`;
}

function quoteSqlIdentifier(value) {
  return `"${String(value).replace(/"/g, '""')}"`;
}

function tableName(collection) {
  return `ctox_business_os__${collection.name}__v${collection.version}`;
}

function sqliteCountLike(sqlitePath, table, marker) {
  const sql = [
    'PRAGMA busy_timeout=10000;',
    `SELECT COUNT(*) FROM ${quoteSqlIdentifier(table)} WHERE deleted=0 AND data LIKE '%' || ${quoteSqlString(marker)} || '%';`,
  ].join('\n');
  const result = spawnSync('sqlite3', [sqlitePath], {
    input: sql,
    encoding: 'utf8',
    timeout: 15000,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error((result.stderr || result.stdout || `sqlite3 exited ${result.status}`).trim());
  }
  const lines = String(result.stdout || '').trim().split(/\r?\n/).filter(Boolean);
  const last = lines[lines.length - 1] || '0';
  const count = Number(last);
  if (!Number.isFinite(count)) throw new Error(`could not parse sqlite count from ${JSON.stringify(result.stdout)}`);
  return count;
}

function sqliteTableExists(sqlitePath, table) {
  const sql = [
    'PRAGMA busy_timeout=10000;',
    `SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ${quoteSqlString(table)};`,
  ].join('\n');
  const result = spawnSync('sqlite3', [sqlitePath], {
    input: sql,
    encoding: 'utf8',
    timeout: 15000,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error((result.stderr || result.stdout || `sqlite3 exited ${result.status}`).trim());
  }
  const count = Number(String(result.stdout || '').trim().split(/\r?\n/).filter(Boolean).pop() || '0');
  return Number.isFinite(count) && count > 0;
}

async function pollUntil(fn, timeoutMs, intervalMs = 500) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() <= deadline) {
    try {
      const value = await fn();
      if (value) return value;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  if (lastError) throw lastError;
  return null;
}

async function openModule(page, moduleId, url, timeoutMs) {
  const rootSelector = `[data-module-root="${moduleId}"]`;
  await page.goto(withoutModuleHash(url), { waitUntil: 'domcontentloaded', timeout: timeoutMs });
  await page.waitForFunction((id) => {
    const app = window.CTOX_BUSINESS_OS_APP;
    return typeof app?.openModule === 'function'
      && Boolean(app.modules?.find?.((module) => module.id === id));
  }, moduleId, { timeout: timeoutMs, polling: 250 });
  await page.evaluate(async (id) => {
    const app = window.CTOX_BUSINESS_OS_APP;
    const target = new URL(location.href);
    target.hash = id;
    history.replaceState(history.state, '', target.href);
    await app.openModule(id, { force: true });
  }, moduleId);
  await page.waitForFunction((selector) => {
    const root = document.querySelector(selector);
    return root?.dataset.moduleReady === 'true';
  }, rootSelector, { timeout: timeoutMs, polling: 100 });
  const mount = await page.evaluate(({ id, selector }) => {
    const root = document.querySelector(selector);
    return {
      failed: root?.dataset.moduleLoadFailed === 'true',
      failure: window.CTOX_BUSINESS_OS_APP?.qaModuleMountFailures?.[id] || null,
      recovery_text: root?.querySelector('.shell-app-recovery')?.textContent?.trim() || '',
    };
  }, { id: moduleId, selector: rootSelector });
  if (mount.failed) {
    throw new Error(`module mount failed: ${mount.failure?.message || mount.recovery_text || moduleId}`);
  }
  await page.waitForSelector(rootSelector, { state: 'visible', timeout: timeoutMs });
  return rootSelector;
}

async function findPrimaryCreateAction(page, rootSelector) {
  return page.evaluate((selector) => {
    function visible(el) {
      const box = el.getBoundingClientRect();
      const style = window.getComputedStyle(el);
      return box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none' && !el.disabled;
    }
    function isPrimaryCreateAction(value) {
      const action = String(value || '').trim().toLowerCase();
      if (!action) return false;
      if (/(^|[-_:])(follow-?up|review|save|submit|cancel|close|edit|archive|delete|remove)([-_:]|$)/.test(action)) return false;
      return /^(add|new|create)([-_:]|$)/.test(action);
    }
    const root = document.querySelector(selector);
    if (!root) return null;
    const actions = Array.from(root.querySelectorAll('[data-action]'))
      .map((el) => ({
        action: el.getAttribute('data-action'),
        text: el.textContent.trim(),
        visible: visible(el),
      }))
      .filter((item) => item.visible && isPrimaryCreateAction(item.action));
    return actions[0]?.action || null;
  }, rootSelector);
}

async function waitForPrimaryCreateAction(page, rootSelector, timeoutMs) {
  return pollUntil(
    () => findPrimaryCreateAction(page, rootSelector),
    timeoutMs,
    500,
  );
}

async function collectBrowserDiagnostics(page, moduleId, rootSelector) {
  try {
    return await page.evaluate(({ moduleId, rootSelector }) => {
      function visible(el) {
        const box = el.getBoundingClientRect();
        const style = window.getComputedStyle(el);
        return box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
      }
      const app = window.CTOX_BUSINESS_OS_APP || null;
      const roots = Array.from(document.querySelectorAll('[data-module-root]')).map((el) => ({
        module_root: el.getAttribute('data-module-root') || '',
        visible: visible(el),
        text_length: String(el.innerText || '').length,
      }));
      const actions = Array.from(document.querySelectorAll('[data-action]')).map((el) => ({
        action: el.getAttribute('data-action') || '',
        text_length: String(el.textContent || el.value || '').trim().length,
        visible: visible(el),
        disabled: Boolean(el.disabled),
        module_root: el.closest('[data-module-root]')?.getAttribute('data-module-root') || '',
      }));
      return {
        hash: location.hash,
        active_module: document.body.dataset.activeModule || '',
        loading_module: document.body.dataset.moduleLoading || '',
        has_shell_state: Boolean(app),
        shell_module_count: Array.isArray(app?.modules) ? app.modules.length : null,
        requested_module_in_shell: Boolean(app?.modules?.some?.((mod) => mod?.id === moduleId)),
        shell_module_ids: Array.isArray(app?.modules) ? app.modules.map((mod) => mod?.id).filter(Boolean).slice(0, 80) : [],
        root_selector: rootSelector,
        root_present: Boolean(document.querySelector(rootSelector)),
        roots,
        actions: actions.slice(0, 80),
        body_text_length: String(document.body.innerText || '').length,
      };
    }, { moduleId, rootSelector });
  } catch (error) {
    return { error: String(error?.message || error) };
  }
}

async function fillVisibleForm(page, rootSelector, marker) {
  return page.evaluate(({ selector, marker }) => {
    function visible(el) {
      const box = el.getBoundingClientRect();
      const style = window.getComputedStyle(el);
      return box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none' && !el.disabled;
    }
    function labelFor(el) {
      const name = el.getAttribute('name') || el.getAttribute('data-field') || el.getAttribute('aria-label') || el.placeholder || '';
      const label = el.closest('label')?.textContent?.trim() || '';
      return String(name || label || 'field').replace(/\s+/g, ' ').slice(0, 48);
    }
    function textValue(el) {
      const name = labelFor(el);
      const type = String(el.getAttribute('type') || '').toLowerCase();
      if (type === 'email') return `${marker.toLowerCase().replace(/[^a-z0-9]+/g, '-')}@example.test`;
      if (type === 'tel') return '+491234567890';
      if (name.toLowerCase().includes('sku')) return `SKU-${marker.slice(-10)}`;
      return `${marker} ${name}`.slice(0, Math.max(16, Number(el.maxLength) > 0 ? Number(el.maxLength) : 180));
    }
    function numberValue(el) {
      const name = labelFor(el).toLowerCase();
      if (name.includes('mrr') || name.includes('amount') || name.includes('budget') || name.includes('cost') || name.includes('rate') || name.includes('value')) return '12000';
      if (name.includes('min') || name.includes('stock') || name.includes('quantity') || name.includes('qty') || name.includes('reorder')) return '100';
      if (name.includes('hour')) return '40';
      return '10';
    }
    function isoDateFromToday(days) {
      const date = new Date();
      date.setDate(date.getDate() + days);
      return date.toISOString().slice(0, 10);
    }
    function dateValue(el) {
      const name = labelFor(el).toLowerCase();
      if (/due|deadline|expiry|expires|renewal|review|audit/.test(name)) return isoDateFromToday(-30);
      return isoDateFromToday(180);
    }
    function selectValue(el) {
      const name = labelFor(el).toLowerCase();
      const options = Array.from(el.options || []).filter((option) => !option.disabled);
      if (!options.length) return '';
      if (/\b(status|state)\b/.test(name)) {
        const preferred = [
          /checked[_ -]?out/i,
          /overdue/i,
          /active/i,
          /open/i,
          /pending/i,
          /in[_ -]?progress/i,
          /at[_ -]?risk/i,
        ];
        for (const pattern of preferred) {
          const candidate = options.find((option) => pattern.test(`${option.value} ${option.textContent || ''}`));
          if (candidate?.value) return candidate.value;
        }
      }
      return options.find((option) => option.value)?.value || options[0].value;
    }
    function setValue(el, value) {
      el.value = value;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    }
    const root = document.querySelector(selector);
    if (!root) return { filled: [], visible_forms: 0 };
    const forms = Array.from(root.querySelectorAll('form')).filter(visible);
    const fieldScopes = forms.length ? forms : [root];
    const filled = [];
    const fields = Array.from(new Set(fieldScopes.flatMap((scope) => Array.from(scope.querySelectorAll('input, textarea, select')))));
    for (const el of fields) {
      if (!visible(el)) continue;
      const tag = el.tagName.toLowerCase();
      const type = String(el.getAttribute('type') || '').toLowerCase();
      if (['hidden', 'button', 'submit', 'reset', 'file', 'image'].includes(type)) continue;
      if (tag === 'select') {
        const value = selectValue(el);
        if (value) setValue(el, value);
        filled.push({ kind: 'select', name: labelFor(el), value: el.value });
      } else if (type === 'checkbox') {
        el.checked = true;
        el.dispatchEvent(new Event('change', { bubbles: true }));
        filled.push({ kind: 'checkbox', name: labelFor(el), value: true });
      } else if (type === 'radio') {
        if (!el.checked) {
          el.checked = true;
          el.dispatchEvent(new Event('change', { bubbles: true }));
        }
        filled.push({ kind: 'radio', name: labelFor(el), value: el.value });
      } else if (type === 'date') {
        setValue(el, dateValue(el));
        filled.push({ kind: 'date', name: labelFor(el), value: el.value });
      } else if (type === 'number' || type === 'range') {
        setValue(el, numberValue(el));
        filled.push({ kind: 'number', name: labelFor(el), value: el.value });
      } else if (tag === 'textarea') {
        setValue(el, `${marker} notes`);
        filled.push({ kind: 'textarea', name: labelFor(el), value: el.value });
      } else {
        setValue(el, textValue(el));
        filled.push({ kind: 'text', name: labelFor(el), value: el.value });
      }
    }
    return { filled, visible_forms: forms.length };
  }, { selector: rootSelector, marker });
}

async function clickSave(page, rootSelector) {
  return page.evaluate((selector) => {
    function visible(el) {
      const box = el.getBoundingClientRect();
      const style = window.getComputedStyle(el);
      return box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none' && !el.disabled;
    }
    const root = document.querySelector(selector);
    if (!root) return null;
    const controls = Array.from(root.querySelectorAll('button, input[type="submit"], [data-action]'))
      .filter(visible)
      .filter((el) => {
        const action = el.getAttribute('data-action') || '';
        const text = el.textContent || el.value || '';
        return /\bsave\b|\bsubmit\b|speichern/i.test(`${action} ${text}`);
      });
    const control = controls[0];
    if (!control) return null;
    const info = {
      action: control.getAttribute('data-action') || '',
      text: (control.textContent || control.value || '').trim().slice(0, 120),
    };
    control.click();
    return info;
  }, rootSelector);
}

async function markerVisible(page, rootSelector, marker, timeoutMs) {
  await page.waitForFunction(({ selector, marker }) => {
    const root = document.querySelector(selector);
    return Boolean(root && root.innerText.includes(marker));
  }, { selector: rootSelector, marker }, { timeout: timeoutMs });
  return true;
}

async function clickAutomation(page, rootSelector, marker) {
  return page.evaluate(({ selector, marker }) => {
    function visible(el) {
      const box = el.getBoundingClientRect();
      const style = window.getComputedStyle(el);
      return box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none' && !el.disabled;
    }
    function automationControls(scope, scopeScore) {
      return Array.from(scope.querySelectorAll('[data-action], button'))
        .filter(visible)
        .map((el, index) => {
          const action = String(el.getAttribute('data-action') || '').toLowerCase();
          const text = String(el.textContent || '').toLowerCase();
          if (/(^|[-_:])(create|new|add|save|submit|cancel|close|edit|delete|remove|copy)([-_:]|$)/.test(action)) return null;
          if (!(action.includes('followup') || action.includes('follow-up') || action.includes('review') || text.includes('follow-up') || text.includes('followup'))) return null;
          const score = scopeScore
            + (action.includes('batch') || text.includes('batch') ? 5 : 0)
            + (action.includes('followup') || action.includes('follow-up') || text.includes('follow-up') || text.includes('followup') ? -3 : 0)
            + (action.includes('review') || text.includes('review') ? -1 : 0)
            + index / 1000;
          return { el, score };
        })
        .filter(Boolean);
    }
    const root = document.querySelector(selector);
    if (!root) return null;
    const scopes = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      if (!String(walker.currentNode.nodeValue || '').includes(marker)) continue;
      let scope = walker.currentNode.parentElement;
      while (scope && scope !== root) {
        if (visible(scope) && automationControls(scope, 0).length > 0) break;
        scope = scope.parentElement;
      }
      if (scope && scope !== root && !scopes.includes(scope)) scopes.push(scope);
    }
    const controls = [
      ...scopes.flatMap((scope, index) => automationControls(scope, index)),
      ...automationControls(root, 10),
    ].sort((a, b) => a.score - b.score);
    const control = controls[0]?.el;
    if (!control) return null;
    const info = {
      action: control.getAttribute('data-action') || '',
      text: (control.textContent || '').trim().slice(0, 120),
      scope: scopes.some((scope) => scope.contains(control)) ? 'record' : 'module',
    };
    control.click();
    return info;
  }, { selector: rootSelector, marker });
}

async function runDeclaredScenario(page, options, scenario) {
  let rootSelector = `[data-module-root="${options.moduleId}"]`;
  const evidence = [];
  for (let index = 0; index < scenario.steps.length; index += 1) {
    const step = scenario.steps[index];
    const selector = step.target ? `${rootSelector} ${scenarioTargetSelector(step.target)}` : null;
    if (step.op === 'click') {
      await page.locator(selector).click({ timeout: step.timeout_ms });
    } else if (step.op === 'fill') {
      const value = renderScenarioValue(step.value, options.marker);
      const locator = page.locator(selector);
      await locator.waitFor({ state: 'visible', timeout: step.timeout_ms });
      const tagName = await locator.evaluate((element) => element.tagName.toLowerCase());
      if (tagName === 'select') await locator.selectOption(value);
      else await locator.fill(value);
    } else if (step.op === 'assert_visible') {
      await page.locator(selector).waitFor({ state: 'visible', timeout: step.timeout_ms });
    } else if (step.op === 'assert_text') {
      const contains = renderScenarioValue(step.contains, options.marker);
      await page.waitForFunction(({ rootSelector: root, targetSelector, expected }) => {
        const moduleRoot = document.querySelector(root);
        const target = moduleRoot?.querySelector(targetSelector);
        return Boolean(target && String(target.textContent || '').includes(expected));
      }, {
        rootSelector,
        targetSelector: scenarioTargetSelector(step.target),
        expected: contains,
      }, { timeout: step.timeout_ms, polling: 250 });
    } else if (step.op === 'reload') {
      await page.reload({ waitUntil: 'domcontentloaded', timeout: step.timeout_ms });
      rootSelector = await openModule(page, options.moduleId, options.url, step.timeout_ms);
    }
    evidence.push({
      index,
      op: step.op,
      target: step.target || null,
      status: 'passed',
    });
  }
  return evidence;
}

async function runE2e(options) {
  const result = {
    ok: false,
    module_id: options.moduleId,
    url: withModuleHash(options.url, options.moduleId),
    marker: options.marker,
    failures: [],
    evidence: {},
  };

  const collections = readModuleCollections(options.moduleId, options.mode);
  result.evidence.collections = collections;
  result.evidence.browser_runtime = browserRuntime.evidence;
  const sqlitePath = join(RELEASE_ROOT, 'runtime/business-os-rxdb.sqlite3');
  result.evidence.native_db = sqlitePath;

  if (collections.length > 0) {
    if (!existsSync(sqlitePath)) {
      result.failures.push(`native RxDB SQLite database not found at ${sqlitePath}`);
      return result;
    }
    const ready = await pollUntil(() => {
      const missing = collections
        .map((collection) => tableName(collection))
        .filter((table) => !sqliteTableExists(sqlitePath, table));
      return missing.length === 0
        ? { ready: true, tables: collections.map((collection) => tableName(collection)) }
        : null;
    }, Math.min(options.timeoutMs, 45000), 1000);
    if (!ready) {
      result.evidence.native_schema_ready = false;
      result.failures.push('native RxDB SQLite module collection tables were not ready before browser E2E');
      return result;
    }
    result.evidence.native_schema_ready = ready;
  }

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
    if (message.type() === 'error') consoleErrors.push(message.text().slice(0, 1200));
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

  try {
    let rootSelector = await openModule(page, options.moduleId, options.url, options.timeoutMs);
    const loadedScenarios = loadAppAuditScenarios(moduleDir(options.moduleId, options.mode));
    const selectedScenarios = selectAppAuditScenarios(
      loadedScenarios.scenarios,
      options.profile,
      options.scenarioId,
    );
    result.evidence.scenario_contract = {
      path: loadedScenarios.path,
      present: loadedScenarios.document != null,
      profile: options.profile,
      selected: selectedScenarios.map((scenario) => scenario.id),
    };
    if (options.scenarioId && selectedScenarios.length === 0) {
      result.failures.push(`declared audit scenario not found: ${options.scenarioId}`);
      return result;
    }
    if (selectedScenarios.length) {
      result.evidence.scenarios = [];
      for (const scenario of selectedScenarios) {
        await openModule(page, options.moduleId, options.url, options.timeoutMs);
        const scenarioOptions = { ...options, marker: `${options.marker}_${scenario.id}` };
        const steps = await runDeclaredScenario(page, scenarioOptions, scenario);
        result.evidence.scenarios.push({ id: scenario.id, marker: scenarioOptions.marker, steps, status: 'passed' });
      }
      result.evidence.scenario_status = 'passed';
      return result;
    }
    if (options.requireScenario) {
      result.failures.push(`${options.profile} app E2E requires at least one matching scenario in tests/audit-scenarios.json`);
      return result;
    }
    result.evidence.scenario_status = 'legacy-heuristic-fallback';
    const action = await waitForPrimaryCreateAction(page, rootSelector, options.timeoutMs);
    result.evidence.create_action = action;
    if (!action) {
      result.evidence.browser_diagnostics = await collectBrowserDiagnostics(page, options.moduleId, rootSelector);
      result.failures.push('no visible primary create action found under module root');
      return result;
    }
    await page.locator(`${rootSelector} [data-action="${action}"]`).click({ timeout: options.timeoutMs });
    await page.waitForTimeout(400);

    result.evidence.form = await fillVisibleForm(page, rootSelector, options.marker);
    if (!result.evidence.form?.filled?.length) {
      result.failures.push('no visible form fields were filled after create action');
      return result;
    }

    result.evidence.save_control = await clickSave(page, rootSelector);
    if (!result.evidence.save_control) {
      result.failures.push('no visible save/submit control found after filling form');
      return result;
    }
    await markerVisible(page, rootSelector, options.marker, Math.min(options.timeoutMs, 20000));
    result.evidence.marker_visible_after_save = true;

    await page.reload({ waitUntil: 'domcontentloaded', timeout: options.timeoutMs });
    rootSelector = await openModule(page, options.moduleId, options.url, options.timeoutMs);
    await markerVisible(page, rootSelector, options.marker, Math.min(options.timeoutMs, 30000));
    result.evidence.marker_visible_after_reload = true;

    if (!existsSync(sqlitePath)) {
      result.failures.push(`native RxDB SQLite database not found at ${sqlitePath}`);
      return result;
    }
    const nativeRecord = await pollUntil(() => {
      for (const collection of collections) {
        const count = sqliteCountLike(sqlitePath, tableName(collection), options.marker);
        if (count > 0) return { collection: collection.name, table: tableName(collection), count };
      }
      return null;
    }, Math.min(options.timeoutMs, 30000), 1000);
    if (!nativeRecord) {
      result.failures.push('saved marker did not appear in native RxDB SQLite module collections');
      return result;
    }
    result.evidence.native_record = nativeRecord;

    result.evidence.automation_action = await clickAutomation(page, rootSelector, options.marker);
    if (!result.evidence.automation_action) {
      result.evidence.browser_diagnostics = await collectBrowserDiagnostics(page, options.moduleId, rootSelector);
      result.failures.push('no visible follow-up/review automation action found');
      return result;
    }
    const commandRecord = await pollUntil(() => {
      const table = 'ctox_business_os__business_commands__v1';
      const count = sqliteCountLike(sqlitePath, table, options.marker);
      if (count > 0) return { table, count };
      return null;
    }, Math.min(options.timeoutMs, 30000), 1000);
    if (!commandRecord) {
      result.evidence.browser_diagnostics = await collectBrowserDiagnostics(page, options.moduleId, rootSelector);
      result.failures.push('automation marker did not appear in native business_commands');
      return result;
    }
    result.evidence.native_command = commandRecord;
  } catch (error) {
    result.evidence.browser_diagnostics = await collectBrowserDiagnostics(
      page,
      options.moduleId,
      `[data-module-root="${options.moduleId}"]`,
    );
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

const result = await runE2e(options);
if (options.output) {
  ensureParentDir(options.output);
  writeFileSync(options.output, `${JSON.stringify(result, null, 2)}\n`);
}
printResult(result, options.json);
process.exit(result.ok ? 0 : 1);
