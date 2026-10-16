import fs from 'node:fs';
import {join} from 'node:path';
import {createRequire} from 'node:module';
const root=process.argv[2], config=JSON.parse(fs.readFileSync(join(root,'config.json')));
const require=createRequire(join(config.modulesRoot,'playwright/package.json'));
const {chromium}=require('playwright');
let browser,launchPromise;
process.once('SIGTERM',()=>{void (launchPromise||Promise.resolve()).then(b=>b?.close()).catch(()=>{}).finally(()=>process.exit(143));});
const wait=async path=>{const deadline=Date.now()+30000;while(Date.now()<deadline){if(fs.existsSync(path))return JSON.parse(fs.readFileSync(path));await new Promise(r=>setTimeout(r,20));}throw Error('fixture file deadline');};
const write=(name,data)=>{fs.writeFileSync(join(root,name+'.tmp'),JSON.stringify(data));fs.renameSync(join(root,name+'.tmp'),join(root,name));};
try {
 launchPromise=chromium.launch({executablePath:config.chrome,headless:true,args:['--disable-gpu','--no-sandbox'],timeout:15000});
 browser=await launchPromise;
 const page=await browser.newPage();
 await page.evaluate(async iceServer=>{
  globalThis.pc=new RTCPeerConnection({iceTransportPolicy:'relay',iceServers:[iceServer]});
  globalThis.received=[];globalThis.dc=pc.createDataChannel('turn-native');
  dc.onmessage=e=>received.push(e.data);
  await pc.setLocalDescription(await pc.createOffer());
  await new Promise((resolve,reject)=>{const t=setTimeout(()=>reject(Error('browser gathering')),10000);const done=()=>{if(pc.iceGatheringState==='complete'){clearTimeout(t);resolve();}};pc.addEventListener('icegatheringstatechange',done);done();});
 },{urls:config.browserUrl,...config.initial});
 write('offer.json',await page.evaluate(()=>pc.localDescription.toJSON()));
 await page.evaluate(answer=>pc.setRemoteDescription(answer),await wait(join(root,'answer.json')));
 await page.waitForFunction(()=>dc.readyState==='open',null,{timeout:15000});
 await page.evaluate(()=>{for(let i=0;i<64;i++)dc.send(`turn-message-${i}`);});
 await page.waitForFunction(()=>received.length===64,null,{timeout:15000});
 // Native refreshes its signaling credentials after five seconds, while this
 // exact data channel/PeerConnection remains live. No reconnect is permitted.
 await new Promise(r=>setTimeout(r,12500));
 await page.evaluate(()=>{if(dc.readyState!=='open')throw Error('credential renewal dropped live channel');for(let i=64;i<128;i++)dc.send(`turn-message-${i}`);});
 await page.waitForFunction(()=>received.length===128,null,{timeout:15000});
 const result=await page.evaluate(async()=>{
  const stats=await pc.getStats();const transport=[...stats.values()].find(s=>s.type==='transport'&&s.selectedCandidatePairId);
  const pair=stats.get(transport?.selectedCandidatePairId),local=stats.get(pair?.localCandidateId),remote=stats.get(pair?.remoteCandidateId);
  return {pass:received.every((m,i)=>m===`turn-message-${i}`)&&local?.candidateType==='relay'&&remote?.candidateType==='relay',
    selected:{local:local?.candidateType,remote:remote?.candidateType,relayProtocol:local?.relayProtocol},delivered:received.length,lost:128-received.length};
 });
 result.browserVersion=browser.version(); result.nodeVersion=process.version;
 write('browser-result.json',result);
 if(!result.pass)throw Error('selected relay or reliable delivery failed');
} catch(error) {write('browser-result.json',{pass:false,error:error.message});process.exitCode=1;}
finally {await browser?.close();}
