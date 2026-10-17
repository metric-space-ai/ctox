// Teilschritt 6 research_writeback: Feldstatus-Zusammenführung, Personenprojektion, Prüfansicht, E-Mail-Urteile.
import test from 'node:test';
import assert from 'node:assert/strict';
import { CrmBaseline } from '../_lib/research-validation.js';
import {
  mergeFieldStatus, unionResearchKeys, previousResearchKeys, applyPersonFieldStatus, projectPersonFieldStatus, openPersonsForField,
  researchFieldIsAnswered, isRefutedNoMatch, nativeReviewViewFromWitnesses, invalidateChangedEmailAddresses, restoreNativeEmailVerdicts,
  alignLeadEmailValidation, parseVerdictNote, normalizeEmail, emailsNeedingValidation, contactPersonKeys,
} from '../_lib/research-field-status.js';

test('fillers never downgrade, key lists are unioned (ref :1021, :975)', () => {
  const merged = mergeFieldStatus({ umsatz: { status: 'verified' }, wz_code: { status: 'unsupported' } },
    { umsatz: { status: 'unsupported', reason: 'x' }, wz_code: { status: 'unsupported', reason: 'neu' }, ort: { status: 'tbd', reason: 'tbd' } });
  assert.deepEqual(merged, { umsatz: { status: 'verified' }, wz_code: { status: 'unsupported', reason: 'neu' }, ort: { status: 'tbd', reason: 'tbd' } });
  const lead = { payload: { researched_field_keys: ['b'], verified_field_keys: ['x'], unverified_field_keys: ['a'] } };
  const prev = previousResearchKeys({ payload: { researched_field_keys: ['a'], verified_field_keys: ['a'], unverified_field_keys: ['x', 'y'] } });
  unionResearchKeys(lead, prev);
  assert.deepEqual(lead.payload, { researched_field_keys: ['a', 'b'], verified_field_keys: ['a', 'x'], unverified_field_keys: ['y'] });
});

test('person statuses: per-person gate, sorted persons, projection onto contacts (ref :1053, :1382)', async () => {
  const lead = { contacts: [{ person_key: 'B', person_email: '' }, { person_key: 'A', sellify_person_id: '7' }] };
  const src = (pk, quote = 'b@acme.de') => ({ source_id: 'acme', url: 'https://acme.de/team', quote, person_key: pk });
  const rejections = await applyPersonFieldStatus(lead, {
    B: { person_email: { status: 'verified', value: 'b@acme.de', sources: [src('B')] }, firma_name: { status: 'verified' } },
    A: { person_email: { status: 'verified', value: 'a@acme.de', sources: [src('B')] }, person_telefon: { foo: 1 } },
    ' ': [],
  }, {}, ['person_email'], new CrmBaseline(), null);
  assert.deepEqual(rejections, [
    'person_field_status[B].firma_name: nur person_*-Felder sind personengebunden',
    'person_field_status[A].person_email: person_key in Beleg (B) und Personenschluessel nicht identisch',
    'person_field_status[A].person_telefon: ohne lesbaren status',
    'person_field_status[]: erwartet ein Objekt je person_key',
  ]);
  assert.equal(lead.person_field_status.B.person_email.person_key, 'B');
  projectPersonFieldStatus(lead);
  assert.equal(lead.contacts[0].person_email, 'b@acme.de');
  assert.equal(lead.contacts[0].field_status.person_email.status, 'verified');
  assert.deepEqual(openPersonsForField(lead, 'person_email'), ['A']);
  assert.equal(openPersonsForField(lead, 'umsatz'), null);
  assert.deepEqual(contactPersonKeys({ person_key: ' 7 ', person_id: 7 }), ['sellify-person-7', '7']);
});

test('refuted no_match and the witness-filtered review view (outbound_field_review.rs:583, :290)', () => {
  const status = { status: 'no_match', revision: { writeback_id: 'w', command_id: 'c' }, review: {
    schema: 'ctox.outbound.field_review.v1', verdict: 'refuted', claim_status: 'no_match', revision_ref: { writeback_id: 'w' },
    command_id: 'c', reason_code: 'contradicted_by_saved_source', review_attempt_id: 'r', attempt: 0, reviewed_at_ms: 1 } };
  assert.equal(isRefutedNoMatch(status), true);
  assert.equal(researchFieldIsAnswered(status), false);
  assert.equal(isRefutedNoMatch({ ...status, person_key: 'p' }), false);
  const lead = { field_status: { umsatz: status } };
  assert.equal(nativeReviewViewFromWitnesses(lead, []).field_status.umsatz.review, null);
  assert.deepEqual(nativeReviewViewFromWitnesses(lead, [{ scope: 'lead', person_key: '', field: 'umsatz', status }]).field_status.umsatz, status);
  assert.equal(researchFieldIsAnswered({ status: ' no_match ' }), true);
});

test('e-mail verdicts: invalidate, restore, align (contact_email_validation.rs)', () => {
  assert.deepEqual(parseVerdictNote('EXPERTE.de verdict: <Info@Weicon.de> | Ungültig'), ['info@weicon.de', 'invalid']);
  assert.deepEqual(parseVerdictNote('x verdict: a@b.de | Unbekannt'), ['a@b.de', 'unknown_catch_all']);
  assert.equal(normalizeEmail('max@example.de'), null);
  assert.deepEqual(emailsNeedingValidation({ contacts: [{ email: 'a@b.de' }, { email: 'A@b.de' }, { email: 'c@b.de', email_validation: 'valid' }] }, 4), ['a@b.de']);

  const lead = { contacts: [{ person_key: 'p', person_email: 'neu@acme.de', person_email_validation: 'valid', field_status: { person_email_validation: {} } }],
    field_status: { person_email_validation: { status: 'verified' } } };
  assert.equal(invalidateChangedEmailAddresses(lead, [{ person_key: 'p', person_email: 'alt@acme.de' }]), 1);
  assert.deepEqual(lead.contacts[0], { person_key: 'p', person_email: 'neu@acme.de', field_status: {} });
  assert.equal(lead.person_field_status, null); // RUST-EIGENHEIT IndexMut
  assert.equal(lead.field_status.person_email_validation.status, 'action_required');

  const native = { status: 'verified', value: 'valid', via: 'native_email_validation', run_id: 'run1', person_key: 'p', validated_email: 'a@acme.de' };
  const lead2 = { contacts: [{ person_key: 'p', person_email: 'a@acme.de' }],
    person_field_status: { p: { person_email_validation: { status: 'action_required', reason: 'daemon prueft' } } },
    field_status: { person_email: { status: 'verified', person_key: 'p' } },
    payload: { email_validation_pass: { checked: [{ email: 'a@acme.de', run_id: 'run1', valid: true }] } } };
  assert.deepEqual(restoreNativeEmailVerdicts(lead2, { p: { person_email_validation: native } }), ['p']);
  assert.equal(lead2.contacts[0].person_email_validation, 'valid');
  assert.deepEqual(lead2.person_field_status.p.person_email_validation, native);
  assert.equal(alignLeadEmailValidation(lead2), false);
  delete lead2.field_status.person_email_validation;
  assert.equal(alignLeadEmailValidation(lead2), true);
  assert.equal(lead2.field_status.person_email_validation.person_key, 'p');
});
