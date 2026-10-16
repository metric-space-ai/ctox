// E32 – reine Leseaktionen als lokale Kernel-Abfrage (Bericht ../E32.md).
// v4-Proben gegen den echten Kern-Stub (Pin 637caf6 bzw. CTOX_APP_API_STUB), je umgestellte Aktion:
//   1. Zeilen in die exportierte Collection schreiben (k.sim.push), Aktion aufrufen.
//   2. Antwort = heutige Antwort für dieselben Daten. Referenz ist der V2-Rumpf am Pin e35a116ae (app.js, per git show)
//      über DENSELBEN Kernel und dieselbe v2-compat-Lesesicht, mit Kern-Befehl (die Quittung verwirft V2). Rust-Fixtures
//      für computer.list/session.list gibt es nicht (src/core/rxdb/tests/fixtures/); zusätzlich prüft die Probe die
//      Kern-Felder, die der Rust-Kern heute liefert (computer.list: count/truncated, store_workjet_computers.rs:159-193;
//      session.list: sessions in Kern-Reihenfolge, store_workjet_sessions.rs:870-905).
//   3. 0 Kern-Befehle (gezählt an api.command), Laufzeit < 50 ms (Median und Maximum warmer Aufrufe).
//   4. Lesesicht nicht bereit/nicht vorhanden → bisherige „nicht unterstützt“-Form der Brücke, kein Befehl.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import vm from 'node:vm';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { adaptKernel } from '../../kernel-adapter.js';
import { createWorkjetBridges } from '../index.js';
import { createV2Compat, isUnsupported } from '../v2-compat.js';
import { checkResponse } from './contract-checkers.mjs';
import { V2_PIN } from './v2-oracle.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: HERE, encoding: 'utf8' }).trim();
const APP_V2 = execFileSync('git', ['show', `${V2_PIN}:src/apps/business-os/app.js`], { cwd: REPO, encoding: 'utf8', maxBuffer: 64 << 20 });
const STUB_DIR = process.env.CTOX_APP_API_STUB || path.join(os.homedir(), '.local/state/shell-v4/stub-pins/b6624eb');
const { createStubKernel } = await import(pathToFileURL(path.join(STUB_DIR, 'ctox-app-api-stub.mjs')).href);
const OWNER = 'owner-1';
const BUDGET_MS = 50;
const json = (v) => JSON.parse(JSON.stringify(v));

// V2-Abschnitte, wie die V2-Tests sie schneiden (workjet-computer-control.test.mjs:6-10, workjet-session-control.test.mjs:6-10).
const slice = (from, to) => {
  const start = APP_V2.indexOf(from);
  const end = APP_V2.indexOf(to, start);
  assert.ok(start > 0 && end > start, `V2-Abschnitt ${from}`);
  return APP_V2.slice(start, end);
};
const V2_COMPUTER = slice('const WORKJET_COMPUTER_CONTROL_MAX_RESULTS', 'const WORKJET_PROJECT_CONTROL_MAX_RESULTS');
const V2_SESSION = slice('const WORKJET_SESSION_CONTROL_MAX_RESULTS', 'function isStaleDataPlaneGeneration');

let n = 0;
async function setup({ latencyMs = 2, api: wrapApi = null } = {}) {
  const k = createStubKernel({ instanceId: `wjb-e32-${process.pid}-${++n}`, userId: OWNER, latencyMs, storage: 'memory',
    actor: { id: OWNER, display_name: 'Owner', role: 'admin', permissions: [] } });
  await k.ready;
  const adapted = adaptKernel(k);
  const commands = [];
  const counting = Object.create(adapted, { appApi: { value(appId, ver) {
    const api = adapted.appApi(appId, ver);
    const wrapped = { ...api, command(type, ...rest) { commands.push(type); return api.command(type, ...rest); } };
    return wrapApi ? wrapApi(wrapped) : wrapped;
  } } });
  const compat = createV2Compat({ kernel: counting });
  return { k, compat, commands, b: createWorkjetBridges(compat), close: async () => { compat.dispose(); await k.close(); } };
}

