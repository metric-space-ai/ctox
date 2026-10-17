import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { PROSPECT_RECORDS, scrapeCtx, seedTarget } from './_scrape.mjs';
import handle, { operationTimeoutMs } from '../outbound.research_source.reprobe.js';

const caps = {};
const ACCOUNTS = 'outbound_lead_generation_scrape_account_states';
const RUNS = 'outbound_lead_generation_scrape_runs';
const inactive = (extra = {}) => ({
  id: 'linkedin-people-v2', target_key: 'linkedin-people-v2', target_id: 'x', generation: 2, reason: 'Customer is not active',
  causal_run_id: 'r1', last_probe_run_id: 'r2', failed_probes: 2, next_probe_at_ms: 10_000_000, probe_lease_owner: null, probe_lease_until_ms: 0, updated_at_ms: 1, ...extra,
});

test('reprobe runs the unchanged script through process.run and clears the probed state (ref :4090, execute.rs:377)', async () => {
  const { t, procCalls } = scrapeCtx({ now: 1000, run: { exit: 0, stdout: PROSPECT_RECORDS } });
  await seedTarget(t.ctx, { target_key: 'linkedin-people-v2', script: 'module.exports = 1;\n' });
  await t.ctx.put(ACCOUNTS, inactive());
  const r = await handle(cmd('outbound.research_source.reprobe', { keep_script: true, target_key: ' linkedin-people-v2 ', probe_input: { company: 'ACME' }, operation_timeout_ms: 1500 }, { id: 'cmd_1' }), t.ctx);
  assert.equal(r.ok, true);
  assert.equal(r.keep_script, true);
  assert.equal(r.script_revision, 1);
  assert.equal(r.script_sha256_before, r.script_sha256_after);
  assert.equal(r.outcome.status, 'succeeded');
  assert.equal(procCalls.length, 1);
  assert.equal(procCalls[0].program, 'olg-scrape-js');
  assert.equal(procCalls[0].timeout_ms, 2000);
  const run = t.all(RUNS)[0];
  assert.equal(run.trigger_kind, 'account_reprobe');
  assert.equal(run.command_id, 'cmd_1');
  // authorized probe took the lease of generation 2 and the success deleted it (account_state.rs:318)
  assert.equal(t.doc(ACCOUNTS, 'linkedin-people-v2'), null);
});

test('INV-SCR3 reprobe still inactive → generation+1, failed_probes+1, 24 h backoff (account_state.rs:291)', async () => {
  const { t } = scrapeCtx({ now: 1000, run: { exit: 0, stdout: { records: [], failure_mode: 'blocked', detail: 'Customer is not active' } } });
  await seedTarget(t.ctx, { target_key: 'linkedin-people-v2' });
  await t.ctx.put(ACCOUNTS, inactive());
  const r = await handle(cmd('outbound.research_source.reprobe', { keep_script: true, target_key: 'linkedin-people-v2', probe_input: {} }), t.ctx);
  assert.equal(r.ok, false);
  assert.equal(r.outcome.status, 'provider_account_inactive');
  const s = t.doc(ACCOUNTS, 'linkedin-people-v2');
  assert.equal(s.generation, 3);
  assert.equal(s.failed_probes, 3);
  assert.equal(s.next_probe_at_ms, 1000 + 24 * 60 * 60 * 1000);
  assert.equal(s.last_probe_run_id, r.outcome.run_id);
  assert.equal(s.probe_lease_owner, null);
});

test('reprobe needs a registered unchanged script', async () => {
  const { t, procCalls } = scrapeCtx();
  const e1 = await rejects(handle(cmd('outbound.research_source.reprobe', { keep_script: true, target_key: 'nope', probe_input: {} }), t.ctx));
  assert.equal(e1.message, 'account reprobe target is not registered');
  await seedTarget(t.ctx, { target_key: 'bare', script: null });
  const e2 = await rejects(handle(cmd('outbound.research_source.reprobe', { keep_script: true, target_key: 'bare', probe_input: {} }), t.ctx));
  assert.equal(e2.message, 'account reprobe requires an existing unchanged registered script');
  assert.equal(procCalls.length, 0);
});

test('default timeout 90 s, bounds and integer check', async () => {
  assert.equal(operationTimeoutMs({}), 90000);
  assert.equal(operationTimeoutMs({ operation_timeout_ms: 300000 }), 300000);
  for (const bad of [999, 300001, -5, 1500.5, '2000', null]) {
    assert.throws(() => operationTimeoutMs({ operation_timeout_ms: bad }), /between 1000 and 300000/);
  }
});

test('rejections in Rust order', async () => {
  const t = createTestCtx({ caps });
  const run = (p) => rejects(handle(cmd('outbound.research_source.reprobe', p), t.ctx));
  assert.match((await run({ target_key: 'a', probe_input: {} })).message, /keep_script=true/);
  assert.match((await run({ keep_script: 'true', target_key: 'a', probe_input: {} })).message, /keep_script=true/);
  assert.match((await run({ keep_script: true, probe_input: {} })).message, /target_key is required/);
  assert.match((await run({ keep_script: true, target_key: 'a/b', probe_input: {} })).message, /invalid source reprobe target_key/);
  assert.match((await run({ keep_script: true, target_key: 'x'.repeat(129), probe_input: {} })).message, /invalid source reprobe target_key/);
  assert.match((await run({ keep_script: true, target_key: 'a', probe_input: [] })).message, /probe_input/);
  assert.match((await run({ keep_script: true, target_key: 'a', probe_input: {}, operation_timeout_ms: 5 })).message, /between/);
  assert.equal(t.calls.caps.length, 0);
});
