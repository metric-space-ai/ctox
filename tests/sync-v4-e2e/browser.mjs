// Real browser runtime adapter. No transport, store, query or readiness mock.
export const schema = { version: 0, primaryKey: 'id', type: 'object', additionalProperties: true,
  properties: { id: { type: 'string', maxLength: 180 }, fixtureOnly: { type: 'boolean' }, ordinal: { type: 'integer' },
    padding: { type: 'string' }, probe_phase: { type: 'string' }, updated_at_ms: { type: 'number' } }, required: ['id','ordinal','padding','updated_at_ms'], indexes: [['ordinal']] };
export function installRelay({ turnPort }) {
  const Socket = globalThis.WebSocket;
  globalThis.__e2eSockets = [];
  globalThis.WebSocket = class extends Socket {
    constructor(...args) {
      super(...args);
      const url = new URL(args[0]);
      const observation = { local: ['127.0.0.1','localhost','192.0.2.1'].includes(url.hostname), port: url.port, opened: false, closeCode: null };
      globalThis.__e2eSockets.push({ socket: this, observation });
      this.addEventListener('open', () => { observation.opened = true; });
      this.addEventListener('close', e => { observation.closeCode = e.code; });
    }
  };
  const Native = globalThis.RTCPeerConnection;
  globalThis.__e2ePeers = []; globalThis.__e2eWire = { sent: 0, received: 0, queryFetch: 0, binarySent: 0 };
  const observe = channel => {
    if (channel.__e2eObserved) return; channel.__e2eObserved = true;
    const bytes = value => typeof value === 'string' ? new TextEncoder().encode(value).byteLength : value?.byteLength ?? value?.size ?? 0;
    const send = channel.send;
    channel.send = function(value) { const result = send.call(this, value); globalThis.__e2eWire.sent += bytes(value);
      if(typeof value==='string'){if(value.includes('rxdb.query.fetch')||value.includes('rxdb.rows.fetch'))globalThis.__e2eWire.queryFetch++;}
      else globalThis.__e2eWire.binarySent++;return result; };
    channel.addEventListener('message', e => { globalThis.__e2eWire.received += bytes(e.data); });
  };
  class ForcedRelay extends Native {
    constructor(config = {}, ...rest) {
      super({ ...config, iceTransportPolicy: 'relay', iceServers: [{ urls: `turn:192.0.2.1:${turnPort}?transport=udp`, username: 'bench', credential: 'bench' }] }, ...rest);
      globalThis.__e2ePeers.push(this); this.addEventListener('datachannel', e => observe(e.channel));
    }
    createDataChannel(...args) { const c = super.createDataChannel(...args); observe(c); return c; }
  }
  globalThis.RTCPeerConnection = ForcedRelay;
}
export async function warm({ schema, expected, leadProfile = 'eager' }) {
  const s = globalThis.CTOX_BUSINESS_OS_APP;
  if (!s?.db || !s.sync) throw Error('Real Shell DB/sync unavailable');
  await s.db.addCollections({ e2e_leads: { schema, syncProfile: leadProfile } });
  const lease = await s.sync.leaseCollection('e2e_leads', 'e2e-warm', { forceDirect: true });
  globalThis.__e2eLease = lease;
  let bridge = await s.sync.startCollection('e2e_leads', { pin: false, forceDirect: true, requireOwner: true });
  if (bridge?.ready) bridge = await bridge.ready;
  globalThis.__e2eBridge = bridge;
  if (typeof bridge?.state?.awaitInitialReplication !== 'function') throw Error('Real initial-replication contract absent: ' + JSON.stringify({mode:bridge?.mode,reason:bridge?.reason,hasState:!!bridge?.state}));
  await bridge.state.awaitInitialReplication();
  if (leadProfile === 'demand-only') await s.db.collections.e2e_leads.find({selector:{},limit:expected.length+1}).exec();
  // Precondition is actual local storage, not a successful app query. Otherwise
  // a query row-cap would become a setup failure instead of a measured red list.
  const local = await s.db.collections.e2e_leads.storageCollection.findDocumentsById(expected);
  const rows = Array.isArray(local) ? local : Object.values(local);
  const cached = rows.length;
  const ids = rows.map(x => x.id);
  const missing = expected.filter(id => !ids.includes(id));
  if (cached !== expected.length) throw Error(`Fixture is not held locally: ${cached}/${expected.length}`);
  if (missing.length) throw Error('Local fixture IDs differ from native truth');
  return { rows: ids.length, cached, wire: { ...globalThis.__e2eWire } };
}
export async function readAndRender({ expected }) {
  const start = performance.now(), s = globalThis.CTOX_BUSINESS_OS_APP;
  if (!s?.db?.collections?.e2e_leads) throw Error('Real app collection missing after reload');
  const cached = await s.db.collections.e2e_leads.storageCollection.findDocumentsById(expected);
  const preQueryLocalCount = Array.isArray(cached) ? cached.length : Object.keys(cached).length;
  const rows = await s.db.collections.e2e_leads.find({ selector: {}, limit: expected.length + 1 }).exec();
  const ids = rows.map(row => row.id);
  const panel = document.createElement('section'); panel.id = 'e2e-visible-leads';
  panel.style.cssText = 'position:fixed;inset:40px;z-index:2147483647;overflow:auto;background:white;color:black';
  panel.replaceChildren(...ids.map(id => { const item = document.createElement('div'); item.dataset.id = id; item.textContent = id; return item; }));
  document.body.append(panel);
  await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
  const rect = panel.getBoundingClientRect();
  if (rect.height <= 0 || getComputedStyle(panel).visibility === 'hidden') throw Error('Rows did not paint');
  return { queryToPaintMs: performance.now() - start, preQueryLocalCount, rows: ids.length, missing: expected.filter(id => !ids.includes(id)),
    duplicates: ids.length - new Set(ids).size, emptyRender: ids.length === 0, wire: { ...globalThis.__e2eWire } };
}
export async function relayProof() {
  const pairs = [];
  for (const pc of globalThis.__e2ePeers || []) {
    if (pc.connectionState !== 'connected') continue;
    const stats = await pc.getStats();
    for (const row of stats.values()) if (row.type === 'transport' && row.selectedCandidatePairId) {
      const pair = stats.get(row.selectedCandidatePairId), local = stats.get(pair.localCandidateId), remote = stats.get(pair.remoteCandidateId);
      pairs.push({ localType: local?.candidateType, remoteType: remote?.candidateType, rttMs: pair.currentRoundTripTime * 1000,
        localAddress: local?.address, remoteAddress: remote?.address });
    }
  }
  return { pairs, forced: pairs.length > 0 && pairs.every(p => p.localType === 'relay') };
}
