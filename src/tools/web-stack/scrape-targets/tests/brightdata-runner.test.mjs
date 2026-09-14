import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawnSync } from 'node:child_process';
import { bundleBrightData } from '../linkedin.com/scripts/bundle-brightdata.mjs';

const companyUrl = 'https://www.linkedin.com/company/fixture/';
const personUrl = 'https://www.linkedin.com/in/fixture-person/';
const companyDataset = 'gd_l1vikfnt1wgvvqz95w';
const profileDataset = 'gd_l1viktl72bvl7bjuj0';
const canary = 'FAKE-runner-secret-must-not-appear-in-evidence';
const input = { company: 'Fixture GmbH', country: 'DE', source_id: 'linkedin.com',
  query: 'Fixture GmbH personnel', research_operation_id: 'research-v1-' + 'b'.repeat(64) };

function fixture(t, { credentialFails = false, authorization = true, uncertain = false } = {}) {
  if (process.platform === 'darwin') assert(process.env.TMPDIR?.startsWith('/Volumes/tmp/'));
  const root = fs.realpathSync(fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(), 'brightdata-runner-')));
  t.after(() => fs.rmSync(root, { recursive: true }));
  const target = path.join(root, 'target'); fs.mkdirSync(target, { mode: 0o700 });
  const runs = path.join(target, 'runs'); fs.mkdirSync(runs);
  const run = path.join(runs, 'scrape_run-fixture'); fs.mkdirSync(run);
  const executable = path.join(root, 'ctox-fixture.cjs');
  const trace = path.join(root, 'cli-trace.json');
  fs.writeFileSync(executable, `#!/usr/bin/env node\nconst fs=require('node:fs');\nconst args=process.argv.slice(2);\nconst file=${JSON.stringify(trace)};\nconst rows=fs.existsSync(file)?JSON.parse(fs.readFileSync(file)):[];\nrows.push({args,root:process.env.CTOX_ROOT});fs.writeFileSync(file,JSON.stringify(rows));\nif(process.env.CTOX_ROOT!==${JSON.stringify(root)})process.exit(8);\nif(args[0]==='secret'){\nif(${credentialFails}){process.stderr.write(${JSON.stringify(canary)});process.exit(2);}\nprocess.stdout.write(JSON.stringify({ok:true,scope:'credentials',name:args[args.indexOf('--name')+1],value:${JSON.stringify(canary)}}));\n}else{const query=args[args.indexOf('--query')+1];process.stdout.write(JSON.stringify({ok:true,provider:'fixture-native-search',source_failures:[],results:[{url:query.includes('/company/')?${JSON.stringify(companyUrl)}:${JSON.stringify(personUrl)}}]}));}\n`, { mode: 0o700 });
  const network = path.join(root, 'network.json');
  const preload = path.join(root, 'provider-fixture.cjs');
  fs.writeFileSync(preload, `const fs=require('node:fs');\nglobalThis.fetch=async (url,init)=>{\nconst endpoint=new URL(url);if(endpoint.origin!=='https://api.brightdata.com'||init.redirect!=='error'||init.headers.Authorization!==${JSON.stringify('Bearer ' + canary)})throw new Error('wrong provider binding');\nconst file=${JSON.stringify(network)};const rows=fs.existsSync(file)?JSON.parse(fs.readFileSync(file)):[];rows.push({url,method:init.method,body:init.body});fs.writeFileSync(file,JSON.stringify(rows));\nlet payload;let status=200;if(init.method==='POST'){if(${uncertain})throw new Error(${JSON.stringify(canary)});status=202;payload={snapshot_id:endpoint.searchParams.get('dataset_id')===${JSON.stringify(companyDataset)}?'sd_company':'sd_profile'};}\nelse if(endpoint.pathname.includes('/progress/')){const company=endpoint.pathname.endsWith('sd_company');payload={snapshot_id:company?'sd_company':'sd_profile',dataset_id:company?${JSON.stringify(companyDataset)}:${JSON.stringify(profileDataset)},status:'ready'};}\nelse if(endpoint.pathname.endsWith('sd_company'))payload=[{url:${JSON.stringify(companyUrl)},name:'Fixture GmbH',country_code:'DE, AT'}];\nelse payload=[{url:${JSON.stringify(personUrl)},input_url:${JSON.stringify(personUrl)},first_name:'Ada',last_name:'Example',position:'Engineering',current_company:{link:${JSON.stringify(companyUrl)},name:'Fixture GmbH'}}];\nreturn new Response(JSON.stringify(payload),{status});};\n`);
  const manifest = path.join(target, 'manifest.json');
  fs.writeFileSync(manifest, JSON.stringify({ target_key: 'linkedin-com', config: {
    expected_provider: 'linkedin.com', async_provider: 'brightdata', access_mode: 'provider_api',
    credential_ref: 'ctox-secret://credentials/BRIGHTDATA_CREW_API_KEY', brightdata_collection_authorized: authorization,
  } }));
  const bundle = path.join(root, 'adapter.cjs'); fs.writeFileSync(bundle, bundleBrightData());
  const env = { ...process.env, CTOX_ROOT: root, CTOX_BIN: executable, CTOX_SCRAPE_TARGET_DIR: target,
    CTOX_SCRAPE_RUN_DIR: run, CTOX_SCRAPE_MANIFEST_PATH: manifest, CTOX_SCRAPE_TARGET_KEY: 'linkedin-com' };
  const read = file => fs.existsSync(file) ? JSON.parse(fs.readFileSync(file)) : [];
  function invoke(request = input, override = {}) {
    const result = spawnSync(process.execPath, ['--require', preload, bundle], {
      env: { ...env, CTOX_SCRAPE_INPUT_JSON: JSON.stringify(request), ...override }, encoding: 'utf8', timeout: 10000, maxBuffer: 65536,
    });
    assert.equal(result.status, 0, 'bounded bundled runner exited successfully');
    assert.equal(result.stderr, '');
    assert(!result.stdout.includes(canary), 'no captured secret/CLI/provider error in stdout');
    return JSON.parse(result.stdout);
  }
  return { root, target, manifest, invoke, network: () => read(network), calls: () => read(trace) };
}

