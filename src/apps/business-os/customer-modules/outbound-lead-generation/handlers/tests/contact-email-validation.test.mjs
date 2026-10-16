// Kontakt-E-Mail-Prüfung als App-Logik (Strang OLG3). Die Fälle 1–13 sind die Rust-Tests aus
// contact_email_validation.rs:1400-2108 (Rust-Stand 4794af03d), 1:1 übertragen; danach Handler-Ende-zu-Ende über
// die App-Scrape-Registry (process.run-Attrappe), Sweep-Trigger und Aufrufer (Writeback/Personenrecherche).
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  INCONCLUSIVE_LABEL, RETRY_AFTER_MS, VALIDATE_COMMAND, applyEmailVerdicts, completeAfterNativeEmailValidation, leadDueForSweep,
  mergedEmailReceipts, reconcileReceiptedEmailStatus, validateLeadContacts, verdictFromRecords,
} from '../_lib/contact-email-validation.js';
import {
  alignLeadEmailValidation, emailsNeedingValidation, invalidateChangedEmailAddresses, parseVerdictNote, restoreNativeEmailVerdicts,
} from '../_lib/research-field-status.js';
import { isEmailVerdict } from '../_lib/research-outcome-patch.js';
import { LEAD_COLLECTION } from '../_lib/update-digest.js';
import { WITNESSES } from '../_lib/field-status-witnesses.js';
import { cmd } from './_ctx.mjs';
import { scrapeCtx, seedTarget } from './_scrape.mjs';
import validateHandler from '../outbound.contacts.email_validate.js';
import sweepHandler from '../outbound.contacts.email_validation_sweep.js';

const clone = (v) => JSON.parse(JSON.stringify(v));
const verdict = (email, valid) => ({
  email, kind: valid ? 'valid' : 'invalid', note: `EXPERTE.de verdict: ${email} | ${valid ? 'Gültig' : 'Ungültig'}`,
  source_url: 'https://www.experte.de/email-pruefen', run_id: 'scrape_run-test',
});
const unknown = (email) => ({
  email, kind: INCONCLUSIVE_LABEL, note: `EXPERTE.de verdict: ${email} | Unbekannt`,
  source_url: 'https://www.experte.de/email-pruefen', run_id: 'scrape_run-test',
});
const setPass = (lead, checked) => {
  lead.payload ??= {};
  lead.payload.email_validation_pass ??= {};
  lead.payload.email_validation_pass.checked = checked;
};

test('1 the verdict note is read without mistaking Ungültig for Gültig (ref :1402)', () => {
  assert.deepEqual(parseVerdictNote('EXPERTE.de verdict: info@weicon.de | Gültig'), ['info@weicon.de', 'valid']);
  assert.deepEqual(parseVerdictNote('EXPERTE.de verdict: R.Weidling@Weicon.de | Ungültig'), ['r.weidling@weicon.de', 'invalid']);
  assert.deepEqual(parseVerdictNote('EXPERTE.de verdict: x@firma.de | invalid'), ['x@firma.de', 'invalid']);
  assert.deepEqual(parseVerdictNote('EXPERTE.de verdict: m.grabbe@kiesow.org | Unbekannt'), ['m.grabbe@kiesow.org', INCONCLUSIVE_LABEL]);
  assert.equal(parseVerdictNote('keine Aussage'), null);
  assert.equal(parseVerdictNote('EXPERTE.de verdict: max.muster@example.de | Ungültig'), null);
});

test('INV-MAIL1 (2) a verdict for another address is ignored (ref :1432)', () => {
  const records = [{ field: 'person_email_validation', value: 'valid', source_url: 'https://www.experte.de/email-pruefen',
    note: 'EXPERTE.de verdict: info@beiersdorf.com | Gültig' }];
  assert.equal(verdictFromRecords(records, 'info@weicon.de', 'r1'), null);
  const found = verdictFromRecords(records, 'INFO@beiersdorf.com', 'r1');
  assert.equal(found.kind, 'valid');
  assert.equal(found.run_id, 'r1');
});

