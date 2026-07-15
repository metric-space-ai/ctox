import assert from 'node:assert/strict';
import {
  buildQuickAppCreateCommand,
  isRuntimeInstalledApp,
  moduleRenamePayload,
  nextQuickAppIdentity,
} from '../appCommands.js';

const identity = nextQuickAppIdentity([
  { id: 'first', title: 'Neue App' },
  { id: 'second', title: 'Neue App 2' },
], 'de', 1234);
assert.deepEqual(identity, { id: 'app-ya', title: 'Neue App 3' });

const create = buildQuickAppCreateCommand({
  moduleId: identity.id,
  title: identity.title,
  actor: { id: 'admin', role: 'admin' },
  now: 1234,
});
assert.equal(create.command_type, 'ctox.business_os.app.create');
assert.equal(create.payload.archetype, 'record-workbench');
assert.equal(create.payload.install_target, 'runtime-installed-module');
assert.match(create.payload.instruction, /left navigation/i);
assert.deepEqual(create.payload.presentation.initial_size, { width: 960, height: 680 });

const app = {
  id: identity.id,
  title: identity.title,
  description: 'Starter',
  version: '0.1.0',
  entry: `installed-modules/${identity.id}/index.html`,
  collections: [`${identity.id.replaceAll('-', '_')}_records`],
  layout: { shell: 'windowed', default_width: 960 },
  source: 'installed',
};
assert.equal(isRuntimeInstalledApp(app), true);
assert.equal(isRuntimeInstalledApp({ id: 'documents', entry: 'modules/documents/index.html', source: 'core' }), false);

const rename = moduleRenamePayload(app, 'Kundenakte');
assert.equal(rename.id, identity.id);
assert.equal(rename.title, 'Kundenakte');
assert.equal(rename.entry, app.entry);
assert.deepEqual(rename.collections, app.collections);
assert.deepEqual(rename.layout, app.layout);
assert.notEqual(rename.layout, app.layout);

console.log('desktop app command tests ok');
