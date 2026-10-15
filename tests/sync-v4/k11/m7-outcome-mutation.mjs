// Use the actual failed rollback receipt that exposed lost child exit status.
import fs from 'node:fs';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
const [negative,positive,output]=process.argv.slice(2);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Lane required');
const bad=JSON.parse(fs.readFileSync(negative)),good=JSON.parse(fs.readFileSync(positive));
if(!bad.failure||bad.installer?.roundtripExecuted===true||!good.dataPreserved)throw Error('Actual negative/positive evidence required');
const dir=fs.mkdtempSync(join(process.env.TMPDIR,'m7-outcome-'));
const source=fs.readFileSync('tests/sync-v4/k11/installer-outcome.mjs','utf8');
const anchor='(roundtrip&&record.installer?.roundtripExecuted!==true)||record.failure||record.error';
if(!source.includes(anchor))throw Error('Mutation anchor absent');
const probe=`import assert from 'node:assert/strict';import fs from 'node:fs';import {completedTransition} from './outcome.mjs';const bad=JSON.parse(fs.readFileSync(process.argv[2])),good=JSON.parse(fs.readFileSync(process.argv[3]));assert.equal(completedTransition(bad,true),false,'Failed actual rollback was accepted');assert.equal(completedTransition(good,false),true,'Positive actual installer was refused');`;
fs.writeFileSync(join(dir,'probe.mjs'),probe);
const run=code=>{fs.writeFileSync(join(dir,'outcome.mjs'),code);return spawnSync(process.execPath,[join(dir,'probe.mjs'),negative,positive],{encoding:'utf8'});};
try{
 const before=run(source),mutated=run(source.replace(anchor,'false')),restored=run(source);
 const pass=before.status===0&&mutated.status!==0&&mutated.stderr.includes('Failed actual rollback was accepted')&&restored.status===0;
 fs.writeFileSync(output,JSON.stringify({negative,positive,mutation:anchor+' -> false',before:before.status,mutated:mutated.status,restored:restored.status,assertion:mutated.stderr.includes('Failed actual rollback was accepted'),pass},null,2));
 if(!pass)throw Error('Actual rollback outcome mutation unproven');
}finally{fs.rmSync(dir,{recursive:true,force:true});}
