const COORDINATORS = Symbol.for('ctox.rxdb.multi-tab-sync-coordinators.v1');
const CHANNEL_PREFIX = 'ctox-rxdb-sync-leader-';
const HEARTBEAT_MS = 5_000;
const LEASE_TTL_MS = 15_000;

export function getMultiTabSyncCoordinator({ databaseName, room } = {}) {
  const key = `${databaseName || 'ctox'}|${room || 'default'}`;
  const root = globalThis;
  if (!root[COORDINATORS]) root[COORDINATORS] = new Map();
  if (!root[COORDINATORS].has(key) || root[COORDINATORS].get(key)?.isClosed?.()) {
    root[COORDINATORS].set(key, createMultiTabSyncCoordinator({ databaseName, room }));
  }
  return root[COORDINATORS].get(key);
}

export function createMultiTabSyncCoordinator({
  databaseName,
  room,
  tabId = globalThis.crypto?.randomUUID?.() || `tab-${Math.random().toString(36).slice(2)}`,
  clock = Date.now,
} = {}) {
  if (!databaseName || !room) throw new TypeError('multi-tab sync coordinator requires databaseName and room');
  const listeners = new Set();
  const dirtyListeners = new Set();
  const externalChangeListeners = new Set();
  const channel = typeof globalThis.BroadcastChannel === 'function'
    ? new BroadcastChannel(`${CHANNEL_PREFIX}${databaseName}-${stableHash(room)}`)
    : null;
  const lockName = `ctox-rxdb-sync:${databaseName}:${stableHash(room)}`;
  let role = 'follower';
  let leaderTabId = '';
  let leaderSeenAtMs = 0;
  let started = false;
  let closed = false;
  let heartbeatTimer = null;
  let electionTimer = null;
  let releaseLock = null;
  let lockRequestRunning = false;

  const emitRole = () => {
    const status = snapshot();
    for (const listener of listeners) {
      try { listener(status); } catch {}
    }
    globalThis.dispatchEvent?.(new CustomEvent('ctox-rxdb-multi-tab-status', { detail: status }));
  };

  const post = (message) => {
    try { channel?.postMessage({ ...message, tabId, atMs: clock() }); } catch {}
  };

  const becomeLeader = (reason) => {
    if (closed) return;
    role = 'leader';
    leaderTabId = tabId;
    leaderSeenAtMs = clock();
    if (heartbeatTimer) clearInterval(heartbeatTimer);
    heartbeatTimer = setInterval(() => {
      leaderSeenAtMs = clock();
      post({ type: 'leader-heartbeat' });
    }, HEARTBEAT_MS);
    heartbeatTimer.unref?.();
    post({ type: 'leader-heartbeat', reason });
    emitRole();
  };

  const becomeFollower = (leader = '', reason = '') => {
    if (heartbeatTimer) clearInterval(heartbeatTimer);
    heartbeatTimer = null;
    const changed = role !== 'follower' || (leader && leader !== leaderTabId);
    role = 'follower';
    if (leader) leaderTabId = leader;
    if (changed) emitRole();
    if (reason) post({ type: 'follower', reason });
  };

  const tryWebLock = async () => {
    if (closed || lockRequestRunning || !globalThis.navigator?.locks?.request) return false;
    lockRequestRunning = true;
    let resolveAttempt;
    const attempted = new Promise((resolve) => { resolveAttempt = resolve; });
    navigator.locks.request(lockName, { mode: 'exclusive', ifAvailable: true }, async (lock) => {
      if (!lock || closed) {
        lockRequestRunning = false;
        resolveAttempt(false);
        return;
      }
      becomeLeader('web-lock');
      resolveAttempt(true);
      await new Promise((resolve) => { releaseLock = resolve; });
      releaseLock = null;
      lockRequestRunning = false;
      becomeFollower('', 'web-lock-released');
    }).catch(() => {
      lockRequestRunning = false;
      resolveAttempt(false);
    });
    return attempted;
  };

  const attemptElection = async () => {
    if (closed || role === 'leader') return;
    if (clock() - leaderSeenAtMs < LEASE_TTL_MS) return;
    if (globalThis.navigator?.locks?.request) {
      await tryWebLock();
      return;
    }
    post({ type: 'leader-claim' });
    await delay(30);
    if (clock() - leaderSeenAtMs >= LEASE_TTL_MS || !leaderTabId || tabId < leaderTabId) {
      becomeLeader('broadcast-election');
    }
  };

  if (channel) {
    channel.onmessage = (event) => {
      const message = event?.data;
      if (!message || message.tabId === tabId) return;
      if (message.type === 'leader-heartbeat') {
        leaderSeenAtMs = clock();
        leaderTabId = String(message.tabId || '');
        if (role === 'leader' && leaderTabId < tabId) {
          releaseLock?.();
          becomeFollower(leaderTabId, 'leader-tiebreak');
        } else if (role !== 'leader') {
          becomeFollower(leaderTabId);
        }
      } else if (message.type === 'leader-claim') {
        if (role === 'leader') post({ type: 'leader-heartbeat', reason: 'claim-rejected' });
        else if (!leaderTabId || String(message.tabId) < leaderTabId) leaderTabId = String(message.tabId);
      } else if (message.type === 'leader-release' && String(message.tabId || '') === leaderTabId) {
        leaderSeenAtMs = 0;
        leaderTabId = '';
        attemptElection().catch(() => {});
      } else if (message.type === 'dirty' && role === 'leader') {
        for (const listener of dirtyListeners) {
          try { listener(message); } catch {}
        }
      } else if (message.type === 'replicated-change' && role === 'follower') {
        for (const listener of externalChangeListeners) {
          try { listener(message); } catch {}
        }
        globalThis.dispatchEvent?.(new CustomEvent('ctox-rxdb-external-change', { detail: message }));
      }
    };
  }

  const lifecycleRelease = () => {
    if (role === 'leader') {
      post({ type: 'leader-release' });
      releaseLock?.();
    }
    becomeFollower('', 'page-lifecycle');
  };
  const lifecycleResume = () => attemptElection().catch(() => {});

  function start() {
    if (started) return Promise.resolve(snapshot());
    started = true;
    globalThis.document?.addEventListener?.('freeze', lifecycleRelease);
    globalThis.addEventListener?.('pagehide', lifecycleRelease);
    globalThis.document?.addEventListener?.('resume', lifecycleResume);
    globalThis.addEventListener?.('pageshow', lifecycleResume);
    electionTimer = setInterval(() => attemptElection().catch(() => {}), HEARTBEAT_MS);
    electionTimer.unref?.();
    return attemptElection().then(snapshot);
  }

  function snapshot() {
    return {
      schema: 'ctox.rxdb.multi-tab-sync.v1',
      databaseName,
      role,
      isLeader: role === 'leader',
      tabId,
      leaderTabId,
      leaderLeaseAgeMs: leaderSeenAtMs ? Math.max(0, clock() - leaderSeenAtMs) : null,
      updatedAtMs: clock(),
    };
  }

  return {
    start,
    snapshot,
    isLeader: () => role === 'leader',
    isClosed: () => closed,
    onRoleChange(listener) { listeners.add(listener); return () => listeners.delete(listener); },
    onDirty(listener) { dirtyListeners.add(listener); return () => dirtyListeners.delete(listener); },
    onExternalChange(listener) { externalChangeListeners.add(listener); return () => externalChangeListeners.delete(listener); },
    notifyDirty(collection, ids = []) { post({ type: 'dirty', collection, ids }); },
    notifyReplicatedChange(collection, ids = []) { post({ type: 'replicated-change', collection, ids }); },
    async close() {
      if (role === 'leader') post({ type: 'leader-release' });
      closed = true;
      releaseLock?.();
      if (heartbeatTimer) clearInterval(heartbeatTimer);
      if (electionTimer) clearInterval(electionTimer);
      globalThis.document?.removeEventListener?.('freeze', lifecycleRelease);
      globalThis.removeEventListener?.('pagehide', lifecycleRelease);
      globalThis.document?.removeEventListener?.('resume', lifecycleResume);
      globalThis.removeEventListener?.('pageshow', lifecycleResume);
      try { channel?.close(); } catch {}
      listeners.clear();
      dirtyListeners.clear();
      externalChangeListeners.clear();
    },
  };
}

function stableHash(value) {
  let hash = 2166136261;
  for (const character of String(value || '')) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export const multiTabSyncCoordinatorTestInternals = Object.freeze({
  HEARTBEAT_MS,
  LEASE_TTL_MS,
  stableHash,
});
