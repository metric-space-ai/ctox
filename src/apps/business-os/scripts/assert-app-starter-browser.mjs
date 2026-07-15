#!/usr/bin/env node
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(appRoot, '../../..');
const starterRoot = join(appRoot, 'app-starter/v2');
const outputRoot = join(repoRoot, 'output/playwright/business-os-app-starter-v2');
const catalog = JSON.parse(readFileSync(join(starterRoot, 'archetypes.json'), 'utf8'));
const archetypes = Object.keys(catalog.archetypes);
const widths = [640, 960, 1180];
const themes = ['light', 'dark'];
const locales = ['de', 'en'];
const problems = [];
const results = [];

mkdirSync(outputRoot, { recursive: true });
const server = http.createServer(serve);
await new Promise((resolveListen, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', resolveListen);
});
const port = server.address().port;
const executablePath = [
  process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
  chromium.executablePath(),
  '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
].find((candidate) => candidate && existsSync(candidate));
const browser = await chromium.launch({ headless: true, executablePath });

try {
  for (const archetype of archetypes) {
    for (const width of widths) {
      for (const theme of themes) {
        for (const locale of locales) {
          const page = await browser.newPage({ viewport: { width: width + 80, height: 820 } });
          page.on('console', (message) => {
            if (message.type() === 'error' || message.type() === 'warning') {
              problems.push(`${archetype}/${width}/${theme}/${locale} console.${message.type()}: ${message.text()}`);
            }
          });
          page.on('pageerror', (error) => problems.push(`${archetype}/${width}/${theme}/${locale} pageerror: ${error.message}`));
          const url = `http://127.0.0.1:${port}/harness?archetype=${archetype}&width=${width}&theme=${theme}&locale=${locale}`;
          await page.goto(url, { waitUntil: 'networkidle' });
          await page.waitForFunction(() => globalThis.__starter?.ready === true);

          const snapshot = await page.evaluate(() => {
            const host = document.querySelector('[data-test-host]');
            const root = host.querySelector('[data-starter-root]');
            const run = root.querySelector('[data-action="run-signature"]');
            const namedControls = Array.from(root.querySelectorAll('button,input,select,textarea')).map((node) => ({
              tag: node.tagName,
              name: node.getAttribute('aria-label')
                || node.textContent?.trim()
                || node.getAttribute('placeholder')
                || (node.id ? root.querySelector(`label[for="${node.id}"]`)?.textContent?.trim() : '')
                || '',
            }));
            return {
              title: root.querySelector('[data-archetype-title]')?.textContent?.trim(),
              runLabel: run?.textContent?.trim(),
              runBackground: getComputedStyle(run).backgroundColor,
              overflowX: host.scrollWidth - host.clientWidth,
              overflowY: host.scrollHeight - host.clientHeight,
              namedControls,
            };
          });
          const definition = catalog.archetypes[archetype];
          const labels = locale === 'de' ? { ...definition, ...definition.de } : definition;
          assert.equal(snapshot.title, labels.title);
          assert.equal(snapshot.runLabel, labels.signature_action);
          assert.ok(snapshot.runBackground !== 'rgba(0, 0, 0, 0)', 'signature action must be visually prominent');
          assert.ok(snapshot.overflowX <= 1, `${archetype} overflows ${width}px by ${snapshot.overflowX}px`);
          assert.ok(snapshot.overflowY <= 1, `${archetype} overflows vertically by ${snapshot.overflowY}px`);
          assert.ok(snapshot.namedControls.every((control) => control.name), `${archetype} has an unnamed control`);

          if (width === 960 && theme === 'light' && locale === 'de') {
            await page.locator('[data-empty-state] [data-action="create-record"]').click();
            await page.locator('input[name="title"]').fill(`Browser ${archetype}`);
            await page.locator('textarea[name="notes"]').fill('Persistenter Browservertrag');
            await page.locator('[data-record-form] button[type="submit"]').click();
            const record = page.locator('[data-record-id]').first();
            await record.waitFor();
            await record.click();
            await page.locator('[data-action="toggle-status"]').click();
            const statusAfterToggle = await page.evaluate(() => globalThis.__starter.records()[0]?.status);
            assert.equal(statusAfterToggle, 'done');
            await page.locator('[data-action="run-signature"]').click();
            const behavior = await page.evaluate(() => ({
              records: globalThis.__starter.records(),
              commands: globalThis.__starter.commands,
              registrations: globalThis.__starter.registrations,
            }));
            assert.equal(behavior.records.length, 1);
            assert.equal(behavior.commands.length, 1);
            assert.equal(behavior.commands[0].payload.archetype, archetype);
            assert.equal(behavior.registrations.at(-1)?.descriptor?.entity?.collection.endsWith('_records'), true);
            assert.equal(behavior.registrations.at(-1)?.descriptor?.entity?.id, behavior.records[0].id);

            await page.waitForTimeout(100);
            await page.screenshot({ path: join(outputRoot, `${archetype}.png`) });

            await page.evaluate(() => globalThis.__starter.setOnline(false));
            await assertVisible(page, '[data-offline-state]');
            await page.evaluate(() => globalThis.__starter.setOnline(true));
            await assertHidden(page, '[data-offline-state]');

            await page.evaluate(() => { globalThis.__starter.failCommands = true; });
            await page.locator('[data-action="run-signature"]').click();
            await assertVisible(page, '[data-error-state]');
          }
          results.push({ archetype, width, theme, locale, ...snapshot });
          await page.close();
        }
      }
    }

    const denied = await browser.newPage({ viewport: { width: 1040, height: 820 } });
    await denied.goto(`http://127.0.0.1:${port}/harness?archetype=${archetype}&width=960&theme=dark&locale=en&write=0`, { waitUntil: 'networkidle' });
    await denied.waitForFunction(() => globalThis.__starter?.ready === true);
    await assertVisible(denied, '[data-permission-state]');
    await denied.locator('[data-action="request-permission"]').click();
    const delegations = await denied.evaluate(() => globalThis.__starter.contextDispatches);
    assert.equal(delegations.length, 1);
    assert.equal(delegations[0].action, 'data');
    assert.match(delegations[0].options.prompt, /approve|freigeben/i);
    await denied.close();
  }
} finally {
  await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
}

