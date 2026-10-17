// E64: validate the actual second recovery receipt, never a restarted service.
import fs from 'node:fs';
import {pathToFileURL} from 'node:url';
export function extraRecoveryComplete(record){
 const s=record?.secondRecovery,calls=record?.calls;
 if(!s?.terminal||!Array.isArray(calls)||!s.beforeReadback?.matchesBefore)return false;
 const r=calls[s.rollbackCallIndex],f=calls[s.forwardCallIndex],status=calls[s.statusCallIndex];
 return s.rollbackCallIndex<s.forwardCallIndex&&s.forwardCallIndex<s.statusCallIndex
  &&r?.code===0&&r.args?.join(' ')==='update rollback'&&r.executableSha256===record.after.binarySha256
  &&f?.code===0&&f.args?.[0]==='update'&&f.args?.[1]==='apply'&&f.args?.includes('--force')&&f.executableSha256===record.after.binarySha256
  &&status?.code===0&&status.args?.join(' ')==='status'
  &&s.rollback?.phase==='rolled_back'&&s.rollback.binarySha256===record.before.binarySha256&&s.rollback.dataReadback?.matchesBefore===true
  &&s.forward?.phase==='completed'&&s.forward.binarySha256===record.after.binarySha256&&s.forward.dataReadback?.matchesBefore===true
  &&s.forward.serviceStatus?.exit===0&&s.forward.serviceStatus.running===true;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const i=process.argv.indexOf('--record');if(i<0)throw Error('Actual installer receipt required');
 const record=JSON.parse(fs.readFileSync(process.argv[i+1]));
 if(process.argv.includes('--remove-second'))delete record.secondRecovery;
 const pass=extraRecoveryComplete(record);console.log(JSON.stringify({pass,mutation:process.argv.includes('--remove-second')}));process.exitCode=pass?0:1;
}
