// Original provider-I/O fixture. Loaded by the REAL native registered executor.
// One source finishes immediately; one accepts a job then finishes on resume.
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const raw = process.env.CTOX_SCRAPE_INPUT_JSON;
const input = JSON.parse(raw);
const runDir = process.env.CTOX_SCRAPE_RUN_DIR;
const counterPath = path.resolve(runDir, '..', '..', 'fixture-counter.json');
const state = fs.existsSync(counterPath)
  ? JSON.parse(fs.readFileSync(counterPath, 'utf8')) : { calls: 0, submissions: 0, operations: [] };
state.calls++;
state.operations.push(input.research_operation_id);
const pending = input.source_id === 'linkedin.com' && state.calls === 1;
if (pending) state.submissions++;
fs.writeFileSync(counterPath, JSON.stringify(state), { mode: 0o600 });
if (pending) {
  console.log(JSON.stringify({ records: [], failure_mode: 'awaiting_provider', continuation: {
    schema: 'ctox.scrape.provider_continuation.v1', run_id: path.basename(runDir),
    input_sha256: createHash('sha256').update(raw).digest('hex'),
    operation_id: input.research_operation_id, source_id: input.source_id,
    target_key: process.env.CTOX_SCRAPE_TARGET_KEY, provider: 'brightdata',
    company: input.company, country: input.country, dataset_id: 'gd_l1viktl72bvl7bjuj0',
    snapshot_id: 'sd_recoveryfixture', query_hash: 'b'.repeat(64),
    phase: 'pending', submission_attempt: 1, retry_after_seconds: 30,
  }}));
} else {
  const linkedin = input.source_id === 'linkedin.com';
  const url = linkedin ? 'https://www.linkedin.com/in/ada-example/' : 'https://www.xing.com/profile/Ada_Example';
  console.log(JSON.stringify({ records: [{
    field: linkedin ? 'person_linkedin' : 'person_xing', value: url,
    source_url: url, source_id: input.source_id, source_key: 'primary',
    company: input.company, confidence: 'high', provider_record_id: url,
    note: 'Original synthetic company-bound recovery fixture.',
  }] }));
}
