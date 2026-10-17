// Strang THR2 (Kern §16.40, §16.39 Nr. 3) + LOOP Runde 2 (§16.44): einziges Ziel einer Freigabe ist die Agentenarbeit;
// Rückrufe an anfragende Apps nur aus der festen Liste, die seit FIX10 leer ist (reports.delegation.* entfernt).
// - anderes Ziel (auch ctox.coding.turn) / nicht gelisteter Rückruf → invalid schon bei threads.ctox_approval.request
// - Altbestand mit anderem Ziel (auch ctox.coding.turn von vor LOOP Runde 2) → approve lehnt ab, nichts gesendet
// - Rückruf mit command_id (in Arbeit befindlicher Altbestand) bleibt zuordenbar; unbekannte command_id → not_found
// - commands_out, die gen-manifest.mjs aus dem Code ableitet, = Rückruf-Liste = leer (FIX10)
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeCtx, cmd, USERS, rejects } from './_ctx.mjs';
import request from '../threads.ctox_approval.request.js';
import approve from '../threads.ctox_approval.approve.js';
import complete from '../threads.ctox_approval.complete.js';
import failed from '../threads.ctox_approval.failed.js';
import { APPROVAL_CALLBACKS, dispatchApprovalCallback, ensureAllowedApprovalTarget } from '../_approval_targets.js';
import { collectConstants, listHandlers, scanCommandCalls, stripLeadingComments } from '../../../../v4/tools/manifest/gen-manifest.mjs';

const JUNIOR = { id: 'junior', role: 'user', display_name: 'Junior' };
const LEAD = { id: 'lead', role: 'chef', display_name: 'Lead' };
const users = { ...USERS, junior: { display_name: 'Junior' }, lead: { display_name: 'Lead' } };
const THREADS_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

// Freigabe einer Agentenarbeit (ohne Rückruf; FIX10).
function delegation(extra = {}) {
  return cmd('threads.ctox_approval.request', {
    approval_request_id: 'approval-t1',
    prompt: 'Bitte die gemeldeten Fälle zusammenfassen und priorisieren.',
    reviewer_user_id: 'lead',
    target_module: 'notes',
    target_record_id: 'rep-1',
    target_command_type: 'business_os.chat.task',
    target_payload: {},
    source_context: { module: 'reports', record_type: 'bug_report', record_id: 'rep-1', label: 'Liste springt' },
    ...extra,
  }, JUNIOR, { record_id: 'rep-1' });
}

async function approveIt(h, id = 'approval-t1') {
  const v = h.doc('ctox_task_approval_requests', id).updated_at_ms;
  return approve(cmd('threads.ctox_approval.approve', { approval_request_id: id, expected_updated_at_ms: v }, LEAD), h.ctx);
}

const callbackCmd = (type, payload) => ({ id: `cb_${type}_${payload.status}`, type, app_id: 'threads', module: 'threads', payload });
const REPORTS_CALLBACK = { on_complete: 'reports.delegation.complete', on_failed: 'reports.delegation.failed' };

test('request: nicht gelistetes Ziel → invalid, nichts geschrieben, nichts gesendet', async () => {
  for (const extra of [
    { target_command_type: 'support.case.reply' },
    { target_command_type: undefined, command_type: 'notes.note.delete' },
    { target_command_type: 'ctox.business_os.app.modify' },
    { target_command_type: 'ctox.coding.turn', target_payload: { module_id: 'notes', prompt: 'x' } },
  ]) {
    const h = makeCtx({ users });
    await rejects(request(delegation(extra), h.ctx), 'invalid', 'is not an approval target of threads');
    assert.equal(h.calls.writes.length, 0);
    assert.equal(h.calls.commands.length, 0);
    assert.equal(h.doc('ctox_task_approval_requests', 'approval-t1') ?? null, null);
  }
  // gelistete Ziele gehen durch (Agentenarbeit als Vorgabe ohne Angabe)
  const ok = makeCtx({ users });
  assert.equal((await request(delegation(), ok.ctx)).status, 'pending');
  const ok2 = makeCtx({ users });
  assert.equal((await request(delegation({ target_command_type: undefined, callback: undefined }), ok2.ctx)).status, 'pending');
  assert.equal(ok2.doc('ctox_task_approval_requests', 'approval-t1').target_command_type, 'business_os.chat.task');
});

