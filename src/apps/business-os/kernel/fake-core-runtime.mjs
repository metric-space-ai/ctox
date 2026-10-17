import {commandRoute} from './command-routing.mjs';
// Fake-Core-Laufzeit (K-1c Punkte 4–7): Nutzer und Rechte, Handler-Laufzeit im Prozess (§16.16/§16.25),
// Trigger-Lauf (§16.20, fremde Collections nur nach §16.34 Nr. 3), Katalog-Primitive audit/agent.task (§16.29/§16.30)
// und die Plattformbefehle ctox.app.publish|update|rollback|release|remove (§16.29 B, §16.33).
// Nur für MemoryCoreTransport (Tests, Shell-Entwicklung). Der echte Core macht das in Rust.
import { CtoxError, cloneValue, isPlainObject, mergePatch, uuidv7, sha256Hex, fnv1a } from './values.mjs';
import { compileQuery, runCompiledQuery, validateSchema } from './app-api.mjs';
import { bytesBase64 } from './artifact-url.mjs';
import { PLATFORM_COLLECTIONS, PLATFORM_APP_ID } from './platform.mjs';
import { SCHEMA_FILE, parseCollectionsFile, classifyCollections, manifestCollectionsToParsed, parsedToManifestCollections } from './schema-diff.mjs';

/** Katalog-Primitive, die der Fake-Core als ctx.cap anbietet (Aktivierung prüft capabilities_required dagegen). */
export const FAKE_CORE_CAPABILITIES = Object.freeze(['audit.event', 'agent.task.enqueue', 'agent.task.get', 'agent.task.find', 'agent.task.cancel', 'model', 'model.run', 'files', 'files.put', 'files.get', 'files.list', 'files.delete', 'process', 'process.run']);
// Zeilenrechte der Plattform-Collections (docs/v4-manifests/ctox-platform.manifest.json, Auszug).
const PLATFORM_RIGHTS = Object.freeze({
  app_user_preferences: { read: 'own', write: 'own' },
  shell_preferences: { read: 'own', write: 'own' },
  app_drafts: { read: 'own+apps.source.view', write: 'own+apps.source.edit' },
  business_apps: { read: 'all', write: 'core' },
  business_app_versions: { read: 'all', write: 'core' },
  app_catalog: { read: 'all', write: 'core' },
  app_archive: { read: 'all', write: 'core' },
  ctox_queue_tasks: { read: 'all', write: 'core' },
});
const MANIFEST_FIELDS_COMPUTED = new Set(['app_id', 'app_ver', 'files', 'generations', 'retires', 'provenance', 'author', 'created_at', 'title', 'entry', 'handlers', 'capabilities_required', 'min_app_api', 'layout', 'styles', 'collections']);
const MAX_ASSET = 1024 * 1024;
const MAX_TRIGGER_DEPTH = 3;
const baseName = (ns) => String(ns).replace(/@g\d+$/, '');
const nsOf = (c, gen) => (gen > 1 ? `${c}@g${gen}` : c);
const asText = (c) => (typeof c === 'string' ? c : new TextDecoder().decode(c));
const fail = (code, message) => { throw new CtoxError(code, message); };

function isoMs(d) {
  const m = /^PT(?:(\d+)H)?(?:(\d+)M)?(?:(\d+(?:\.\d+)?)S)?$/.exec(String(d ?? ''));
  if (!m) return null;
  return ((Number(m[1] ?? 0) * 60 + Number(m[2] ?? 0)) * 60 + Number(m[3] ?? 0)) * 1000;
}
function joinPath(dir, rel) {
  const out = dir ? dir.split('/') : [];
  for (const seg of rel.split('/')) { if (seg === '.' || seg === '') continue; if (seg === '..') { if (!out.length) fail('invalid', `Import außerhalb des Pakets: ${rel}`); out.pop(); } else out.push(seg); }
  return out.join('/');
}
const SYSTEM_ADMIN = Object.freeze({ id: 'admin', role: 'admin', permissions: [] });
const hasPerm = (user, p) => user?.role === 'admin' || (user?.permissions ?? []).includes(p);

