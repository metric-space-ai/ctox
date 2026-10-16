import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle, {
  safeRelativeScriptPath, reconciliationTargetKey, containsSecretKey, safeRevision,
} from '../outbound.research.adapters.reconcile.js';
import handleDone from '../outbound.research.adapters.reconcile.done.js';

const SRC = 'outbound_lead_generation_sources';
const ADA = 'outbound_lead_generation_adapters';
const POL = 'outbound_lead_generation_research_policies';
const payload = (extra = {}) => ({
  instruction: 'Gleiche ab', title: 'Recherche-Adapter abgleichen', priority: 'low',
  configuration_digest: 'sha256:abc', policy_id: 'policy_1', field_keys: ['name'],
  sources: [{ id: 'xing.com', target_key: 'xing', adapter_id: 'adapter_leadgen_xing' }],
  writeback_contract: { source_collection: SRC, adapter_collection: ADA, policy_collection: POL },
  ...extra,
});
const data = () => ({
  [SRC]: { 'xing.com': { id: 'xing.com', requires_credential: true, credential_secret_name: 'XING_LOGIN', auth_status: 'credential_available', created_at_ms: 1 } },
  [ADA]: { adapter_leadgen_xing: { id: 'adapter_leadgen_xing', created_at_ms: 2, payload: { keep: 1 } } },
  [POL]: { policy_1: { id: 'policy_1' } },
});
const caps = { 'secret.scope': async () => ({ scope: 's' }), 'secret.exists': async ({ name }) => ({ exists: name === 'XING_LOGIN' }) };
const reply = (body) => ({ task_id: 'task_9', text: JSON.stringify({ schema: 'ctox.outbound.adapter_reconciliation.v1', configuration_digest: 'sha256:abc', ...body }) });
const ready = { source_id: 'xing.com', status: 'ready', scrape_status: 'test_passed', auth_status: 'required', adapter_revision: 'sha256:1', script_path: 'runtime/scraping/targets/xing/scripts/capture.js', test: { ok: true } };

test('phase 1 queues one agent task', async () => {
  const t = createTestCtx({ caps: { 'agent.task.enqueue': async () => ({ task_id: 'task_1' }) } });
  const r = await handle(cmd('outbound.research.adapters.reconcile', payload(), { id: 'cmd_r' }), t.ctx);
  assert.deepEqual(r, { ok: true, queued: true, task_id: 'task_1' });
  const call = t.calls.caps[0].args[0];
  // §16.30 flach: skill statt suggested_skill, Rückruf über eigenen Befehl statt reply_to_handler.
  assert.deepEqual(Object.keys(call).sort(), ['idempotency_key', 'instruction', 'on_complete', 'on_failed', 'payload', 'priority', 'skill', 'title']);
  assert.equal(call.skill, 'universal-scraping');
  assert.equal(call.instruction, 'Gleiche ab');
  assert.equal(call.idempotency_key, 'outbound.research.adapters.reconcile:cmd_r');
  assert.equal(call.on_complete, 'outbound.research.adapters.reconcile.done');
  assert.equal(call.on_failed, 'outbound.research.adapters.reconcile.done');
  assert.deepEqual(call.payload.command, { id: 'cmd_r', type: 'outbound.research.adapters.reconcile', payload: payload(), client_context: null });
  assert.equal(t.calls.writes.length, 0);
});

