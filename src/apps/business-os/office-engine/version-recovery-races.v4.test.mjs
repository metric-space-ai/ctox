import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { build } from '../node_modules/esbuild/lib/main.js';

// Shell v4 (Release-Reihe 0.2.x): v4-Gegenstück zu version-recovery-races.test.mjs (V2, bleibt unverändert, 0.1.x).
// App-API 4.0 liest Versionen synchron aus dem lokalen Stand
// (api.get/api.query, V4-PORT.md documents Nr. 3). Den V2-Wiederanlauf (findOne query_cancelled →
// startCollection forceDirect → waitForOpenPeerId) gibt es nicht mehr; die Regel bleibt: ein Lade-/Live-Abschluss
// überschreibt nie einen gleichzeitigen Editorzustand (Entwurf, laufendes Speichern) und ersetzt die gewählte
// Version nur bei freiem Editor – dann als eigene Kopie der eingefrorenen Live-Zeile.
const base = fileURLToPath(new URL('../modules', import.meta.url));
const ctoxScheme = { name: 'ctox-scheme', setup(b) {
  b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
  b.onResolve({ filter: /^ctox:lib\// }, (args) => ({ path: args.path, external: true }));
} };

function fakeApi(versions) {
  const reads = [];
  return {
    reads,
    get(collection, id) { reads.push([collection, id]); return { get: () => versions[id] || null }; },
    query(collection) { return { get: () => [], subscribe(cb) { cb([]); return () => {}; } }; },
    status: { report() {}, clear() {} },
  };
}

for (const [module, kind, records, versionsCollection] of [
  ['documents', 'ctox-documents', 'documents', 'document_versions'],
  ['spreadsheets', 'ctox-spreadsheets', 'spreadsheets', 'spreadsheet_versions'],
]) {
  const source = await readFile(`${base}/${module}/index.js`, 'utf8');
  const bundle = await build({
    stdin: { contents: `${source}\nexport { loadSelectedVersion as testLoadSelectedVersion };`,
      resolveDir: `${base}/${module}`, sourcefile: 'index.js', loader: 'js' },
    bundle: true, platform: 'browser', format: 'esm', write: false, logLevel: 'silent', plugins: [ctoxScheme],
  });
  const { testLoadSelectedVersion: load } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);

  const fixture = () => {
    const live = Object.freeze({ id: 'v2', [`${module === 'documents' ? 'document' : 'spreadsheet'}_id`]: 'file' });
    const api = fakeApi({ v2: live });
    const original = { id: 'v1' };
    const handle = { kind, recordId: 'file', activity: 0, saving: false };
    const state = {
      api, t: (_key, fallback) => fallback, disposers: [],
      [records]: [{ id: 'file', current_version_id: 'v2' }],
      selectedId: 'file', selectedVersion: original, editorHandle: handle, dirty: false, saving: false,
    };
    return { api, live, original, handle, state };
  };

  for (const change of ['edit', 'saving']) {
    test(`${module}: live version completion preserves concurrent ${change}`, async () => {
      const f = fixture();
      if (change === 'edit') { f.state.dirty = true; f.handle.activity += 1; }
      if (change === 'saving') { f.handle.saving = true; f.state.saving = true; }
      const expected = { selectedId: f.state.selectedId, dirty: f.state.dirty, saving: f.state.saving, handleSaving: f.handle.saving };
      assert.equal(await load(f.state), null);
      assert.equal(f.state.selectedVersion, f.original, 'protected editor keeps its version object');
      assert.deepEqual({ selectedId: f.state.selectedId, dirty: f.state.dirty, saving: f.state.saving, handleSaving: f.handle.saving }, expected);
    });
  }

  test(`${module}: free editor takes the live version as its own copy, read stays local`, async () => {
    const f = fixture();
    await load(f.state);
    assert.deepEqual(f.state.selectedVersion, f.live);
    assert.notEqual(f.state.selectedVersion, f.live, 'editor must not mutate the frozen live row');
    assert.equal(f.state.dirty, false);
    assert.deepEqual(f.api.reads, [[versionsCollection, 'v2']]);
  });
}
