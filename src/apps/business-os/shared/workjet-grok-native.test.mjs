import test from 'node:test';
import assert from 'node:assert/strict';
import {requestWorkjetGrok,GROK_METHOD,GROK_CAPABILITY} from './workjet-grok-native.mjs';
const id='5fa0a379-6d66-4b73-8bea-cc6591b31542';
test('uses bounded admitted native channel and correlates reply',async()=>{
 let call; const request={action:'instance.grok.check',operationId:id,modelId:'authenticated-fixture'};
 const sync={requestNative:async(...args)=>{call=args;return {...args[1],installed:true,models:[]};}};
 const response=await requestWorkjetGrok(sync,request);
 assert.equal(response.operationId,id);assert.equal(call[0],GROK_METHOD);assert.equal(call[2].requiredCapability,GROK_CAPABILITY);assert.equal(call[2].timeoutMs,24000);
});
test('rejects credential input and wrong/retired correlation',async()=>{
 await assert.rejects(requestWorkjetGrok({}, {action:'instance.grok.read',operationId:id}));
 await assert.rejects(requestWorkjetGrok({requestNative:()=>{}},{action:'instance.grok.start',operationId:id,secret:'private'}));
 await assert.rejects(requestWorkjetGrok({requestNative:async()=>({version:1,action:'instance.grok.read',operationId:'other'})},{action:'instance.grok.read',operationId:id}));
});
