// Strang OLGSCR: Scrape-Registry, Läufer und neue Befehle der App (vorher Rust capabilities/scrape/*).
import test from 'node:test';
import assert from 'node:assert/strict';
import { cmd, rejects } from './_ctx.mjs';
import { PROSPECT_RECORDS, scrapeCtx, seedTarget } from './_scrape.mjs';
import {
  accountStates, latestScript, normalizeSourcesFromConfig, registerScript, registryRuns, sha256Hex, slugify, stableDigest, targetScriptRegistration,
} from '../_lib/scrape-registry.js';
import { classifyOutcome, executeScrape, extractedRecordFields, normalizeRecords, parseExecutionPayload } from '../_lib/scrape-runner.js';
import { foldRegistryRuns } from '../_lib/update-digest.js';
import upsertCmd from '../outbound.scrape.target.upsert.js';
import registerCmd from '../outbound.scrape.script.register.js';
import executeCmd from '../outbound.scrape.execute.js';

const TARGETS = 'outbound_lead_generation_scrape_targets';
const SCRIPTS = 'outbound_lead_generation_scrape_scripts';
const RUNS = 'outbound_lead_generation_scrape_runs';
const ACCOUNTS = 'outbound_lead_generation_scrape_account_states';
const HOUR = 60 * 60 * 1000;

