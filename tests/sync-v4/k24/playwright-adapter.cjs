// Platform glue for the unchanged upstream tool. The shell resets input.value
// after reading the folder; Playwright waits for a nonempty FileList and times
// out even though the real upload surface already received the files. CDP uses
// the same native Chromium directory selection without that stale postcondition.
const fs=require('node:fs'),path=require('node:path');
exports.create=({packagePath,chrome,runtime,out})=>{
 const real=require(packagePath);
 const log=x=>fs.appendFileSync(path.join(out,'browser-diagnostics.jsonl'),JSON.stringify({at:new Date().toISOString(),...x})+'\n');
 return {...real,chromium:{...real.chromium,launch:async options=>{
  const browser=await real.chromium.launch({...options,executablePath:chrome});
  const createContext=browser.newContext.bind(browser);
  browser.newContext=async options=>{
   const context=await createContext(options);
   context.on('page',page=>{
    page.on('pageerror',e=>log({kind:'pageerror',message:e.message.slice(0,1000)}));
    page.on('console',m=>{if(['error','warning'].includes(m.type()))log({kind:m.type(),message:m.text().slice(0,1000)});});
    page.on('response',r=>{if(r.status()>=400)log({kind:'http',status:r.status(),path:new URL(r.url()).pathname});});
    const original=page.setInputFiles.bind(page);
    page.setInputFiles=async(selector,files,options)=>{
     if(selector!=='[data-k24-harness] input[type=file]'||typeof files!=='string')return original(selector,files,options);
     const directory=fs.realpathSync(files),allowed=path.join(runtime,'src/apps/business-os/v4/k24')+path.sep;
     if(!directory.startsWith(allowed)||!fs.statSync(directory).isDirectory())throw Error('Only the delivered K24 fixture folder may be selected');
     const cdp=await context.newCDPSession(page);
     try{
      const {root}=await cdp.send('DOM.getDocument');const {nodeId}=await cdp.send('DOM.querySelector',{nodeId:root.nodeId,selector});
      if(!nodeId)throw Error('Actual Chromium folder input absent');
      await cdp.send('DOM.setFileInputFiles',{nodeId,files:[directory]});
      log({kind:'native-directory-selection',method:'DOM.setFileInputFiles',folder:path.relative(runtime,directory),shellResetsInputAfterRead:true});
     }finally{await cdp.detach();}
    };
   });
   return context;
  };
  return browser;
 }}};
};
