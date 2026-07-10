const HLC_NODE_STORAGE_KEY = 'ctox.businessOs.hlcNodeId.v1';
let cachedNodeId = null;

export function hybridLogicalClockNodeId() {
  if (cachedNodeId) return cachedNodeId;
  try {
    const stored = globalThis.localStorage?.getItem?.(HLC_NODE_STORAGE_KEY);
    if (stored) return (cachedNodeId = sanitizeNodeId(stored));
  } catch {}
  const generated = sanitizeNodeId(
    globalThis.crypto?.randomUUID?.() || `browser-${Math.random().toString(36).slice(2, 14)}`,
  );
  cachedNodeId = generated;
  try { globalThis.localStorage?.setItem?.(HLC_NODE_STORAGE_KEY, generated); } catch {}
  return generated;
}

export function nextHybridLogicalClock(previous, {
  nowMs = Date.now(),
  nodeId = hybridLogicalClockNodeId(),
} = {}) {
  const prior = parseHybridLogicalClock(previous);
  const wall = Math.max(0, Math.floor(Number(nowMs) || 0));
  const physicalMs = Math.max(wall, prior?.physicalMs || 0);
  const logical = prior && physicalMs === prior.physicalMs ? prior.logical + 1 : 0;
  return formatHybridLogicalClock({ physicalMs, logical, nodeId });
}

export function compareHybridLogicalClocks(left, right) {
  const a = parseHybridLogicalClock(left);
  const b = parseHybridLogicalClock(right);
  if (!a && !b) return 0;
  if (!a) return -1;
  if (!b) return 1;
  if (a.physicalMs !== b.physicalMs) return a.physicalMs < b.physicalMs ? -1 : 1;
  if (a.logical !== b.logical) return a.logical < b.logical ? -1 : 1;
  return a.nodeId.localeCompare(b.nodeId);
}

export function parseHybridLogicalClock(value) {
  const match = /^([0-9a-z]+):([0-9a-z]+):([0-9a-z_-]+)$/i.exec(String(value || ''));
  if (!match) return null;
  const physicalMs = Number.parseInt(match[1], 36);
  const logical = Number.parseInt(match[2], 36);
  if (!Number.isSafeInteger(physicalMs) || !Number.isSafeInteger(logical)) return null;
  return { physicalMs, logical, nodeId: sanitizeNodeId(match[3]) };
}

export function formatHybridLogicalClock({ physicalMs, logical = 0, nodeId = 'native' }) {
  return `${Math.max(0, Math.floor(physicalMs)).toString(36)}:${Math.max(0, Math.floor(logical)).toString(36)}:${sanitizeNodeId(nodeId)}`;
}

function sanitizeNodeId(value) {
  return String(value || 'unknown').toLowerCase().replace(/[^0-9a-z_-]/g, '').slice(0, 48) || 'unknown';
}