test('3 only unchecked real addresses are queued (ref :1446)', () => {
  const lead = { contacts: [
    { name: 'Ralph Weidling', person_email: 'r.weidling@weicon.de' },
    { name: 'Ann-Katrin Weidling', email: 'a.weidling@weicon.de', person_email_validation: 'valid' },
    { name: 'Nicht geprueft', email: 's.beilmann@weicon.de', person_email_validation: 'no_match' },
    { name: 'Doppelt', person_email: 'R.Weidling@weicon.de' },
    { name: 'Probe', person_email: 'max.muster@example.de' },
    { name: 'Ohne Adresse' },
  ] };
  assert.deepEqual(emailsNeedingValidation(lead, 4), ['r.weidling@weicon.de', 's.beilmann@weicon.de']);
});

test('4 a verdict lands on the contact that owns the address (ref :1466)', () => {
  const lead = {
    contacts: [{ person_key: 'p-ralph', name: 'Ralph Weidling', person_email: 'r.weidling@weicon.de' }, { person_key: 'p-sascha', name: 'Sascha Beilmann' }],
    evidence: [],
    field_status: { person_email_validation: { status: 'no_match', reason: 'Sandbox' } },
  };
  assert.equal(applyEmailVerdicts(lead, [verdict('r.weidling@weicon.de', true)]), 1);
  assert.equal(lead.contacts[0].person_email_validation, 'valid');
  assert.equal(lead.person_field_status['p-ralph'].person_email_validation.status, 'verified');
  assert.deepEqual(lead.contacts[0].field_status.person_email_validation, lead.person_field_status['p-ralph'].person_email_validation);
  assert.equal(lead.contacts[1].person_email_validation, undefined);
  assert.equal(lead.evidence.length, 1);
  assert.equal(lead.evidence[0].person_key, 'p-ralph');
  assert.equal(lead.evidence[0].source_id, 'experte.de');
  assert.equal(lead.field_status.person_email_validation.status, 'verified');
  assert.equal(lead.field_status.person_email_validation.value, 'valid');
  applyEmailVerdicts(lead, [verdict('r.weidling@weicon.de', true)]);
  assert.equal(lead.evidence.length, 1);
});

test('INV-MAIL3 (5) a deliverable pattern address becomes the persons verified email (ref :1505)', () => {
  const open = { status: 'action_required', value: 'r.weidling@weicon.de',
    sources: [{ source_id: 'weicon.de', url: 'https://www.weicon.de/impressum', quote: 'info@weicon.de, s.beilmann@weicon.de' }],
    reason: 'aus dem Muster v.nachname@weicon.de gebildet' };
  const lead = {
    contacts: [
      { person_key: 'p-ralph', person_email: 'r.weidling@weicon.de', field_status: { person_email: clone(open) } },
      { person_key: 'p-anna', person_email: 'a.muster@weicon.de', field_status: { person_email: clone(open) } },
    ],
    person_field_status: { 'p-ralph': { person_email: clone(open) }, 'p-anna': { person_email: clone(open) } },
    evidence: [],
  };
  applyEmailVerdicts(lead, [verdict('r.weidling@weicon.de', true), verdict('a.muster@weicon.de', false)]);
  const ralph = lead.person_field_status['p-ralph'].person_email;
  assert.equal(ralph.status, 'verified');
  assert.equal(ralph.value, 'r.weidling@weicon.de');
  assert.equal(ralph.sources.length, 2, 'the pattern source stays, the verdict joins it');
  assert.equal(ralph.sources[1].source_id, 'experte.de');
  assert.deepEqual(lead.contacts[0].field_status.person_email, ralph);
  assert.equal(lead.person_field_status['p-anna'].person_email.status, 'action_required');
  assert.equal(lead.contacts[1].field_status.person_email.status, 'action_required');
  const other = {
    contacts: [{ person_key: 'p1', person_email: 'x@weicon.de' }],
    person_field_status: { p1: { person_email: { status: 'action_required', value: 'y@weicon.de' } } },
  };
  applyEmailVerdicts(other, [verdict('x@weicon.de', true)]);
  assert.equal(other.person_field_status.p1.person_email.status, 'action_required');  // Zusatz OLG3 (Mutationsbeleg): die geprüfte, aber unzustellbare Musteradresse derselben Person bleibt offen.
  const undeliverable = {
    contacts: [{ person_key: 'p1', person_email: 'x@weicon.de', field_status: { person_email: { status: 'action_required', value: 'x@weicon.de' } } }],
    person_field_status: { p1: { person_email: { status: 'action_required', value: 'x@weicon.de' } } },
  };
  applyEmailVerdicts(undeliverable, [verdict('x@weicon.de', false)]);
  assert.equal(undeliverable.person_field_status.p1.person_email.status, 'action_required');
  assert.equal(undeliverable.contacts[0].field_status.person_email.status, 'action_required');
});

