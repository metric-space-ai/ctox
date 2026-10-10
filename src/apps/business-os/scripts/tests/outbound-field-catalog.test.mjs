import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  customFieldKey, fieldGroupsFor, isCustomFieldKey, normalizeCustomFields, normalizeFieldLabels,
} from '../../customer-modules/outbound-lead-generation/field-catalog.mjs';

const base = [
  { id: 'company', label: 'Unternehmen', fields: [['firma_name', 'Firmenname'], ['firma_fax', 'Fax']] },
  { id: 'contact', label: 'Ansprechpartner', fields: [['person_titel', 'Titel']] },
];

test('operator names, switched-off fields and own fields shape the groups', () => {
  const groups = fieldGroupsFor(base, {
    labels: { firma_name: 'Firmierung' },
    disabled: new Set(['firma_fax']),
    custom: [{ key: 'custom_zertifikate', label: 'Zertifikate', area: 'company', description: '' }],
  });
  assert.deepEqual(groups.map((group) => group.label), ['Unternehmen', 'Ansprechpartner', 'Eigene Felder']);
  assert.deepEqual(groups[0].fields, [['firma_name', 'Firmierung']]);
  assert.deepEqual(groups[2].fields, [['custom_zertifikate', 'Zertifikate']]);
});

test('own field keys are stable, unique and never collide with built-in keys', () => {
  assert.equal(customFieldKey('Umsatz 2024 (Mio €)'), 'custom_umsatz_2024_mio');
  assert.equal(customFieldKey('Größe der Flotte'), 'custom_groesse_der_flotte');
  assert.equal(customFieldKey('Zertifikate', ['custom_zertifikate']), 'custom_zertifikate_2');
  assert.equal(customFieldKey('!!!'), 'custom_feld');
  assert.ok(isCustomFieldKey(customFieldKey('Zertifikate')));
  assert.equal(isCustomFieldKey('firma_name'), false);
});

test('stored field definitions are cleaned before use', () => {
  assert.deepEqual(normalizeCustomFields([
    { key: 'custom_a', label: ' A ', area: 'contact' },
    { key: 'custom_a', label: 'Doppelt' },
    { key: 'firma_name', label: 'Kein eigenes Feld' },
    { key: 'custom_b', label: '' },
  ]), [{ key: 'custom_a', label: 'A', area: 'contact', description: '' }]);
  assert.deepEqual(normalizeFieldLabels({ firma_name: ' Firmierung ', 'x y': 'bad', firma_fax: '' }), { firma_name: 'Firmierung' });
});
