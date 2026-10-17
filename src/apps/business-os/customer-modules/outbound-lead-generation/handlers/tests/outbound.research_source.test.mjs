import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { PROSPECT_RECORDS, scrapeCtx, seedTarget } from './_scrape.mjs';
import upsert from '../outbound.research_source.upsert.js';
import generate from '../outbound.research_source.generate_adapter.js';
import sourceTest from '../outbound.research_source.test.js';
import authAssist from '../outbound.research_source.auth_assist.js';
import {
  researchScrapeTestInput, swapRemove, rustDebugStrings, runtimeAppStarterCollectionName, failedTestStatus,
} from '../_lib/research-source-adapter.js';

const ADAPTER = {
  id: 'adapter_leadgen_xing', source_id: 'xing.com', label: 'XING', url: 'https://www.xing.com/',
  adapter_kind: 'scrape_target', target_key: 'xing', countries: ['DE', 'AT'], field_keys: ['name', 'Title'],
  enabled: true, requires_credential: false,
};
const baseCaps = (extra = {}) => ({
  'secret.scope': async () => ({ scope: 'tenant-a' }),
  'secret.exists': async ({ name }) => ({ exists: name === 'xing_login' }),
  ...extra,
});
const payload = (extra = {}) => ({ adapter_id: ADAPTER.id, source_id: ADAPTER.source_id, adapter: { ...ADAPTER }, ...extra });

test('upsert builds the adapter record and writes back into the app collection (ref :2776, :3037)', async () => {
  const t = createTestCtx({ now: 5000, caps: baseCaps() });
  const r = await upsert(cmd('outbound.research_source.upsert', payload({
    scrape_contract: { fields: ['name'] }, writeback: { collection: 'outbound_lead_generation_adapters' },
  }), { id: 'cmd_u' }), t.ctx);
  assert.equal(r.ok, true);
  assert.equal(r.command_effect, 'active');
  assert.equal(r.auth_assist, null);
  const rec = t.doc('outbound_research_adapters', ADAPTER.id);
  assert.equal(rec.status, 'active');
  assert.equal(rec.scrape_status, 'target_available');
  assert.equal(rec.auth_status, 'not_required');
  assert.equal(rec.campaign_id, '');
  assert.equal(rec.last_error, '');
  assert.equal(rec.created_at_ms, 5000);
  assert.deepEqual(rec.payload.credential_ref, { required: false, scope: 'tenant-a', secret_value_in_payload: false });
  assert.equal(rec.payload.last_command_type, 'outbound.research_source.upsert');
  assert.deepEqual(rec.payload.scrape_contract, { fields: ['name'] });
  assert.equal(rec.payload.scrape_registry_effect, undefined);
  const wb = t.doc('outbound_lead_generation_adapters', ADAPTER.id);
  assert.equal(wb.last_command_id, 'cmd_u');
  assert.deepEqual(t.calls.writes.map((w) => w.collection), ['outbound_research_adapters', 'outbound_lead_generation_adapters']);
});

test('existing record keeps created_at_ms; defaults and record_id fallback', async () => {
  const t = createTestCtx({
    now: 9000, caps: baseCaps(),
    data: { outbound_research_adapters: { a1: { id: 'a1', created_at_ms: 10, scrape_status: 'registered', custom: 'kept' } } },
  });
  const r = await upsert(cmd('outbound.research_source.upsert', { source_id: 'web.de' }, { record_id: ' a1 ' }), t.ctx);
  assert.equal(r.adapter_id, 'a1');
  const rec = t.doc('outbound_research_adapters', 'a1');
  assert.equal(rec.created_at_ms, 10);
  assert.equal(rec.custom, 'kept');
  assert.equal(rec.scrape_status, 'registered');
  assert.equal(rec.url, 'https://web.de/');
  assert.equal(rec.adapter_kind, 'custom_url');
  assert.deepEqual(rec.countries, []);
  assert.equal(rec.enabled, true);
});

