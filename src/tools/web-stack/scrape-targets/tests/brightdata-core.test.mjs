import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { canonicalLinkedIn, checkedQuery, discoverProfiles, collectionBinding,
  extractProfiles, advanceCollection, DATASET } = require('../linkedin.com/scripts/brightdata-core.cjs');
const profile = 'https://www.linkedin.com/in/fixture-person/';
const companyUrl = 'https://www.linkedin.com/company/fixture-company/';
const query = { company: 'Fixture GmbH', country: 'DE' };
const binding = () => collectionBinding(query, companyUrl, [profile]);
const row = () => ({ url: profile, input_url: profile, first_name: 'Erika', last_name: 'Beispiel',
  position: 'Geschäftsführung', current_company: { name: query.company, link: companyUrl },
  current_company_name: query.company, email: 'not-authoritative@example.invalid', gender: 'female' });
const reply = (body, status = 200) => new Response(JSON.stringify(body), { status });

test('canonical URLs reject credentials, foreign hosts, encoded paths and non-profile routes', () => {
  assert.equal(canonicalLinkedIn('https://de.linkedin.com/in/fixture-person?trk=x', 'in'), profile);
  for (const url of ['http://www.linkedin.com/in/a', 'https://linkedin.com.evil.test/in/a',
    'https://a@www.linkedin.com/in/a', 'https://www.linkedin.com:444/in/a',
    'https://www.linkedin.com/in/a%2fb', 'https://www.linkedin.com/company/a',
    'https://www.linkedin.com/in/a/extra']) assert.equal(canonicalLinkedIn(url, 'in'), null, url);
});

test('actual company/country-only input produces one bounded discovery, never field evidence', async () => {
  let calls = 0;
  const result = await discoverProfiles(query, async q => {
    calls++;
    assert.equal(q.query, '"Fixture GmbH" site:linkedin.com/in/');
    return { provider: 'fixture-search', source_failures: [], results: [
      { url: profile, title: 'An unrelated or manipulated snippet' },
      { url: profile + '?tracking=duplicate' }, { url: 'https://evil.invalid/in/person' },
      ...Array.from({ length: 30 }, (_, n) => ({ url: `https://www.linkedin.com/in/person-${n}/` }))] };
  });
  assert.equal(calls, 1); assert.equal(result.urls.length, 3);
  assert.equal(result.truncated, true); assert.equal(result.evidence_eligible, false);
  assert.equal(result.records, undefined);
  assert.throws(() => checkedQuery({ company: '\n', country: 'DE' }));
  assert.throws(() => checkedQuery({ company: 'Fixture', country: 'US' }));
});

test('blocked, partial and malformed searches do not trigger profile collection candidates', async () => {
  for (const payload of [null, {}, { results: [], provider: 'fixture-search' },
    { results: [{ url: profile }], provider: 'fixture-search', source_failures: [{ kind: 'blocked' }] },
    { ok: false, results: [{ url: profile }], provider: 'fixture-search', source_failures: [] }]) {
    const result = await discoverProfiles(query, async () => payload);
    assert.equal(result.ok, false); assert.deepEqual(result.urls, []);
  }
  assert.equal((await discoverProfiles(query, async () => { throw new Error('private-error'); })).code, 'discovery_unavailable');
});

test('structured names and exact current employer yield only supported person fields', () => {
  const result = extractProfiles([row()], binding());
  assert.equal(result.matched_profiles, 1);
  assert.deepEqual(result.records.map(r => r.field), ['person_vorname', 'person_nachname', 'person_position', 'person_linkedin']);
  assert(result.records.every(r => r.source_url === profile && r.source_id === 'linkedin.com'));
});

test('wrong company, past employer, unrelated URL, masked name and provider errors are rejected', () => {
  for (const mutate of [r => r.current_company.name = 'Other GmbH',
    r => r.current_company.link = 'https://www.linkedin.com/company/other/',
    r => { r.experience = [r.current_company]; r.current_company = null; },
    r => r.current_company_name = 'Conflicting GmbH', r => r.input_url = 'https://www.linkedin.com/in/other/',
    r => r.url = 'https://www.linkedin.com/in/other/', r => r.first_name = 'E***',
    r => r.first_name = 'x'.repeat(201), r => r.error = 'private-error']) {
    const candidate = row(); mutate(candidate);
    const result = extractProfiles([candidate], binding());
    assert.equal(result.records.length, 0); assert.equal(result.rejected.length, 1);
    assert(!JSON.stringify(result).includes('private-error'));
  }
  assert.equal(extractProfiles([row(), row()], binding()).rejected[0].reason, 'duplicate_profile');
});

