#!/usr/bin/env node
// Mutate only a disposable copy of the acceptance oracle, then restore and prove green.
import fs from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
if(process.platform!=='linux'||!process.env.TMPDIR||!process.env.CARGO_TARGET_DIR)throw Error('Lane required');
const root=fs.mkdtempSync(join(process.env.TMPDIR,'oracle-'));
const original=fs.readFileSync('tests/sync-v4-e2e/criteria.mjs','utf8');
const needle='m.missing.length === 0';
const first=original.indexOf(needle);
if(first<0)throw Error('Mutation anchor missing');
fs.copyFileSync('tests/sync-v4-e2e/criteria.test.mjs',join(root,'criteria.test.mjs'));
const run=()=>spawnSync(process.execPath,['--test',join(root,'criteria.test.mjs')],{encoding:'utf8'});
const receipt={mutation:'criteria.mjs: completeReload, missing.length === 0 → true',before:null,mutated:null,restored:null,pass:false};
try{
 fs.writeFileSync(join(root,'criteria.mjs'),original); const before=run(); receipt.before=before.status;
 fs.writeFileSync(join(root,'criteria.mjs'),original.slice(0,first)+'true'+original.slice(first+needle.length));
 const mutated=run();receipt.mutated=mutated.status;
 if(!mutated.stdout.includes('false !== true')&&!mutated.stdout.includes('true !== false'))throw Error('Mutation did not fail the acceptance invariant');
 fs.writeFileSync(join(root,'criteria.mjs'),original);const restored=run();receipt.restored=restored.status;
 receipt.pass=before.status===0&&mutated.status!==0&&restored.status===0;
 if(!receipt.pass)throw Error('Acceptance-oracle mutation proof failed');
 console.log(JSON.stringify(receipt));
}finally{fs.rmSync(root,{recursive:true,force:true});}
