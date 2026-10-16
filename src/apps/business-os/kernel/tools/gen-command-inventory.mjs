// Reproducible kernel/stub projection; the native generated inventory is the sole source.
// node src/apps/business-os/kernel/tools/gen-command-inventory.mjs [--stub-root /path/to/private/stub]
import {readFile,writeFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
const source=new URL('../../../../core/business_os/business_command_inventory.json',import.meta.url);
const native=JSON.parse(await readFile(source,'utf8'));
const keys=['exact_control_types','predicate_control_types','control_prefixes','browser_runtime_types','browser_literal_types'];
const inventory=Object.fromEntries(keys.map(k=>[k,native[k]]));
const output='// Generated from src/core/business_os/business_command_inventory.json. Check: tests/e39-inventory.test.mjs.\nexport const COMMAND_INVENTORY = '+JSON.stringify(inventory,null,2)+';\n';
await writeFile(new URL('../command-inventory.generated.mjs',import.meta.url),output);
const arg=process.argv.indexOf('--stub-root');
if(arg!==-1){
 const path=join(resolve(process.argv[arg+1]),'ctox-app-api-stub.mjs'),stub=await readFile(path,'utf8');
 const end=stub.indexOf('const platformTypes');
 if(end<0||!stub.startsWith('// Generated from src/core/business_os/business_command_inventory.json.'))throw Error('Private stub inventory projection not found');
 await writeFile(path,output.replace('export const','const')+'\n'+stub.slice(end));
}