test('validation and writeback scoping errors in Rust order', async () => {
  const t = createTestCtx({ caps: baseCaps() });
  assert.match((await rejects(upsert(cmd('outbound.research_source.upsert', {}), t.ctx))).message, /adapter_id is required/);
  assert.match((await rejects(upsert(cmd('outbound.research_source.upsert', { adapter_id: 'a' }), t.ctx))).message, /source_id is required/);
  const wb = (writeback) => rejects(upsert(cmd('outbound.research_source.upsert', payload({ writeback })), t.ctx));
  assert.match((await wb(null)).message, /collection is required/);
  assert.match((await wb({ collection: 'Bad-Name' })).message, /invalid adapter writeback collection/);
  assert.match((await wb({ collection: 'outbound_research_adapters' })).message, /scoped to the source module/);
  assert.match((await wb({ collection: 'outbound_lead_generation_adapters', record_id: 'other' })).message, /must match adapter_id/);
  assert.equal(runtimeAppStarterCollectionName(' Outbound-Lead  Generation '), 'outbound_lead_generation_records');
  assert.equal(runtimeAppStarterCollectionName('---'), 'agent_app_records');
});

test('auth_assist requests a browser session with the secret reference (ref :2964-3025)', async () => {
  const requests = [];
  const t = createTestCtx({
    now: 7000,
    caps: baseCaps({ 'browser.auth_assist.request': async (args) => { requests.push(args); return { request_id: 'req_1' }; } }),
  });
  const adapter = { ...ADAPTER, requires_credential: true, credential_secret_name: 'xing_login', auth_mode: 'browser_session' };
  const r = await authAssist(cmd('outbound.research_source.auth_assist', payload({ adapter, auth_assist: { mode: 'x' } }), {
    id: 'cmd_a', actor: { id: ' user_7 ', role: 'user' },
  }), t.ctx);
  assert.deepEqual(r.auth_assist, { request_id: 'req_1' });
  // CAP-3b #844 caps/browser.rs AuthAssistRequest (deny_unknown_fields): credential_ref nur ^ctox-secret://credentials/,
  // Eigentümer = Core-gebundener Akteur; owner_user_id/deterministic_for_task/accept_as_trusted_local entfallen.
  assert.equal(requests[0].credential_ref, 'ctox-secret://credentials/xing_login');
  assert.deepEqual(Object.keys(requests[0]).filter((k) => !['source_id', 'target_url_override', 'credential_ref', 'login_hint',
    'preferred_auth_assist', 'requesting_task_id', 'source_module', 'command_path'].includes(k)), []);
  assert.equal(requests[0].requesting_task_id, 'cmd_a');
  assert.equal(requests[0].target_url_override, 'https://www.xing.com/');
  assert.deepEqual(requests[0].preferred_auth_assist, { mode: 'x' });
  const rec = t.doc('outbound_research_adapters', ADAPTER.id);
  assert.equal(rec.auth_status, 'browser_session_requested');
  assert.equal(rec.payload.credential_ref.status, 'credential_available');
  assert.deepEqual(rec.payload.auth_assist, { request_id: 'req_1' });
  assert.equal(t.calls.writes.length, 2);
});

test('credential status: name missing / missing secret / auth_requested', async () => {
  const t = createTestCtx({ caps: baseCaps({ 'browser.auth_assist.request': async () => ({}) }) });
  await upsert(cmd('outbound.research_source.upsert', payload({ adapter: { ...ADAPTER, requires_credential: true } })), t.ctx);
  assert.equal(t.doc('outbound_research_adapters', ADAPTER.id).auth_status, 'credential_name_missing');
  await upsert(cmd('outbound.research_source.upsert', payload({ adapter: { ...ADAPTER, requires_credential: true, credential_secret_name: 'nope' } })), t.ctx);
  assert.equal(t.doc('outbound_research_adapters', ADAPTER.id).auth_status, 'credential_missing');
  const r = await authAssist(cmd('outbound.research_source.auth_assist', payload({ adapter: { ...ADAPTER, requires_credential: true, credential_secret_name: 'nope' } })), t.ctx);
  assert.equal(r.adapter.payload.credential_ref.status, 'auth_requested');
});

const TARGETS = 'outbound_lead_generation_scrape_targets';
const RUNS = 'outbound_lead_generation_scrape_runs';

