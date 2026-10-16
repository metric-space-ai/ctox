// Shell v4 – Strang SKL0: Simulation von ctx.cap.agent.task (Kern §16.30) und ctx.asset (§16.34) für Handler-Tests.
//
// Bildet nach, was der Core für eine App tut, ohne Harness:
//   - enqueue({title, instruction, skill?, thread_key?, payload, priority?, idempotency_key?, on_complete?, on_failed?})
//     → {task_id}; zeichnet jeden Aufruf auf (sim.enqueued). Strenge Prüfung: unbekannte Schlüssel, fehlende Pflicht-
//     felder, Skill-Auflösung und Callback-Typen → Fehler code 'invalid'.
//   - Skill-Auflösung: `<app_id>/<name>` muss im Manifest der App deklariert sein (skills:[{name,path}]) UND die Datei
//     im Paket existieren; fremder Namensraum → invalid. Name ohne `/` = System-Skill des Core (src/skills/**/SKILL.md).
//   - get / find / update / cancel / set_metadata / workspace.read wie §16.30 Nr. 2.
//   - complete(task_id, result) / fail(task_id, error): der „Core“ schreibt GENAU EINEN Befehl
//       {type: on_complete|on_failed, payload: {task_id, status, result, error?, workspace_refs?}}
//     an den Handler der eigenen App (Routing nach Typ). Höchstens einmal je task_id: jede weitere Zustellung wird
//     verworfen und in sim.log protokolliert.
//   - asset(path) → string: liest aus den Paketdateien der eigenen App (normalisiert, kein `..`/absolut, nur Dateien
//     des Artefakts – Testdateien gehören nicht dazu –, ≤ 1 MiB, UTF-8 streng).
//   - foreignTriggerAllowed(): Trigger auf fremde Collection nur mit exports:['watch'] beim Eigentümer und
//     foreign_collections-Deklaration (ops enthält 'watch') bei der auslösenden App (§16.34). Zustellung selbst nicht
//     simuliert.
//
// Benutzung im Handler-Test siehe README.md in diesem Ordner.

import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { collectAgentPackage, DEFAULT_REPO, isTestPath, listHandlers } from '../manifest/gen-manifest.mjs';

export const ENQUEUE_KEYS = Object.freeze(['title', 'instruction', 'skill', 'thread_key', 'payload', 'priority', 'idempotency_key', 'on_complete', 'on_failed']);
export const UPDATE_KEYS = Object.freeze(['title', 'instruction', 'priority', 'payload', 'thread_key']);
export const ASSET_MAX_BYTES = 1024 * 1024;
const TERMINAL = new Set(['completed', 'failed', 'cancelled']);

export function simError(code, message, detail) {
  const err = new Error(message);
  err.name = 'CtoxError';
  err.code = code;
  if (detail !== undefined) err.detail = detail;
  return err;
}

const isPlainObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const nonEmpty = (v) => typeof v === 'string' && v.trim() !== '';
const clone = (v) => (v === undefined ? undefined : structuredClone(v));

/** Namen der System-Skills des Core (Frontmatter `name:` und Ordnername unter src/skills/**). */
export function loadCoreSkillNames(repoRoot = DEFAULT_REPO) {
  const names = new Set();
  const stack = [path.join(repoRoot, 'src/skills')];
  while (stack.length) {
    const dir = stack.pop();
    let entries;
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { continue; }
    for (const e of entries) {
      const abs = path.join(dir, e.name);
      if (e.isDirectory()) { stack.push(abs); continue; }
      if (e.name !== 'SKILL.md') continue;
      names.add(path.basename(dir));
      const head = fs.readFileSync(abs, 'utf8').slice(0, 2000);
      const m = /^name:\s*["']?([^"'\n]+?)["']?\s*$/m.exec(head);
      if (m) names.add(m[1].trim());
    }
  }
  return names;
}

/** Paketpfad normalisieren (§16.34): relativ, ohne `..`/`.`/leere Segmente, ohne Backslash und NUL. */
export function normalizeAssetPath(p) {
  if (typeof p !== 'string' || !p) throw simError('invalid', 'asset: Pfad fehlt');
  if (p.startsWith('/') || /^[A-Za-z]:/.test(p) || p.includes('\\') || p.includes('\0')) throw simError('invalid', `asset: unzulässiger Pfad ${JSON.stringify(p)}`);
  const rel = p.replace(/^(\.\/)+/, '');
  const parts = rel.split('/');
  if (parts.some((s) => s === '' || s === '.' || s === '..')) throw simError('invalid', `asset: unzulässiger Pfad ${JSON.stringify(p)}`);
  return rel;
}

