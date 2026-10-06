import assert from 'node:assert/strict';
import { test } from 'node:test';
import { optionalKeysForRequiredCheckbox } from '../../customer-modules/outbound-lead-generation/required-field-selection.mjs';

test('checked means required while persisted optional keys retain their existing meaning', () => {
  const old = new Set(['firma_fax', 'person_email', 'future_field']);
  const required = optionalKeysForRequiredCheckbox(old, 'person_email', true);
  assert.equal(required.has('person_email'), false); assert.equal(old.has('person_email'), true);
  assert.equal(required.has('future_field'), true);
  const optional = optionalKeysForRequiredCheckbox(required, 'firma_name', false);
  assert.equal(optional.has('firma_name'), true); assert.equal(required.has('firma_name'), false);
  assert.throws(() => optionalKeysForRequiredCheckbox(old, ' ', true), TypeError);
  assert.throws(() => optionalKeysForRequiredCheckbox(old, 'firma_name', 'true'), TypeError);
});
