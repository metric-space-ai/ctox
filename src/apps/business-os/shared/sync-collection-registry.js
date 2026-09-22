// The runtime owns bridge replacement. Leases retain interest in a collection,
// not ownership of one replication generation. Counts derive from live leases.
export class CollectionSyncRegistry extends Map {
  #resolved = new Map();
  #leases = new Map();

  current(collection) {
    return this.#resolved.get(collection) || {
      mode: 'pending', collection, state: null, reason: 'bridge-unavailable',
    };
  }

  set(collection, promise) {
    super.set(collection, promise);
    this.#resolved.set(collection, {
      mode: 'pending', collection, state: null, ready: promise, reason: 'startup-in-progress',
    });
    this.#notify(collection);
    Promise.resolve(promise).then(bridge => {
      if (this.get(collection) !== promise) return;
      this.#resolved.set(collection, bridge);
      this.#notify(collection);
    }, error => {
      if (this.get(collection) !== promise) return;
      this.#resolved.set(collection, { mode: 'failed', collection, state: null, error });
      this.#notify(collection);
    });
    return this;
  }

  delete(collection) {
    const removed = super.delete(collection);
    this.#resolved.delete(collection);
    this.#notify(collection);
    return removed;
  }

  clear() {
    const collections = [...this.keys()];
    super.clear();
    this.#resolved.clear();
    for (const collection of collections) this.#notify(collection);
  }

  leaseCount(collection) { return this.#leases.get(collection)?.size || 0; }
  leaseCounts() { return [...this.#leases].map(([collection, leases]) => [collection, leases.size]); }

  acquire(collection, reason, onRelease) {
    const registry = this;
    const leases = this.#leases.get(collection) || new Set();
    this.#leases.set(collection, leases);
    const listeners = new Set();
    let closed = false;
    let terminalMode = 'released';
    const notify = () => {
      for (const listener of [...listeners]) {
        try { listener(lease.bridge); }
        catch (error) { console.error('[sync-lease] observer failed', error); }
      }
    };
    const detach = (mode) => {
      if (closed) return false;
      closed = true;
      terminalMode = mode;
      leases.delete(entry);
      if (registry.#leases.get(collection) === leases && !leases.size) registry.#leases.delete(collection);
      notify();
      listeners.clear();
      return true;
    };
    const lease = {
      mode: 'leased', collection, reason,
      get bridge() {
        return closed ? { mode: terminalMode, collection, state: null } : registry.current(collection);
      },
      set bridge(candidate) {
        // Existing app adapters assign the result of startCollection/ready.
        // This is an assertion of runtime ownership, never a second writer.
        const current = registry.current(collection);
        if (!closed && (candidate === current
          || (candidate?.mode === 'pending' && candidate.ready
            && candidate.ready === registry.get(collection)))) return;
        const error = new Error('A collection lease can only reference its current runtime bridge.');
        error.code = 'SYNC_LEASE_BRIDGE_NOT_CURRENT';
        throw error;
      },
      subscribeBridge(listener) {
        if (closed) {
          listener(lease.bridge);
          return { unsubscribe() {} };
        }
        listeners.add(listener);
        try { listener(lease.bridge); }
        catch (error) { listeners.delete(listener); throw error; }
        return { unsubscribe: () => listeners.delete(listener) };
      },
      async release() {
        if (!detach('released')) return false;
        await onRelease(registry.leaseCount(collection));
        return true;
      },
    };
    const entry = { notify, revoke: () => detach('stopped') };
    leases.add(entry);
    return lease;
  }

  revokeLeases(collection) {
    for (const entry of [...(this.#leases.get(collection) || [])]) entry.revoke();
  }

  revokeAllLeases() {
    for (const collection of [...this.#leases.keys()]) this.revokeLeases(collection);
  }

  #notify(collection) {
    for (const entry of [...(this.#leases.get(collection) || [])]) entry.notify();
  }
}
