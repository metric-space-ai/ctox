#!/usr/bin/env node
// Run inside gpu-build-run.sh. Reuse #810's isolated real-daemon launcher;
// this wrapper owns only the P6 incidents and never starts a replacement core.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {verifiedBuild} from '../../sync-v4-e2e/native.mjs';

const argv=process.argv.slice(2), flags=new Map();
for(let i=0;i<argv.length;i+=2) {
  if(!argv[i]?.startsWith('--') || !argv[i+1])throw Error('Expected --key value');
  if(flags.has(argv[i]))throw Error('Duplicate option: '+argv[i]);
  flags.set(argv[i],argv[i+1]);
}
if(process.platform!=='linux' || !process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
for(const forbidden of ['--scenario','--catalog','--seconds'])if(flags.has(forbidden))throw Error('P6 runner owns '+forbidden);
const expected=flags.get('--expected-revision');
if(!/^[a-f0-9]{40}$/.test(expected??''))throw Error('--expected-revision must be the immutable requested daemon revision');
const output=resolve(flags.get('--output')??join(process.env.BUILD_LANE_BIN,'../artifacts','p6-incidents-'+Date.now()));
const artifactRoot=resolve(process.env.BUILD_LANE_BIN,'../artifacts');
if(!output.startsWith(artifactRoot+'/'))throw Error('Output must be a fresh private lane artifact directory');
fs.mkdirSync(output,{recursive:false,mode:0o700});
const catalog=[['K13',180],['K21',1200],['K22',900]];
const summary={revision:expected,startedAt:new Date().toISOString(),scenarios:[],pass:false,terminal:false};
const save=()=>fs.writeFileSync(join(output,'p6-result.json'),JSON.stringify(summary,null,2));
save();
let build;
try {
  build=verifiedBuild(flags.get('--build'));
  if(build.revision!==expected)throw Error('Daemon receipt does not match requested cutover revision');
} catch(error) {
  summary.scenarios=catalog.map(([id])=>({id,status:'not_run',pass:false,acceptanceComplete:false,
    reason:'build unverified: '+error.message,daemonExecuted:false}));
}
if(build && !summary.scenarios.length) {
  const forward=argv.filter((_,i)=>!['--output','--expected-revision'].includes(argv[i-i%2]));
  for(const [id,seconds] of catalog) {
    const dir=join(output,id),log=fs.openSync(join(output,id+'.log'),'w',0o600);
    let run;
    try {
      run=spawnSync(process.execPath,['scripts/sync-v4/e2e-run.mjs',...forward,
        '--catalog','yes','--scenario',id,'--seconds',String(seconds),'--output',dir],
        {stdio:['ignore',log,log],timeout:(seconds+60)*1000,killSignal:'SIGTERM'});
    } finally {fs.closeSync(log);}
    const path=join(dir,'result.json'),report=fs.existsSync(path)?JSON.parse(fs.readFileSync(path)):null;
    const row=report?.scenarios?.find(row=>row.id===id);
    const complete=run.status===0 && report?.terminal===true && report.cleanup==='owned groups absent'
      && report.scenarios.length===1 && row?.status==='measured' && row.pass===true && row.acceptanceComplete===true;
    summary.scenarios.push({id,status:row?.status??'not_run',pass:complete,acceptanceComplete:complete,
      evidence:path,processExit:run.status,reason:row?.reason??row?.error??report?.error??run.error?.message??null,
      cleanup:report?.cleanup??null});
    save();
  }
}
summary.terminal=true;summary.finishedAt=new Date().toISOString();
summary.pass=summary.scenarios.length===catalog.length && summary.scenarios.every(row=>row.pass);
save();
for(const row of summary.scenarios)console.log(row.id+' | '+(row.pass?'green':'red')+' | '+row.status+' | '+(row.evidence??join(output,'p6-result.json')));
process.exitCode=summary.pass?0:1;
