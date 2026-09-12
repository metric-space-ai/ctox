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
  await context.route('**/modules/notes/index.js*', () => {
    stalledModuleRequests += 1;
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
    await page.waitForFunction(() => Boolean(
      document.querySelector('[data-ctox-reporter]')
      && document.querySelector('[data-ctox-chat-root]')
    ), null, { timeout: 10_000 });
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
} finally {
  await new Promise((resolveServer) => server.close(resolveServer));
}
