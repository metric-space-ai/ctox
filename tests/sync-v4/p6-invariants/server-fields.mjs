// Captures real kernel outbox encoding, executes the native P8/policy test,
// then delivers its real page/receipt to the same live kernel and checks archive_local.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {spawnSync} from 'node:child_process';
import {join} from 'node:path';
import {createKernel} from '../../../src/apps/business-os/kernel/kernel.mjs';
import {KernelTransport} from '../../../src/apps/business-os/kernel/transport.mjs';
import {openBackend} from '../../../src/apps/business-os/kernel/store-idb-v5.mjs';
const mode=process.argv[2],out=process.env.P6_FIELDS_DIR;
assert.ok(['patch','guard','stale'].includes(mode));
class WireCapture extends KernelTransport {
  async connect(identity,events){this.events=events;events.link({state:'online',rtt_ms:0,relay:false});}
  async subscribe(){}
  async write(entry){this.sent=structuredClone(entry);}
}
const t=new WireCapture(),k=createKernel({instanceId:'p6-fields-'+process.pid,userId:'alice',storage:'memory',multiTab:false,transport:t});
const functions={patch:'p6_fields_peer_patch_preserves_concurrent_native_fields',guard:'p6_fields_explicit_server_change_rejected_and_journaled',stale:'p6_fields_stale_server_intent_rejected_and_journaled'};
try{
  await k.ready;
  const fresh=mode==='guard';
  const doc={id:'lead',native_epoch:fresh?2:1,pipeline:{stage:'open',native_score:fresh?20:10}};
  await k.receivePage({collection:'leads',scope_hash:'local',epoch:'1',gen:1,cursor_to:'1',complete:true,docs:[{id:'lead',data:doc,seq:'1',rev:fresh?'native:new':'native:old'}]});
  const patch=mode==='patch'?{pipeline:{stage:'won'}}:{native_epoch:999,pipeline:{native_score:999}};
  const handle=k.appApi('p6-leads','1').patch('leads','lead',patch);await handle.local;
  for(let n=0;n<200 && !t.sent;n++)await new Promise(r=>setTimeout(r,5));
  assert.ok(t.sent,'real kernel did not dispatch the pending patch');
  assert.equal(t.sent.op,'patch');
  assert.deepEqual(t.sent.body,patch,'kernel patch carried stale server-owned fields');
  const keys=['client_seq','op_id','prev_op_id','collection','id','base_rev','base_seq','gen','schema_ver','app_ver','op','body'];
  const wire=Object.fromEntries(keys.map(key=>[key,t.sent[key]]));
  const input=join(out,mode+'-input.json'),output=join(out,mode+'-native.json');
  fs.writeFileSync(input,JSON.stringify({case:mode,wire},null,2));
  const child=spawnSync(process.env.P6_FIELDS_BINARY,[functions[mode],'--nocapture','--test-threads=1'],{env:{...process.env,P6_FIELDS_INPUT:input,P6_FIELDS_OUTPUT:output},encoding:'utf8',timeout:30000});
  fs.writeFileSync(join(out,mode+'-native.log'),(child.stdout??'')+(child.stderr??''));
  if(!fs.existsSync(output))throw Error('native bridge failed before producing a receipt: '+child.status+' '+child.error);
  const actual=JSON.parse(fs.readFileSync(output));
  if(mode==='patch'){
    assert.notEqual(actual.receipt.status,'rejected','peer-only patch was rejected after concurrent native update');
    await k.receivePage({collection:'leads',scope_hash:'local',epoch:'1',gen:1,cursor_to:actual.receipt.seq,complete:true,docs:[{id:'lead',data:actual.doc,seq:actual.receipt.seq,rev:actual.receipt.rev}]});
  }
  await t.events.write_result(actual.receipt);
  const result=await Promise.race([handle.result,new Promise((_,reject)=>setTimeout(()=>reject(Error('kernel result deadline')),5000))]);
  const db=await openBackend(k.dbName,'memory');
  try{
    const archive=await db.all('archive_local');const pending=await db.all('outbox');
    assert.ok(!pending.some(r=>r.op_id===handle.op_id),'terminal native result remained pending');
    if(mode==='patch'){
      assert.equal(archive.length,0,'peer-only patch was archived as denied');
      const stored=await db.get('docs',['leads','lead']);
      assert.equal(stored.data.native_epoch,2);assert.equal(stored.data.pipeline.native_score,20);assert.equal(stored.data.pipeline.stage,'won');
    }else{
      assert.equal(result.status,'rejected','server-owned intent was accepted after merge erased it');
      assert.equal(result.reason,'command_only_field','server-owned mutation lost specific rejection');
      const entry=archive.find(r=>r.op_id===handle.op_id);
      assert.ok(entry,'kernel discarded the denied server-owned input instead of journaling it');
      assert.equal(entry.reason,'command_only_field');assert.deepEqual(entry.body,patch);
    }
  }finally{await db.close();}
  assert.equal(child.status,0,'native invariant failed; inspect '+mode+'-native.log');
  console.log('P6_FIELDS_CASE',mode,'pass');
}finally{await k.close();}
