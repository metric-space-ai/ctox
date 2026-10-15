#!/usr/bin/env node
// Rehearse an exact binary transition in the owned synthetic profile. Missing
// migration/publication entry points remain not_run, never a successful upgrade.
import fs from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {verifiedBuild,owner} from '../../sync-v4-e2e/native.mjs';
const o={};for(let i=2;i<process.argv.length;i+=2){if(!process.argv[i].startsWith('--')||!process.argv[i+1])throw Error('Expected --key value');o[process.argv[i].slice(2)]=process.argv[i+1];}
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const old=verifiedBuild(o.before),next=verifiedBuild(o.after),output=resolve(o.output);
const allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
if(!output.startsWith(allowed)||fs.existsSync(output))throw Error('New private artifact output required');
fs.mkdirSync(output,{mode:0o700});
const report={schema:'ctox.m7.rehearsal.v1',owner,startedAt:new Date().toISOString(),before:{revision:old.revision,binarySha256:old.binarySha256},after:{revision:next.revision,binarySha256:next.binarySha256},steps:[],pass:false,terminal:false};
const save=()=>fs.writeFileSync(join(output,'rehearsal.json'),JSON.stringify(report,null,2));save();
const run=(id,command,args,timeout)=>{
 const path=join(output,id+'.log'),fd=fs.openSync(path,'w',0o600),startedAtMs=Date.now();
 const p=spawnSync(command,args,{stdio:['ignore',fd,fd],timeout,maxBuffer:1024*1024});fs.closeSync(fd);
 const row={id,command,args,startedAtMs,durationMs:Date.now()-startedAtMs,exit:p.status,signal:p.signal,error:p.error?.message,log:path,status:p.status===0?'green':p.status===2?'not_run':'red'};
 report.steps.push(row);save();return row;
};
try{
 const prerequisites=[o['turn-bin']??'/usr/bin/turnserver',o.chrome??'/opt/google/chrome/chrome',join(o['modules-root']??resolve(process.env.BUILD_LANE_BIN,'../deps/shell-browser-collection-auth/node_modules'),'playwright/package.json')];
 const missing=prerequisites.filter(p=>!fs.existsSync(p));
 report.steps.push({id:'runtime-prerequisites',status:missing.length?'not_run':'green',missing});save();
 if(missing.length)throw Error('Runtime prerequisites absent: '+missing.join(', '));
 const prefix=o['installed-evidence']?dirname(resolve(o['installed-evidence'])):join(output,'install');
 if(o['installed-evidence']){
  const bytes=fs.readFileSync(join(prefix,'result.json')),p=JSON.parse(bytes);
  if(!prefix.startsWith(allowed)||p.id!=='K11'||!p.terminal||!p.installerExecuted||!p.dataPreserved||!p.controller?.groupAbsent||!p.beforeSnapshot?.quiesced||!p.afterSnapshot?.quiesced||p.beforeRevision!==old.revision||p.afterRevision!==next.revision||p.installer?.activeBinarySha256!==next.binarySha256)throw Error('Reused installer evidence does not match this transition');
  report.steps.push({id:'sqlite-backup-install-restore',status:'green',reused:true,evidence:join(prefix,'result.json'),evidenceSha256:createHash('sha256').update(bytes).digest('hex'),startedAt:p.startedAt,finishedAt:p.finishedAt});save();
 }else{
  const install=run('sqlite-backup-install-restore',process.execPath,['tests/sync-v4/k11/installer-smoke.mjs',o.before,o.after,prefix,o['rollback-roundtrip']==='yes'?'yes':'no'],14*60*1000);
  if(install.exit!==0)throw Error('Isolated supported installer/snapshot rehearsal failed');
 }
 const proof=JSON.parse(fs.readFileSync(join(prefix,'result.json')));
 report.installer=proof;
 report.steps.push({id:'upgrade-dev-branch-selection',status:'not_run',reason:'upgrade --dev is hardcoded to main (src/core/install/mod.rs:1387-1393), not the supplied V4 revision. Exact supported update apply --version exercised instead; controller must deliver/merge the pinned V4 release.'});
 const importer=join(next.source,'src/core/store/import/mod.rs');
 if(o['import-build']&&o['base-inputs']){
  const receipt=JSON.parse(fs.readFileSync(o['import-build']));
  if(!receipt.pass||!receipt.terminal||receipt.revision!==next.revision||!receipt.binary.startsWith(allowed)||createHash('sha256').update(fs.readFileSync(receipt.binary)).digest('hex')!==receipt.binarySha256)throw Error('Import composition does not match the exact integration');
  const store=join(output,'import.sqlite3'),evidence=join(output,'import.json');
  const row=run('base-release-import-library',receipt.binary,[join(prefix,'snapshot-before/databases'),o['base-inputs'],store,evidence],180000);
  if(fs.existsSync(evidence))report.baseImport=JSON.parse(fs.readFileSync(evidence));
  report.steps.push({id:'base-release-import-828',status:'not_run',componentStatus:row.exit===0&&report.baseImport?.pass?'green':'red',evidence,reason:'Actual #828 registry/import library exercised on the profile snapshot, but no product CLI/store export or daemon import attachment exists in this revision. Separate destination is not the daemon store.'});
 }else report.steps.push({id:'base-release-import-828',status:'not_run',sourcePresent:fs.existsSync(importer),reason:'No verified product import operation supplied; the presence of library source is not a completed migration.'});
 if(o['shell-version']){
  const dir=join(output,'signed-shell');
  const row=run('signed-shell-reference-mechanics',process.execPath,['tests/sync-v4/k11/m7-shell.mjs',join(prefix,'result.json'),dir,o['shell-version']],5*60*1000);
  const evidence=join(dir,'shell.json');if(fs.existsSync(evidence))report.signedShell=JSON.parse(fs.readFileSync(evidence));
  report.steps.push({id:'signed-shell-activation',status:'not_run',componentStatus:row.exit===0&&report.signedShell?.pass?'green':'red',evidence,reason:'Production signed release mechanics exercised with the main reference slot. This reference does not certify a signed V4 app/Shell artifact or post-restart V4 health.'});
 }else report.steps.push({id:'signed-shell-activation',status:'not_run',reason:'No verified signed V4 slot artifact supplied. Existing Shell bind snapshot is not a signed native slot.'});save();
 const args=['tests/sync-v4/k-support/gate.mjs','--target','v4='+next.revision,'--kernel-revision',o['kernel-revision']??next.revision,'--build',o.after,'--gate-output',join(output,'catalog'),'--installed-profile',join(prefix,'result.json'),'--profile','small','--clients','1','--rtt','150','--loss','0','--k11-evidence',join(prefix,'result.json')];
 for(const key of ['modules-root','turn-bin','turn-lib','chrome','kernel-archive','kernel-archive-sha256'])if(o[key])args.push('--'+key,o[key]);
 const gate=run('upgraded-profile-regression',process.execPath,args,6*60*1000);
 const evidence=join(output,'catalog/gate.json');
 if(fs.existsSync(evidence))report.catalog=JSON.parse(fs.readFileSync(evidence));
 // Even a successful physical RTC preflight is not import/migration acceptance.
 report.fullM7Accepted=false;
 report.reason='Real profile upgrade/snapshot custody measured; M7 remains incomplete until real base import, signed Shell activation and native V4 scenario acceptance execute.';
 report.catalogExit=gate.exit;
}catch(error){report.error=error.message;}
finally{report.finishedAt=new Date().toISOString();report.terminal=true;save();}
console.log(JSON.stringify({output,terminal:true,pass:false,steps:report.steps.map(({id,status,durationMs,exit,reason})=>({id,status,durationMs,exit,reason})),error:report.error}));
process.exitCode=report.error?1:2;
