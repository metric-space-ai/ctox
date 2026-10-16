// Runs the Shell owner's news v4-api assertions unchanged. Only fixture bootstrap,
// lane Chromium executable and evidence paths are adapted; no UI source is edited.
import {readFile,writeFile,mkdir,cp,rename} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {join,resolve} from 'node:path';
import {spawn} from 'node:child_process';
const root=fileURLToPath(new URL('../../../../../',import.meta.url));
const out=resolve(process.env.KERNEL_EVIDENCE_DIR),stub=resolve(process.env.CTOX_APP_API_STUB);
await mkdir(join(out,'shots'),{recursive:true});await cp(stub,join(out,'stub'),{recursive:true});
const privateRoot=join(out,'stub');await rename(join(privateRoot,'ctox-app-api-stub.mjs'),join(privateRoot,'implementation.mjs'));
const manifest=JSON.parse(await readFile(join(root,'src/apps/business-os/v4/manifests/news.manifest.json'),'utf8'));
await writeFile(join(privateRoot,'ctox-app-api-stub.mjs'),`export * from './implementation.mjs';
import {createStubKernel as create} from './implementation.mjs';
const manifest=${JSON.stringify(manifest)};
export function createStubKernel(options){const kernel=create(options);const ready=kernel.ready.then(async()=>{await kernel.sim.publishAppVersion('news','1',{collections:manifest.collections,manifestExtra:{foreign_collections:manifest.foreign_collections,exports:manifest.exports}});});return {...kernel,ready};}
`);
const source=await readFile(join(root,'src/apps/business-os/modules/news/tests/v4-api.test.mjs'),'utf8');
const adapted=source.replace("const BOS = normalize(join(HERE, '../../..'));",`const BOS = ${JSON.stringify(join(root,'src/apps/business-os'))};`).replace('browser = await chromium.launch();',"browser = await chromium.launch({executablePath:process.env.CHROME_BIN,args:['--no-sandbox']});");
if(adapted===source || !adapted.includes('executablePath:process.env.CHROME_BIN'))throw Error('News harness fixture anchors changed');
const file=join(out,'news-v4-api.test.mjs');await writeFile(file,adapted);
const child=spawn(process.execPath,['--test',file],{stdio:'inherit',env:{...process.env,CTOX_APP_API_STUB:privateRoot,NEWS_SHOTS:'1'}});
const exit=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',(code,signal)=>resolve(code??(signal?1:0)));});process.exitCode=exit;
