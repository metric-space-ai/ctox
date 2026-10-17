// Origin: CTOX
// License: AGPL-3.0-only
// Reine Handler-Logik von workjet.worker.dispatch gegen einen In-Memory-ctx (E54.3b). Lauf:
//   cd src/apps/business-os && node --test modules/workjet/tests/*.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import handle from '../handlers/workjet.worker.dispatch.js';
import { CONTRACT, PENDING_CAPACITY } from '../handlers/_lib/dispatch.js';
import { sha256Hex } from '../handlers/_lib/sha256.js';
import { ENV, OWNER, PROJECT, THREAD, command, makeCtx, receipt, startup } from './_fixture.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const run = (h, payload, actor) => handle(command(payload, actor), h.ctx);
const rejects = async (promise, code, re) => {
  await assert.rejects(promise, (err) => { assert.equal(err.code, code, err.message); if (re) assert.match(err.message, re); return true; });
};
const register = (h, extra = {}) => run(h, { action: 'register_source', source_environment_id: ENV, source_supervisor_thread_id: THREAD, project_id: PROJECT, ...extra });
const dispatch = (h, extra = {}, actor) => run(h, { action: 'dispatch', dispatch_key: 'k1', task: 'Fix the bug', project_id: PROJECT, source_supervisor_thread_id: THREAD, ...extra }, actor);

