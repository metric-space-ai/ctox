// Test-only browser observer. Records metadata, never changes engine frames/timing.
function installPhaseTrace() {
  const trace = globalThis.__syncV3Trace = { version: 1, bootAt: performance.now(), events: [], marks: {}, errors: [] };
  const pending = new Set(), transfers = new Map();
  let channelSequence = 0, peerSequence = 0;
  const emit = event => {
    if (trace.events.length >= 20000) { if (!trace.errors.includes('event-budget')) trace.errors.push('event-budget'); return; }
    trace.events.push({ at: performance.now(), ...event });
  };
  trace.mark = name => { trace.marks[name] = performance.now(); };
  trace.mark('boot');
  globalThis.__syncV3BootAt = trace.bootAt;
  trace.drain = async () => { while (pending.size) await Promise.all([...pending]); };
  trace.pairsFor = (stats, connectionState = null) => {
    const selected = new Set([...stats.values()].filter(item => item.type === 'transport').map(item => item.selectedCandidatePairId).filter(Boolean));
    const pairs = [];
    for (const entry of stats.values()) if (entry.type === 'candidate-pair'
      && (entry.state === 'succeeded' || (entry.state === 'in-progress' && connectionState === 'connected'))
      && (selected.size ? selected.has(entry.id) : entry.nominated)) {
      const remote = stats.get(entry.remoteCandidateId);
      pairs.push({ currentRoundTripTimeMs: Number.isFinite(entry.currentRoundTripTime) ? entry.currentRoundTripTime * 1000 : null,
        remoteAddress: remote?.address || remote?.ip || null, remotePort: remote?.port || null,
        bytesReceived: entry.bytesReceived, bytesSent: entry.bytesSent });
    }
    return pairs;
  };
  const logical = (payload, direction, channel, at, transfer = null) => {
    const envelope = payload?.params?.[0] || {};
    const rows = Array.isArray(envelope) ? envelope : [];
    emit({ kind: 'logical', at, direction, channel, id: typeof payload?.id === 'string' ? payload.id : null,
      method: typeof payload?.method === 'string' ? payload.method.slice(0, 100) : null,
      collection: payload?.collection || envelope.collectionName || null,
      streamId: typeof envelope.requestId === 'string' ? envelope.requestId : null,
      streamFinal: envelope.complete === true || envelope.final === true,
      responseRows: Array.isArray(payload?.result) ? payload.result.length : null,
      resultDocuments: Array.isArray(payload?.result?.documents) ? payload.result.documents.length : null,
      writeMarker: rows.map(row => row?.newDocumentState?.write_marker).find(marker => /^s0-write-\d+$/.test(marker || '')) || null,
      transferId: transfer?.id || null, startedAt: transfer?.startAt ?? at,
      frames: transfer?.totalFrames || 0, encoding: transfer?.encoding || null,
    });
  };
  const decode = async (text, encoding) => {
    if (!encoding) return JSON.parse(text);
    if (encoding !== 'deflate-raw-base64') throw Error('trace-unknown-frame-encoding');
    const binary = atob(text), bytes = Uint8Array.from(binary, char => char.charCodeAt(0));
    const reader = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw')).getReader();
    const decoder = new TextDecoder(); let size = 0, output = '';
    for (;;) {
      const part = await reader.read(); if (part.done) break;
      size += part.value.byteLength; if (size > 16 * 1024 * 1024) { await reader.cancel(); throw Error('trace-inflate-budget'); }
      output += decoder.decode(part.value, { stream: true });
    }
    return JSON.parse(output + decoder.decode());
  };
  const observe = (value, direction, channel) => {
    const at = performance.now();
    if (typeof value !== 'string') return;
    let payload; try { payload = JSON.parse(value); } catch { return; }
    if (!payload?.ctoxFrame) { logical(payload, direction, channel, at); return; }
    const key = `${channel}|${direction}|${payload.transferId}|${payload.attempt || 0}`;
    emit({ kind: 'frame', at, direction, channel, transferId: payload.transferId,
      frameKind: payload.kind, seq: payload.seq ?? null, ackSeq: payload.ackSeq ?? null,
      final: payload.final === true, attempt: payload.attempt || 0,
      totalFrames: payload.totalFrames ?? null, windowSize: payload.windowSize ?? null,
      bytes: value.length, encoding: payload.encoding || null });
    if (payload.kind === 'start') {
      if (transfers.size >= 32 || payload.totalBytes > 8 * 1024 * 1024) { trace.errors.push('trace-transfer-budget'); return; }
      transfers.set(key, { id: payload.transferId, startAt: at, totalFrames: payload.totalFrames,
        encoding: payload.encoding, chunks: new Map(), bytes: 0 });
    } else if (payload.kind === 'chunk') {
      const transfer = transfers.get(key); if (!transfer) return;
      if (!transfer.chunks.has(payload.seq)) { transfer.chunks.set(payload.seq, payload.data); transfer.bytes += payload.data.length; }
      if (transfer.bytes > 8 * 1024 * 1024) { transfers.delete(key); trace.errors.push('trace-transfer-budget'); return; }
      if (transfer.chunks.size === transfer.totalFrames) {
        transfers.delete(key);
        let text = ''; for (let seq = 0; seq < transfer.totalFrames; seq++) text += transfer.chunks.get(seq) || '';
        const job = decode(text, transfer.encoding).then(decoded => logical(decoded, direction, channel, at, transfer))
          .catch(error => trace.errors.push(error.message)).finally(() => pending.delete(job));
        pending.add(job);
      }
    }
  };
  const Original = globalThis.RTCPeerConnection;
  const send = globalThis.RTCDataChannel.prototype.send;
  const channels = new WeakMap();
  const attach = (channel, peer) => {
    if (channels.has(channel)) return;
    const id = `${peer}:channel${channelSequence++}`; channels.set(channel, id);
    emit({ kind: 'channel-created', channel: id, label: channel.label });
    channel.addEventListener('open', () => emit({ kind: 'channel-open', channel: id, label: channel.label }));
    channel.addEventListener('message', event => { try { observe(event.data, 'in', id); } catch (error) { trace.errors.push(error.message); } });
  };
  globalThis.RTCDataChannel.prototype.send = function (value) {
    // Call the native method first: observer errors cannot turn a successful send into failure.
    const result = send.call(this, value);
    try { observe(value, 'out', channels.get(this) || 'unknown'); } catch (error) { trace.errors.push(error.message); }
    return result;
  };
  globalThis.__syncV3Rtc = [];
  globalThis.RTCPeerConnection = class extends Original {
    constructor(...args) {
      super(...args); const peer = `peer${peerSequence++}`;
      globalThis.__syncV3Rtc.push(this); emit({ kind: 'rtc-created', peer });
      for (const name of ['iceconnectionstatechange', 'connectionstatechange', 'signalingstatechange']) {
        this.addEventListener(name, () => emit({ kind: name, peer,
          ice: this.iceConnectionState, connection: this.connectionState, signaling: this.signalingState }));
      }
      this.addEventListener('datachannel', event => attach(event.channel, peer));
      const create = this.createDataChannel.bind(this);
      this.createDataChannel = (...values) => { const channel = create(...values); attach(channel, peer); return channel; };
    }
  };
}
module.exports = { installPhaseTrace };
