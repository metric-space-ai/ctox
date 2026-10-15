// Sequential, bounded contract mutations in a disposable copy, never in source.
import {cp,mkdir,mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
const kernel=resolve('src/apps/business-os/kernel'),out=resolve(process.env.KERNEL_EVIDENCE_DIR??process.env.TMPDIR,'k2-mutations');
await mkdir(out,{recursive:true});const scratch=await mkdtemp(join(out,'copy-')),base=join(scratch,'src/apps/business-os/kernel');
await cp(kernel,base,{recursive:true});await mkdir(resolve(base,'../rxdb/src'),{recursive:true});await cp(resolve(kernel,'../rxdb/src/hello.mjs'),resolve(base,'../rxdb/src/hello.mjs'));
const cases=[
 ['window','scheduler.mjs','this.sent[lane]-this.acked[lane]<this.windowFrames','true','scheduler.test.mjs','window sends'],
 ['ack-preemption','scheduler.mjs','if(noAck)this.queues[lane].unshift(entry);else this.queues[lane].push(entry);','this.queues[lane].push(entry);','scheduler.test.mjs','window sends'],
 ['association-budget','scheduler.mjs','this.buffered()+size>this.maxBuffered','false','scheduler.test.mjs','connection buffer'],
 ['unicode-codec','framing.mjs',"codec==='gzip'?1","codec==='gzip'?2",'scheduler.test.mjs','gzip reassembly'],
 ['parallel-ingress','ingress.mjs','this.chains[lane]','this.chains[0]','scheduler.test.mjs','blocked initial'],
 ['future-ack','scheduler.mjs','||n>this.sent[lane]','','scheduler.test.mjs','malformed fragments'],
 ['write-chain','transport-webrtc.mjs',"{type:'write',...entry}","{type:'write',...entry,prev_op_id:undefined}",'webrtc.test.mjs','P4 hello'],
 ['page-auth','transport-webrtc.mjs','!await this.options.verifyPackage(message,this.session)','false','webrtc.test.mjs','bad signature'],
 ['page-progress','transport-webrtc.mjs','seqGreater(cursor,head)','false','webrtc.test.mjs','bad signature'],
 ['download-hash','transport-webrtc.mjs','await sha256Hex(blob)!==ref','false','webrtc.test.mjs','CAS upload'],
 ['pairing','transport-webrtc.mjs','hello?.instance_id!==identity.instanceId','false','webrtc.test.mjs','wrong pairing'],
 ['clock-metrics','kernel.mjs',"timing=next;notify('metrics');","notify('metrics');",'kernel-v4.test.mjs','real transport'],
 ['platform-owner','kernel.mjs','SYSTEM_COLLECTIONS.has(collection)?PLATFORM_APP_ID:app.id','app.id','kernel-v4.test.mjs','platform collections'],
 ['rollback-pointer','kernel.mjs','activated_ver:lastGood','activated_ver:ver','contract.test.mjs','activation.reportFailed'],
 ['artifact-retry','kernel.mjs','attempt<2','attempt<1','contract.test.mjs','Artefakt mit falschem Hash'],
 ['layout','kernel.mjs','normalizeLayout(o.layout??prevManifest?.layout)','normalizeLayout(null)','contract.test.mjs','kernel.apps liefert layout'],
 ['tab-isolation','kernel.mjs','tab:${tabKey}:${key}','tab:shared:${key}','contract.test.mjs','windowState je Tab'],
 ['preference-owner','kernel.mjs',"collection==='shell_preferences' && id!==userId",'false','contract.test.mjs','windowState je Tab'],
 ['record-id','kernel.mjs','record_id:commandOptions.record_id','record_id: undefined','contract.test.mjs','registerHeader nimmt'],
 ['first-shell-deadline','kernel.mjs','await withTimeout(Promise.resolve().then(()=>shellModule.startShell(kernel,{restored:o.restored})),remountTimeoutMs,\n        ()=>{try{shellModule.stopShell?.();}catch{}});','await shellModule.startShell(kernel,{restored:o.restored});','contract.test.mjs','Shell-Tausch'],
 ['shell-artifact-url','artifact-url.mjs','~shell@${encodeURIComponent(shellVersion)}','','loader.test.mjs','artifact URLs'],
 ['page-cursor-atomicity','store-idb-v5.mjs','const result = await fn(this._ops(next));','this.stores = next; const result = await fn(this._ops(next));','kernel-v4.test.mjs','real transport'],
 ['status-alias','kernel.mjs','status:statusZone','status:undefined','contract.test.mjs','Kompatibilität'],
 ['reconnect-address','transport-webrtc.mjs','this.options={...this.options,address,channels:undefined,peer:undefined}','this.options={...this.options,channels:undefined,peer:undefined}','kernel-v4.test.mjs','real transport'],
 ['clock-start','transport-webrtc.mjs','async startHello(){this.started=this.now();','async startHello(){','webrtc.test.mjs','P4 hello'],
 ['shell-failure-report','loader.mjs',"else await kernel.activation.reportFailed('shell',String(ver),outcome.error??'shell_start_failed');",'','loader.test.mjs','loader follows shell activation'],
];
const receipts=[];
try{for(const [id,file,before,after,test,pattern]of cases){const path=join(base,file),original=await readFile(path,'utf8');if(!original.includes(before))throw Error(`Mutation anchor missing: ${id}`);const line=original.slice(0,original.indexOf(before)).split('\n').length;
 const baseline=spawnSync(process.execPath,['--test','--test-concurrency=1','--test-timeout=3500',`--test-name-pattern=${pattern}`,join(base,'tests',test)],{encoding:'utf8',timeout:7000,maxBuffer:2*1024*1024});
 if(baseline.status!==0)throw Error(`Mutation baseline is red: ${id}\n${baseline.stdout}\n${baseline.stderr}`);
 await writeFile(path,original.split(before).join(after));
 const run=spawnSync(process.execPath,['--test','--test-concurrency=1','--test-timeout=3500',`--test-name-pattern=${pattern}`,join(base,'tests',test)],{encoding:'utf8',timeout:7000,maxBuffer:2*1024*1024});
 await writeFile(path,original);const output=(run.stdout??'')+(run.stderr??'');const killed=run.status!==0&&/not ok/.test(output)&&!/SyntaxError|ERR_MODULE_NOT_FOUND/.test(output);
 const receipt={id,file,line,before,after,test,pattern,exit:run.status,killed,output};receipts.push(receipt);await writeFile(join(out,'mutation-proof.json'),JSON.stringify(receipts,null,2));
 console.log(`${killed?'MUTATION_KILLED':'MUTATION_SURVIVED'} ${id} ${file}:${line} -> ${test} /${pattern}/`);if(!killed)process.exitCode=1;
 }
 const browser=id=>spawnSync('bash',[join(base,'tests/run-browser.sh')],{cwd:scratch,encoding:'utf8',timeout:40000,maxBuffer:2*1024*1024,env:{...process.env,KERNEL_EVIDENCE_DIR:join(out,id)}});
 const control=browser('browser-control');if(control.status!==0)throw Error(`Browser mutation baseline is red\n${control.stdout}\n${control.stderr}`);
 for(const [id,file,before,after,expected]of [
  ['claim-before-import','loader.mjs',"worker.postMessage('ctox:claim');",'/* omitted claim */',/Shell replacement failed|claim deadline|Shell imported before claim/],
  ['claim-every-start','loader.mjs',"worker.postMessage('ctox:claim');","if(!container.controller)worker.postMessage('ctox:claim');",/Shell replacement failed|Shell imported before claim/],
  ['close-owned-transactions','store-idb-v5.mjs','await Promise.allSettled([...this.transactions.values()]);','',/Close other identity tabs/],
 ]){
  const path=join(base,file),original=await readFile(path,'utf8');if(!original.includes(before))throw Error(`Browser mutation anchor missing: ${id}`);
  await writeFile(path,original.replace(before,after));const run=browser(id);await writeFile(path,original);
  const output=(run.stdout??'')+(run.stderr??'');const killed=run.status!==0&&expected.test(output);
  receipts.push({id,file,line:original.slice(0,original.indexOf(before)).split('\n').length,before,after,test:'browser.mjs',baseline_exit:control.status,exit:run.status,killed,output});
  await writeFile(join(out,'mutation-proof.json'),JSON.stringify(receipts,null,2));console.log(`${killed?'MUTATION_KILLED':'MUTATION_SURVIVED'} ${id}`);if(!killed)process.exitCode=1;
 }
 }finally{await rm(scratch,{recursive:true,force:true});}
