// Accounting is committed in the same transaction as its source row.
export const CACHE_USAGE_KEY = 'cache:usage:v1';
export const CACHE_COLLECTION_PREFIX = 'cache:collection:';
export const CACHE_BLOB_PREFIX = 'cache:blob:';
export const CACHE_OUTBOX_PREFIX = 'cache:outbox:';
export const CACHE_UPLOAD_PREFIX = 'cache:upload:';
export const CACHE_OUTBOX_READY_KEY = 'cache:outbox-ready:v1';
export const CACHE_REJECT_PREFIX = 'cache:rejected:';
const encoder = new TextEncoder();
export const cacheBytes = row => !row || row.cache_evicted ? 0 : encoder.encode(JSON.stringify(row)).byteLength;
const pinned = row => Number(!!(row?.legacy_unconfirmed || row?.local_file_rejection));
export function cacheAccounting(ops) {
 const adjust = async (store, before, after) => {
  if(store==='outbox'){
   for(const [row,delta] of [[before,-1],[after,1]])if(row){
    const keys=[CACHE_OUTBOX_PREFIX+JSON.stringify(row.collection)];
    if(row.op==='upload')keys.push(CACHE_UPLOAD_PREFIX+JSON.stringify(row.id));
    for(const key of keys){const count=((await ops.get('meta',key))?.count??0)+delta;
     if(count)await ops.put('meta',{key,count});else await ops.del('meta',key);}
   }
   return;
  }
  const oldSize=store==='docs'?cacheBytes(before):(before?.size??before?.blob?.size??0);
  const newSize=store==='docs'?cacheBytes(after):(after?.size??after?.blob?.size??0);
  const total=await ops.get('meta',CACHE_USAGE_KEY);
  // Existing IDB databases are initialized once before their handle is exposed.
  if(!total)throw new Error('Cache accounting not initialized');
  await ops.put('meta',{key:CACHE_USAGE_KEY,value:total.value+newSize-oldSize});
  if(store==='docs') {
   const collection=(after??before).collection,key=CACHE_COLLECTION_PREFIX+collection;
   const group=await ops.get('meta',key)??{key,cache_collection:collection,bytes:0,pinned:0};
   await ops.put('meta',{...group,bytes:group.bytes+newSize-oldSize,pinned:group.pinned+pinned(after)-pinned(before)});
   for(const [row,delta] of [[before,-1],[after,1]])if(row?.local_file_rejection && row.data?.ref){
    const key=CACHE_REJECT_PREFIX+row.data.ref,old=await ops.get('meta',key);
    const count=(old?.count??0)+delta;
    if(count)await ops.put('meta',{key,ref:row.data.ref,count});else await ops.del('meta',key);
   }
  } else {
   const key=CACHE_BLOB_PREFIX+(after??before).ref;
   if(after)await ops.put('meta',{key,ref:after.ref,bytes:newSize,pinned:!!after.pinned,at:after.last_used??0});
   else await ops.del('meta',key);
  }
 };
 return {...ops,
  put:async(s,v)=>{if(s==='docs'||s==='blobs'||s==='outbox')await adjust(s,await ops.get(s,s==='docs'?[v.collection,v.id]:s==='outbox'?v.op_id:v.ref),v);return ops.put(s,v);},
  del:async(s,k)=>{if(s==='docs'||s==='blobs'||s==='outbox'){const old=await ops.get(s,k);if(old)await adjust(s,old,null);}return ops.del(s,k);},
 };
}
