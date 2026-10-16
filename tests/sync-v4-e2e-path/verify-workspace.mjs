// GPU-lane check for the real root-test namespace regression; no replacement Store.
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
const out=path.resolve(process.env.CARGO_TARGET_DIR,'evidence/e2e-1e');
fs.mkdirSync(out,{recursive:true});
const report={started:new Date().toISOString(),steps:{}};
function check(name,args){
 const log=path.join(out,name+'.log'),fd=fs.openSync(log,'w');
 let result;
 try{result=spawnSync('cargo',[...args,'--message-format=json','-j2'],{stdio:['ignore',fd,fd],timeout:1800000});}
 finally{fs.closeSync(fd);}
 const errors=[];
 for(const line of fs.readFileSync(log,'utf8').split('\n')){
  try{const value=JSON.parse(line);if(value.reason==='compiler-message'&&value.message.level==='error')errors.push(value.message.rendered??value.message.message);}catch{}
 }
 const step={exit:result.status,signal:result.signal,error:result.error?.message,log,errors};
 report.steps[name]=step;console.log('E2E_WORKSPACE '+JSON.stringify({name,...step}));return step;
}
const file='src/core/store/rights_tests.rs',before='use ctox_store::change_log;',after='#[path = "change_log.rs"]\nmod change_log;';
const original=fs.readFileSync(file,'utf8');
try{
 const baseline=check('workspace',['check','--workspace','--all-targets']);
 if(baseline.exit!==0)process.exitCode=1;
 else{
  if(original.split(before).length!==2)throw Error('Non-unique rights test import');
  try{
   fs.writeFileSync(file,original.replace(before,after));
   const mutant=check('test-import-mutant',['check','--bin','ctox','--tests']);
   report.mutation={file,line:2,before,after,baseline_exit:0,exit:mutant.exit,killed:mutant.exit!==0&&mutant.errors.some(error=>/subscription\.rs/.test(error)&&/unresolved import/.test(error))};
   console.log('E2E_IMPORT_MUTATION '+JSON.stringify(report.mutation));
   if(!report.mutation.killed)process.exitCode=1;
  }finally{fs.writeFileSync(file,original);}
  const restored=check('test-import-restored',['check','--bin','ctox','--tests']);
  if(restored.exit!==0)process.exitCode=1;
 }
}finally{
 fs.writeFileSync(file,original);
 report.finished=new Date().toISOString();
 fs.writeFileSync(path.join(out,'workspace-proof.json'),JSON.stringify(report,null,2));
}
