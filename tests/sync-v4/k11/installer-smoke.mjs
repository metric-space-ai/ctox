// Exercises only the real, supported binary installer. Its success is never
// full K11 acceptance: live generations, migration and clients are separate.
import fs from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {Native,verifiedBuild,owner} from '../../sync-v4-e2e/native.mjs';
import {schema} from '../../sync-v4-e2e/browser.mjs';
import {completedTransition} from './installer-outcome.mjs';
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const [oldReceipt,newReceipt,outArg,roundtrip]=process.argv.slice(2),before=verifiedBuild(oldReceipt),after=verifiedBuild(newReceipt);
const output=resolve(outArg),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
if(!output.startsWith(allowed))throw Error('New private artifact prefix required');
fs.mkdirSync(output,{recursive:false,mode:0o700});
const root=join(output,'tenant-m7'),native=new Native({...before,root,output,profile:'small'});
const record={id:'K11',scope:'supported installer only',owner,startedAt:new Date().toISOString(),beforeRevision:before.revision,afterRevision:after.revision,installerExecuted:false,migrationExecuted:false,acceptanceComplete:false,pass:false};
const save=()=>fs.writeFileSync(join(output,'result.json'),JSON.stringify(record,null,2));save();
const snapshot=(phase)=>{
 const dir=join(output,'snapshot-'+phase),at=Date.now();
 const run=spawnSync('python3',[join(dirname(new URL(import.meta.url).pathname),'m7-backup.py'),'snapshot','--root',root,'--output',dir],{encoding:'utf8',timeout:180000});
 if(run.status!==0||run.error)throw Error('SQLite API snapshot '+phase+' failed: '+(run.error?.message??run.stderr));
 const restore=spawnSync('python3',[join(dirname(new URL(import.meta.url).pathname),'m7-backup.py'),'restore','--output',dir,'--destination',join(output,'restore-validation-'+phase)],{encoding:'utf8',timeout:180000});
 if(restore.status!==0||restore.error)throw Error('SQLite restore verification '+phase+' failed: '+(restore.error?.message??restore.stderr));
 return {manifest:join(dir,'snapshot.json'),durationMs:Date.now()-at,quiesced:true,restored:JSON.parse(restore.stdout),databaseCount:JSON.parse(fs.readFileSync(join(dir,'snapshot.json'))).databases.length};
};
try{
 const fixture=await native.prepare(schema,resolve(dirname(new URL(import.meta.url).pathname),'../../sync-v4-e2e'));
 const truth=native.seed('read',resolve(dirname(new URL(import.meta.url).pathname),'../../sync-v4-e2e'));
 const bodies=native.seed('bodies',resolve(dirname(new URL(import.meta.url).pathname),'../../sync-v4-e2e'));
 const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(k=>[k,canonical(value[k])])):value;
 const digest=rows=>createHash('sha256').update(JSON.stringify(canonical(rows))).digest('hex');
 record.fixture={rows:truth.rows.length,documentsCovered:bodies.rows.length,fullDocumentSha256:digest(bodies.rows),metadataSha256:digest(truth.rows)};save();
 record.root=root;record.beforeSnapshot=snapshot('before');save();
 const controller=join(dirname(new URL(import.meta.url).pathname),'upgrade-controller.mjs'),log=fs.openSync(join(output,'controller.log'),'w',0o600);
 const run=spawnSync(process.execPath,[controller,oldReceipt,newReceipt,root,join(output,'upgrade'),roundtrip==='yes'?'yes':'no'],{stdio:['ignore',log,log],timeout:11*60*1000});fs.closeSync(log);
 record.controllerExit=run.status;record.controllerError=run.error?.message;
 for(const [name,file]of [['installer','installer.json'],['failure','failure.json'],['controller','controller.json']]){
  const p=join(output,'upgrade',file);if(fs.existsSync(p))record[name]=JSON.parse(fs.readFileSync(p));
 }
 record.installerExecuted=completedTransition(record,roundtrip==='yes');
 record.roundtripExecuted=record.installer?.roundtripExecuted===true;
 if(record.installerExecuted){
  record.currentRelease=fs.realpathSync(join(root,'managed/current'));
  const current=native.seed('read',resolve(dirname(new URL(import.meta.url).pathname),'../../sync-v4-e2e'));
  const content=native.seed('bodies',resolve(dirname(new URL(import.meta.url).pathname),'../../sync-v4-e2e'));
  record.afterFixture={rows:current.rows.length,documentsCovered:content.rows.length,fullDocumentSha256:digest(content.rows),metadataSha256:digest(current.rows)};
  record.dataPreserved=record.afterFixture.fullDocumentSha256===record.fixture.fullDocumentSha256&&record.afterFixture.documentsCovered===record.afterFixture.rows&&record.fixture.documentsCovered===record.fixture.rows;
  record.afterSnapshot=snapshot('after');save();
 }
 record.next='A real publication/migration operation and generation/row invariants must follow; installing/restarting alone never satisfies K11.';
}catch(error){record.error=error.message;}
finally{await native.close();record.finishedAt=new Date().toISOString();record.terminal=true;save();}
console.log(JSON.stringify({output,installerExecuted:record.installerExecuted,migrationExecuted:false,acceptanceComplete:false,error:record.error??record.failure?.error}));
process.exitCode=record.installerExecuted&&record.dataPreserved?0:1;
