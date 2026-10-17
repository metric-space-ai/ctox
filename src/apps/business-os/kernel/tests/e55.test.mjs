import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStubKernel} from '../testkit.mjs';
import {decodeAppGrant,validAppGrantClaims} from '../app-grants.mjs';

test('E55 fake native contract acquires one signed grant per activated app',async t=>{
 const k=createStubKernel({instanceId:`e55-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:0});t.after(()=>k.close());await k.ready;
 let collections=0;
 for(let a=0;a<41;a++){
  const own=Object.fromEntries(Array.from({length:a<31?6:5},(_,i)=>[`app${a}_c${i}`,{schema:{type:'object',properties:{id:{type:'string'}}}}]));collections+=Object.keys(own).length;
  const state=await k.sim.publishAppVersion(`app${a}`,'1',{collections:own,files:{'index.js':'export function mount(){}'}});assert.equal(state.state,'activated',JSON.stringify(state));
 }
 assert.equal(collections,236);
 const messages=k.sim.scopeGrants(),requests=messages.filter(m=>m.type==='scope.grant.request'),tokens=messages.filter(m=>m.type==='scope.grant').map(m=>m.token);
 assert.equal(requests.length,41);assert.equal(tokens.length,41);
 for(let i=0;i<tokens.length;i++){const decoded=decodeAppGrant(tokens[i]);assert.ok(validAppGrantClaims(decoded?.claims,{instanceId:k.identity.instance_id,userId:'alice',deviceId:k.identity.device_id,appId:`app${i}`}));assert.equal(decoded.claims.collections.length,i<31?6:5);}
});
test('E55 activation without a grant surfaces Collection grant missing',async t=>{
 const k=createStubKernel({instanceId:`e55-denied-${process.pid}`,userId:'alice',storage:'memory',multiTab:false,latencyMs:0});t.after(()=>k.close());await k.ready;
 k.sim.failAppGrant('denied');const denied=await k.sim.publishAppVersion('denied','1',{files:{'index.js':'export function mount(){}'}});
 assert.equal(denied.state,'failed');assert.equal(denied.error,'Collection grant missing');assert.ok(k.statusZone.get().some(s=>s.text==='Collection grant missing'));
});
