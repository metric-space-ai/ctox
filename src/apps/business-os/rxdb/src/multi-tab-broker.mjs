// V1.5 multi-tab leader broker.
//
// Two browser tabs viewing the same Business-OS instance both want to fetch
// the same query windows. Without coordination they each issue a remote
// `rxdb.query.fetch` — wasted bandwidth and 2x server-side load. This broker
// elects ONE tab per (databaseName, windowKey) to do the fetch, and other
// tabs subscribe to the result via BroadcastChannel.

const CHANNEL_PREFIX = 'ctox-rxdb-v1_5-broker-';
const CLAIM_TTL_MS = 30_000;
const CLAIM_ELECTION_MS = 25;
const CLAIM_RENEW_MS = 10_000;

export function createBroadcastChannelBroker({ databaseName, tabId = randomTabId(), clock = Date.now } = {}) {
  if (!databaseName) throw new TypeError('broker requires databaseName');
  if (typeof globalThis.BroadcastChannel !== 'function') {
    return createMemoryBroker({ databaseName, tabId, clock });
  }
  const channel = new globalThis.BroadcastChannel(`${CHANNEL_PREFIX}${databaseName}`);
  const localClaims = new Map(); // windowKey -> { expiresAt }
  const remoteClaims = new Map(); // windowKey -> { tabId, expiresAt }
  const completions = new Map(); // windowKey -> Promise resolvers waiting on remote completion
  let closed = false;

  function post(message) {
    if (closed) return false;
    try {
      channel.postMessage(message);
      return true;
    } catch {
      return false;
    }
  }

  channel.onmessage = (event) => {
    const msg = event?.data;
    if (!msg || typeof msg !== 'object') return;
    const now = clock();
    if (msg.type === 'claim') {
      remoteClaims.set(msg.windowKey, { tabId: msg.tabId, expiresAt: now + CLAIM_TTL_MS });
      const local = localClaims.get(msg.windowKey);
      if (local && String(msg.tabId) < String(tabId)) {
        clearInterval(local.renewTimer);
        localClaims.delete(msg.windowKey);
      }
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
    get closed() { return closed; },
    async claim(windowKey) {
      if (closed) return false;
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
      const renewTimer = setInterval(() => {
        const claim = localClaims.get(windowKey);
        if (!claim) return;
        claim.expiresAt = clock() + CLAIM_TTL_MS;
        if (!post({ type: 'claim', windowKey, tabId, at: clock(), renewal: true })) {
          clearInterval(claim.renewTimer);
          localClaims.delete(windowKey);
        }
      }, CLAIM_RENEW_MS);
      localClaims.set(windowKey, { expiresAt: now + CLAIM_TTL_MS, renewTimer });
      if (!post({ type: 'claim', windowKey, tabId, at: now })) {
        clearInterval(renewTimer);
        localClaims.delete(windowKey);
        return false;
      }
      await new Promise((resolve) => setTimeout(resolve, CLAIM_ELECTION_MS));
      if (closed) {
        clearInterval(renewTimer);
        localClaims.delete(windowKey);
        return false;
      }
      const contender = remoteClaims.get(windowKey);
      if (contender && !expired(contender, clock()) && String(contender.tabId) < String(tabId)) {
        clearInterval(renewTimer);
        localClaims.delete(windowKey);
        return false;
      }
      return true;
    },
    async release(windowKey, result = null) {
      const local = localClaims.get(windowKey);
      if (local) clearInterval(local.renewTimer);
      localClaims.delete(windowKey);
      post({ type: 'complete', windowKey, tabId, result, at: clock() });
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
      if (closed) return;
      // Release every owned window before closing the channel. Without this,
      // a replacement replication state in this or another tab keeps seeing
      // the dead owner until CLAIM_TTL_MS and fails its first query window.
      for (const [windowKey, claim] of localClaims.entries()) {
        clearInterval(claim.renewTimer);
        post({ type: 'release', windowKey, tabId, at: clock(), reason: 'broker-close' });
      }
      localClaims.clear();
      for (const waiter of completions.values()) waiter.resolve(null);
      completions.clear();
      closed = true;
      channel.onmessage = null;
      try { channel.close(); } catch {}
    },
  };
}

// In-Node / environments without BroadcastChannel: degenerate single-tab broker.
export function createMemoryBroker({ databaseName, tabId = randomTabId(), clock = Date.now } = {}) {
  const claims = new Set();
  let closed = false;
  return {
    kind: 'memory',
    tabId,
    get closed() { return closed; },
    async claim(windowKey) {
      if (closed) return false;
      if (claims.has(windowKey)) return false;
      claims.add(windowKey);
      return true;
    },
    async release(windowKey) { claims.delete(windowKey); },
    async waitForRemote() { return null; },
    close() { closed = true; claims.clear(); },
  };
}

function randomTabId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return `tab-${Math.random().toString(36).slice(2, 12)}`;
}
