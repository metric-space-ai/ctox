import {readFile,writeFile} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
const file='src/apps/business-os/kernel/cache.mjs',original=await readFile(file,'utf8');
// The frozen production implementation is the real pre-fix full-scan path.
const old=spawnSync('git',['show','ad8fb8ed9:src/apps/business-os/kernel/cache.mjs'],{encoding:'utf8'});if(old.status)throw Error(old.stderr);
const args=['--test','--test-name-pattern=incremental cache tick','src/apps/business-os/kernel/tests/cache.test.mjs'];
try{
 await writeFile(file,old.stdout);const red=spawnSync(process.execPath,args,{encoding:'utf8',timeout:60000});const log=(red.stdout??'')+(red.stderr??'');
 console.log(log);
 if(red.error||red.signal||!red.status||!log.includes('cache tick exceeded 5 ms'))throw Error('old-cache-full-scan survived or failed for another reason');
 console.log('MUTATION old-cache-full-scan RED cache tick exceeded 5 ms');
}finally{await writeFile(file,original);}
const green=spawnSync(process.execPath,args,{encoding:'utf8',timeout:60000});console.log(green.stdout);if(green.status||green.error)throw Error(green.stderr||'restore failed');console.log('RESTORE GREEN');
