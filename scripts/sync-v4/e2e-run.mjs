#!/usr/bin/env node
// Entrypoint used only inside the canonical Linux build lane.
import fs from 'node:fs';
import { spawn } from 'node:child_process';
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
fs.cpSync(resolve(here,'../../tests/sync-v4-e2e'),join(snapshot,'tests/sync-v4-e2e'),{recursive:true});
fs.copyFileSync(join(here,'e2e-netns.mjs'),join(snapshot,'scripts/e2e-netns.mjs'));
const manifestPath = resolve(here,'../../source-manifest.json');
const sourceManifest = fs.existsSync(manifestPath) ? JSON.parse(fs.readFileSync(manifestPath)) : null;
if (sourceManifest) for (const row of sourceManifest.files) {
  const file = resolve(here,'../..',row.path);
  if (!file.startsWith(resolve(here,'../..')+'/') || createHash('sha256').update(fs.readFileSync(file)).digest('hex') !== row.sha256) throw Error('Harness source changed during staging');
}
const config = { owner, ...build, output, profile, clients, rttMs, warmRttMs, lossPercent, uid: process.getuid(), gid: process.getgid(),
  seedAfterStart: args['seed-after-start'] === 'yes',
  root: join(output,`tenant-${build.revision.slice(0,12)}`), port: 19865, signalingPort: 19866, turnPort: 34789,
  runner: join(snapshot,'tests/sync-v4-e2e/runner.mjs'), sourceManifest,
  durationSeconds: Number(args.seconds || 900), hostNetworkNamespace: fs.readlinkSync('/proc/self/ns/net'),
  chrome: args.chrome || '/opt/google/chrome/chrome', modulesRoot: args['modules-root'] || join(lane,'deps/shell-browser-collection-auth/node_modules'),
  turnBin: args['turn-bin'] || '/usr/bin/turnserver', turnLib: args['turn-lib'] || '', scenario: args.scenario || null };
if(!Number.isInteger(config.durationSeconds)||config.durationSeconds<30||config.durationSeconds>1800) throw Error('Run duration bound 30..1800 seconds');
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
