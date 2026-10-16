// Real foreground + E38 browser entry. No V3 SQL/readiness/bootstrap path.
import fs from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createRequire} from 'node:module';
import {Native,pause} from '../../sync-v4-e2e/native.mjs';
import {installServiceEntry} from './service-entry.mjs';
import {installBrowserEntry} from './browser-entry.mjs';
const config=JSON.parse(fs.readFileSync(process.argv[2]));
if(!config.canonicalTmpdir || !fs.statSync(config.canonicalTmpdir).isDirectory())throw Error('Canonical lane TMPDIR required');
process.env.TMPDIR=config.canonicalTmpdir;
installServiceEntry(Native);
const native=new Native(config),contexts=[],bindings=new WeakMap();
const scenario=(await import(pathToFileURL(join(config.scenarioRoot,config.scenario.toLowerCase(),'scenario.mjs')))).default;
const row={id:scenario.id,criterion:scenario.criterion,status:'not_run',pass:false,acceptanceComplete:false};
const report={owner:config.owner,revision:config.revision,binarySha256:config.binarySha256,netem:config.netem,
  startedAt:new Date().toISOString(),processLimits:fs.readFileSync('/proc/self/limits','utf8').split('\n').filter(line=>line.startsWith('Max processes')),
  scenarios:[row],terminal:false,pass:false,browserEntries:[]};
const save=()=>fs.writeFileSync(join(config.output,'result.json'),JSON.stringify(report,null,2));
let browser,interrupted=false;
process.once('SIGTERM',()=>{interrupted=true;void native.close();});
const deadline=setTimeout(()=>{interrupted=true;void native.close();},config.durationSeconds*1000);
save();
try {
  report.fixture=await native.prepare();save();
  if(config.prepareOnly){row.reason='not_run(branches-pending): native enrollment prepared; criteria not started';}else {
  native.start('signaling',process.execPath,[join(config.source,'src/core/rxdb/tools/local_signaling_server.js'),String(config.signalingPort)],
    {...process.env,SIGNALING_HOST:'127.0.0.1',SIGNALING_PORT:String(config.signalingPort)});
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
  const require=createRequire(join(config.modulesRoot,'playwright/package.json'));
  const {chromium}=require('playwright');
  browser=await chromium.launch({executablePath:config.chrome,headless:true,timeout:30000,args:['--disable-gpu','--no-sandbox']});
  const page=async(identity=native.fixture.identities.find(i=>i.name==='admin'))=>{
    const enrolled=native.fixture.identities.find(i=>i.id===identity.id);
    if(!enrolled?.cookie)throw Error('Server-issued fixture cookie required before navigation');
    const context=await browser.newContext();contexts.push(context);
    await context.addCookies([{name:'ctox_business_os_auth',value:enrolled.cookie,url:origin,httpOnly:true,sameSite:'Strict'}]);
    const connect=await installBrowserEntry(context,origin);
    const p=await context.newPage();bindings.set(p,()=>connect(p,enrolled,native.fixture));
    const response=await p.goto(origin+'/',{waitUntil:'domcontentloaded',timeout:20000});
    report.browserEntries.push({userId:enrolled.id,path:'/',status:response?.status()});save();
    return p;
  };
  const h={config,page,newClient:page,pause,offline:(p,value)=>p.context().setOffline(value),
    observe:(phase,measured)=>{row.phase=phase;(row.observations??=[]).push({phase,...measured});save();},
    os:async p=>{const value=await bindings.get(p)();report.browserEntries.at(-1).proof=value.proof;save();return value;}};
  Object.assign(row,await scenario.run(h));
  }
} catch(error) {
  // Generated fixture tokens and launch contexts are never included in reports.
  const safe=String(error.message).replace(/(?:https?|wss?):\/\/\S+/g,'[endpoint]').slice(0,600);
  report.error=safe;row.reason=safe;
} finally {
  if(row.status!=='measured') {
    const reason=row.reason??row.error??report.error??'Native scenario control unavailable';
    const category=reason.match(/not_run\(([^)]+)\)/)?.[1]??'native-browser-entry';
    row.criteria=Object.fromEntries(Object.entries(scenario.criterion).map(([name,target])=>[name,{target,actual:null,status:row.status==='failed'?'not_measured(scenario_failed)':'not_run('+category+')'}]));
  }
  const space=path=>{const s=fs.statfsSync(path);return {capacityBytes:s.blocks*s.bsize,availableBytes:s.bavail*s.bsize};};
  report.storage={privateTmp:space('/tmp'),fixtureRoot:fs.existsSync(config.root)?space(config.root):null};
  clearTimeout(deadline);
  for(const context of contexts)await context.close().catch(()=>{});
  await browser?.close().catch(()=>{});
  try{await native.close();report.cleanup='owned groups absent';}catch{report.cleanup='owned group cleanup failed';}
  for(const file of [config.fixturePrivate])if(file)fs.rmSync(file,{force:true});
  report.terminal=true;report.finishedAt=new Date().toISOString();
  report.pass=row.status==='measured'&&row.pass&&row.acceptanceComplete&&report.cleanup==='owned groups absent';
  save();
}
console.log(config.scenario,row.status,row.reason??row.error??report.error??'');
process.exitCode=report.pass?0:1;