test('request: Rückruf außerhalb der festen Liste → invalid, nichts geschrieben', async () => {
  const h = makeCtx({ users });
  // FIX10: auch der frühere Rückruf an reports ist nicht mehr gelistet.
  await rejects(request(delegation({ callback: REPORTS_CALLBACK }), h.ctx), 'invalid', 'not a declared approval callback');
  await rejects(request(delegation({ callback: { on_complete: 'reports.other.complete', on_failed: 'reports.delegation.failed' } }), h.ctx),
    'invalid', 'not a declared approval callback');
  await rejects(request(delegation({
    source_context: { module: 'notes', record_id: 'n1' },
    callback: { on_complete: 'notes.delegation.complete', on_failed: 'notes.delegation.failed' },
  }), h.ctx), 'invalid', 'not a declared approval callback');
  assert.equal(h.calls.writes.length, 0);
});

test('approve: Altbestand mit nicht gelistetem Ziel → invalid, nichts gesendet, bleibt pending', async () => {
  const h = makeCtx({ users });
  await request(delegation(), h.ctx);
  const a = h.doc('ctox_task_approval_requests', 'approval-t1');
  await h.ctx.put('ctox_task_approval_requests', { ...a, target_command_type: 'support.case.reply' });
  const writes = h.calls.writes.length;
  await rejects(approveIt(h), 'invalid', 'is not an approval target of threads');
  assert.equal(h.calls.commands.length, 0);
  assert.equal(h.sim.enqueued.length, 0);
  assert.equal(h.calls.writes.length, writes);
  assert.equal(h.doc('ctox_task_approval_requests', 'approval-t1').status, 'pending');
});

test('LOOP Runde 2: ctox.coding.turn ist kein Freigabeziel mehr – Anfrage invalid mit Verweis auf die Code-Ansicht; Altbestand bei approve invalid, nichts gesendet', async () => {
  const h = makeCtx({ users });
  await rejects(request(delegation({ target_command_type: 'ctox.coding.turn' }), h.ctx), 'invalid', "coding turns are started only from the app's code view");
  assert.equal(h.calls.writes.length, 0);
  assert.throws(() => ensureAllowedApprovalTarget('ctox.coding.turn'), (e) => e.code === 'invalid');
  // Altbestand: vor LOOP Runde 2 angelegte Freigabe eines Coding-Turns
  await request(delegation(), h.ctx);
  const a = h.doc('ctox_task_approval_requests', 'approval-t1');
  await h.ctx.put('ctox_task_approval_requests', { ...a, target_command_type: 'ctox.coding.turn', target_payload: { module_id: 'notes', prompt: 'x' } });
  const writes = h.calls.writes.length;
  await rejects(approveIt(h), 'invalid', "coding turns are started only from the app's code view");
  assert.equal(h.calls.commands.length, 0);
  assert.equal(h.sim.enqueued.length, 0);
  assert.equal(h.calls.writes.length, writes);
  assert.equal(h.doc('ctox_task_approval_requests', 'approval-t1').status, 'pending');
});

