import { CtoxError } from './values.mjs';
export const CLIENT_CACHE_BUDGET = 2_000_000_000;
const encoder=new TextEncoder();
const bytes=row=>row.cache_evicted?0:encoder.encode(JSON.stringify(row)).byteLength;
function empty(row,lseq){return {collection:row.collection,id:row.id,data:null,base:{data:null,deleted:false,rev:null,seq:'0'},rev:null,seq:'0',deleted:true,pending:false,cache_evicted:true,lseq};}
async function invalidate(t,collections){
 for(const meta of await t.all('meta'))if(meta.key.startsWith('cursor:') && collections.has(meta.collection))await t.put('meta',{...meta,evicted_at_seq:meta.cursor_to,cursor_to:'0',complete:false,evicted:true});
}

/** Explicit collection eviction; no input, artifacts, archive or snapshots.
 * lseq tombstones repair readers even when the broadcast was lost.
 */
export async function evictCollection(db,collection,assertGeneration){
 return db.tx(async t=>{
  await assertGeneration(t);
  if((await t.all('outbox')).some(e=>e.collection===collection))throw new CtoxError('quota','Collection has pending writes and cannot be evicted');
  let seq=(await t.get('meta','local_seq'))?.value??0;const records=[];
  for(const row of await t.all('docs'))if(row.collection===collection){
   if(row.legacy_unconfirmed)throw new CtoxError('quota','Unconfirmed V3 input cannot be evicted');
   const record=empty(row,++seq);await t.put('docs',record);records.push(record);
  }
  await invalidate(t,new Set([collection]));await t.put('meta',{key:'local_seq',value:seq});return records;
 });
}

/** One atomic LRU pass. Without `budget`, reclaim the oldest item for quota
 * recovery. With `budget`, reclaim enough rebuildable cache to meet the soft
 * byte budget. JSON byte accounting is conservative; pinned data can exceed it
 * and is reported, never silently erased. No query row limit is imposed.
 * `transaction` deliberately bypasses the quota retry hook.
 */
export async function reclaimCache(db,assertGeneration,lastUsed=new Map(),protectedCollections=new Set(),budget){
 const transaction=db.transaction?.bind(db)??db.tx.bind(db);
 return transaction(async t=>{
  await assertGeneration(t);
  const outbox=await t.all('outbox'),docs=await t.all('docs'),blobs=await t.all('blobs');
  const pinned=new Set(outbox.map(e=>e.collection));
  for(const row of docs)if(row.legacy_unconfirmed)pinned.add(row.collection);
  const groups=new Map();let usage=0;
  for(const row of docs){
   const size=bytes(row);usage+=size;
   let group=groups.get(row.collection);
   if(!group){group={collection:row.collection,at:lastUsed.get(row.collection)??0,size:0,rows:[]};groups.set(row.collection,group);}
   group.size+=size;group.rows.push(row);
  }
  const candidates=[...groups.values()].filter(g=>g.size && !pinned.has(g.collection) && !protectedCollections.has(g.collection));
  for(const blob of blobs){
   const size=blob.size??blob.blob?.size??0;usage+=size;
   if(!blob.pinned && !outbox.some(e=>e.op==='upload' && e.id===blob.ref))candidates.push({blob:blob.ref,at:blob.last_used??0,size});
  }
  candidates.sort((a,b)=>a.at-b.at);
  let seq=(await t.get('meta','local_seq'))?.value??0;
  const records=[],collections=[],evictedBlobs=[];
  for(const candidate of candidates){
   if(budget!==undefined && usage<=budget)break;
   if(candidate.blob){await t.del('blobs',candidate.blob);evictedBlobs.push(candidate.blob);}
   else{
    collections.push(candidate.collection);
    for(const row of candidate.rows){const record=empty(row,++seq);await t.put('docs',record);records.push(record);}
   }
   usage=Math.max(0,usage-candidate.size);
   if(budget===undefined)break;
  }
  if(records.length){await invalidate(t,new Set(collections));await t.put('meta',{key:'local_seq',value:seq});}
  return {reclaimed:!!(collections.length||evictedBlobs.length),records,collections,blobs:evictedBlobs,
   collection:collections[0],blob:evictedBlobs[0],usage_bytes:usage,within_budget:budget===undefined || usage<=budget};
 },'readwrite');
}