test('generate_adapter registers an unknown target in the app registry and queues generation (ref :3933, :3759)', async () => {
  const queued = [];
  const { t } = scrapeCtx({ now: 100, caps: baseCaps({ 'agent.task.enqueue': async (req) => { queued.push(req); return { task_id: 'task_1' }; } }) });
  const r = await generate(cmd('outbound.research_source.generate_adapter', payload(), { id: 'cmd_g' }), t.ctx);
  const target = t.doc(TARGETS, 'xing');
  assert.equal(target.target_kind, 'prospect-research');
  assert.deepEqual(target.config.country_hints, ['DE', 'AT']);
  assert.equal(target.config.sources[0].source_key, 'primary');
  assert.equal(target.script_registered, false);
  const effect = r.adapter.payload.scrape_registry_effect;
  assert.equal(effect.registered_from, 'adapter_manifest');
  assert.equal(effect.target_manifest, `${TARGETS}/xing`);
  assert.equal(effect.generation_task.task_id, 'task_1');
  assert.equal(effect.generation_task.task_status, 'pending');
  assert.equal(r.adapter.status, 'generation_queued');
  assert.equal(r.adapter.scrape_status, 'generation_queued');
  assert.equal(r.adapter.test_ok, false);
  assert.equal(queued[0].priority, 'low');
  assert.equal(queued[0].skill, 'universal-scraping');
  assert.equal(queued[0].idempotency_key, 'outbound-research-adapter:xing:cmd_g');
  assert.equal(queued[0].payload.kind, 'scrape_repair');
  assert.match(queued[0].instruction, /Arbeite ausschliesslich im isolierten Target-Workspace outbound-lead-generation\/scrape\/targets\/xing;/);
  assert.match(queued[0].instruction, /`outbound\.scrape\.script\.register`/);
  assert.match(queued[0].instruction, /Melde input_required\.\n\ntarget_manifest:\n\{\}\n\nscrape_contract:\n\{\}\n$/);
  assert.equal(t.calls.caps.filter((c) => c.name.startsWith('scrape.')).length, 0);
});

test('generate_adapter: registered script → adapter_ready; blocked queue; registration failure', async () => {
  const { t } = scrapeCtx({ caps: baseCaps() });
  await seedTarget(t.ctx, { target_key: 'xing' });
  const r = await generate(cmd('outbound.research_source.generate_adapter', payload()), t.ctx);
  assert.equal(r.adapter.status, 'adapter_ready');
  assert.equal(r.adapter.scrape_status, 'registered');
  const blocked = scrapeCtx({ caps: baseCaps({ 'agent.task.enqueue': async () => ({ task_id: 't', status: 'blocked' }) }) });
  assert.equal((await generate(cmd('outbound.research_source.generate_adapter', payload()), blocked.t.ctx)).adapter.status, 'generation_blocked');
  const broken = scrapeCtx({ caps: baseCaps() });
  const get = broken.t.ctx.get;
  broken.t.ctx.get = async (collection, id) => { if (collection === TARGETS) throw new Error('db locked'); return get(collection, id); };
  const rb = await generate(cmd('outbound.research_source.generate_adapter', payload()), broken.t.ctx);
  assert.equal(rb.adapter.status, 'adapter_failed');
  assert.equal(rb.adapter.scrape_status, 'registration_failed');
  assert.equal(rb.adapter.payload.scrape_registry_effect.phase, 'register');
  const noKey = createTestCtx({ caps: baseCaps() });
  const rk = await generate(cmd('outbound.research_source.generate_adapter', payload({ adapter: { ...ADAPTER, target_key: '' } })), noKey.ctx);
  assert.equal(rk.adapter.scrape_status, 'target_key_missing');
  const skip = await generate(cmd('outbound.research_source.generate_adapter', payload({ adapter: { ...ADAPTER, adapter_kind: 'manual' } })), noKey.ctx);
  assert.deepEqual(skip.adapter.payload.scrape_registry_effect, { ok: true, skipped: true, reason: 'source_is_not_scrape_target', adapter_kind: 'manual' });
});