// Heutige Antwort: V2-Rumpf über dieselbe v2-compat-Lesesicht; der Befehl wird wie heute gesendet (Kern antwortet ok,
// die Quittung verwirft V2). Liefert Antwort und die gesendeten Befehlstypen.
async function v2Reference(compat, source, fnName, request) {
  const sent = [];
  const state = {
    session: compat.state.session,
    db: compat.state.db,
    sync: compat.state.sync,
    commandBus: { async dispatch(command) { sent.push(command.command_type); return { ok: true, status: 'completed', command_id: command.command_id, result: { ok: true } }; } },
  };
  const ctx = { state, actorContext: compat.actorContext, newId: compat.newId, waitForSyncBridgeReady: async () => {},
    window: { setTimeout }, setTimeout, clearTimeout };
  vm.runInNewContext(`${source}\nglobalThis.__fn = ${fnName};`, ctx);
  return { response: json(await ctx.__fn(json(request))), sent };
}

async function timeWarm(fn, runs = 20) {
  const times = [];
  for (let i = 0; i < runs; i += 1) {
    const t = performance.now();
    await fn();
    times.push(performance.now() - t);
  }
  times.sort((a, b) => a - b);
  return { median: times[Math.floor(runs / 2)], max: times[runs - 1] };
}

// ---- computer.list -------------------------------------------------------------------------------------------------
const computerRow = (id, extra = {}) => ({ id, display_name: `Rechner ${id}`, hosting_mode: 'workstation', status: 'assigned',
  capabilities: ['codex', 'claude'], capability_config: [{ kind: 'gpu', model: 'A4500', vram_gib: 20 }], agentless: false,
  owner_user_id: OWNER, self_hosted_colocation: false, private_key: 'nie-projizieren', ...extra });
const COMPUTERS = [
  computerRow('gpu3'),
  computerRow('mac', { display_name: 'Ärger-Mac', hosting_mode: 'self_hosted', self_hosted_colocation: true, capability_config: undefined, agentless: undefined }),
  computerRow('nas', { display_name: 'alpha NAS', agentless: true }),
  computerRow('foreign', { owner_user_id: 'other-owner' }),
  computerRow('old', { status: 'unassigned' }),
  computerRow('gone', { is_deleted: true }),
];
// Rust-Kern heute (store_workjet_computers.rs:166-192): Besitzer, status assigned, nicht is_deleted, Kappung 100.
const rustComputerCount = (rows) => rows.filter((r) => r.owner_user_id === OWNER && r.status === 'assigned' && r.is_deleted !== true).length;

test('[E32] computer.list: lokale Kernel-Abfrage = heutige Antwort (V2-Rumpf mit Befehl) für dieselben Zeilen, 0 Befehle, < 50 ms', async (t) => {
  const s = await setup();
  try {
    await s.k.sim.push('workjet_computers', COMPUTERS);
    for (const request of [{ action: 'computer.list' }, { action: 'computer.list', includeOperationalDetails: true }]) {
      const before = s.commands.length;
      const v4 = json(await s.b.workjetComputerControl(request));
      assert.equal(s.commands.length - before, 0, 'E32: kein Kern-Befehl');
      const ref = await v2Reference(s.compat, V2_COMPUTER, 'workjetComputerControl', request);
      assert.deepEqual(ref.sent, ['ctox.workjet.computer.list'], 'Referenz lief über den heutigen Befehlsweg');
      assert.deepEqual(v4, ref.response, JSON.stringify(request));
      assert.deepEqual(checkResponse('computer.list', v4), []);
      assert.equal(v4.computers.length, rustComputerCount(COMPUTERS), 'Kern-Feld count (heute vom Kern)');
      assert.ok(v4.computers.every((c) => !('private_key' in c)));
    }
    const warm = await timeWarm(() => s.b.workjetComputerControl({ action: 'computer.list', includeOperationalDetails: true }));
    t.diagnostic(`computer.list warm: Median ${warm.median.toFixed(2)} ms, Max ${warm.max.toFixed(2)} ms`);
    assert.ok(warm.max < BUDGET_MS, `Laufzeit ${warm.max.toFixed(2)} ms ≥ ${BUDGET_MS} ms`);
    assert.equal(s.commands.length, 0, 'E32: über alle Aufrufe kein Kern-Befehl');
  } finally { await s.close(); }
});

test('[E32] computer.list folgt der lokalen Sicht: neue/entfernte Zuweisung ohne Befehl sichtbar', async () => {
  const s = await setup();
  try {
    await s.k.sim.push('workjet_computers', [computerRow('gpu3')]);
    assert.deepEqual(json(await s.b.workjetComputerControl({ action: 'computer.list' })).computers.map((c) => c.id), ['gpu3']);
    await s.k.sim.push('workjet_computers', [computerRow('gpu4', { display_name: 'A-gpu4' }), computerRow('gpu3', { status: 'unassigned' })]);
    assert.deepEqual(json(await s.b.workjetComputerControl({ action: 'computer.list' })).computers.map((c) => c.id), ['gpu4']);
    assert.equal(s.commands.length, 0);
  } finally { await s.close(); }
});

