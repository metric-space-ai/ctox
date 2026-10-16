// Real installer entry point, deliberately unavailable outside a private sandbox.
// No host service command, hand-written schema DDL or restart-as-upgrade fallback.
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {createServer} from 'node:http';
import {spawn,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {verifiedBuild,owner} from '../../sync-v4-e2e/native.mjs';
const hash=b=>createHash('sha256').update(b).digest('hex');
export function verifiedUpgrade(oldReceipt,newReceipt,root){
 const before=verifiedBuild(oldReceipt),after=verifiedBuild(newReceipt);
 if(before.revision===after.revision||before.binarySha256===after.binarySha256)throw Error('K11 requires two distinct verified binaries');
 const mark=JSON.parse(fs.readFileSync(join(root,'.sync-v4-e2e.json')));
 if(mark.owner!==owner||mark.synthetic!==true||!resolve(root).includes('/artifacts/'))throw Error('Only an owned synthetic acceptance prefix may be upgraded');
 return {before,after};
}
export function requireSandbox(proof){
 for(const key of ['net','mnt','pid'])if(!proof?.host?.[key]||proof.host[key]===fs.readlinkSync('/proc/self/ns/'+key))throw Error('Private '+key+' namespace required');
 if(!proof.runtimeMasked||fs.existsSync('/run/dbus/system_bus_socket'))throw Error('Host service bus must be inaccessible');
 if(fs.existsSync('/run/user/'+process.getuid()+'/bus')){
  if(!proof.serviceManager||proof.namespaces.cgroup===proof.host.cgroup||proof.serviceManager.runtime!=='/run/user/'+process.getuid())throw Error('Only the real private service-manager bus is permitted');
 }
 if(process.getuid()===0)throw Error('Installer runs as the fixture owner, not root');
}
export async function install({oldReceipt,newReceipt,root,output,proof,tmpdir,roundtrip=false,resumeRolledBack=false}){
 requireSandbox(proof);if(!tmpdir||!fs.statSync(tmpdir).isDirectory())throw Error('Canonical lane TMPDIR required');const {before,after}=verifiedUpgrade(oldReceipt,newReceipt,root);
 const installRoot=join(root,'managed'),stateRoot=join(root,'runtime');
 const home=join(root,'installer-home');fs.mkdirSync(home,{recursive:true,mode:0o700});
 const oldRelease='native-e2e-'+before.revision.slice(0,12),release='native-e2e-'+after.revision.slice(0,12);
 const releases=join(installRoot,'releases');fs.mkdirSync(releases,{recursive:true});
 // Initial fixture layout only. The transition below is the actual supported
 // update apply path, including its own backup, maintenance and link switch.
 const initial=join(releases,oldRelease);
 if(resumeRolledBack){
  const state=JSON.parse(fs.readFileSync(join(stateRoot,'update_state.json')));
  if(state.phase!=='rolled_back'||fs.realpathSync(join(installRoot,'current'))!==initial||hash(fs.readFileSync(join(initial,'bin/ctox')))!==before.binarySha256)throw Error('Recovery must start from the actual verified rolled-back state');
 }else{
  fs.mkdirSync(initial);
  for(const part of ['src','contracts','Cargo.toml'])fs.symlinkSync(join(before.source,part),join(initial,part));
  fs.mkdirSync(join(initial,'bin'));fs.copyFileSync(before.binary,join(initial,'bin/ctox'));
  fs.symlinkSync(stateRoot,join(initial,'runtime'));fs.symlinkSync(initial,join(installRoot,'current'));
  fs.writeFileSync(join(installRoot,'install_manifest.json'),JSON.stringify({schema_version:1,install_root:installRoot,state_root:stateRoot,current_release:oldRelease,previous_release:null,adopted_from:null,release_channel:null,updated_at:new Date().toISOString()}));
 }
 let archive,archiveSha256;
 if(after.package){
  archive=resolve(after.package);archiveSha256=hash(fs.readFileSync(archive));
  if(archiveSha256!==after.packageSha256||!after.reference?.includes('release.yml'))throw Error('Canonical release package custody differs');
 }else{
 const bundle=join(output,'bundle');fs.mkdirSync(bundle);
 const manifest=fs.readFileSync(join(after.source,'contracts/binary_bundle_manifest.txt'),'utf8');
 for(const path of ['src/apps/business-os','contracts']){const dest=join(bundle,path);fs.mkdirSync(resolve(dest,'..'),{recursive:true});fs.cpSync(join(after.source,path),dest,{recursive:true});}
 const paths=manifest.split('\n').map(x=>x.trim()).filter(x=>x&&!x.startsWith('#'));
 for(const relative of paths){const src=resolve(after.source,relative);if(!src.startsWith(after.source+'/')||!fs.existsSync(src))throw Error('Verified installer bundle missing '+relative);const dest=join(bundle,relative);fs.mkdirSync(resolve(dest,'..'),{recursive:true});if(!fs.existsSync(dest))fs.cpSync(src,dest,{recursive:true});}
 fs.mkdirSync(join(bundle,'bin'));fs.copyFileSync(after.binary,join(bundle,'bin/ctox'));fs.chmodSync(join(bundle,'bin/ctox'),0o700);
 archive=join(output,'ctox-linux-x64.tar.gz');execFileSync('tar',['-czf',archive,'-C',bundle,'.'],{timeout:180000});
 archiveSha256=hash(fs.readFileSync(archive));
 }
 let base;const requests=[];
 const server=createServer((req,res)=>{
  requests.push({path:req.url,at:new Date().toISOString()});
  if(req.url==='/ctox-linux-x64.tar.gz'){res.writeHead(200,{'content-type':'application/gzip'});fs.createReadStream(archive).pipe(res);}
  else if(req.url==='/ctox-linux-x64.tar.gz.sha256'){res.end(archiveSha256+'  ctox-linux-x64.tar.gz\n');}
  else if(req.url==='/repos/e2e/fixture/releases/tags/'+release){res.setHeader('content-type','application/json');res.end(JSON.stringify({tag_name:release,tarball_url:base+'/unused-source',prerelease:true,draft:false,assets:[{name:'ctox-linux-x64.tar.gz',browser_download_url:base+'/ctox-linux-x64.tar.gz'},{name:'ctox-linux-x64.tar.gz.sha256',browser_download_url:base+'/ctox-linux-x64.tar.gz.sha256'}]}));}
  else{res.statusCode=404;res.end();}
 });
 await new Promise(r=>server.listen(0,'127.0.0.1',r));base='http://127.0.0.1:'+server.address().port;
 // Explicit allowlist avoids ambient credentials, proxies and host service buses.
 const env={PATH:process.env.PATH,HOME:home,TMPDIR:tmpdir,LANG:'C.UTF-8',CTOX_ROOT:join(installRoot,'current'),CTOX_INSTALL_ROOT:installRoot,CTOX_STATE_ROOT:stateRoot,XDG_CONFIG_HOME:join(home,'.config'),XDG_CACHE_HOME:join(home,'.cache'),XDG_RUNTIME_DIR:proof.serviceManager?.runtime??join(home,'run')};if(proof.serviceManager)env.DBUS_SESSION_BUS_ADDRESS=proof.serviceManager.bus;fs.mkdirSync(env.XDG_RUNTIME_DIR,{recursive:true,mode:0o700});
 const calls=[];
 const serviceCalls=[];
 const quiesce=()=>{
  if(!proof.serviceManager)throw Error('Real private service manager required for recovery');
  for(const unit of ['ctox-watchdog.timer','ctox-watchdog.service','ctox.service']){
   const at=Date.now(),p=execFileSync('systemctl',['--user','stop',unit],{env,encoding:'utf8',timeout:30000});
   serviceCalls.push({args:['stop',unit],startedAtMs:at,durationMs:Date.now()-at,output:p.trim()});
  }
  const active=execFileSync('systemctl',['--user','show','ctox.service','--property=MainPID','--value'],{env,encoding:'utf8',timeout:10000}).trim();
  if(active!=='0')throw Error('Fixture daemon is not quiescent before rollback');
 };
 const cli=async args=>{
  const out=fs.openSync(join(output,'installer-'+calls.length+'.log'),'w',0o600),at=Date.now();
  // Recovery must execute the installed candidate's fixed lease code, rather
  // than continuing to invoke the historical producer after the link switch.
  const executable=fs.realpathSync(join(installRoot,'current/bin/ctox'));
  const executableSha256=hash(fs.readFileSync(executable));
  const p=spawn(executable,[...args,'--root',env.CTOX_ROOT],{cwd:env.CTOX_ROOT,env,stdio:['ignore',out,out]});fs.closeSync(out);
  const timer=setTimeout(()=>p.kill('SIGKILL'),180000);
  const code=await new Promise((r,j)=>{p.once('exit',r);p.once('error',j);});clearTimeout(timer);
  calls.push({args,executable,executableSha256,startedAtMs:at,durationMs:Date.now()-at,code});if(code!==0)throw Error('Real installer CLI failed: '+args.join(' ')+' exit '+code);
 };
 const record={before:{revision:before.revision,binarySha256:before.binarySha256},after:{revision:after.revision,binarySha256:after.binarySha256},stateRoot,archive,archiveSha256,canonicalReleasePackage:!!after.package,calls,serviceCalls,requests,resumeRolledBack,installerExecuted:false,migrationExecuted:false};
 const readback=phase=>{
  const file=join(output,'data-'+phase+'.json');
  const args=[new URL('./m7-data-readback.py',import.meta.url).pathname,'--root',root,'--output',file];
  if(phase!=='before')args.push('--expect',join(output,'data-before.json'));
  execFileSync('python3',args,{env:{...env,BUILD_LANE_BIN:process.env.BUILD_LANE_BIN},encoding:'utf8',timeout:30000});
  return {evidence:file,...JSON.parse(fs.readFileSync(file))};
 };
 try{
  record.dataBefore=readback('before');
  await cli(['update','status']);
  await cli(['update','channel','set-github','--repo','e2e/fixture','--api-base',base]);
  await cli(['update','apply','--version',release]);
  const current=fs.realpathSync(join(installRoot,'current'));
  const actual=hash(fs.readFileSync(join(current,'bin/ctox')));
  if(actual!==after.binarySha256||current!==join(releases,release))throw Error('Installer did not activate the verified second binary');
  record.installerExecuted=true;record.activeBinarySha256=actual;
  record.updateState=JSON.parse(fs.readFileSync(join(stateRoot,'update_state.json')));
  if(record.updateState.phase!=='completed'||!record.updateState.state_backup_path)throw Error('Installer did not complete with state backup');
  if(roundtrip){
   const startedAtMs=Date.now(),snapshot=record.updateState.state_backup_path;
   quiesce();
   await cli(['update','rollback']);
   const rolled=JSON.parse(fs.readFileSync(join(stateRoot,'update_state.json'))),oldCurrent=fs.realpathSync(join(installRoot,'current'));
   if(rolled.phase!=='rolled_back'||oldCurrent!==initial||hash(fs.readFileSync(join(oldCurrent,'bin/ctox')))!==before.binarySha256)throw Error('Supported rollback did not activate the verified old binary');
   record.rollback={startedAtMs,durationMs:Date.now()-startedAtMs,currentRelease:oldCurrent,binarySha256:before.binarySha256,phase:rolled.phase,stateBackupPath:snapshot,stateRestoreSemantics:'Official update rollback restores the recorded pre-update state; not binary-only'};
   quiesce();record.rollback.dataReadback=readback('rollback');
   const forwardAt=Date.now();quiesce();await cli(['update','apply','--version',release,'--force']);
   const finalCurrent=fs.realpathSync(join(installRoot,'current')),final=JSON.parse(fs.readFileSync(join(stateRoot,'update_state.json')));
   if(final.phase!=='completed'||finalCurrent!==join(releases,release)||hash(fs.readFileSync(join(finalCurrent,'bin/ctox')))!==after.binarySha256)throw Error('Forward re-install did not restore the verified candidate');
   record.forward={startedAtMs:forwardAt,durationMs:Date.now()-forwardAt,binarySha256:after.binarySha256,phase:final.phase};
   quiesce();record.forward.dataReadback=readback('forward');
   record.roundtripExecuted=true;record.finalUpdateState=final;
  }
  return record;
 }finally{await new Promise(r=>server.close(r));fs.writeFileSync(join(output,'installer.json'),JSON.stringify(record,null,2));}
}
