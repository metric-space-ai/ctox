// V1.5 multi-tab leader broker.
//
// Two browser tabs viewing the same Business-OS instance both want to fetch
// the same query windows. Without coordination they each issue a remote
// `rxdb.query.fetch` — wasted bandwidth and 2x server-side load. This broker
// elects ONE tab per (databaseName, windowKey) to do the fetch, and other
// tabs subscribe to the result via BroadcastChannel.

const CHANNEL_PREFIX = 'ctox-rxdb-v1_5-broker-';
const CLAIM_TTL_MS = 30_000;

export function createBroadcastChannelBroker({ databaseName, tabId = randomTabId(), clock = Date.now } = {}) {
  if (!databaseName) throw new TypeError('broker requires databaseName');
  if (typeof globalThis.BroadcastChannel !== 'function') {
    return createMemoryBroker({ databaseName, tabId, clock });
  }
  const channel = new globalThis.BroadcastChannel(`${CHANNEL_PREFIX}${databaseName}`);
  const localClaims = new Map(); // windowKey -> { expiresAt }
  const remoteClaims = new Map(); // windowKey -> { tabId, expiresAt }
  const completions = new Map(); // windowKey -> Promise resolvers waiting on remote completion

  channel.onmessage = (event) => {
    const msg = event?.data;
    if (!msg || typeof msg !== 'object') return;
    const now = clock();
    if (msg.type === 'claim') {
      remoteClaims.set(msg.windowKey, { tabId: msg.tabId, expiresAt: now + CLAIM_TTL_MS });
    } else if (msg.type === 'release') {
      remoteClaims.delete(msg.windowKey);
    } else if (msg.type === 'complete') {
      remoteClaims.delete(msg.windowKey);
      const waiter = completions.get(msg.windowKey);
      if (waiter) {
        completions.delete(msg.windowKey);
        waiter.resolve(msg.result);
      }
    }
  };

  function expired(claim, now) {
    return !claim || claim.expiresAt < now;
  }

  return {
    kind: 'broadcast-channel',
    tabId,
    async claim(windowKey) {
      const now = clock();
      // Drop expired remote claims (other tab crashed without releasing).
      const remote = remoteClaims.get(windowKey);
      if (remote && expired(remote, now)) {
        remoteClaims.delete(windowKey);
      } else if (remote) {
        return false;
      }
      const local = localClaims.get(windowKey);
      if (local && !expired(local, now)) return false;
      localClaims.set(windowKey, { expiresAt: now + CLAIM_TTL_MS });
      channel.postMessage({ type: 'claim', windowKey, tabId, at: now });
      return true;
    },
    async release(windowKey, result = null) {
      localClaims.delete(windowKey);
      channel.postMessage({ type: 'complete', windowKey, tabId, result, at: clock() });
    },
    async waitForRemote(windowKey, timeoutMs = 5_000) {
      return new Promise((resolve) => {
        const timer = setTimeout(() => {
          completions.delete(windowKey);
          resolve(null);
        }, timeoutMs);
        completions.set(windowKey, {
          resolve: (val) => { clearTimeout(timer); resolve(val); },
        });
      });
    },
    close() {
      try { channel.close(); } catch {}
    },
  };
}

// In-Node / environments without BroadcastChannel: degenerate single-tab broker.
export function createMemoryBroker({ databaseName, tabId = randomTabId(), clock = Date.now } = {}) {
  const claims = new Set();
  return {
    kind: 'memory',
    tabId,
    async claim(windowKey) {
      if (claims.has(windowKey)) return false;
      claims.add(windowKey);
      return true;
    },
    async release(windowKey) { claims.delete(windowKey); },
    async waitForRemote() { return null; },
    close() {},
  };
}

function randomTabId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return `tab-${Math.random().toString(36).slice(2, 12)}`;
}
