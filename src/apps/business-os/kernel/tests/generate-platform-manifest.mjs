import {readFile,writeFile} from 'node:fs/promises';
const source=new URL('../../../../core/business_os/v4/ctox-platform.manifest.json',import.meta.url);
const json=JSON.parse(await readFile(source,'utf8'));
const text='const PLATFORM_MANIFEST = '+JSON.stringify(json)+';\n';
await writeFile(new URL('../platform-manifest.generated.mjs',import.meta.url),'// Generated from src/core/business_os/v4/ctox-platform.manifest.json; regenerate with tests/generate-platform-manifest.mjs.\nexport '+text);
const arg=process.argv.indexOf('--stub-root');
if(arg!==-1){const path=new URL('file://'+process.argv[arg+1].replace(/\/$/,'')+'/ctox-app-api-stub.mjs');const stub=await readFile(path,'utf8');if(!/^const PLATFORM_MANIFEST = .*;$/m.test(stub))throw Error('Platform manifest anchor missing');await writeFile(path,stub.replace(/^const PLATFORM_MANIFEST = .*;$/m,text.trimEnd()));}
