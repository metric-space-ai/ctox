// Measurement-only browser ESM. No collection materialization or runtime policy.
// RTCDataChannelStats counts SCTP application payload bytes, NOT IP/TURN/DTLS
// overhead. Collection frameTransport counters are not inputs to this meter.
const fields = ['bytesReceived', 'bytesSent', 'messagesReceived', 'messagesSent'];
export class DataChannelByteMeter {
  #meterId = globalThis.crypto.randomUUID();
  #identities = new WeakMap();
  #nextIdentity = 0;

  async snapshot(peerConnections) {
    const unique = [...new Set(peerConnections)];
    if (unique.length === 0) throw new Error('No actual RTCPeerConnection supplied');
    const channels = [];
    for (const connection of unique) {
      if (!connection || typeof connection.getStats !== 'function') {
        throw new Error('Invalid RTCPeerConnection');
      }
      if (!this.#identities.has(connection)) this.#identities.set(connection, ++this.#nextIdentity);
      const connectionId = this.#identities.get(connection);
      const stats = await connection.getStats();
      const channelIds = new Set();
      for (const report of stats.values()) {
        if (report.type !== 'data-channel') continue;
        if (typeof report.id !== 'string' || !report.id || channelIds.has(report.id)) {
          throw new Error('Invalid or duplicate data-channel stats identity');
        }
        channelIds.add(report.id);
        for (const field of fields) {
          if (!Number.isSafeInteger(report[field]) || report[field] < 0) {
            throw new Error(`Missing or invalid RTCDataChannelStats.${field}`);
          }
        }
        channels.push({ connectionId, channelId: report.id, label: report.label,
          state: report.state, ...Object.fromEntries(fields.map((field) => [field, report[field]])) });
      }
      if (channelIds.size === 0) throw new Error('RTCDataChannelStats unavailable; bytes unknown');
    }
    return { version: 1, meterId: this.#meterId, meter: 'RTCDataChannelStats', unit: 'SCTP application payload bytes',
      connections: unique.length, channels };
  }
}

export function counterDelta(before, after) {
  if (before?.version !== 1 || after?.version !== 1 || before.meter !== 'RTCDataChannelStats'
      || after.meter !== before.meter || !before.meterId || after.meterId !== before.meterId
      || before.unit !== 'SCTP application payload bytes' || after.unit !== before.unit) {
    throw new Error('Incompatible counter snapshots');
  }
  const index = (snapshot) => {
    if (!Array.isArray(snapshot.channels) || snapshot.channels.length === 0) {
      throw new Error('Counter channels missing');
    }
    const entries = new Map();
    for (const channel of snapshot.channels) {
      const key = `${channel.connectionId}:${channel.channelId}`;
      if (!Number.isSafeInteger(channel.connectionId) || channel.connectionId < 1
          || typeof channel.channelId !== 'string' || !channel.channelId || entries.has(key)) {
        throw new Error('Counter identity duplicated or invalid');
      }
      for (const field of fields) {
        if (!Number.isSafeInteger(channel[field]) || channel[field] < 0) {
          throw new Error(`Counter ${field} missing or invalid`);
        }
      }
      entries.set(key, channel);
    }
    return entries;
  };
  const previous = index(before);
  const current = index(after);
  if (previous.size !== current.size || [...previous.keys()].some((key) => !current.has(key))) {
    throw new Error('Connection/channel changed during measurement; interval incomplete');
  }
  const totals = Object.fromEntries(fields.map((field) => [field, 0]));
  const channels = [];
  for (const [key, latest] of current) {
    const first = previous.get(key);
    const delta = {};
    for (const field of fields) {
      delta[field] = latest[field] - first[field];
      if (delta[field] < 0) throw new Error('Counter reset; interval incomplete');
      totals[field] += delta[field];
      if (!Number.isSafeInteger(totals[field])) throw new Error('Counter total exceeds safe integer');
    }
    channels.push({ connectionId: latest.connectionId, channelId: latest.channelId,
      label: latest.label, ...delta });
  }
  return { version: 1, meterId: after.meterId, meter: after.meter, unit: after.unit, totals, channels };
}
