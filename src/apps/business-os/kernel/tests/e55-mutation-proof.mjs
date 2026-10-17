// Run after the positive Node20 suite. Mutants are restored even on a failed proof.
import {readFile,writeFile} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
const root=new URL('../',import.meta.url);
const cases=[
 {name:'grant-verification-coalescing',file:'transport-webrtc.mjs',old:'    clearTimeout(pending.timer);\n    try{',next:'    this.grantRequests.delete(appId);clearTimeout(pending.timer);\n    try{',args:['--test-name-pattern=grant acquisition stays coalesced','tests/boot.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'shell-hello-only',file:'transport-webrtc.mjs',old:"if(app_id==='shell'){if(existing)return existing;",next:"if(false){if(existing)return existing;",args:['--test-name-pattern=Shell grant comes only','tests/boot.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'session-shell-hello-only',file:'session.mjs',old:"if(appId==='shell')throw new CtoxError('forbidden','Shell grant requires hello_ok');",next:'void appId;',args:['--test-name-pattern=Shell grant comes only','tests/boot.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'boot-app-grant-dedupe',file:'transport-webrtc.mjs',old:'if(existing&&!renew)return existing;',next:'if(false)return existing;',args:['--test-name-pattern=Shell grant comes only','tests/boot.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'boot-denied-grant-dedupe',file:'transport-webrtc.mjs',old:'if(!renew&&this.grantFailures.has(app_id))throw this.grantFailures.get(app_id);',next:'void app_id;',args:['--test-name-pattern=Shell grant comes only','tests/boot.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'boot-grant-rights-fence',file:'transport-webrtc.mjs',old:'claims.rights_ver===this.currentRightsVer&&',next:'',args:['--test-name-pattern=Shell grant comes only','tests/boot.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'grant-lease-renewal',file:'transport-webrtc.mjs',old:'armGrantRefresh(app_id,token){this.cancelGrantRefresh(app_id);',next:'armGrantRefresh(app_id,token){return;this.cancelGrantRefresh(app_id);',args:['--test-name-pattern=live owner grants renew','tests/boot.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'forged-token',file:'boot.mjs',old:"if(await subtle.verify({name:'Ed25519'},key,signature,bytes))return true;",next:'if(true)return true;',args:['--test-name-pattern=app grants reject wrong identity','tests/boot.test.mjs'],reason:'ERR_ASSERTION'},
 {name:'grant-required',file:'kernel.mjs',old:'const grant=await transport.grantApp(appId);',next:"const grant='mutant';",args:['--test-name-pattern=activation without a grant','tests/e55.test.mjs'],reason:'ERR_ASSERTION'},
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
