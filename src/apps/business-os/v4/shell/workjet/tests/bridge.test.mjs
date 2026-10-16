// Workjet-Brücke gegen den echten Kern-Stub (Stub-Pin 8489b4e, In-Memory-Speicher): Adapter + Router + Vertrag.
// Lauf: node --test src/apps/business-os/v4/shell/workjet/tests/bridge.test.mjs
//   Stub-Pfad: CTOX_APP_API_STUB, sonst ~/.local/state/shell-v4/stub-pins/b6624eb.
import assert from 'node:assert/strict';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { pathToFileURL } from 'node:url';
import { adaptKernel } from '../../kernel-adapter.js';
import { installWorkjetBridges, createWorkjetBridges, inventoryList, INVENTORY } from '../index.js';
import { createV2Compat, isUnsupported } from '../v2-compat.js';
import { CHECKERS, checkResponse } from './contract-checkers.mjs';
import { PROJECT_ACTIONS } from '../actions/project.js';

const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/b6624eb');
const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR, 'ctox-app-api-stub.mjs')).href);

let n = 0;
async function mk() {
  const k = createStubKernel({ instanceId: `wjb0-${process.pid}-${++n}`, userId: 'owner-1', latencyMs: 2, storage: 'memory',
    actor: { id: 'owner-1', display_name: 'Owner', role: 'admin', permissions: [] } });
  await k.ready;
  return k;
}
// sim.inspect liefert die Speicherzeile; das Dokument steckt je nach Stub-Stand in .doc/.value/.data.
const docOf = (r) => r?.doc ?? r?.value ?? r?.data ?? r;
// Strang D: Aktionstabelle nur mit den Projektaktionen von Strang A (Router-/Einhänge-Regeln ohne D-Aktionen prüfen).
const PROJECT_ONLY = { project: PROJECT_ACTIONS, computer: {}, session: {}, device: {} };
const projectRow = (id, extra = {}) => ({ id, name: `Projekt ${id}`, status: 'active', owner_user_id: 'owner-1', created_at_ms: 1_700_000_000_000, ...extra });

test('Globals: nur Brücken mit freigeschalteten Aktionen hängen ein; uninstall räumt und beendet die Sitzung', async () => {
  const k = await mk();
  const target = {};
  // Strang D: Rechner/Sitzung/Gerät sind inzwischen freigeschaltet; die Regel „leere Brücke hängt nicht ein“ prüft
  // dieser Test daher mit einer Tabelle, in der nur Projektaktionen stehen (vollständiges Einhängen: oracle.test.mjs,
  // Block Strang D).
  const h = installWorkjetBridges({ kernel: adaptKernel(k), target, actions: PROJECT_ONLY });
  assert.deepEqual(h.installed, ['workjetProjectControl']);
  assert.equal(typeof target.workjetProjectControl, 'function');
  for (const name of ['workjetComputerControl', 'workjetSessionControl', 'workjetSessionEvents', 'workjetBusinessOsDeviceControl']) {
    assert.equal(Object.hasOwn(target, name), false, `${name} ohne portierte Aktion bleibt weg (Workjet: unsupported)`);
  }
  h.uninstall();
  assert.equal(Object.hasOwn(target, 'workjetProjectControl'), false);
  assert.equal(h.compat.state.session, null);
  await k.close();
});

test('unbekannte und nicht portierte Aktionen werfen code unsupported mit dem Workjet-Merkmal je Brücke', async () => {
  const k = await mk();
  const compat = createV2Compat({ kernel: adaptKernel(k) });
  const b = createWorkjetBridges(compat, { actions: PROJECT_ONLY }); // Strang D: Router-Merkmale ohne freigeschaltete D-Aktionen
  // Workjet CtoxGuestManager: /(?:unsupported|unknown)\s+(?:workjet\s+)?(?:project[- ]control\s+)?action/i → unsupported
  const workjetProjectRegex = /(?:unsupported|unknown)\s+(?:workjet\s+)?(?:project[- ]control\s+)?action/i;
  for (const action of ['project.jour_fixe.narration.local_publish', 'speech.dictation', 'instance.grok.read', 'gibt.es.nicht']) { // Strang A: project.create ist freigeschaltet → presentation.read; WJB-R: presentation.* freigeschaltet (E26) → narration.local_publish (portiert, nicht im INVENTORY)
    await assert.rejects(b.workjetProjectControl({ action }), (e) => isUnsupported(e) && workjetProjectRegex.test(e.message));
  }
  await assert.rejects(b.workjetComputerControl({ action: 'computer.list' }), (e) => isUnsupported(e) && e.message === 'workjet_computers collection is not registered.');
  await assert.rejects(b.workjetBusinessOsDeviceControl({ action: 'binding.list' }), (e) => isUnsupported(e) && e.nativeCode === 'CTOX_WEBRTC_CAPABILITY_MISSING');
  await assert.rejects(b.workjetSessionControl({ action: 'session.list' }), (e) => isUnsupported(e));
  await assert.rejects(b.workjetProjectControl(null), TypeError);
  await k.close();
});