test('6 an undeliverable address is answered but never marked valid (ref :1561)', () => {
  const lead = { contacts: [{ person_key: 'p1', person_email: 'falsch@weicon.de' }] };
  applyEmailVerdicts(lead, [verdict('falsch@weicon.de', false)]);
  assert.equal(lead.contacts[0].person_email_validation, 'invalid');
  assert.equal(lead.field_status.person_email_validation.value, 'invalid');
});

test('7 a later worker placeholder cannot undo a native verdict for the same person and email (ref :1574)', () => {
  const checked = { contacts: [{ person_key: 'p1', person_email: 'a@weicon.de' }] };
  applyEmailVerdicts(checked, [verdict('a@weicon.de', true)]);
  setPass(checked, [{ email: 'a@weicon.de', valid: true, run_id: 'scrape_run-test' }]);
  const previous = clone(checked.person_field_status);
  checked.field_status.person_email_validation = { status: 'action_required', reason: 'Daemon prueft spaeter' };
  checked.person_field_status.p1.person_email_validation = { status: 'action_required', person_key: 'p1' };
  checked.contacts[0].field_status.person_email_validation = { status: 'action_required' };
  const restoredLead = clone(checked);
  assert.equal(restoreNativeEmailVerdicts(restoredLead, previous).length, 1);
  assert.equal(restoredLead.field_status.person_email_validation.status, 'verified');
  assert.equal(restoredLead.person_field_status.p1.person_email_validation.value, 'valid');
  assert.equal(restoredLead.contacts[0].field_status.person_email_validation.value, 'valid');

  const differentEmail = clone(restoredLead);
  differentEmail.contacts[0].person_email = 'new@weicon.de';
  differentEmail.field_status.person_email_validation = { status: 'action_required' };
  assert.equal(restoreNativeEmailVerdicts(differentEmail, previous).length, 0);
  assert.equal(differentEmail.field_status.person_email_validation.status, 'action_required');

  const differentPerson = clone(restoredLead);
  differentPerson.contacts[0].person_key = 'p2';
  differentPerson.field_status.person_email_validation = { status: 'action_required' };
  assert.equal(restoreNativeEmailVerdicts(differentPerson, previous).length, 0);

  const forged = clone(restoredLead);
  forged.payload.email_validation_pass.checked = [];
  forged.field_status.person_email_validation = { status: 'action_required' };
  assert.equal(restoreNativeEmailVerdicts(forged, previous).length, 0);
});

test('8 a correction citing the native verdict is kept but cannot raise it (ref :1650)', () => {
  const checked = { contacts: [{ person_key: 'p1', person_email: 'a@weicon.de' }] };
  const negative = verdict('a@weicon.de', false);
  applyEmailVerdicts(checked, [negative]);
  setPass(checked, [{ email: 'a@weicon.de', valid: false, run_id: 'scrape_run-test' }]);
  const previous = clone(checked.person_field_status);
  const deliver = (lead, status) => {
    lead.person_field_status.p1.person_email_validation = clone(status);
    lead.contacts[0].field_status.person_email_validation = clone(status);
    lead.field_status.person_email_validation = clone(status);
  };
  const corrected = clone(checked);
  deliver(corrected, { status: 'action_required', person_key: 'p1', attempts: [{ kind: 'scrape', result: negative.note }] });
  assert.deepEqual(restoreNativeEmailVerdicts(corrected, previous), []);
  for (const s of [corrected.person_field_status.p1.person_email_validation, corrected.contacts[0].field_status.person_email_validation,
    corrected.field_status.person_email_validation]) assert.equal(s.status, 'action_required');

  const foreign = clone(checked);
  deliver(foreign, { status: 'action_required', person_key: 'p1', sources: [{ quote: 'EXPERTE.de verdict: b@weicon.de | Ungültig' }] });
  assert.deepEqual(restoreNativeEmailVerdicts(foreign, previous), ['p1']);
  assert.equal(foreign.person_field_status.p1.person_email_validation.via, 'native_email_validation');

  const raised = clone(checked);
  deliver(raised, { status: 'verified', value: 'valid', person_key: 'p1', sources: [{ quote: negative.note }] });
  assert.deepEqual(restoreNativeEmailVerdicts(raised, previous), ['p1']);
  assert.equal(raised.person_field_status.p1.person_email_validation.value, 'invalid');
});

