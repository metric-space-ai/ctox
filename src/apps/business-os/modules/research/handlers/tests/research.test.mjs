import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import { sha256Hex, parseCsv, canonicalJson, clipText } from '../_lib/util.js';
import { receiptIsEligible, validateCsv } from '../_lib/systematic.js';
import complete from '../research.systematic.run.complete.js';

const RUN = 'run-1';
const CMD = 'cmd-r1';
const HASH = 'a'.repeat(64);

test('sha256 matches node:crypto', () => {
  for (const s of ['', 'abc', 'ä'.repeat(100), 'x'.repeat(1000)]) {
    assert.equal(sha256Hex(s), createHash('sha256').update(s, 'utf8').digest('hex'));
  }
});

test('csv parser handles quotes, CRLF, and rejects ragged rows', () => {
  assert.deepEqual(parseCsv('a,b\r\n"x,1","y ""q"""\n'), { headers: ['a', 'b'], rows: [['x,1', 'y "q"']] });
  assert.throws(() => parseCsv('a,b\n1\n'), (e) => e.code === 'invalid');
});

test('canonical json sorts keys like serde BTreeMap; clip_text collapses', () => {
  assert.equal(canonicalJson({ b: 1, a: [{ d: null, c: 'x' }] }), '{"a":[{"c":"x","d":null}],"b":1}');
  assert.equal(clipText('a  b\n c', 1200), 'a b c');
  assert.equal(clipText('abcdefgh', 6), 'abc...');
});

function eligibleReceipt(extra = {}) {
  return {
    research_run_id: RUN, research_command_id: CMD,
    source_id: 's1', evidence_id: 'e1', snapshot_id: 'snap1', retrieved_at: '2026-10-01',
    snapshot_hash: `sha256:${HASH}`, snapshot_path: 'runtime/snap1.html',
    canonical_url: 'https://example.org/paper', evidence_eligible: true, verification_status: 'verified',
    transport_verified: true, content_extracted: true, actual_full_text_or_data: true,
    evidence_relevance_score: 9, http_status: 200, source_tier: 'primary', source_type: 'journal',
    url_role: 'original_content', content_scope: 'full_text', ...extra,
  };
}

test('receipt eligibility gate', () => {
  assert.equal(receiptIsEligible(eligibleReceipt()), true);
  assert.equal(receiptIsEligible(eligibleReceipt({ canonical_url: 'https://doi.org/10.1/x' })), false);
  assert.equal(receiptIsEligible(eligibleReceipt({ http_status: 204 })), false);
  assert.equal(receiptIsEligible(eligibleReceipt({ status: 'Off_Topic' })), false);
  assert.equal(receiptIsEligible(eligibleReceipt({ evidence_relevance_score: 7 })), false);
  assert.equal(receiptIsEligible(eligibleReceipt({ snapshot_hash: HASH })), false);
});

const manifest = {
  schema_version: 'ctox.research.evidence.v2', research_run_id: RUN, research_command_id: CMD,
  evidence: [{ evidence_id: 'e1', source_id: 's1', snapshot_id: 'snap1', canonical_url: 'https://example.org/paper', snapshot_sha256: HASH }],
  claims: [{ claim_id: 'c1', evidence_id: 'e1', source_id: 's1', snapshot_id: 'snap1', canonical_url: 'https://example.org/paper', evidence_quote: 'Zitat' }],
};
const base = 'research_run_id,research_command_id';
const csvFiles = {
  'dashboard/knowledge/source_candidates.csv': `${base},name\n${RUN},${CMD},A\n`,
  'dashboard/knowledge/evidence_points.csv': `${base},claim_id,evidence_id,source_id,snapshot_id,canonical_url,snapshot_hash,quote\n${RUN},${CMD},c1,e1,s1,snap1,https://example.org/paper,sha256:${HASH.toUpperCase()},Zitat\n`,
  'dashboard/knowledge/evaluation_matrix.csv': `${base},k\n${RUN},${CMD},1\n`,
  'dashboard/knowledge/semantic_graph_nodes.csv': `${base},node\n${RUN},${CMD},n\n`,
  'dashboard/knowledge/semantic_graph_edges.csv': `${base},edge\n${RUN},${CMD},e\n`,
  'dashboard/knowledge/source_catalog.csv': `${base},source_id,canonical_url,snapshot_hash\n${RUN},${CMD},s1,https://example.org/paper,${HASH}\n`,
};

