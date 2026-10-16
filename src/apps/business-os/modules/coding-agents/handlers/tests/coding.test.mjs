import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';
import { sourceSanitizeSlug } from '../_coding.js';

test('source_sanitize_slug', () => {
  assert.equal(sourceSanitizeSlug('  Notes App!! '), 'notes-app');
  assert.equal(sourceSanitizeSlug('--a__b--'), 'a-b');
  assert.equal(sourceSanitizeSlug(undefined), '');
});

test('models: module_id Pflicht und normalisiert', async () => {
  const ctx = createCtx({ caps: { 'agent.coding.models': { models: [{ id: 'ctox' }] } } });
  assert.deepEqual(await run(ctx, 'ctox.coding.models', { module_id: 'Notes' }), { models: [{ id: 'ctox' }] });
  assert.deepEqual(ctx.$.capsTo('agent.coding.models')[0].args[0], { module_id: 'notes' });
  await assert.rejects(run(ctx, 'ctox.coding.models', { module_id: '!!' }), { code: 'invalid', message: 'module_id is required' });
});

// §16.44-Form (CAP-3d, befund-cap-3.md Punkt 4): turn {app_id, draft_id, base_draft_rev, prompt, model?, turn_id?}.
const TURN = { app_id: 'notes', draft_id: 'ada/notes', base_draft_rev: '17-abc', prompt: 'mach blau' };

test('turn: §16.44-Form geht flach an agent.coding.turn (model/turn_id optional)', async () => {
  const ctx = createCtx({ caps: { 'agent.coding.turn': { status: 'running', turn_id: 't1', state: 'queued' } } });
  assert.deepEqual(await run(ctx, 'ctox.coding.turn', TURN), { status: 'running', turn_id: 't1', state: 'queued' });
  assert.deepEqual(ctx.$.capsTo('agent.coding.turn')[0].args[0], TURN);
  await run(ctx, 'ctox.coding.turn', { ...TURN, model: 'kimi-k3', turn_id: 'turn-1' });
  assert.deepEqual(ctx.$.capsTo('agent.coding.turn')[1].args[0], { ...TURN, model: 'kimi-k3', turn_id: 'turn-1' });
  await run(ctx, 'ctox.coding.turn', { ...TURN, model: null });
  assert.deepEqual(ctx.$.capsTo('agent.coding.turn')[2].args[0], TURN);
});

test('turn: V3-Form, Mischform, fehlende/leere/fremde Felder → invalid ohne Capability-Aufruf', async () => {
  const ctx = createCtx();
  const reject = (payload, message) => assert.rejects(run(ctx, 'ctox.coding.turn', payload), (e) => e.code === 'invalid' && e.message.startsWith(message));
  await reject({ module_id: 'notes', prompt: 'x' }, 'payload_invalid:module_id');
  await reject({ ...TURN, faux: false }, 'payload_invalid:faux');
  await reject({ ...TURN, preset_id: 'ctox' }, 'payload_invalid:preset_id');
  await reject({ ...TURN, payload: {} }, 'payload_invalid:payload');
  for (const key of ['app_id', 'draft_id', 'base_draft_rev', 'prompt']) {
    const { [key]: _drop, ...rest } = TURN;
    await reject(rest, `payload_invalid:${key}`);
    await reject({ ...TURN, [key]: '  ' }, `payload_invalid:${key}`);
  }
  await reject({ ...TURN, model: '' }, 'payload_invalid:model');
  await reject({ ...TURN, turn_id: 'mit leerzeichen' }, 'payload_invalid:turn_id');
  await reject({ ...TURN, prompt: 'x'.repeat(64 * 1024 + 1) }, 'payload_invalid:prompt');
  assert.equal(ctx.$.capsTo('agent.coding.turn').length, 0);
});

test('cancel: {turn_id} geht an agent.coding.cancel; sonst invalid', async () => {
  const ctx = createCtx({ caps: { 'agent.coding.cancel': { status: 'completed', turn_id: 't1', state: 'cancelled' } } });
  assert.deepEqual(await run(ctx, 'ctox.coding.cancel', { turn_id: 't1' }), { status: 'completed', turn_id: 't1', state: 'cancelled' });
  assert.deepEqual(ctx.$.capsTo('agent.coding.cancel')[0].args[0], { turn_id: 't1' });
  await assert.rejects(run(ctx, 'ctox.coding.cancel', {}), { code: 'invalid', message: 'payload_invalid:turn_id' });
  await assert.rejects(run(ctx, 'ctox.coding.cancel', { turn_id: 't1', module_id: 'x' }), { code: 'invalid', message: 'payload_invalid:module_id' });
  assert.equal(ctx.$.capsTo('agent.coding.cancel').length, 1);
});

test('unavailable (#848 ungebunden): sauberer Befehlsfehler mit Zonentext, kein Absturz – für turn und cancel, auch bei Nicht-Error-Wurf', async () => {
  const unbound = () => { throw { code: 'unavailable', message: 'coding turns not bound' }; }; // Kern wirft {code, message}
  const ctx = createCtx({ caps: { 'agent.coding.turn': unbound, 'agent.coding.cancel': unbound } });
  for (const [type, payload] of [['ctox.coding.turn', TURN], ['ctox.coding.cancel', { turn_id: 't1' }]]) {
    const error = await run(ctx, type, payload).then(() => null, (e) => e);
    assert.ok(error instanceof Error, `${type}: Error-Objekt (kein roher Wurf)`);
    assert.equal(error.code, 'unavailable');
    assert.equal(error.retryable, true);
    assert.match(error.message, /Coding Agent im Kern noch nicht verfügbar/);
  }
  // Andere Kernfehler bleiben unverändert (z. B. forbidden), nur in Fehlerform gebracht.
  const ctx2 = createCtx({ caps: { 'agent.coding.turn': () => { throw { code: 'forbidden', message: 'apps.source.edit' }; } } });
  await assert.rejects(run(ctx2, 'ctox.coding.turn', TURN), (e) => e instanceof Error && e.code === 'forbidden' && e.message === 'apps.source.edit');
});
