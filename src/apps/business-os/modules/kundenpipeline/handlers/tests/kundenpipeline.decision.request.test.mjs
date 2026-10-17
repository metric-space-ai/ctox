import assert from 'node:assert/strict';
import test from 'node:test';
import handle, { normalizeAgentDecisionRequest } from '../kundenpipeline.decision.request.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup } from './_fixtures.mjs';

// Payload aus dem Rust-Test decision_hub.rs request_payload :1480
const requestPayload = () => ({
  decision_key: 'choose-storage',
  title: 'Speicherstrategie festlegen',
  question: 'Welche Speicherstrategie soll umgesetzt werden?',
  context: 'Variante A ist robuster, Variante B schneller verfügbar.',
  options: [
    { id: 'robust', label: 'Robust', description: 'Mehr Vorarbeit' },
    { id: 'fast', label: 'Schnell', description: 'Früher verfügbar' },
  ],
  recommendation: 'robust',
  urgency: 'high',
  source: { authority: 'workjet', environment_id: 'env-1', thread_id: 'thread-1' },
  correlation: { turn_id: 'turn-1', idempotency_key: 'choose-storage' },
});

test('idempotent und konflikterkennend (Rust-Test agent_decision_request_is_idempotent_and_conflict_detecting)', async () => {
  const t = await setup();
  const c = cmd('kundenpipeline.decision.request', requestPayload(), { actor: { id: 'agent:orchestrator', role: 'agent' } });
  const first = await handle(c, t.ctx);
  const second = await handle(c, t.ctx);
  assert.equal(first.decision_id, second.decision_id);
  assert.equal(first.status, 'open');
  assert.match(first.decision_id, /^kpl-e-agent-[0-9a-f]{64}$/);
  const stored = t.doc('kundenpipeline_entscheidungen', first.decision_id);
  assert.equal(stored.typ, 'agent_escalation');
  assert.equal(stored.requested_by, 'agent:orchestrator');
  assert.equal(stored.owner_user_id, 'user_owner');
  assert.deepEqual(stored.participant_ids, ['user_owner', 'user_admin']);
  assert.deepEqual(stored.aktionen_json.map((a) => a.wert), ['robust', 'fast']);
  assert.equal('expires_at_ms' in stored, false);
  assert.equal(stored.zeilen_json.includes(''), true);

  const changed = requestPayload();
  changed.question = 'Eine andere Frage unter demselben Key';
  await rejects(handle(cmd('kundenpipeline.decision.request', changed), t.ctx), 'conflict', /decision_key_conflict/);
});

test('lehnt unbeschraenkte oder doppelte Optionen ab (Rust-Test agent_decision_request_rejects_unbounded_or_duplicate_options)', () => {
  const p = requestPayload();
  p.options = [{ id: 'only', label: 'Only' }];
  assert.throws(() => normalizeAgentDecisionRequest(p, NOW), (e) => e.code === 'invalid');
  p.options = [{ id: 'same', label: 'First' }, { id: 'same', label: 'Second' }];
  assert.throws(() => normalizeAgentDecisionRequest(p, NOW), (e) => e.code === 'invalid' && /unique/.test(e.message));
});

test('Validierung: urgency, recommendation, idempotency_key, expires_at_ms', () => {
  const cases = [
    [(p) => (p.urgency = 'low'), /urgency/],
    [(p) => (p.recommendation = 'x'), /recommendation/],
    [(p) => (p.correlation.idempotency_key = 'other'), /idempotency_key/],
    [(p) => (p.expires_at_ms = NOW - 1), /future/],
    [(p) => (p.expires_at_ms = NOW + 31 * 86_400_000), /30 days/],
    [(p) => delete p.source, /source/],
    [(p) => (p.title = 'x'.repeat(161)), /character limit/],
  ];
  for (const [mutate, pattern] of cases) {
    const p = requestPayload();
    mutate(p);
    assert.throws(() => normalizeAgentDecisionRequest(p, NOW), (e) => e.code === 'invalid' && pattern.test(e.message));
  }
});

test('Fingerprint folgt der Rust-Schluesselreihenfolge (serde_json preserve_order)', () => {
  const n = normalizeAgentDecisionRequest(requestPayload(), NOW);
  assert.deepEqual(Object.keys(n), [
    'decision_key', 'title', 'question', 'context', 'options', 'recommendation', 'urgency', 'expires_at_ms', 'source', 'correlation',
  ]);
  assert.equal(n.source.workjet_instance_id, null);
});

test('ohne Akteur gilt decision-hub-agent; Nutzerliste-Fehler -> leere Teilnehmer', async () => {
  const t = await setup({ caps: { 'identity.users.list': async () => { throw new Error('down'); } } });
  const c = cmd('kundenpipeline.decision.request', requestPayload(), { actor: {} });
  const r = await handle(c, t.ctx);
  const stored = t.doc('kundenpipeline_entscheidungen', r.decision_id);
  assert.equal(stored.requested_by, 'decision-hub-agent');
  assert.deepEqual(stored.participant_ids, []);
  assert.equal(stored.owner_user_id, '');
});

test('expires_at_ms wird gespeichert und als expired gemeldet', async () => {
  const t = await setup();
  const p = requestPayload();
  p.expires_at_ms = NOW + 60_000;
  const r = await handle(cmd('kundenpipeline.decision.request', p), t.ctx);
  assert.equal(r.status, 'open');
  t.setNow(NOW + 120_000);
  // Wie Rust: die Normalisierung prueft die Frist vor dem Idempotenz-Lookup.
  await rejects(handle(cmd('kundenpipeline.decision.request', p), t.ctx), 'invalid', /future/);
});
