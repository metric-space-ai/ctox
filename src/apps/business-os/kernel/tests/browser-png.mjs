// K-1b Punkt 6, Chromium-Gegenprobe: PNG-Artefakt (Bytes) → IndexedDB → kernel.artifacts.read → <img> dekodiert.
// Aufruf: PLAYWRIGHT_MODULE=<playwright-core> CHROME_BIN=<chrome> node src/apps/business-os/kernel/tests/browser-png.mjs
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, extname } from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE ?? 'playwright');
const root = resolve('src/apps/business-os/kernel');
const server = createServer(async (req, res) => {
  try {
    if (req.url === '/') { res.setHeader('Content-Type', 'text/html'); res.end('<!doctype html><meta charset="utf-8"><title>K-1b PNG</title><body></body>'); return; }
    const path = resolve(root, `.${decodeURIComponent(new URL(req.url, 'http://local').pathname)}`);
    if (!path.startsWith(`${root}/`)) throw Error('outside root');
    res.setHeader('Content-Type', extname(path) === '.mjs' ? 'text/javascript' : 'text/plain'); res.end(await readFile(path));
  } catch { res.statusCode = 404; res.end('Not found'); }
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
let browser;
try {
  browser = await chromium.launch({ headless: true, ...(process.env.CHROME_BIN ? { executablePath: process.env.CHROME_BIN } : {}) });
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const r = await page.evaluate(async () => {
    const { createStubKernel } = await import('/kernel.mjs');
    // 2×2-PNG, rot/grün/blau/weiß
    const b64 = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEklEQVR4nGP4z8DAAMIM/4EAAB/uBfsL2WiLAAAAAElFTkSuQmCC';
    const png = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const k = createStubKernel({ instanceId: `png-${Date.now()}`, userId: 'u1', latencyMs: 5, storage: 'indexeddb' });
    await k.ready;
    await k.sim.publishAppVersion('crm', '1', {
      files: { 'index.js': 'export function mount(){ return { unmount(){} }; }', 'icon.png': new Blob([png], { type: 'image/png' }) },
      layout: { icon_asset: 'icon.png' },
    });
    const app = k.apps.get()[0];
    const m = await k.artifacts.manifest('crm', '1');
    const hash = m.files[app.layout.icon_asset];
    const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', png)), (x) => x.toString(16).padStart(2, '0')).join('');
    const blob = await k.artifacts.read(hash);
    const img = new Image();
    img.src = URL.createObjectURL(blob);
    document.body.append(img);
    await img.decode();
    const out = { storage: k.storageKind, hashOk: hash === digest, type: blob.type, w: img.naturalWidth, h: img.naturalHeight };
    await k.close();
    return out;
  });
  console.log(JSON.stringify({ ...r, errors }));
  if (!(r.storage === 'indexeddb' && r.hashOk && r.type === 'image/png' && r.w === 2 && r.h === 2 && !errors.length)) process.exitCode = 1;
} finally {
  await browser?.close();
  server.close();
}
