import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';
import { placementTypeRequiresDeploymentGate } from '../ats.placement.create.js';

const NOW = 1_700_000_000_000;
const DAY = 86_400_000;
const FOREIGN = {
  business_credentials: { owner_app: 'nachweise', ops: ['read'] },
  accounting_invoices: { owner_app: 'invoices', ops: ['read', 'write'] },
};
const cfg = (values = {}) => ({ 'config.get': ({ key }) => ({ value: values[key] ?? null }) });
const creds = (verified) => ({ business_credentials: [{ id: 'c1', subject_id: 'cand', credential_type: 'aug_permit', verified, updated_at_ms: 1 }] });

test('AÜG-Typen wie placement_type_requires_deployment_gate', () => {
  for (const t of ['AUE', ' zeitarbeit ', 'Temp_Staffing', 'überlassung', 'anü']) assert.equal(placementTypeRequiresDeploymentGate(t), true, t);
  for (const t of ['permanent', '', 'ÜBERLASSUNG']) assert.equal(placementTypeRequiresDeploymentGate(t), false, t);
});

test('placement.create direkt: Placement + Honorar-Entwurf + Prüfspur', async () => {
  const ctx = createCtx({ wishForeign: FOREIGN, caps: cfg(), now: NOW });
  const out = await run(ctx, 'ats.placement.create', { candidate_id: 'cand', client_account_id: 'ACME', fee: 15000.5, vacancy_id: 'v1' });
  assert.deepEqual(out, { ok: true, allowed: true, placement_id: `plac_${NOW}`, fee_invoice_id: `inv_placement_plac_${NOW}` });
  const p = ctx.$.doc('placements', `plac_${NOW}`);
  assert.equal(p.guarantee_days, 90);
  assert.equal(p.start_ms, NOW);
  assert.equal(p.status, 'confirmed');
  const inv = ctx.$.doc('accounting_invoices', `inv_placement_plac_${NOW}`);
  assert.equal(inv.subtotal_cents, 1500050);
  assert.equal(inv.tax_cents, 285010);
  assert.equal(inv.source_placement_id, `plac_${NOW}`);
  assert.equal(ctx.$.capsTo('audit.event')[0].args[0].event_type, 'business_os.ats.placement_created');
});

test('placement.create AÜG: ohne konfigurierte Pflichtnachweise abgelehnt (fail closed)', async () => {
  const ctx = createCtx({ wishForeign: FOREIGN, caps: cfg(), now: NOW });
  const out = await run(ctx, 'ats.placement.create', { candidate_id: 'cand', placement_type: 'aue' });
  assert.deepEqual(out, { ok: true, allowed: false, blockers: [{ reason: 'aue_required_credentials_unconfigured' }] });
  assert.equal(ctx.$.all('placements').length, 0);
  assert.deepEqual(ctx.$.capsTo('audit.event')[0].args[0].payload.summary.reason_codes, ['aue_required_credentials_unconfigured']);
});

test('placement.create AÜG: Konfigliste erzwingt Gate; unverifiziert → Blocker; verifiziert → angelegt', async () => {
  const config = cfg({ CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS: ' aug_permit; ,g25' });
  const denied = createCtx({ seed: creds(false), wishForeign: FOREIGN, caps: config, now: NOW });
  const out = await run(denied, 'ats.placement.create', { candidate_id: 'cand', placement_type: 'zeitarbeit' });
  assert.deepEqual(out, { ok: true, allowed: false, blockers: [
    { credential_type: 'aug_permit', reason: 'unverified' }, { credential_type: 'g25', reason: 'missing' }] });
  assert.equal(denied.$.all('placements').length, 0);
  assert.deepEqual(denied.$.capsTo('audit.event')[0].args[0].payload.summary.reason_codes, ['deployment_not_ready']);
  const ok = createCtx({ seed: { business_credentials: [
    { id: 'c1', subject_id: 'cand', credential_type: 'aug_permit', verified: true, updated_at_ms: 1 },
    { id: 'c2', subject_id: 'cand', credential_type: 'g25', verified: true, updated_at_ms: 1 }] }, wishForeign: FOREIGN, caps: config, now: NOW });
  assert.equal((await run(ok, 'ats.placement.create', { candidate_id: 'cand', placement_type: 'zeitarbeit' })).allowed, true);
});

test('early_leave: innerhalb Garantie anteilige Gutschrift, außerhalb nur Status', async () => {
  const seed = { placements: [{ id: 'p1', candidate_id: 'cand', start_ms: NOW - 30 * DAY, guarantee_days: 90, fee: 9000,
    client_account_id: 'ACME', fee_invoice_id: 'inv_placement_p1', updated_at_ms: 1 }] };
  const ctx = createCtx({ seed, wishForeign: FOREIGN, now: NOW });
  const out = await run(ctx, 'ats.placement.early_leave', { placement_id: 'p1' });
  assert.deepEqual(out, { ok: true, placement_id: 'p1', within_guarantee: true, clawback: 6000, credit_note_id: 'cn_earlyleave_p1' });
  const cn = ctx.$.doc('accounting_invoices', 'cn_earlyleave_p1');
  assert.equal(cn.invoice_type, 'credit_note_out');
  assert.equal(cn.credit_note_for_id, 'inv_placement_p1');
  assert.equal(cn.subtotal_cents, 600000);
  const p = ctx.$.doc('placements', 'p1');
  assert.equal(p.status, 'early_leave');
  assert.equal(p.left_at_ms, NOW);
  const late = await run(ctx, 'ats.placement.early_leave', { placement_id: 'p1', left_at_ms: NOW + 100 * DAY });
  assert.deepEqual(late, { ok: true, placement_id: 'p1', within_guarantee: false, clawback: 0, credit_note_id: '' });
  await assert.rejects(run(ctx, 'ats.placement.early_leave', { placement_id: 'px' }), { code: 'not_found', message: 'placement not found' });
});
