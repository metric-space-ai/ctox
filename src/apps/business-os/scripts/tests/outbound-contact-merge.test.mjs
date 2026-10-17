import test from 'node:test';
import assert from 'node:assert/strict';
import { mergeStoredContacts } from '../../customer-modules/outbound-lead-generation/contact-merge.mjs';

test('a stale browser contact list keeps contacts the server stored meanwhile', () => {
  const stored = [
    { id: 'c1', person_key: 'p-pfaffenbach-stefan', name: 'Stefan Pfaffenbach' },
    { id: 'c2', person_key: 'p-nold-soeren', name: 'Sören Nold' },
    { id: 'c3', person_key: 'p-luenskens-stephan', name: 'Stephan Luenskens' },
  ];
  const stale = [{ id: 'c1', person_key: 'p-pfaffenbach-stefan', name: 'Stefan Pfaffenbach', sellify_person_id: '42' }];
  const merged = mergeStoredContacts(stored, stale);
  assert.deepEqual(merged.map((contact) => contact.person_key), ['p-pfaffenbach-stefan', 'p-nold-soeren', 'p-luenskens-stephan']);
  assert.equal(merged[0].sellify_person_id, '42');
});

test('the incoming version of a contact wins by id or person_key', () => {
  const stored = [{ id: 'c1', person_key: 'p-a', position: 'alt' }, { id: 'old-id', person_key: 'p-b', position: 'alt' }];
  const incoming = [{ id: 'c1', person_key: 'p-a', position: 'neu' }, { id: 'new-id', person_key: 'p-b', position: 'neu' }];
  assert.deepEqual(mergeStoredContacts(stored, incoming), incoming);
});

test('an empty list still clears on purpose', () => {
  assert.deepEqual(mergeStoredContacts([{ id: 'c1' }], []), []);
  assert.deepEqual(mergeStoredContacts(undefined, [{ id: 'c1' }]), [{ id: 'c1' }]);
});
