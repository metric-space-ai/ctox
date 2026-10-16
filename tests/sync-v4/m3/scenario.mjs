import assert from 'node:assert/strict';
export default {
 id:'M3',title:'Real browser read/write receipt and durable reload',
 criterion:{launchContext:200,collectionRead:true,appGrant:true,commitReceipt:true,reloadPersistence:true},
 async run(h) {
  const page=await h.page();await h.os(page);
  const value={marker:'p6-real-browser-'+Date.now()};
  const result=await page.evaluate(async value=>{
   const {kernel,transport}=globalThis.__e2eOS;
   const apps=kernel.shellApi().apps.get();
   if(!Array.isArray(apps)||!apps.length)throw Error('No native collection rows reached browser');
   const api=kernel.appApi('shell',globalThis.__p6Boot.loader.shellVersion);
   const grant=await transport.grantApp('shell');
   const write=api.prefs.set('p6.real-path',value);await write.local;
   const receipt=await write.result;
   if(receipt.status!=='accepted')throw Error('Browser write rejected: '+receipt.reason);
   const docId='shell/'+kernel.userId+'/p6.real-path';
   const stored=await globalThis.__p6NativeControl({operation:'readNative',args:{collection:'app_user_preferences',id:docId}});
   if(stored.rows.length!==1||stored.rows[0].value.marker!==value.marker)throw Error('E14 receipt preceded native persistence');
   return {rows:apps.length,appGrant:!!grant,receipt,readback:true};
  },value);
  await page.reload({waitUntil:'domcontentloaded',timeout:20000});await h.os(page);
  await page.waitForFunction(marker=>{
   const api=globalThis.__e2eOS.kernel.appApi('shell',globalThis.__p6Boot.loader.shellVersion);
   return api.prefs.get('p6.real-path')?.marker===marker;
  },value.marker,{timeout:15000});
  assert.equal(result.readback,true);
  return {status:'measured',pass:true,acceptanceComplete:true,measured:{...result,reloadPersistence:true}};
 }
};
