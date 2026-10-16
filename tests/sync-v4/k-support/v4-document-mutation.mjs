// Actual daemon/Chromium counterexample: a fulfilled synthetic document lacks
// the real daemon document's network provenance. Never mutate shared source.
import fs from 'node:fs';
import {join,dirname,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
const [receipt,kernelArchive,kernelSha,kernelRevision,modulesRoot,turnBin,turnLib,outArg]=process.argv.slice(2);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical lane required');
const output=resolve(outArg),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts')+'/';
if(!output.startsWith(allowed))throw Error('Owned new output required');fs.mkdirSync(output,{recursive:false,mode:0o700});
const repository=resolve(dirname(new URL(import.meta.url).pathname),'../../..'),manifest=JSON.parse(fs.readFileSync(join(repository,'source-manifest.json'))),root=join(output,'source');
const sha=b=>createHash('sha256').update(b).digest('hex');
for(const f of manifest.files){const b=fs.readFileSync(join(repository,f.path));if(sha(b)!==f.sha256)throw Error('Staged source changed');const dest=join(root,f.path);fs.mkdirSync(dirname(dest),{recursive:true});fs.writeFileSync(dest,b);}
const archive=join(root,'kernel.tar');fs.copyFileSync(kernelArchive,archive);
const relative='tests/sync-v4/k-support/v4-runner.mjs',file=join(root,relative),before=fs.readFileSync(file,'utf8');
const line=" const document=await page.goto(origin+'/local-modules/e2e-probe/v4-preflight.html');";
if(before.split(line).length!==2)throw Error('Mutation anchor not unique');
const altered=before.replace(line," await context.route('**/e2e-v4-probe.html',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><title>Isolated native V4 preflight</title><body>Native V4 preflight</body>'}));\n const document=await page.goto(origin+'/e2e-v4-probe.html');");
const saveManifest=bytes=>{const next=structuredClone(manifest);next.files.find(f=>f.path===relative).sha256=sha(bytes);next.mutation={path:relative,originalSha256:sha(before),measuredSha256:sha(bytes)};fs.writeFileSync(join(root,'source-manifest.json'),JSON.stringify(next,null,2));};
const run=name=>{
 const out=join(output,name),log=fs.openSync(join(output,name+'.log'),'w',0o600);
 const p=spawnSync(process.execPath,[join(root,'scripts/sync-v4/e2e-run.mjs'),'--build',receipt,'--catalog','yes','--v4','yes','--kernel-archive',archive,'--kernel-archive-sha256',kernelSha,'--kernel-revision',kernelRevision,'--modules-root',modulesRoot,'--turn-bin',turnBin,'--turn-lib',turnLib,'--seconds','150','--output',out],{cwd:root,stdio:['ignore',log,log],timeout:190000});fs.closeSync(log);
 const result=JSON.parse(fs.readFileSync(join(out,'result.json'))),controller=JSON.parse(fs.readFileSync(join(out,'controller.json')));
 return {exit:p.status,binding:result.binding??null,diagnostics:result.browserDiagnostics,groupAbsent:controller.groupAbsent,evidence:join(out,'result.json')};
};
const result={mutation:'real served document → synthetic route.fulfill document',startedAt:new Date().toISOString(),pass:false};
try{fs.writeFileSync(file,altered);saveManifest(altered);result.mutated=run('fulfilled');}
finally{fs.writeFileSync(file,before);saveManifest(before);}
result.restored=run('served');
result.pass=result.mutated.diagnostics?.some(e=>e.classes?.includes('ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS'))===true&&result.mutated.binding?.bootstrapHttpStatus===200&&result.mutated.binding?.kernelImported===true&&result.mutated.binding?.nativeConnected===false&&result.restored.binding?.nativeConnected===true&&result.restored.binding?.relay?.forced===true&&result.mutated.groupAbsent===true&&result.restored.groupAbsent===true;
result.finishedAt=new Date().toISOString();fs.writeFileSync(join(output,'mutation.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));if(!result.pass)process.exitCode=1;
