import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { collections, migrationStrategies } from '../../modules/ctox/schema.js';
import { applyDeclarativeMigration } from '../../shared/declarative-migrations.js';

const packaged = JSON.parse(readFileSync(new URL('../../modules/ctox/collections.schema.json', import.meta.url)));
const contract = JSON.parse(readFileSync(new URL('../../../../core/business_os/business_os_schema_contract.json', import.meta.url)));
assert.equal(collections.workjet_projects.version, 4);
for (const [name, schema] of Object.entries(collections).filter(([, schema]) => schema.version > 0)) {
  assert.deepEqual(packaged.collections[name], schema, `${name}: packaged/browser schema parity`);
  assert.deepEqual(contract[name], schema, `${name}: native/browser schema parity`);
  for (let version = 1; version <= schema.version; version += 1) {
    assert.equal(typeof migrationStrategies[name]?.[version], 'function', `${name}: browser step ${version}`);
    assert.ok(Array.isArray(packaged.migration_strategies[name]?.[String(version)]?.operations), `${name}: native step ${version}`);
  }
}
const legacy = {
  id: 'existing-project', name: 'Existing project', status: 'active',
  owner_user_id: '196a89ba-ee86-4413-885c-04ca60e6f291',
  description: 'Existing description', created_at_ms: 50, updated_at_ms: 100,
  _rev: '11-retained', _deleted: false, _meta: { lwt: 100 },
};
const configured = {
  ...legacy, repo_url: 'https://github.com/metric-space-ai/ctox', public_url: 'https://ctox.dev',
  info: { summary: 'Owner summary', goal: 'Owner goal', phase: 'build' },
  jour_fixe: { weekday: 1, time: '13:00', timezone: 'Europe/Berlin' },
};
const selected = { ...configured, supervisor_luma_id: 'project-supervisor-luma' };
const optedIn = { ...selected, execution_policy: { schema: 'ctox.workjet.project_execution_policy.v1', mode: 'autonomous_worktree', revision: 3 } };
const revoked = { ...selected, execution_policy: { schema: 'ctox.workjet.project_execution_policy.v1', mode: 'default', revision: 4 } };
assert.equal(collections.workjet_projects.required.includes('execution_policy'), false);
for (const original of [legacy, configured, selected, optedIn, revoked, { ...selected, status: 'archived', archived_at_ms: 200, is_deleted: true, _deleted: true }]) {
  const before = structuredClone(original);
  let js = original, native = original;
  for (let step = 1; step <= collections.workjet_projects.version; step += 1) {
    js = migrationStrategies.workjet_projects[step](js);
    native = applyDeclarativeMigration(native, packaged.migration_strategies.workjet_projects[String(step)]);
  }
  assert.deepEqual(js, before);
  assert.deepEqual(native, before);
  assert.deepEqual(original, before, 'identity upgrade preserves ownership/configuration/revisions/tombstones');
  assert.deepEqual(applyDeclarativeMigration(native, packaged.migration_strategies.workjet_projects['1']), native);
}
console.log('All versioned cockpit schemas have complete matching migration chains; project identity/configuration/Luma selection/execution policy survives v0->v4');
