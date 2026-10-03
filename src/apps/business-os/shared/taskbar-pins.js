const CACHE_VERSION = 2;

function cleanPins(value) {
  if (!Array.isArray(value)) return [];
  return value
    .map((id) => String(id || '').trim())
    .filter((id, index, pins) => id && pins.indexOf(id) === index);
}

function pinsArePresent(value) {
  return Array.isArray(value)
    || Boolean(value && typeof value === 'object' && Array.isArray(value.pins));
}

export function decodeTaskbarPinCache(raw) {
  let parsed = raw;
  if (typeof raw === 'string') {
    try {
      parsed = JSON.parse(raw);
    } catch {
      parsed = null;
    }
  }
  if (!pinsArePresent(parsed)) {
    return { pins: [], updatedAtMs: 0, legacy: false, present: false };
  }
  if (Array.isArray(parsed)) {
    return { pins: cleanPins(parsed), updatedAtMs: 0, legacy: true, present: true };
  }
  return {
    pins: cleanPins(parsed.pins),
    updatedAtMs: Math.max(0, Number(parsed.updated_at_ms || parsed.updatedAtMs || 0) || 0),
    legacy: Number(parsed.version || 0) < CACHE_VERSION,
    present: true,
  };
}

export function encodeTaskbarPinCache(pins, updatedAtMs = Date.now()) {
  return JSON.stringify({
    version: CACHE_VERSION,
    pins: cleanPins(pins),
    updated_at_ms: Math.max(0, Number(updatedAtMs || 0) || 0),
  });
}

export function resolveTaskbarPinState({
  localPins,
  localUpdatedAtMs = 0,
  localPresent,
  remotePins,
  remoteUpdatedAtMs = 0,
}) {
  const local = {
    pins: cleanPins(localPins),
    updatedAtMs: Math.max(0, Number(localUpdatedAtMs || 0) || 0),
    source: 'local',
  };
  const remote = {
    pins: cleanPins(remotePins),
    updatedAtMs: Math.max(0, Number(remoteUpdatedAtMs || 0) || 0),
    source: 'remote',
  };
  const hasLocal = typeof localPresent === 'boolean'
    ? localPresent
    : localPins !== null && localPins !== undefined;
  // `remotePins: []` is an authoritative empty selection, not a missing layout.
  const hasRemote = remotePins !== null && remotePins !== undefined;
  if (!hasRemote) return local;
  if (!hasLocal) return remote;
  if (local.updatedAtMs > remote.updatedAtMs) return local;
  return remote;
}
