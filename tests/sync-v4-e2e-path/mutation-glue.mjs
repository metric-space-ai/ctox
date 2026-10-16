// Run only inside the GPU lane snapshot. Restore production source on every exit.
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
import {resolve} from 'node:path';
const out=resolve(process.env.CARGO_TARGET_DIR,'evidence/glue-mutations');
await mkdir(out,{recursive:true});
const cases=[
 {id:'v4-channel-owner',file:'src/core/rxdb/src/plugins/replication_webrtc/connection_handler_rs.rs',before:'if label.starts_with("ctox-v4-") {',after:'if false {',test:'v4_channel_callback_preserves_bootstrap_and_its_event_consumer',assertion:'V4 callback must not replace bootstrap'},
 {id:'replay-authority-rejection',file:'src/core/rxdb/src/plugins/replication_webrtc/scheduler.rs',before:'let publication =\n                        authorize(lane, message.value.application_id, &message.value.bytes)?;',after:'let Ok(publication) = authorize(lane, message.value.application_id, &message.value.bytes) else { continue; };',test:'v4_session_native_replays_after_channel_loss_and_acks_while_application_is_busy',assertion:'replay_pending'},
];
const receipts=[];
const run=item=>spawnSync('cargo',['test','--locked','--manifest-path','src/core/rxdb/Cargo.toml','-j2','--lib',item.test,'--','--test-threads=2','--nocapture'],{encoding:'utf8',timeout:300000,maxBuffer:4*1024*1024});
for(const item of cases){
 const original=await readFile(item.file,'utf8');
 if(original.split(item.before).length!==2)throw Error('Mutation anchor missing: '+item.id);
 const baseline=run(item);await writeFile(resolve(out,item.id+'-baseline.log'),(baseline.stdout??'')+(baseline.stderr??''));
 if(baseline.status!==0)throw Error('Red baseline: '+item.id);
 try{
  await writeFile(item.file,original.replace(item.before,item.after));
  const mutant=run(item),output=(mutant.stdout??'')+(mutant.stderr??'');
  await writeFile(resolve(out,item.id+'-mutant.log'),output);
  const killed=mutant.status!==0&&output.includes(item.assertion)&&/FAILED/.test(output)&&!/could not compile/.test(output);
  receipts.push({...item,line:original.slice(0,original.indexOf(item.before)).split('\n').length,baseline_exit:0,exit:mutant.status,killed});
  console.log('GLUE_MUTATION '+JSON.stringify(receipts.at(-1)));
  if(!killed)process.exitCode=1;
 }finally{await writeFile(item.file,original);await writeFile(resolve(out,'result.json'),JSON.stringify(receipts,null,2));}
}
// Leave a green, restored binary; a mutant executable is never acceptance evidence.
for(const item of cases){const restored=run(item);await writeFile(resolve(out,item.id+'-restored.log'),(restored.stdout??'')+(restored.stderr??''));if(restored.status!==0)throw Error('Restored baseline failed: '+item.id);}
