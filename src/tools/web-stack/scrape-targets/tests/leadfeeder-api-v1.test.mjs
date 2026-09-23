import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { search } = require('../leadfeeder.com/scripts/api-v1.cjs');
const config = { account_id: '123456', credential_ref: 'ctox-secret://credentials/TEST_LEADFEEDER', native_root: '/fixture/ctox' };
const input = { company: 'Fixture GmbH', country: 'DE' };
const fixture = () => ({ data: [{ type: 'company_summary', id: '42', attributes: {
  name: 'Fixture GmbH', address: { country_code: 'DE', city: 'Fixture City' },
  employee_range: '11-50', url: 'https://www.fixture.example/', email: 'admin@fixture.example',
  industries: { industry: [{ code: '123', name: 'Industry' }] },
} }], meta: { request_id: 'request-fixture', credits: { charged: 0 }, pagination: { next_cursor: null } } });
const dependencies = (body = fixture(), status = 200) => ({ loadSecret: () => 'canary-key', fetch: async () => new Response(JSON.stringify(body), { status }) });

test('uses one fixed v1 credit-free POST and manifest-only credentials/account', async () => {
  let calls = 0;
  const result = await search({ ...input, account_id: '999', credential_ref: 'ctox-secret://credentials/OTHER', url: 'https://evil.invalid/' }, config, {
    loadSecret: checked => { assert.equal(checked.secret_name, 'TEST_LEADFEEDER'); return 'canary-key'; },
    fetch: async (url, options) => {
      calls++;
      assert.equal(url.origin + url.pathname, 'https://api.leadfeeder.com/v1/companies/search');
      assert.equal(url.searchParams.get('account_id'), '123456');
      assert.equal(url.searchParams.get('page[size]'), '5');
      assert.equal(options.headers['X-Api-Key'], 'canary-key');
      assert.equal(options.headers.Authorization, undefined);
      assert.equal(options.method, 'POST');
      assert.equal(options.redirect, 'error');
      assert.deepEqual(JSON.parse(options.body), { search_terms: ['Fixture GmbH'], locations: [{ country_code: 'DE' }] });
      return new Response(JSON.stringify(fixture()));
    },
  });
  assert.equal(calls, 1);
  assert.equal(result.failure_mode, undefined);
  assert.equal(result.api_query_evidence.request_id, 'request-fixture');
  assert.equal(result.api_query_evidence.credits_charged, 0);
  assert.equal(result.records.find(r => r.field === 'firma_domain').value, 'fixture.example');
  assert.equal(result.records.find(r => r.field === 'mitarbeiter').value, '11-50');
  assert(!result.records.some(r => r.field.includes('email') || r.field === 'wz_code'));
  assert(!JSON.stringify(result).includes('canary-key'));
});

test('auth, provider errors and native loader errors cannot become successful records or leak secrets', async () => {
  for (const status of [401, 403, 404, 429, 500]) {
    const result = await search(input, config, dependencies({ error: 'canary-key' }, status));
    assert(result.failure_mode);
    assert.deepEqual(result.records, []);
    assert(!JSON.stringify(result).includes('canary-key'));
  }
  const result = await search(input, config, { loadSecret: () => { throw new Error('canary-key'); } });
  assert.equal(result.error_code, 'credential_unavailable');
  assert(!JSON.stringify(result).includes('canary-key'));
});

test('no match, wrong country and ambiguous identity retain query evidence without success', async () => {
  for (const mutate of [p => p.data = [], p => p.data[0].attributes.address.country_code = 'AT',
    p => p.data[0].attributes.name = 'Different GmbH', p => p.data.push({ ...p.data[0], id: '43' })]) {
    const payload = fixture(); mutate(payload);
    const result = await search(input, config, dependencies(payload));
    assert.equal(result.failure_mode, 'partial_output');
    assert.equal(result.api_query_evidence.request_id, 'request-fixture');
    assert.deepEqual(result.records, []);
  }
});

test('invalid configuration and query fail before accessing credentials', async () => {
  let called = false;
  const deps = { loadSecret: () => { called = true; return 'never'; } };
  for (const override of [{ credential_ref: 'plaintext' }, { account_id: 'me' }, { native_root: 'relative' }]) {
    assert.equal((await search(input, { ...config, ...override }, deps)).error_code, 'invalid_api_configuration');
  }
  assert.equal((await search({ company: '', country: 'DE' }, config, deps)).error_code, 'company_and_dach_country_required');
  assert.equal(called, false);
});

test('missing credit receipt, malformed payload and oversized replies fail closed', async () => {
  const payload = fixture(); delete payload.meta.credits;
  assert.equal((await search(input, config, dependencies(payload))).error_code, 'credit_free_search_not_confirmed');
  assert.equal((await search(input, config, dependencies({}))).error_code, 'api_invalid_envelope');
  assert.equal((await search(input, config, dependencies(null))).error_code, 'api_invalid_envelope');
  assert.equal((await search(input, config, dependencies({ ...fixture(), data: [null] }))).error_code, 'api_invalid_envelope');
  const result = await search(input, config, { loadSecret: () => 'canary-key', fetch: async () => new Response('x'.repeat(1_048_577)) });
  assert.equal(result.error_code, 'api_invalid_response');
});

test('a truncated search cannot establish a unique company even with one exact match', async () => {
  const payload = fixture(); payload.meta.pagination.next_cursor = 'another-page';
  const result = await search(input, config, dependencies(payload));
  assert.equal(result.error_code, 'bounded_page_incomplete');
  assert.equal(result.api_query_evidence.matched_count, 1);
  assert.deepEqual(result.records, []);
});
