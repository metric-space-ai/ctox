// Origin: CTOX
// License: AGPL-3.0-only
import assert from 'node:assert/strict';
import test from 'node:test';
import { requestDictation, DICTATION_METHOD, DICTATION_CAPABILITY } from './speech-dictation.mjs';
const commandId = '11111111-1111-4111-8111-111111111111';
const streamId = '22222222-2222-4222-8222-222222222222';
const request = { action:'speech.dictation', commandId, op:'open' };
const response = { ...request, streamId, state:'open', events:[], text:null, error:null };
test('standalone dictation uses trusted instance and bounded native channel, no meeting', async () => {
  const calls = []; let checked = 0;
  const sync = { requestNative: async (...args) => { calls.push(args); return response; } };
  assert.deepEqual(await requestDictation(sync, 'selected-instance', request, () => checked++), response);
  assert.equal(checked, 2);
  assert.deepEqual(calls, [[DICTATION_METHOD,
    { commandId, op:'open', scope:{instanceId:'selected-instance'} },
    { requiredCapability:DICTATION_CAPABILITY, timeoutMs:20000 }]]);
});
test('each operation retains correlation and never accepts renderer scope or text', async () => {
  for (const field of ['instanceId', 'scope', 'projectId', 'meetingId', 'deckRevision', 'text', 'receipt',
    'provider', 'model', 'capabilityToken', 'autoSend']) {
    await assert.rejects(requestDictation({requestNative:()=>assert.fail('must not send')},
      'selected', {...request,[field]:'forged'}, () => {}), /Invalid dictation/);
  }
  for (const extra of [
    {op:'open',streamId}, {op:'read',streamId,afterSequence:-1},
    {op:'write',streamId,sequence:0,pcmBase64:'AAA='},
    {op:'write',streamId,sequence:1,pcmBase64:'!invalid!'},
    {op:'cancel',streamId,sequence:1}, {op:'finish',streamId:'foreign-string'},
  ]) await assert.rejects(requestDictation({}, 'selected', {...request,...extra}, () => {}), /Invalid dictation/);
});
test('partial snapshot and full final are separate and aligned with the Composer contract', async () => {
  const input = {...request,op:'read',streamId,afterSequence:0};
  const partial = {...response,...input,events:[{sequence:100,text:'full partial'}]};
  delete partial.afterSequence;
  const sync = { requestNative:async()=>partial };
  assert.equal((await requestDictation(sync,'selected',input,()=>{})).events[0].text,'full partial');
  assert.equal((await requestDictation(sync,'selected',input,()=>{})).text,null);
  const final = {...partial,state:'finished',text:'complete final'};
  assert.equal((await requestDictation({requestNative:async()=>final},'selected',input,()=>{})).text,'complete final');
});
test('wrong command, stream, state and unexpected private fields cannot reach the draft', async () => {
  const input = {...request,op:'finish',streamId};
  const good = {...response,op:'finish',state:'finished',text:'draft'};
  for (const patch of [
    {commandId:streamId}, {streamId:commandId}, {op:'write'}, {state:'committed'},
    {receipt:{handle:'private'}}, {meetingRevision:1}, {key:'secret'}, {text:null},
    {state:'failed',text:'leaked final',error:'transport'}, {error:'private provider body'},
  ]) await assert.rejects(requestDictation({requestNative:async()=>({...good,...patch})},
    'selected',input,()=>{}), /mismatched dictation/);
});
test('configured backend error is a sanitized terminal response', async () => {
  const failed = {...response,state:'failed',error:'missing_credential'};
  assert.deepEqual(await requestDictation({requestNative:async()=>failed},'selected',request,()=>{}),failed);
  const canceled = {...response,state:'canceled',op:'cancel',events:[]};
  assert.deepEqual(await requestDictation({requestNative:async()=>canceled},'selected',
    {...request,op:'cancel',streamId},()=>{}),canceled);
});
test('authority change while a native call is pending rejects its late draft result', async () => {
  let current = true;
  await assert.rejects(requestDictation({requestNative:async()=>{current=false;return response;}},
    'selected',request,()=>{if(!current)throw new Error('retired');}),/retired/);
});
test('write, finish and cancellation have a five second call bound', async () => {
  const input = {...request,op:'write',streamId,sequence:1,pcmBase64:'AAAA'};
  let options;
  await requestDictation({requestNative:async(_method,_request,opts)=>{options=opts;return {...response,op:'write'};}},
    'selected',input,()=>{});
  assert.equal(options.timeoutMs,5000);
});
