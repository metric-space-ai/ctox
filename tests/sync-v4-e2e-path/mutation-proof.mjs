// Mutations run only in this admitted lane's private source snapshot, restored in finally.
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
import {resolve} from 'node:path';
const out=process.env.E2E_MUTATIONS_DIR?resolve(process.env.E2E_MUTATIONS_DIR):resolve(process.env.CARGO_TARGET_DIR,process.env.E2E_MUTATIONS_NATIVE_ONLY==='1'?'evidence/e2e-native-mutations':'evidence/e2e-mutations');await mkdir(out,{recursive:true});const receipts=[];await writeFile(resolve(out,'mutation-proof.json'),'[]');
const run=(cmd,args,name,env={})=>{const result=spawnSync(cmd,args,{encoding:'utf8',timeout:180000,maxBuffer:4*1024*1024,env:{...process.env,...env}});return {result,output:(result.stdout??'')+(result.stderr??'')};};
const cases=[
 {id:'ingress-window',file:'src/apps/business-os/kernel/transport-webrtc.mjs',before:'this.incoming[lane]+bytes.length>budget',after:'this.incoming[lane]+bytes.length>512*1024',test:'tests/sync-v4-e2e-path/ingress-window.test.mjs'},
 {id:'peer-state',file:'src/apps/business-os/kernel/transport-webrtc.mjs',before:"this.listen(endpoint.peer,'connectionstatechange',peerState);this.listen(endpoint.peer,'iceconnectionstatechange',peerState);",after:'',test:'tests/sync-v4-e2e-path/peer-state.test.mjs'},
 {id:'create-base-seq',file:'src/apps/business-os/kernel/transport-webrtc.mjs',before:"base_seq:entry.base_seq==='0'&&entry.base_rev==null?null:entry.base_seq??null",after:'base_seq:entry.base_seq??null',test:'tests/sync-v4-e2e-path/client-seq.test.mjs'},
 {id:'merged-wire',file:'src/apps/business-os/kernel/transport-webrtc.mjs',before:"['accepted','rejected','migrated','merged'].includes(message.status)",after:"['accepted','rejected','migrated'].includes(message.status)",test:'tests/sync-v4-e2e-path/requires-page.test.mjs'},
 {id:'canonical-page-before-retire',file:'src/apps/business-os/kernel/kernel.mjs',before:"const canonical=res.requires_page || res.status==='merged' || res.status==='migrated';",after:'const canonical=false;',test:'tests/sync-v4-e2e-path/requires-page.test.mjs'},
 {id:'client-seq-wire',file:'src/apps/business-os/kernel/transport-webrtc.mjs',before:'client_seq:decimalSeq(entry.client_seq),',after:'',test:'tests/sync-v4-e2e-path/client-seq.test.mjs'},
 {id:'client-seq-window',file:'src/apps/business-os/kernel/kernel.mjs',before:'if (e.client_seq != null && BigInt(e.client_seq) >= BigInt(floor) + 64n) continue;',after:'/* gap window deliberately removed */',test:'tests/sync-v4-e2e-path/client-seq.test.mjs'},
 {id:'client-seq-high-water',file:'src/apps/business-os/kernel/outbox.mjs',before:"await t.put('meta', { key: 'client_seq', value: String(seq) });",after:'void seq;',test:'tests/sync-v4-e2e-path/client-seq.test.mjs'},
 {id:'client-seq-reuse',file:'src/apps/business-os/kernel/outbox.mjs',before:'entry.client_seq = String(++seq);',after:'entry.client_seq = String(seq);',test:'tests/sync-v4-e2e-path/client-seq.test.mjs'},
 {id:'stale-leader-send',file:'src/apps/business-os/kernel/kernel.mjs',before:"const sent = await db.tx(async t=>{await assertGen(t);const entry=",after:"const sent = await db.tx(async t=>{const entry=",test:'tests/sync-v4-e2e-path/leader.test.mjs'},
 {id:'session-replay',file:'src/apps/business-os/kernel/transport-webrtc.mjs',before:'replay(){return Promise.all(this.state.retained.flatMap',after:'replay(){return Promise.all([].flatMap',test:'tests/sync-v4-e2e-path/native-wire.test.mjs'},
 {id:'session-duplicate',file:'src/apps/business-os/kernel/transport-webrtc.mjs',before:'if(BigInt(id)<=this.state.delivered[lane])',after:'if(false)',test:'tests/sync-v4-e2e-path/native-wire.test.mjs'},
];
for(const item of (process.env.E2E_MUTATIONS_NATIVE_ONLY==='1'?[]:cases.filter(item=>!process.env.E2E_MUTATIONS_ONLY||item.id===process.env.E2E_MUTATIONS_ONLY))){const original=await readFile(item.file,'utf8');if(original.split(item.before).length!==2)throw Error('mutation anchor '+item.id);
 const baseline=run(process.execPath,['--test','--test-reporter=tap','--test-timeout=10000',item.test],item.id+'-baseline');await writeFile(resolve(out,item.id+'-baseline.log'),baseline.output);if(baseline.result.status!==0)throw Error('red baseline '+item.id);
 try{await writeFile(item.file,original.replace(item.before,item.after));const mutant=run(process.execPath,['--test','--test-reporter=tap','--test-timeout=10000',item.test],item.id);await writeFile(resolve(out,item.id+'.log'),mutant.output);
 const receipt={...item,line:original.slice(0,original.indexOf(item.before)).split('\n').length,baseline_exit:0,exit:mutant.result.status,killed:mutant.result.status!==0&&/not ok/.test(mutant.output)&&!/SyntaxError|ERR_MODULE_NOT_FOUND/.test(mutant.output)};receipts.push(receipt);console.log('E2E_MUTATION '+JSON.stringify(receipt));if(!receipt.killed)process.exitCode=1;
 }finally{await writeFile(item.file,original);await writeFile(resolve(out,'mutation-proof.json'),JSON.stringify(receipts,null,2));}
}
if(process.env.E2E_NATIVE_PROOF==='1'){
 const file='src/core/store/write_contract.rs',before='if let Some(result) = prior(tx, participant, seq, now)? {',after='if let Some(result) = None::<WriteResult> {';const original=await readFile(file,'utf8');if(original.split(before).length!==2)throw Error('dedup mutation anchor');
 // The caller must have completed the unmodified native path first.
 const baseline=JSON.parse(await readFile(resolve(process.env.E2E_OUT,'result.json'),'utf8'));if(!baseline.pass)throw Error('native baseline is red');
 try{await writeFile(file,original.replace(before,after));const build=run('cargo',['build','--manifest-path','tests/sync-v4-e2e-path/native/Cargo.toml','-j','2'],'native-mutant-build');await writeFile(resolve(out,'native-mutant-build.log'),build.output);if(build.result.status!==0)throw Error('native mutant does not compile');
 const check=run('bash',['tests/sync-v4-e2e-path/run.sh'],'dedup-native',{E2E_OUT:resolve(out,'native-dedup')});await writeFile(resolve(out,'dedup-native.log'),check.output);const result=JSON.parse(await readFile(resolve(out,'native-dedup/result.json'),'utf8'));
 const receipt={id:'native-client-seq-dedup',file,line:original.slice(0,original.indexOf(before)).split('\n').length,before,after,baseline_pass:true,exit:check.result.status,killed:check.result.status!==0&&!result.pass&&result.beforeDrop?.status==='accepted'&&/accepted|write result|seq|event deadline/.test(result.error??'')};receipts.push(receipt);console.log('E2E_MUTATION '+JSON.stringify(receipt));if(!receipt.killed)process.exitCode=1;
 }finally{await writeFile(file,original);await writeFile(resolve(out,'mutation-proof.json'),JSON.stringify(receipts,null,2));const restored=run('cargo',['build','--manifest-path','tests/sync-v4-e2e-path/native/Cargo.toml','-j','2'],'restored-native');await writeFile(resolve(out,'restored-native.log'),restored.output);if(restored.result.status!==0)throw Error('restored native build failed');}
}
