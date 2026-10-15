import assert from 'node:assert/strict';
import {connect,participant,control,publish,activation,rawStore,assertExactScope,
  collectionRecords,queueOffline,pendingResult,trace,assertRevocation,until} from './native.mjs';

export default {
  id:'K13',title:'Manifest and rights changes reach non-admin clients without stale catalog authority',
  coverage:'Real source publication, three server-authenticated roles, raw IDB scope, signed revoke, artifacts and pending outbox.',
  steps:['Authenticate admin, chef and user in separate browser contexts',
    'Publish the copied Outbound app with changed icon, collection and foreign collection declarations',
    'Expand user rights and observe new rows, then queue an offline write',
    'Revoke under native writer load, reconnect and inspect frame, IDB, artifacts and rejected write'],
  criterion:{activationMs:2000,expansionMs:2000,revokeFrames:1,localRemovalMs:5000,icon404:0},
  async run(h) {
    const session=await connect(h,'K13');if(session.result)return session.result;
    const {page:owner,fixture:f}=session;
    assert.equal(f.identities.length,3);
    assert.deepEqual(f.identities.map(i=>i.role).sort(),['admin','chef','user']);
    assert.ok(f.app?.baseVersion && f.app?.next?.manifest?.foreign_collections,'Use a real copied app release, not an unknown empty app');
    assert.notDeepEqual(f.app.next.manifest.layout,f.app.previous.manifest.layout,'Icon/layout must change');
    const participants=await Promise.all(f.identities.map(i=>participant(h,f,i)));
    const user=participants.find(p=>p.identity.role==='user');
    const published=await publish(owner,f.app.id,f.app.next,f.app.baseVersion);
    assert.ok(['accepted','migrated'].includes(published.status));
    const activationStart=Date.now(), openings=[];
    for (const p of participants) {
      await activation(p.page,f.app.id,published.rev);
      const opened=await p.page.evaluate(async ({appId,version})=>{
        const os=globalThis.__e2eOS;
        await os.openApp(appId);
        const manifest=await os.shell.artifacts.manifest(appId,version);
        const path=manifest.layout.icon_asset;
        if(!path)throw Error('Published app has no icon asset');
        const hash=manifest.files[path];
        const blob=await os.shell.artifacts.read(hash);
        const api=os.kernel.appApi(appId,version);
        const errors=os.shell.status.forApp(appId).get().filter(s=>/forbidden|access|missing/i.test(s.code));
        return {role:api.actor.role,iconBytes:blob.size,hash,errors};
      },{appId:f.app.id,version:published.rev});
      assert.ok(opened.iconBytes>0);assert.deepEqual(opened.errors,[]);
      openings.push({...opened,elapsedMs:Date.now()-activationStart});
      assertExactScope(await rawStore(p.page,'docs'),f.collection,f.beforeIds[p.identity.id],f.maskedFields[p.identity.id]??[]);
    }
    assert.ok(openings.every(o=>o.elapsedMs<=2000),'Authorized app/data/icon activation exceeded 2s');
    const expandedAt=Date.now(), expanded=await control(owner,'setRights',f.expand);
    assert.match(String(expanded.seq),/^[0-9]+$/);
    const expansionMs=await until(h,async()=>{
      const ids=collectionRecords(await rawStore(user.page,'docs'),f.collection).filter(r=>!r.deleted).map(r=>r.id).sort();
      return JSON.stringify(ids)===JSON.stringify([...f.expandedIds].sort());
    },2000,'Expanded rights delivery');
    assert.ok(Date.now()-expandedAt<=2000);
    assertExactScope(await rawStore(user.page,'docs'),f.newCollection,f.newCollectionIds,[]);
    await h.offline(user.page,true);
    const pending=await queueOffline(user.page,f.app.id,f.collection,f.pendingDoc);
    assert.ok((await rawStore(user.page,'outbox')).some(e=>e.op_id===pending),'No real pending write was queued');
    let pressure;
    try {
      pressure=await control(owner,'startWrites',{collection:f.collection,intervalMs:25});
      assert.ok(pressure.owner && pressure.stopAt,'Native load must have a bounded owner');
      const before=await trace(user.page);
      const revoked=await control(owner,'setRights',f.revoke);
      await h.offline(user.page,false);
      const removalMs=await until(h,async()=>collectionRecords(await rawStore(user.page,'docs'),f.collection).length===0,5000,'Revoked IDB cleanup');
      const rejection=await pendingResult(user.page,pending);
      assert.equal(rejection.status,'rejected');assert.equal(rejection.reason,'revoked');
      const archive=await rawStore(user.page,'archive_local');
      assert.ok(archive.some(e=>e.op_id===pending && e.reason==='revoked'),'Rejected pending input was not archived as revoked');
      const artifacts=await rawStore(user.page,'artifacts');
      assert.ok(f.revokedArtifacts.length>0);
      assert.ok(f.revokedArtifacts.every(hash=>!artifacts.some(a=>a.hash===hash)),'Revoked artifact remains locally accessible');
      const after=await trace(user.page);
      const sendTrace=await control(owner,'sendTrace',{deviceId:user.identity.deviceId,rightsSeq:revoked.seq});
      assertRevocation(before,after,revoked,f.collection,sendTrace);
      return {status:'measured',pass:true,acceptanceComplete:true,
        measured:{openings,expansionMs,removalMs,rights:revoked,rejection,wire:after,nonAdmin:true}};
    } finally { if(pressure)await control(owner,'stopWrites',{owner:pressure.owner}); }
  }
};
