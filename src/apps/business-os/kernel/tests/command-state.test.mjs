// K-1c Punkt 10: KEEP-CRITICAL-Invarianten von shared/command-lifecycle.js auf dem V4-Befehlsdokument.
// Quelle der Wahrheit bleibt src/core/rxdb/tests/fixtures/business-command-lifecycle-v2.json (nur gelesen).
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createStubKernel } from '../testkit.mjs';
import {
  COMMAND_TRANSITIONS, COMMAND_IMMUTABLE_FIELDS, V2_TO_V4_PHASE, V2_TERMINAL_TO_V4, checkCommandTransition,
} from '../command-state.mjs';

const FIXTURE = JSON.parse(readFileSync(fileURLToPath(new URL('../../../../core/rxdb/tests/fixtures/business-command-lifecycle-v2.json', import.meta.url)), 'utf8'));
let n = 0;
async function mk(t, o = {}) {
  const k = createStubKernel({ locks: testLocks, instanceId: `cs${process.pid}-${++n}`, userId: 'u1', latencyMs: 2, ...o });
  t.after(() => k.close());
  await k.ready;
  return k;
}
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 4000) { const t0 = Date.now(); while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error('until: Zeitüberschreitung'); await tick(5); } }
const cmd = (status, extra = {}) => ({ id: 'c1', type: 'crm.do', app_id: 'crm', payload: { a: 1 }, created_at: '2026-10-15T00:00:00Z', actor_id: 'u1', status, ...extra });

test('Projektion: V4-Übergänge = Fixture-Übergänge projiziert (Fixture bleibt Quelle)', () => {
  const v2 = FIXTURE.allowedExecutionTransitions;
  assert.deepEqual(Object.keys(V2_TO_V4_PHASE).sort(), FIXTURE.executionPhases.filter((p) => p !== 'terminal').sort());
  assert.deepEqual(Object.keys(V2_TERMINAL_TO_V4).sort(), [...FIXTURE.terminalOutcomeStatuses].sort());
  const terminal = Object.values(V2_TERMINAL_TO_V4);
  const proj = {};
  for (const [from, tos] of Object.entries(v2)) {
    for (const a of from === 'terminal' ? terminal : [V2_TO_V4_PHASE[from]]) {
      for (const to of tos) for (const b of to === 'terminal' ? (from === 'terminal' ? [a] : terminal) : [V2_TO_V4_PHASE[to]]) (proj[a] ??= new Set()).add(b);
    }
  }
  proj.queued.add('rejected'); proj.rejected = new Set(['rejected']); // V4-eigen: write_result rejected nur aus queued
  assert.deepEqual(Object.fromEntries(Object.entries(COMMAND_TRANSITIONS).map(([k, v]) => [k, [...v].sort()])), Object.fromEntries(Object.entries(proj).map(([k, v]) => [k, [...v].sort()])));
  // unveränderliche Absicht: jedes v2-Feld hat sein V4-Gegenstück
  const map = { command_id: 'id', idempotency_key: 'id', command_type: 'type', module: 'app_id', payload: 'payload', record_id: 'record_id', client_context: 'client_context', created_at_ms: 'created_at' };
  for (const f of FIXTURE.immutableIntentFields) if (f !== 'payload_hash') assert.ok(COMMAND_IMMUTABLE_FIELDS.includes(map[f]), `${f} → ${map[f]}`);
});

// 1:1 zu shared/command-lifecycle.test.mjs „lifecycle accepts review and bounded rework transitions“
test('Lebenszyklus: Review und begrenzte Nacharbeit (running → running → queued(retry) → queued) sind zulässig', () => {
  assert.equal(checkCommandTransition(cmd('running'), cmd('running', { progress: 0.5 })), null);
  assert.equal(checkCommandTransition(cmd('running'), cmd('queued')), null);
  assert.equal(checkCommandTransition(cmd('queued'), cmd('queued')), null);
  assert.equal(checkCommandTransition(cmd('done', { result: { ok: 1 } }), cmd('done', { result: { ok: 1 } })), null, 'erneute Zustellung idempotent');
});

