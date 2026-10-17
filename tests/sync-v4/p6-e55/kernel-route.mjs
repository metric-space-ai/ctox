// Run the actual kernel verifier closure from a pinned, unmodified source snapshot.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {webcrypto} from 'node:crypto';
const [vectorPath,kernelPath,mutate]=process.argv.slice(2);
const v=JSON.parse(await readFile(vectorPath,'utf8'));
const {decodeAppGrant,validAppGrantClaims,grantCanonicalJson:canonicalJson,APP_SCOPE_DOMAIN:SCOPE_DOMAIN}=await import(pathToFileURL(kernelPath+'/app-grants.mjs'));
const boot=await readFile(kernelPath+'/boot.mjs','utf8');
let body=boot.match(/const verifyScopeToken=(async\(token,\{rights_ver,appId='shell'\}\)=>\{[\s\S]*?\n \});/)[1];
if(mutate==='wrong-key')body=body.replace("if(await subtle.verify({name:'Ed25519'},key,signature,bytes))return true;",'if(true)return true;');
const key=await webcrypto.subtle.importKey('raw',Buffer.from(v.public.slice(8),'hex'),'Ed25519',false,['verify']);
const verify=new Function('decodeAppGrant','validAppGrantClaims','instanceId','userId','current','signatureBytes','utf8','SCOPE_DOMAIN','canonicalJson','keys','subtle','return '+body)(decodeAppGrant,validAppGrantClaims,v.claims.instance_id,v.claims.user_id,{hello:{offer:{device_proof:{device_id:v.claims.device_id}}}},s=>Buffer.from(s,'base64url'),s=>new TextEncoder().encode(s),SCOPE_DOMAIN,canonicalJson,new Map([['instance',key]]),webcrypto.subtle);
const clock=Date.now;Date.now=()=>1001*1000;
try{
 const context={rights_ver:v.claims.rights_ver,appId:v.claims.app_id};
 assert.equal(await verify(v.token,context),true,'native E22 grant rejected by kernel');
 assert.equal(await verify(v.wrong_token,context),false,'foreign key accepted by kernel');
 assert.equal(await verify(v.token,{...context,appId:'b'}),false,'foreign app accepted by kernel');
 Date.now=()=>v.claims.exp*1000;
 assert.equal(await verify(v.token,context),false,'expired native grant accepted by kernel');
 console.log('P6_E55 kernel native-route token + foreign key + app + expiry PASS');
}finally{Date.now=clock;}
