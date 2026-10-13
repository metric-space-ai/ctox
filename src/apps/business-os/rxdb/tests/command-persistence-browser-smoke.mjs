import http from 'node:http';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(testDir, '..');
const { chromium } = await import(pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href);
const server = http.createServer((request, response) => {
  try {
    const path = resolve(root, `.${new URL(request.url, 'http://localhost').pathname}`);
    if (!path.startsWith(`${root}/`)) throw new Error('invalid fixture path');
    response.writeHead(200, { 'content-type': 'text/javascript' });
    response.end(readFileSync(path));
  } catch {
    response.writeHead(200, { 'content-type': 'text/html' });
    response.end('<!doctype html><title>isolated command persistence</title>');
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
      properties: { id: { type: 'string', maxLength: 128 }, payload: { type: 'string' } },
      required: ['id'],
    };
    const name = `command-persistence-${crypto.randomUUID()}`;
    const db = await createRxDatabase({ name });
    await db.addCollections({ business_commands: { schema }, fixture_leads: { schema } });
    const commands = db.business_commands;
    const leads = db.fixture_leads.storageCollection;
    await Promise.all([commands.storageCollection.initializeRecovery(), leads.initializeRecovery()]);
    const docs = Array.from({ length: 10_000 }, (_, index) => ({ id: `lead-${index}`, payload: 'x'.repeat(1024) }));
    const origin = { role: 'ctox_instance', peerId: 'isolated-fixture' };
    await leads.bulkWrite(docs, { replicationOrigin: origin });
    const phases = {};
    for (const [owner, method] of [
      [commands.storageCollection, 'initializeRecovery'], [commands.storageCollection, 'prepareJournalRows'],
      [commands.storageCollection, '_bulkWriteOnce'], [db.storage.recoveryJournal, 'appendBatch'],
      [db.storage.recoveryJournal, 'commitBatch'],
    ]) {
      const original = owner[method].bind(owner);
      owner[method] = async (...args) => {
        const start = performance.now();
        try { return await original(...args); }
        finally { (phases[method] ||= []).push(performance.now() - start); }
      };
    }
    const times = [];
    let replicationDone = false;
    const replicationStarted = performance.now();
    const replication = leads.bulkWrite(docs.map((doc) => ({ ...doc, payload: `${doc.payload}updated` })), {
      replicationOrigin: origin, force: true,
    }).then(() => { replicationDone = true; });
    // Let the actual replication-origin storage call open its first transaction.
    await new Promise((done) => setTimeout(done, 20));
    const writesDuringReplication = [];
    await Promise.all(Array.from({ length: 20 }, async (_, index) => {
      await new Promise((done) => setTimeout(done, index * 10));
      writesDuringReplication.push(!replicationDone);
      const start = performance.now();
      await commands.insert({ id: `command-${index}`, payload: 'durable command' });
      times.push(performance.now() - start);
    }));
    await replication;
    const replicationMs = performance.now() - replicationStarted;
    const batches = await db.storage.recoveryJournal.listBatches('pending', 'business_commands');
    if (batches.length !== 20 || batches.some((batch) => !batch.primaryCommittedAtMs)) {
      throw new Error('insert resolved without a committed recovery journal entry');
    }
    await db.close();
    const reopened = await createRxDatabase({ name });
    await reopened.addCollections({ business_commands: { schema }, fixture_leads: { schema } });
    await reopened.business_commands.storageCollection.initializeRecovery();
    const persisted = await reopened.business_commands.storageCollection.allDocuments();
    const replicated = await reopened.fixture_leads.storageCollection.allDocuments();
    if (persisted.length !== 20 || replicated.length !== 10_000 || replicated.some((doc) => !doc.payload.endsWith('updated'))) {
      throw new Error('concurrent writes did not survive reopen with all replicated documents');
    }
    await reopened.close();
    indexedDB.deleteDatabase(name);
    indexedDB.deleteDatabase(`${name}__recovery_v2`);
    const p95 = (values) => [...values].sort((a, b) => a - b)[Math.ceil(values.length * 0.95) - 1];
    return { n: times.length, localPersistenceMs: times, p95Ms: p95(times), phases,
      phaseP95Ms: Object.fromEntries(Object.entries(phases).map(([key, values]) => [key, p95(values)])),
      writesDuringReplication: writesDuringReplication.filter(Boolean).length, replicationMs,
      durableAfterReopen: persisted.length, replicatedAfterReopen: replicated.length };
  });
  console.log(JSON.stringify(result));
  if (process.env.COMMAND_PERSISTENCE_EVIDENCE) writeFileSync(process.env.COMMAND_PERSISTENCE_EVIDENCE, JSON.stringify(result, null, 2));
  if (result.writesDuringReplication !== 20) throw new Error('fixture failed to overlap every command with replication');
  if (result.p95Ms >= 50) throw new Error(`local_persistence p95 ${result.p95Ms.toFixed(2)}ms exceeds 50ms`);
} finally {
  await browser.close();
  await new Promise((done) => server.close(done));
}
