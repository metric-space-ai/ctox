import assert from 'node:assert/strict';
const api = await import(process.argv.includes('--source') ? '../src/index.mjs' : '../dist/ctox-rxdb-js.mjs');
import { normalizeQueryProjection, projectQueryDocument } from '../src/query-projection.mjs';
import { createMemoryProjectedQueryCache, PROJECTED_QUERY_WINDOW_MAX_BYTES } from '../src/query-projection-cache.mjs';
const { createRxDatabase, createQueryDemandLoader, createSidecarWithMemoryBackend, canonicalQueryJson } = api;

assert.deepEqual(normalizeQueryProjection(['payload.z', 'payload', 'id', 'payload.z']), ['id', 'payload']);
assert.throws(() => normalizeQueryProjection(['payload.__proto__.secret']), /safe/);
assert.equal(canonicalQueryJson({collection:'leads', projection:[]}), canonicalQueryJson({collection:'leads'}));
assert.equal(canonicalQueryJson({collection:'leads', projection:['payload.b','payload.a']}), canonicalQueryJson({collection:'leads', projection:['payload.a','payload.b']}));
assert.notEqual(canonicalQueryJson({collection:'leads', projection:['payload.a']}), canonicalQueryJson({collection:'leads'}));
assert.deepEqual(projectQueryDocument({id:'a', payload:{weitere_kampagnen:['x','y'], tasks:[{status:'open',secret:1},null,7],large:'excluded'}}, ['id','payload.weitere_kampagnen','payload.tasks.status']), {id:'a',payload:{weitere_kampagnen:['x','y'],tasks:[{status:'open'},null,null]}});

let now = 1000;
let digest = 'authorized-role-1';
let generation = 'bridge-1';
let nativeRows = [{id:'a', _rev:'1-a', _deleted:false, status:'open', payload:{title:'Alpha', weitere_kampagnen:['x','y'], full:'X'.repeat(20_000)}}];
const canonical = new Map();
const writes = [];
const storage = {
  databaseName:'projection-smoke', primaryPath:'id', schemaIndexes:()=>[], observe:()=>()=>{},
  async bulkWrite(rows){ writes.push(rows); for(const row of rows) canonical.set(row.id,structuredClone(row)); },
  async findDocumentsById(ids){ return Object.fromEntries(ids.filter(id=>canonical.has(id)).map(id=>[id,structuredClone(canonical.get(id))])); },
  async queryDocuments(){ return [...canonical.values()].map(row=>structuredClone(row)); },
  async allDocuments(){ return [...canonical.values()].map(row=>structuredClone(row)); },
};
const db = await createRxDatabase({name:'projection-smoke', storage:{nativeStorage:{collection:()=>storage, close(){}}}});
await db.addCollections({leads:{schema:{version:1,primaryKey:'id',type:'object',properties:{id:{type:'string'},status:{type:'string'},payload:{type:'object'}}}}});
const sidecar = createSidecarWithMemoryBackend({databaseName:'projection-meta',clock:()=>now});
const requests = [];
const loader = createQueryDemandLoader({storageCollection:storage,sidecar,collectionName:'leads',schemaVersion:1,clock:()=>now,
  queryGeneration:()=>generation,readPermissionDigest:()=>digest,
  onQueryWindowChanged:()=>db.leads.notifyQueryWindowChange(),
  requestQueryFetch:async request=>{
    requests.push(request);
    return {documents: request.projection ? nativeRows.map(row=>projectQueryDocument(row,request.projection)) : structuredClone(nativeRows),
      authoritativeRevision:request.queryFingerprint, ...(request.projection?{appliedProjection:request.projection}:{})};
  },
});
db.leads.setDemandLoader(loader);
const query = db.leads.find({selector:{},sort:[{id:'asc'}],limit:200,projection:['status','payload.title','payload.weitere_kampagnen']});
const [first,duplicate] = await Promise.all([query.exec(),query.exec()]);
assert.equal(requests.length,1,'concurrent projected callers must share one fetch');
assert.equal(writes.length,0,'partial rows must not enter canonical storage');
assert.equal(canonical.size,0);
assert.deepEqual(first[0].toJSON().payload,{title:'Alpha',weitere_kampagnen:['x','y']});
assert.equal(duplicate[0].payload.full,undefined);
for(const mutate of [()=>first[0].patch({status:'done'}),()=>first[0].remove(),()=>first[0].incrementalModify(row=>({...row,status:'done'})),()=>db.leads.upsert(first[0].toJSON()),()=>db.leads.bulkUpsert([first[0].toJSON()])]) {
  await assert.rejects(mutate,{code:'PROJECTED_DOCUMENT_READ_ONLY'});
}
const detached=first[0].toJSON(); detached.payload.title='caller mutation';
assert.equal((await query.exec())[0].payload.title,'Alpha','caller mutations must not alter cached rows');
assert.equal(requests.length,1);
const full=await db.leads.findOne('a').exec();
assert.equal(full.payload.full.length,20_000,'full detail must issue a separate full query');
assert.equal(writes.length,1);
assert.notEqual(requests[0].queryFingerprint,requests[1].queryFingerprint);
assert.equal((await db.leads.findOne({selector:{id:'a'},requireRevision:'edit-1'}).exec()).payload.full.length,20_000);
generation='bridge-2';
await db.leads.findOne({selector:{id:'a'},requireRevision:'edit-1'}).exec();
assert.equal(requests.length,4,'a replacement bridge must not reuse strict full hydration');

