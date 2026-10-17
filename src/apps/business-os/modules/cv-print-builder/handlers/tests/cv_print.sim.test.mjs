// Strang SKL-B4: Automatisierung der cv-print-builder-App gegen die agent-task-Simulation (Kern §16.30/§16.34).
// Knopf → ctox.cv_print.apply_parse → enqueue (App-Skill + Template) → Rückruf .done/.failed → eigene Collections.
import test from 'node:test';
import assert from 'node:assert/strict';
import { APP_DIR, createCtx, makeCommand } from './_ctx.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';

const { default: applyParse } = await import('../ctox.cv_print.apply_parse.js');

const T = 'ctox.cv_print.apply_parse';
const REF = 'b'.repeat(64);
const PAYLOAD = {
  title: 'CV strukturieren: eve.pdf', chat_id: 'chat_cv_doc_3', filename: 'eve.pdf', mime_type: 'application/pdf',
  size_bytes: 1000, sha256: REF, document_id: 'doc_3', version_id: 'doc_3_v1', source_file_ref: REF,
  source_file: { kind: 'file', ref: REF, file_id: 'cv_file_doc_3', name: 'eve.pdf', mime_type: 'application/pdf' },
  instruction: 'Strukturiere {{nicht}} das Profil.',
};
const seed = () => ({
  files: [{ id: 'cv_file_doc_3', name: 'eve.pdf', mime: 'application/pdf', size: 1000, ref: REF }],
  documents: [{ id: 'doc_3', document_id: 'doc_3', title: 'eve.pdf', status: 'parsing', display_cache: { phase: 'parsing' } }],
  document_versions: [{ id: 'doc_3_v1', document_id: 'doc_3', version: 1, model_json: { workflow: { phase: 'parsing', command_id: 'cmd_parse' } } }],
});

// Gemeinsamer Speicher für Knopf- und Rückruf-Handler; Capabilities als Objekt, damit die Simulation agent.task anhängt.
function setup() {
  const ctx = createCtx({ seed: seed() });
  ctx.cap = { doc: { extract: async () => ({ text: 'Eve Example\nCTO' }) } };
  const sim = createAgentTaskSim({ appId: 'cv-print-builder', appDir: APP_DIR, ctxFactory: () => ctx });
  return { ctx, sim };
}

async function press(ctx, sim) {
  const command = makeCommand(T, PAYLOAD, { id: 'cmd_parse' });
  sim.attach(ctx, command);
  return applyParse(command, ctx);
}

test('Knopf → Befehl → enqueue: App-Skill aus dem Paket, Template prompts/parse.md, beide Rückrufe der App', async () => {
  const { ctx, sim } = setup();
  const out = await press(ctx, sim);
  assert.equal(sim.enqueued.length, 1);
  const [{ task_id: taskId, args, skill }] = sim.enqueued;
  assert.deepEqual(out, { task_id: taskId, status: 'queued' });
  assert.deepEqual(skill, { skill: 'cv-print-builder/parser', scope: 'app', path: 'skills/parser/SKILL.md' });
  assert.equal(args.on_complete, 'ctox.cv_print.apply_parse.done');
  assert.equal(args.on_failed, 'ctox.cv_print.apply_parse.failed');
  assert.equal(args.idempotency_key, 'cmd_parse');
  // Text wörtlich wie cv_print_command_prompt + cv_print_pdf_text_prompt_enrichment (store.rs:27384, :25926).
  const expected = `Strukturiere {{nicht}} das Profil.

CV PDF extracted text:
The CTOX daemon read this PDF from Business OS files and extracted bounded text before queueing the skill. Use this text as the source of truth for the structured CV profile. Do not reopen or inline the PDF unless the extracted text is obviously corrupt.
- source_file_id: cv_file_doc_3
- source_file_ref: ${REF}
- filename: eve.pdf
- mime_type: application/pdf
- size_bytes: 1000
- sha256: ${REF}

\`\`\`text
Eve Example
CTO
\`\`\`

Business OS command:
- command_id: cmd_parse
- module: cv-print-builder
- type: ctox.cv_print.apply_parse
- document_id: doc_3
- version_id: doc_3_v1
- source_file_id: cv_file_doc_3

Return only the minified \`ctox.cv_print_profile.v1\` JSON. The CTOX service will persist it through \`ctox.cv_print.apply_parse\`.`;
  assert.equal(args.instruction, expected);
  assert.equal(ctx.$.writes.length, 0, 'Einreihen schreibt nichts');
});

