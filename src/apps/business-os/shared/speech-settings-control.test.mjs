import assert from 'node:assert/strict';
import test from 'node:test';
import { requestSpeechSettings } from './speech-settings-control.mjs';
const commandId = '7ebd0f26-2c01-4d2d-bb43-426090630286';
test('secret input goes directly to the native auxiliary channel and is never echoed', async () => {
  const input = { action:'speech.settings.key', commandId, secret:'private-fixture' };
  let calls = 0;
  const result = await requestSpeechSettings({ async requestNative(method, request, options) {
    assert.equal(method,'ctox.workjet.speech.settings.v1');
    assert.equal(request,input);
    assert.deepEqual(options,{requiredCapability:'ctox-workjet-speech-settings-v1',timeoutMs:25000});
    calls++;
    return {action:request.action,commandId};
  } },input,()=>{});
  assert.equal(calls,1);
  assert.equal(JSON.stringify(result).includes('private-fixture'),false);
});
test('transcription probes keep scope/correlation and their bounded native deadline', async () => {
  const input={action:'speech.settings.check.transcription',commandId};
  const result=await requestSpeechSettings({async requestNative(method,request,options) {
    assert.equal(method,'ctox.workjet.speech.settings.v1');
    assert.equal(options.timeoutMs,30000);
    assert.equal(options.requiredCapability,'ctox-workjet-speech-settings-v1');
    return {...request,sttCheck:{state:'error',errorClass:'missing_credential'}};
  }},input,()=>{});
  assert.equal(result.sttCheck.state,'error');
});

test('stale scope and mismatched receipts are discarded', async () => {
  const request={action:'speech.settings.read',commandId};
  let current=true;
  await assert.rejects(requestSpeechSettings({async requestNative(){current=false;return request;}}, request,
    ()=>{if(!current)throw new Error('retired');}),/retired/);
  await assert.rejects(requestSpeechSettings({async requestNative(){return {...request,commandId:'other'};}},request,()=>{}),/another request/);
});
test('missing transport and provider errors cannot leak credential input', async () => {
  const input={action:'speech.settings.key',commandId,secret:'private-fixture'};
  await assert.rejects(requestSpeechSettings({},input,()=>{}),/connected CTOX instance/);
  await assert.rejects(requestSpeechSettings({async requestNative(){throw new Error('private-fixture provider body');}},input,()=>{}),
    error=>error.message.includes('unavailable')&&!error.message.includes('private-fixture'));
});
