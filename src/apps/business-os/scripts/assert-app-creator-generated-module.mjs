#!/usr/bin/env node

import fs from 'node:fs/promises';

const creatorUrl = new URL('../modules/creator/index.js', import.meta.url);
const source = await fs.readFile(creatorUrl, 'utf8');

const required = [
  ['collections schema output', "state.generatedFiles['collections.schema.json']"],
  ['runtime schema format', "ctox-business-os-module-collections-v1"],
  ['schema.js compatibility facade', "state.generatedFiles['schema.js']"],
  ['schema.js JSON facade entries', 'Object.entries(collectionSchemas)'],
  ['deployment saves schema JSON', "'collections.schema.json'"],
  ['manifest includes command channel', "moduleCollections = Array.from(new Set(['business_commands', ...versionedCollections]))"],
  ['generated app has collection resolver', 'function getCollection(name)'],
  ['generated app reads RxDB collection', 'collection.find().exec()'],
  ['generated app creates RxDB records', 'collection.insert({'],
  ['generated app loads selected RxDB doc', 'collection.findOne(state.selectedId).exec()'],
  ['generated app patches RxDB records', 'doc.patch({'],
  ['generated app starts command sync', "state.ctx.sync?.startCollection?.('business_commands')"],
  ['generated app dispatches command bus task', 'state.ctx.commandBus.dispatch({'],
  ['generated app uses chat task command type', "type: 'business_os.chat.task'"],
  ['generated app preserves command_type', "command_type: 'business_os.chat.task'"],
  ['generated app includes source record snapshot', 'record_snapshot: record'],
];

const forbidden = [
  ['generated app direct ctx.db collection access', /ctx\.db\[[^\]]+\]/],
  ['generated app direct PRIMARY_COLL access', /state\.ctx\.db(?:\?\.)?\[PRIMARY_COLL\]/],
  ['generated app raw DB unwrap', /\bdb\.raw\b/],
];

const failures = [];

for (const [label, needle] of required) {
  if (!source.includes(needle)) {
    failures.push(`missing ${label}: ${needle}`);
  }
}

for (const [label, pattern] of forbidden) {
  if (pattern.test(source)) {
    failures.push(`forbidden ${label}: ${pattern}`);
  }
}

if (failures.length > 0) {
  console.error('[assert-app-creator-generated-module] Creator generated module guard failed:');
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log('[assert-app-creator-generated-module] OK: generated modules use JSON schemas, RxDB CRUD, and business_commands.');
