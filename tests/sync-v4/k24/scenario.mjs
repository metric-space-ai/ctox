export default {
 id:'K24',title:'Delivered Shell v4 K24 tool, criteria PASS/OFFEN/FAIL',
 coverage:'Exact upstream K24PREP tool at 1ab7ceb03; native URL only on isolated instance. Bind diagnostic results are never native acceptance.',
 steps:['Verify origin hash per delivered file','Run upstream folder-upload scenario','Retain its per-criterion status verbatim'],
 criterion:{allNativeCriteria:'PASS',restarts:0,reloads:0,activationMs:2000},
 async run(){return {status:'not_run',reason:'K24 is run by k24/execute.mjs, outside the V3 application adapter; see gate K24 evidence',pass:false,acceptanceComplete:false};}
};
