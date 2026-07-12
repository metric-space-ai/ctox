import http from 'node:http';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const playwrightModule = process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href
  : '../../node_modules/playwright/index.mjs';
const { chromium } = await import(playwrightModule);

const testDir = dirname(fileURLToPath(import.meta.url));
const bundle = readFileSync(resolve(testDir, '../dist/ctox-rxdb-js.mjs'));
const server = http.createServer((request, response) => {
  if (request.url === '/bundle.mjs') {
    response.writeHead(200, { 'content-type': 'text/javascript' });
    response.end(bundle);
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html' });
  response.end('<!doctype html><title>multi tab browser smoke</title>');
});
await new Promise((resolveReady) => server.listen(0, '127.0.0.1', resolveReady));
const { port } = server.address();
const systemChrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser = await chromium.launch({
  headless: true,
  ...(existsSync(systemChrome) ? { executablePath: systemChrome } : {}),
});

try {
  const context = await browser.newContext();
  const first = await context.newPage();
  const second = await context.newPage();
  await Promise.all([
    first.goto(`http://127.0.0.1:${port}/`),
    second.goto(`http://127.0.0.1:${port}/`),
  ]);
  const room = `browser-room-${Date.now()}`;
  await first.evaluate(async ({ room }) => {
    const { createMultiTabSyncCoordinator } = await import('/bundle.mjs');
    globalThis.__coord = createMultiTabSyncCoordinator({ databaseName: 'multi-tab-browser', room, tabId: 'tab-a' });
    globalThis.__dirty = null;
    globalThis.__replicated = null;
    globalThis.__coord.onDirty((message) => { globalThis.__dirty = message; });
    globalThis.__coord.onExternalChange((message) => { globalThis.__replicated = message; });
    await globalThis.__coord.start();
  }, { room });
  await second.evaluate(async ({ room }) => {
    const { createMultiTabSyncCoordinator } = await import('/bundle.mjs');
    globalThis.__coord = createMultiTabSyncCoordinator({ databaseName: 'multi-tab-browser', room, tabId: 'tab-b' });
    globalThis.__dirty = null;
    globalThis.__replicated = null;
    globalThis.__coord.onDirty((message) => { globalThis.__dirty = message; });
    globalThis.__coord.onExternalChange((message) => { globalThis.__replicated = message; });
    await globalThis.__coord.start();
  }, { room });

  await waitFor(first, () => globalThis.__coord?.isLeader?.() === true, 'first tab leader');
  await waitFor(second, () => globalThis.__coord?.isLeader?.() === false, 'second tab follower');
  await second.evaluate(() => globalThis.__coord.notifyDirty('tickets', ['ticket-browser-1']));
  await waitFor(first, () => globalThis.__dirty?.ids?.[0] === 'ticket-browser-1', 'dirty event at leader');

  await first.evaluate(() => globalThis.dispatchEvent(new Event('pagehide')));
  await waitFor(second, () => globalThis.__coord?.isLeader?.() === true, 'follower leader handover');
  await second.evaluate(() => globalThis.__coord.notifyReplicatedChange('tickets', ['ticket-browser-2']));
  await waitFor(first, () => globalThis.__replicated?.ids?.[0] === 'ticket-browser-2', 'replicated event at follower');

  const roles = await Promise.all([
    first.evaluate(() => globalThis.__coord.snapshot()),
    second.evaluate(() => globalThis.__coord.snapshot()),
  ]);
  assert(roles.filter((status) => status.isLeader).length === 1, 'exactly one real tab must own the sync line');
  await Promise.all([
    first.evaluate(() => globalThis.__coord.close()),
    second.evaluate(() => globalThis.__coord.close()),
  ]);
  await context.close();
  console.log('ctox-rxdb real-browser multi-tab leader smoke OK');
} finally {
  await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
}

async function waitFor(page, predicate, label, timeoutMs = 5_000) {
  await page.waitForFunction(predicate, null, { timeout: timeoutMs }).catch((error) => {
    throw new Error(`Timed out waiting for ${label}`, { cause: error });
  });
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
