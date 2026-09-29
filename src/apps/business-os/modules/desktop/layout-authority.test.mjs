import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { ensureDesktopLayoutWithAuthority, readLocalDesktopIcons, readLocalDesktopLayout } from './layout-authority.js?v=20260929-desktop-icon-cancel-v1';

const defaultLayout = () => ({
  wallpaper_url: '',
  wallpaper_mode: 'cover',
  taskbar_pins: ['ctox'],
  grid_cell_w: 104,
  grid_cell_h: 104,
  grid_offset: 24,
});

{
  const launcherIcons = [{ id: 'ctox' }];
  let reads = 0;
  const cancelledCollection = {
    find() {
      reads += 1;
      return { exec: async () => { throw new Error('QUERY_CANCELLED: replication-cancel'); } };
    },
  };
  assert.deepEqual(
    await readLocalDesktopIcons({
      collection: cancelledCollection,
      fallbackIcons: () => launcherIcons,
    }),
    { docs: launcherIcons, usingFallbackDocs: true },
    'a cancelled icon read paints local launcher defaults without a data write',
  );
  assert.equal(reads, 1);
  await assert.rejects(
    readLocalDesktopIcons({
      collection: { find: () => ({ exec: async () => { throw new Error('UNAUTHORIZED'); } }) },
      fallbackIcons: () => launcherIcons,
    }),
    /UNAUTHORIZED/,
    'an authorization failure must not be disguised as a transient icon read',
  );
}

function collection(initial) {
  let document = initial;
  const calls = { insert: [], findOne: 0 };
  return {
    calls,
    set(document_) {
      document = document_;
    },
    async insert(seed) {
      calls.insert.push(seed);
    },
    async findOne() {
      calls.findOne += 1;
      return {
        exec: async () => (document ? { toJSON: () => document } : null),
      };
    },
  };
}

{
  const saved = { id: 'layout', grid_cell_w: 180, taskbar_pins: ['documents'] };
  const db = collection(saved);
  const result = await readLocalDesktopLayout({
    collection: db,
    documentId: 'layout',
    defaultLayout,
  });
  assert.deepEqual(result, saved);
  assert.equal(db.calls.findOne, 1);
  assert.deepEqual(db.calls.insert, []);
}

{
  const db = collection();
  const result = await readLocalDesktopLayout({
    collection: db,
    documentId: 'layout',
    defaultLayout,
  });
  assert.deepEqual(result, defaultLayout());
  assert.deepEqual(db.calls.insert, []);
}

await ensureDesktopLayoutWithAuthority({
  collection: null,
  documentId: 'layout',
  defaultLayout,
  readNativeDocument: async () => ({ taskbar_pins: ['user'] }),
  insertMissingSeed: async () => assert.fail('authority must suppress seeding'),
}).then((result) => assert.deepEqual(result, { taskbar_pins: ['user'] }));

{
  const db = collection();
  const authority = { toJSON: () => ({ taskbar_pins: ['user'] }) };
  const result = await ensureDesktopLayoutWithAuthority({
    collection: db,
    documentId: 'layout',
    defaultLayout,
    readNativeDocument: async () => authority,
    insertMissingSeed: async () => assert.fail('authority must suppress seeding'),
    now: () => 42,
  });
  assert.deepEqual(result, { taskbar_pins: ['user'] });
  assert.equal(db.calls.insert.length, 0);
  assert.equal(db.calls.findOne, 0);
}

{
  const db = collection();
  const saved = { id: 'layout', grid_cell_w: 180 };
  const result = await ensureDesktopLayoutWithAuthority({
    collection: db,
    documentId: 'layout',
    defaultLayout,
    unknownLayout: () => saved,
    readNativeDocument: async () => {
      throw new Error('authority unavailable');
    },
    insertMissingSeed: async () => assert.fail('failed authority must not seed'),
    now: () => 42,
  });
  assert.deepEqual(result, saved);
  assert.equal(db.calls.insert.length, 0);
  assert.equal(db.calls.findOne, 0);
}

