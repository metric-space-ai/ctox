// Drive the headed Chrome on :9222 (own profile) against the local instance.
import { chromium } from 'playwright';
const out = process.env.OUT || '/Users/michaelwelsch/.cache/ctox-crew-shots';
const step = process.argv[2] || 'probe';
const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const ctx = browser.contexts()[0];
let page = ctx.pages().find((p) => p.url().startsWith('http://127.0.0.1:8765')) || ctx.pages()[0];
if (!page) page = await ctx.newPage();
if (!page.url().startsWith('http://127.0.0.1:8765')) await page.goto('http://127.0.0.1:8765/', { waitUntil: 'domcontentloaded' });
const snap = () => page.evaluate(async () => {
  const app = window.CTOX_BUSINESS_OS_APP; const d = app?.sync?.diagnostics || {}; const c = d.collections || {}; const q = c.ctox_queue_tasks || {};
  const t = (p) => Promise.race([p, new Promise((r) => setTimeout(() => r(null), 3000))]);
  const count = async (n) => { try { const col = app?.db?.raw?.[n]; if (!col) return 'none'; const r = await t(col.find({ limit: 300 }).exec()); return Array.isArray(r) ? r.length : 'timeout'; } catch (e) { return 'err ' + e.message; } };
  return { phase: d.phase, build: [...document.scripts].map((s) => s.src).find((s) => s.includes('app.js'))?.split('v=')[1], apc: q.frameTransport?.activePeerCount, crs: q.frameTransport?.collectionReadinessState, irs: q.initialReplicationState, tasks: await count('ctox_queue_tasks'), crew: await count('ctox_crew_members'), status: await count('ctox_harness_status') };
});
if (step === 'probe') {
  await page.goto('http://127.0.0.1:8765/', { waitUntil: 'domcontentloaded' });
  const started = Date.now(); let last = null;
  while (Date.now() - started < 120000) {
    last = await snap().catch((e) => ({ error: e.message }));
    if (last && typeof last.crew === 'number' && last.crew > 0 && typeof last.tasks === 'number') break;
    await page.waitForTimeout(5000);
  }
  console.log(JSON.stringify({ elapsedMs: Date.now() - started, last }));
  await page.screenshot({ path: `${out}/desktop.png` });
}
if (step === 'shot') {
  await page.screenshot({ path: `${out}/${process.argv[3] || 'shot'}.png` });
  console.log(JSON.stringify(await snap()));
}
await browser.close();