test('test input: company required, country fallback, email rule, swap_remove order (RUST-EIGENHEIT)', () => {
  const c = (p) => ({ id: ' cmd_t ', payload: p });
  assert.throws(() => researchScrapeTestInput(c({}), {}, 's'), /test_input.company/);
  const input = researchScrapeTestInput(c({ test_input: { company: 'ACME', email: 'a@b.de' } }), { countries: ['AT'] }, 'src');
  assert.deepEqual(input, { company: 'ACME', country: 'AT', source_id: 'src', adapter_test: true, task_id: 'cmd_t', operation_timeout_ms: 90000, email: 'a@b.de' });
  assert.deepEqual(Object.keys(swapRemove(input, 'task_id')), ['company', 'country', 'source_id', 'adapter_test', 'email', 'operation_timeout_ms']);
  const noMail = researchScrapeTestInput(c({ company: 'X', test_input: { email: 'a@@b' } }), {}, 's');
  assert.equal(noMail.country, 'DE');
  assert.equal(noMail.email, undefined);
  assert.throws(() => researchScrapeTestInput(c({ company: 'X', test_input: { operation_timeout_ms: 10 } }), {}, 's'), /between 1000 and 300000/);
  assert.equal(rustDebugStrings(['a', 'b"c']), '["a", "b\\"c"]');
});

test('test with existing script runs via process.run and passes with valid evidence (ref :3313-3440)', async () => {
  const { t, procCalls, fs, host } = scrapeCtx({ now: 1000, caps: baseCaps(), run: { exit: 0, stdout: PROSPECT_RECORDS } });
  await seedTarget(t.ctx, { target_key: 'xing', script: 'console.log(1);\n' });
  const r = await sourceTest(cmd('outbound.research_source.test', payload({ test_input: { company: 'ACME', operation_timeout_ms: 1500 } }), { id: 'cmd_t' }), t.ctx);
  assert.equal(procCalls.length, 1);
  // §16.48 Nr. 3: Allowlist-Eintrag olg-scrape-js (node + runtime/host.mjs), Skript + Konfiguration als Host-Dokument
  // auf stdin, kein args, kein env (Grundumgebung setzt die Plattform).
  const call = procCalls[0];
  assert.deepEqual(Object.keys(call).sort(), ['cwd_ref', 'program', 'stdin', 'timeout_ms']);
  assert.equal(call.program, 'olg-scrape-js');
  const doc = host(call);
  assert.equal(doc.schema, 'ctox.olg.scrape_host.v1');
  assert.equal(doc.language, 'javascript');
  assert.equal(doc.script, 'console.log(1);\n');
  call.env = doc.env;
  assert.equal(call.timeout_ms, 32000);
  assert.match(call.cwd_ref, /^outbound-lead-generation\/scrape\/targets\/xing\/runs\/run-/);
  assert.deepEqual(JSON.parse(call.env.CTOX_SCRAPE_INPUT_JSON), { company: 'ACME', country: 'DE', source_id: 'xing.com', adapter_test: true, task_id: 'cmd_t', operation_timeout_ms: 1500 });
  assert.equal(call.env.CTOX_SCRAPE_TRIGGER_KIND, 'manual');
  assert.equal(call.env.CTOX_SCRAPE_COMMAND_ID, 'cmd_t');
  assert.equal(call.env.CTOX_SCRAPE_TARGET_KEY, 'xing');
  assert.equal(call.env.CTOX_SCRAPE_OUTPUT_DIR, 'outputs');
  assert.equal(JSON.parse(call.env.CTOX_SCRAPE_MANIFEST_JSON).target_key, 'xing');
  const rec = r.adapter;
  assert.equal(rec.status, 'test_ok');
  assert.equal(rec.scrape_status, 'test_executed');
  assert.equal(rec.test_ok, true);
  assert.equal(rec.last_success_at_ms, 1000);
  assert.equal(rec.evidence.valid, true);
  assert.deepEqual(rec.last_test.fields_extracted, ['NAME', 'city']);
  assert.deepEqual(rec.last_test.missing_fields, ['Title']);
  assert.equal(rec.payload.scrape_registry_effect.registered_from, 'existing_registry');
  assert.equal(rec.payload.scrape_registry_effect.script_revision_no, 1);
  const run = t.doc(RUNS, rec.last_run_id);
  assert.equal(run.status, 'succeeded');
  assert.equal(run.result.records_found, 3);
  assert.equal(run.trigger_kind, 'manual');
  assert.equal(t.calls.caps.filter((c) => c.name.startsWith('scrape.')).length, 0);
});