test('standalone bundled runner resumes across real processes without repeating discovery or company download', t => {
  const f = fixture(t);
  const outcomes = Array.from({ length: 5 }, () => f.invoke());
  for (const result of outcomes.slice(0, 4)) assert.equal(result.failure_mode, 'awaiting_provider');
  assert.equal(outcomes[0].continuation.dataset_id, companyDataset);
  assert.equal(outcomes[2].continuation.dataset_id, profileDataset);
  const final = outcomes[4];
  assert.equal(final.failure_mode, undefined);
  assert.equal(final.records.length, 4);
  assert.equal(final.company_evidence.company_snapshot_id, 'sd_company');
  assert.equal(final.api_query_evidence.snapshot_id, 'sd_profile');
  assert.equal(f.calls().filter(x => x.args[0] === 'web').length, 2);
  assert(f.calls().filter(x => x.args[0] === 'secret').every(x => x.args.at(-1) === 'BRIGHTDATA_CREW_API_KEY'));
  assert.equal(f.network().filter(x => x.method === 'POST').length, 2);
  assert.equal(f.network().filter(x => x.url.includes('/snapshot/sd_company')).length, 1);
  for (const name of fs.readdirSync(path.join(f.target, 'brightdata-state')).filter(x => x.endsWith('.json'))) {
    const file = path.join(f.target, 'brightdata-state', name);
    assert.equal(fs.statSync(file).mode & 0o777, 0o600);
    assert(!fs.readFileSync(file, 'utf8').includes(canary));
  }
});

test('changed raw research query cannot rediscover or submit within an existing operation', t => {
  const f = fixture(t);
  assert.equal(f.invoke().failure_mode, 'awaiting_provider');
  const calls = f.calls(), network = f.network();
  assert.equal(f.invoke({ ...input, query: 'different query' }).error_code, 'runner_state_or_context_invalid');
  assert.deepEqual(f.calls(), calls); assert.deepEqual(f.network(), network);
});

test('manifest authorization and secret reference cannot be supplied by research input', t => {
  const f = fixture(t, { authorization: false });
  const result = f.invoke({ ...input, collectionAuthorized: true, credential_ref: 'ctox-secret://credentials/FOREIGN', stateRoot: '/outside' });
  assert.equal(result.error_code, 'collection_not_authorized');
  assert.deepEqual(f.calls(), []); assert.deepEqual(f.network(), []);
});

test('native secret failure is bounded and redacted without a provider submission', t => {
  const f = fixture(t, { credentialFails: true });
  const result = f.invoke();
  assert.equal(result.error_code, 'credential_unavailable');
  assert.equal(result.failure_mode, 'auth_required'); assert.deepEqual(f.network(), []);
});

test('ambiguous accepted company POST is never resubmitted after runner restart', t => {
  const f = fixture(t, { uncertain: true });
  assert.equal(f.invoke().error_code, 'submission_outcome_unknown');
  assert.equal(f.invoke().error_code, 'submission_outcome_unknown');
  assert.equal(f.network().length, 1); assert.equal(f.calls().filter(x => x.args[0] === 'web').length, 2);
});

test('foreign manifest and symlinked durable state fail before native calls', t => {
  const f = fixture(t);
  const outside = path.join(f.root, 'outside'); fs.mkdirSync(outside);
  fs.symlinkSync(outside, path.join(f.target, 'brightdata-state'));
  assert.equal(f.invoke().error_code, 'runner_context_invalid');
  assert.deepEqual(f.calls(), []); assert.deepEqual(f.network(), []);
  assert.equal(f.invoke(input, { CTOX_SCRAPE_MANIFEST_PATH: f.root }).error_code, 'runner_context_invalid');
});
