export default {
 id:'K15',title:'Native intake survives continuously committing writers',
 coverage:'Real concurrent commits and native command intake; full mode runs thirty minutes and restarts at midpoint. App-specific harness/ack producers still require their real routes.',
 steps:['Warm actual browser/native replication','Eight continuous external committers plus browser writers','Read commands and queue CLI during load','Restart daemon at midpoint in full mode','Inspect native journal and native command truth'],
 criterion:{busyErrors:0,failedCommands:0,allWritesTerminal:true},
 async run(h){
  const p=await h.page();await h.warm(p);const docs=(await h.browserRows(p)).slice(8,16),full=h.config.catalogDuration==='full',duration=full?1800000:30000,start=Date.now(),offset=h.nativeLog().length;
  const foreign=await h.pressure(Math.min(1800,Math.ceil(duration/1000)+15)),receipts=[],errors=[],commands=[],queue=[],tasks=[];let stop=false,restarted=false;
  const producers=docs.map(async(d,i)=>{let n=0;while(!stop&&Date.now()-start<duration){try{await h.browserPut(p,[{...d,probe_phase:'producer-'+i+'-'+n++}]);receipts.push({producer:i,n,at:Date.now()});}catch(e){errors.push({producer:i,error:e.message});}await h.pause(full?400:50);}});
  const control=(async()=>{while(!stop&&Date.now()-start<duration){tasks.push(h.readCommand(p).catch(e=>({error:e.message})));await h.pause(full?50:2000);}})();
  try{while(Date.now()-start<duration){queue.push(h.queueList());if(queue.length%5===0)h.observe('sustained-commits',{elapsedMs:Date.now()-start,browserWrites:receipts.length,browserErrors:errors.length,commandsSubmitted:tasks.length,queueReads:queue.length,externalWriterPid:foreign.pid});if(full&&!restarted&&Date.now()-start>=900000){await h.restart();restarted=true;}await h.pause(2000);}}
  finally{stop=true;await Promise.all([...producers,control]);}
  const answers=await Promise.all(tasks);commands.push(...answers);
  await h.pause(Math.max(0,duration+16000-(Date.now()-start)));
  const external=h.pressureResult(foreign),pressureVerified=external.completed&&external.stats?.writers?.length===8&&external.stats.writers.every(w=>w.commits>0);
  const nativeCommands=h.commandTruth(),log=h.nativeLog().slice(offset);
  const busy=log.split('\n').filter(s=>/SQLITE_BUSY|database is locked|extended code.?517/i.test(s));
  return {measured:{durationMs:Date.now()-start,full,restarted,receipts,errors,commands,queue,nativeCommands,externalWriterPid:foreign.pid,external,pressureVerified,busyLines:busy.map(s=>s.slice(0,1000)),
   missingCoverage:['native-event/ack/projection-producers','three-client-full-first-sync','commit/queue-age-percentiles','ack-first-attempt-terminal']},
   incidentReproduced:pressureVerified&&busy.length>0,status:pressureVerified?'measured':'inconclusive',pass:pressureVerified&&busy.length===0&&errors.length===0&&commands.every(c=>!c.error)&&queue.every(q=>q.ok),acceptanceComplete:false};
 }
};
