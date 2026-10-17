// Test-Hilfen für die App-eigene Scrape-Registry (Strang OLGSCR): files-Attrappe und
// process.run-Attrappe über die generischen OS-Primitive, plus Registrierung eines Ziels.
import { createTestCtx } from './_ctx.mjs';
import { registerScript, upsertTarget } from '../_lib/scrape-registry.js';

// Attrappen nach CAP-3b #844 (caps/files.rs, caps/process.rs); der Stub 74137b1 kennt ctx.cap nicht.
//   files.put({name, mime, text|base64}) → {id, ref}       ref = sha256(Inhalt), neuer Datensatz je put
//   files.get({ref}|{id}) → {id, ref, name, mime, size, owner_app, created_at_ms, text?}; fehlt → not_found
//   process.run({program, args?, cwd_ref?, stdin?|stdin_ref?, env?, timeout_ms?}) (§16.48 siehe unten)
//     → {exit, timed_out, duration_ms, stdout, stdout_truncated, stderr, stderr_truncated}
const FILES_CONTRACT_KEYS = { put: ['name', 'mime', 'text', 'base64'], get: ['id', 'ref'] };
const PROCESS_KEYS = ['program', 'args', 'cwd_ref', 'stdin', 'stdin_ref', 'env', 'timeout_ms'];
const contractError = (code, message) => Object.assign(new Error(message), { code });
const rejectUnknown = (args, allowed, cap) => {
  const extra = Object.keys(args ?? {}).filter((k) => !allowed.includes(k));
  if (extra.length) throw contractError('invalid', `${cap}: unknown field(s) ${extra.join(', ')} (deny_unknown_fields)`);
};
const sha256 = async (text) => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text))),
  (b) => b.toString(16).padStart(2, '0')).join('');

/** In-memory `files` nach Vertrag. `texts`/`names` sind nach ref geführt (Testzugriff). */
export function fileStore() {
  const texts = new Map();
  const names = new Map();
  const byId = new Map();
  let n = 0;
  const store = {
    texts,
    names,
    unavailable: false, // true = files nicht gebunden (bis Cutover): jeder Aufruf → unavailable
    tamper: null, // (name, text) => text — simulates a changed stored artifact
    caps: {
      'files.put': async (args) => {
        if (store.unavailable) throw contractError('unavailable', 'files collection not bound');
        rejectUnknown(args, FILES_CONTRACT_KEYS.put, 'files.put');
        if (typeof args?.name !== 'string' || /[\\/]/.test(args.name) || (args.text == null) === (args.base64 == null)) {
          throw contractError('invalid', 'files.put: name and exactly one of text|base64');
        }
        const text = args.text ?? atob(args.base64);
        const ref = await sha256(text);
        const id = `file_${++n}`;
        texts.set(ref, text);
        names.set(ref, args.name);
        byId.set(id, ref);
        return { id, ref };
      },
      'files.get': async (args) => {
        if (store.unavailable) throw contractError('unavailable', 'files collection not bound');
        rejectUnknown(args, FILES_CONTRACT_KEYS.get, 'files.get');
        if ((args?.id == null) === (args?.ref == null)) throw contractError('invalid', 'files.get: exactly one of id|ref');
        const ref = args.ref ?? byId.get(args.id);
        if (!texts.has(ref)) throw contractError('not_found', 'file not found');
        const text = store.tamper ? store.tamper(names.get(ref) ?? '', texts.get(ref)) : texts.get(ref);
        return { id: args.id ?? null, ref, name: names.get(ref) ?? '', mime: 'application/octet-stream',
          size: new TextEncoder().encode(text).length, owner_app: 'outbound-lead-generation', created_at_ms: 0, text };
      },
    },
    byName(fragment) {
      for (const [ref, name] of names) if (name.includes(fragment)) return JSON.parse(texts.get(ref));
      return undefined;
    },
  };
  return store;
}

