// K-1f: Coding Agent im Quelltext-Modus (§16.44, Vertrag befund-coding-panel.md / Core-PR #848) im Fake-Core simuliert.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../testkit.mjs';

let n = 0;
const MOUNT = 'export function mount(){ return { unmount(){} }; }';
const EDIT = { role: 'user', permissions: ['apps.source.view', 'apps.source.edit'] };
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 4000, what = 'Bedingung') { const t0 = Date.now(); while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error(`until: ${what}`); await tick(5); } }
// ein Core je Instanz, Nutzer mit eigenen DBs (K-1c Punkt 4)
async function users(t, list, o = {}) {
  const instanceId = `k1f-${process.pid}-${++n}`;
  const out = {};
  for (const [id, coreUser] of Object.entries(list)) {
    const k = createStubKernel({ locks: testLocks, instanceId, coreNamespace: instanceId, userId: id, actor: { role: 'admin', permissions: ['apps.source.view', 'apps.source.edit'] }, coreUser, latencyMs: 2, ...o });
    t.after(() => k.close());
    await k.ready;
    out[id] = k;
  }
  return out;
}
async function prepare(k) {
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT, 'lib/util.js': 'export const a = 1;\n' } });
  await k.source('crm').draft.write('lib/util.js', 'export const a = 1;\nexport const b = 2;\n');
  await k.sim.settle();
  return k.source('crm').draft.info();
}
const turnOf = async (k, commandId) => (await k.sim.coreRows('coding_turns')).find((x) => x.command_id === commandId);
const turnCmd = (k, payload, opts) => k.appApi('crm', '1').command('ctox.coding.turn', payload, opts);
const final = async (k, h) => { await until(() => ['done', 'failed', 'cancelled'].includes(h.status.get().status), 4000, 'Befehlsende'); return h.status.get(); };

test('§16.44: Turn schreibt eine neue Entwurfsrevision per CAS; aktive Version unberührt; Nachrichten erst am Ende; Rückruf höchstens einmal', async (t) => {
  const { u1 } = await users(t, { u1: EDIT }, { codingDelayMs: 80 });
  const info = await prepare(u1);
  assert.ok(info.rev, 'draft.info().rev = base_draft_rev');
  const h = turnCmd(u1, { app_id: 'crm', draft_id: 'u1/crm', base_draft_rev: info.rev, prompt: 'Bitte in lib/util.js einen Kommentar ergänzen', turn_id: 'turn-a' }, { on_complete: 'crm.coding.done', on_failed: 'crm.coding.failed' });
  assert.equal((await h.result).status, 'accepted');
  await until(async () => (await turnOf(u1, h.id))?.state === 'running', 4000, 'running');
  assert.deepEqual([(await turnOf(u1, h.id)).messages, (await turnOf(u1, h.id)).tool_events], [[], []], 'kein Zwischenstand');
  const st = await final(u1, h);
  assert.equal(st.status, 'done', JSON.stringify(st));
  await u1.sim.settle();
  const turn = await turnOf(u1, h.id);
  const after = await u1.source('crm').draft.info();
  assert.deepEqual([turn.id, turn.state, turn.changed_files, turn.draft_rev_after], ['turn-a', 'done', ['lib/util.js'], after.rev]);
  assert.notEqual(after.rev, info.rev);
  assert.match(after.files['lib/util.js'], /^export const a = 1;\nexport const b = 2;\n\/\/ coding-agent turn-a: Bitte in lib\/util\.js/);
  assert.deepEqual(turn.messages.map((m) => m.role), ['user', 'assistant']);
  assert.deepEqual(turn.tool_events.map((e) => [e.kind, e.path]), [['read', 'lib/util.js'], ['edit', 'lib/util.js']]);
  assert.equal(u1.apps.get()[0].active_ver, '1', 'aktive Version bleibt');
  assert.equal(await u1.source('crm').read('lib/util.js', '1'), 'export const a = 1;\n');
  await tick(30);
  const callbacks = (await u1.sim.coreRows('business_commands')).filter((c) => c.id === `coding-callback:${h.id}`);
  assert.deepEqual(callbacks.map((c) => [c.type, c.payload.turn_id, c.payload.status]), [['crm.coding.done', 'turn-a', 'done']]);
});

