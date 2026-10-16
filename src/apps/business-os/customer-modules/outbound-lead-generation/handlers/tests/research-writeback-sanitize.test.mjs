// Teilschritt 4 (Rest) research_writeback: Bereinigung, Endprüfungen, Artefaktpfade (Fehlertexte wörtlich aus Rust).
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx } from './_ctx.mjs';
import { parseResearchWritebackRequest } from '../_lib/research-writeback-payload.js';
import { CrmBaseline } from '../_lib/research-validation.js';
import {
  sanitizeResearchWriteback, validateTerminalField, actionRequiredHasAuthReference, swapRemoveKey, safeWorkspaceSegment,
  taskWorkspace, validateOriginalResearchCommand, validateTaskCorrelation, addFieldStatusEvidence, canonicalRequestedFields,
  phaseAVerifiedSources, phaseAAttemptedSources, stampWritebackFieldRevisions, fieldDefinition,
} from '../_lib/research-writeback-sanitize.js';

const base = { record_id: 'l', module: 'outbound-lead-generation', research_command_id: 'c' };
const req = (extra) => parseResearchWritebackRequest({ ...base, ...extra });
const noCrm = new CrmBaseline();

test('sanitize: dropped fields, unverified values, evidence and host rules (ref :2434)', () => {
  const r = req({
    field_status: {
      firma_name: { status: 'verified', value: 'ACME GmbH', sources: [{ source_id: 'nd', url: 'https://www.northdata.de/a', quote: 'ACME GmbH' }] },
      firma_ort: { status: 'no_match', value: 'Hamburg', attempts: [] },
      mitarbeiter: { status: 'verified', value: '30', sources: [{ source_id: 'lf', url: 'https://leadfeeder.com/x', quote: 'employee range 11-100' }] },
      umsatz: { status: 'verified', value: '5', sources: [{ source_id: 'ex', url: 'https://example.com/x', quote: 'Umsatz 5 Mio' }] },
    },
    result: {
      fields: { firma_name: { value: 'ACME GmbH' }, firma_ort: { value: 'Hamburg' }, wz_code: { value: '1' } },
      evidence: [
        { field_key: 'firma_name', source_id: 'nd', url: 'https://northdata.de/a', quote: 'ACME GmbH' },
        { field_key: 'firma_name', source_id: 'nd', url: 'ftp://x', quote: 'ACME GmbH' },
        { field_key: 'wz_code', source_id: 'nd', url: 'https://northdata.de/a', quote: '1' },
        { source_id: 'nd' },
      ],
      person_records: [{ person_key: 'p1' }, { person_key: ' ' }],
    },
  });
  const requested = ['firma_name', 'firma_ort', 'mitarbeiter', 'umsatz', 'firma_plz'];
  const rejections = sanitizeResearchWriteback(r, requested, noCrm);
  assert.deepEqual(rejections, [
    'result.fields.wz_code: nicht angefordert',
    'result.fields.firma_ort: Wert ohne verifizierten Status verworfen (no_match)',
    'result.person_records: 1 Eintraege ohne person_key verworfen',
    'result.evidence[1]: Beleg-URL ist keine http(s)-Adresse',
    'result.evidence[2]: Feld wz_code war nicht angefordert',
    'result.evidence[3]: field_key fehlt',
    'field_status.mitarbeiter: 1 Beleg(e) verworfen (Zitat nennt den Wert nicht, nur eine Spanne oder eine andere Zahl)',
    'field_status.mitarbeiter: verified verlangt 1 unabhaengige Quell-Hosts, gefunden: 0',
    'field_status.umsatz: Beleg vom Dokumentations-Host `example.com` beweist nichts',
    'field_status.firma_plz: nicht geliefert',
  ]);
  assert.equal(r.field_status.firma_name.status, 'verified');
  assert.equal(r.field_status.umsatz.status, 'unsupported');
  assert.equal(r.field_status.umsatz.reason, 'Vom Rueckschreiben verworfen, siehe rejections');
  assert.deepEqual(Object.keys(r.field_status), ['firma_name', 'firma_ort', 'firma_plz', 'mitarbeiter', 'umsatz']);
  assert.equal(r.result.fields.firma_ort.value, null);
  assert.equal(r.result.evidence.length, 1);
});