test('sha256Hex entspricht bekannten Prüfwerten', () => {
  assert.equal(sha256Hex(''), 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
  assert.equal(sha256Hex('abc'), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  for (const text of ['ä€😀', 'x'.repeat(1000), '{"a":1}']) assert.equal(sha256Hex(text), createHash('sha256').update(text, 'utf8').digest('hex'));
});

test('outcome-contract.js ist eine wortgleiche Kopie des generierten Shell-Vertrags', () => {
  const copy = readFileSync(join(HERE, '../handlers/_lib/outcome-contract.js'), 'utf8');
  const source = readFileSync(join(HERE, '../../../shared/workjet-worker-outcome-contract.generated.mjs'), 'utf8');
  assert.ok(copy.endsWith(source));
});

test('register_source → Empfang wie nativ, Zeile in workjet_worker_dispatch_sources', async () => {
  const h = makeCtx();
  const reg = await register(h);
  assert.deepEqual(Object.keys(reg).sort(), ['authorityEpoch', 'contract', 'ownerUserId', 'projectId', 'registrationId', 'revision',
    'sourceEnvironmentId', 'sourceInstanceId', 'sourceSupervisorThreadId', 'sourceWorkspaceId', 'state'].sort());
  assert.equal(reg.contract, CONTRACT);
  assert.equal(reg.revision, 1);
  assert.equal(reg.state, 'active');
  const rows = h.docs('workjet_worker_dispatch_sources');
  assert.equal(rows.length, 1);
  assert.deepEqual(rows[0].record, reg);
  // unverändert: gleiche Revision, ohne/mit passender expected_revision
  assert.deepEqual(await register(h), reg);
  assert.deepEqual(await register(h, { expected_revision: 1 }), reg);
  // Ersatz (anderes Environment) braucht exakte Revision
  await rejects(register(h, { source_environment_id: 'env-2' }), 'conflict', /exact revision/);
  const replaced = await register(h, { source_environment_id: 'env-2', expected_revision: 1 });
  assert.equal(replaced.revision, 2);
  assert.equal(replaced.registrationId, reg.registrationId);
  // expected_revision ohne bestehende Registrierung
  await rejects(run(makeCtx(), { action: 'register_source', source_environment_id: ENV, source_supervisor_thread_id: THREAD, project_id: PROJECT, expected_revision: 3 }), 'conflict', /does not exist/);
});

test('dispatch → pending-Absicht; Wiederholung mit gleichem Schlüssel = dieselbe Absicht, keine zweite Zeile; anderer Inhalt → abgelehnt', async () => {
  const h = makeCtx();
  const reg = await register(h);
  const first = await dispatch(h, { title: 'Bug' });
  assert.equal(first.contract, CONTRACT);
  assert.equal(first.state, 'pending');
  assert.equal(first.result, null);
  assert.deepEqual(Object.keys(first.intent).sort(), ['intentId', 'projectId', 'registrationId', 'registrationRevision', 'sourceEnvironmentId', 'sourceSupervisorThreadId', 'task', 'title'].sort());
  assert.equal(first.intent.registrationId, reg.registrationId);
  const rows = h.docs('workjet_worker_dispatch_intents');
  assert.equal(rows.length, 1);
  assert.equal(rows[0].status, 'pending');
  assert.equal(rows[0].intent_id, first.intent.intentId);
  const replay = await dispatch(h, { title: 'Bug' });
  assert.deepEqual(replay, first);
  assert.equal(h.docs('workjet_worker_dispatch_intents').length, 1);
  await rejects(dispatch(h, { title: 'Bug', task: 'Something else' }), 'conflict', /different scope or task/);
  assert.equal(h.docs('workjet_worker_dispatch_intents').length, 1);
});

test('Obergrenze: die 129. offene Absicht wird abgelehnt', async () => {
  const h = makeCtx();
  await register(h);
  for (let i = 0; i < PENDING_CAPACITY; i += 1) await dispatch(h, { dispatch_key: `k${i}` });
  assert.equal(h.docs('workjet_worker_dispatch_intents').length, 128);
  await rejects(dispatch(h, { dispatch_key: 'k-overflow' }), 'conflict', /capacity exceeded/);
  assert.equal(h.docs('workjet_worker_dispatch_intents').length, 128);
  // Replay eines bestehenden Schlüssels bleibt erlaubt
  assert.equal((await dispatch(h, { dispatch_key: 'k0' })).state, 'pending');
});

test('poll sieht die älteste offene Absicht; complete zweimal: erstes gewinnt, abweichendes zweites abgelehnt', async () => {
  const h = makeCtx();
  const reg = await register(h);
  const a = await dispatch(h, { dispatch_key: 'a' });
  await dispatch(h, { dispatch_key: 'b' });
  const polled = await run(h, { action: 'poll', source_environment_id: ENV });
  assert.deepEqual(Object.keys(polled).sort(), ['contract', 'intents', 'registration']);
  assert.deepEqual(polled.intents, [a.intent]);
  assert.deepEqual(polled.registration, reg);
  assert.deepEqual(await run(h, { action: 'poll', source_environment_id: 'env-unknown' }), { contract: CONTRACT, intents: [], registration: null });
  const result = startup(a.intent);
  const done = await run(h, { action: 'complete', registration_id: reg.registrationId, revision: 1, intent_id: a.intent.intentId, result });
  assert.deepEqual(done, { contract: CONTRACT, intentId: a.intent.intentId, registrationId: reg.registrationId, revision: 1, result });
  // gleiche Wiederholung (Schlüsselreihenfolge egal) ok, abweichende abgelehnt
  const reordered = Object.fromEntries(Object.entries(result).reverse());
  assert.deepEqual((await run(h, { action: 'complete', registration_id: reg.registrationId, revision: 1, intent_id: a.intent.intentId, result: reordered })).result, reordered);
  await rejects(run(h, { action: 'complete', registration_id: reg.registrationId, revision: 1, intent_id: a.intent.intentId, result: { schemaVersion: 1, status: 'failed', reason: 'create-failed' } }), 'conflict', /differs from prior result/);
  assert.deepEqual(h.doc('workjet_worker_dispatch_intents', a.intent.intentId).result, result);
  assert.equal(h.doc('workjet_worker_dispatch_intents', a.intent.intentId).status, 'completed');
  // Kopf der Warteschlange ist jetzt b
  assert.equal((await run(h, { action: 'poll', source_environment_id: ENV })).intents[0].intentId !== a.intent.intentId, true);
  // Replay des abgeschlossenen dispatch liefert completed + Ergebnis
  const replay = await dispatch(h, { dispatch_key: 'a' });
  assert.equal(replay.state, 'completed');
  assert.deepEqual(replay.result, result);
  // ungültiges Ergebnis
  await rejects(run(h, { action: 'complete', registration_id: reg.registrationId, revision: 1, intent_id: a.intent.intentId, result: { schemaVersion: 1, status: 'pending' } }), 'invalid');
});

test('report_outcome → Zeile in workjet_worker_dispatch_outcomes, unveränderlich; observe zeigt Bestätigung und Bericht', async () => {
  const h = makeCtx();
  const reg = await register(h);
  const { intent } = await dispatch(h, { title: 'T', computer_id: 'mac-1' });
  const base = { registration_id: reg.registrationId, revision: 1, intent_id: intent.intentId };
  await rejects(run(h, { action: 'report_outcome', ...base, receipt: receipt(intent) }), 'conflict', /no startup acknowledgement/);
  await run(h, { action: 'complete', ...base, result: startup(intent) });
  const out = await run(h, { action: 'report_outcome', ...base, receipt: receipt(intent) });
  assert.deepEqual(Object.keys(out).sort(), ['accepted_at_ms', 'provenance', 'receipt', 'registration_revision']);
  assert.equal(out.provenance, 'authenticated_source_report');
  assert.equal(out.registration_revision, 1);
  const rows = h.docs('workjet_worker_dispatch_outcomes');
  assert.equal(rows.length, 1);
  assert.equal(rows[0].intent_id, intent.intentId);
  assert.deepEqual(await run(h, { action: 'report_outcome', ...base, receipt: receipt(intent) }), out);
  const other = receipt(intent);
  other.pull_request.state = 'closed';
  await rejects(run(h, { action: 'report_outcome', ...base, receipt: other }), 'conflict', /immutable prior receipt/);
  const badUrl = receipt(intent);
  badUrl.pull_request.url = 'https://github.com/metric-space-ai/ctox/pull/8';
  await rejects(run(h, { action: 'report_outcome', ...base, receipt: badUrl }), 'invalid', /pull request/);
  const notStopped = receipt(intent);
  notStopped.execution_stopped = false;
  await rejects(run(h, { action: 'report_outcome', ...base, receipt: notStopped }), 'conflict', /stopped/);

  const seen = await run(h, { action: 'observe', project_id: PROJECT, source_supervisor_thread_id: THREAD });
  assert.deepEqual(Object.keys(seen).sort(), ['contract', 'observations', 'projectId', 'supervisorThreadId', 'truncated']);
  assert.equal(seen.observations.length, 1);
  const o = seen.observations[0];
  assert.deepEqual(Object.keys(o).sort(), ['acknowledgement', 'computerId', 'dispatchKey', 'execution', 'executionKey', 'intentId', 'registrationCurrent',
    'registrationRevision', 'reportedOutcome', 'taskPreview', 'title', 'workerProfileId'].sort());
  assert.equal(o.registrationCurrent, true);
  assert.equal(o.acknowledgement.status, 'dispatched');
  assert.equal(o.execution, null);
  assert.deepEqual(o.reportedOutcome, out);
  await rejects(run(h, { action: 'observe', project_id: PROJECT, source_supervisor_thread_id: THREAD, limit: 33 }), 'invalid', /1\.\.32/);
});

test('observe: limit und truncated', async () => {
  const h = makeCtx();
  await register(h);
  for (const k of ['a', 'b', 'c']) await dispatch(h, { dispatch_key: k });
  const seen = await run(h, { action: 'observe', project_id: PROJECT, source_supervisor_thread_id: THREAD, limit: 2 });
  assert.equal(seen.observations.length, 2);
  assert.equal(seen.truncated, true);
  assert.equal(seen.observations[0].dispatchKey, 'c'); // neueste zuerst (ORDER BY rowid DESC)
});

test('revoke_source: exakte Revision, danach weder poll noch dispatch; Wiederholung idempotent', async () => {
  const h = makeCtx();
  const reg = await register(h);
  await dispatch(h);
  await rejects(run(h, { action: 'revoke_source', registration_id: reg.registrationId, revision: 2 }), 'conflict', /exact revision/);
  const revoked = await run(h, { action: 'revoke_source', registration_id: reg.registrationId, revision: 1 });
  assert.equal(revoked.state, 'revoked');
  assert.equal(revoked.revision, 2);
  assert.deepEqual(await run(h, { action: 'revoke_source', registration_id: reg.registrationId, revision: 1 }), revoked);
  assert.deepEqual((await run(h, { action: 'poll', source_environment_id: ENV })).intents, []);
  await rejects(dispatch(h, { dispatch_key: 'k2' }), 'conflict', /binding stale/);
  await rejects(run(h, { action: 'revoke_source', registration_id: 'nope', revision: 1 }), 'not_found');
});

test('fremde Sitzung → forbidden (fremder Nutzer, fremdes Projekt, fremder Thread, Rolle ohne Recht)', async () => {
  const h = makeCtx();
  const reg = await register(h);
  const { intent } = await dispatch(h);
  const other = { id: 'other-1', role: 'admin' };
  await rejects(dispatch(h, {}, other), 'forbidden');
  await rejects(run(h, { action: 'dispatch', dispatch_key: 'x', task: 't', project_id: PROJECT, source_supervisor_thread_id: THREAD }, other), 'forbidden', /unavailable to this user/);
  await rejects(run(h, { action: 'dispatch', dispatch_key: 'x', task: 't', project_id: PROJECT, source_supervisor_thread_id: 'thread-o' }), 'forbidden');
  await rejects(run(h, { action: 'observe', project_id: PROJECT, source_supervisor_thread_id: THREAD }, other), 'forbidden');
  await rejects(run(h, { action: 'register_source', source_environment_id: ENV, source_supervisor_thread_id: THREAD, project_id: PROJECT }, other), 'forbidden');
  await rejects(run(h, { action: 'dispatch', dispatch_key: 'x', task: 't', project_id: PROJECT, source_supervisor_thread_id: THREAD }, { id: 'ghost', role: 'admin' }), 'forbidden', /not a persisted native user/);
  await rejects(dispatch(h, {}, { id: OWNER, role: 'user' }), 'forbidden', /role changed/);
  // fremder Nutzer sieht weder Registrierung noch Absicht
  await rejects(run(h, { action: 'complete', registration_id: reg.registrationId, revision: 1, intent_id: intent.intentId, result: startup(intent) }, other), 'not_found');
  assert.deepEqual((await run(h, { action: 'poll', source_environment_id: ENV }, other)).intents, []);
});

test('Eingaben: task > 16384 Bytes → invalid, unbekannte Felder/Aktion → invalid, nichts geschrieben', async () => {
  const h = makeCtx();
  await register(h);
  await rejects(dispatch(h, { task: 'x'.repeat(16385) }), 'invalid', /task/);
  await rejects(dispatch(h, { task: 'ä'.repeat(8193) }), 'invalid', /task/); // 16386 UTF-8-Bytes
  assert.equal((await dispatch(h, { task: 'x'.repeat(16384) })).state, 'pending');
  await rejects(dispatch(h, { dispatch_key: 'k9', task: ' padded ' }), 'invalid');
  await rejects(dispatch(h, { dispatch_key: 'k9', title: 't'.repeat(201) }), 'invalid', /title/);
  await rejects(dispatch(h, { dispatch_key: 'k9', surprise: 1 }), 'invalid', /unknown field/);
  await rejects(run(h, { action: 'nope' }), 'invalid');
  await rejects(run(h, { action: 'dispatch', task: 't' }), 'invalid', /missing field/);
  assert.equal(h.docs('workjet_worker_dispatch_intents').length, 1);
  // _context ist Transportmetadaten und wird ignoriert (ref: :329-334)
  assert.equal((await dispatch(h, { dispatch_key: 'k10', _context: { gateway: 1 } })).state, 'pending');
});

// E54.3c (Kern, eng statt offen): bis der Kern workjet_supervisor_leases projiziert, ersetzen Rolle admin + Projekt-Owner +
// Bindung über workjet_project_chats + capability_epoch = Epoche der Quelle die native Supervisor-Lease – für dispatch,
// complete und report_outcome. Eine chef-Rolle mit sonst passender Bindung bekommt forbidden.
test('E54.3c: dispatch/complete/report_outcome nur für admin mit passender Epoche; chef und fremde Epoche → forbidden', async () => {
  const chef = { id: OWNER, role: 'chef' };
  const setRole = (h, role) => h.ctx.patch('business_users', 'bu-owner', { role });
  // chef registriert seine Quelle (wie nativ), darf aber nichts ausschicken.
  const hc = makeCtx();
  await setRole(hc, 'chef');
  await run(hc, { action: 'register_source', source_environment_id: ENV, source_supervisor_thread_id: THREAD, project_id: PROJECT }, chef);
  await rejects(dispatch(hc, {}, chef), 'forbidden', /E54\.3c/);
  assert.equal(hc.docs('workjet_worker_dispatch_intents').length, 0);
  // admin legt Absicht an und bestätigt; danach Rolle → chef: complete und report_outcome forbidden.
  const h = makeCtx();
  const reg = await register(h);
  const a = await dispatch(h, { dispatch_key: 'a' });
  await setRole(h, 'chef');
  await rejects(run(h, { action: 'complete', registration_id: reg.registrationId, revision: 1, intent_id: a.intent.intentId, result: startup(a.intent) }, chef), 'forbidden', /E54\.3c/);
  await setRole(h, 'admin');
  await run(h, { action: 'complete', registration_id: reg.registrationId, revision: 1, intent_id: a.intent.intentId, result: startup(a.intent) });
  await setRole(h, 'chef');
  await rejects(run(h, { action: 'report_outcome', registration_id: reg.registrationId, revision: 1, intent_id: a.intent.intentId, receipt: receipt(a.intent) }, chef), 'forbidden', /E54\.3c/);
  assert.equal(h.docs('workjet_worker_dispatch_outcomes').length, 0);
  // Epoche: Quelle mit Epoche 0 registriert, capability_epoch jetzt 2 → admin bekommt forbidden.
  const he = makeCtx();
  await register(he);
  await he.ctx.patch('business_users', 'bu-owner', { capability_epoch: 2 });
  await rejects(dispatch(he, { dispatch_key: 'e' }), 'forbidden', /epoch differs/);
});
