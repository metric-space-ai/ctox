import fs from 'node:fs';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
if(process.platform!=='linux'||!process.env.CARGO_TARGET_DIR)throw Error('Lane required');
const root=fs.mkdtempSync(join(process.env.TMPDIR,'installer-oracle-'));
const dir=join(root,'tests/sync-v4/k11');fs.mkdirSync(dir,{recursive:true});
fs.mkdirSync(join(root,'tests/sync-v4-e2e'),{recursive:true});
fs.copyFileSync('tests/sync-v4-e2e/native.mjs',join(root,'tests/sync-v4-e2e/native.mjs'));
fs.copyFileSync('tests/sync-v4/k11/safety.test.mjs',join(dir,'safety.test.mjs'));
const source=fs.readFileSync('tests/sync-v4/k11/upgrade.mjs','utf8');
const needle="proof.host[key]===fs.readlinkSync('/proc/self/ns/'+key)";
const run=()=>spawnSync(process.execPath,['--test',join(dir,'safety.test.mjs')],{encoding:'utf8'});
try{
 fs.writeFileSync(join(dir,'upgrade.mjs'),source);const before=run();
 if(!source.includes(needle))throw Error('Safety mutation anchor absent');
 fs.writeFileSync(join(dir,'upgrade.mjs'),source.replace(needle,'false'));const mutated=run();
 fs.writeFileSync(join(dir,'upgrade.mjs'),source);const restored=run();
 const assertion=/AssertionError/.test(mutated.stdout),pass=before.status===0&&mutated.status!==0&&assertion&&restored.status===0;
 console.log(JSON.stringify({mutation:needle+' → false',before:before.status,mutated:mutated.status,restored:restored.status,assertion,pass}));if(!pass)throw Error('Installer safety mutation unproven');
}finally{fs.rmSync(root,{recursive:true,force:true});}
