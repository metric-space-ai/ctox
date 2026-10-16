// Origin: CTOX
// License: Apache-2.0
// Automatisierung der documents-App (Strang SKL-B3, Kern §16.30) gegen die agent-task-Simulation:
// Knopf-Befehl → enqueue (Skill + Template aus dem Paket) → Rückruf complete/failed → Ergebnis in den eigenen
// Collections; Doppelzustellung ohne Wirkung. Die DOCX-Rückschrieb-Fälle (bis SKL-B3 in documents.test.mjs gegen
// den Handler research.systematic.report.create) laufen jetzt über den Rückruf research.systematic.report.complete.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
// Handler importieren ctox:shared/prompt-template.js; Lauf mit der gemeinsamen Auflösung:
//   node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs   (aus handlers/)
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';
import reportCreate from '../research.systematic.report.create.js';
import reportComplete from '../research.systematic.report.complete.js';
import reportFailed from '../research.systematic.report.failed.js';
import summarize from '../document.summarize.js';
import reviewRisks from '../document.review-risks.js';
import runbookComplete from '../document.runbook.complete.js';
import runbookFailed from '../document.runbook.failed.js';

const APP_DIR = fileURLToPath(new URL('../../', import.meta.url));
const NOW = 1_760_000_000_000;
const HEX_A = 'a'.repeat(64);
const HEX_B = 'b'.repeat(64);
const prompt = (name) => readFileSync(new URL(`../../prompts/${name}.md`, import.meta.url), 'utf8').trim();

// ctx des Test-Stores mit überschreibbarem cap.agent (die Simulation hängt agent.task und asset an).
function simCtx(env) {
  const cap = new Proxy({ agent: {} }, { get: (target, key) => (key in target ? target[key] : env.ctx.cap[key]) });
  return { ...env.ctx, cap };
}
function newSim(env) {
  return createAgentTaskSim({ appId: 'documents', appDir: APP_DIR, ctxFactory: () => simCtx(env) });
}
async function run(sim, env, handler, command) {
  const ctx = sim.attach(simCtx(env), command);
  return handler(command, ctx);
}

function receiptRow(id, hex) {
  return {
    source_id: id, evidence_id: `ev_${id}`, snapshot_id: `snap_${id}`, retrieved_at: '2026-10-01',
    snapshot_hash: `sha256:${hex}`, evidence_eligible: true, verification_status: 'verified',
    transport_verified: true, content_extracted: true, actual_full_text_or_data: true,
    evidence_relevance_score: 9, http_status: 200, canonical_url: 'https://example.org/paper',
    source_tier: 'primary', source_type: 'journal', url_role: 'original_content', content_scope: 'full_text',
  };
}
function reportEnv({ rows = [receiptRow('r1', HEX_A), receiptRow('r2', HEX_B)], docxName = 'bearing-study.docx', extra = {} } = {}) {
  const data = {
    knowledge_tables: { 'table:receipts': { id: 'table:receipts', table_id: 'receipts' } },
    files: {
      snap_a: { id: 'snap_a', ref: HEX_A, size: 10 },
      snap_b: { id: 'snap_b', ref: HEX_B, size: 10 },
      docx_1: { id: 'docx_1', ref: 'docx-sha', name: docxName, size: 8192, created_at_ms: 5 },
    },
    ...extra,
  };
  return createTestCtx({
    data,
    now: NOW,
    caps: {
      'office.edit.prepare': async (args) => ({
        session: { kind: 'document', source: { ref: args.blob_id.ref }, editor: { ref: 'ed' } },
        manifest: { primary_text: 'Systematischer Bericht' }, model: {}, protocol: 'p', protocol_version: 1, sha256: 'ed',
      }),
      'dataframe.window': async (args) => {
        if (args.table !== 'receipts') throw { code: 'not_found', message: 'no table' };
        return { rows: rows.slice(args.skip, args.skip + args.limit), row_count: rows.length };
      },
    },
  });
}
// Befehl wie index.js dispatchNewDocumentReport (ohne instruction/required_skills seit SKL-B3).
function newReportCommand(extra = {}, id = 'cmd_report_1') {
  return cmd('research.systematic.report.create', {
    inbound_channel: 'business_os.documents',
    title: 'Bearing Study',
    prompt: 'Lagerlebensdauer für Drohnenmotoren bewerten',
    report_type_id: 'feasibility_study',
    selected_runbook_id: 'research.report.feasibility_study',
    desired_format: 'docx',
    output_filename: 'bearing-study.docx',
    output_path: 'runtime/business-os/documents/generated/bearing-study.docx',
    knowledge_context: null,
    tags: ['research', 'bearings'],
    writeback_contract: { module: 'documents', collection: 'documents', filename: 'bearing-study.docx', document_id: '' },
    client_context: { surface: 'documents-new-document' },
    ...extra,
  }, { id, record_id: 'doc_bearings' });
}
const LINKS = [
  { kind: 'source_receipt', id: 'r1', snapshot_hash: `sha256:${HEX_A}` },
  { kind: 'source_receipt', id: 'r2', table_id: 'table:receipts:chunk:3' },
];
const answerText = (answer) => `DOCX erstellt.\n${JSON.stringify(answer)}`;

