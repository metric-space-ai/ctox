import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';

const NOW = 1_700_000_000_000;
const DAY = 86_400_000;
// In V4-HANDLERS.md beantragte foreign_collections (DSGVO-Auskunft/Löschung über alle ATS-Collections)
const ATS_FOREIGN = Object.fromEntries(['applications', 'business_credentials', 'submissions', 'offers', 'placements',
  'interview_scorecards', 'interview_meetings', 'signature_requests', 'planning_time_records'].map((c) => [c, { owner_app: 'x', ops: ['read', 'write'] }]));
const caps = (cfg = {}) => ({
  'config.get': ({ key }) => ({ value: cfg[key] ?? null }),
  'audit.query': { events: [{ event_type: 'business_os.ats.candidate_presented' }] },
});

test('consent.check: gültig, widerrufen, Nachweispflicht über Konfig', async () => {
  const seed = { business_consents: [
    { id: 'k1', subject_id: 'c1', purpose: 'present_to_client', granted_at_ms: 1, updated_at_ms: 1 },
    { id: 'k2', subject_id: 'c1', purpose: 'marketing', legal_basis: 'legitimate_interest', updated_at_ms: 1 },
  ] };
  const ctx = createCtx({ seed, caps: caps() });
  assert.deepEqual(await run(ctx, 'ats.consent.check', { subject_id: 'c1', purpose: 'present_to_client' }), { ok: true, allowed: true, purpose: 'present_to_client' });
  assert.equal((await run(ctx, 'ats.consent.check', { subject_id: 'c1', purpose: 'marketing' })).allowed, true);
  assert.equal((await run(ctx, 'ats.consent.check', { subject_id: 'c2', purpose: 'marketing' })).allowed, false);
  assert.deepEqual(await run(ctx, 'ats.consent.check', { subject_id: 'c2' }), { ok: true, allowed: true, purpose: null });
  const strict = createCtx({ seed, caps: caps({ CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE: '1' }) });
  assert.equal((await run(strict, 'ats.consent.check', { subject_id: 'c1', purpose: 'marketing' })).allowed, false);
  await assert.rejects(run(ctx, 'ats.consent.check', {}), { code: 'invalid', message: 'ats.consent.check requires subject_id' });
});

test('retention.due liest nur; purge nur ATS-Collections, löscht fällige, protokolliert', async () => {
  const seed = { business_consents: [
    { id: 'old', subject_id: 's', purpose: 'p', created_at_ms: NOW - 400 * DAY, updated_at_ms: 1 },
    { id: 'new', subject_id: 's', purpose: 'p', created_at_ms: NOW - 10 * DAY, updated_at_ms: 1 },
    { id: 'none', subject_id: 's', purpose: 'p', updated_at_ms: 1 },
  ] };
  const ctx = createCtx({ seed, caps: caps(), now: NOW });
  assert.deepEqual(await run(ctx, 'ats.retention.due', { collection: 'business_consents', retention_days: 365 }), { ok: true, count: 1, due_ids: ['old'] });
  assert.equal(ctx.$.writes.length, 0);
  // v4: Lesegrenze ist das Manifest (eigene + foreign_collections). accounting_invoices ist seit M2 lesend deklariert;
  // eine nicht deklarierte Collection bleibt verboten (forbidden statt still leer).
  await assert.rejects(run(ctx, 'ats.retention.due', { collection: 'customer_accounts', retention_days: 1 }), { code: 'forbidden' });
  await assert.rejects(run(ctx, 'ats.retention.purge', { collection: 'accounting_invoices', retention_days: 1 }),
    { code: 'forbidden', message: 'collection "accounting_invoices" is not an ATS-writable collection' });
  const out = await run(ctx, 'ats.retention.purge', { collection: 'business_consents', retention_days: 365 });
  assert.deepEqual(out, { ok: true, purged: 1, purged_ids: ['old'] });
  assert.equal(ctx.$.doc('business_consents', 'old'), null);
  assert.ok(ctx.$.doc('business_consents', 'new'));
  const ev = ctx.$.capsTo('audit.event')[0].args[0];
  assert.equal(ev.event_type, 'business_os.ats.retention_purged');
  assert.deepEqual(ev.payload.summary, { collection: 'business_consents', purged_count: 1, purged_ids: ['old'], retention_days: 365 });
  assert.deepEqual(ev.payload.actor, { id: 'tester', role: 'admin', trusted: true });
});

