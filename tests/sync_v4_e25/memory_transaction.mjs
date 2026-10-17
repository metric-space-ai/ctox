import {test} from 'node:test';
import assert from 'node:assert/strict';
import {MemoryBackend} from '../../src/apps/business-os/kernel/store-idb-v5.mjs';

test('structurally shared transaction values preserve rollback and isolate caller references',async()=>{
 const db=new MemoryBackend('e25-rollback');
 const original={collection:'business_commands',id:'a',data:{payload:{nested:['original']}}};
 await db.tx(async t=>{await t.put('docs',original);await t.put('meta',{key:'checkpoint',value:'1'});});
 original.data.payload.nested.push('caller-mutation');
 await assert.rejects(db.tx(async t=>{
  const read=await t.get('docs',['business_commands','a']);read.data.payload.nested.push('aborted');
  await t.put('docs',read);await t.put('meta',{key:'checkpoint',value:'2'});throw Error('abort');
 }),/abort/);
 assert.deepEqual((await db.get('docs',['business_commands','a'])).data.payload.nested,['original']);
 assert.equal((await db.get('meta','checkpoint')).value,'1');
 await db.tx(async t=>{const read=await t.get('docs',['business_commands','a']);read.data.payload.nested.push('read-without-put');});
 assert.deepEqual((await db.get('docs',['business_commands','a'])).data.payload.nested,['original']);
 await db.tx(async t=>{const read=await t.get('docs',['business_commands','a']);read.data.payload.nested.push('committed');await t.put('docs',read);await t.put('meta',{key:'checkpoint',value:'3'});});
 assert.deepEqual((await db.get('docs',['business_commands','a'])).data.payload.nested,['original','committed']);
 assert.equal((await db.get('meta','checkpoint')).value,'3');
});