test('INV-MAIL2 (9) a catch-all unknown is checked but never invalid and never rechecked (ref :1715)', () => {
  const lead = { research_status: 'needs_review', contacts: [{ person_key: 'p-grabbe-michael', person_email: 'm.grabbe@kiesow.org' }], payload: {} };
  assert.deepEqual(emailsNeedingValidation(lead, 4), ['m.grabbe@kiesow.org']);
  const v = unknown('m.grabbe@kiesow.org');
  assert.equal(applyEmailVerdicts(lead, [v]), 1);
  const contact = lead.contacts[0];
  assert.equal(contact.person_email_validation, INCONCLUSIVE_LABEL);
  for (const s of [contact.field_status.person_email_validation, lead.person_field_status['p-grabbe-michael'].person_email_validation]) {
    assert.equal(s.status, 'action_required');
    assert.equal(s.value, null);
    assert.equal(s.verdict, INCONCLUSIVE_LABEL);
    assert.equal(s.validated_email, 'm.grabbe@kiesow.org');
  }
  assert.notEqual(lead.field_status.person_email_validation.value, 'invalid');
  assert.ok(lead.evidence.every((e) => e.evidence_gate.evidence_eligible === false));
  assert.deepEqual(emailsNeedingValidation(lead, 4), []);
  lead.payload.email_validation_pass = { at_ms: 0, checked: [{ email: v.email, valid: false, verdict: INCONCLUSIVE_LABEL, run_id: v.run_id }] };
  assert.equal(leadDueForSweep(lead, RETRY_AFTER_MS * 10), false);
  assert.ok(isEmailVerdict(INCONCLUSIVE_LABEL));
  const previous = clone(lead.person_field_status);
  const placeholder = clone(lead);
  placeholder.person_field_status['p-grabbe-michael'].person_email_validation = { status: 'action_required', person_key: 'p-grabbe-michael' };
  placeholder.contacts[0].person_email_validation = null;
  assert.deepEqual(restoreNativeEmailVerdicts(placeholder, previous), ['p-grabbe-michael']);
  assert.equal(placeholder.contacts[0].person_email_validation, INCONCLUSIVE_LABEL);
  assert.deepEqual(emailsNeedingValidation(placeholder, 4), []);
});

test('10 the lead-level email check never shows another persons verdict (ref :1777)', () => {
  const lead = {
    contacts: [{ person_key: 'p-grabbe-michael', person_email: 'm.grabbe@kiesow.org' }, { person_key: 'p-wallbaum-katja', person_email: 'k.wallbaum@kiesow.org' }],
    field_status: {
      person_email: { status: 'verified', value: 'k.wallbaum@kiesow.org', person_key: 'p-wallbaum-katja' },
      person_nachname: { status: 'verified', value: 'Wallbaum', person_key: 'p-wallbaum-katja' },
    },
  };
  applyEmailVerdicts(lead, [verdict('m.grabbe@kiesow.org', false)]);
  assert.equal(lead.field_status.person_email_validation.validated_email, 'm.grabbe@kiesow.org');
  assert.ok(alignLeadEmailValidation(lead));
  assert.equal(lead.field_status.person_email_validation, undefined);
  assert.equal(alignLeadEmailValidation(lead), false);
  applyEmailVerdicts(lead, [unknown('k.wallbaum@kiesow.org')]);
  alignLeadEmailValidation(lead);
  const status = lead.field_status.person_email_validation;
  assert.equal(status.person_key, 'p-wallbaum-katja');
  assert.equal(status.validated_email, 'k.wallbaum@kiesow.org');
  assert.equal(status.verdict, INCONCLUSIVE_LABEL);
  applyEmailVerdicts(lead, [verdict('m.grabbe@kiesow.org', true)]);
  alignLeadEmailValidation(lead);
  assert.equal(lead.field_status.person_email_validation.person_key, 'p-wallbaum-katja');
  const keyless = { contacts: [{ person_key: 'p1', person_email: 'a@weicon.de' }, { person_key: 'p2', person_email: 'b@weicon.de' }] };
  applyEmailVerdicts(keyless, [verdict('a@weicon.de', false), verdict('b@weicon.de', true)]);
  assert.equal(alignLeadEmailValidation(keyless), false);
  assert.equal(keyless.field_status.person_email_validation.value, 'valid');
});

