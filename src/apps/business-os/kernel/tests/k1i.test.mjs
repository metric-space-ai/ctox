// K-1i: process.run nach CAP-3d (#844, §16.47) im Fake-Core – simuliert, nie ein echter Prozess.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStubKernel } from '../kernel.mjs';

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 4000) { const t0 = Date.now(); while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error('until'); await tick(5); } }
const MOUNT = 'export function mount(){ return { unmount(){} }; }';
const run = async (k, app, req) => {
  const h = k.appApi(app, '1').command(`${app}.run`, { req });
  await until(() => ['done', 'failed'].includes(h.status.get().status));
  return h.status.get();
};

test('CAP-3d process.run: Allowlist, Grund-Umgebung nur ergänzbar, stdin/stdin_ref nur eigene App, Ergebnis-Objekt', async (t) => {
  const k = createStubKernel({ locks: testLocks, instanceId: `k1i${process.pid}`, userId: 'u1', latencyMs: 2 });
  t.after(() => k.close());
  await k.ready;
  const handler = { 'index.js': MOUNT, 'handlers/run.js': 'export default async function handle(c, ctx) { return ctx.cap.process.run(c.payload.req); }' };
  for (const app of ['crm', 'hr']) await k.sim.publishAppVersion(app, '1', { files: handler, handlers: { [`${app}.run`]: 'handlers/run.js' }, capabilitiesRequired: ['process', 'files'] });
  await k.sim.publishAppVersion('nox', '1', { files: handler, handlers: { 'nox.run': 'handlers/run.js' } });
  const ok = await run(k, 'crm', { program: 'echo', args: ['hallo', 'welt'] });
  assert.deepEqual(ok.result, { exit_code: 0, stdout: 'hallo welt\n', stderr: '', timed_out: false });
  // Grund-Umgebung der Allowlist + App-env; Grundvariable nicht ersetzbar
  const env = await run(k, 'crm', { program: 'env', env: { CTOX_SCRAPE_JOB: 'j1' } });
  assert.equal(env.result.stdout, 'CTOX_BIN=/opt/ctox/bin\nCTOX_SCRAPE_JOB=j1\nLANG=C.UTF-8\nPATH=/usr/local/bin:/usr/bin:/bin\n');
  assert.match((await run(k, 'crm', { program: 'env', env: { PATH: '/tmp/evil' } })).error, /^invalid: env_base_override:PATH/);
  // Interpreter + Skript nur aus der Allowlist
  const scrape = await run(k, 'crm', { program: 'scrape', args: ['--job', 'j1'], stdin: 'x' });
  assert.deepEqual(JSON.parse(scrape.result.stdout).argv, ['/usr/local/bin/tsx', '/opt/ctox/scripts/scrape.ts', '--job', 'j1']);
  assert.match((await run(k, 'crm', { program: '/bin/sh', args: ['-c', 'id'] })).error, /^forbidden: program_not_allowed/);
  // stdin direkt bzw. als files-Datensatz der EIGENEN App
  assert.equal((await run(k, 'crm', { program: 'cat', stdin: 'zeile\n' })).result.stdout, 'zeile\n');
  const own = await k.appApi('crm', '1').files.put({ text: 'aus datei\n' }, { name: 'in.txt' });
  await k.sim.settle();
  assert.equal((await run(k, 'crm', { program: 'cat', stdin_ref: { ref: own.ref } })).result.stdout, 'aus datei\n');
  assert.match((await run(k, 'hr', { program: 'cat', stdin_ref: { ref: own.ref } })).error, /^forbidden/, 'fremde App');
  assert.match((await run(k, 'crm', { program: 'cat', stdin: 'a', stdin_ref: { ref: own.ref } })).error, /^invalid: payload_invalid:stdin/);
  assert.match((await run(k, 'crm', { program: 'cat', stdin: 'x'.repeat(1024 * 1024 + 1) })).error, /^invalid: stdin_too_large/);
  // Zeitlimit, Exit-Code, Deklaration
  assert.deepEqual((await run(k, 'crm', { program: 'sleep', args: ['5'], timeout_ms: 100 })).result, { exit_code: null, stdout: '', stderr: 'timeout after 100 ms\n', timed_out: true });
  assert.equal((await run(k, 'crm', { program: 'false' })).result.exit_code, 1);
  assert.match((await run(k, 'nox', { program: 'echo' })).error, /^forbidden: cap\.process\.run nicht in capabilities_required/);
  // Plattform-Allowlist ersetzbar (Name → Pfad)
  k.sim.setProcessAllowlist({ cat: '/usr/bin/cat' });
  assert.match((await run(k, 'crm', { program: 'echo', args: ['x'] })).error, /^forbidden: program_not_allowed/);
});
