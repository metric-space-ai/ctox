import fs from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { performance } from 'node:perf_hooks';
import { Native, until, pause, owner } from './native.mjs';
import { schema, installRelay, warm, readAndRender, relayProof } from './browser.mjs';
import K02 from './scenario-K02.mjs';
import K08 from './scenario-K08.mjs';
import ATTACH from './scenario-K04-ATTACH.mjs';
const config = JSON.parse(fs.readFileSync(process.argv[2]));
const here = dirname(fileURLToPath(import.meta.url));
const report = { owner, startedAt: new Date().toISOString(), revision: config.revision,
  binarySha256: config.binarySha256, profile: config.profile, netem: config.netem,
  clientCount: config.clients, synthetic: true, scenarios: [], terminal: false, pass: false };
const save = () => fs.writeFileSync(join(config.output, 'result.json'), JSON.stringify(report, null, 2));
const native = new Native(config); let browser, shortTmp;
const contexts = [];
const bounded = async (promise, ms, label) => { let timer; try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error(label)), ms); })]); } finally { clearTimeout(timer); } };
const shutdown = async () => { for (const ctx of contexts) await ctx.close().catch(() => {}); await browser?.close().catch(() => {}); await native.close(); };
let interrupted = false;
process.once('SIGTERM', () => { interrupted = true; void shutdown(); });
const hardDeadline = setTimeout(() => { interrupted = true; void shutdown(); }, config.durationSeconds * 1000);
save();
try {
  report.phase = 'seed'; save(); report.fixture = await native.prepare(schema, here); save();
  const expected = native.seed('read', here).rows.filter(x => !x.deleted).map(x => x.id);
  native.start('signaling', process.execPath, [join(config.source, 'src/core/rxdb/tools/local_signaling_server.js'), String(config.signalingPort)],
    { ...process.env, SIGNALING_HOST: '127.0.0.1', SIGNALING_PORT: String(config.signalingPort) });
  native.start('turn', config.turnBin, ['-n','--lt-cred-mech','--realm=bench.test','--user=bench:bench','--no-cli','--no-tls','--no-dtls','--no-tcp-relay','--allow-loopback-peers','--relay-threads=2',
    `--userdb=${join(config.output,'turn.sqlite')}`,'--listening-ip=192.0.2.1','--relay-ip=192.0.2.1',`--listening-port=${config.turnPort}`,'--min-port=49152','--max-port=49251',
    `--pidfile=${join(config.output,'turn.pid')}`,`--log-file=${join(config.output,'turn-detail.log')}`,'--no-stdout-log'],
    { ...process.env, LD_LIBRARY_PATH: config.turnLib || process.env.LD_LIBRARY_PATH || '' });
  native.serve();
  const origin = `http://127.0.0.1:${config.port}`;
  report.phase = 'daemon-ready'; save();
  await until(async () => { const r = await fetch(`${origin}/index.html?rxdbSmoke=1`, { signal: AbortSignal.timeout(2000) }); return r.ok && (await r.text()).includes('native_rxdb_peer_available'); }, 60000, 'isolated daemon boot');
  const require = createRequire(join(config.modulesRoot, 'playwright/package.json'));
  const { chromium } = require('playwright');
  const { tmpdir } = await import('node:os');
  shortTmp = fs.mkdtempSync(join(tmpdir(), 'e2e-'));
  report.phase = 'chromium'; save();
  browser = await chromium.launch({ executablePath: config.chrome, headless: true, timeout: 30000,
    env: { ...process.env, TMPDIR: shortTmp }, args: ['--disable-gpu'] });
  report.browser = browser.version();
  const setupPage = async ctx => {
    const page = await ctx.newPage(); page.setDefaultTimeout(60000);
    page.on('console', m => { if (['warning','error'].includes(m.type())) {
      report.browserDiagnostics ||= []; report.browserDiagnostics.push({ at: new Date().toISOString(), kind: m.type(),
        // Only synthetic origin and public status vocabulary, no auth/URLs/documents.
        classes: m.text().match(/timeout|failed|not.registered|query.ready|readiness|denied|WebRTC|signaling|error/gi) || [], length: m.text().length });
    } });
    await page.goto(`${origin}/index.html?rxdbSmoke=1`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => !!globalThis.CTOX_BUSINESS_OS_APP?.db && !!globalThis.CTOX_BUSINESS_OS_APP?.sync, null, { timeout: 90000 });
    return page;
  };
  for (const scenario of [K02, K08, ATTACH].filter(s => !config.scenario || s.id === config.scenario)) {
    if (interrupted) throw Error('Run interrupted');
    const row = { id: scenario.id, title: scenario.title, steps: scenario.steps, criterion: scenario.criterion, startedAt: new Date().toISOString(), status: 'setup', pass: false };
    report.scenarios.push(row); report.phase = scenario.id; save();
    const context = await browser.newContext(); contexts.push(context);
    await context.addInitScript(installRelay, { turnPort: config.turnPort });
    const pages = [];
    const h = {
      page: async () => { const p = await setupPage(context); pages.push(p); return p; },
      warm: async page => {
        row.warm = await bounded(page.evaluate(warm, { schema, expected }), 180000, "Initial fixture replication exceeded 180 seconds");
        row.relay = await page.evaluate(relayProof);
        if (!row.relay.forced) throw Error('Selected candidate pair did not prove forced TURN');
        // Extra genuine clients share the same instance but independent IDB identities.
        for (let i = 1; i < config.clients; i++) {
          const extra = await browser.newContext(); contexts.push(extra);
          await extra.addInitScript(installRelay, { turnPort: config.turnPort });
          const p = await setupPage(extra); await p.evaluate(warm, { schema, expected });
          const proof = await p.evaluate(relayProof); if (!proof.forced) throw Error('Additional client bypassed TURN');
          row.additionalClients ||= []; row.additionalClients.push(proof);
        }
        row.status = 'running'; save();
      },
      reloadAndRead: async page => {
        const before = performance.now();
        await page.reload({ waitUntil: 'domcontentloaded' });
        await page.waitForFunction(() => !!globalThis.CTOX_BUSINESS_OS_APP?.db, null, { timeout: 60000 });
        await page.evaluate(async schema => { await globalThis.CTOX_BUSINESS_OS_APP.db.addCollections({ e2e_leads: { schema, syncProfile: 'eager' } }); }, schema);
        const result = await page.evaluate(readAndRender, { expected });
        return { ...result, visibleMs: performance.now() - before, navigationIncluded: true };
      },
      releaseWarmLease: page => page.evaluate(async () => { await globalThis.__e2eLease?.release(); await globalThis.CTOX_BUSINESS_OS_APP.sync.stopCollection("e2e_leads"); }),
      openApp: async page => {
        const before = performance.now();
        await page.evaluate(async () => { globalThis.__e2eAppResult = null; await globalThis.CTOX_BUSINESS_OS_APP.openModule('e2e-probe'); });
        await page.waitForFunction(() => !!globalThis.__e2eAppResult, null, { timeout: 60000 });
        const result = await page.evaluate(() => globalThis.__e2eAppResult);
        if (!result.visible) throw Error('Actual module did not paint');
        return { visibleMs: performance.now() - before, rows: result.ids.length, emptyRender: result.emptyRender,
          missing: expected.filter(id => !result.ids.includes(id)), source: 'real Shell openModule → module context query' };
      },
      checkpoint: page => page.evaluate(async () => {
        const storage = globalThis.CTOX_BUSINESS_OS_APP.db.collections.e2e_leads.storageCollection;
        if (!storage.replicationCheckpointStatus) throw Error('Replication checkpoint observation missing');
        return storage.replicationCheckpointStatus();
      }),
      inject: async operation => native.seed(operation, here),
      truth: async () => native.seed('read', here),
      awaitLocal: async (page, id, timeout, revision) => {
        const start = performance.now(); let found = null;
        while (performance.now() - start < timeout) {
          found = await page.evaluate(async id => {
            const rows = await globalThis.CTOX_BUSINESS_OS_APP.db.collections.e2e_leads.storageCollection.findDocumentsById([id]);
            return Array.isArray(rows) ? rows[0] || null : rows[id] || null;
          }, id);
          if (found && (!revision || found._rev === revision)) break;
          await pause(100);
        }
        return { found: !!found && (!revision || found._rev === revision), elapsedMs: performance.now() - start,
          revision: found?._rev ?? null, lwt: found?._meta?.lwt ?? null };
      },
      offline: async (page, value) => {
        const cdp = await context.newCDPSession(page);
        await context.setOffline(value);
        await cdp.send('Network.emulateNetworkConditions', { offline: value, latency: 0, downloadThroughput: -1, uploadThroughput: -1, packetLoss: value ? 100 : 0 });
        await cdp.detach();
      },
      restart: () => native.restart(),
      changeRelease: async build => { await native.stop(native.daemon); native.config.binary = build.binary; native.serve(); },
    };
    try {
      const result = await scenario.run(h); Object.assign(row, result);
      row.status = 'measured';
      if (scenario.id === 'K04-ATTACH' && !row.measured.positiveControl.found) { row.pass = false; row.status = 'inconclusive'; row.reason = 'Forward-write positive control failed; cannot attribute missing row to checkpoint'; }
    } catch (e) { row.error = e.message; row.status = row.status === 'running' ? 'failed' : 'not_run'; }
    finally { row.finishedAt = new Date().toISOString();
      for (let i = 0; i < pages.length; i++) await pages[i].screenshot({ path: join(config.output,`${row.id}-${i}.png`) }).catch(() => {});
      await context.close(); save(); }
  }
  report.pass = report.scenarios.length > 0 && report.scenarios.every(s => s.status === 'measured' && s.pass);
} catch (e) { report.error = e.message; process.exitCode = 1; }
finally {
  clearTimeout(hardDeadline);
  try { await shutdown(); report.cleanup = 'owned groups absent'; } catch (e) { report.cleanupError = e.message; process.exitCode = 1; }
  if (shortTmp) fs.rmSync(shortTmp, { recursive: true, force: true });
  report.terminal = true; report.finishedAt = new Date().toISOString(); save();
  fs.writeFileSync(join(config.output,'result.md'), `| ${config.revision.slice(0,12)} | ${config.profile} | ${report.scenarios.map(s => `${s.id}: ${s.status}/${s.pass ? 'grün' : 'rot'}`).join('; ')} | ${config.output}/result.json |\n`);
  if (!report.pass) process.exitCode = 1;
}
