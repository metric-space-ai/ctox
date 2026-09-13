import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { companyCollectionBinding, collectionBinding, advanceCollection, COMPANY_DATASET, DATASET } = require('../linkedin.com/scripts/brightdata-core.cjs');
const { openCheckpoint } = require('../linkedin.com/scripts/brightdata-state.cjs');
const { projectProviderWait } = require('../linkedin.com/scripts/brightdata-continuation.cjs');

const input = { company: 'Fixture GmbH', country: 'DE', source_id: 'linkedin.com', research_operation_id: 'research-v1-' + 'a'.repeat(64) };
const companyUrl = 'https://www.linkedin.com/company/fixture/';
const personUrl = 'https://www.linkedin.com/in/fixture-person/';
const context = { rawInput: JSON.stringify(input), runDirectory: '/native/runs/scrape_run-company', targetKey: 'linkedin-com' };

function fixture(t) {
  if (process.platform === 'darwin') assert(process.env.TMPDIR?.startsWith('/Volumes/tmp/'));
  const stateRoot = fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(), 'brightdata-company-'));
  t.after(() => fs.rmSync(stateRoot, { recursive: true }));
  return { stateRoot, operationId: input.research_operation_id };
}

test('company and profile stages resume separate jobs within one native operation', async t => {
  const options = fixture(t), company = companyCollectionBinding(input, [companyUrl]);
  const requests = [];
  async function step(binding, payload, status = 200) {
    const journal = openCheckpoint({ ...options, binding });
    return advanceCollection(binding, journal.load(), { ...journal,
      loadSecret: async () => 'fixture-secret-canary',
      fetch: async (url, init) => {
        requests.push({ url, method: init.method, body: init.body });
        return new Response(JSON.stringify(payload), { status });
      },
    });
  }
  const submitted = await step(company, { snapshot_id: 'sd_company' }, 202);
  const wait = projectProviderWait(submitted, context);
  assert.equal(wait.failure_mode, 'awaiting_provider');
  assert.equal(wait.continuation.dataset_id, COMPANY_DATASET);
  assert.equal(wait.continuation.operation_id, input.research_operation_id);
  assert.deepEqual(wait.api_query_evidence.company_urls, [companyUrl]);
  assert.equal(wait.api_query_evidence.profile_urls, undefined);
  await step(company, { snapshot_id: 'sd_company', dataset_id: COMPANY_DATASET, status: 'ready' });
  const verified = await step(company, [{ url: companyUrl, name: input.company, country_code: 'DE, AT' }]);
  assert.equal(verified.company_verified, true);
  assert.deepEqual(verified.records, []);
  assert.equal(verified.company_evidence.country_match_kind, 'reported_presence_not_registered_headquarters');

  const profile = collectionBinding(input, verified.company_profile_url, [personUrl]);
  const profileWait = projectProviderWait(await step(profile, { snapshot_id: 'sd_profile' }, 202), context);
  assert.equal(profileWait.continuation.operation_id, wait.continuation.operation_id);
  assert.equal(profileWait.continuation.dataset_id, DATASET);
  assert.notEqual(profileWait.continuation.snapshot_id, wait.continuation.snapshot_id);
  await step(profile, { snapshot_id: 'sd_profile', dataset_id: DATASET, status: 'ready' });
  const people = await step(profile, [{ url: personUrl, input_url: personUrl, first_name: 'Ada', last_name: 'Example',
    position: 'Engineering', current_company: { link: companyUrl, name: input.company } }]);
  assert.equal(people.failure_mode, undefined);
  assert.equal(people.records.length, 4);
  assert.equal(openCheckpoint({ ...options, binding: company }).load().snapshot_id, 'sd_company');
  assert.equal(openCheckpoint({ ...options, binding: profile }).load().snapshot_id, 'sd_profile');
  const posts = requests.filter(request => request.method === 'POST');
  assert.equal(posts.length, 2, 'one submission per dataset across real journal reopenings');
  assert(new URL(posts[0].url).searchParams.get('dataset_id') === COMPANY_DATASET);
  assert.deepEqual(JSON.parse(posts[0].body), [{ url: companyUrl }]);
  assert(new URL(posts[1].url).searchParams.get('dataset_id') === DATASET);
  assert(!JSON.stringify([wait, verified, profileWait, people]).includes('fixture-secret-canary'));
});

test('company stage rejects wrong dataset progress and changed query on restart', async t => {
  const options = fixture(t), binding = companyCollectionBinding(input, [companyUrl]);
  const journal = openCheckpoint({ ...options, binding });
  const base = { query_hash: binding.query_hash, binding, submission_attempt: 1 };
  journal.claimSubmission({ ...base, phase: 'submitting' });
  journal.saveState({ ...base, phase: 'pending', snapshot_id: 'sd_company' });
  const result = await advanceCollection(binding, journal.load(), { ...journal,
    loadSecret: async () => 'canary',
    fetch: async () => new Response(JSON.stringify({ snapshot_id: 'sd_company', dataset_id: DATASET, status: 'ready' })),
  });
  assert.equal(result.error_code, 'progress_identity_mismatch');
  assert.equal(journal.load().phase, 'pending');
  const changed = companyCollectionBinding({ ...input, country: 'CH' }, [companyUrl]);
  assert.throws(() => openCheckpoint({ ...options, binding: changed }), /binding_mismatch|state_invalid/);
  const wait = projectProviderWait({ records: [], error_code: 'collection_pending', failure_mode: 'temporary_unreachable', continuation: journal.load() },
    { ...context, rawInput: JSON.stringify({ ...input, company: 'Other GmbH' }) });
  assert.equal(wait.error_code, 'invalid_provider_continuation');
});

test('uncertain company submission never creates a replacement POST', async t => {
  const options = fixture(t), binding = companyCollectionBinding(input, [companyUrl]);
  const journal = openCheckpoint({ ...options, binding });
  let posts = 0;
  const first = await advanceCollection(binding, null, { ...journal, loadSecret: async () => 'canary',
    fetch: async () => { posts++; throw new Error('connection lost after send'); } });
  assert.equal(first.error_code, 'submission_outcome_unknown');
  const reopened = openCheckpoint({ ...options, binding });
  const next = await advanceCollection(binding, reopened.load(), { ...reopened, loadSecret: async () => 'canary',
    fetch: async () => { posts++; throw new Error('must not submit'); } });
  assert.equal(next.error_code, 'submission_outcome_unknown');
  assert.equal(posts, 1);
});

test('company bindings reject profile URLs and unknown datasets before network', async () => {
  assert.throws(() => companyCollectionBinding(input, [personUrl]));
  let calls = 0;
  const binding = { ...companyCollectionBinding(input, [companyUrl]), dataset_id: 'foreign-dataset' };
  const result = await advanceCollection(binding, null, { loadSecret: async () => { calls++; }, fetch: async () => { calls++; } });
  assert.equal(result.error_code, 'invalid_collection_binding');
  assert.equal(calls, 0);
});
