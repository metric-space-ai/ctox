// Teilschritt 4a research_writeback: Vorprüfungen (Fehlertexte wörtlich aus Rust).
import test from 'node:test';
import assert from 'node:assert/strict';
import { parseResearchWritebackRequest } from '../_lib/research-writeback-payload.js';
import {
  deriveResultFieldsFromFieldStatus, rejectEvidenceFreeWriteback, validateFieldStatusKeys, validateResultShape,
  validateResultFieldStatusConsistency, isFillerFieldStatus, parseRequestedFields,
} from '../_lib/research-writeback-checks.js';

const base = { record_id: 'l', module: 'outbound-lead-generation', research_command_id: 'c' };
const noCrm = { check: () => null };
const req = (extra) => parseResearchWritebackRequest({ ...base, ...extra });

test('derive result.fields from verified field_status (ref :1673)', () => {
  const r = req({ field_status: {
    firma_name: { status: ' Verified ', value: 'ACME', sources: [{ url: 'https://a.de', quote: 'ACME' }] },
    person_email: { status: 'verified', value: 'a@a.de', person_key: 'p1' },
    umsatz: { status: 'no_match', attempts: [] },
  } });
  deriveResultFieldsFromFieldStatus(r);
  assert.deepEqual(Object.keys(r.result.fields), ['firma_name', 'person_email']);
  assert.equal(r.result.fields.person_email.person_key, 'p1');
  assert.equal(r.result.fields.firma_name.sources[0].source_id, 'a.de');
});

test('evidence-free writeback is rejected; one attempt or person evidence suffices (ref :1707)', () => {
  assert.throws(() => rejectEvidenceFreeWriteback(req({ field_status: { umsatz: { status: 'no_match' } } })), /^Error: evidence-free research writeback rejected/);
  assert.doesNotThrow(() => rejectEvidenceFreeWriteback(req({ field_status: { umsatz: { status: 'no_match', attempts: [{ kind: 'web_search' }] } } })));
  assert.doesNotThrow(() => rejectEvidenceFreeWriteback(req({ field_status: {}, result: { person_field_status: { p1: { person_email: { status: 'verified' } } } } })));
  assert.equal(isFillerFieldStatus({ status: 'Unsupported' }), true);
  assert.equal(isFillerFieldStatus({ status: 'no_match', reason: ' TBD ' }), true);
  assert.equal(isFillerFieldStatus({ status: 'unsupported', attempts: [{}] }), false);
});

test('field_status keys must match requested fields exactly (ref :3072)', () => {
  assert.throws(() => parseRequestedFields(['firma_name', 'erfunden']), /unsupported person-research field `erfunden`/);
  assert.throws(() => validateFieldStatusKeys(['umsatz', 'firma_name'], { firma_name: {}, mitarbeiter: {} }),
    { message: 'field_status must cover all requested_fields exactly; missing=["umsatz"], unexpected=["mitarbeiter"]' });
  assert.doesNotThrow(() => validateFieldStatusKeys(['umsatz'], { umsatz: {} }));
});

test('result shape and consistency rules (ref :3612, :3715)', () => {
  const requested = ['firma_name', 'person_email'];
  const shape = (result) => () => validateResultShape({ person_records: [], evidence: [], ...result }, requested, noCrm);
  assert.throws(shape({ fields: { umsatz: {} } }), /contains a field that was not requested/);
  assert.throws(shape({ fields: { firma_name: 'ACME' } }), /must be a structured object, not free text/);
  assert.throws(shape({ fields: { person_email: { value: 'a@a.de' } } }), /result.fields.person_email requires person_key/);
  assert.throws(shape({ fields: {}, person_records: [{}] }), /person_records\[0\] requires person_key/);
  const ev = { field_key: 'firma_name', source_id: 's', url: 'sellify://company/1', quote: 'ACME' };
  assert.throws(shape({ fields: {}, evidence: [ev] }), /evidence\[0\] URL must be HTTP\(S\)/);
  assert.doesNotThrow(() => validateResultShape({ fields: {}, person_records: [], evidence: [ev] }, requested, { check: () => true }));
  assert.throws(shape({ fields: {}, evidence: [{ ...ev, url: 'https://a.de', quote: ' ' }] }), /evidence\[0\] requires quote/);
  const fs = (status) => req({ field_status: { person_email: status } }).field_status;
  assert.throws(() => validateResultFieldStatusConsistency({ fields: { person_email: { value: 'b@a.de', person_key: 'p' } } },
    fs({ status: 'verified', value: 'a@a.de' })), /does not match field_status value/);
  assert.throws(() => validateResultFieldStatusConsistency({ fields: { person_email: { person_key: 'p' } } },
    fs({ status: 'verified', value: 'a@a.de', sources: [{ url: 'https://a.de', person_key: 'q' }] })), /evidence person_key must match/);
  assert.throws(() => validateResultFieldStatusConsistency({ fields: {} }, fs({ status: 'no_match', value: 'x' })), /non-verified field `person_email` must not carry/);
  // RUST-EIGENHEIT: ' verified' (untrimmed) counts as non-verified here
  assert.throws(() => validateResultFieldStatusConsistency({ fields: {} }, fs({ status: ' verified', value: 'x' })), /non-verified/);
});
