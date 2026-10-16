// v4 counterpart of scripts/assert-command-consumer-inventory.mjs (CI line selection E37, see
// scripts/CI-V4-LINE.md). The V2 script and scripts/command-consumer-inventory.json stay byte-identical and run on
// release/v2; this file runs on every other line.
//
// V2 invariants and their v4 form (same invariant, v4 dispatch path):
//
//   C1 dispatch_consumers (exact inventory of every runtime file that dispatches business commands).
//      v4: two exact inventories, both frozen below –
//        a) V2 bus path `commandBus.dispatch` / `requireCommandBus().dispatch` (same regex as V2): the files that
//           still dispatch through the V2 bus (V2 shell app.js, shared/*, office bridge) plus v4/shell/workjet/*
//           (v2-compat, debt L-42 in v4/debt/LOESCHPLAN.md);
//        b) v4 path `api.command(` (App-API 4.0 §10.2): every file that dispatches through the app API. Every such
//           file inside an app folder (modules/<id>/, customer-modules/<id>/) must belong to an app with a v4
//           manifest v4/manifests/<id>.manifest.json.
//   C2 projection_readers (exact inventory of every runtime file that reads the business_commands projection).
//      v4: one exact inventory = V2 regex (collection('business_commands'), .business_commands, …) ∪ v4 regex
//      api.query|get|subscribe|watch|find|findOne('business_commands'). Every reader inside an app folder must
//      declare business_commands in its v4 manifest (collections or foreign_collections with op 'read').
//   C3 legacy_projection_waiters = [] – `function waitForCommandProjection(` anywhere: unchanged.
//   C4 direct_intent_writers = [] – nobody creates a command document directly, only through the dispatch path.
//      v4: V2 regex (…business_commands.insert|upsert|incrementalUpsert(, upsertDoc(…business_commands) ∪
//      api.put|insert|upsert('business_commands'. As in V2, modifying an existing command document is not an
//      intent write (V2 does not match patch either); see CI-V4-LINE.md for modules/credentials' api.patch.
//
// Scan scope = V2 scope (all .js/.mjs under src/apps/business-os with the V2 exclusions) with these additions,
// each a test/tooling location analogous to V2's *.test.mjs exclusion:
//   node_modules/ (dependencies, never runtime source), */tests/, */fixtures/, kernel/testkit/ (test code and
//   guard fixtures, e.g. v4/tools/guards/fixtures/tree/** and modules/mail/tests/mail-browser-qa.v4.mjs),
//   v4/tools/ (Node tooling that names api.command as a scan pattern, e.g. v4/tools/port/scan.mjs).
// NO_V4: none.
//
// Changing an inventory below is a reviewed act, exactly like editing command-consumer-inventory.json in V2.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(scriptDir, '..');
const manifestsRoot = path.join(appRoot, 'v4/manifests');

const INVENTORY = Object.freeze({
  schema: 'ctox.business_os.command_consumer_inventory.v4',
  generated_at: '2026-10-16',
  dispatch_consumers_v2_bus: [
    'app.js',
    'office-engine/src/business-os-bridge.mjs',
    'shared/app-actions.js',
    'shared/business-chat.js',
    'shared/react-settings.js',
    'shared/universal-importer.js',
    'v4/shell/workjet/actions/computer.js',
    'v4/shell/workjet/actions/instance.js',
    'v4/shell/workjet/actions/meeting.js',
    'v4/shell/workjet/actions/project.js',
    'v4/shell/workjet/actions/session.js',
    'v4/shell/workjet/actions/supervisor.js',
    'v4/shell/workjet/v2-compat.js',
  ],
  dispatch_consumers_v4_api: [
    'customer-modules/outbound-lead-generation/index.js',
    'customer-modules/outbound-lead-generation/v4-port.mjs',
    'modules/app-store/index.js',
    'modules/appsec-pentest/index.js',
    'modules/browser/index.js',
    'modules/buchhaltung/index.js',
    'modules/consent/index.js',
    'modules/conversations/index.js',
    'modules/creator/index.js',
    'modules/credentials/index.js',
    'modules/ctox/index.js',
    'modules/customers/index.js',
    'modules/cv-print-builder/index.js',
    'modules/desktop/index.js',
    'modules/documents/index.js',
    'modules/documents/office-bridge-v4.js',
    'modules/esign/index.js',
    'modules/file-viewer/index.js',
    'modules/importer/index.js',
    'modules/intake/index.js',
    'modules/invoices/index.js',
    'modules/iot/index.js',
    'modules/knowledge/index.js',
    'modules/kundenpipeline/index.js',
    'modules/mail/index.js',
    'modules/matching/ui/ctoxCommandAdapter.js',
    'modules/matching/ui/v4Runtime.js',
    'modules/nachweise/index.js',
    'modules/news/index.js',
    'modules/outbound/v4-bridge.js',
    'modules/placements/index.js',
    'modules/reports/index.js',
    'modules/research/index.js',
    'modules/spreadsheets/index.js',
    'modules/spreadsheets/office-bridge-v4.js',
    'modules/submissions/index.js',
    'modules/support/index.js',
    'modules/threads/index.js',
    'modules/tickets/index.js',
    'shared/context-chat.js',
    'v4/shell/workjet/actions/meeting.js',
    'v4/shell/workjet/v2-compat.js',
    'v4/source/coding-port.js',
  ],
  projection_readers: [
    'app.js',
    'customer-modules/outbound-lead-generation/v4-port.mjs',
    'modules/appsec-pentest/index.js',
    'modules/creator/index.js',
    'modules/credentials/index.js',
    'modules/documents/index.js',
    'modules/importer/index.js',
    'modules/mail/index.js',
    'modules/reports/index.js',
    'modules/research/index.js',
    'modules/support/index.js',
    'shared/business-chat.js',
    'shared/command-bus.js',
    'shared/react-settings.js',
    'v4/shell/workjet/actions/computer.js',
    'v4/shell/workjet/actions/meeting.js',
    'v4/shell/workjet/actions/project.js',
    'v4/shell/workjet/actions/session.js',
    'v4/shell/workjet/actions/supervisor.js',
    'v4/shell/workjet/v2-compat.js',
  ],
  legacy_projection_waiters: [],
  direct_intent_writers: [],
});