test('project.list über kernel.command + kernel.query: V2-Antwortform und Workjet-Vertrag', async () => {
  const k = await mk();
  const seen = [];
  k.sim.onCommand('ctox.workjet.project.list', (payload, ctx) => {
    seen.push({ payload, ctx });
    return { ok: true, collection: 'workjet_projects', owner_user_id: 'owner-1', count: 2, truncated: false, project_ids: ['p-1', 'p-2'] };
  });
  await k.sim.push('workjet_projects', [projectRow('p-1'), projectRow('p-2', { created_at_ms: 1_800_000_000_000 }), projectRow('p-x', { status: 'archived' })]);
  await k.sim.push('workjet_working_copies', [{ id: 'wc-1', project_id: 'p-1', computer_id: 'mac', path: '/src/p1', status: 'active', owner_user_id: 'owner-1' }]);
  const target = {};
  const h = installWorkjetBridges({ kernel: adaptKernel(k), target });
  const res = JSON.parse(JSON.stringify(await target.workjetProjectControl({ action: 'project.list' })));
  assert.deepEqual(checkResponse('project.list', res), []);
  assert.equal(res.count, 2);
  assert.deepEqual(res.projects.map((p) => p.id), ['p-2', 'p-1'], 'neuestes zuerst (V2-Sortierung)');
  assert.deepEqual(res.projects[1].workingCopies, [{ id: 'wc-1', computerId: 'mac', path: '/src/p1', status: 'active' }]);
  assert.equal(seen.length, 1);
  assert.deepEqual(seen[0].payload, { limit: 100 });
  assert.equal(seen[0].ctx.app_id, 'shell');
  // Kernel-Dokument trägt record_id = Owner (§16.19 Nr. 8)
  const doc = docOf(await k.sim.inspect('business_commands', seen[0].ctx.command_id));
  assert.equal(doc?.record_id, 'owner-1');
  h.uninstall();
  await k.close();
});

test('project.list: Projektion hinter der Quittung → retryable INCOMPLETE (kein stiller Teilstand)', async () => {
  const k = await mk();
  k.sim.onCommand('ctox.workjet.project.list', () => ({ ok: true, collection: 'workjet_projects', owner_user_id: 'owner-1', count: 2, truncated: false, project_ids: ['p-1', 'p-2'] }));
  await k.sim.push('workjet_projects', [projectRow('p-1')]);
  const b = createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) }));
  await assert.rejects(b.workjetProjectControl({ action: 'project.list' }), (e) => e.code === 'WORKJET_PROJECT_LIST_INCOMPLETE' && e.retryable === true);
  await k.close();
});

test('project.worker.add über kernel.command: Quittung korreliert, Vertrag erfüllt', async () => {
  const k = await mk();
  const seen = [];
  k.sim.onCommand('ctox.workjet.project.worker.add', (payload, ctx) => {
    seen.push({ payload, ctx });
    return { ok: true, contract: 'workjet-project-chats.v1', first_chat_id: 'workjet_private_abc_1' };
  });
  const b = createWorkjetBridges(createV2Compat({ kernel: adaptKernel(k) }));
  const request = { action: 'project.worker.add', commandId: 'cmd-1', projectId: 'p-1', workerProfileId: 'w-1', createdAt: '2026-10-16T08:00:00.000Z' };
  const res = await b.workjetProjectControl(request);
  assert.deepEqual(res, { action: 'project.worker.add', commandId: 'cmd-1', projectId: 'p-1', workerProfileId: 'w-1', chatId: 'workjet_private_abc_1' });
  assert.deepEqual(checkResponse('project.worker.add', res), []);
  assert.deepEqual(seen[0].payload, { project_id: 'p-1', worker_profile_id: 'w-1' });
  const doc = docOf(await k.sim.inspect('business_commands', seen[0].ctx.command_id));
  assert.equal(doc.type, 'ctox.workjet.project.worker.add');
  assert.equal(doc.record_id, 'p-1');
  // Kernfehler (failed) → V2 wirft seine eigene Quittungsfehlermeldung, kein Absturz
  await k.sim.failCommand('ctox.workjet.project.worker.add', 'forbidden: policy');
  // WJB-R (E26 idempotency_key): dieselbe commandId wäre dieselbe Operation (Kern liefert den vorhandenen Befehl) → neue ID
  await assert.rejects(b.workjetProjectControl({ ...request, commandId: 'cmd-2' }), /uncorrelated or unsuccessful receipt/);
  await k.close();
});

