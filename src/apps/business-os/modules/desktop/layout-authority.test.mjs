import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { ensureDesktopLayoutWithAuthority } from './layout-authority.js';

const defaultLayout = () => ({
  wallpaper_url: '',
  wallpaper_mode: 'cover',
  taskbar_pins: ['ctox'],
  grid_cell_w: 104,
  grid_cell_h: 104,
  grid_offset: 24,
});

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
  const result = await ensureDesktopLayoutWithAuthority({
    collection: db,
    documentId: 'layout',
    defaultLayout,
    readNativeDocument: async () => {
      throw new Error('authority unavailable');
    },
    insertMissingSeed: async () => assert.fail('failed authority must not seed'),
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

const desktopSource = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
assert.match(
  desktopSource,
  /readNativeDocument: ctx\.readNativeCollectionDocument[\s\S]*readNativeCollectionDocument\('desktop_layout', LAYOUT_DOC_ID, \{ timeoutMs: 5000 \}\)/,
  'layout creation must be gated by the bounded native authority read',
);