// ---- session.list --------------------------------------------------------------------------------------------------
const sessionRow = (id, extra = {}) => ({ id, project_id: 'project-1', working_copy_id: `copy-${id}`, computer_id: 'computer-1',
  thread_id: 'thread-1', coding_session_id: null, run_status: 'running', fence_epoch: 0, owner_user_id: OWNER,
  created_at_ms: 1_699_999_000_000, updated_at_ms: 1_700_000_000_000, is_deleted: false, ...extra });
const SESSIONS = [
  sessionRow('s-b'),
  sessionRow('s-a'), // gleicher updated_at_ms wie s-b → id aufsteigend
  sessionRow('s-new', { updated_at_ms: 1_700_000_000_500, active_transfer_id: 'transfer-1', fence_epoch: 2 }),
  sessionRow('s-old', { updated_at_ms: 1_600_000_000_000, created_at_ms: undefined, thread_id: undefined }),
  sessionRow('s-foreign', { owner_user_id: 'other-owner' }),
  sessionRow('s-deleted', { is_deleted: true }),
];
// Rust-Kern heute (store_workjet_sessions.rs:877-904): Besitzer, nicht is_deleted, updated_at_ms absteigend, dann id.
const rustSessionOrder = (rows) => rows.filter((r) => r.owner_user_id === OWNER && r.is_deleted !== true)
  .sort((l, r) => (r.updated_at_ms - l.updated_at_ms) || (l.id < r.id ? -1 : l.id > r.id ? 1 : 0)).slice(0, 100).map((r) => r.id);

test('[E32] session.list: lokale Kernel-Abfrage = heutige Antwort (V2-Rumpf mit Befehl) und Kern-Reihenfolge, 0 Befehle, < 50 ms', async (t) => {
  const s = await setup();
  try {
    await s.k.sim.push('workjet_sessions', SESSIONS);
    const v4 = json(await s.b.workjetSessionControl({ action: 'session.list' }));
    assert.equal(s.commands.length, 0, 'E32: kein Kern-Befehl');
    const ref = await v2Reference(s.compat, V2_SESSION, 'workjetSessionControl', { action: 'session.list' });
    assert.deepEqual(ref.sent, ['ctox.workjet.session.list'], 'Referenz lief über den heutigen Befehlsweg');
    assert.deepEqual(v4, ref.response);
    assert.deepEqual(checkResponse('session.list', v4), []);
    assert.deepEqual(v4.sessions.map((x) => x.id), rustSessionOrder(SESSIONS), 'Reihenfolge und Filter wie Kern-Feld sessions');
    const warm = await timeWarm(() => s.b.workjetSessionControl({ action: 'session.list' }));
    t.diagnostic(`session.list warm: Median ${warm.median.toFixed(2)} ms, Max ${warm.max.toFixed(2)} ms`);
    assert.ok(warm.max < BUDGET_MS, `Laufzeit ${warm.max.toFixed(2)} ms ≥ ${BUDGET_MS} ms`);
    assert.equal(s.commands.length, 0);
  } finally { await s.close(); }
});

