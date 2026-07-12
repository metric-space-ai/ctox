import http from 'node:http';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { dirname, extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const businessOsRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const server = http.createServer((request, response) => {
  const url = new URL(request.url || '/', 'http://127.0.0.1');
  const pathname = url.pathname === '/' ? '/index.html' : url.pathname;
  const filePath = normalize(join(businessOsRoot, pathname));
  if (!filePath.startsWith(`${businessOsRoot}${sep}`)) {
    response.writeHead(403);
    response.end('forbidden');
    return;
  }
  try {
    if (!statSync(filePath).isFile()) throw new Error('not a file');
    response.writeHead(200, { 'content-type': contentType(filePath) });
    response.end(readFileSync(filePath));
  } catch {
    response.writeHead(404);
    response.end('not found');
  }
});
await new Promise((resolveReady) => server.listen(0, '127.0.0.1', resolveReady));
const { port } = server.address();
const playwrightModule = process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href
  : '../../node_modules/playwright/index.mjs';
const { chromium } = await import(playwrightModule);
const systemChrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser = await chromium.launch({
  headless: true,
  ...(existsSync(systemChrome) ? { executablePath: systemChrome } : {}),
});

try {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}/`);
  const result = await page.evaluate(async () => {
    const { resetBusinessDb, createBusinessDb } = await import('/shared/db.js');
    const databaseName = `recovery-reset-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const oldestPendingAtMs = Date.now();
    localStorage.setItem(`ctox.businessOs.recoveryStatus.${databaseName}`, JSON.stringify({
      pendingWrites: 1,
      pendingBytes: 128,
      oldestPendingAtMs,
      lastExportAtMs: 0,
    }));
    let blockedCode = '';
    try {
      await resetBusinessDb({ name: databaseName });
    } catch (error) {
      blockedCode = error?.code || '';
    }
    localStorage.setItem(`ctox.businessOs.recoveryStatus.${databaseName}`, JSON.stringify({
      pendingWrites: 1,
      pendingBytes: 128,
      oldestPendingAtMs,
      lastExportAtMs: oldestPendingAtMs + 1,
    }));
    await resetBusinessDb({ name: databaseName });
    const db = await createBusinessDb({ name: databaseName });
    const retryOk = await db.recovery.retryPrimaryOpen();
    db.close();
    indexedDB.deleteDatabase(databaseName);
    indexedDB.deleteDatabase(`${databaseName}__recovery_v2`);
    return { blockedCode, retryOk };
  });
  assert(result.blockedCode === 'recovery_export_required', 'primary reset must require export while pending writes are unexported');
  assert(result.retryOk === true, 'retryPrimaryOpen must reopen primary after allowed reset');
  console.log('ctox-rxdb recovery primary reset browser smoke OK', result);
} finally {
  await browser.close();
  await new Promise((resolveClose) => server.close(resolveClose));
}

function contentType(filePath) {
  return {
    '.html': 'text/html',
    '.js': 'text/javascript',
    '.mjs': 'text/javascript',
    '.css': 'text/css',
    '.json': 'application/json',
  }[extname(filePath)] || 'application/octet-stream';
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
