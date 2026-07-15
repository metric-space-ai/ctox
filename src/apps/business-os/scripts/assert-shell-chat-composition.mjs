#!/usr/bin/env node
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '../../../..');
const outputDir = process.env.SHELL_CHAT_COMPOSITION_OUTPUT_DIR
  || path.join(repoRoot, 'output/playwright', `shell-chat-composition-${timestampForPath()}`);
const reportPath = path.join(outputDir, 'shell-chat-composition.json');
const screenshotPath = path.join(outputDir, 'shell-chat-composition-expanded.png');
fs.mkdirSync(outputDir, { recursive: true });

const { chromium } = require(resolvePlaywrightModule());
const failures = [];
const observations = [];
const consoleEvents = [];
const server = createServer((request, response) => serveRequest(request, response));
const port = await listen(server);
const url = `http://127.0.0.1:${port}/`;
const browser = await chromium.launch({
  headless: process.env.SHELL_CHAT_COMPOSITION_HEADLESS !== '0',
  executablePath: existingChromeExecutable(chromium),
  args: ['--disable-gpu'],
});

try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on('console', (message) => consoleEvents.push({ type: message.type(), text: message.text() }));
  page.on('pageerror', (error) => consoleEvents.push({ type: 'pageerror', text: error?.stack || String(error) }));
  page.on('requestfailed', (request) => consoleEvents.push({ type: 'requestfailed', text: `${request.method()} ${request.url()}` }));

  await page.goto(url, { waitUntil: 'load' });
  await page.waitForFunction(() => window.shellHarness?.ready === true, null, { timeout: 5000 });

  const expanded = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-normal', ...expanded });
  expect(expanded.chatExpanded, 'expanded chat must set the shell composition state');
  expect(!expanded.chatSide, 'chat must never move into a right-hand side rail');
  expect(!expanded.chatCompact, 'desktop chat must retain its conversation stage');
  expect(closeRect(expanded.window, expanded.baselineWindow), `expanding chat must not move a normal window: ${JSON.stringify({ actual: expanded.window, expected: expanded.baselineWindow })}`);
  expect(expanded.chat.x >= 0 && expanded.chat.right <= expanded.viewport.width, 'chat root must remain inside the viewport');
  expect(expanded.dock.bottom <= expanded.viewport.height, 'chat dock must remain anchored inside the bottom viewport edge');
  expect(expanded.bottomAppSwitcherPresent === false, 'bottom app switcher must not exist');

  const windowAction = page.locator('[data-window-action]');
  expect(await windowAction.count() === 1, 'window action locator must be unique');
  await windowAction.click();
  expect(await page.evaluate(() => window.shellHarness.windowClicks) === 1, 'real pointer click must reach the window action');

  const topAppTab = page.locator('[data-top-app-tab]');
  expect(await topAppTab.count() === 1, 'top app tab locator must be unique');
  const minimizeAction = page.locator('.shell-window-control--minimize');
  expect(await minimizeAction.count() === 1, 'window minimize locator must be unique');
  await minimizeAction.click();
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.shell-window')).display === 'none');
  await topAppTab.click();
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.shell-window')).display !== 'none');
  await page.waitForSelector('[data-top-app-tab][data-state="focused"]');
  const restoredFromTopTab = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-restored-from-top-tab', ...restoredFromTopTab });
  expect(closeRect(restoredFromTopTab.window, restoredFromTopTab.baselineWindow), 'restoring from the top tab must preserve normal window geometry');
  expect(!restoredFromTopTab.chatSide, 'restoring an app must not move chat to the side');
  await minimizeAction.click();
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.shell-window')).display === 'none');
  await topAppTab.focus();
  await topAppTab.press('Enter');
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.shell-window')).display !== 'none');
  await page.waitForSelector('[data-top-app-tab][data-state="focused"]');
  await page.screenshot({ path: screenshotPath, fullPage: true });

  const chatToggle = page.locator('[data-chat-open]');
  expect(await chatToggle.count() === 1, 'chat toggle locator must be unique');
  await chatToggle.click();
  await page.waitForFunction(() => !document.body.hasAttribute('data-shell-chat-dock-expanded'));
  const collapsed = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'collapsed-restored', ...collapsed });
  expect(!collapsed.chatExpanded, 'collapsed chat must clear the shell composition state');
  expect(closeRect(collapsed.window, collapsed.baselineWindow), `normal window geometry must restore after collapse: ${JSON.stringify({ actual: collapsed.window, expected: collapsed.baselineWindow })}`);

  await chatToggle.click();
  await page.waitForFunction(() => document.body.hasAttribute('data-shell-chat-dock-expanded'));
  await page.evaluate(() => window.shellHarness.maximize());
  const maximized = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-maximized', ...maximized });
  expect(maximized.overlap.windowDock === 0, 'maximized window must stop above the persistent chat dock');
  expect(maximized.window.right <= maximized.viewport.width, 'maximized window must use the full shell width');

  await page.evaluate(() => window.shellHarness.snapBottom());
  const snapped = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-bottom-snap', ...snapped });
  expect(snapped.overlap.windowDock === 0, 'bottom-snapped window must stop above the persistent chat dock');
  expect(snapped.window.height >= 199, `bottom snap must preserve the minimum window height, got ${snapped.window.height}`);

  const layoutEventsBeforeResize = await page.evaluate(() => window.shellHarness.layoutEvents);
  await page.setViewportSize({ width: 900, height: 600 });
  await page.waitForFunction((previous) => window.shellHarness.layoutEvents > previous, layoutEventsBeforeResize);
  const resized = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'expanded-resized-viewport', ...resized });
  expect(resized.overlap.windowDock === 0, 'resized snapped window must retain zero window/dock overlap');
  expect(!resized.chatSide && !resized.chatCompact, 'resizing must not switch chat into an alternate rail mode');
  expect(resized.chat.right <= resized.viewport.width, 'resized chat must remain inside the viewport');

  await page.setViewportSize({ width: 1200, height: 620 });
  await page.waitForFunction(() => window.shellHarness.collect().viewport.width === 1200);
  await page.evaluate(() => window.shellHarness.addInactiveChatClones());
  const chatWindows = await page.evaluate(() => window.shellHarness.collectChatWindows());
  observations.push({ phase: 'multi-chat-bottom-dock', chats: chatWindows });
  expect(chatWindows.length >= 1, 'bottom chat stage must retain at least the active conversation');
  for (const chat of chatWindows.filter((entry) => entry.active)) {
    expect(chat.x >= 0 && chat.right <= 1200, `active chat must remain inside viewport: ${JSON.stringify(chat)}`);
  }

  await page.evaluate(() => window.shellHarness.restoreNormal());
  await dragWindowHeaderTo(page, 1, null);
  const leftSnap = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-snap-left', ...leftSnap });
  expect(leftSnap.snapZone === 'left', `straight horizontal drag must snap left, got ${leftSnap.snapZone}`);

  await page.evaluate(() => window.shellHarness.restoreNormal());
  await dragWindowHeaderTo(page, 1199, null);
  const rightSnap = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-snap-right', ...rightSnap });
  expect(rightSnap.snapZone === 'right', `straight horizontal drag must snap right, got ${rightSnap.snapZone}`);

  await page.evaluate(() => window.shellHarness.restoreNormal());
  const surfaceTop = await page.locator('[data-surface]').evaluate((node) => node.getBoundingClientRect().top);
  await dragWindowHeaderTo(page, 600, surfaceTop + 180);
  const freelyMoved = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-free-move', ...freelyMoved });
  expect(freelyMoved.snapZone === null, 'moving a window inside the desktop must not force a snap');
  await dragWindowHeaderTo(page, null, surfaceTop + 1);
  const topSnap = await page.evaluate(() => window.shellHarness.collect());
  observations.push({ phase: 'drag-snap-top', ...topSnap });
  expect(topSnap.snapZone === 'top', `vertical drag must snap top, got ${topSnap.snapZone}`);

  const fatalConsole = consoleEvents.filter((event) => ['pageerror', 'requestfailed', 'error'].includes(event.type));
  expect(fatalConsole.length === 0, `browser console/network must stay clean: ${JSON.stringify(fatalConsole)}`);

  fs.writeFileSync(reportPath, JSON.stringify({
    ok: failures.length === 0,
    failures,
    observations,
    consoleEvents,
    screenshotPath,
  }, null, 2));

  if (failures.length) {
    console.error(JSON.stringify({ ok: false, failures, reportPath, screenshotPath }, null, 2));
    process.exitCode = 1;
  } else {
    console.log(JSON.stringify({ ok: true, reportPath, screenshotPath, phases: observations.length }, null, 2));
  }
} finally {
  await browser.close().catch(() => {});
  await new Promise((resolve) => server.close(resolve));
}

