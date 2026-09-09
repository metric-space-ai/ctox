import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const helper = source.match(/function collectForeignSchemaModules\(mod\) \{[\s\S]*?\n\}/)?.[0];
function dependencies(collections, registered) {
  assert.ok(helper, 'production dependency resolver must exist');
  const state = {
    db: { raw: Object.fromEntries(registered.map(name => [name, {}])) },
    modules: [
      { id: 'mail', collections: ['business_commands', 'mail_messages'] },
      { id: 'support', collections: ['business_commands', 'support_tickets'] },
      { id: 'sellify', collections: ['business_commands', 'sellify_companies'] },
    ],
  };
  const context = vm.createContext({ state, mod: { id: 'ctox', collections } });
  return Array.from(vm.runInContext(`${helper}\ncollectForeignSchemaModules(mod).map(m => m.id)`, context));
}

test('registered shared collections do not make Crew depend on unrelated installed apps', () => {
  assert.deepEqual(dependencies(['business_commands', 'ctox_queue_tasks'], ['business_commands', 'ctox_queue_tasks']), []);
});

test('missing foreign collections still register their owner', () => {
  assert.deepEqual(dependencies(['business_commands', 'sellify_companies'], ['business_commands']), ['sellify']);
  assert.deepEqual(dependencies(['business_commands', 'sellify_companies'], ['business_commands', 'sellify_companies']), []);
});
