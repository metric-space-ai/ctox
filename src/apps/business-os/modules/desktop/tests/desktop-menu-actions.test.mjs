import assert from 'node:assert/strict';
import { createCtoxLauncher } from '../ctoxLauncher.js';
import {
  addMissingDesktopIcons,
  arrangeDesktopIcons,
  desktopIconWriteAvailability,
  replaceDesktopIcons,
  runDesktopActionOnce,
} from '../desktopMenuActions.js';

function makeCollection(initial = []) {
  const records = new Map(initial.map((item) => [item.id, { ...item }]));
  const wrap = (value) => ({
    ...value,
    incrementalPatch: async (patch) => Object.assign(value, patch),
    remove: async () => records.delete(value.id),
  });
  return {
    records,
    find: () => ({ exec: async () => [...records.values()].map(wrap) }),
    insert: async (seed) => { records.set(seed.id, { ...seed }); },
  };
}

const custom = { id: 'custom', target_module: 'notes', target_type: 'module', label: 'My Notes', x: 72, y: 90, sort_index: 4, hidden: false };
const hidden = { id: 'desk_icon_explorer', target_module: 'explorer', target_type: 'app', label: 'Explorer', x: 5, y: 5, hidden: true };
const collection = makeCollection([custom, hidden]);
const options = {
  collection,
  entries: [{ id: 'explorer', kind: 'app', title: 'Explorer' }, { id: 'notes', kind: 'module', title: 'Notes' }],
  gridPosition: (index) => ({ x: index * 20, y: 10 }),
  glyphFor: () => '◇',
  insertMissingSeed: (db, id, seed) => db.insert(seed),
};
assert.equal(await addMissingDesktopIcons(options), 1);
assert.equal(await addMissingDesktopIcons(options), 0);
assert.deepEqual(collection.records.get('custom'), custom);
assert.equal(collection.records.get('desk_icon_explorer').hidden, false);
assert.equal(collection.records.size, 2);

const remembered = [];
await arrangeDesktopIcons({
  collection,
  knows: () => true,
  labelFor: (doc) => doc.label,
  gridPosition: (index) => ({ x: index * 100, y: 25 }),
  rememberPosition: (...args) => remembered.push(args),
  order: 'name',
});
assert.equal(remembered.length, 2);
const reloaded = makeCollection([...collection.records.values()]);
assert.deepEqual(
  (await reloaded.find().exec()).map((doc) => ({ id: doc.id, x: doc.x, y: doc.y, sort_index: doc.sort_index }))
    .sort((left, right) => left.sort_index - right.sort_index),
  [
    { id: 'desk_icon_explorer', x: 0, y: 25, sort_index: 0 },
    { id: 'custom', x: 100, y: 25, sort_index: 1 },
  ],
);

const snapshot = [...collection.records.values()].map((doc) => ({ ...doc }));
await replaceDesktopIcons({ collection, snapshot: [{ id: 'desk_icon_notes', target_type: 'module' }], insertMissingSeed: options.insertMissingSeed });
assert.equal(collection.records.has('custom'), false);
await replaceDesktopIcons({ collection, snapshot, insertMissingSeed: options.insertMissingSeed });
assert.equal(collection.records.get('custom').label, 'My Notes');

const apps = [{ id: 'explorer' }];
assert.equal(await createCtoxLauncher({ modules: [], apps, currentModuleId: 'desktop' }).open('explorer'), false);
assert.equal(await createCtoxLauncher({ modules: [], apps, currentModuleId: 'desktop', openApp: async () => null }).open('explorer'), false);
await assert.rejects(
  createCtoxLauncher({ modules: [], apps, currentModuleId: 'desktop', openApp: async () => { throw new Error('denied'); } }).open('explorer'),
  /denied/,
);
assert.equal(await createCtoxLauncher({ modules: [], apps, currentModuleId: 'desktop', openApp: async () => 'window-1' }).open('explorer'), true);

const messages = [];
let retries = 0;
const availability = desktopIconWriteAvailability({
  collection,
  readiness: { ready: false },
  reason: 'Sync unavailable; retry',
  notify: (message) => messages.push(message),
  retry: () => { retries += 1; },
});
assert.equal(availability.disabled, true);
assert.equal(availability.disabledReason, 'Sync unavailable; retry');
availability.onDisabled();
assert.deepEqual(messages, ['Sync unavailable; retry']);
assert.equal(retries, 1);
assert.equal(desktopIconWriteAvailability({ collection, readiness: { ready: true } }).disabled, false);
assert.equal(desktopIconWriteAvailability({ collection, readiness: null }).disabled, true);

let resolveDispatch;
let dispatches = 0;
const pending = new Set();
const firstDispatch = runDesktopActionOnce(pending, 'rename:app', () => {
  dispatches += 1;
  return new Promise((resolve) => { resolveDispatch = resolve; });
});
assert.equal(await runDesktopActionOnce(pending, 'rename:app', () => { dispatches += 1; }), undefined);
assert.equal(dispatches, 1);
resolveDispatch('done');
assert.equal(await firstDispatch, 'done');
assert.equal(pending.size, 0);

console.log('desktop menu action behavior ok');
