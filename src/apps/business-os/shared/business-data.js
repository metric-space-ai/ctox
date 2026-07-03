/**
 * Shared CTOX DB helpers for Business OS modules.
 *
 * Modules should describe what data they need and render domain state. The
 * shell/runtime owns database handles, schema registration, foreground
 * collection priority, replication, and recovery. These helpers keep module
 * code on the live RxDB path (`find().$` / `findOne().$`) and return a cleanup
 * handle that is safe to call from mount() teardown.
 *
 * @example
 * const cleanup = liveList(ctx, 'projects', {
 *   selector: { status: { $ne: 'archived' } },
 *   sort: [{ updated_at_ms: 'desc' }],
 * }, (projects) => renderProjects(projects));
 *
 * @example
 * const cleanup = subscribeCollections(ctx, ['projects', 'project_items'], ({ snapshots }) => ({
 *   projects: snapshots.projects || [],
 *   items: snapshots.project_items || [],
 * }), (data) => renderProjectWorkbench(data));
 */

const DEFAULT_DEBOUNCE_MS = 50;

/**
 * @typedef {object} BusinessDataCleanup
 * @property {() => void} unsubscribe
 * @property {() => void} dispose
 */

/**
 * Subscribe to one live CTOX DB query.
 *
 * @param {object} ctx Business OS module context.
 * @param {string} collectionName CTOX DB collection name.
 * @param {object|string} query Mango query, primary key, or selector shorthand.
 * @param {(data: object[]|object|null) => void} onData Called on initial load and deltas.
 * @param {{ single?: boolean, optional?: boolean, includeDeleted?: boolean, onError?: (error: Error) => void }} options
 * @returns {BusinessDataCleanup}
 */
export function liveQuery(ctx, collectionName, query = {}, onData = () => {}, options = {}) {
  const collection = collectionFor(ctx, collectionName);
  if (!collection) {
    if (options.optional) return noopCleanup();
    throw new Error(`CTOX DB collection is not available: ${collectionName}`);
  }
  const rxQuery = options.single ? collection.findOne(query) : collection.find(query);
  const observable = rxQuery?.$;
  if (!observable?.subscribe) {
    if (options.optional) return noopCleanup();
    throw new Error(`CTOX DB collection is not observable: ${collectionName}`);
  }
  const subscription = observable.subscribe((value) => {
    try {
      onData(normalizeLiveValue(value, { includeDeleted: options.includeDeleted }));
    } catch (error) {
      reportError(options, error);
    }
  });
  return cleanupFromSubscription(subscription);
}

/**
 * Subscribe to a live list and render it.
 *
 * @param {object} ctx Business OS module context.
 * @param {string} collectionName CTOX DB collection name.
 * @param {object} query Mango query.
 * @param {(items: object[]) => void} render Render callback.
 * @param {{ optional?: boolean, includeDeleted?: boolean, onError?: (error: Error) => void }} options
 * @returns {BusinessDataCleanup}
 */
export function liveList(ctx, collectionName, query = {}, render = () => {}, options = {}) {
  return liveQuery(ctx, collectionName, query, (items) => {
    render(Array.isArray(items) ? items : []);
  }, { ...options, single: false });
}

/**
 * Subscribe to several collections and render one combined view model.
 *
 * The loader receives the latest per-collection snapshots and can return any
 * module-specific view model. `onData` is called after every debounced change
 * once every collection has emitted its initial snapshot.
 *
 * @param {object} ctx Business OS module context.
 * @param {string[]} collectionNames CTOX DB collection names.
 * @param {(input: { ctx: object, snapshots: Record<string, object[]>, changedCollection: string }) => any|Promise<any>} loader
 * @param {(data: any) => void} onData
 * @param {{ query?: object|((collectionName: string) => object), debounceMs?: number, requireInitial?: boolean, includeDeleted?: boolean, onError?: (error: Error) => void }} options
 * @returns {BusinessDataCleanup}
 */