function researchWorld({ files = {}, receiptStatus = 'pass', catalogRows } = {}) {
  const manifestText = JSON.stringify(manifest);
  const manifestSha = createHash('sha256').update(manifestText).digest('hex');
  const all = {
    'validation/evidence-manifest.json': manifestText,
    '.ctox/systematic-research-validation.json': JSON.stringify({
      schema_version: 'ctox.systematic-research.validation.v1', status: receiptStatus,
      research_run_id: RUN, research_command_id: CMD, manifests: [{ manifest_sha256: manifestSha }],
    }),
    ...csvFiles,
    ...files,
  };
  const imports = [];
  const created = [];
  const settled = [];
  const { ctx, state } = makeCtx({
    now: 1_760_000_123_000,
    collections: {
      business_commands: [{
        id: CMD, module: 'research', command_type: 'research.systematic.run', record_id: 'rt-1',
        payload: { knowledge_domain: 'drones', research_run_id: RUN, title: 'Nachrecherche' }, client_context: { inbound_channel: 'business_os.research' },
      }],
      research_tasks: [{ id: 'rt-1', title: 'Drohnen', status: 'running', payload: { keep: true } }],
      research_runs: [{ id: RUN, command_id: CMD, status: 'queued', payload: {} }],
      // Katalog der App knowledge (foreign read) und Snapshot-Bytes als files-Datensatz (§16.32).
      knowledge_tables: [
        { id: 'table:k1', kind: 'dataframe', title: 'Quellen', updated_at_ms: 1, payload: { domain: 'drones', table_key: 'source_catalog' } },
        { id: 'table:k2', kind: 'dataframe', title: 'Belege', updated_at_ms: 1, payload: { domain: 'drones', table_key: 'evidence_points' } },
        { id: 'table:k9', kind: 'dataframe', title: 'Andere', updated_at_ms: 1, payload: { domain: 'boats', table_key: 'source_catalog' } },
      ],
      files: [{ id: 'snap-file', ref: HASH.toLowerCase(), size: 10 }],
    },
    caps: {
      agent: {
        task: {
          enqueue: (r) => ({ task_id: `task-${r.idempotency_key}` }),
          get: ({ task_id }) => (task_id === 'task-9' ? { task_id, payload: { business_command_id: CMD } } : null),
          workspace: {
            read: ({ path }) => (path in all ? { text: all[path], sha256: createHash('sha256').update(all[path]).digest('hex') } : null),
          },
        },
        // CAP-3b #844: agent.command.settle gibt es nicht – ein Aufruf schlüge hier fehl (Attrappe nach Vertrag).
        queue: {
          task: {
            get: ({ task_id }) => (task_id === 'task-9' ? { message_key: 'task-9', route_status: 'handled', workspace_root: '/ws' } : null),
            for_command: () => ({ task_id: 'task-9' }),
          },
        },
      },
      knowledge: {
        // Kernprimitiv knowledge.query({argv, workspace_task_id?}) = `ctox knowledge data …`.
        query: ({ argv, workspace_task_id: wsTask }) => {
          const key = argv[argv.indexOf('--key') + 1];
          if (argv[1] === 'describe') return key === 'source_catalog' ? { ok: true } : { ok: false };
          if (argv[1] === 'create') { created.push(key); return { ok: true }; }
          if (argv[1] === 'import') {
            assert.equal(wsTask, 'task-9', 'import liest aus dem Workspace der Aufgabe');
            imports.push(key);
            const text = all[argv[argv.indexOf('--from-file') + 1]];
            return { ok: true, row_count: text.trim().split('\n').length - 1 };
          }
          throw { code: 'invalid', message: `unexpected argv ${argv.join(' ')}` };
        },
      },
      dataframe: {
        window: ({ table, skip, limit }) => {
          const rows = table === 'k1' ? (catalogRows ?? [eligibleReceipt(), eligibleReceipt({ source_id: 's2', http_status: 500 })]) : [{ research_run_id: RUN, research_command_id: CMD, claim_id: 'c1' }];
          return { rows: rows.slice(skip, skip + limit), row_count: rows.length };
        },
      },
    },
  });
  return { ctx, state, imports, created, settled };
}

test('research.systematic.run.complete promotes, snapshots evidence and persists', async () => {
  const { ctx, state, imports, created, settled } = researchWorld();
  const out = await complete(makeCommand('research.systematic.run.complete', { command_id: CMD, reply_text: 'Fertig  mit\nErgebnis' }), ctx);
  assert.equal(out.status, 'completed');
  assert.equal(out.task_id, 'task-9');
  assert.deepEqual(imports.at(-1), 'source_catalog');
  assert.equal(imports.length, 6);
  assert.equal(created.length, 5);
  assert.equal(out.result.identified_count, 2);
  assert.equal(out.result.accepted_count, 1);
  assert.equal(out.result.summary, 'Fertig mit Ergebnis');
  assert.match(out.result.knowledge_version.version_id, /^sha256:[0-9a-f]{64}$/);
  assert.deepEqual(out.result.knowledge_version.table_ids, ['table:k1', 'table:k2']);
  const task = state.collections.get('research_tasks').get('rt-1');
  assert.equal(task.status, 'ready');
  assert.equal(task.payload.keep, true);
  assert.equal(task.payload.last_run.accepted_count, 1);
  const runDoc = state.collections.get('research_runs').get(RUN);
  assert.equal(runDoc.status, 'completed');
  assert.equal(runDoc.task_queue_id, 'task-9');
  // Befehls-Lebenszyklus: Ergebnis trägt der Rückruf selbst, kein Schreiben in business_commands.
  assert.equal(settled.length, 0);
  assert.equal(out.task_status, 'completed');
  assert.equal(out.result.accepted_count, 1);
});

