// One bounded entry point for the E37.2 signed-main private rehearsal.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {verifiedBuild} from '../../sync-v4-e2e/native.mjs';
import {extraRecoveryComplete} from './m7-extra-recovery.mjs';
const opt=n=>{const i=process.argv.indexOf('--'+n);return i<0?null:process.argv[i+1];};
const dry=process.argv.includes('--candidate-dry-run'),candidateSigned=process.argv.includes('--candidate-with-signed-slot');
if(dry&&candidateSigned)throw Error('Choose one rehearsal mode');
const slotSource=opt('slot-source-sha'),slotSha=opt('slot-sha256'),signalingPending=process.argv.includes('--expect-signaling-deploy');
const extraRecovery=process.argv.includes('--extra-recovery-roundtrip');
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const old=opt('old-receipt'),dest=opt('output'),mainSha=opt('main-sha'),tag=opt('signed-slot-tag');
let release=opt('daemon-release');const archive=opt('daemon-artifact');
if(!old||!dest||(!release&&!archive)||(release&&archive))throw Error('Need --old-receipt --output and either --daemon-release receipt or --daemon-artifact archive');
const out=resolve(dest),allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts');
if(!out.startsWith(allowed+'/')||fs.existsSync(out))throw Error('New private artifact output required');
if(Buffer.byteLength(join(out,'rehearsal/tenant-m7/runtime/store/store.sock'))>103)throw Error('Choose a shorter private output prefix for Unix store IPC');
fs.mkdirSync(out,{mode:0o700});
if(archive){
 const tarSha=opt('artifact-sha256'),binSha=opt('binary-sha256');
 if(!tarSha||!binSha||!mainSha)throw Error('Published archive/binary checksums and main source SHA required');
 const p=spawnSync('python3',[new URL('./m7-artifact-receipt.py',import.meta.url).pathname,'--archive',archive,'--artifact-sha256',tarSha,'--binary-sha256',binSha,'--main-sha',mainSha,'--output',join(out,'daemon')],{encoding:'utf8',timeout:180000});
 if(p.status!==0)throw Error('Published native artifact custody failed: '+p.stderr);
 release=join(out,'daemon/release.json');
}
const artifact=verifiedBuild(release);
if(!artifact.package||!artifact.packageSha256||!artifact.reference?.includes('release.yml'))throw Error('Canonical native package/receipt required');
let version=null;
if(!dry){
 if(!/^[a-f0-9]{40}$/.test(mainSha||'')||artifact.revision!==mainSha)throw Error('Main artifact exact source required');
 // Lane mirrors can have an old local main while the pinned candidate object
 // is current. Witness the official main ref without mutating shared refs.
 const official='https://github.com/metric-space-ai/ctox.git';
 const witness=spawnSync('git',['ls-remote','--exit-code',official,'refs/heads/main'],{encoding:'utf8',timeout:30000});
 const mainRef=/^([a-f0-9]{40})\s+refs\/heads\/main$/m.exec(witness.stdout||'')?.[1];
 if(witness.status!==0||!mainRef)throw Error('Cannot witness official main source');
 if(spawnSync('git',['cat-file','-e',mainRef+'^{commit}'],{stdio:'ignore'}).status!==0){
  const fetch=spawnSync('git',['fetch','--no-write-fetch-head','--filter=blob:none',official,mainRef],{stdio:'ignore',timeout:90000});
  if(fetch.status!==0)throw Error('Cannot obtain witnessed main ancestry objects');
 }
 fs.writeFileSync(join(out,'main-source-witness.json'),JSON.stringify({observedAtMs:Date.now(),repository:official,officialMainSha:mainRef,sharedRefsChanged:false,slotSource:slotSource||mainSha},null,2),{mode:0o600});
 if(!mainRef||(!candidateSigned&&spawnSync('git',['merge-base','--is-ancestor',mainSha,mainRef],{stdio:'ignore'}).status!==0))throw Error('Given native source is not on the canonical fetched main ref');
 if(candidateSigned&&(!/^[a-f0-9]{40}$/.test(slotSource||'')||spawnSync('git',['merge-base','--is-ancestor',slotSource,mainRef],{stdio:'ignore'}).status!==0||!/^[a-f0-9]{64}$/.test(slotSha||'')))throw Error('Candidate requires an explicit signed-main slot source and checksum');
 if(!candidateSigned&&slotSource&&slotSource!==mainSha)throw Error('Independent slot source requires explicit private candidate mode');
 version=/^business-os-shell-v(0\.2\.0-beta\.\d+)$/.exec(tag||'')?.[1];if(!version)throw Error('Real signed0.2.0-beta.N release tag required');
}else if(tag)throw Error('Candidate dry-run cannot claim a signed main slot');
const verified=spawnSync('python3',['-c',"import importlib.util,json,sys;s=importlib.util.spec_from_file_location('release','tests/sync-v4/k11/m7-release.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m);print(json.dumps(m.verify_package(json.load(open(sys.argv[1])))))",release],{encoding:'utf8',timeout:180000});if(verified.status!==0)throw Error('Native release inventory/content verification failed: '+verified.stderr);
const config=join(out,'signed-smoke.json');fs.writeFileSync(config,JSON.stringify({mainSha:mainSha||artifact.revision,version,releaseTag:tag||null,candidateDryRun:dry,candidateSigned,slotSourceSha:slotSource||mainSha,slotSha256:slotSha,signalingPending,extraRecoveryRoundtrip:extraRecovery}),{mode:0o600});
// Host policy selects only the existing isolated setup, never host services.
let namespaceMode=opt('namespace-mode')||process.env.M7_NAMESPACE_MODE||'auto';
if(namespaceMode==='auto'){
 const policy='/proc/sys/kernel/apparmor_restrict_unprivileged_userns';
 const restricted=fs.existsSync(policy)&&fs.readFileSync(policy,'utf8').trim()==='1';
 namespaceMode=restricted&&spawnSync('sudo',['-n','true'],{stdio:'ignore',timeout:3000}).status===0?'privileged':'unprivileged';
}
if(!['privileged','unprivileged'].includes(namespaceMode))throw Error('Unknown private namespace setup mode');
const log=fs.openSync(join(out,'driver.log'),'w',0o600),at=Date.now();
const child=spawnSync(process.execPath,[new URL('./installer-smoke.mjs',import.meta.url).pathname,old,release,join(out,'rehearsal'),'yes',config],{stdio:['ignore',log,log],timeout:(extraRecovery?16:12)*60*1000,env:{...process.env,M7_NAMESPACE_MODE:namespaceMode}});fs.closeSync(log);
const resultFile=join(out,'rehearsal/result.json'),record=fs.existsSync(resultFile)?JSON.parse(fs.readFileSync(resultFile)):{};
const result={schema:'ctox.m7.signed-dress.v1',startedAtMs:at,terminal:true,candidateDryRun:dry,candidateSigned,signalingPending,namespaceMode,mainSha:mainSha||artifact.revision,releaseTag:tag||null,controllerExit:child.status,controllerError:child.error?.message,durationMs:Date.now()-at,installerExecuted:record.installer?.installerExecuted===true,completedInstallerTransition:record.installerExecuted===true,roundtripExecuted:record.roundtripExecuted===true,dataPreserved:record.dataPreserved===true,afterUpdate:record.installer?.signedSmoke,afterForward:record.installer?.forward?.signedSmoke,installerFailure:record.error||record.failure?.error,installerFailureDetail:record.installer?.calls?.findLast(call=>call.code!==0)?.failure};
result.pass=!dry&&!candidateSigned&&!signalingPending&&child.status===0&&result.installerExecuted&&result.roundtripExecuted&&result.dataPreserved&&result.afterUpdate?.pass===true&&result.afterForward?.pass===true;
result.extraRecoveryRequested=extraRecovery;result.extraRecovery=record.installer?.secondRecovery;result.afterExtraForward=record.installer?.secondRecovery?.forward?.signedSmoke;
result.providerGatewaySmoke=record.installer?.providerGatewaySmoke;
result.lifecyclePass=child.status===0&&result.installerExecuted&&result.roundtripExecuted&&result.dataPreserved&&(!extraRecovery||extraRecoveryComplete(record.installer));
const required=['entry','service-worker','kernel','entry-p99'];
result.preparationPass=result.lifecyclePass&&[result.afterUpdate,result.afterForward,...(extraRecovery?[result.afterExtraForward]:[])].every(step=>step&&(!version||step.signedSlot?.status==='pass')&&required.every(id=>step.smoke?.criteria?.some(c=>c.id===id&&c.status==='pass'))&&step.smoke.criteria.every(c=>c.status==='pass'||(signalingPending&&c.status==='not_run'&&c.reason==='signaling-deploy'))&&(!step.commandNativeReceipt||step.commandNativeReceipt.status!=='fail'));
result.pass=result.pass&&result.preparationPass;
fs.writeFileSync(join(out,'result.json'),JSON.stringify(result,null,2),{mode:0o600});
console.log(JSON.stringify({pass:result.pass,preparationPass:result.preparationPass,candidateDryRun:dry,durationMs:result.durationMs,installerFailure:result.installerFailure}));process.exitCode=result.pass?0:((dry||candidateSigned||signalingPending)&&result.preparationPass?0:2);
