const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
function loggedBusyObserver(child) {
  let count = 0;
  for (const stream of [child.stdout, child.stderr]) {
    let tail = '';
    stream.on('data', chunk => {
      const lines = (tail + chunk.toString()).split('\n'); tail = lines.pop().slice(-4096);
      for (const line of lines) if (/SQLITE_BUSY|SQLITE_LOCKED|database (?:table )?is locked|database is busy/i.test(line)) count++;
    });
  }
  return () => count;
}
async function browserSoak({ seconds }) {
  const trace = globalThis.__syncV3Trace; trace.recording = false; trace.resourceRecording = false;
  const state = globalThis.ctoxBusinessOsSmoke.state, name = 'sync_v3_scale_leads', collection = state.db.raw[name];
  const lease = await state.sync.leaseCollection(name, 'sync-v3-s0-lock-soak', { forceDirect: true });
  let writes = 0, cycles = 0;
  try {
    let bridge = await state.sync.startCollection(name, { pin: false, forceDirect: true, requireOwner: true });
    if (!bridge.state && bridge.ready) bridge = await bridge.ready;
    await bridge.state.awaitInitialReplication();
    const docs = await collection.find({ selector: {}, sort: [{ ordinal: 'asc' }], limit: 8 }).exec();
    if (docs.length !== 8) throw Error('Soak fixture rows incomplete');
    const peer = bridge.state.peer, deadline = performance.now() + seconds * 1000;
    await globalThis.__syncV3SoakPoint({ status: 'RUNNING', cycles, writes });
    while (performance.now() < deadline) {
      if (cycles % 6 === 0) {
        const marker = `s0-soak-${cycles}`, ids = new Set(docs.map(d => d.id)), accepted = new Set();
        const original = peer.request;
        let resolveAck, rejectAck, timer;
        const ack = new Promise((resolve, reject) => { resolveAck = resolve; rejectAck = reject;
          timer = setTimeout(() => reject(Error('Soak native ACK timeout')), 20000); }); ack.catch(() => {});
        peer.request = async function (...args) {
          const rows = args[1] === 'masterWrite' && args[4] === name ? args[2]?.[0] : null;
          const selected = Array.isArray(rows) && rows.some(r => r.newDocumentState?.write_marker === marker);
          const connection = selected ? peer.connections.get(args[0]) : null;
          try {
            const result = await original.apply(this, args);
            if (selected && Array.isArray(result) && result.length === 0) {
              if (!connection || peer.connections.get(args[0]) !== connection) throw Error('Soak ACK connection changed');
              for (const row of rows) if (row.newDocumentState?.write_marker === marker && ids.has(row.newDocumentState.id)) accepted.add(row.newDocumentState.id);
              if (accepted.size === ids.size) resolveAck();
            }
            return result;
          } catch (error) { if (selected) rejectAck(error); throw error; }
        };
        try {
          await Promise.all(docs.map(async d => {
            const current = (await collection.findOne(d.id).exec()).toJSON();
            await collection.upsert({ ...current, write_marker: marker, updated_at_ms: Date.now() });
          }));
          await ack;
          if (!await globalThis.__syncV3SoakReadback([...ids], marker)) throw Error('Soak accepted rows absent from SQLite');
          writes += ids.size;
        } finally { clearTimeout(timer); peer.request = original; }
      }
      await globalThis.__syncV3SoakPoint({ status: 'RUNNING', cycles: ++cycles, writes });
      await new Promise(resolve => setTimeout(resolve, Math.min(5000, Math.max(0, deadline - performance.now()))));
    }
    return await globalThis.__syncV3SoakPoint({ status: 'COMPLETED', cycles, writes });
  } catch (error) {
    await globalThis.__syncV3SoakPoint({ status: 'FAILED', cycles, writes, error: error.message }); throw error;
  } finally { await lease.release(); }
}
async function runSoak(page, sqlite, runtimeRoot, seconds, statusPath, busyCount) {
  if (![20, 86400].includes(seconds) || !statusPath) throw Error('Soak duration/status contract invalid');
  fs.mkdirSync(path.dirname(statusPath), { recursive: true });
  const started = Date.now(), probeTotals = { samples: 0, busyRetries: 0, denied: 0, maxAdmissionMs: 0 }, recent = [];
  await page.exposeFunction('__syncV3SoakReadback', (ids, marker) => {
    if (ids.length !== 8 || ids.some(id => !/^sync-v3-[a-z0-9_-]+$/.test(id)) || !/^s0-soak-\d+$/.test(marker)) throw Error('Soak readback scope invalid');
    return Number(sqlite(`SELECT COUNT(*) FROM ctox_business_os__sync_v3_scale_leads__v0 WHERE id IN (${ids.map(id => `'${id}'`).join(',')}) AND json_extract(data,'$.write_marker')='${marker}';`).trim()) === 8;
  });
  await page.exposeFunction('__syncV3SoakPoint', value => {
    const child = spawnSync('python3', [path.join(__dirname, 'sqlite-lock-probe.py'), '--database', path.join(runtimeRoot, 'runtime/business-os-rxdb.sqlite3')], { encoding: 'utf8', timeout: 3000 });
    if (child.status !== 0) throw Error('Bounded isolated SQLite admission probe failed');
    const probe = JSON.parse(child.stdout); probeTotals.samples++; probeTotals.busyRetries += probe.busyRetries;
    probeTotals.denied += !probe.acquired; probeTotals.maxAdmissionMs = Math.max(probeTotals.maxAdmissionMs, probe.elapsedMs);
    recent.push({ at: new Date().toISOString(), ...probe }); if (recent.length > 20) recent.shift();
    const result = { schema: 'ctox.sync_v3.sqlite_lock_soak.v1', ...value, startedAt: new Date(started).toISOString(),
      updatedAt: new Date().toISOString(), elapsedSeconds: (Date.now() - started) / 1000, requestedSeconds: seconds,
      sourceHead: process.env.BUILD_LANE_HEAD, nativeLogBusyEvents: busyCount(), probeTotals, recent,
      workload: '8 concurrent real browser upserts every ~30s; accepted native ACK + SQLite readback; admission probe every~5s. Isolated scale tenant; not THESEN workload certification.',
      limitation: 'Logged native lock errors + measured probe writer admission. Unlogged native busy-handler waits remain UNKNOWN. Probe immediately rolls back and changes no records.',
      measurementComplete: value.status === 'COMPLETED', pass: value.status === 'COMPLETED' ? true : null };
    if (value.status === 'FAILED') result.pass = false;
    fs.writeFileSync(statusPath + '.next', JSON.stringify(result, null, 2)); fs.renameSync(statusPath + '.next', statusPath);
    return result;
  });
  return page.evaluate(browserSoak, { seconds });
}
module.exports = { runSoak, loggedBusyObserver };
