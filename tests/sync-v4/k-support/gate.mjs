#!/usr/bin/env node
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {parseTarget,resolveRevision,comparison,markdown} from './target.mjs';
import {catalogAccepted} from '../../sync-v4-e2e/criteria.mjs';
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const expected=['K01','K07','K11','K15','K02','K24'];
const declared=fs.readdirSync('tests/sync-v4').filter(n=>/^k[0-9]+$/.test(n)).map(n=>'K'+n.slice(1));
if(!declared.length)throw Error('ZERO scenarios discovered');
if(expected.some(id=>!declared.includes(id)))throw Error('Missing mandatory catalog scenario');
const output=resolve(process.env.BUILD_LANE_BIN,'../artifacts/e2e-k-'+Date.now());
fs.mkdirSync(output,{recursive:false,mode:0o700});
const raw=process.argv.slice(2),options={};const args=[];
for(let i=0;i<raw.length;i+=2){if(!raw[i]?.startsWith('--')||raw[i+1]===undefined)throw Error('Expected --key value');
 if(['target','target-requested','kernel-revision','k24-source','k24-mode','k24-evidence'].includes(raw[i].slice(2)))options[raw[i].slice(2)]=raw[i+1];else args.push(raw[i],raw[i+1]);}
const target=parseTarget(options.target);
const baseline=JSON.parse(fs.readFileSync(new URL('./baseline-v3.json',import.meta.url)));