test('[E32] Laufzeit vorher/nachher bei Kernlatenz 50 ms: Befehlsweg (V2-Referenz über kernel.command) gegen lokale Abfrage', async (t) => {
  const s = await setup({ latencyMs: 50 });
  try {
    s.k.sim.onCommand('ctox.workjet.computer.list', () => ({ ok: true, collection: 'workjet_computers', count: 1, truncated: false }));
    await s.k.sim.push('workjet_computers', [computerRow('gpu3')]);
    // vorher: derselbe V2-Rumpf, aber der Befehl läuft wirklich über v2-compat dispatch → api.command (heutiger Weg)
    const viaCommand = async () => {
      const state = { session: s.compat.state.session, db: s.compat.state.db, sync: s.compat.state.sync, commandBus: s.compat.state.commandBus }; // v4-allow: debt L-42
      const ctx = { state, actorContext: s.compat.actorContext, newId: s.compat.newId, waitForSyncBridgeReady: async () => {}, window: { setTimeout }, setTimeout };
      vm.runInNewContext(`${V2_COMPUTER}\nglobalThis.__fn = workjetComputerControl;`, ctx);
      return ctx.__fn({ action: 'computer.list' });
    };
    await s.b.workjetComputerControl({ action: 'computer.list' });
    const before = await timeWarm(viaCommand, 5);
    const sentBefore = s.commands.length;
    const after = await timeWarm(() => s.b.workjetComputerControl({ action: 'computer.list' }), 5);
    t.diagnostic(`Kernlatenz 50 ms: vorher (Befehl) Median ${before.median.toFixed(1)} ms, nachher (lokal) Median ${after.median.toFixed(2)} ms`);
    assert.equal(sentBefore, 5, 'vorher: ein Befehl je Aufruf');
    assert.equal(s.commands.length, sentBefore, 'nachher: kein Befehl');
    assert.ok(before.median >= 50, 'Befehlsweg wartet mindestens eine Kernlatenz');
    assert.ok(after.max < BUDGET_MS);
  } finally { await s.close(); }
});

// ---- Lesesicht nicht bereit ----------------------------------------------------------------------------------------
test('[E32] Lesesicht im Fehlerzustand (z. B. nicht exportiert) → „nicht unterstützt“ je Brücke, kein Befehl', async () => {
  const s = await setup();
  try {
    await s.k.sim.failCollection('workjet_computers', 'forbidden', 'not exported');
    await s.k.sim.failCollection('workjet_sessions', 'forbidden', 'not exported');
    await assert.rejects(s.b.workjetComputerControl({ action: 'computer.list' }),
      (e) => isUnsupported(e) && e.message === 'workjet_computers collection is not registered.' && /PEER_UNAVAILABLE/.test(e.detail));
    await assert.rejects(s.b.workjetSessionControl({ action: 'session.list' }),
      (e) => isUnsupported(e) && e.message === 'Unsupported Workjet session control action: session.list');
    assert.equal(s.commands.length, 0);
  } finally { await s.close(); }
});

test('[E32] Kernel ohne query (Collection nicht lesbar) → „nicht unterstützt“, kein Befehl', async () => {
  const s = await setup({ api: (api) => ({ ...api, query: undefined }) });
  try {
    assert.ok(s.compat.gaps.has('query'));
    await assert.rejects(s.b.workjetComputerControl({ action: 'computer.list' }),
      (e) => isUnsupported(e) && e.message === 'workjet_computers collection is not registered.');
    await assert.rejects(s.b.workjetSessionControl({ action: 'session.list' }), (e) => isUnsupported(e));
    assert.equal(s.commands.length, 0);
  } finally { await s.close(); }
});

test('[E32] Lesesicht holt auf (catching_up): Aktion wartet auf live und liefert dann den vollständigen Stand', async () => {
  const s = await setup();
  try {
    await s.k.sim.push('workjet_sessions', [sessionRow('s-1')]);
    await s.k.sim.setBacklog(3, 'workjet_sessions');
    let settled = false;
    const pending = s.b.workjetSessionControl({ action: 'session.list' }).then((r) => { settled = true; return r; });
    await new Promise((r) => setTimeout(r, 60));
    assert.equal(settled, false, 'wartet, solange die Collection aufholt');
    await s.k.sim.push('workjet_sessions', [sessionRow('s-2', { updated_at_ms: 1_700_000_000_100 })]);
    await s.k.sim.setBacklog(0, 'workjet_sessions');
    assert.deepEqual(json(await pending).sessions.map((x) => x.id), ['s-2', 's-1']);
    assert.equal(s.commands.length, 0);
  } finally { await s.close(); }
});

// Nicht umgestellt (E32 (b), Belege in E32.md): diese Leseaktionen senden weiter genau ihren Kern-Befehl.
test('[E32] (b)-Aktionen bleiben Kern-Befehle: session.transfer.status sendet ctox.workjet.session.transfer.status', async () => {
  const s = await setup();
  try {
    s.k.sim.onCommand('ctox.workjet.session.transfer.status', () => ({ ok: true, transfer_id: 't-1', state: 'packing', retryable: false }));
    await s.b.workjetSessionControl({ action: 'session.transfer.status', commandId: 'st-1', transferId: 't-1' });
    assert.ok(s.commands.includes('ctox.workjet.session.transfer.status'));
  } finally { await s.close(); }
});
