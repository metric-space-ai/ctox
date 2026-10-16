export const CTOX_ERROR_CODES = ['revision_conflict', 'invalid', 'forbidden', 'not_found', 'conflict', 'revoked', 'expired', 'quota', 'unavailable', 'stale_base', 'migration_required', 'locally_modified', 'token_expired', 'token_used'];
export class CtoxError extends Error {
  constructor(code, message, extra = {}) {
    super(message || code);
    this.name = 'CtoxError';
    this.code = CTOX_ERROR_CODES.includes(code) ? code : 'invalid';
    if (extra.path !== undefined) this.path = extra.path;
    if (extra.reason !== undefined) this.reason = extra.reason;
    if (extra.detail !== undefined) this.detail = extra.detail;
  }
}
export class StaleLeader extends Error {}

// ---------------------------------------------------------------------------------------------------------------
// Hilfsfunktionen
// ---------------------------------------------------------------------------------------------------------------
export const hasBlob = typeof Blob !== 'undefined';
export const isBlob = (v) => hasBlob && v instanceof Blob;
export const isPlainObject = (v) => v !== null && typeof v === 'object' && (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null);

export function cloneValue(v) {
  if(v === undefined || typeof v === 'function')return v;
  return structuredClone(v);
}
export function deepFreeze(v) {
  if(typeof HTMLElement==='function' && v instanceof HTMLElement)return v;
  if (v === null || typeof v !== 'object' || Object.isFrozen(v) || isBlob(v) || ArrayBuffer.isView(v)) return v;
  Object.freeze(v);
  for (const k of Object.keys(v)) deepFreeze(v[k]);
  return v;
}
export function deepEqual(a, b) {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== 'object' || typeof b !== 'object') return Number.isNaN(a) && Number.isNaN(b);
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) return a.length === b.length && a.every((x, i) => deepEqual(x, b[i]));
  const ka = Object.keys(a), kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && deepEqual(a[k], b[k]));
}
export function jsonEq(a, b) {
  if (a === b) return true;
  try { return JSON.stringify(a) === JSON.stringify(b); } catch { return false; }
}
export function liveEqual(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => x === b[i] || jsonEq(x, b[i]));
  return jsonEq(a, b);
}
// RFC 7396 JSON-Merge-Patch, mutiert nichts.
export function mergePatch(target, patch) {
  if (!isPlainObject(patch)) return cloneValue(patch);
  const out = isPlainObject(target) ? { ...target } : {};
  for (const [k, v] of Object.entries(patch)) {
    if (v === null) delete out[k];
    else Object.defineProperty(out,k,{value:mergePatch(Object.prototype.hasOwnProperty.call(out,k)?out[k]:undefined,v),enumerable:true,writable:true,configurable:true});
  }
  return out;
}
export function fnv1a(str) {
  let h = 0x811c9dc5;
  for (let i = 0; i < str.length; i++) { h ^= str.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h.toString(16).padStart(8, '0');
}
function randomBytes(n) {
  const b = new Uint8Array(n);
  if (globalThis.crypto?.getRandomValues) globalThis.crypto.getRandomValues(b);
  else for (let i = 0; i < n; i++) b[i] = Math.floor(Math.random() * 256);
  return b;
}
let uuidLastMs = 0, uuidCounter = 0;
// UUIDv7 (RFC 9562), monoton je Realm: 48 Bit ms, 12 Bit Zähler, 62 Bit Zufall. Lexikografisch = zeitlich sortiert.
export function uuidv7() {
  let ms = Date.now();
  if (ms <= uuidLastMs) {
    ms = uuidLastMs;
    uuidCounter++;
    if (uuidCounter > 0xfff) { ms = ++uuidLastMs; uuidCounter = 0; }
  } else { uuidLastMs = ms; uuidCounter = 0; }
  const b = randomBytes(16);
  for (let i = 5; i >= 0; i--) { b[i] = ms % 256; ms = Math.floor(ms / 256); }
  b[6] = 0x70 | ((uuidCounter >> 8) & 0x0f);
  b[7] = uuidCounter & 0xff;
  b[8] = 0x80 | (b[8] & 0x3f);
  const h = Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

// SHA-256: crypto.subtle, sonst reines JS (z. B. unsichere Kontexte).
const SHA_K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
  0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
  0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
  0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
  0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
  0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
export function sha256Js(bytes) {
  const len = bytes.length;
  const total = ((len + 9 + 63) >> 6) << 6;
  const buf = new Uint8Array(total);
  buf.set(bytes);
  buf[len] = 0x80;
  const dv = new DataView(buf.buffer);
  dv.setUint32(total - 4, (len * 8) >>> 0);
  dv.setUint32(total - 8, Math.floor((len * 8) / 0x100000000));
  const H = new Uint32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]);
  const w = new Uint32Array(64);
  const ror = (x, n) => (x >>> n) | (x << (32 - n));
  for (let off = 0; off < total; off += 64) {
    for (let i = 0; i < 16; i++) w[i] = dv.getUint32(off + i * 4);
    for (let i = 16; i < 64; i++) {
      const a = w[i - 15], b = w[i - 2];
      w[i] = (w[i - 16] + (ror(a, 7) ^ ror(a, 18) ^ (a >>> 3)) + w[i - 7] + (ror(b, 17) ^ ror(b, 19) ^ (b >>> 10))) >>> 0;
    }
    let [a, b, c, d, e, f, g, h] = H;
    for (let i = 0; i < 64; i++) {
      const t1 = (h + (ror(e, 6) ^ ror(e, 11) ^ ror(e, 25)) + ((e & f) ^ (~e & g)) + SHA_K[i] + w[i]) >>> 0;
      const t2 = ((ror(a, 2) ^ ror(a, 13) ^ ror(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) >>> 0;
      h = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    H[0] += a; H[1] += b; H[2] += c; H[3] += d; H[4] += e; H[5] += f; H[6] += g; H[7] += h;
  }
  return Array.from(H, (x) => x.toString(16).padStart(8, '0')).join('');
}
async function toBytes(content) {
  if (typeof content === 'string') return new TextEncoder().encode(content);
  if (isBlob(content)) return new Uint8Array(await content.arrayBuffer());
  if (content instanceof ArrayBuffer) return new Uint8Array(content);
  if (ArrayBuffer.isView(content)) return new Uint8Array(content.buffer, content.byteOffset, content.byteLength);
  throw new CtoxError('invalid', 'Inhalt muss string, Blob oder Bytes sein');
}
export async function sha256Hex(content) {
  const bytes = await toBytes(content);
  if (globalThis.crypto?.subtle?.digest) {
    try {
      const d = await globalThis.crypto.subtle.digest('SHA-256', bytes);
      return Array.from(new Uint8Array(d), (x) => x.toString(16).padStart(2, '0')).join('');
    } catch { /* unsicherer Kontext → JS */ }
  }
  return sha256Js(bytes);
}

// ---------------------------------------------------------------------------------------------------------------
