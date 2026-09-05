import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { collections } from '../../modules/ctox/schema.js';
import { canUseBusinessPermission, BusinessOsPermissions } from '../../shared/permissions.js';
const fixture = JSON.parse(readFileSync(new URL('../../../../core/rxdb/tests/fixtures/crew-identity.json', import.meta.url)));
const contract = JSON.parse(readFileSync(new URL('../../../../core/business_os/business_os_schema_contract.json', import.meta.url)));
for (const [collection, doc] of [['ctox_crew_members', fixture.member], ['ctox_crew_learnings', fixture.learning]]) {
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
