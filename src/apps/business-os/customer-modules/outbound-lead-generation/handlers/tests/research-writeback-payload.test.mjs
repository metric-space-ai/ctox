// Teilschritt 3 research_writeback: tolerantes Einlesen (Fälle aus den Rust-Kommentaren :44-49, :59-66, :180-189,
// :244-250, :291-299, :337-343, :420-427, :585-590).
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  parseResearchWritebackRequest, hoistTopLevelFieldEntries, unwrapItemCarriers, lenientFieldStatus, lenientSources,
  lenientValues, lenientObject, hostOfUrl, fieldKeyFromStr, mapRemove, serializeFieldStatus,
} from '../_lib/research-writeback-payload.js';

const base = { record_id: 'lead_1', module: 'outbound-lead-generation', research_command_id: 'cmd_r' };

test('envelope: missing / unknown / wrong type in serde order', () => {
  const err = (p) => { try { parseResearchWritebackRequest(p); } catch (e) { return e; } throw new Error('no error'); };
  assert.equal(err({ module: 'm' }).message, 'invalid outbound.lead.research_writeback payload: missing field `record_id`');
  assert.equal(err({ ...base }).message, 'invalid outbound.lead.research_writeback payload: missing field `field_status`');
  assert.equal(err({ ...base, field_status: {}, extra_key: 1 }).message,
    'invalid outbound.lead.research_writeback payload: unknown field `extra_key`, expected one of `record_id`, `module`, `research_command_id`, `gap_task_id`, `field_status`, `result`');
  assert.equal(err({ ...base, record_id: 5, field_status: {} }).message, 'invalid outbound.lead.research_writeback payload: invalid type: integer `5`, expected a string');
  assert.equal(err({ ...base, field_status: 7 }).message, 'invalid outbound.lead.research_writeback payload: field_status must be an object keyed by field, got 7');
  assert.equal(err({ item: base }).message, 'invalid outbound.lead.research_writeback payload: invalid type: sequence, expected struct ResearchWritebackRequest');
  assert.equal(err({ ...base, field_status: 3 }).code, 'invalid');
});

test('hoist: carriers, bare values, stray entries, result members (ref :50)', () => {
  const r = parseResearchWritebackRequest({
    ...base,
    entries: { gap_task_id: 'gap_1' },
    firma_telefon: '+49 40 1',
    firma_fruehere_namen: { status: 'no_match', reason: 'keine' },
    person_records: { item: [{ name: 'Ada' }] },
    field_status: { firma_name: { status: 'verified', value: 'ACME', sources: { item: [{ url: 'https://www.northdata.de/x', quote: 'ACME' }] } } },
  });
  assert.equal(r.gap_task_id, 'gap_1');
  assert.deepEqual(Object.keys(r.field_status), ['firma_fruehere_namen', 'firma_name']);
  assert.equal(r.field_status.firma_name.sources[0].source_id, 'northdata.de');
  assert.deepEqual(r.result.fields, { firma_telefon: '+49 40 1' });
  assert.deepEqual(r.result.person_records, [{ name: 'Ada' }]);
  // result-level stray keys move into result.fields — RUST-EIGENHEIT incl. custom_fields
  const r2 = parseResearchWritebackRequest({ ...base, field_status: {}, result: { person_vorname: 'Ada', custom_fields: { custom_x: { value: 1 } } } });
  assert.deepEqual(r2.result.fields, { person_vorname: { value: 'Ada' }, custom_fields: { custom_x: { value: 1 } } });
  assert.deepEqual(r2.result.custom_fields, {});
  // a non-object result disappears (default) instead of failing
  assert.deepEqual(parseResearchWritebackRequest({ ...base, field_status: {}, result: null }).result.fields, {});
});

test('field_status leniency: string JSON, list form, dropped entries, flatten extra (ref :193, :251)', () => {
  const fs = lenientFieldStatus(JSON.stringify([
    { field: 'umsatz', status: 'verified', value: 5, person_key: 'p', sources: '[{"host":"www.Bundesanzeiger.de","quote":2024,"requires_credential":"false"}]' },
    { key: 'mitarbeiter', status: '' },
    { name: 'firma_ort', status: 'verified', sources: [{ person_key: 3 }] },
    'junk',
  ]));
  assert.deepEqual(Object.keys(fs), ['umsatz']);
  assert.deepEqual(fs.umsatz.sources[0], { source_id: 'www.Bundesanzeiger.de', url: '', quote: '2024', person_key: null, requires_credential: false, task_id: '', command_id: '' });
  assert.deepEqual(fs.umsatz.extra, { person_key: 'p' });
  assert.deepEqual(Object.keys(serializeFieldStatus(fs.umsatz)), ['status', 'value', 'sources', 'attempts', 'reason', 'person_key']);
  assert.deepEqual(lenientFieldStatus(null), {});
  assert.deepEqual(lenientFieldStatus({ a: 'x', b: { status: 'no_match', attempts: { kind: 'web_search' } } }).b.attempts,
    [{ kind: 'web_search', query_or_url: '', result: null, artifact_path: '', at: null }]);
});

test('helpers: carriers, sources, values, object, host, field keys, swap_remove', () => {
  assert.deepEqual(unwrapItemCarriers({ a: { List: { x: 1 } } }), { a: [{ x: 1 }] });
  assert.deepEqual(unwrapItemCarriers({ value: [1] }), { value: [1] });
  assert.deepEqual(lenientSources({ sources: [{ url: 'https://a.de/x?y' }] })[0].source_id, 'a.de');
  assert.throws(() => lenientSources('"text"'), /not a string/);
  assert.deepEqual(lenientValues(''), []);
  assert.deepEqual(lenientValues({ values: { a: 1 } }), [{ a: 1 }]);
  assert.throws(() => lenientValues(5), /expected a list of objects, got 5/);
  assert.deepEqual(lenientObject('  '), {});
  assert.deepEqual(lenientObject([1]), [1]);
  assert.throws(() => lenientObject('[1]'), /expected an object of field entries/);
  assert.equal(hostOfUrl(' HTTPS://www.www.Example.de:8080/a#b '), 'example.de:8080');
  assert.equal(fieldKeyFromStr(' PLZ '), 'firma_plz');
  assert.equal(fieldKeyFromStr('GESCHÄFTSFÜHRUNG'), null);
  assert.equal(fieldKeyFromStr('geschäftsführung'), 'firma_geschaeftsfuehrung');
  const o = { a: 1, b: 2, c: 3, d: 4 };
  assert.equal(mapRemove(o, 'b'), 2);
  assert.deepEqual(Object.keys(o), ['a', 'd', 'c']);
  assert.deepEqual(Object.keys(hoistTopLevelFieldEntries({ items: { z: 1 }, record_id: 'r' })), ['record_id', 'z']);
});