const gapLead = () => ({
  contacts: [{ person_key: 'p1', person_email: 'a@weicon.de' }],
  research_status: 'needs_review',
  payload: { native_research_terminal_status: 'needs_review', native_research_requested_fields: ['firma_name', 'person_email_validation'], native_research_rejections_count: 0 },
  field_status: { firma_name: { status: 'verified', value: 'Weicon' }, person_email_validation: { status: 'action_required' } },
});

test('INV-MAIL5 (11) a native verdict completes only a receipted final email gap (ref :1839)', () => {
  const lead = gapLead();
  applyEmailVerdicts(lead, [verdict('a@weicon.de', true)]);
  assert.ok(completeAfterNativeEmailValidation(lead));
  assert.equal(lead.research_status, 'completed');
  const reopen = (l) => { l.research_status = 'needs_review'; l.payload.native_research_terminal_status = 'needs_review'; return l; };
  const anotherAddress = reopen(clone(lead));
  anotherAddress.contacts.push({ person_email: 'b@weicon.de' });
  assert.equal(completeAfterNativeEmailValidation(anotherAddress), false);
  const anotherOpenField = reopen(clone(lead));
  anotherOpenField.field_status.firma_name = { status: 'action_required' };
  assert.equal(completeAfterNativeEmailValidation(anotherOpenField), false);
  const older = reopen(clone(lead));
  delete older.payload.native_research_rejections_count;
  assert.equal(completeAfterNativeEmailValidation(older), false);
});

const legacyPair = () => ({
  contacts: [
    { person_key: 'sellify-person-41504', person_email: 'a@biogen.com', person_email_validation: 'valid' },
    { person_key: 'sellify-person-41503', person_email: 'b@biogen.com', person_email_validation: 'valid' },
  ],
  person_field_status: null,
  field_status: { person_email_validation: { status: 'action_required', person_key: 'sellify-person-41503', reason: 'CTOX-Daemon validiert automatisch via experte.de' } },
  payload: { email_validation_pass: { checked: [
    { email: 'a@biogen.com', valid: true, run_id: 'scrape_run-aaa' },
    { email: 'b@biogen.com', valid: true, run_id: 'scrape_run-bbb' },
  ] } },
  research_status: 'needs_review',
});

test('INV-MAIL6 (12) an old two-person receipt repairs only the email field (ref :1901)', () => {
  const legacy = legacyPair();
  const repaired = clone(legacy);
  assert.equal(reconcileReceiptedEmailStatus(repaired), 2);
  assert.equal(repaired.person_field_status['sellify-person-41504'].person_email_validation.run_id, 'scrape_run-aaa');
  assert.equal(repaired.person_field_status['sellify-person-41503'].person_email_validation.run_id, 'scrape_run-bbb');
  assert.equal(repaired.field_status.person_email_validation.status, 'verified');
  assert.equal(repaired.research_status, 'needs_review');
  assert.deepEqual(repaired.payload, legacy.payload);
  assert.equal(reconcileReceiptedEmailStatus(repaired), 0);

  const firstStatus = clone(repaired.person_field_status['sellify-person-41504'].person_email_validation);
  const oneAlreadyBound = clone(legacy);
  oneAlreadyBound.person_field_status = { 'sellify-person-41504': { person_email_validation: clone(firstStatus) } };
  oneAlreadyBound.contacts[0].field_status = { person_email_validation: clone(firstStatus) };
  assert.equal(reconcileReceiptedEmailStatus(oneAlreadyBound), 1);
  assert.equal(oneAlreadyBound.field_status.person_email_validation.status, 'verified');
  assert.equal(oneAlreadyBound.research_status, 'needs_review');

  const unbound = clone(legacy);
  const forgedStatus = { ...clone(firstStatus), run_id: 'scrape_run-other' };
  unbound.person_field_status = { 'sellify-person-41504': { person_email_validation: clone(forgedStatus) } };
  unbound.contacts[0].field_status = { person_email_validation: clone(forgedStatus) };
  assert.equal(reconcileReceiptedEmailStatus(unbound), 1);
  assert.equal(unbound.field_status.person_email_validation.status, 'action_required');

  const changedEmail = clone(legacy);
  changedEmail.contacts[1].person_email = 'new@biogen.com';
  assert.equal(reconcileReceiptedEmailStatus(changedEmail), 1);
  assert.equal(changedEmail.field_status.person_email_validation.status, 'action_required');
  assert.equal(changedEmail.person_field_status['sellify-person-41503'], undefined);

  const duplicateKey = clone(legacy);
  duplicateKey.contacts[1].person_key = 'sellify-person-41504';
  assert.equal(reconcileReceiptedEmailStatus(duplicateKey), 0);

  const mismatched = clone(legacy);
  mismatched.payload.email_validation_pass.checked[1].valid = false;
  assert.equal(reconcileReceiptedEmailStatus(mismatched), 1);
  assert.equal(mismatched.field_status.person_email_validation.status, 'action_required');

  const unrelated = clone(legacy);
  unrelated.field_status.person_email_validation.reason = 'Widerspruch in externen Quellen';
  assert.equal(reconcileReceiptedEmailStatus(unrelated), 0);

  // ABWEICHUNG-Beleg: Laufkennung des App-Läufers (run-<uuid>) gilt wie scrape_run-…; anderes Präfix nicht.
  const appRun = clone(legacy);
  appRun.payload.email_validation_pass.checked[0].run_id = 'run-00000000-0000-4000-8000-000000000001';
  appRun.payload.email_validation_pass.checked[1].run_id = 'other-1';
  assert.equal(reconcileReceiptedEmailStatus(appRun), 1);
});

