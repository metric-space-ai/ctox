#!/usr/bin/env node
import assert from 'node:assert/strict';
import { existsSync, readFileSync, statSync } from 'node:fs';
import http from 'node:http';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

import {
  LAUNCH_CONTEXT_DEADLINE_MS,
  SHELL_GENERATION_PROBE_DEADLINE_MS,
} from '../shared/startup-deadlines.js';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
assert.equal(LAUNCH_CONTEXT_DEADLINE_MS, 30_000);
assert.equal(SHELL_GENERATION_PROBE_DEADLINE_MS, 5_000);

const server = http.createServer((request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost');
    if (!url.pathname.startsWith('/business-os/')) return send(response, 404, 'Not Found', 'text/plain');
    const relative = normalize(url.pathname.slice('/business-os/'.length)).replace(/^(\.\.[/\\])+/, '');
    const candidate = join(appRoot, relative);
    if (!candidate.startsWith(`${appRoot}/`) || !statSync(candidate, { throwIfNoEntry: false })?.isFile()) {
      return send(response, 404, 'Not Found', 'text/plain');
    }
    send(response, 200, readFileSync(candidate), contentType(candidate));
  } catch {
    send(response, 404, 'Not Found', 'text/plain');
  }
});
await new Promise((resolveListen, rejectListen) => {
  server.once('error', rejectListen);
  server.listen(0, '127.0.0.1', resolveListen);
});

function launchChromium() {
  const executablePath = [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
    chromium.executablePath(),
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
  ].find((candidate) => candidate && existsSync(candidate));
  return chromium.launch({ headless: true, executablePath });
}

