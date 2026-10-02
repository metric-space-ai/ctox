import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const fixtureUrl = new URL('./assert-shell-chat-composition.mjs', import.meta.url);
const source = fs.readFileSync(fixtureUrl, 'utf8')
  .replace(/^#!.*\n/, '').replace(/^import .*;$/gm, '')
  .replaceAll('import.meta.url', JSON.stringify(fixtureUrl.href));
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;

for (const stage of ['launch', 'context', 'context-close-error']) {
  test(`composition fixture cleans owned allocations after ${stage} failure`, async () => {
    const failure = new Error(`injected ${stage} failure`);
    let browserCloses = 0;
    let serverCloses = 0;
    const server = {
      listening: false,
      listen(_port, _host, callback) { this.listening = true; queueMicrotask(callback); },
      address() { return { port: 12345 }; },
      close(callback) { serverCloses++; this.listening = false; callback(); },
    };
    const browser = {
      async newContext() { throw failure; },
      async close() { browserCloses++; if (stage === 'context-close-error') throw new Error('injected close failure'); },
    };
    const chromium = {
      executablePath() { return '/fixture/chrome'; },
      async launch() { if (stage === 'launch') throw failure; return browser; },
    };
    const fixtureRequire = () => ({ chromium });
    fixtureRequire.resolve = () => '/fixture/playwright';
    const run = new AsyncFunction('createServer', 'createRequire', 'fs', 'path', 'fileURLToPath', source);
    await assert.rejects(run(() => server, () => fixtureRequire,
      { mkdirSync() {}, existsSync() { return true; } }, path, fileURLToPath), error => error === failure);
    assert.equal(serverCloses, 1, 'owned server must close even if Chromium never launched');
    assert.equal(server.listening, false);
    assert.equal(browserCloses, stage === 'launch' ? 0 : 1);
  });
}
