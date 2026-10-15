import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { CtoxError, invalid, mango } from './mango.mjs';
export { CtoxError, mango };
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const own = (obj, key) => Object.hasOwn(obj ?? {}, key);
const clone = value => structuredClone(value);
const fail = (code, message) => { throw new CtoxError(code, message); };
const ops = ['read', 'write'];
const declared = (names, name) => names.some(prefix => name === prefix || name.startsWith(prefix + '.'));
const resolveCapability = (tree, name) => own(tree, name) ? tree[name] : name.split('.').reduce((value, key) => own(value, key) ? value[key] : undefined, tree);
function json(value, limit = 1024 * 1024) {
  let text;
  try { text = JSON.stringify(value); } catch { invalid('Result must be JSON serializable'); }
  if (text === undefined) invalid('Result must be JSON serializable');
  if (Buffer.byteLength(text) > limit) fail('quota', 'Result exceeds 1 MiB');
  return JSON.parse(text);
}
export function validateManifest(manifest) {
  if (!object(manifest) || typeof manifest.app_id !== 'string' || !manifest.app_id.trim() || !object(manifest.collections ?? {}) || !object(manifest.handlers ?? {})) invalid('Invalid app manifest');
  if (!Array.isArray(manifest.capabilities_required ?? []) || (manifest.capabilities_required ?? []).some(name => typeof name !== 'string' || !/^[a-z][a-z0-9_-]*(?:\.[a-z][a-z0-9_-]*)*$/.test(name))) invalid('Invalid capability declaration');
  for (const name of ['collections', 'handlers', 'migrations', 'foreign_collections', 'exports', 'files']) if (own(manifest, name) && !object(manifest[name])) invalid('Invalid manifest section', name);
  for (const [name, spec] of Object.entries(manifest.collections ?? {})) if (!name || !object(spec)) invalid('Invalid collection schema', name);
  for (const value of Object.values(manifest.handlers ?? {})) if (typeof value !== 'string' || !value) invalid('Invalid handler declaration');
  for (const [name, rule] of Object.entries(manifest.foreign_collections ?? {})) {
    if (own(manifest.collections, name) || !object(rule) || typeof rule.owner_app !== 'string' || !rule.owner_app || rule.owner_app === manifest.app_id || !Array.isArray(rule.ops) || rule.ops.some(op => !ops.includes(op))) invalid('Invalid foreign collection declaration', name);
  }
  for (const [name, grants] of Object.entries(manifest.exports ?? {})) if (!own(manifest.collections, name) || !Array.isArray(grants) || grants.some(op => !ops.includes(op))) invalid('Invalid collection export', name);
  return manifest;
}
export function createContext(manifest, fixtures = {}, { migration = false } = {}) {
  validateManifest(manifest);
  const collections = fixtures.collections ??= Object.create(null);
  const checkPolicy = async request => {
    if (fixtures.policy && await fixtures.policy({ app_id: manifest.app_id, actor: fixtures.actor, ...request }) !== true) fail('forbidden', 'Actor policy rejected operation');
  };
  const collectionAccess = async (name, operation) => {
    if (typeof name !== 'string') invalid('Collection must be named');
    if (!own(manifest.collections, name)) {
      const rule = manifest.foreign_collections?.[name];
      const owner = rule && fixtures.apps?.[rule.owner_app];
      if (!rule?.ops.includes(operation) || !owner || owner.app_id !== rule.owner_app || !own(owner.collections, name) || !owner.exports?.[name]?.includes(operation)) fail('forbidden', 'Foreign collection is not declared and exported for ' + operation + ': ' + name);
      if (!own(collections, name)) fail('not_found', 'Foreign collection fixture is unavailable: ' + name);
    }
    await checkPolicy({ operation, collection: name });
    if (!own(collections, name)) collections[name] = [];
    if (!Array.isArray(collections[name])) invalid('Collection fixture must be an array', name);
    return collections[name];
  };
  const cap = async (name, args, implementation) => {
    if (migration) fail('forbidden', 'Migration functions cannot invoke capabilities');
    if (!declared(manifest.capabilities_required ?? [], name) || !declared(fixtures.grants ?? [], name)) fail('forbidden', 'Capability is not declared and approved: ' + name);
    await checkPolicy({ operation: 'capability', capability: name });
    const fn = implementation ?? resolveCapability(fixtures.capabilities, name);
    if (typeof fn !== 'function') fail('unavailable', 'Native capability fixture is unavailable: ' + name);
    return clone(await fn(...clone(args)));
  };
  const namespace = prefix => new Proxy(() => {}, {
    get: (_, key) => {
      if (key === 'then' || typeof key === 'symbol') return undefined;
      if (!/^[a-z][a-z0-9_-]*$/.test(key) || ['constructor', 'prototype', '__proto__'].includes(key)) invalid('Invalid capability name');
      return namespace(prefix ? prefix + '.' + key : key);
    },
    apply: (_, _this, args) => cap(prefix, args),
  });
  const context = {
    async read(name, query = { selector: {} }) {
      const docs = await collectionAccess(name, 'read');
      return mango(docs, query);
    },
    async write(name, op) {
      if (migration) fail('forbidden', 'Migration functions return proposals and cannot write');
      const docs = await collectionAccess(name, 'write');
      if (!object(op) || !['put', 'patch', 'remove'].includes(op.type)) invalid('Write requires type put, patch or remove');
      const id = op.id ?? op.doc?.id;
      if (typeof id !== 'string' || !id) invalid('Write requires a document id');
      const index = docs.findIndex(doc => doc.id === id);
      const current = index < 0 ? undefined : docs[index];
      if (own(op, 'base_rev') && op.base_rev !== (current?.rev ?? null)) fail('conflict', 'Source revision differs');
      if (op.type !== 'put' && !current) fail('not_found', 'Document is unavailable');
      if (op.type === 'remove') {
        docs.splice(index, 1);
        return { status: 'accepted', id };
      }
      if (op.type === 'patch' && !object(op.patch)) invalid('Patch must be an object');
      const value = op.type === 'put' ? op.doc : { ...current, ...op.patch };
      if (!object(value) || value.id !== id) invalid('Write cannot change document identity');
      const rev = 'fixture:' + (fixtures.nextRevision = (fixtures.nextRevision ?? 0) + 1);
      const next = { ...json(value), rev };
      if (index < 0) docs.push(next); else docs[index] = next;
      return { status: 'accepted', id, rev };
    },
    async command(type, payload) {
      if (migration) fail('forbidden', 'Migration functions cannot dispatch commands');
      if (!own(manifest.handlers, type)) fail('forbidden', 'Command is not declared by this app');
      await checkPolicy({ operation: 'command', type });
      if (typeof fixtures.command !== 'function') fail('unavailable', 'Command fixture is unavailable');
      return clone(await fixtures.command(type, clone(payload)));
    },
    model: req => cap('model', [req], fixtures.model),
    http: req => cap('http', [req], fixtures.http),
    cap: namespace(''),
    log(level, message, fields = {}) {
      if (!['debug', 'info', 'warn', 'error'].includes(level) || typeof message !== 'string') invalid('Invalid log entry');
      (fixtures.logs ??= []).push({ app_id: manifest.app_id, level, message, fields: clone(fields) });
    },
  };
  return Object.freeze(context);
}
export async function loadFunction(manifest, handlerPath, fixtures) {
  if (typeof handlerPath !== 'string' || path.isAbsolute(handlerPath) || handlerPath.split(/[\\/]/).some(part => part === '..' || part === '') || handlerPath.includes('\\')) invalid('Handler path must be app-relative');
  const root = await fs.realpath(fixtures.root ?? process.cwd());
  const file = await fs.realpath(path.resolve(root, handlerPath)).catch(() => fail('not_found', 'Handler file is unavailable'));
  const relative = path.relative(root, file);
  if (relative.startsWith('..' + path.sep) || relative === '..' || path.isAbsolute(relative)) fail('forbidden', 'Handler escapes its app root');
  const actual = createHash('sha256').update(await fs.readFile(file)).digest('hex');
  if (manifest.files) {
    const expected = manifest.files[handlerPath];
    if (typeof expected !== 'string' || actual !== expected) fail('conflict', 'Handler differs from its manifest hash');
  }
  const url = pathToFileURL(file);
  url.searchParams.set('sha256', actual);
  const module = await import(url.href);
  const fn = module.default ?? module.handle ?? module.migrate;
  if (typeof fn !== 'function') invalid('Handler must export a function');
  return fn;
}
export async function runHandler(manifest, handlerPath, input, { fixtures = {} } = {}) {
  validateManifest(manifest);
  if (!Object.values(manifest.handlers ?? {}).includes(handlerPath)) fail('forbidden', 'Handler path is not declared');
  for (const name of manifest.capabilities_required ?? []) {
    if (!declared(fixtures.grants ?? [], name)) fail('forbidden', 'Required capability is not approved: ' + name);
    const implementation = name === 'model' ? fixtures.model : name === 'http' ? fixtures.http : resolveCapability(fixtures.capabilities, name);
    if (typeof implementation !== 'function' && !object(implementation)) fail('unavailable', 'Required capability needs a native update: ' + name);
  }
  const fn = await loadFunction(manifest, handlerPath, fixtures);
  return json(await fn(clone(input), createContext(manifest, fixtures)));
}
export async function runMigration(manifest, collection, docs, { fixtures = {} } = {}) {
  validateManifest(manifest);
  if (!own(manifest.collections, collection) || !Array.isArray(docs)) invalid('Migration requires an owned collection and document batch');
  const spec = manifest.migrations?.[collection];
  if (!object(spec) || typeof spec.migrate_fn !== 'string') invalid('Migration function is not declared');
  const migrate = await loadFunction(manifest, spec.migrate_fn, fixtures);
  const ctx = createContext(manifest, fixtures, { migration: true });
  const proposals = [];
  const ids = new Set();
  for (const original of docs) {
    if (!object(original) || typeof original.id !== 'string' || !original.id || ids.has(original.id) || typeof original.rev !== 'string' || !original.rev) invalid('Migration requires unique ids and source revisions');
    ids.add(original.id);
    const result = await migrate(clone(original), ctx);
    if (object(result) && result.archive === true && Object.keys(result).length === 1) proposals.push({ id: original.id, base_rev: original.rev, action: 'archive', archived_doc: clone(original) });
    else {
      if (!object(result) || result.id !== original.id) invalid('Migration cannot change document identity');
      proposals.push({ id: original.id, base_rev: original.rev, action: 'write', doc: json(result) });
    }
  }
  return json(proposals);
}
