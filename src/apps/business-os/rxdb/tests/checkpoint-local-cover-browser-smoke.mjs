// Actual IndexedDB resume/eviction authority, including quota failure and store recreation.
import assert from 'node:assert/strict';
import http from 'node:http';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '../../node_modules/playwright/index.mjs';
const bundle = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), '../dist/ctox-rxdb-js.mjs'));
const server = http.createServer((req, res) => {
  res.writeHead(200, { 'content-type': req.url === '/bundle.mjs' ? 'text/javascript' : 'text/html' });
  res.end(req.url === '/bundle.mjs' ? bundle : '<!doctype html><title>Checkpoint storage fixture</title>');
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
let browser;
try {
  const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  browser = await chromium.launch({ headless: true, ...(existsSync(chrome) ? { executablePath: chrome } : {}) });
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const result = await page.evaluate(async () => {
    const { openCtoxIndexedDbStorage, replicationWebRtcTestInternals, formatHybridLogicalClock } = await import('/bundle.mjs');
    const { localCheckpointValidityKey: key, localCheckpointStillCovers: covers } = replicationWebRtcTestInternals;
    const check = (condition, message) => { if (!condition) throw new Error(message); };
    const databaseName = `checkpoint-cover-${Date.now()}`;
    const name = 'fixture_leads';
    const schema = { version: 1, primaryKey: 'id', type: 'object', properties: { id: { type: 'string', maxLength: 100 } } };
    const options = { replicationOrigin: { role: 'ctox_instance', sessionId: 'fixture-native' } };
    let storage = await openCtoxIndexedDbStorage({ databaseName });
    let records = storage.collection(name, { schema });
    const write = async (id, revision, physicalMs) => records.bulkWrite([{ id, _rev: revision, _meta: { ctoxHlc: formatHybridLogicalClock({ physicalMs, logical: 0, nodeId: 'fixture-native' }) } }], options);
    try {
      await write('older-row', '1-native', Date.now());
      await write('head-row', '1-native', Date.now() + 1000);
      const initialStatus = await records.replicationCheckpointStatus('fixture-schema');
      const retained = key(initialStatus);
      await write('head-row', '2-native', Date.now() + 2000);
      check(covers(retained, key(await records.replicationCheckpointStatus('fixture-schema'))), 'ordinary writes preserve incremental resume');
      await storage.close();
      storage = await openCtoxIndexedDbStorage({ databaseName }); records = storage.collection(name, { schema });
      check(covers(retained, key(await records.replicationCheckpointStatus('fixture-schema'))), 'generation and resume survive browser storage reopen');

      // A retained checkpoint can predate localStorage failure. Row deletion
      // must invalidate it atomically even if a separate counter cannot write.
      const originalSet = Storage.prototype.setItem;
      Storage.prototype.setItem = function () { throw new DOMException('Fixture quota', 'QuotaExceededError'); };
      try { await records.hardDeleteByIds(['older-row']); }
      finally { Storage.prototype.setItem = originalSet; }
      const afterEviction = await records.replicationCheckpointStatus('fixture-schema');
      check(!(await records.getStoredRecord('older-row')), 'older row was actually removed');
      check(afterEviction.latestLwt >= initialStatus.latestLwt, 'latest row still exists after older-row eviction');
      check(!covers(retained, key(afterEviction)), 'eviction invalidates resume even with localStorage unavailable');

      await write('abort-victim', '1-native', Date.now() + 3000);
      const beforeAbort = key(await records.replicationCheckpointStatus('fixture-schema'));
      const originalPut = IDBObjectStore.prototype.put;
      IDBObjectStore.prototype.put = function (...args) {
        if (this.name === 'collectionSchemaMarkers') {

          throw new DOMException('Fixture marker quota', 'QuotaExceededError');
        }
        return originalPut.apply(this, args);
      };
      let failed = false;
      try { await records.hardDeleteByIds(['abort-victim']); }
      catch { failed = true; }
      finally { IDBObjectStore.prototype.put = originalPut; }
      check(failed, 'failed marker transaction must reject deletion');
      check(Boolean(await records.getStoredRecord('abort-victim')), 'aborted marker write rolls row deletion back');
      check(key(await records.replicationCheckpointStatus('fixture-schema')) === beforeAbort, 'aborted eviction retains original generation');

      await storage.clearCachedCollections([name]);
      await write('head-row', '3-native', Date.now() + 4000);
      const afterClear = await records.replicationCheckpointStatus('fixture-schema');
      check(!covers(beforeAbort, key(afterClear)), 'clear and refill cannot reuse the former checkpoint');
      const beforeRecreate = key(afterClear);
      await storage.close();
      await new Promise((resolve, reject) => {
        const request = indexedDB.deleteDatabase(databaseName);
        request.onsuccess = resolve; request.onerror = () => reject(request.error);
        request.onblocked = () => reject(new Error('Fixture storage was not closed'));
      });
      storage = await openCtoxIndexedDbStorage({ databaseName }); records = storage.collection(name, { schema });
      await write('head-row', '4-native', Date.now() + 5000);
      const recreated = await records.replicationCheckpointStatus('fixture-schema');
      check(recreated.localStoreGeneration !== afterClear.localStoreGeneration, 'recreated store receives a different identity');
      check(!covers(beforeRecreate, key(recreated)), 'recreated store never inherits the previous store checkpoint');
      return { resume: true, reopened: true, eviction: true, abortRollback: true, cleared: true, recreated: true };
    } finally { await storage.close(); }
  });
  assert.deepEqual(result, { resume: true, reopened: true, eviction: true, abortRollback: true, cleared: true, recreated: true });
  console.log('checkpoint local cover browser smoke OK');
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