/**
 * Trigger auf eine Collection zulässig? Eigene Collection immer; fremde nur, wenn der Eigentümer sie mit 'watch'
 * exportiert und die auslösende App sie mit owner_app und ops ⊇ ['watch'] in foreign_collections deklariert.
 */
export function foreignTriggerAllowed({ appManifest, ownerManifest, collection }) {
  if (appManifest?.collections && Object.prototype.hasOwnProperty.call(appManifest.collections, collection)) return { ok: true, reason: 'own' };
  const decl = appManifest?.foreign_collections?.[collection];
  if (!decl) return { ok: false, reason: 'not_declared' };
  if (!ownerManifest || decl.owner_app !== ownerManifest.app_id) return { ok: false, reason: 'owner_mismatch' };
  if (!Array.isArray(decl.ops) || !decl.ops.includes('watch')) return { ok: false, reason: 'watch_not_declared' };
  const exported = ownerManifest.exports?.[collection];
  if (!Array.isArray(exported) || !exported.includes('watch')) return { ok: false, reason: 'watch_not_exported' };
  return { ok: true, reason: 'foreign_watch' };
}

/**
 * @param {object} o
 * @param {string} o.appId            eigene App
 * @param {string} [o.appDir]         Paketordner (Dateien, handlers/, skills/ falls kein Manifest)
 * @param {object} [o.manifest]       Manifest mit skills:[{name,path}] (sonst aus dem Paket abgeleitet)
 * @param {Record<string,string>} [o.files] Paketdateien im Speicher (statt appDir)
 * @param {Record<string,Function|{default:Function}>} [o.handlers] Handler je Befehlstyp (statt appDir/handlers)
 * @param {(command)=>object|Promise<object>} [o.ctxFactory] ctx für Callback-Handler (sim hängt agent.task/asset an)
 * @param {Iterable<string>} [o.coreSkills] System-Skills des Core (Standard: src/skills/** des Repos)
 */
