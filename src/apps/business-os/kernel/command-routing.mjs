// E39/E42: identity is the document app_id, never payload. Shared browser/Core decision.
import { COMMAND_INVENTORY } from './command-inventory.generated.mjs';
const platformTypes = new Set(['exact_control_types','predicate_control_types','browser_runtime_types','browser_literal_types'].flatMap(k=>COMMAND_INVENTORY[k]));
const chatTypes = new Set(['business_os.chat.task','business_os.context.ask','business_os.data.modify']);
export function isPlatformCommand(type) {
 if(typeof type!=='string')return false;
 return platformTypes.has(type) || COMMAND_INVENTORY.control_prefixes.some(p=>type.startsWith(p)) || chatTypes.has(type) || type.startsWith('ctox.app.') || type.startsWith('ctox.coding.') || /^ctox\.workjet\..+\.v1$/.test(type) || type==='ctox.plan.step.retry';
}
export function commandTarget(body, manifest) {
 if (isPlatformCommand(body.type)) return {platform:true,target:'ctox-platform'};
 const choices=[...new Set((manifest?.commands_out??[]).filter(x=>x.type===body.type).map(x=>x.app))];
 if (body.target_app!==undefined && !choices.includes(body.target_app)) return {error:'forbidden'};
 if (choices.length>1 && body.target_app===undefined) return {error:'forbidden'};
 return {platform:false,target:body.target_app??choices[0]??body.app_id};
}
export function commandRoute(body, manifestOf, user, testRoute=false) {
 const route=commandTarget(body,manifestOf(body.app_id));
 if(route.error || route.platform)return route;
 // An isolated fake Core without a user registry represents the admin bootstrap identity.
 user??={id:body.actor_id??'admin',role:'admin',permissions:[]};
 const target=manifestOf(route.target),cross=route.target!==body.app_id;
 if(cross && !(target?.commands_in??[]).some(x=>x.type===body.type && (x.from==='*'||(Array.isArray(x.from)&&(x.from.includes(body.app_id)||x.from.includes('*'))))))return {error:'forbidden'};
 if(!target?.handlers?.[body.type] && !(testRoute && !cross))return {error:cross?'forbidden':'unknown_command'};
 const rights=target?.command_rights??{};
 // Same precedence as native ManifestRules::authorize_command: exact, then longest prefix.
 const right=rights[body.type]??Object.entries(rights).filter(([k])=>k.endsWith('*')&&body.type.startsWith(k.slice(0,-1))).sort((a,b)=>b[0].length-a[0].length)[0]?.[1];
 if(user && right && !(Array.isArray(right.roles)&&right.roles.includes(user.role)) && !(right.assigned_users && user.assigned_apps?.includes(route.target)))return {error:'forbidden'};
 // The simulator's canonical collection write rules mirror its Core rights facade. No invented permission token.
 const writable = Object.values(target?.collections??{}).some(c=>String(c.rights?.write??'all').split('+').every(token=>token!=='core'&&(token==='all'||token==='*'||token==='own'||token===`app:${route.target}`||token===`role:${user?.role}`||(token==='admin'&&user?.role==='admin')||(user?.permissions??[]).includes(token))));
 if(cross && user && user.role!=='admin' && !right && !writable)return {error:'forbidden'};
 return route;
}
