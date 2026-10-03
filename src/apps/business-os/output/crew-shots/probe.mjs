import { chromium } from 'playwright';
const out = process.env.OUT || '/Users/michaelwelsch/.cache/ctox-crew-shots';
const browser = await chromium.launch({ headless: true, args: ['--disable-gpu'] });
const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: 'dark', locale: 'de-DE' });
const page = await ctx.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(String(e.message).slice(0, 200)));
await page.goto('http://127.0.0.1:8765/', { waitUntil: 'domcontentloaded' });
const started = Date.now();
let last = null;
while (Date.now() - started < 90000) {
  last = await page.evaluate(async () => {
    const app = window.CTOX_BUSINESS_OS_APP; const d = app?.sync?.diagnostics || {}; const c = d.collections || {};
    const q = c.ctox_queue_tasks || {};
    let tasks = null; try { const col = app?.db?.raw?.ctox_queue_tasks; if (col) tasks = (await Promise.race([col.find({ limit: 200 }).exec(), new Promise(r => setTimeout(() => r(null), 3000))]))?.length ?? 'timeout'; } catch (e) { tasks = 'err ' + e.message; }
    let crew = null; try { const col = app?.db?.raw?.ctox_crew_members; if (col) crew = (await Promise.race([col.find({ limit: 50 }).exec(), new Promise(r => setTimeout(() => r(null), 3000))]))?.length ?? 'timeout'; } catch (e) { crew = 'err ' + e.message; }
    return { phase: d.phase, build: [...document.scripts].map(s => s.src).find(s => s.includes('app.js'))?.split('v=')[1], apc: q.frameTransport?.activePeerCount, crs: q.frameTransport?.collectionReadinessState, irs: q.initialReplicationState, tasks, crew };
  }).catch((e) => ({ error: e.message }));
  if (last && last.apc > 0 && typeof last.tasks === 'number' && last.tasks > 0) break;
  await page.waitForTimeout(5000);
}
console.log(JSON.stringify({ elapsedMs: Date.now() - started, last, errors: errors.slice(0, 5) }));
await page.screenshot({ path: `${out}/desktop.png` });
await browser.close();
