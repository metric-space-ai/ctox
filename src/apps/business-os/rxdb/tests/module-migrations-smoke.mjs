import assert from 'node:assert/strict';
import { readdirSync, existsSync } from 'node:fs';

const modules = new URL('../../modules/', import.meta.url);
for (const entry of readdirSync(modules, { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const url = new URL(`${entry.name}/schema.js`, modules);
  if (!existsSync(url)) continue;
  const { collections = {}, migrationStrategies = {} } = await import(url.href);
  for (const [name, definition] of Object.entries(collections)) {
    const schema = definition.schema || definition;
    const strategies = migrationStrategies[name] || definition.migrationStrategies || {};
    assert.equal(Object.keys(strategies).length, schema.version, `${entry.name}/${name}: complete migration map`);
    for (let version = 1; version <= schema.version; version++) {
      assert.equal(typeof strategies[version], 'function', `${entry.name}/${name}: migration ${version}`);
    }
    if (name === 'workjet_projects') {
      const oldDoc = { id: 'existing-v0-project', name: 'Project', owner_user_id: 'owner-1', archived: true, updated_at_ms: 7 };
      assert.deepEqual(strategies[1](oldDoc), oldDoc, `${entry.name}: project upgrade preserves owner and archived state`);
      const configured = { ...oldDoc, repo_url: 'https://example.test/repo', info: { goal: 'Keep saved configuration' }, jour_fixe: { weekday: 1, time: '09:00', timezone: 'Europe/Berlin' } };
      assert.deepEqual(strategies[1](configured), configured, `${entry.name}: project upgrade preserves native configuration`);
    }
    if (name === 'ctox_queue_tasks') {
      const oldDoc = { id: 'existing-v2-task', crew_member_id: null };
      assert.deepEqual(strategies[3](oldDoc), oldDoc, `${entry.name}: additive v2 upgrade`);
    }
  }
}
console.log('All module collections have contiguous migration strategies');
