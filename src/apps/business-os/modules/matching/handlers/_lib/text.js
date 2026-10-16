// Text-Helfer aus importer.rs (4794af03d), treu portiert. Rust-`regex` ist
// Unicode-bewusst: \d = \p{Nd}, \b = Unicode-Wortgrenze; nachgebaut mit /u.

const W = '[\\p{Alphabetic}\\p{M}\\p{Nd}\\p{Pc}\\p{Join_Control}]';
export const B = `(?:(?<=${W})(?!${W})|(?<!${W})(?=${W}))`;
export const D = '\\p{Nd}';

// UTF-8-Kodierung ohne TextEncoder (vm-Kontext der App-Laufzeit bietet nur
// ECMAScript-Builtins; TextEncoder ist eine Web-API).
function utf8(text) {
  const out = [];
  for (const ch of text) {
    let cp = ch.codePointAt(0);
    if (cp >= 0xd800 && cp <= 0xdfff) cp = 0xfffd;
    if (cp < 0x80) out.push(cp);
    else if (cp < 0x800) out.push(0xc0 | (cp >> 6), 0x80 | (cp & 63));
    else if (cp < 0x10000) out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
    else out.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 63), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
  }
  return new Uint8Array(out);
}
// Rust `String::len()` = UTF-8-Bytes.
export function byteLen(value) {
  return utf8(value).length;
}

export function charCount(value) {
  return [...value].length;
}

// Rust `str::lines()`: trennt an \n, entfernt ein \r davor, keine leere Schlusszeile.
export function lines(text) {
  if (text === '') return [];
  const parts = text.split('\n');
  if (parts[parts.length - 1] === '') parts.pop();
  return parts.map((line) => (line.endsWith('\r') ? line.slice(0, -1) : line));
}

// importer.rs::clean_text
export function cleanText(text) {
  return String(text).replaceAll(' ', ' ').split(/\s+/u).filter(Boolean).join(' ').trim();
}

// importer.rs::normalize_lines
export function normalizeLines(text) {
  return lines(text).map(cleanText).filter((line) => line !== '').join('\n');
}

// importer.rs::truncate_chars
export function truncateChars(value, maxChars) {
  const chars = [...value];
  const out = chars.slice(0, maxChars).join('');
  return chars.length > maxChars ? `${out}...` : out;
}

// importer.rs::regex_capture – Gruppe 1, sonst Gesamttreffer; clean_text; nicht leer.
export function regexCapture(text, regex) {
  const m = regex.exec(text);
  if (!m) return null;
  const value = cleanText(m[1] !== undefined ? m[1] : m[0]);
  return value ? value : null;
}

// importer.rs::str_path
export function strPath(value, path) {
  let current = value;
  for (const key of path) {
    if (current === null || typeof current !== 'object' || !(key in current)) return '';
    current = current[key];
  }
  return typeof current === 'string' ? current.trim() : '';
}

// importer.rs::first_nonempty
export function firstNonempty(values) {
  return values.map((v) => v.trim()).find((v) => v !== '') ?? '';
}

// importer.rs::now_iso – RFC 3339, Sekunden, "Z".
export function nowIso(ms) {
  return new Date(ms).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

// SHA-256 über Bytes (ctx hat keine Krypto-API).
const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
export function sha256Bytes(input) {
  const bytes = typeof input === 'string' ? utf8(input) : input;
  const bitLen = bytes.length * 8;
  const padded = new Uint8Array(((bytes.length + 9 + 63) >> 6) << 6);
  padded.set(bytes);
  padded[bytes.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, Math.floor(bitLen / 2 ** 32));
  view.setUint32(padded.length - 4, bitLen >>> 0);
  const h = new Uint32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]);
  const w = new Uint32Array(64);
  const rotr = (x, n) => (x >>> n) | (x << (32 - n));
  for (let off = 0; off < padded.length; off += 64) {
    for (let i = 0; i < 16; i++) w[i] = view.getUint32(off + i * 4);
    for (let i = 16; i < 64; i++) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let i = 0; i < 64; i++) {
      const t1 = (hh + (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) + ((e & f) ^ (~e & g)) + K[i] + w[i]) >>> 0;
      const t2 = ((rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) >>> 0;
      hh = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] += a; h[1] += b; h[2] += c; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
  }
  const out = new Uint8Array(32);
  const ov = new DataView(out.buffer);
  h.forEach((x, i) => ov.setUint32(i * 4, x));
  return out;
}

export function hexSha256(input) {
  return [...sha256Bytes(input)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
// importer.rs::short_hash – base64url ohne Padding, erste 10 Zeichen.
export function shortHash(value) {
  const bytes = sha256Bytes(value);
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const n = (bytes[i] << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63];
    if (i + 1 < bytes.length) out += B64[(n >> 6) & 63];
    if (i + 2 < bytes.length) out += B64[n & 63];
  }
  return out.replaceAll('+', '-').replaceAll('/', '_').slice(0, 10);
}

// BASE64_STANDARD.decode (strikt: Alphabet + Padding).
export function decodeBase64(text) {
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(text) || text.length % 4 === 1) return null;
  const clean = text.replace(/=+$/, '');
  const out = [];
  let buffer = 0;
  let bits = 0;
  for (const ch of clean) {
    buffer = (buffer << 6) | B64.indexOf(ch);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out.push((buffer >> bits) & 0xff);
    }
  }
  return new Uint8Array(out);
}

// importer.rs::slug
export function slug(value) {
  const ascii = value.toLowerCase().replaceAll('ä', 'ae').replaceAll('ö', 'oe').replaceAll('ü', 'ue').replaceAll('ß', 'ss');
  let out = '';
  let lastDash = false;
  for (const ch of ascii) {
    if (/^[A-Za-z0-9]$/.test(ch)) {
      out += ch;
      lastDash = false;
    } else if (!lastDash) {
      out += '-';
      lastDash = true;
    }
  }
  out = out.replace(/^-+|-+$/g, '');
  return out === '' ? `record-${shortHash(value)}` : [...out].slice(0, 96).join('');
}
