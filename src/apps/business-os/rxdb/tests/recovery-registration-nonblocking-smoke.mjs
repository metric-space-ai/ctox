import http from 'node:http';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const bundle = readFileSync(resolve(testDir, '../dist/ctox-rxdb-js.mjs'));
const server = http.createServer((request, response) => {
  if (request.url === '/bundle.mjs') {
    response.writeHead(200, { 'content-type': 'text/javascript' });
    response.end(bundle);
    return;
  }
  response.writeHead(200, { 'content-type': 'text/html' });
  response.end('<!doctype html><title>recovery registration smoke</title>');
});
await new Promise((resolveReady) => server.listen(0, '127.0.0.1', resolveReady));
const { port } = server.address();
const playwrightModule = process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href
  : '../../node_modules/playwright/index.mjs';
const { chromium } = await import(playwrightModule);
const systemChrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser = await chromium.launch({
  headless: true,
  ...(existsSync(systemChrome) ? { executablePath: systemChrome } : {}),
});

try {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}/`);
  const result = await page.evaluate(async () => {
    const { openCtoxIndexedDbStorage } = await import('/bundle.mjs');
    const databaseName = `recovery-registration-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const storage = await openCtoxIndexedDbStorage({ databaseName });
    const tickets = storage.collection('tickets', {
      schema: {
        version: 0,
        type: 'object',
        primaryKey: 'id',
        properties: {
          id: { type: 'string', maxLength: 128 },
          title: { type: 'string' },
        },
        required: ['id'],
      },
    });
    const before = await storage.recoveryJournal.getStatus();
    await tickets.initializeRecovery();
    await tickets.bulkUpsert([{ id: 'ticket-1', title: 'nonblocking registration' }]);
    const afterWrite = await storage.recoveryJournal.getStatus();
    const reopened = await openCtoxIndexedDbStorage({ databaseName });
    const reopenedTickets = reopened.collection('tickets', {
      schema: {
        version: 0,
        type: 'object',
        primaryKey: 'id',
        properties: {
          id: { type: 'string', maxLength: 128 },
          title: { type: 'string' },
        },
        required: ['id'],
      },
    });
    await reopenedTickets.initializeRecovery();
    const replayed = await reopenedTickets.findOne('ticket-1');
    const afterReplay = await reopened.recoveryJournal.getStatus();
    storage.close();
    reopened.close();
    indexedDB.deleteDatabase(databaseName);
    indexedDB.deleteDatabase(`${databaseName}__recovery_v2`);
    return {
      pendingBefore: before.pendingWrites,
      pendingAfterWrite: afterWrite.pendingWrites,
      replayedTitle: replayed?.title || '',
      pendingAfterReplay: afterReplay.pendingWrites,
    };
  });
  assert(result.pendingBefore === 0, 'fresh recovery journal should start empty');
  assert(result.pendingAfterWrite === 1, 'local write must be journaled before primary commit');
  assert(result.replayedTitle === 'nonblocking registration', 'reopened primary must see replayed local write');
  assert(result.pendingAfterReplay === 1, 'unacknowledged local write must remain pending after replay');
  console.log('ctox-rxdb recovery registration nonblocking smoke OK', result);
} finally {
  await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
