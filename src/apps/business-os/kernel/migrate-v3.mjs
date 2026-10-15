import { CtoxError, cloneValue } from './values.mjs';

/** Upgrade transaction only: the supplied DB already belongs to this (instance,user).
 * Never scan/import a global V3 DB whose identity has not been authenticated.
 * Keep `documents` intact so an aborted upgrade cannot destroy the V3 source.
 */
export function migrateV3Upgrade(db, transaction, oldVersion) {
  if (oldVersion !== 4 || !db.objectStoreNames.contains('documents')) return;
  const source = transaction.objectStore('documents');
  const docs = transaction.objectStore('docs');
  const meta = transaction.objectStore('meta');
  const archive = transaction.objectStore('archive_local');
  let count = 0;
  const request = source.openCursor();
  request.onerror = () => transaction.abort();
  request.onsuccess = () => {
    const cursor = request.result;
    if (!cursor) {
      meta.put({ key: 'migration:v3', value: { completed: true, imported: count, from: 4 } });
      meta.put({ key: 'local_seq', value: count });
      return;
    }
    const old = cursor.value;
    try {
      const converted = convertV3Record(old, ++count);
      docs.put(converted);
      // V3 local changes have no V4 op_id/authority contract. Preserve the full
      // input for explicit recovery; never silently replay it as another actor.
      if (!old.replicationOriginRole && old.pushable !== 0) {
        archive.put({ ref: `migration:v3:${JSON.stringify(cursor.primaryKey)}`,
          collection: converted.collection, id: converted.id, body: cloneValue(old.doc),
          reason: 'v3_unconfirmed', legacy: cloneValue(old) });
      }
      cursor.continue();
    } catch { transaction.abort(); }
  };
}
export function convertV3Record(record, lseq) {
  if (!record || typeof record.collection !== 'string' || !record.doc || typeof record.doc.id !== 'string') {
    throw new CtoxError('invalid', 'Invalid V3 document; upgrade aborted without deleting source');
  }
  const data = cloneValue(record.doc);
  const confirmed = !!record.replicationOriginRole || record.pushable === 0;
  const baseDoc = confirmed ? data : cloneValue(record.base ?? null);
  const base = { data: baseDoc, deleted: !!baseDoc?._deleted, rev: baseDoc?._rev ?? null, seq: '0' };
  return { collection: record.collection, id: data.id, data, deleted: !!data._deleted,
    rev: data._rev ?? null, seq: '0', base, pending: false, lseq, legacy_unconfirmed: !confirmed };
}

/** Explicit bootstrap migration from an identity-verified, quiesced V3 DB.
 * Bounded chunks; resume marker and each chunk commit together. The source is
 * never cleared/deleted. A failed chunk is replayed on the next startup.
 */
export async function takeoverV3Database(target, legacy, identity) {
  if(!legacy || legacy.instanceId!==identity.instanceId || legacy.userId!==identity.userId || legacy.quiesced!==true) {
    throw new CtoxError('forbidden','V3 source must be identity-verified and quiesced');
  }
  if(legacy.name===target.name)return; // in-place version upgrade already handled it
  const marker=`migration:v3:${legacy.name}`;
  let progress=await target.get('meta',marker);
  if(progress?.complete)return;
  const source=await new Promise((resolve,reject)=>{
    const request=indexedDB.open(legacy.name);
    request.onupgradeneeded=()=>{request.transaction.abort();reject(new CtoxError('not_found','V3 source does not exist'));};
    request.onerror=()=>reject(request.error);
    request.onsuccess=()=>{
      const db=request.result;
      if(db.version!==4 || !db.objectStoreNames.contains('documents')){db.close();reject(new CtoxError('invalid','Expected V3 database version4'));}
      else resolve(db);
    };
  });
  try {
    for(;;){
      const rows=await new Promise((resolve,reject)=>{
        const tx=source.transaction('documents','readonly'),batch=[];
        const range=progress?.lastKey===undefined?null:IDBKeyRange.lowerBound(progress.lastKey,true);
        const cursor=tx.objectStore('documents').openCursor(range);
        cursor.onsuccess=()=>{const c=cursor.result;if(c&&batch.length<128){batch.push({key:c.primaryKey,value:c.value});c.continue();}};
        cursor.onerror=()=>reject(cursor.error);tx.onabort=()=>reject(tx.error);tx.oncomplete=()=>resolve(batch);
      });
      progress=await target.tx(async t=>{
        let local=(await t.get('meta','local_seq'))?.value??0;
        for(const row of rows){
          const converted=convertV3Record(row.value,++local);
          const existing=await t.get('docs',[converted.collection,converted.id]);
          if(!existing)await t.put('docs',converted);
          if(converted.legacy_unconfirmed)await t.put('archive_local',{ref:`${marker}:${JSON.stringify(row.key)}`,collection:converted.collection,id:converted.id,body:cloneValue(row.value.doc),reason:'v3_unconfirmed',legacy:cloneValue(row.value)});
        }
        const next={key:marker,lastKey:rows.at(-1)?.key??progress?.lastKey,complete:rows.length<128,imported:(progress?.imported??0)+rows.length};
        await t.put('meta',next);await t.put('meta',{key:'local_seq',value:local});return next;
      });
      if(progress.complete)return;
    }
  }finally{source.close();}
}
