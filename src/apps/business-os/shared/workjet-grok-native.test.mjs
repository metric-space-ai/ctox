import test from 'node:test';
import assert from 'node:assert/strict';
import {requestWorkjetGrok,GROK_METHOD,GROK_CAPABILITY} from './workjet-grok-native.mjs';
const id='5fa0a379-6d66-4b73-8bea-cc6591b31542';
test('uses bounded admitted native channel and correlates reply',async()=>{
 let call; const request={version:1,action:'instance.grok.check',operationId:id,modelId:'grok-4.7'};
 const sync={requestNative:async(...args)=>{call=args;return {...args[1],installed:true,models:[]};}};
 const response=await requestWorkjetGrok(sync,request);
 assert.equal(response.operationId,id);assert.equal(call[0],GROK_METHOD);assert.equal(call[2].requiredCapability,GROK_CAPABILITY);assert.equal(call[2].timeoutMs,24000);
});
test('rejects credential input and wrong/retired correlation',async()=>{
 await assert.rejects(requestWorkjetGrok({}, {version:1,action:'instance.grok.read',operationId:id}));
 await assert.rejects(requestWorkjetGrok({requestNative:()=>{}},{version:1,action:'instance.grok.start',operationId:id,secret:'private'}));
 await assert.rejects(requestWorkjetGrok({requestNative:async()=>({version:1,action:'instance.grok.read',operationId:'other'})},{version:1,action:'instance.grok.read',operationId:id}));
});
test('requires exact version and operation ID without replacing invalid correlation',async()=>{
  let calls=0; const sync={requestNative:async()=>{calls++;}};
  for(const request of [{action:'instance.grok.read',operationId:id},{version:2,action:'instance.grok.read',operationId:id},{version:1,action:'instance.grok.read',operationId:'bad'}]) await assert.rejects(requestWorkjetGrok(sync,request));
  assert.equal(calls,0);
});
test('checks caller scope before/after RPC and sanitizes native failures',async()=>{
  const request={version:1,action:'instance.grok.read',operationId:id};
  let checks=0;
  await assert.rejects(requestWorkjetGrok({requestNative:async()=>request},request,()=>{if(++checks===2)throw new Error('retired');}),/retired/);
  assert.equal(checks,2);
  await assert.rejects(requestWorkjetGrok({requestNative:async()=>{throw new Error('private-provider-body');}},request),error=>!error.message.includes('private-provider-body'));
});
