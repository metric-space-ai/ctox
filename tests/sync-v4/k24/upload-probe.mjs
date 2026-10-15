// Diagnostic for the actual Chromium directory-upload primitive used upstream.
// No sync-path simulation and no K24 acceptance claim.
import {createRequire} from 'node:module';
import {resolve} from 'node:path';
const require=createRequire(import.meta.url),{chromium}=require(process.argv[2]);
const browser=await chromium.launch({headless:true,executablePath:'/opt/google/chrome/chrome'});
try{
 const page=await browser.newPage();await page.setContent('<input type="file" hidden multiple webkitdirectory>');
 const directory=resolve(process.argv[3]??'tests/sync-v4/k24/app/v1/k24-probe');const started=Date.now();let error=null;
 try{await page.setInputFiles('input',directory,{timeout:5000});}catch(e){error=e.message;}
 const files=await page.locator('input').evaluate(e=>Array.from(e.files??[]).map(f=>({name:f.name,relative:f.webkitRelativePath,bytes:f.size})));
 console.log(JSON.stringify({diagnosticOnly:true,directoryUpload:{elapsedMs:Date.now()-started,error,files}}));
 if(error||!files.some(f=>f.relative.endsWith('/handlers/k24.tick.add.js')))process.exitCode=1;
 await page.setContent('<input type="file" hidden multiple webkitdirectory><output></output>');
 await page.evaluate(()=>{const e=document.querySelector('input');e.addEventListener('change',async()=>{const names=[];for(const f of e.files){await f.text();names.push(f.webkitRelativePath);}document.querySelector('output').textContent=JSON.stringify(names);e.value='';});});
 const cdp=await page.context().newCDPSession(page),{root}=await cdp.send('DOM.getDocument');
 const {nodeId}=await cdp.send('DOM.querySelector',{nodeId:root.nodeId,selector:'input'});
 await cdp.send('DOM.setFileInputFiles',{nodeId,files:[directory]});await cdp.detach();
 await page.waitForFunction(()=>document.querySelector('output').textContent.includes('handlers/k24.tick.add.js'));
 console.log(JSON.stringify({diagnosticOnly:true,resettingInputCdp:await page.evaluate(()=>({fileListReset:document.querySelector('input').files.length===0,actuallyRead:JSON.parse(document.querySelector('output').textContent)}))}));
}finally{await browser.close();}