test('complete → neue Version in document_versions + documents; Doppelzustellung ohne Wirkung', async () => {
  const { ctx, sim } = setup();
  await press(ctx, sim);
  const [{ task_id: taskId }] = sim.enqueued;
  const d = await sim.complete(taskId, { text: '{"candidate":{"name":"Eve","currentRole":"CTO"}}' });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.command.type, 'ctox.cv_print.apply_parse.done');
  assert.deepEqual(d.result, { document_id: 'doc_3', version_id: 'doc_3_v2', version: 2, candidate_name: 'Eve' });
  const version = ctx.$.doc('document_versions', 'doc_3_v2');
  assert.equal(version.business_command_id, 'cmd_parse');
  assert.equal(version.model_json.source.file_ref, REF);
  assert.equal(ctx.$.doc('documents', 'doc_3').current_version_id, 'doc_3_v2');
  assert.equal(ctx.$.doc('documents', 'doc_3').status, 'review');
  const writes = ctx.$.writes.length;

  assert.deepEqual(await sim.complete(taskId, { text: '{}' }), { delivered: false, reason: 'duplicate' });
  // Auch eine echte zweite Zustellung an den Handler (Kern-Wiederholung) legt keine Version an.
  const { default: done } = await import('../ctox.cv_print.apply_parse.done.js');
  const again = await done(d.command, ctx);
  assert.equal(again.duplicate, true);
  assert.equal(again.version_id, 'doc_3_v2');
  assert.equal(ctx.$.writes.length, writes, 'keine Schreibvorgänge bei Doppelzustellung');
  assert.equal(ctx.$.doc('document_versions', 'doc_3_v3'), null);
});

test('fail → Profil zeigt Fehler (phase error, task_status failed); zweite Zustellung ohne Wirkung', async () => {
  const { ctx, sim } = setup();
  await press(ctx, sim);
  const [{ task_id: taskId }] = sim.enqueued;
  const d = await sim.fail(taskId, { code: 'failed', message: 'pdftotext fehlt' });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.command.type, 'ctox.cv_print.apply_parse.failed');
  assert.deepEqual(d.result, { ok: false, status: 'failed', error: 'pdftotext fehlt', applied: true, version_id: 'doc_3_v1' });
  const wf = ctx.$.doc('document_versions', 'doc_3_v1').model_json.workflow;
  assert.equal(wf.phase, 'error');
  assert.equal(wf.task_status, 'failed');
  assert.equal(wf.error, 'pdftotext fehlt');
  assert.equal(ctx.$.doc('documents', 'doc_3').status, 'error');
  assert.equal(ctx.$.doc('documents', 'doc_3').display_cache.phase, 'error');
  const writes = ctx.$.writes.length;
  const { default: failed } = await import('../ctox.cv_print.apply_parse.failed.js');
  const again = await failed(d.command, ctx);
  assert.equal(again.applied, false);
  assert.equal(ctx.$.writes.length, writes);
});

test('cancel → on_failed mit status cancelled landet am Profil', async () => {
  const { ctx, sim } = setup();
  await press(ctx, sim);
  const [{ task_id: taskId }] = sim.enqueued;
  // Abbruch (Orchestrator-Festlegung 15.10.): der Kern stellt on_failed mit status 'cancelled' zu.
  const { default: failed } = await import('../ctox.cv_print.apply_parse.failed.js');
  sim.attach(ctx, null);
  const out = await failed(makeCommand('ctox.cv_print.apply_parse.failed', { task_id: taskId, status: 'cancelled', result: null }), ctx);
  assert.equal(out.applied, true);
  assert.equal(out.status, 'cancelled');
  const wf = ctx.$.doc('document_versions', 'doc_3_v1').model_json.workflow;
  assert.equal(wf.task_status, 'cancelled');
  assert.equal(wf.error, 'task cancelled');
});

test('Paket: kein PARSER_SKILL.md mehr, Skill und Template sind Artefaktdateien', async () => {
  const { sim } = setup();
  assert.deepEqual(sim.declaredSkills, [{ name: 'parser', path: 'skills/parser/SKILL.md' }]);
  assert.match(sim.asset('skills/parser/SKILL.md'), /^---\nname: parser\n/);
  assert.doesNotMatch(sim.asset('skills/parser/SKILL.md'), /ctox business-os|business_os\.chat\.task|desktop_file_chunks/);
  assert.throws(() => sim.asset('PARSER_SKILL.md'), { code: 'not_found' });
});
