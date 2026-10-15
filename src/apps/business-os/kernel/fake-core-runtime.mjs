// Fake-Core-Laufzeit (K-1c Punkte 4–7): Nutzer und Rechte, Handler-Laufzeit im Prozess (§16.16/§16.25),
// Trigger-Lauf (§16.20, fremde Collections nur nach §16.34 Nr. 3), Katalog-Primitive audit/agent.task (§16.29/§16.30)
// und die Plattformbefehle ctox.app.publish|update|rollback|release|remove (§16.29 B, §16.33).
// Nur für MemoryCoreTransport (Tests, Shell-Entwicklung). Der echte Core macht das in Rust.
import { CtoxError, cloneValue, isPlainObject, mergePatch, uuidv7 } from './values.mjs';
import { compileQuery, runCompiledQuery, validateSchema } from './app-api.mjs';
import { bytesBase64 } from './artifact-url.mjs';
import { PLATFORM_COLLECTIONS, PLATFORM_APP_ID } from './platform.mjs';
import { SCHEMA_FILE, parseCollectionsFile, classifyCollections, manifestCollectionsToParsed, parsedToManifestCollections } from './schema-diff.mjs';

/** Katalog-Primitive, die der Fake-Core als ctx.cap anbietet (Aktivierung prüft capabilities_required dagegen). */
export const FAKE_CORE_CAPABILITIES = Object.freeze(['audit.event', 'agent.task.enqueue', 'agent.task.get', 'agent.task.find', 'agent.task.cancel']);
// Zeilenrechte der Plattform-Collections (docs/v4-manifests/ctox-platform.manifest.json, Auszug).
const PLATFORM_RIGHTS = Object.freeze({
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
  function commandRight(user, body) {
    const m = activeManifest(body?.app_id);
    const rights = m?.command_rights ?? {};
    const entry = rights[body.type] ?? Object.entries(rights).find(([k]) => k.endsWith('*') && body.type.startsWith(k.slice(0, -1)))?.[1];
    if (!entry) return true;
    return user.role === 'admin' || (Array.isArray(entry.roles) && entry.roles.includes(user.role));
  }
  /** null = erlaubt, sonst Ablehnungscode. */
  function checkWrite(user, entry, old) {
    const c = baseName(entry.collection);
    // Plattformbefehle prüft der Core immer (CAS, Codes §16.33); ohne bekannten Nutzer gilt der Client als Admin (Bind-Aufbau).
    if (c === 'business_commands' && entry.op === 'put' && entry.body?.status === 'queued' && String(entry.body.type).startsWith('ctox.app.')) return checkPlatformCommand(user ?? SYSTEM_ADMIN, entry.body);
    if (!user) return null;
    if (c === 'business_commands') {
      const body = entry.body ?? old?.data;
      if (entry.op === 'put' && body?.status === 'queued') {
        if (String(body.type).startsWith('ctox.app.')) return checkPlatformCommand(user, body);
        if (!commandRight(user, body)) return 'forbidden';
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
      if (!declared.has(name)) fail('forbidden', `cap.${name} nicht in capabilities_required von ${appId}`);
      return fn(args);
    };
    return Object.freeze({
      ...api,
      async transaction(fn) { return fn(Object.freeze({ ...api })); },
      async command(type, payload = {}, opts = {}) {
        const id = opts.id ?? uuidv7();
        const doc = { id, type, payload: cloneValue(payload), status: 'queued', app_id: appId, actor_id: command.actor?.id ?? 'core', created_at: new Date().toISOString(), ...(opts.record_id !== undefined ? { record_id: opts.record_id } : {}) };
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
      cap: Object.freeze({
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

  /** Ausführung eines Befehls: Plattformbefehl → App-Handler (Routing nach (app_id, type)). null = kein Handler. */
  async function execute(user, doc, depth) {
    if (String(doc.type).startsWith('ctox.app.')) return runPlatformCommand(user, doc);
    const manifest = activeManifest(doc.app_id);
    const path = manifest?.handlers?.[doc.type];
    if (!path) return null;
    const command = { id: doc.id, type: doc.type, payload: cloneValue(doc.payload ?? {}), actor: { id: doc.actor_id ?? user?.id ?? 'core', role: user?.role ?? 'system' }, app_id: doc.app_id, app_ver: manifest.app_ver, ...(doc.record_id !== undefined ? { record_id: doc.record_id } : {}) };
    try { return { status: 'done', progress: 1, result: await runHandler(doc.app_id, manifest, path, command, depth) }; } catch (e) { return { status: 'failed', error: String(e.message) }; }
  }

  return {
    canRead, checkWrite, execute, onCommit, refreshEvery, idle, log, tasks,
    setAgentOutcome(fn) { agentOutcome = typeof fn === 'function' ? fn : null; },
    capabilities: FAKE_CORE_CAPABILITIES,
  };
}
