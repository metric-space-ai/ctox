// E51: app/user identity comes exclusively from the bound API context.
import {CtoxError,cloneValue} from './values.mjs';
import {normalizeJson} from './app-api.mjs';
export const PREFERENCE_COLLECTION='app_user_preferences';
export function preferenceId(appId,userId,key){
 if(typeof key!=='string'||!key.length||key.length>64||!/^[a-z0-9._-]+$/.test(key))throw new CtoxError('invalid','Invalid preference key',{path:'key'});
 return `${appId}/${userId}/${key}`;
}
export function preferenceError(appId,userId,id,doc,rows){
 if(!appId||!userId||!id.startsWith(`${appId}/${userId}/`))return 'forbidden';
 if(doc){
  if(doc.app_id!==appId||doc.user_id!==userId||doc.id!==id)return 'forbidden';
  try{if(preferenceId(appId,userId,doc.key)!==id)return 'invalid';}catch{return 'invalid';}
  if(Object.keys(doc).sort().join(',')!=='app_id,id,key,user_id,value')return 'invalid';
  let encoded;try{encoded=JSON.stringify(normalizeJson(doc.value,'value'));}catch{return 'invalid';}
  if(new TextEncoder().encode(encoded).length>16384)return 'quota';
  if(rows.filter(r=>r.id!==id&&r.app_id===appId&&r.user_id===userId).length>=64)return 'quota';
 }
 return null;
}
export function createPreferencesApi({appId,userId,read,write}){
 return Object.freeze({
  get(key){const id=preferenceId(appId,userId,key),doc=read(id);return doc&&doc.app_id===appId&&doc.user_id===userId?cloneValue(doc.value):null;},
  set(key,value){const id=preferenceId(appId,userId,key),doc={id,app_id:appId,user_id:userId,key,value:normalizeJson(value,'value')};const error=preferenceError(appId,userId,id,doc,[]);if(error)throw new CtoxError(error,'Preference rejected',{path:'value'});return write('put',id,doc);},
  delete(key){return write('delete',preferenceId(appId,userId,key),null);},
 });
}
