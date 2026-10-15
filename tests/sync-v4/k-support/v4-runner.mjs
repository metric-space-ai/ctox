// Actual daemon/browser boundary. An unavailable native V4 authority is an
// explicit dependency, never a fabricated Core or a legacy collection fallback.
import fs from 'node:fs';
import {join,resolve,extname} from 'node:path';
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {Native,until,owner} from '../../sync-v4-e2e/native.mjs';
import {schema,installRelay} from '../../sync-v4-e2e/browser.mjs';
import {probeNativeV4} from './v4-binding.mjs';
const c=JSON.parse(fs.readFileSync(process.argv[2]));
if(!c.canonicalTmpdir||!fs.statSync(c.canonicalTmpdir).isDirectory())throw Error('Canonical lane TMPDIR missing');
process.env.TMPDIR=c.canonicalTmpdir;
const result={owner,revision:c.revision,binarySha256:c.binarySha256,browserKernel:JSON.parse(fs.readFileSync(join(c.kernelAssets,'origin.json'))),harnessSource:c.sourceManifest,startedAt:new Date().toISOString(),phase:'seed',scenarios:[],pass:false,terminal:false,v4:true};
const save=()=>fs.writeFileSync(join(c.output,'result.json'),JSON.stringify(result,null,2));
const native=new Native(c);let browser;let shutdownPromise;
const close=()=>shutdownPromise??=(async()=>{await browser?.close().catch(()=>{});await native.close();})();

