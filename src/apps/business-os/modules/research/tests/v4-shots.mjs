// Screenshots der App gegen den Kern-Stub (Pin 8265bd7) in 1280 und 420 Breite → tests/shots/.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node src/apps/business-os/modules/research/tests/v4-shots.mjs
import { fileURLToPath } from 'node:url';
import { chromium, startServer, seedKernel, pageHtml } from './v4-fixture.mjs';

const OUT = fileURLToPath(new URL('./shots/', import.meta.url));
const errors = [];
for (const [width, height] of [[1280, 800], [420, 820]]) {
  const { server, base } = await startServer(pageHtml(width, height, { shell: true }));
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width, height } });
  page.on('pageerror', (e) => errors.push(`${width}: ${e.message}`));
  await page.goto(base);
  await seedKernel(page);
  await page.evaluate(async () => { window.__app = await window.__mod.mount(document.getElementById('win'), window.__api, {}); });
  await page.evaluate(() => window.__k.sim.settle());
  await page.waitForTimeout(1500);
  const geometry = await page.evaluate(() => {
    const win = document.getElementById('win').getBoundingClientRect();
    const over = [...document.querySelectorAll('#win *')].filter((el) => {
      const r = el.getBoundingClientRect();
      return r.width > 0 && (r.right > win.right + 1 || r.left < win.left - 1);
    }).map((el) => el.className || el.tagName).slice(0, 8);
    return { scrollW: document.getElementById('win').scrollWidth, width: win.width, overflow: over };
  });
  console.log(width, JSON.stringify(geometry));
  await page.screenshot({ path: `${OUT}research-${width}.png` });
  await page.evaluate(() => document.querySelector('.research-left [data-action="new-task"]')?.click());
  await page.waitForTimeout(200);
  await page.screenshot({ path: `${OUT}research-${width}-dialog.png` });
  await browser.close();
  await new Promise((r) => server.close(r));
}
if (errors.length) { console.error(errors); process.exit(1); }