assert.deepEqual(problems, [], problems.join('\n'));
const report = {
  ok: true,
  schema: 'ctox.business_os.app_starter_browser_matrix.v1',
  cells: results.length,
  archetypes: archetypes.length,
  widths,
  themes,
  locales,
  problems,
  results,
};
writeFileSync(join(outputRoot, 'report.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(`Canonical Business OS starter browser matrix OK: ${results.length} cells, ${archetypes.length} denied/delegation flows`);

async function assertVisible(page, selector) {
  await page.locator(selector).waitFor({ state: 'visible' });
}

async function assertHidden(page, selector) {
  await page.locator(selector).waitFor({ state: 'hidden' });
}

function serve(request, response) {
  const url = new URL(request.url || '/', 'http://127.0.0.1');
  if (url.pathname === '/favicon.ico') {
    response.writeHead(204, { 'cache-control': 'no-store' });
    response.end();
    return;
  }
  if (url.pathname === '/harness') {
    const archetype = archetypes.includes(url.searchParams.get('archetype')) ? url.searchParams.get('archetype') : catalog.default;
    const width = widths.includes(Number(url.searchParams.get('width'))) ? Number(url.searchParams.get('width')) : 960;
    const theme = themes.includes(url.searchParams.get('theme')) ? url.searchParams.get('theme') : 'light';
    const locale = locales.includes(url.searchParams.get('locale')) ? url.searchParams.get('locale') : 'de';
    const canWrite = url.searchParams.get('write') !== '0';
    send(response, 200, harnessHtml({ archetype, width, theme, locale, canWrite }), 'text/html; charset=utf-8');
    return;
  }
  const fixture = url.pathname.match(/^\/fixture\/([^/]+)\/(.+)$/);
  if (fixture && archetypes.includes(fixture[1])) {
    const [_, archetype, relative] = fixture;
    const moduleId = `starter-${archetype}`;
    const collection = `${moduleId.replaceAll('-', '_')}_records`;
    if (relative === 'index.js') return send(response, 200, render('index.js.tpl', moduleId, collection), 'text/javascript; charset=utf-8');
    if (relative === 'schema.js') return send(response, 200, render('schema.js.tpl', moduleId, collection), 'text/javascript; charset=utf-8');
    if (relative === 'core/records.mjs') return send(response, 200, render('core/records.mjs.tpl', moduleId, collection), 'text/javascript; charset=utf-8');
    if (relative === 'core/archetype.mjs') {
      return send(response, 200, `export const ARCHETYPE = ${JSON.stringify({ id: archetype, ...catalog.archetypes[archetype] })};\n`, 'text/javascript; charset=utf-8');
    }
    if (relative === 'core/request.mjs') return send(response, 200, "export const REQUEST_NOTE = 'Browser contract';\n", 'text/javascript; charset=utf-8');
    const candidate = resolve(starterRoot, relative);
    if (candidate.startsWith(`${starterRoot}/`)) return serveFile(candidate, response);
  }
  if (url.pathname === '/app.css') return serveFile(join(appRoot, 'app.css'), response);
  if (url.pathname === '/shared/base.css') return serveFile(join(appRoot, 'shared/base.css'), response);
  send(response, 404, 'Not Found', 'text/plain; charset=utf-8');
}

function harnessHtml({ archetype, width, theme, locale, canWrite }) {
  return `<!doctype html>
<html lang="${locale}" data-theme="${theme}">
<head>
  <meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
  <link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
  <style>html,body{margin:0;width:100%;height:100%;overflow:hidden}body{display:grid;place-items:center;background:var(--bg)}[data-test-host]{container:business-app-window / inline-size;width:${width}px;height:740px;overflow:hidden;border:1px solid var(--line)}</style>
</head>
<body><main data-test-host></main>
<script type="module">
  const rows = new Map();
  const commands = [];
  const registrations = [];
  const contextDispatches = [];
  const notifications = [];
  let online = true;
  Object.defineProperty(navigator, 'onLine', { configurable: true, get: () => online });
  const collection = {
    find: () => ({ exec: async () => Array.from(rows.values()).map((row) => ({ toJSON: () => ({ ...row }) })) }),
    upsert: async (row) => { rows.set(row.id, { ...row }); },
    insert: async (row) => { rows.set(row.id, { ...row }); },
    $: { subscribe: () => ({ unsubscribe() {} }) }
  };
  const ctx = {
    host: document.querySelector('[data-test-host]'),
    locale: ${JSON.stringify(locale)},
    db: { collection: () => collection },
    permissions: { canWriteCollection: () => ${canWrite} },
    commandBus: { dispatch: async (command) => {
      if (globalThis.__starter.failCommands) throw new Error('Synthetic command failure');
      commands.push(command); return { status: 'accepted' };
    } },
    contextActions: {
      register: (element, descriptor) => { const item = { element, descriptor }; registrations.push(item); return () => {}; },
      dispatch: async (action, options) => { contextDispatches.push({ action, options: { ...options, target: undefined } }); return { status: 'delegated' }; }
    },
    notifications: { show: (notification) => notifications.push(notification) }
  };
  globalThis.__starter = {
    ready: false, commands, registrations, contextDispatches, notifications, failCommands: false,
    records: () => Array.from(rows.values()),
    setOnline(value) { online = Boolean(value); window.dispatchEvent(new Event(online ? 'online' : 'offline')); }
  };
  const module = await import('/fixture/${archetype}/index.js');
  globalThis.__starter.cleanup = await module.mount(ctx);
  globalThis.__starter.ready = true;
</script></body></html>`;
}

function render(relative, moduleId, collection) {
  return readFileSync(join(starterRoot, relative), 'utf8')
    .replaceAll('__MODULE_ID__', moduleId)
    .replaceAll('__COLLECTION__', collection);
}

function serveFile(path, response) {
  try {
    send(response, 200, readFileSync(path), ({
      '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
      '.css': 'text/css; charset=utf-8', '.json': 'application/json; charset=utf-8', '.svg': 'image/svg+xml',
    })[extname(path)] || 'application/octet-stream');
  } catch {
    send(response, 404, 'Not Found', 'text/plain; charset=utf-8');
  }
}

function send(response, status, body, contentType) {
  response.writeHead(status, { 'content-type': contentType, 'cache-control': 'no-store' });
  response.end(body);
}
