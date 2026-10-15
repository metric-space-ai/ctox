// Embedded by the native supervisor; no filesystem imports or runtime downloads.
import { SourceTextModule, createContext } from 'node:vm';
import { createInterface } from 'node:readline';

const LIMIT = 1024 * 1024;
const ERROR_CODES = new Set(['invalid', 'forbidden', 'not_found', 'conflict', 'revoked', 'expired', 'quota', 'unavailable']);
const send = frame => process.stdout.write(JSON.stringify(frame) + '\n');
const permission = process.permission;
if (!permission || !process.execArgv.includes('--permission') ||
    !process.execArgv.includes('--max-old-space-size=256') ||
    !process.execArgv.includes('--experimental-vm-modules') ||
    Number(process.versions.node.split('.')[0]) < 25 ||
    ['fs.read', 'fs.write', 'net', 'child', 'worker', 'addons'].some(name => permission.has(name))) {
  process.exit(78);
}
send({ type: 'ready', version: process.versions.node, permissions: 'empty' });
const pending = new Map();
let active = null;
let sequence = 0;
function rpc(callId, method, args) {
  if (active !== callId) return Promise.reject(new Error('Call is no longer active'));
  const callbackId = ++sequence;
  return new Promise((resolve, reject) => {
    pending.set(callbackId, { callId, resolve, reject });
    send({ type: 'ctx', call_id: callId, callback_id: callbackId, method, args });
  });
}
function context(callId) {
  const cap = name => new Proxy(() => {}, {
    get: (_, key) => key === 'then' || typeof key === 'symbol' ? undefined : cap(name ? name + '.' + key : key),
    apply: (_, _this, args) => rpc(callId, 'capability', [name, args]),
  });
  const logs = [];
  return { logs, ctx: Object.freeze({
    read: async (collection, query = { selector: {} }) => {
      const normalized = normalizeMango(query); mango([], normalized);
      const response = await rpc(callId, 'read', [collection, normalized]);
      if (Array.isArray(response)) return response;
      if (response?.kind === 'ctox.authorized_snapshot.v1' && Array.isArray(response.documents)) return mango(response.documents, normalized);
      throw Object.assign(new Error('Invalid Core read response'), { code: 'invalid' });
    },
    write: (collection, op) => rpc(callId, 'write', [collection, op]),
    command: (type, payload) => rpc(callId, 'command', [type, payload]),
    model: req => rpc(callId, 'capability', ['model', [req]]),
    http: req => rpc(callId, 'capability', ['http', [req]]),
    cap: cap(''),
    log: (level, message, fields = {}) => { const log = rpc(callId, 'log', [level, message, fields]); log.catch(() => {}); logs.push(log); },
  }) };
}
// One VM context and module generation per pooled app process.
const modules = new Map();
const sandbox = createContext(Object.create(null), {
  codeGeneration: { strings: false, wasm: false },
});
async function load(sources, entry) {
  function moduleFor(name) {
    if (!Object.hasOwn(sources, name)) throw new Error('Undeclared module: ' + name);
    if (!modules.has(name)) modules.set(name, new SourceTextModule(sources[name], {
      context: sandbox, identifier: 'ctox-app:/' + name,
      importModuleDynamically() { throw new Error('Dynamic imports are unavailable'); },
    }));
    return modules.get(name);
  }
  const entryModule = moduleFor(entry);
  if (entryModule.status === 'unlinked') await entryModule.link((specifier, parent) => {
    if (!specifier.startsWith('./') && !specifier.startsWith('../')) throw new Error('Only bundled relative imports are permitted');
    const resolved = new URL(specifier, parent.identifier);
    if (resolved.protocol !== 'ctox-app:' || !resolved.pathname.startsWith('/')) throw new Error('Invalid app import');
    return moduleFor(decodeURIComponent(resolved.pathname.slice(1)));
  });
  if (entryModule.status === 'linked') await entryModule.evaluate();
  if (entryModule.status === 'errored') throw entryModule.error;
  const fn = entryModule.namespace.default ?? entryModule.namespace.handle ?? entryModule.namespace.migrate;
  if (typeof fn !== 'function') throw new Error('Handler must export a function');
  return fn;
}
async function call(frame) {
  if (active !== null) throw new Error('Concurrent call on an app process');
  active = frame.id;
  try {
    const fn = await load(frame.sources, frame.path);
    const { ctx, logs } = context(frame.id);
    let value;
    if (frame.kind === 'migration') {
      value = [];
      for (const original of frame.input) {
        const doc = await fn(structuredClone(original), ctx);
        if (doc && doc.archive === true && Object.keys(doc).length === 1)
          value.push({ id: original.id, base_rev: original.rev, action: 'archive', archived_doc: original });
        else {
          if (!doc || doc.id !== original.id) throw new Error('Migration cannot change identity');
          value.push({ id: original.id, base_rev: original.rev, action: 'write', doc });
        }
      }
    } else value = await fn(frame.input, ctx);
    await Promise.all(logs);
    const encoded = JSON.stringify(value);
    if (encoded === undefined || Buffer.byteLength(encoded) > LIMIT) throw new Error('Result exceeds JSON/1 MiB contract');
    send({ type: 'result', id: frame.id, value: JSON.parse(encoded) });
  } catch (error) {
    send({ type: 'error', id: frame.id, error: { code: ERROR_CODES.has(error.code) ? error.code : 'invalid', message: String(error.message).slice(0, 2048) } });
  } finally {
    active = null;
    for (const [id, request] of pending) if (request.callId === frame.id) {
      pending.delete(id);
      request.reject(new Error('Handler returned before callback completed'));
    }
  }
}
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
lines.on('line', line => {
  if (Buffer.byteLength(line) > 8 * LIMIT) process.exit(78);
  try {
    const frame = JSON.parse(line);
    if (frame.type === 'ctx_result') {
      const request = pending.get(frame.callback_id);
      if (!request || request.callId !== frame.call_id) throw new Error('Unmatched callback');
      pending.delete(frame.callback_id);
      if (frame.error) request.reject(Object.assign(new Error(frame.error.message), { code: frame.error.code }));
      else request.resolve(frame.value);
    } else if (frame.type === 'call') {
      call(frame).catch(() => process.exit(78));
    } else throw new Error('Unknown protocol frame');
  } catch { process.exit(78); }
});
lines.on('close', () => process.exit(0));
