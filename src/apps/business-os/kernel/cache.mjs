import { CtoxError } from './values.mjs';
import {CACHE_USAGE_KEY,CACHE_COLLECTION_PREFIX,CACHE_BLOB_PREFIX,CACHE_REJECT_PREFIX} from './cache-accounting.mjs';
export const CLIENT_CACHE_BUDGET = 2_000_000_000;
const yieldTurn=()=>new Promise(resolve=>setTimeout(resolve,0));
function empty(row,lseq){return {collection:row.collection,id:row.id,data:null,base:{data:null,deleted:false,rev:null,seq:'0'},rev:null,seq:'0',deleted:true,pending:false,cache_evicted:true,lseq};}
async function invalidate(t,collections){
 for(const collection of collections)for(const meta of await t.checkpointsFor(collection)){
  if(meta.evicted && meta.cursor_to==='0' && !meta.complete)continue;
  await t.put('meta',{...meta,evicted_at_seq:meta.cursor_to,cursor_to:'0',complete:false,evicted:true});
 }
}
async function canEvict(t,collection){
 const group=await t.get('meta',CACHE_COLLECTION_PREFIX+collection);
 return !group?.pinned && !await t.hasOutboxForCollection(collection);
}
/** Bounded transactions recheck leader and pending input before each slice.
 * Cursor invalidation and lseq tombstones remain atomic with each changed row.
 * New pending input between slices is retained; already evicted cache can replay.
 */
export async function evictCollection(db,collection,assertGeneration){
 const transaction=db.transaction?.bind(db)??db.tx.bind(db);const records=[];let cursor=null;
 do {
  const page=await transaction(async t=>{
   await assertGeneration(t);
   if(!await canEvict(t,collection)){
    if(!records.length)throw new CtoxError('quota','Collection has pending, unconfirmed or rejected input and cannot be evicted');
    return {rows:[],next:null};
   }
   const batch=await t.scan('docs',cursor,32,collection);let seq=(await t.get('meta','local_seq'))?.value??0;const rows=[];
   for(const row of batch.rows){const record=empty(row,++seq);await t.put('docs',record);rows.push(record);}
   if(rows.length || cursor===null){await invalidate(t,new Set([collection]));await t.put('meta',{key:'local_seq',value:seq});}
   return {rows,next:batch.next};
  },'readwrite');
  records.push(...page.rows);cursor=page.next;
  if(cursor!==null)await yieldTurn();
 }while(cursor!==null);
 return records;
}
/** The normal budget tick reads two small metadata rows, never documents.
 * Accounting includes the same conservative JSON bytes and blob sizes as before.
 * Only actual pressure enumerates LRU metadata; eviction yields between slices.
 * Protected input can exceed the unchanged budget and is surfaced, never erased.
 */
export async function reclaimCache(db,assertGeneration,lastUsed=new Map(),protectedCollections=new Set(),budget){
 const read=db.read.bind(db),transaction=db.transaction?.bind(db)??db.tx.bind(db);
 let usage=await read(async t=>{await assertGeneration(t);return (await t.get('meta',CACHE_USAGE_KEY)).value;});
 const records=[],collections=[],evictedBlobs=[];
 const result=()=>({reclaimed:!!(collections.length||evictedBlobs.length),records,collections,blobs:evictedBlobs,collection:collections[0],blob:evictedBlobs[0],usage_bytes:usage,within_budget:budget===undefined||usage<=budget});
 if(budget!==undefined && usage<=budget)return result();
 const metadata=[];let cursor=null;
 do {const page=await read(t=>t.scan('meta',cursor,32));metadata.push(...page.rows);cursor=page.next;if(cursor!==null)await yieldTurn();}while(cursor!==null);
 const rejectedRefs=new Set(metadata.filter(m=>m.key.startsWith(CACHE_REJECT_PREFIX)&&m.count).map(m=>m.ref));
 const candidates=[];
 for(const m of metadata){
  if(m.key.startsWith(CACHE_COLLECTION_PREFIX)&&m.bytes&&!m.pinned&&!protectedCollections.has(m.cache_collection))candidates.push({collection:m.cache_collection,at:lastUsed.get(m.cache_collection)??0});
  if(m.key.startsWith(CACHE_BLOB_PREFIX)&&!m.pinned&&!rejectedRefs.has(m.ref))candidates.push({blob:m.ref,at:m.at});
 }
 candidates.sort((a,b)=>a.at-b.at || Number(!!a.blob)-Number(!!b.blob));
 for(const candidate of candidates){
  if(budget!==undefined && usage<=budget)break;
  if(candidate.blob){
   const removed=await transaction(async t=>{
    await assertGeneration(t);const blob=await t.get('blobs',candidate.blob);
    if(!blob||blob.pinned||(await t.get('meta',CACHE_REJECT_PREFIX+candidate.blob))?.count||await t.hasUpload(candidate.blob))return false;
    await t.del('blobs',candidate.blob);return true;
   },'readwrite');
   if(removed)evictedBlobs.push(candidate.blob);
  }else{
   try {const rows=await evictCollection(db,candidate.collection,assertGeneration);if(rows.length){records.push(...rows);collections.push(candidate.collection);}}
   catch(error){if(error?.code!=='quota')throw error;}
  }
  usage=await read(async t=>(await t.get('meta',CACHE_USAGE_KEY)).value);
  if(budget===undefined && (collections.length||evictedBlobs.length))break;
  await yieldTurn();
 }
 return result();
}
