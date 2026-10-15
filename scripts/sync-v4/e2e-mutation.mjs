#!/usr/bin/env node
// Only disposable oracle copies are mutated; real E2E adapters stay untouched.
import fs from 'node:fs';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
if(process.platform!=='linux'||!process.env.TMPDIR||!process.env.CARGO_TARGET_DIR)throw Error('Lane required');
const root=fs.mkdtempSync(join(process.env.TMPDIR,'oracle-'));
const original=fs.readFileSync('tests/sync-v4-e2e/criteria.mjs','utf8');
fs.copyFileSync('tests/sync-v4-e2e/criteria.test.mjs',join(root,'criteria.test.mjs'));
const run=()=>spawnSync(process.execPath,['--test',join(root,'criteria.test.mjs')],{encoding:'utf8'});
const receipts=[];
try{
 for(const needle of ['m.missing.length === 0','expected.length > 0','s.acceptanceComplete===true']){
  const first=original.indexOf(needle);if(first<0)throw Error('Mutation anchor missing: '+needle);
  fs.writeFileSync(join(root,'criteria.mjs'),original);const before=run();
  fs.writeFileSync(join(root,'criteria.mjs'),original.slice(0,first)+'true'+original.slice(first+needle.length));const mutated=run();
  fs.writeFileSync(join(root,'criteria.mjs'),original);const restored=run();
  const assertion=/false !== true|true !== false/.test(mutated.stdout);
  const row={mutation:needle+' → true',before:before.status,mutated:mutated.status,restored:restored.status,assertion,pass:before.status===0&&mutated.status!==0&&assertion&&restored.status===0};
  receipts.push(row);if(!row.pass)throw Error('Oracle mutation did not prove its contract: '+needle);
 }
 console.log(JSON.stringify({receipts,pass:true}));
}finally{fs.rmSync(root,{recursive:true,force:true});}