if(args.includes('--output')||args.includes('--scenario')||args.includes('--catalog'))throw Error('Gate owns catalog/output; no scenario subset');
const summary={target,baselineRevision:baseline.revision,startedAt:new Date().toISOString(),expected,scenarios:[],terminal:false,pass:false};
const save=()=>{
 if(target.kind==='v4')summary.comparison=comparison(baseline,summary.scenarios);
 else summary.comparison=Object.entries(baseline.scenarios).map(([id,b])=>({id,v3Value:b.value,v4Value:null,targetValue:b.targetValue,verdict:'not_run',reason:'V3 execution does not measure V4',v3Evidence:b.evidence}));
 fs.writeFileSync(join(output,'gate.json'),JSON.stringify(summary,null,2));
 fs.writeFileSync(join(output,'comparison.json'),JSON.stringify(summary.comparison,null,2));
 fs.writeFileSync(join(output,'comparison.md'),markdown(summary.comparison));
};
save();
if(target.kind==='v4'){
 target.requestedLabel=options['target-requested']??target.requested;
 target.revision=resolveRevision(target.requested);
 if(!target.revision&&/^[a-f0-9]{40}$/.test(target.requested)){
  const fetch=spawnSync('git',['fetch','origin',target.requested],{encoding:'utf8',timeout:60000});target.fetchExit=fetch.status;
  if(fetch.status===0)target.revision=resolveRevision(target.requested);
 }
 target.kernelRevision=resolveRevision(options['kernel-revision']??'fa1b88498cdb9beed020a9bc8f32a4f4037dca20');
 let reason;
 if(!target.revision)reason='Requested V4 revision is absent: '+target.requested;
 else if(!target.kernelRevision)reason='Browser kernel #823 revision is absent';
 else{
  const provided=args.indexOf('--build');
  let receipt;
  if(provided>=0)receipt=args[provided+1];else{
   const producer=resolve(process.env.BUILD_LANE_BIN,'../artifacts/v4-e2e-build-'+target.revision.slice(0,12));
   const log=fs.openSync(join(output,'build.log'),'w',0o600);
   const build=spawnSync(process.execPath,['scripts/sync-v4/e2e-build.mjs','--revision',target.revision,'--output',producer],{stdio:['ignore',log,log],timeout:65*60*1000});fs.closeSync(log);
   summary.buildExit=build.status;receipt=join(producer,'build.json');
  }
  try{
   const {verifiedBuild}=await import('../../sync-v4-e2e/native.mjs');const b=verifiedBuild(receipt);
   if(b.revision!==target.revision)throw Error('Provided binary does not match requested target');
   summary.build={receipt,revision:b.revision,binarySha256:b.binarySha256};
   // Do not drive a V4 target through legacy db.collections/startCollection.
   // #823 intentionally delegates pairing, hello, signatures and scopes to the
   // host; a native bootstrap has to supply those authorities before K cases.
   const kernelSource=spawnSync('git',['show',target.kernelRevision+':src/apps/business-os/kernel/transport-webrtc.mjs'],{encoding:'utf8'});
   summary.browserKernel={revision:target.kernelRevision,sourcePresent:kernelSource.status===0,
    hostSupplies:['createPeer','hello','resolveScope','verifyPackage'],nativeBindingExecuted:false};
   reason='V4 app/store scenario adapter is not yet bound to native pairing/hello/signature/scope authorities required by #823; V3 db.collections/lease fallback deliberately not used. This is an open harness integration, not a measured product failure.';

  }catch(e){reason='V4 build unverified: '+e.message;summary.buildReceipt=receipt;}
 }
 for(const id of expected){
  if(id==='K24'&&options['k24-evidence']){
   const file=resolve(options['k24-evidence']),row=JSON.parse(fs.readFileSync(file));
   if(row.id!=='K24'||row.terminal!==true||row.cleanup!=='owned groups absent'||row.originCommit!=='1ab7ceb03ceaeac763ea7ec99be56896608f83cb')throw Error('K24 evidence is not terminal, owned, or from the pinned tool');
   summary.scenarios.push({...row,id,evidence:file,measured:{target:row.target,criteria:row.criteria,measures:row.measured},bindingOnly:row.target==='bind'});
  }else summary.scenarios.push({id,status:'not_run',pass:false,acceptanceComplete:false,reason,evidence:summary.build?.receipt??summary.buildReceipt??null});
 }

 summary.terminal=true;summary.finishedAt=new Date().toISOString();summary.actuallyRan=summary.scenarios.filter(r=>r.status==='measured').map(r=>r.id);summary.pass=false;save();
 console.log(markdown(summary.comparison));console.log('V4 native scenarios not_run: '+reason+'; evidence '+output);process.exit(2);
}
for(const id of expected){
 const dir=join(output,id);
 if(id==='K24'&&options['k24-source']){
  const log=fs.openSync(join(output,'K24.log'),'w',0o600);
  const modulesAt=args.indexOf('--modules-root');
  const p=spawnSync(process.execPath,['tests/sync-v4/k24/execute.mjs','--target',options['k24-mode']??'bind','--source',options['k24-source'],'--output',dir,'--playwright',join(modulesAt>=0?args[modulesAt+1]:resolve(process.env.BUILD_LANE_BIN,'../deps/shell-browser-collection-auth/node_modules'),'playwright')],{stdio:['ignore',log,log],timeout:13*60*1000});fs.closeSync(log);
  const file=join(dir,'result.json'),row=fs.existsSync(file)?JSON.parse(fs.readFileSync(file)):{};
  summary.scenarios.push({...row,id,evidence:file,processExit:p.status,accepted:row.pass===true&&row.acceptanceComplete===true});save();
  console.log(`${id} | ${row.acceptanceComplete&&row.pass?'green':row.status==='not_run'?'not_run':'red'} | ${file}`);continue;
 }

 const log=fs.openSync(join(output,id+'.log'),'w',0o600);
 // A new tenant per scenario prevents prior bulk writes and failed injections
 // from silently changing the next incident's supposedly deterministic profile.
 let p;try{p=spawnSync(process.execPath,['scripts/sync-v4/e2e-run.mjs',...args,'--catalog','yes','--scenario',id,'--output',dir],{stdio:['ignore',log,log],timeout:35*60*1000});}finally{fs.closeSync(log);}
 const file=join(dir,'result.json');
 const report=fs.existsSync(file)?JSON.parse(fs.readFileSync(file)):null;
 const row=report?.scenarios.find(s=>s.id===id);
 const accepted=!!report&&catalogAccepted(report,[id])&&p.status===0;
 summary.scenarios.push({id,measured:row?.measured??null,reason:row?.reason??null,status:row?.status||'not_run',measuredPass:row?.pass===true,accepted,acceptanceComplete:row?.acceptanceComplete===true,
  error:row?.error||report?.error||p.error?.message||null,cleanup:report?.cleanup||null,processExit:p.status,evidence:file,log:join(output,id+'.log')});
 save();console.log(`${id} | ${accepted?'green':'red'} | ${file} (${row?.status||'not_run'}; complete=${row?.acceptanceComplete===true})`);
}
summary.terminal=true;summary.finishedAt=new Date().toISOString();
summary.actuallyRan=summary.scenarios.filter(s=>['measured','failed'].includes(s.status)).map(s=>s.id);
summary.pass=summary.actuallyRan.length>0&&summary.scenarios.every(s=>s.accepted);save();
console.log('K-no. | red/green | evidence');
for(const row of summary.scenarios)console.log(`${row.id} | ${row.status==='not_run'?'not_run':row.accepted?'green':'red'} | ${row.evidence}`);
if(!summary.actuallyRan.length)throw Error('ZERO scenarios ran; see '+join(output,'gate.json'));
process.exitCode=summary.pass?0:1;