test('slugify, target_id and source normalization follow Rust (mod.rs:1870, :1900, :567)', async () => {
  assert.equal(slugify('  XING · People v2! '), 'xing-people-v2');
  assert.equal(slugify('!!!'), 'scrape-target');
  assert.equal(await sha256Hex('abc'), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  assert.equal(await stableDigest('abc'), 'ba7816bf8f01cfea');
  const sources = normalizeSourcesFromConfig([
    { url: 'https://a.example/', display_name: 'Portal A', kind: 'listing', tags: [' x ', '', 3] },
    { start_url: 7, url: 'https://ignored.example/', source_key: 'portal-a' },
    { start_url: 7, source_key: 'b' },
    'nope',
  ], 'https://start.example/', 'k');
  assert.deepEqual(sources.map((s) => [s.source_key, s.start_url, s.source_kind]), [['portal-a', 'https://a.example/', 'listing'], ['b', 'https://start.example/', 'generic']]);
  assert.deepEqual(sources[0].tags, ['x']);
  assert.equal(sources[0].extraction_module, 'sources/portal-a/extractor.js');
  assert.equal(normalizeSourcesFromConfig(null, 'u', 'k')[0].source_key, 'primary');
});

test('outbound.scrape.target.upsert: validation, slug key, created_at kept (registry.rs:103)', async () => {
  const { t } = scrapeCtx({ now: 1000 });
  assert.equal((await rejects(upsertCmd(cmd('outbound.scrape.target.upsert', { manifest: { target_key: 'x' } }), t.ctx))).message, 'target payload requires non-empty start_url');
  const r = await upsertCmd(cmd('outbound.scrape.target.upsert', { manifest: { display_name: 'North Data', start_url: ' https://northdata.de ' } }), t.ctx);
  assert.equal(r.target_key, 'north-data');
  assert.equal(r.target_id, `scrape_target-${await stableDigest('north-data')}`);
  assert.equal(r.manifest_path, `${TARGETS}/north-data`);
  t.setNow(2000);
  await upsertCmd(cmd('outbound.scrape.target.upsert', { target_key: 'north-data', start_url: 'https://northdata.de', status: 'paused' }), t.ctx);
  const doc = t.doc(TARGETS, 'north-data');
  assert.equal(doc.status, 'paused');
  assert.equal(doc.created_at, new Date(1000).toISOString());
  assert.equal(doc.display_name, 'north-data');
});

test('INV-SCR5 outbound.scrape.script.register dedupes by sha256 and reactivates (registry.rs:221)', async () => {
  const { t, fs } = scrapeCtx();
  await seedTarget(t.ctx, { target_key: 'xing', script: null });
  assert.equal((await rejects(registerCmd(cmd('outbound.scrape.script.register', { target_key: 'xing', script_text: '  \n' }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(registerCmd(cmd('outbound.scrape.script.register', { target_key: 'nope', script_text: 'x' }), t.ctx))).message, 'target_key not found');
  const a = await registerCmd(cmd('outbound.scrape.script.register', { target_key: 'xing', script_text: 'A\n' }), t.ctx);
  const b = await registerCmd(cmd('outbound.scrape.script.register', { target_key: 'xing', script_text: 'B' }), t.ctx);
  const again = await registerCmd(cmd('outbound.scrape.script.register', { target_key: 'xing', script_text: '  A  ' }), t.ctx);
  assert.deepEqual([a.revision_no, b.revision_no, again.revision_no], [1, 2, 1]);
  assert.equal(again.deduplicated, true);
  assert.equal(t.all(SCRIPTS).length, 2);
  assert.equal(t.doc(TARGETS, 'xing').latest_script_revision_no, 1);
  assert.equal(fs.texts.get(t.doc(SCRIPTS, 'xing:r1').script_ref), 'A\n');
  assert.equal((await targetScriptRegistration(t.ctx, 'xing')).script_registered, true);
});

test('INV-SCR5 registration is unusable when the stored body no longer hashes to the revision (registry.rs:79)', async () => {
  const { t, fs } = scrapeCtx();
  await seedTarget(t.ctx, { target_key: 'xing', script: 'ok();\n' });
  assert.equal((await targetScriptRegistration(t.ctx, 'xing')).script_registered, true);
  fs.tamper = (name, text) => (name.startsWith('rev') ? 'evil();' : text);
  assert.equal((await targetScriptRegistration(t.ctx, 'xing')).script_registered, false);
  assert.equal(await targetScriptRegistration(t.ctx, 'nope'), null);
  await rejects(executeScrape(t.ctx, { target_key: 'xing', trigger_kind: 'manual', timeout_seconds: 5, input: {} }));
});

test('payload parsing, records and extracted fields (mod.rs:1033-1140)', () => {
  assert.deepEqual(parseExecutionPayload('  '), {});
  assert.deepEqual(parseExecutionPayload('[1]'), { records: [1] });
  assert.throws(() => parseExecutionPayload('{'), /stdout must be valid json/);
  assert.deepEqual(normalizeRecords({ result: { items: [1, 2] } }), [1, 2]);
  assert.equal(normalizeRecords({ x: 1 }), null);
  assert.deepEqual(extractedRecordFields(PROSPECT_RECORDS), ['NAME', 'city']);
});

test('INV-SCR6 classification order (classify.rs)', () => {
  const ok = { exit_code: 0, timed_out: false, stderr_text: '' };
  assert.equal(classifyOutcome({ failure_mode: 'blocked', detail: 'Account is suspended' }, ok, 0, 0).status, 'provider_account_inactive');
  assert.equal(classifyOutcome({ failure_mode: 'blocked', detail: 'captcha' }, ok, 0, 0).status, 'blocked');
  assert.equal(classifyOutcome({ failure_mode: 'auth_required' }, ok, 0, 0).reason, 'explicit_failure_mode_auth_required');
  assert.equal(classifyOutcome({ partial_output: true }, ok, 5, 0).status, 'partial_output');
  assert.equal(classifyOutcome({}, { ...ok, timed_out: true }, 5, 0).reason, 'command_timed_out');
  assert.equal(classifyOutcome({}, { ...ok, exit_code: 1, stderr_text: 'net::ERR_NAME_NOT_RESOLVED' }, 0, 0).reason, 'transient_error_hint');
  assert.equal(classifyOutcome({}, ok, 2, 5).reason, 'records_found_below_expected_min:2<5');
  assert.equal(classifyOutcome({}, { ...ok, exit_code: 3 }, 0, 0).reason, 'command_failed_exit_Some(3)');
  assert.equal(classifyOutcome({}, ok, 0, 0).reason, 'empty_record_set_on_reachable_portal');
  assert.equal(classifyOutcome({}, ok, 1, 0).status, 'succeeded');
});

test('outbound.scrape.execute records run, artifacts and a first inactive account state (execute.rs:909, account_state.rs:262)', async () => {
  const { t, fs } = scrapeCtx({ now: 5000, run: { exit: 0, stdout: { records: [], failure_mode: 'provider_account_inactive', detail: 'Customer is not active' } } });
  await seedTarget(t.ctx, { target_key: 'li' });
  assert.equal((await rejects(executeCmd(cmd('outbound.scrape.execute', { target_key: 'li', timeout_seconds: 0 }), t.ctx))).code, 'invalid');
  const r = await executeCmd(cmd('outbound.scrape.execute', { target_key: 'li', input: { company: 'ACME' } }, { id: 'cmd_x' }), t.ctx);
  assert.equal(r.status, 'provider_account_inactive');
  const run = t.doc(RUNS, r.run_id);
  assert.equal(run.result.detail, 'Customer is not active');
  assert.equal(run.command_id, 'cmd_x');
  assert.equal(JSON.parse(fs.texts.get(r.run_manifest_path)).run_id, r.run_id);
  const s = t.doc(ACCOUNTS, 'li');
  assert.deepEqual([s.generation, s.failed_probes, s.causal_run_id, s.next_probe_at_ms], [1, 1, r.run_id, 5000 + 6 * HOUR]);
  const folded = foldRegistryRuns(await registryRuns(t.ctx));
  assert.deepEqual(folded.get('li'), { status: 'provider_account_inactive', at_ms: 5000, detail: 'Customer is not active · provider_account_inactive', last_ok_ms: 0 });
});

test('INV-SCR3 an inactive account suppresses the provider call until the probe is due (account_state.rs:166)', async () => {
  const { t, procCalls } = scrapeCtx({ now: 1000, run: { exit: 0, stdout: PROSPECT_RECORDS } });
  await seedTarget(t.ctx, { target_key: 'li' });
  await t.ctx.put(ACCOUNTS, { id: 'li', target_key: 'li', target_id: 'x', generation: 1, reason: 'Customer is not active', causal_run_id: 'r1', last_probe_run_id: 'r1', failed_probes: 1, next_probe_at_ms: 2000, probe_lease_owner: null, probe_lease_until_ms: 0, updated_at_ms: 1 });
  const suppressed = await executeScrape(t.ctx, { target_key: 'li', trigger_kind: 'manual', timeout_seconds: 5, input: {} });
  assert.equal(suppressed.reason, 'provider_account_inactive_suppressed');
  assert.match(suppressed.error, /^Konto beim Anbieter inaktiv: Customer is not active \(erfasst in r1, zuletzt geprüft in r1\); kein Anbieteraufruf\./);
  assert.equal(procCalls.length, 0);
  assert.equal(t.all(RUNS).length, 0);
  t.setNow(2000);
  const due = await executeScrape(t.ctx, { target_key: 'li', trigger_kind: 'manual', timeout_seconds: 5, input: {} });
  assert.equal(due.status, 'succeeded');
  assert.equal(procCalls.length, 1);
  assert.equal(t.doc(ACCOUNTS, 'li'), null);
});

test('registerScript is the only writer of the active revision; registry reads need no Core capability', async () => {
  const { t } = scrapeCtx();
  await seedTarget(t.ctx, { target_key: 'xing', script: 'v1' });
  await registerScript(t.ctx, { target_key: 'xing', script_text: 'v2' });
  assert.equal(t.doc(TARGETS, 'xing').latest_script_revision_no, 2);
  assert.equal(t.calls.caps.filter((c) => !c.name.startsWith('files.')).length, 0);
});

// Aus dem gelöschten Handler-Test outbound.research_source.registry_read übernommen (Strang CAPSIG2): die UI liest
// accountStates/latestScript direkt (index.js), die Invariante INV-SCR4 bleibt hier belegt.
test('INV-SCR4 probe_in_progress only while the lease is unexpired (ref :1338)', async () => {
  const data = { outbound_lead_generation_scrape_account_states: {
    northdata: { id: 'northdata', target_id: 't2', generation: 1, reason: 'Customer is not active', causal_run_id: 'r1', last_probe_run_id: 'r1', failed_probes: 1, next_probe_at_ms: 5000, probe_lease_owner: 'r2', probe_lease_until_ms: 9000, updated_at_ms: 10 },
  } };
  const { t } = scrapeCtx({ data, now: 9000 });
  assert.equal((await accountStates(t.ctx)).t2.probe_in_progress, false);
  t.setNow(8999);
  assert.equal((await accountStates(t.ctx)).t2.probe_in_progress, true);
});

test('latest_script: no revision → unavailable; > max bytes → truncated with marker (ref :1420)', async () => {
  const { t } = scrapeCtx();
  await seedTarget(t.ctx, { target_key: 'big', script: null });
  assert.deepEqual(await latestScript(t.ctx, 'big', 10), { target_key: 'big', available: false });
  await seedTarget(t.ctx, { target_key: 'big', script: 'äbcdefghijklmnop' });
  const s = await latestScript(t.ctx, 'big', 10);
  assert.equal(s.truncated, true);
  assert.equal(s.text, 'äbcdefghi\n/* … gekürzt: Skript größer als 150 KB … */\n');
});

// CAP-3b #844: files.* bleibt bis zum Cutover `unavailable` (files::bound() ohne Bindung). Der Lauf scheitert
// sauber mit Code `unavailable` (Zone-Meldung) statt das Skript als fehlend zu werten oder abzustürzen.
test('CAP-3b files unavailable: execute fails with code unavailable, no process.run, no run document', async () => {
  const { t, fs, procCalls } = scrapeCtx({ now: 1000, run: { exit: 0, stdout: PROSPECT_RECORDS } });
  await seedTarget(t.ctx, { target_key: 'xing' });
  fs.unavailable = true;
  const err = await rejects(executeCmd(cmd('outbound.scrape.execute', { target_key: 'xing', input: { company: 'ACME' } }), t.ctx));
  assert.equal(err.code, 'unavailable');
  assert.match(err.message, /files\.get/);
  assert.equal(procCalls.length, 0);
  assert.equal(t.all(RUNS).length, 0);
});

test('OLG725 process.run §16.48: olg-scrape-js + Host-Dokument auf stdin, kein args/env; über 1 MiB per stdin_ref', async () => {
  const { t, procCalls, host, fs } = scrapeCtx({ now: 1000, run: { exit: 0, stdout: PROSPECT_RECORDS } });
  await seedTarget(t.ctx, { target_key: 'xing' });
  // Früher scheiterte ein Umgebungswert über 4096 Byte (CAP-3b); auf stdin trägt die Konfiguration bis 1 MiB.
  const ok = await executeScrape(t.ctx, { target_key: 'xing', trigger_kind: 'manual', timeout_seconds: 5, input: { blob: 'x'.repeat(5000), owner_user_id: 'u1' } });
  assert.equal(ok.status, 'succeeded');
  const call = procCalls[0];
  assert.deepEqual(Object.keys(call).sort(), ['cwd_ref', 'program', 'stdin', 'timeout_ms']);
  assert.equal(call.program, 'olg-scrape-js');
  const doc = host(call);
  assert.equal(doc.schema, 'ctox.olg.scrape_host.v1');
  assert.equal(doc.script, 'console.log(JSON.stringify([]));\n');
  assert.equal(doc.script_sha256, await sha256Hex(doc.script.trim()));
  assert.equal(doc.env.CTOX_OWNER_USER_ID, 'u1');
  assert.equal(JSON.parse(doc.env.CTOX_SCRAPE_INPUT_JSON).blob.length, 5000);
  assert.ok(!['PATH', 'CTOX_BIN', 'LANG'].some((k) => k in doc.env), 'Grundumgebung setzt die Plattform');
  await executeScrape(t.ctx, { target_key: 'xing', trigger_kind: 'manual', timeout_seconds: 5, input: { blob: 'y'.repeat(1024 * 1024) } });
  const big = procCalls[1];
  assert.equal(big.stdin, undefined);
  assert.equal(typeof big.stdin_ref, 'string');
  assert.equal(JSON.parse(host(big).env.CTOX_SCRAPE_INPUT_JSON).blob.length, 1024 * 1024);
  assert.ok(fs.names.get(big.stdin_ref).endsWith('-host-input.json'));
});

test('OLG725 TypeScript läuft über olg-scrape-ts; bash/sh weder registrierbar noch ausführbar (olg-scrape-sh abgelehnt)', async () => {
  const { t, procCalls, host } = scrapeCtx({ now: 1000, run: { exit: 0, stdout: PROSPECT_RECORDS } });
  await seedTarget(t.ctx, { target_key: 'xing', script: null });
  await registerScript(t.ctx, { target_key: 'xing', script_text: 'const n: number = 1;\nconsole.log("[]");\n', language: 'ts' });
  await executeScrape(t.ctx, { target_key: 'xing', trigger_kind: 'manual', timeout_seconds: 5, input: {} });
  assert.equal(procCalls[0].program, 'olg-scrape-ts');
  assert.equal(host(procCalls[0]).language, 'typescript');
  for (const language of ['bash', 'sh', 'shell', 'python']) {
    const e = await rejects(registerScript(t.ctx, { target_key: 'xing', script_text: 'echo []', language }));
    assert.equal(e.code, 'invalid', language);
    const mitEntry = await rejects(registerScript(t.ctx, { target_key: 'xing', script_text: 'echo []', language, entry_command: ['bash', '{script_path}'] }));
    assert.equal(mitEntry.code, 'invalid', `${language} mit entry_command`);
  }
  // Bestandsrevision in bash (vor §16.48 registriert): kein Lauf, kein Rückfall auf einen direkten Aufruf.
  const rev = t.all(SCRIPTS)[0];
  await t.ctx.put(SCRIPTS, { ...rev, language: 'bash', entry_command: ['bash', '{script_path}'] });
  const e = await rejects(executeScrape(t.ctx, { target_key: 'xing', trigger_kind: 'manual', timeout_seconds: 5, input: {} }));
  assert.equal(e.code, 'invalid');
  assert.match(e.message, /olg-scrape-sh abgelehnt/);
  assert.equal(procCalls.length, 1);
});

test('OLG725 ohne Sandbox (unavailable): Lauf wartet auf Freigabe, kein direkter Aufruf, kein Konto-Zustand', async () => {
  const { t, procCalls, sandbox } = scrapeCtx({ now: 1000, run: { exit: 0, stdout: PROSPECT_RECORDS }, sandbox: { available: false } });
  await seedTarget(t.ctx, { target_key: 'xing' });
  const e = await rejects(executeScrape(t.ctx, { target_key: 'xing', trigger_kind: 'manual', timeout_seconds: 5, input: {}, command_id: 'cmd_x' }));
  assert.equal(e.code, 'unavailable');
  assert.match(e.message, /wartet auf Freigabe/);
  assert.match(e.message, /olg-scrape-js/);
  assert.equal(procCalls.length, 0, 'kein Rückfall auf node/tsx direkt');
  const runs = t.all(RUNS);
  assert.equal(runs.length, 1);
  assert.equal(runs[0].status, 'awaiting_approval');
  assert.equal(runs[0].command_id, 'cmd_x');
  assert.equal(runs[0].result.reason, 'sandbox_unavailable');
  assert.equal(t.all(ACCOUNTS).length, 0);
  sandbox.available = true;
  const ok = await executeScrape(t.ctx, { target_key: 'xing', trigger_kind: 'manual', timeout_seconds: 5, input: {} });
  assert.equal(ok.status, 'succeeded', 'nach Freigabe läuft derselbe Weg');
});
