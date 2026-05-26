// Canonical query fingerprint shared between JS and Rust.
//
// The fingerprint is a SHA-256 over a canonical JSON encoding of the
// (collection, schemaVersion, protocolVersion, selector, sort, limit, skip,
// window) tuple. Both sides MUST produce identical bytes for identical inputs.
// Verified by the corpus under src/core/rxdb/tests/fixtures/query_fingerprint/.

import { canonicalJson, sha256Hex } from './schema.mjs';

const PROTOCOL_VERSION = '1.5';

export function canonicalizeQueryInput(input) {
  if (!input || typeof input !== 'object') {
    throw new TypeError('query input must be an object');
  }
  const collection = String(input.collection || '');
  if (!collection) throw new Error('collection is required');
  const schemaVersion = Number.isFinite(Number(input.schemaVersion))
    ? Number(input.schemaVersion)
    : 0;
  return {
    collection,
    schemaVersion,
    protocolVersion: PROTOCOL_VERSION,
    selector: canonicalizeSelector(input.selector),
    sort: canonicalizeSort(input.sort),
    limit: normalizeOptionalNumber(input.limit),
    skip: normalizeOptionalNumber(input.skip),
    window: canonicalizeWindow(input.window),
  };
}

export function canonicalQueryJson(input) {
  return canonicalJson(canonicalizeQueryInput(input));
}

export async function queryFingerprint(input) {
  return sha256Hex(canonicalQueryJson(input));
}

function canonicalizeSelector(selector) {
  if (selector === undefined || selector === null) return {};
  if (typeof selector !== 'object' || Array.isArray(selector)) {
    throw new TypeError('selector must be a plain object');
  }
  return canonicalizeSelectorValue(selector);
}

function canonicalizeSelectorValue(value) {
  if (value === null) return null;
  if (Array.isArray(value)) {
    return value.map(canonicalizeSelectorValue);
  }
  if (typeof value === 'object') {
    const out = {};
    for (const key of Object.keys(value).sort()) {
      const v = canonicalizeSelectorValue(value[key]);
      if (key === '$in' || key === '$nin') {
        out[key] = sortAndDedupeArray(v);
      } else {
        out[key] = v;
      }
    }
    return out;
  }
  return value;
}

function sortAndDedupeArray(value) {
  if (!Array.isArray(value)) return value;
  const seen = new Set();
  const out = [];
  for (const item of value) {
    const key = canonicalJson(item);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(item);
  }
  out.sort((a, b) => {
    const sa = canonicalJson(a);
    const sb = canonicalJson(b);
    return sa < sb ? -1 : sa > sb ? 1 : 0;
  });
  return out;
}

function canonicalizeSort(sort) {
  if (sort === undefined || sort === null) return [];
  if (!Array.isArray(sort)) {
    throw new TypeError('sort must be an array of single-key direction objects');
  }
  return sort.map((entry) => {
    if (typeof entry !== 'object' || entry === null || Array.isArray(entry)) {
      throw new TypeError('sort entries must be single-key objects');
    }
    const keys = Object.keys(entry);
    if (keys.length !== 1) {
      throw new TypeError('sort entries must have exactly one key');
    }
    const key = keys[0];
    const direction = normalizeSortDirection(entry[key]);
    return { [key]: direction };
  });
}

function normalizeSortDirection(direction) {
  const raw = typeof direction === 'string' ? direction.toLowerCase() : direction;
  if (raw === 'desc' || raw === -1 || raw === '-1') return 'desc';
  if (raw === 'asc' || raw === 1 || raw === '1') return 'asc';
  throw new TypeError(`invalid sort direction: ${direction}`);
}

function normalizeOptionalNumber(value) {
  if (value === undefined || value === null) return null;
  const n = Number(value);
  if (!Number.isFinite(n) || n < 0) {
    throw new TypeError('optional number must be a non-negative finite value');
  }
  return Math.floor(n);
}

function canonicalizeWindow(window) {
  if (window === undefined || window === null) return null;
  if (typeof window !== 'object') {
    throw new TypeError('window must be an object');
  }
  return {
    offset: normalizeOptionalNumber(window.offset) ?? 0,
    limit: normalizeOptionalNumber(window.limit) ?? 200,
  };
}
