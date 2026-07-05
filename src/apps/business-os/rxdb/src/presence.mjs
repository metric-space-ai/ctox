// Presence (ctox-presence-v1): ephemeral "who is looking at / editing what"
// state between browser peers, relayed through the native CTOX peer.
//
// Presence is ADVISORY UX STATE ONLY. It is never persisted (no IndexedDB, no
// SQLite, no collection), never replicated as documents, and must never gate
// any permission or mutation — the server-side policy path stays the only
// authority. The wire shape is a transport-control frame like
// `rxdb.activeCollections`: the browser sends its local entry set via
// `rxdb.presence.update`, the native hub keeps it in memory and pushes each
// peer the aggregate of every OTHER peer's live entries as a `presence$`
// response frame (see connection_handler_rs.rs).
//
// Local entries carry an owner key (one per module/app surface) so several
// Business OS modules in one tab can publish independently; the wire set is
// the union. An entry is a plain JSON object; the conventional fields are
// `{ collection, recordId, actorId, actorName, mode }` with mode one of
// "viewing" | "editing", but the transport treats entries as opaque.

import { CTOX_PRESENCE_RPC } from './protocol-contract.generated.mjs';

// Collapse bursts of local presence changes into one wire send, mirroring
// ACTIVE_NOTIFY_DEBOUNCE_MS in active-collections.mjs.
export const PRESENCE_NOTIFY_DEBOUNCE_MS = 100;

class PresenceRegistry {
  constructor({
    clock = () => Date.now(),
    refreshMs = Number(CTOX_PRESENCE_RPC.refreshMs) || 20_000,
  } = {}) {
    this.clock = clock;
    this.refreshMs = refreshMs;
    // ownerKey -> Array<entryObject>
    this.localByOwner = new Map();
    // The last aggregate received from the native hub (other peers' entries).
    this.remoteEntries = [];
    this.localListeners = new Set();
    this.remoteListeners = new Set();
    this.notifyTimer = null;
    this.refreshTimer = null;
    this.lastNotifiedKey = null;
  }

  // Replace ONE owner's local entries (empty array or null clears them).
  // Owners are module/app surfaces; the wire set is the union of all owners.
  setLocal(ownerKey, entries) {
    const key = String(ownerKey || 'default');
    const list = (Array.isArray(entries) ? entries : [])
      .filter((entry) => entry && typeof entry === 'object' && !Array.isArray(entry));
    if (list.length === 0) this.localByOwner.delete(key);
    else this.localByOwner.set(key, list);
    this.scheduleNotify();
    this.armRefreshTimer();
  }

  clearLocal(ownerKey) {
    this.setLocal(ownerKey, []);
  }

  // The union of every owner's local entries, deterministic order.
  localEntries() {
    const out = [];
    for (const list of this.localByOwner.values()) out.push(...list);
    out.sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
    return out;
  }

  // Transport hook: fires with the local entry union whenever it changes, and
  // once per refresh window while non-empty (`{ refresh: true }`) so the
  // native TTL clock keeps getting re-stamped. Fires immediately on
  // subscribe. Returns an unsubscribe function.
  onLocalChange(listener) {
    if (typeof listener !== 'function') return () => {};
    this.localListeners.add(listener);
    try { listener(this.localEntries(), { refresh: false }); } catch {}
    return () => { this.localListeners.delete(listener); };
  }

  // App hook: fires with the remote aggregate (other peers' entries) whenever
  // the native hub pushes a new one. Fires immediately on subscribe.
  onRemoteChange(listener) {
    if (typeof listener !== 'function') return () => {};
    this.remoteListeners.add(listener);
    try { listener(this.remoteEntries.slice()); } catch {}
    return () => { this.remoteListeners.delete(listener); };
  }

  // Transport hook: a `presence$` push replaces the remote aggregate
  // wholesale (the hub always sends the full set, never deltas).
  applyRemote(entries) {
    this.remoteEntries = (Array.isArray(entries) ? entries : [])
      .filter((entry) => entry && typeof entry === 'object' && !Array.isArray(entry));
    for (const listener of this.remoteListeners) {
      try { listener(this.remoteEntries.slice()); } catch {}
    }
  }

  scheduleNotify() {
    if (this.notifyTimer != null) return;
    this.notifyTimer = setTimeout(() => {
      this.notifyTimer = null;
      const entries = this.localEntries();
      const key = JSON.stringify(entries);
      if (key === this.lastNotifiedKey) return; // unchanged — skip the send
      this.lastNotifiedKey = key;
      for (const listener of this.localListeners) {
        try { listener(entries, { refresh: false }); } catch {}
      }
    }, PRESENCE_NOTIFY_DEBOUNCE_MS);
    this.notifyTimer.unref?.();
  }

  // Idle discipline: the refresh timer exists ONLY while local entries exist.
  // An idle tab with no presence publishes nothing and keeps no timer.
  armRefreshTimer() {
    const hasLocal = this.localByOwner.size > 0;
    if (!hasLocal) {
      if (this.refreshTimer != null) {
        clearInterval(this.refreshTimer);
        this.refreshTimer = null;
      }
      return;
    }
    if (this.refreshTimer != null) return;
    this.refreshTimer = setInterval(() => {
      if (this.localByOwner.size === 0) {
        clearInterval(this.refreshTimer);
        this.refreshTimer = null;
        return;
      }
      for (const listener of this.localListeners) {
        try { listener(this.localEntries(), { refresh: true }); } catch {}
      }
    }, this.refreshMs);
    this.refreshTimer.unref?.();
  }
}

// Process-wide singleton (one shared room peer per browser tab).
let SINGLETON = null;

export function getPresenceRegistry() {
  if (!SINGLETON) SINGLETON = new PresenceRegistry();
  return SINGLETON;
}

// Test hook: isolated registry with injectable clock/refresh window.
export function createPresenceRegistry(options = {}) {
  return new PresenceRegistry(options);
}
