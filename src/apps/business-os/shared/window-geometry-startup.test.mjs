import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const names = [
  'registerCoreCollections', 'primeWindowGeometryCache', 'mergeWindowGeometryCache',
  'currentWindowGeometryScope', 'windowGeometryDocumentMatchesCurrentScope',
  'isLegacyWindowGeometryDocument',
];
const definitions = names.map(name => {
  const definition = source.match(new RegExp('^(?:async )?function ' + name + '\\([^]*?^\\}', 'm'))?.[0];
  assert.ok(definition, 'exercise the actual ' + name);
  return definition;
}).join('\n');

function fixture() {
  let resolveRead;
  let rejectRead;
  const read = new Promise((resolve, reject) => { resolveRead = resolve; rejectRead = reject; });
  const scope = { workspace: 'workspace-a', actor: 'actor-a' };
  const cached = { id: 'scoped-old', owner_id: 'desktop-app:files', updated_at_ms: 100, x: 20 };
  const counts = { registrations: 0, persisted: 0, errors: 0 };
  const state = {
    windowGeometryCache: new Map(),
    db: {
      addCollections: async () => { counts.registrations++; },
      collections: { desktop_windows: { find: () => ({ exec: () => read }) } },
    },
  };
  const register = runInNewContext(definitions + '\nregisterCoreCollections', {
    state, performance: { now: () => 0 },
    loadCoreSchemaModules: async () => ({ ctox: {}, desktop: {} }),
    withMigrationStrategies: () => ({}),
    setStartupProgress() {}, shellText: key => key,
    readWindowGeometryLocalCache: () => new Map([[cached.owner_id, cached]]),
    currentWorkspaceStorageScope: () => scope.workspace,
    currentActorStorageScope: () => scope.actor,
    persistWindowGeometryLocalCache: () => { counts.persisted++; },
    withStartupTimeout: promise => promise,
    console: { log() {}, error() { counts.errors++; } },
  });
  const finish = async (rows = []) => {
    resolveRead(rows.map(payload => ({ toJSON: () => payload })));
    await new Promise(resolve => setImmediate(resolve));
  };
  return { register, state, scope, cached, counts, finish, rejectRead };
}

test('cached placement permits startup while IndexedDB refresh is pending and preserves newer moves', { timeout: 1000 }, async () => {
  const f = fixture();
  await f.register();
  assert.equal(f.counts.registrations, 1);
  assert.equal(f.state.windowGeometryCache.get(f.cached.owner_id), f.cached);
  assert.equal(f.counts.persisted, 0, 'optional read must still be pending when registration completes');
  const moved = { ...f.cached, x: 500, updated_at_ms: 200 };
  f.state.windowGeometryCache.set(moved.owner_id, moved);
  await f.finish([
    { ...f.cached, workspace_scope: 'workspace-a', actor_scope: 'actor-a' },
    { id: 'other', owner_id: 'desktop-app:notes', workspace_scope: 'workspace-a', actor_scope: 'actor-a', updated_at_ms: 150 },
    { id: 'foreign', owner_id: 'desktop-app:mail', workspace_scope: 'workspace-b', actor_scope: 'actor-a', updated_at_ms: 300 },
  ]);
  assert.equal(f.state.windowGeometryCache.get(moved.owner_id), moved, 'late stale storage must not replace a new local move');
  assert.equal(f.state.windowGeometryCache.has('desktop-app:notes'), true, 'current-scope refresh still completes');
  assert.equal(f.state.windowGeometryCache.has('desktop-app:mail'), false, 'foreign placement remains excluded');
  assert.equal(f.counts.persisted, 1);
});

for (const changed of ['db', 'workspace', 'actor']) {
  test('late geometry refresh is discarded after ' + changed + ' replacement', { timeout: 1000 }, async () => {
    const f = fixture();
    await f.register();
    if (changed === 'db') f.state.db = {};
    else f.scope[changed] = 'replacement';
    const retained = { ...f.cached, x: 800, updated_at_ms: 300 };
    f.state.windowGeometryCache.set(retained.owner_id, retained);
    await f.finish([{ ...f.cached, workspace_scope: 'workspace-a', actor_scope: 'actor-a', updated_at_ms: 900 }]);
    assert.equal(f.state.windowGeometryCache.get(retained.owner_id), retained);
    assert.equal(f.counts.persisted, 0, 'a retired refresh must not write a different scoped cache');
  });
}

test('failed optional geometry read preserves cached placement after startup', { timeout: 1000 }, async () => {
  const f = fixture();
  await f.register();
  f.rejectRead(new Error('retained storage temporarily unavailable'));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.windowGeometryCache.get(f.cached.owner_id), f.cached);
  assert.equal(f.counts.errors, 1);
  assert.equal(f.counts.persisted, 0);
});
