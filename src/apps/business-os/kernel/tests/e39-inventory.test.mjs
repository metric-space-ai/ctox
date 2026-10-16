import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {COMMAND_INVENTORY} from '../command-inventory.generated.mjs';
import {isPlatformCommand} from '../command-routing.mjs';
import {createReliableChannels} from '../transport-webrtc.mjs';
test('E42 generated platform classes match authoritative inventory, every entry routes to Core',async()=>{
 const native=JSON.parse(await readFile(new URL('../../../../core/business_os/business_command_inventory.json',import.meta.url)));
 for(const [name,types] of Object.entries(COMMAND_INVENTORY)){assert.deepEqual(types,native[name],name);for(const type of types)assert.equal(isPlatformCommand(name==='control_prefixes'?type+'probe':type),true,type);}
 for(const type of ['business_os.chat.task','business_os.context.ask','business_os.data.modify','ctox.app.new','ctox.coding.new','ctox.workjet.new.v1','ctox.plan.step.retry'])assert.equal(isPlatformCommand(type),true,type);
 assert.equal(isPlatformCommand('not.registered'),false);
});
test('E41 browser and native share the same reliable lane names and ordering',async()=>{
 const names=['ctox-v4-control','ctox-v4-commands','ctox-v4-interactive','ctox-v4-live','ctox-v4-initial-sync','ctox-v4-transfer'];
 const created=[];createReliableChannels({createDataChannel(label){created.push(label);return {label};}});assert.deepEqual(created,names);
 const rust=await readFile(new URL('../../../../core/rxdb/src/plugins/replication_webrtc/lanes.rs',import.meta.url),'utf8');
 assert.deepEqual([...rust.matchAll(/Self::\w+ => "([^"]+)"/g)].map(x=>x[1]),names);
});
