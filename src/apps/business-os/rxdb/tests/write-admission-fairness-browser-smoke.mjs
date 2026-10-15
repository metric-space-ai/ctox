import http from 'node:http';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(process.env.WRITE_ADMISSION_RUNTIME_ROOT || resolve(testDir, '..'));
const playwrightModule = process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href
  : '../../node_modules/playwright/index.mjs';
const { chromium } = await import(playwrightModule);
const server = http.createServer((request, response) => {
  try {
    const path = resolve(root, `.${new URL(request.url, 'http://localhost').pathname}`);
    if (!path.startsWith(`${root}/`)) throw new Error('invalid fixture path');
    const content = readFileSync(path);
    response.writeHead(200, { 'content-type': 'text/javascript' });
    response.end(content);
  } catch {
    response.writeHead(200, { 'content-type': 'text/html' });
    response.end('<!doctype html><title>write admission fairness</title>');
  }
});
await new Promise((done) => server.listen(0, '127.0.0.1', done));
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const result = await page.evaluate(async () => {
    const { createRxDatabase } = await import('/src/index.mjs');
    const schema = {
      version: 0, type: 'object', primaryKey: 'id',
      properties: { id: { type: 'string', maxLength: 128 }, payload: { type: 'string' } }, required: ['id'],
    };
    const name = `write-fairness-${crypto.randomUUID()}`;
    const db = await createRxDatabase({ name });
    await db.addCollections({ business_commands: { schema }, fixture_leads: { schema } });
    const commands = db.business_commands;
    const storage = commands.storageCollection;
    const leads = db.fixture_leads.storageCollection;
    await Promise.all([storage.initializeRecovery(), leads.initializeRecovery()]);

    let entered;
    let release;
    const heldEntered = new Promise((done) => { entered = done; });
    const heldRelease = new Promise((done) => { release = done; });
    const originalWrite = storage._bulkWriteOnce.bind(storage);
    storage._bulkWriteOnce = async (rows, options) => {
      if (!options?.replicationOrigin?.role && rows[0]?.id === 'pressure-held') {
        entered();
        await heldRelease;
      }
      return originalWrite(rows, options);
    };
    // Keep a REAL, already journaled local operation in flight so an
    // accidental zero-writer gap cannot make the unbounded implementation pass.
    const held = commands.insert({ id: 'pressure-held', payload: 'journaled before primary' });
    await heldEntered;
    const heldBatch = (await db.storage.recoveryJournal.listBatches('pending', 'business_commands'))[0];
    if (!heldBatch || heldBatch.primaryCommittedAtMs || heldBatch.rows[0]?.id !== 'pressure-held') {
      throw new Error('held operation was not durably journaled before the primary-write barrier');
    }

    let pressureActive = true;
    let completedLocalWrites = 0;
    const errors = [];
    const writers = Array.from({ length: 3 }, (_, worker) => (async () => {
      let sequence = 0;
      while (pressureActive) {
        await commands.insert({ id: `pressure-${worker}-${sequence++}`, payload: 'continuous local write' });
        completedLocalWrites += 1;
      }
    })().catch((error) => { errors.push(String(error.message)); }));
    let committedRowsWhilePressureActive = 0;
    let firstCommitMs = null;
    const start = performance.now();
    const originalNativeWrite = leads._bulkWriteOnce.bind(leads);
    leads._bulkWriteOnce = async (...args) => {
      const result = await originalNativeWrite(...args);
      if (pressureActive) {
        committedRowsWhilePressureActive += Object.keys(result.success).length;
        firstCommitMs ??= performance.now() - start;
      }
      return result;
    };
    const replication = leads.bulkWrite(Array.from({ length: 100 }, (_, index) => ({
      id: `native-${index}`, payload: 'replication progresses under write pressure',
    })), { replicationOrigin: { role: 'ctox_instance', peerId: 'isolated-fairness-fixture' } });
    let deadline;
    const completedUnderPressure = await Promise.race([
      replication.then(() => true, (error) => { errors.push(String(error.message)); return false; }),
      new Promise((done) => { deadline = setTimeout(() => done(false), 1500); }),
    ]);
    clearTimeout(deadline);
    const localCompletionsBeforeStop = completedLocalWrites;
    const rowsBeforeStop = committedRowsWhilePressureActive;
    const pressureWindowMs = performance.now() - start;
    pressureActive = false;
    release();
    await Promise.all([held, ...writers, replication]);
    storage._bulkWriteOnce = originalWrite;
    leads._bulkWriteOnce = originalNativeWrite;
    const nativeRows = await leads.allDocuments();
    const localRows = await storage.allDocuments();
    const journal = await db.storage.recoveryJournal.listBatches('pending', 'business_commands');
    const durable = journal.length === localRows.length && journal.every((batch) => batch.primaryCommittedAtMs > 0);
    await db.close();
    indexedDB.deleteDatabase(name);
    indexedDB.deleteDatabase(`${name}__recovery_v2`);
    return { completedUnderPressure, rowsBeforeStop, firstCommitMs, pressureWindowMs,
      localCompletionsBeforeStop, nativeRows: nativeRows.length, localRows: localRows.length, durable, errors };
  });
  console.log(JSON.stringify(result));
  if (process.env.WRITE_ADMISSION_EVIDENCE) writeFileSync(process.env.WRITE_ADMISSION_EVIDENCE, JSON.stringify(result, null, 2));
  if (!result.completedUnderPressure || result.rowsBeforeStop !== 100 || result.localCompletionsBeforeStop < 8) {
    throw new Error('replication starved under continuous real local writes and an in-flight journaled operation');
  }
  if (!result.durable || result.nativeRows !== 100 || result.errors.length) throw new Error('fair admission lost durable writes');
} finally {
  await browser.close();
  await new Promise((done) => server.close(done));
}
