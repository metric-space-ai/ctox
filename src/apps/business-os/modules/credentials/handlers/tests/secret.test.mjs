import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';
import { INSTANCE_CREDENTIALS, isValidCredentialKey } from '../_secret.js';

test('Schlüsselformat wie is_valid_credential_key', () => {
  for (const ok of ['OPENAI_API_KEY', 'A', 'X1_2']) assert.equal(isValidCredentialKey(ok), true, ok);
  for (const bad of ['', 'openai', '1ABC', '_A', 'A-B', 'A'.repeat(65), 'ÄB']) assert.equal(isValidCredentialKey(bad), false, bad);
});

test('put: Wert nie im Handler, Name getrimmt; ungültig → invalid ohne Kernaufruf', async () => {
  const ctx = createCtx();
  const out = await run(ctx, 'ctox.secret.put', { name: ' SELLIFY_TOKEN ', value: 's3cret' }, { id: 'c1' });
  assert.deepEqual(out, { ok: true, name: 'SELLIFY_TOKEN' });
  const call = ctx.$.capsTo('secret.put')[0].args[0];
  assert.deepEqual(call, { command_id: 'c1', name: 'SELLIFY_TOKEN', value: { secret_intake: true } });
  assert.equal(JSON.stringify(ctx.$.capCalls).includes('s3cret'), false);
  await assert.rejects(run(ctx, 'ctox.secret.put', { name: 'bad' }), { code: 'invalid', message: /UPPER_SNAKE_CASE/ });
  assert.equal(ctx.$.capsTo('secret.put').length, 1);
});

test('CAP-3 #831: Instanzschlüssel mit scope credentials, eigene Schlüssel im App-Scope (put + delete)', async () => {
  const ctx = createCtx();
  await run(ctx, 'ctox.secret.put', { name: 'OPENAI_API_KEY' }, { id: 'c2' });
  await run(ctx, 'ctox.secret.put', { name: 'CTO_EMAIL_PASSWORD' }, { id: 'c3' });
  await run(ctx, 'ctox.secret.delete', { name: 'OPENAI_API_KEY' });
  await run(ctx, 'ctox.secret.delete', { name: 'SELLIFY_TOKEN' });
  const puts = ctx.$.capsTo('secret.put').map((c) => c.args[0]);
  assert.deepEqual(puts[0], { command_id: 'c2', name: 'OPENAI_API_KEY', value: { secret_intake: true }, scope: 'credentials' });
  assert.equal(puts[1].scope, 'credentials');
  const deletes = ctx.$.capsTo('secret.delete').map((c) => c.args[0]);
  assert.deepEqual(deletes, [{ name: 'OPENAI_API_KEY', scope: 'credentials' }, { name: 'SELLIFY_TOKEN' }]);
  // Katalog = secrets.rs CREDENTIAL_CATALOG (28 Schlüssel), alle im Env-Format.
  assert.equal(INSTANCE_CREDENTIALS.length, 28);
  assert.ok(INSTANCE_CREDENTIALS.every((e) => isValidCredentialKey(e.name)));
});

test('delete / list: Kernform {scope, secrets} je Schlüsselbund → App-Antwort {catalog, extra}', async () => {
  const ctx = createCtx({ caps: { 'secret.list': async (args) => (args.scope === 'credentials'
    ? { scope: 'credentials', secrets: [{ name: 'OPENAI_API_KEY', description: null, updated_at: 't1' }, { name: 'LEGACY_X', description: 'alt', updated_at: 't2' }] }
    : { scope: 'app:credentials', secrets: [{ name: 'A', description: null, updated_at: 't3' }] }) } });
  assert.deepEqual(await run(ctx, 'ctox.secret.delete', { name: 'A' }), { ok: true, name: 'A' });
  await assert.rejects(run(ctx, 'ctox.secret.delete', { name: 'a' }), { code: 'invalid', message: 'invalid credential key' });
  const out = await run(ctx, 'ctox.secret.list', {});
  assert.deepEqual(ctx.$.capsTo('secret.list').map((c) => c.args[0]), [{ scope: 'credentials' }, {}]);
  assert.equal(out.catalog.length, 28);
  assert.deepEqual(out.catalog.find((e) => e.name === 'OPENAI_API_KEY'), { name: 'OPENAI_API_KEY', description: 'OpenAI API key', is_set: true, updated_at: 't1' });
  assert.equal(out.catalog.find((e) => e.name === 'HF_TOKEN').is_set, false);
  assert.deepEqual(out.extra, [
    { name: 'LEGACY_X', description: 'alt', is_set: true, updated_at: 't2' },
    { name: 'A', description: '', is_set: true, updated_at: 't3' },
  ]);
});
