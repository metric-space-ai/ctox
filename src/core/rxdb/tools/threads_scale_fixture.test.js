'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { DatabaseSync } = require('node:sqlite');
const test = require('node:test');
const vm = require('node:vm');

test('threads scale fixture seeds native-valid envelopes before peer startup', async () => {
  const source = fs.readFileSync(path.join(__dirname, 'browser_rust_smoke.js'), 'utf8');
  const definition = source.match(/async function seedBusinessOsThreadsScaleNativeSetup\(\) \{[\s\S]*?\n\}/)?.[0];
  assert.ok(definition, 'exercise the actual production-smoke seeder');
  const database = new DatabaseSync(':memory:');
  const tables = [
    'ctox_business_os__business_commands__v2',
    'ctox_business_os__user_threads__v1',
    'ctox_business_os__user_thread_messages__v1',
    'ctox_business_os__user_notifications__v0',
  ];
  const schemas = JSON.parse(fs.readFileSync(path.join(__dirname, '../../business_os/business_os_schema_contract.json'), 'utf8'));
  const collectionNames = ['business_commands', 'user_threads', 'user_thread_messages', 'user_notifications'];
  const canonicalTables = collectionNames.map(name => 'ctox_business_os__' + name + '__v' + schemas[name].version);
  const context = vm.createContext({
    Date,
    sqlite: sql => database.exec(sql),
    quoteSqlIdentifier: value => '"' + String(value).replaceAll('"', '""') + '"',
    sqlString: value => String(value).replaceAll("'", "''"),
    waitForSqliteTables: async names => {
      for (const name of names) assert.ok(database.prepare('SELECT name FROM sqlite_master WHERE name = ?').get(name));
    },
    sqliteRowCount: (table, where) => database.prepare(`SELECT COUNT(*) AS count FROM "${table}" WHERE ${where}`).get().count,
  });
  try {
    const seed = await vm.runInContext(`${definition}\nseedBusinessOsThreadsScaleNativeSetup()`, context);
    assert.deepEqual({ ...seed }, { commands: 10000, threads: 10000, messages: 10000, notifications: 10000 });
    assert.deepEqual(tables, canonicalTables, "seed current native schemas without a startup migration");
    const seededTables = database.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all().map(row => row.name);
    assert.deepEqual(seededTables, [...canonicalTables].sort());
    for (const table of canonicalTables) {
      for (const row of database.prepare(`SELECT revision, data, deleted, lastWriteTime FROM "${table}"`).iterate()) {
        const document = JSON.parse(row.data);
        // Native valid_rxdb_revision requires a numeric height and a nonempty
        // hash without another dash. Invalid command revisions trigger a
        // full startup rewrite before the peer can answer native queries.
        assert.match(row.revision, /^\d+-[^-]+$/, `${table}: no legacy revision repair`);
        assert.equal(document._rev, row.revision);
        assert.equal(document._meta.lwt, row.lastWriteTime);
        assert.equal(row.deleted, 0);
        assert.equal(document._deleted, false);
        assert.equal(document.is_deleted, false);
      }
    }
    const commands = tables[0];
    assert.equal(database.prepare(`SELECT COUNT(*) AS count FROM "${commands}" WHERE json_extract(data, '$.status') != 'completed'`).get().count, 0);
  } finally {
    database.close();
  }
});
