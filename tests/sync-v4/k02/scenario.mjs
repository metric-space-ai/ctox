import slice from '../../sync-v4-e2e/scenario-K02.mjs';
import {completeReload} from '../../sync-v4-e2e/criteria.mjs';
export default {...slice,coverage:'Synthetic real-app DB query/paint, with native writes and optional timed/quota matrix. Actual Outbound UI and pre-fix red witness are separate evidence.',
 async run(h){
  if(h.config.catalogDuration!=='full'){const result=await slice.run(h);return {...result,acceptanceComplete:false,measured:{...result.measured,missingCoverage:['ten-minute-native-writes','300-detail-reads','five-reloads','quota-variant','pre-fix-red-witness']}};}
  const page=await h.page();await h.warm(page);const variants=[];
  for(const quota of [null,50*1024*1024]){
   if(quota)await h.quota(page,quota);
   const started=Date.now(),writer=await h.pressure(600,1,1000),details=await h.details(page),reloads=[];
   for(let i=1;i<=5;i++){
    await h.pause(Math.max(0,started+i*120000-Date.now()));
    const row=await h.reloadAndRead(page),truth=await h.truth();
    row.nativeRows=truth.rows.filter(d=>!d.deleted).length;row.at=new Date().toISOString();
    // Producer changes one document/s. Count all received bytes, a stricter
    // bound than collection-only bytes; no unclassified frame is ignored.
    row.changedDocumentUpperBoundBytes=120*88000;
    row.byteBudget=1.5*row.changedDocumentUpperBoundBytes;
    row.byteBoundPass=row.wire.received<=row.byteBudget;
    reloads.push(row);h.observe('reload-'+i,{quota,visibleMs:row.visibleMs,rows:row.rows,nativeRows:row.nativeRows,missing:row.missing,receivedBytes:row.wire.received,byteBoundPass:row.byteBoundPass});
   }
   variants.push({quota,details,writerPid:writer.pid,external:h.pressureResult(writer),reloads});
  }
  const measured={variants,missingCoverage:['actual-Outbound-detail-UI','quota-state-zone/outbox-invariant','named-pre-fix-red-witness']};
  return {measured,pass:variants.every(v=>v.reloads.every(m=>completeReload(m)&&m.rows===m.nativeRows&&m.byteBoundPass)),acceptanceComplete:false};
 }};
