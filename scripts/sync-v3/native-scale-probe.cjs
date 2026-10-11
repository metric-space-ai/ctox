// Isolated native scale fixture + real shell/DB/RTC measurements. No tenant access.
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

const names = ['sync_v3_scale_leads', 'sync_v3_scale_commands', 'sync_v3_scale_tasks', 'sync_v3_scale_chats'];
function definitions() {
  return Object.fromEntries(names.map(name => [name, { syncProfile: 'demand-only', schema: {
    version: 0, primaryKey: 'id', type: 'object', additionalProperties: true,
    properties: { id: { type: 'string', maxLength: 180 }, fixtureOnly: { type: 'boolean' },
      ordinal: { type: 'integer' }, padding: { type: 'string' }, updated_at_ms: { type: 'number' } },
    required: ['id', 'fixtureOnly', 'ordinal', 'padding', 'updated_at_ms'], indexes: [['ordinal']],
  } }]));
}
async function seed(runtimeRoot, sqlite) {
  const temp = path.resolve(process.env.TMPDIR || '/nonexistent');
  if (!path.resolve(runtimeRoot).startsWith(temp + path.sep)) throw Error('S0 seed requires a fresh admitted TMPDIR prefix');
  const { loadFixture, fixtureDocument } = await import(pathToFileURL(path.join(__dirname, 'fixture.mjs')));
  const specification = await loadFixture();
  const schemas = definitions();
  const moduleRoot = path.join(runtimeRoot, 'runtime/business-os/local-modules/sync-v3-scale');
  fs.mkdirSync(moduleRoot, { recursive: true });
  fs.writeFileSync(path.join(moduleRoot, 'module.json'), JSON.stringify({
    id: 'sync-v3-scale', title: 'S0 Isolated Scale', version: '1.0.0',
    entry: 'local-modules/sync-v3-scale/index.js', collections: names,
  }));
  fs.writeFileSync(path.join(moduleRoot, 'index.js'), 'export function mount() { return { destroy() {} }; }\n');
  fs.writeFileSync(path.join(moduleRoot, 'collections.schema.json'), JSON.stringify({
    schema_format: 'ctox-business-os-module-collections-v1', collections: schemas,
  }));
  const population = [];
  const baseTime = Date.now() - 60000;
  for (const [offset, collection] of specification.collections.entries()) {
    const name = names[offset];
    const table = `ctox_business_os__${name}__v0`;
    sqlite(`CREATE TABLE "${table}" (id TEXT PRIMARY KEY,revision TEXT NOT NULL,
      deleted INTEGER NOT NULL DEFAULT 0,lastWriteTime REAL NOT NULL,data TEXT NOT NULL);
      CREATE INDEX "${table}_lwt" ON "${table}"(lastWriteTime,id);`);
    let batch = [];
    for (let index = 0; index < collection.count; index++) {
      const doc = { ...fixtureDocument(collection, index), updated_at_ms: baseTime + index,
        _rev: '1-sync-v3-scale', _deleted: false, _meta: { lwt: baseTime + index }, _attachments: {} };
      const excess = Buffer.byteLength(JSON.stringify(doc)) - collection.documentBytes;
      doc.padding = doc.padding.slice(0, doc.padding.length - excess);
      const encoded = JSON.stringify(doc);
      if (Buffer.byteLength(encoded) !== collection.documentBytes) throw Error('Native envelope size mismatch');
      batch.push(`('${doc.id}','${doc._rev}',0,${doc._meta.lwt},'${encoded.replaceAll("'", "''")}')`);
      if (batch.length === 32 || index === collection.count - 1) {
        sqlite(`BEGIN IMMEDIATE;INSERT INTO "${table}" VALUES ${batch.join(',')};COMMIT;`);
        batch = [];
      }
    }
    const observed = JSON.parse(sqlite(`SELECT json_object('count',COUNT(*),'bytes',SUM(length(CAST(data AS BLOB)))) FROM "${table}";`));
    if (observed.count !== collection.count || observed.bytes !== collection.count * collection.documentBytes) throw Error('Native fixture count/bytes mismatch');
    population.push({ sourceCollection: collection.name, nativeCollection: name, ...observed });
  }
  return { fixture: specification.id, totalDocuments: population.reduce((n, p) => n + p.count, 0),
    totalDocumentBytes: population.reduce((n, p) => n + p.bytes, 0), population,
    schemaScope: 'canonical isolated module schemas, size-equivalent envelopes; not production business command semantics' };
}

async function install(browser) {
  await browser.addInitScript(require('./phase-trace.cjs').installPhaseTrace);
}

