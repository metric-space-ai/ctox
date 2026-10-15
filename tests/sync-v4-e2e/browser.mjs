// Real browser runtime adapter. No transport, store, query or readiness mock.
export const schema = { version: 0, primaryKey: 'id', type: 'object', additionalProperties: true,
  properties: { id: { type: 'string', maxLength: 180 }, fixtureOnly: { type: 'boolean' }, ordinal: { type: 'integer' },
    padding: { type: 'string' }, updated_at_ms: { type: 'number' } }, required: ['id','ordinal','padding','updated_at_ms'], indexes: [['ordinal']] };
export function installRelay({ turnPort }) {
  const Native = globalThis.RTCPeerConnection;
  globalThis.__e2ePeers = []; globalThis.__e2eWire = { sent: 0, received: 0 };
  const observe = channel => {
    if (channel.__e2eObserved) return; channel.__e2eObserved = true;
    const bytes = value => typeof value === 'string' ? new TextEncoder().encode(value).byteLength : value?.byteLength ?? value?.size ?? 0;
    const send = channel.send;
    channel.send = function(value) { const result = send.call(this, value); globalThis.__e2eWire.sent += bytes(value); return result; };
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
export async function warm({ schema, expected }) {
  const s = globalThis.CTOX_BUSINESS_OS_APP;
  if (!s?.db || !s.sync) throw Error('Real Shell DB/sync unavailable');
  await s.db.addCollections({ e2e_leads: { schema, syncProfile: 'eager' } });
  const lease = await s.sync.leaseCollection('e2e_leads', 'e2e-warm');
  globalThis.__e2eLease = lease;
  let bridge = await s.sync.startCollection('e2e_leads', { pin: false });
  if (bridge?.ready) bridge = await bridge.ready;
  globalThis.__e2eBridge = bridge;
  if (typeof bridge?.state?.awaitInitialReplication !== 'function') throw Error('Real initial-replication contract absent');
  await bridge.state.awaitInitialReplication();
  // The first query is a real app query; warm-up may legitimately go to native.
  const rows = await s.db.collections.e2e_leads.find({ selector: {}, limit: expected.length + 1 }).exec();
  const ids = rows.map(x => x.id);
  const missing = expected.filter(id => !ids.includes(id));
  if (missing.length) throw Error(`Warm-up incomplete: ${ids.length}/${expected.length}, missing ${missing.slice(0,4)}`);
  const local = await s.db.collections.e2e_leads.storageCollection.findDocumentsById(expected);
  const cached = Array.isArray(local) ? local.length : Object.keys(local).length;
  if (cached !== expected.length) throw Error(`Fixture is not held locally: ${cached}/${expected.length}`);
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
