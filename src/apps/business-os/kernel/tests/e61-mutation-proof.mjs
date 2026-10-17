import {readFile,writeFile} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
// Run from the repository root; this driver mutates and restores only its own contracts.
const path=p=>new URL(p,new URL('file://'+process.cwd()+'/'));
const files=['src/core/business_os/v4/ctox-platform.manifest.json','src/apps/business-os/kernel/platform-manifest.generated.mjs','src/apps/business-os/kernel/kernel.mjs','src/apps/business-os/kernel/tests/native-platform-catalog.mjs'];
const originals=new Map(await Promise.all(files.map(async f=>[f,await readFile(path(f),'utf8')])));
const run=(args)=>spawnSync(process.execPath,args,{encoding:'utf8',timeout:30000});
const assertRed=(name,p,reason)=>{const log=(p.stdout??'')+(p.stderr??'');if(p.error||p.signal||!p.status||!log.includes(reason))throw Error(name+' survived or failed for another reason:\n'+log);console.log('MUTATION '+name+' RED '+reason);};
try{
 const manifest=JSON.parse(originals.get(files[0]));const native=JSON.parse(await readFile(path('src/core/business_os/business_os_schema_contract.json'),'utf8'));
 if(native.business_commands.version!==2)throw Error('legacy V2 negative contract changed');
 manifest.collections.business_commands.schema=native.business_commands;
 await writeFile(path(files[0]),JSON.stringify(manifest,null,2)+'\n');
 const gen=run(['src/apps/business-os/kernel/tests/generate-platform-manifest.mjs']);if(gen.status)throw Error(gen.stderr);
 assertRed('legacy-v2-platform-command-schema',run(['--test','--test-name-pattern=s9 projection: authoritative terminal','src/apps/business-os/kernel/tests/s9-delivery.test.mjs']),'Pflichtfeld command_id fehlt');
 await writeFile(path(files[0]),originals.get(files[0]));await writeFile(path(files[1]),originals.get(files[1]));
 const old='!isPlainObject(definition?.schema)';if(!originals.get(files[2]).includes(old))throw Error('zero version anchor missing');
 await writeFile(path(files[2]),originals.get(files[2]).replace(old,'!definition?.schema?.version'));
 assertRed('native-files-version-zero',run(['--test','--test-name-pattern=E35 native CAS','src/apps/business-os/kernel/tests/e35-wire.test.mjs']),'Active platform catalog missing');
 await writeFile(path(files[2]),originals.get(files[2]));
 const fixture='manifest:PLATFORM_MANIFEST';if(!originals.get(files[3]).includes(fixture))throw Error('catalog anchor missing');
 await writeFile(path(files[3]),originals.get(files[3]).replace(fixture,"manifest:{...PLATFORM_MANIFEST,collections:Object.fromEntries(['business_commands','files','ctox_queue_tasks'].map(name=>[name,{schema:{version:3,type:'object'}}]))}"));
 assertRed('permissive-catalog-fixture',run(['--test','--test-name-pattern=E61 platform','src/apps/business-os/kernel/tests/e48.test.mjs']),'ERR_ASSERTION');
} finally {for(const [f,source] of originals)await writeFile(path(f),source);}
const restored=run(['--test','src/apps/business-os/kernel/tests/s9-delivery.test.mjs','src/apps/business-os/kernel/tests/e35-wire.test.mjs','src/apps/business-os/kernel/tests/e48.test.mjs']);
if(restored.status)throw Error(restored.stdout+restored.stderr);console.log('RESTORE GREEN');
