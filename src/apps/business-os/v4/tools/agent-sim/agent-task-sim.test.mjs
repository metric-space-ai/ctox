// node --test src/apps/business-os/v4/tools/agent-sim/agent-task-sim.test.mjs  (Mutationsbeleg: mutation-proof.mjs)
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createAgentTaskSim, foreignTriggerAllowed, normalizeAssetPath, ASSET_MAX_BYTES } from './agent-task-sim.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PROMPT_TEMPLATE = pathToFileURL(path.resolve(HERE, '../../../shared/prompt-template.js')).href;

// Beispiel-App „demo“ im Zielbild: Knopf → demo.run (Template + enqueue) → demo.run.complete/.failed schreibt Collection.
function demoApp() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-sim-'));
  const put = (rel, text) => { fs.mkdirSync(path.dirname(path.join(root, rel)), { recursive: true }); fs.writeFileSync(path.join(root, rel), text); };
  put('module.json', JSON.stringify({ id: 'demo' }));
  put('skills/summarize/SKILL.md', '---\nname: summarize\n---\nFasse zusammen.\n');
  put('skills/summarize/references/style.md', 'kurz\n');
  put('prompts/summary.md', 'Fasse {{topic}} zusammen ({{limit}} Sätze).\n');
  put('handlers/demo.run.js', `import { renderPrompt } from '${PROMPT_TEMPLATE}';
export default async function handle(command, ctx) {
  const instruction = await renderPrompt(ctx, 'summary', { topic: command.payload.topic, limit: 3 });
  const { task_id } = await ctx.cap.agent.task.enqueue({
    title: 'Zusammenfassung', instruction, skill: 'demo/summarize', thread_key: 'demo/' + command.payload.topic,
    payload: { topic: command.payload.topic }, idempotency_key: 'demo-run:' + command.id,
    on_complete: 'demo.run.complete', on_failed: 'demo.run.failed',
  });
  await ctx.write('demo_runs', { id: command.id, task_id, state: 'queued' });
  return { task_id };
}
`);
  put('handlers/demo.run.complete.js', `export default async function handle(command, ctx) {
  await ctx.write('demo_runs', { id: command.payload.task_id, state: command.payload.status, text: command.payload.result?.text ?? null });
  return { ok: true };
}
`);
  put('handlers/demo.run.failed.js', `export default async function handle(command, ctx) {
  await ctx.write('demo_runs', { id: command.payload.task_id, state: command.payload.status, error: command.payload.error.message });
  return { ok: true };
}
`);
  put('handlers/tests/demo.test.mjs', '// Test, nicht im Artefakt\n');
  return root;
}

function recorderCtx() {
  const writes = [];
  return { writes, write: async (collection, doc) => { writes.push({ collection, doc }); } };
}

async function runButton(sim, appDir, payload, id = 'cmd_1') {
  const command = { id, type: 'demo.run', payload };
  const ctx = sim.attach(recorderCtx(), command);
  const handler = (await import(pathToFileURL(path.join(appDir, 'handlers/demo.run.js')).href)).default;
  return { result: await handler(command, ctx), ctx };
}

