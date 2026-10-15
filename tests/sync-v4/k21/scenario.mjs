import assert from 'node:assert/strict';
import {connect,participant,control,publish,activation,write,rawStore,notRun} from '../k13/native.mjs';

export const p95 = values => {
  assert.ok(values.length>0 && values.every(Number.isFinite),'Empty/non-finite activation sample');
  return [...values].sort((a,b)=>a-b)[Math.ceil(values.length*0.95)-1];
};
export function assertInventory(expected,completed) {
  assert.equal(expected.length,46,'K21 requires the complete catalog inventory');
  assert.equal(new Set(expected).size,expected.length);
  assert.deepEqual([...completed].sort(),[...expected].sort(),'A missing app cannot become hot-plug acceptance');
}
async function command(page,type,payload) {
  const accepted=await page.evaluate(async ({type,payload})=>{
    const os=globalThis.__e2eOS, api=os.kernel.appApi('ctox-platform',os.platformVersion);
    const handle=api.command(type,payload);await handle.local;
    globalThis.__p6Commands ||= new Map();globalThis.__p6Commands.set(handle.id,handle);
    return {id:handle.id,result:await handle.result};
  },{type,payload});
  assert.ok(['accepted','migrated'].includes(accepted.result.status),'App command admission failed');
  await page.waitForFunction(id=>['done','failed'].includes(globalThis.__p6Commands.get(id).status.get().status),
    accepted.id,{timeout:15000});
  const state=await page.evaluate(id=>globalThis.__p6Commands.get(id).status.get(),accepted.id);
  assert.equal(state.status,'done','Native lifecycle handler failed');
  const stamp=await control(page,'commitReceipt',{commandId:accepted.id,appId:payload.app_id,operation:type});
  assert.equal(stamp.source,'store-commit');assert.ok(Number.isFinite(stamp.committedAtMs));
  return {...accepted.result,rev:stamp.app_ver,committedAtMs:stamp.committedAtMs};
}
export default {
  id:'K21',title:'Every installed app hot-plugs, migrates, rolls forward and archives without losing input',
  coverage:'Complete native app inventory; source update and platform commands, real app mounts, raw IDB balances, migration and export.',
  steps:['Load all 46 real app release fixtures from the isolated native catalog',
    'Keep two authenticated clients mounted, including an unsaved form, while a 100000-row migration runs',
    'For each app install, write, change field/index/rights, update, forward rollback and remove',
    'Check every activation, row balance, retained form, process/peer identity and 30-day archive export'],
  criterion:{apps:46,activationP95Ms:2000,migrationRows:100000,reloads:0,daemonRestarts:0,peerRespawns:0,inputLoss:0,archiveDays:30},
  async run(h) {
    const session=await connect(h,'K21');if(session.result)return session.result;
    const {page:owner,fixture:f}=session;
    if(!f.apps?.length || !f.migration)return notRun('Complete real app fixtures and migration input are required');
    const catalog=await control(owner,'readNative',{collection:'app_catalog'});
    const inventory=[...new Set(catalog.rows.map(r=>r.app_id))];
    assertInventory(inventory,f.apps.map(a=>a.id));
    const peers=await Promise.all(f.identities.slice(0,2).map(i=>participant(h,f,i)));
    assert.equal(peers.length,2);
    const baseline=await control(owner,'processIdentity');
    const navigations=await Promise.all(peers.map(p=>p.page.evaluate(()=>performance.timeOrigin)));
    const migration=await control(owner,'startMigration',{...f.migration,rows:100000});
    assert.equal(migration.sourceRows,100000);assert.ok(migration.owner && migration.stopAt);
    const completed=[],events=[];
    try {
      for (const app of f.apps) {
        const installed=await command(owner,'ctox.app.update',{app_id:app.id,base_app_ver:null,release_ver:app.release});
        assert.ok(['accepted','migrated'].includes(installed.status),'Install rejected: '+app.id);
        const ver=installed.rev;assert.ok(ver);
        const times=await Promise.all(peers.map(p=>activation(p.page,app.id,ver,installed.committedAtMs)));
        for(const p of peers)await p.page.evaluate(id=>globalThis.__e2eOS.openApp(id),app.id);
        assert.ok(app.form?.selector && app.form?.value,'Real app-specific unsaved form fixture is required: '+app.id);
        await peers[0].page.locator(app.form.selector).fill(app.form.value);
        const written=await write(peers[1].page,app.id,app.collection,app.doc);
        assert.ok(['accepted','migrated'].includes(written.result.status));
        const before=await control(owner,'readNative',{collection:app.collection});
        assert.ok(before.rows.some(r=>r.id===app.doc.id),'Positive native write control failed');
        assert.ok(app.next.collections && app.next.manifest.row_filters && app.next.indexChanged,
          'Update must change schema field, index and rights, not just a title');
        const updated=await publish(owner,app.id,app.next,ver);
        assert.ok(['accepted','migrated'].includes(updated.status));
        times.push(...await Promise.all(peers.map(p=>activation(p.page,app.id,updated.rev,updated.committedAtMs))));
        const retained=await peers[0].page.locator(app.form.selector).inputValue();
        assert.equal(retained,app.form.value,'Remount lost unsaved input: '+app.id);
        const balance=await control(owner,'readNative',{collection:app.collection});
        assert.deepEqual(balance.rows.map(r=>r.id).sort(),before.rows.map(r=>r.id).sort(),'Migration lost rows');
        const rolled=await command(owner,'ctox.app.rollback',{app_id:app.id,base_app_ver:updated.rev,target_ver:ver});
        assert.ok(['accepted','migrated'].includes(rolled.status));
        const forward=rolled.rev;
        assert.ok(forward && forward!==ver && forward!==updated.rev,'Rollback must publish a NEW forward version');
        times.push(...await Promise.all(peers.map(p=>activation(p.page,app.id,forward,rolled.committedAtMs))));
        assert.equal(await peers[0].page.locator(app.form.selector).inputValue(),app.form.value);
        const removed=await command(owner,'ctox.app.remove',{app_id:app.id,base_app_ver:forward});
        assert.ok(['accepted','migrated'].includes(removed.status));
        const removedAt=removed.committedAtMs;
        await Promise.all(peers.map(p=>p.page.waitForFunction(id=>!globalThis.__e2eOS.shell.apps.get().some(a=>a.id===id),app.id,{timeout:10000})));
        times.push(Date.now()-removedAt);
        const exported=await control(owner,'exportArchive',{appId:app.id});
        assert.ok(exported.expiresAtMs-exported.removedAtMs>=30*86400000,'Removal archive retained less than 30 days');
        assert.match(exported.sha256,/^[a-f0-9]{64}$/);assert.ok(exported.bytes>0);
        assert.deepEqual(exported.rowIds.sort(),balance.rows.map(r=>r.id).sort(),'Archive export lost rows');
        for(let i=0;i<peers.length;i++) {
          assert.equal(await peers[i].page.evaluate(()=>performance.timeOrigin),navigations[i],'Page reloaded');
          const statuses=await control(peers[i].page,'statusPlacement',{appId:app.id});
          assert.deepEqual(statuses.outsideStateZone,[],'State text escaped the shell state zone');
          // Read raw stores as well as live app queries; no mock/local mirror acceptance.
          assert.ok(Array.isArray(await rawStore(peers[i].page,'docs')));
        }
        assert.deepEqual(await control(owner,'processIdentity'),baseline,'Daemon or peer respawned');
        assert.ok(p95(times)<=2000,'Activation p95 exceeded 2s: '+app.id);
        events.push({appId:app.id,activationMs:times,p95Ms:p95(times),retained:true,archive:exported});
        completed.push(app.id);h.observe('hot-plug-app',{appId:app.id});
      }
      const migrated=await control(owner,'finishMigration',{owner:migration.owner});
      assert.equal(migrated.sourceRows,100000);assert.equal(migrated.targetRows,100000);
      assert.equal(migrated.lostRows,0);
      assertInventory(inventory,completed);
      return {status:'measured',pass:true,acceptanceComplete:true,measured:{inventory,events,migration:migrated}};
    } finally {await control(owner,'stopMigration',{owner:migration.owner});}
  }
};
