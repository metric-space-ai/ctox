// Narrow continuation of a terminal, owned rehearsal; never repeats green recovery.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
const [prior,directory,activeReceipt]=process.argv.slice(2),base=resolve(prior),out=resolve(directory);
if(process.platform!=='linux'||!process.env.BUILD_LANE_BIN)throw Error('Canonical Linux lane required');
const allowed=resolve(process.env.BUILD_LANE_BIN,'../artifacts');
if(!base.startsWith(allowed+'/')||!out.startsWith(allowed+'/')||fs.existsSync(out))throw Error('Owned existing prior/new output required');
const previous=join(base,'rehearsal/upgrade'),controller=JSON.parse(fs.readFileSync(join(previous,'controller.json'))),manager=JSON.parse(fs.readFileSync(join(previous,'service-manager.json')));
if(!controller.groupAbsent||!controller.network?.childTerminal||!manager.childrenTerminal||!manager.privateCgroupRemoved)throw Error('Prior private service/network cleanup not verified');
try{process.kill(-controller.pgid,0);throw Error('Prior sandbox still exists');}catch(e){if(e.code!=='ESRCH')throw e;}
const c=JSON.parse(fs.readFileSync(join(previous,'sandbox.json')));
const smoke={...JSON.parse(fs.readFileSync(join(base,'signed-smoke.json'))),dataReference:join(previous,'data-before.json')};
if(activeReceipt){
 const file=resolve(activeReceipt);if(!file.startsWith(allowed+'/'))throw Error('Private activation receipt required');
 const r=JSON.parse(fs.readFileSync(file)),full=file===join(base,'result.json');
 const selected=full?r.afterForward:r.signedSmoke;
 const done=JSON.parse(fs.readFileSync(full?join(previous,'controller.json'):join(resolve(file,'..'),'probe/controller.json')));
 const sameProfile=full?r.lifecyclePass===true&&r.roundtripExecuted===true&&r.dataPreserved===true&&r.mainSha===smoke.mainSha:r.prior===base;
 if(!sameProfile||selected?.signedSlot?.status!=='pass'||selected.signedSlot.artifactSha256!==smoke.slotSha256||selected.signedSlot.sourceCommit!==smoke.slotSourceSha||!done.groupAbsent||!done.network?.childTerminal)throw Error('Prior active slot or terminal controller custody differs');
 smoke.verifiedActiveReceipt=file;
}
if(!smoke.version||!smoke.slotSha256)throw Error('Real signed slot config required');
fs.mkdirSync(out,{mode:0o700});
const config=join(out,'signed-smoke.json');fs.writeFileSync(config,JSON.stringify(smoke),{mode:0o600});
const child=spawnSync(process.execPath,[new URL('./upgrade-controller.mjs',import.meta.url).pathname,c.oldReceipt,c.newReceipt,c.root,join(out,'probe'),'no','resume-slot',config],{stdio:'inherit',timeout:11*60*1000,env:{...process.env,M7_NAMESPACE_MODE:c.mappedUser?'unprivileged':'privileged'}});
const receipt=join(out,'probe/installer.json'),r=fs.existsSync(receipt)?JSON.parse(fs.readFileSync(receipt)):{};
const required=['entry','service-worker','kernel','entry-p99'];
const criteria=r.signedSmoke?.smoke?.criteria||[];
const staticPass=required.every(id=>criteria.some(x=>x.id===id&&x.status==='pass'));
const allowedPending=criteria.every(x=>x.status==='pass'||(x.status==='not_run'&&x.reason==='signaling-deploy'));
const result={scope:'Signed slot activation, static HTTP and preserved synthetic rows; no final M7 acceptance',staticPass,fullAcceptancePass:r.signedSmoke?.pass===true,schema:'ctox.m7.slot-continuation.v1',measuredAt:new Date().toISOString(),prior:base,controllerExit:child.status,signedSmoke:r.signedSmoke,dataReadback:r.dataReadback,pass:staticPass&&allowedPending&&r.signedSmoke?.commandNativeReceipt?.status!=='fail'&&child.status===0&&r.signedSmoke?.signedSlot?.status==='pass'&&r.dataReadback?.matchesBefore===true};
fs.writeFileSync(join(out,'result.json'),JSON.stringify(result,null,2),{mode:0o600});
console.log(JSON.stringify({pass:result.pass,slot:result.signedSmoke?.signedSlot,controllerExit:child.status}));
process.exitCode=result.pass?0:2;
