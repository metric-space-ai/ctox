// Real-data screenshots through the headed Chrome on :9222.
import { chromium } from 'playwright';
const out = process.env.OUT || '/Users/michaelwelsch/.cache/ctox-crew-shots';
const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const ctx = browser.contexts()[0];
let page = ctx.pages().find((p) => p.url().startsWith('http://127.0.0.1:8765')) || (await ctx.newPage());
const log = (label, extra = '') => console.log(`[${new Date().toISOString().slice(11, 19)}] ${label} ${extra}`);
await page.goto('http://127.0.0.1:8765/', { waitUntil: 'domcontentloaded' });
// 1. wait for replication: crew members present in the store
const started = Date.now(); let ready = null;
while (Date.now() - started < 240000) {
  ready = await page.evaluate(async () => {
    const app = window.CTOX_BUSINESS_OS_APP; const t = (p) => Promise.race([p, new Promise((r) => setTimeout(() => r(null), 3000))]);
    const n = async (c) => { const col = app?.db?.raw?.[c]; if (!col) return -1; const r = await t(col.find({ limit: 300 }).exec()); return Array.isArray(r) ? r.length : -2; };
    return { crew: await n('ctox_crew_members'), tasks: await n('ctox_queue_tasks'), phase: app?.sync?.diagnostics?.phase };
  }).catch((e) => ({ error: e.message }));
  if (ready && ready.crew > 0) break;
  await page.waitForTimeout(5000);
}
log('store', JSON.stringify(ready));
// 2. open the CTOX app via the top bar
const openApp = async (name) => {
  const tab = page.locator(`[data-top-app-tab]`, { hasText: name }).first();
  if (await tab.count()) await tab.click(); else await page.getByRole('button', { name, exact: false }).first().click();
  await page.waitForTimeout(4000);
};
await openApp('CTOX');
await page.waitForTimeout(6000);
// tuck away any restored chat window that covers the workspace
await page.evaluate(() => document.querySelectorAll('[data-chat-minimize]').forEach((b) => b.click()));
await page.waitForTimeout(1500);
await page.screenshot({ path: `${out}/01-ctox-app.png` });
log('shot 01', await page.evaluate(() => ({ members: document.querySelectorAll('.ctox-crew-home-member').length, tasks: document.querySelectorAll('[data-ctox-left] [data-task-id]').length, footer: document.querySelector('.ctox-harness-footer')?.textContent?.trim().slice(0, 120) })).then(JSON.stringify));
// 3. profile drawer of the first member (crew home) or via task member button
const member = page.locator('.ctox-crew-home-member, .ctox-task-member, [data-open-crew-member]').first();
if (await member.count()) {
  await member.evaluate((el) => el.click()); await page.waitForTimeout(2500);
  await page.screenshot({ path: `${out}/02-member-profile.png` });
  log('shot 02', await page.evaluate(() => document.querySelector('.ctox-task-drawer, .ctox-detail-drawer, [data-ctox-drawer]')?.textContent?.trim().slice(0, 200)));
  await page.keyboard.press('Escape'); await page.waitForTimeout(800);
}
// 4. a task with its crew member: click first task card
const task = page.locator('[data-ctox-left] [data-task-id]').first();
if (await task.count()) {
  await task.evaluate((el) => el.click()); await page.waitForTimeout(3000);
  await page.screenshot({ path: `${out}/03-task-selected.png` });
}
// 5. tickets app
await openApp('Tickets'); await page.waitForTimeout(5000);
await page.screenshot({ path: `${out}/04-tickets.png` });
// 6. chat bar: open the crew bar (FAB) and screenshot the pool
const fab = page.locator('.ctox-chat-fab').first();
if (await fab.count()) { await fab.click(); await page.waitForTimeout(2000); }
await page.screenshot({ path: `${out}/05-chat-bar.png` });
log('pool', await page.evaluate(() => document.querySelectorAll('[data-crew-drag]').length));
// 7. drag a member from the pool onto the CTOX app window
await openApp('CTOX'); await page.waitForTimeout(3000);
const slot = page.locator('[data-crew-drag]').first();
if (await slot.count()) {
  const box = await slot.boundingBox(); const target = await page.locator('[data-ctox-main]').first().boundingBox();
  if (box && target) {
    const sx = box.x + box.width / 2, sy = box.y + box.height / 2; const tx = target.x + target.width * 0.55, ty = target.y + target.height * 0.45;
    await page.mouse.move(sx, sy); await page.mouse.down(); await page.mouse.move(sx + 12, sy - 12, { steps: 4 });
    for (let i = 1; i <= 12; i++) { await page.mouse.move(sx + (tx - sx) * i / 12, sy + (ty - sy) * i / 12, { steps: 2 }); await page.waitForTimeout(60); }
    await page.screenshot({ path: `${out}/06-drag-ghost.png` });
    await page.mouse.up(); await page.waitForTimeout(1200);
    await page.screenshot({ path: `${out}/07-drop-context-menu.png` });
    log('menu', await page.evaluate(() => ({ menu: !!document.querySelector('.ctox-global-context-menu'), crew: document.querySelector('.ctox-context-crew')?.textContent?.trim().slice(0, 120) })).then(JSON.stringify));
    await page.keyboard.press('Escape');
  }
}
await browser.close();
