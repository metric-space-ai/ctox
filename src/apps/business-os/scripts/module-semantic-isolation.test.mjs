import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer, createConnection } from 'node:net';
import { once } from 'node:events';
import { runModuleSemanticCheck } from '../../../skills/system/product_engineering/business-os-app-module-development/scripts/module_semantic_isolation.mjs';

const root = mkdtempSync(join(tmpdir(), 'ctox-semantic-isolation-'));
const moduleDir = join(root, 'candidate');
mkdirSync(moduleDir);
const path = join(moduleDir, 'schema.mjs');
const run = (code) => {
  writeFileSync(path, code);
  return runModuleSemanticCheck(moduleDir, { kind: 'schema', path });
};
try {
  assert.deepEqual(run('export const collections = { records: { type: "object" } };'),
    { records: { type: 'object' } });
  const secret = join(root, 'host-secret');
  writeFileSync(secret, 'must remain unreadable');
  const marker = join(root, 'host-write');
  for (const target of [marker, join(moduleDir, 'candidate-write')]) {
    assert.throws(() => run(`import { writeFileSync } from 'node:fs';
      writeFileSync(${JSON.stringify(target)}, 'escaped'); export const collections = {};`),
    /isolated semantic check failed/);
    assert.equal(existsSync(target), false);
  }
  assert.throws(() => run(`import { readFileSync } from 'node:fs';
    export const collections = { stolen: readFileSync(${JSON.stringify(secret)}, 'utf8') };`),
  /isolated semantic check failed/);

  let connections = 0;
  const server = createServer((socket) => { connections++; socket.end(); });
  await new Promise((resolve, reject) => {
    server.once('error', reject); server.listen(0, '127.0.0.1', resolve);
  });
  try {
    const port = server.address().port;
    const control = createConnection({ host: '127.0.0.1', port });
    await once(control, 'close');
    assert.equal(connections, 1, 'positive control reaches the listener');
    assert.throws(() => run(`import { createConnection } from 'node:net';
      await new Promise((resolve, reject) => {
        const socket = createConnection({ host: '127.0.0.1', port: ${port} });
        socket.once('connect', () => { socket.end(); resolve(); }); socket.once('error', reject);
      }); export const collections = {};`), /isolated semantic check failed/);
    await new Promise((resolve) => setTimeout(resolve, 50));
    assert.equal(connections, 1, 'candidate never reaches host listener');
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
  assert.throws(() => run('while (true) {}'), /isolated semantic check failed/);
  console.log('[module-semantic-isolation.test] OK: valid export; denied host read/write, candidate write, network; bounded loop');
} finally {
  rmSync(root, { recursive: true, force: true });
}
