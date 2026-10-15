import fs from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { Native, until, pause, owner, verifiedBuild } from './native.mjs';
import { schema, installRelay, warm, readAndRender, relayProof } from './browser.mjs';

const config = JSON.parse(fs.readFileSync(process.argv[2]));
const here = dirname(fileURLToPath(import.meta.url));
const scenarios = await Promise.all(fs.readdirSync(here).filter(name => /^scenario-.+\.mjs$/.test(name))
  .map(async name => (await import(pathToFileURL(join(here, name)))).default));
if (config.catalog) { scenarios.length = 0;
 for (const name of fs.readdirSync(config.scenarioRoot).filter(n => /^k[0-9]+$/.test(n))) scenarios.push((await import(pathToFileURL(join(config.scenarioRoot,name,'scenario.mjs')))).default);
}
const order = config.catalog ? ['K01','K07','K11','K15','K02','K24'] : ['K08', 'K02', 'K04-ATTACH'];
scenarios.sort((a,b) => (order.indexOf(a.id) < 0 ? 999 : order.indexOf(a.id)) - (order.indexOf(b.id) < 0 ? 999 : order.indexOf(b.id)));
for (const s of scenarios) if (!s.id || !Array.isArray(s.steps) || !s.criterion || typeof s.run !== 'function') throw Error('Invalid scenario contract');
if (config.scenario && !scenarios.some(s => s.id === config.scenario)) throw Error('Unknown scenario');
const report = { owner, startedAt: new Date().toISOString(), revision: config.revision,
  harnessSource: config.sourceManifest,
  binarySha256: config.binarySha256, profile: config.profile, netem: config.netem,
  clientCount: config.clients, synthetic: true, fullCatalogAcceptance: false,
  fixtureLifecycle: config.seedAfterStart ? 'schema → daemon ready → native population → browsers' : 'native population → daemon → browsers',
  measurementScope: config.profile === 'small' ? 'wiring profile, not large-volume acceptance' : 'requested incident slices under measured synthetic volume',
  scenarios: [], terminal: false, pass: false };
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

  native.start('signaling', process.execPath, [join(config.source, 'src/core/rxdb/tools/local_signaling_server.js'), String(config.signalingPort)],
    { ...process.env, SIGNALING_HOST: '127.0.0.1', SIGNALING_PORT: String(config.signalingPort) });
  native.start('turn', config.turnBin, ['-n','--lt-cred-mech','--realm=bench.test','--user=bench:bench','--no-cli','--no-tls','--no-dtls','--no-tcp-relay','--allow-loopback-peers','--relay-threads=2',
    `--userdb=${join(config.output,'turn.sqlite')}`,'--listening-ip=192.0.2.1','--relay-ip=192.0.2.1',`--listening-port=${config.turnPort}`,'--min-port=49152','--max-port=49251',
    `--pidfile=${join(config.output,'turn.pid')}`,`--log-file=${join(config.output,'turn-detail.log')}`,'--no-stdout-log'],
    { ...process.env, LD_LIBRARY_PATH: config.turnLib || process.env.LD_LIBRARY_PATH || '' });
  native.serve();
  const origin = `http://127.0.0.1:${config.port}`;
  report.phase = 'daemon-ready'; save();
  await until(async () => {
    const r = await fetch(origin + '/index.html?rxdbSmoke=1', { signal: AbortSignal.timeout(2000) });
    if (!r.ok) return false;
    const html = await r.text(), marker = 'window.CTOX_BUSINESS_OS_CONFIG=', start = html.indexOf(marker);
    if (start < 0) return false;
    const end = html.indexOf(';window.CTOX_BUSINESS_OS_DESIGN_TEMPLATES=', start);
    if (end < 0) return false;
    const config = JSON.parse(html.slice(start + marker.length, end));
    return config.native_rxdb_peer_available === true && !!config.native_rxdb_peer_status?.peer_session_id;
  }, 60000, 'isolated daemon boot');
  if (config.seedAfterStart) { report.phase = 'populate-running-daemon'; save(); report.fixture = native.seed('seed',here); save(); }
  const require = createRequire(join(config.modulesRoot, 'playwright/package.json'));
  const { chromium } = require('playwright');
  const { tmpdir } = await import('node:os');
  shortTmp = fs.mkdtempSync(join(tmpdir(), 'e2e-'));
  report.phase = 'chromium'; save();
  browser = await chromium.launch({ executablePath: config.chrome, headless: true, timeout: 30000,
    env: { ...process.env, TMPDIR: shortTmp }, args: ['--disable-gpu'] });
  report.browser = browser.version();
  const setupPage = async (ctx, created = () => {}) => {
    const page = await ctx.newPage(); page.setDefaultTimeout(60000);
    created(page);
    page.on('console', m => { if (['warning','error'].includes(m.type())) {
      report.browserDiagnostics ||= []; report.browserDiagnostics.push({ at: new Date().toISOString(), kind: m.type(),
        // Only synthetic origin and public status vocabulary, no auth/URLs/documents.
        classes: m.text().match(/timeout|failed|not.registered|query.ready|readiness|denied|WebRTC|signaling|error/gi) || [], length: m.text().length });
      if (report.browserDiagnostics.length > 200) report.browserDiagnostics.shift();
    } });
    await page.goto(`${origin}/index.html?rxdbSmoke=1`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => !!globalThis.CTOX_BUSINESS_OS_APP?.db && !!globalThis.CTOX_BUSINESS_OS_APP?.sync
      && globalThis.CTOX_BUSINESS_OS_APP?.initialModuleOpened === true, null, { timeout: 90000 });
    return page;
  };
  if(config.prime) {
    report.phase='explicit-cold-precondition';save();
    const primer=await browser.newContext();contexts.push(primer);await primer.addInitScript(installRelay,{turnPort:config.turnPort});
    const at=Date.now();
    try {const p=await setupPage(primer);report.primer={warm:await bounded(p.evaluate(warm,{schema,expected:native.seed('read',here).rows.filter(r=>!r.deleted).map(r=>r.id),leadProfile:config.leadProfile}),180000,'Primer deadline'),elapsedMs:Date.now()-at};}
    catch(e){report.primer={error:e.message,elapsedMs:Date.now()-at,notAnAcceptanceResult:true};}
    finally{await primer.close();save();}
  }
  for (const scenario of scenarios.filter(s => !config.scenario || s.id === config.scenario)) {
    if (interrupted) throw Error('Run interrupted');
    const rowStarted = performance.now();
    const expected = native.seed('read', here).rows.filter(x => !x.deleted).map(x => x.id);
    const row = { id: scenario.id, title: scenario.title, coverage: scenario.coverage, steps: scenario.steps, criterion: scenario.criterion, startedAt: new Date().toISOString(), status: 'setup', pass: false };
    report.scenarios.push(row); report.phase = scenario.id; save();
    const context = await browser.newContext(); contexts.push(context);
    const scenarioContexts = [context];
    await context.addInitScript(installRelay, { turnPort: config.turnPort });
    const pages = [];
    const h = {
      page: async () => setupPage(context, page => pages.push(page)),
      warm: async page => {
        row.phase = 'initial-replication'; save();
        const shape = (rttMs, phase) => {
          if (fs.readlinkSync('/proc/self/ns/net') !== config.netem.namespace || config.netem.namespace === config.hostNetworkNamespace) throw Error('Network change outside owned namespace');
          execFileSync('sudo',['-n','tc','qdisc','change','dev','lo','parent','1:3','handle','30:','netem','delay',String(rttMs/2)+'ms','loss',String(config.lossPercent)+'%','limit','10000']);
          report.netemStages ||= []; report.netemStages.push({at:new Date().toISOString(),scenario:scenario.id,phase,rttMs,
            qdisc:execFileSync('/usr/sbin/tc',['-s','qdisc','show','dev','lo'],{encoding:'utf8'})}); save();
        };
        if (config.warmRttMs !== config.rttMs) shape(config.warmRttMs, 'precondition-only');
        row.warm = await bounded(page.evaluate(warm, { schema, expected, leadProfile: config.leadProfile }), 180000, "Initial fixture replication exceeded 180 seconds");
        row.warmRttMs = config.warmRttMs;
        if (config.warmRttMs !== config.rttMs) shape(config.rttMs, 'measurement');
        row.relay = await page.evaluate(relayProof);
        if (!row.relay.forced) throw Error('Selected candidate pair did not prove forced TURN');
        // Extra genuine clients share the same instance but independent IDB identities.
        for (let i = 1; i < config.clients; i++) {
          const extra = await browser.newContext(); contexts.push(extra);
          scenarioContexts.push(extra);
          await extra.addInitScript(installRelay, { turnPort: config.turnPort });
          const p = await setupPage(extra); await p.evaluate(warm, { schema, expected, leadProfile: config.leadProfile });
          const proof = await p.evaluate(relayProof); if (!proof.forced) throw Error('Additional client bypassed TURN');
          row.additionalClients ||= []; row.additionalClients.push(proof);
        }
        row.status = 'running'; save();
      },
      reloadAndRead: async page => {
        row.phase = 'reload-to-visible'; save();
        const before = performance.now();
        await page.reload({ waitUntil: 'domcontentloaded' });
        await page.waitForFunction(() => !!globalThis.CTOX_BUSINESS_OS_APP?.db, null, { timeout: 60000 });
        await page.evaluate(async schema => { await globalThis.CTOX_BUSINESS_OS_APP.db.addCollections({ e2e_leads: { schema, syncProfile: 'eager' } }); }, schema);
        const result = await page.evaluate(readAndRender, { expected });
        return { ...result, visibleMs: performance.now() - before, navigationIncluded: true };
      },
      releaseWarmLease: page => page.evaluate(async () => { await globalThis.__e2eLease?.release(); await globalThis.CTOX_BUSINESS_OS_APP.sync.stopCollection("e2e_leads"); }),
      openApp: async page => {
        row.phase = 'first-module-window'; save();
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
      checkpointPull: page => bounded(page.evaluate(async () => {
        const state = globalThis.__e2eBridge?.state;
        const peer = state?.openPeerIds?.()[0];
        if (!peer || typeof state.pullFromPeer !== 'function') throw Error('Actual checkpointed native pull unavailable');
        const original = state.requestMasterChangesSince, replies = [];
        state.requestMasterChangesSince = async function(...args) {
          const response = await original.apply(this,args);
          const documents = response.result?.documents;
          if (!Array.isArray(documents)) throw Error('Native checkpoint pull returned an error or malformed result');
          replies.push({ rows:documents.length, probe:documents.filter(d => d.id === 'e2e-leads-behind').map(d => ({id:d.id,phase:d.probe_phase,updatedAtMs:d.updated_at_ms})) });
          return response;
        };
        try { await state.pullFromPeer(peer); } finally { state.requestMasterChangesSince = original; }
        if (!replies.length) throw Error('No native checkpoint reply observed');
        return { replies, ordinaryProductionPull: true };
      }),30000,'Checkpoint pull deadline'),
      inject: async operation => { row.phase = 'ATTACH-' + operation; save(); return native.seed(operation, here); },
      truth: async () => native.seed('read', here),
      awaitLocal: async (page, id, timeout, phase) => {
        const start = performance.now(); let found = null;
        while (performance.now() - start < timeout) {
          found = await page.evaluate(async id => {
            const rows = await globalThis.CTOX_BUSINESS_OS_APP.db.collections.e2e_leads.storageCollection.findDocumentsById([id]);
            return Array.isArray(rows) ? rows[0] || null : rows[id] || null;
          }, id);
          if (found && (!phase || found.probe_phase === phase)) break;
          await pause(100);
        }
        return { found: !!found && (!phase || found.probe_phase === phase), elapsedMs: performance.now() - start,
          revision: found?._rev ?? found?.revision ?? null, lwt: found?._meta?.lwt ?? null, phase: found?.probe_phase ?? null };
      },
      config, origin, pause,
      observe: (phase,measured) => {
        row.phase=phase;row.observations||=[];row.observations.push({at:new Date().toISOString(),phase,...measured});
        if(row.observations.length>400)row.observations.splice(1,row.observations.length-400);
        row.measured||={};row.measured.lastProgress=measured;save();
      },
      os: page => page.evaluate(async path=>{
        if(!path)return {available:false,reason:'--os-bootstrap actual native transport adapter required'};
        if(!path.startsWith('/')||path.startsWith('//')||path.includes('?'))throw Error('OS bootstrap must be an exact same-origin static module');
        const module=await import(path),os=await module.connectScenario();
        if(os.transportKind!=='native-webrtc'||!os.nativePeerId||!os.kernel?.shellApi||!os.openApp)throw Error('Native WebRTC OS adapter proof/contract missing');
        globalThis.__e2eOS={...os,shell:os.kernel.shellApi()};
        return {available:true,nativePeerId:os.nativePeerId,transportKind:os.transportKind};
      },config.osBootstrap),
      nativeCli: args => native.cli(args),
      queueList: () => {const at=Date.now();try {const value=JSON.parse(native.cli(['queue','list']));return {elapsedMs:Date.now()-at,ok:true,rows:Array.isArray(value)?value.length:null};}catch(e){return {elapsedMs:Date.now()-at,ok:false,error:e.message};}},
      quota: async (page,bytes) => {const c=await page.context().newCDPSession(page);try{await c.send('Storage.overrideQuotaForOrigin',{origin,quotaSize:bytes});}finally{await c.detach();}},
      details: page => page.evaluate(async ()=>{
        const s=globalThis.CTOX_BUSINESS_OS_APP,c=s.db.collections.e2e_leads;
        const rows=await c.find({selector:{},limit:300}).exec();
        const times=[];
        for(const d of rows){const at=performance.now();const r=await c.findOne(d.id).exec();if(!r)throw Error('Lead detail missing');times.push(performance.now()-at);}
        return {opened:times.length,durationsMs:times};
      }),
      commandTruth: () => native.seed('commands',here),
      pressure: async (seconds,writers=8,intervalMs=25) => {
        const child=native.start('external-pressure-'+Date.now(),'python3',[join(here,'seed.py'),'--root',config.root,'--profile',config.profile,'--operation','pressure','--seconds',String(seconds),'--writers',String(writers),'--interval-ms',String(intervalMs)]);
        await pause(250);if(child.exitCode!==null||child.signalCode!==null)throw Error('External committer failed before measurement');return child;
      },
      pressureResult: child => {
        const record=native.receipts.find(r=>r.pid===child.pid),file=join(config.output,record.kind+'.log');
        const lines=fs.readFileSync(file,'utf8').trim().split('\n');
        let stats;try{stats=JSON.parse(lines.at(-1));}catch{stats=null;}
        return {completed:child.exitCode===0,stats,evidence:file};
      },
      nativeLog: () => fs.readFileSync(join(config.output,'daemon.log'),'utf8'),
      bodies: () => native.seed('bodies', here).rows,
      serverNewer: () => native.seed('server-newer', here),
      newClient: async () => {
        const ctx = await browser.newContext(); contexts.push(ctx); scenarioContexts.push(ctx);
        await ctx.addInitScript(installRelay,{turnPort:config.turnPort});
        return setupPage(ctx,p => pages.push(p));
      },
      wire: page => page.evaluate(()=>({...globalThis.__e2eWire})),
      browserRows: page => page.evaluate(async () => {
        const rows = await globalThis.CTOX_BUSINESS_OS_APP.db.collections.e2e_leads.find({selector:{},limit:100000}).exec();
        return rows.map(d=>d.toJSON?.() || d);
      }),
      browserPut: (page,docs) => bounded(page.evaluate(async docs => {
        const c=globalThis.CTOX_BUSINESS_OS_APP.db.collections.e2e_leads;
        for(const d of docs){const {_rev,_meta,_attachments,_deleted,revision,lastWriteTime,...body}=d;await c.upsert({...body,updated_at_ms:Date.now()});}
        return {local:docs.length,at:Date.now()};
      },docs),30000,'Browser local write deadline'),
      readCommand: async page => bounded(page.evaluate(async () => {
        const at=performance.now(),wall=Date.now(), bus=globalThis.CTOX_BUSINESS_OS_APP.commandBus;
        if (!bus?.dispatch) throw Error('Real Shell command bus unavailable');
        try { const r=await bus.dispatch({command_type:'ctox.workjet.project.list',payload:{},client_context:{command_timing_probe:true}},{until:'accepted',timeoutMs:5000});
          const timing=r?.result?.command_timing || r?.command_timing || r?.result?.result?.command_timing || null;
          return {elapsedMs:performance.now()-at,startedAtMs:wall,nativeTiming:timing,coreDispatchMs:timing?timing.native_dispatch_entered-wall:null,status:r?.status,commandId:r?.command_id || r?.id,error:['failed','rejected'].includes(r?.status)?r.status:null};
        } catch(e) {return {elapsedMs:performance.now()-at,error:e.code || e.message};}
      }),12000,'Read command observation deadline'),
      maintenance: page => page.evaluate(() => {
        const s=globalThis.CTOX_BUSINESS_OS_APP;
        return {active:s.maintenance?.active ?? null,phase:s.maintenance?.phase ?? null};
      }),
      freeze: (page,value) => page.context().newCDPSession(page).then(async c=>{try{await c.send('Page.setWebLifecycleState',{state:value?'frozen':'active'});}finally{await c.detach();}}),
      offline: async (page, value) => {
        const cdp = await page.context().newCDPSession(page);
        await page.context().setOffline(value);
        await cdp.send('Network.emulateNetworkConditions', { offline: value, latency: 0, downloadThroughput: -1, uploadThroughput: -1, packetLoss: value ? 100 : 0 });
        await cdp.detach();
      },
      restart: () => native.restart(),
      schemaChange: async cliArgs => {
        // Scenario supplies the revision's actual migration CLI, never a SQL
        // schema rewrite or fabricated generation. Unsupported CLI stays red.
        if (!Array.isArray(cliArgs) || !['business-os','sync'].includes(cliArgs[0]) || cliArgs.some(x => typeof x !== 'string')
          || cliArgs.includes('--root')) throw Error('Schema transition requires the real isolated native CLI');
        const result = native.cli([...cliArgs, '--root', config.root]);
        report.schemaTransitions ||= [];
        report.schemaTransitions.push({ at: new Date().toISOString(), cli: cliArgs.slice(0,3), stateRootPreserved: true,
          responseBytes: Buffer.byteLength(result), scenarioMustVerifyGenerationAndRows: true }); save();
        return result;
      },
      changeRelease: async receiptPath => {
        const build = verifiedBuild(receiptPath);
        await native.stop(native.daemon);
        for (const part of ['src', 'contracts', 'Cargo.toml']) {
          const target = join(config.root, part);
          if (!fs.lstatSync(target).isSymbolicLink()) throw Error('Only owned source links may change');
          fs.unlinkSync(target); fs.symlinkSync(join(build.source, part), target);
        }
        native.config.binary = build.binary; native.config.source = build.source;
        fs.unlinkSync(join(config.root, 'bin/ctox')); fs.symlinkSync(build.binary,join(config.root,'bin/ctox'));
        native.serve();
        report.releaseTransitions ||= [];
        report.releaseTransitions.push({ at: new Date().toISOString(), revision: build.revision, binarySha256: build.binarySha256,
          stateRootPreserved: true, scenarioMustVerifySchemaAndRows: true });
        save();
      },
    };
    try {
      const result = await scenario.run(h); Object.assign(row, result);
      row.status = result.status || 'measured';
      if (scenario.id === 'K04-ATTACH' && (!row.measured.positiveControl.found || !row.measured.positiveControl.nativeConfirmed)) { row.pass = false; row.status = 'inconclusive'; row.reason = 'Forward-write positive control failed; cannot attribute missing row to checkpoint'; }
    } catch (e) {
      row.error = e.message; row.status = row.status === 'running' ? 'failed' : 'not_run';
      row.failureSnapshot = { at: new Date().toISOString(), phase: row.phase || 'browser-connect', expected: expected.length };
      try { row.failureSnapshot.native = native.seed('read', here); } catch (error) { row.failureSnapshot.nativeUnavailable = error.message; }
      if (pages[0] && !pages[0].isClosed()) try {
        row.failureSnapshot.browser = await bounded(pages[0].evaluate(async expected => {
          const c = globalThis.CTOX_BUSINESS_OS_APP?.db?.collections?.e2e_leads;
          if (!c) return { available: false, reason: 'collection-not-open' };
          const data = await c.storageCollection.findDocumentsById(expected);
          const rows = Array.isArray(data) ? data : Object.values(data);
          const ids = new Set(rows.map(r => r.id));
          return { available: true, found: rows.length, missing: expected.filter(id => !ids.has(id)),
            rows: rows.map(r => ({ id:r.id, revision:r._rev, lwt:r._meta?.lwt })), wire: globalThis.__e2eWire,
            peers: (globalThis.__e2ePeers || []).map(p => ({ connectionState:p.connectionState,iceConnectionState:p.iceConnectionState,signalingState:p.signalingState })),
            sockets: (globalThis.__e2eSockets || []).map(s => ({...s.observation,readyState:s.socket.readyState})) };
        }, expected), 3000, 'failure snapshot deadline');
      } catch(error) { row.failureSnapshot.browserUnavailable = error.message; }
    }
    finally { row.finishedAt = new Date().toISOString();
      row.elapsedMs = performance.now() - rowStarted;
      for (let i = 0; i < pages.length; i++) await pages[i].screenshot({ path: join(config.output,`${row.id}-${i}.png`) }).catch(() => {});
      for (const ctx of scenarioContexts) await ctx.close(); save(); }
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
