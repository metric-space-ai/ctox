// Explicit fake issuer; never reachable from the production entry points (E52).
import {APP_SCOPE_DOMAIN,grantCanonicalJson,validAppGrantClaims} from './app-grants.mjs';
const enc=s=>new TextEncoder().encode(s),url=bytes=>btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'');
export function createFakeAppGrantIssuer(){
 let keys;
 return async claims=>{
  if(!validAppGrantClaims(claims))throw new Error('Invalid fake app grant claims');
  keys??=crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify']);const pair=await keys;
  const signature=url(await crypto.subtle.sign('Ed25519',pair.privateKey,enc(APP_SCOPE_DOMAIN+grantCanonicalJson(claims))));
  return url(enc(grantCanonicalJson({claims,signature})));
 };
}