// process.run nach §16.48 Nr. 3 (Kern-Entscheid zu CAP-3d): Allowlist-Einträge mit untrusted_code:true laufen nur in der
// Sandbox E20, ohne Sandbox → unavailable. Zugelassen olg-scrape-js (node + runtime/host.mjs), olg-scrape-ts
// (tsx + runtime/host.ts); olg-scrape-sh abgelehnt. stdin ≤ 1 MiB oder stdin_ref (genau eins). Die Grundumgebung
// PATH/CTOX_BIN/LANG setzt die Plattform; ein env der App darf sie nicht ersetzen.
export const ALLOWLIST = Object.freeze({
  'olg-scrape-js': { interpreter: 'node', script: 'runtime/host.mjs', untrusted_code: true },
  'olg-scrape-ts': { interpreter: 'tsx', script: 'runtime/host.ts', untrusted_code: true },
});
const RESERVED_ENV = ['PATH', 'CTOX_BIN', 'LANG'];

/** stdin-Dokument eines process.run-Aufrufs (direkt oder über stdin_ref aus der files-Attrappe). */
export function hostDoc(fs, args) {
  const text = typeof args.stdin === 'string' ? args.stdin : fs.texts.get(args.stdin_ref);
  return JSON.parse(text);
}

/** process.run attrappe: spec = {exit, stdout, stderr, timed_out} or async (args, host) => spec. `sandbox` = {available}. */
export function processCap(fs, spec, calls = [], sandbox = { available: true }) {
  return async (args) => {
    rejectUnknown(args, PROCESS_KEYS, 'process.run');
    const entry = ALLOWLIST[args?.program];
    if (!entry) throw contractError('forbidden', `program ${args?.program} is not allowlisted`);
    if ((args.stdin == null) === (args.stdin_ref == null)) throw contractError('invalid', '$.stdin: exactly one of stdin|stdin_ref');
    if (typeof args.stdin === 'string' && new TextEncoder().encode(args.stdin).length > 1024 * 1024) throw contractError('invalid', '$.stdin');
    if (args.stdin_ref != null && !fs.texts.has(args.stdin_ref)) throw contractError('not_found', '$.stdin_ref');
    for (const [key, value] of Object.entries(args.env ?? {})) {
      if (RESERVED_ENV.includes(key)) throw contractError('invalid', `$.env.${key}: platform base environment`);
      if (typeof value !== 'string' || new TextEncoder().encode(value).length > 4096) throw contractError('invalid', '$.env');
    }
    if (entry.untrusted_code && !sandbox.available) throw contractError('unavailable', 'sandbox E20 not available for untrusted_code');
    calls.push(args);
    const out = typeof spec === 'function' ? await spec(args, hostDoc(fs, args)) : spec;
    return {
      exit: out.exit ?? 0, timed_out: out.timed_out === true, duration_ms: 1,
      stdout: typeof out.stdout === 'string' ? out.stdout : JSON.stringify(out.stdout ?? ''), stdout_truncated: false,
      stderr: out.stderr ?? '', stderr_truncated: false,
    };
  };
}

/** ctx with files + process.run; `caps` may add more. Returns {t, fs, procCalls}. */
export function scrapeCtx({ now = 1_000, caps = {}, data = {}, run = { exit: 0, stdout: [] }, sandbox = { available: true } } = {}) {
  const fs = fileStore();
  const procCalls = [];
  const all = { ...fs.caps, 'process.run': processCap(fs, run, procCalls, sandbox), ...caps };
  const t = createTestCtx({ now, caps: all, data });
  return { t, fs, procCalls, caps: all, sandbox, host: (call) => hostDoc(fs, call) };
}

/** Register target + script through the app registry. */
export async function seedTarget(ctx, { target_key: targetKey = 'xing', start_url: startUrl = 'https://www.xing.com/', script = 'console.log(JSON.stringify([]));\n', config = {} } = {}) {
  await upsertTarget(ctx, { target_key: targetKey, display_name: targetKey.toUpperCase(), start_url: startUrl, target_kind: 'prospect-research', config });
  if (script != null) await registerScript(ctx, { target_key: targetKey, script_text: script });
}

export const PROSPECT_RECORDS = [
  { field: 'NAME', value: 'ACME GmbH', confidence: 0.9, source_url: 'https://www.xing.com/acme', note: '' },
  { field: 'city', value: 'Köln', confidence: 0.8, source_url: 'https://www.xing.com/acme', note: '' },
  { field: 'empty', value: '  ', confidence: 0.1, source_url: 'https://www.xing.com/acme', note: '' },
];