function expect(condition, message) {
  if (!condition) failures.push(message);
}

function closeRect(actual, expected, tolerance = 1) {
  return ['x', 'y', 'width', 'height'].every((key) => Math.abs(actual[key] - expected[key]) <= tolerance);
}

async function dragWindowHeaderTo(page, targetX, targetY) {
  const header = page.locator('.shell-window [data-window-header]');
  const box = await header.boundingBox();
  if (!box) throw new Error('window header is not visible');
  const startX = box.x + Math.min(box.width * 0.55, 420);
  const startY = box.y + box.height / 2;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(targetX ?? startX, targetY ?? startY, { steps: 12 });
  await page.mouse.up();
  await page.waitForTimeout(80);
}

function serveRequest(request, response) {
  const requestUrl = new URL(request.url || '/', 'http://localhost');
  if (requestUrl.pathname === '/favicon.ico') {
    response.writeHead(204).end();
    return;
  }
  if (requestUrl.pathname === '/') {
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end(harnessHtml());
    return;
  }
  const filePath = path.normalize(path.join(repoRoot, decodeURIComponent(requestUrl.pathname)));
  if (!filePath.startsWith(repoRoot) || !fs.existsSync(filePath)) {
    response.writeHead(404, { 'Content-Type': 'text/plain' });
    response.end('not found');
    return;
  }
  const contentTypes = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml' };
  response.writeHead(200, { 'Content-Type': contentTypes[path.extname(filePath)] || 'application/octet-stream' });
  response.end(fs.readFileSync(filePath));
}

