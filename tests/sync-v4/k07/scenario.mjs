const percentile=(rows,p)=>rows.length?[...rows].sort((a,b)=>a-b)[Math.ceil(rows.length*p)-1]:null;
export default {
 id:'K07',title:'Read commands under bidirectional real collection pressure',
 coverage:'Real daemon command path and 20 MB fixture writes; application matrix, file-CAS and CPU audit are separately required.',
 steps:['Connect through forced TURN','Continuously read full lead rows and write a 20 MB fixture batch','Submit fifty harmless project.list commands at two-second intervals','Measure acceptance latency and query network traffic'],
 criterion:{commandP99:'RTT + 100 ms',maxCommandMs:5000,queryNetworkFrames:0},
 async run(h) {
  const a=await h.page();await h.warm(a);
  await a.evaluate(()=>{globalThis.__e2eQueryStart={...globalThis.__e2eWire};});
  const documents=(await h.browserRows(a)).slice(0,20).map((d,i)=>({...d,padding:'b'.repeat(1024*1024),probe_phase:'bulk-'+i}));
  let stop=false;const reads=[];
  const pressure=(async()=>{await h.browserPut(a,documents);while(!stop){const at=Date.now();await h.browserRows(a);reads.push(Date.now()-at);await h.pause(50);}})();
  const commands=[];
  try {for(let i=0;i<50;i++){commands.push(h.readCommand(a).catch(e=>({error:e.message,elapsedMs:12000})));await h.pause(2000);}}
  finally {stop=true;await pressure;}
  const answers=await Promise.all(commands),latencies=answers.filter(r=>!r.error).map(r=>r.elapsedMs);
  const p99=percentile(latencies,.99),errors=answers.filter(r=>r.error);
  return {measured:{answers,bulkBytes:documents.reduce((n,d)=>n+d.padding.length,0),readDurationsMs:reads,
   p50:percentile(latencies,.5),p95:percentile(latencies,.95),p99,budgetMs:h.config.rttMs+100,
   missingCoverage:['fresh-first-sync/app-matrix','file-CAS-upload','native-10-writes-per-second','idle-CPU','decoded-query-RPC-counter']},
   pass:errors.length===0&&p99!==null&&p99<=h.config.rttMs+100&&latencies.every(n=>n<=5000),acceptanceComplete:false};
 }
};
