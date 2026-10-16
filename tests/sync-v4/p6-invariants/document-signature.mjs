import {test} from 'node:test';
import assert from 'node:assert/strict';
import {generateKeyPairSync,sign,verify} from 'node:crypto';
import {createStubKernel} from '../../../src/apps/business-os/kernel/testkit.mjs';
import {WebRTCKernelTransport} from '../../../src/apps/business-os/kernel/transport-webrtc.mjs';

// Component proof at the actual transport -> kernel atomic-store boundary.
// The ephemeral key is real Ed25519. No predicate that accepts a magic signature.
// Native session/hello and browser IDB are deliberately not claimed by this test.
test('p6_invalid_signed_document_is_never_applied',async()=>{
 const {privateKey,publicKey}=generateKeyPairSync('ed25519');
 const bytes=message=>{const {sig,...body}=message;return Buffer.from(JSON.stringify(body));};
 const signed=body=>({...body,sig:sign(null,bytes(body),privateKey).toString('base64')});
 const k=createStubKernel({instanceId:'p6-signature',userId:'owner',storage:'memory',multiTab:false,scopeHash:'p6-rows'});
 const transport=new WebRTCKernelTransport({verifyPackage:async message=>verify(null,bytes(message),publicKey,Buffer.from(message.sig,'base64'))});
 transport.session={epoch:'1'};
 transport.scopes.set('rows',{scope_hash:'p6-rows',gen:1});
 transport.events={page:message=>k.receivePage(message)};
 const page=(id,n,seq)=>signed({type:'live',collection:'rows',scope_hash:'p6-rows',gen:1,epoch:'1',package_id:`p6-${id}`,seq_from:seq,seq_to:seq,cursor_to:seq,head_seq:seq,complete:true,core_sent_at_ms:Date.now(),docs:[{id,seq,rev:`${seq}-p6`,data:{id,n}}]});
 try {
  await k.ready;
  await k.sim.publishAppVersion('notes','1',{collections:{rows:{schema:{type:'object',properties:{id:{type:'string'},n:{type:'integer'}}}}}});
  const api=k.appApi('notes','1');
  await transport.apply(page('valid',7,'1'),3);
  assert.equal(api.get('rows','valid').get().n,7,'valid signed control did not reach the store');
  const forged=page('forged',8,'2');
  forged.docs[0].data.n=9;
  await assert.rejects(transport.apply(forged,3),/Page signature invalid/);
  const {openBackend}=await import('../../../src/apps/business-os/kernel/store-idb-v5.mjs');
  const db=await openBackend(k.dbName,'memory');
  assert.equal(await db.get('docs',['rows','forged']),undefined,'invalid signed document reached the atomic store');
  assert.equal(api.get('rows','valid').get().n,7);
 } finally {await k.close();}
});
