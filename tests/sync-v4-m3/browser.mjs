// V4 hello adapter over the real browser transport; no application replay.
import { createCtoxWebRtcNativePeer } from '/runtime/webrtc-native.mjs';

// Adapted from v4/bench-transport 4e3262ca; only V4 handshake/readiness added.
export async function start(config) {

  const b64 = bytes => btoa(String.fromCharCode(...new Uint8Array(bytes))).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
  const unb64 = text => Uint8Array.from(atob(text.replaceAll('-', '+').replaceAll('_', '/')), x => x.charCodeAt(0));
  const fixtureKey = async seed => crypto.subtle.importKey('pkcs8',
    Uint8Array.from([0x30,0x2e,0x02,0x01,0x00,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x04,0x22,0x04,0x20,
      ...new Uint8Array(32).fill(seed)]), 'Ed25519', true, ['sign']);
  const slot = Number(config.slot.replace('browser', ''));
  const key = await fixtureKey(11 + slot), serverKey = await fixtureKey(42);
  const publicJwk = await crypto.subtle.exportKey('jwk', key);
  const serverJwk = await crypto.subtle.exportKey('jwk', serverKey);
  const verifyServerHello = async ({ hello }) => {
    if (hello.device_proof.public_key !== serverJwk.x || hello.device_proof.device_id !== 'native-fixture') return false;
    const publicKey = await crypto.subtle.importKey('raw', unb64(serverJwk.x), 'Ed25519', false, ['verify']);
    return crypto.subtle.verify('Ed25519', publicKey, unb64(hello.device_proof.signature),
      new TextEncoder().encode(hello.device_proof.nonce));
  };
  const createHello = async ({ challenge }) => ({
    type: 'hello', protocol_ver: 4, bootstrap_ver: 1,
    app_api_ver: { min: '4.0', max: '4.0' }, instance_id: 'm3-fixture',
    capabilities: ['bench'], scopes_wanted: [],
    device_proof: { device_id: config.slot, public_key: publicJwk.x, nonce: challenge,
      signature: b64(await crypto.subtle.sign('Ed25519', key, new TextEncoder().encode(challenge))) },
    transport: { codecs: ['gzip'], max_frame_bytes: 16384, fragmentation: true, reassembly: true },
  });

  const NativeRTC = window.RTCPeerConnection;
  window.RTCPeerConnection = class extends NativeRTC {
    constructor(options) {
      super({ ...options, iceTransportPolicy: 'relay' });
      this.addEventListener('datachannel', e => attach(e.channel));
      this.addEventListener('icecandidateerror', e => {
        if (errors.length < 10000) errors.push({ at: performance.now() - started, code: e.errorCode, message: e.errorText });
      });

      const create = this.createDataChannel;
      this.createDataChannel = (...args) => { const channel = create.apply(this, args); attach(channel); return channel; };
    }
  };
  const events = [], errors = [], samples = [], pending = new Map();
  let running = true, peer, generation = 0, sequence = 0, maxBufferedBytes = 0, maxFrameBytes = 0;
  const encoder = new TextEncoder(), started = performance.now();
  const counters = { producerSkipped: 0, wireSentBytes: 0, wireReceivedBytes: 0 };
  const attached = new WeakSet();
  function attach(channel) {
    if (attached.has(channel)) return; attached.add(channel);
    channel.addEventListener('message', e => { counters.wireReceivedBytes += typeof e.data === 'string' ? encoder.encode(e.data).length : e.data.byteLength; });
  }
  const record = row => { if (events.length < 200000) events.push({ at: performance.now() - started, ...row }); else counters.metadataOverflow = true; };
  const description = value => value ? { type: value.type,
    sdp: value.sdp.replace(/a=ice-pwd:[^\r\n]*/g, 'a=ice-pwd:[redacted]') } : null;

  const send = RTCDataChannel.prototype.send;
  RTCDataChannel.prototype.send = function (data) {
    const result = send.call(this, data);
    const bytes = typeof data === 'string' ? encoder.encode(data).length : data.byteLength;
    counters.wireSentBytes += bytes; maxFrameBytes = Math.max(maxFrameBytes, bytes); return result;
  };
  const payloadBytes = new Uint8Array(5_000_000);
  let seed = 0x12345678;
  for (let i = 0; i < payloadBytes.length; i++) {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    payloadBytes[i] = 32 + ((seed >>> 24) % 95);
  }
  const bulk = new TextDecoder().decode(payloadBytes);
  function connect() {
    peer = createCtoxWebRtcNativePeer({ signalingUrl: config.signalingUrl, room: config.room,
      clientId: `${config.slot}-g${generation++}`, expectedNativePeerId: 'native000001',
      v4Hello: { createHello, verifyServerHello, requiredCapabilities: ['bench'] },
      iceServers: [{ urls: [config.turnUrl], username: 'bench', credential: 'bench' }],
      requestHandlers: { masterWrite: async () => ({ benchAck: true }) } });
    peer.on('message', ({ detail: { payload } }) => {
      if (payload?.method && payload.params?.[0]?.class) {
        const value = payload.params[0];
        record({ kind: 'received', id: payload.id, class: value.class, bytes: encoder.encode(value.body || '').length });
      } else if (payload?.id && Object.hasOwn(payload, 'result')) {
        const waiter = pending.get(payload.id);
        if (waiter) { pending.delete(payload.id); clearTimeout(waiter.timer);
          record({ kind: 'ack', id: payload.id, class: waiter.class, error: Boolean(payload.error), latencyMs: performance.now() - waiter.at }); waiter.resolve(!payload.error); }
      }
    });
    peer.on('error', ({ detail: error }) => { if (errors.length < 10000) errors.push({ at: performance.now() - started, code: String(error?.code || 'transport_error') }); });
    peer.on('peer-open', ({ detail: { peerId } }) => record({ kind: 'connected', peer: peerId, generation }));
    peer.connect();
  }
  connect();
  function offer(cls) {
    const id = `b:${config.slot}:${cls}:${sequence++}`;
    const body = cls === 'bulk' ? bulk : 'small control fixture';
    record({ kind: 'attempt', id, class: cls, bytes: body.length });
    let accepted = false;
    try { accepted = peer.send('native000001', { id, method: cls === 'control' ? 'ctoxProtocol' : 'masterWrite',
      collection: `bench_${cls}`, params: [{ id, class: cls, body }] }); } catch {}
    record({ kind: 'send_result', id, class: cls, bytes: body.length, ok: accepted, completionSemantics: 'queue_admission_only' });
    if (!accepted) return Promise.resolve(false);
    return new Promise(resolve => {
      const timer = setTimeout(() => { pending.delete(id); record({ kind: 'ack_timeout', id, class: cls }); resolve(false); }, 10000);
      pending.set(id, { at: performance.now(), resolve, timer, class: cls });
    });
  }
  let producing = false;
  const timers = [];
  function begin() {
    if (producing) return; producing = true; record({ kind: 'measurement_start' });
    timers.push(setInterval(() => {
      if ([...pending.values()].filter(x => x.class === 'control').length >= 16) { counters.producerSkipped++; return; }
      void offer('control');
    }, 100));
    timers.push(setInterval(() => { for (const cls of ['command', 'interactive', 'live', 'initial']) void offer(cls); }, 1000));
    void (async () => {
      while (running) { const ok = await offer('bulk'); if (!ok) await new Promise(r => setTimeout(r, 100)); }
    })();
  }
  const sampleTimer = setInterval(async () => {
    let bufferedBytes = 0;
    for (const connection of peer.connections.values()) bufferedBytes += connection.channel?.bufferedAmount || 0;
    maxBufferedBytes = Math.max(maxBufferedBytes, bufferedBytes);
    const connection = peer.connections.get('native000001');
    if (!connection?.peer || samples.length >= 2000) return;
    try {
      const report = await connection.peer.getStats();
      const transport = [...report.values()].find(x => x.type === 'transport' && x.selectedCandidatePairId);
      const pair = transport ? report.get(transport.selectedCandidatePairId) : [...report.values()].find(x => x.type === 'candidate-pair' && x.nominated && x.state === 'succeeded');
      if (!pair) return;
      const local = report.get(pair.localCandidateId), remote = report.get(pair.remoteCandidateId);
      samples.push({ at: performance.now() - started, connectionState: connection.peer.connectionState,
        localType: local?.candidateType, remoteType: remote?.candidateType,
        relay: local?.candidateType === 'relay' || remote?.candidateType === 'relay',
        rttMs: Number.isFinite(pair.currentRoundTripTime) ? pair.currentRoundTripTime * 1000 : null });
    } catch {}
  }, 1000);
  const bufferTimer = setInterval(() => {
    const total = [...peer.connections.values()].reduce((n, x) => n + (x.channel?.bufferedAmount || 0), 0);
    maxBufferedBytes = Math.max(maxBufferedBytes, total);
  }, 20);
  window.bench = {
    ready: () => peer.connections.get('native000001')?.v4Live === true,
    begin,
    async drop() { record({ kind: 'injected_disconnect' }); peer.close(); await new Promise(r => setTimeout(r, 1000)); if (running) connect(); },
    stop() { record({ kind: 'measurement_stop' }); running = false; for (const t of timers) clearInterval(t); },
    snapshot() { return { slot: config.slot, events, errors, samples, counters, maxBufferedBytes, maxFrameBytes,
      connections: [...peer.connections].map(([id, c]) => ({ id, connectionState: c.peer?.connectionState,
        iceState: c.peer?.iceConnectionState, gatheringState: c.peer?.iceGatheringState,
        localDescription: description(c.peer?.localDescription), remoteDescription: description(c.peer?.remoteDescription) })),
      browserTransport: peer.getTransportStatus(), elapsedMs: performance.now() - started }; },

    close() { running = false; for (const t of timers) clearInterval(t); clearInterval(sampleTimer); clearInterval(bufferTimer);
      peer.close(); for (const waiter of pending.values()) { clearTimeout(waiter.timer); waiter.resolve(false); } pending.clear(); },
  };
}