export function subscribeCollections(ctx, collectionNames = [], loader = ({ snapshots }) => snapshots, onData = () => {}, options = {}) {
  const names = [...new Set((collectionNames || []).map((name) => String(name || '').trim()).filter(Boolean))];
  const cleanups = [];
  const snapshots = Object.fromEntries(names.map((name) => [name, []]));
  const seen = new Set();
  const requireInitial = options.requireInitial !== false;
  const debounceMs = Number.isFinite(Number(options.debounceMs))
    ? Math.max(0, Number(options.debounceMs))
    : DEFAULT_DEBOUNCE_MS;
  let disposed = false;
  let changedCollection = '';
  let timer = null;
  let runVersion = 0;

  const schedule = (name) => {
    changedCollection = name || changedCollection;
    if (requireInitial && seen.size < names.length) return;
    if (timer != null) return;
    timer = setTimer(() => {
      timer = null;
      void emit();
    }, debounceMs);
  };

  const emit = async () => {
    const version = ++runVersion;
    try {
      const data = await loader({
        ctx,
        snapshots: cloneSnapshots(snapshots),
        changedCollection,
      });
      if (disposed || version !== runVersion) return;
      onData(data);
    } catch (error) {
      reportError(options, error);
    }
  };

  try {
    for (const name of names) {
      const query = typeof options.query === 'function' ? options.query(name) : (options.query || {});
      cleanups.push(liveList(ctx, name, query, (items) => {
        snapshots[name] = items;
        seen.add(name);
        schedule(name);
      }, {
        includeDeleted: options.includeDeleted,
        onError: options.onError,
      }));
    }
  } catch (error) {
    for (const cleanup of cleanups) cleanup.unsubscribe();
    throw error;
  }

  const unsubscribe = () => {
    disposed = true;
    if (timer != null) {
      clearTimer(timer);
      timer = null;
    }
    for (const cleanup of cleanups) cleanup.unsubscribe();
  };
  return { unsubscribe, dispose: unsubscribe };
}

export function readCollection(ctx, collectionName, query = {}) {
  const collection = collectionFor(ctx, collectionName);
  if (!collection?.find) return Promise.resolve([]);
  return collection.find(query).exec().then((docs) => normalizeList(docs));
}

export function readDocument(ctx, collectionName, idOrQuery) {
  const collection = collectionFor(ctx, collectionName);
  if (!collection?.findOne) return Promise.resolve(null);
  return collection.findOne(idOrQuery).exec().then((doc) => normalizeDoc(doc));
}

function collectionFor(ctx, name) {
  return ctx?.db?.collection?.(name) || null;
}

function normalizeLiveValue(value, options = {}) {
  if (Array.isArray(value)) return normalizeList(value, options);
  return normalizeDoc(value, options);
}

function normalizeList(docs, options = {}) {
  return (docs || [])
    .map((doc) => normalizeDoc(doc, options))
    .filter((doc) => doc && (options.includeDeleted || (doc._deleted !== true && doc.is_deleted !== true)));
}

function normalizeDoc(doc) {
  return doc?.toJSON?.() || doc || null;
}

function cloneSnapshots(snapshots) {
  return Object.fromEntries(Object.entries(snapshots).map(([name, docs]) => [name, docs.slice()]));
}

function cleanupFromSubscription(subscription) {
  const unsubscribe = () => {
    try { subscription?.unsubscribe?.(); } catch {}
  };
  return { unsubscribe, dispose: unsubscribe };
}

function noopCleanup() {
  return { unsubscribe: () => {}, dispose: () => {} };
}

function reportError(options, error) {
  if (typeof options?.onError === 'function') {
    options.onError(error);
    return;
  }
  console.warn('[business-data] live data callback failed', error);
}

function setTimer(fn, ms) {
  const timer = setTimeout(fn, ms);
  timer?.unref?.();
  return timer;
}

function clearTimer(timer) {
  clearTimeout(timer);
}