test('INV-SCR1 tampered records artifact invalidates the evidence (ref :3658)', async () => {
  const { t, fs } = scrapeCtx({ caps: baseCaps(), run: { exit: 0, stdout: PROSPECT_RECORDS } });
  await seedTarget(t.ctx, { target_key: 'xing' });
  fs.tamper = (name, text) => (name.endsWith('-records.json') ? '[]' : text);
  const r = await sourceTest(cmd('outbound.research_source.test', payload({ company: 'ACME' })), t.ctx);
  assert.equal(r.adapter.status, 'test_evidence_invalid');
  assert.equal(r.adapter.evidence.records_artifact.sha256_matches, false);
  assert.equal(r.adapter.last_error,
    'scrape test validation failed: status=succeeded; records_found=3; expected_fields=["name", "Title"]; extracted_fields=["NAME", "city"]; missing_fields=["Title"]; latency_ms=0');
});

test('failed test: classification from failure_mode/exit; auth_required maps auth_status', async () => {
  assert.deepEqual(failedTestStatus({ status: 'blocked', reason: 'explicit_failure_mode_auth_required', records_found: 0 }, true), ['test_auth_required', 'test_auth_required']);
  assert.deepEqual(failedTestStatus({ status: 'portal_drift', reason: 'command_failed_exit_2', records_found: 0 }, true), ['test_failed', 'test_error']);
  assert.deepEqual(failedTestStatus({ status: 'succeeded', reason: 'x', records_found: 3 }, true), ['test_fields_missing', 'test_fields_missing']);
  const auth = scrapeCtx({ caps: baseCaps(), run: { exit: 0, stdout: { records: [], failure_mode: 'authorization_required', detail: 'login wall' } } });
  await seedTarget(auth.t.ctx, { target_key: 'xing' });
  const ra = await sourceTest(cmd('outbound.research_source.test', payload({ company: 'ACME' })), auth.t.ctx);
  assert.equal(ra.adapter.status, 'test_auth_required');
  assert.equal(ra.adapter.auth_status, 'auth_required');
  const crash = scrapeCtx({ caps: baseCaps(), run: { exit: 2, stdout: '', stderr: 'TypeError: x is undefined' } });
  await seedTarget(crash.t.ctx, { target_key: 'xing' });
  const rc = await sourceTest(cmd('outbound.research_source.test', payload({ company: 'ACME' })), crash.t.ctx);
  assert.equal(rc.adapter.status, 'test_failed');
  assert.equal(rc.adapter.scrape_status, 'test_error');
  assert.equal(rc.adapter.last_test.reason, 'command_failed_exit_Some(2)');
});

test('test preflight: missing company, execution error, unregistered target queues generation', async () => {
  const t = createTestCtx({ caps: baseCaps() });
  const r = await sourceTest(cmd('outbound.research_source.test', payload()), t.ctx);
  assert.equal(r.adapter.status, 'test_input_required');
  assert.equal(r.adapter.scrape_status, 'input_required');
  assert.deepEqual(r.adapter.payload.scrape_registry_effect.required_fields, ['test_input.company']);
  assert.equal(t.calls.caps.filter((c) => c.name.startsWith('scrape.') || c.name.startsWith('process.')).length, 0);
  const err = scrapeCtx({ caps: baseCaps({ 'process.run': async () => { throw new Error('runner crashed'); } }) });
  await seedTarget(err.t.ctx, { target_key: 'xing' });
  const re = await sourceTest(cmd('outbound.research_source.test', payload({ company: 'ACME' })), err.t.ctx);
  assert.equal(re.adapter.status, 'test_failed');
  assert.equal(re.adapter.scrape_status, 'test_error');
  assert.equal(re.adapter.last_test.status, 'execution_error');
  assert.equal(re.adapter.last_error, 'runner crashed');
  const fresh = scrapeCtx({ caps: baseCaps({ 'agent.task.enqueue': async () => ({ task_id: 'k', status: 'running' }) }) });
  const rf = await sourceTest(cmd('outbound.research_source.test', payload({ company: 'ACME' })), fresh.t.ctx);
  assert.equal(rf.adapter.status, 'generation_queued');
  const prompt = fresh.t.calls.caps.find((c) => c.name === 'agent.task.enqueue').args[0].instruction;
  assert.match(prompt, /"adapter_test":true,"operation_timeout_ms":90000\}/);
  assert.match(prompt, /"timeout_seconds":120,/);
});
