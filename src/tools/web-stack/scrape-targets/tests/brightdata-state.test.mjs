import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const modulePath = require.resolve('../linkedin.com/scripts/brightdata-state.cjs');
const { openCheckpoint } = require(modulePath);
const { collectionBinding, advanceCollection } = require('../linkedin.com/scripts/brightdata-core.cjs');
const binding = collectionBinding({ company: 'Fixture GmbH', country: 'DE' },
  'https://www.linkedin.com/company/fixture/', ['https://www.linkedin.com/in/fixture-person/']);
const submitting = () => ({ query_hash: binding.query_hash, phase: 'submitting', binding, submission_attempt: 1 });
const pending = () => ({ ...submitting(), phase: 'pending', snapshot_id: 'sd_fixture' });

function fixture(t) {
  if (process.platform === 'darwin') assert(process.env.TMPDIR?.startsWith('/Volumes/tmp/'), 'Mac tests require explicitly routed tmp volume');
  const stateRoot = fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(), 'brightdata-state-'));
  t.after(() => fs.rmSync(stateRoot, { recursive: true }));
  return { stateRoot, operationId: 'fixture-command-1', binding };
}

function child(options, source) {
  return new Promise((resolve, reject) => {
    const processHandle = spawn(process.execPath, ['-e', source, modulePath, JSON.stringify(options)],
      { stdio: ['ignore', 'pipe', 'pipe'], timeout: 5000 });
    let stdout = '', stderr = '';
    processHandle.stdout.on('data', data => { stdout += data; });
    processHandle.stderr.on('data', data => { stderr += data; });
    processHandle.on('error', reject);
    processHandle.on('close', code => resolve({ code, stdout, stderr }));
  });
}

test('reopened journal resumes the same snapshot across all allowed phases', t => {
  const options = fixture(t);
  const first = openCheckpoint(options);
  assert.equal(first.load(), null); assert.equal(first.claimSubmission(submitting()), true);
  first.saveState(pending());
  const reopened = openCheckpoint(options);
  assert.deepEqual(reopened.load(), pending());
  reopened.saveState({ ...pending(), phase: 'ready' });
  reopened.saveState({ ...pending(), phase: 'completed' });
  assert.equal(openCheckpoint(options).load().phase, 'completed');
  assert.equal(openCheckpoint(options).claimSubmission(submitting()), false);
});

test('two actual child processes can claim an operation only once', async t => {
  const options = fixture(t);
  const script = `const { openCheckpoint } = require(process.argv[1]);
    const o = JSON.parse(process.argv[2]); const s = openCheckpoint(o);
    console.log(s.claimSubmission({phase:'submitting',query_hash:o.binding.query_hash,binding:o.binding}));`;
  const outputs = await Promise.all([child(options, script), child(options, script)]);
  assert(outputs.every(output => output.code === 0), JSON.stringify(outputs));
  assert.deepEqual(outputs.map(output => output.stdout.trim()).sort(), ['false', 'true']);
  assert.equal(openCheckpoint(options).load().phase, 'submitting');
});

test('native 120-poll budget does not exhaust revisions on unchanged pending state', t => {
  const options = fixture(t), journal = openCheckpoint(options);
  journal.claimSubmission(submitting()); journal.saveState(pending());
  for (let poll = 0; poll < 120; poll++) journal.saveState(pending());
  const directory = fs.readdirSync(options.stateRoot).map(name => path.join(options.stateRoot, name))[0];
  assert.equal(fs.readdirSync(directory).filter(name => /^revision-/.test(name)).length, 2);
  journal.saveState({ ...pending(), phase: 'ready' });
  journal.saveState({ ...pending(), phase: 'completed' });
  for (let reread = 0; reread < 120; reread++) journal.saveState({ ...pending(), phase: 'completed' });
  assert.equal(fs.readdirSync(directory).filter(name => /^revision-/.test(name)).length, 4);
  assert.equal(openCheckpoint(options).load().phase, 'completed');
});

test('unchanged pending save does not silently accept a stale process checkpoint', async t => {
  const options = fixture(t), stale = openCheckpoint(options);
  stale.claimSubmission(submitting()); stale.saveState(pending());
  const result = await child(options, `const {openCheckpoint}=require(process.argv[1]);
    const journal=openCheckpoint(JSON.parse(process.argv[2]));
    journal.saveState({...journal.load(),phase:'ready'});`);
  assert.equal(result.code, 0, result.stderr);
  assert.throws(() => stale.saveState(pending()), /checkpoint_write_conflict/);
  assert.equal(openCheckpoint(options).load().phase, 'ready');
});

test('process exit after durable claim does not authorize another provider POST', async t => {
  const options = fixture(t);
  const result = await child(options, `const { openCheckpoint } = require(process.argv[1]);
    const o = JSON.parse(process.argv[2]); const s = openCheckpoint(o);
    s.claimSubmission({phase:'submitting',query_hash:o.binding.query_hash,binding:o.binding}); process.exit(7);`);
  assert.equal(result.code, 7);
  const reopened = openCheckpoint(options);
  let posts = 0;
  const outcome = await advanceCollection(binding, reopened.load(), {
    ...reopened, loadSecret: async () => 'canary', fetch: async () => { posts++; } });
  assert.equal(outcome.error_code, 'submission_outcome_unknown'); assert.equal(posts, 0);
});

