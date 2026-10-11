import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { canSeeModuleForAppVersion } from '../../../apps/business-os/shared/app-lifecycle.js';
import { BusinessOsPermissions as P } from '../../../apps/business-os/shared/permissions.js';

const source = readFileSync(new URL('./browser_rust_smoke.js', import.meta.url), 'utf8');
const seed = source.match(/const seedFreshProfileModuleCatalog = (async \(\) => \{[\s\S]*?\n          \});/)?.[1];
assert.ok(seed, 'the real fresh-profile catalog seeder must exist');

test('a delayed catalog projection preserves the fixture builder permission', async () => {
  const privateModule = { id: 'private-app', source: 'installed', install_scope: 'installed', version: '0.5.0' };
  const teamModule = { id: 'team-app', source: 'installed', lifecycle: { audience: 'team' } };
  const restrictedModule = { id: 'restricted-app', source: 'installed', lifecycle: { audience: 'restricted' } };
  const permissions = [P.AppsView, P.AppsModify, P.AppsRelease];
  const governance = {
    founders: { [privateModule.id]: [{ user_id: 'builder', active: true }] },
    permission_model: {
      role_defaults: { founder: { workspace: [], module: [], assigned_module: permissions } },
      module_assignments: { [privateModule.id]: { builder: permissions } }, explicit_grants: [],
    },
  };
  const previousGovernance = { founders: {}, permission_model: { role_defaults: {}, module_assignments: {}, explicit_grants: [] } };
  const session = { user: { id: 'builder', role: 'founder' } };
  let published;
  const collection = {
    findOne: () => ({ exec: async () => ({ toJSON: () => ({ id: 'module-catalog', ok: true, modules: [], governance: previousGovernance }) }) }),
    async upsert(value) { published = structuredClone(value); },
  };
  const state = { db: { collection: () => collection }, session, governance };
  const install = vm.runInNewContext('(' + seed + ')', {
    state, governance, privateModule, teamModule, restrictedModule, scaleModules: [],
    moduleIds: [privateModule.id, teamModule.id, restrictedModule.id], scaleModuleIds: [], Date,
  });
  await install();
  assert.equal(canSeeModuleForAppVersion(privateModule, { session, governance: state.governance }), true);
  // Deliver the notification after the smoke installs its actor, as on CI.
  // refreshModules assigns the catalog governance together with its modules.
  state.modules = published.modules;
  state.governance = published.governance;
  assert.equal(canSeeModuleForAppVersion(privateModule, { session, governance: state.governance }), true,
    'the replicated catalog must carry the grants for the fixture modules');
});