// 1:1 zu „lifecycle rejects terminal regression and immutable payload changes“ (KEEP-CRITICAL, critical-tests.json)
test('Lebenszyklus: kein Rückschritt aus dem Terminalzustand, keine geänderte Absicht, Ergebnis unveränderlich', () => {
  for (const t of ['done', 'failed', 'cancelled', 'rejected']) {
    assert.equal(checkCommandTransition(cmd(t), cmd('running'))?.code, 'invalid_transition', `${t} → running`);
    for (const u of ['done', 'failed', 'cancelled', 'rejected']) if (u !== t) assert.equal(checkCommandTransition(cmd(t), cmd(u))?.code, 'invalid_transition', `${t} → ${u}`);
  }
  assert.equal(checkCommandTransition(cmd('done', { result: 1 }), cmd('done', { result: 2 }))?.code, 'invalid_transition');
  assert.equal(checkCommandTransition(cmd('running'), cmd('accepted'))?.code, 'invalid_transition', 'kein Rückschritt running → accepted');
  assert.equal(checkCommandTransition(cmd('accepted'), cmd('queued', { payload: { a: 2 } }))?.code, 'idempotency_conflict');
  assert.equal(checkCommandTransition(cmd('queued'), cmd('queued', { type: 'crm.other' }))?.code, 'idempotency_conflict');
});

// zu „lifecycle requires terminal phase and status to agree“: V4 hat EIN Statusfeld; unbekannte Zustände sind ungültig
test('Lebenszyklus: unbekannter Zustand ist ungültig', () => {
  assert.equal(checkCommandTransition(null, cmd('terminal'))?.code, 'invalid_transition');
  assert.equal(checkCommandTransition(cmd('running'), cmd('completed'))?.code, 'invalid_transition');
});

test('Kernel: Zustellung eines Rückschritts wird verworfen – Terminalzustand genau einmal, Zustandszone meldet es', async (t) => {
  const k = await mk(t);
  const api = k.appApi('crm', '1');
  const seen = [];
  const live = api.get('business_commands', 'c1');
  live.subscribe((d) => { if (d) seen.push(d.status); });
  const page = (status, seq, extra = {}) => k.receivePage({ collection: 'business_commands', epoch: '1', scope_hash: 'local', gen: 1, cursor_to: String(seq), head_seq: String(seq), complete: true,
    docs: [{ collection: 'business_commands', id: 'c1', seq: String(seq), rev: `${seq}-x`, data: cmd(status, extra) }] });
  await page('running', 1001);
  await page('done', 1002, { result: { ok: true } });
  await page('done', 1002, { result: { ok: true } }); // gleiche Zustellung erneut
  await page('running', 1003); // Rückschritt mit neuerer seq
  await page('done', 1004, { result: { ok: false } }); // anderes Ergebnis nach Abschluss
  await tick(10);
  assert.deepEqual(live.get(), cmd('done', { result: { ok: true } }));
  assert.deepEqual(seen.filter((s) => s === 'done').length, 1, 'genau ein Übergang nach done');
  assert.ok(k.status.forApp('kernel').get().some((e) => e.code === 'command.invalid_transition'));
});

// KEEP-CRITICAL: doppelte Befehls-ID darf keine geänderte Absicht oder Terminal-Regression zulassen.
test('Core: doppelte Befehls-id mit geänderter Absicht → idempotency_conflict, gleiche Absicht → kein Rückschritt', async (t) => {
  const k = await mk(t);
  k.sim.onCommand('crm.do', async () => ({ ok: true }));
  const api = k.appApi('crm', '1');
  const h = api.command('crm.do', { a: 1 });
  await until(() => h.status.get().status === 'done');
  const core = await k.sim.coreRows('business_commands');
  const doc = core.find((d) => d.id === h.id);
  // gleiche id erneut (wie eine zweite Zustellung des Clients mit neuer op): Absicht gleich → angenommen, bleibt done
  const again = await k.sim.coreWrite({ op_id: `${h.id}-dup`, collection: 'business_commands', id: h.id, op: 'put', body: { ...doc, status: 'queued', result: undefined }, base_rev: null });
  assert.equal(again.status, 'accepted');
  const changed = await k.sim.coreWrite({ op_id: `${h.id}-chg`, collection: 'business_commands', id: h.id, op: 'put', body: { ...doc, status: 'queued', payload: { a: 2 } }, base_rev: null });
  assert.deepEqual([changed.status, changed.reason], ['rejected', 'idempotency_conflict']);
  await k.sim.push('business_commands', [{ ...doc, status: 'running' }]); // später Zustandsschreiber im Core: Rückschritt
  await tick(20);
  const after = (await k.sim.coreRows('business_commands')).find((d) => d.id === h.id);
  assert.deepEqual([after.status, after.payload, after.result], ['done', { a: 1 }, { ok: true }]);
  assert.equal(h.status.get().status, 'done');
});
