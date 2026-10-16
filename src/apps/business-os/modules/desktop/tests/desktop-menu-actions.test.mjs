// Desktop-Menüaktionen gegen den App-API-4.0-Schreibweg (api.put/patch/remove über den Live-Stand `docs`).
import assert from 'node:assert/strict';
import { createCtoxLauncher } from '../ctoxLauncher.js';
import { addMissingDesktopIcons, arrangeDesktopIcons, replaceDesktopIcons, runDesktopActionOnce } from '../desktopMenuActions.js';

function fakeApi(initial = []) {
  const records = new Map(initial.map((item) => [item.id, { ...item }]));
  const ops = [];
  const handle = () => ({ local: Promise.resolve(), result: Promise.resolve({ status: 'accepted' }) });
  return {
    records, ops,
    put(collection, doc) { ops.push(['put', collection, doc.id]); records.set(doc.id, { ...doc }); return handle(); },
    patch(collection, id, patch) { ops.push(['patch', collection, id]); Object.assign(records.get(id), patch); return handle(); },
    remove(collection, id) { ops.push(['remove', collection, id]); records.delete(id); return handle(); },
    docs() { return [...records.values()].map((r) => ({ ...r })); },
  };
}

const custom = { id: 'custom', target_module: 'notes', target_type: 'module', label: 'My Notes', x: 72, y: 90, sort_index: 4, hidden: false };
const hidden = { id: 'desk_icon_explorer', target_module: 'explorer', target_type: 'module', label: 'Explorer', x: 5, y: 5, hidden: true };
const api = fakeApi([custom, hidden]);
const entries = [{ id: 'notes', title: 'Notes', kind: 'module' }, { id: 'explorer', title: 'Explorer', kind: 'module' }, { id: 'ctox', title: 'CTOX', kind: 'module' }];

const added = addMissingDesktopIcons({ api, docs: api.docs(), entries, gridPosition: (i) => ({ x: i * 10, y: 0 }), glyphFor: () => '◻︎' });
assert.equal(added.count, 2, 'explorer (versteckt) und ctox (fehlt)');
assert.equal(api.records.get('desk_icon_explorer').hidden, false, 'verstecktes Icon wird sichtbar statt doppelt angelegt');
assert.equal(api.records.get('desk_icon_ctox').target_module, 'ctox');
assert.deepEqual(api.ops.map((o) => o[0]), ['patch', 'put']);

api.ops.length = 0;
const arranged = arrangeDesktopIcons({ api, docs: api.docs(), knows: (id) => ['notes', 'explorer', 'ctox'].includes(id), labelFor: (d) => d.label, gridPosition: (i) => ({ x: i * 100, y: 0 }), order: 'name' });
assert.equal(arranged.length, 3);
assert.deepEqual(['custom', 'desk_icon_ctox', 'desk_icon_explorer'].map((id) => api.records.get(id).sort_index), [2, 0, 1], 'nach Name: CTOX, Explorer, My Notes');

api.ops.length = 0;
replaceDesktopIcons({ api, docs: api.docs(), snapshot: [{ id: 'desk_icon_ctox', target_type: 'module', target_module: 'ctox', updated_at_ms: 1 }] });
assert.deepEqual([...api.records.keys()], ['desk_icon_ctox']);
assert.ok(api.ops.every((o) => o[1] === 'desktop_icons'));

const pending = new Set();
let runs = 0;
const first = runDesktopActionOnce(pending, 'k', async () => { runs += 1; await new Promise((r) => setTimeout(r, 5)); return 'ok'; });
assert.equal(await runDesktopActionOnce(pending, 'k', async () => { runs += 1; }), undefined, 'gleiche Aktion läuft nicht doppelt');
assert.equal(await first, 'ok');
assert.equal(runs, 1);

const navigations = [];
const launcher = createCtoxLauncher({ modules: [{ id: 'desktop' }, { id: 'notes', title: 'Notes' }], currentModuleId: 'desktop', navigate: (id, params) => { navigations.push([id, params]); return true; } });
assert.equal(launcher.knows('desktop'), false, 'eigene App erscheint nicht im Launcher');
assert.equal(launcher.open('notes', { recordId: 'n1' }), true);
assert.deepEqual(navigations, [['notes', { record: 'n1' }]]);
assert.equal(launcher.open('missing'), true, 'Ziel prüft die Shell (navigate meldet unbekannte Ziele selbst)');
assert.equal(launcher.glyphFor('ctox'), '◆');
console.log('desktop-menu-actions: ok');
