#!/usr/bin/env node
// P6 incidents use #810's namespace, Native lifecycle and run(h) protocol.
// The child selects the actual service composition root. No V3 SQL fixture,
// fake Core, standalone business-os serve or foreign source patch is allowed.
import fs from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {spawn,execFileSync} from 'node:child_process';
import {verifiedBuild} from './build-receipt.mjs';
const argv=process.argv.slice(2),flags=new Map();
for(let i=0;i<argv.length;i+=2) {
  if(!argv[i]?.startsWith('--') || !argv[i+1])throw Error('Expected --key value');
  if(flags.has(argv[i]))throw Error('Duplicate option: '+argv[i]);
  flags.set(argv[i],argv[i+1]);
}
if(process.platform!=='linux' || !process.env.BUILD_LANE_BIN || !process.env.TMPDIR || !process.env.CARGO_TARGET_DIR)
  throw Error('Canonical Linux lane required');
const allowed=new Set(['--build','--expected-revision','--output','--chrome','--modules-root','--turn-bin','--turn-lib','--os-bootstrap']);
for(const flag of flags.keys())if(!allowed.has(flag))throw Error('Unsupported P6 option: '+flag);
const expected=flags.get('--expected-revision');
if(!/^[a-f0-9]{40}$/.test(expected??''))throw Error('Immutable expected daemon revision required');
const lane=resolve(process.env.BUILD_LANE_BIN,'..'),artifacts=join(lane,'artifacts');
const output=resolve(flags.get('--output')??join(artifacts,'p6-incidents-'+Date.now()));
if(!output.startsWith(artifacts+'/'))throw Error('Output must be a fresh private lane artifact directory');
fs.mkdirSync(output,{recursive:false,mode:0o700});
const here=dirname(fileURLToPath(import.meta.url)),repository=resolve(here,'../../..');
const files=execFileSync('git',['ls-files','-c','-o','--exclude-standard','-z','--',
  'tests/sync-v4-e2e','tests/sync-v4','scripts/sync-v4'],{cwd:repository,encoding:'utf8'})
  .split('\0').filter(Boolean);
const hash=file=>createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const sourceManifest={head:execFileSync('git',['rev-parse','HEAD'],{cwd:repository,encoding:'utf8'}).trim(),
  scope:'immutable dedicated harness source; includes owned P6 test overlay',
  files:[...new Set(files)].map(path=>({path,sha256:hash(join(repository,path))}))};
const unchanged=()=>sourceManifest.files.every(row=>hash(join(repository,row.path))===row.sha256);
const summary={revision:expected,owner:'01a14204-d613',startedAt:new Date().toISOString(),
  daemonEntry:'service --foreground',harnessSource:sourceManifest,scenarios:[],pass:false,terminal:false};