test('13 a changed person address invalidates the old verdict before completion (ref :1997)', () => {
  const lead = {
    contacts: [{ person_key: 'p1', person_email: 'old@weicon.de' }],
    research_status: 'needs_review',
    payload: { native_research_terminal_status: 'needs_review', native_research_requested_fields: ['person_email_validation'], native_research_rejections_count: 0 },
  };
  applyEmailVerdicts(lead, [verdict('old@weicon.de', true)]);
  const previousContacts = clone(lead.contacts);
  lead.contacts[0].person_email = 'new@weicon.de';
  assert.equal(invalidateChangedEmailAddresses(lead, previousContacts), 1);
  assert.equal(lead.contacts[0].person_email_validation, undefined);
  assert.equal(lead.person_field_status.p1.person_email_validation, undefined);
  assert.equal(lead.field_status.person_email_validation.status, 'action_required');
  assert.deepEqual(emailsNeedingValidation(lead, 4), ['new@weicon.de']);
  assert.equal(completeAfterNativeEmailValidation(lead), false);
});

test('14 five-address batches keep the first valid receipt and lead verdict (ref :2032)', () => {
  const lead = { contacts: [0, 1, 2, 3, 4].map((i) => ({ person_key: `p${i}`, person_email: `p${i}@weicon.de` })) };
  const first = [0, 1, 2, 3].map((i) => verdict(`p${i}@weicon.de`, i === 0));
  applyEmailVerdicts(lead, first);
  lead.payload = { email_validation_receipts: mergedEmailReceipts(lead, first) };
  setPass(lead, first.map((v) => ({ email: v.email, valid: v.kind === 'valid', run_id: v.run_id })));
  const last = verdict('p4@weicon.de', false);
  applyEmailVerdicts(lead, [last]);
  lead.payload.email_validation_receipts = mergedEmailReceipts(lead, [last]);
  setPass(lead, [{ email: last.email, valid: false, run_id: last.run_id }]);
  const previous = clone(lead.person_field_status);
  for (let i = 0; i < 5; i += 1) {
    lead.contacts[i].field_status.person_email_validation = { status: 'action_required' };
    lead.person_field_status[`p${i}`].person_email_validation = { status: 'action_required' };
  }
  lead.field_status.person_email_validation = { status: 'action_required' };
  assert.equal(restoreNativeEmailVerdicts(lead, previous).length, 5);
  assert.equal(lead.field_status.person_email_validation.value, 'valid');
  assert.equal(lead.person_field_status.p0.person_email_validation.value, 'valid');
});

