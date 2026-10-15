import assert from 'node:assert/strict';
import {connect,participant,control,rawStore,assertExactScope,write,trace,assertRevocation,until,notRun} from '../k13/native.mjs';

export function assertPeerRejected(result) {
  assert.equal(result.receiverApplied,false,'An unauthorized peer packet was applied');
  assert.equal(result.receiverRejected,true,'A dropped or unsent packet is not an authorization rejection');
  assert.ok(['old_rights_ver','invalid_signature','revoked'].includes(result.reason));
  assert.match(result.packetSha256,/^[a-f0-9]{64}$/);
  assert.equal(result.transportKind,'native-webrtc');
}
export default {
  id:'K22',title:'Concurrent authenticated roles retain exact scopes and reject stale or forged peer packets',
  coverage:'Ten minutes; three users/two roles plus Workjet, mobile and native participants, conflict merge and raw IDB inspection.',
  steps:['Connect distinct authenticated participants to the isolated WELSCH-shaped fixture',
    'Make concurrent edits from the same base to disjoint and colliding fields',
    'Write for ten minutes, forward a genuine peer packet, then try stale-version and forged-signature packets',
    'Revoke one user under load and inspect exact IDB scopes, final send frames and every surviving write'],
  criterion:{durationMs:600000,foreignRows:0,maskedFields:0,parallelRightsFailures:0,revokeFrames:1,invalidPeerApplied:0},
  async run(h) {
    const session=await connect(h,'K22');if(session.result)return session.result;
    const {page:owner,fixture:f}=session;
    assert.equal(f.identities.length,3);assert.equal(new Set(f.identities.map(i=>i.id)).size,3);
    assert.equal(new Set(f.identities.map(i=>i.role)).size,2);
    if(!f.externalParticipants || !['workjet','mobile','native'].every(kind=>f.externalParticipants.some(p=>p.kind===kind)))
      return notRun('Real Workjet, mobile and native participant routes are required; browser-only is not K22');
    const peers=await Promise.all(f.identities.map(i=>participant(h,f,i)));
    const first=peers[0],second=peers[1];
    assert.equal(first.identity.role,second.identity.role);
    const equal=await control(owner,'participantScopes',{devices:[first.identity.deviceId,second.identity.deviceId]});
    assert.equal(equal[0].scope_hash,equal[1].scope_hash,'Valid forwarding positive control needs equivalent projections');
    assert.notEqual(equal[0].device_id,equal[1].device_id);
    const conflict=await control(owner,'sameBaseWrites',{collection:f.collection,id:f.conflict.id,
      participants:[first.identity.deviceId,second.identity.deviceId],
      writes:[f.conflict.first,f.conflict.second]});
    assert.equal(conflict.transportKind,'native-webrtc');
    assert.ok(conflict.baseRev && conflict.opIds?.length===2);
    const merged=(await control(owner,'readNative',{collection:f.collection,id:f.conflict.id})).rows;
    assert.equal(merged.length,1);
    for(const [field,value] of Object.entries(f.conflict.expectedMerged))assert.deepEqual(merged[0][field],value,'Disjoint field lost or server collision rule changed');
    const loser=await rawStore(second.page,'archive_local');
    assert.ok(loser.some(r=>r.op_id===conflict.loserOpId),'Collision is not visible in the losing client archive');
    const external=await control(owner,'startParticipants',{participants:f.externalParticipants,stopAfterMs:610000,intervalMs:50});
    assert.ok(external.owner && external.stopAt);
    const stats=peers.map(p=>({id:p.identity.id,accepted:0,rejected:[],errors:[]}));
    const start=Date.now();let stopped=false,revoked=false,revocation,wire;
    const producers=peers.map((p,index)=>(async()=>{
      let n=0;
      while(!stopped) {
        try {
          const change={id:f.writeDocs[p.identity.id],body:{[f.writeFields[p.identity.id]]:p.identity.id+'-'+n++}};
          const result=(await write(p.page,f.appId,f.collection,change,true)).result;
          if(['accepted','migrated'].includes(result.status))stats[index].accepted++;
          else stats[index].rejected.push({at:Date.now(),reason:result.reason,expected:revoked && p.identity.id===f.revokedUser});
        } catch(error) {stats[index].errors.push(error.message);}
        await h.pause(50);
      }
    })());
    let forwarding,invalid;
    try {
      const remaining=async target=>{const wait=start+target-Date.now();if(wait>0)await h.pause(wait);};
      await remaining(300000);
      forwarding=await control(owner,'forwardPeer',{from:first.identity.deviceId,to:second.identity.deviceId,variant:'genuine',pauseNativeDelivery:true});
      assert.equal(forwarding.receiverApplied,true);assert.equal(forwarding.transportKind,'native-webrtc');
      assert.match(forwarding.packetSha256,/^[a-f0-9]{64}$/);
      const local=(await rawStore(second.page,'docs')).find(r=>r.id===forwarding.documentId);
      assert.ok(local && local.data?.[forwarding.probeField]===forwarding.probeValue,'Peer positive control not persisted');
      const target=peers.find(p=>p.identity.id===f.revokedUser),before=await trace(target.page);
      revocation=await control(owner,'setRights',f.revoke);revoked=true;
      await until(h,async()=>!(await rawStore(target.page,'docs')).some(r=>r.collection===f.collection),5000,'Revoked raw IDB cleanup');
      wire=await trace(target.page);
      const sendTrace=await control(owner,'sendTrace',{deviceId:target.identity.deviceId,rightsSeq:revocation.seq});
      assertRevocation(before,wire,revocation,f.collection,sendTrace);
      const destination=await rawStore(second.page,'docs');
      invalid=[];
      for(const variant of ['old_rights_ver','forged_signature']) {
        const packet=await control(owner,'forwardPeer',{from:first.identity.deviceId,to:second.identity.deviceId,variant,pauseNativeDelivery:true});
        assertPeerRejected(packet);invalid.push(packet);
      }
      assert.deepEqual(await rawStore(second.page,'docs'),destination,'Rejected packet changed persisted IDB');
      await remaining(600000);
    } finally {
      stopped=true;await Promise.all(producers);
      const terminal=await control(owner,'stopParticipants',{owner:external.owner});
      assert.ok(terminal.participants.every(p=>p.accepted>0 && p.nativeConfirmed===true),'External participant did not perform real native-confirmed writes');
      assert.deepEqual(terminal.unexpectedRightsFailures,[]);
    }
    const durationMs=Date.now()-start;assert.ok(durationMs>=600000);
    for(let i=0;i<peers.length;i++) {
      const p=peers[i];assert.ok(stats[i].accepted>0,'No positive parallel write control');
      assert.deepEqual(stats[i].errors,[]);
      assert.deepEqual(stats[i].rejected.filter(r=>!r.expected),[],'Parallel live authority check failed');
      assertExactScope(await rawStore(p.page,'docs'),f.collection,
        p.identity.id===f.revokedUser?[]:f.expectedIds[p.identity.id],f.maskedFields[p.identity.id]??[]);
    }
    return {status:'measured',pass:true,acceptanceComplete:true,
      measured:{durationMs,stats,forwarding,invalid,revocation,wire}};
  }
};