test('completion rejects a failing validation receipt before any import', async () => {
  const { ctx, imports } = researchWorld({ receiptStatus: 'fail' });
  const error = await rejectsWithCode(complete(makeCommand('research.systematic.run.complete', { command_id: CMD }), ctx), 'invalid');
  assert.match(error.message, /passing validation receipt/);
  assert.equal(imports.length, 0);
});

test('completion rejects CSV rows with foreign run binding', async () => {
  const { ctx } = researchWorld({ files: { 'dashboard/knowledge/evaluation_matrix.csv': `${base},k\nother,${CMD},1\n` } });
  const error = await rejectsWithCode(complete(makeCommand('research.systematic.run.complete', { command_id: CMD }), ctx), 'invalid');
  assert.match(error.message, /evaluation_matrix.csv row 1 is bound to another research run/);
});

test('completion without eligible receipt persists failure on task, run and command', async () => {
  const { ctx, state, settled } = researchWorld({ catalogRows: [eligibleReceipt({ verification_status: 'pending' })] });
  await rejectsWithCode(complete(makeCommand('research.systematic.run.complete', { command_id: CMD }), ctx), 'invalid');
  assert.equal(state.collections.get('research_tasks').get('rt-1').status, 'failed');
  assert.equal(state.collections.get('research_runs').get(RUN).payload.error.command_id, CMD);
  assert.equal(settled.length, 0);
  assert.match(state.collections.get('research_runs').get(RUN).payload.error.message, /eligible source receipt/);
});

test('measured_load_points validation (only when requested)', () => {
  const out = { file_name: 'measured_load_points.csv', table_key: 'measured_load_points', require_manifest_evidence: true, require_manifest_claim: false };
  const eligible = new Map([['s1', ['https://example.org/paper', HASH]]]);
  const head = `${base},source_id,canonical_url,snapshot_hash,source_row_ref,measurement_kind,is_derived,rpm,propeller_size,prop_diameter_in,prop_pitch_in,torque_Nm,archive_manifest_hash,archive_member_path,archive_member_hash,thrust_N`;
  const good = `${RUN},${CMD},s1,https://example.org/paper,${HASH},row 4,measured,false,5000,10x4.5,10,4.5,,${HASH},a/b.csv,${HASH},12.5`;
  assert.equal(validateCsv(`${head}\n${good}\n`, out, RUN, CMD, eligible, new Map()), 1);
  assert.throws(() => validateCsv(`${head}\n${good.replace('measured', 'derived')}\n`, out, RUN, CMD, eligible, new Map()), (e) => /not identified as a direct measurement/.test(e.message));
  assert.throws(() => validateCsv(`${head}\n${good.replace(',5000,', ',0,')}\n`, out, RUN, CMD, eligible, new Map()), (e) => /positive machine-readable RPM/.test(e.message));
  assert.throws(() => validateCsv(`${head},fact_label\n${good},x\n`, out, RUN, CMD, eligible, new Map()), (e) => /generic fact columns/.test(e.message));
});


test('§16.30 Rückruf {task_id, status, result}: Befehls-ID aus agent.task.get, Antworttext aus result', async () => {
  const { ctx, state } = researchWorld();
  const out = await complete(makeCommand('research.systematic.run.complete', { task_id: 'task-9', status: 'completed', result: { text: 'Fertig' } }), ctx);
  assert.equal(out.command_id, CMD);
  assert.equal(out.result.summary, 'Fertig');
  assert.ok(!state.capCalls.some((c) => c.path.startsWith('agent.queue.projection')), 'projection.refresh entfällt (§16.30)');
  assert.ok(state.capCalls.some((c) => c.path === 'agent.task.workspace.read'));
});

test('§16.32 Beleg ohne Snapshot-Bytes in files zählt nicht als angenommen → Abschluss abgelehnt', async () => {
  const { ctx, state } = researchWorld();
  state.collections.get('files').clear();
  const error = await rejectsWithCode(complete(makeCommand('research.systematic.run.complete', { command_id: CMD, reply_text: 'x' }), ctx), 'invalid');
  assert.match(error.message, /persisted, eligible source receipt/);
});
