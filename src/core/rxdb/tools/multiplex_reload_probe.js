// Isolated Browser -> WebRTC -> native SQLite acceptance fixture.
// Never run these fixture SQL writes against an operator/tenant database.
const fs = require('node:fs');
const path = require('node:path');

function definitions() {
  const schema = {
    version: 0, primaryKey: 'id', type: 'object',
    properties: {
      id: { type: 'string', maxLength: 180 },
      content: { type: 'string' },
      revision_number: { type: 'number' },
      updated_at_ms: { type: 'number' },
    },
    required: ['id', 'content', 'revision_number', 'updated_at_ms'],
    additionalProperties: true,
  };
  return Object.fromEntries(Array.from({ length: 21 }, (_, i) => [
    `sync_reload_probe_${String(i).padStart(2, '0')}`, { schema },
  ]));
}

function seed(sourceRoot, sqlite) {
  const schemas = definitions();
  const manifestPath = path.join(sourceRoot, 'module.json');
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  manifest.collections = Object.keys(schemas);
  fs.writeFileSync(manifestPath, JSON.stringify(manifest));
  fs.writeFileSync(path.join(sourceRoot, 'index.js'), 'export function mount() { return { destroy() {} }; }\n');
  fs.writeFileSync(path.join(sourceRoot, 'icon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" fill="#2458ed"/></svg>');
  fs.writeFileSync(path.join(sourceRoot, 'collections.schema.json'), JSON.stringify({
    schema_format: 'ctox-business-os-module-collections-v1', collections: schemas,
  }));
  const statements = ['BEGIN;'];
  const now = Date.now() - 60000;
  for (const [index, name] of Object.keys(schemas).entries()) {
    const table = `ctox_business_os__${name}__v0`;
    statements.push(`CREATE TABLE "${table}" (
      id TEXT PRIMARY KEY, revision TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
      lastWriteTime REAL NOT NULL, data TEXT NOT NULL);
      CREATE INDEX "${table}_lwt" ON "${table}"(lastWriteTime,id);`);
    const count = index === 0 ? 3911 : index === 1 ? 54 : 24;
    for (let n = 0; n < count; n++) {
      const id = `record_${String(n).padStart(4, '0')}`;
      const deleted = index === 1 && n >= 36;
      const lwt = now + n;
      const doc = {
        id, content: (index === 1 ? 'Lead Import ' : 'Thread Zustand ').repeat(index === 1 ? 2900 : 40),
        revision_number: 1, updated_at_ms: lwt,
        _rev: '1-fixture', _deleted: deleted, _meta: { lwt }, _attachments: {},
      };
      statements.push(`INSERT INTO "${table}" VALUES ('${id}','1-fixture',${Number(deleted)},${lwt},'${JSON.stringify(doc).replaceAll("'", "''")}');`);
    }
  }
  statements.push('COMMIT;');
  sqlite(statements.join('\n'));
}

function advanceOfflineFixture(sqlite) {
  const table = 'ctox_business_os__sync_reload_probe_01__v0';
  const now = Date.now();
  // The browser is about:blank here. Only the explicitly isolated fixture is
  // modified. Existing IndexedDB is retained to exercise stale rows and deletes.
  sqlite(`BEGIN;
    UPDATE "${table}" SET revision='23-research', lastWriteTime=${now},
      data=json_set(data,'$._rev','23-research','$._meta.lwt',${now},
        '$.updated_at_ms',${now},'$.revision_number',23,
        '$.content','13 recherchierte Felder')
      WHERE id='record_0000';
    UPDATE "${table}" SET revision='2-deleted',deleted=1,lastWriteTime=${now + 1},
      data=json_set(data,'$._rev','2-deleted','$._deleted',json('true'),
        '$._meta.lwt',${now + 1},'$.updated_at_ms',${now + 1})
      WHERE id>='record_0019' AND id<'record_0036';
    COMMIT;`);
}

async function run(page, sqlite) {
  const url = page.url();
  const schemas = definitions();
  const names = Object.keys(schemas);
  const samples = [];
  for (let round = 0; round < 3; round++) {
    await page.goto('about:blank');
    if (round === 1) advanceOfflineFixture(sqlite);
    const started = Date.now();
    await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 60000 });
    await page.waitForFunction(() => Boolean(
      globalThis.ctoxBusinessOsSmoke?.state?.db?.raw
      && globalThis.ctoxBusinessOsSmoke?.state?.sync?.startCollection
      && globalThis.ctoxBusinessOsSmoke?.state?.commandBus?.dispatch
    ), null, { timeout: Math.max(1, 60000 - (Date.now() - started)) });
    const work = page.evaluate(async ({ schemas, round }) => {
      const state = globalThis.ctoxBusinessOsSmoke.state;
      const names = Object.keys(schemas);
      const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
      await state.db.addCollections(schemas);
      await Promise.all(names.map(name => state.sync.startCollection(name)));
      const auth = await fetch('/api/business-os/auth/capability', {
        method: 'POST', credentials: 'same-origin', cache: 'no-store',
      });
      if (!auth.ok) throw new Error(`fixture capability failed: ${auth.status}`);
      const { capability_token } = await auth.json();
      if (!capability_token) throw new Error('fixture capability missing');
      const commandTimings = [];
      // Cold pulls and demand queries compete with real policy-gated commands.
      const commands = (async () => {
        for (let i = 0; i < 10; i++) {
          const start = performance.now();
          const result = await state.commandBus.dispatch({
            id: `reload_command_${round}_${Date.now()}_${i}`,
            module: 'ctox', type: 'ctox.provider_subscription.status',
            payload: {}, client_context: { capability_token, source: 'multiplex-reload-fixture' },
          }, { until: 'terminal', timeoutMs: 60000 });
          if (result?.status !== 'completed') throw new Error('fixture command not completed');
          commandTimings.push(performance.now() - start);
        }
      })();
      const queries = Promise.all(names.map(async name => {
        const rows = await state.db.raw[name].find({ selector: {}, limit: 200 }).exec();
        return { name, rows: rows.length };
      }));
      await Promise.all([commands, queries]);
      const deadline = performance.now() + 60000;
      let entries;
      while (performance.now() < deadline) {
        const diagnostics = globalThis.ctoxBusinessOsSyncDiagnostics;
        entries = names.map(name => ({ name, ...diagnostics.collections?.[name] }));
        if (entries.every(entry => entry.initialReplicationState === 'complete')) break;
        if (entries.some(entry => entry.status === 'error')) {
          throw new Error(`collection failed: ${JSON.stringify(entries.map(({name,initialReplicationState,lastError}) => ({name,initialReplicationState,lastError})))}`);
        }
        await delay(50);
      }
      if (!entries.every(entry => entry.initialReplicationState === 'complete')) {
        throw new Error(`initial replication incomplete: ${JSON.stringify(entries.map(({name,initialReplicationState}) => ({name,initialReplicationState})))}`);
      }
      const leads = await state.db.raw.sync_reload_probe_01.find({ selector: {}, limit: 100 }).exec();
      const first = (await state.db.raw.sync_reload_probe_01.findOne('record_0000').exec()).toJSON();
      if (leads.length !== (round === 0 ? 36 : 19)) throw new Error(`tombstone divergence: ${leads.length}`);
      if (first.revision_number !== (round === 0 ? 1 : 23)) throw new Error('stale import version survived reload');
      // Demand fetches have a 200-row page bound; verify the full data set by paging.
      const pages = await Promise.all(Array.from({ length: 20 }, (_, pageIndex) =>
        state.db.raw.sync_reload_probe_00.find({
          selector: {}, sort: [{ id: "asc" }], limit: 200, skip: pageIndex * 200,
        }).exec()));
      const threads = new Set(pages.flat().map(row => row.id));
      if (threads.size !== 3911) throw new Error(`missing thread states: ${threads.size}`);
      return { completedCollections: entries.length, leads: leads.length,
        revision: first.revision_number, threadStates: threads.size, commandTimings };
    }, { schemas, round });
    let timer;
    const evidence = await Promise.race([
      work,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`reload round ${round} exceeded 60 seconds`)),
          Math.max(1, 60000 - (Date.now() - started)));
      }),
    ]).finally(() => clearTimeout(timer));
    evidence.reloadMs = Date.now() - started;
    evidence.round = round;
    if (evidence.reloadMs >= 60000) throw new Error(`reload exceeded 60 seconds: ${JSON.stringify(evidence)}`);
    samples.push(evidence);
    console.log(`multiplex_reload_sample=${JSON.stringify(evidence)}`);
  }
  return { names, samples };
}

module.exports = { seed, run };
