import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.pipeline.lead_qualification.js';
import { slugForKey } from '../_lib/knowledge.js';

function knowledgeCap(rows, log) {
  return async (argv) => {
    log.push(argv);
    if (argv[0] === 'data' && argv[1] === 'describe') return { ok: argv[5] !== 'campaign_c_1_research_runs' };
    if (argv[0] === 'data' && argv[1] === 'select') return { ok: true, rows };
    return { ok: true };
  };
}

test('qualifies contact rows and appends run status', async () => {
  const log = [];
  const rows = [
    { contact_id: 'p1', contact_name: 'Frau A', pipeline_id: 'pipe' },
    { contact_id: 'p2', role: '  ' },
    { contact_id: 'p3', email: 'x@y.de' },
  ];
  const t = createTestCtx({ now: 33, caps: { 'knowledge.query': ({ argv }) => knowledgeCap(rows, log)(argv) } });
  const result = await handle(cmd('outbound.pipeline.lead_qualification', { pipeline_id: 'pipe', campaign: { id: 'C 1', name: 'Kampagne' } }, { id: 'cmd-q' }), t.ctx);
  assert.deepEqual(result.record_ids, ['p1', 'p3']);
  assert.equal(result.records_count, 2);
  const create = log.find((a) => a[1] === 'create');
  assert.deepEqual(create.slice(0, 5), ['data', 'create', '--domain', 'outbound', '--key']);
  assert.equal(create[5], 'campaign_c_1_research_runs');
  const appends = log.filter((a) => a[1] === 'append');
  const contacts = JSON.parse(appends[0][appends[0].indexOf('--rows') + 1]);
  assert.equal(appends[0][5], 'campaign_c_1_contacts');
  assert.equal(contacts[0].stage, 'lead_qualified');
  assert.equal(contacts[0].lead_score, 80);
  assert.equal(contacts[0].pipeline_id, 'pipe');
  const run = JSON.parse(appends[1][appends[1].indexOf('--rows') + 1])[0];
  assert.equal(run.run_type, 'lead_qualification');
  assert.equal(run.run_id, 'cmd-q');
  assert.equal(run.runbook_id, 'business-os.outbound.c_1.runbook.v1');
});

test('guards: no rows, nothing qualifiable, missing pipeline, wrong app', async () => {
  const empty = createTestCtx({ caps: { 'knowledge.query': ({ argv }) => knowledgeCap([], [])(argv) } });
  assert.equal((await rejects(handle(cmd('outbound.pipeline.lead_qualification', { pipeline_id: 'p', campaign: { id: 'c' } }), empty.ctx))).code, 'not_found');
  const blank = createTestCtx({ caps: { 'knowledge.query': ({ argv }) => knowledgeCap([{ contact_id: 'x' }], [])(argv) } });
  assert.equal((await rejects(handle(cmd('outbound.pipeline.lead_qualification', { pipeline_id: 'p', campaign: { id: 'c' } }), blank.ctx))).code, 'conflict');
  assert.equal((await rejects(handle(cmd('outbound.pipeline.lead_qualification', { campaign: { id: 'c' } }), blank.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.pipeline.lead_qualification', {}, { app_id: 'x' }), blank.ctx))).code, 'invalid');
});

test('slug_for_key port', () => {
  assert.equal(slugForKey('Ärzte & Co. 2026!'), 'rzte_co._2026');
  assert.equal(slugForKey('___'), 'campaign');
});
