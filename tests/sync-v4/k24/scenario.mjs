export default {
 id:'K24',title:'Delivered Shell v4 K24 tool, criteria PASS/OFFEN/FAIL',
 coverage:'Exact upstream K24PREP tool at 1ab7ceb03; native URL only on isolated instance. Bind diagnostic results are never native acceptance.',
 steps:['Verify origin hash per delivered file','Run upstream folder-upload scenario','Retain its per-criterion status verbatim'],
 criterion:{allNativeCriteria:'PASS',restarts:0,reloads:0,activationMs:2000},
 async run(h){
  const page=await h.page();await h.warm(page);const availability=await h.os(page);
  const historical=availability.v3Shell&&availability.kernelStaticAvailable===false;
  return {status:historical?'measured':'not_run',measured:{availability,published:false},expectedLegacyFailure:historical,
   reason:historical?'V3 reference: canonical kernel path is absent; upstream K24 lifecycle cannot run':'Run the delivered tool through k24/execute.mjs; no second hot-plug implementation',pass:false,acceptanceComplete:false};
 }

};
