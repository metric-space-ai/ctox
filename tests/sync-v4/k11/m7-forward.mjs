// Recover the actually rolled-back private fixture with the supported forward
// installer. The earlier rollback CLI failure remains a failure, not erased.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {Native,verifiedBuild,owner} from '../../sync-v4-e2e/native.mjs';
const [negative,oldReceipt,newReceipt,directory]=process.argv.slice(2);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const proof=JSON.parse(fs.readFileSync(negative)),before=verifiedBuild(oldReceipt),after=verifiedBuild(newReceipt),output=resolve(directory),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
if(!resolve(negative).startsWith(allowed)||!output.startsWith(allowed)||!proof.terminal||!proof.failure||!proof.controller?.groupAbsent||proof.owner!==owner||proof.beforeRevision!==before.revision)throw Error('Actual retired negative rollback receipt required');
fs.mkdirSync(output,{mode:0o700});
const result={owner,scope:'forward recovery after actual rollback CLI failure',negative:resolve(negative),negativeSha256:createHash('sha256').update(fs.readFileSync(negative)).digest('hex'),startedAt:new Date().toISOString(),rollbackCliAccepted:false,fullM7Accepted:false,pass:false};
try{
 const fd=fs.openSync(join(output,'controller.log'),'w',0o600);
 const p=spawnSync(process.execPath,['tests/sync-v4/k11/upgrade-controller.mjs',oldReceipt,newReceipt,proof.root,join(output,'upgrade'),'no','resume-rolled-back'],{stdio:['ignore',fd,fd],timeout:11*60*1000});fs.closeSync(fd);
 for(const [name,file]of [['installer','installer.json'],['controller','controller.json'],['serviceManager','service-manager.json']]){const path=join(output,'upgrade',file);if(fs.existsSync(path))result[name]=JSON.parse(fs.readFileSync(path));}
 if(p.status!==0||!result.installer?.installerExecuted||!result.controller?.groupAbsent||!result.serviceManager?.privateCgroupRemoved)throw Error('Supported forward recovery failed');
 const native=new Native({...after,root:proof.root,output,profile:'small'});
 const rows=native.seed('bodies',resolve('tests/sync-v4-e2e'));
 const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(k=>[k,canonical(value[k])])):value;
 result.fullDocumentSha256=createHash('sha256').update(JSON.stringify(canonical(rows.rows))).digest('hex');
 result.documentsCovered=rows.rows.length;
 if(result.fullDocumentSha256!==proof.fixture.fullDocumentSha256||rows.rows.length!==proof.fixture.documentsCovered)throw Error('Forward recovery changed fixture documents');
 result.pass=true;
}catch(error){result.error=error.message;}
finally{result.finishedAt=new Date().toISOString();result.terminal=true;fs.writeFileSync(join(output,'result.json'),JSON.stringify(result,null,2));}
console.log(JSON.stringify({output,pass:result.pass,rollbackCliAccepted:false,error:result.error}));process.exitCode=result.pass?0:1;
