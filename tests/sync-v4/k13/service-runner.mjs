// P6 uses the real foreground root; no V3 readiness or SQL fixture path.
import fs from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createRequire} from 'node:module';
import {Native,pause} from '../../sync-v4-e2e/native.mjs';
import {installServiceEntry} from './service-entry.mjs';
const config=JSON.parse(fs.readFileSync(process.argv[2]));
if(config.osBootstrap && (!config.osBootstrap.startsWith('/') || config.osBootstrap.startsWith('//') || /[?#]/.test(config.osBootstrap)))
  throw Error('OS bootstrap must be an exact same-origin static module path');
if(!config.canonicalTmpdir || !fs.statSync(config.canonicalTmpdir).isDirectory())throw Error('Canonical lane TMPDIR required');
process.env.TMPDIR=config.canonicalTmpdir;
installServiceEntry(Native);
const native=new Native(config),contexts=[];
const scenario=(await import(pathToFileURL(join(config.scenarioRoot,config.scenario.toLowerCase(),'scenario.mjs')))).default;
const report={owner:config.owner,revision:config.revision,binarySha256:config.binarySha256,netem:config.netem,
  startedAt:new Date().toISOString(),processLimits:fs.readFileSync('/proc/self/limits','utf8').split('\n').filter(line=>line.startsWith('Max processes')),
  scenarios:[],terminal:false,pass:false};
const save=()=>fs.writeFileSync(join(config.output,'result.json'),JSON.stringify(report,null,2));
let browser,interrupted=false;
process.once('SIGTERM',()=>{interrupted=true;void native.close();});
const deadline=setTimeout(()=>{interrupted=true;void native.close();},config.durationSeconds*1000);
save();
try {
  await native.prepare();
  const at=Date.now();native.serve();
  const log=join(config.output,'daemon.log');
  for(let n=0;n<300;n++) {
    if(interrupted)throw Error('Scenario interrupted');
    if(native.daemon.exitCode!==null || native.daemon.signalCode!==null)throw Error('Foreground exited before ready');
    const text=fs.existsSync(log)?fs.readFileSync(log,'utf8'):'';
    if(/ctox v4 store daemon ready/.test(text)) {
      const entry=JSON.parse(fs.readFileSync(join(config.output,'p6-daemon-entry.json')));
      report.daemon={ready:true,startupMs:Date.now()-Date.parse(entry.at),verificationAndStartupMs:Date.now()-at,engineBindings:Number(text.match(/engine bindings=(\d+)/)?.[1]),
        pid:native.daemon.pid,pidNamespace:fs.readlinkSync('/proc/self/ns/pid')};break;
    }
    await pause(100);
  }
  if(!report.daemon?.ready)throw Error('Foreground readiness deadline');
  save();
  const origin='http://127.0.0.1:'+config.port;
  try {
    const response=await fetch(origin+'/api/business-os/sync/config',{signal:AbortSignal.timeout(2000)});
    report.bootstrap={reachable:true,status:response.status};
  } catch(error) {
    report.bootstrap={reachable:false,code:error.cause?.code??error.name};
  }
  const row={id:scenario.id,criterion:scenario.criterion,status:'not_run',pass:false,acceptanceComplete:false};
  report.scenarios.push(row);
  if(!report.bootstrap.reachable || !config.osBootstrap) {
    row.reason=(!report.bootstrap.reachable?'Real foreground has no listening browser bootstrap':'No native-backed OS scenario adapter supplied')+
      '; native browser/fixture transport must be bound before authenticated K measurements';
    row.blockedBeforePolicyEvaluation=true;
    row.criteria=Object.fromEntries(Object.entries(scenario.criterion).map(([name,target])=>[name,{target,actual:null,status:!report.bootstrap.reachable?'not_run(native-browser-listener)':'not_run(native-scenario-control)'}]));
    row.dependencies=[{file:'src/core/service/store_native.rs',line:77,reason:'Fresh tenant requires explicit native enrollment; no synthetic voter/authority'},
      {file:'src/core/service/store_startup.rs',line:147,reason:'CUTOVER-6 binds Store rights before engine admission; readiness is not browser admission'},
      {file:'src/core/service/store_inputs.rs',line:125,reason:'Native session is required, never fabricated'}];
  } else {
    native.start('signaling',process.execPath,[join(config.source,'src/core/rxdb/tools/local_signaling_server.js'),String(config.signalingPort)],
      {...process.env,SIGNALING_HOST:'127.0.0.1',SIGNALING_PORT:String(config.signalingPort)});
    const require=createRequire(join(config.modulesRoot,'playwright/package.json'));
    const {chromium}=require('playwright');
    browser=await chromium.launch({executablePath:config.chrome,headless:true,timeout:30000,args:['--disable-gpu','--no-sandbox']});
    const page=async()=>{const context=await browser.newContext();contexts.push(context);const p=await context.newPage();
      await p.goto(origin+'/index.html',{waitUntil:'domcontentloaded'});return p;};
    const h={config,page,newClient:page,pause,offline:(p,value)=>p.context().setOffline(value),
      observe:(phase,measured)=>{row.phase=phase;(row.observations??=[]).push({phase,...measured});save();},
      os:p=>p.evaluate(async path=>{const module=await import(path),os=await module.connectScenario();
        if(os.transportKind!=='native-webrtc'||!os.nativePeerId||!os.kernel?.shellApi)throw Error('Actual native OS binding required');
        globalThis.__e2eOS={...os,shell:os.kernel.shellApi()};return {available:true};},config.osBootstrap)};
    Object.assign(row,await scenario.run(h));
  }
} catch(error) {
  report.error=error.message;
  if(!report.scenarios.length)report.scenarios.push({id:scenario.id,status:'not_run',pass:false,acceptanceComplete:false,reason:error.message,
    criteria:Object.fromEntries(Object.entries(scenario.criterion).map(([name,target])=>[name,{target,actual:null,status:'not_run(native-entry)'}]))});
} finally {
  clearTimeout(deadline);
  for(const context of contexts)await context.close().catch(()=>{});
  await browser?.close().catch(()=>{});
  try{await native.close();report.cleanup='owned groups absent';}catch(error){report.cleanup=error.message;}
  report.terminal=true;report.finishedAt=new Date().toISOString();
  report.pass=report.scenarios.length===1&&report.scenarios.every(row=>row.status==='measured'&&row.pass&&row.acceptanceComplete)&&report.cleanup==='owned groups absent';
  save();
}
console.log(config.scenario,report.scenarios[0]?.status,report.scenarios[0]?.reason??report.error??'');
process.exitCode=report.pass?0:1;
