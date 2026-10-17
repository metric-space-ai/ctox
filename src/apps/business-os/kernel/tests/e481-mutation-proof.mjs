import {readFile,writeFile} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
const root=new URL('../',import.meta.url);
const cases=[
 ['real-platform-publication','kernel.mjs','const builtinPlatform = fakeCore ?','const builtinPlatform = true ?','E48.1 real','tests/e48.test.mjs'],
 ['grant-timer-from-send','transport-webrtc.mjs',"this.scheduler.send('control',{type:'scope.grant.request',app_id}).then(()=>{","pending.timer=setTimeout(()=>{},10);this.scheduler.send('control',{type:'scope.grant.request',app_id}).then(()=>{",'THESEN timers','tests/e481-queue.test.mjs'],
 ['transfer-timer-from-send','transport-webrtc.mjs',"this.scheduler.send('transfer',{type,request_id,...body}).then(()=>{","pending.timer=setTimeout(()=>{},10);this.scheduler.send('transfer',{type,request_id,...body}).then(()=>{",'THESEN timers','tests/e481-queue.test.mjs'],
 ['no-queue-replacement','transport-webrtc.mjs',"done.reject(new CtoxError('unavailable','Transfer deadline exceeded'));","this.fail(new CtoxError('unavailable','Local queue timeout'));done.reject(new CtoxError('unavailable','Transfer deadline exceeded'));",'THESEN timers','tests/e481-queue.test.mjs'],
 ['native-commands-before-bulk','transport-webrtc.mjs','for(let lane=0;lane<3;lane++)if(ready(lane))return lane;','for(let lane=0;lane<1;lane++)if(ready(lane))return lane;','THESEN commands','tests/e481-queue.test.mjs'],
 ['fixture-commands-before-bulk','scheduler.mjs','for(const lane of [0,1,2])','for(const lane of [0,2])','THESEN commands','tests/e481-queue.test.mjs'],
 ['submit-failed-before-write','kernel.mjs','if(cmdSubmitFailed.has(id))return cmdSubmitFailed.get(id);','void id;','E48.1 real','tests/e48.test.mjs'],
];
for(const [name,file,old,next,pattern,testFile]of cases){const url=new URL(file,root),source=await readFile(url,'utf8');if(source.split(old).length!==2)throw Error('Mutation anchor missing: '+name);try{await writeFile(url,source.replace(old,next));const result=spawnSync(process.execPath,['--test','--test-concurrency=2','--test-name-pattern='+pattern,testFile],{cwd:root,encoding:'utf8',timeout:30000});const output=(result.stdout??'')+(result.stderr??'');if(result.error||result.signal||!result.status||!output.includes('ERR_ASSERTION'))throw Error('Mutation survived or unrelated failure: '+name+'\n'+output);console.log('MUTATION '+name+': red');}finally{await writeFile(url,source);}}
