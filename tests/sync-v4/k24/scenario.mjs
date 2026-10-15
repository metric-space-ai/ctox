import {randomUUID} from 'node:crypto';
import {unknownApp} from './fixture.mjs';
export default {
 id:'K24',title:'An unknown app hot-plugs through the real OS publication contract',
 coverage:'Source publication, handler, every-trigger and incompatible migration; non-admin/Workjet and archive require the final OS integration adapter.',
 steps:['Open unchanged native daemon and real browser','Publish an unknown app via source.draft API','Verify activation across clients within two seconds','Run native handler and one-minute trigger','Publish incompatible schema migration and verify remount retains input'],
 criterion:{activationMs:2000,restarts:0,reloads:0,handlerAndTrigger:true,inputLost:false},
 async run(h){
  const p=await h.page();await h.warm(p);const fixture=unknownApp('e2e-'+randomUUID());
  const availability=await h.os(p);
  if(!availability.available)return {measured:{availability,appId:fixture.id,published:false},status:'not_run',pass:false,acceptanceComplete:false,
   reason:'No native-backed V4 source bootstrap supplied; old V3 cannot run the required OS contract. No fake-core fallback.'};
  const peers=[p,await h.newClient(),await h.newClient()];
  for(const peer of peers.slice(1)){const os=await h.os(peer);if(!os.available)throw Error('One client lacks real OS adapter');}
  const started=Date.now();
  const published=await p.evaluate(async f=>{
   const source=globalThis.__e2eOS.shell.source(f.id);
   for(const [path,text]of Object.entries(f.files))await source.draft.write(path,text);
   await source.draft.setManifest(f.manifest);await source.draft.setCollections({collections:{[f.collection]:f.schema}});
   const h=source.draft.publish({base_app_ver:null});await h.local;return h.result;
  },fixture);
  if(!['accepted','migrated'].includes(published.status))return {measured:{published},pass:false,acceptanceComplete:false};
  const activation=[];
  for(const peer of peers){await peer.waitForFunction(id=>globalThis.__e2eOS.shell.activation.get(id).get()?.state==='activated',fixture.id,{timeout:10000});activation.push({elapsedMs:Date.now()-started});}
  await p.evaluate(async id=>{await globalThis.__e2eOS.openApp(id);},fixture.id);
  await p.locator('[data-k24="input"]').fill('retained draft');await p.locator('[data-k24="command"]').click();
  const observed=[];
  for(const peer of peers){await peer.waitForFunction(({id,collection,ver})=>{const api=globalThis.__e2eOS.kernel.appApi(id,ver);return api.get(collection,'handler-row').get()?.kind==='handler';},{id:fixture.id,collection:fixture.collection,ver:published.rev},{timeout:10000});observed.push('handler');}
  await p.waitForFunction(({id,collection,ver})=>globalThis.__e2eOS.kernel.appApi(id,ver).get(collection,'tick-row').get()?.kind==='trigger',{id:fixture.id,collection:fixture.collection,ver:published.rev},{timeout:90000});
  const migration=await p.evaluate(async f=>{
   const src=globalThis.__e2eOS.shell.source(f.id),next={...f.schema,version:1,properties:{...f.schema.properties,label:{type:'string'}},required:['id','label']};delete next.properties.title;next.indexes=['label'];
   await src.draft.setCollections({collections:{[f.collection]:next},migrations:{[f.collection]:{from_ver:f.ver,steps:[{op:'rename',from:'title',to:'label'}]}}});
   const h=src.draft.publish({base_app_ver:f.ver});await h.local;return h.result;
  },{...fixture,ver:published.rev});
  await p.waitForFunction(({id,ver})=>globalThis.__e2eOS.shell.activation.get(id).get()?.app_ver===ver,{id:fixture.id,ver:migration.rev},{timeout:10000});
  const retained=await p.locator('[data-k24="input"]').inputValue();
  return {measured:{appId:fixture.id,published,activation,handlerClients:observed.length,trigger:true,migration,retained,
    missingCoverage:['non-admin-identity','Workjet-Ops-client','archive/removal-OS-command','native-RO-handler-truth']},
   pass:activation.every(m=>m.elapsedMs<=2000)&&retained==='retained draft'&&['accepted','migrated'].includes(migration.status),acceptanceComplete:false};
 }
};
