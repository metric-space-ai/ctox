import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { packagedSchema } from './packaged-schema.mjs';
const { collections } = await packagedSchema('ctox');

const fixture = JSON.parse(readFileSync(new URL('../../../../core/rxdb/tests/fixtures/crew-identity.json', import.meta.url)));
const contract = JSON.parse(readFileSync(new URL('../../../../core/business_os/business_os_schema_contract.json', import.meta.url)));
assert.ok(Object.hasOwn(collections.ctox_queue_tasks.properties, fixture.lifecycle.assignment_field));
assert.equal(fixture.lifecycle.unavailable_event, 'crew_selection_unavailable');
assert.equal(fixture.lifecycle.resting_window_ms, 86400000);
for (const field of fixture.public_fields) {
  assert.ok(Object.hasOwn(collections.ctox_crew_members.properties, field), field);
  assert.ok(!['soul', 'specialties', 'stats'].includes(field), field);
}
for (const [collection, doc] of [['ctox_crew_members', fixture.member], ['ctox_crew_learnings', fixture.learning]]) {
  assert.deepEqual(contract[collection], collections[collection]);
  for (const field of collections[collection].required) assert.notEqual(doc[field], undefined);
  for (const index of collections[collection].indexes.flat()) assert.notEqual(doc[index], null);
}
// The native command test crew_controls_enforce_roles_even_with_explicit_grants_and_replay_receipts
// exercises the actual authority, including adversarial grants and replay.
// Schema compatibility does not need another browser role-policy implementation.
console.log('Crew identity declarative schema contract: passed');
