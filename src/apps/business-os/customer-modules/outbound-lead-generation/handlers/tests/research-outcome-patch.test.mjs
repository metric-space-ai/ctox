// Teilschritt 5 research_writeback: Lead-Ergebnispatch (Beispiele aus den Rust-Kommentaren).
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  researchOutcomePatch, javascriptFingerprint, normalizeDomain, normalizeCountryToIso2, restoreGermanSpellingFromQuotes,
  samePersonByName, contactNameIsPlaceholder, deduplicateContacts, roleIsValid, mergeJsonObjectValues, candidatePersonEmail,
} from '../_lib/research-outcome-patch.js';

test('value normalisation (ref :1354, :1408, :1506, :1966)', () => {
  assert.equal(normalizeDomain('www.Aeroxon.de'), 'aeroxon.de');
  assert.equal(normalizeDomain('https://www.beiersdorf.de/path'), 'beiersdorf.de');
  assert.equal(normalizeDomain('localhost'), 'localhost');
  assert.equal(normalizeDomain('a b.de'), 'a b.de');
  assert.equal(normalizeCountryToIso2('Deutschland'), 'DE');
  assert.equal(normalizeCountryToIso2(' fr '), 'FR');
  assert.equal(normalizeCountryToIso2('Frankreich'), 'Frankreich');
  assert.equal(restoreGermanSpellingFromQuotes('Nuernberg', ['D-90451 Nürnberg']), 'Nürnberg');
  assert.equal(restoreGermanSpellingFromQuotes('Nuernberg', ['D-90451 Nuernberg']), null);
  assert.equal(javascriptFingerprint(''), (2166136261).toString(36));
  assert.equal(javascriptFingerprint('AB'), javascriptFingerprint('ab'));
  assert.equal(candidatePersonEmail({ person_records: [{ email: ' a@b.de ' }] }), 'a@b.de');
});

test('contact identity rules (ref :1205, :1269, :834)', () => {
  assert.equal(samePersonByName({ person_nachname: 'Bail' }, { person_vorname: 'Alena', person_nachname: 'Bail' }), true);
  assert.equal(samePersonByName({ person_vorname: 'Mark H.', person_nachname: 'Breitenfelder' }, { person_vorname: 'Mark', person_nachname: 'Breitenfelder' }), true);
  assert.equal(samePersonByName({ person_vorname: 'Olivier', person_nachname: 'Blondeau' }, { person_vorname: 'Philippe', person_nachname: 'Blondeau' }), false);
  assert.equal(contactNameIsPlaceholder({ person_vorname: 'test', person_nachname: 'test' }), true);
  assert.equal(contactNameIsPlaceholder({ person_vorname: 'n/a', person_nachname: 'n/a', email: 'x@y.de' }), false);
  const kept = deduplicateContacts([{ id: 'a', person_key: 'p' }, { id: 'a', person_key: 'p', phone: '1' }, { id: 'a', person_key: 'q' }]);
  assert.deepEqual(kept, [{ id: 'a', person_key: 'p', phone: '1' }, { id: 'a', person_key: 'q' }]);
  assert.equal(roleIsValid('Geschäftsführer', 'Max Muster', []), true);
  assert.equal(roleIsValid('Hamburg', 'Max Muster', ['Hamburg']), false);
  assert.equal(roleIsValid('Dr.', 'Max', []), false);
  assert.equal(roleIsValid('Max Muster', 'X', []), false);
  assert.deepEqual(mergeJsonObjectValues({ a: { b: 1 } }, { a: { c: 2 } }), { a: { b: 1, c: 2 } });
  assert.deepEqual(mergeJsonObjectValues([1], { a: 1 }), { a: 1 });
});

test('outcome patch: company data, keyed person fields, evidence and verified keys (ref :555)', () => {
  const existing = { id: 'lead_1', data: { firma_name: 'alt' }, contacts: [{ id: 'c1', person_key: 'p1', person_vorname: 'Thomas', person_nachname: 'Schauzu' }],
    selected_contact_ids: ['c1', 'gone'], evidence: [] };
  const result = { result: {
    fields: {
      firma_land: { value: 'Deutschland', candidates: [{ source_id: 'nd', source_url: 'https://northdata.de', quote: 'Land: Deutschland', value: 'Deutschland' }] },
      firma_ort: { value: 'Nuernberg', candidates: [{ source_id: 'nd', source_url: 'https://northdata.de/o', quote: 'D-90451 Nürnberg', value: 'Nuernberg' }] },
      person_vorname: { value: 'Hans-Robert', person_key: 'p2' },
      person_nachname: { value: 'Jacob', person_key: 'p2' },
      person_email: { value: 'x@y.de' },
      umsatz: { value: '' },
    },
    evidence: { item: [] },
    tool: 'web_stack',
  } };
  const patch = researchOutcomePatch(existing, result, 1000);
  assert.equal(patch.data.firma_land, 'DE');
  assert.equal(patch.data.firma_ort, 'Nürnberg');
  assert.deepEqual(patch.payload.unbound_person_field_keys, ['person_email']);
  assert.deepEqual(patch.payload.researched_field_keys, ['firma_land', 'firma_ort', 'person_vorname', 'person_nachname']);
  assert.deepEqual(patch.payload.verified_field_keys, ['firma_land', 'firma_ort']);
  assert.equal(patch.contacts.length, 2);
  assert.equal(patch.contacts[0].person_vorname, 'Thomas');
  assert.equal(patch.contacts[1].name, 'Hans-Robert Jacob');
  assert.match(patch.contacts[1].id, /^contact_[0-9a-z]+$/);
  assert.deepEqual(patch.selected_contact_ids, ['c1']);
  assert.equal(patch.research_status, 'needs_review');
  assert.equal(patch.payload.research_tool, 'web_stack');
  assert.equal(patch.evidence.length, 2);
  assert.equal(patch.evidence[0].label, 'nd');
});

test('outcome patch: known Sellify persons and researched person records merge (ref :956, :881)', () => {
  const existing = { id: 'lead_2', contacts: [{ id: 'imp', person_vorname: 'Mark H.', person_nachname: 'Breitenfelder', person_email_validation: 'valid' }] };
  const patch = researchOutcomePatch(existing, {
    known_person_records: [{ vorname: 'Mark', nachname: 'Breitenfelder', funktion: 'Geschäftsführer', sellify_person_id: '42' }],
    person_records: [{ person_key: '42', person_vorname: 'Mark', person_nachname: 'Breitenfelder', person_telefon: '+49 1',
      person_email_validation: 'no_match', evidence: [{ field: 'person_telefon', source_id: 's', source_url: 'https://a.de', quote: '+49 1' }] }],
  }, 5);
  assert.equal(patch.contacts.length, 1);
  const c = patch.contacts[0];
  assert.equal(c.id, 'contact_sellify_42');
  assert.equal(c.crm_known, true);
  // person_telefon bleibt leer: der Alias `phone` traegt den Wert (ref :1089)
  assert.equal(c.person_telefon, '');
  assert.equal(c.phone, '+49 1');
  assert.equal(c.person_email_validation, 'valid');
  assert.equal(patch.evidence[0].person_key, '42');
  assert.equal(patch.evidence[0].field_key, 'person_telefon');
});
