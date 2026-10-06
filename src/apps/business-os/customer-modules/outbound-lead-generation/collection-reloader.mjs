// Collection invalidations are notifications, never document queries.
// One serialized consumer retains invalidations arriving during reads or retries.
export function createCollectionReloader({
  collections, reload, afterReload = () => {}, onError = () => {},
  debounceMs = 150, intervalMs = 1000,
  setTimer = globalThis.setTimeout, clearTimer = globalThis.clearTimeout,
}) {
  const keys = Object.keys(collections);
  const pending = new Set();
  const subscriptions = [];
  let timer = null;
  let running = false;
  let closed = false;
  let failures = 0;
  function arm(delay) {
    if (closed || running || timer !== null || !pending.size) return;
    timer = setTimer(flush, delay);
  }
  function request(requested = keys) {
    if (closed) return;
    for (const key of requested) if (keys.includes(key)) pending.add(key);
    arm(debounceMs);
  }
  async function flush() {
    timer = null;
    if (closed || running || !pending.size) return;
    running = true;
    const requested = [...pending];
    pending.clear();
    let delay = intervalMs;
    try {
      await reload(requested);
      if (!closed) await afterReload(requested);
      failures = 0;
    } catch (error) {
      if (!closed) {
        for (const key of requested) pending.add(key);
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
      subscriptions.push(collection.$.subscribe(() => request([key]), { invalidateOnly: true }));
    }
  } catch (error) {
    dispose();
    throw error;
  }
  return { request, dispose };
}
