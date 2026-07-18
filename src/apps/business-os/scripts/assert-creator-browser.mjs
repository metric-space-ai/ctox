#!/usr/bin/env node
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import http from 'node:http';
import { dirname, extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const problems = [];
const server = http.createServer(serve);
await new Promise((resolveListen, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', resolveListen);
});
const port = server.address().port;
const executablePath = [
  process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE,
  chromium.executablePath(),
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
].find((candidate) => candidate && existsSync(candidate));
const browser = await chromium.launch({ headless: true, executablePath });

try {
  const page = await browser.newPage({ viewport: { width: 1180, height: 820 } });
  page.on('console', (message) => {
    if (['error', 'warning'].includes(message.type())) problems.push(`console.${message.type()}: ${message.text()}`);
  });
  page.on('pageerror', (error) => problems.push(`pageerror: ${error.message}`));
  await page.goto(`http://127.0.0.1:${port}/harness`, { waitUntil: 'networkidle' });
  await page.waitForFunction(() => globalThis.__creator?.ready === true);

  assert.equal(await page.locator('[data-example-id]').count(), 10);
  await page.locator('[data-example-id="crm"]').click();
  assert.match(await page.locator('#app-request-input').inputValue(), /CRM-App/);
  assert.equal(await page.locator('#btn-deploy-app').isEnabled(), true);

  // Inspiration URLs live behind the collapsed options disclosure now.
  await page.locator('.creator-options > summary').click();
  await page.locator('#creator-inspiration-url').fill('https://linear.app/#product');
  await page.locator('#btn-add-inspiration').click();
  assert.equal(await page.locator('[data-inspiration-list] .creator-url-chip').count(), 1);
  assert.match(await page.locator('[data-inspiration-list]').innerText(), /linear\.app/);
  await page.locator('.creator-options > summary').click();

  await page.locator('#btn-deploy-app').click();
  await page.waitForFunction(() => globalThis.__creator.commands.length === 1);
  const command = await page.evaluate(() => globalThis.__creator.commands[0]);
  assert.equal(command.command_type, 'ctox.business_os.app.create');
  assert.deepEqual(command.payload.inspiration_urls, ['https://linear.app/']);
  assert.match(command.payload.instruction, /CRM-App/);

  const layout = await page.evaluate(() => {
    const root = document.querySelector('[data-creator-root]');
    return {
      overflowX: root.scrollWidth - root.clientWidth,
      title: document.querySelector('[data-t="centerTitle"]')?.textContent?.trim(),
      promptVisible: document.querySelector('#app-request-input')?.getBoundingClientRect().height > 100,
      technicalSettingsClosed: !document.querySelector('.creator-options')?.open,
      technicalStatusClosed: !document.querySelector('.creator-status-details')?.open,
    };
  });
  assert.equal(layout.title, 'Was möchtest du bauen?');
  assert.equal(layout.promptVisible, true);
  assert.equal(layout.technicalSettingsClosed, true);
  assert.equal(layout.technicalStatusClosed, true);
  assert.ok(layout.overflowX <= 1, `Creator overflows desktop by ${layout.overflowX}px`);
  await page.close();

  const compact = await browser.newPage({ viewport: { width: 640, height: 820 } });
  compact.on('pageerror', (error) => problems.push(`compact pageerror: ${error.message}`));
  await compact.goto(`http://127.0.0.1:${port}/harness`, { waitUntil: 'networkidle' });
  await compact.waitForFunction(() => globalThis.__creator?.ready === true);
  const compactOverflow = await compact.evaluate(() => {
    const root = document.querySelector('[data-creator-root]');
    return root.scrollWidth - root.clientWidth;
  });
  assert.ok(compactOverflow <= 1, `Creator overflows compact viewport by ${compactOverflow}px`);
  await compact.close();
} finally {
  await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
}

assert.deepEqual(problems, [], problems.join('\n'));
console.log('Business OS App Creator browser flow OK: examples, URL references, command dispatch, desktop and compact layout');

function serve(request, response) {
  const url = new URL(request.url || '/', 'http://127.0.0.1');
  if (url.pathname === '/harness') return send(response, 200, harnessHtml(), 'text/html; charset=utf-8');
  if (url.pathname === '/favicon.ico') return send(response, 204, '', 'text/plain');
  const candidate = resolve(appRoot, url.pathname.replace(/^\/+/, ''));
  if (!candidate.startsWith(`${appRoot}/`) || !existsSync(candidate)) return send(response, 404, 'Not Found', 'text/plain');
  const type = ({
    '.html': 'text/html; charset=utf-8',
    '.js': 'text/javascript; charset=utf-8',
    '.mjs': 'text/javascript; charset=utf-8',
    '.css': 'text/css; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.svg': 'image/svg+xml',
  })[extname(candidate)] || 'application/octet-stream';
  return send(response, 200, readFileSync(candidate), type);
}

function harnessHtml() {
  return `<!doctype html>
<html lang="de" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<style>html,body{margin:0;width:100%;height:100%;overflow:hidden}body{background:var(--bg)}[data-test-host]{container:business-app-window / inline-size;width:100%;height:100%;overflow:hidden}</style>
</head><body><main data-test-host></main><script type="module">
const commands=[]; const notifications=[];
const ctx={
  host:document.querySelector('[data-test-host]'), locale:'de', module:{id:'creator'},
  sync:{startCollection:async()=>{}}, db:{collection:()=>null},
  commandBus:{dispatch:async(command)=>{commands.push(command);return {status:'completed',task_id:'test-task'};}},
  notifications:{show:(item)=>notifications.push(item)}, session:{user:{id:'admin',role:'admin',is_admin:true}},
};
globalThis.__creator={ready:false,commands,notifications};
const module=await import('/modules/creator/index.js');
globalThis.__creator.cleanup=await module.mount(ctx);
globalThis.__creator.ready=true;
</script></body></html>`;
}

function send(response, status, body, contentType) {
  response.writeHead(status, { 'content-type': contentType, 'cache-control': 'no-store' });
  response.end(body);
}
