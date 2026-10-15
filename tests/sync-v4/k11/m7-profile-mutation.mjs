// Use the real completed installation; falsify only our private evidence copy.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {verifiedBuild,owner} from '../../sync-v4-e2e/native.mjs';
const [evidence,receipt,directory]=process.argv.slice(2),build=verifiedBuild(receipt),out=resolve(directory),lane=resolve(process.env.BUILD_LANE_BIN,'..');
if(process.platform!=='linux'||!out.startsWith(join(lane,'artifacts')+'/')||fs.existsSync(out))throw Error('New private lane output required');
fs.mkdirSync(out,{mode:0o700});
const source=fs.readFileSync(new URL('./installed-profile.mjs',import.meta.url),'utf8'),needle="proof.dataPreserved!==true||";
if(source.split(needle).length!==2)throw Error('Custody guard missing/ambiguous');
const positive=JSON.parse(fs.readFileSync(evidence)),bad=join(out,'unpreserved.json');
fs.writeFileSync(bad,JSON.stringify({...positive,dataPreserved:false}));
const module=join(out,'installed-profile.mjs');
const check=join(out,'check.mjs');
fs.writeFileSync(check,`import assert from 'node:assert/strict';\nimport {installedProfile} from ${JSON.stringify(pathToFileURL(module).href)};\nconst b=${JSON.stringify(build)};\ninstalledProfile(${JSON.stringify(resolve(evidence))},b,${JSON.stringify(lane)},${JSON.stringify(owner)});\nassert.throws(()=>installedProfile(${JSON.stringify(bad)},b,${JSON.stringify(lane)},${JSON.stringify(owner)}),/Incomplete or mismatched installed profile/,'Unpreserved profile must not start a native daemon');\n`);
const run=phase=>{const r=spawnSync(process.execPath,[check],{encoding:'utf8',timeout:30000});fs.writeFileSync(join(out,phase+'.log'),(r.stdout??'')+(r.stderr??''));return r;};
const row={sourceSha256:createHash('sha256').update(source).digest('hex'),line:source.slice(0,source.indexOf(needle)).split('\n').length,change:'remove dataPreserved guard',realEvidence:resolve(evidence),pass:false};
try{
 fs.writeFileSync(module,source);const baseline=run('baseline');if(baseline.status!==0)throw Error('Actual installed profile baseline failed: '+baseline.stderr);
 fs.writeFileSync(module,source.replace(needle,''));const mutated=run('mutated');
 if(mutated.status===0||!mutated.stderr.includes('Unpreserved profile must not start a native daemon'))throw Error('Custody guard mutation was not detected: '+mutated.stderr);
 row.baselineExit=baseline.status;row.mutatedExit=mutated.status;
}finally{fs.writeFileSync(module,source);}
const restored=run('restored');if(restored.status!==0)throw Error('Restored custody guard failed: '+restored.stderr);
row.restoredExit=restored.status;row.restoredSourceSha256=createHash('sha256').update(fs.readFileSync(module)).digest('hex');row.pass=row.restoredSourceSha256===row.sourceSha256;
fs.writeFileSync(join(out,'mutation.json'),JSON.stringify(row,null,2));console.log(JSON.stringify(row));
