import assert from 'node:assert/strict';
import http from 'node:http';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const arg=process.argv.indexOf('--output-dir');
const output=arg<0?path.join(root,'../../../output/playwright/module-local-controls'):path.resolve(process.argv[arg+1]);
mkdirSync(output,{recursive:true});
const fixtureLogic="\nimport { autoWirePaneGrammar } from '/shared/pane-grammar.js';\nconst emptyQuery=()=>({exec:async()=>[], $:{subscribe(fn){fn([]);return {unsubscribe(){}};}}});\nconst collection={find:emptyQuery,findOne:()=>({exec:async()=>null}),$:{subscribe(){return {unsubscribe(){}};}}};\nlet calls=0;\nawait mount({\n host:document.querySelector('#fixture-host'),left:document.createElement('div'),right:document.createElement('div'),\n locale:'en',db:{collection:()=>collection},\n sync:{collectionReadiness:()=>({ready:true,state:'live'}),startCollection:async()=>{}},\n commandBus:{async dispatch(command){\n   if(command.type?.startsWith('ctox.knowledge.book.')){\n     document.querySelector('#dispatch-count').textContent=String(++calls);\n     const response=await fetch('/fixture-command');\n     if((await response.text())==='unavailable')throw new Error('fixture transport unavailable');\n   }\n   return {ok:true,command_id:'fixture-command',exploits:[]};\n }},\n openLeftDrawer(){throw new Error('global drawer must not open');},\n openRightDrawer(){throw new Error('global drawer must not open');},\n openBottomDrawer(){throw new Error('global drawer must not open');}\n});\nautoWirePaneGrammar(document.querySelector('#fixture-host'));\ndocument.body.dataset.ready='true';\n</script></body></html>";
// Real module UI, explicit in-memory data/command fixtures. Not a tenant replication test.
function fixture(app,width){
return '<!doctype html><html lang="en" data-theme="dark"><head><link rel="icon" href="data:,">'
+'<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">'
+'<style>body{margin:0}#fixture-host{position:absolute;left:20px;top:80px;width:'+width+'px;height:740px;container-type:inline-size;container-name:business-app-window}</style>'
+'</head><body><button id="outside">Other window</button><output id="dispatch-count">0</output><div id="fixture-host"></div><script type="module">'
+'import { mount } from "/modules/'+app+'/index.js";'+fixtureLogic;
}
const mime={'.js':'text/javascript','.mjs':'text/javascript','.css':'text/css','.html':'text/html','.svg':'image/svg+xml','.png':'image/png','.json':'application/json'};
const server=http.createServer((req,res)=>{
 const url=new URL(req.url,'http://fixture');
 if(url.pathname==='/'){res.setHeader('content-type','text/html');res.end(fixture(url.searchParams.get('app'),Number(url.searchParams.get('width'))));return;}
 const file=path.resolve(root,'.'+decodeURIComponent(url.pathname));
 if(!file.startsWith(root+path.sep)||!existsSync(file)){res.writeHead(404);res.end();return;}
 res.setHeader('content-type',mime[path.extname(file)]||'application/octet-stream');res.end(readFileSync(file));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const chrome='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const executablePath=process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH||(existsSync(chrome)?chrome:undefined);
let browser;
const results=[];
async function withinHost(page,locator){
 const host=await page.locator('#fixture-host').boundingBox(),box=await locator.boundingBox();
 assert.ok(box&&box.x>=host.x-1&&box.y>=host.y-1&&box.x+box.width<=host.x+host.width+1&&box.y+box.height<=host.y+host.height+1,JSON.stringify({host,box}));
}
try{
 browser=await chromium.launch({headless:true,...(executablePath?{executablePath}:{})});
 for(const app of ['knowledge','appsec-pentest','reports'])for(const width of [360,640,1180]){
  const context=await browser.newContext({viewport:{width:width+40,height:900}});
  const page=await context.newPage();page.setDefaultTimeout(10000);
  const errors=[];
  let releaseCommand;
  let commandPending;
  const pendingCommand=new Promise(resolve=>{commandPending=resolve;});
  let commandRequests=0;
  await page.route('**/fixture-command', async route=>{
    if(++commandRequests===1) {
      await new Promise(resolve=>{releaseCommand=resolve;commandPending();});
      await route.fulfill({status:200,body:'unavailable'});
    } else await route.fulfill({status:200,body:'confirmed'});
  });
  page.on('pageerror',error=>errors.push(error.message));
  page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
  try{
   await page.goto('http://127.0.0.1:'+server.address().port+'/?app='+app+'&width='+width);
   await page.locator('body[data-ready="true"]').waitFor();
   if(app==='knowledge'){
    for(const action of ['create','import','export']){
     const trigger=page.locator('[data-action="'+action+'-knowledge-book"]');
     await trigger.click();
     const dialog=page.getByRole('dialog');
     await dialog.waitFor({state:'visible'});await withinHost(page,dialog);
     const close=dialog.getByRole('button',{name:'Schließen',exact:true});
     await close.focus();await close.press('Shift+Tab');
     assert.equal(await dialog.evaluate(el=>el.contains(document.activeElement)),true);
     await page.keyboard.press('Escape');await dialog.waitFor({state:'detached'});
     assert.equal(await trigger.evaluate(el=>el===document.activeElement),true);
    }
    await page.locator('[data-action="create-knowledge-book"]').click();
    const dialog=page.getByRole('dialog'),title=dialog.locator('input[name="title"]'),submit=dialog.locator('button[type="submit"]');
    assert.equal(await submit.isEnabled(),false);await title.fill('Retained draft');await submit.click();
    assert.equal(await submit.isEnabled(),false);
    await new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>reject(new Error('fixture command was not dispatched')),10000);
      pendingCommand.then(()=>{clearTimeout(timer);resolve();});
    });
    assert.equal(typeof releaseCommand,'function');
    releaseCommand();
    await dialog.getByRole('status').filter({hasText:'Bitte erneut versuchen'}).waitFor();
    assert.equal(await title.inputValue(),'Retained draft');assert.equal(await submit.isEnabled(),true);
    assert.equal(await dialog.getByRole('status').evaluate(el=>getComputedStyle(el).whiteSpace),'normal');
    await withinHost(page,submit);
    assert.equal(await page.locator('#dispatch-count').textContent(),'1');
    await page.screenshot({path:path.join(output,app+'-'+width+'-retry.png')});
    await submit.click();await page.getByRole('dialog',{name:'Knowledge Command',exact:true}).waitFor();
    assert.equal(await page.locator('#dispatch-count').textContent(),'2');
    await page.getByRole('dialog').getByRole('button',{name:'Schließen',exact:true}).click();
   }else if(app==='appsec-pentest'){
    assert.equal(await page.locator('dialog[open]').count(),0);
    await page.locator('[data-appsec-create]').click();
    const dialog=page.locator('[data-appsec-test-dialog]');
    await dialog.waitFor({state:'visible'});await withinHost(page,dialog);
    assert.equal(await dialog.evaluate(el=>el.matches(':modal')),false);
    await page.locator('#outside').click();
    await page.screenshot({path:path.join(output,app+'-'+width+'-dialog.png')});
    await dialog.getByRole('button',{name:'Close',exact:true}).click();await dialog.waitFor({state:'hidden'});
   }else{
    const tray=page.locator('[data-pg-tray]'),toggle=page.locator('[data-pg-tray-toggle]');
    assert.equal(await tray.isVisible(),false);await toggle.click();await tray.waitFor({state:'visible'});
    await page.locator('[data-pg-filter]').selectOption('open');
    await page.waitForFunction(()=>document.querySelector('[data-pg-tray-toggle]').classList.contains('has-active-filters'));
    await withinHost(page,toggle);await page.screenshot({path:path.join(output,app+'-'+width+'-filter.png')});
    await page.locator('[data-pg-reset]').click();assert.equal(await page.locator('[data-pg-filter]').inputValue(),'all');
    await page.locator('[data-reports-view-toggle]').click();
    assert.equal(await page.locator('[data-reports-view-toggle]').getAttribute('data-pg-view'),'list');
    await toggle.click();await tray.waitFor({state:'hidden'});
   }
   assert.deepEqual(errors,[]);results.push({app,width,passed:true});
  }finally{await context.close();}
 }
 console.log('Module local controls: '+results.length+'/9 passed');
}finally{
 writeFileSync(path.join(output,'result.json'),JSON.stringify({results},null,2)+'\n');
 await browser?.close();await new Promise(resolve=>server.close(resolve));
}
