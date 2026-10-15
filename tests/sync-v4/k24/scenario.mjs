export default {
 id:'K24',title:'An unknown app hot-plugs through the OS publication contract',
 coverage:'Real runtime capability discovery only until the V4 publication adapter exists. Unsupported is not a sync-defect reproduction.',
 steps:['Open three isolated clients against the unchanged daemon','Discover the OS publication API without adding code to Core/Shell','Publish unknown app with schema/index/handler/every-trigger','Measure all-client activation, migration and archive'],
 criterion:{activationMs:2000,restarts:0,reloads:0},
 async run(h){const p=await h.page();await h.warm(p);
  const observed=await p.evaluate(()=>({v3Shell:!!globalThis.CTOX_BUSINESS_OS_APP,
   kernel:typeof globalThis.ctox,appApi:typeof globalThis.CTOX_APP_API,
   // Names/types only, never protected data or credentials.
   publication:typeof globalThis.CTOX_BUSINESS_OS_APP?.publishApp}));
  return {measured:{observed,published:false,missingCoverage:['typed-publication-adapter','non-admin/Workjet-clients','handler-and-OS-primitive','every-trigger','migration-remount-input','archive-removal']},
   pass:false,acceptanceComplete:false,reason:'V4 app publication adapter not implemented; expected-red prerequisite, not a completed K24 run'};}
};
