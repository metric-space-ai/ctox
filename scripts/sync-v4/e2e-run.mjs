#!/usr/bin/env node
// Entrypoint used only inside the canonical Linux build lane.
import fs from 'node:fs';
import { spawn } from 'node:child_process';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { owner, verifiedBuild } from '../../tests/sync-v4-e2e/native.mjs';
const args = {};
for (let i=2;i<process.argv.length;i+=2) { if(!process.argv[i]?.startsWith('--') || !process.argv[i+1]) throw Error('Expected --key value'); args[process.argv[i].slice(2)] = process.argv[i+1]; }
if(process.platform!=='linux'||!process.env.CARGO_TARGET_DIR||!process.env.TMPDIR||!process.env.BUILD_LANE_BIN) throw Error('Canonical Linux lane required');
const build = verifiedBuild(args.build);
const profile = args.profile || 'small', clients = Number(args.clients || 1), rttMs = Number(args.rtt || 150), lossPercent = Number(args.loss || 1);
if(!['small','thesen-like'].includes(profile)||!Number.isInteger(clients)||clients<1||clients>3||![20,150,600].includes(rttMs)||lossPercent<0||lossPercent>10) throw Error('Unsupported bounded profile');
const lane = resolve(process.env.BUILD_LANE_BIN,'..');
const output = resolve(args.output || join(lane,'artifacts',`v4-e2e-${Date.now()}-${process.pid}`));
if(!output.startsWith(join(lane,'artifacts')+'/')) throw Error('Output must be a new private lane artifact directory');
fs.mkdirSync(output,{recursive:false,mode:0o700});
const here = dirname(fileURLToPath(import.meta.url));
const config = { owner, ...build, output, profile, clients, rttMs, lossPercent, uid: process.getuid(), gid: process.getgid(),
  root: join(output,`tenant-${build.revision.slice(0,12)}`), port: 19865, signalingPort: 19866, turnPort: 34789,
  runner: resolve(here,'../../tests/sync-v4-e2e/runner.mjs'),
  durationSeconds: Number(args.seconds || 900), hostNetworkNamespace: fs.readlinkSync('/proc/self/ns/net'),
  chrome: args.chrome || '/opt/google/chrome/chrome', modulesRoot: args['modules-root'] || join(lane,'deps/shell-browser-collection-auth/node_modules'),
  turnBin: args['turn-bin'] || '/usr/bin/turnserver', turnLib: args['turn-lib'] || '', scenario: args.scenario || null };
if(!Number.isInteger(config.durationSeconds)||config.durationSeconds<30||config.durationSeconds>1800) throw Error('Run duration bound 30..1800 seconds');
const file = join(output,'config.private.json'); fs.writeFileSync(file,JSON.stringify(config,null,2),{mode:0o600});
const child = spawn('sudo',['-n','unshare','--net','--',process.execPath,join(here,'e2e-netns.mjs'),file],{stdio:'inherit',detached:true});
const record = { owner, controllerPid: process.pid, namespaceOwnerPid: child.pid, group: child.pid, output,
  stop: `terminal or ${config.durationSeconds+40}s`, startedAt:new Date().toISOString() };
fs.writeFileSync(join(output,'controller.json'),JSON.stringify(record,null,2));
const kill = signal => {try{process.kill(-child.pid,signal);}catch(e){if(e.code!=='ESRCH')throw e;}};
let escalator;
const timer = setTimeout(()=>{kill('SIGTERM');escalator=setTimeout(()=>kill('SIGKILL'),7000);},(config.durationSeconds+30)*1000);
process.once('SIGTERM',()=>kill('SIGTERM'));
const exit = await new Promise((r,j)=>{child.once('error',j);child.once('exit',(code,signal)=>r({code,signal}));});
clearTimeout(timer);clearTimeout(escalator);kill('SIGKILL');
record.exit=exit;record.finishedAt=new Date().toISOString();
try{process.kill(-child.pid,0);record.groupAbsent=false;}catch(e){record.groupAbsent=e.code==='ESRCH';}
fs.writeFileSync(join(output,'controller.json'),JSON.stringify(record,null,2));
console.log(JSON.stringify({output,exit,groupAbsent:record.groupAbsent}));process.exitCode=exit.code??1;