async function assertLaunchContextDeadline() {
  const browser = await launchChromium();
  const context = await browser.newContext();
  let launchRequests = 0;

  await context.route('**/api/business-os/launch-context', async (route) => {
    launchRequests += 1;
    if (launchRequests === 1) {
      // The shell deadline, accelerated for the fixture, must cancel this request.
      return new Promise(() => {});
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        session: { ok: true, authenticated: false, auth_required: true, reason: 'pairing_config_missing' },
        config: null,
        designTemplates: [],
      }),
    });
  });
  await context.addInitScript((productionDeadline) => {
    const originalSetTimeout = globalThis.setTimeout.bind(globalThis);
    globalThis.setTimeout = (callback, delay, ...args) => originalSetTimeout(
      callback,
      delay === productionDeadline ? 100 : delay,
      ...args,
    );
  }, LAUNCH_CONTEXT_DEADLINE_MS);

  try {
    const page = await context.newPage();
    const pageErrors = [];
    page.on('pageerror', (error) => pageErrors.push(error.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/business-os/index.html`);
    await page.waitForSelector('#startup-error-card:not([hidden])', { state: 'visible' });

    const failed = await page.evaluate(() => ({
      title: document.querySelector('.friendly-error-title')?.textContent?.trim(),
      details: document.getElementById('startup-error-msg')?.textContent?.trim(),
      retryVisible: !document.getElementById('startup-retry-btn')?.closest('[hidden]'),
      retryText: document.getElementById('startup-retry-btn')?.textContent?.trim(),
    }));
    assert.equal(failed.title, 'Netzwerk-Zeitüberschreitung beim Start');
    assert.match(failed.details, /Business OS launch context timed out after 30 seconds/);
    assert.equal(failed.retryVisible, true);
    assert.match(failed.retryText, /Erneut versuchen/);

    await page.click('#startup-retry-btn');
    await page.waitForSelector('html[data-auth-state="locked"]');
    // The unauthenticated/logout gate must not mount either companion.
    const lockedCompanions = await page.evaluate(() => ({
      reporter: Boolean(document.querySelector('[data-ctox-reporter]')),
      chat: Boolean(document.querySelector('[data-ctox-chat-root]')),
    }));
    assert.deepEqual(lockedCompanions, { reporter: false, chat: false });
    assert.ok(launchRequests >= 2, 'Retry must issue a new launch-context request');
    assert.deepEqual(pageErrors, []);
  } finally {
    await context.close();
    await browser.close();
  }
}

async function assertCompanionsStartDuringStalledModuleLaunch() {
  const browser = await launchChromium();
  const context = await browser.newContext();
  let stalledModuleRequests = 0;
  let resolveStalledModule;
  const stalledModule = new Promise((resolveRequest) => {
    resolveStalledModule = resolveRequest;
  });

  await context.route('**/api/business-os/launch-context', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({
      session: {
        ok: true,
        authenticated: true,
        auth_required: false,
        source: 'fixture',
        user: { id: 'fixture-user', display_name: 'Fixture User', role: 'admin' },
        reason: null,
      },
      config: {
        ok: true,
        app_hosting: 'local',
        sync_mode: 'p2p-first',
        instance_id: 'startup-companions-fixture',
        peer_id: 'browser-fixture',
        peer_role: 'browser',
        sync_room: 'ctox-business-os:startup-companions-fixture:fixture',
        signaling_urls: ['ws://127.0.0.1:9/ctox-business-os'],
        ice_servers: [],
      },
      designTemplates: [],
    }),
  }));
  await context.route(/^http:\/\/127\.0\.0\.1:\d+\/api\/(?!business-os\/launch-context)/, (route) => route.abort());
  await context.route(/^ws:\/\//, (route) => route.abort());
  await context.route('**/modules/notes/index.js*', (route) => {
    // Catalog revision hashing fetches the same asset before module launch.
    // Hold only its executable import so the fixture reaches that boundary.
    if (route.request().resourceType() !== 'script') return route.continue();
    stalledModuleRequests += 1;
    resolveStalledModule();
    return new Promise(() => {});
  });

  const page = await context.newPage();
  const pageErrors = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  const consoleErrors = [];
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type()) && consoleErrors.length < 40) consoleErrors.push(message.text());
  });

  try {
    await page.goto(`http://127.0.0.1:${server.address().port}/business-os/index.html#notes`);
    let stalledModuleTimer;
    try {
      await Promise.all([
        page.waitForFunction(() => Boolean(
          document.querySelector('[data-ctox-reporter]')
          && document.querySelector('[data-ctox-chat-root]')
        ), null, { timeout: 10_000 }),
        new Promise((resolveRequest, rejectRequest) => {
          stalledModuleTimer = setTimeout(() => rejectRequest(new Error('Selected module import was not reached')), 10_000);
          stalledModule.then(resolveRequest);
        }),
      ]);
    } finally {
      clearTimeout(stalledModuleTimer);
    }
    assert.ok(stalledModuleRequests >= 1, 'fixture must actually hold the selected module launch');
    const companions = await page.evaluate(() => ({
      reporter: Boolean(document.querySelector('[data-ctox-reporter]')),
      chat: Boolean(document.querySelector('[data-ctox-chat-root]')),
    }));
    assert.deepEqual(companions, { reporter: true, chat: true });
    assert.deepEqual(pageErrors, []);
  } catch (error) {
    const state = await page.evaluate(() => ({
      authState: document.documentElement.dataset.authState,
      startupStatus: document.getElementById('startup-status-text')?.textContent,
      resources: performance.getEntriesByType('resource').map((entry) => new URL(entry.name).pathname).filter((path) => /business-(chat|reporter)|notes|window-manager/.test(path)),
      startupError: document.getElementById('startup-error-msg')?.textContent?.trim(),
      reporter: Boolean(document.querySelector('[data-ctox-reporter]')),
      chat: Boolean(document.querySelector('[data-ctox-chat-root]')),
    }));
    console.error(JSON.stringify({ stalledModuleRequests, pageErrors, consoleErrors, state }));
    throw error;
  } finally {
    await context.close();
    await browser.close();
  }
}

async function assertWarmSecondOpenUsesPersistedCatalog() {
  const browser = await launchChromium();
  const context = await browser.newContext();
  const session = {
    ok: true,
    authenticated: true,
    auth_required: false,
    source: 'fixture',
    user: { id: 'warm-catalog-user', display_name: 'Warm Catalog User', role: 'admin' },
    reason: null,
  };
  const config = {
    ok: true,
    app_hosting: 'local',
    sync_mode: 'p2p-first',
    instance_id: 'warm-catalog-fixture',
    peer_id: 'browser-warm-catalog',
    peer_role: 'browser',
    sync_room: 'ctox-business-os:warm-catalog-fixture:fixture',
    signaling_urls: ['ws://127.0.0.1:9/ctox-business-os'],
    ice_servers: [],
  };
  await context.route('**/api/business-os/launch-context', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ session, config, designTemplates: [] }),
  }));
  await context.route(/^http:\/\/127\.0\.0\.1:\d+\/api\/(?!business-os\/launch-context)/, (route) => route.abort());
  await context.route(/^ws:\/\//, (route) => route.abort());
  let slowPackagedRequests = 0;
  let slowPackaged = false;
  await context.route(/\/(?:system-apps\.json|modules\/registry\.json)\?v=/, async (route) => {
    if (!slowPackaged) return route.continue();
    slowPackagedRequests += 1;
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 7000));
    return route.abort();
  });

  try {
    const first = await context.newPage();
    await first.goto(`http://127.0.0.1:${server.address().port}/business-os/index.html`);
    await first.waitForFunction(() => Boolean(
      window.CTOX_BUSINESS_OS_APP?.db?.collection?.('business_module_catalog')
    ), null, { timeout: 10_000 });
    await first.evaluate(async () => {
      const collection = window.CTOX_BUSINESS_OS_APP.db.collection('business_module_catalog');
      await collection.upsert({
        id: 'module-catalog',
        ok: true,
        modules: [{
          id: 'desktop',
          title: 'Desktop',
          entry: 'modules/desktop/index.html',
          collections: ['business_commands', 'desktop_icons', 'desktop_layout'],
          core: true,
          install_scope: 'core',
        }],
        templates: [],
        governance: {},
        updated_at_ms: Date.now(),
      });
    });
    await first.close();

    slowPackaged = true;
    const second = await context.newPage();
    const pageErrors = [];
    second.on('pageerror', (error) => pageErrors.push(error.message));
    const startedAt = Date.now();
    await second.goto(`http://127.0.0.1:${server.address().port}/business-os/index.html`);
    await second.waitForSelector('[data-desktop-root]', { state: 'visible', timeout: 5000 });
    const firstPaintMs = Date.now() - startedAt;
    const persistedCatalog = await second.evaluate(async () => {
      const doc = await window.CTOX_BUSINESS_OS_APP.db
        .collection('business_module_catalog').findOne('module-catalog').exec();
      return doc?.toJSON?.() || null;
    });
    assert.ok(firstPaintMs < 5000, `cached Desktop first paint took ${firstPaintMs}ms`);
    assert.equal(persistedCatalog?.modules?.[0]?.id, 'desktop');
    assert.equal(slowPackagedRequests, 0, 'warm catalog first paint must not request packaged manifest');
    assert.deepEqual(pageErrors, []);
    console.log(`warm-second-open firstPaintMs=${firstPaintMs} cachedModules=${persistedCatalog.modules.length} slowPackagedRequests=${slowPackagedRequests}`);
    await second.close();
  } finally {
    await context.close();
    await browser.close();
  }
}
function contentType(path) {
  return ({
    '.css': 'text/css; charset=utf-8',
    '.html': 'text/html; charset=utf-8',
    '.js': 'text/javascript; charset=utf-8',
    '.mjs': 'text/javascript; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.svg': 'image/svg+xml',
  })[extname(path)] || 'application/octet-stream';
}

function send(response, status, body, type) {
  response.writeHead(status, { 'content-type': type, 'cache-control': 'no-store' });
  response.end(body);
}

try {
  await assertLaunchContextDeadline();
  await assertCompanionsStartDuringStalledModuleLaunch();
  await assertWarmSecondOpenUsesPersistedCatalog();
} finally {
  await new Promise((resolveServer) => server.close(resolveServer));
}
