import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const {createRxDatabase} = await import(process.argv.includes('--source') ? '../src/index.mjs' : '../dist/ctox-rxdb-js.mjs');

const shellSource = await readFile(new URL('../../app.js', import.meta.url), 'utf8');
const shellFunction = name => {
  const start = shellSource.indexOf('function ' + name + '(');
  assert(start >= 0, 'missing actual shell function ' + name);
  const end = shellSource.indexOf('\nfunction ', start + 1);
  assert(end > start, 'missing actual shell function boundary ' + name);
  return shellSource.slice(start, end);
};
const readMethods = new Set(['find', 'findOne']);
const writeMethods = new Set(['insert', 'upsert', 'bulkWrite']);
const wait = () => new Promise(resolve => setTimeout(resolve, 120));

for (const mode of ['direct', 'control-plane', 'maintenance-scope', 'permission-guard']) {
  const name = mode === 'control-plane' ? 'business_commands' : 'leads';
  const observers = new Set();
  const reads = {find:0, exec:0, query:0, all:0, byId:0, demand:0};
  const row = {id:'a', _rev:'1-a', _deleted:false, payload:'X'.repeat(50_000)};
  const storage = {
    primaryPath:'id', schemaIndexes:()=>[],
    observe(listener) { observers.add(listener); return () => observers.delete(listener); },
    async queryDocuments() { reads.query++; return [row]; },
    async allDocuments() { reads.all++; return [row]; },
    async findDocumentsById() { reads.byId++; return {a:row}; },
  };
  const db = await createRxDatabase({name:'invalidate-only-' + mode, storage:{nativeStorage:{collection:()=>storage, close(){}}}});
  await db.addCollections({[name]:{schema:{version:0, primaryKey:'id', type:'object', properties:{id:{type:'string'}, payload:{type:'string'}}}}});
  const collection = db.collection(name);
  const loader = {async resolveQuery() { reads.demand++; return [row]; }};
  collection.setDemandLoader(loader);
  const find = collection.find.bind(collection);
  collection.find = (...args) => {
    reads.find++;
    const query = find(...args);
    const exec = query.exec.bind(query);
    query.exec = (...options) => { reads.exec++; return exec(...options); };
    return query;
  };
  let handle = collection;
  if (mode === 'maintenance-scope') {
    const state = {db:{collection: requested => requested === name ? collection : null}, maintenance:{active:true}};
    const makeScoped = new Function('state', 'READ_COLLECTION_METHODS', 'WRITE_COLLECTION_METHODS',
      shellFunction('maintenanceReadOnlyCollection') + '\n' + shellFunction('createScopedSystemDbFacade') + '\nreturn createScopedSystemDbFacade;')(state, readMethods, writeMethods);
    const scoped = makeScoped('module:test', [name]);
    assert.equal(scoped.collection('business_users'), null, 'scope must continue rejecting unrelated collections');
    handle = scoped.collection(name);
  }
  if (mode === 'permission-guard') {
    let allowed = true;
    const guard = {};
    const assertPermission = (_guard, requested, permission) => {
      assert.equal(requested, name);
      assert.equal(permission, 'read');
      if (!allowed) throw new Error('READ_DENIED');
    };
    const makeGuarded = new Function('assertGuardedCollectionPermission', 'BusinessOsPermissions', 'READ_COLLECTION_METHODS', 'WRITE_COLLECTION_METHODS',
      shellFunction('createGuardedCollectionProxy') + '\nreturn createGuardedCollectionProxy;')(assertPermission, {DataRead:'read'}, readMethods, writeMethods);
    handle = makeGuarded(guard, name, collection);
    allowed = false;
    assert.throws(() => handle.$.subscribe(() => {}, {invalidateOnly:true}), /READ_DENIED/);
    assert.equal(observers.size, 0, 'denied subscription must not attach a store listener');
    allowed = true;
  }
  const hints = [];
  const subscription = handle.$.subscribe(event => hints.push(event), {invalidateOnly:true, emitPendingChanges:true});
  try {
    await wait();
    assert.deepEqual(hints, [{collectionName:name, invalidated:true}], mode + ': initial hint must contain no snapshot or document payload');
    assert.deepEqual(Object.values(reads), [0,0,0,0,0,0], mode + ': subscribing must make zero find/exec/storage/demand reads');
    for (let i=0; i<5; i++) for (const observer of observers) observer({success:{a:{...row, _rev:String(i+2)+'-a'}}});
    await wait();
    assert.equal(hints.length, 2, mode + ': burst store changes must coalesce to one hint');
    collection.notifyQueryWindowChange();
    await wait();
    assert.equal(hints.length, 3, mode + ': projected-window changes must invalidate without canonical writes');
    collection.setDemandLoader({...loader});
    await wait();
    assert.equal(hints.length, 4, mode + ': loader replacement must invalidate every collection');
    assert(hints.every(event => Object.keys(event).sort().join(',') === 'collectionName,invalidated'));
    assert.deepEqual(Object.values(reads), [0,0,0,0,0,0], mode + ': changes must make zero find/exec/storage/demand reads');
    subscription.unsubscribe();
    subscription.unsubscribe();
    assert.equal(observers.size, 0, mode + ': store listener must retire');
    collection.notifyQueryWindowChange();
    collection.setDemandLoader({...loader});
    await wait();
    assert.equal(hints.length, 4, mode + ': unsubscribe must cancel all future hints');
  } finally {
    subscription.unsubscribe();
    await db.close();
  }
}
console.log('collection invalidation-only smoke PASS: zero snapshot reads, scopes, permissions, store/window/generation hints and retirement');