test('reply projects discovered sources, adapters and policy (ref :2093)', async () => {
  const t = createTestCtx({ now: 777, data: data(), caps });
  const r = await handle(cmd('outbound.research.adapters.reconcile', payload(), {
    id: 'cmd_r',
    agent_reply: reply({
      status: 'needs_attention',
      discovered_sources: [{ id: 'northdata.de', url: 'https://www.northdata.de/', reason: 'genannt' }],
      adapters: [ready, { source_id: 'northdata.de', status: 'auth_required', scrape_status: 'auth_required' }],
    }),
  }), t.ctx);
  assert.equal(r.status, 'needs_attention');
  assert.equal(r.task_id, 'task_9');
  const nd = t.doc(SRC, 'northdata.de');
  assert.equal(nd.target_key, 'northdata-de');
  assert.deepEqual(nd.countries, ['DE', 'AT', 'CH']);
  assert.deepEqual(nd.field_keys, ['name']);
  assert.equal(nd.payload.discovered_by_command_id, 'cmd_r');
  const ax = t.doc(ADA, 'adapter_leadgen_xing');
  assert.equal(ax.created_at_ms, 2);
  assert.equal(ax.payload.keep, 1);
  assert.equal(ax.payload.reconciliation_task_id, 'task_9');
  assert.equal(ax.auth_status, 'required');
  // stored credential wins over the worker's guess
  assert.equal(t.doc(SRC, 'xing.com').auth_status, 'credential_available');
  assert.equal(t.doc(ADA, 'adapter_leadgen_northdata-de').auth_status, 'auth_required');
  const pol = t.doc(POL, 'policy_1');
  assert.equal(pol.reconciliation_status, 'needs_attention');
  assert.equal(pol.reconciliation_error, 'one or more adapters require operator attention');
});

test('validation is all-or-nothing and failures are projected (ref :2602)', async () => {
  const cases = [
    [{ status: 'completed', adapters: [] }, /omitted sources: xing.com/],
    [{ status: 'completed', adapters: [{ ...ready, test: { ok: false } }] }, /passing test result/],
    [{ status: 'needs_attention', adapters: [ready] }, /inconsistent/],
    [{ status: 'completed', adapters: [{ ...ready, script_path: '../x.js' }] }, /invalid adapter script path/],
    [{ status: 'completed', adapters: [ready], meta: { session_token: 'x' } }, /credential values/],
    [{ status: 'completed', adapters: [ready], discovered_sources: [{ id: 'a.de', url: 'ftp://a.de/' }] }, /must be HTTP\(S\)/],
  ];
  for (const [body, pattern] of cases) {
    const t = createTestCtx({ now: 5, data: data(), caps });
    const error = await rejects(handle(cmd('outbound.research.adapters.reconcile', payload(), { agent_reply: reply(body) }), t.ctx));
    assert.match(error.message, pattern);
    assert.equal(t.doc(ADA, 'adapter_leadgen_xing').status, 'failed');
    assert.equal(t.doc(SRC, 'xing.com').adapter_status, 'needs_attention');
    assert.equal(t.doc(POL, 'policy_1').reconciliation_status, 'failed');
    assert.equal(t.doc(POL, 'policy_1').reconciliation_error, error.message);
  }
  const bad = createTestCtx({ data: data(), caps });
  const e1 = await rejects(handle(cmd('outbound.research.adapters.reconcile', payload(), { agent_reply: { task_id: 't', text: 'no json' } }), bad.ctx));
  assert.equal(e1.message, 'adapter reconciliation reply must be one strict JSON object');
  const e2 = await rejects(handle(cmd('outbound.research.adapters.reconcile', payload(), { agent_reply: reply({ configuration_digest: 'other', status: 'completed', adapters: [] }) }), bad.ctx));
  assert.match(e2.message, /does not match the requested configuration/);
});

test('queue failure marks records and fails with the queue error', async () => {
  const t = createTestCtx({ data: data(), caps });
  const error = await rejects(handle(cmd('outbound.research.adapters.reconcile', payload(), { id: 'c', agent_reply: { task_id: 'q', error: 'deadline_exceeded: 900s' } }), t.ctx));
  assert.equal(error.message, 'deadline_exceeded: 900s');
  assert.equal(t.doc(ADA, 'adapter_leadgen_xing').last_task_id, 'q');
  const scoped = createTestCtx({ data: data(), caps });
  await rejects(handle(cmd('outbound.research.adapters.reconcile', payload({ writeback_contract: { source_collection: 'other_sources', adapter_collection: ADA } }), { agent_reply: { task_id: 'q', error: 'x' } }), scoped.ctx));
  assert.equal(scoped.calls.writes.length, 0);
  assert.equal(scoped.calls.logs[0].level, 'warn');
});