function listen(serverInstance) {
  return new Promise((resolve) => serverInstance.listen(0, '127.0.0.1', () => resolve(serverInstance.address().port)));
}

function resolvePlaywrightModule() {
  for (const candidate of [
    process.env.PLAYWRIGHT_MODULE_PATH,
    'playwright',
    path.join(repoRoot, 'src/apps/business-os/node_modules/playwright'),
    '/tmp/ctox-pw-smoke/node_modules/playwright',
    '/tmp/ctox-chatbar-pw/node_modules/playwright',
  ].filter(Boolean)) {
    try { return require.resolve(candidate); } catch {}
  }
  throw new Error('No Playwright runtime found. Set PLAYWRIGHT_MODULE_PATH.');
}

function existingChromeExecutable(chromiumRuntime) {
  return [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
    chromiumRuntime.executablePath?.(),
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
  ].filter(Boolean).find((candidate) => fs.existsSync(candidate));
}

function timestampForPath() {
  return new Date().toISOString().replace(/[:.]/g, '-');
}

function harnessHtml() {
  return `<!doctype html>
<html lang="de">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="stylesheet" href="/src/apps/business-os/app.css">
  <style>
    :root { --bg:#111315; --surface:#171a1d; --surface-2:#1d2125; --line:#30363b; --text:#e6e9eb; --muted:#9ba4aa; --accent:#72b8aa; --accent-soft:#173c38; --hairline:#2a3035; --panel-shadow:none; }
    html, body { margin:0; width:100%; height:100%; overflow:hidden; background:var(--bg); }
    .workspace-frame { position:fixed; inset:52px 0 0; display:block; }
    .harness-topbar { position:fixed; inset:0 0 auto; height:52px; display:flex; align-items:center; padding:0 8px; border-bottom:1px solid var(--line); background:var(--surface); }
    .harness-topbar button { min-height:36px; }
    .harness-window-content { display:flex; align-items:flex-start; justify-content:center; height:100%; padding:12px; }
    .harness-window-content button { min-height:32px; }
  </style>
</head>
<body>
  <header class="harness-topbar"><button type="button" data-top-app-tab>Testfenster</button></header>
  <main class="workspace-frame" data-surface></main>
  <div class="shell-window-layer" data-window-layer><div class="shell-snap-preview" data-snap-preview hidden></div></div>
  <script type="module">
    import { initBusinessChat } from '/src/apps/business-os/shared/business-chat.js';
    import { createEventBus } from '/src/apps/business-os/shared/event-bus.js';
    import { createWindowManager } from '/src/apps/business-os/shared/window-manager.js';
    import { createShellChatCompositionController } from '/src/apps/business-os/shared/shell-chat-composition.js';

    const owner = 'composition-user';
    const chat = {
      id:'chat_composition', title:'Dock composition', open:true, minimized:false, maximized:false,
      owner_user_id:owner, messages:[], draft:'', contextMeta:{ module:'threads' },
      createdAt:Date.now(), updated_at_ms:Date.now(), attachments:[], showFollowUp:false,
    };
    localStorage.setItem('ctox.businessOs.chat.v1', JSON.stringify({
      selectedDate: localDateString(new Date()), activeChatId:chat.id, dockCollapsed:false, chats:[chat],
    }));

    let layoutEvents = 0;
    window.addEventListener('ctox-business-os-chat-layout', () => { layoutEvents += 1; });
    const eventBus = createEventBus();
    const wm = createWindowManager({
      windowLayer:document.querySelector('[data-window-layer]'),
      surfaceEl:document.querySelector('[data-surface]'),
      rootEl:document.documentElement,
      snapPreviewEl:document.querySelector('[data-snap-preview]'),
      eventBus,
    });
    wm.setInsets({ top:0, right:0, bottom:0, left:0 });
    const controller = createShellChatCompositionController({ windowManager:wm });
    controller.start();
    ['window:opened','window:closed','window:minimized','window:restored'].forEach((name) => eventBus.on(name, () => controller.refresh()));
    const handle = wm.create({
      ownerId:'module:test', title:'Testfenster', x:80, y:60, width:1000, height:610, minWidth:640, minHeight:480,
      content:'<div class="harness-window-content"><button type="button" data-window-action>Freigabe ausführen</button></div>',
    });
    let windowClicks = 0;
    document.querySelector('[data-window-action]').addEventListener('click', () => { windowClicks += 1; });
    const topAppTab = document.querySelector('[data-top-app-tab]');
    const syncTopAppTab = () => {
      const state = wm.describe(handle.id)?.state;
      topAppTab.dataset.state = state === 'minimized' ? 'running' : 'focused';
    };
    topAppTab.addEventListener('click', () => {
      const state = wm.describe(handle.id)?.state;
      if (state === 'minimized') {
        wm.restore(handle.id);
        wm.focus(handle.id);
      } else {
        wm.focus(handle.id);
      }
      syncTopAppTab();
    });
    ['window:minimized','window:restored','window:focused'].forEach((name) => eventBus.on(name, syncTopAppTab));
    syncTopAppTab();
    const baselineWindow = box(document.querySelector('.shell-window'));

    initBusinessChat({
      session:{ authenticated:true, user:{ id:owner, name:'Composition User' } },
      commandBus:{ dispatch:async () => ({ command_id:'cmd_test', task_id:'task_test', status:'queued' }) },
      db:makeDb(chat),
      getActiveModule:() => ({ id:'threads', name:'Threads' }),
    });

    window.shellHarness = {
      ready:false,
      get windowClicks(){ return windowClicks; },
      get layoutEvents(){ return layoutEvents; },
      maximize(){ if (wm.describe(handle.id)?.state !== 'maximized') wm.toggleMaximize(handle.id); },
      snapBottom(){ if (wm.describe(handle.id)?.state === 'maximized') wm.toggleMaximize(handle.id); wm.snapTo(handle.id, 'bottom'); },
      restoreNormal(){ const el=document.querySelector('.shell-window'); if(wm.describe(handle.id)?.state==='maximized'){ wm.toggleMaximize(handle.id); } else if(el?.classList.contains('is-snapped')){ wm.toggleMaximize(handle.id); wm.toggleMaximize(handle.id); } },
      collect,
      addInactiveChatClones(){ const active=document.querySelector('.ctox-chat-window.is-active'); if(!active) return; ['left','right'].forEach((rel,index) => { const clone=active.cloneNode(true); clone.classList.remove('is-active'); clone.dataset.chatId='clone_'+rel; clone.dataset.chatRel=rel; clone.style.left=(700+index*400)+'px'; active.parentElement.appendChild(clone); }); },
      collectChatWindows(){ return [...document.querySelectorAll('.ctox-chat-window')].filter((node) => { const style=getComputedStyle(node); const rect=node.getBoundingClientRect(); return style.display!=='none' && rect.width>0 && rect.height>0; }).map((node) => ({...box(node),active:node.classList.contains('is-active')})); },
    };

    waitFor(() => document.body.hasAttribute('data-shell-chat-dock-expanded')).then(() => { window.shellHarness.ready = true; });

    function collect() {
      const windowRect=box(document.querySelector('.shell-window'));
      const chatRect=box(document.querySelector('[data-ctox-chat-root]'));
      const dockRect=box(document.querySelector('[data-chat-dock]'));
      return {
        viewport:{ width:innerWidth, height:innerHeight }, baselineWindow,
        window:windowRect, chat:chatRect, dock:dockRect,
        bottomAppSwitcherPresent:Boolean(document.querySelector('[data-shell-taskbar], .shell-taskbar')),
        chatExpanded:document.body.hasAttribute('data-shell-chat-dock-expanded'),
        chatSide:document.body.hasAttribute('data-shell-chat-dock-side'),
        chatCompact:document.body.hasAttribute('data-shell-chat-dock-compact'),
        snapZone:document.querySelector('.shell-window')?.dataset.snapZone || null,
        overlap:{
          windowChat:intersection(windowRect, chatRect),
          windowDock:intersection(windowRect, dockRect),
        },
      };
    }

    function makeDb(initial) {
      let value=structuredClone(initial);
      const doc=() => ({ toJSON:() => structuredClone(value), incrementalPatch:async (next) => { value={...value,...structuredClone(next)}; } });
      return { raw:{
        business_chats:{ $:{ subscribe:() => ({ unsubscribe(){} }) }, find:() => ({ exec:async () => [doc()] }), findOne:() => ({ exec:async () => doc() }), insert:async (next) => { value=structuredClone(next); return doc(); } },
        business_commands:{ $:{ subscribe:() => ({ unsubscribe(){} }) } },
        ctox_queue_tasks:{ $:{ subscribe:() => ({ unsubscribe(){} }) } },
      } };
    }
    function box(node){ const r=node?.getBoundingClientRect?.(); return r ? { x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom } : { x:0,y:0,width:0,height:0,right:0,bottom:0 }; }
    function intersection(a,b){ return Math.max(0,Math.min(a.right,b.right)-Math.max(a.x,b.x))*Math.max(0,Math.min(a.bottom,b.bottom)-Math.max(a.y,b.y)); }
    function localDateString(date){ return date.getFullYear()+'-'+String(date.getMonth()+1).padStart(2,'0')+'-'+String(date.getDate()).padStart(2,'0'); }
    async function waitFor(predicate){ const end=Date.now()+4000; while(Date.now()<end){ if(predicate()) return; await new Promise((resolve)=>setTimeout(resolve,16)); } throw new Error('composition timeout'); }
  </script>
</body>
</html>`;
}