test('Ende-zu-Ende: Knopf → Template → enqueue mit Skill → complete stellt genau einen Callback zu', async () => {
  const dir = demoApp();
  try {
    const sinks = [];
    const sim = createAgentTaskSim({ appId: 'demo', appDir: dir, coreSkills: [], ctxFactory: () => { const c = recorderCtx(); sinks.push(c); return c; } });
    const { result, ctx } = await runButton(sim, dir, { topic: 'KI' });
    assert.equal(sim.enqueued.length, 1);
    const e = sim.enqueued[0];
    assert.equal(e.args.instruction, 'Fasse KI zusammen (3 Sätze).\n');
    assert.deepEqual(e.skill, { skill: 'demo/summarize', scope: 'app', path: 'skills/summarize/SKILL.md' });
    assert.equal(e.command_id, 'cmd_1');
    assert.equal(result.task_id, e.task_id);
    assert.deepEqual(ctx.writes, [{ collection: 'demo_runs', doc: { id: 'cmd_1', task_id: e.task_id, state: 'queued' } }]);

    const d1 = await sim.complete(e.task_id, { text: 'kurz' });
    assert.equal(d1.delivered, true);
    assert.deepEqual(d1.command, {
      id: `cmd_callback_${e.task_id}`, type: 'demo.run.complete', app_id: 'demo', module: 'demo', record_id: null,
      payload: { task_id: e.task_id, status: 'completed', result: { text: 'kurz' } },
    });
    assert.deepEqual(sinks[0].writes, [{ collection: 'demo_runs', doc: { id: e.task_id, state: 'completed', text: 'kurz' } }]);
    assert.equal(typeof sinks[0].cap.agent.task.get, 'function', 'Callback-ctx trägt agent.task');

    // zweite Zustellung (complete oder fail): verworfen und protokolliert
    assert.deepEqual(await sim.complete(e.task_id, { text: 'nochmal' }), { delivered: false, reason: 'duplicate' });
    assert.deepEqual(await sim.fail(e.task_id, 'spät'), { delivered: false, reason: 'duplicate' });
    assert.equal(sim.deliveries.length, 1);
    assert.equal(sinks.length, 1);
    assert.deepEqual(sim.log.filter((l) => l.event === 'callback_dropped').map((l) => l.outcome), ['completed', 'failed']);
    assert.equal((await sim.api().get({ task_id: e.task_id })).status, 'completed');
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('fail: on_failed-Befehl mit error und workspace, Routing nur an den on_failed-Handler', async () => {
  const calls = [];
  const handlers = {
    'x.done': (cmd) => { calls.push(['done', cmd.payload]); return 'ok'; },
    'x.failed': (cmd) => { calls.push(['failed', cmd.payload]); return 'ok'; },
  };
  const sim = createAgentTaskSim({ appId: 'x', handlers, files: {}, coreSkills: ['systematic-research'] });
  const { task_id } = await sim.api().enqueue({ title: 't', instruction: 'i', skill: 'systematic-research', payload: {}, on_complete: 'x.done', on_failed: 'x.failed' });
  const d = await sim.fail(task_id, { code: 'timeout', message: 'zu lang' }, { workspace: { './out/report.md': '# R', 'a.json': '{}' } });
  assert.equal(d.delivered, true);
  assert.equal(d.result, 'ok');
  assert.deepEqual(calls, [['failed', { task_id, status: 'failed', result: null, error: { code: 'timeout', message: 'zu lang' }, workspace_refs: ['a.json', 'out/report.md'] }]]);
  assert.equal(await sim.api().workspace.read({ task_id, path: 'out/report.md' }), '# R');
  await assert.rejects(sim.api().workspace.read({ task_id, path: 'nope.md' }), { code: 'not_found' });
  await assert.rejects(sim.api().workspace.read({ task_id, path: '../a.json' }), { code: 'invalid' });
  // String-Fehler wird zu {code:'failed', message}
  const t2 = await sim.api().enqueue({ title: 't', instruction: 'i', payload: {}, on_failed: 'x.failed' });
  await sim.fail(t2.task_id, 'kaputt');
  assert.deepEqual(calls[1][1].error, { code: 'failed', message: 'kaputt' });
  assert.equal(calls.length, 2);
});

test('Callback-Handler wirft: einmal zugestellt, Fehler sichtbar, keine zweite Zustellung', async () => {
  const handlers = { 'x.done': () => { throw Object.assign(new Error('nein'), { code: 'conflict' }); } };
  const sim = createAgentTaskSim({ appId: 'x', handlers, files: {}, coreSkills: [] });
  const { task_id } = await sim.api().enqueue({ title: 't', instruction: 'i', payload: {}, on_complete: 'x.done' });
  const d = await sim.complete(task_id, {});
  assert.equal(d.delivered, true);
  assert.equal(d.error.code, 'conflict');
  assert.ok(sim.log.some((l) => l.event === 'callback_handler_error' && l.code === 'conflict'));
  assert.equal((await sim.complete(task_id, {})).reason, 'duplicate');
});

test('ohne on_complete: kein Befehl, Task abgeschlossen; abgebrochener Task wird nicht mehr zugestellt', async () => {
  const handlers = { 'x.done': () => assert.fail('darf nicht laufen') };
  const sim = createAgentTaskSim({ appId: 'x', handlers, files: {}, coreSkills: [] });
  const a = await sim.api().enqueue({ title: 't', instruction: 'i', payload: {}, on_failed: 'x.done' });
  assert.deepEqual(await sim.complete(a.task_id, {}), { delivered: false, reason: 'no_callback' });
  assert.equal(sim.deliveries.length, 0);
  const b = await sim.api().enqueue({ title: 't', instruction: 'i', payload: {}, on_complete: 'x.done' });
  await sim.api().cancel({ task_id: b.task_id, status_note: 'Nutzer' });
  assert.deepEqual(await sim.complete(b.task_id, {}), { delivered: false, reason: 'terminal' });
  await assert.rejects(sim.api().cancel({ task_id: b.task_id }), { code: 'conflict' });
  await assert.rejects(sim.complete('task_unbekannt', {}), { code: 'not_found' });
});

test('Skill-Auflösung: deklariert + Datei vorhanden, sonst invalid; fremder Namensraum invalid; System-Skills', async () => {
  const files = { 'skills/a/SKILL.md': '# a', 'skills/b/SKILL.md': '# b' };
  const manifest = { skills: [{ name: 'a', path: 'skills/a/SKILL.md' }, { name: 'ghost', path: 'skills/ghost/SKILL.md' }] };
  const sim = createAgentTaskSim({ appId: 'app', manifest, files, handlers: {}, coreSkills: ['universal-scraping'] });
  const base = { title: 't', instruction: 'i', payload: {} };
  const ok = await sim.api().enqueue({ ...base, skill: 'app/a' });
  assert.ok(ok.task_id);
  for (const [skill, re] of [
    ['app/b', /nicht im Manifest/], // Datei da, aber nicht deklariert
    ['app/ghost', /fehlt im Paket/], // deklariert, Datei fehlt
    ['other/a', /fremder Skill-Namensraum/],
    ['app/a/x', /Form/],
    ['knowledge', /existiert nicht im Core/],
    ['', /skill muss/],
  ]) {
    await assert.rejects(sim.api().enqueue({ ...base, skill }), (err) => err.code === 'invalid' && re.test(err.message), skill);
  }
  assert.ok((await sim.api().enqueue({ ...base, skill: 'universal-scraping' })).task_id);
  assert.equal(sim.enqueued.length, 2, 'abgelehnte Aufrufe werden nicht aufgezeichnet');
  // Standard: System-Skills aus src/skills des Repos; Phantome bleiben invalid
  const real = createAgentTaskSim({ appId: 'app', files: {}, handlers: {} });
  assert.ok((await real.api().enqueue({ ...base, skill: 'systematic-research' })).task_id);
  await assert.rejects(real.api().enqueue({ ...base, skill: 'knowledge' }), { code: 'invalid' });
  await assert.rejects(real.api().enqueue({ ...base, skill: 'business-os-support-workflow' }), { code: 'invalid' });
  // ohne Manifest: aus dem Paket (files bzw. appDir) abgeleitet
  const derived = createAgentTaskSim({ appId: 'app', files, handlers: {}, coreSkills: [] });
  assert.ok((await derived.api().enqueue({ ...base, skill: 'app/b' })).task_id);
});

test('enqueue: strenge Argumente nach §16.30', async () => {
  const sim = createAgentTaskSim({ appId: 'x', files: {}, handlers: { 'x.done': () => {} }, coreSkills: [] });
  const base = { title: 't', instruction: 'i', payload: {} };
  const bad = [
    [null, /Objekt/],
    [{ ...base, prompt: 'p' }, /unbekannte Felder prompt/],
    [{ ...base, suggested_skill: 'x' }, /unbekannte Felder/],
    [{ ...base, reply_to: {} }, /unbekannte Felder/],
    [{ ...base, title: '' }, /title/],
    [{ ...base, instruction: '  ' }, /instruction/],
    [{ ...base, payload: [] }, /payload/],
    [{ title: 't', instruction: 'i' }, /payload/],
    [{ ...base, priority: 3 }, /priority/],
    [{ ...base, on_complete: 'x.unbekannt' }, /kein Befehlstyp/],
    [{ ...base, on_failed: '' }, /on_failed/],
  ];
  for (const [args, re] of bad) await assert.rejects(sim.api().enqueue(args), (err) => err.code === 'invalid' && re.test(err.message), JSON.stringify(args));
  assert.equal(sim.enqueued.length, 0);
});

test('idempotency_key, find, get, update, set_metadata', async () => {
  const sim = createAgentTaskSim({ appId: 'x', files: {}, handlers: {}, coreSkills: [] });
  const task = sim.api({ id: 'cmd_9' });
  const a = await task.enqueue({ title: 't', instruction: 'i', payload: { n: 1 }, idempotency_key: 'k1' });
  const b = await task.enqueue({ title: 'anders', instruction: 'i', payload: {}, idempotency_key: 'k1' });
  assert.equal(a.task_id, b.task_id);
  assert.equal(sim.enqueued.length, 1);
  assert.ok(sim.log.some((l) => l.event === 'idempotent_hit'));
  assert.equal((await task.find({ idempotency_key: 'k1' })).task_id, a.task_id);
  assert.equal((await task.find({ command_id: 'cmd_9' })).task_id, a.task_id);
  assert.equal(await task.find({ command_id: 'cmd_x' }), null);
  await assert.rejects(task.find({}), { code: 'invalid' });
  await assert.rejects(task.find({ idempotency_key: 'k1', command_id: 'cmd_9' }), { code: 'invalid' });
  const got = await task.get({ task_id: a.task_id });
  got.payload.n = 99;
  assert.equal((await task.get({ task_id: a.task_id })).payload.n, 1, 'get liefert eine Kopie');
  assert.equal((await task.update({ task_id: a.task_id, title: 'neu' })).title, 'neu');
  await assert.rejects(task.update({ task_id: a.task_id, status: 'completed' }), { code: 'invalid' });
  await task.set_metadata({ task_id: a.task_id, key: 'phase', value: { n: 2 } });
  assert.deepEqual((await task.get({ task_id: a.task_id })).metadata, { phase: { n: 2 } });
  await assert.rejects(task.set_metadata({ task_id: a.task_id, key: '' }), { code: 'invalid' });
  sim.markRunning(a.task_id);
  await assert.rejects(task.update({ task_id: a.task_id, title: 'zu spät' }), { code: 'conflict' });
  await assert.rejects(task.get({ task_id: 'nope' }), { code: 'not_found' });
});

test('ctx.asset: Paketdateien, Normalisierung, keine Testdateien, ≤ 1 MiB, UTF-8, kein Ausbruch per Symlink', async () => {
  const dir = demoApp();
  const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-sim-out-'));
  try {
    fs.writeFileSync(path.join(outside, 'secret.md'), 'geheim');
    fs.symlinkSync(path.join(outside, 'secret.md'), path.join(dir, 'prompts/link.md'));
    fs.writeFileSync(path.join(dir, 'prompts/big.md'), 'x'.repeat(ASSET_MAX_BYTES + 1));
    fs.writeFileSync(path.join(dir, 'prompts/max.md'), 'x'.repeat(ASSET_MAX_BYTES));
    fs.writeFileSync(path.join(dir, 'prompts/bin.md'), Buffer.from([0xff, 0xfe, 0x00]));
    const sim = createAgentTaskSim({ appId: 'demo', appDir: dir, coreSkills: [] });
    assert.equal(sim.asset('prompts/summary.md'), 'Fasse {{topic}} zusammen ({{limit}} Sätze).\n');
    assert.equal(sim.asset('./skills/summarize/references/style.md'), 'kurz\n');
    assert.equal(sim.asset('prompts/max.md').length, ASSET_MAX_BYTES);
    for (const p of ['/etc/passwd', '../x', 'prompts/../module.json', 'prompts//a.md', 'a\\b', '', 'C:/x']) {
      assert.throws(() => sim.asset(p), { code: 'invalid' }, p);
    }
    assert.throws(() => sim.asset('handlers/tests/demo.test.mjs'), { code: 'not_found' });
    assert.throws(() => sim.asset('prompts/fehlt.md'), { code: 'not_found' });
    assert.throws(() => sim.asset('prompts/link.md'), { code: 'not_found' });
    assert.throws(() => sim.asset('prompts'), { code: 'not_found' });
    assert.throws(() => sim.asset('prompts/big.md'), (err) => err.code === 'invalid' && /1 MiB/.test(err.message));
    assert.throws(() => sim.asset('prompts/bin.md'), (err) => err.code === 'invalid' && /UTF-8/.test(err.message));
    const mem = createAgentTaskSim({ appId: 'm', files: { 'prompts/a.md': 'A' }, coreSkills: [] });
    assert.equal(mem.asset('prompts/a.md'), 'A');
    assert.equal(normalizeAssetPath('./././prompts/a.md'), 'prompts/a.md');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
    fs.rmSync(outside, { recursive: true, force: true });
  }
});

test('foreignTriggerAllowed: eigene Collection, fremde nur mit watch-Export und watch-Deklaration', () => {
  const owner = { app_id: 'mail', collections: { communication_messages: {} }, exports: { communication_messages: ['read', 'watch'], drafts: ['read'] } };
  const app = { app_id: 'kp', collections: { kp_items: {} }, foreign_collections: {
    communication_messages: { owner_app: 'mail', ops: ['read', 'watch'] },
    drafts: { owner_app: 'mail', ops: ['read', 'watch'] },
    other: { owner_app: 'mail', ops: ['read'] },
    wrong: { owner_app: 'crm', ops: ['read', 'watch'] },
  } };
  const check = (collection) => foreignTriggerAllowed({ appManifest: app, ownerManifest: owner, collection });
  assert.deepEqual(check('kp_items'), { ok: true, reason: 'own' });
  assert.deepEqual(check('communication_messages'), { ok: true, reason: 'foreign_watch' });
  assert.deepEqual(check('drafts'), { ok: false, reason: 'watch_not_exported' });
  assert.deepEqual(check('other'), { ok: false, reason: 'watch_not_declared' });
  assert.deepEqual(check('wrong'), { ok: false, reason: 'owner_mismatch' });
  assert.deepEqual(check('undeclared'), { ok: false, reason: 'not_declared' });
});
