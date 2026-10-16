// Run after the positive Node20 suite. Mutants are restored even on a failed proof.
import {readFile,writeFile} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
const root=new URL('../',import.meta.url);
const cases=[
 {name:'grant-required',file:'kernel.mjs',old:'const grant=await transport.grantApp(appId);',next:"const grant='mutant';",args:['tests/e55.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'bulk-grants-in-hello',file:'tests/boot.test.mjs',old:"scope_tokens:[this.tokenFor('shell')]",next:"scope_tokens:[this.tokenFor('shell'),...Array.from({length:41},(_,a)=>this.tokenFor('app'+a))]",args:['--test-name-pattern=E55 41','tests/boot.test.mjs'],reason:'hello'},
];
for(const c of cases){
 const file=new URL(c.file,root),before=await readFile(file,'utf8');
 if(before.split(c.old).length!==2)throw Error(`Mutation anchor missing: ${c.name}`);
 try{
  await writeFile(file,before.replace(c.old,c.next));
  const result=spawnSync(process.execPath,['--test',...c.args],{cwd:root,encoding:'utf8',timeout:45000,maxBuffer:4*1024*1024});
  const output=(result.stdout??'')+(result.stderr??'');
  if(result.error||result.signal||!result.status||!output.includes(c.reason))throw Error(`Mutation survived or failed for an unrelated reason: ${c.name}\n${output}`);
  console.log(`MUTATION ${c.name}: red`);
 }finally{await writeFile(file,before);}
}
