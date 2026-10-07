// Collection invalidations are notifications, never document queries.
// One serialized consumer retains invalidations arriving during reads or retries.
export function createCollectionReloader({
  collections, reload, afterReload = () => {}, onError = () => {},
  debounceMs = 150, intervalMs = 1000,
  setTimer = globalThis.setTimeout, clearTimer = globalThis.clearTimeout,
}) {
  const keys = Object.keys(collections);
  const pending = new Set();
  // Per key: the changed rows named by the invalidation events since the last
  // flush, or null once any event did not name them (then the key reloads
  // in full).
  const pendingChanges = new Map();
  const subscriptions = [];
  let timer = null;
  let running = false;
  let closed = false;
  let failures = 0;
  function arm(delay) {
    if (closed || running || timer !== null || !pending.size) return;
    timer = setTimer(flush, delay);
  }
  function request(requested = keys, event = null) {
    if (closed) return;
    const changes = Array.isArray(event?.changes) ? event.changes : null;
    for (const key of requested) {
      if (!keys.includes(key)) continue;
      const known = pendingChanges.has(key) ? pendingChanges.get(key) : (pending.has(key) ? null : new Map());
      if (known && changes) {
        for (const change of changes) if (change?.id) known.set(String(change.id), change);
        pendingChanges.set(key, known);
      } else {
        pendingChanges.set(key, null);
      }
      pending.add(key);
    }
    arm(debounceMs);
  }
  async function flush() {
    timer = null;
    if (closed || running || !pending.size) return;
    running = true;
    const requested = [...pending];
    const changesByKey = new Map(requested.map(key => [key, pendingChanges.get(key) ?? null]));
    pending.clear();
    pendingChanges.clear();
    let delay = intervalMs;
    try {
      await reload(requested, changesByKey);
      if (!closed) await afterReload(requested);
      failures = 0;
    } catch (error) {
      if (!closed) {
        const failedKeys = Array.isArray(error?.failedKeys) ? error.failedKeys : requested;
        // A failed read retries in full: its delta may already be stale.
        for (const key of failedKeys) {
          if (!requested.includes(key)) continue;
          pending.add(key);
          pendingChanges.set(key, null);
        }
        failures += 1;
        delay = Math.min(60000, 2000 * 2 ** Math.min(failures - 1, 5));
        try { onError(error, requested); } catch { /* reporting cannot lose the retry */ }
      }
    } finally {
      running = false;
      arm(delay);
    }
  }
  function dispose() {
    closed = true;
    pending.clear();
    if (timer !== null) clearTimer(timer);
    timer = null;
    for (const subscription of subscriptions) subscription?.unsubscribe?.();
  }
  try {
    for (const [key, collection] of Object.entries(collections)) {
      if (typeof collection?.$?.subscribe !== 'function') {
        throw new Error('Outbound requires the shell collection invalidation API: ' + key);
      }
      subscriptions.push(collection.$.subscribe((event) => request([key], event), { invalidateOnly: true }));
    }
  } catch (error) {
    dispose();
    throw error;
  }
  return { request, dispose };
}
