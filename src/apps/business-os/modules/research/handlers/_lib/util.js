// Helfer aus store.rs (4794af03d), treu portiert. Reine Funktionen.

// store.rs::first_string_field
export function firstStringField(value, keys) {
  if (!value || typeof value !== 'object') return null;
  for (const key of keys) {
    const v = value[key];
    if (typeof v === 'string' && v.trim() !== '') return v.trim();
  }
  return null;
}

// store.rs::clip_text
export function clipText(value, maxChars) {
  const collapsed = String(value ?? '').split(/\s+/u).filter(Boolean).join(' ');
  const chars = [...collapsed];
  if (chars.length <= maxChars) return collapsed;
  return `${chars.slice(0, Math.max(0, maxChars - 3)).join('')}...`;
}

// serde_json::to_vec(Value) ohne Feature preserve_order: Objekt-Schlüssel
// sortiert (BTreeMap), kompakt. UNKLAR: f64 mit ganzzahligem Wert schreibt
// serde als "1.0", JS als "1" – Hash weicht dann vom Rust-Hash ab.
export function canonicalJson(value) {
  if (value === null || typeof value !== 'object') return JSON.stringify(value ?? null);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  const keys = Object.keys(value).filter((k) => value[k] !== undefined).sort();
  return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(value[k])}`).join(',')}}`;
}

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

// SHA-256 (FIPS 180-4) über UTF-8 – ctx hat keine Krypto-API.
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
export function sha256Hex(text) {
  const bytes = utf8(text);
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
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (hh + S1 + ch + K[i] + w[i]) >>> 0;
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (S0 + maj) >>> 0;
      hh = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] += a; h[1] += b; h[2] += c; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
  }
  return [...h].map((x) => x.toString(16).padStart(8, '0')).join('');
}

// CSV wie csv::ReaderBuilder::new().flexible(false): Komma, '"' mit '""'-Escape,
// erste Zeile = Kopf, jede Zeile muss die Kopf-Feldzahl haben. Leere Zeilen
// werden (wie im csv-Crate) übersprungen.
export function parseCsv(text) {
  const records = [];
  let field = '';
  let record = [];
  let inQuotes = false;
  let i = 0;
  let sawAny = false;
  const pushRecord = () => {
    record.push(field);
    if (!(record.length === 1 && record[0] === '' && !sawAny)) records.push(record);
    record = [];
    field = '';
    sawAny = false;
  };
  const src = text.startsWith('﻿') ? text.slice(1) : text;
  while (i < src.length) {
    const c = src[i];
    if (inQuotes) {
      if (c === '"') {
        if (src[i + 1] === '"') { field += '"'; i += 2; continue; }
        inQuotes = false; i++; continue;
      }
      field += c; i++; continue;
    }
    if (c === '"') { inQuotes = true; sawAny = true; i++; continue; }
    if (c === ',') { record.push(field); field = ''; sawAny = true; i++; continue; }
    if (c === '\r' || c === '\n') {
      pushRecord();
      if (c === '\r' && src[i + 1] === '\n') i++;
      i++;
      continue;
    }
    field += c; sawAny = true; i++;
  }
  if (field !== '' || record.length > 0 || sawAny) pushRecord();
  if (records.length === 0) return { headers: [], rows: [] };
  const [headers, ...rows] = records;
  rows.forEach((row, index) => {
    if (row.length !== headers.length) {
      throw { code: 'invalid', message: `CSV error: record ${index + 1} has ${row.length} fields, but the header has ${headers.length}` };
    }
  });
  return { headers, rows };
}

// Minimaler Ersatz für `url::Url::parse` (Schema + Host). Der vm-Kontext der
// App-Laufzeit hat keine WHATWG-`URL` (geprüft: typeof URL === 'undefined');
// ist sie vorhanden, wird sie benutzt.
export function parseUrlParts(url) {
  if (typeof globalThis.URL === 'function') {
    try {
      const u = new globalThis.URL(url);
      return { scheme: u.protocol.replace(/:$/, ''), host: u.hostname };
    } catch {
      return null;
    }
  }
  const m = /^([A-Za-z][A-Za-z0-9+.-]*):(?:\/\/)?(?:[^@/?#]*@)?(\[[^\]]*\]|[^:/?#\s]*)(?::\d*)?(?:[/?#]|$)/.exec(url);
  if (!m || /\s/.test(url)) return null;
  return { scheme: m[1].toLowerCase(), host: m[2].toLowerCase() };
}
