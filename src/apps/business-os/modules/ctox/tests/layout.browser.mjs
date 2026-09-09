// Component browser acceptance: real renderer and shell CSS, synthetic task data.
// Production authentication, replication and performance are separate gates.
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { build } from 'esbuild';
import { chromium } from 'playwright';
import { stampShellDocument } from '../../../scripts/build-shell-artifact.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const out = process.env.CTOX_EVIDENCE_DIR || path.resolve(root, '../../../output/welsch-harness-layout-20260906');
await mkdir(out, { recursive: true });
const bundle = await build({ entryPoints: [path.join(root, 'modules/ctox/index.js')], bundle: true, write: false, format: 'esm', platform: 'browser', logLevel: 'silent' });
const markup = await readFile(path.join(root, 'modules/ctox/index.html'), 'utf8');
const html = stampShellDocument(Buffer.from(`<!doctype html><html data-theme="dark"><head>
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/ctox/index.css"></head><body>
<div class="shell-window-layer" style="position:fixed;inset:0"><section class="shell-window is-focused" data-shell-window="true" data-shell-contract="v2" data-shell-window-chrome="shared-v2" data-shell-header-rows="2" data-shell-icon-rows="2" data-owner-id="desktop-app:ctox" style="position:absolute;inset:0;width:100%;height:100%">
<div class="shell-window-content"><div class="module-root shell-window-module-root" data-module-root="ctox" data-module-ready="true"><aside class="shell-window-module-pane shell-window-module-pane--left"></aside><div class="shell-window-module-column-resizer shell-window-module-column-resizer--left"></div><main class="module-content" data-module-content>${markup}</main><div class="shell-window-module-column-resizer shell-window-module-column-resizer--right"></div><aside class="shell-window-module-pane shell-window-module-pane--right"></aside></div></div>
</section></div><script type="module">
import {__ctoxTestHooks as hooks} from '/bundle.js';
import {readEmbeddedIdentity} from '/shared/shell-release-status.js';
const host=document.querySelector('[data-module-root]');
window.CTOX_BUSINESS_OS_APP = { openSettingsDrawer: (options) => { window.openedSettings = options; } };
const data=hooks.mergeBundleWithCommands({runs:[],queue:[],communications:[],tickets:[],tools:[]},
[{id:'layout-command',command_id:'layout-command',execution_task_id:'layout-task',execution_mode:'queue',execution_phase:'queued',status:'accepted',payload:{title:'Cereda'},execution_progress:{phase:'queued',steps:[]}}],
[{id:'layout-task',command_id:'layout-command',status:'queued',route_status:'failed',failure_class:'terminal',failure_attempt_count:4,status_note:'thread/start MCP handshake timeout',updated_at_ms:Date.now()}]);
const model=hooks.buildHarnessModel(data,{ok:false},'de');
const task=model.tasks.find(task=>task.id==='layout-task');
const state={ctx:{host},model,lang:'de',flow:{ok:false},selectedTaskId:task.id,selectedStepIndex:0,selectedTaskStepIndex:2,selectedNodeId:'',zoom:1,taskSearch:'',taskViewMode:'cards',taskPrimaryView:'all',taskSourceFilter:'all',taskPinFilter:'all',taskSort:'updated',taskSortDirection:'desc',pinnedTaskIds:new Set(),webStackPanelOpen:false,webStack:{loading:false,data:null,error:''},dataLoaded:true,dataError:'',runtimeStatus:'ready',flowViewport:{left:0,top:0}};
host.querySelector('[data-ctox-left]').innerHTML=hooks.taskColumnMarkup(model.tasks,state);
hooks.renderMain({...state,model:null}); // Locale notification before hydration must not throw.
hooks.renderMain(state);
state.ctx.session = { user: { role: 'admin' } };
window.crewFixture = { state, hooks };
document.body.dataset.loadedVersion=readEmbeddedIdentity(document).version;
document.body.dataset.fixtureReady='true';
</script></body></html>`), { version: '1.2.3-beta.1', sourceCommit: 'a'.repeat(40) });
const server = http.createServer(async (req,res) => {
  try {
    const pathname=new URL(req.url,'http://localhost').pathname;
    if(pathname==='/'){res.setHeader('content-type','text/html');res.end(html);return;}
    if(pathname==='/bundle.js'){res.setHeader('content-type','text/javascript');res.end(bundle.outputFiles[0].contents);return;}
    const file=path.resolve(root, '.'+pathname);
    if(!file.startsWith(root)){res.writeHead(403).end();return;}
    res.setHeader('content-type',file.endsWith('.css')?'text/css':file.endsWith('.js')?'text/javascript':'application/octet-stream');
    res.end(await readFile(file));
  }catch{res.writeHead(404).end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const browser=await chromium.launch({headless:true, ...(process.env.PLAYWRIGHT_CHANNEL ? {channel:process.env.PLAYWRIGHT_CHANNEL} : {})});
const results=[];
try {
  for(const width of [430,630,768,1000,1280]){
    const page=await browser.newPage({viewport:{width,height:710}});
    const errors=[];
    const requests=[];
    page.on('pageerror',error=>errors.push(String(error)));
    page.on('request',request=>requests.push(new URL(request.url()).pathname));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.locator('body[data-fixture-ready="true"]').waitFor({timeout:10000}).catch(error=>{throw new Error(errors.join('\n')||String(error));});
    const measured=await page.locator('[data-flow-canvas]').evaluate(canvas=>({height:canvas.getBoundingClientRect().height,width:canvas.getBoundingClientRect().width,nodes:canvas.querySelectorAll('.ctox-flow-node-g').length}));
    results.push({viewportWidth:width,...measured,errors});
    await page.screenshot({path:path.join(out,`${width}.png`)});
    assert.equal(errors.length,0);
    assert.equal(await page.locator('body').getAttribute('data-loaded-version'),'1.2.3-beta.1');
    assert.ok(!requests.includes('/ctox-shell-manifest.json'),'loaded identity must not require a second manifest request');
    assert.equal(measured.nodes,16);
    const reason=await page.locator('.ctox-task-reason').textContent();
    assert.match(reason,/4 Versuche/);
    assert.match(reason,/Verbindung zu einem Werkzeug/);
    assert.doesNotMatch(reason,/MCP|thread\/start/);
    assert.match(await page.locator('[data-pg-band="waiting"]').textContent(),/\(0\)/);
    assert.equal(await page.locator('.ctox-task-pipeline').getAttribute('aria-label'),'Fehler');
    assert.equal(await page.locator('[data-task-id="layout-task"][data-creature-node-id="model-failed"]').count(),1);
    assert.ok(measured.height>=200,`Harness collapsed at width ${width}: ${JSON.stringify(measured)}`);
    await page.locator('[data-node-id="queued"]').scrollIntoViewIfNeeded();
    const visible=await page.locator('[data-node-id="queued"]').evaluate(node=>{const r=node.getBoundingClientRect();return document.elementsFromPoint(r.x+r.width/2,r.y+r.height/2).some(e=>e===node||node.contains(e));});
    assert.ok(visible,`Harness node is clipped at width ${width}`);
    if(width===1280){
      const headerHeight = await page.locator('[data-ctox-main] > header').evaluate(node => node.getBoundingClientRect().height);
      assert.ok(headerHeight <= 40, `Header must be one row, got ${headerHeight}`);
      const gap = await page.locator('.ctox-harness-app').evaluate(node => {
        const frame = node.getBoundingClientRect();
        const main = node.querySelector('[data-ctox-main]').getBoundingClientRect();
        return frame.bottom - main.bottom;
      });
      assert.ok(gap <= 16, `Unused bottom reserve: ${gap}px`);
      await page.locator('[data-job-toggle]').click();
      assert.deepEqual(await page.locator('[data-job-panel] select[name="priority"] option').allTextContents(), ['Dringend', 'Hoch', 'Normal', 'Niedrig']);
      await page.locator('[data-job-panel] input[name="title"]').fill('Ungespeicherter Entwurf');
      await page.evaluate(() => window.crewFixture.hooks.renderMain(window.crewFixture.state));
      assert.equal(await page.locator('[data-job-panel] input[name="title"]').inputValue(), 'Ungespeicherter Entwurf');
      await page.locator('[data-job-toggle]').click();
      assert.equal(await page.locator('[data-job-panel]').isVisible(), false);
      await page.locator('[data-ctox-main] .ctox-more-actions > summary').click();
      const manage = page.locator('[data-manage-channels]');
      assert.equal(await manage.isVisible(), true);
      await manage.click();
      assert.deepEqual(await page.evaluate(() => window.openedSettings), { initialTab: 'channels' });
      assert.equal(await page.locator('.ctox-more-actions-body:popover-open').count(), 0);
      await page.evaluate(() => {
        window.crewFixture.state.ctx.openLeftDrawer = content => {
          document.getElementById("fixture-task-drawer")?.remove();
          const panel = document.createElement("aside");
          panel.id = "fixture-task-drawer";
          panel.style.cssText = "position:fixed;inset:0 0 0 50%;overflow:auto;z-index:100";
          panel.append(content);
          document.body.append(panel);
        };
      });
      await page.locator("[data-ctox-main] .ctox-more-actions > summary").click();
      await page.locator("[data-open-selected-task]").click();
      const taskHistory = page.locator(".ctox-drawer-timeline");
      assert.equal(await taskHistory.getAttribute("open"), null);
      assert.equal(await taskHistory.locator(".ctox-drawer-steps").isVisible(), false);
      assert.ok(await taskHistory.evaluate(e => e.getBoundingClientRect().height) <= 40);
      await taskHistory.locator("summary").click();
      assert.equal(await taskHistory.locator(".ctox-drawer-steps").isVisible(), true);
      await page.evaluate(() => {
        const {state,hooks}=window.crewFixture;
        hooks.syncDetailDrawer(state); // unchanged refresh must preserve the actual DOM
        state.model.tasks[0].statusNote = 'A new persisted status arrived';
        hooks.syncDetailDrawer(state); // changed facts must preserve disclosure state too
      });
      assert.equal(await taskHistory.getAttribute("open"), "");
      assert.equal(await taskHistory.locator(".ctox-drawer-steps").isVisible(), true);
      await page.locator(".ctox-drawer-edit-fold > summary").click();
      const drawerTitle = page.locator("#fixture-task-drawer input[name=title]");
      await drawerTitle.fill("Entwurf bleibt bei Live-Updates");
      await page.evaluate(() => {
        const {state,hooks}=window.crewFixture;
        state.model.tasks[0].statusNote = 'Another update while typing';
        hooks.syncDetailDrawer(state);
      });
      assert.equal(await drawerTitle.inputValue(), "Entwurf bleibt bei Live-Updates");
      assert.equal(await drawerTitle.evaluate(e=>e === document.activeElement), true);
      await taskHistory.locator("summary").click();
      assert.equal(await taskHistory.locator(".ctox-drawer-steps").isVisible(), false);
      await page.locator("#fixture-task-drawer").evaluate(e => e.remove());
      await page.locator('[data-node-id="model-failed"]').scrollIntoViewIfNeeded();
      for(const theme of ['dark','light']){
        await page.locator('html').evaluate((element,theme)=>element.dataset.theme=theme,theme);
        await page.screenshot({path:path.join(out,`routing-failed-${theme}.png`)});
      }
      await page.evaluate(() => {
        const {state,hooks}=window.crewFixture;
        state.selectedTaskId='';
        state.model.tasks=[];
        state.model.timeline=state.model.nodes.slice(0,2);
        hooks.renderMain(state);
      });
      const history = page.locator('.ctox-history-fold');
      assert.equal(await history.count(), 1, 'Actual history remains accessible');
      assert.equal(await history.getAttribute('open'), null);
      const closedHeight = await history.evaluate(e=>e.getBoundingClientRect().height);
      assert.ok(closedHeight <= 40, 'Closed history uses one row');
      await history.locator('summary').click();
      assert.equal(await history.getAttribute('open'), '');
      assert.ok(await history.evaluate(e=>e.getBoundingClientRect().height) > closedHeight);
      await history.locator('summary').click();
      assert.equal(await history.getAttribute('open'), null);
    }
    if (width === 1280) {
      await page.evaluate(async () => {
        const { openReactSettings } = await import('/shared/react-settings.js');
        const mount = document.createElement('aside');
        mount.id = 'runtime-settings-fixture';
        mount.style.cssText = 'position:fixed;inset:0;overflow:auto;z-index:1000;background:var(--bg)';
        document.body.append(mount);
        let projection = {
          id: 'runtime-settings', updated_at_ms: 1, can_manage: true,
          runtime: { provider: 'minimax', chat_model: 'MiniMax-M3', reasoning_effort: 'high' },
          auth: { mode: 'api_key', api_key_configured: false },
          diagnostics: { auth_needs_attention: true },
        };
        window.runtimeFixtureCommands = [];
        await openReactSettings({
          mount, modules: [], session: { user: { id: 'owner', role: 'admin', is_admin: true } },
          sync: { startCollection: async () => {} },
          db: { collection: name => name === 'business_commands' ? {} : name === 'ctox_runtime_settings' ? {
            findOne: () => ({ exec: async () => ({ toJSON: () => projection }) }),
          } : null },
          commandBus: { dispatch: async command => {
            window.runtimeFixtureCommands.push(command);
            projection = { ...projection, updated_at_ms: Date.now(), runtime: { ...command.payload },
              auth: { mode: command.payload.auth_mode, api_key_configured: true }, diagnostics: {} };
            return { result: { ok: true }, status: 'accepted' };
          } },
        });
      });
      const settings = page.locator('#runtime-settings-fixture');
      await settings.locator('[data-runtime-api-key]').fill('fixture-only-not-a-real-key');
      await settings.locator('[data-runtime-save]').click();
      await settings.getByText('Runtime/Auth gespeichert.', { exact: true }).waitFor();
      const saved = await page.evaluate(() => window.runtimeFixtureCommands);
      assert.equal(saved.length, 1);
      assert.equal(saved[0].type, 'ctox.runtime_settings.save');
      assert.equal(saved[0].payload.provider, 'minimax');
      assert.equal(saved[0].payload.api_key, 'fixture-only-not-a-real-key');
      assert.equal(saved[0].client_context.actor.id, 'owner');
      await settings.locator('[data-runtime-refresh]').click();
      await settings.getByPlaceholder('Gespeichert · leer lassen, um ihn zu behalten').waitFor();
      assert.equal(await settings.locator('[role=alert]').count(), 0);
      await page.screenshot({path:path.join(out,'runtime-save-reload.png')});
      await settings.evaluate(e => e.remove());
    }
    await page.close();
  }
  console.log(JSON.stringify({passed:results.length,results}));
}finally{
  await writeFile(path.join(out,'results.json'),JSON.stringify(results,null,2));
  await browser.close();
  await new Promise(resolve=>server.close(resolve));
}