test('query hash binds country, exact company URL and the bounded URL set', () => {
  assert.notEqual(collectionBinding({ ...query, country: 'AT' }, companyUrl, [profile]).query_hash, binding().query_hash);
  assert.throws(() => collectionBinding(query, companyUrl, [profile, profile]));
  assert.throws(() => collectionBinding(query, 'https://evil.invalid', [profile]));
});

test('async collection persists claim then snapshot and resumes without another POST', async () => {
  let state = null;
  const requests = [], saved = [];
  const deps = {
    loadSecret: async () => 'fixture-secret-canary',
    claimSubmission: async value => { assert.equal(state, null); state = value; saved.push(value); return true; },
    saveState: async value => { state = value; saved.push(value); },
    fetch: async (url, opts) => {
      requests.push({ url, method: opts.method });
      assert.equal(opts.redirect, 'error'); assert.equal(opts.headers.Authorization, 'Bearer fixture-secret-canary');
      if (opts.method === 'POST') {
        assert.equal(state.phase, 'submitting');
        assert.equal(new URL(url).origin, 'https://api.brightdata.com');
        assert.deepEqual(JSON.parse(opts.body), [{ url: profile }]);
        return reply({ snapshot_id: 'sd_fixture123' });
      }
      if (url.includes('/progress/')) return reply({ snapshot_id: 'sd_fixture123', dataset_id: DATASET, status: 'ready' });
      return reply([row()]);
    },
  };
  const first = await advanceCollection(binding(), state, deps);
  assert.equal(first.error_code, 'collection_pending'); assert.equal(state.phase, 'pending');
  assert.deepEqual(first.records, []);
  const second = await advanceCollection(binding(), state, deps);
  assert.equal(second.error_code, 'collection_pending'); assert.equal(state.phase, 'ready');
  const final = await advanceCollection(binding(), state, deps);
  assert.equal(final.failure_mode, undefined); assert.equal(final.records.length, 4);
  assert.equal(state.phase, 'completed'); assert.equal(requests.filter(r => r.method === 'POST').length, 1);
  assert(!JSON.stringify({ saved, first, second, final }).includes('fixture-secret-canary'));
});

test('uncertain submission blocks retries and cannot leak provider error or secret', async () => {
  let state = null, count = 0;
  const deps = { loadSecret: async () => 'fixture-secret-canary',
    claimSubmission: async s => { state = s; return true; }, saveState: async s => { state = s; },
    fetch: async () => { count++; throw new Error('fixture-secret-canary'); } };
  const first = await advanceCollection(binding(), null, deps);
  assert.equal(first.error_code, 'submission_outcome_unknown');
  const second = await advanceCollection(binding(), state, deps);
  assert.equal(second.error_code, 'submission_outcome_unknown'); assert.equal(count, 1);
  assert(!JSON.stringify({ first, second, state }).includes('fixture-secret-canary'));
});

test('checkpoint and API identity violations fail closed', async () => {
  const pending = { query_hash: binding().query_hash, snapshot_id: 'sd_fixture123', phase: 'pending' };
  const deps = { loadSecret: async () => 'canary', saveState: async () => {}, claimSubmission: async () => false,
    fetch: async () => reply({ snapshot_id: 'sd_other', dataset_id: DATASET, status: 'ready' }) };
  assert.equal((await advanceCollection(binding(), null, deps)).error_code, 'collection_already_claimed');
  assert.equal((await advanceCollection(binding(), { ...pending, query_hash: 'other' }, deps)).error_code, 'checkpoint_query_mismatch');
  assert.equal((await advanceCollection(binding(), { ...pending, snapshot_id: '../../elsewhere' }, deps)).error_code, 'checkpoint_snapshot_invalid');
  assert.equal((await advanceCollection(binding(), pending, deps)).error_code, 'progress_identity_mismatch');
});

test('provider auth errors, partial snapshots and state write failure are not success', async () => {
  const ready = { query_hash: binding().query_hash, snapshot_id: 'sd_fixture123', phase: 'ready' };
  const deps = { loadSecret: async () => 'canary', saveState: async () => {},
    fetch: async () => reply([], 401) };
  assert.equal((await advanceCollection(binding(), ready, deps)).failure_mode, 'auth_required');
  deps.fetch = async () => reply([]);
  assert.equal((await advanceCollection(binding(), ready, deps)).error_code, 'profile_evidence_incomplete');
  deps.fetch = async () => reply([row()]);
  deps.saveState = async () => { throw new Error('private-filesystem-error'); };
  assert.equal((await advanceCollection(binding(), ready, deps)).error_code, 'checkpoint_unavailable');
});