// --------------------------------------------------------------------------- research.systematic.report.*

test('report.create: enqueue mit System-Skill systematic-research, Template aus dem Paket und eigenem Rückruf', async () => {
  const env = reportEnv();
  const sim = newSim(env);
  const result = await run(sim, env, reportCreate, newReportCommand());
  assert.equal(result.status, 'queued');
  assert.equal(sim.enqueued.length, 1);
  const [{ args, skill, task_id: taskId }] = sim.enqueued;
  assert.equal(result.task_id, taskId);
  assert.deepEqual(skill, { skill: 'systematic-research', scope: 'core' });
  assert.equal(args.on_complete, 'research.systematic.report.complete');
  assert.equal(args.on_failed, 'research.systematic.report.failed');
  assert.equal(args.idempotency_key, 'documents:cmd_report_1');
  assert.equal(args.thread_key, 'business-os/documents');
  // Wortlaut aus index.js (bis SKL-B3), Platzhalter gefüllt, keine offenen {{.
  assert.match(args.instruction, /^Erstelle das Word-Dokument "Bearing Study"\.\nNutzerauftrag: Lagerlebensdauer für Drohnenmotoren bewerten\n/);
  assert.ok(args.instruction.includes('Verwende report_type_id=feasibility_study.'));
  assert.ok(args.instruction.includes(prompt('knowledge-auto')));
  assert.ok(args.instruction.includes(prompt('research-report-feasibility-study')));
  assert.ok(args.instruction.includes('Speichere das finale DOCX im Workspace unter runtime/business-os/documents/generated/bearing-study.docx.'));
  assert.doesNotMatch(args.instruction, /\{\{/);
  const source = args.payload.source_command;
  assert.equal(source.id, 'cmd_report_1');
  assert.equal(source.record_id, 'doc_bearings');
  assert.equal(source.payload.output_filename, 'bearing-study.docx');
  assert.equal(source.payload.instruction, undefined);
  assert.equal(source.payload.required_skills, undefined);
  assert.equal(env.all('documents').length, 0, 'enqueue schreibt kein Dokument');
});

test('report.create: auto-Runbook und gewählte Knowledge-Tabelle; Import mit Runbook ohne Titel/Dateiname', async () => {
  const env = reportEnv({ extra: { documents: { doc_imp: { id: 'doc_imp', title: 'runbook-test', filename: 'runbook-test.md' } } } });
  const sim = newSim(env);
  await run(sim, env, reportCreate, cmd('research.systematic.report.create', {
    document_id: 'doc_imp', prompt: '', runbook_id: 'research.report.auto',
    knowledge_context: { id: 'table:loads', title: 'Measured load points', domain: 'drones', is_procedural_skill: false },
  }, { id: 'cmd_imp', record_id: 'doc_imp' }));
  const { args } = sim.enqueued[0];
  assert.ok(args.instruction.includes('Wähle den passenden report_type_id aus den CTOX Report-Blueprints.'));
  assert.ok(args.instruction.includes('Verwende die ausgewählte Knowledge-Tabelle "Measured load points" (ID table:loads, Domain drones) als Datenbasis.'));
  assert.ok(args.instruction.includes(`Nutzerauftrag: ${prompt('research-report-auto')}`), 'ohne Nutzerprompt trägt das Runbook den Auftrag');
  assert.equal(args.payload.source_command.payload.output_filename, 'runbook-test.docx');
  assert.equal(args.payload.source_command.record_id, 'doc_imp');
});

test('report.create lehnt ein unbekanntes Runbook ab, ohne eine Aufgabe anzulegen', async () => {
  const env = reportEnv();
  const sim = newSim(env);
  const err = await rejects(run(sim, env, reportCreate, newReportCommand({ selected_runbook_id: 'research.report.bogus' })));
  assert.equal(err.code, 'invalid');
  assert.equal(sim.enqueued.length, 0);
});

test('report.complete schreibt Dokument + Version mit Research-Lineage; Doppelzustellung ohne Wirkung', async () => {
  const env = reportEnv();
  const sim = newSim(env);
  await run(sim, env, reportCreate, newReportCommand());
  const taskId = sim.enqueued[0].task_id;
  const d = await sim.complete(taskId, { text: answerText({ knowledge_version_id: 'knowledge-v9', source_receipt_links: LINKS }) });
  assert.equal(d.delivered, true);
  assert.equal(d.command.type, 'research.systematic.report.complete');
  assert.equal(d.error, undefined, d.error?.message);
  const result = d.result;
  assert.equal(result.ok, true);
  assert.equal(result.task_id, taskId);
  assert.equal(result.document_id, 'doc_bearings');
  assert.equal(result.version_id, 'doc_bearings_v1');
  assert.equal(result.source_sha256, 'docx-sha');
  assert.equal(result.blob_id, 'docx_1');
  assert.equal(result.bytes, 8192);
  assert.deepEqual(result.source_receipt_snapshot_hashes, [`sha256:${HEX_A}`, `sha256:${HEX_B}`]);
  const prep = env.calls.caps.find((c) => c.name === 'office.edit.prepare');
  assert.deepEqual(prep.args[0].blob_id, { ref: 'docx-sha' });
  const doc = env.doc('documents', 'doc_bearings');
  assert.equal(doc.current_version_id, 'doc_bearings_v1');
  assert.equal(doc.source_kind, 'ctox_generated_docx');
  assert.equal(doc.status, 'Draft');
  assert.equal(doc.title, 'Bearing Study');
  assert.equal(doc.business_command_id, 'cmd_report_1');
  assert.deepEqual(doc.tags, ['research', 'bearings']);
  const version = env.doc('document_versions', 'doc_bearings_v1');
  assert.equal(version.model_json.type, 'docx');
  assert.equal(version.knowledge_version.version_id, 'knowledge-v9');

  // Zweite Zustellung über die Simulation wird verworfen; ein erneuter Aufruf des Handlers schreibt nichts.
  assert.deepEqual(await sim.complete(taskId, { text: 'nochmal' }), { delivered: false, reason: 'duplicate' });
  const writes = env.calls.writes.length;
  const again = await reportComplete(d.command, d.ctx);
  assert.equal(again.status, 'ignored');
  assert.equal(env.calls.writes.length, writes);
  assert.equal(env.all('document_versions').length, 1);
});

test('report.complete bumps to v2 when the document already has a version', async () => {
  const env = reportEnv({ extra: { document_versions: { doc_bearings_v1: { id: 'doc_bearings_v1', document_id: 'doc_bearings', version: 1 } } } });
  const sim = newSim(env);
  await run(sim, env, reportCreate, newReportCommand({}, 'cmd_report_2'));
  const d = await sim.complete(sim.enqueued[0].task_id, { knowledge_version_id: 'k9', source_receipt_links: [{ kind: 'source_receipt', id: 'r1' }] });
  assert.equal(d.result.version_id, 'doc_bearings_v2');
});

test('report.complete findet die DOCX im Workspace der Aufgabe (workspace_refs → workspace.read)', async () => {
  const env = reportEnv({ docxName: 'anderer-name.docx' });
  const sim = newSim(env);
  await run(sim, env, reportCreate, newReportCommand());
  const d = await sim.complete(sim.enqueued[0].task_id, { knowledge_version_id: 'k9', source_receipt_links: LINKS }, {
    workspace: { 'runtime/business-os/documents/generated/bearing-study.docx': { ref: 'docx-sha' } },
  });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.result.source_sha256, 'docx-sha');
  assert.deepEqual(d.command.payload.workspace_refs, ['runtime/business-os/documents/generated/bearing-study.docx']);
});

// Fachliche Ablehnungen des Rückschriebs (bis SKL-B3 gegen report.create getestet) – kein Dokument entsteht.
const REFUSALS = [
  ['ohne Knowledge-Stand', {}, { source_receipt_links: [{ kind: 'source_receipt', id: 'r1' }] }, 'invalid'],
  ['ohne Quellbeleg-Links', {}, { knowledge_version_id: 'k9' }, 'invalid'],
  ['mit nur Metadaten-Quelle', { rows: [{ ...receiptRow('r1', HEX_A), metadata_only: true }] }, { knowledge_version_id: 'k9', source_receipt_links: [{ kind: 'source_receipt', id: 'r1' }] }, 'invalid'],
  ['ohne persistierte Snapshot-Bytes', { rows: [receiptRow('r1', 'c'.repeat(64))] }, { knowledge_version_id: 'k9', source_receipt_links: [{ kind: 'source_receipt', id: 'r1' }] }, 'conflict'],
  ['mit geändertem Snapshot-Hash', {}, { knowledge_version_id: 'k9', source_receipt_links: [{ kind: 'source_receipt', id: 'r1', snapshot_hash: `sha256:${HEX_B}` }] }, 'conflict'],
  ['ohne erzeugte DOCX', { docxName: 'other.docx' }, { knowledge_version_id: 'k9', source_receipt_links: [{ kind: 'source_receipt', id: 'r1' }] }, 'not_found'],
];
for (const [name, envOptions, answer, code] of REFUSALS) {
  test(`report.complete lehnt den Rückschrieb ${name} ab (${code})`, async () => {
    const env = reportEnv(envOptions);
    const sim = newSim(env);
    await run(sim, env, reportCreate, newReportCommand());
    const d = await sim.complete(sim.enqueued[0].task_id, { text: answerText(answer) });
    assert.equal(d.error?.code, code);
    assert.equal(env.all('documents').length, 0);
    assert.equal(env.all('document_versions').length, 0);
  });
}

test('report.failed und Abbruch: Rückruf an die eigene App, kein Dokument, Grund im Ergebnis', async () => {
  const env = reportEnv();
  const sim = newSim(env);
  await run(sim, env, reportCreate, newReportCommand());
  const taskId = sim.enqueued[0].task_id;
  const d = await sim.fail(taskId, 'harness timeout');
  assert.equal(d.command.type, 'research.systematic.report.failed');
  assert.deepEqual({ ok: d.result.ok, status: d.result.status, error: d.result.error, command_id: d.result.command_id },
    { ok: false, status: 'failed', error: 'harness timeout', command_id: 'cmd_report_1' });
  assert.equal(env.all('documents').length, 0);
  assert.deepEqual(await sim.complete(taskId, { text: 'spät' }), { delivered: false, reason: 'duplicate' });
  // Abbruch kommt als on_failed mit status 'cancelled' (README agent-sim, Festlegung 15.10.).
  const cancelled = await reportFailed({ ...d.command, payload: { task_id: taskId, status: 'cancelled', result: null } }, d.ctx);
  assert.equal(cancelled.status, 'cancelled');
});

// --------------------------------------------------------------------------- document.* (Seed-Runbooks)

function docEnv() {
  return createTestCtx({
    now: NOW,
    data: {
      documents: {
        doc_1: {
          id: 'doc_1', title: 'Angebot', filename: 'angebot.docx', status: 'Imported', current_version_id: 'doc_1_v1',
          index_text: 'Lieferumfang: 12 Lager. Gewährleistung 24 Monate.', created_at_ms: 1, updated_at_ms: 1,
        },
      },
      document_versions: { doc_1_v1: { id: 'doc_1_v1', document_id: 'doc_1', version: 1, source_sha256: 'src-sha' } },
    },
  });
}
const summarizeCommand = (id = 'cmd_sum_1', payload = {}) => cmd('document.summarize', {
  document_id: 'doc_1', version_id: 'doc_1_v1', prompt: 'Fokus Gewährleistung', runbook_id: 'document.summarize', ...payload,
}, { id, record_id: 'doc_1' });

test('document.summarize: enqueue mit System-Skill doc, Seed-Wortlaut aus prompts/, Lauf am Dokument vermerkt', async () => {
  const env = docEnv();
  const sim = newSim(env);
  const result = await run(sim, env, summarize, summarizeCommand());
  const [{ args, skill, task_id: taskId }] = sim.enqueued;
  assert.equal(result.task_id, taskId);
  assert.deepEqual(skill, { skill: 'doc', scope: 'core' });
  assert.equal(args.on_complete, 'document.runbook.complete');
  assert.equal(args.on_failed, 'document.runbook.failed');
  assert.ok(args.instruction.startsWith('Fasse das ausgewählte DOCX strukturiert zusammen.\n'));
  assert.ok(args.instruction.includes('document_id doc_1, version_id doc_1_v1, Datei (files-Ref) src-sha.'));
  assert.ok(args.instruction.includes('Zusatzauftrag: Fokus Gewährleistung'));
  assert.ok(args.instruction.includes('Lieferumfang: 12 Lager.'));
  assert.doesNotMatch(args.instruction, /\{\{/);
  const run1 = env.doc('documents', 'doc_1').runbook_runs['document.summarize'];
  assert.deepEqual(run1, { task_id: taskId, command_id: 'cmd_sum_1', version_id: 'doc_1_v1', status: 'queued', updated_at_ms: NOW });
});

test('document.review-risks nutzt sein eigenes Seed-Template', async () => {
  const env = docEnv();
  const sim = newSim(env);
  await run(sim, env, reviewRisks, cmd('document.review-risks', { document_id: 'doc_1' }, { id: 'cmd_risk', record_id: 'doc_1' }));
  assert.ok(sim.enqueued[0].args.instruction.startsWith(`${prompt('document-review-risks')}\n`));
  assert.doesNotMatch(sim.enqueued[0].args.instruction, /Zusatzauftrag/);
});

test('document.runbook.complete schreibt das Ergebnis an den Lauf; Doppelzustellung und überholter Lauf ohne Wirkung', async () => {
  const env = docEnv();
  const sim = newSim(env);
  await run(sim, env, summarize, summarizeCommand('cmd_sum_1'));
  const first = sim.enqueued[0].task_id;
  await run(sim, env, summarize, summarizeCommand('cmd_sum_2'));
  const second = sim.enqueued[1].task_id;
  // Der erste Lauf ist vom zweiten überholt: sein Ergebnis wird verworfen.
  const stale = await sim.complete(first, { text: 'alt' });
  assert.equal(stale.result.status, 'ignored');
  const d = await sim.complete(second, { text: 'Zwölf Lager, 24 Monate Gewährleistung.' });
  assert.equal(d.result.status, 'completed');
  const run2 = env.doc('documents', 'doc_1').runbook_runs['document.summarize'];
  assert.equal(run2.task_id, second);
  assert.equal(run2.status, 'completed');
  assert.equal(run2.result_text, 'Zwölf Lager, 24 Monate Gewährleistung.');
  const writes = env.calls.writes.length;
  const again = await runbookComplete({ ...d.command, payload: { ...d.command.payload, result: { text: 'überschrieben?' } } }, d.ctx);
  assert.equal(again.status, 'ignored');
  assert.equal(env.calls.writes.length, writes);
  assert.equal(env.doc('documents', 'doc_1').runbook_runs['document.summarize'].result_text, 'Zwölf Lager, 24 Monate Gewährleistung.');
});

test('document.runbook.failed vermerkt Fehler bzw. Abbruch am Lauf', async () => {
  const env = docEnv();
  const sim = newSim(env);
  await run(sim, env, summarize, summarizeCommand('cmd_sum_f'));
  const d = await sim.fail(sim.enqueued[0].task_id, { code: 'failed', message: 'model quota' });
  assert.equal(d.command.type, 'document.runbook.failed');
  const failedRun = env.doc('documents', 'doc_1').runbook_runs['document.summarize'];
  assert.deepEqual([failedRun.status, failedRun.error, failedRun.result_text], ['failed', 'model quota', null]);

  await run(sim, env, summarize, summarizeCommand('cmd_sum_c'));
  const taskId = sim.enqueued[1].task_id;
  const out = await runbookFailed({ id: 'cb', type: 'document.runbook.failed', payload: { task_id: taskId, status: 'cancelled', result: null } }, sim.attach(simCtx(env)));
  assert.equal(out.status, 'cancelled');
  assert.equal(env.doc('documents', 'doc_1').runbook_runs['document.summarize'].status, 'cancelled');
});

test('document.summarize auf ein unbekanntes Dokument legt keine Aufgabe an', async () => {
  const env = docEnv();
  const sim = newSim(env);
  const err = await rejects(run(sim, env, summarize, cmd('document.summarize', { document_id: 'doc_x' }, { id: 'c', record_id: 'doc_x' })));
  assert.equal(err.code, 'not_found');
  assert.equal(sim.enqueued.length, 0);
});