export function createCoreRuntime(core) {
  const key = (c, id) => JSON.stringify([c, id]);
  const data = (c, id) => { const r = core.docs.get(key(c, id)); return r && !r.deleted ? r.data : null; };
  const rowsOf = (c) => [...core.docs.values()].filter((r) => r.collection === c && !r.deleted && r.data);
  const activeVer = (appId) => data('business_apps', appId)?.active_ver ?? null;
  const manifestOf = (appId, ver) => (ver ? data('business_app_versions', `${appId}@${ver}`)?.manifest ?? null : null);
  const activeManifest = (appId) => manifestOf(appId, activeVer(appId));
  // Cache der aktiven Apps und der Collection-Eigentümer; jeder Commit auf business_apps/_versions verwirft ihn.
  let appCache = null;
  function apps() {
    if (appCache) return appCache;
    const ids = rowsOf('business_apps').map((r) => r.data.id).filter((id) => !id.startsWith('lib:'));
    const owner = new Map(), watchers = new Map();
    for (const id of ids) {
      const m = activeManifest(id);
      for (const c of Object.keys(m?.collections ?? {})) if (!owner.has(c)) owner.set(c, { id, rights: m.collections[c].rights ?? null });
      for (const t of m?.triggers ?? []) if (t?.on?.collection) { if (!watchers.has(t.on.collection)) watchers.set(t.on.collection, []); watchers.get(t.on.collection).push(id); }
    }
    appCache = { ids, owner, watchers };
    return appCache;
  }
  const activeApps = () => apps().ids;
  const tasks = new Map(), taskByKey = new Map(), callbacks = new Set();
  const modules = new Map();
  const everyTimers = new Map();
  const running = new Set();
  const log = [];
  let agentOutcome = null;

  // ---- Punkt 4: Rechte (Zeilen, Collections, Befehle) ----
  function rightsOf(ns) {
    const c = baseName(ns);
    if (PLATFORM_COLLECTIONS.includes(c) || PLATFORM_RIGHTS[c]) return PLATFORM_RIGHTS[c] ?? { read: 'all', write: 'all' };
    return apps().owner.get(c)?.rights ?? { read: 'all', write: 'all' };
  }
  function ownRow(c, user, id, doc) {
    if (c === 'shell_preferences') return id === user.id;
    if (c === 'app_drafts') return typeof id === 'string' && id.startsWith(`${user.id}/`);
    return [doc?.owner_id, doc?.user_id, doc?.actor_id].includes(user.id);
  }
  // Regel: Tokens mit „+“ verknüpft (alle müssen gelten): all | core | own | admin | role:<r> | app:<id> | <recht.x>
  function allowed(rule, user, { c, id, doc, appId, read }) {
    if (rule == null || rule === 'all' || rule === '*') return true;
    if (typeof rule !== 'string') return true;
    return rule.split('+').every((tok) => {
      if (tok === 'all' || tok === '*') return true;
      if (tok === 'core') return false;
      if (tok === 'own') return ownRow(c, user, id, doc) || (read && c === 'app_drafts' && hasPerm(user, 'apps.modify'));
      if (tok === 'admin') return user.role === 'admin';
      if (tok.startsWith('role:')) return user.role === 'admin' || user.role === tok.slice(5);
      if (tok.startsWith('app:')) return appId === tok.slice(4);
      return hasPerm(user, tok);
    });
  }
  function canRead(user, row) {
    if (!user) return true;
    const r = rightsOf(row.collection).read;
    return allowed(r, user, { c: baseName(row.collection), id: row.id, doc: row.data, read: true });
  }
  /** null = erlaubt, sonst Ablehnungscode. */
  function checkWrite(user, entry, old) {
    const c = baseName(entry.collection);
    if(c==='business_commands' && entry.op==='put' && entry.body?.status==='queued') {
      const route=commandRoute(entry.body,activeManifest,user,entry.test_route===true);
      if(route.error)return {code:route.error,reason:route.error};
    }
    // Plattformbefehle prüft der Core immer (CAS, Codes §16.33); ohne bekannten Nutzer gilt der Client als Admin (Bind-Aufbau).
    if (c === 'business_commands' && entry.op === 'put' && entry.body?.status === 'queued' && String(entry.body.type).startsWith('ctox.app.')) return checkPlatformCommand(user ?? SYSTEM_ADMIN, entry.body);
    if (c === 'business_commands' && entry.op === 'put' && entry.body?.status === 'queued' && String(entry.body.type).startsWith('ctox.coding.')) return checkCodingCommand(user ?? { ...SYSTEM_ADMIN, id: entry.body.actor_id ?? 'admin' }, entry.body);
    if (!user) return null;
    if (c === 'business_commands') {
      const body = entry.body ?? old?.data;
      if (entry.op === 'put' && body?.status === 'queued') {
        if (String(body.type).startsWith('ctox.app.')) return checkPlatformCommand(user, body);
      }
      return null;
    }
    const w = rightsOf(entry.collection).write;
    const doc = entry.op === 'delete' ? old?.data : entry.op === 'patch' ? mergePatch(old?.data ?? {}, entry.body ?? {}) : entry.body;
    return allowed(w, user, { c, id: entry.id, doc, appId: entry.app_id }) ? null : 'forbidden';
  }

  // ---- Schreiben im Core (Handler, Trigger, Plattformbefehle) ----
  async function commit(collection, docs, depth = 0) {
    await core.commit(collection, docs, depth);
  }

  // ---- Punkt 6: Handler-Laufzeit ----
  function fileText(manifest, path) {
    const hash = manifest?.files?.[path];
    if (!hash) fail('not_found', `${path} nicht im Paket ${manifest?.app_id}@${manifest?.app_ver}`);
    const c = core.artifacts.get(hash);
    if (c == null) fail('not_found', `Artefakt ${hash} (${path}) fehlt im Core`);
    return asText(c);
  }
  const SPEC_RE = /(\bfrom\s*|\bimport\s*\(\s*|\bimport\s*)(['"])(\.{1,2}\/[^'"\r\n]+)\2/g;
  function toUrl(text) {
    if (typeof window !== 'undefined' && typeof URL.createObjectURL === 'function' && typeof Blob === 'function') return URL.createObjectURL(new Blob([text], { type: 'text/javascript' }));
    return `data:text/javascript;base64,${bytesBase64(new TextEncoder().encode(text))}`;
  }
  // Relative Importe innerhalb des Pakets (§16.19 Nr. 8) werden auf die URLs der Abhängigkeiten umgeschrieben.
  function moduleUrl(manifest, path, stack = []) {
    const k = `${manifest.app_id}@${manifest.app_ver}:${path}`;
    if (modules.has(k)) return modules.get(k);
    if (stack.includes(path)) fail('invalid', `Importzyklus: ${[...stack, path].join(' → ')}`);
    const dir = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
    const text = fileText(manifest, path).replace(SPEC_RE, (all, lead, q, spec) => `${lead}${q}${moduleUrl(manifest, joinPath(dir, spec), [...stack, path])}${q}`);
    const url = toUrl(text);
    modules.set(k, url);
    return url;
  }
  async function handlerFn(manifest, path) {
    const mod = await import(/* @vite-ignore */ moduleUrl(manifest, path));
    if (typeof mod.default !== 'function') fail('invalid', `${path}: kein default export handle(command, ctx)`);
    return mod.default;
  }
  // ---- K-1g: model.run (#830 ctx.model → 'model.run', CATALOG §6) – deterministische simulierte Antwort ----
  function modelRun(appId, req) {
    if (!isPlainObject(req)) fail('invalid', 'model.run: Anfrage-Objekt nötig');
    const input = req.input ?? req.prompt ?? req.messages;
    const text = typeof input === 'string' ? input
      : Array.isArray(input) ? input.map((m) => (typeof m === 'string' ? m : typeof m?.content === 'string' ? m.content : Array.isArray(m?.content) ? m.content.map((c) => c?.text ?? '').join(' ') : '')).join('\n')
      : fail('invalid', 'model.run: input, prompt oder messages nötig');
    const model = typeof req.model === 'string' && req.model ? req.model : 'fake-core';
    const out = `Simulierte Modellantwort (${model}) für ${appId}: ${text.replace(/\s+/g, ' ').trim().slice(0, 200)}`;
    return {
      id: `resp_${fnv1a(JSON.stringify([appId, model, text]))}`, object: 'response', model, status: 'completed',
      output: [{ type: 'message', role: 'assistant', content: [{ type: 'output_text', text: out }] }],
      output_text: out, usage: { input_tokens: text.split(/\s+/).filter(Boolean).length, output_tokens: out.split(/\s+/).length },
    };
  }
  // ---- K-1g: files.* (CAP-3b #844) im Handler – nur Dateien der eigenen App, Ergebnisse immer Objekte, nie Base64 ----
  const FILE_TEXT_MAX = 512 * 1024;
  function ownCoreFile(appId, sel) {
    if (!isPlainObject(sel) || (typeof sel.id !== 'string' && typeof sel.ref !== 'string')) fail('invalid', 'files: {id} oder {ref} nötig');
    const docs = rowsOf('files').map((r) => r.data).filter((d) => (sel.id ? d.id === sel.id : d.ref === sel.ref));
    const mine = docs.find((d) => d.owner_app === appId);
    if (!mine && docs.length) fail('forbidden', 'Datei gehört einer anderen App');
    return mine ?? null;
  }
  const coreFileInfo = (d) => ({ id: d.id, ref: d.ref, name: d.name ?? null, mime: d.mime ?? 'application/octet-stream', size: d.size ?? 0 });
  function filesCap(appId, command, depth) {
    return {
      async put(a = {}) {
        if (!isPlainObject(a)) fail('invalid', 'files.put: Objekt nötig');
        const bytes = typeof a.text === 'string' ? new TextEncoder().encode(a.text) : a.bytes instanceof Uint8Array ? a.bytes : fail('invalid', 'files.put: text oder bytes nötig');
        const mime = String(a.mime ?? (typeof a.text === 'string' ? 'text/plain; charset=utf-8' : 'application/octet-stream'));
        const ref = await sha256Hex(bytes);
        core.blobs.set(ref, new Blob([bytes], { type: mime }));
        const id = typeof a.id === 'string' && a.id ? a.id : uuidv7();
        await commit('files', [{ id, ref, name: String(a.name ?? 'datei'), mime, size: bytes.byteLength, owner_app: appId, owner_user: command?.actor?.id ?? null, created_at_ms: Date.now() }], depth);
        return { id, ref };
      },
      async get(sel) {
        const d = ownCoreFile(appId, sel);
        if (!d) return { found: false, ref: sel.ref ?? null, ...(sel.id ? { id: sel.id } : {}) };
        const blob = core.blobs.get(d.ref);
        if (!blob) return { found: false, ...coreFileInfo(d) };
        const bytes = new Uint8Array(await blob.arrayBuffer());
        let text;
        if (bytes.byteLength <= FILE_TEXT_MAX) { try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); } catch { text = undefined; } }
        return { found: true, ...coreFileInfo(d), size: bytes.byteLength, ...(text !== undefined ? { text } : { bytes }) };
      },
      async list() { return { files: rowsOf('files').map((r) => r.data).filter((d) => d.owner_app === appId).map(coreFileInfo).sort((a, b) => (a.id < b.id ? -1 : 1)) }; },
      async delete(sel) {
        const d = ownCoreFile(appId, sel);
        if (!d) return { deleted: false };
        await commit('files', [{ id: d.id, _deleted: true }], depth);
        return { deleted: true, id: d.id };
      },
    };
  }
  // ---- K-1i: process.run (CAP-3d #844, §16.47) – Allowlist der Plattform, Grund-Umgebung, stdin/stdin_ref; SIMULIERT ----
  // Kein echter Prozess: die Ausführung ist deterministisch nachgebildet (cat → stdin, echo → args, env → Umgebung,
  // false → exit 1, sleep N → timed_out, wenn N·1000 > timeout_ms; sonst JSON-Beschreibung des Aufrufs).
  const DEFAULT_BASE_ENV = Object.freeze({ PATH: '/usr/local/bin:/usr/bin:/bin', CTOX_BIN: '/opt/ctox/bin', LANG: 'C.UTF-8' });
  const DEFAULT_PROCESS_ALLOWLIST = Object.freeze({
    cat: { program: '/bin/cat', env: DEFAULT_BASE_ENV },
    echo: { program: '/bin/echo', env: DEFAULT_BASE_ENV },
    env: { program: '/usr/bin/env', env: DEFAULT_BASE_ENV },
    false: { program: '/usr/bin/false', env: DEFAULT_BASE_ENV },
    sleep: { program: '/bin/sleep', env: DEFAULT_BASE_ENV },
    scrape: { interpreter: '/usr/local/bin/tsx', script: '/opt/ctox/scripts/scrape.ts', env: DEFAULT_BASE_ENV },
  });
  const STDIN_MAX = 1024 * 1024, ENV_VALUE_MAX = 4096;
  const PROCESS_FIELDS = ['program', 'args', 'env', 'stdin', 'stdin_ref', 'timeout_ms'];
  async function simulateProcessRun(appId, req, allowlist, readOwnFile) {
    const bad = (code, reason) => { throw new CtoxError(code, reason); };
    if (!isPlainObject(req)) bad('invalid', 'payload_invalid:request');
    for (const k of Object.keys(req)) if (!PROCESS_FIELDS.includes(k)) bad('invalid', `payload_invalid:${k}`);
    if (typeof req.program !== 'string' || !req.program) bad('invalid', 'payload_invalid:program');
    const raw = (allowlist ?? DEFAULT_PROCESS_ALLOWLIST)[req.program];
    if (!raw) bad('forbidden', 'program_not_allowed');
    const entry = typeof raw === 'string' ? { program: raw, env: {} } : raw;
    const args = req.args ?? [];
    if (!Array.isArray(args) || args.some((a) => typeof a !== 'string')) bad('invalid', 'payload_invalid:args');
    const baseEnv = { ...(entry.env ?? {}) };
    const appEnv = req.env ?? {};
    if (!isPlainObject(appEnv)) bad('invalid', 'payload_invalid:env');
    for (const [key, v] of Object.entries(appEnv)) {
      if (typeof v !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) bad('invalid', 'payload_invalid:env');
      if (Object.prototype.hasOwnProperty.call(baseEnv, key)) bad('invalid', `env_base_override:${key}`); // Grundvariable nie ersetzbar
      if (new TextEncoder().encode(v).byteLength > ENV_VALUE_MAX) bad('invalid', `env_value_too_large:${key}`);
    }
    if (req.stdin !== undefined && req.stdin_ref !== undefined) bad('invalid', 'payload_invalid:stdin');
    let stdin = '';
    if (req.stdin !== undefined) {
      if (typeof req.stdin !== 'string') bad('invalid', 'payload_invalid:stdin');
      if (new TextEncoder().encode(req.stdin).byteLength > STDIN_MAX) bad('invalid', 'stdin_too_large');
      stdin = req.stdin;
    } else if (req.stdin_ref !== undefined) {
      if (!isPlainObject(req.stdin_ref) || typeof req.stdin_ref.ref !== 'string') bad('invalid', 'payload_invalid:stdin_ref');
      const f = await readOwnFile(req.stdin_ref.ref); // nur files-Datensätze der aufrufenden App (sonst forbidden)
      if (!f?.found) bad('not_found', 'stdin_ref_not_found');
      if (typeof f.text !== 'string') bad('invalid', 'stdin_ref_not_utf8');
      if (new TextEncoder().encode(f.text).byteLength > STDIN_MAX) bad('invalid', 'stdin_too_large');
      stdin = f.text;
    }
    const timeout = req.timeout_ms ?? 30000;
    if (!Number.isInteger(timeout) || timeout <= 0) bad('invalid', 'payload_invalid:timeout_ms');
    const env = { ...baseEnv, ...appEnv }; // Kind = leere Umgebung + Grund-Umgebung + App-env
    const argv = entry.interpreter ? [entry.interpreter, entry.script, ...args] : [entry.program, ...args];
    const name = String(argv[entry.interpreter ? 1 : 0]).split('/').pop();
    let exit_code = 0, stdout = '', stderr = '', timed_out = false;
    if (name === 'cat') stdout = stdin;
    else if (name === 'echo') stdout = `${args.join(' ')}\n`;
    else if (name === 'env') stdout = Object.keys(env).sort().map((k) => `${k}=${env[k]}`).join('\n') + '\n';
    else if (name === 'false') exit_code = 1;
    else if (name === 'sleep') { if (Number(args[0] ?? 0) * 1000 > timeout) { timed_out = true; exit_code = null; stderr = `timeout after ${timeout} ms\n`; } }
    else stdout = `${JSON.stringify({ app: appId, argv, env_keys: Object.keys(env).sort(), stdin_bytes: new TextEncoder().encode(stdin).byteLength })}\n`;
    return { exit_code, stdout, stderr, timed_out };
  }
  function makeCtx(appId, manifest, command, depth) {
    const declared = new Set(manifest.capabilities_required ?? []);
    const ns = (c, op) => {
      if (manifest.collections?.[c]) return nsOf(c, manifest.generations?.[c] ?? 1);
      const f = manifest.foreign_collections?.[c];
      if (f) {
        const ops = Array.isArray(f) ? f : f.ops ?? ['read'];
        if (!ops.includes(op) && !(op === 'read' && ops.includes('watch'))) fail('forbidden', `${appId}: ${op} auf fremde Collection ${c} nicht deklariert`);
        const owner = typeof f === 'object' && !Array.isArray(f) ? f.app ?? f.owner : typeof f === 'string' ? f : null;
        const om = owner ? activeManifest(owner) : null;
        return nsOf(c, om?.generations?.[c] ?? 1);
      }
      if (PLATFORM_COLLECTIONS.includes(c) && op === 'read') return c;
      return fail('forbidden', `${appId}: Collection ${c} gehört nicht zur App`);
    };
    const schemaOf = (c) => manifest.collections?.[c]?.schema ?? null;
    const writeOne = async (c, op, id, body) => {
      const n = ns(c, op);
      const old = data(n, id);
      let doc;
      if (op === 'delete') { if (!old) fail('not_found', `${c}/${id} fehlt`); await commit(n, [{ id, _deleted: true }], depth); return { status: 'accepted' }; }
      if (op === 'patch') { if (!old) fail('not_found', `${c}/${id} fehlt`); doc = mergePatch(cloneValue(old), body); } else doc = cloneValue(body);
      if (!isPlainObject(doc) || typeof doc.id !== 'string' || !doc.id) fail('invalid', `${c}: Dokument mit id nötig`);
      const schema = schemaOf(c);
      if (schema) validateSchema(doc, schema, '');
      await commit(n, [doc], depth);
      return { status: 'accepted', id: doc.id, seq: core.headSeq() };
    };
    const api = {
      read: async (c, mango = {}) => runCompiledQuery(rowsOf(ns(c, 'read')).map((r) => cloneValue(r.data)), compileQuery(mango)),
      get: async (c, id) => cloneValue(data(ns(c, 'read'), id)),
      put: (c, doc) => writeOne(c, 'put', doc?.id, doc),
      patch: (c, id, p) => writeOne(c, 'patch', id, p),
      remove: (c, id) => writeOne(c, 'delete', id, null),
    };
    const cap = (name, fn) => async (args = {}) => {
      if (!declared.has(name) && !declared.has(name.split('.')[0])) fail('forbidden', `cap.${name} nicht in capabilities_required von ${appId}`);
      return fn(args);
    };
    return Object.freeze({
      ...api,
      async transaction(fn) { return fn(Object.freeze({ ...api })); },
      async command(type, payload = {}, opts = {}) {
        const id = opts.id ?? uuidv7();
        const doc = { id, type, payload: cloneValue(payload), status: 'queued', app_id: appId, actor_id: command.actor?.id ?? 'core', created_at: new Date().toISOString(), ...(opts.record_id !== undefined ? { record_id: opts.record_id } : {}), ...(opts.target_app===undefined?{}:{target_app:opts.target_app}) };
        const route=commandRoute(doc,activeManifest,core.users.get(command.actor?.id)??command.actor);
        if(route.error)fail(route.error,route.error);
        await commit('business_commands', [doc], depth);
        core.spawnCommand(doc, depth + 1);
        return { id, status: 'accepted' };
      },
      now: () => Date.now(),
      uuid: () => uuidv7(),
      log: (level, msg, d) => { log.push({ app_id: appId, level, msg, data: d ?? null, at: Date.now() }); },
      asset(path) {
        if (typeof path !== 'string' || !path || path.startsWith('/') || path.split('/').some((s) => s === '..' || s === '.')) fail('invalid', `ctx.asset: Pfad ${path}`);
        const text = fileText(manifest, path);
        if (new TextEncoder().encode(text).byteLength > MAX_ASSET) fail('invalid', `ctx.asset: ${path} > 1 MiB`);
        return text;
      },
      /** #830: ctx.model(req) = Katalogfunktion model.run (deklariert als `model`). */
      model: (req) => cap('model.run', async (r) => modelRun(appId, r))(req),
      cap: Object.freeze({
        files: Object.freeze(Object.fromEntries(Object.entries(filesCap(appId, command, depth)).map(([fn, f]) => [fn, cap(`files.${fn}`, f)]))),
        model: Object.freeze({ run: cap('model.run', async (r) => modelRun(appId, r)) }),
        process: Object.freeze({ run: cap('process.run', (r) => simulateProcessRun(appId, r, core.processAllowlist, (ref) => filesCap(appId, command, depth).get({ ref }))) }),
        audit: Object.freeze({ event: cap('audit.event', async (a) => { const id = uuidv7(); await commit('_audit', [{ id, app_id: appId, ...cloneValue(a), at_ms: Date.now() }], depth); return { id }; }) }),
        agent: Object.freeze({ task: Object.freeze({
          enqueue: cap('agent.task.enqueue', (a) => enqueueTask(appId, manifest, a, command, depth)),
          get: cap('agent.task.get', async ({ task_id }) => cloneValue(tasks.get(task_id) ?? null)),
          find: cap('agent.task.find', async ({ idempotency_key, command_id }) => cloneValue([...tasks.values()].find((t) => (idempotency_key && t.idempotency_key === idempotency_key) || (command_id && t.command_id === command_id)) ?? null)),
          cancel: cap('agent.task.cancel', async ({ task_id, status_note }) => finishTask(task_id, 'cancelled', { note: status_note ?? null })),
        }) }),
      }),
    });
  }
  async function runHandler(appId, manifest, path, command, depth) {
    const fn = await handlerFn(manifest, path);
    const ctx = makeCtx(appId, manifest, command, depth);
    try {
      const result = await fn(Object.freeze(cloneValue(command)), ctx);
      return result === undefined ? null : cloneValue(result);
    } catch (e) {
      const code = e?.code ?? 'failed';
      throw new CtoxError(String(code), `${code}: ${e?.message ?? e}`);
    }
  }

  // ---- §16.30: agent.task (Fake-Harness: Lauf endet nach kurzer Zeit, Rückruf höchstens einmal je task_id) ----
  async function enqueueTask(appId, manifest, a, command, depth) {
    if (!a?.title || !a?.instruction) fail('invalid', 'agent.task.enqueue: title und instruction nötig');
    if (a.idempotency_key && taskByKey.has(`${appId}:${a.idempotency_key}`)) return { task_id: taskByKey.get(`${appId}:${a.idempotency_key}`) };
    if (a.skill && !String(a.skill).startsWith(`${appId}/`)) fail('forbidden', `Skill ${a.skill} gehört nicht zu ${appId}`);
    if (a.skill) {
      const name = String(a.skill).slice(appId.length + 1);
      const skill = (manifest.skills ?? []).find((s) => s.name === name);
      if (!skill || !manifest.files?.[skill.path]) fail('not_found', `Skill ${a.skill} nicht im aktiven Artefakt`);
    }
    for (const t of [a.on_complete, a.on_failed]) if (t && !manifest.handlers?.[t]) fail('invalid', `Rückruf ${t} ist kein Handler der eigenen App`);
    const task_id = `task:${uuidv7()}`;
    const task = { id: task_id, task_id, app_id: appId, app_ver: manifest.app_ver, title: String(a.title), instruction: String(a.instruction), skill: a.skill ?? null, payload: cloneValue(a.payload ?? {}), idempotency_key: a.idempotency_key ?? null, command_id: command?.id ?? null, on_complete: a.on_complete ?? null, on_failed: a.on_failed ?? null, status: 'queued', created_at_ms: Date.now() };
    tasks.set(task_id, task);
    if (task.idempotency_key) taskByKey.set(`${appId}:${task.idempotency_key}`, task_id);
    await commit('ctox_queue_tasks', [{ id: task_id, app_id: appId, title: task.title, skill: task.skill, status: 'queued', created_at_ms: task.created_at_ms }], depth);
    setTimeout(() => { void runFakeAgent(task_id); }, core.agentDelayMs ?? 100).unref?.();
    return { task_id };
  }
  async function runFakeAgent(task_id) {
    const t = tasks.get(task_id);
    if (!t || t.status !== 'queued') return;
    let outcome;
    try { outcome = agentOutcome ? await agentOutcome(cloneValue(t)) : { status: 'done', result: { summary: `Fake-Harness: ${t.title}`, skill: t.skill } }; } catch (e) { outcome = { status: 'failed', result: { error: String(e?.message ?? e) } }; }
    await finishTask(task_id, outcome.status === 'failed' ? 'failed' : 'done', outcome.result ?? null);
  }
  async function finishTask(task_id, status, result) {
    const t = tasks.get(task_id);
    if (!t || t.status !== 'queued') return cloneValue(t ?? null);
    t.status = status; t.result = result;
    await commit('ctox_queue_tasks', [{ id: task_id, app_id: t.app_id, title: t.title, skill: t.skill, status, created_at_ms: t.created_at_ms, finished_at_ms: Date.now() }]);
    const type = status === 'done' ? t.on_complete : t.on_failed;
    if (type && !callbacks.has(task_id)) { // §16.30 Nr. 3: genau ein Rückruf je task_id
      callbacks.add(task_id);
      const doc = { id: uuidv7(), type, payload: { task_id, status, result: cloneValue(result), workspace_refs: [] }, status: 'queued', app_id: t.app_id, actor_id: 'core', created_at: new Date().toISOString() };
      await commit('business_commands', [doc]);
      core.spawnCommand(doc, 1);
    }
    return cloneValue(t);
  }

  // ---- Punkt 6: Trigger (§16.20, §16.34 Nr. 3) ----
  function triggersOf(appId) { return (activeManifest(appId)?.triggers ?? []).filter((t) => t && typeof t.handler === 'string'); }
  function watches(appId, manifest, c) {
    if (manifest.collections?.[c]) return true;
    const f = manifest.foreign_collections?.[c];
    if (!f) return false;
    const ops = Array.isArray(f) ? f : f.ops ?? [];
    if (!ops.includes('watch')) return false;
    for (const other of activeApps()) { // Eigentümer muss die Collection mit exports:['watch'] freigeben
      if (other === appId) continue;
      const om = activeManifest(other);
      if (om?.collections?.[c]) return (om.exports?.[c] ?? []).includes('watch');
    }
    return false;
  }
  function onCommit(collection, changes, depth) {
    const c = baseName(collection);
    if (c === 'business_apps' || c === 'business_app_versions') { appCache = null; refreshEvery(); }
    if (depth >= MAX_TRIGGER_DEPTH) return;
    if (c.startsWith('_') || PLATFORM_COLLECTIONS.includes(c) || !apps().watchers.has(c)) return;
    for (const appId of new Set(apps().watchers.get(c))) {
      const manifest = activeManifest(appId);
      if (!manifest || !watches(appId, manifest, c)) continue;
      for (const t of triggersOf(appId)) {
        if (!t.on || t.on.collection !== c) continue;
        for (const ch of changes) {
          if (t.on.op && t.on.op !== ch.op) continue;
          const command = { id: uuidv7(), type: `trigger:${t.handler}`, payload: { collection: c, id: ch.id, op: ch.op, doc: cloneValue(ch.doc) }, actor: { id: 'core', role: 'system' }, app_id: appId, app_ver: manifest.app_ver };
          void runHandler(appId, manifest, t.handler, command, depth + 1).catch((e) => log.push({ app_id: appId, level: 'error', msg: `Trigger ${t.handler}: ${e.message}`, at: Date.now() }));
        }
      }
    }
  }
  function refreshEvery() {
    const want = new Map();
    for (const appId of activeApps()) {
      const manifest = activeManifest(appId);
      for (const t of triggersOf(appId)) {
        if (!t.every) continue;
        const ms = isoMs(t.every);
        if (!ms) continue;
        // Schlüssel ohne Version: ein Versionswechsel setzt den Takt nicht zurück (Handler kommt beim Feuern aus der aktiven Version)
        want.set(`${appId}:${t.handler}:${ms}`, { appId, handler: t.handler, ms: core.everyMs ?? Math.max(60000, ms) });
      }
    }
    for (const [k, timer] of everyTimers) if (!want.has(k)) { clearInterval(timer); everyTimers.delete(k); }
    if (!core.listeners.size) return;
    for (const [k, w] of want) {
      if (everyTimers.has(k)) continue;
      const timer = setInterval(() => {
        if (running.has(k)) return; // höchstens eine laufende Instanz je Trigger
        const manifest = activeManifest(w.appId);
        if (!manifest) return;
        running.add(k);
        const command = { id: uuidv7(), type: `trigger:${w.handler}`, payload: { every: true }, actor: { id: 'core', role: 'system' }, app_id: w.appId, app_ver: manifest.app_ver };
        runHandler(w.appId, manifest, w.handler, command, 1).catch((e) => log.push({ app_id: w.appId, level: 'error', msg: `Trigger ${w.handler}: ${e.message}`, at: Date.now() })).finally(() => running.delete(k));
      }, w.ms);
      timer.unref?.();
      everyTimers.set(k, timer);
    }
  }
  function idle() { for (const t of everyTimers.values()) clearInterval(t); everyTimers.clear(); }

  // ---- Punkt 7: Plattformbefehle ctox.app.* (§16.29 B, §16.33) ----
  function casCode(p) {
    const active = activeVer(p.app_id);
    return (active ?? null) !== (p.base_app_ver ?? null) ? 'stale_base' : null;
  }
  const genOlder = (fromMan, toMan) => Object.entries(fromMan?.generations ?? {}).some(([c, g]) => (toMan?.generations?.[c] ?? 1) < g);
  function checkPlatformCommand(user, body) {
    const p = body.payload ?? {};
    if (typeof p.app_id !== 'string' || !p.app_id) return 'invalid';
    const type = body.type;
    if (type === 'ctox.app.publish') {
      if (!hasPerm(user, 'apps.source.edit')) return 'forbidden';
      const cas = casCode(p); if (cas) return cas;
      const draft = data('app_drafts', p.draft_id ?? `${user.id}/${p.app_id}`);
      if (!draft || draft.app_id !== p.app_id) return 'not_found';
      if (user.role !== 'admin' && !draft.id.startsWith(`${user.id}/`)) return 'forbidden'; // nur eigene Entwürfe
      return null;
    }
    if (type === 'ctox.app.update') {
      const cat = data('app_catalog', `${p.app_id}@${p.release_ver}`);
      if (p.discard_local && !hasPerm(user, 'apps.modify')) return 'forbidden';
      if (!hasPerm(user, 'apps.install') && !hasPerm(user, 'apps.modify')) return 'forbidden';
      const cas = casCode(p); if (cas) return cas; // Installation: base_app_ver null, App existiert schon → stale_base
      if (!cat) return 'not_found';
      const cur = activeManifest(p.app_id);
      if (cur?.provenance?.locally_modified && !p.discard_local) return 'locally_modified';
      if (cur && genOlder(cur, cat.manifest)) return 'migration_required';
      return null;
    }
    if (type === 'ctox.app.rollback') {
      if (!hasPerm(user, 'apps.modify')) return 'forbidden';
      const cas = casCode(p); if (cas) return cas;
      const target = manifestOf(p.app_id, p.target_ver);
      if (!target) return 'not_found';
      if (genOlder(activeManifest(p.app_id), target)) return 'migration_required';
      return null;
    }
    if (type === 'ctox.app.release' || type === 'ctox.app.remove') {
      if (!hasPerm(user, 'apps.modify')) return 'forbidden';
      const cas = casCode(p); if (cas) return cas;
      if (!activeVer(p.app_id)) return 'not_found';
      return null;
    }
    return 'invalid';
  }
  const nextVer = () => `s${core.headSeq() + 1n}`;
  async function setActive(appId, ver, manifest, extra = {}) {
    const app = data('business_apps', appId);
    await commit('business_app_versions', [{ id: `${appId}@${ver}`, app_id: appId, app_ver: ver, manifest, migrated_at_seq: String(core.headSeq()), ...extra }]);
    await commit('business_apps', [{ ...(app ?? {}), id: appId, title: manifest.title ?? app?.title ?? appId, active_ver: ver, migrated_at_seq: String(core.headSeq()) }]);
  }
  async function runPlatformCommand(user, doc) {
    const p = doc.payload ?? {};
    const code = checkPlatformCommand(user ?? SYSTEM_ADMIN, doc); // erneut prüfen: Zustand kann sich geändert haben
    if (code) return { status: 'failed', error: code, result: { code } };
    const appId = p.app_id;
    const cur = activeManifest(appId);
    if (doc.type === 'ctox.app.publish') {
      const draft = data('app_drafts', p.draft_id ?? `${user?.id}/${appId}`);
      const base = cur;
      const files = { ...(base?.files ?? {}) };
      for (const [path, h] of Object.entries(draft.files ?? {})) { if (h === null) delete files[path]; else files[path] = h; }
      delete files['manifest.json'];
      for (const h of Object.values(files)) if (!core.artifacts.has(h)) return { status: 'failed', error: 'invalid', result: { code: 'invalid', detail: `Artefakt ${h} fehlt im Core` } };
      const mf = draft.manifest_fields ?? {};
      const parsedBase = base?.files?.[SCHEMA_FILE] ? parseCollectionsFile(asText(core.artifacts.get(base.files[SCHEMA_FILE]))) : manifestCollectionsToParsed(base?.collections);
      const parsedNext = files[SCHEMA_FILE] ? parseCollectionsFile(asText(core.artifacts.get(files[SCHEMA_FILE]))) : parsedBase;
      const diff = classifyCollections(parsedBase, parsedNext, { baseVer: activeVer(appId) });
      if (diff.some((x) => x.kind === 'generation')) return { status: 'failed', error: 'migration_required', result: { code: 'migration_required', detail: 'Generationswechsel nur über draft.publish mit Migration' } };
      const extra = {};
      for (const [k, v] of Object.entries(base ?? {})) if (!MANIFEST_FIELDS_COMPUTED.has(k)) extra[k] = cloneValue(v); // B4
      for (const [k, v] of Object.entries(mf)) if (!MANIFEST_FIELDS_COMPUTED.has(k)) extra[k] = cloneValue(v);
      const ver = nextVer();
      const manifest = {
        ...extra, app_id: appId, app_ver: ver, files, entry: mf.entry ?? base?.entry ?? 'index.js', title: mf.title ?? base?.title,
        collections: parsedToManifestCollections(parsedNext), generations: { ...(base?.generations ?? {}) }, retires: {},
        handlers: mf.handlers ?? base?.handlers ?? {}, styles: mf.styles ?? base?.styles ?? [], layout: mf.layout ?? base?.layout ?? null,
        min_app_api: mf.min_app_api ?? base?.min_app_api ?? '4.0', capabilities_required: mf.capabilities_required ?? base?.capabilities_required ?? [],
        provenance: { base_release: base?.provenance?.base_release ?? null, base_hash: null, locally_modified: true }, author: user?.id ?? 'core', created_at: new Date().toISOString(),
      };
      await setActive(appId, ver, manifest);
      await commit('app_drafts', [{ id: draft.id, _deleted: true }]);
      return { status: 'done', result: { app_ver: ver } };
    }
    if (doc.type === 'ctox.app.update') {
      const cat = data('app_catalog', `${appId}@${p.release_ver}`);
      if (cur?.provenance?.locally_modified) { // §16.33: angepasste Fassung archivieren, nie löschen
        const row = data('business_app_versions', `${appId}@${cur.app_ver}`);
        await commit('business_app_versions', [{ ...row, origin: 'local', status: 'archived' }]);
      }
      const manifest = { ...cloneValue(cat.manifest), app_id: appId, app_ver: p.release_ver, provenance: { base_release: p.release_ver, base_hash: null, locally_modified: false } };
      await setActive(appId, p.release_ver, manifest, { origin: 'catalog' });
      return { status: 'done', result: { app_ver: p.release_ver } };
    }
    if (doc.type === 'ctox.app.rollback') { // vorwärts: Zielfassung wird neue Version
      const target = manifestOf(appId, p.target_ver);
      const ver = nextVer();
      await setActive(appId, ver, { ...cloneValue(target), app_ver: ver, created_at: new Date().toISOString(), provenance: { ...(target.provenance ?? {}), rolled_back_from: cur?.app_ver ?? null } });
      return { status: 'done', result: { app_ver: ver } };
    }
    if (doc.type === 'ctox.app.release') {
      const ver = cur.app_ver;
      await commit('app_catalog', [{ id: `${appId}@${ver}`, app_id: appId, app_ver: ver, title: cur.title ?? appId, layout: cur.layout ?? null, source: 'instance', channel: p.channel ?? 'stable', manifest: cloneValue(cur), published_at: new Date().toISOString() }]);
      return { status: 'done', result: { app_ver: ver } };
    }
    if (doc.type === 'ctox.app.remove') { // Archiv (§9.5): Versionen und Daten bleiben, die App verschwindet aus business_apps
      const app = data('business_apps', appId);
      await commit('app_archive', [{ id: `${appId}@${cur.app_ver}`, app_id: appId, app_ver: cur.app_ver, app: cloneValue(app), removed_by: user?.id ?? 'core', removed_at: new Date().toISOString() }]);
      await commit('business_apps', [{ id: appId, _deleted: true }]);
      return { status: 'done', result: { archived: `${appId}@${cur.app_ver}` } };
    }
    return { status: 'failed', error: 'invalid' };
  }

  // Unified diff mit 3 Kontextzeilen je Hunk (ein Hunk um den geänderten Bereich; für git apply geeignet).
  function unifiedDiff(path, before, after, context = 3) {
    const split = (t) => (t === '' ? [] : t.replace(/\n$/, '').split('\n'));
    const a = split(before), b = split(after);
    let pre = 0;
    while (pre < a.length && pre < b.length && a[pre] === b[pre]) pre++;
    let suf = 0;
    while (suf < a.length - pre && suf < b.length - pre && a[a.length - 1 - suf] === b[b.length - 1 - suf]) suf++;
    const from = Math.max(0, pre - context);
    const aEnd = Math.min(a.length, a.length - suf + context), bEnd = Math.min(b.length, b.length - suf + context);
    const lines = [];
    for (let i = from; i < pre; i++) lines.push(` ${a[i]}`);
    for (let i = pre; i < a.length - suf; i++) lines.push(`-${a[i]}`);
    for (let i = pre; i < b.length - suf; i++) lines.push(`+${b[i]}`);
    for (let i = a.length - suf; i < aEnd; i++) lines.push(` ${a[i]}`);
    const aLen = aEnd - from, bLen = bEnd - from;
    const head = `@@ -${aLen ? from + 1 : from},${aLen} +${bLen ? from + 1 : from},${bLen} @@`;
    return `--- a/${path}\n+++ b/${path}\n${head}\n${lines.join('\n')}\n`;
  }
  // ---- K-1f: Coding Agent im Quelltext-Modus (§16.44, Vertrag befund-coding-panel.md / Core-PR #848), simuliert ----
  // Der simulierte Agent ändert deterministisch: er hängt an die erste im Prompt genannte Datei des Entwurfs (sonst die erste
  // Textdatei) eine Kommentarzeile an. Nachrichten und tool_events erscheinen wie beim echten Sidecar erst am Turn-Ende.
  const TURN_FIELDS = ['app_id', 'draft_id', 'base_draft_rev', 'prompt', 'model', 'turn_id'];
  const TURN_ID = /^[A-Za-z0-9_.:-]{1,128}$/;
  const activeTurns = new Map(); // draft_id → turn_id (Prüfung und Reservierung in einem Schritt wie die Writer-Transaktion)
  const turnRow = (id) => core.docs.get(key('coding_turns', id));
  const draftRow = (id) => { const r = core.docs.get(key('app_drafts', id)); return r && !r.deleted && r.data ? r : null; };
  const inv = (f) => ({ code: 'invalid', reason: `payload_invalid:${f}` });
  function runningTurnFor(draftId) {
    if (activeTurns.has(draftId)) return activeTurns.get(draftId);
    for (const r of rowsOf('coding_turns')) if (r.data.draft_id === draftId && ['queued', 'running'].includes(r.data.state)) return r.id;
    return null;
  }
  /** null = angenommen, sonst {code, reason} (write_result rejected). */
  function checkCodingCommand(user, body) {
    const p = isPlainObject(body.payload) ? body.payload : {};
    if (body.type === 'ctox.coding.turn') {
      for (const k of Object.keys(p)) if (!TURN_FIELDS.includes(k)) return inv(k);
      for (const f of ['app_id', 'draft_id', 'base_draft_rev', 'prompt']) if (typeof p[f] !== 'string' || !p[f].trim()) return inv(f);
      if (p.model !== undefined && (typeof p.model !== 'string' || !p.model)) return inv('model');
      if (p.turn_id !== undefined && (typeof p.turn_id !== 'string' || !TURN_ID.test(p.turn_id))) return inv('turn_id');
      if (!hasPerm(user, 'apps.source.edit')) return { code: 'forbidden', reason: 'apps.source.edit' };
      const row = draftRow(p.draft_id);
      if (!row) return { code: 'not_found', reason: 'draft_not_found' };
      if (row.data.user_id !== user.id || row.data.app_id !== p.app_id) return { code: 'forbidden', reason: 'draft_not_owned' }; // auch Admin nicht
      if (row.rev !== p.base_draft_rev) return { code: 'conflict', reason: 'stale_draft_rev' };
      if (runningTurnFor(p.draft_id)) return { code: 'conflict', reason: 'turn_running' };
      const taken = p.turn_id ? turnRow(p.turn_id) : null;
      if (taken && taken.data?.command_id !== body.id) return { code: 'conflict', reason: 'turn_id_taken' };
      return null;
    }
    if (body.type === 'ctox.coding.cancel') {
      for (const k of Object.keys(p)) if (k !== 'turn_id') return inv(k);
      if (typeof p.turn_id !== 'string' || !p.turn_id) return inv('turn_id');
      const t = turnRow(p.turn_id);
      if (!t || t.deleted) return { code: 'not_found', reason: 'turn_not_found' };
      if (t.data.user_id !== user.id) return { code: 'forbidden', reason: 'turn_not_owned' };
      if (!['queued', 'running'].includes(t.data.state)) return { code: 'conflict', reason: 'turn_not_running' };
      return null;
    }
    return { code: 'invalid', reason: 'payload_invalid:type' };
  }
  /** Annahme eines Turns (synchron mit dem Write): Turn-id vergeben, Entwurf reservieren, Turn-Dokument queued anlegen. */
  function acceptCodingTurn(user, body) {
    const p = body.payload;
    const turn_id = p.turn_id ?? `turn-${uuidv7()}`;
    activeTurns.set(p.draft_id, turn_id);
    const now = Date.now();
    const doc = { id: turn_id, app_id: p.app_id, draft_id: p.draft_id, user_id: user.id, command_id: body.id, origin_app: body.app_id,
      base_draft_rev: p.base_draft_rev, prompt: p.prompt, model: p.model ?? null, state: 'queued', messages: [], tool_events: [],
      draft_rev_after: null, changed_files: [], callback: { on_complete: body.on_complete ?? null, on_failed: body.on_failed ?? null },
      created_at_ms: now, updated_at_ms: now };
    turnsAccepted.set(body.id, turn_id);
    void commit('coding_turns', [doc]);
    return turn_id;
  }
  const turnsAccepted = new Map(); // command_id → turn_id
  async function setTurn(turn_id, patch) {
    const cur = turnRow(turn_id)?.data;
    if (!cur) return null;
    if (!['queued', 'running'].includes(cur.state) && patch.state) return cur; // Endzustand genau einmal
    const next = { ...cur, ...patch, updated_at_ms: Date.now() };
    await commit('coding_turns', [next]);
    return next;
  }
  async function codingCallback(doc, turn, status, result) {
    const type = status === 'done' ? turn.callback?.on_complete : turn.callback?.on_failed;
    const id = `coding-callback:${doc.id}`;
    if (!type || data('business_commands', id)) return; // höchstens einmal
    const cb = { id, type, payload: { command_id: doc.id, turn_id: turn.id, status, result: cloneValue(result) }, status: 'queued', app_id: turn.origin_app, actor_id: 'core', created_at: new Date().toISOString() };
    await commit('business_commands', [cb]);
    core.spawnCommand(cb, 1);
  }
  async function runCodingTurn(doc) {
    const turn_id = turnsAccepted.get(doc.id) ?? doc.payload?.turn_id;
    await until(() => turnRow(turn_id)); // das queued-Dokument ist schon unterwegs
    let turn = turnRow(turn_id).data;
    const finish = async (state, extra, cmd) => {
      activeTurns.delete(turn.draft_id);
      const ended = await setTurn(turn_id, { state, finished_at_ms: Date.now(), ...extra });
      const t = ended ?? turn;
      const finalState = t.state; // ein Abbruch kann vorher gewonnen haben
      const result = { turn_id, state: finalState, draft_rev_after: t.draft_rev_after ?? null, changed_files: t.changed_files ?? [], ...(t.patch ? { patch: t.patch } : {}), ...(t.error ? { error: t.error } : {}) };
      await codingCallback(doc, t, finalState === 'done' ? 'done' : finalState, result);
      if (finalState === 'done') return { status: 'done', progress: 1, result };
      if (finalState === 'cancelled') return { status: 'cancelled', result };
      return { status: 'failed', error: `${t.error?.code ?? 'failed'}: ${t.error?.reason ?? finalState}`, result };
    };
    if (turn.state !== 'queued') return finish(turn.state, {});
    turn = await setTurn(turn_id, { state: 'running' }) ?? turn;
    // §16.44-Ergänzung (1): Arbeitsbaum = VOLLER Baum = Basisversion + Entwurfs-Überlagerung (null = gelöscht), Snapshot beim Start
    const startRow = draftRow(turn.draft_id);
    const baseFiles = startRow?.data?.base_app_ver ? manifestOf(turn.app_id, startRow.data.base_app_ver)?.files ?? {} : {};
    const files = { ...baseFiles };
    for (const [path, h] of Object.entries(startRow?.data?.files ?? {})) { if (h === null) delete files[path]; else files[path] = h; }
    await new Promise((r) => setTimeout(r, core.codingDelayMs ?? 60));
    if (turnRow(turn_id).data.state === 'cancelled') return finish('cancelled', {});
    const textPaths = Object.keys(files).filter((p) => files[p] && typeof core.artifacts.get(files[p]) === 'string').sort();
    const named = textPaths.find((p) => turn.prompt.includes(p));
    const path = named ?? textPaths[0] ?? 'NOTES.md';
    const old = files[path] ? String(core.artifacts.get(files[path])) : '';
    const line = `// coding-agent ${turn_id}: ${turn.prompt.replace(/\s+/g, ' ').slice(0, 60)}`;
    const next = `${old}${old && !old.endsWith('\n') ? '\n' : ''}${line}\n`;
    const hash = await sha256Hex(next);
    core.artifacts.set(hash, next);
    const at = Date.now();
    const transcript = {
      messages: [{ role: 'user', text: turn.prompt, at_ms: at }, { role: 'assistant', text: `Kommentarzeile in ${path} ergänzt.`, at_ms: at }],
      tool_events: [{ kind: 'read', path, at_ms: at }, { kind: 'edit', path, at_ms: at }],
    };
    if (turnRow(turn_id).data.state === 'cancelled') return finish('cancelled', transcript);
    // Rückschreiben als neue Entwurfsrevision per CAS auf base_draft_rev
    const cur = draftRow(turn.draft_id);
    if (cur && cur.rev === turn.base_draft_rev) {
      await commit('app_drafts', [{ ...cur.data, files: { ...cur.data.files, [path]: hash }, updated_at_ms: at }]);
      return finish('done', { ...transcript, draft_rev_after: draftRow(turn.draft_id).rev, changed_files: [path] });
    }
    // Nutzer hat inzwischen editiert: nichts überschreiben, Patch als files-Datensatz
    const patch = unifiedDiff(path, old, next); // §16.44-Ergänzung (4): 3 Kontextzeilen, Basis = base_draft_rev
    const bytes = new TextEncoder().encode(patch);
    const ref = await sha256Hex(bytes);
    core.blobs.set(ref, new Blob([bytes], { type: 'text/x-diff' }));
    const file_id = `coding-patch:${turn_id}`;
    await commit('files', [{ id: file_id, ref, name: `${file_id.replace(':', '-')}.diff`, mime: 'text/x-diff', size: bytes.length, owner_app: turn.app_id, owner_user: turn.user_id, created_at_ms: at }]);
    return finish('conflict', { ...transcript, changed_files: [path], patch: { ref, file_id }, error: { status: 'failed', code: 'conflict', reason: 'draft_changed' } });
  }
  async function runCodingCancel(doc) {
    const turn_id = doc.payload.turn_id;
    const t = turnRow(turn_id)?.data;
    if (!t || !['queued', 'running'].includes(t.state)) return { status: 'failed', error: 'conflict: turn_not_running' };
    await setTurn(turn_id, { state: 'cancelled', finished_at_ms: Date.now() }); // zuerst persistieren, dann „Sidecar beenden“
    activeTurns.delete(t.draft_id);
    return { status: 'done', progress: 1, result: { turn_id, state: 'cancelled' } };
  }
  async function until(pred, ms = 5000) { const t0 = Date.now(); while (!pred()) { if (Date.now() - t0 > ms) fail('unavailable', 'Turn-Dokument fehlt'); await new Promise((r) => setTimeout(r, 2)); } }

  // K-WJ-B2: Plattformbefehle, die execute() ausführt; App-Befehle nur mit handlers-Eintrag der aktiven Version.
  function hasCommandRoute(appId,type,body={app_id:appId,type}) {
    // Authorization is checked separately; invalid declarations must be forbidden, not unknown.
    const r=commandRoute(body,activeManifest,null);
    return r.error!=='unknown_command';
  }
  /** Ausführung eines Befehls: Plattformbefehl → App-Handler (Routing nach (app_id, type)). null = kein Handler. */
  async function execute(user, doc, depth) {
    user??={...SYSTEM_ADMIN,id:doc.actor_id??'admin'};
    if (String(doc.type).startsWith('ctox.app.')) return runPlatformCommand(user, doc);
    if (doc.type === 'ctox.coding.turn') return runCodingTurn(doc);
    if (doc.type === 'ctox.coding.cancel') return runCodingCancel(doc);
    const route=commandRoute(doc,activeManifest,user);
    if(route.error)return {status:'failed',error:route.error};
    if(route.platform)return {status:'done',progress:1,result:{route:'core',type:doc.type}}; // explicit fake Core outcome, no app dispatch
    const manifest = activeManifest(route.target);
    const path = manifest?.handlers?.[doc.type];
    if (!path) return null;
    const command = { id: doc.id, type: doc.type, payload: cloneValue(doc.payload ?? {}), actor: { id: doc.actor_id ?? user?.id ?? 'core', role: user?.role ?? 'system' }, app_id: doc.app_id, app_ver: manifest.app_ver, ...(doc.record_id !== undefined ? { record_id: doc.record_id } : {}) };
    try { return { status: 'done', progress: 1, result: await runHandler(route.target, manifest, path, command, depth) }; } catch (e) { return { status: 'failed', error: String(e.message) }; }
  }

  return {
    resolveCommand: (body,user,testRoute=false)=>commandRoute(body,activeManifest,user,testRoute),
    canRead, checkWrite, execute, hasCommandRoute, onCommit, refreshEvery, idle, log, tasks, acceptCodingTurn, modelRun,
    setAgentOutcome(fn) { agentOutcome = typeof fn === 'function' ? fn : null; },
    capabilities: FAKE_CORE_CAPABILITIES,
  };
}
