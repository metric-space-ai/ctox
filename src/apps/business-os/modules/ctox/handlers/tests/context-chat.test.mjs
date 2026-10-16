// SKL-D: Standardbefehle ctox.chat.task / ctox.app.modify.request → enqueue (Template + Skill) → Rückruf →
// Antwort in business_chats, gegen die agent-task-Simulation (v4/tools/agent-sim).
// Aus src/apps/business-os: node --import ./v4/tools/node/ctox-shared.mjs --test modules/ctox/handlers/tests/context-chat.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';
import { buildContextChatCommand } from 'ctox:shared/context-chat.js';
import chatTask from '../ctox.chat.task.js';
import appModify from '../ctox.app.modify.request.js';

const APP_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const NOW = 1_760_000_000_000;

function world(seed = {}) {
  const { ctx, state } = makeCtx({ now: NOW, collections: seed });
  ctx.cap = {}; // die Simulation setzt ctx.cap.agent.task
  const sim = createAgentTaskSim({ appId: 'ctox', appDir: APP_DIR, ctxFactory: () => ctx });
  return { ctx, state, sim };
}

const chat = (state, id) => state.collections.get('business_chats')?.get(id) ?? null;

// Befehl so, wie ihn der Baustein in einer anderen App erzeugt (Quelle der Wahrheit: shared/context-chat.js).
function fromBlock(spec, extra = {}) {
  const { type, payload, record_id } = buildContextChatCommand(spec);
  return makeCommand(type, payload, { id: extra.id || `cmd-${type}`, record_id, app_id: spec.source_app, actor: extra.actor, client_context: extra.client_context });
}

const DATA = { source_app: 'documents', record_ref: 'doc-7', selection: 'Absatz 3', text: 'Fasse das Dokument zusammen.', mode: 'data', title: 'Mit Dokumenten arbeiten · Vertrag', context: { column: 'list', capability_token: 'geheim' } };

