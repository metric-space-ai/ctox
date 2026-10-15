// Platform glue for the unchanged upstream tool. Select its actual directory
// through Playwright in the foreground page. The Shell may consume/reset the
// FileList; only its real completed-upload state may end that stale wait.
// No file payload, Core response, sync path or upstream criterion is replaced.
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
    // Preserve the upstream wait and its failure. Capture only our synthetic
    // harness, never a tenant page or protected browser state.
    const wait=page.waitForFunction.bind(page);
    page.waitForFunction=async(...args)=>{
     try{return await wait(...args);}catch(error){
      const state=await page.evaluate(()=>({
       path:location.pathname,
       text:document.querySelector('[data-k24-harness]')?.innerText?.slice(0,12000),
       inputs:[...document.querySelectorAll('[data-k24-harness] input')].map(i=>({type:i.type,hidden:i.hidden,value:i.type==='file'?undefined:i.value,files:i.files?[...i.files].map(f=>({name:f.name,path:f.webkitRelativePath,size:f.size})):undefined})),
       buttons:[...document.querySelectorAll('[data-k24-harness] button')].map(b=>({text:b.innerText,disabled:b.disabled}))
      })).catch(e=>({captureError:e.message}));
      log({kind:'wait-failure',message:error.message.slice(0,1000),state});
      await page.screenshot({path:path.join(out,'wait-failure-'+Date.now()+'.png'),fullPage:true}).catch(()=>{});
      throw error;
     }
    };
    const original=page.setInputFiles.bind(page);

    page.setInputFiles=async(selector,files,options)=>{
     if(selector!=='[data-k24-harness] input[type=file]'||typeof files!=='string')return original(selector,files,options);
     const directory=fs.realpathSync(files),allowed=path.join(runtime,'src/apps/business-os/v4/k24')+path.sep;
     if(!directory.startsWith(allowed)||!fs.statSync(directory).isDirectory())throw Error('Only the delivered K24 fixture folder may be selected');
     await page.bringToFront();
     await page.locator(selector).evaluate(input=>{
      globalThis.__k24SelectedFiles=null;
      input.addEventListener('change',()=>{globalThis.__k24SelectedFiles=[...input.files].map(f=>({path:f.webkitRelativePath,name:f.name,size:f.size}));},{once:true,capture:true});
     });
     // Use Playwright's native directory chooser. Its FileList postcondition can
     // become stale after the Shell has already consumed and reset the input.
     // Only the real Shell's visible completed-upload state may end that wait.
     const selection=original(selector,directory,{...options,timeout:15000});
     selection.catch(error=>log({kind:'chooser-completion',message:error.message.slice(0,1000)}));
     try{await Promise.race([selection,wait(()=>{
      const i=document.querySelector('[data-k24-harness] [data-app-id]');
      const b=document.querySelector('[data-k24-harness] [data-publish]');
      return i&&!i.hidden&&i.value&&b&&!b.disabled;
     },null,{timeout:16000})]);}catch(error){
      log({kind:'chooser-failure',message:error.message.slice(0,1000),state:await page.evaluate(()=>({selected:globalThis.__k24SelectedFiles,visibility:document.visibilityState,focused:document.hasFocus(),upload:document.querySelector('[data-k24-harness]')?.outerHTML?.slice(0,8000)}))});
      await page.screenshot({path:path.join(out,'chooser-failure.png'),fullPage:true});throw error;
     }
     log({kind:'native-directory-selection',method:'Playwright directory chooser; real Shell completion raced against stale FileList postcondition',folder:path.relative(runtime,directory),selected:await page.evaluate(()=>globalThis.__k24SelectedFiles)});

    };
   });
   return context;
  };
  return browser;
 }}};
};
