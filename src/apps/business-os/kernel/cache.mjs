import { CtoxError } from './values.mjs';

/** Rebuildable cache only. Outbox, rejected input, artifacts, window snapshots,
 * and unconfirmed uploads are never included in collection eviction.
 * Tombstones retain lseq so a reader missing the broadcast can reconcile.
 */
export async function evictCollection(db, collection, assertGeneration) {
  return db.tx(async t => {
    await assertGeneration(t);
    if((await t.all('outbox')).some(entry=>entry.collection===collection)) {
      throw new CtoxError('quota','Collection has pending writes and cannot be evicted');
    }
    let seq=(await t.get('meta','local_seq'))?.value??0;
    const records=[];
    for(const row of await t.all('docs'))if(row.collection===collection){
      if(row.legacy_unconfirmed)throw new CtoxError('quota','Unconfirmed V3 input cannot be evicted');
      const record={collection:row.collection,id:row.id,data:null,base:{data:null,deleted:false,rev:null,seq:'0'},rev:null,seq:'0',deleted:true,pending:false,cache_evicted:true,lseq:++seq};
      await t.put('docs',record);records.push(record);
    }
    for(const meta of await t.all('meta'))if(meta.key.startsWith('cursor:') && meta.collection===collection){
      await t.put('meta',{...meta,evicted_at_seq:meta.cursor_to,cursor_to:'0',complete:false,evicted:true});
    }
    await t.put('meta',{key:'local_seq',value:seq});
    return records;
  });
}

/** Reclaim one oldest rebuildable item, never user input or release artifacts.
 * `transaction` bypasses the quota retry hook intentionally.
 */
export async function reclaimCache(db, assertGeneration, lastUsed=new Map(), protectedCollections=new Set()) {
  const transaction=(db.transaction?.bind(db) ?? db.tx.bind(db));
  return transaction(async t=>{
    await assertGeneration(t);
    const outbox=await t.all('outbox'), docs=await t.all('docs');
    const pinned=new Set(outbox.map(e=>e.collection));
    for(const row of docs)if(row.legacy_unconfirmed)pinned.add(row.collection);
    const candidates=[];
    for(const row of docs)if(row.data && !pinned.has(row.collection) && !protectedCollections.has(row.collection) && !candidates.some(c=>c.collection===row.collection)) {
      candidates.push({collection:row.collection,at:lastUsed.get(row.collection)??0});
    }
    for(const blob of await t.all('blobs'))if(!blob.pinned && !outbox.some(e=>e.op==='upload' && e.id===blob.ref))candidates.push({blob:blob.ref,at:blob.last_used??0});
    candidates.sort((a,b)=>a.at-b.at);
    const candidate=candidates[0];
    if(!candidate)return {reclaimed:false,records:[]};
    if(candidate.blob){await t.del('blobs',candidate.blob);return {reclaimed:true,records:[],blob:candidate.blob};}
    let seq=(await t.get('meta','local_seq'))?.value??0;const records=[];
    for(const row of docs)if(row.collection===candidate.collection){
      const record={collection:row.collection,id:row.id,data:null,base:{data:null,deleted:false,rev:null,seq:'0'},rev:null,seq:'0',deleted:true,pending:false,cache_evicted:true,lseq:++seq};
      await t.put('docs',record);records.push(record);
    }
    for(const meta of await t.all('meta'))if(meta.key.startsWith('cursor:') && meta.collection===candidate.collection)await t.put('meta',{...meta,evicted_at_seq:meta.cursor_to,cursor_to:'0',complete:false,evicted:true});
    await t.put('meta',{key:'local_seq',value:seq});
    return {reclaimed:true,records,collection:candidate.collection};
  },'readwrite');
}
