#!/usr/bin/env node
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, '..');
const host = '127.0.0.1';
const server = http.createServer((request, response) => {
  const url = new URL(request.url || '/', `http://${host}`);
  const relative = decodeURIComponent(url.pathname === '/' ? '/design-lab.html' : url.pathname);
  const candidate = path.resolve(appRoot, `.${relative}`);
  if (candidate !== appRoot && !candidate.startsWith(`${appRoot}${path.sep}`)) {
    response.writeHead(403).end('Forbidden');
    return;
  }
  fs.readFile(candidate, (error, body) => {
    if (error) {
      response.writeHead(error.code === 'ENOENT' ? 404 : 500).end(error.code === 'ENOENT' ? 'Not Found' : 'Server Error');
      return;
    }
    response.writeHead(200, { 'content-type': contentType(candidate), 'cache-control': 'no-store' });
    response.end(body);
  });
});

await new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, host, resolve);
});
const address = server.address();
const url = `http://${host}:${address.port}/design-lab.html`;
try {
  await run('capture-design-matrix.mjs', { BUSINESS_OS_DESIGN_LAB_URL: url });
  await run('assert-visual-diff.mjs');
  await run('assert-accessibility-contract.mjs', { BUSINESS_OS_DESIGN_LAB_URL: url });
  await run('assert-app-starter-browser.mjs');
} finally {
  await new Promise((resolve) => server.close(resolve));
}
console.log('business_os_design_quality_gates_ok=1');

function run(script, extraEnv = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [path.join(__dirname, script)], {
      cwd: path.resolve(appRoot, '../../..'),
      env: { ...process.env, ...extraEnv },
      stdio: 'inherit',
    });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${script} failed with ${signal ? `signal ${signal}` : `exit code ${code}`}`));
    });
  });
}

function contentType(file) {
  return ({
    '.html': 'text/html; charset=utf-8',
    '.js': 'text/javascript; charset=utf-8',
    '.mjs': 'text/javascript; charset=utf-8',
    '.css': 'text/css; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.svg': 'image/svg+xml',
  })[path.extname(file).toLowerCase()] || 'application/octet-stream';
}
