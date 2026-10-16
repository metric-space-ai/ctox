// Real-incident mutation proof: selecting a folder in a background Chrome tab
// used to stall before the first K24 upload. The upstream tool stays immutable.
import fs from 'node:fs';
import {join,dirname,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
const [source,playwright,outArg]=process.argv.slice(2),originalHere=dirname(new URL(import.meta.url).pathname);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const output=resolve(outArg),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
if(!output.startsWith(allowed))throw Error('Owned new output required');fs.mkdirSync(output,{recursive:false,mode:0o700});
// Mutate an owned private runner copy, never the shared staged harness, even
// when a deadline/signal interrupts a synchronous upstream invocation.
const here=join(output,'runner'),origin=JSON.parse(fs.readFileSync(join(originalHere,'ORIGIN.json')));
for(const relative of [...origin.files.map(f=>f.path),'ORIGIN.json','execute.mjs','bind-assets.mjs','playwright-adapter.cjs']){
 const to=join(here,relative);fs.mkdirSync(dirname(to),{recursive:true});fs.copyFileSync(join(originalHere,relative),to);
}
const file=join(here,'playwright-adapter.cjs');
const before=fs.readFileSync(file,'utf8'),line='     await page.bringToFront();';
if(before.split(line).length!==2)throw Error('Foreground mutation anchor not unique');
const run=name=>{
 const dir=join(output,name),log=fs.openSync(join(output,name+'.log'),'w',0o600);
 const p=spawnSync(process.execPath,[join(here,'execute.mjs'),'--target','bind','--source',source,'--output',dir,'--playwright',playwright],{stdio:['ignore',log,log],timeout:13*60*1000});fs.closeSync(log);
 const raw=JSON.parse(fs.readFileSync(join(dir,'result.json')));
 return {exit:p.status,uploaded:raw.subs?.some(s=>s.crit==='V'&&s.name==='Ordner gewählt (webkitdirectory), App-ID aus manifest.json'&&s.status==='PASS')===true,fatal:raw.fatal??null,cleanup:raw.cleanup,evidence:join(dir,'result.json')};
};
const result={mutation:line+' → omitted',startedAt:new Date().toISOString(),pass:false};
try{
 fs.writeFileSync(file,before.replace(line,'     // mutation: page left in background'));
 result.mutated=run('without-foreground');
}finally{fs.writeFileSync(file,before);}
result.restored=run('restored');
result.pass=!result.mutated.uploaded&&result.restored.uploaded&&result.mutated.cleanup==='owned groups absent'&&result.restored.cleanup==='owned groups absent';
result.finishedAt=new Date().toISOString();fs.writeFileSync(join(output,'mutation.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
if(!result.pass)process.exitCode=1;
