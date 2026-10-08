import {test} from "node:test";
import assert from "node:assert/strict";
import {requestJourFixeSpeech} from "./jour-fixe-speech.mjs";
const request={action:"project.jour_fixe.speech",projectId:"project",meetingId:"meeting",deckRevision:1,op:"open",requestId:"11111111-1111-4111-8111-111111111111"};
test("speech uses only the trusted selected instance and existing authorized WebRTC path",async()=>{
 const calls=[];const sync={requestNative:async(...args)=>{calls.push(args);return{streamId:"stream",state:"open",events:[],receipt:null,error:null,requestId:request.requestId}}};
 const value=await requestJourFixeSpeech(sync,"native-instance",request);
 assert.equal(calls[0][0],"ctox.workjet.jour_fixe.speech.v1");
 assert.equal(calls[0][1].scope.instanceId,"native-instance");
 assert.equal(calls[0][2].requiredCapability,"ctox-workjet-jour-fixe-speech-v1");
 assert.equal(value.projectId,"project");
 assert.equal(value.op,"open");
});
test("renderer cannot pass an instance override, transcript, provider, or final receipt",async()=>{
 for(const field of ["instanceId","scope","text","model","receipt","speaker","capabilityToken"])
   await assert.rejects(requestJourFixeSpeech({requestNative:()=>assert.fail("must not send")},"native-instance",{...request,[field]:"forged"}),/Invalid speech/);
});