export function createAgentTaskSim({ appId, appDir = null, manifest = null, files = null, handlers = null, ctxFactory = null, coreSkills } = {}) {
  if (!nonEmpty(appId)) throw new Error('createAgentTaskSim: appId fehlt');
  const tasks = new Map();
  const enqueued = [];
  const deliveries = [];
  const log = [];
  const delivered = new Set();
  let seq = 0;
  let core = coreSkills ? new Set(coreSkills) : null;
  const coreNames = () => (core ||= loadCoreSkillNames());

  const declaredSkills = manifest
    ? (Array.isArray(manifest.skills) ? manifest.skills : [])
    : files
      ? Object.keys(files).map((f) => /^skills\/([^/]+)\/SKILL\.md$/.exec(f)).filter(Boolean).map((m) => ({ name: m[1], path: m[0] }))
      : appDir ? collectAgentPackage(appDir).skills : [];
  const handlerTypes = handlers ? new Set(Object.keys(handlers)) : appDir ? new Set(Object.keys(listHandlers(appDir))) : null;

  function readPackage(rel) {
    if (files) {
      if (!Object.prototype.hasOwnProperty.call(files, rel)) return null;
      const v = files[rel];
      return typeof v === 'string' ? Buffer.from(v, 'utf8') : Buffer.from(v);
    }
    if (!appDir) return null;
    const abs = path.join(appDir, rel);
    let real;
    try { real = fs.realpathSync(abs); } catch { return null; }
    const root = fs.realpathSync(appDir);
    if (real !== root && !real.startsWith(root + path.sep)) return null;
    if (!fs.statSync(real).isFile()) return null;
    return fs.readFileSync(real);
  }

  function asset(p) {
    const rel = normalizeAssetPath(p);
    const buf = isTestPath(rel) ? null : readPackage(rel);
    if (!buf) throw simError('not_found', `asset: ${rel} ist nicht im Artefakt von ${appId}`);
    if (buf.length > ASSET_MAX_BYTES) throw simError('invalid', `asset: ${rel} ist größer als 1 MiB (${buf.length} Bytes)`);
    try { return new TextDecoder('utf-8', { fatal: true }).decode(buf); } catch { throw simError('invalid', `asset: ${rel} ist kein UTF-8 (Binäres über files)`); }
  }

  function resolveSkill(skill) {
    if (skill === undefined || skill === null) return null;
    if (!nonEmpty(skill)) throw simError('invalid', 'enqueue: skill muss ein Name sein');
    if (!skill.includes('/')) {
      if (!coreNames().has(skill)) throw simError('invalid', `enqueue: System-Skill ${skill} existiert nicht im Core`, { skill });
      return { skill, scope: 'core' };
    }
    const [ns, name, ...rest] = skill.split('/');
    if (rest.length || !ns || !name) throw simError('invalid', `enqueue: Skill-Name ${skill} hat nicht die Form <app_id>/<name>`, { skill });
    if (ns !== appId) throw simError('invalid', `enqueue: fremder Skill-Namensraum ${ns} (eigene App ${appId})`, { skill });
    const decl = declaredSkills.find((s) => s.name === name);
    if (!decl) throw simError('invalid', `enqueue: Skill ${skill} ist nicht im Manifest deklariert`, { skill });
    let rel;
    try { rel = normalizeAssetPath(decl.path); } catch { throw simError('invalid', `enqueue: Skill ${skill} hat einen ungültigen Pfad`, { skill }); }
    if (!readPackage(rel)) throw simError('invalid', `enqueue: Skill-Datei ${rel} fehlt im Paket`, { skill, path: rel });
    return { skill, scope: 'app', path: rel };
  }

  function checkCallbackType(key, value) {
    if (value === undefined || value === null) return;
    if (!nonEmpty(value)) throw simError('invalid', `enqueue: ${key} muss ein Befehlstyp sein`);
    if (handlerTypes && !handlerTypes.has(value)) throw simError('invalid', `enqueue: ${key} ${value} ist kein Befehlstyp der App ${appId} (kein Handler)`, { [key]: value });
  }

  const taskOf = (taskId) => {
    const t = tasks.get(taskId);
    if (!t) throw simError('not_found', `agent.task: ${taskId} unbekannt`);
    return t;
  };
  const snapshot = (t) => clone({ ...t });

  function api(command = null) {
    return {
      async enqueue(args) {
        if (!isPlainObject(args)) throw simError('invalid', 'enqueue: Argument muss ein Objekt sein');
        const unknown = Object.keys(args).filter((k) => !ENQUEUE_KEYS.includes(k));
        if (unknown.length) throw simError('invalid', `enqueue: unbekannte Felder ${unknown.join(', ')}`, { unknown });
        if (!nonEmpty(args.title)) throw simError('invalid', 'enqueue: title fehlt');
        if (!nonEmpty(args.instruction)) throw simError('invalid', 'enqueue: instruction fehlt');
        if (!isPlainObject(args.payload)) throw simError('invalid', 'enqueue: payload muss ein Objekt sein');
        for (const k of ['thread_key', 'priority', 'idempotency_key']) {
          if (args[k] !== undefined && args[k] !== null && !nonEmpty(args[k])) throw simError('invalid', `enqueue: ${k} muss Text sein`);
        }
        const resolved = resolveSkill(args.skill);
        checkCallbackType('on_complete', args.on_complete);
        checkCallbackType('on_failed', args.on_failed);
        if (nonEmpty(args.idempotency_key)) {
          const prev = [...tasks.values()].find((t) => t.idempotency_key === args.idempotency_key);
          if (prev) {
            log.push({ event: 'idempotent_hit', task_id: prev.task_id, idempotency_key: args.idempotency_key });
            return { task_id: prev.task_id };
          }
        }
        seq += 1;
        const taskId = `task_${appId}_${seq}`;
        const task = {
          task_id: taskId,
          app_id: appId,
          command_id: command?.id ?? null,
          status: 'queued',
          title: args.title,
          instruction: args.instruction,
          skill: resolved?.skill ?? null,
          thread_key: args.thread_key ?? null,
          payload: clone(args.payload),
          priority: args.priority ?? null,
          idempotency_key: args.idempotency_key ?? null,
          on_complete: args.on_complete ?? null,
          on_failed: args.on_failed ?? null,
          metadata: {},
        };
        tasks.set(taskId, task);
        enqueued.push({ task_id: taskId, command_id: task.command_id, args: clone(args), skill: resolved });
        log.push({ event: 'enqueued', task_id: taskId });
        return { task_id: taskId };
      },
      async get({ task_id } = {}) { return snapshot(taskOf(task_id)); },
      async find(args = {}) {
        const keys = ['idempotency_key', 'command_id'].filter((k) => args[k] !== undefined);
        if (keys.length !== 1 || !nonEmpty(args[keys[0]])) throw simError('invalid', 'find: genau eines von idempotency_key oder command_id');
        const t = [...tasks.values()].find((x) => x[keys[0]] === args[keys[0]]);
        return t ? snapshot(t) : null;
      },
      async update({ task_id, ...fields } = {}) {
        const t = taskOf(task_id);
        const unknown = Object.keys(fields).filter((k) => !UPDATE_KEYS.includes(k));
        if (unknown.length) throw simError('invalid', `update: unzulässige Felder ${unknown.join(', ')}`, { unknown });
        if (t.status !== 'queued') throw simError('conflict', `update: Task ${task_id} ist ${t.status}`);
        for (const [k, v] of Object.entries(fields)) t[k] = clone(v);
        return snapshot(t);
      },
      async cancel({ task_id, status_note } = {}) {
        const t = taskOf(task_id);
        if (TERMINAL.has(t.status)) throw simError('conflict', `cancel: Task ${task_id} ist bereits ${t.status}`);
        t.status = 'cancelled';
        if (status_note !== undefined) t.status_note = status_note;
        log.push({ event: 'cancelled', task_id });
        return snapshot(t);
      },
      async set_metadata({ task_id, key, value } = {}) {
        const t = taskOf(task_id);
        if (!nonEmpty(key)) throw simError('invalid', 'set_metadata: key fehlt');
        t.metadata[key] = clone(value);
        return snapshot(t);
      },
      workspace: {
        async read({ task_id, path: p } = {}) {
          const t = taskOf(task_id);
          const rel = normalizeAssetPath(p);
          const ws = t.workspace || {};
          if (!Object.prototype.hasOwnProperty.call(ws, rel)) throw simError('not_found', `workspace.read: ${rel} fehlt in ${task_id}`);
          return ws[rel];
        },
      },
    };
  }

  function attach(ctx = {}, command = null) {
    ctx.cap ||= {};
    ctx.cap.agent ||= {};
    ctx.cap.agent.task = api(command);
    ctx.asset = asset;
    return ctx;
  }

  async function resolveHandler(type) {
    if (handlers) {
      const h = handlers[type];
      return typeof h === 'function' ? h : typeof h?.default === 'function' ? h.default : null;
    }
    if (!appDir) return null;
    const file = path.join(appDir, 'handlers', `${type}.js`);
    if (!fs.existsSync(file)) return null;
    const mod = await import(pathToFileURL(file).href);
    return typeof mod.default === 'function' ? mod.default : null;
  }

  async function settle(taskId, outcome, { result = null, error = null, workspace = null } = {}) {
    const t = taskOf(taskId);
    if (delivered.has(taskId) || TERMINAL.has(t.status)) {
      const reason = delivered.has(taskId) ? 'duplicate' : 'terminal';
      log.push({ event: 'callback_dropped', task_id: taskId, outcome, reason });
      return { delivered: false, reason };
    }
    t.status = outcome;
    if (isPlainObject(workspace)) t.workspace = Object.fromEntries(Object.entries(workspace).map(([k, v]) => [normalizeAssetPath(k), v]));
    const type = outcome === 'completed' ? t.on_complete : t.on_failed;
    if (!type) {
      log.push({ event: 'no_callback', task_id: taskId, outcome });
      return { delivered: false, reason: 'no_callback' };
    }
    delivered.add(taskId);
    const payload = { task_id: taskId, status: outcome, result: outcome === 'completed' ? clone(result) : null };
    if (outcome === 'failed') payload.error = typeof error === 'string' ? { code: 'failed', message: error } : clone(error ?? { code: 'failed', message: 'failed' });
    if (t.workspace) payload.workspace_refs = Object.keys(t.workspace).sort();
    const cmd = { id: `cmd_callback_${taskId}`, type, app_id: appId, module: appId, record_id: null, payload };
    const entry = { task_id: taskId, command: clone(cmd) };
    deliveries.push(entry);
    log.push({ event: 'callback', task_id: taskId, type });
    const handler = await resolveHandler(type);
    if (!handler) {
      entry.error = simError('not_found', `kein Handler für ${type} in ${appId}`);
      return { delivered: true, command: cmd, error: entry.error };
    }
    const ctx = attach(ctxFactory ? await ctxFactory(cmd) : {}, cmd);
    try {
      entry.result = await handler(cmd, ctx);
      return { delivered: true, command: cmd, result: entry.result, ctx };
    } catch (err) {
      entry.error = err;
      log.push({ event: 'callback_handler_error', task_id: taskId, type, code: err?.code ?? null });
      return { delivered: true, command: cmd, error: err, ctx };
    }
  }

  return {
    appId,
    tasks,
    enqueued,
    deliveries,
    log,
    declaredSkills,
    api,
    asset,
    attach,
    markRunning(taskId) { const t = taskOf(taskId); if (t.status === 'queued') t.status = 'running'; return snapshot(t); },
    complete(taskId, result, options = {}) { return settle(taskId, 'completed', { ...options, result }); },
    fail(taskId, error, options = {}) { return settle(taskId, 'failed', { ...options, error }); },
  };
}
