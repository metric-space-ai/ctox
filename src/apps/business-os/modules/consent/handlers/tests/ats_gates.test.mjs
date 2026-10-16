// Gate-Primitive – Fälle nach ats_gates.rs #[cfg(test)] (:415ff.) und den Rust-Kommentaren.
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  computeNachweisBilling, consentValid, credentialStatus, evaluateBillingGate, evaluateConsentGate,
  evaluateDeploymentReadiness, findDoubleSubmission, retentionDue, signatureRequestStatus, checkedScaled, atsCanonicalInvoice, atsInvoiceLine,
} from '../_ats.js';

const NOW = 1_750_000_000_000;
const DAY = 86_400_000;

test('credential_status: unverifiziert zuerst, <24h abgelaufen = expired, Warnfenster 30 Tage', () => {
  assert.equal(credentialStatus({ valid_until_ms: NOW + 100 * DAY }, NOW), 'unverified');
  assert.equal(credentialStatus({ verified: true, valid_from_ms: NOW + 1 }, NOW), 'notyetvalid');
  assert.equal(credentialStatus({ verified: true }, NOW), 'valid');
  assert.equal(credentialStatus({ verified: true, valid_until_ms: 0 }, NOW), 'valid');
  assert.equal(credentialStatus({ verified: true, valid_until_ms: NOW - 1000 }, NOW), 'expired');
  assert.equal(credentialStatus({ verified: true, valid_until_ms: NOW + 10 * DAY }, NOW), 'expiring');
  assert.equal(credentialStatus({ verified: true, valid_until_ms: NOW + 31 * DAY }, NOW), 'valid');
});

test('evaluate_deployment_readiness: neuer gültiger deckt alten abgelaufenen, blocking-Typ, missing', () => {
  const creds = [
    { credential_type: 'g25', verified: true, valid_until_ms: NOW - DAY },
    { credential_type: 'g25', verified: true, valid_until_ms: NOW + 100 * DAY },
    { credential_type: 'staplerschein', verified: false, deployment_blocking: true },
  ];
  const r = evaluateDeploymentReadiness(creds, ['g25', 'fuehrerschein'], NOW);
  assert.equal(r.ready, false);
  assert.deepEqual(r.blockers, [['fuehrerschein', 'missing'], ['staplerschein', 'unverified']]);
  assert.equal(evaluateDeploymentReadiness(creds.slice(0, 2), ['g25'], NOW).ready, true);
});

test('consent_valid / evaluate_consent_gate', () => {
  assert.equal(consentValid({ purpose: 'p', granted_at_ms: 1 }, NOW, false), true);
  assert.equal(consentValid({ purpose: 'p', granted_at_ms: 0 }, NOW, false), false);
  assert.equal(consentValid({ granted_at_ms: 1, withdrawn_at_ms: NOW }, NOW, false), false);
  assert.equal(consentValid({ granted_at_ms: 1, expires_at_ms: NOW }, NOW, false), false);
  assert.equal(consentValid({ legal_basis: 'legitimate_interest' }, NOW, false), true);
  assert.equal(consentValid({ legal_basis: 'legitimate_interest' }, NOW, true), false);
  assert.equal(consentValid({ legal_basis: 'legitimate_interest', basis_evidence: ' LIA-1 ' }, NOW, true), true);
  assert.equal(consentValid({ legal_basis: 'vibes' }, NOW, false), false);
  assert.equal(evaluateConsentGate(undefined, [], NOW, false), true);
  assert.equal(evaluateConsentGate('', [], NOW, false), true);
  assert.equal(evaluateConsentGate('x', [{ purpose: 'x', granted_at_ms: 1 }], NOW, false), true);
  assert.equal(evaluateConsentGate('y', [{ purpose: 'x', granted_at_ms: 1 }], NOW, false), false);
});

test('retention_due, find_double_submission, signature_request_status', () => {
  assert.equal(retentionDue(0, 10, NOW), false);
  assert.equal(retentionDue(NOW - 10 * DAY, 10, NOW), true);
  assert.equal(retentionDue(NOW - 10 * DAY + 1, 10, NOW), false);
  const subs = [
    { id: 's0', candidate_id: 'c', client_account_id: 'k', status: 'withdrawn', sent_at_ms: NOW },
    { id: 's1', candidate_id: 'c', client_account_id: 'k', sent_at_ms: NOW - 200 * DAY },
    { candidate_id: 'c', client_account_id: 'k' },
  ];
  assert.equal(findDoubleSubmission(subs.slice(0, 2), 'c', 'k', 180, NOW), undefined);
  assert.equal(findDoubleSubmission(subs, 'c', 'k', 180, NOW), '', 'ohne id und ohne sent_at → Konflikt mit leerer id');
  assert.equal(signatureRequestStatus([{ state: 'signed' }, { state: 'declined' }], undefined, 1, NOW), 'declined');
  assert.equal(signatureRequestStatus([{ state: 'signed' }], undefined, 1, NOW), 'completed');
  assert.equal(signatureRequestStatus([{ state: 'pending' }], NOW, 1, NOW), 'expired');
  assert.equal(signatureRequestStatus([{ state: 'signed' }, {}], undefined, 1, NOW), 'partially_signed');
  assert.equal(signatureRequestStatus([{}], undefined, 1, NOW), 'sent');
  assert.equal(signatureRequestStatus([], undefined, undefined, NOW), 'created');
});

test('compute_nachweis_billing / evaluate_billing_gate', () => {
  const b = computeNachweisBilling([{ hours: 8 }, { type: 'nacht', hours: 2 }, { type: 'unbekannt', hours: 1 }], 30, { nacht: 25 });
  assert.deepEqual(b.lines, [
    { category: 'regular', hours: 9, rate: 30, amount: 270 },
    { category: 'nacht', hours: 2, rate: 37.5, amount: 75 },
  ]);
  assert.equal(b.net_total, 345);
  assert.deepEqual(evaluateBillingGate({}), ['entleiher_signature_missing', 'no_time_entries']);
  assert.deepEqual(evaluateBillingGate({ entleiher_signed: true, signed_at_ms: 1, entries: [{ hours: -1 }] }), ['invalid_hours']);
  assert.deepEqual(evaluateBillingGate({ entleiher_signed: true, signed_at_ms: 1, entries: [{ hours: 0 }] }), ['no_billable_hours']);
  assert.deepEqual(evaluateBillingGate({ entleiher_signed: true, signed_at_ms: 1, entries: [{ hours: 1 }] }), []);
});

test('Rechnungsentwurf: Netto/Steuer in Cent, checked_scaled', () => {
  assert.equal(checkedScaled(12.345, 100), 1235);
  assert.equal(checkedScaled(Number.NaN, 100), 0);
  const inv = atsCanonicalInvoice('i1', 'sale_out', 'ACME', [atsInvoiceLine(1, 'x', 1000, 0.19)], NOW, 'source_placement_id', 'p1');
  assert.equal(inv.subtotal_cents, 100000);
  assert.equal(inv.tax_cents, 19000);
  assert.equal(inv.total_cents, 119000);
  assert.equal(inv.open_cents, 119000);
  assert.equal(inv.search_text, 'acme');
  assert.equal(inv.source_placement_id, 'p1');
  assert.equal(inv.state, 'draft');
});
