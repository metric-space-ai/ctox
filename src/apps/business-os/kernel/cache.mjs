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
      const record={...row,data:null,deleted:true,pending:false,cache_evicted:true,lseq:++seq};
      await t.put('docs',record);records.push(record);
    }
    for(const meta of await t.all('meta'))if(meta.key.startsWith('cursor:') && meta.collection===collection){
      await t.put('meta',{...meta,cursor_to:'0',complete:false,evicted:true});
    }
    await t.put('meta',{key:'local_seq',value:seq});
    return records;
  });
}