test('identifier helpers follow Rust semantics', () => {
  assert.equal(safeRelativeScriptPath('runtime/x/./capture.mjs'), true);
  assert.equal(safeRelativeScriptPath('./capture.js'), false);
  assert.equal(safeRelativeScriptPath('/abs/capture.js'), false);
  assert.equal(safeRelativeScriptPath('a/.js'), false);
  assert.equal(safeRelativeScriptPath('a/b.js/'), true);
  assert.equal(safeRelativeScriptPath('a/b.ts'), false);
  assert.equal(reconciliationTargetKey('--Bundes-Anzeiger.de__'), 'bundes-anzeiger-de');
  assert.equal(reconciliationTargetKey('müller.de'), 'm--ller-de');
  assert.equal(containsSecretKey({ a: [{ 'Api-Key': 1 }] }), true);
  assert.equal(containsSecretKey({ credential_ref: 'ctox-secret://x' }), false);
  assert.equal(safeRevision('sha256:abc'), true);
});

test('Rückruf .done: completed → Phase 2 mit Ausgangsbefehl aus agent.task.get; Duplikat je task_id schreibt nichts', async () => {
  const original = { id: 'cmd_r', type: 'outbound.research.adapters.reconcile', payload: payload() };
  const body = JSON.parse(reply({ status: 'completed', adapters: [ready] }).text);
  const t = createTestCtx({ now: 777, data: data(), caps: { ...caps, 'agent.task.get': async ({ task_id }) => (task_id === 'task_9' ? { payload: { command: original } } : null) } });
  const r = await handleDone(cmd('outbound.research.adapters.reconcile.done', { task_id: 'task_9', status: 'completed', result: { text: JSON.stringify(body) } }), t.ctx);
  assert.equal(r.status, 'completed');
  assert.equal(t.doc(POL, 'policy_1').reconciliation_task_id, 'task_9');
  assert.equal(t.doc(ADA, 'adapter_leadgen_xing').payload.reconciliation_task_id, 'task_9');
  const writes = t.calls.writes.length;
  const again = await handleDone(cmd('outbound.research.adapters.reconcile.done', { task_id: 'task_9', status: 'completed', result: { text: JSON.stringify(body) } }), t.ctx);
  assert.deepEqual(again, { ok: true, task_id: 'task_9', duplicate: true });
  assert.equal(t.calls.writes.length, writes);
});

test('Rückruf .done: failed → Fehlerprojektion wie Warteschlangenfehler (ref :15080); fremde Aufgabe → not_found', async () => {
  const original = { id: 'cmd_r', type: 'outbound.research.adapters.reconcile', payload: payload() };
  const t = createTestCtx({ now: 777, data: data(), caps: { ...caps, 'agent.task.get': async () => ({ payload: { command: original } }) } });
  const e = await rejects(handleDone(cmd('outbound.research.adapters.reconcile.done', { task_id: 'q', status: 'failed', status_note: ' deadline_exceeded: 900s ' }), t.ctx));
  assert.equal(e.message, 'deadline_exceeded: 900s');
  assert.equal(t.doc(POL, 'policy_1').reconciliation_status, 'failed');
  assert.equal(t.doc(POL, 'policy_1').reconciliation_error, 'deadline_exceeded: 900s');
  const other = createTestCtx({ caps: { 'agent.task.get': async () => ({ payload: { command: { type: 'x' } } }) } });
  assert.equal((await rejects(handleDone(cmd('outbound.research.adapters.reconcile.done', { task_id: 'z', status: 'completed' }), other.ctx))).code, 'not_found');
});
