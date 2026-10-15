#!/usr/bin/env node
// Entrypoint used only inside the canonical Linux build lane.
import fs from 'node:fs';
import { spawn, execFileSync } from 'node:child_process';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { owner, verifiedBuild } from '../../tests/sync-v4-e2e/native.mjs';
const args = {};
for (let i=2;i<process.argv.length;i+=2) { if(!process.argv[i]?.startsWith('--') || !process.argv[i+1]) throw Error('Expected --key value'); args[process.argv[i].slice(2)] = process.argv[i+1]; }
if(process.platform!=='linux'||!process.env.CARGO_TARGET_DIR||!process.env.TMPDIR||!process.env.BUILD_LANE_BIN) throw Error('Canonical Linux lane required');
const build = verifiedBuild(args.build);
const profile = args.profile || 'small', clients = Number(args.clients || 1), rttMs = Number(args.rtt || 150), warmRttMs = Number(args['warm-rtt'] || rttMs), lossPercent = Number(args.loss || 1);
if(!['small','thesen-like'].includes(profile)||!Number.isInteger(clients)||clients<1||clients>3||![20,150,600].includes(rttMs)||![20,150,600].includes(warmRttMs)||lossPercent<0||lossPercent>10) throw Error('Unsupported bounded profile');
const lane = resolve(process.env.BUILD_LANE_BIN,'..');
const output = resolve(args.output || join(lane,'artifacts',`v4-e2e-${Date.now()}-${process.pid}`));
if(!output.startsWith(join(lane,'artifacts')+'/')) throw Error('Output must be a new private lane artifact directory');
fs.mkdirSync(output,{recursive:false,mode:0o700});
const here = dirname(fileURLToPath(import.meta.url));
const snapshot = join(output, '.runner');
fs.mkdirSync(join(snapshot, 'tests'), { recursive: true });
fs.mkdirSync(join(snapshot, 'scripts'), { recursive: true });
const repository = resolve(here,'../..');
const manifestPath = join(repository,'source-manifest.json');
const sourceManifest = fs.existsSync(manifestPath) ? JSON.parse(fs.readFileSync(manifestPath)) : {
  head:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),scope:'owned harness files; per-file hashes',
  files:[...new Set(execFileSync('git',['ls-files','-c','-o','--exclude-standard','-z','--','tests/sync-v4-e2e','tests/sync-v4','scripts/sync-v4'],{encoding:'utf8'}).split('\0').filter(Boolean))]
    .filter(path=>/^(tests\/sync-v4-e2e\/|tests\/sync-v4\/k[^/]+\/|scripts\/sync-v4\/(e2e-[^/]+\.mjs|gate-regression\.sh)$)/.test(path))
    .map(path=>({path,sha256:createHash('sha256').update(fs.readFileSync(join(repository,path))).digest('hex')}))
};
for (const row of sourceManifest.files) {
  const file=resolve(repository,row.path);
  if(!file.startsWith(repository+'/')||!fs.lstatSync(file).isFile())throw Error('Harness manifest path escape or non-file');
  const bytes=fs.readFileSync(file);
  if(createHash('sha256').update(bytes).digest('hex')!==row.sha256)throw Error('Harness source changed during staging');
  if(row.path.startsWith('tests/sync-v4-e2e/') || (args.catalog==='yes'&&row.path.startsWith('tests/sync-v4/k'))){
    const dest=join(snapshot,row.path);fs.mkdirSync(dirname(dest),{recursive:true});fs.writeFileSync(dest,bytes);
  }
}
if(!sourceManifest.files.some(r=>r.path==='scripts/sync-v4/e2e-netns.mjs'))throw Error('Namespace helper absent from source custody');
fs.copyFileSync(join(here,'e2e-netns.mjs'),join(snapshot,'scripts/e2e-netns.mjs'));
let kernelAssets=null;
if(args.v4==='yes'){
 const revision=args['kernel-revision'];
 if(!/^[a-f0-9]{40}$/.test(revision??''))throw Error('V4 requires an immutable browser kernel revision');
 kernelAssets=join(snapshot,'browser-kernel');fs.mkdirSync(kernelAssets);
 const bytes=args['kernel-archive']?fs.readFileSync(resolve(repository,args['kernel-archive'])):execFileSync('git',['archive',revision,'src/apps/business-os/kernel','src/apps/business-os/rxdb/src'],{maxBuffer:32*1024*1024});
 if(args['kernel-archive']&&createHash('sha256').update(bytes).digest('hex')!==args['kernel-archive-sha256'])throw Error('Pinned browser kernel archive checksum mismatch');
 const entries=execFileSync('tar',['-tf','-'],{input:bytes,encoding:'utf8'}).trim().split('\n');
 if(entries.some(p=>p.startsWith('/')||p.split('/').includes('..')||!['src/apps/business-os/kernel/','src/apps/business-os/rxdb/src/'].some(prefix=>p.startsWith(prefix)||prefix.startsWith(p))))throw Error('Browser kernel archive path escape');
 execFileSync('tar',['-xf','-','-C',kernelAssets],{input:bytes});
 fs.writeFileSync(join(kernelAssets,'origin.json'),JSON.stringify({revision,archiveSha256:createHash('sha256').update(bytes).digest('hex')}));
}
const config = { owner, ...build, output, profile, clients, rttMs, warmRttMs, lossPercent, uid: process.getuid(), gid: process.getgid(),
  seedAfterStart: args['seed-after-start'] === 'yes', leadProfile: args['lead-profile'] || 'eager',
  v4: args.v4 === 'yes', kernelAssets, kernelRevision:args['kernel-revision']??null, osBootstrap: args['os-bootstrap'] || null, catalogDuration: args['catalog-duration'] === 'full' ? 'full' : 'slice', prime: args.prime === 'yes', upgradeBuild: args['upgrade-build'] || null, schemaCli: args['schema-cli'] ? JSON.parse(fs.readFileSync(args['schema-cli'])) : null, catalog: args.catalog === 'yes', scenarioRoot: args.catalog === 'yes' ? join(snapshot,'tests/sync-v4') : null,
  root: join(output,`tenant-${build.revision.slice(0,12)}`), port: 19865, signalingPort: 19866, turnPort: 34789,
  runner: args.v4 === 'yes' ? join(snapshot,'tests/sync-v4/k-support/v4-runner.mjs') : join(snapshot,'tests/sync-v4-e2e/runner.mjs'), sourceManifest, buildReceipt: resolve(args.build), lane, canonicalTmpdir: process.env.TMPDIR,
  durationSeconds: Number(args.seconds || 900), hostNetworkNamespace: fs.readlinkSync('/proc/self/ns/net'),
  chrome: args.chrome || '/opt/google/chrome/chrome', modulesRoot: args['modules-root'] || join(lane,'deps/shell-browser-collection-auth/node_modules'),
  turnBin: args['turn-bin'] || '/usr/bin/turnserver', turnLib: args['turn-lib'] || '', scenario: args.scenario || null };
