export default {
 id:'K01',title:'A stale browser must not erase a newer native value',
 coverage:'Real stale-full-document incident core. Desktop pins, archive_local and three-client reload remain required for full K01.',
 steps:['Replicate native rows into browser A','Disconnect A and update 50 native documents','Write ten stale full documents from A','Reconnect and compare native values after 60 seconds'],
 criterion:{serverFieldLoss:0,statusRollback:0,observationMs:60000},
 async run(h) {
  const a=await h.page();await h.warm(a);
  const old=(await h.browserRows(a)).slice(0,10);
  h.observe('baseline',{browserRows:old.length,nativeRows:(await h.truth()).rows.length});
  await h.offline(a,true);
  const changed=await h.serverNewer();
  h.observe('offline-native-update',{changedIds:changed.changed});
  const writes=await h.browserPut(a,old.map((d,i)=>({...d,probe_phase:i<5?'stale-conflict':d.probe_phase,client_note:'offline-edit'})));
  h.observe('offline-local-writes',{localWrites:writes.local});await h.offline(a,false); await h.pause(60000);
  const truth=await h.bodies(), rows=truth.filter(d=>changed.changed.includes(d.id));
  const fresh=rows.find(d=>d.id===old[0].id);
  await h.browserPut(a,[{...fresh,client_note:'fresh-positive-control'}]);
  let control=null;const controlStart=Date.now();
  while(Date.now()-controlStart<15000){control=(await h.bodies()).find(d=>d.id===fresh.id);if(control?.client_note==='fresh-positive-control')break;await h.pause(200);}
  const positiveControl=control?.client_note==='fresh-positive-control';
  const diverging=rows.filter(d=>d.server_only!=='keep'||d.probe_phase!=='researched').map(d=>({id:d.id,revision:d._rev,lwt:d._meta?.lwt,phase:d.probe_phase,serverOnly:d.server_only??null}));
  return {measured:{changed:changed.changed,localWrites:writes,positiveControl,diverging,serverRows:rows.length,observedMs:60000,
   missingCoverage:['desktop-pins','write_result/archive_local','third-client-startup-write-audit']},
   incidentReproduced:positiveControl&&diverging.length>0,status:positiveControl?'measured':'inconclusive',pass:positiveControl&&diverging.length===0,acceptanceComplete:false};
 }
};
