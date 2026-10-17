import {readFile,writeFile} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const kernel=fileURLToPath(new URL('../',import.meta.url)).replace(/\/$/,'');
const [stub]=process.argv.slice(2);
if(!stub)throw Error('usage: node tests/hotplug-mutation-proof.mjs <stub-root>');
const cases=[
 ['pending-owner-read','kernel.mjs','if(allowPending && foreign?.ops?', 'if(false && foreign?.ops?'],
 ['query-generation-refresh','kernel.mjs','const ns=resolveNs(app,collection);depend(`c:${ns}`);lastUsed.set(ns,Date.now());','depend(`c:${ns}`);lastUsed.set(ns,Date.now());'],
 ['live-generation-dependency','kernel.mjs','if(subs.size){if(!tagIndex.has(tag))','if(false){if(!tagIndex.has(tag))'],
];
for(const [name,file,old,next]of cases){const path=kernel+'/'+file,original=await readFile(path,'utf8');if(!original.includes(old))throw Error('Mutation anchor missing: '+name);try{await writeFile(path,original.replace(old,next));const p=spawnSync(process.execPath,['--test','--test-concurrency=2',kernel+'/tests/hotplug.test.mjs'],{encoding:'utf8',timeout:30000});const text=p.stdout+p.stderr;if(!p.status||p.signal||!text.includes('ERR_ASSERTION'))throw Error(name+' survived or unrelated: '+text);console.log('MUTATION '+name+' red');}finally{await writeFile(path,original);}}
for(const [name,old,next]of [['stub-pending-owner','if(allowPending && foreign?.ops?','if(false && foreign?.ops?'],['stub-query-generation','const ns=resolveNs(app,collection);depend(`c:${ns}`);subscribeNs(ns);','depend(`c:${ns}`);subscribeNs(ns);'],['stub-live-dependency','if(subs.size){if(!tagIndex.has(tag))','if(false){if(!tagIndex.has(tag))']]){const path=stub+'/ctox-app-api-stub.mjs',original=await readFile(path,'utf8');if(!original.includes(old))throw Error('Mutation anchor missing: '+name);try{await writeFile(path,original.replace(old,next));const p=spawnSync(process.execPath,['--test','--test-concurrency=2',stub+'/test/hotplug.test.mjs'],{encoding:'utf8',timeout:30000});const text=p.stdout+p.stderr;if(!p.status||p.signal||!text.includes('ERR_ASSERTION'))throw Error(name+' survived or unrelated: '+text);console.log('MUTATION '+name+' red');}finally{await writeFile(path,original);}}