const save=()=>fs.writeFileSync(join(output,'p6-result.json'),JSON.stringify(summary,null,2));
save();
let build;
try {
  build=verifiedBuild(flags.get('--build'));
  if(build.revision!==expected)throw Error('Daemon receipt does not match requested cutover revision');
} catch(error) {
  summary.scenarios=['K13','K21','K22'].map(id=>({id,status:'not_run',pass:false,acceptanceComplete:false,
    reason:'build unverified: '+error.message,daemonExecuted:false}));
}
let cancelled=false;
async function namespace(config,log) {
  const child=spawn('unshare',['--user','--map-root-user','--net','--mount','--pid','--fork','--mount-proc','--',process.execPath,
    join(here,'netns.mjs'),config],{stdio:['ignore',log,log],detached:true});
  const row={owner:summary.owner,pid:child.pid,pgid:child.pid,purpose:'isolated P6 scenario namespace',
    stop:'terminal or bounded namespace deadline',startedAt:new Date().toISOString()};
  const controller=join(dirname(config),'p6-controller.json');
  const record=()=>fs.writeFileSync(controller,JSON.stringify(row,null,2));record();
  const kill=signal=>{try{process.kill(-child.pid,signal);}catch(e){if(e.code!=='ESRCH')throw e;}};
  let escalation;
  const seconds=JSON.parse(fs.readFileSync(config)).durationSeconds;
  const timer=setTimeout(()=>{kill('SIGTERM');escalation=setTimeout(()=>kill('SIGKILL'),7000);},(seconds+30)*1000);
  const cancel=()=>{cancelled=true;kill('SIGTERM');};process.once('SIGTERM',cancel);
  try {
    const exit=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',(code,signal)=>resolve({code,signal}));});
    row.exit=exit;kill('SIGKILL');
    row.groupAbsent=false;
    for(let attempt=0;attempt<40;attempt++) {
      try{process.kill(-child.pid,0);}catch(e){if(e.code==='ESRCH'){row.groupAbsent=true;break;}throw e;}
      await new Promise(r=>setTimeout(r,50));
    }
    return row;
  } finally {
    clearTimeout(timer);clearTimeout(escalation);process.removeListener('SIGTERM',cancel);
    row.finishedAt=new Date().toISOString();record();
  }
}
if(build && !summary.scenarios.length) {
  for(const [id,seconds] of [['K13',180],['K21',1200],['K22',900]]) {
    if(cancelled){summary.scenarios.push({id,status:'not_run',pass:false,acceptanceComplete:false,reason:'Controller interrupted',daemonExecuted:false});save();continue;}
    const dir=join(output,id);fs.mkdirSync(dir,{mode:0o700});
    const config={owner:summary.owner,...build,output:dir,profile:'small',clients:1,rttMs:20,warmRttMs:20,lossPercent:0,
      uid:process.getuid(),gid:process.getgid(),hostNamespaces:Object.fromEntries(['user','net','mnt','pid'].map(k=>[k,fs.readlinkSync('/proc/self/ns/'+k)])),seedAfterStart:false,leadProfile:'eager',prime:false,
      catalog:true,scenarioRoot:resolve(here,'..'),scenario:id,osBootstrap:flags.get('--os-bootstrap')??null,
      root:join(dir,'tenant-'+expected.slice(0,12)),port:19865,signalingPort:19866,turnPort:34789,
      runner:join(here,'service-runner.mjs'),sourceManifest,buildReceipt:resolve(flags.get('--build')),
      lane,canonicalTmpdir:process.env.TMPDIR,durationSeconds:seconds,hostNetworkNamespace:fs.readlinkSync('/proc/self/ns/net'),
      chrome:flags.get('--chrome')??'/opt/google/chrome/chrome',
      modulesRoot:flags.get('--modules-root')??join(lane,'deps/shell-browser-collection-auth/node_modules'),
      turnBin:flags.get('--turn-bin')??'/usr/bin/turnserver',turnLib:flags.get('--turn-lib')??''};
    const configPath=join(dir,'config.private.json');
    fs.writeFileSync(configPath,JSON.stringify(config,null,2),{mode:0o600});
    const log=fs.openSync(join(output,id+'.log'),'w',0o600);
    let controller,error;
    try {
      // Browser/TURN prerequisites are checked only when a native listener exists.
      // An unrelated missing browser must not prevent observing the real daemon.
      if(!unchanged())throw Error('Harness source changed before scenario');
      controller=await namespace(configPath,log);
    } catch(e){error=e.message;}finally{fs.closeSync(log);}
    const path=join(dir,'result.json'),report=fs.existsSync(path)?JSON.parse(fs.readFileSync(path)):null;
    const entryPath=join(dir,'p6-daemon-entry.json'),boot=fs.existsSync(entryPath)?JSON.parse(fs.readFileSync(entryPath)):null;
    const daemonPath=join(dir,'daemon.log'),daemonLog=fs.existsSync(daemonPath)?fs.readFileSync(daemonPath,'utf8'):'';
    const entryBound=boot?.revision===expected && boot?.binarySha256===build.binarySha256
      && boot?.root===config.root && Number.isInteger(boot?.pid)
      && JSON.stringify(boot?.args)===JSON.stringify(['service','--foreground','--root',config.root]);
    const row=report?.scenarios?.find(row=>row.id===id);
    const namespacePath=join(dir,'p6-namespace.json');
    const ns=fs.existsSync(namespacePath)?JSON.parse(fs.readFileSync(namespacePath)):null;
    const namespaceVerified=ns?.hostUid===config.uid && ns?.hostGid===config.gid && ns?.pid===1

      && ['user','net','mnt','pid'].every(k=>ns.namespaces?.[k] && ns.namespaces[k]!==config.hostNamespaces[k])
      && ns?.uidMap?.join(',')===[0,config.uid,1].join(',') && ns?.gidMap?.join(',')===[0,config.gid,1].join(',')
      && ns?.processLimit?.insideSandbox===true && ns?.privilegedHostEntry===false
      && report?.processLimits?.some(line=>/Max processes\s+4096\s+4096/.test(line));
    const entryNamespaceBound=entryBound && boot?.pidNamespace===ns?.namespaces?.pid;
    const cleanup=controller?.groupAbsent===true && report?.cleanup==='owned groups absent';
    const complete=entryNamespaceBound && namespaceVerified && unchanged() && controller?.exit?.code===0 && report?.terminal===true && cleanup
      && report.scenarios.length===1 && row?.status==='measured' && row.pass===true && row.acceptanceComplete===true;
    summary.scenarios.push({id,status:row?.status??'not_run',pass:complete,acceptanceComplete:complete,
      evidence:path,processExit:controller?.exit??null,controller:join(dir,'p6-controller.json'),
      daemonEntryEvidence:entryPath,entryBound,daemonExecuted:entryBound && /ctox v4 store daemon ready/.test(daemonLog),
      namespaceEvidence:namespacePath,namespaceVerified,entryNamespaceBound,criteria:row?.criteria??null,daemon:report?.daemon??null,
      reason:error??row?.reason??row?.error??report?.error??(!entryBound?'Real service entry was not verified':null),
      cleanup,sourceUnchanged:unchanged()});save();
  }
}
summary.terminal=true;summary.finishedAt=new Date().toISOString();
summary.pass=summary.scenarios.length===3 && summary.scenarios.every(row=>row.pass);save();
for(const row of summary.scenarios)console.log(row.id+' | '+(row.pass?'green':'red')+' | '+row.status+' | '+(row.evidence??join(output,'p6-result.json')));
process.exitCode=summary.pass?0:1;