{
  const db = collection();
  const saved = { id: 'layout', grid_cell_w: 180 };
  const result = await ensureDesktopLayoutWithAuthority({
    collection: db,
    documentId: 'layout',
    defaultLayout,
    unknownLayout: () => saved,
    readNativeDocument: async () => null,
    isCurrent: () => false,
    insertMissingSeed: async () => assert.fail('unmounted desktop must not seed'),
  });
  assert.deepEqual(result, saved);
  assert.deepEqual(db.calls.insert, []);
}

{
  const db = collection();
  const result = await ensureDesktopLayoutWithAuthority({
    collection: db,
    documentId: 'layout',
    defaultLayout,
    readNativeDocument: async () => undefined,
    insertMissingSeed: async () => assert.fail('undefined authority is unknown, not absence'),
    now: () => 42,
  });
  assert.deepEqual(result, defaultLayout());
  assert.equal(db.calls.insert.length, 0);
  assert.equal(db.calls.findOne, 0);
}

{
  const db = collection();
  const result = await ensureDesktopLayoutWithAuthority({
    collection: db,
    documentId: 'layout',
    defaultLayout,
    unknownLayout: () => ({ grid_cell_w: 180 }),
    readNativeDocument: async () => null,
    insertMissingSeed: async (scopedDb, id, seed) => {
      assert.equal(scopedDb, db);
      assert.equal(id, 'layout');
      await scopedDb.insert(seed);
    },
    now: () => 42,
  });
  assert.deepEqual(result, { id: 'layout', ...defaultLayout(), updated_at_ms: 42 });
  assert.equal(db.calls.insert.length, 1);
  assert.equal(db.calls.findOne, 1);
}

{
  const db = collection();
  const result = await ensureDesktopLayoutWithAuthority({
    collection: null,
    documentId: 'layout',
    defaultLayout,
    readNativeDocument: async () => null,
    insertMissingSeed: async () => assert.fail('missing collection must not seed'),
    now: () => 42,
  });
  assert.deepEqual(result, { id: 'layout', ...defaultLayout(), updated_at_ms: 42 });
}

{
  const winner = { id: 'layout', taskbar_pins: ['winner'] };
  const db = collection(winner);
  const result = await ensureDesktopLayoutWithAuthority({
    collection: db,
    documentId: 'layout',
    defaultLayout,
    readNativeDocument: async () => null,
    insertMissingSeed: async () => {
      db.set({ id: 'layout', taskbar_pins: ['race-winner'] });
      throw Object.assign(new Error('duplicate key'), { status: 409 });
    },
    now: () => 42,
  });
  assert.deepEqual(result, { id: "layout", taskbar_pins: ["race-winner"] });
  assert.deepEqual(db.calls.insert, []);
  assert.equal(db.calls.findOne, 1);
}

const shellSource = readFileSync(new URL('../../app.js', import.meta.url), 'utf8');
assert.match(
  shellSource,
  /readNativeCollectionDocument: mod\.id === 'desktop'[\s\S]*state\.sync\?\.readCollectionNativeDocument\(collection, documentId, options\)/,
  'desktop must receive the strict native collection reader',
);
assert.match(
  shellSource,
  /readCollectionNativeDocument\(collection, documentId, options\)[\s\S]*\.then\(\(document\) => document \?\? null\)/,
  'the strict reader must normalize native absence to explicit null',
);

const desktopSource = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
assert.match(
  desktopSource,
  /readNativeDocument: ctx\.readNativeCollectionDocument[\s\S]*readNativeCollectionDocument\('desktop_layout', LAYOUT_DOC_ID, \{ timeoutMs: 5000 \}\)/,
  'layout creation must be gated by the bounded native authority read',
);
const mountSource = desktopSource.slice(
  desktopSource.indexOf('export async function mount(ctx)'),
  desktopSource.indexOf('function wireSyncStatusWidget()'),
);
assert.ok(
  mountSource.indexOf('await readLocalDesktopLayout(') < mountSource.indexOf('await renderIcons()'),
  'first paint must use the locally saved layout',
);
assert.ok(
  mountSource.indexOf('await renderIcons()') < mountSource.indexOf('const reconciliationTimer = setTimeout('),
  'native layout reconciliation must not block the local first paint',
);
