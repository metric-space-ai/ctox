// Runs the Shell owner's intake v4-api assertions unchanged. Only fixture bootstrap,
// lane Chromium executable and evidence paths are adapted; no UI source is edited.
import {readFile,writeFile,mkdir,cp} from 'node:fs/promises';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {join,resolve} from 'node:path';
import {spawn} from 'node:child_process';
const root=fileURLToPath(new URL('../../../../../',import.meta.url));
const out=resolve(process.env.KERNEL_EVIDENCE_DIR),stub=resolve(process.env.CTOX_APP_API_STUB);
await mkdir(join(out,'shots'),{recursive:true});await cp(stub,join(out,'stub'),{recursive:true});
const privateRoot=join(out,'stub');
const source=await readFile(join(root,'src/apps/business-os/modules/intake/tests/v4-api.test.mjs'),'utf8');
const adapted=source.replace("from '../../../v4/tools/port/real-manifest.mjs'",'from '+JSON.stringify(pathToFileURL(join(root,'src/apps/business-os/v4/tools/port/real-manifest.mjs')).href)).replace("const BOS = normalize(join(HERE, '../../..'));",`const BOS = ${JSON.stringify(join(root,'src/apps/business-os'))};`).replace('browser = await chromium.launch();',"browser = await chromium.launch({executablePath:process.env.CHROME_BIN,args:['--no-sandbox']});");
if(adapted===source || !adapted.includes('executablePath:process.env.CHROME_BIN'))throw Error('Intake harness fixture anchors changed');
const file=join(out,'intake-v4-api.test.mjs');await writeFile(file,adapted);
const child=spawn(process.execPath,['--test',...(process.env.E48_NEWS_TEST_PATTERN?['--test-name-pattern='+process.env.E48_NEWS_TEST_PATTERN]:[]),file],{stdio:'inherit',env:{...process.env,CTOX_APP_API_STUB:privateRoot,NEWS_SHOTS:'1'}});
const exit=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',(code,signal)=>resolve(code??(signal?1:0)));});process.exitCode=exit;