test('E25: fehlende Kernroute (unknown_command) → unsupported; requestNative = Befehl gleichen Namens mit Zwischenständen', async () => {
  const k = await mk();
  const compat = createV2Compat({ kernel: adaptKernel(k) });
  const b = createWorkjetBridges(compat);
  await k.sim.failCommand('ctox.workjet.project.list', 'unknown_command: ctox.workjet.project.list');
  await assert.rejects(b.workjetProjectControl({ action: 'project.list' }), (e) => isUnsupported(e) && /unsupported .*action/i.test(e.message));
  k.sim.onCommand('ctox.workjet.speech.settings.v1', (payload) => ({ ok: true, echo: payload }));
  assert.deepEqual(await compat.state.sync.requestNative('ctox.workjet.speech.settings.v1', { action: 'read' }), { ok: true, echo: { action: 'read' } });
  await k.sim.failCommand('ctox.workjet.calendar.read.v1', 'unknown_command: ctox.workjet.calendar.read.v1');
  await assert.rejects(compat.state.sync.requestNative('ctox.workjet.calendar.read.v1', { action: 'accounts' }), isUnsupported);
  assert.deepEqual(compat.nativeCalls, ['ctox.workjet.speech.settings.v1', 'ctox.workjet.calendar.read.v1']);
  await k.close();
});

test('v2-compat wie V2: record_id null wird nicht gesendet, Quittung trägt transport, Nativfehler roh mit code', async () => {
  const k = await mk();
  const compat = createV2Compat({ kernel: adaptKernel(k) });
  const seen = [];
  k.sim.onCommand('ctox.workjet.probe', (payload, ctx) => { seen.push(ctx.command_id); return { ok: true }; });
  const receipt = await compat.state.commandBus.dispatch({ command_type: 'ctox.workjet.probe', record_id: null, payload: { a: 1 } }, { until: 'terminal' }); // v4-allow: debt L-42
  assert.equal(receipt.transport, 'rxdb-command-bus');
  assert.equal(receipt.ok, true);
  assert.equal(docOf(await k.sim.inspect('business_commands', seen[0])).record_id ?? null, null, 'kein Text "null"');
  await k.sim.failCommand('ctox.workjet.device.v1', 'forbidden: workjet device management is not allowed');
  await assert.rejects(compat.state.sync.requestNative('ctox.workjet.device.v1', { action: 'binding.list' }), // v4-allow: debt L-42
    (e) => e.message === 'workjet device management is not allowed' && e.code === 'forbidden');
  await k.close();
});

test('Kernel ohne App-API-Datenteil (Rahmen-Attrappe): Brücke stürzt nicht, Aktionen melden typisierte Fehler', async () => {
  const fake = { apps: { get: () => [], subscribe: (cb) => { cb([]); return () => {}; } }, activation: { onChange: () => () => {} }, identity: { instance_id: 'lab', user_id: 'lab' } };
  const target = {};
  const h = installWorkjetBridges({ kernel: adaptKernel(fake), target });
  await assert.rejects(target.workjetProjectControl({ action: 'project.list' }));
  h.uninstall();
});

test('Vertragsabgleich: jede freigeschaltete Aktion hat einen Prüfer und steht in der Inventarliste', () => {
  const all = new Set(Object.keys(INVENTORY).flatMap(inventoryList));
  for (const action of Object.keys(PROJECT_ACTIONS)) {
    assert.ok(CHECKERS[action], `Prüfer für ${action}`);
    assert.ok(all.has(action), `${action} in INVENTORY`);
  }
  // Gemessene Liste (workjet-bridge-actions.md): Projekt 60, Rechner 6, Sitzung 6, Gerät 4 = 76 („rund 80“),
  // dazu Gruppe E (Workjet #784, Kern #875): Kalender verbinden start/complete = 78.
  assert.equal(all.size, 78, `Inventar ${all.size} Aktionen`);
});