test('15 the sweep checks idle leads with unchecked addresses once per window (ref :2077)', () => {
  const now = 100 * 3600 * 1000;
  const lead = (status, passAt) => {
    const l = { research_status: status, contacts: [{ person_email: 'a.weidling@weicon.de', person_email_validation: 'no_match' }], payload: {} };
    if (passAt != null) l.payload.email_validation_pass = { at_ms: passAt };
    return l;
  };
  assert.equal(leadDueForSweep(lead('needs_review', null), now), true);
  assert.equal(leadDueForSweep(lead('running', null), now), false);
  assert.equal(leadDueForSweep(lead('queued', null), now), false);
  assert.equal(leadDueForSweep(lead('completed', now - 3600 * 1000), now), false);
  assert.equal(leadDueForSweep(lead('completed', now - RETRY_AFTER_MS), now), true);
  assert.equal(leadDueForSweep({ research_status: 'completed', contacts: [{ person_email: 'a.weidling@weicon.de', person_email_validation: 'valid' }] }, now), false);
});

// ------------------------------------------------------------------ Handler Ende-zu-Ende (Prüflauf über die App-Registry)

const EXPERTE_SCRIPT = 'console.log(JSON.stringify({records: []}));\n';
function fenceCaps(held = new Set()) {
  return {
    held,
    'fence.claim': async ({ key }) => (held.has(key) ? { token: null } : (held.add(key), { token: key })),
    'fence.release': async ({ token }) => { held.delete(token); return { ok: true }; },
  };
}
/** process.run answers like the experte-de script: one verdict record for the address in CTOX_SCRAPE_INPUT_JSON (§16.48: im Host-Dokument auf stdin). */
function experteRun(fs, labels) {
  return async (args, host) => {
    const email = JSON.parse(host.env.CTOX_SCRAPE_INPUT_JSON).email;
    const label = labels[email];
    if (label == null) return { exit: 0, stdout: { records: [] } };
    return { exit: 0, stdout: { records: [{ field: 'person_email_validation', value: label === 'Gültig' ? 'valid' : 'invalid',
      source_url: 'https://www.experte.de/email-pruefen', note: `EXPERTE.de verdict: ${email} | ${label}` }] } };
  };
}

async function harness({ lead, labels, held }) {
  let fsRef = null;
  const fence = fenceCaps(held);
  const { t, fs, procCalls } = scrapeCtx({
    now: 5_000,
    caps: { 'fence.claim': fence['fence.claim'], 'fence.release': fence['fence.release'] },
    run: async (args, host) => experteRun(fsRef, labels)(args, host),
    data: { [LEAD_COLLECTION]: { [lead.id]: lead } },
  });
  fsRef = fs;
  await seedTarget(t.ctx, { target_key: 'experte-de', start_url: 'https://www.experte.de/email-pruefen', script: EXPERTE_SCRIPT, config: { expected_min_records: 1 } });
  return { t, fs, procCalls, fence };
}

test('handler: a writeback-ready lead gets its verdicts, receipts and completion through the app scrape runner', async () => {
  const lead = { id: 'lead_1', ...gapLead() };
  lead.contacts.push({ name: 'Probe', person_email: 'max.muster@example.de' });
  const { t, procCalls, fence } = await harness({ lead, labels: { 'a@weicon.de': 'Gültig' } });
  const out = await validateHandler(cmd(VALIDATE_COMMAND, { record_id: 'lead_1' }, { record_id: 'lead_1' }), t.ctx);
  assert.deepEqual(out, { ok: true, record_id: 'lead_1', changed: 1 });
  assert.equal(procCalls.length, 1, 'documentation host is never checked');
  const stored = t.doc(LEAD_COLLECTION, 'lead_1');
  assert.equal(stored.contacts[0].person_email_validation, 'valid');
  assert.equal(stored.person_field_status.p1.person_email_validation.run_id.startsWith('run-'), true);
  assert.equal(stored.research_status, 'completed');
  assert.equal(stored.payload.native_research_terminal_status, 'completed');
  assert.equal(stored.research_updated_at_ms, 5_000);
  const runId = stored.person_field_status.p1.person_email_validation.run_id;
  assert.deepEqual(stored.payload.email_validation_receipts, { 'a@weicon.de': { email: 'a@weicon.de', valid: true, verdict: 'valid', run_id: runId } });
  assert.deepEqual(stored.payload.email_validation_pass, { at_ms: 5_000, checked: [{ email: 'a@weicon.de', valid: true, verdict: 'valid', run_id: runId }], failed: [] });
  assert.ok(t.doc(WITNESSES, 'lead_1'), 'witnesses are rewritten in the same transaction (Preserve)');
  assert.equal(fence.held.size, 0, 'both fences are released');
  // A second pass finds nothing to check and writes nothing.
  const before = t.calls.writes.length;
  assert.equal((await validateHandler(cmd(VALIDATE_COMMAND, { record_id: 'lead_1' }), t.ctx)).changed, 0);
  assert.equal(t.calls.writes.length, before);
});

