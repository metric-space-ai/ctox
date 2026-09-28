#!/usr/bin/env node
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const outputDir = path.join(repoRoot, 'output/playwright', 'ctox-crew-map');
fs.mkdirSync(outputDir, { recursive: true });

const { chromium } = require(resolvePlaywrightModule());
const consoleErrors = [];
const server = createServer((request, response) => serve(request, response));
const port = await new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server.address().port)));
const browser = await chromium.launch({ headless: true, executablePath: existingChromeExecutable(chromium), args: ['--disable-gpu'] });

try {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('pageerror', (error) => consoleErrors.push(error.message || String(error)));
  page.on('requestfailed', (request) => consoleErrors.push(`${request.method()} ${request.url()} ${request.failure()?.errorText || ''}`));

  const url = `http://127.0.0.1:${port}/`;
  await page.goto(url, { waitUntil: 'domcontentloaded' });
  const first = await collectSelections(page, true);
  await assertReducedMotionStops(page);
  await page.emulateMedia({ reducedMotion: 'no-preference' });

  await page.reload({ waitUntil: 'domcontentloaded' });
  const reloaded = await collectSelections(page, false);
  for (let i = 0; i < first.length; i++) {
    if (first[i].identityColor !== reloaded[i].identityColor) throw new Error('crew identity changed after a clean reload');
  }
  if (consoleErrors.length) throw new Error(`browser console/network errors: ${consoleErrors.join(' | ')}`);

  const report = { ok: true, url, first, reloaded, consoleErrors };
  fs.writeFileSync(path.join(outputDir, 'ctox-crew-map.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ ok: true, scenarios: 7, outputDir }, null, 2));
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}

async function collectSelections(page, screenshots) {
  const results = [];
  for (const selected of ['task-working', 'task-waiting', 'task-failed']) {
    const result = await collect(page, selected);
    assertResult(result);
    await assertMotionSettles(page);
    if (screenshots) await page.screenshot({ path: path.join(outputDir, `${selected}.png`), fullPage: true });
    results.push(result);
  }
  return results;
}

async function collect(page, selected) {
  await page.waitForFunction(() => Boolean(window.__ctoxCrewReady));
  await page.evaluate((id) => window.__selectCrewTask(id), selected);
  await page.evaluate(() => window.__triggerCrewTurn?.());
  await page.waitForTimeout(220);
  return page.evaluate((selected) => {
    const snapshot = window.__ctoxCrewMotionEngine?.snapshot?.() || [];
    const impulsesFor = (node) => snapshot.find((actor) => actor.node === node)?.impulses || [];
    const slots = Array.from(document.querySelectorAll('.ctox-flow-creature-slot'));
    const byTask = Object.fromEntries(slots.map((slot) => [slot.dataset.taskId, {
      node: slot.dataset.creatureNodeId,
      mode: slot.querySelector('.ctox-crew-creature')?.dataset.crewMode || '',
      impulses: impulsesFor(slot.querySelector('.ctox-crew-creature')),
      activityTurns: slot.querySelector('.ctox-crew-creature')?.dataset.activityTurns || '',
      activityUpdatedAt: slot.querySelector('.ctox-crew-creature')?.dataset.activityUpdatedAt || '',
      xEyes: slot.querySelectorAll('.ctox-crew-eyes-x path').length,
    }]));
    document.querySelector(`[data-task-id="${selected}"]`)?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    const focusedAfterClick = window.__focusedTask || '';
    document.querySelector('[data-task-id="task-working"]')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    return {
      selected,
      selectedIds: slots.filter(slot => slot.classList.contains('is-selected')).map(slot => slot.dataset.taskId),
      count: slots.length,
      byTask,
      focusedAfterClick,
      focusedAfterKeyboard: window.__focusedTask || '',
      visibleTaskId: document.querySelector('.ctox-chat-track code')?.textContent || '',
      fullTaskId: document.querySelector('.ctox-chat-track')?.dataset.taskId || '',
      identityColor: document.querySelector('[data-task-id="task-working"] .ctox-crew-creature')?.style.getPropertyValue('--crew-color') || '',
      chatIdentityColor: document.querySelector('[data-chat-creature] .ctox-crew-ref')?.style.getPropertyValue('--crew-color') || '',
      chatHasCreatureCopy: Boolean(document.querySelector('[data-chat-creature] .ctox-crew-creature')),
      miloBeings: Array.from(document.querySelectorAll('.ctox-flow-creature-slot .ctox-crew-creature')).filter((node) => node.style.getPropertyValue('--crew-color') === '#00aa9a').length,
      miloCount: document.querySelector('[data-crew-pos-key="member:crew:milo"]')?.dataset.crewCount || '',
      seatAway: Object.fromEntries(Array.from(document.querySelectorAll('#bar [data-seat]')).map((seat) => [seat.dataset.seat, seat.querySelector('.ctox-crew-creature')?.dataset.crewAway === 'true'])),
      visibilityState: document.visibilityState,
      motionRunning: Boolean(window.__ctoxCrewMotionEngine?.running),
    };
  }, selected);
}

// A turn gesture is finite: after it the working creature returns to its
// continuous working pose (the engine keeps running for the base motion).
async function assertMotionSettles(page) {
  await page.waitForTimeout(1500);
  const settled = await page.evaluate(() => {
    const node = document.querySelector('[data-task-id="task-working"] .ctox-crew-creature');
    const actor = (window.__ctoxCrewMotionEngine?.snapshot?.() || []).find((entry) => entry.node === node);
    return { impulses: actor?.impulses || null, mode: actor?.mode || '', moving: Boolean(actor?.transform) };
  });
  if (!settled.impulses || settled.impulses.length || settled.mode !== 'working' || !settled.moving) {
    throw new Error(`turn gesture did not settle back into the working pose: ${JSON.stringify(settled)}`);
  }
}

async function assertReducedMotionStops(page) {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.waitForTimeout(200);
  const state = await page.evaluate(() => ({
    running: Boolean(window.__ctoxCrewMotionEngine?.running),
    transforms: Array.from(document.querySelectorAll('.ctox-crew-figure')).filter((node) => node.style.transform).length,
  }));
  if (state.running || state.transforms) throw new Error(`reduced motion must stop all creature motion: ${JSON.stringify(state)}`);
}

function assertResult(result) {
  const expected = result.selected === 'task-working' ? ['task-working'] : ['task-working', result.selected];
  if (result.count !== expected.length || Object.keys(result.byTask).some(id => !expected.includes(id))) {
    throw new Error(`expected selected task plus running tasks only: ${JSON.stringify(result)}`);
  }
  if (result.selectedIds.length !== 1 || result.selectedIds[0] !== result.selected) throw new Error('selected creature is not marked');
  if (result.selected === 'task-waiting' && (result.byTask['task-waiting']?.node !== 'queued' || result.byTask['task-waiting']?.mode !== 'sleeping')) throw new Error('waiting creature is not sleeping at queued');
  if (result.byTask['task-working']?.node !== 'running' || result.byTask['task-working']?.mode !== 'working') throw new Error('working creature is not active at running');
  if (!result.byTask['task-working']?.impulses?.includes('tool')) throw new Error(`fresh durable tool turn did not trigger a finite creature impulse: ${JSON.stringify(result)}`);
  if (result.byTask['task-waiting']?.impulses?.length) throw new Error('a waiting creature gets no activity gesture without a durable turn');
  if (!result.motionRunning) throw new Error('visible working crew must be animated');
  if (result.selected === 'task-failed' && (result.byTask['task-failed']?.node !== 'model-failed' || result.byTask['task-failed']?.xEyes !== 2)) throw new Error('failed creature lacks the failed node or X eyes');
  if (result.focusedAfterClick !== result.selected || result.focusedAfterKeyboard !== 'task-working') throw new Error('map creature selection is not mouse/keyboard reachable');
  if (result.fullTaskId !== 'queue:system::task_1234567890abcdef' || !result.visibleTaskId.startsWith('…')) throw new Error('chat task id deep-link is missing');
  if (!result.identityColor || result.identityColor !== result.chatIdentityColor) throw new Error('chat and map do not share the same creature identity');
  if (result.chatHasCreatureCopy) throw new Error('a chat names its member with a reference badge, never a creature copy');
  // One living body per member per screen: whoever stands on the map shows
  // only its still portrait in the crew bar; everyone else lives in the bar.
  const onMap = { 'crew:milo': true, 'crew:nori': result.selected === 'task-waiting', 'crew:tavi': result.selected === 'task-failed' };
  for (const [member, expected] of Object.entries(onMap)) {
    if (result.seatAway[member] !== expected) throw new Error(`crew bar seat of ${member} must ${expected ? 'show the portrait while its body is on the map' : 'keep the living body'}: ${JSON.stringify(result.seatAway)}`);
  }
  if (result.miloBeings !== 1 || result.miloCount !== '2') throw new Error(`a member with two running tasks must stand on the map exactly once with a count of 2: ${JSON.stringify({ beings: result.miloBeings, count: result.miloCount })}`);
}

function serve(request, response) {
  const requestUrl = new URL(request.url || '/', 'http://localhost');
  if (requestUrl.pathname === '/favicon.ico') {
    response.writeHead(204);
    response.end();
    return;
  }
  if (requestUrl.pathname === '/') {
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end(harnessHtml());
    return;
  }
  const filePath = path.normalize(path.join(repoRoot, decodeURIComponent(requestUrl.pathname)));
  if (!filePath.startsWith(repoRoot) || !fs.existsSync(filePath)) {
    response.writeHead(404);
    response.end('not found');
    return;
  }
  const contentType = filePath.endsWith('.css') ? 'text/css' : filePath.endsWith('.json') ? 'application/json' : 'text/javascript';
  response.writeHead(200, { 'Content-Type': contentType });
  response.end(fs.readFileSync(filePath));
}

function harnessHtml() {
  return `<!doctype html><html lang="de"><head><meta charset="utf-8"><style>
    :root{--background:#080d10;--surface:#10181d;--text:#dce7ea;--muted:#718187;--accent:#1685ee;--success:#34a26f;--danger:#e75c62;--ctox-flow-node-fill:#10181d;--ctox-flow-node-stroke:#314047;--ctox-flow-lane-fill:#0b1216;--ctox-flow-lane-stroke:#213039;--ctox-flow-muted-fill:#718187;--ctox-flow-edge:#314047}
    body{margin:0;background:var(--background);color:var(--text);font-family:system-ui}main{width:1200px;margin:40px auto}svg{width:100%;height:660px}.proof-only{position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%)}.demo-node{fill:var(--surface);stroke:#314047}.demo-label{fill:var(--text);font:700 18px system-ui;text-anchor:middle}
  </style><link rel="stylesheet" href="/src/apps/business-os/modules/ctox/index.css"></head><body><main><svg viewBox="0 0 1000 620"><g><rect class="demo-node" x="110" y="202" width="140" height="76" rx="12"/><text class="demo-label" x="180" y="246">Queue</text><rect class="demo-node" x="430" y="202" width="140" height="76" rx="12"/><text class="demo-label" x="500" y="246">Working</text><rect class="demo-node" x="750" y="402" width="140" height="76" rx="12"/><text class="demo-label" x="820" y="446">Failed</text></g><g id="crew"></g></svg><div class="proof-only" id="chat"></div><div class="proof-only" data-chat-creature id="chat-creature"></div><div id="bar" style="display:flex;gap:6px;height:32px"></div></main><script type="module">
    import { __ctoxTestHooks } from '/src/apps/business-os/modules/ctox/index.js?v=20260831-crew-telemetry-v331';
    import { __businessChatTestInternals, crewReferenceHtml, crewCreatureHtml, syncCrewProceduralMotion } from '/src/apps/business-os/shared/business-chat.js?v=20260831-crew-telemetry-v331';
    const crewMembers=[{id:'crew:milo',name:'Milo',shape:'blob',color:'#00aa9a',archived:false},{id:'crew:nori',name:'Nori',shape:'square',color:'#7c6df2',archived:false},{id:'crew:tavi',name:'Tavi',shape:'triangle',color:'#e97255',archived:false}];
    const working={id:'task-working',commandId:'cmd-working',crewMemberId:'crew:milo',title:'Working task',status:'running',executionPhase:'running',executionProgress:{version:1,revision:1,phase:'work',percent:45,current_step:2,completed_steps:1,total_steps:2,steps:[{position:1,label:'Collect',status:'completed',activity_turns:1},{position:2,label:'Verify',status:'in_progress',activity_turns:1}],review:{status:'pending'},activity_turns:{total:2,thinking:1,tools:1,last_kind:'tool'},updated_at_ms:Date.now()-10000}};
    // Milo has a second running task: he is still ONE being on the map (Owner 28.09.2026).
    const working2={...working,id:'task-working-2',commandId:'cmd-working-2',title:'Second working task'};
    const waiting={id:'task-waiting',commandId:'cmd-waiting',crewMemberId:'crew:nori',title:'Waiting task',status:'queued',executionPhase:'queued'};
    const failed={id:'task-failed',commandId:'cmd-failed',crewMemberId:'crew:tavi',title:'Failed task',status:'failed',executionPhase:'terminal',terminalStatus:'failed'};
    const model={activeTask:working,activeNodeId:'running',tasks:[working,working2,waiting,failed],nodeMap:new Map([['queued',{id:'queued',x:180,y:240}],['running',{id:'running',x:500,y:240}],['model-failed',{id:'model-failed',x:820,y:440}]])};
    window.__selectCrewTask=(id)=>{
      const selected=model.tasks.find(task=>task.id===id);
      if(!selected) throw new Error('unknown fixture task: '+id);
      document.querySelector('#crew').innerHTML=__ctoxTestHooks.flowCrewSvg(model,selected,{lang:'de',crewMembers});
      document.querySelectorAll('.ctox-flow-creature-slot').forEach((slot)=>{const select=()=>window.__focusedTask=slot.dataset.taskId;slot.addEventListener('click',select);slot.addEventListener('keydown',(event)=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();select()}})});
      syncCrewProceduralMotion(document.querySelector('main'));
    };
    // The crew bar: one seat per member, like the shell dock.
    document.querySelector('#bar').innerHTML=crewMembers.map((m)=>'<span data-seat="'+m.id+'" style="display:inline-grid;width:28px;height:28px">'+crewCreatureHtml({id:'seat-'+m.id,crewKey:m.id,crewIdentity:{id:m.id,name:m.name,shape:m.shape,color:m.color}},'idle','fab')+'</span>').join('');
    window.__selectCrewTask(working.id);
    document.querySelector('#chat').innerHTML=__businessChatTestInternals.messageMarkup({id:'m1',role:'ctox',text:'Recherche gestartet.',taskId:'queue:system::task_1234567890abcdef',commandId:'cmd-working',status:'running'});
    document.querySelector('#chat-creature').innerHTML=crewReferenceHtml({id:'chat-random',crewIdentity:{id:'crew:milo',name:'Milo',shape:'blob',color:'#00aa9a'},messages:[{commandId:'cmd-working',taskId:'task-working'}]},26);
    syncCrewProceduralMotion(document.querySelector('main'));
    window.__triggerCrewTurn=()=>{const node=document.querySelector('[data-task-id="task-working"] .ctox-crew-creature');node.dataset.activityTurns=String(Number(node.dataset.activityTurns||0)+1);node.dataset.activityUpdatedAt=String(Date.now());node.dataset.activityKind='tool';syncCrewProceduralMotion(document.querySelector('main'))};
    window.__ctoxCrewReady=true;
  </script></body></html>`;
}

function resolvePlaywrightModule() {
  for (const candidate of [process.env.PLAYWRIGHT_MODULE_PATH, 'playwright', '/tmp/ctox-pw-smoke/node_modules/playwright', '/tmp/ctox-chatbar-pw/node_modules/playwright'].filter(Boolean)) {
    try { return require.resolve(candidate); } catch {}
  }
  throw new Error('No Playwright runtime found');
}

function existingChromeExecutable(chromiumRuntime) {
  return [process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE, chromiumRuntime.executablePath?.(), '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/Applications/Chromium.app/Contents/MacOS/Chromium'].filter(Boolean).find(fs.existsSync);
}
