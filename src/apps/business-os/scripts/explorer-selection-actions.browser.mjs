import assert from 'node:assert/strict';
import http from 'node:http';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputArg = process.argv.indexOf('--output-dir');
const output = outputArg >= 0 ? path.resolve(process.argv[outputArg + 1]) : path.join(root, '../../../output/playwright/explorer-selection-actions');
mkdirSync(output, { recursive: true });
const fixture = `<!doctype html><html data-theme="dark"><head><link rel="icon" href="data:,">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<style>body{margin:0}#fixture-host{height:760px;container-type:inline-size;container-name:business-app-window}</style>
</head><body><div id="fixture-host" data-module-root="explorer"></div><output id="context-target"></output>
<script type="module">
import { mount } from '/modules/explorer/index.js';
import { createContextMenu } from '/shared/context-menu.js';
const records = new Map([['fixture-folder', {
  id:'fixture-folder',parent_id:'fs_root',path:'/Fixture folder',name:'Fixture folder',
  kind:'folder',created_at_ms:100,updated_at_ms:100,is_deleted:false
}]]);
const documentFor = id => records.has(id) ? {
  toJSON:()=>structuredClone(records.get(id)),
  incrementalPatch:async patch=>records.set(id,{...records.get(id),...patch})
} : null;
const files = {
  find:()=>({exec:async()=>[...records.keys()].map(documentFor)}),
  findOne:id=>({exec:async()=>documentFor(id)}),
  upsert:async value=>records.set(value.id,value)
};
document.addEventListener('contextmenu', event => {
  event.preventDefault();
  const row=event.target.closest('[data-context-record-id]');
  document.querySelector('#context-target').textContent=row?.dataset.contextRecordId || '';
});
await mount({
  host:document.querySelector('#fixture-host'), locale:new URL(location).searchParams.get('lang'),
  db:{collection:name=>name==='desktop_files' ? files : null},
  contextMenu:createContextMenu({host:document.body,viewportEl:document.documentElement})
});
</script></body></html>`;
const mime = {'.js':'text/javascript','.mjs':'text/javascript','.css':'text/css','.html':'text/html','.svg':'image/svg+xml','.png':'image/png'};
const server = http.createServer((request,response)=>{
  const pathname=new URL(request.url,'http://fixture').pathname;
  if(pathname==='/') {response.setHeader('content-type','text/html'); response.end(fixture); return;}
  const file=path.resolve(root,'.'+decodeURIComponent(pathname));
  if(!file.startsWith(root+path.sep)||!existsSync(file)){response.writeHead(404);response.end();return;}
  response.setHeader('content-type',mime[path.extname(file)]||'application/octet-stream');
  response.end(readFileSync(file));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const chrome='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const executablePath=process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || (existsSync(chrome)?chrome:undefined);
let browser;
const results=[];
try {
  browser=await chromium.launch({headless:true,...(executablePath?{executablePath}:{})});
  for(const locale of ['de','en']) for(const width of [640,1180]) {
    const context=await browser.newContext({viewport:{width,height:820}});
    const page=await context.newPage();
    page.setDefaultTimeout(10000);
    const errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    page.on('console',message=>{if(message.type()==='error')errors.push(message.text()+' '+message.location().url);});
    try {
      await page.goto('http://127.0.0.1:'+server.address().port+'/?lang='+locale);
      const row=page.locator('[data-id="fixture-folder"]');
      await row.waitFor({state:'visible'});
      await row.click();
      const actions=page.getByRole('button',{name:locale==='de'?'Aktionen für Auswahl':'Actions for selection',exact:true});
      assert.equal(await actions.isEnabled(),true);
      // Right-click metadata reaches the shell listener; the module owns no competing handler.
      await row.click({button:'right'});
      await page.locator('#context-target').filter({hasText:'fixture-folder'}).waitFor();
      await actions.focus();
      await actions.press('Enter');
      const activeMenu=page.locator('.shell-context-menu.is-active');
      const rename=activeMenu.getByRole('menuitem',{name:locale==='de'?'Umbenennen':'Rename'});
      await rename.waitFor({state:'visible'});
      const menuBox=await activeMenu.boundingBox();
      assert.ok(menuBox && menuBox.x>=0 && menuBox.x+menuBox.width<=width+1);
      await page.screenshot({path:path.join(output,locale+'-'+width+'-actions.png')});
      await rename.click();
      const dialog=page.getByRole('dialog',{name:locale==='de'?'Umbenennen':'Rename',exact:true});
      await dialog.getByRole('textbox').fill('Renamed fixture');
      await dialog.getByRole('button',{name:locale==='de'?'Speichern':'Save',exact:true}).click();
      await row.filter({hasText:'Renamed fixture'}).waitFor();
      await actions.click();
      await activeMenu.getByRole('menuitem',{name:locale==='de'?'In Papierkorb':'Move to trash'}).click();
      await page.getByRole('dialog').getByRole('button',{name:locale==='de'?'Abbrechen':'Cancel',exact:true}).click();
      await row.waitFor({state:'visible'});
      await page.locator('[data-explorer-search]').fill('no-fixture-matches');
      await page.waitForFunction(()=>document.querySelector('[data-explorer-selection-actions]')?.disabled===true);
      assert.equal(await row.count(),0);
      assert.deepEqual(errors,[]);
      results.push({locale,width,passed:true});
    } finally {await context.close();}
  }
  console.log('Explorer selection actions: '+results.length+'/4 passed (keyboard menu, rename, cancel deletion, empty selection, shared context metadata)');
} finally {
  writeFileSync(path.join(output,'result.json'),JSON.stringify({results},null,2)+'\n');
  await browser?.close();
  await new Promise(resolve=>server.close(resolve));
}