test('Baustein data → ctox.chat.task → enqueue: Bestandstext, kein Skill, Rückrufe, keine Geheimnisse; Chat-Turn in business_chats', async () => {
  const { ctx, state, sim } = world();
  const command = fromBlock(DATA, { client_context: { action: 'context-chat', api_key: 'geheim2' } });
  assert.equal(command.type, 'ctox.chat.task');
  const out = await chatTask(command, sim.attach(ctx, command));
  assert.equal(out.status, 'queued');
  const [{ args, skill }] = sim.enqueued;
  assert.equal(skill, null, 'Kontext-Auftrag ohne Skill (Bestand: keiner)');
  assert.equal(args.skill, undefined);
  // ref: store.rs:27080 command_prompt-Format, Anweisung = Nutzertext
  assert.ok(args.instruction.startsWith(`Fasse das Dokument zusammen.\nBusiness OS command:\n- command_id: ${command.id}\n- module: documents\n- type: ctox.chat.task\n- record_id: doc-7\n`));
  assert.match(args.instruction, /Payload JSON:\n\{[\s\S]*"selection": "Absatz 3"/);
  assert.doesNotMatch(args.instruction, /geheim|capability_token|api_key/);
  assert.equal(args.title, DATA.title);
  assert.equal(args.thread_key, 'business-os/documents');
  assert.equal(args.on_complete, 'ctox.chat.task.complete');
  assert.equal(args.on_failed, 'ctox.chat.task.failed');
  assert.equal(args.idempotency_key, command.id);
  assert.equal(args.payload.chat_id, command.id);
  const doc = chat(state, command.id);
  assert.equal(doc.title, DATA.title);
  assert.equal(doc.tracking_task_id, out.task_id);
  assert.equal(doc.tracking_status, 'queued');
  assert.deepEqual(doc.messages.map((m) => [m.role, m.text]), [['user', 'Fasse das Dokument zusammen.']]);
});

test('Modus ask → Bestands-Anweisung „ausschließlich lesend“ vor dem Nutzertext', async () => {
  const { ctx, sim } = world();
  const command = fromBlock({ ...DATA, mode: 'ask', text: 'Wer hat unterschrieben?' });
  await chatTask(command, sim.attach(ctx, command));
  assert.ok(sim.enqueued[0].args.instruction.startsWith('Beantworte die folgende Frage ausschließlich lesend. Nutze nur vorhandene Daten und Kontext; führe keine Änderungen an Daten, Records, Dateien oder der App aus. Antworte knapp und direkt.\n\nWer hat unterschrieben?\nBusiness OS command:'));
});

test('ctox.app.modify.request: System-Skill business-os-app-module-development, App-Ziel-Block, Rechteprüfung im Handler', async () => {
  const { ctx, sim } = world();
  const spec = { ...DATA, mode: 'app', canModifyApp: true, text: 'Spalte Fälligkeit ergänzen.', title: 'Dokumente-App anpassen · Liste' };
  const command = fromBlock(spec, { actor: { id: 'u-admin', role: 'admin' } });
  assert.equal(command.type, 'ctox.app.modify.request');
  assert.equal(command.record_id, 'documents');
  await appModify(command, sim.attach(ctx, command));
  const [{ args, skill }] = sim.enqueued;
  assert.deepEqual(skill, { skill: 'business-os-app-module-development', scope: 'core' });
  assert.ok(args.instruction.startsWith('Modifiziere die App documents anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, Daten selbst nicht als primäres Ziel verändern.\n\nSpalte Fälligkeit ergänzen.\nBusiness OS task resources:\n- required_skills: business-os-app-module-development\n'));
  assert.match(args.instruction, /- module_id: documents\n- install_target: runtime-installed-module\n- app_directory: runtime\/business-os\/installed-modules\/documents\n/);
  assert.match(args.instruction, /- validation: ctox business-os app validate documents --installed\n/);
  assert.match(args.instruction, /Business OS app request summary:\n- app_title: Dokumente-App anpassen · Liste\n/);
  assert.equal(args.on_complete, 'ctox.chat.task.complete');

  for (const actor of [{ id: 'u1', role: 'user' }, { id: 'u2', role: 'founder' }]) {
    const w = world();
    const denied = fromBlock(spec, { id: `cmd-denied-${actor.id}`, actor });
    await rejectsWithCode(appModify(denied, w.sim.attach(w.ctx, denied)), 'forbidden');
    assert.equal(w.sim.enqueued.length, 0);
    assert.equal(chat(w.state, denied.id), null);
  }
  const w = world();
  const granted = fromBlock(spec, { id: 'cmd-perm', actor: { id: 'u3', role: 'user', permissions: ['apps.source.edit'] } });
  await appModify(granted, w.sim.attach(w.ctx, granted));
  assert.equal(w.sim.enqueued.length, 1);
});

test('Pflichtwerte: ohne Text, ohne App, app-Modus über chat.task, zu langer Text → invalid, kein Auftrag', async () => {
  const cases = [
    makeCommand('ctox.chat.task', { source_app: 'mail', text: '  ' }),
    makeCommand('ctox.chat.task', { text: 'x' }),
    makeCommand('ctox.chat.task', { source_app: 'mail', text: 'x', mode: 'app' }),
    makeCommand('ctox.chat.task', { source_app: 'mail', text: 'x'.repeat(8001) }),
    makeCommand('ctox.app.modify.request', { source_app: 'mail', text: 'x', mode: 'data' }, { actor: { id: 'a', role: 'admin' } }),
  ];
  for (const command of cases) {
    const { ctx, state, sim } = world();
    const handler = command.type === 'ctox.chat.task' ? chatTask : appModify;
    await rejectsWithCode(handler(command, sim.attach(ctx, command)), 'invalid');
    assert.equal(sim.enqueued.length, 0);
    assert.equal(state.writes.length, 0);
  }
});

async function started(seed) {
  const w = world(seed);
  const command = fromBlock(DATA);
  await chatTask(command, w.sim.attach(w.ctx, command));
  return { ...w, command, taskId: w.sim.enqueued[0].task_id };
}

test('complete → Antwort als role ctox mit replyFor in business_chats; Doppelzustellung und Wiederholung ohne Wirkung', async () => {
  const { state, sim, command, taskId } = await started();
  const d = await sim.complete(taskId, { text: 'Zusammenfassung: drei Punkte.' });
  assert.equal(d.command.type, 'ctox.chat.task.complete');
  assert.equal(d.result.status, 'completed');
  const doc = chat(state, command.id);
  const reply = doc.messages.at(-1);
  assert.deepEqual([reply.role, reply.text, reply.replyFor, reply.status], ['ctox', 'Zusammenfassung: drei Punkte.', taskId, 'completed']);
  assert.equal(doc.tracking_active, false);
  assert.equal(doc.tracking_status, 'completed');
  assert.deepEqual(await sim.complete(taskId, { text: 'nochmal' }), { delivered: false, reason: 'duplicate' });
  // Zustellung am Simulator vorbei (Core stellt erneut zu): Handler selbst ist idempotent.
  const again = await (await import('../ctox.chat.task.complete.js')).default(d.command, d.ctx);
  assert.equal(again.duplicate, true);
  assert.equal(chat(state, command.id).messages.filter((m) => m.replyFor === taskId).length, 1);
});

test('failed und cancelled → genau eine Fehlernachricht failureFor; danach kein complete-Eintrag', async () => {
  for (const status of ['failed', 'cancelled']) {
    const { ctx, state, sim, command, taskId } = await started();
    const failedHandler = (await import('../ctox.chat.task.failed.js')).default;
    // Abbruch: der Core stellt on_failed mit status 'cancelled' zu (README agent-sim, Festlegung 15.10.); die Simulation
    // kennt nur fail(), daher wird die Zustellung hier wie vom Core gebaut.
    const d = status === 'failed'
      ? await sim.fail(taskId, { code: 'model_error', message: 'Modell nicht erreichbar' })
      : await (async () => {
        const cmd = { id: `cmd_callback_${taskId}`, type: 'ctox.chat.task.failed', app_id: 'ctox', payload: { task_id: taskId, status: 'cancelled', result: null } };
        const c = sim.attach(world().ctx, cmd);
        Object.assign(c, { read: ctx.read, get: ctx.get, put: ctx.put, transaction: ctx.transaction });
        return { command: cmd, ctx: c, result: await failedHandler(cmd, c) };
      })();
    if (status === 'cancelled') assert.equal(d.result.status, 'cancelled');
    const doc = chat(state, command.id);
    const failure = doc.messages.filter((m) => m.failureFor === taskId);
    assert.equal(failure.length, 1, status);
    assert.equal(doc.tracking_active, false);
    if (status === 'failed') {
      assert.equal(failure[0].text, 'Modell nicht erreichbar');
      assert.equal(failure[0].errorCode, 'model_error');
    }
    const again = await failedHandler(d.command, d.ctx);
    assert.equal(again.duplicate, true);
    const lateComplete = (await import('../ctox.chat.task.complete.js')).default;
    const late = await lateComplete({ ...d.command, type: 'ctox.chat.task.complete', payload: { task_id: taskId, status: 'completed', result: { text: 'spät' } } }, d.ctx);
    assert.equal(late.duplicate, true);
    assert.equal(chat(state, command.id).messages.some((m) => m.text === 'spät'), false);
  }
});

test('Folge-Turn im selben Chat (chat_id): Nachrichten werden angehängt, Antwort landet beim richtigen Turn', async () => {
  const { ctx, state, sim, command, taskId } = await started();
  const follow = makeCommand('ctox.chat.task', { ...buildContextChatCommand({ ...DATA, text: 'Und Punkt 4?' }).payload, chat_id: command.id }, { id: 'cmd-follow' });
  await chatTask(follow, sim.attach(ctx, follow));
  const followTask = sim.enqueued[1].task_id;
  await sim.complete(taskId, { text: 'Antwort 1' });
  const doc = chat(state, command.id);
  assert.deepEqual(doc.messages.map((m) => m.text), ['Fasse das Dokument zusammen.', 'Und Punkt 4?', 'Antwort 1']);
  assert.equal(doc.tracking_task_id, followTask, 'laufender Folge-Turn bleibt aktiv');
  assert.equal(doc.tracking_active, true);
  await sim.complete(followTask, { reply: 'Antwort 2' });
  assert.equal(chat(state, command.id).tracking_active, false);
});

test('Rückruf ohne chat_id im Task findet den Chat über tracking_task_id; ohne Chat keine Schreibvorgänge', async () => {
  const { ctx, state, sim, command, taskId } = await started();
  const cb = { id: 'cb-1', type: 'ctox.chat.task.complete', payload: { task_id: taskId, status: 'completed', result: 'Text' } };
  ctx.cap.agent.task.get = async () => null;
  const out = await (await import('../ctox.chat.task.complete.js')).default(cb, ctx);
  assert.equal(out.chat_id, command.id);
  const writes = state.writes.length;
  const none = await (await import('../ctox.chat.task.complete.js')).default({ ...cb, payload: { ...cb.payload, task_id: 'task-unknown' } }, ctx);
  assert.equal(none.reason, 'chat_missing');
  assert.equal(state.writes.length, writes);
});
