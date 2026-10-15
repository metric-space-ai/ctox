// Exercises only the real, supported binary installer. Its success is never
// full K11 acceptance: live generations, migration and clients are separate.
import fs from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {Native,verifiedBuild,owner} from '../../sync-v4-e2e/native.mjs';
import {schema} from '../../sync-v4-e2e/browser.mjs';
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const [oldReceipt,newReceipt,outArg]=process.argv.slice(2),before=verifiedBuild(oldReceipt),after=verifiedBuild(newReceipt);
const output=resolve(outArg),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
if(!output.startsWith(allowed))throw Error('New private artifact prefix required');
fs.mkdirSync(output,{recursive:false,mode:0o700});
const root=join(output,'tenant-'+before.revision.slice(0,12)),native=new Native({...before,root,output,profile:'small'});
const record={id:'K11',scope:'supported installer only',owner,startedAt:new Date().toISOString(),beforeRevision:before.revision,afterRevision:after.revision,installerExecuted:false,migrationExecuted:false,acceptanceComplete:false,pass:false};
const save=()=>fs.writeFileSync(join(output,'result.json'),JSON.stringify(record,null,2));save();
try{
 const fixture=await native.prepare(schema,resolve(dirname(new URL(import.meta.url).pathname),'../../sync-v4-e2e'));
 const truth=native.seed('read',resolve(dirname(new URL(import.meta.url).pathname),'../../sync-v4-e2e'));
 record.fixture={rows:truth.rows.length,sha256:createHash('sha256').update(JSON.stringify(truth.rows)).digest('hex')};save();
 const controller=join(dirname(new URL(import.meta.url).pathname),'upgrade-controller.mjs'),log=fs.openSync(join(output,'controller.log'),'w',0o600);
 const run=spawnSync(process.execPath,[controller,oldReceipt,newReceipt,root,join(output,'upgrade')],{stdio:['ignore',log,log],timeout:11*60*1000});fs.closeSync(log);
 record.controllerExit=run.status;record.controllerError=run.error?.message;
 for(const [name,file]of [['installer','installer.json'],['failure','failure.json'],['controller','controller.json']]){
  const p=join(output,'upgrade',file);if(fs.existsSync(p))record[name]=JSON.parse(fs.readFileSync(p));
 }
 record.installerExecuted=record.installer?.installerExecuted===true&&record.controller?.groupAbsent===true&&run.status===0;
 if(record.installerExecuted){
  const current=native.seed('read',resolve(dirname(new URL(import.meta.url).pathname),'../../sync-v4-e2e'));
  record.afterFixture={rows:current.rows.length,sha256:createHash('sha256').update(JSON.stringify(current.rows)).digest('hex')};
  record.dataPreserved=record.afterFixture.sha256===record.fixture.sha256;
 }
 record.next='A real publication/migration operation and generation/row invariants must follow; installing/restarting alone never satisfies K11.';
}catch(error){record.error=error.message;}
finally{await native.close();record.finishedAt=new Date().toISOString();record.terminal=true;save();}
console.log(JSON.stringify({output,installerExecuted:record.installerExecuted,migrationExecuted:false,acceptanceComplete:false,error:record.error??record.failure?.error}));
process.exitCode=record.installerExecuted&&record.dataPreserved?0:1;
