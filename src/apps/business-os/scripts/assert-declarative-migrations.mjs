// Guard the JSON-only module migration DSL used by app.js and module
// standalone data sources. The shell no longer imports schema.js for
// migrationStrategies, so versioned collection migrations must be executable
// from collections.schema.json alone.

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { executableDeclarativeMigrationStrategies } from '../shared/declarative-migrations.js';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, '..');
const repoRoot = resolve(appRoot, '../../..');
const modulesRoot = join(appRoot, 'modules');
const failures = [];

const documentsByModule = new Map();

for (const id of readdirSync(modulesRoot).sort()) {
  const dir = join(modulesRoot, id);
  const schemaPath = join(dir, 'collections.schema.json');
  if (!statSync(dir).isDirectory() || !existsSync(schemaPath)) continue;
  const document = JSON.parse(readFileSync(schemaPath, 'utf8'));
  documentsByModule.set(id, document);
  for (const [collection, strategies] of Object.entries(document.migration_strategies || {})) {
    try {
      executableDeclarativeMigrationStrategies(strategies);
    } catch (error) {
      failures.push(`${relative(repoRoot, schemaPath)}: ${collection} migrations failed to compile: ${error.message}`);
    }
  }
}

expectMigration('ctox', 'business_commands', '1', {
  id: 'cmd_1',
  module: 'tickets',
  status: 'pending',
}, (doc) => doc.inbound_channel === 'tickets');

expectMigration('ctox', 'business_commands', '1', {
  id: 'cmd_2',
  module: 'tickets',
  inbound_channel: 'email',
  status: 'pending',
}, (doc) => doc.inbound_channel === 'email');

expectMigration('notes', 'notes', '1', {
  id: 'note_1',
  title: 'Legacy',
  is_favorite: 1,
}, (doc) => (
  doc.notebook === ''
  && doc.tags === ''
  && doc.is_favorite === true
  && doc.is_trashed === false
  && doc.is_locked === false
  && doc.lock_passcode === ''
));

expectMigration('outbound', 'outbound_messages', '1', {
  id: 'msg_1',
  payload: {
    channel: 'letter',
    recipient_address_text: 'Ada Lovelace',
  },
}, (doc) => (
  doc.channel === 'letter'
  && doc.recipient_address_text === 'Ada Lovelace'
  && doc.document_id === ''
  && doc.document_version_id === ''
  && doc.document_pdf_url === ''
  && doc.physical_sent_at_ms === 0
));

expectMigration('matching', 'matching_results', '1', {
  id: 'match_1',
  score: 0.9,
}, (doc) => doc.id === 'match_1' && doc.score === 0.9);

if (failures.length) {
  console.error(`Business OS declarative migrations failed:\n${failures.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log(`Business OS declarative migrations OK (${documentsByModule.size} modules checked)`);

function expectMigration(moduleId, collection, version, oldDoc, predicate) {
  const document = documentsByModule.get(moduleId);
  const strategy = document?.migration_strategies?.[collection];
  const executable = executableDeclarativeMigrationStrategies(strategy);
  const migrate = executable?.[version];
  if (typeof migrate !== 'function') {
    failures.push(`${moduleId}/${collection}: missing migration ${version}`);
    return;
  }
  const migrated = migrate(oldDoc);
  if (!predicate(migrated)) {
    failures.push(`${moduleId}/${collection}: migration ${version} produced ${JSON.stringify(migrated)}`);
  }
}
