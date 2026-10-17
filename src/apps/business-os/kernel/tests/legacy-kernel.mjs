// Test-only migration bridge preserving K-1 specialized outcome assertions.
// The product kernel has no mount API or module registry (§16.19).
export * from '../kernel.mjs';
import {createStubKernel as real} from '../testkit.mjs';
import {createAppHost} from '../testkit/app-host.mjs';
export function createStubKernel(options={}){
 const kernel=real(options),host=createAppHost(kernel,{mountTimeoutMs:options.remountTimeoutMs??9000});
 const shell={...kernel.shell,mount:(...args)=>host.mount(...args)};
 const sim=new Proxy(kernel.sim,{get(target,key){
  if(key==='registerModule')return (...args)=>host.registerModule(...args);
  if(key==='publishAppVersion')return async(app,ver,{module,...rest}={})=>{if(module)host.registerModule(app,ver,module);const result=await target.publishAppVersion(app,ver,rest);await host.idle();return result;};
  if(key==='windowState')return async()=>{const rows=await target.windowStateRows();return rows.map(row=>({...row,...row.value,key:row.key.endsWith(':shell')?'shell':row.key,snapshot:row.value?.snapshot??row.value}));};
  return Reflect.get(target,key);
 }});
 return new Proxy(kernel,{get(target,key){
  if(key==='shell')return shell;if(key==='shellApi')return()=>shell;if(key==='sim')return sim;
  if(key==='close')return async()=>{const cleanup=host.dispose();await target.close();await cleanup;};
  if(key==='logout')return async options=>{const cleanup=host.dispose();await target.logout(options);await cleanup;};
  return Reflect.get(target,key);
 }});
}
