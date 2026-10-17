const percentile=(rows,p)=>rows.length?[...rows].sort((a,b)=>a-b)[Math.ceil(rows.length*p)-1]:null;
export default {
 id:'K07',title:'Read commands under bidirectional real collection pressure',
 coverage:'Real daemon command path and 20 MB fixture writes; application matrix, file-CAS and CPU audit are separately required.',
 steps:['Connect through forced TURN','Continuously read full lead rows and write a 20 MB fixture batch','Submit fifty harmless project.list commands at two-second intervals','Measure acceptance latency and query network traffic'],
 criterion:{commandP99:'RTT + 100 ms',maxCommandMs:5000,queryNetworkFrames:0},
 async run(h) {
  const a=await h.page();await h.warm(a);
  const wireBefore=await h.wire(a);
  const documents=(await h.browserRows(a)).slice(0,20).map((d,i)=>({...d,padding:'b'.repeat(1024*1024),probe_phase:'bulk-'+i}));
  let stop=false;const reads=[];
  const pressure=(async()=>{await h.browserPut(a,documents);while(!stop){const at=Date.now();await h.browserRows(a);reads.push(Date.now()-at);await h.pause(50);}})();
  const commands=[];
  try {for(let i=0;i<50;i++){commands.push(h.readCommand(a).catch(e=>({error:e.message,elapsedMs:12000})));if(i%5===0)h.observe('command-under-load',{submitted:commands.length,fullReads:reads.length,elapsedMs:i*2000});await h.pause(2000);}}
  finally {stop=true;await pressure;}
  const answers=await Promise.all(commands),latencies=answers.filter(r=>!r.error).map(r=>r.elapsedMs);
  const wireAfter=await h.wire(a),queryFetch=wireAfter.queryFetch-wireBefore.queryFetch,binarySent=wireAfter.binarySent-wireBefore.binarySent;
  const nativeCommands=h.commandTruth();
  const core=answers.map(a=>{const truth=nativeCommands.rows.find(d=>d.id===a.commandId);return truth?.nativeTiming?.native_dispatch_entered-a.startedAtMs;}).filter(Number.isFinite);
  const p99=percentile(latencies,.99),errors=answers.filter(r=>r.error);
  return {measured:{answers,bulkBytes:documents.reduce((n,d)=>n+d.padding.length,0),readDurationsMs:reads,
   queryFetch,binarySent,nativeCommands,coreDispatch:{p50:percentile(core,.5),p95:percentile(core,.95),p99:percentile(core,.99),complete:core.length===answers.length},p50:percentile(latencies,.5),p95:percentile(latencies,.95),p99,budgetMs:h.config.rttMs+100,
   missingCoverage:['fresh-first-sync/app-matrix','file-CAS-upload','native-10-writes-per-second','idle-CPU','compressed-frame-classification-if-binary']},
   pass:answers.length===50&&errors.length===0&&core.every(n=>n>=0)&&core.length===answers.length&&percentile(core,.99)<=h.config.rttMs+100&&latencies.every(n=>n<=5000)&&queryFetch===0&&binarySent===0,acceptanceComplete:false};
 }
};