const emissions=[];
const subscription=query.$.subscribe(rows=>emissions.push(rows.map(row=>row.toJSON())));
await until(()=>emissions.some(rows=>rows[0]?.status==='open'));
nativeRows=[{...nativeRows[0],_rev:'2-a',status:'done',payload:{...nativeRows[0].payload,title:'Beta'}}];
await loader.invalidateDocuments(nativeRows);
await until(()=>emissions.some(rows=>rows[0]?.status==='done'));
assert.equal(canonical.get('a').status,'open','projected refresh must not overwrite the full record');
subscription.unsubscribe();

const beforeUnknown=requests.length;
digest='';
assert.deepEqual(await query.exec(),[],'unknown permission identity must not serve cached projection');
assert.equal(requests.length,beforeUnknown+1);
digest='authorized-role-2';
assert.equal((await query.exec())[0].status,'done');
const window=(await sidecar.backend.scanQueryWindows()).find(window=>window.projection);
assert.equal(window.permissionDigest,digest);
assert.ok(window.projectionKey);
await sidecar.backend.clear();
assert.equal((await query.exec())[0].status,'done','evicted metadata and payload must refetch');
await assert.rejects(db.leads.find({limit:201,projection:['status']}).exec(),{code:'PROJECTED_QUERY_WINDOW_TOO_LARGE'});

const legacyLoader=createQueryDemandLoader({storageCollection:storage,sidecar:createSidecarWithMemoryBackend({databaseName:'legacy'}),collectionName:'leads',readPermissionDigest:()=>digest,
  requestQueryFetch:async()=>({documents:structuredClone(nativeRows)})});
await assert.rejects(legacyLoader.resolveQuery({projection:['status']}),{code:'QUERY_PROJECTION_NOT_SUPPORTED'});
assert.equal(writes.length,3,'unconfirmed projection must not become canonical data');
db.leads.setDemandLoader(null);
await assert.rejects(db.leads.findOne({selector:{id:'a'},requireRevision:'edit-2'}).exec(),{code:'QUERY_GENERATION_REQUIRED'});

const cache=createMemoryProjectedQueryCache();
for(let index=0;index<65;index+=1) await cache.put('window-'+index,[{id:String(index)}],index);
assert.equal(await cache.get('window-0'),null,'window-count LRU must evict the oldest payload');
assert.ok(await cache.get('window-64'));
await assert.rejects(cache.put('oversize',[{id:'fat',value:'x'.repeat(PROJECTED_QUERY_WINDOW_MAX_BYTES)}]),{code:'PROJECTED_QUERY_WINDOW_TOO_LARGE'});
await cache.clear();
assert.equal(await cache.get('window-64'),null);
await db.close();
console.log('query projection smoke PASS: nested rows, isolated bounded cache, authority, strict hydration, mutation and live refresh');

async function until(predicate){const end=Date.now()+3000;while(!predicate()){if(Date.now()>end)throw new Error('live projection deadline');await new Promise(resolve=>setTimeout(resolve,10));}}
