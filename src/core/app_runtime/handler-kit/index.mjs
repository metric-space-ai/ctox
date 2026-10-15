import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { registerHooks } from 'node:module';
import { createHash, randomUUID, webcrypto } from 'node:crypto';
import { CtoxError, invalid, mango } from './mango.mjs';
import { handlerContext } from './context.mjs';
import { assetText } from './asset.mjs';
export { CtoxError, mango };
const object = v => v !== null && typeof v === 'object' && !Array.isArray(v);
const own = (obj, key) => Object.hasOwn(obj ?? {}, key);
const clone = v => structuredClone(v);
const fail = (code, message) => { throw new CtoxError({ code, message }); };
const declared = (names, name) => names.some(prefix => name === prefix || name.startsWith(prefix + '.'));
const resolveCapability = (tree, name) => own(tree, name) ? tree[name] : name.split('.').reduce((value, key) => own(value, key) ? value[key] : undefined, tree);
const states = new WeakMap();
const stateFor = f => { if (!states.has(f)) states.set(f, { revision: 0 }); return states.get(f); };
globalThis.crypto ??= webcrypto;
function json(value, limit = 1024 * 1024) {
  let text;
  try { text = JSON.stringify(value); } catch { invalid('Result must be JSON serializable'); }
  if (text === undefined) invalid('Result must be JSON serializable');
  if (Buffer.byteLength(text) > limit) fail('quota', 'Result exceeds 1 MiB');
  return JSON.parse(text);
}
export function handlerReference(ref, { library = false } = {}) {
  if (typeof ref !== 'string') invalid('Handler reference required');
  const [file, name = 'default', extra] = ref.split('#');
  if (extra !== undefined || !file || path.isAbsolute(file) || file.includes('\\') || file.split('/').some(p => !p || p === '.' || p === '..') || !/^[A-Za-z_$][\w$]*$/.test(name)) invalid('Invalid handler reference');
  if (!library && path.posix.basename(file).startsWith('_')) invalid('Library files cannot be handlers');
  return { file, name };
}
export function triggerInterval(value) {
  const m = typeof value === 'string' && /^P(?:(\d+(?:\.\d+)?)D)?(?:T(?:(\d+(?:\.\d+)?)H)?(?:(\d+(?:\.\d+)?)M)?(?:(\d+(?:\.\d+)?)S)?)?$/.exec(value);
  const ms = m && ((+m[1] || 0) * 86400 + (+m[2] || 0) * 3600 + (+m[3] || 0) * 60 + (+m[4] || 0)) * 1000;
  if (!Number.isSafeInteger(ms) || ms < 60000) invalid('Trigger interval must be an ISO duration >= PT1M');
  return ms;
}
export function validateManifest(manifest) {
  if (!object(manifest) || typeof manifest.app_id !== 'string' || !manifest.app_id.trim()) invalid('Invalid app manifest');
  for (const name of ['collections','handlers','migrations','foreign_collections','exports','files','command_rights']) if (own(manifest, name) && !object(manifest[name])) invalid('Invalid manifest section', name);
  if (!Array.isArray(manifest.capabilities_required ?? []) || (manifest.capabilities_required ?? []).some(name => typeof name !== 'string' || !/^[a-z][a-z0-9_-]*(?:\.[a-z][a-z0-9_-]*)*$/.test(name))) invalid('Invalid capability declaration');
  for (const [name, spec] of Object.entries(manifest.collections ?? {})) if (!name || !object(spec)) invalid('Invalid collection schema', name);
  for (const ref of Object.values(manifest.handlers ?? {})) handlerReference(ref);
  for (const [name, rule] of Object.entries(manifest.foreign_collections ?? {})) {
    if (own(manifest.collections,name) || !object(rule) || typeof rule.owner_app !== 'string' || !rule.owner_app || rule.owner_app === manifest.app_id || !Array.isArray(rule.ops) || rule.ops.some(op => !['read','write','watch'].includes(op))) invalid('Invalid foreign collection declaration', name);
  }
  for (const [name, grants] of Object.entries(manifest.exports ?? {})) if (!own(manifest.collections,name) || !Array.isArray(grants) || grants.some(op => !['read','write','watch'].includes(op))) invalid('Invalid collection export', name);
  if (!Array.isArray(manifest.triggers ?? [])) invalid('Invalid triggers');
  for (const trigger of manifest.triggers ?? []) {
    handlerReference(trigger?.handler);
    if (!!trigger.on === !!trigger.every) invalid('Trigger needs exactly one source');
    if (trigger.every) triggerInterval(trigger.every);
    if (trigger.on && (!['insert','update','delete'].includes(trigger.on.op) || (!own(manifest.collections,trigger.on.collection) && !manifest.foreign_collections?.[trigger.on.collection]?.ops.includes('watch')))) invalid('Invalid change trigger or missing watch grant');
  }
  for (const [type, rule] of Object.entries(manifest.command_rights ?? {})) {
    if (!type || !object(rule) || !Array.isArray(rule.roles) || rule.roles.some(v => typeof v !== 'string') || (rule.assigned_users !== undefined && typeof rule.assigned_users !== 'boolean')) invalid('Invalid command_rights');
  }
  return manifest;
}
function commandRights(manifest, command, fixtures) {
  const rules = Object.entries(manifest.command_rights ?? {}).filter(([p]) => p === command.type || p.endsWith('*') && command.type.startsWith(p.slice(0,-1))).sort(([a],[b]) => Number(b === command.type)-Number(a === command.type) || b.length-a.length);
  const rule = rules[0]?.[1];
  if (rule && !command.actor?.is_admin && !rule.roles.includes(command.actor?.role) && !(rule.assigned_users && fixtures.assigned_users?.includes(command.actor?.id))) fail('forbidden','Core command_rights rejected command');
}
export function createContext(manifest, fixtures = {}, { migration = false } = {}) {
  validateManifest(manifest);
  const collections = fixtures.collections ??= Object.create(null);
  const state = stateFor(fixtures);
  const policy = async request => {
    if (fixtures.policy && await fixtures.policy({ app_id:manifest.app_id, actor:fixtures.actor, ...request }) !== true) fail('forbidden','Actor policy rejected operation');
  };
  const access = async (name, operation) => {
    if (typeof name !== 'string') invalid('Collection must be named');
    if (!own(manifest.collections,name)) {
      const rule = manifest.foreign_collections?.[name], owner = rule && fixtures.apps?.[rule.owner_app];
      if (!rule?.ops.includes(operation) || !owner || owner.app_id !== rule.owner_app || !own(owner.collections,name) || !owner.exports?.[name]?.includes(operation)) fail('forbidden','Foreign collection is not declared and exported for '+operation+': '+name);
      if (!own(collections,name)) fail('not_found','Foreign collection fixture unavailable');
    }
    await policy({ operation, collection:name });
    collections[name] ??= [];
    if (!Array.isArray(collections[name])) invalid('Collection fixture must be an array', name);
    return collections[name];
  };
  const cap = async (name, args, implementation) => {
    if (!declared(manifest.capabilities_required ?? [],name) || !declared(fixtures.grants ?? [],name)) fail('forbidden','Capability is not declared and approved: '+name);
    await policy({ operation:'capability', capability:name });
    const fn = implementation ?? resolveCapability(fixtures.capabilities,name);
    if (typeof fn !== 'function') fail('unavailable','Native capability fixture unavailable: '+name);
    return clone(await fn(...clone(args)));
  };
  const bindings = {
    async asset(path) { await policy({ operation:'asset', path }); return assetText(manifest,path,fixtures); },
    async read(name, query) { return mango((await access(name,'read')).filter(doc => !doc.deleted && !doc._deleted), query); },
    async write(name, op) {
      const docs = await access(name,'write');
      if (!object(op) || !['put','patch','remove'].includes(op.type)) invalid('Write requires type put, patch or remove');
      const id = op.id ?? op.doc?.id;
      if (typeof id !== 'string' || !id) invalid('Write requires id');
      const index = docs.findIndex(doc => doc.id === id), current = index < 0 ? undefined : docs[index];
      if (current?.deleted) fail('conflict','Document was deleted');
      if (own(op,'base_rev') && op.base_rev !== (current?.rev ?? null)) fail('conflict','Source revision differs');
      if (op.type !== 'put' && !current) fail('not_found','Document unavailable');
      const rev = 'fixture:' + (fixtures.nextRevision = (fixtures.nextRevision ?? 0)+1);
      let next;
      if (op.type === 'remove') next = { id, rev, deleted:true };
      else {
        if (op.type === 'patch' && !object(op.patch)) invalid('Patch must be an object');
        const value = op.type === 'put' ? op.doc : { ...current, ...op.patch };
        if (!object(value) || value.id !== id) invalid('Write cannot change identity');
        next = { ...json(value), rev };
      }
      if (index < 0) docs.push(next); else docs[index] = next;
      state.revision++;
      return { status:'accepted', id, rev };
    },
    capability: cap,
    model: req => cap('model',[req],fixtures.model),
    http: req => cap('http',[req],fixtures.http),
    async command(type,payload,opts) {
      if (!own(manifest.handlers,type)) fail('forbidden','Command is not declared by this app');
      await policy({ operation:'command',type, options:opts });
      if (typeof fixtures.command !== 'function') fail('unavailable','Command fixture unavailable');
      return clone(await fixtures.command(type,clone(payload),clone(opts)));
    },
    now: () => typeof fixtures.now === 'function' ? fixtures.now() : fixtures.now ?? Date.now(),
    uuid: () => fixtures.uuid ? fixtures.uuid() : randomUUID(),
    async cursor(op,key,value) {
      if (typeof key !== 'string' || !key) invalid('Cursor key required');
      await policy({ operation:'cursor.'+op, key });
      if (fixtures.cursor?.[op]) return fixtures.cursor[op](...(op === 'get' ? [key] : [value,key]));
      const scope = fixtures.trigger_id ?? 'fixture';
      const cursors = fixtures.cursors ??= {};
      if (op === 'get') return clone(cursors[scope]?.[key] ?? null);
      (cursors[scope] ??= {})[key] = json(value); return { status:'accepted' };
    },
    log(level,message,fields = {}) {
      if (!['debug','info','warn','error'].includes(level) || typeof message !== 'string') invalid('Invalid log entry');
      (fixtures.logs ??= []).push({ app_id:manifest.app_id, level, message, fields:clone(fields) });
    },
    async transaction(fn) {
      const revision = state.revision;
      const working = { ...fixtures, collections:clone(collections), nextRevision:fixtures.nextRevision };
      const txContext = createContext(manifest,working);
      const result = await fn(txContext);
      if (state.revision !== revision) fail('conflict','Transaction snapshot changed');
      // Same actor policy is rechecked at the committing boundary.
      await policy({ operation:'transaction.commit' });
      if (state.revision !== revision) fail('conflict','Transaction snapshot changed');
      for (const name of Object.keys(collections)) delete collections[name];
      Object.assign(collections,working.collections);
      fixtures.nextRevision = working.nextRevision;
      state.revision++;
      return result;
    },
  };
  return handlerContext(bindings,{ migration });
}
export async function loadFunction(manifest, reference, fixtures = {}) {
  const { file, name } = handlerReference(reference);
  const files = new Map();
  const rootPrefix = file.startsWith('handlers/') ? 'handlers/' : path.posix.dirname(file) === '.' ? '' : path.posix.dirname(file)+'/';
  const add = (key, source) => {
    const bytes = Buffer.from(source);
    if (manifest.files && manifest.files[key] !== createHash('sha256').update(bytes).digest('hex')) fail('conflict','Module differs from its manifest hash');
    files.set(key,bytes.toString('utf8'));
  };
  if (fixtures.artifacts) {
    for (const [key, hash] of Object.entries(manifest.files ?? {})) if (key.startsWith(rootPrefix)) {
      const value = fixtures.artifacts instanceof Map ? fixtures.artifacts.get(hash) : fixtures.artifacts[hash];
      if (value === undefined) fail('not_found','CAS artifact unavailable: '+hash);
      add(key,value);
    }
  } else {
    const root = await fs.realpath(fixtures.root ?? process.cwd());
    const walk = async directory => {
      for (const entry of await fs.readdir(path.join(root,directory),{withFileTypes:true})) {
        const key = directory+entry.name;
        if (entry.isDirectory()) await walk(key+'/');
        else if (/\.(mjs|js)$/.test(key)) {
          const actual = await fs.realpath(path.join(root,key));
          if (!actual.startsWith(root+path.sep)) fail('forbidden','Handler escapes app root');
          add(key,await fs.readFile(actual));
        }
      }
    };
    await walk(rootPrefix);
  }
  if (!files.has(file)) fail('not_found','Handler file unavailable');
  const base = 'ctox-handler-kit://'+randomUUID()+'/';
  const hook = registerHooks({
    resolve(specifier,context,next) {
      if (specifier.startsWith(base)) return {url:specifier,shortCircuit:true};
      if (!context.parentURL?.startsWith(base)) return next(specifier,context);
      if (!specifier.startsWith('./') && !specifier.startsWith('../')) fail('forbidden','Only relative handler imports allowed');
      const url = new URL(specifier,context.parentURL);
      const key = decodeURIComponent(url.pathname.slice(1));
      if (url.host !== new URL(base).host || !key.startsWith(rootPrefix) || !files.has(key)) fail('forbidden','Import escapes handler bundle');
      return {url:url.href,shortCircuit:true};
    },
    load(url,context,next) {
      if (!url.startsWith(base)) return next(url,context);
      const key = decodeURIComponent(new URL(url).pathname.slice(1));
      if (!files.has(key)) fail('not_found','Module unavailable');
      return {format:'module',source:files.get(key),shortCircuit:true};
    },
  });
  try {
    const module = await import(base+file);
    const fn = name === 'default' ? module.default ?? module.handle ?? module.migrate : module[name];
    if (typeof fn !== 'function') invalid('Handler export must be a function');
    return fn;
  } finally { hook.deregister(); }
}
export async function runHandler(manifest,handlerPath,input,{fixtures = {}} = {}) {
  validateManifest(manifest); handlerReference(handlerPath);
  if (!Object.values(manifest.handlers ?? {}).includes(handlerPath) && !(manifest.triggers ?? []).some(t => t.handler === handlerPath) && !fixtures.completion) fail('forbidden','Handler is not declared');
  for (const name of manifest.capabilities_required ?? []) {
    if (!declared(fixtures.grants ?? [],name)) fail('forbidden','Required capability not approved');
    const implementation = name === 'model' ? fixtures.model : name === 'http' ? fixtures.http : resolveCapability(fixtures.capabilities,name);
    if (typeof implementation !== 'function' && !object(implementation)) fail('unavailable','Required capability needs native update');
  }
  const type = Object.keys(manifest.handlers ?? {}).find(key => manifest.handlers[key] === handlerPath);
  const command = fixtures.completion ? clone(input) : {
    id:input?.type ? input.id : randomUUID(), type:type ?? 'trigger', payload:clone(input),
    ...input?.type ? clone(input) : {}, actor:clone(fixtures.actor ?? input?.actor ?? {}),
    app_id:manifest.app_id, app_ver:manifest.app_ver ?? 'fixture',
  };
  if (!fixtures.completion) {
    if (typeof command.id !== 'string' || typeof command.type !== 'string') invalid('Command id/type required');
    commandRights(manifest,command,fixtures);
    if (fixtures.policy && await fixtures.policy({app_id:manifest.app_id,actor:command.actor,operation:'invoke',command}) !== true) fail('forbidden','Core admission denied command');
  }
  const fn = await loadFunction(manifest,handlerPath,fixtures);
  fixtures.actor = command.actor ?? fixtures.actor;
  return json(await fn(command,createContext(manifest,fixtures)));
}
export async function runMigration(manifest,collection,docs,{fixtures = {}} = {}) {
  validateManifest(manifest);
  if (!own(manifest.collections,collection) || !Array.isArray(docs)) invalid('Migration requires owned collection and document batch');
  const spec = manifest.migrations?.[collection];
  if (!object(spec) || typeof spec.migrate_fn !== 'string') invalid('Migration function not declared');
  const fn = await loadFunction(manifest,spec.migrate_fn,fixtures), ctx = createContext(manifest,fixtures,{migration:true}), proposals = [], ids = new Set();
  for (const original of docs) {
    if (!object(original) || typeof original.id !== 'string' || !original.id || ids.has(original.id) || typeof original.rev !== 'string' || !original.rev) invalid('Migration requires unique ids and source revisions');
    ids.add(original.id);
    const result = await fn(clone(original),ctx);
    if (object(result) && result.archive === true && Object.keys(result).length === 1) proposals.push({id:original.id,base_rev:original.rev,action:'archive',archived_doc:clone(original)});
    else { if (!object(result) || result.id !== original.id) invalid('Migration cannot change identity'); proposals.push({id:original.id,base_rev:original.rev,action:'write',doc:json(result)}); }
  }
  return json(proposals);
}
