// Shared handler API, embedded unchanged into the supervised worker.
// All authority lives in the supplied Core/fixture bindings.
export function handlerContext(bindings, { migration = false, transaction = false } = {}) {
  let inTransaction = false;
  const error = (code, message) => { const e = new Error(message); e.code = code; throw e; };
  const outside = () => {
    if (inTransaction) error('forbidden', 'Use tx inside a transaction; external effects are forbidden');
  };
  const effect = () => { outside(); if (migration || transaction) error('forbidden', 'External effects are forbidden in migrations/transactions'); };
  const write = (collection, op) => {
    outside(); if (migration) error('forbidden', 'Migration cannot write');
    return bindings.write(collection, op);
  };
  const options = value => {
    if (value === undefined) return {};
    if (!value || typeof value !== 'object' || Array.isArray(value)) error('invalid', 'Options must be an object');
    return value;
  };
  const namespace = name => new Proxy(() => {}, {
    get: (_, key) => {
      if (key === 'then' || typeof key === 'symbol') return undefined;
      if (!/^[a-z][a-z0-9_-]*$/.test(key) || ['constructor', 'prototype', '__proto__'].includes(key)) error('invalid', 'Invalid capability name');
      return namespace(name ? name + '.' + key : key);
    },
    apply: (_, _this, args) => { effect(); return bindings.capability(name, args); },
  });
  const ctx = {
    read(collection, query = { selector: {} }) { outside(); return bindings.read(collection, query); },
    async get(collection, id) {
      if (typeof id !== 'string' || !id) error('invalid', 'Document id required');
      return (await ctx.read(collection, { selector: { id }, limit: 1 }))[0] ?? null;
    },
    write,
    put: (collection, doc, opts) => write(collection, { ...options(opts), type: 'put', doc }),
    patch: (collection, id, patch, opts) => write(collection, { ...options(opts), type: 'patch', id, patch }),
    remove: (collection, id, opts) => write(collection, { ...options(opts), type: 'remove', id }),
    async transaction(fn) {
      effect();
      if (typeof fn !== 'function') error('invalid', 'Transaction callback required');
      inTransaction = true;
      try {
        return await bindings.transaction(tx => fn(handlerContext(tx, { transaction: true })));
      } finally { inTransaction = false; }
    },
    async command(type, payload, opts = {}) {
      effect(); options(opts);
      if (Object.keys(opts).some(key => !['id','record_id','module','client_context'].includes(key))) error('invalid', 'Unknown command option');
      const result = await bindings.command(type, payload, opts);
      if (!result || typeof result.id !== 'string' || !['accepted','rejected'].includes(result.status)) error('invalid', 'Core command must return an acceptance result');
      return result;
    },
    model(req) { effect(); return bindings.model ? bindings.model(req) : bindings.capability('model', [req]); },
    http(req) { effect(); return bindings.http ? bindings.http(req) : bindings.capability('http', [req]); },
    cap: namespace(''),
    now: () => bindings.now(),
    uuid: () => bindings.uuid(),
    cursor: Object.freeze({
      get(key = 'default') { effect(); return bindings.cursor('get', key); },
      set(value, key = 'default') { effect(); return bindings.cursor('set', key, value); },
    }),
    log(level, message, data = {}) { outside(); if (transaction) error('forbidden', 'Transaction has no external effects'); return bindings.log(level, message, data); },
  };
  // A tx exposes precisely the transactional data API, not a nested transaction.
  if (transaction) return Object.freeze(Object.fromEntries(['read','get','write','put','patch','remove'].map(key => [key, ctx[key]])));
  return Object.freeze(ctx);
}
