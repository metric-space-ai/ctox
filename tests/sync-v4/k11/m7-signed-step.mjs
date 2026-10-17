// Signed main-slot smoke; no alternate trust store or product data transport.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync,execFileSync} from 'node:child_process';
export async function performDressSmoke({config,root,output,env,cli,quiesce,phase}){
 const startedAtMs=Date.now(),out=join(output,'smoke-'+phase);fs.mkdirSync(out,{mode:0o700});
 const current=fs.realpathSync(join(root,'managed/current')),state=join(root,'runtime');
 const result={phase,startedAtMs,signedSlot:{status:'not_run',owner:'Shell v4/main release',reason:'No signed main slot exists yet; E37.2 forbids test-key substitution'},pass:false};
 try{
  let expectedRoot=join(current,'src/apps/business-os/v4/index.html');
  if(config.version){
   const stage=await cli(['business-os','shell-update','stage','--version',config.version],{allowFailure:true});
   result.signedSlot={status:stage.code===0?'staged':'fail',stageExit:stage.code,version:config.version};
   if(stage.code===0){
    // The product CLI has already verified signature + inventory. Bind the
    // actual selected signed manifest to the main artifact, never same-name bytes.
    const slot=fs.realpathSync(join(state,'business-os-shell/slots',config.version));
    if(!slot.startsWith(resolve(root)+'/'))throw Error('Slot escapes private fixture');
    const manifest=JSON.parse(fs.readFileSync(join(slot,'.ctox-shell-release.v2.json')));
    if(manifest.sourceCommit!==(config.slotSourceSha||config.mainSha)||manifest.version!==config.version)throw Error('Signed slot/main daemon source mismatch');
    result.signedSlot.sourceCommit=manifest.sourceCommit;result.signedSlot.artifactSha256=manifest.artifact.sha256;
    if(config.slotSha256&&manifest.artifact.sha256!==config.slotSha256)throw Error('Signed slot archive checksum differs from pinned official asset');
    const active=await cli(['business-os','shell-update','activate'],{allowFailure:true});
    result.signedSlot.activateExit=active.code;result.signedSlot.status=active.code===0?'pass':'fail';
    if(active.code===0)expectedRoot=join(slot,'v4/index.html');
   }
  }
  // HttpSurface chooses its immutable asset root at startup. Restart only this
  // namespace's real service after official activation, never a host service.
  quiesce();result.serviceStartAtMs=Date.now();execFileSync('systemctl',['--user','start','ctox.service'],{env,timeout:30000});
  env={...env,M7_SERVICE_START_AT:String(result.serviceStartAtMs),M7_EXPECT_SIGNALING_DEPLOY:config.signalingPending?'1':'0'};
  const wrapper=join(resolve(env.BUILD_LANE_BIN,'..'),'tools/with-playwright.sh');
  const command=[wrapper,'node',new URL('./m7-t4-smoke.mjs',import.meta.url).pathname,'--base','http://127.0.0.1:8765','--expected-root',expectedRoot,'--output',out,'--proof',join(output,'sandbox.json')];
  const log=fs.openSync(join(out,'wrapper.log'),'w',0o600);
  const child=spawnSync(command[0],command.slice(1),{env,stdio:['ignore',log,log],timeout:95000,detached:true});fs.closeSync(log);
  if(child.pid){result.browserProcess={pid:child.pid,pgid:child.pid,stop:'smoke terminal or95s timeout'};try{process.kill(-child.pid,'SIGKILL');}catch(e){if(e.code!=='ESRCH')throw e;}}
  result.smokeExit=child.status;result.smokeError=child.error?.message;
  if(fs.existsSync(join(out,'smoke.json')))result.smoke=JSON.parse(fs.readFileSync(join(out,'smoke.json')));
  result.pass=result.signedSlot.status==='pass'&&result.smoke?.pass===true;
 }catch(error){result.error=error.message;}
 finally{result.durationMs=Date.now()-startedAtMs;result.terminal=true;fs.writeFileSync(join(out,'step.json'),JSON.stringify(result,null,2),{mode:0o600});}
 return result;
}
