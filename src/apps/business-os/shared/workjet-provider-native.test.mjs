import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { projectNativeProviderRegistry, requestNativeProviders } from './workjet-provider-native.mjs';

const operationId = '7d2b9a30-5a24-4ba0-92da-5fdf499d3769';
// This model was observed from the authenticated Anthropic live catalog.
const model = 'claude-opus-5-5';
const reference = { accountId: 'canonical-account', holderInstanceId: 'native-holder', accountRevision: 2 };
const request = action => ({ version: 1, action, operationId });
function registry() {
  return { ok: true, schema: 'ctox.provider-federation-registry.v1', revision: 1,
    accounts: [{ id: reference.accountId, holder: { kind: 'ctox_instance', id: reference.holderInstanceId },
      revision: 2, provider: 'claude', enabled: true, credentialReady: true, observedAtMs: 1000,
      nativeAccountReference: { ...reference }, modelCatalogObserved: true, inferenceVerified: false,
      modelCatalog: { observed: true, fresh: true, models: [model], lastSuccessAtMs: 1000,
        lastAttempt: { checkedAtMs: 1000, httpStatus: 200, elapsedMs: 30, retryAfterSeconds: null,
          failure: null, success: true } }, excludedModels: [], effectiveModels: [model] }],
    providers: [{ provider: 'claude', selection: [model] }] };
}
function receipt(command, result = registry()) {
  return { command_id: command.id, ok: true, status: 'completed', transport: 'rxdb-command-bus',
    target_record_id: '', payload: { ...command.payload, inbound_channel: 'ctox' }, result };
}
function transport(rewrite = value => value) {
  const calls = [];
  return { calls, assertCurrent() {}, async dispatch(command, options) {
    calls.push({ command, options }); return rewrite(receipt(command));
  } };
}
test('reading uses one existing admitted command without adoption or account mutation', async () => {
  const port = transport();
  const result = await requestNativeProviders(request('instance.providers.read'), port);
  assert.equal(port.calls.length, 1);
  assert.equal(port.calls[0].command.command_type, 'ctox.workjet.providers.list');
  assert.equal(port.calls[0].command.record_id, null);
  assert.deepEqual(port.calls[0].command.payload, {});
  assert.deepEqual(port.calls[0].options, { until: 'terminal', sync_queue_tasks: false, timeoutMs: 25000 });
  assert.deepEqual(result.registry.accounts[0].nativeAccountReference, reference);
  assert.equal(result.registry.accounts[0].inferenceVerified, false);
});
test('explicit adoption, live observation, selection and exclusion retain exact revisions', async () => {
  for (const [action, fields, type, payload] of [
    ['adopt', {}, 'adopt_native', {}],
    ['observe', { accountId: reference.accountId, expectedAccountRevision: 2 },
      'observe_native', { account_id: reference.accountId, expected_account_revision: 2 }],
    ['models.select', { provider: 'claude', models: [model], expectedRevision: 1 },
      'models.select', { provider: 'claude', models: [model], expected_revision: 1 }],
    ['models.exclude', { accountId: reference.accountId, expectedAccountRevision: 2,
      models: [model], expectedRevision: 1 }, 'models.exclude',
      { account_id: reference.accountId, expected_account_revision: 2, models: [model], expected_revision: 1 }],
  ]) {
    const port = transport();
    await requestNativeProviders({ ...request('instance.providers.' + action), ...fields }, port);
    assert.equal(port.calls[0].command.command_type, 'ctox.workjet.providers.' + type);
    assert.deepEqual(port.calls[0].command.payload, payload);
  }
});
test('caller credentials, owner claims and substituted holder selectors are rejected before dispatch', async () => {
  for (const fields of [{ owner: 'another-owner' }, { secret: 'not-a-real-credential' },
    { holderInstanceId: 'other-holder' }, { accountId: 'not-allowed-for-read' }, { version: 2 }]) {
    const port = transport();
    await assert.rejects(requestNativeProviders({ ...request('instance.providers.read'), ...fields }, port));
    assert.equal(port.calls.length, 0);
  }
});
test('a native reference cannot be inferred from a matching account name or substituted revision', () => {
  for (const fields of [{ accountId: 'other-account' }, { holderInstanceId: 'other-holder' },
    { accountRevision: 3 }]) {
    const value = registry(); Object.assign(value.accounts[0].nativeAccountReference, fields);
    assert.throws(() => projectNativeProviderRegistry(value));
  }
  const value = registry(); delete value.accounts[0].nativeAccountReference;
  assert.throws(() => projectNativeProviderRegistry(value));
});
test('unknown private fields are stripped from every renderer result', () => {
  const value = registry();
  value.secret = 'private'; value.accounts[0].private_local_account_id = 'private';
  value.accounts[0].private_binding = 'private'; value.accounts[0].holder.token = 'private';
  value.accounts[0].modelCatalog.lastAttempt.message = 'private';
  const result = projectNativeProviderRegistry(value);
  assert.doesNotMatch(JSON.stringify(result), /private|token|message/);
  assert.deepEqual(result.accounts[0].nativeAccountReference, reference);
});
test('catalog success never becomes inference success; inconsistent fresh snapshots fail closed', () => {
  const value = registry(); value.accounts[0].inferenceVerified = true;
  assert.equal(projectNativeProviderRegistry(value).accounts[0].inferenceVerified, false);
  value.accounts[0].modelCatalog.lastAttempt.success = false;
  assert.throws(() => projectNativeProviderRegistry(value));
  value.accounts[0].modelCatalog.fresh = false;
  value.accounts[0].modelCatalog.lastAttempt.failure = 'transport_unavailable';
  value.accounts[0].modelCatalog.lastAttempt.httpStatus = null;
  assert.equal(projectNativeProviderRegistry(value).accounts[0].modelCatalog.lastAttempt.success, false);
});
test('scope retirement prevents delivery of a late result', async () => {
  let release; let current = true;
  const port = { assertCurrent() {
    if (!current) throw Object.assign(new Error('Scope changed.'), { code: 'PROVIDER_SCOPE_CHANGED' });
  }, dispatch(command) { return new Promise(resolve => { release = () => resolve(receipt(command)); }); } };
  const pending = requestNativeProviders(request('instance.providers.read'), port);
  current = false; release();
  await assert.rejects(pending, error => error.code === 'PROVIDER_SCOPE_CHANGED');
});
test('mismatched command IDs, payload, target and nonterminal receipts are rejected', async () => {
  for (const rewrite of [
    value => ({ ...value, command_id: 'another-command' }),
    value => ({ ...value, target_record_id: 'another-record' }),
    value => ({ ...value, payload: { owner_user_id: 'another-owner' } }),
    value => ({ ...value, status: 'accepted' }),
    value => ({ ...value, ok: false }),
    value => ({ ...value, transport: 'http' }),
  ]) await assert.rejects(requestNativeProviders(request('instance.providers.read'), transport(rewrite)));
  await assert.rejects(requestNativeProviders({ ...request('instance.providers.observe'),
    accountId: reference.accountId, expectedAccountRevision: 2 },
  transport(value => ({ ...value, payload: { ...value.payload, account_id: 'another-account' } }))));
});
test('invalid and oversized selections fail before admission', async () => {
  for (const modelIds of [[model, model], [''], Array.from({ length: 257 }, () => model)]) {
    const port = transport();
    await assert.rejects(requestNativeProviders({ ...request('instance.providers.models.select'),
      provider: 'claude', models: modelIds, expectedRevision: 1 }, port));
    assert.equal(port.calls.length, 0);
  }
});
test('transport failures never disclose their raw text or claim rejected provider credentials', async () => {
  await assert.rejects(requestNativeProviders(request('instance.providers.read'), {
    assertCurrent() {}, dispatch() { throw new Error('private credential detail'); },
  }), error => error.code === 'PROVIDER_CONTROL_UNAVAILABLE' && !/private|rejected/.test(error.message));
});
test('delivery times out without retry or a second command', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  let count = 0;
  const pending = requestNativeProviders(request('instance.providers.read'), {
    assertCurrent() {}, dispatch() { count++; return new Promise(() => {}); },
  });
  const rejected = assert.rejects(pending, error => error.code === 'PROVIDER_CONTROL_TIMEOUT');
  t.mock.timers.tick(25000);
  await rejected;
  assert.equal(count, 1);
});
test('shell integration captures the existing command bus and instance authority', async () => {
  const source = await readFile(new URL('../app.js', import.meta.url), 'utf8');
  assert.match(source, /requestNativeProviders\(request,/);
  assert.match(source, /state.commandBus !== commandBus/);
  assert.match(source, /source: 'workjet-provider-control'/);
  assert.match(source, /state.sync !== sync/);
});