const runtimeFiles = walk(appRoot)
  .filter((file) => /\.(?:js|mjs)$/.test(file))
  .filter((file) => !excluded(relative(file)));

const v2Dispatch = /commandBus(?:\?\.|\.)dispatch\b|requireCommandBus\([^)]*\)\.dispatch\b/;
const v4Dispatch = /\bapi\.command\s*\(/;
const v2ProjectionRead = /(?:collection\?*\.\(['"]business_commands['"]\)|\.business_commands\b|activeCollection\(['"]business_commands['"]\)|documentCollection\([^\n]+['"]business_commands['"]\))/;
const v4ProjectionRead = /\bapi\.(?:query|get|subscribe|watch|find|findOne)\(\s*['"]business_commands['"]/;

const dispatchV2 = matchingFiles((source) => v2Dispatch.test(source));
const dispatchV4 = matchingFiles((source) => v4Dispatch.test(source));
const projectionReaders = matchingFiles((source) => v2ProjectionRead.test(source) || v4ProjectionRead.test(source));
const legacyProjectionWaiters = matchingFiles((source) => /function\s+waitForCommandProjection\s*\(/.test(source));
const directIntentWriters = matchingFiles((source) => (
  /(?:businessCommandsCollection|commandsCollection|business_commands)(?:\?\.|\.)\s*(?:insert|upsert|incrementalUpsert)(?:\?\.)?\s*\(/.test(source)
  || /upsertDoc\s*\(\s*[^,\n]{0,240}business_commands/.test(source)
  || /\bapi\.(?:put|insert|upsert)\(\s*['"]business_commands['"]/.test(source)
));

const failures = [];
assertExact('dispatch_consumers_v2_bus', dispatchV2);
assertExact('dispatch_consumers_v4_api', dispatchV4);
assertExact('projection_readers', projectionReaders);
assertExact('legacy_projection_waiters', legacyProjectionWaiters);
assertExact('direct_intent_writers', directIntentWriters);
assertAppFilesDeclared('dispatch_consumers_v4_api', dispatchV4, (manifest) => Boolean(manifest));
assertAppFilesDeclared('projection_readers', projectionReaders, (manifest) => (
  Boolean(manifest)
  && (Object.hasOwn(manifest.collections || {}, 'business_commands')
    || (manifest.foreign_collections?.business_commands?.ops || []).includes('read'))
));

if (failures.length) {
  console.error(`Business OS command consumer inventory (v4 line) failed:\n${failures.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('Business OS command consumer inventory OK (v4 line)', {
  dispatchConsumersV2Bus: dispatchV2.length,
  dispatchConsumersV4Api: dispatchV4.length,
  projectionReaders: projectionReaders.length,
  legacyProjectionWaiters: legacyProjectionWaiters.length,
  directIntentWriters: directIntentWriters.length,
});

function matchingFiles(predicate) {
  return runtimeFiles
    .filter((file) => predicate(fs.readFileSync(file, 'utf8')))
    .map(relative)
    .sort();
}

function assertExact(field, actual) {
  const expected = [...(INVENTORY[field] || [])].sort();
  if (JSON.stringify(actual) === JSON.stringify(expected)) return;
  const missing = expected.filter((file) => !actual.includes(file));
  const unexpected = actual.filter((file) => !expected.includes(file));
  failures.push(`${field} drifted; missing: ${missing.join(', ') || '-'}; unexpected: ${unexpected.join(', ') || '-'}`);
}

function assertAppFilesDeclared(field, files, isDeclared) {
  for (const file of files) {
    const appId = file.match(/^(?:modules|customer-modules)\/([^/]+)\//)?.[1];
    if (!appId) continue;
    const manifestPath = path.join(manifestsRoot, `${appId}.manifest.json`);
    let manifest = null;
    try {
      manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    } catch {
      manifest = null;
    }
    if (!isDeclared(manifest)) {
      failures.push(`${field}: ${file} – app ${appId} ${manifest ? 'does not declare business_commands (collections or foreign_collections read)' : 'has no v4 manifest v4/manifests/' + appId + '.manifest.json'}`);
    }
  }
}

function relative(file) {
  return path.relative(appRoot, file).split(path.sep).join('/');
}

function excluded(file) {
  // V2 exclusions, unchanged:
  return file.startsWith('installed-modules/')
    || file.startsWith('rxdb/dist/')
    || file.startsWith('rxdb/src/')
    || file.startsWith('rxdb/tests/')
    || file.startsWith('scripts/')
    || file.endsWith('.test.js')
    || file.endsWith('.test.mjs')
    || file.endsWith('/test.mjs')
    // v4-allow: debt L-14 (prüft V2-schema.js bis S9)
    || file.endsWith('/schema.js')
    // v4 additions (test/tooling locations, see header):
    || /(^|\/)(node_modules|tests|fixtures|testkit)\//.test(file)
    || file.startsWith('v4/tools/');
}

function walk(directory) {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === 'node_modules') continue;
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walk(target));
    else files.push(target);
  }
  return files;
}
