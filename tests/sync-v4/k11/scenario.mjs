export default {
 id:'K11',title:'Upgrade must not make the instance read-only',
 coverage:'Real upgrade adapter prerequisite; no fabricated maintenance flags or SQLite writes to installer state.',
 steps:['Connect a real browser','Read actual maintenance state','Require an isolated release/schema transition adapter','Check local writes during migration and acceptance after restart'],
 criterion:{readOnlySamples:0,rejectedForMaintenance:0,lostWrites:0},
 async run(h){const p=await h.page();await h.warm(p);const before=await h.maintenance(p);
  // A plain kill/respawn is NOT the incident upgrade path. Failing closed here
  // prevents a restart smoke from being reported as K11 acceptance.
  return {measured:{before,upgradeExecuted:false,missingCoverage:['isolated-managed-installer-prefix','second-binary/schema-generation','hidden/offline-clients','writes-during-transition']},
   status:'not_run',reason:'No safe isolated managed-update adapter yet; no host service invoked',pass:false,acceptanceComplete:false};}
};
