#!/usr/bin/env node
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {catalogAccepted} from '../../sync-v4-e2e/criteria.mjs';
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const expected=['K01','K07','K11','K15','K02','K24'];
const declared=fs.readdirSync('tests/sync-v4').filter(n=>/^k[0-9]+$/.test(n)).map(n=>'K'+n.slice(1));
if(!declared.length)throw Error('ZERO scenarios discovered');
if(expected.some(id=>!declared.includes(id)))throw Error('Missing mandatory catalog scenario');
const output=resolve(process.env.BUILD_LANE_BIN,'../artifacts/e2e-k-'+Date.now());
fs.mkdirSync(output,{recursive:false,mode:0o700});
const args=process.argv.slice(2);
if(args.includes('--output')||args.includes('--scenario')||args.includes('--catalog'))throw Error('Gate owns catalog/output; no scenario subset');
const summary={startedAt:new Date().toISOString(),expected,scenarios:[],terminal:false,pass:false};
const save=()=>fs.writeFileSync(join(output,'gate.json'),JSON.stringify(summary,null,2));
save();
for(const id of expected){
 const dir=join(output,id);
 const log=fs.openSync(join(output,id+'.log'),'w',0o600);
 // A new tenant per scenario prevents prior bulk writes and failed injections
 // from silently changing the next incident's supposedly deterministic profile.
 let p;try{p=spawnSync(process.execPath,['scripts/sync-v4/e2e-run.mjs',...args,'--catalog','yes','--scenario',id,'--output',dir],{stdio:['ignore',log,log],timeout:35*60*1000});}finally{fs.closeSync(log);}
 const file=join(dir,'result.json');
 const report=fs.existsSync(file)?JSON.parse(fs.readFileSync(file)):null;
 const row=report?.scenarios.find(s=>s.id===id);
 const accepted=!!report&&catalogAccepted(report,[id])&&p.status===0;
 summary.scenarios.push({id,status:row?.status||'not_run',measuredPass:row?.pass===true,accepted,acceptanceComplete:row?.acceptanceComplete===true,
  error:row?.error||report?.error||p.error?.message||null,cleanup:report?.cleanup||null,processExit:p.status,evidence:file,log:join(output,id+'.log')});
 save();console.log(`${id} | ${accepted?'green':'red'} | ${file} (${row?.status||'not_run'}; complete=${row?.acceptanceComplete===true})`);
}
summary.terminal=true;summary.finishedAt=new Date().toISOString();
summary.actuallyRan=summary.scenarios.filter(s=>['measured','failed'].includes(s.status)).map(s=>s.id);
summary.pass=summary.actuallyRan.length>0&&summary.scenarios.every(s=>s.accepted);save();
console.log('K-no. | red/green | evidence');
for(const row of summary.scenarios)console.log(`${row.id} | ${row.accepted?'green':'red'} | ${row.evidence}`);
if(!summary.actuallyRan.length)throw Error('ZERO scenarios ran; see '+join(output,'gate.json'));
process.exitCode=summary.pass?0:1;
