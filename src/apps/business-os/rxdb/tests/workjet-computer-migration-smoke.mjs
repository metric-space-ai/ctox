import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { collections, migrationStrategies } from '../../modules/ctox/schema.js';
import { applyDeclarativeMigration } from '../../shared/declarative-migrations.js';

const packaged = JSON.parse(readFileSync(new URL('../../modules/ctox/collections.schema.json', import.meta.url)));
const contract = JSON.parse(readFileSync(new URL('../../../../core/business_os/business_os_schema_contract.json', import.meta.url)));
const schema = collections.workjet_computers;
assert.equal(schema.version, 1, 'a changed v0 hash requires a new collection version');
assert.deepEqual(packaged.collections.workjet_computers, schema);
assert.deepEqual(contract.workjet_computers, schema);

const legacy = {
  id: 'legacy-computer', display_name: 'Existing workstation',
  hosting_mode: 'workstation', status: 'assigned', capabilities: ['coding'],
  self_hosted_colocation: false, owner_user_id: 'owner',
  created_at_ms: 100, updated_at_ms: 100,
  _rev: '3-original', _deleted: false, _meta: { lwt: 100 },
};
const defaults = {
  device_binding_id: '', actor_epoch: 0, last_seen_at_ms: 0,
  replication_up: false, is_deleted: false,
};
const bound = {
  ...legacy, id: 'bound-computer', status: 'unassigned', unassigned_at_ms: 200,
  device_binding_id: 'existing-binding', actor_epoch: 7, last_seen_at_ms: 190,
  replication_up: true, is_deleted: true, _deleted: true,
};
for (const [original, expected] of [
  [legacy, { ...legacy, ...defaults }],
  [{ ...legacy, ...defaults }, { ...legacy, ...defaults }],
  [bound, bound],
]) {
  const before = structuredClone(original);
  const js = migrationStrategies.workjet_computers[1](original);
  const declarative = applyDeclarativeMigration(original, packaged.migration_strategies.workjet_computers['1']);
  assert.deepEqual(js, expected);
  assert.deepEqual(declarative, expected, 'packaged and browser transformations agree');
  assert.deepEqual(original, before, 'migration does not mutate source or revision metadata');
  assert.deepEqual(migrationStrategies.workjet_computers[1](js), js, 'retry is idempotent');
}
console.log('Workjet computer migration preserves legacy data, bindings, tombstones and revisions');
