import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';

const NOW = 1_700_000_000_000;
const DAY = 86_400_000;
const FOREIGN = { business_consents: { owner_app: 'consent', ops: ['read'] } };
const caps = { 'config.get': { value: null } };
const consent = { business_consents: [{ id: 'k1', subject_id: 'cand', purpose: 'present_to_client', granted_at_ms: 1, updated_at_ms: 1 }] };

test('present: mit Einwilligung und ohne Konflikt → Vorstellung angelegt', async () => {
  const ctx = createCtx({ seed: consent, wishForeign: FOREIGN, caps, now: NOW });
  const out = await run(ctx, 'ats.submission.present', { candidate_id: 'cand', client_account_id: 'ACME', vacancy_id: 'v1' });
  assert.deepEqual(out, { ok: true, allowed: true, submission_id: `subm_${NOW}` });
  const s = ctx.$.doc('submissions', `subm_${NOW}`);
  assert.equal(s.status, 'sent');
  assert.equal(s.sent_at_ms, NOW);
  assert.equal(ctx.$.capsTo('audit.event')[0].args[0].event_type, 'business_os.ats.candidate_presented');
});

test('present: ohne Einwilligung und mit Doppelvorstellung → beide Blocker, nichts angelegt', async () => {
  const seed = { submissions: [{ id: 's0', candidate_id: 'cand', client_account_id: 'ACME', sent_at_ms: NOW - 10 * DAY, updated_at_ms: 1 }] };
  const ctx = createCtx({ seed, wishForeign: FOREIGN, caps, now: NOW });
  const out = await run(ctx, 'ats.submission.present', { candidate_id: 'cand', client_account_id: 'ACME' });
  assert.deepEqual(out, { ok: true, allowed: false, blockers: [
    { reason: 'consent_to_present_missing' }, { reason: 'double_submission', conflicting_submission_id: 's0' }] });
  assert.equal(ctx.$.all('submissions').length, 1);
  const ev = ctx.$.capsTo('audit.event')[0].args[0];
  assert.equal(ev.event_type, 'business_os.ats.candidate_present_denied');
  assert.deepEqual(ev.payload.summary.reason_codes, ['consent_to_present_missing', 'double_submission']);
});

test('present: Pflichtfelder', async () => {
  const ctx = createCtx({ wishForeign: FOREIGN, caps });
  await assert.rejects(run(ctx, 'ats.submission.present', { candidate_id: 'cand' }), { code: 'invalid', message: 'ats.submission.present requires client_account_id' });
  await assert.rejects(run(ctx, 'ats.submission.present', {}), { code: 'invalid', message: 'ats.submission.present requires candidate_id' });
});