if(args['installed-profile']){
 if(args.v4!=='yes')throw Error('Installed profile binding is V4-only');
 const file=resolve(args['installed-profile']);
 if(!file.startsWith(join(lane,'artifacts')+'/'))throw Error('Private installation evidence required');
 const proof=JSON.parse(fs.readFileSync(file));
 if(proof.id!=='K11'||proof.terminal!==true||proof.installerExecuted!==true||proof.dataPreserved!==true||proof.controller?.groupAbsent!==true||proof.afterRevision!==build.revision||proof.installer?.activeBinarySha256!==build.binarySha256||!proof.beforeSnapshot?.quiesced||!proof.afterSnapshot?.quiesced)throw Error('Incomplete or mismatched installed profile');
 const root=resolve(proof.root),current=fs.realpathSync(join(root,'managed/current'));
 if(!root.startsWith(join(lane,'artifacts')+'/')||current!==proof.currentRelease||!current.startsWith(root+'/managed/releases/'))throw Error('Installed profile path mismatch');
 const marker=JSON.parse(fs.readFileSync(join(root,'.sync-v4-e2e.json')));
 if(marker.owner!==owner||marker.synthetic!==true||createHash('sha256').update(fs.readFileSync(join(current,'bin/ctox'))).digest('hex')!==build.binarySha256)throw Error('Installed profile owner/binary mismatch');
 config.root=current;config.preparedProfile={evidence:file,root,fixture:proof.afterFixture};
}
if (!['eager','demand-only'].includes(config.leadProfile)) throw Error('Unsupported fixture profile');
if(!Number.isInteger(config.durationSeconds)||config.durationSeconds<30||config.durationSeconds>2100) throw Error('Run duration bound 30..2100 seconds');
for (const file of [config.turnBin, config.chrome]) fs.accessSync(file, fs.constants.X_OK);
fs.accessSync(join(config.modulesRoot, 'playwright/package.json'), fs.constants.R_OK);
fs.accessSync(join(config.source, 'src/apps/business-os/app.js'), fs.constants.R_OK);
const file = join(output,'config.private.json'); fs.writeFileSync(file,JSON.stringify(config,null,2),{mode:0o600});
const child = spawn('sudo',['-n','unshare','--net','--',process.execPath,join(snapshot,'scripts/e2e-netns.mjs'),file],{stdio:'inherit',detached:true});
const record = { owner, controllerPid: process.pid, privilegeControllerPid: child.pid, group: child.pid, output,
  stop: `terminal or ${config.durationSeconds+40}s`, startedAt:new Date().toISOString() };
fs.writeFileSync(join(output,'controller.json'),JSON.stringify(record,null,2));
const kill = signal => {try{process.kill(-child.pid,signal);}catch(e){if(e.code!=='ESRCH')throw e;}};
let escalator;
const timer = setTimeout(()=>{kill('SIGTERM');escalator=setTimeout(()=>kill('SIGKILL'),7000);},(config.durationSeconds+30)*1000);
process.once('SIGTERM',()=>kill('SIGTERM'));
const exit = await new Promise((r,j)=>{child.once('error',j);child.once('exit',(code,signal)=>r({code,signal}));});
clearTimeout(timer);clearTimeout(escalator);kill('SIGKILL');
record.exit=exit;record.finishedAt=new Date().toISOString();
record.namespaceProof = JSON.parse(fs.readFileSync(file)).netem;
record.groupAbsent = false;
for (let attempt = 0; attempt < 40; attempt++) {
  try { process.kill(-child.pid,0); } catch(e) { if(e.code==='ESRCH') { record.groupAbsent=true; break; } throw e; }
  await new Promise(r => setTimeout(r,50));
}
fs.writeFileSync(join(output,'controller.json'),JSON.stringify(record,null,2));
console.log(JSON.stringify({output,exit,groupAbsent:record.groupAbsent}));process.exitCode=exit.code??1;