process.once('SIGTERM',()=>void close());
const timer=setTimeout(()=>void close(),c.durationSeconds*1000);save();
try{
 result.fixture=c.preparedProfile?.fixture??await native.prepare(schema,resolve(new URL('../../sync-v4-e2e/',import.meta.url).pathname));
 if(c.preparedProfile)result.upgradedProfile=c.preparedProfile;
 fs.writeFileSync(join(c.root,'runtime/business-os/local-modules/e2e-probe/v4-preflight.html'),'<!doctype html><title>Isolated native V4 preflight</title><body>Native V4 preflight</body>');
 native.start('signaling',process.execPath,[join(c.source,'src/core/rxdb/tools/local_signaling_server.js'),String(c.signalingPort)],{...process.env,SIGNALING_HOST:'127.0.0.1',SIGNALING_PORT:String(c.signalingPort)});
 native.start('turn',c.turnBin,['-n','--lt-cred-mech','--realm=bench.test','--user=bench:bench','--no-cli','--no-tls','--no-dtls','--no-tcp-relay','--allow-loopback-peers','--relay-threads=2',`--userdb=${join(c.output,'turn.sqlite')}`,'--listening-ip=192.0.2.1','--relay-ip=192.0.2.1',`--listening-port=${c.turnPort}`,'--min-port=49152','--max-port=49251',`--pidfile=${join(c.output,'turn.pid')}`,`--log-file=${join(c.output,'turn-detail.log')}`,'--no-stdout-log'],{...process.env,LD_LIBRARY_PATH:c.turnLib||''});
 native.serve();result.phase='daemon-ready';save();
 const origin=`http://127.0.0.1:${c.port}`;
 await until(async()=>{const r=await fetch(origin+'/api/business-os/sync/config',{signal:AbortSignal.timeout(2000)});if(!r.ok)return false;const b=await r.json();return b.native_rxdb_peer_available===true;},60000,'real isolated native bootstrap');
 result.runningImage={path:fs.readlinkSync('/proc/'+native.daemon.pid+'/exe'),sha256:createHash('sha256').update(fs.readFileSync('/proc/'+native.daemon.pid+'/exe')).digest('hex')};
 if(result.runningImage.sha256!==c.binarySha256)throw Error('Running daemon differs from the verified installed image');
 // Read-only static requests exercise the selected signed slot. No app code is
 // executed here, and these timings are not app mount/interactive acceptance.
 if(c.preparedProfile){
  result.shellStatic=[];
  for(const path of ['/','/app.js?v=m7-'+c.revision.slice(0,12)]){
   const at=Date.now();
   try{
    const response=await fetch(origin+path,{redirect:'manual',signal:AbortSignal.timeout(10000)}),body=Buffer.from(await response.arrayBuffer());
    const row={path,status:response.status,durationMs:Date.now()-at,bytes:body.length,sha256:createHash('sha256').update(body).digest('hex')};
    if(response.headers.has('location')){const next=new URL(response.headers.get('location'),origin);row.redirect={origin:next.origin,path:next.pathname};}
    if(path.startsWith('/app.js'))row.appBuild=body.toString('utf8').match(/\bAPP_BUILD\s*=\s*['"]([^'"]+)['"]/)?.[1]??null;
    result.shellStatic.push(row);
   }catch(error){result.shellStatic.push({path,status:null,durationMs:Date.now()-at,error:error.name,message:error.message,budgetMs:10000});}
  }
  result.shellStaticReady=result.shellStatic.every(r=>r.status===200);
  save();
 }
 const require=createRequire(join(c.modulesRoot,'playwright/package.json')),{chromium}=require('playwright');
 browser=await chromium.launch({executablePath:c.chrome,headless:true,args:['--disable-gpu'],env:{...process.env,HOME:join(c.root,'runtime/home'),TMPDIR:c.canonicalTmpdir}});
 const context=await browser.newContext();await context.addInitScript(installRelay,{turnPort:c.turnPort});
 await context.route(/\/(kernel\/|rxdb\/src\/)/,async route=>{
  const path=decodeURIComponent(new URL(route.request().url()).pathname),root=join(c.kernelAssets,'src/apps/business-os');
  const file=resolve(root,'.'+path);
  if(!file.startsWith(root+'/')||!['.js','.mjs'].includes(extname(file)))return route.abort();
  if(!fs.existsSync(file))return route.fulfill({status:404,body:'Pinned static kernel dependency absent'});
  await route.fulfill({contentType:'text/javascript',body:fs.readFileSync(file)});
 });
 // The document is really served by the isolated daemon (including actual
 // loopback address-space provenance), not a synthetic intercepted response.
 // Only pinned static JS dependencies are fulfilled above; never data frames.
 const page=await context.newPage();
 const document=await page.goto(origin+'/local-modules/e2e-probe/v4-preflight.html');
 if(document.status()!==200||await page.title()!=='Isolated native V4 preflight')throw Error('Native fixture document was not served');
 result.browserDiagnostics=[];
 page.on('console',message=>{
  const classes=message.text().match(/ERR_[A-Z_]+|Unexpected response code: \d+|Mixed Content|Content Security Policy|WebSocket handshake|Local network access|address space|blocked/gi);
  if(classes)result.browserDiagnostics.push({at:Date.now(),kind:message.type(),classes});
 });
 const cdp=await context.newCDPSession(page);await cdp.send('Network.enable');
 cdp.on('Network.webSocketHandshakeResponseReceived',event=>result.browserDiagnostics.push({at:Date.now(),kind:'websocket-handshake',status:event.response.status}));
 cdp.on('Network.webSocketFrameError',event=>result.browserDiagnostics.push({at:Date.now(),kind:'websocket-error',classes:event.errorMessage.match(/ERR_[A-Z_]+|Unexpected response code: \d+|Mixed Content|Content Security Policy|WebSocket handshake|Local network access|address space|blocked/gi)??[],messageLength:event.errorMessage.length}));
 result.phase='native-v4-preflight';save();
 result.binding=await page.evaluate(probeNativeV4,{timeoutMs:30000});

 await page.screenshot({path:join(c.output,'v4-native-preflight.png')});
 result.reason=result.binding.reason;
}catch(error){result.error=error.message;result.reason=`${result.phase}: ${error.message}`;}
finally{
 clearTimeout(timer);await close();
 const ids=['K01','K07','K11','K15','K02','K24'];
 result.scenarios=ids.map(id=>({id,status:'not_run',pass:false,acceptanceComplete:false,reason:result.reason,measured:{phase:result.phase,binding:result.binding??null}}));
 result.cleanup='owned native groups absent';result.finishedAt=new Date().toISOString();result.terminal=true;save();
}
console.log(JSON.stringify({output:c.output,phase:result.phase,binding:result.binding??null,reason:result.reason}));process.exitCode=2;