test('subject.export: über alle ATS-Collections, subject_id oder candidate_id, ohne Dubletten, mit Prüfspur', async () => {
  const seed = {
    business_consents: [{ id: 'k1', subject_id: 'c1', purpose: 'p', updated_at_ms: 1 }],
    submissions: [{ id: 's1', candidate_id: 'c1', client_account_id: 'x' }, { id: 's2', candidate_id: 'c2' }],
    placements: [{ id: 'p1', candidate_id: 'c1', subject_id: 'c1' }],
  };
  const ctx = createCtx({ seed, caps: caps(), wishForeign: ATS_FOREIGN, now: NOW });
  const out = await run(ctx, 'ats.subject.export', { candidate_id: 'c1' });
  assert.equal(out.record_count, 3);
  assert.deepEqual(Object.keys(out.collections), ['business_consents', 'submissions', 'placements']);
  assert.equal(out.collections.placements.length, 1);
  assert.equal(out.audit_trail.length, 1);
  assert.equal(out.exported_at_ms, NOW);
  assert.deepEqual(ctx.$.capsTo('audit.query')[0].args[0], { command_type_prefix: 'business_os.ats.', summary: { candidate_id: 'c1' }, order: 'observed_at_ms' });
  assert.deepEqual(ctx.$.capsTo('audit.event')[0].args[0].payload.summary, { candidate_id: 'c1', record_count: 3, audit_event_count: 1 });
  await assert.rejects(run(ctx, 'ats.subject.export', {}), { code: 'invalid', message: 'ats.subject.export requires subject_id' });
});

test('subject.erase: alles zum Subjekt in einer Transaktion gelöscht, Fremdes bleibt', async () => {
  const seed = {
    business_consents: [{ id: 'k1', subject_id: 'c1', purpose: 'p', updated_at_ms: 1 }],
    submissions: [{ id: 's1', candidate_id: 'c1' }, { id: 's2', candidate_id: 'c2' }],
    applications: [{ id: 'a1', subject_id: 'c1', candidate_id: 'c1' }],
  };
  const ctx = createCtx({ seed, caps: caps(), wishForeign: ATS_FOREIGN });
  const out = await run(ctx, 'ats.subject.erase', { subject_id: 'c1' });
  assert.equal(out.erased_count, 3);
  assert.deepEqual(out.erased, { applications: ['a1'], business_consents: ['k1'], submissions: ['s1'] });
  assert.equal(ctx.$.doc('submissions', 's1'), null);
  assert.ok(ctx.$.doc('submissions', 's2'));
  assert.equal(ctx.$.capsTo('audit.event')[0].args[0].event_type, 'business_os.ats.subject_erased');
});

test('ohne Fremd-Deklaration verweigert v4 den Export (forbidden statt still leer)', async () => {
  // Das Manifest deklariert seit M2 alle ATS-Collections; fehlt der Lesezugriff auf eine davon (hier: submissions
  // ohne `read`), muss der Export scheitern statt die Collection still zu überspringen.
  const ctx = createCtx({ caps: caps(), wishForeign: { submissions: { owner_app: 'submissions', ops: [] } } });
  await assert.rejects(run(ctx, 'ats.subject.export', { subject_id: 'c1' }), { code: 'forbidden' });
  // Mit der Manifest-Deklaration allein (ohne Zusatzwünsche) läuft der Export.
  const declared = createCtx({ caps: caps(), seed: { submissions: [{ id: 's1', candidate_id: 'c1' }] } });
  assert.equal((await run(declared, 'ats.subject.export', { subject_id: 'c1' })).record_count, 1);
});