test('sanitize: Sellify alone proves nothing, agreeing CRM record is added as source', () => {
  const crm = CrmBaseline.fromResearchPayload({ sellify_company: { contact_id: 77, telefon: '+49 40 123456' } });
  const r = req({ field_status: {
    firma_telefon: { status: 'verified', value: '+49 40 123456', sources: [{ source_id: 'sellify', url: 'sellify://company/77', quote: '+49 40 123456' }] },
    firma_fax: { status: 'verified', value: '+49 40 99', sources: [{ source_id: 'sellify', url: 'sellify://company/1', quote: '+49 40 99' }] },
  } });
  const rejections = sanitizeResearchWriteback(r, ['firma_fax', 'firma_telefon'], crm);
  assert.match(rejections[0], /^field_status.firma_fax: 1 Sellify-Beleg\(e\) verworfen \(gueltig sind nur sellify:\/\/company\/77/);
  assert.equal(rejections[1], 'field_status.firma_fax: verified verlangt 1 unabhaengige Quell-Hosts, gefunden: 0');
  assert.equal(rejections[2], 'field_status.firma_telefon: Sellify allein belegt nichts: eine externe Quelle muss den Wert bestaetigen');
});

test('swap_remove ordering and helpers', () => {
  const o = { a: 1, b: 2, c: 3, d: 4 };
  swapRemoveKey(o, 'b');
  assert.deepEqual(Object.keys(o), ['a', 'd', 'c']);
  assert.equal(safeWorkspaceSegment('--a b/ç--'), 'a-b');
  assert.equal(safeWorkspaceSegment('///'), 'research');
  assert.equal(taskWorkspace({ workspace_root_rel: 'runtime/research/person/cmd-1' }, { research_command_id: 'cmd 1' }), 'runtime/research/person/cmd-1');
  assert.throws(() => taskWorkspace({ workspace_root_rel: 'x' }, { research_command_id: 'c' }), /does not match the Phase-A workspace/);
  assert.throws(() => validateOriginalResearchCommand(null, base), /research_command_id `c` does not exist/);
  assert.throws(() => validateOriginalResearchCommand({ command_type: 'web_stack.person_research', module: 'x', record_id: 'l' }, base), /same module and record_id/);
  assert.throws(() => validateTaskCorrelation({ record_id: 'l', research_command_id: 'c', module: base.module, gap_task_id: 'g' }, { ...base, gap_task_id: '' }), /gap task id does not match/);
  assert.equal(fieldDefinition('umsatz'), 'aktuellster belegbarer Umsatz mit Zeitraum und Einheit');
  assert.throws(() => canonicalRequestedFields({ payload: { fields: ['nope'] } }, {}), /unsupported person-research field `nope`/);
  assert.equal(canonicalRequestedFields({ payload: {} }, {}).length, 33);
});

test('terminal field rules and artifact paths over cap.agent.task.workspace.read (§16.30; ref :3096, :3514)', async () => {
  // Kern: workspace.read kanonisiert, verweigert Ausbrüche und Verzeichnisse (Fehler), liefert {text, sha256}.
  const files = {
    'gap_closure/attempts/umsatz/1.json': { text: '{"ok":true}', sha256: 'a' },
    'gap_closure/attempts/umsatz/2.json': { text: '', sha256: 'e' },
  };
  const reads = [];
  const { ctx } = createTestCtx({ caps: { 'agent.task.workspace.read': async ({ task_id, path }) => {
    reads.push([task_id, path]);
    if (!files[path]) throw new Error('No such file or directory');
    return files[path];
  } } });
  const attempt = (n, kind = 'web_read') => ({ kind, query_or_url: 'q', result: null, artifact_path: `gap_closure/attempts/umsatz/${n}.json`, at: 1 });
  const status = (s, extra) => ({ status: s, value: null, sources: [], attempts: [], reason: '', extra: {}, ...extra });
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('pending'), {}), /non-terminal or unsupported status `pending`/);
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('unsupported', { attempts: [attempt(0)] }), {}), /positive numeric `<n>.json`/);
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('unsupported', { attempts: [{ ...attempt(1), artifact_path: '/abs/1.json' }] }), {}), /must be under `gap_closure\/attempts\/umsatz\/`/);
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('unsupported', { attempts: [attempt(2)] }), {}), /existing non-empty file/);
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('unsupported', { attempts: [attempt(3)] }), {}), /artifact_path `gap_closure\/attempts\/umsatz\/3.json` does not exist/);
  await validateTerminalField(ctx, 't', 'umsatz', status('unsupported', { attempts: [attempt(1)] }), {});
  // nicht direkt im Versuchsverzeichnis (Unterordner, `..`) → abgewiesen, bevor der Kern gefragt wird
  const before = reads.length;
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('unsupported', { attempts: [{ ...attempt(1), artifact_path: 'gap_closure/attempts/umsatz/sub/1.json' }] }), {}), /must remain directly under/);
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('unsupported', { attempts: [{ ...attempt(1), artifact_path: 'gap_closure/attempts/umsatz/../name/1.json' }] }), {}), /must remain directly under/);
  assert.equal(reads.length, before);
  assert.deepEqual(reads[0], ['t', 'gap_closure/attempts/umsatz/2.json']);
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('verified', { value: '5', sources: [] }), {}), /requires at least 1 independent sources/);
  await assert.rejects(validateTerminalField(ctx, 't', 'umsatz', status('action_required'), {}), /requires an auth-assist reference/);
  const src = { source_id: 'xing', url: 'https://xing.com', quote: 'q', person_key: null, requires_credential: false, task_id: '', command_id: '' };
  assert.equal(actionRequiredHasAuthReference(status('action_required', { sources: [src] }), { attempted_sources: [{ source_id: 'xing', requires_credential: true }] }), true);
  assert.equal(actionRequiredHasAuthReference(status('action_required', { attempts: [{ result: { auth_assist: { source_id: 'x', command_id: 'c' } } }] }), {}), true);
  assert.equal(actionRequiredHasAuthReference(status('action_required', { sources: [src] }), {}), false);
});

