import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';

const NOW = 1_700_000_000_000;
const DAY = 86_400_000;
const FOREIGN = {
  planning_time_records: { owner_app: 'shiftflow', ops: ['read', 'write'] },
  signature_requests: { owner_app: 'esign', ops: ['read'] },
  accounting_invoices: { owner_app: 'invoices', ops: ['write', 'read'] },
};
const cfg = (values = {}) => ({ 'config.get': ({ key }) => ({ value: values[key] ?? null }) });

test('deployment.check: bereit / Blocker', async () => {
  const seed = { business_credentials: [{ id: 'cr1', subject_id: 'w1', credential_type: 'g25', verified: true, valid_until_ms: NOW + 100 * DAY, updated_at_ms: 1 }] };
  const ctx = createCtx({ seed, now: NOW });
  assert.deepEqual(await run(ctx, 'ats.deployment.check', { subject_id: 'w1', required_types: ['g25'] }), { ok: true, ready: true, blockers: [] });
  assert.deepEqual(await run(ctx, 'ats.deployment.check', { subject_id: 'w1', required_types: ['g25', 'erste_hilfe', 7] }),
    { ok: true, ready: false, blockers: [{ credential_type: 'erste_hilfe', reason: 'missing' }] });
  await assert.rejects(run(ctx, 'ats.deployment.check', {}), { code: 'invalid', message: 'ats.deployment.check requires subject_id' });
});

test('credential.verify (H12): stempelt Prüfer, verlangt authentifizierten Handelnden', async () => {
  const seed = { business_credentials: [{ id: 'cred-1', subject_id: 'cand-1', credential_type: 'license', verified: false, updated_at_ms: 1 }] };
  const ctx = createCtx({ seed, now: NOW });
  const out = await run(ctx, 'ats.credential.verify', { credential_id: 'cred-1' }, { actor: { id: 'chef1', role: 'chef' } });
  assert.deepEqual(out, { ok: true, credential_id: 'cred-1', verified: true, verified_by: 'chef1' });
  const doc = ctx.$.doc('business_credentials', 'cred-1');
  assert.equal(doc.verified, true);
  assert.equal(doc.verified_by, 'chef1');
  assert.equal(doc.verified_at_ms, NOW);
  await assert.rejects(run(ctx, 'ats.credential.verify', { credential_id: 'cred-1' }, { actor: { id: 'rxdb-command' } }),
    { code: 'forbidden', message: 'ats.credential.verify requires an authenticated verifier' });
  await assert.rejects(run(ctx, 'ats.credential.verify', { id: 'nope' }), { code: 'not_found', message: 'credential not found' });
});

const nachweis = (extra = {}) => ({ planning_time_records: [{ id: 'n1', entleiher_account_id: 'kunde-1', charge_rate: 40,
  entries: [{ hours: 8 }, { type: 'sonntag', hours: 4 }], ...extra }] });

test('signoff ohne Signaturpflicht: abgezeichnet, Rechnungsentwurf mit Zuschlag, freigegeben', async () => {
  const ctx = createCtx({ seed: nachweis(), wishForeign: FOREIGN, caps: cfg(), now: NOW });
  const out = await run(ctx, 'ats.leistungsnachweis.signoff', { record_id: 'n1', surcharge_pct: { sonntag: 50 } });
  assert.deepEqual(out, { ok: true, record_id: 'n1', billing_released: true, blockers: [], invoice_id: 'inv_nachweis_n1', net_total: 560 });
  const inv = ctx.$.doc('accounting_invoices', 'inv_nachweis_n1');
  assert.deepEqual(inv.lines.map((l) => [l.description, l.quantity, l.unit_price_cents]), [['Arbeitsstunden (regular)', 8000, 4000], ['Arbeitsstunden (sonntag)', 4000, 6000]]);
  assert.equal(inv.subtotal_cents, 56000);
  assert.equal(inv.tax_cents, 10640);
  assert.equal(inv.party_id, 'kunde-1');
  assert.equal(inv.source_nachweis_id, 'n1');
  const rec = ctx.$.doc('planning_time_records', 'n1');
  assert.equal(rec.entleiher_signed, true);
  assert.equal(rec.signed_at_ms, NOW);
  assert.equal(rec.billing_released, true);
});

test('signoff: fehlender Satz / keine Stunden → Blocker, keine Rechnung, billing_released false', async () => {
  const ctx = createCtx({ seed: nachweis({ charge_rate: 0, entries: [] }), wishForeign: FOREIGN, caps: cfg(), now: NOW });
  const out = await run(ctx, 'ats.leistungsnachweis.signoff', { nachweis_id: 'n1' });
  assert.deepEqual(out.blockers, ['no_time_entries', 'missing_charge_rate']);
  assert.equal(out.billing_released, false);
  assert.equal(out.invoice_id, '');
  assert.equal(ctx.$.doc('accounting_invoices', 'inv_nachweis_n1'), null);
  assert.equal(ctx.$.doc('planning_time_records', 'n1').billing_released, false);
});

test('signoff mit Signaturpflicht: ohne abgeschlossene Anfrage nicht abgezeichnet; mit jüngster abgeschlossener ja', async () => {
  const strict = cfg({ CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE: '1' });
  const ctx = createCtx({ seed: { ...nachweis(), signature_requests: [{ id: 'sr1', document_id: 'n1', status: 'sent', updated_at_ms: 5 }] }, wishForeign: FOREIGN, caps: strict, now: NOW });
  const out = await run(ctx, 'ats.leistungsnachweis.signoff', { record_id: 'n1' });
  assert.deepEqual(out.blockers, ['entleiher_signature_missing', 'entleiher_signature_proof_missing']);
  assert.equal(ctx.$.doc('planning_time_records', 'n1').entleiher_signed, undefined);
  const ok = createCtx({ seed: { ...nachweis(), signature_requests: [
    { id: 'sr1', document_id: 'n1', status: 'sent', updated_at_ms: 5 },
    { id: 'sr2', document_id: 'n1', status: 'completed', updated_at_ms: 9 },
  ] }, wishForeign: FOREIGN, caps: strict, now: NOW });
  assert.equal((await run(ok, 'ats.leistungsnachweis.signoff', { record_id: 'n1' })).billing_released, true);
  const byId = createCtx({ seed: { ...nachweis(), signature_requests: [{ id: 'sr1', document_id: 'x', status: 'completed', updated_at_ms: 1 }] }, wishForeign: FOREIGN, caps: strict, now: NOW });
  assert.equal((await run(byId, 'ats.leistungsnachweis.signoff', { record_id: 'n1', signature_request_id: 'sr1' })).billing_released, true);
});

test('signoff: Ziel-Collection nur ATS (Sicherheit), record_id Pflicht', async () => {
  const ctx = createCtx({ seed: nachweis(), wishForeign: FOREIGN, caps: cfg() });
  await assert.rejects(run(ctx, 'ats.leistungsnachweis.signoff', { record_id: 'n1', collection: 'accounting_invoices' }), { code: 'forbidden' });
  await assert.rejects(run(ctx, 'ats.leistungsnachweis.signoff', {}), { code: 'invalid', message: 'ats.leistungsnachweis.signoff requires record_id' });
});
