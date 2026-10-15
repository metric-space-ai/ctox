import test from 'node:test';
import assert from 'node:assert/strict';
import {assertExactScope,assertRevocation} from './native.mjs';
import {assertInventory,p95} from '../k21/scenario.mjs';
import {assertPeerRejected} from '../k22/scenario.mjs';

// Acceptance-oracle invariants only. These do not stand in for a daemon run.
test('incident acceptance rejects leaked persisted state, incomplete inventory and unauthorized delivery',()=>{
  const clean=[{collection:'leads',id:'own',data:{id:'own',profile:{name:'public'}},base:{data:{id:'own'}}}];
  assertExactScope(clean,'leads',['own'],['profile.private.secret']);
  assert.throws(()=>assertExactScope([...clean,{collection:'leads@g2',id:'foreign',data:{id:'foreign'}}],'leads',['own'],[]),/foreign\/missing/);
  for(const location of ['data','base']) {
    const rows=structuredClone(clean);
    const body={id:'own',profile:{private:{secret:'never persist'}}};
    if(location==='base')rows[0].base.data=body;else rows[0].data=body;
    assert.throws(()=>assertExactScope(rows,'leads',['own'],['profile.private.secret']),/Masked field/);
  }
  const inventory=Array.from({length:46},(_,i)=>'real-app-'+i);
  assertInventory(inventory,inventory);
  assert.throws(()=>assertInventory(inventory,inventory.slice(1)),/missing app/);
  assert.throws(()=>p95([]),/Empty/);
  const rejected={receiverApplied:false,receiverRejected:true,reason:'old_rights_ver',packetSha256:'a'.repeat(64),transportKind:'native-webrtc'};
  assertPeerRejected(rejected);
  assert.throws(()=>assertPeerRejected({...rejected,receiverApplied:true}),/unauthorized peer/);
  assert.throws(()=>assertPeerRejected({...rejected,receiverRejected:false}),/not an authorization rejection/);
  const rights={seq:'9007199254740994',rights_ver:'2'};
  const after={frame:8,revokes:[{frame:8,rights_ver:'2',collections:['leads']}],pages:[]};
  const send={source:'native-final-send',rights_seq:rights.seq,commit_frame:'9007199254740994',
    frames:[{type:'scope_revoke',frame:'9007199254740995',rights_ver:'2',collections:['leads']}]};
  assertRevocation({frame:4},after,rights,'leads',send);
  assert.throws(()=>assertRevocation({frame:4},after,rights,'leads',{...send,frames:[
    {...send.frames[0],frame:'9007199254740996'}]}),/more than one native send frame/);
  assert.throws(()=>assertRevocation({frame:4},after,rights,'leads',undefined),/native-final-send/);
  assert.throws(()=>assertRevocation({frame:4},{...after,pages:[{frame:9,collection:'leads',
    docs:[{id:'leak',op:'put',seq:'9007199254740995'}]}]},rights,'leads',send),/Post-revocation/);
});
