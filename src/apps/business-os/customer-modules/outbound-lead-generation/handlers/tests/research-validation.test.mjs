// Teilschritt 2 research_writeback: Prüfregeln (Beispiele aus den Rust-Kommentaren :3388-3392, :3449-3455).
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  crmAgrees, crmComparable, crmRecordKeys, validateSource, hostIsReservedForDocumentation, validateNoMatch, validateAttemptShape,
} from '../_lib/research-validation.js';

test('crm_agrees compares whole tokens', () => {
  assert.equal(crmAgrees('ca. 145', '45'), false);
  assert.equal(crmAgrees('+49 40', '+4940636841000'), false);
  assert.equal(crmAgrees('+49 8031/2434-15', '+498031243415'), true);
  assert.equal(crmAgrees('Musterstraße 1, 20095 Hamburg', 'Hamburg'), true);
  assert.equal(crmAgrees('x', 'x'), false);
  assert.equal(crmComparable('+49 40 63684-1000'), '4940636841000');
  assert.deepEqual(crmRecordKeys('person_position'), ['person_position', 'person_funktion']);
  assert.deepEqual(crmRecordKeys('unknown'), []);
});

test('source, reserved hosts, no_match and attempt rules', () => {
  assert.doesNotThrow(() => validateSource('firma_name', { source_id: 'nd', url: ' https://northdata.de/x ', quote: 'ACME GmbH' }));
  assert.throws(() => validateSource('f', { source_id: ' ', url: '', quote: 'q' }), /source_id must not be empty/);
  assert.throws(() => validateSource('f', { source_id: 's', url: 'https://a.de', quote: '' }), /quote must not be empty/);
  assert.throws(() => validateSource('f', { source_id: 's', url: 'northdata.de', quote: 'q' }), /invalid source URL/);
  assert.throws(() => validateSource('f', { source_id: 's', url: 'ftp://a.de', quote: 'q' }), /must be HTTP\(S\)/);
  assert.equal(hostIsReservedForDocumentation('example.com'), true);
  assert.equal(hostIsReservedForDocumentation('shop.example.org'), true);
  assert.equal(hostIsReservedForDocumentation('acme.test'), false);
  assert.throws(() => validateNoMatch('umsatz', { attempts: [{ kind: 'web_search' }, { kind: 'web_read' }] }), /at least 1 web_search and 2/);
  assert.doesNotThrow(() => validateNoMatch('umsatz', { attempts: [{ kind: 'web_search' }, { kind: 'web_read' }, { kind: 'browser_capture' }] }));
  assert.throws(() => validateAttemptShape('f', { kind: 'guess', query_or_url: 'x', at: 1 }), /unsupported attempt kind `guess`/);
  assert.throws(() => validateAttemptShape('f', { kind: 'web_read', query_or_url: ' ', at: 1 }), /query_or_url/);
  assert.throws(() => validateAttemptShape('f', { kind: 'web_read', query_or_url: 'u', at: null }), /at must not be null/);
});

test('CrmBaseline: records, check, agreeing source, hint (Teilschritt 4b, ref :3170)', async () => {
  const { CrmBaseline } = await import('../_lib/research-validation.js');
  const crm = CrmBaseline.fromResearchPayload({
    lead_id: 'lead_7',
    sellify_company: { contact_id: 4711, name: 'ACME GmbH', telefon: '+49 8031 2434-15', ort: ' ', mitarbeiter: 120 },
    known_person_records: [{ sellify_person_id: 'sellify-person-1', person_vorname: 'Ada' }, { sellify_person_id: ' ' }],
  });
  assert.deepEqual(crm.sortedKeys(), ['company/4711', 'company/lead_7', 'person/sellify-person-1']);
  assert.equal(crm.check('firma_name', 'https://acme.de', 'ACME'), null);
  assert.equal(crm.check('firma_name', ' SELLIFY://company/4711/ ', 'Firma ACME GmbH'), true);
  assert.equal(crm.check('firma_name', 'sellify://company/999', 'ACME GmbH'), false);
  assert.equal(crm.check('firma_ort', 'sellify://company/4711', 'Hamburg'), false);
  assert.equal(crm.check('mitarbeiter', 'sellify://company/lead_7', 'ca. 120 Mitarbeiter'), true);
  assert.deepEqual(crm.agreeingSource('firma_telefon', '+498031243415', null),
    { source_id: 'sellify', url: 'sellify://company/4711', quote: '+49 8031 2434-15', person_key: null, requires_credential: false, task_id: '', command_id: '' });
  assert.equal(crm.agreeingSource('person_vorname', 'Ada', 'sellify-person-1').url, 'sellify://person/sellify-person-1');
  assert.equal(crm.agreeingSource('person_vorname', 'Ada', null), null);
  assert.equal(crm.hint(), 'gueltig sind nur sellify://company/4711, sellify://person/sellify-person-1; das Zitat muss den dort gespeicherten Wert des Feldes enthalten');
  assert.equal(new CrmBaseline().hint(), 'der Auftrag bringt keinen Sellify-Datensatz mit');
});
