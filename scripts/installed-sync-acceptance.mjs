/** Installed WebRTC acceptance. Called from an admitted, owned Playwright CLI session.
 * No credentials, customer roots, HTTP record transport, or production sync edits.
 */
import { readFileSync, writeFileSync, mkdirSync, realpathSync, statSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { join, resolve, relative } from 'node:path';
import { performance } from 'node:perf_hooks';

const OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb';
const EXPECTED_SOURCE = '5fbba4af6ab4bc58135d71246608484ed8364035';
const sleep = ms => new Promise(r => setTimeout(r, ms));
const invariant = (value, reason) => { if (!value) {
  const error = new Error(reason); error.name = 'InstalledCriterionError'; throw error;
} };
const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const inside = (parent, child) => {
  const rel = relative(parent, child);
  return rel !== '' && !rel.startsWith('../') && !rel.startsWith('/') && rel !== '..';
};

export function validateConfig(config, configPath) {
  invariant(process.platform === 'linux', 'Acceptance faults require the isolated Linux host');
  invariant(config.owner === OWNER && config.isolated === true, 'Explicit isolated owner binding required');
  invariant(config.source === EXPECTED_SOURCE, 'Use the one approved shared acceptance source');
  const base = realpathSync(config.acceptanceBase);
  invariant(base.startsWith('/mnt/nvme1/') || base.startsWith('/home/metricspace/'), 'Approved host staging only');
  const root = realpathSync(config.root);
  invariant(inside(base, root), 'Tenant must be nested inside the owned acceptance base');
  invariant(inside(base, realpathSync(configPath)), 'Private configuration must stay inside the acceptance base');
  invariant((statSync(configPath).mode & 0o077) === 0, 'Private config must have mode0600');
  const marker = JSON.parse(readFileSync(join(root, '.devops-isolated-acceptance.json')));
  invariant(marker.owner === OWNER && marker.synthetic === true && marker.source === config.source,
    'Fresh synthetic authority marker required; DR/customer restores are forbidden');
  invariant(!/thesen|welsch/i.test(root), 'Customer paths may not be fault targets');
  invariant(inside(root, realpathSync(join(root, 'runtime'))), 'State must stay inside the synthetic prefix');
  const binary = realpathSync(config.binary);
  invariant(inside(root, binary), 'Execute the installed prefix binary, not another service');
  invariant(digest(binary) === config.binarySha256, 'Installed shared binary checksum mismatch');
  invariant(Number.isInteger(config.port) && config.port >= 19000 && config.port <= 29999,
    'Use an isolated loopback listener, never a production service port');
  invariant(Array.isArray(config.goals) && config.goals.length > 0 && config.goals.length <= 3 &&
    new Set(config.goals).size === config.goals.length && config.goals.every(n => [5, 6, 7].includes(n)),
    'Only distinct assigned sync goals supported');
  return { ...config, root, binary, acceptanceBase: base };
}

class OwnedNative {
  constructor(config, output) {
    this.config = config; this.output = output; this.children = new Set(); this.peer = null;
    this.env = { ...process.env, CTOX_ROOT: config.root, CTOX_STATE_ROOT: join(config.root, 'runtime'),
      CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2' };
    this.processes = [];
  }
  save() { writeFileSync(join(this.output, 'owned-processes.json'), JSON.stringify(this.processes, null, 2)); }
  async cli(args, secret = false, timeout = 60000) {
    const child = spawn(this.config.binary, args, { cwd: this.config.root, env: this.env, detached: true,
      stdio: ['ignore', 'pipe', 'pipe'] });
    this.children.add(child);
    const row = { pid: child.pid, pgid: child.pid, kind: args.slice(0, 3).join(' '), terminal: false };
    this.processes.push(row); this.save();
    let stdout = '', bytes = 0;
    const timer = setTimeout(() => { try { process.kill(-child.pid, 'SIGKILL'); } catch {} }, timeout);
    child.stdout.on('data', b => { bytes += b.length; if (bytes > 2 ** 20) {
      try { process.kill(-child.pid, 'SIGKILL'); } catch {};
    } else stdout += b; });
    child.stderr.on('data', () => {}); // Native output may contain invitation credentials.
    try {
      const code = await new Promise((r, j) => { child.once('error', j); child.once('close', r); });
      row.exit = code; invariant(code === 0, `Native ${row.kind} failed (private output suppressed)`);
      return secret ? undefined : JSON.parse(stdout);
    } finally { clearTimeout(timer); row.terminal = child.exitCode !== null || child.signalCode !== null;
      this.children.delete(child); this.save(); }
  }
  start(kind, args) {
    const child = spawn(this.config.binary, args, { cwd: this.config.root, env: this.env,
      stdio: 'ignore', detached: true });
    this.children.add(child);
    const row = { pid: child.pid, pgid: child.pid, kind, terminal: false, stop: 'runner finally or1800s' };
    this.processes.push(row); child.once('error', () => {});
    child.once('close', code => { row.exit = code; row.terminal = true; this.children.delete(child); this.save(); });
    this.save(); return child;
  }
  async stop(child, signal = 'SIGTERM') {
    if (!child) return;
    try { process.kill(-child.pid, signal); } catch (e) { if (e.code !== 'ESRCH') throw e; }
    const until = performance.now() + 5000;
    while (child.exitCode === null && child.signalCode === null && performance.now() < until) await sleep(50);
    try { process.kill(-child.pid, 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') throw e; }
    // Verify the entire owned group, not merely the peer leader.
    const groupDeadline = performance.now() + 5000;
    for (;;) {
      try { process.kill(-child.pid, 0); } catch (e) { if (e.code === 'ESRCH') break; throw e; }
      invariant(performance.now() < groupDeadline, 'Owned process group survived cleanup'); await sleep(50);
    }
  }
  async restartPeer() {
    await this.stop(this.peer, 'SIGKILL');
    this.peer = this.start('isolated-native-peer', ['business-os', 'peer', 'start', '--root', this.config.root]);
  }
  async invite(client) {
    const file = join(this.config.acceptanceBase, `invite-${client}-${randomUUID()}.private.json`);
    await this.cli(['business-os', 'desktop', 'invite', '--display-name', 'DevOps isolated acceptance',
      '--user', 'devops-isolated-sync-owner', '--user-display-name', 'Isolated acceptance owner',
      '--role', 'chef', '--ttl-hours', '1', '--format', 'json', '--output', file, '--root', this.config.root], true);
    invariant((statSync(file).mode & 0o077) === 0, 'Native invitation is not private');
    const value = JSON.parse(readFileSync(file));
    invariant(value.transport === 'webrtc' && value.instance_id && value.sync_room, 'Native WebRTC invitation required');
    return value;
  }
  async read(ids) {
    const script = `import sqlite3,json,sys\nc=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True)\nids=json.loads(sys.stdin.read())\nrows=c.execute('SELECT record_id,payload_json,deleted FROM business_records WHERE collection=?',('desktop_icons',))\nprint(json.dumps({i:json.loads(p) for i,p,d in rows if i in ids and not d}))\n`;
    const child = spawn('python3', ['-c', script, join(this.config.root, 'runtime', 'business-os.sqlite3')],
      { stdio: ['pipe', 'pipe', 'ignore'] });
    let body = ''; const timer = setTimeout(() => child.kill('SIGKILL'), 10000);
    child.stdout.on('data', b => { body += b; if (body.length > 16 * 2 ** 20) child.kill('SIGKILL'); });
    child.stdin.end(JSON.stringify(ids));
    try { const code = await new Promise((r, j) => { child.once('error', j); child.once('close', r); });
      invariant(code === 0, 'Isolated native read-only readback failed'); return JSON.parse(body);
    } finally { clearTimeout(timer); }
  }
  async close() { for (const child of [...this.children]) await this.stop(child); }
}

async function attach(context, origin, config, name, skewMs = 0) {
  const page = await context.newPage();
  await page.goto(`${origin}/rxdb/manifest.json`, { waitUntil: 'domcontentloaded', timeout: 30000 });
  await page.evaluate(async ({ config, name, skewMs }) => {
    if (skewMs) {
      const RealDate = Date;
      class SkewedDate extends RealDate {
        constructor(...args) { super(...(args.length ? args : [RealDate.now() + skewMs])); }
        static now() { return RealDate.now() + skewMs; }
      }
      globalThis.Date = SkewedDate;
    }
    const { createBusinessDb } = await import('/shared/db.js');
    const { createSyncRuntime } = await import('/shared/sync.js');
    const { collections, migrationStrategies } = await import('/modules/desktop/schema.js');
    const db = await createBusinessDb({ name });
    const definition = collections.desktop_icons;
    await db.addCollections({ desktop_icons: migrationStrategies?.desktop_icons
      ? { schema: definition, migrationStrategies: migrationStrategies.desktop_icons } : definition });
    const diagnostics = [], wirePulls = [];
    const runtime = db.rxdb;
    db.rxdb = { ...runtime, async replicateWebRTC(options) {
      const state = await runtime.replicateWebRTC(options);
      const beforeFirstPeer = state.openPeerIds().length === 0;
      const request = state.requestMasterChangesSince.bind(state);
      state.requestMasterChangesSince = async (...args) => {
        const response = await request(...args);
        if (options.collection.name === 'desktop_icons') wirePulls.push({
          at: performance.now(), completeObservation: beforeFirstPeer,
          rows: Array.isArray(response.result?.documents) ? response.result.documents.length : null,
          payloadBytes: new TextEncoder().encode(JSON.stringify(response.result)).byteLength,
        });
        return response; // Observe real replies without changing request, result or checkpoint.
      };
      return state;
    } };
    const sync = createSyncRuntime({ db, config, capabilityTokenProvider: async () => config.session?.capability_token || config.capability_token || null, onDiagnostic(value) {
      // Collect only non-secret diagnostics needed for the stated criteria.
      diagnostics.push({ at: performance.now(), phase: value.phase,
        collection: value.collections?.desktop_icons,
        storage: value.browserStorage }); if (diagnostics.length > 2000) diagnostics.shift();
    } });
    const session = { db, sync, diagnostics, wirePulls, state: null };
    globalThis.__installedAcceptance = session;
    const bridge = await sync.startCollection('desktop_icons');
    const ready = bridge?.ready ? await bridge.ready : bridge;
    session.state = ready?.state;
  }, { config, name, skewMs });
  return page;
}
const docs = (page, ids) => page.evaluate(async ids => {
  const collection = globalThis.__installedAcceptance.db.collections.desktop_icons;
  // Read the real local cache without triggering remote demand queries.
  const found = await collection.storageCollection.findDocumentsById(ids);
  return Array.isArray(found) ? Object.fromEntries(found.map(doc => [doc.id, doc])) : found;
}, ids);
const same = (actual, expected) => expected.every(d => Object.entries(d).every(([k, v]) => actual[d.id]?.[k] === v));
async function converge(native, page, expected, timeout = 60000) {
  const started = performance.now();
  while (performance.now() - started < timeout) {
    if (same(await native.read(expected.map(x => x.id)), expected) && same(await docs(page, expected.map(x => x.id)), expected))
      return performance.now() - started;
    await sleep(100);
  }
  invariant(false, 'Native and second browser failed exact-value convergence');
}
async function waitNative(native, expected) {
  const started = performance.now();
  while (performance.now() - started < 60000) {
    if (same(await native.read(expected.map(x => x.id)), expected)) return;
    await sleep(100);
  }
  invariant(false, 'Server changes did not persist before client reopen');
}
async function write(page, values) {
  return page.evaluate(async values => {
    const c = globalThis.__installedAcceptance.db.collections.desktop_icons, timings = [];
    for (const value of values) { const start = performance.now(); await c.upsert(value); timings.push(performance.now() - start); }
    return timings;
  }, values);
}
async function closePage(page) {
  if (!page || page.isClosed()) return;
  await page.evaluate(async () => { await globalThis.__installedAcceptance?.sync.stop(); await globalThis.__installedAcceptance?.db.close(); });
  await page.close();
}
async function metrics(page) {
  return page.evaluate(async () => {
    const s = globalThis.__installedAcceptance;
    return { diagnostics: s.diagnostics, wirePulls: s.wirePulls,
      unsynced: await s.db.getUnsyncedWriteSummary(), conflicts: await s.db.conflicts.list() };
  });
}

/** A CLI run-code callback passes its OWN browser; two contexts keep separate IndexedDB caches. */
export async function runAcceptance(browser, configPath) {
  const config = validateConfig(JSON.parse(readFileSync(configPath)), configPath);
  const output = resolve(config.output); invariant(inside(config.acceptanceBase, output), 'Evidence escaped owned staging');
  mkdirSync(output, { recursive: false, mode: 0o700 });
  const native = new OwnedNative(config, output), contexts = [], receipts = [];
  const started = performance.now();
  const deadline = setTimeout(() => {
    for (const context of contexts) void context.close();
    void native.close();
  }, 1800000);
  const origin = `http://127.0.0.1:${config.port}`;
  const revisions = { workjet: config.workjetRevision ?? null, native: config.source,
    nativeBinarySha256: config.binarySha256, shell: config.source, contractHashes: config.contractHashes };
  try {
    native.peer = native.start('isolated-native-peer', ['business-os', 'peer', 'start', '--root', config.root]);
    native.start('isolated-static-shell', ['business-os', 'serve', '--addr', `127.0.0.1:${config.port}`]);
    for (const goal of config.goals) {
      const receipt = { goal, revisions, hosts: [config.host], steps: [], measured: {}, criterion: {}, pass: false,
        artifacts: [], clientType: 'Installed canonical DB+sync modules in real Chromium; not a Shell UI acceptance',
        transport: 'webrtc', customerWrites: false };
      receipts.push(receipt);
      let A, B, a, b;
      try {
        a = await browser.newContext(); b = await browser.newContext(); contexts.push(a, b);
        a.setDefaultTimeout(30000); b.setDefaultTimeout(30000);
        const name = `ctox-installed-acceptance-${goal}-${randomUUID()}`;
        A = await attach(a, origin, await native.invite('A'), name + '-a');
        B = await attach(b, origin, await native.invite('B'), name + '-b');
        if (goal === 5) {
          receipt.criterion = { runs: 3, documentsPerRun: 200, offlineMs: 30000, maxLocalWriteMs: 200, maxCatchupMs: 10000 };
          receipt.measured.runs = [];
          for (let round = 0; round < 3; round++) {
            const values = Array.from({ length: 200 }, (_, i) => ({ id: `acceptance-${name}-${round}-${i}`,
              target_type: 'acceptance', label: `round${round}-document${i}`, x: i, y: round, updated_at_ms: Date.now() }));
            await a.setOffline(true); const offlineStart = performance.now();
            const timings = await write(A, values);
            await native.restartPeer();
            await closePage(B); B = await attach(b, origin, await native.invite('B'), name + '-b');
            await sleep(Math.max(0, 30000 - (performance.now() - offlineStart)));
            const offlineMs = performance.now() - offlineStart;
            const reconnectStart = performance.now();
            await a.setOffline(false);
            // Renew native-issued login and reopen the SAME IndexedDB; no cache wipe.
            await closePage(A); A = await attach(a, origin, await native.invite('A-relogin'), name + '-a');
            await converge(native, B, values);
            const catchupMs = performance.now() - reconnectStart;
            receipt.measured.runs.push({ round, offlineMs,
              maxLocalWriteMs: Math.max(...timings), catchupMs, exactDocumentsOnServerAndB: 200 });
          }
          receipt.pass = receipt.measured.runs.every(x => x.maxLocalWriteMs <= 200 && x.catchupMs <= 10000 && x.offlineMs >= 30000);
        } else if (goal === 6) {
          receipt.criterion = { cachedDocuments: 10000, changedServerDocuments: 50, maxTransferredDocuments: 50,
            separateLocalAndCatchupTimes: true, backlogMustReachZero: true };
          const values = Array.from({ length: 10000 }, (_, i) => ({ id: `acceptance-${name}-${i}`,
            target_type: 'acceptance', label: `initial${i}`, x: i, y: 0, updated_at_ms: Date.now() }));
          await A.evaluate(async values => { await globalThis.__installedAcceptance.db.collections.desktop_icons.bulkUpsert(values); }, values);
          await converge(native, B, values, 240000);
          await closePage(B);
          const changed = values.slice(0, 50).map(d => ({ ...d, label: d.label + '-changed', updated_at_ms: Date.now() }));
          await write(A, changed);
          await waitNative(native, changed);
          const reopenStart = performance.now(); B = await attach(b, origin, await native.invite('B-reopen'), name + '-b');
          receipt.measured.localUsableMs = performance.now() - reopenStart;
          receipt.measured.catchupMs = await converge(native, B, changed);
          receipt.measured.diagnostics = await metrics(B);
          // Do not infer wire row counts from HTTP sizes, final cache count or checkpoint ages.
          const pulls = receipt.measured.diagnostics.wirePulls;
          receipt.measured.transferredDocuments = pulls.length && pulls.every(p => p.completeObservation && p.rows !== null)
            ? pulls.reduce((total, p) => total + p.rows, 0) : null;
          receipt.measured.webRtcReplyBytes = pulls.reduce((total, p) => total + p.payloadBytes, 0);
          receipt.measured.backlogIndicatorObserved = false;
          receipt.measured.telemetryMissing = 'Actual Shell backlog indicator must be observed; module counters do not certify UI';
          receipt.pass = false;
        } else {
          receipt.criterion = { clockOffsetsMs: [-600000, 600000], noFalseClockError: true,
            distinctFieldMerge: true, sameFieldConflictBothValues: true, staleRevisionTypedUnapplied: true };
          receipt.measured.clock = [];
          for (const skewMs of receipt.criterion.clockOffsetsMs) {
            await closePage(A); A = await attach(a, origin, await native.invite('A-clock'), name + '-a', skewMs);
            const value = { id: `acceptance-${name}-clock-${skewMs}`, target_type: 'acceptance', label: 'clock-check', updated_at_ms: Date.now() };
            await write(A, [value]);
            await converge(native, B, [value]);
            receipt.measured.clock.push({ skewMs, metrics: await metrics(A), converged: true });
          }
          await closePage(A); A = await attach(a, origin, await native.invite('A-clock-reset'), name + '-a');
          const id = `acceptance-${name}-merge`, base = { id, target_type: 'acceptance', label: 'base', x: 0, y: 0, updated_at_ms: Date.now() };
          await write(A, [base]); await converge(native, B, [base]);
          const staleBaseline = (await docs(A, [id]))[id];
          await a.setOffline(true); await b.setOffline(true);
          await write(A, [{ ...base, x: 1 }]); await write(B, [{ ...base, y: 2 }]);
          await a.setOffline(false); await b.setOffline(false);
          await converge(native, B, [{ id, x: 1, y: 2 }]);
          receipt.measured.distinctFieldMerge = true;
          await a.setOffline(true); await b.setOffline(true);
          const beforeA = (await docs(A, [id]))[id], beforeB = (await docs(B, [id]))[id];
          await write(A, [{ ...beforeA, label: 'same-field-A' }]); await write(B, [{ ...beforeB, label: 'same-field-B' }]);
          await a.setOffline(false); await b.setOffline(false); await sleep(5000);
          receipt.measured.sameField = { A: await metrics(A), B: await metrics(B), server: await native.read([id]) };
          const masterBefore = await native.read([id]);
          const staleResponse = await A.evaluate(async ({ old, id }) => {
            const state = globalThis.__installedAcceptance.state;
            if (!state?.peer?.request || !state.waitForOpenPeerId || !old?._rev) return { available: false };
            const peer = await state.waitForOpenPeerId();
            const result = await state.peer.request(peer, 'masterWrite', [[{
              assumedMasterState: old,
              newDocumentState: { ...old, label: 'STALE-MUST-NOT-APPLY' },
            }]], 10000, 'desktop_icons');
            // A conflicts array is the real protocol type, not a browser exception.
            return { available: true, conflictArray: Array.isArray(result),
              conflictingId: Array.isArray(result) && result.some(doc => doc.id === id),
              errorCode: typeof result?.ctoxError?.code === 'string' ? result.ctoxError.code : null };
          }, { old: staleBaseline, id });
          const masterAfter = await native.read([id]);
          receipt.measured.staleRevision = { response: staleResponse,
            unapplied: JSON.stringify(masterBefore) === JSON.stringify(masterAfter),
            usedWebrtcMasterWrite: staleResponse.available === true };
          const visibleBothValues = [...receipt.measured.sameField.A.conflicts,
            ...receipt.measured.sameField.B.conflicts].some(record => {
              const text = JSON.stringify(record);
              return text.includes('same-field-A') && text.includes('same-field-B');
            });
          receipt.measured.sameFieldBothValuesVisible = visibleBothValues;
          receipt.pass = receipt.measured.clock.every(row => row.converged &&
            !JSON.stringify(row.metrics.conflicts).includes('clock_skew_detected')) &&
            receipt.measured.distinctFieldMerge && visibleBothValues &&
            receipt.measured.staleRevision.unapplied &&
            (staleResponse.conflictingId || Boolean(staleResponse.errorCode));
        }
        receipt.steps.push('Installed runtime opened, real native peer and two separate Chromium contexts used');
      } catch (error) {
        receipt.failure = { name: error.name,
          errorSha256: createHash('sha256').update(String(error.message)).digest('hex'),
          message: error.name === 'InstalledCriterionError' ? error.message
            : 'Bounded installed acceptance failed; no credentials or raw native errors exported' };
        receipt.steps.push('First failing assertion retained; no repeated fault loop or inferred pass');
      } finally {
        for (const page of [A, B]) { try { if (page && !page.isClosed()) {
          const file = join(output, `goal-${goal}-${page === A ? 'A' : 'B'}.png`);
          await page.screenshot({ path: file }); receipt.artifacts.push(file);
          await closePage(page);
        } } catch {} }
        await a?.close(); await b?.close();
        const file = join(output, `${String(goal).padStart(2, '0')}-installed-sync.json`);
        writeFileSync(file, JSON.stringify(receipt, null, 2) + '\n', { mode: 0o600 });
      }
    }
  } finally {
    clearTimeout(deadline);
    for (const context of contexts) { try { await context.close(); } catch {} }
    await native.close();
    writeFileSync(join(output, 'runner-summary.json'), JSON.stringify({ owner: OWNER, source: config.source,
      seconds: performance.now() - started, receipts: receipts.map(x => ({ goal: x.goal, pass: x.pass })),
      stopped: true, scope: 'No production writes; no sync source repairs' }, null, 2));
  }
  return { output, goals: receipts.map(x => ({ goal: x.goal, pass: x.pass })) };
}
