export default {
 id:'K15',title:'Native intake survives continuously committing writers',
 coverage:'Bounded real-browser writer-pressure slice; not the catalog thirty-minute eight-native-producer workload.',
 steps:['Warm actual browser/native replication','Eight concurrent browser producers mutate owned rows for thirty seconds','Run harmless native read commands while commits continue','Inspect native journal for busy/517 errors'],
 criterion:{busyErrors:0,failedCommands:0},
 async run(h){const p=await h.page();await h.warm(p);const docs=(await h.browserRows(p)).slice(0,8),start=Date.now(),offset=h.nativeLog().length;
  const receipts=[],errors=[];
  await Promise.all(docs.map(async(d,i)=>{let n=0;while(Date.now()-start<30000){try{await h.browserPut(p,[{...d,probe_phase:'producer-'+i+'-'+n++}]);receipts.push({producer:i,n,at:Date.now()});}catch(e){errors.push({producer:i,error:e.message});break;}await h.pause(50);}}));
  const command=await h.readCommand(p),log=h.nativeLog().slice(offset);
  const busy=log.split('\n').filter(s=>/SQLITE_BUSY|database is locked|extended code.?517/i.test(s));
  return {measured:{durationMs:Date.now()-start,receipts,errors,command,busyLines:busy.map(s=>s.slice(0,1000)),
   missingCoverage:['thirty-minute-duration','native-event/ack/projection-producers','continuous-external-committer','three-clients','midpoint-restart','commit/queue-percentiles']},
   incidentReproduced:busy.length>0,pass:busy.length===0&&errors.length===0&&!command.error,acceptanceComplete:false};}
};
