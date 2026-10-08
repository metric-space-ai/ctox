/** Installed WebRTC acceptance. Called from an admitted, owned Playwright browser.
 * No credentials, customer roots, HTTP record transport, or production sync edits.
 */
import { readFileSync, writeFileSync, mkdirSync, realpathSync, statSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { join, resolve, relative } from 'node:path';
import { performance } from 'node:perf_hooks';
import { browserTransportSnapshot } from './installed-sync-browser-transport.mjs';

const OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb';
const EXPECTED_SOURCE = 'b48220db385d53f07c8414c21f7ce22cdd8ff44d';
const sleep = ms => new Promise(r => setTimeout(r, ms));
const invariant = (value, reason) => { if (!value) {
  const error = new Error(reason); error.name = 'InstalledCriterionError'; throw error;
} };
const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
export function logExcerpt(text) {
  // Keep only named diagnostic vocabulary, never arbitrary values, URLs or credentials.
  const classes = [...new Set(String(text).match(/\b(?:AUTH_DENIED|UNAUTHORIZED|STREAM_LIMIT_EXCEEDED|clock_skew_detected|master_key_busy|SQLITE_BUSY|WebRTC|replication|signaling|handshake|authenticated|disconnected|connected|denied|timeout|conflict|push|pull|error|failed)\b/gi) || [])];
  return { classes, bytes: Buffer.byteLength(String(text)),
    sha256: createHash('sha256').update(String(text)).digest('hex') };
}
export function documentAudit(expected, sources) {
  invariant(expected.every(value => typeof value.id === 'string' && value.id.startsWith('acceptance-')),
    'Only generated synthetic acceptance IDs may be exported');
  const metadata = doc => {
    if (!doc) return null;
    const scalar = value => typeof value === 'number' || typeof value === 'boolean' || value === null ? value
      : typeof value === 'string' && /^[A-Za-z0-9_.:+-]{1,180}$/.test(value) ? value : null;
    const clock = value => value && typeof value === 'object'
      ? Object.fromEntries(Object.entries(value).slice(0, 12).map(([key, item]) => [key, scalar(item)])) : scalar(value);
    const hlc = Object.fromEntries(Object.entries(doc).filter(([key]) => /hlc/i.test(key)).map(([key, item]) => [key, clock(item)]));
    for (const key of ['_meta', '_ctox']) if (doc[key] && typeof doc[key] === 'object') {
      for (const [field, item] of Object.entries(doc[key])) if (/hlc/i.test(field)) hlc[`${key}.${field}`] = clock(item);
    }
    return { revision: scalar(doc._rev ?? doc.revision ?? null), hlc: Object.keys(hlc).length ? hlc : null,
      lwt: scalar(doc._meta?.lwt ?? null), deleted: Boolean(doc._deleted),
      metadataScope: 'Returned installed payload plus actual native SQLite revision/lastWriteTime columns; absent HLC stays null' };
  };
  const counts = Object.fromEntries(Object.entries(sources).map(([name, actual]) => [name, actual === null
    ? { available: false } : { available: true, found: expected.filter(d => actual[d.id]).length,
      exact: expected.filter(d => same(actual, [d])).length }]));
  const differences = expected.filter(value => Object.values(sources).some(actual => actual !== null && !same(actual, [value])))
    .map(value => ({ id: value.id, sources: Object.fromEntries(Object.entries(sources).map(([name, actual]) => [name,
      { available: actual !== null, missing: actual !== null && !actual[value.id],
        differingFields: actual?.[value.id] ? Object.keys(value).filter(key => actual[value.id][key] !== value[key]) : [],
        metadata: metadata(actual?.[value.id]) }])) }));
  return { expected: expected.length, counts, differences };
}
const inside = (parent, child) => {
  const rel = relative(parent, child);
  return rel !== '' && !rel.startsWith('../') && !rel.startsWith('/') && rel !== '..';
};

export function validateConfig(config, configPath) {
  invariant(process.platform === 'linux', 'Acceptance faults require the isolated Linux host');
  invariant(config.owner === OWNER && config.isolated === true, 'Explicit isolated owner binding required');
  invariant(config.source === EXPECTED_SOURCE, 'Use the one approved shared acceptance source');
  invariant(typeof config.actorId === 'string' && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(config.actorId),
    'Fresh preparation must provide a canonical synthetic actor UUID');
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

const nativeControllers = new Set();
let stopping = false;
export async function stopOwnedAcceptance() {
  stopping = true;
  await Promise.all([...nativeControllers].map(controller => controller.close()));
}
class OwnedNative {
  constructor(config, output) {
    this.config = config; this.output = output; this.children = new Set(); this.peer = null; this.events = [];
    const home = join(config.root, 'runtime', 'acceptance-home');
    mkdirSync(home, { recursive: true, mode: 0o700 });
    const inherited = Object.fromEntries(['PATH', 'LANG', 'LC_ALL', 'TMPDIR', 'CARGO_TARGET_DIR',
      'XDG_CACHE_HOME', 'npm_config_cache'].filter(key => process.env[key]).map(key => [key, process.env[key]]));
    this.env = { ...inherited, HOME: home, CTOX_ROOT: config.root, CTOX_STATE_ROOT: join(config.root, 'runtime'),
      CARGO_BUILD_JOBS: '2', RUST_TEST_THREADS: '2' };
    this.processes = [];
    nativeControllers.add(this);
  }
  save() { writeFileSync(join(this.output, 'owned-processes.json'), JSON.stringify(this.processes, null, 2)); }
  async cli(args, secret = false, timeout = 60000) {
    invariant(!stopping, 'Acceptance unit is stopping; no new native command allowed');
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
      await this.stop(child); this.save(); }
  }
  start(kind, args) {
    invariant(!stopping, 'Acceptance unit is stopping; no new native process allowed');
    const child = spawn(this.config.binary, args, { cwd: this.config.root, env: this.env,
      stdio: ['ignore', 'pipe', 'pipe'], detached: true });
    // Private rolling raw excerpts stay on the isolated host; exported evidence is vocabulary-only.
    const privateLog = join(this.output, `${kind}-${child.pid}.log.private`), lines = [];
    const capture = stream => stream.on('data', chunk => {
      for (const line of String(chunk).split('\n').filter(Boolean).slice(-64)) {
        const at = new Date().toISOString();
        lines.push(`${at} ${line.slice(0, 2048)}`);
        const event = { at, kind, ...logExcerpt(line) };
        if (event.classes.length) this.events.push(event);
      }
      while (lines.length > 64) lines.shift();
      while (this.events.length > 200) this.events.shift();
      writeFileSync(privateLog, lines.join('\n') + '\n', { mode: 0o600 });
    });
    capture(child.stdout); capture(child.stderr);
    this.children.add(child);
    const row = { pid: child.pid, pgid: child.pid, kind, terminal: false, privateLog,
      stop: 'runner finally or1800s' };
    this.processes.push(row); child.once('error', () => {});
    child.once('close', code => { row.exit = code; row.terminal = true; this.save(); });
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
    this.children.delete(child);
    const row = this.processes.find(item => item.pid === child.pid);
    if (row) { row.cleanup = 'owned group absent'; row.terminal = true; this.save(); }
  }
  async restartPeer() {
    await this.stop(this.peer, 'SIGKILL');
    this.peer = this.start('isolated-native-peer', ['business-os', 'peer', 'start', '--root', this.config.root]);
  }
  async invite(client) {
    const file = join(this.config.acceptanceBase, `invite-${client}-${randomUUID()}.private.json`);
    // Native fs::write preserves an existing mode, but does not itself create0600.
    writeFileSync(file, '', { mode: 0o600, flag: 'wx' });
    await this.cli(['business-os', 'desktop', 'invite', '--display-name', 'DevOps isolated acceptance',
      '--user', this.config.actorId, '--user-display-name', 'Isolated acceptance owner',
      '--role', 'chef', '--ttl-hours', '1', '--format', 'json', '--output', file, '--root', this.config.root], true);
    invariant((statSync(file).mode & 0o077) === 0, 'Native invitation is not private');
    const value = JSON.parse(readFileSync(file));
    invariant(value.transport === 'webrtc' && value.instance_id && value.sync_room, 'Native WebRTC invitation required');
    return value;
  }
  async read(ids) {
    const script = `import sqlite3,json,sys\nc=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True)\nids=set(json.loads(sys.stdin.read()))\nrows=c.execute('SELECT id,data,revision,lastWriteTime,deleted FROM ctox_business_os__desktop_icons__v0')\nout={}\nfor i,p,rev,lwt,deleted in rows:\n if i in ids and not deleted:\n  doc=json.loads(p);doc.setdefault('_rev',rev);doc.setdefault('_meta',{}).setdefault('lwt',lwt);out[i]=doc\nprint(json.dumps(out))\n`;
    const child = spawn('python3', ['-c', script, join(this.config.root, 'runtime', 'business-os-rxdb.sqlite3')],
      { stdio: ['pipe', 'pipe', 'ignore'] });
    let body = ''; const timer = setTimeout(() => child.kill('SIGKILL'), 10000);
    child.stdout.on('data', b => { body += b; if (body.length > 16 * 2 ** 20) child.kill('SIGKILL'); });
    child.stdin.end(JSON.stringify(ids));
    try { const code = await new Promise((r, j) => { child.once('error', j); child.once('close', r); });
      invariant(code === 0, 'Isolated native read-only readback failed'); return JSON.parse(body);
    } finally { clearTimeout(timer); }
  }
  async close() {
    for (const child of [...this.children]) await this.stop(child);
    nativeControllers.delete(this);
  }
}

/** Goal23 component evidence only. The installed three-component health remains a separate criterion. */
export async function measureShellRollback(configPath) {
  const config = validateConfig(JSON.parse(readFileSync(configPath)), configPath);
  const output = join(config.acceptanceBase, 'component-rollback');
  const { existsSync } = await import('node:fs');
  if (existsSync(join(output, 'component-rollback.json'))) {
    const previous = JSON.parse(readFileSync(join(output, 'component-rollback.json')));
    invariant(previous.pass && previous.owner === OWNER && previous.source === config.source
      && previous.host === config.host && previous.isolated && !previous.productionWrites
      && previous.steps.length === 3
      && previous.steps[0].appSha256 === config.contractHashes['app.js']
      && previous.steps[2].appSha256 === config.contractHashes['app.js'],
    'Only a completed exact-prefix component proof may be reused');
    const recheck = join(config.acceptanceBase, `component-recheck-${Date.now()}`);
    mkdirSync(recheck, { mode: 0o700 });
    const native = new OwnedNative(config, recheck);
    try {
      const current = await native.cli(['business-os', 'shell-update', 'status']);
      invariant(current.currentSlot === null
        && digest(join(config.root, 'src/apps/business-os/app.js')) === config.contractHashes['app.js'],
      'Previously restored builtin component has changed');
    } finally { await native.close(); }
    return { output, componentRollbackPassed: true, wholeInstalledSetPassed: false, reused: true };
  }
  mkdirSync(output, { mode: 0o700 });
  const native = new OwnedNative(config, output);
  const record = { owner: OWNER, source: config.source, host: config.host, isolated: true,
    component: 'business-os-shell', steps: [], pass: false, productionWrites: false };
  let server = null, activated = false, restored = false;
  const shell = args => native.cli(['business-os', 'shell-update', ...args], false, 180000);
  const serve = async () => {
    await native.stop(server);
    server = native.start('isolated-static-shell-rollback', ['business-os', 'serve', '--addr', `127.0.0.1:${config.port}`]);
    const until = performance.now() + 30000;
    while (performance.now() < until) {
      invariant(server.exitCode === null && server.signalCode === null, 'Owned static server exited');
      try {
        const response = await fetch(`http://127.0.0.1:${config.port}/app.js?v=${randomUUID()}`,
          { cache: 'no-store', signal: AbortSignal.timeout(3000) });
        if (response.ok) {
          const bytes = Buffer.from(await response.arrayBuffer());
          invariant(bytes.length < 4 * 2 ** 20, 'Static app size bound');
          return createHash('sha256').update(bytes).digest('hex');
        }
      } catch (error) { if (error.name === 'InstalledCriterionError') throw error; }
      await sleep(100);
    }
    invariant(false, 'Isolated static server did not serve a versioned app');
  };
  try {
    const initial = await shell(['status']);
    invariant(initial.currentSlot === null, 'Fresh acceptance prefix must initially serve builtin Main');
    const beforeHash = await serve();
    invariant(beforeHash === config.contractHashes['app.js'], 'Installed builtin app differs from verified shared Main');
    record.steps.push({ action: 'baseline', slot: null, appSha256: beforeHash });
    await shell(['stage', '--version', '0.1.46-beta.79']);
    await shell(['activate']); activated = true;
    const previous = await shell(['status']);
    invariant(previous.currentSlot === '0.1.46-beta.79', 'Signed predecessor was not activated');
    const previousHash = await serve();
    invariant(previousHash !== beforeHash, 'Backward switch must change the actually served app');
    record.steps.push({ action: 'activate-signed-predecessor', slot: '0.1.46-beta.79', appSha256: previousHash,
      signatureVerifiedByNativeStage: true });
    await shell(['rollback']); restored = true;
    const final = await shell(['status']);
    invariant(final.currentSlot === null, 'Supported rollback did not restore builtin Main');
    const afterHash = await serve();
    invariant(afterHash === beforeHash, 'Forward restoration did not restore exact installed app');
    invariant(digest(config.binary) === config.binarySha256, 'Shell switch changed installed native binary');
    record.steps.push({ action: 'restore-builtin-Main', slot: null, appSha256: afterHash });
    record.pass = true;
  } catch (error) {
    record.failure = { name: error.name, message: error.name === 'InstalledCriterionError' ? error.message
      : 'Component rollback failed; private native output suppressed' };
  } finally {
    if (activated && !restored) {
      try { await shell(['rollback']); record.restoredAfterFailure = true; }
      catch { record.restoredAfterFailure = false; }
    }
    await native.close();
    writeFileSync(join(output, 'component-rollback.json'), JSON.stringify(record, null, 2) + '\n', { mode: 0o600 });
  }
  return { output, componentRollbackPassed: record.pass, wholeInstalledSetPassed: false };
}

async function attach(context, origin, config, name, skewMs = 0, existingPage = null, localProbeIds = []) {
  const localOpenStarted = performance.now();
  const page = existingPage || await context.newPage();
  if (!existingPage) {
    page.installedLogEvents = [];
    const capture = (kind, value) => {
      const event = { at: new Date().toISOString(), kind, ...logExcerpt(value) };
      if (event.classes.length) page.installedLogEvents.push(event);
      if (page.installedLogEvents.length > 200) page.installedLogEvents.shift();
    };
    page.on('console', message => capture(message.type(), message.text()));
    page.on('pageerror', error => capture('pageerror', error.message));
  }
  if (existingPage) await page.reload({ waitUntil: 'domcontentloaded', timeout: 30000 });
  else await page.goto(`${origin}/rxdb/manifest.json`, { waitUntil: 'domcontentloaded', timeout: 30000 });
  const localReadback = await page.evaluate(async ({ config, name, skewMs, localProbeIds }) => {
    if (skewMs) {
      const RealDate = Date;
      class SkewedDate extends RealDate {
        constructor(...args) { super(...(args.length ? args : [RealDate.now() + skewMs])); }
        static now() { return RealDate.now() + skewMs; }
      }
      globalThis.Date = SkewedDate;
    }
    const { createBusinessDb } = await import('/shared/db.js');
    const { collections, migrationStrategies } = await import('/modules/desktop/schema.js');
    const db = await createBusinessDb({ name });
    const definition = collections.desktop_icons;
    await db.addCollections({ desktop_icons: migrationStrategies?.desktop_icons
      ? { schema: definition, migrationStrategies: migrationStrategies.desktop_icons } : definition });
    const localStarted = performance.now();
    const localDocuments = localProbeIds.length
      ? await db.collections.desktop_icons.storageCollection.findDocumentsById(localProbeIds) : {};
    const localReadMs = performance.now() - localStarted;
    globalThis.__installedAcceptance = { db, sync: null, diagnostics: [], wirePulls: [], state: null, writesCompleted: 0 };
    return { localReadMs, cachedDocuments: Array.isArray(localDocuments)
      ? localDocuments.length : Object.keys(localDocuments).length };
  }, { config, name, skewMs, localProbeIds });
  // Capture local readiness BEFORE creating or awaiting the remote sync bridge.
  page.installedCacheReadback = { ...localReadback, localOpenAndReadMs: performance.now() - localOpenStarted };
  await page.evaluate(async config => {
    const session = globalThis.__installedAcceptance;
    const { db, diagnostics, wirePulls } = session;
    const { createSyncRuntime } = await import('/shared/sync.js');
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
    session.sync = sync;
    const bridge = await sync.startCollection('desktop_icons');
    const ready = bridge?.ready ? await bridge.ready : bridge;
    session.state = ready?.state;
  }, config);
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
  let server = {}, client = {};
  while (performance.now() - started < timeout) {
    invariant(!stopping, 'Acceptance interrupted; no further convergence or fault attempts');
    server = await native.read(expected.map(x => x.id));
    client = await docs(page, expected.map(x => x.id));
    if (same(server, expected) && same(client, expected))
      return performance.now() - started;
    await sleep(100);
  }
  const error = new Error('Native and second browser failed exact-value convergence');
  error.name = 'InstalledCriterionError';
  error.convergence = { expectedDocuments: expected.length, elapsedMs: performance.now() - started,
    nativeFound: Object.keys(server).length, secondBrowserFound: Object.keys(client).length,
    nativeExact: expected.filter(value => same(server, [value])).length,
    secondBrowserExact: expected.filter(value => same(client, [value])).length };
  throw error;
}
async function waitNative(native, expected) {
  const started = performance.now();
  while (performance.now() - started < 60000) {
    invariant(!stopping, 'Acceptance interrupted; no further native wait or fault attempts');
    if (same(await native.read(expected.map(x => x.id)), expected)) return;
    await sleep(100);
  }
  invariant(false, 'Server changes did not persist before client reopen');
}
async function write(page, values) {
  return page.evaluate(async values => {
    const c = globalThis.__installedAcceptance.db.collections.desktop_icons, timings = [];
    for (const value of values) { const start = performance.now(); await c.upsert(value);
      globalThis.__installedAcceptance.writesCompleted++; timings.push(performance.now() - start); }
    return timings;
  }, values);
}
async function closePage(page) {
  if (!page || page.isClosed()) return;
  await page.evaluate(async () => { await globalThis.__installedAcceptance?.sync.stop(); await globalThis.__installedAcceptance?.db.close(); });
  await page.close();
}
const networkSessions = new WeakMap();
async function offline(context, page, value) {
  await context.setOffline(value);
  const session = networkSessions.get(page) || await context.newCDPSession(page);
  networkSessions.set(page, session);
  // CDP's packetLoss explicitly affects WebRTC rather than just HTTP.
  // https://github.com/ChromeDevTools/devtools-protocol/blob/master/pdl/domains/Network.pdl
  await session.send('Network.emulateNetworkConditions', {
    offline: value, latency: 0, downloadThroughput: -1, uploadThroughput: -1,
    packetLoss: value ? 100 : 0,
  });
}
async function metrics(page) {
  return page.evaluate(async () => {
    const s = globalThis.__installedAcceptance;
    return { diagnostics: s.diagnostics, wirePulls: s.wirePulls,
      unsynced: await s.db.getUnsyncedWriteSummary(), conflicts: await s.db.conflicts.list() };
  });
}

// Only counters, flags and named short status/code fields leave browser memory.
// Invitations, errors with URLs, arbitrary strings and document values are excluded.
export function diagnosticScalars(value, depth = 0, field = '') {
  if (/token|secret|credential|password|authorization|bearer|url|endpoint/i.test(field)
    || /^(documents?|conflicts?|newDocumentState|assumedMasterState)$/i.test(field)) return undefined;
  if (value === null || typeof value === 'boolean' || (typeof value === 'number' && Number.isFinite(value))) return value;
  if (typeof value === 'string') return /^(phase|status|state|code|errorCode|connectionState|signalingState)$/.test(field)
    && /^[A-Za-z_][A-Za-z0-9_ .:-]{0,79}$/.test(value) ? value : undefined;
  if (!value || typeof value !== 'object' || depth >= 4) return undefined;
  if (Array.isArray(value)) return value.slice(-12).map(item => diagnosticScalars(item, depth + 1));
  return Object.fromEntries(Object.entries(value).slice(0, 64)
    .map(([key, item]) => [key, diagnosticScalars(item, depth + 1, key)]).filter(([, item]) => item !== undefined));
}
async function failureMetrics(page) {
  if (!page || page.isClosed()) return { available: false };
  let timer;
  let transport = { available: false, reason: 'transport-unavailable' };
  try {
    const [raw] = await Promise.race([Promise.all([
      metrics(page),
      page.evaluate(browserTransportSnapshot).then(value => { transport = value; })
        .catch(() => {}),
    ]), new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('Failure snapshot deadline')), 4000);
    })]);
    return { available: true, diagnostics: diagnosticScalars(raw.diagnostics.slice(-12)),
      unsynced: diagnosticScalars(raw.unsynced), conflictCount: raw.conflicts.length,
      wirePulls: diagnosticScalars(raw.wirePulls.slice(-12)), transport };
  } catch { return { available: false, transport }; }
  finally { clearTimeout(timer); }
}

