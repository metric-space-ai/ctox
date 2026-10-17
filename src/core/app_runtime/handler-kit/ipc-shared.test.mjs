import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createContext,runHandler} from './index.mjs';
const cas=files=>{const hashes={},artifacts={};for(const[p,s]of Object.entries(files)){const h=createHash('sha256').update(s).digest('hex');hashes[p]=h;artifacts[h]=s;}return{hashes,artifacts};};
test('app IPC needs both active declarations, target user right, and own callback',async()=>{
  const sender={app_id:'sender',handlers:{done:'handlers/done.js'},commands_out:[{app:'receiver',type:'run'}]};
  const receiver={app_id:'receiver',handlers:{run:'handlers/run.js'},commands_in:[{type:'run',from:['sender']}],command_rights:{run:{roles:['writer']}}};
  const fixtures={actor:{id:'u',role:'writer'},apps:{receiver},command:(_t,_p,o)=>({id:o.id,status:'accepted'})};
  assert.equal((await createContext(sender,fixtures).command('run',{}, {id:'c',on_complete:'done',on_failed:'done'})).id,'c');
  for(const mutation of ['sender','receiver','actor','callback']){
    const a=structuredClone(sender),b=structuredClone(receiver),f={...fixtures,apps:{receiver:b}};
    if(mutation==='sender')a.commands_out=[];
    if(mutation==='receiver')b.commands_in=[];
    if(mutation==='actor')f.actor={id:'u',role:'viewer'};
    await assert.rejects(createContext(a,f).command('run',{}, {on_complete:mutation==='callback'?'foreign.done':'done'}),{code:'forbidden'});
  }
});
test('shared imports pin shell CAS, reject unexported modules, and relink on activation',async()=>{
  const app=cas({'handlers/run.js':"import {value} from 'ctox:shared/pure.js'; export default()=>value;"});
  const manifest={app_id:'app',handlers:{run:'handlers/run.js'},files:app.hashes};
  const shell=v=>{const c=cas({'shared/pure.js':`export const value=${v};`,'shared/private.js':'export const value=999;'});return {manifest:{app_ver:String(v),files:c.hashes,shared_exports:['shared/pure.js']},artifacts:c.artifacts};};
  const fixtures={artifacts:app.artifacts,shell:shell(1)};
  assert.equal(await runHandler(manifest,'handlers/run.js',{}, {fixtures}),1);
  fixtures.shell=shell(2);
  assert.equal(await runHandler(manifest,'handlers/run.js',{}, {fixtures}),2);
  fixtures.shell.manifest.shared_exports=[];
  await assert.rejects(runHandler(manifest,'handlers/run.js',{}, {fixtures}),{code:'forbidden'});
  fixtures.shell=shell(3);fixtures.shell.artifacts[fixtures.shell.manifest.files['shared/pure.js']]='export const value=999;';
  await assert.rejects(runHandler(manifest,'handlers/run.js',{}, {fixtures}),{code:'conflict'});
});