test('projection helpers: field-status evidence, phase-A sources, revision stamp', () => {
  const result = { fields: { umsatz: { person_key: 'x' } } };
  addFieldStatusEvidence(result, { umsatz: { status: 'verified', value: '5', sources: [{ source_id: 's', url: 'u', quote: 'q', person_key: ' ' }] } });
  assert.deepEqual(result.fields.umsatz, { person_key: 'x', value: '5', candidates: [{ value: '5', source_id: 's', source_url: 'u', quote: 'q', person_key: 'x', via: 'gap_closure' }] });
  assert.deepEqual(phaseAVerifiedSources([{ field: 'umsatz', url: ' https://a.de ', source_id: 'a', value: 5 }, { field_key: 'umsatz', source_url: 'x', source_id: 'a', quote: 'q' }], 'umsatz'),
    [{ field_key: 'umsatz', source_id: 'a', source_url: 'https://a.de', url: 'https://a.de', quote: '5', person_key: null }]);
  assert.deepEqual(phaseAAttemptedSources({ search_runs: [{ q: 1 }, 'raw'], plan: [{ attempt_kind: 'x' }] }), [{ attempt_kind: 'x' }, { q: 1, attempt_kind: 'web_search' }, 'raw']);
  const r = req({ field_status: { umsatz: { status: 'verified', zeta: 1 } }, result: { person_field_status: { p: { person_email: { status: 'verified' } } } } });
  stampWritebackFieldRevisions(r, { at_ms: 1 });
  assert.deepEqual(Object.keys(r.field_status.umsatz.extra), ['review', 'revision', 'zeta']);
  assert.deepEqual(r.result.person_field_status.p.person_email, { status: 'verified', review: null, revision: { at_ms: 1 } });
});