async function run(page, sqlite, runtimeRoot, rttMs, fixture) {
  await page.exposeFunction('__syncV3EvidenceCheckpoint', value => fs.writeFileSync(
    path.join(runtimeRoot, 'sync-v3-scale-partial.json'), JSON.stringify(value, null, 2) + '\n'));
  await page.exposeFunction('__syncV3NativeReadback', (name, id, marker) => {
    if (!names.includes(name) || !/^sync-v3-[a-z0-9_-]+$/.test(id) || !/^s0-write-[0-9]+$/.test(marker)) throw Error('Invalid isolated readback');
    const row = sqlite(`SELECT data FROM "ctox_business_os__${name}__v0" WHERE id='${id}' AND deleted=0;`).trim();
    return row ? JSON.parse(row).write_marker === marker : false;
  });
  const result = await page.evaluate(async ({ schemas, rttMs, fixture }) => {
    const trace = globalThis.__syncV3Trace;
    trace.mark('fixture-setup');
    const state = globalThis.ctoxBusinessOsSmoke.state;
    const raw = state.db.raw;
    const missing = Object.fromEntries(Object.entries(schemas).filter(([name]) => !raw[name]));
    if (Object.keys(missing).length) await state.db.addCollections(missing);
    const leases = [];
    const samples = [];
    const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
    const withDeadline = async (promise, ms, label) => {
      let timer;
      try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error(label)), ms); })]); }
      finally { clearTimeout(timer); }
    };
    try {
      const names = Object.keys(schemas);
      const collectionReadyAt = performance.now();
      const bridges = await Promise.all(names.map(async name => {
        leases.push(await state.sync.leaseCollection(name, 'sync-v3-s0-measurement', { forceDirect: true }));
        let bridge = await state.sync.startCollection(name, { pin: false, forceDirect: true, requireOwner: true });
        if (!bridge?.state && bridge?.ready) bridge = await withDeadline(bridge.ready, 60000, 'Scale bridge setup timeout');
        if (typeof bridge?.state?.awaitInitialReplication !== 'function') throw Error('Scale initial replication seam missing');
        // startCollection can resolve before onPeerReady attaches the demand loader.
        // Await the production readiness contract, not an empty local query or a retry.
        await withDeadline(bridge.state.awaitInitialReplication(), 60000, 'Scale collection readiness timeout');
        if (!bridge.state.demandLoaderActive || bridge.state.cancelled) throw Error('Scale demand loader not ready');
        return bridge;
      }));
      const leadName = names[0];
      const queryStarted = performance.now();
      trace.mark('fixture-query');
      const rows = await withDeadline(raw[leadName].find({ selector: {}, sort: [{ ordinal: 'asc' }], limit: 20 }).exec(), 60000, 'Scale query timeout');
      trace.mark('rows-ready');
      globalThis.__syncV3WindowDiagnostic = { rowCount: rows.length, uniqueIds: new Set(rows.map(row => row.id)).size,
        firstOrdinal: rows[0]?.ordinal ?? null, ordinals: rows.map(row => row.ordinal ?? null) };
      if (rows.length !== 20 || new Set(rows.map(row => row.id)).size !== 20 || rows[0].ordinal !== 0) {
        throw Error(`Scale visible window incomplete: ${JSON.stringify(globalThis.__syncV3WindowDiagnostic)}`);
      }
      const panel = document.createElement('section');
      panel.id = 'sync-v3-visible-data';
      panel.style.cssText = 'position:fixed;inset:80px 24px auto;z-index:2147483647;background:white;color:black;padding:16px';
      panel.replaceChildren(...rows.map(row => {
        const item = document.createElement('p'); item.dataset.id = row.id; item.textContent = row.id; return item;
      }));
      document.body.append(panel);
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const visibleAt = performance.now();
      trace.mark('visible');
      if (panel.getBoundingClientRect().height <= 0 || getComputedStyle(panel).visibility !== 'visible') throw Error('Scale rows not visible');
      await globalThis.__syncV3EvidenceCheckpoint({ requestedRttMs: rttMs,
        coldPageToVisibleMs: visibleAt - globalThis.__syncV3BootAt,
        collectionSetupToVisibleMs: visibleAt - collectionReadyAt,
        queryToVisibleMs: visibleAt - queryStarted, visibleRows: rows.length, fixture });
      const first = rows[0].toJSON();
      const bridge = bridges[0];
      if (!bridge?.state?.peer?.request) throw Error('Actual replication request seam missing');
      const peer = bridge.state.peer;
      const original = peer.request;
      const requestProofs = [], proofJobs = [];
      try {
        for (let index = 0; index < 5; index++) {
          const marker = `s0-write-${index}`;
          let resolveAck, rejectAck;
          const ack = new Promise((resolve, reject) => { resolveAck = resolve; rejectAck = reject; });
          ack.catch(() => {});
          const started = performance.now();
          trace.marks[`write-${index}-start`] = started;
          let conflictReplies = 0;
          const attempts = [];
          peer.request = async function (...args) {
            const selected = args[1] === 'masterWrite' && args[4] === leadName
              && JSON.stringify(args[2]).includes(marker);
            const connection = selected ? peer.connections?.get?.(args[0]) : null;
            let recordProof;
            if (selected) {
              const proof = { sample: index, method: args[1], peerId: args[0], observedAt: performance.now(),
                channelState: connection?.channel?.readyState || null, pairs: [], snapshots: [] };
              requestProofs.push(proof);
              recordProof = boundary => {
                const job = Promise.resolve(connection?.peer?.getStats?.()).then(stats => {
                  const connectionState = connection?.peer?.connectionState || null;
                  const pairs = stats ? trace.pairsFor(stats, connectionState) : [];
                  proof.snapshots.push({ boundary, capturedAt: performance.now(), pairs, connectionState,
                    candidateStates: stats ? [...stats.values()].filter(item => item.type === 'candidate-pair')
                      .map(item => ({ id: item.id, state: item.state, nominated: item.nominated })) : [] });
                  proof.pairs.push(...pairs);
                });
                job.catch(() => {}); proofJobs.push(job);
              };
              recordProof('before-request');
            }
            try {
              const requestStartedAt = performance.now();
              const response = await original.apply(this, args);
              if (selected) {
                recordProof('after-response');
                if (!connection || peer.connections?.get?.(args[0]) !== connection) rejectAck(Error('Accepted write changed its actual request connection'));
                attempts.push({ startAt: requestStartedAt, endAt: performance.now(), conflicts: Array.isArray(response) ? response.length : null });
                if (!Array.isArray(response)) rejectAck(Error('Native write did not return the canonical ACK/conflict result'));
                else if (response.length) conflictReplies++; // Let the real engine reconcile and retry.
                else resolveAck(performance.now() - started);
              }
              return response;
            } catch (error) { if (selected) rejectAck(error); throw error; }
          };
          const current = index ? (await raw[leadName].findOne(first.id).exec()).toJSON() : first;
          await raw[leadName].upsert({ ...current, write_marker: marker, updated_at_ms: Date.now() });
          const localMs = performance.now() - started;
          trace.mark(`write-${index}-local-commit`);
          const nativeAckMs = await withDeadline(ack, 30000, 'Native masterWrite ACK timeout');
          trace.marks[`write-${index}-ack`] = started + nativeAckMs;
          if (rttMs && nativeAckMs < rttMs * 0.75) throw Error('Write ACK bypassed delayed relay');
          if (!await globalThis.__syncV3NativeReadback(leadName, first.id, marker)) throw Error('Native ACK not backed by SQLite write');
          trace.mark(`write-${index}-sqlite-verified`);
          samples.push({ sample: index, localCommitMs: localMs, nativeAckMs, conflictReplies, sqliteVerified: true, attempts });
        }
      } finally { peer.request = original; }
      await Promise.all(proofJobs);
      const pairs = requestProofs.flatMap(proof => proof.pairs);
      globalThis.__syncV3RequestProofDiagnostic = requestProofs;
      if (requestProofs.length !== samples.reduce((count, sample) => count + sample.attempts.length, 0)
        || requestProofs.some(proof => proof.channelState !== 'open' || !proof.pairs.length)) throw Error('Actual write request relay proof incomplete');
      if (!pairs.length || pairs.some(pair => pair.remoteAddress !== '127.0.0.1' || !Number.isInteger(pair.remotePort))) throw Error('No selected loopback relay candidate proof');
      await trace.drain();
      if (trace.errors.length) throw Error(`Phase trace failed: ${JSON.stringify(trace.errors)}`);
      return { mode: 'sync-v3-scale-relay', requestedRttMs: rttMs,
        coldPageToVisibleMs: visibleAt - globalThis.__syncV3BootAt,
        collectionSetupToVisibleMs: visibleAt - collectionReadyAt,
        queryToVisibleMs: visibleAt - queryStarted, visibleRows: rows.length,
        writes: samples, selectedCandidatePairs: pairs, requestConnectionProofs: requestProofs, installedAcceptance: false,
        phaseTrace: { version: trace.version, bootAt: trace.bootAt, marks: trace.marks, events: trace.events, errors: trace.errors },
        visibleDefinition: '20 native demand-query rows painted in isolated shell overlay',
        writeDefinition: 'local upsert to exact native masterWrite ACK, independently verified in SQLite' };
    } finally { for (const lease of leases) await lease.release(); }
  }, { schemas: definitions(), rttMs, fixture }).catch(async error => {
    const partial = await page.evaluate(async () => {
      const trace = globalThis.__syncV3Trace; await trace?.drain?.();
      return { marks: trace?.marks || {}, events: trace?.events || [], errors: trace?.errors || [],
        queryWindow: globalThis.__syncV3WindowDiagnostic || null, requestProofs: globalThis.__syncV3RequestProofDiagnostic || null };
    }).catch(() => ({ unavailable: true }));
    fs.writeFileSync(path.join(runtimeRoot, 'sync-v3-phase-failure.json'), JSON.stringify({ error: error.message, ...partial }, null, 2) + '\n');
    throw error;
  });
  result.fixture = fixture;
  fs.writeFileSync(path.join(runtimeRoot, 'sync-v3-scale-result.json'), JSON.stringify(result, null, 2) + '\n');
  await page.screenshot({ path: path.join(runtimeRoot, 'sync-v3-visible-data.png') });
  return result;
}
module.exports = { seed, install, run, definitions };
