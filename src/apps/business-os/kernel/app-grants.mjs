// E55: one instance-signed grant per app, acquired on the control lane.
export const APP_SCOPE_DOMAIN='ctox:v4:scope\0';
export const SHELL_APP_ID='shell';
const grantRecord=v=>v!==null&&typeof v==='object'&&!Array.isArray(v);
const grantText=v=>typeof v==='string'&&v.length>0;
export const grantCanonicalJson=v=>Array.isArray(v)?'['+v.map(grantCanonicalJson).join(',')+']':grantRecord(v)?'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+grantCanonicalJson(v[k])).join(',')+'}':JSON.stringify(v);
const grantKeys=(v,keys)=>grantRecord(v)&&Object.keys(v).sort().join(',')===[...keys].sort().join(',');
export function decodeAppGrant(token){
 if(typeof token!=='string'||token.length>96000||! /^[A-Za-z0-9_-]+$/.test(token))return null;
 try{const b=token.replace(/-/g,'+').replace(/_/g,'/'),v=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(Uint8Array.from(atob(b+'='.repeat((4-b.length%4)%4)),c=>c.charCodeAt(0))));return grantKeys(v,['claims','signature'])&&grantRecord(v.claims)&&/^[A-Za-z0-9_-]{86}$/.test(v.signature)?v:null;}catch{return null;}
}
export function validAppGrantClaims(c,{instanceId,userId,deviceId,rightsVer,appId,now=Date.now()}={}){
 if(!grantKeys(c,['instance_id','user_id','app_id','scope_hash','rights_ver','device_id','exp','collections'])||![c.instance_id,c.user_id,c.app_id,c.scope_hash,c.device_id].every(grantText)||typeof c.rights_ver!=='string'||! /^(0|[1-9][0-9]*)$/.test(c.rights_ver)||BigInt(c.rights_ver)>18446744073709551615n||!Number.isSafeInteger(c.exp)||c.exp*1000<=now)return false;
 if((instanceId!==undefined&&c.instance_id!==instanceId)||(userId!==undefined&&c.user_id!==userId)||(deviceId!==undefined&&c.device_id!==deviceId)||(rightsVer!==undefined&&c.rights_ver!==String(rightsVer))||(appId!==undefined&&c.app_id!==appId))return false;
 const names=new Set();return Array.isArray(c.collections)&&c.collections.every(row=>{
  if(!grantKeys(row,['collection','gen','rights','field_mask_digest'])||!grantText(row.collection)||names.has(row.collection)||!Number.isSafeInteger(row.gen)||row.gen<1||!grantText(row.field_mask_digest)||!Array.isArray(row.rights)||new Set(row.rights).size!==row.rights.length||!row.rights.length||row.rights.some(r=>!['read','write'].includes(r)))return false;
  names.add(row.collection);return true;
 });
}
export function appGrantScope(collection,tokens,identity){
 for(const token of tokens){const c=decodeAppGrant(token)?.claims;if(!validAppGrantClaims(c,identity))continue;const row=c.collections.find(row=>row.collection===collection&&row.rights.includes('read'));if(row)return {scope_hash:c.scope_hash,gen:row.gen,participant:c.user_id,app_id:c.app_id};}
 return null;
}
