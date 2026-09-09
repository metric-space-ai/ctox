import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

// Run the production fetch wrapper without starting the rest of the shell.
const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const start = app.indexOf('async function fetchBusinessOsControlJson(');
const end = app.indexOf('\nfunction shellCtoxHealthProblem(', start);
assert.ok(start >= 0 && end > start);
const source = app.slice(start, end);

function fixture(session = { maintenance_control_token: 'scoped.fixture' }) {
  const calls = [];
  const context = vm.createContext({
    URL,
    window: { location: new URL('https://welsch.ctox.dev/'), CTOX_BUSINESS_OS_SESSION: session },
    fetch: async (url, options) => {
      calls.push({ url, options });
      return Response.json({ ok: true, active: false });
    },
  });
  vm.runInContext(source, context);
  return { calls, fetch: (url, options) => context.fetchBusinessOsControlJson(url, options) };
}
const endpoint = '/api/business-os/ctox/maintenance';
test('desktop launch grant reaches only the same-origin maintenance read', async () => {
  const f = fixture();
  await f.fetch(endpoint);
  const options = f.calls[0].options;
  assert.equal(options.headers['x-ctox-maintenance-token'], 'scoped.fixture');
  assert.equal(options.redirect, 'error');
  assert.equal(options.credentials, 'same-origin');
  assert.equal(options.cache, 'no-store');
});
test('grant is not sent to other origins, other endpoints or mutations', async () => {
  const f = fixture();
  for (const [url, options] of [
    ['https://other.ctox.dev' + endpoint],
    ['https://welsch.ctox.dev.attacker.invalid' + endpoint],
    ['/api/business-os/launch-context'],
    ['/api/business-os/runtime-settings'],
    ['/api/business-os/ctox/update/apply', { method: 'POST', body: '{}' }],
    [endpoint, { method: 'POST' }],
    [endpoint + '/'],
  ]) await f.fetch(url, options);
  for (const call of f.calls) assert.equal(call.options.headers['x-ctox-maintenance-token'], undefined);
});
test('native capabilities never substitute for a maintenance grant', async () => {
  for (const session of [null, {}, { capability_token: 'native.secret' }, { maintenance_control_token: '<redacted>' }]) {
    const f = fixture(session);
    await f.fetch(endpoint);
    assert.equal(f.calls[0].options.headers['x-ctox-maintenance-token'], undefined);
    assert.equal(f.calls[0].options.headers.Authorization, undefined);
  }
});
test('control errors remain visible to the caller', async () => {
  const context = vm.createContext({
    URL,
    window: { location: new URL('https://welsch.ctox.dev/'), CTOX_BUSINESS_OS_SESSION: {} },
    fetch: async () => Response.json({ error: 'login required' }, { status: 401 }),
  });
  vm.runInContext(source, context);
  await assert.rejects(context.fetchBusinessOsControlJson(endpoint), /login required/);
});
