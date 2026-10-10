// Isolated S0 UDP impairment relay. No TURN service or tenant endpoint is used.
const dgram = require('node:dgram');
const { performance } = require('node:perf_hooks');
const { writeFileSync } = require('node:fs');

class UdpRelay {
  constructor(rttMs, evidencePath) {
    if (![0, 300, 600].includes(rttMs)) throw Error('Relay RTT must be 0, 300 or 600 ms');
    this.rttMs = rttMs;
    this.evidencePath = evidencePath;
    this.pairs = new Map();
    this.closed = false;
    this.errors = [];
  }
  async pair(key) {
    if (!this.pairs.has(key)) {
      this.pairs.set(key, (async () => {
        const socket = dgram.createSocket('udp4');
        const pair = { socket, native: null, browser: null, pending: [], timers: new Set(),
          queuedBytes: 0, forwarded: { native: 0, browser: 0 }, bytes: { native: 0, browser: 0 },
          holdCount: 0, holdSumMs: 0, holdMinMs: null, holdMaxMs: 0 };
        socket.on('error', (error) => this.errors.push(error.code || 'udp_error'));
        socket.on('message', (data, from) => {
          const side = pair.native?.address === from.address && pair.native?.port === from.port
            ? 'native' : from.address === '127.0.0.1' ? 'browser' : null;
          if (!side || this.closed) return;
          // Chromium may advertise its LAN candidate but send to the relay
          // through a loopback candidate. Learn the observed return endpoint
          // like a NAT relay; native remains pinned to its declared loopback.
          if (side === 'browser') {
            pair.browser = { address: from.address, port: from.port };
            pair.browserObserved = true;
          }
          if (pair.queuedBytes + data.length > 16 * 1024 * 1024 || pair.timers.size > 16384) {
            this.errors.push('relay_queue_bound');
            return;
          }
          const item = { data: Buffer.from(data), side, arrived: performance.now() };
          pair.queuedBytes += data.length;
          if (!pair.native || !pair.browser) pair.pending.push(item);
          else this.forward(pair, item);
        });
        await new Promise((resolve, reject) => {
          socket.once('error', reject);
          socket.bind(0, '127.0.0.1', () => { socket.removeListener('error', reject); resolve(); });
        });
        pair.port = socket.address().port;
        return pair;
      })());
    }
    return this.pairs.get(key);
  }
  forward(pair, item) {
    const other = pair[item.side === 'native' ? 'browser' : 'native'];
    const wait = Math.max(0, this.rttMs / 2 - (performance.now() - item.arrived));
    const timer = setTimeout(() => {
      pair.timers.delete(timer);
      pair.queuedBytes -= item.data.length;
      if (this.closed) return;
      const hold = performance.now() - item.arrived;
      pair.holdCount++;
      pair.holdSumMs += hold;
      pair.holdMinMs = Math.min(pair.holdMinMs ?? hold, hold);
      pair.holdMaxMs = Math.max(pair.holdMaxMs, hold);
      pair.socket.send(item.data, other.port, other.address, error => {
        if (error) this.errors.push(error.code || 'udp_send_error');
        else { pair.forwarded[item.side]++; pair.bytes[item.side] += item.data.length; }
      });
    }, wait);
    pair.timers.add(timer);
  }
  rewriteCandidate(pair, role, line) {
    const parts = line.trim().replace(/^a=/, '').split(/\s+/);
    if (!/^candidate:/.test(parts[0]) || parts[1] !== '1'
      || parts[2].toLowerCase() !== 'udp' || parts[7] !== 'host'
      || !/^\d+\.\d+\.\d+\.\d+$/.test(parts[4])) return null;
    const port = Number(parts[5]);
    if (!Number.isInteger(port) || port < 1 || port > 65535) throw Error('Invalid candidate port');
    const side = role === 'ctox_instance' ? 'native' : role === 'browser' ? 'browser' : null;
    if (!side || (side === 'native' && parts[4] !== '127.0.0.1')) throw Error('Non-isolated relay endpoint');
    const endpoint = { address: parts[4], port };
    if (side === 'native' && pair.native
      && (pair.native.address !== endpoint.address || pair.native.port !== endpoint.port)) return null;
    if (side === 'native' || !pair.browserObserved) pair[side] = endpoint;
    parts[4] = '127.0.0.1';
    parts[5] = String(pair.port);
    if (pair.native && pair.browser) {
      for (const item of pair.pending.splice(0)) this.forward(pair, item);
    }
    return parts.join(' ');
  }
  async rewrite(message, sender, receiver) {
    if (this.closed) throw Error('Relay retired');
    const roles = new Set([sender.role, receiver.role]);
    if (!roles.has('browser') || !roles.has('ctox_instance')) throw Error('Relay needs browser/native peers');
    const key = [sender.id, receiver.id].sort().join(':');
    const pair = await this.pair(key);
    const copy = structuredClone(message);
    const data = copy.data || copy.signal;
    if (!data) return copy;
    if (typeof data.sdp === 'string') {
      data.sdp = data.sdp.split('\r\n').flatMap(line => {
        if (!line.startsWith('a=candidate:')) return [line];
        const rewritten = this.rewriteCandidate(pair, sender.role, line);
        return rewritten ? [`a=${rewritten}`] : [];
      }).join('\r\n');
    }
    if (data.candidate) {
      const value = typeof data.candidate === 'string' ? data.candidate : data.candidate.candidate;
      if (typeof value !== 'string') return copy;
      const rewritten = this.rewriteCandidate(pair, sender.role, value);
      if (!rewritten) return null; // Never leak a direct bypass candidate.
      if (typeof data.candidate === 'string') data.candidate = rewritten;
      else data.candidate.candidate = rewritten;
    }
    return copy;
  }
  async snapshot() {
    const pairs = await Promise.all(this.pairs.values());
    return { version: 1, type: 'loopback-udp-datagram-relay', requestedRttMs: this.rttMs,
      oneWayDelayMs: this.rttMs / 2, errors: this.errors,
      pairs: pairs.map(pair => ({ relayPort: pair.port, endpointsKnown: Boolean(pair.native && pair.browser),
        forwarded: pair.forwarded, bytes: pair.bytes, pendingDatagrams: pair.pending.length,
        holdCount: pair.holdCount, holdMinMs: pair.holdMinMs, holdMaxMs: pair.holdMaxMs,
        holdMeanMs: pair.holdCount ? pair.holdSumMs / pair.holdCount : null })) };
  }
  async close() {
    if (this.closed) return;
    this.closed = true;
    const result = await this.snapshot();
    for (const pair of await Promise.all(this.pairs.values())) {
      for (const timer of pair.timers) clearTimeout(timer);
      await new Promise(resolve => pair.socket.close(resolve));
    }
    if (this.evidencePath) writeFileSync(this.evidencePath, JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
  }
}
module.exports = { UdpRelay };
