export default {
 id:'K11',title:'Upgrade and schema transition must not make the instance read-only',
 coverage:'Actual supervised daemon binary transition; schema operations must be supplied as the revision’s real CLI. No maintenance-state fabrication, no host service commands.',
 steps:['Connect visible A, hidden B and offline C','Write from A and B every five seconds','Switch to a verified second daemon binary, preserve state and invoke compatible/incompatible migration CLI','Observe maintenance, native persistence and recovery'],
 criterion:{readOnlySamples:0,rejectedForMaintenance:0,lostWrites:0,corePauseBeyondRestartMs:10000},
 async run(h){
  const a=await h.page();await h.warm(a);const before=await h.maintenance(a);
  if(!h.config.upgradeBuild || !h.config.schemaCli?.compatible || !h.config.schemaCli?.incompatible || !h.config.schemaCli?.database)
   return {measured:{before,upgradeExecuted:false,missingInputs:['--upgrade-build second/build.json','--schema-cli revision-specific-real-migrations.json']},status:'not_run',reason:'Binary/schema transition inputs absent; plain restart is not K11',pass:false,acceptanceComplete:false};
  const b=await h.newClient();await h.warm(b);const c=await h.newClient();await h.warm(c);
  await h.offline(c,true);await a.bringToFront();
  const samples=[],writes=[],errors=[];let stop=false;
  const [da,db]=(await h.browserRows(a)).slice(0,2);
  const producer=async(page,doc,who)=>{let n=0;while(!stop){const phase='upgrade-'+who+'-'+n++;
   try{await h.browserPut(page,[{...doc,probe_phase:phase}]);writes.push({id:doc.id,phase,at:Date.now(),who});}
   catch(e){errors.push({who,at:Date.now(),error:e.message});}await h.pause(5000);}};
  const loops=[producer(a,da,'A'),producer(b,db,'B')];
  const sampler=(async()=>{while(!stop){samples.push({at:Date.now(),...await h.maintenance(a)});await h.pause(100);}})();
  const started=Date.now();let switched,installer,generationBefore,generationCompatible,generationAfter;
  try{installer=await h.installRelease(h.config.upgradeBuild);switched=Date.now();generationBefore=h.migrationProof();await h.schemaChange(h.config.schemaCli.compatible);generationCompatible=h.migrationProof();await h.schemaChange(h.config.schemaCli.incompatible);generationAfter=h.migrationProof();await h.pause(30000);}
  finally{stop=true;await Promise.all([...loops,sampler]);}
  await h.offline(c,false);
  const truth=await h.bodies(),last=new Map(writes.map(w=>[w.id,w]));
  const missing=[...last.values()].filter(w=>!truth.some(d=>d.id===w.id&&d.probe_phase===w.phase));
  return {measured:{before,samples,writes,errors,missing,started,switched,installer,generationBefore,generationCompatible,generationAfter,
   missingCoverage:['100k-document-migration-volume','targeted-single-collection-outage','write-result-per-operation','collection-specific-fault-injection']},
   pass:installer?.installerExecuted===true&&generationAfter?.generations.some(g=>g.state==='active'&&g.rows>=100000&&g.generation>Math.max(...generationBefore.generations.map(x=>x.generation)))&&samples.every(s=>s.active===false)&&errors.length===0&&missing.length===0,acceptanceComplete:false};
 }
};
