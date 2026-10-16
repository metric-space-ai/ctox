// Strang OLG725: Rückruf nach outbound.scrape.script.read (OLG liest seine eigenen Dateien, Kern §16.47 Nr. 4).
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.research_source.script_read.done.js';

const SCRIPT = { target_key: 'dnb', available: true, revision_no: 2, language: 'javascript', script_sha256: 'bb', created_at: null, truncated: false, text: 'console.log(2)' };
const done = (payload) => cmd('outbound.research_source.script_read.done', payload);

test('on_complete: Skriptsicht von OLG wird Ergebnis des Rückrufs, nichts wird geschrieben', async () => {
  const t = createTestCtx();
  const r = await handle(done({ command_id: 'c1:script', status: 'done', result: { ok: true, schema: 'ctox.outbound_lead_generation.scrape_script.v1', script: SCRIPT } }), t.ctx);
  assert.deepEqual(r, { ok: true, schema: 'ctox.outbound.research_source_script.v1', command_id: 'c1:script', script: SCRIPT });
  assert.deepEqual(t.calls.writes, [], 'kein Skript-Dump in eine Collection');
  assert.deepEqual(t.calls.caps, []);
});

test('on_failed: Fehlertext von OLG als unavailable', async () => {
  const t = createTestCtx();
  const e = await rejects(handle(done({ command_id: 'c1:script', status: 'failed', error: { code: 'not_found', reason: 'scrape target dnb could not be read' } }), t.ctx));
  assert.equal(e.code, 'unavailable');
  assert.match(e.message, /c1:script, failed\): not_found: scrape target dnb could not be read/);
});

test('cancelled (§16.48 Nr. 1): Abbruch als unavailable mit Status, ohne error nur der Status', async () => {
  const t = createTestCtx();
  const e = await rejects(handle(done({ command_id: 'c2:script', status: 'cancelled' }), t.ctx));
  assert.equal(e.code, 'unavailable');
  assert.match(e.message, /c2:script, cancelled\): command cancelled$/);
  const e2 = await rejects(handle(done({ command_id: 'c2:script', status: 'cancelled', error: { code: 'cancelled', reason: 'by user' } }), t.ctx));
  assert.match(e2.message, /cancelled\): cancelled: by user$/);
});

test('nur done|failed|cancelled: completed, leer und fremde Werte sind invalid', async () => {
  const t = createTestCtx();
  for (const status of ['completed', '', 'running', undefined]) {
    const e = await rejects(handle(done({ command_id: 'c3', status, result: { ok: true, schema: 'ctox.outbound_lead_generation.scrape_script.v1', script: SCRIPT } }), t.ctx));
    assert.equal(e.code, 'invalid', `status ${status}`);
  }
});

test('höchstens einmal je command_id: wiederholter Rückruf liefert dasselbe Ergebnis, schreibt nichts', async () => {
  const t = createTestCtx();
  const p = { command_id: 'c4:script', status: 'done', result: { ok: true, schema: 'ctox.outbound_lead_generation.scrape_script.v1', script: SCRIPT } };
  assert.deepEqual(await handle(done(p), t.ctx), await handle(done(p), t.ctx));
  assert.deepEqual(t.calls.writes, []);
});

test('fremdes oder kaputtes Ergebnis wird abgelehnt; command_id ist Pflicht', async () => {
  const t = createTestCtx();
  const e1 = await rejects(handle(done({ command_id: 'c1:script', status: 'done', result: { ok: true, script: SCRIPT } }), t.ctx));
  assert.equal(e1.code, 'invalid');
  const e2 = await rejects(handle(done({ status: 'done', result: {} }), t.ctx));
  assert.equal(e2.code, 'invalid');
});