test('stale writers cannot overwrite or regress a newer durable revision', t => {
  const options = fixture(t), a = openCheckpoint(options);
  a.claimSubmission(submitting()); a.saveState(pending());
  const b = openCheckpoint(options);
  a.saveState({ ...pending(), phase: 'ready' });
  assert.throws(() => b.saveState(pending()), /checkpoint_write_conflict/);
  assert.equal(openCheckpoint(options).load().phase, 'ready');
  assert.throws(() => a.saveState(pending()), /checkpoint_transition_invalid/);
  assert.throws(() => a.saveState({ ...pending(), phase: 'completed', snapshot_id: 'sd_other' }), /snapshot_changed/);
});

test('restart after definite HTTP401 permits one newly authenticated submission', async t => {
  const options = fixture(t);
  const result = await child(options, `const { openCheckpoint } = require(process.argv[1]);
    const { advanceCollection } = require(require('node:path').join(require('node:path').dirname(process.argv[1]), 'brightdata-core.cjs'));
    const o = JSON.parse(process.argv[2]); const s = openCheckpoint(o);
    advanceCollection(o.binding,s.load(),{...s,loadSecret:async()=> 'old-canary',
      fetch:async()=>new Response('{}',{status:401})}).then(r=>console.log(r.error_code));`);
  assert.equal(result.code, 0); assert.equal(result.stdout.trim(), 'api_unauthorized');
  const reopened = openCheckpoint(options);
  assert.equal(reopened.load().phase, 'rejected');
  let posts = 0;
  const outcome = await advanceCollection(binding, reopened.load(), { ...reopened,
    loadSecret: async () => 'new-canary', fetch: async (_url, request) => {
      posts++; assert.equal(request.headers.Authorization, 'Bearer new-canary');
      return new Response(JSON.stringify({ snapshot_id: 'sd_afterreauth' }));
    } });
  assert.equal(posts, 1); assert.equal(outcome.error_code, 'collection_pending');
  const finalState = openCheckpoint(options).load();
  assert.equal(finalState.phase, 'pending'); assert.equal(finalState.submission_attempt, 2);
  assert.equal(finalState.snapshot_id, 'sd_afterreauth');
});

test('definite rejection retry is bounded and cannot change the operation query', async t => {
  const options = fixture(t);
  let posts = 0, loads = 0;
  for (let attempt = 1; attempt <= 3; attempt++) {
    const state = openCheckpoint(options);
    const result = await advanceCollection(binding, state.load(), { ...state,
      loadSecret: async () => { loads++; return 'bad-canary'; },
      fetch: async () => { posts++; return new Response('{}', { status: 401 }); } });
    assert.equal(result.error_code, attempt < 3 ? 'api_unauthorized' : 'reauthorization_budget_exhausted');
  }
  assert.equal(posts, 2); assert.equal(loads, 2);
  const state = openCheckpoint(options);
  assert.throws(() => state.claimSubmission({ ...submitting(), submission_attempt: 1 }), /attempt_transition_invalid/);
  assert.throws(() => state.claimSubmission({ ...submitting(), submission_attempt: 3 }), /attempt_invalid/);
});

test('a changed query cannot reuse an operation; a separate operation remains independent', t => {
  const options = fixture(t), state = openCheckpoint(options);
  state.claimSubmission(submitting());
  const different = collectionBinding({ company: 'Other GmbH', country: 'AT' },
    binding.company_profile_url, binding.urls);
  assert.throws(() => openCheckpoint({ ...options, binding: different }), /checkpoint_state_invalid/);
  assert.equal(openCheckpoint({ ...options, operationId: 'fixture-command-2' }).load(), null);
});

test('credential and raw-error fields are excluded from disk records', t => {
  const options = fixture(t), state = openCheckpoint(options);
  state.claimSubmission({ ...submitting(), secret: 'secret-canary', error: 'raw-error-canary' });
  state.saveState({ ...pending(), Authorization: 'secret-canary', api_query_evidence: { raw: 'raw-error-canary' } });
  const directory = path.join(options.stateRoot, fs.readdirSync(options.stateRoot)[0]);
  const contents = fs.readdirSync(directory).map(name => fs.readFileSync(path.join(directory, name), 'utf8')).join('');
  assert(!contents.includes('secret-canary')); assert(!contents.includes('raw-error-canary'));
  for (const name of fs.readdirSync(directory)) assert.equal(fs.statSync(path.join(directory, name)).mode & 0o077, 0);
});

test('corrupt, missing and symlinked revisions fail closed without automatic repair', t => {
  for (const mode of ['corrupt', 'gap', 'symlink']) {
    const options = fixture(t), state = openCheckpoint(options);
    state.claimSubmission(submitting()); state.saveState(pending());
    const directory = path.join(options.stateRoot, fs.readdirSync(options.stateRoot)[0]);
    const file = path.join(directory, 'revision-001.json');
    if (mode === 'corrupt') fs.writeFileSync(file, '{}');
    else if (mode === 'gap') fs.unlinkSync(file);
    else { fs.renameSync(file, path.join(options.stateRoot, 'outside.json')); fs.symlinkSync(path.join(options.stateRoot, 'outside.json'), file); }
    assert.throws(() => openCheckpoint(options));
    assert.equal(fs.existsSync(path.join(directory, 'revision-002.json')), true);
  }
});

test('symlinked roots and writable-by-others directories are rejected', t => {
  const options = fixture(t);
  const link = options.stateRoot + '-link';
  fs.symlinkSync(options.stateRoot, link); t.after(() => fs.unlinkSync(link));
  assert.throws(() => openCheckpoint({ ...options, stateRoot: link }), /unsafe_checkpoint_directory/);
  fs.chmodSync(options.stateRoot, 0o777);
  assert.throws(() => openCheckpoint(options), /writable_by_others/);
  fs.chmodSync(options.stateRoot, 0o700);
});