test('§16.44: CAS-Konflikt behält die Nutzeränderung; Patch als files-Datensatz coding-patch:<turn_id>; Befehl failed conflict', async (t) => {
  const { u1 } = await users(t, { u1: EDIT }, { codingDelayMs: 150 });
  const info = await prepare(u1);
  const h = turnCmd(u1, { app_id: 'crm', draft_id: 'u1/crm', base_draft_rev: info.rev, prompt: 'lib/util.js kommentieren', turn_id: 'turn-c' });
  await until(async () => (await turnOf(u1, h.id))?.state === 'running', 4000, 'running');
  await u1.source('crm').draft.write('lib/util.js', 'export const a = 42; // Nutzer\n'); // Nutzer editiert währenddessen
  await u1.sim.settle();
  const st = await final(u1, h);
  assert.deepEqual([st.status, st.error], ['failed', 'conflict: draft_changed']);
  await u1.sim.settle();
  assert.equal((await u1.source('crm').draft.info()).files['lib/util.js'], 'export const a = 42; // Nutzer\n', 'Nutzeränderung bleibt');
  const turn = await turnOf(u1, h.id);
  assert.deepEqual([turn.state, turn.error.code, turn.error.reason, turn.patch.file_id, turn.draft_rev_after], ['conflict', 'conflict', 'draft_changed', 'coding-patch:turn-c', null]);
  const file = (await u1.sim.coreRows('files')).find((f) => f.id === 'coding-patch:turn-c');
  assert.deepEqual([file.ref, file.owner_app, file.owner_user, file.mime], [turn.patch.ref, 'crm', 'u1', 'text/x-diff']);
  const patch = await (await u1.appApi('crm', '1').files.get(turn.patch.ref)).text();
  assert.match(patch, /^--- a\/lib\/util\.js\n\+\+\+ b\/lib\/util\.js\n@@ -1,2 \+1,3 @@\n export const a = 1;\n export const b = 2;\n\+\/\/ coding-agent turn-c: /); // 3 Kontextzeilen (hier 2 vorhanden)
});

test('§16.44: nur eigener Entwurf, Recht apps.source.edit, Payload und Basis geprüft – vor jeder Wirkung', async (t) => {
  const { u1, u2, u3 } = await users(t, { u1: EDIT, u2: EDIT, u3: { role: 'user', permissions: ['apps.source.view'] } });
  const info = await prepare(u1);
  const rej = async (k, payload) => { const r = await turnCmd(k, payload).result; return [r.status, r.code, r.reason]; };
  const ok = { app_id: 'crm', draft_id: 'u1/crm', base_draft_rev: info.rev, prompt: 'x' };
  assert.deepEqual(await rej(u2, ok), ['rejected', 'forbidden', 'draft_not_owned'], 'fremder Entwurf');
  assert.deepEqual(await rej(u3, ok), ['rejected', 'forbidden', 'apps.source.edit']);
  assert.deepEqual(await rej(u1, { ...ok, base_draft_rev: undefined }), ['rejected', 'invalid', 'payload_invalid:base_draft_rev']);
  assert.deepEqual(await rej(u1, { ...ok, draft_id: '' }), ['rejected', 'invalid', 'payload_invalid:draft_id']);
  assert.deepEqual(await rej(u1, { ...ok, extra: 1 }), ['rejected', 'invalid', 'payload_invalid:extra']);
  assert.deepEqual(await rej(u1, { ...ok, turn_id: 'kein leerzeichen' }), ['rejected', 'invalid', 'payload_invalid:turn_id']);
  assert.deepEqual(await rej(u1, { ...ok, draft_id: 'u1/fehlt', app_id: 'fehlt' }), ['rejected', 'not_found', 'draft_not_found']);
  assert.deepEqual(await rej(u1, { ...ok, base_draft_rev: '1-alt' }), ['rejected', 'conflict', 'stale_draft_rev']);
  assert.deepEqual(await u1.sim.coreRows('coding_turns'), [], 'keine Wirkung');
});

test('§16.44: zweiter Turn je Entwurf abgewiesen; ctox.coding.cancel bricht ab, ohne zu schreiben', async (t) => {
  const { u1 } = await users(t, { u1: EDIT }, { codingDelayMs: 150 });
  const info = await prepare(u1);
  const p = { app_id: 'crm', draft_id: 'u1/crm', base_draft_rev: info.rev, prompt: 'lib/util.js' };
  const h1 = turnCmd(u1, { ...p, turn_id: 'turn-1' }, { on_failed: 'crm.coding.failed' });
  const h2 = turnCmd(u1, { ...p, turn_id: 'turn-2' });
  assert.equal((await h1.result).status, 'accepted');
  const r2 = await h2.result;
  assert.deepEqual([r2.status, r2.code, r2.reason], ['rejected', 'conflict', 'turn_running']);
  await until(async () => (await turnOf(u1, h1.id))?.state === 'running', 4000, 'running');
  const c = u1.appApi('crm', '1').command('ctox.coding.cancel', { turn_id: 'turn-1' });
  assert.deepEqual(await final(u1, c), { status: 'done', progress: 1, result: { turn_id: 'turn-1', state: 'cancelled' } });
  const s1 = await final(u1, h1);
  assert.equal(s1.status, 'cancelled');
  await tick(200);
  await u1.sim.settle();
  assert.equal((await turnOf(u1, h1.id)).state, 'cancelled', 'Endzustand bleibt');
  const d = await u1.source('crm').draft.info();
  assert.deepEqual([d.rev, d.files['lib/util.js']], [info.rev, 'export const a = 1;\nexport const b = 2;\n'], 'nichts zurückgeschrieben');
  const again = await u1.appApi('crm', '1').command('ctox.coding.cancel', { turn_id: 'turn-1' }).result;
  assert.deepEqual([again.code, again.reason], ['conflict', 'turn_not_running']);
  assert.equal((await u1.sim.coreRows('business_commands')).filter((x) => x.id === `coding-callback:${h1.id}`).length, 1, 'on_failed genau einmal');
  assert.equal((await turnCmd(u1, { ...p, turn_id: 'turn-3' }).result).status, 'accepted', 'nach Abbruch wieder frei');
});
