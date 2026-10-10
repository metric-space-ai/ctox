import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { projectNativeModelProbe, requestNativeModelCheck } from './workjet-provider-native.mjs';

// Authenticated live Claude account catalog, g3-claude-live-models-20261009.json.
const modelId = 'claude-opus-5-5';
const operationId = '7d2b9a30-5a24-4ba0-92da-5fdf499d3769';
const request = { version: 1, action: 'instance.providers.models.check', operationId,
  accountId: 'canonical-account', expectedAccountRevision: 2, modelId };
const probe = { modelId, checkedAtMs: 1000, elapsedMs: 50, status: 'ok', source: 'upstream',
  failure: null, httpStatus: 200, retryAtMs: null };
const reply = () => ({ version: 1, op: 'check', commandId: operationId,
  accountId: request.accountId, accountRevision: 2, probe: { ...probe } });

test('check uses only the exact admitted native method and a bounded correlated DTO', async () => {
  let calls = 0;
  const result = await requestNativeModelCheck({ async requestNative(method, dto, options) {
    calls++;
    assert.equal(method, 'ctox.workjet.models.check.v1');
    assert.deepEqual(dto, { version: 1, op: 'check', commandId: operationId,
      accountId: request.accountId, accountRevision: 2, modelId });
    assert.deepEqual(options, { requiredCapability: 'ctox-workjet-model-check-v1', timeoutMs: 25000 });
    return { ...reply(), secret: 'private', probe: { ...probe, body: 'private', secret: 'private' } };
  } }, request, () => {});
  assert.equal(calls, 1);
  assert.equal(result.operationId, operationId);
  assert.deepEqual(result.probe, probe);
  assert.doesNotMatch(JSON.stringify(result), /private|secret|body/);
});

test('caller authority, holder endpoints and malformed target IDs never reach the transport', async () => {
  for (const fields of [{ root: 'private' }, { token: 'private' }, { owner: 'another' },
    { endpoint: 'private' }, { expectedAccountRevision: 0 }, { accountId: '' },
    { modelId: '' }, { version: 2 }, { operationId: 'not-a-uuid' }]) {
    let calls = 0;
    await assert.rejects(requestNativeModelCheck({ requestNative() { calls++; } }, { ...request, ...fields }, () => {}));
    assert.equal(calls, 0);
  }
});

test('only coherent upstream success is green; local failures cannot claim authentication errors', () => {
  for (const changes of [{ source: 'gateway' }, { failure: 'auth' }, { httpStatus: 401 },
    { httpStatus: null }, { status: 'unknown' }, { retryAtMs: 4000 }]) {
    assert.throws(() => projectNativeModelProbe({ ...probe, ...changes }));
  }
  const cooldown = { ...probe, status: 'unavailable', source: 'gateway',
    failure: 'gateway_cooldown', httpStatus: 503, retryAtMs: 4000 };
  assert.deepEqual(projectNativeModelProbe(cooldown), cooldown);
  assert.throws(() => projectNativeModelProbe({ ...cooldown, failure: 'auth' }));
  for (const [failure, httpStatus] of [['auth',401], ['model_not_found',404], ['quota_rate_limit',429],
    ['provider',503], ['invalid_response',200]]) {
    const red = { ...probe, status: 'failed', failure, httpStatus };
    assert.deepEqual(projectNativeModelProbe(red), red);
    assert.throws(() => projectNativeModelProbe({ ...red, source: 'gateway' }));
  }
});

test('mismatched command, account, revision and model receipts are rejected', async () => {
  for (const changes of [{ commandId: 'another' }, { accountId: 'another' },
    { accountRevision: 3 }, { version: 2 }, { op: 'read' },
    { probe: { ...probe, modelId: '' } }]) {
    await assert.rejects(requestNativeModelCheck({ async requestNative() { return { ...reply(), ...changes }; } },
      request, () => {}));
  }
});

test('scope retirement and transport exceptions do not publish a late result or private errors', async () => {
  let current = true;
  await assert.rejects(requestNativeModelCheck({ async requestNative() { current = false; return reply(); } }, request,
    () => { if (!current) throw new Error('scope retired'); }), /scope retired/);
  await assert.rejects(requestNativeModelCheck({ async requestNative() { throw new Error('private secret'); } },
    request, () => {}), error => error.code === 'PROVIDER_MODEL_CHECK_UNAVAILABLE'
      && !/private|secret|rejected/.test(error.message));
});

test('the browser deadline is bounded even if the native channel ignores its timeout', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  let calls = 0;
  const pending = requestNativeModelCheck({ requestNative() { calls++; return new Promise(() => {}); } }, request, () => {});
  const rejected = assert.rejects(pending, error => error.code === 'PROVIDER_MODEL_CHECK_UNAVAILABLE');
  t.mock.timers.tick(25000);
  await rejected;
  assert.equal(calls, 1);
});

test('actual project control keeps model checks on the admitted instance without command or meeting writes', async () => {
  const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
  const start = source.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS');
  const end = source.indexOf('async function waitForSyncBridgeReady', start);
  let retire = () => {};
  const state = { session: { id: 'owner' }, db: {}, syncConfig: { instance_id: 'instance' },
    commandBus: { dispatch() { assert.fail('no command or meeting mutation'); } },
    sync: { async requestNative() { retire(); return reply(); } } };
  const context = vm.createContext({ state, requestNativeModelCheck, actorContext: session => session });
  vm.runInContext(source.slice(start, end) + '\n' + 'globalThis.control = workjetProjectControl;', context);
  assert.deepEqual(JSON.parse(JSON.stringify(await context.control(request))), {
    version: 1, action: request.action, operationId, accountId: request.accountId, accountRevision: 2, probe,
  });
  retire = () => { state.syncConfig = { instance_id: 'replaced' }; };
  await assert.rejects(context.control(request), error => error.code === 'PROVIDER_SCOPE_CHANGED');
});
