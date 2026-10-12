// Isolated real Chromium IndexedDB experiment; never opens an operator profile.
// Usage: xvfb-run electron --no-sandbox measure_browser_checkpoint_hol.cjs OUTPUT.json
const { app, BrowserWindow } = require('electron');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const output = process.argv.at(-1);
const source = pathToFileURL(path.resolve(__dirname, '../../../apps/business-os/rxdb/src/storage-indexeddb.mjs')).href;
const deadline = setTimeout(() => { console.error('experiment deadline exceeded'); app.exit(1); }, 30000);
app.whenReady().then(async () => {
  const page = path.join(app.getPath('userData'), 'checkpoint-fixture.html');
  fs.writeFileSync(page, '<!doctype html><title>Isolated checkpoint fixture</title>');
  const win = new BrowserWindow({ show: false, webPreferences: { backgroundThrottling: false, contextIsolation: true, nodeIntegration: false } });
  await win.loadFile(page);
  const result = await win.webContents.executeJavaScript(`(async () => {
    const { openCtoxIndexedDbStorage } = await import(${JSON.stringify(source)});
    const storage = await openCtoxIndexedDbStorage({ databaseName: 'isolated-checkpoint-hol-' + crypto.randomUUID() });
    const collection = storage.collection('checkpoint-reader');
    await collection.replicationCheckpointStatus();
    const db = storage.db;
    const originalTransaction = db.transaction.bind(db);
    const trials = [];
    for (const mode of ['readwrite', 'readonly-control']) {
      for (let iteration = 0; iteration < 5; iteration++) {
        const order = [];
        const started = performance.now();
        const blocker = originalTransaction('documents', 'readonly');
        const blockerDone = new Promise((resolve, reject) => {
          blocker.oncomplete = () => { order.push('earlier-reader-completed'); resolve(); };
          blocker.onerror = () => reject(blocker.error);
        });
        const store = blocker.objectStore('documents');
        // Keep a real earlier transaction active with ordinary IDB requests.
        const next = () => {
          const request = store.get(['unrelated-collection', 'fixture']);
          request.onsuccess = () => { if (performance.now() - started < 250) next(); };
        };
        next();
        db.transaction = (stores, requestedMode, ...args) => originalTransaction(stores,
          mode === 'readonly-control' && requestedMode === 'readwrite' ? 'readonly' : requestedMode, ...args);
        const checkpoint = collection.replicationCheckpointStatus().then(value => {
          order.push('checkpoint-completed'); return { value, elapsedMs: performance.now() - started };
        });
        // A later read of another collection uses the same physical document store.
        const later = originalTransaction('documents', 'readonly');
        const laterRead = new Promise((resolve, reject) => {
          const request = later.objectStore('documents').get(['another-collection', 'fixture']);
          request.onsuccess = () => { order.push('later-reader-completed'); resolve(performance.now() - started); };
          request.onerror = () => reject(request.error);
        });
        const [status, laterElapsedMs] = await Promise.all([checkpoint, laterRead, blockerDone]);
        db.transaction = originalTransaction;
        const blocked = order.indexOf('earlier-reader-completed') < order.indexOf('later-reader-completed');
        if (blocked !== (mode === 'readwrite')) throw new Error('unexpected real IDB transaction ordering: ' + order.join(','));
        trials.push({ mode, iteration, checkpointMs: status.elapsedMs, laterReaderMs: laterElapsedMs, order,
          generation: status.value.localStoreGeneration, evictionGeneration: status.value.evictionGeneration });
      }
    }
    storage.close();
    return { engine: navigator.userAgent, trials, pass: true,
      limitation: 'Synthetic held transaction proves scheduling HOL; does not measure installed Welsch backlog or attribute its latency.' };
  })()`);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, JSON.stringify(result, null, 2) + '\n');
  console.log('checkpoint HOL experiment PASS; 10 real Chromium trials; output=' + output);
  clearTimeout(deadline); app.quit();
}).catch(error => { console.error(error.stack || error); clearTimeout(deadline); app.exit(1); });