/** The admitted host launcher passes its OWN browser; two contexts keep separate IndexedDB caches. */
export async function runAcceptance(browser, configPath) {
  const config = validateConfig(JSON.parse(readFileSync(configPath)), configPath);
  const output = resolve(`${config.output}-${Date.now()}-${process.pid}`);
  invariant(inside(config.acceptanceBase, output), 'Evidence escaped owned staging');
  mkdirSync(output, { recursive: false, mode: 0o700 });
  const native = new OwnedNative(config, output), contexts = [], receipts = [];
  const started = performance.now();
  const deadline = setTimeout(() => {
    for (const context of contexts) void context.close();
    void stopOwnedAcceptance();
  }, 1800000);
  const origin = `http://127.0.0.1:${config.port}`;
  const revisions = { workjet: config.workjetRevision ?? null, native: config.source,
    nativeBinarySha256: config.binarySha256, shell: config.source, contractHashes: config.contractHashes };
  try {
    native.peer = native.start('isolated-native-peer', ['business-os', 'peer', 'start', '--root', config.root]);
    native.start('isolated-static-shell', ['business-os', 'serve', '--addr', `127.0.0.1:${config.port}`]);
    for (const goal of config.goals) {
      const criteria = {
        5: { runs: 3, documentsPerRun: 200, offlineMs: 30000, maxLocalWriteMs: 200, maxCatchupMs: 10000 },
        6: { cachedDocuments: 10000, changedServerDocuments: 50, maxTransferredDocuments: 50,
          separateLocalAndCatchupTimes: true, backlogMustReachZero: true },
        7: { clockOffsetsMs: [-600000, 600000], noFalseClockError: true, distinctFieldMerge: true,
          sameFieldConflictBothValues: true, staleRevisionTypedUnapplied: true },
      };
      const receipt = { goal, revisions, hosts: [config.host], steps: [], measured: { phases: [], currentPhase: 'client-setup',
        nativeReadbackSource: 'mode=ro business-os-rxdb.sqlite3::ctox_business_os__desktop_icons__v0; no legacy projection fallback' }, criterion: criteria[goal], pass: false,
        startedAt: new Date().toISOString(),
        artifacts: [], clientType: 'Installed canonical DB+sync modules in real Chromium; not a Shell UI acceptance',
        transport: 'webrtc', customerWrites: false };
      receipts.push(receipt);
      let A, B, a, b, invitationA, invitationB;
      const receiptPath = join(output, `${String(goal).padStart(2, '0')}-installed-sync.json`);
      const saveReceipt = () => writeFileSync(receiptPath, JSON.stringify(receipt, null, 2) + '\n', { mode: 0o600 });
      const snapshot = async expected => {
        const captureStarted = performance.now();
        const ids = expected.map(value => value.id);
        const readBrowser = async page => {
          if (!page || page.isClosed()) return null;
          let timer;
          try { return await Promise.race([docs(page, ids), new Promise((_, reject) => {
            timer = setTimeout(() => reject(new Error('Snapshot deadline')), 4000);
          })]); } catch { return null; } finally { clearTimeout(timer); }
        };
        const [server, clientA, clientB] = await Promise.all([
          native.read(ids).catch(() => null), readBrowser(A), readBrowser(B)]);
        let writesInCurrentA = null;
        if (A && !A.isClosed()) {
          let timer;
          try { writesInCurrentA = await Promise.race([A.evaluate(() => globalThis.__installedAcceptance?.writesCompleted ?? null),
            new Promise(resolve => { timer = setTimeout(() => resolve(null), 4000); })]); }
          catch {} finally { clearTimeout(timer); }
        }
        let health = null;
        if (!stopping) try {
          const value = await native.cli(['business-os', 'rxdb', 'status', '--json', '--root', config.root], false, 10000);
          health = { running: value.running, replicationUp: value.replicationUp,
            stages: diagnosticScalars(value.health_stages), heartbeatFresh: value.heartbeat?.fresh };
        } catch {}
        return { at: new Date().toISOString(), captureElapsedMs: performance.now() - captureStarted,
          writesInCurrentASession: writesInCurrentA,
          ...documentAudit(expected, { A: clientA, native: server, B: clientB }), nativeHealth: health,
          logs: { native: native.events.slice(-24), A: A?.installedLogEvents?.slice(-24) || [],
            B: B?.installedLogEvents?.slice(-24) || [],
            scope: 'Timestamped vocabulary-only excerpts; raw native rolling files are host-private, not exported' } };
      };
      const phase = async (name, expected, action) => {
        const row = receipt.measured.phases.find(value => value.phase === name && value.status === 'not_run')
          || { phase: name, status: 'not_run' };
        if (!receipt.measured.phases.includes(row)) receipt.measured.phases.push(row);
        row.status = 'running'; row.startedAt = new Date().toISOString();
        row.writesAcknowledgedByA = /baseline|A-offline-30s/.test(name) ? null : 0;
        receipt.measured.currentPhase = name; saveReceipt();
        const before = performance.now();
        try { const result = await action(row); row.status = 'completed'; return result; }
        catch (error) { row.status = 'failed'; throw error; }
        finally { row.finishedAt = new Date().toISOString(); row.elapsedMs = performance.now() - before;
          row.snapshot = await snapshot(expected); saveReceipt(); }
      };
      try {
        a = await browser.newContext(); b = await browser.newContext(); contexts.push(a, b);
        a.setDefaultTimeout(30000); b.setDefaultTimeout(30000);
        const name = `ctox-installed-acceptance-${goal}-${randomUUID()}`;
        invitationA = await native.invite('A'); invitationB = await native.invite('B');
        A = await attach(a, origin, invitationA, name + '-a');
        B = await attach(b, origin, invitationB, name + '-b');
        const probe = { id: `acceptance-${name}-connected`, target_type: 'acceptance',
          label: 'live-WebRTC-baseline', updated_at_ms: Date.now() };
        if (goal === 5) receipt.measured.phases = [{ phase: 'baseline', status: 'not_run' },
          ...Array.from({ length: 3 }, (_, round) => ['A-offline-30s', 'peer-kill-respawn', 'B-reload', 'A-relogin', 'catchup']
            .map(label => ({ phase: `round${round}-${label}`, status: 'not_run' }))).flat()];
        await phase('baseline', [probe], async row => {
          const timings = await write(A, [probe]); row.writesAcknowledgedByA = timings.length;
          row.maxLocalWriteMs = Math.max(...timings); await converge(native, B, [probe]);
        });
        receipt.steps.push('Live baseline: A write persisted natively and reached B over WebRTC before any fault');
        const health = await native.cli(['business-os', 'rxdb', 'status', '--json', '--root', config.root]);
        receipt.measured.nativeHealth = { running: health.running, replicationUp: health.replicationUp,
          stages: health.health_stages, heartbeatFresh: health.heartbeat?.fresh };
        if (goal === 5) {
          receipt.criterion = { runs: 3, documentsPerRun: 200, offlineMs: 30000, maxLocalWriteMs: 200, maxCatchupMs: 10000 };
          receipt.measured.runs = [];
          for (let round = 0; round < 3; round++) {
            const values = Array.from({ length: 200 }, (_, i) => ({ id: `acceptance-${name}-${round}-${i}`,
              target_type: 'acceptance', label: `round${round}-document${i}`, x: i, y: round, updated_at_ms: Date.now() }));
            let offlineStart, timings;
            await phase(`round${round}-A-offline-30s`, values, async row => {
              await offline(a, A, true); offlineStart = performance.now();
              timings = await write(A, values); row.writesAcknowledgedByA = timings.length;
              row.maxLocalWriteMs = Math.max(...timings);
              await sleep(Math.max(0, 30000 - (performance.now() - offlineStart)));
            });
            await phase(`round${round}-peer-kill-respawn`, values, async row => {
              row.beforePid = native.peer.pid; await native.restartPeer(); row.afterPid = native.peer.pid;
            });
            await phase(`round${round}-B-reload`, values, async () => {
              await B.evaluate(async () => {
                await globalThis.__installedAcceptance.sync.stop(); await globalThis.__installedAcceptance.db.close();
              });
              B = await attach(b, origin, await native.invite('B'), name + '-b', 0, B);
            });
            const offlineServer = await native.read(values.map(d => d.id));
            invariant(Object.keys(offlineServer).length === 0, 'Client offline fault leaked WebRTC writes to native');
            const offlineMs = performance.now() - offlineStart;
            const reconnectStart = performance.now();
            await phase(`round${round}-A-relogin`, values, async () => {
              await offline(a, A, false);
              // Renew native-issued login and reopen the SAME IndexedDB; no cache wipe.
              await closePage(A); A = await attach(a, origin, await native.invite('A-relogin'), name + '-a');
            });
            let catchupMs;
            await phase(`round${round}-catchup`, values, async () => {
              await converge(native, B, values); catchupMs = performance.now() - reconnectStart;
            });
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
          const reopenStart = performance.now();
          B = await attach(b, origin, invitationB, name + '-b', 0, null, values.map(d => d.id));
          receipt.measured.localCacheReadyMs = B.installedCacheReadback.localOpenAndReadMs;
          receipt.measured.localCachedDocumentsBeforeSync = B.installedCacheReadback.cachedDocuments;
          receipt.measured.localReadMs = B.installedCacheReadback.localReadMs;
          await converge(native, B, changed);
          receipt.measured.catchupMs = performance.now() - reopenStart;
          receipt.measured.localUsabilityScope = 'Installed cache open and10000-document local read before sync; actual Shell UI usability remains unmeasured';
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
          await offline(a, A, true); await offline(b, B, true);
          await write(A, [{ ...base, x: 1 }]); await write(B, [{ ...base, y: 2 }]);
          await offline(a, A, false); await offline(b, B, false);
          await converge(native, B, [{ id, x: 1, y: 2 }]);
          receipt.measured.distinctFieldMerge = true;
          await offline(a, A, true); await offline(b, B, true);
          const beforeA = (await docs(A, [id]))[id], beforeB = (await docs(B, [id]))[id];
          await write(A, [{ ...beforeA, label: 'same-field-A' }]); await write(B, [{ ...beforeB, label: 'same-field-B' }]);
          await offline(a, A, false); await offline(b, B, false); await sleep(5000);
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
              return text.includes(id) && text.includes('same-field-A') && text.includes('same-field-B');
            });
          receipt.measured.sameFieldBothValuesVisible = visibleBothValues;
          receipt.pass = receipt.measured.clock.every(row => row.converged &&
            !JSON.stringify(row.metrics.conflicts).includes('clock_skew_detected')) &&
            receipt.measured.distinctFieldMerge && visibleBothValues &&
            receipt.measured.staleRevision.unapplied &&
            staleResponse.conflictArray && staleResponse.conflictingId;
        }
        receipt.steps.push('Installed runtime opened, real native peer and two separate Chromium contexts used');
      } catch (error) {
        receipt.interrupted = stopping;
        receipt.failedAt = new Date().toISOString();
        if (error.convergence) receipt.measured.convergence = error.convergence;
        receipt.measured.failureClients = { A: await failureMetrics(A), B: await failureMetrics(B) };
        receipt.failure = { name: error.name,
          errorSha256: createHash('sha256').update(String(error.message)).digest('hex'),
          message: error.name === 'InstalledCriterionError' ? error.message
            : 'Bounded installed acceptance failed; no credentials or raw native errors exported' };
        receipt.steps.push('First failing assertion retained; no repeated fault loop or inferred pass');
      } finally {
        receipt.finishedAt = new Date().toISOString();
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
