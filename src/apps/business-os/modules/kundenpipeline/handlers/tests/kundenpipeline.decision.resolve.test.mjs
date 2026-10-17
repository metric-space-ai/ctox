import assert from 'node:assert/strict';
import test from 'node:test';
import request from '../kundenpipeline.decision.request.js';
import handle from '../kundenpipeline.decision.resolve.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup } from './_fixtures.mjs';

const payload = () => ({
  decision_key: 'choose-storage',
  title: 'Speicherstrategie festlegen',
  question: 'Welche Speicherstrategie?',
  options: [{ id: 'robust', label: 'Robust' }, { id: 'fast', label: 'Schnell' }],
  source: { environment_id: 'env-1', thread_id: 'thread-1' },
  correlation: { idempotency_key: 'choose-storage' },
});

async function created(t, extra = {}) {
  const r = await request(cmd('kundenpipeline.decision.request', { ...payload(), ...extra }), t.ctx);
  return r.decision_id;
}

test('nimmt genau eine deklarierte Option einmal an (Rust-Test agent_decision_resolution_accepts_one_declared_option_once)', async () => {
  const t = await setup();
  const id = await created(t);
  const body = { entscheidung_id: id, option_id: 'robust', comment: 'Bitte mit Migration umsetzen.', kanal: 'desktop' };
  const resolved = await handle(cmd('kundenpipeline.decision.resolve', body), t.ctx);
  assert.equal(resolved.status, 'resolved');
  assert.equal(resolved.resolution.option_id, 'robust');
  assert.equal(resolved.resolution.akteur, 'user_owner');
  const replay = await handle(cmd('kundenpipeline.decision.resolve', body), t.ctx);
  assert.equal(replay.resolution.option_id, 'robust');
  await rejects(
    handle(cmd('kundenpipeline.decision.resolve', { ...body, option_id: 'fast' }), t.ctx),
    'conflict',
    /decision_resolution_conflict/,
  );
});

test('unbekannte Option, unbekannte Entscheidung, falscher Typ, abgelaufen', async () => {
  const t = await setup({
    seed: { kundenpipeline_entscheidungen: [{ id: 'kpl-e-triage-x', typ: 'triage', status: 'offen', updated_at_ms: NOW }] },
  });
  const id = await created(t, { expires_at_ms: NOW + 1000 });
  await rejects(handle(cmd('kundenpipeline.decision.resolve', { entscheidung_id: id, option_id: 'nope' }), t.ctx), 'invalid', /unknown Decision Hub option/);
  await rejects(handle(cmd('kundenpipeline.decision.resolve', { entscheidung_id: 'missing', option_id: 'robust' }), t.ctx), 'not_found');
  await rejects(handle(cmd('kundenpipeline.decision.resolve', { entscheidung_id: 'kpl-e-triage-x', option_id: 'robust' }), t.ctx), 'invalid', /only accepts agent_escalation/);
  t.setNow(NOW + 2000);
  await rejects(handle(cmd('kundenpipeline.decision.resolve', { entscheidung_id: id, option_id: 'robust' }), t.ctx), 'expired');
  await rejects(handle(cmd('kundenpipeline.decision.resolve', { option_id: 'robust' }), t.ctx), 'invalid', /entscheidung_id is required/);
});
