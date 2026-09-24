const COORDINATORS = Symbol.for('ctox.rxdb.multi-tab-sync-coordinators.v1');
const CHANNEL_PREFIX = 'ctox-rxdb-sync-leader-';
const HEARTBEAT_MS = 5_000;
const LEASE_TTL_MS = 15_000;
const DIRTY_ACK_TIMEOUT_MS = 10_000;
// Kept well below a human's patience: when the leader cannot answer, the
// caller still has to fall back to a direct bridge afterwards.
const NATIVE_REQUEST_TIMEOUT_MS = 6_000;

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
  const pendingDirtyAcks = new Map();
  const pendingNativeRequests = new Map();
  const nativeRequestHandlers = new Set();
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
  let releaseWaitAbort = null;
  let retryReleaseElection = false;
  let lifecycleSuspended = false;
  let resumeReleasedLeader = false;

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

  const handleDirty = async (message) => {
    let error = '';
    try {
      await Promise.all([...dirtyListeners].map((listener) => listener(message)));
    } catch (cause) {
      error = String(cause?.message || cause || 'leader push failed').slice(0, 240);
    }
    if (message.requestId) {
      post({
        type: 'dirty-ack',
        requestId: message.requestId,
        targetTabId: message.tabId,
        ok: !error,
        error,
      });
    }
    if (error && !message.requestId) throw new Error(error);
  };

  // Only the leader owns a WebRTC data channel, so a follower tab cannot run
  // requestNative() itself. Rather than let every non-leader tab fail, the
  // follower forwards the call over the same BroadcastChannel the dirty
  // handshake already uses and the leader answers on its behalf.
  const handleNativeRequestLocally = async (method, params, options) => {
    const handler = [...nativeRequestHandlers][0];
    if (typeof handler !== 'function') {
      throw new Error('This tab has no native request handler.');
    }
    return handler(method, params || {}, options || {});
  };

  const handleNativeRequest = async (message) => {
    const handler = [...nativeRequestHandlers][0];
    const respond = (payload) => post({
      type: 'native-response',
      requestId: message.requestId,
      targetTabId: message.tabId,
      ...payload,
    });
    if (typeof handler !== 'function') {
      respond({ ok: false, error: 'The multi-tab leader has no native request handler.' });
      return;
    }
    try {
      const result = await handler(message.method, message.params || {}, message.options || {});
      respond({ ok: true, result: result === undefined ? null : result });
    } catch (cause) {
      respond({ ok: false, error: String(cause?.message || cause || 'native request failed').slice(0, 240) });
    }
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

  const tryWebLock = async (waitForRelease = false) => {
    if (closed || lifecycleSuspended || !globalThis.navigator?.locks?.request) return false;
    if (lockRequestRunning) {
      if (waitForRelease) retryReleaseElection = true;
      return false;
    }
    lockRequestRunning = true;
    const abort = waitForRelease ? new AbortController() : null;
    if (abort) releaseWaitAbort = abort;
    let resolveAttempt;
    const attempted = new Promise((resolve) => { resolveAttempt = resolve; });
    const finishFailedAttempt = () => {
      if (releaseWaitAbort === abort) releaseWaitAbort = null;
      lockRequestRunning = false;
      resolveAttempt(false);
      if (retryReleaseElection && !closed) {
        retryReleaseElection = false;
        tryWebLock(true).catch(() => {});
      }
    };
    const options = waitForRelease
      ? { mode: 'exclusive', signal: abort.signal }
      : { mode: 'exclusive', ifAvailable: true };
    navigator.locks.request(lockName, options, async (lock) => {
      if (!lock || closed || lifecycleSuspended) {
        finishFailedAttempt();
        return;
      }
      if (releaseWaitAbort === abort) releaseWaitAbort = null;
      retryReleaseElection = false;
      becomeLeader('web-lock');
      resolveAttempt(true);
      await new Promise((resolve) => { releaseLock = resolve; });
      releaseLock = null;
      lockRequestRunning = false;
      becomeFollower('', 'web-lock-released');
      if (retryReleaseElection && !closed && !lifecycleSuspended && !leaderTabId) {
        retryReleaseElection = false;
        tryWebLock(true).catch(() => {});
      } else {
        retryReleaseElection = false;
      }
    }).catch(finishFailedAttempt);
    return attempted;
  };

  const attemptElection = async () => {
    if (closed || lifecycleSuspended || role === 'leader') return;
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
      } else if (message.type === 'leader-release'
        && String(message.tabId || '')
        && (!leaderTabId || String(message.tabId) === leaderTabId)) {
        leaderSeenAtMs = 0;
        leaderTabId = '';
        // Queue behind the releasing Web Lock: its broadcast can arrive before
        // the browser has actually completed the old lock callback.
        if (!lifecycleSuspended) {
          if (globalThis.navigator?.locks?.request) tryWebLock(true).catch(() => {});
          else attemptElection().catch(() => {});
        }
      } else if (message.type === 'dirty' && role === 'leader') {
        handleDirty(message).catch(() => {});
      } else if (message.type === 'dirty-ack' && String(message.targetTabId || '') === tabId) {
        const pending = pendingDirtyAcks.get(String(message.requestId || ''));
        if (pending) {
          pendingDirtyAcks.delete(String(message.requestId || ''));
          clearTimeout(pending.timer);
          if (message.ok === false) pending.reject(new Error(message.error || 'Leader could not push the collection.'));
          else pending.resolve(message);
        }
      } else if (message.type === 'native-request' && role === 'leader') {
        handleNativeRequest(message).catch(() => {});
      } else if (message.type === 'native-response' && String(message.targetTabId || '') === tabId) {
        const pending = pendingNativeRequests.get(String(message.requestId || ''));
        if (pending) {
          pendingNativeRequests.delete(String(message.requestId || ''));
          clearTimeout(pending.timer);
          if (message.ok === false) pending.reject(new Error(message.error || 'The leader could not serve the native request.'));
          else pending.resolve(message.result ?? null);
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
    lifecycleSuspended = true;
    retryReleaseElection = false;
    releaseWaitAbort?.abort();
    if (role === 'leader') {
      resumeReleasedLeader = true;
      leaderSeenAtMs = 0;
      leaderTabId = '';
      post({ type: 'leader-release' });
      releaseLock?.();
    }
    becomeFollower('', 'page-lifecycle');
  };
  const lifecycleResume = () => {
    lifecycleSuspended = false;
    const reacquireReleasedLine = resumeReleasedLeader && !leaderTabId;
    resumeReleasedLeader = false;
    if (reacquireReleasedLine && globalThis.navigator?.locks?.request) tryWebLock(true).catch(() => {});
    else attemptElection().catch(() => {});
  };

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
    notifyDirtyAndWait(collection, ids = [], { timeoutMs = DIRTY_ACK_TIMEOUT_MS } = {}) {
      if (role === 'leader') {
        return handleDirty({ type: 'dirty', collection, ids, tabId, atMs: clock() });
      }
      if (!channel || !leaderTabId) return Promise.reject(new Error('No multi-tab sync leader is available.'));
      const requestedLeaderTabId = leaderTabId;
      const requestId = globalThis.crypto?.randomUUID?.() || `dirty-${tabId}-${clock()}-${Math.random().toString(36).slice(2)}`;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pendingDirtyAcks.delete(requestId);
          // A tab can retain the Web Lock while its JS event loop is blocked
          // by a native browser dialog. Stop advertising that unresponsive
          // tab as healthy so the caller can open the bounded direct WebRTC
          // fallback. If the old leader resumes, its next heartbeat changes
          // leaderTabId again and the sync runtime demotes the direct bridge.
          if (leaderTabId === requestedLeaderTabId) {
            leaderSeenAtMs = 0;
            leaderTabId = '';
            emitRole();
            attemptElection().catch(() => {});
          }
          reject(new Error(`Multi-tab leader did not acknowledge ${collection} within ${timeoutMs}ms.`));
        }, Math.max(100, Number(timeoutMs) || DIRTY_ACK_TIMEOUT_MS));
        pendingDirtyAcks.set(requestId, { resolve, reject, timer });
        post({ type: 'dirty', requestId, collection, ids });
      });
    },
    onNativeRequest(handler) {
      nativeRequestHandlers.add(handler);
      return () => nativeRequestHandlers.delete(handler);
    },
    hasNativeRequestHandler: () => nativeRequestHandlers.size > 0,
    requestNativeViaLeader(method, params = {}, options = {}, { timeoutMs = NATIVE_REQUEST_TIMEOUT_MS } = {}) {
      if (role === 'leader') return handleNativeRequestLocally(method, params, options);
      if (!channel || !leaderTabId) {
        return Promise.reject(new Error('No multi-tab sync leader is available for native requests.'));
      }
      const requestedLeaderTabId = leaderTabId;
      const requestId = globalThis.crypto?.randomUUID?.() || `native-${tabId}-${clock()}-${Math.random().toString(36).slice(2)}`;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pendingNativeRequests.delete(requestId);
          // A leader that never answers is either running an older build
          // without this handler or is wedged. Either way it must stop being
          // advertised as healthy, or the Browser app stays dead in this tab
          // forever. Drop the lease and stand for election so the caller's
          // direct fallback has a chance to become the leader itself.
          if (leaderTabId === requestedLeaderTabId) {
            leaderSeenAtMs = 0;
            leaderTabId = '';
            emitRole();
            attemptElection().catch(() => {});
          }
          reject(new Error(`The multi-tab leader did not answer ${method} within ${timeoutMs}ms.`));
        }, Math.max(100, Number(timeoutMs) || NATIVE_REQUEST_TIMEOUT_MS));
        pendingNativeRequests.set(requestId, { resolve, reject, timer });
        post({ type: 'native-request', requestId, method, params, options });
      });
    },
    notifyReplicatedChange(collection, ids = []) { post({ type: 'replicated-change', collection, ids }); },
    async close() {
      if (role === 'leader') post({ type: 'leader-release' });
      closed = true;
      lifecycleSuspended = true;
      resumeReleasedLeader = false;
      retryReleaseElection = false;
      releaseWaitAbort?.abort();
      releaseLock?.();
      if (heartbeatTimer) clearInterval(heartbeatTimer);
      if (electionTimer) clearInterval(electionTimer);
      globalThis.document?.removeEventListener?.('freeze', lifecycleRelease);
      globalThis.removeEventListener?.('pagehide', lifecycleRelease);
      globalThis.document?.removeEventListener?.('resume', lifecycleResume);
      globalThis.removeEventListener?.('pageshow', lifecycleResume);
      try { channel?.close(); } catch {}
      for (const pending of pendingDirtyAcks.values()) {
        clearTimeout(pending.timer);
        pending.reject(new Error('Multi-tab sync coordinator closed before leader acknowledgement.'));
      }
      pendingDirtyAcks.clear();
      for (const pending of pendingNativeRequests.values()) {
        clearTimeout(pending.timer);
        pending.reject(new Error('Multi-tab sync coordinator closed before the native response arrived.'));
      }
      pendingNativeRequests.clear();
      nativeRequestHandlers.clear();
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
  DIRTY_ACK_TIMEOUT_MS,
  stableHash,
});