test('in Arbeit befindlicher Altbestand: Rückruf per command_id schließt genau einmal ab, nichts weitergeleitet', async () => {
  const h = makeCtx({ users });
  const r = await (async () => { await request(delegation(), h.ctx); return approveIt(h); })();
  // Als wäre die Freigabe vor LOOP Runde 2 als Coding-Turn ausgeführt worden: Rückruf des Kerns trägt command_id.
  const payload = { command_id: r.approved_command_id, status: 'completed', result: { text: 'Turn fertig' } };
  const out = await complete(callbackCmd('threads.ctox_approval.complete', payload), h.ctx);
  assert.equal(out.status, 'completed');
  assert.equal(h.doc('ctox_task_approval_requests', 'approval-t1').execution_status, 'completed');
  assert.equal(h.calls.commands.length, 0);
  const writes = h.calls.writes.length;
  assert.equal((await failed(callbackCmd('threads.ctox_approval.failed', { command_id: r.approved_command_id, status: 'failed', error: 'spät' }), h.ctx)).status, 'ignored');
  assert.equal(h.calls.writes.length, writes);
  assert.equal(h.calls.commands.length, 0);
});

test('Rückruf ohne task_id und command_id → invalid; unbekannte command_id → not_found', async () => {
  const h = makeCtx({ users });
  await rejects(complete(callbackCmd('threads.ctox_approval.complete', { status: 'completed' }), h.ctx), 'invalid', 'task_id or command_id is required');
  await rejects(complete(callbackCmd('threads.ctox_approval.complete', { command_id: 'cmd_x', status: 'completed' }), h.ctx), 'not_found', 'command cmd_x');
});

test('Altbestand mit nicht gelistetem Rückruf: Abschluss wird geschrieben, nichts weitergeleitet', async () => {
  for (const cb of [{ app_id: 'notes', on_complete: 'notes.x.complete', on_failed: 'notes.x.failed' }, { app_id: 'reports', ...REPORTS_CALLBACK }]) {
    const h = makeCtx({ users });
    await request(delegation(), h.ctx);
    const a = h.doc('ctox_task_approval_requests', 'approval-t1');
    await h.ctx.put('ctox_task_approval_requests', { ...a, callback: cb });
    const r = await approveIt(h);
    const out = await complete(callbackCmd('threads.ctox_approval.complete', { task_id: r.approved_task_id, status: 'completed', result: 'ok' }), h.ctx);
    assert.equal(out.status, 'completed');
    assert.equal(out.forwarded.accepted, null, cb.app_id);
    assert.equal(h.calls.commands.length, 0, `${cb.app_id}: nichts gesendet`);
    assert.ok(h.calls.logs.some((l) => l.level === 'warn' && /not declared/.test(l.msg)));
  }
});

test('FIX10: Rückruf-Liste leer, kein Versand – auch nicht an reports.delegation.*', async () => {
  assert.deepEqual(APPROVAL_CALLBACKS, []);
  for (const type of ['notes.x.complete', REPORTS_CALLBACK.on_complete, REPORTS_CALLBACK.on_failed]) {
    const h = makeCtx({});
    assert.equal(await dispatchApprovalCallback(h.ctx, type, {}, { id: 'c2', module: 'reports' }), null, type);
    assert.equal(h.calls.commands.length, 0, type);
  }
});

test('gen-manifest leitet commands_out von threads genau aus der Rückruf-Liste ab (leer: kein ctox.coding.turn, kein reports.*, nichts unaufgelöst)', () => {
  const files = fs.readdirSync(path.join(THREADS_DIR, 'handlers')).filter((f) => f.endsWith('.js'));
  const consts = collectConstants(files.map((f) => stripLeadingComments(fs.readFileSync(path.join(THREADS_DIR, 'handlers', f), 'utf8'))));
  const { out, unresolved } = scanCommandCalls(THREADS_DIR, 'threads', listHandlers(THREADS_DIR), consts);
  assert.deepEqual(unresolved, []);
  const derived = [...new Set(out.map((o) => `${o.app} ${o.type}`))].sort();
  assert.ok(!derived.some((d) => d.endsWith(' ctox.coding.turn') || d.startsWith('reports ')), derived.join(', '));
  const expected = [
    ...APPROVAL_CALLBACKS.flatMap((c) => [`${c.app} ${c.on_complete}`, `${c.app} ${c.on_failed}`]),
  ].sort();
  assert.deepEqual(derived, expected);
});
