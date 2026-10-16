import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { normalizeCollectionsSchema } from '../../v4/tools/manifest/gen-manifest.mjs';
import migrateLearning from '../../v4/schemas/migrations/ctox/ctox_crew_learnings.js';
// E45: compare the generated native contract with the canonical source,
// preserving the platform collection used by the Crew lifecycle guard.
const source = (app) => normalizeCollectionsSchema(JSON.parse(readFileSync(
  new URL(`../../v4/schemas/${app}.collections.schema.json`, import.meta.url),
))).collections;
const collections = { ...source('platform'), ...source('ctox') };
import { canUseBusinessPermission, BusinessOsPermissions } from '../../shared/permissions.js';
const fixture = JSON.parse(readFileSync(new URL('../../../../core/rxdb/tests/fixtures/crew-identity.json', import.meta.url)));
const contract = JSON.parse(readFileSync(new URL('../../../../core/business_os/business_os_schema_contract.json', import.meta.url)));
assert.ok(Object.hasOwn(collections.ctox_queue_tasks.properties, fixture.lifecycle.assignment_field));
assert.equal(fixture.lifecycle.unavailable_event, 'crew_selection_unavailable');
assert.equal(fixture.lifecycle.resting_window_ms, 86400000);
for (const field of fixture.public_fields) {
  assert.ok(Object.hasOwn(collections.ctox_crew_members.properties, field), field);
  assert.ok(!['soul', 'specialties', 'stats'].includes(field), field);
}
for (const [collection, original] of [['ctox_crew_members', fixture.member], ['ctox_crew_learnings', fixture.learning]]) {
  // Historical fixture stays intact; validate its real canonical migration.
  const doc = collection === 'ctox_crew_learnings' ? migrateLearning(structuredClone(original)) : original;
  assert.deepEqual(contract[collection], collections[collection]);
  for (const field of collections[collection].required) assert.notEqual(doc[field], undefined);
  for (const index of collections[collection].indexes.flat()) assert.notEqual(doc[index], null);
}
for (const role of ['admin', 'founder', 'user']) {
  for (const command of fixture.commands) {
    const allowed = canUseBusinessPermission({session: {user: {id: 'fixture', role}}, permission: BusinessOsPermissions.CrewManage, scopeType: 'record', scopeId: command});
    assert.equal(allowed, role === 'admin' || (role === 'founder' && ['ctox.crew.learning.confirm', 'ctox.crew.learning.update'].includes(command)), `${role}: ${command}`);
  }
}
console.log('Crew identity schemas and role contract: passed');
