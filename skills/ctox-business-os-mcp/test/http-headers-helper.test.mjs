import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { readHeaders, LIMITS } from '../scripts/http-headers-helper.mjs';

const fixture = fileURLToPath(new URL('./fixtures/header-loader.mjs', import.meta.url));
const script = fileURLToPath(new URL('../scripts/http-headers-helper.mjs', import.meta.url));
const marker = 'offline-fixture-secret-NEVER-LOG';
function run(scenario = 'valid', options = {}) {
  let child, args;
  const result = readHeaders({ loaderPath: fixture, ...options, spawnImpl: (runtime, argv, config) => {
    args = argv;
    child = spawn(runtime, argv, { ...config, env: { ...process.env, CTOX_HEADER_HELPER_FIXTURE: scenario } });
    return child;
  } });
  return { result, child: () => child, args: () => args };
}
test('trusted module is invoked in a bounded child, returning only Authorization', async () => {
  const f = run();
  assert.deepEqual(await f.result, { Authorization: `Bearer ${marker}` });
  assert.deepEqual(f.args(), [script, '--load-child', fixture]);
  assert.equal(f.args().join(' ').includes(marker), false);
  assert.equal(f.child().exitCode, 0);
});
test('loader errors, stray stdout/stderr and malformed tokens fail without secret text', async () => {
  for (const scenario of ['throw', 'stdout', 'stderr', 'invalid', 'injection']) {
    const f = run(scenario);
    await assert.rejects(f.result, (error) => {
      assert.equal(error.message.includes(marker), false);
      assert.match(error.message, /loader_failed|invalid_loader_output|unexpected_loader_stderr/);
      return true;
    });
  }
});
test('bounded pipes kill the owned child and do not retain oversized output', async () => {
  for (const [scenario, code] of [['stdout-limit', 'loader_stdout_limit'], ['stderr-limit', 'loader_stderr_limit']]) {
    const f = run(scenario);
    const closed = once(f.child(), 'close');
    await assert.rejects(f.result, { message: code });
    await closed;
  }
});
test('timeout and abort kill the loader process; no retries or token rotation', async () => {
  const f = run('hang', { limits: { ...LIMITS, timeoutMs: 200 } });
  const closed = once(f.child(), 'close');
  await assert.rejects(f.result, { message: 'helper_timeout' });
  const [, killed] = await closed;
  assert.equal(killed, 'SIGKILL');
  const controller = new AbortController();
  const g = run('hang', { signal: controller.signal });
  const closedAgain = once(g.child(), 'close');
  controller.abort();
  await assert.rejects(g.result, { message: 'helper_aborted' });
  await closedAgain;
});
test('invalid local config and already-aborted work never execute the loader', async () => {
  for (const loaderPath of ['relative.mjs', 'https://example.invalid/loader.mjs', '/tmp/loader.js', null]) {
    await assert.rejects(readHeaders({ loaderPath, spawnImpl: () => assert.fail('spawn') }), /invalid_configuration/);
  }
  const controller = new AbortController(); controller.abort();
  await assert.rejects(readHeaders({ loaderPath: fixture, signal: controller.signal,
    spawnImpl: () => assert.fail('spawn') }), /helper_aborted/);
  await assert.rejects(readHeaders({ loaderPath: fixture, spawnImpl: () => { throw new Error(marker); } }),
    { message: 'helper_spawn_failed' });
});
test('real CLI emits one JSON header object to its private stdout pipe; failures are static', async () => {
  for (const scenario of ['valid', 'throw', 'stdout', 'stderr', 'injection']) {
    const child = spawn(process.execPath, [script, '--secret-loader', fixture], {
      stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, CTOX_HEADER_HELPER_FIXTURE: scenario },
    });
    let stdout = '', stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    const [code] = await once(child, 'close');
    if (scenario === 'valid') {
      assert.equal(code, 0); assert.equal(stderr, '');
      assert.deepEqual(JSON.parse(stdout), { Authorization: `Bearer ${marker}` });
      assert.equal(stdout.split('\n').length, 2);
    } else {
      assert.equal(code, 1); assert.equal(stdout, '');
      assert.equal(stderr, 'ctox-mcp: header helper failed\n');
    }
  }
});
