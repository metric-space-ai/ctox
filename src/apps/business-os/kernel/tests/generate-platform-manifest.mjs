import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {pathToFileURL} from 'node:url';
const source=new URL('../../../../core/business_os/v4/ctox-platform.manifest.json',import.meta.url);
const raw=await readFile(source),json=JSON.parse(raw),sha=createHash('sha256').update(raw).digest('hex');
const manifest='const PLATFORM_MANIFEST = '+JSON.stringify(json)+';';
const hash=`export const PLATFORM_MANIFEST_SOURCE_SHA256 = '${sha}';`;
const command='node src/apps/business-os/kernel/tests/generate-platform-manifest.mjs';
const check=process.argv.includes('--check');
async function output(path,text){if(check){if(await readFile(path,'utf8')!==text)throw Error(`Platform manifest drift; regenerate with ${command} --stub-root <stub>`);}else await writeFile(path,text);}
await output(new URL('../platform-manifest.generated.mjs',import.meta.url),'// Generated from src/core/business_os/v4/ctox-platform.manifest.json; regenerate with tests/generate-platform-manifest.mjs.\n'+hash+'\nexport '+manifest+'\n');
const arg=process.argv.indexOf('--stub-root');
if(arg!==-1){
 const root=new URL('./',pathToFileURL(process.argv[arg+1].replace(/\/$/,'')+'/'));
 const path=new URL('ctox-app-api-stub.mjs',root);let stub=await readFile(path,'utf8');
 if(!/^const PLATFORM_MANIFEST = .*;$/m.test(stub))throw Error('Platform manifest anchor missing');
 stub=stub.replace(/^(export )?const PLATFORM_MANIFEST_SOURCE_SHA256 = .*;\n/m,'').replace(/^const PLATFORM_MANIFEST = .*;$/m,hash+'\n'+manifest);
 await output(path,stub);
 const fixtures=new URL('test/fixtures/',root);if(!check)await mkdir(fixtures,{recursive:true});
 await output(new URL('ctox-platform.manifest.json',fixtures),raw.toString('utf8'));
}
