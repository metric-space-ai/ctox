// Real foreground + E38 browser entry. No V3 SQL/readiness/bootstrap path.
import fs from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createRequire} from 'node:module';
import {Native,pause} from '../../sync-v4-e2e/native.mjs';
import {installServiceEntry} from './service-entry.mjs';
import {nativeControl} from './control-client.mjs';
import {installBrowserEntry} from './browser-entry.mjs';
const config=JSON.parse(fs.readFileSync(process.argv[2]));
if(!config.canonicalTmpdir || !fs.statSync(config.canonicalTmpdir).isDirectory())throw Error('Canonical lane TMPDIR required');
process.env.TMPDIR=config.canonicalTmpdir;
installServiceEntry(Native);
const native=new Native(config),contexts=[],bindings=new WeakMap(),principals=new WeakMap();
const scenario=(await import(pathToFileURL(join(config.scenarioRoot,config.scenario.toLowerCase(),'scenario.mjs')))).default;
const row={id:scenario.id,criterion:scenario.criterion,status:'not_run',pass:false,acceptanceComplete:false};
const report={bootstrapResponses:[],browserErrors:[],owner:config.owner,revision:config.revision,binarySha256:config.binarySha256,netem:config.netem,
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
  const call=nativeControl(config),admin=native.fixture.identities.find(i=>i.name==='admin');
  const probeId='p6-probe-control';
  const committed=await call(admin,'writeProbe',{id:probeId,n:17});
  const stored=await call(admin,'readNative',{collection:'p6_control_rows',id:probeId});
  if(committed.source!=='owner-writer-commit' || stored.rows.length!==1 || stored.rows[0].n!==17)throw Error('Native owner control positive proof failed');
  let denied=false;try{await call({...admin,cookie:'p6-unissued'},'readNative',{collection:'p6_control_rows'});}catch(e){denied=e.message==='control request denied';}
  if(!denied)throw Error('Native control admitted an unissued session');
  let userDenied=false;try{await call(native.fixture.identities.find(i=>i.name==='user'),'writeProbe',{id:probeId,n:99});}catch(e){userDenied=e.message==='control request denied';}
  if(!userDenied)throw Error('Native control admitted a non-admin fixture write');
  const afterDenial=await call(admin,'readNative',{collection:'p6_control_rows',id:probeId});
  if(afterDenial.rows.length!==1 || afterDenial.rows[0].n!==17)throw Error('Denied control changed the stored probe');
  const beforeRights=await call(admin,'readRights');
  let staleRejected=false;try{await call(admin,'setRights',{expected_ver:beforeRights.rights_ver+1,rules:beforeRights.rules});}catch(e){staleRejected=e.message==='control request denied';}
  const afterStale=await call(admin,'readRights');
  if(!staleRejected || afterStale.rights_ver!==beforeRights.rights_ver || JSON.stringify(afterStale.rules)!==JSON.stringify(beforeRights.rules))throw Error('Stale fixture rights CAS changed policy');
  const pressure=await call(admin,'startWrites',{collection:'p6_control_rows',intervalMs:25,stopAfterMs:5000});
  let updated,pressureDone;
  try {
    updated=await call(admin,'setRights',{expected_ver:beforeRights.rights_ver,rules:beforeRights.rules});
    await pause(250);
  } finally {pressureDone=await call(admin,'stopWrites',{owner:pressure.owner});}
  if(pressureDone.committed<2 || !updated || BigInt(pressureDone.first_seq)>=BigInt(updated.seq) || BigInt(pressureDone.last_seq)<=BigInt(updated.seq))throw Error('Actual native load did not bracket the rights commit');
  report.control={ownerBacked:true,writeReadback:true,nonAdminWriteRejected:userDenied,deniedWriteUnchanged:true,
    staleRightsCasRejected:staleRejected,rights:{before:beforeRights.rights_ver,after:updated.rights_ver,seq:updated.seq},
    pressure:pressureDone,seq:committed.seq,unissuedSessionRejected:denied,process:await call(admin,'processIdentity')};
  save();
  if(config.controlOnly)throw Error('not_run(control-qualification-only)');
  try {
    const shell=await call(admin,'sourceRelease',{appId:'shell'});
    report.shellProvisioning={source:shell.source,appId:shell.appId,files:shell.files,status:shell.receipt.status,appVer:shell.receipt.app_ver};
  } catch(error) {
    report.shellProvisioning={status:'not_run',reason:error.message};
  }
  save();
  const origin='http://127.0.0.1:'+config.port;
  const require=createRequire(join(config.modulesRoot,'playwright/package.json'));
  const {chromium}=require('playwright');
  // No executablePath/channel override: Playwright selects its pinned Chromium.
  const executable=chromium.executablePath();
  if(!fs.existsSync(executable))throw Error('not_run(playwright-chromium): bundled Chromium is not installed');
  report.browser={kind:'playwright-chromium',executable};save();
  browser=await chromium.launch({headless:true,timeout:30000,args:['--disable-gpu','--no-sandbox']});
  const page=async(identity=native.fixture.identities.find(i=>i.name==='admin'))=>{
    const enrolled=native.fixture.identities.find(i=>i.id===identity.id);
    if(!enrolled?.cookie)throw Error('Server-issued fixture cookie required before navigation');
    const context=await browser.newContext();contexts.push(context);
    await context.addCookies([{name:'ctox_business_os_auth',value:enrolled.cookie,url:origin,httpOnly:true,sameSite:'Strict'}]);
    // bootV4 alone fetches the single-use E50 launch ticket.
    const connect=await installBrowserEntry(context,origin);
    const p=await context.newPage();
    const bootstrapPath=path=>path==='/api/business-os/launch-context' || path==='/sw.js' || path.startsWith('/kernel/') || path.startsWith('/business-os/');
    p.on('response',response=>{const url=new URL(response.url());if(url.origin===origin && bootstrapPath(url.pathname) && report.bootstrapResponses.length<250){report.bootstrapResponses.push({userId:enrolled.id,path:url.pathname,status:response.status()});save();}});
    p.on('requestfailed',request=>{const url=new URL(request.url());if(url.origin===origin && bootstrapPath(url.pathname) && report.bootstrapResponses.length<250){report.bootstrapResponses.push({userId:enrolled.id,path:url.pathname,failed:request.failure()?.errorText});save();}});
    const browserError=(kind,message)=>{if(report.browserErrors.length<50){report.browserErrors.push({userId:enrolled.id,kind,message:String(message).replace(/(?:https?|wss?):\/\/\S+/g,'[endpoint]').replace(/[A-Za-z0-9_-]{32,}/g,'[long-value]').slice(0,500)});save();}};
    p.on('pageerror',error=>browserError(error.name,error.message));
    p.on('console',message=>{if(message.type()==='error')browserError('console',message.text());});
    principals.set(p,enrolled);
    await p.exposeBinding('__p6NativeControl',(_source,{operation,args})=>call(principals.get(p),operation,args));
    bindings.set(p,()=>connect(p,principals.get(p),native.fixture));
    const response=await p.goto(origin+'/',{waitUntil:'domcontentloaded',timeout:20000});
    report.browserEntries.push({userId:enrolled.id,path:'/',status:response?.status()});save();
    return p;
  };
  const reauthenticate=async(p,identity)=>{
    const next=native.fixture.identities.find(i=>i.id===identity.id);
    if(!next?.cookie)throw Error('Server-issued replacement session required');
    // This awaited barrier precedes BOTH the credential change and the next launch.
    const retired=await p.evaluate(async()=>{
      const b=globalThis.__p6Boot;if(!b)throw Error('No admitted identity to retire');
      const old={userId:b.kernel.userId,role:b.launchContext.actor.role};
      await b.kernel.logout();await b.loader.close();
      const channels=[...new Set(Object.values(b.transport.channels??{}))];
      await Promise.all(channels.map(c=>c.readyState==='closed'?Promise.resolve():new Promise((resolve,reject)=>{
        const closed=()=>{clearTimeout(timer);c.removeEventListener('close',closed);resolve();};
        const timer=setTimeout(()=>{c.removeEventListener('close',closed);reject(Error('Old peer subscription close deadline'));},5000);
        c.addEventListener('close',closed);if(c.readyState==='closed')closed();
      })));
      if(!b.transport.closed || channels.some(c=>c.readyState!=='closed'))throw Error('Old peer subscriptions remain active');
      return {...old,transportClosed:true,channelsClosed:true};
    });
    report.identityHandoffs??=[];report.identityHandoffs.push({...retired,nextUserId:next.id,nextRole:next.role,retractedBeforeCredentials:true});save();
    await p.context().clearCookies();
    await p.context().addCookies([{name:'ctox_business_os_auth',value:next.cookie,url:origin,httpOnly:true,sameSite:'Strict'}]);
    principals.set(p,next);
    await p.goto(origin+'/',{waitUntil:'domcontentloaded',timeout:20000});
    return bindings.get(p)();
  };
  const h={config,page,newClient:page,reauthenticate,pause,offline:(p,value)=>p.context().setOffline(value),
    observe:(phase,measured)=>{row.phase=phase;(row.observations??=[]).push({phase,...measured});save();},
    os:async p=>{try{
      let value=await bindings.get(p)();
      // Qualify one actual admin -> other-role -> admin handoff before fixture
      // prepare can short-circuit the scenario. Every launch gets a new ticket.
      if(!report.identityHandoffs?.length){
        const original=principals.get(p),other=native.fixture.identities.find(i=>i.role!==original.role);
        if(!other)throw Error('A second real role is required for handoff qualification');
        await reauthenticate(p,other);value=await reauthenticate(p,original);
      }
      report.browserEntries.at(-1).proof=value.proof;save();return value;
    }finally{report.browserEntries.at(-1).boot=await p.evaluate(()=>({called:!!globalThis.__p6BootOptions,completed:!!globalThis.__p6Boot,error:globalThis.__p6BootError??null})).catch(()=>({pageClosed:true}));save();}}};
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