test('INV-MAIL4 handler: a run without a verdict is recorded as failure; a busy lead keeps its record unchanged', async () => {
  const lead = { id: 'lead_2', contacts: [{ person_key: 'p1', person_email: 'a@weicon.de' }, { person_key: 'p2', person_email: 'b@weicon.de' }], payload: {} };
  const { t } = await harness({ lead, labels: { 'a@weicon.de': 'Ungültig' } });
  assert.equal((await validateHandler(cmd(VALIDATE_COMMAND, { record_id: 'lead_2' }), t.ctx)).changed, 1);
  const stored = t.doc(LEAD_COLLECTION, 'lead_2');
  assert.equal(stored.contacts[0].person_email_validation, 'invalid');
  assert.equal(stored.contacts[1].person_email_validation, undefined);
  assert.deepEqual(stored.payload.email_validation_pass.failed.map((f) => f.email), ['b@weicon.de']);

  const busy = { id: 'lead_3', contacts: [{ person_key: 'p1', person_email: 'c@weicon.de' }], payload: {} };
  const h = await harness({ lead: busy, labels: { 'c@weicon.de': 'Gültig' }, held: new Set(['outbound-research:lead_3']) });
  await assert.rejects(validateHandler(cmd(VALIDATE_COMMAND, { record_id: 'lead_3' }), h.t.ctx), { code: 'conflict', message: 'lead lead_3 stayed busy; 1 verdicts not applied' });
  assert.deepEqual(h.t.doc(LEAD_COLLECTION, 'lead_3'), busy);
  // In-flight: a second command for the same lead does not run.
  const inFlight = await harness({ lead: { ...busy, id: 'lead_4' }, labels: {}, held: new Set(['email-validation:lead_4']) });
  assert.equal((await validateHandler(cmd(VALIDATE_COMMAND, { record_id: 'lead_4' }), inFlight.t.ctx)).in_flight, true);
  assert.equal(inFlight.procCalls.length, 0);
});

test('handler: the patch applies to the lead reloaded after the slow runs (ref :1240)', async () => {
  const lead = { id: 'lead_5', contacts: [{ person_key: 'p1', person_email: 'a@weicon.de' }], payload: {} };
  const { t } = await harness({ lead, labels: { 'a@weicon.de': 'Gültig' } });
  const run = async (ctx, email) => {
    await ctx.put(LEAD_COLLECTION, { ...lead, note: 'concurrent' });
    return verdict(email, true);
  };
  // validateLeadContacts reloads the lead after the runs, so the concurrent write is the new master: no conflict.
  assert.equal(await validateLeadContacts(t.ctx, 'lead_5', run), 1);
  assert.equal(t.doc(LEAD_COLLECTION, 'lead_5').note, 'concurrent');
});

test('sweep trigger dispatches one validation command per due lead', async () => {
  const now = 100 * 3600 * 1000;
  const { t } = await harness({ lead: { id: 'due', research_status: 'completed', contacts: [{ person_email: 'a@weicon.de' }], payload: {} }, labels: {} });
  t.setNow(now);
  await t.ctx.put(LEAD_COLLECTION, { id: 'busy', research_status: 'running', contacts: [{ person_email: 'b@weicon.de' }] });
  await t.ctx.put(LEAD_COLLECTION, { id: 'recent', research_status: 'completed', contacts: [{ person_email: 'c@weicon.de' }], payload: { email_validation_pass: { at_ms: now - 1000 } } });
  assert.deepEqual(await sweepHandler(cmd('trigger', {}), t.ctx), { ok: true, due: 1 });
  assert.deepEqual(t.calls.commands.map((c) => [c.type, c.payload.record_id]), [[VALIDATE_COMMAND, 'due']]);
});
