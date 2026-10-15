import fs from 'node:fs';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
if(process.platform!=='linux'||!process.env.CARGO_TARGET_DIR)throw Error('Lane required');
const root=fs.mkdtempSync(join(process.env.TMPDIR,'target-oracle-'));
const source=fs.readFileSync(new URL('./target.mjs',import.meta.url),'utf8');
const test=fs.readFileSync(new URL('./target.test.mjs',import.meta.url));
const needle="if(!row||!['measured','failed'].includes(row.status))return 'not_run';";
try{
 fs.writeFileSync(join(root,'target.test.mjs'),test);
 const run=()=>spawnSync(process.execPath,['--test',join(root,'target.test.mjs')],{encoding:'utf8'});
 for(const [needle,replacement] of [["if(!row||!['measured','failed'].includes(row.status))return 'not_run';",''],['c.status===\'FAIL\'&&c.subs>0',"c.status==='FAIL'"]]){
  fs.writeFileSync(join(root,'target.mjs'),source);const before=run();
  if(!source.includes(needle))throw Error('Mutation anchor absent');
  fs.writeFileSync(join(root,'target.mjs'),source.replace(needle,replacement));const mutated=run();
  fs.writeFileSync(join(root,'target.mjs'),source);const restored=run();
  const pass=before.status===0&&mutated.status!==0&&/AssertionError/.test(mutated.stdout)&&restored.status===0;
  console.log(JSON.stringify({mutation:needle+' → '+replacement,before:before.status,mutated:mutated.status,restored:restored.status,assertion:/AssertionError/.test(mutated.stdout),pass}));
  if(!pass)throw Error('Missing-target/bind mutation proof failed');
 }
}finally{fs.rmSync(root,{recursive:true,force:true});}
