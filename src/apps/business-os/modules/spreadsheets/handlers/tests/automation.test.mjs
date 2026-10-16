// Origin: CTOX
// License: Apache-2.0
// Seed-Runbooks spreadsheet.* (Strang SKL-B3, Kern §16.30) gegen die agent-task-Simulation: Befehl → enqueue
// (System-Skill spreadsheet, Template aus prompts/) → Rückruf complete/failed → Ergebnis an der Tabelle;
// Doppelzustellung und überholte Läufe ohne Wirkung.
// Lauf (aus handlers/): node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';
import summarize from '../spreadsheet.summarize.js';
import auditFormulas from '../spreadsheet.audit-formulas.js';
import riskReview from '../spreadsheet.risk-review.js';
import runbookComplete from '../spreadsheet.runbook.complete.js';
import runbookFailed from '../spreadsheet.runbook.failed.js';

const APP_DIR = fileURLToPath(new URL('../../', import.meta.url));
const NOW = 1_760_000_000_000;
const prompt = (name) => readFileSync(new URL(`../../prompts/${name}.md`, import.meta.url), 'utf8').trim();

function simCtx(env) {
  const cap = new Proxy({ agent: {} }, { get: (target, key) => (key in target ? target[key] : env.ctx.cap[key]) });
  return { ...env.ctx, cap };
}
function setup() {
  const env = createTestCtx({
    now: NOW,
    data: {
      spreadsheets: {
        sh_1: {
          id: 'sh_1', title: 'Budget 2026', filename: 'budget.xlsx', status: 'Imported', current_version_id: 'sh_1_v1',
          index_text: 'Q1;Q2\n100;250', created_at_ms: 1, updated_at_ms: 1,
        },
      },
      spreadsheet_versions: { sh_1_v1: { id: 'sh_1_v1', spreadsheet_id: 'sh_1', version: 1, source_sha256: 'sheet-sha' } },
    },
  });
  const sim = createAgentTaskSim({ appId: 'spreadsheets', appDir: APP_DIR, ctxFactory: () => simCtx(env) });
  const run = (handler, command) => handler(command, sim.attach(simCtx(env), command));
  return { env, sim, run };
}
// Befehl wie index.js dispatchSpreadsheetRunbook.
const runbookCommand = (type, id, extra = {}) => cmd(type, {
  spreadsheet_id: 'sh_1', version_id: 'sh_1_v1', prompt: '', runbook_id: type, source_action: 'spreadsheet_runbook', ...extra,
}, { id, record_id: 'sh_1' });

const CASES = [
  ['spreadsheet.summarize', summarize, 'spreadsheet-summarize'],
  ['spreadsheet.audit-formulas', auditFormulas, 'spreadsheet-audit-formulas'],
  ['spreadsheet.risk-review', riskReview, 'spreadsheet-risk-review'],
];
for (const [type, handler, template] of CASES) {
  test(`${type}: enqueue mit System-Skill spreadsheet, Seed-Wortlaut aus prompts/${template}.md, Lauf vermerkt`, async () => {
    const { env, sim, run } = setup();
    const result = await run(handler, runbookCommand(type, `cmd_${template}`));
    const [{ args, skill, task_id: taskId }] = sim.enqueued;
    assert.equal(result.task_id, taskId);
    assert.deepEqual(skill, { skill: 'spreadsheet', scope: 'core' });
    assert.equal(args.on_complete, 'spreadsheet.runbook.complete');
    assert.equal(args.on_failed, 'spreadsheet.runbook.failed');
    assert.equal(args.idempotency_key, `spreadsheets:cmd_${template}`);
    assert.ok(args.instruction.startsWith(`${prompt(template)}\n`));
    assert.ok(args.instruction.includes('spreadsheet_id sh_1, version_id sh_1_v1, Datei (files-Ref) sheet-sha.'));
    assert.ok(args.instruction.includes('Q1;Q2\n100;250'));
    assert.doesNotMatch(args.instruction, /\{\{|Zusatzauftrag/);
    assert.deepEqual(env.doc('spreadsheets', 'sh_1').runbook_runs[type],
      { task_id: taskId, command_id: `cmd_${template}`, version_id: 'sh_1_v1', status: 'queued', updated_at_ms: NOW });
  });
}

test('spreadsheet.summarize übernimmt den Nutzerprompt als Zusatzauftrag', async () => {
  const { sim, run } = setup();
  await run(summarize, runbookCommand('spreadsheet.summarize', 'cmd_p', { prompt: 'Nur Q2' }));
  assert.ok(sim.enqueued[0].args.instruction.includes('Zusatzauftrag: Nur Q2'));
});

test('spreadsheet.runbook.complete schreibt das Ergebnis; Doppelzustellung und überholter Lauf ohne Wirkung', async () => {
  const { env, sim, run } = setup();
  await run(summarize, runbookCommand('spreadsheet.summarize', 'cmd_1'));
  await run(summarize, runbookCommand('spreadsheet.summarize', 'cmd_2'));
  const [first, second] = sim.enqueued.map((e) => e.task_id);
  assert.equal((await sim.complete(first, { text: 'alt' })).result.status, 'ignored');
  const d = await sim.complete(second, { text: 'Q2 wächst um 150 %.' });
  assert.equal(d.command.type, 'spreadsheet.runbook.complete');
  assert.equal(d.result.status, 'completed');
  const runEntry = env.doc('spreadsheets', 'sh_1').runbook_runs['spreadsheet.summarize'];
  assert.deepEqual([runEntry.task_id, runEntry.status, runEntry.result_text], [second, 'completed', 'Q2 wächst um 150 %.']);
  assert.deepEqual(await sim.complete(second, { text: 'nochmal' }), { delivered: false, reason: 'duplicate' });
  const writes = env.calls.writes.length;
  const again = await runbookComplete({ ...d.command, payload: { ...d.command.payload, result: { text: 'überschrieben?' } } }, d.ctx);
  assert.equal(again.status, 'ignored');
  assert.equal(env.calls.writes.length, writes);
});

test('spreadsheet.runbook.failed vermerkt Fehler bzw. Abbruch am Lauf', async () => {
  const { env, sim, run } = setup();
  await run(auditFormulas, runbookCommand('spreadsheet.audit-formulas', 'cmd_f'));
  const d = await sim.fail(sim.enqueued[0].task_id, { code: 'failed', message: 'model quota' });
  assert.equal(d.command.type, 'spreadsheet.runbook.failed');
  const failedRun = env.doc('spreadsheets', 'sh_1').runbook_runs['spreadsheet.audit-formulas'];
  assert.deepEqual([failedRun.status, failedRun.error, failedRun.result_text], ['failed', 'model quota', null]);

  await run(auditFormulas, runbookCommand('spreadsheet.audit-formulas', 'cmd_c'));
  const taskId = sim.enqueued[1].task_id;
  const out = await runbookFailed({ id: 'cb', type: 'spreadsheet.runbook.failed', payload: { task_id: taskId, status: 'cancelled', result: null } }, sim.attach(simCtx(env)));
  assert.equal(out.status, 'cancelled');
  assert.equal(env.doc('spreadsheets', 'sh_1').runbook_runs['spreadsheet.audit-formulas'].status, 'cancelled');
});

test('spreadsheet.summarize auf eine unbekannte Tabelle legt keine Aufgabe an', async () => {
  const { sim, run } = setup();
  const err = await rejects(run(summarize, cmd('spreadsheet.summarize', { spreadsheet_id: 'sh_x' }, { id: 'c', record_id: 'sh_x' })));
  assert.equal(err.code, 'not_found');
  assert.equal(sim.enqueued.length, 0);
});
