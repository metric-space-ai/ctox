// KNOW2: knowledge.book.ingest – Knowledge Book anderer Apps (heute research) über files-ref (Kern §16.32) in
// knowledge_items / knowledge_runbooks, Vorgang in knowledge_tasks. Aus modules/knowledge/handlers:
// node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createCtx } from './_ctx.mjs';
import ingest from '../knowledge.book.ingest.js';
import { BOOK_SCHEMA } from '../_lib/book.js';

const NOW = 1_700_000_000_000;
const REF = 'a'.repeat(64);

export const bookJson = (extra = {}) => JSON.stringify({
  schema_version: BOOK_SCHEMA,
  knowledge_domain: 'drone/bearings',
  knowledge_version_id: 'knowledge-v7',
  title: 'Lagerlasten',
  summary: 'Skillbook und ein Runbook.',
  elements: [
    { id: 'drone/bearings/skillbook/main', kind: 'skillbook', title: 'Drohnenlager', markdown: '# Drohnenlager', citations: [{ source_id: 'src-1' }] },
    { id: 'drone/bearings/runbook/last', kind: 'runbook', title: 'Lastfall prüfen', markdown: '1. …', citations: [] },
  ],
  gaps: ['Temperatur'],
  provenance: { app: 'research', research_task_id: 'rt-1' },
  ...extra,
});

const command = (payload = {}, id = 'agent-task-1:knowledge-book') => ({
  id,
  type: 'knowledge.book.ingest',
  record_id: 'rt-1',
  payload: { book: { ref: REF }, title: 'Knowledge Book · Lagerlasten', knowledge_domain: 'drone/bearings', record_id: 'rt-1', source: { app: 'research', research_task_id: 'rt-1' }, ...payload },
  actor: { id: 'system' },
  app_id: 'knowledge',
});

function world(files = { [REF]: bookJson() }) {
  return createCtx({ now: NOW, files, seed: { knowledge_items: [{ id: 'drone/bearings/skillbook/main', kind: 'skillbook', title: 'Alt', payload: { keep: 1 }, updated_at_ms: 1 }] } });
}

test('gültiges Book → knowledge_items/knowledge_runbooks upsert mit Herkunft, knowledge_tasks completed, eine Transaktion', async () => {
  const ctx = world();
  const out = await ingest(command(), ctx);
  assert.equal(out.status, 'completed');
  const item = ctx.$.doc('knowledge_items', 'drone/bearings/skillbook/main');
  assert.equal(item.title, 'Drohnenlager');
  assert.equal(item.markdown, '# Drohnenlager');
  assert.equal(item.payload.keep, 1);
  assert.equal(item.payload.book_ref, REF);
  assert.equal(item.payload.import_command_id, 'agent-task-1:knowledge-book');
  assert.deepEqual(item.payload.citations, [{ source_id: 'src-1' }]);
  assert.equal(item.payload.source.app, 'research');
  const runbook = ctx.$.doc('knowledge_runbooks', 'drone/bearings/runbook/last');
  assert.equal(runbook.kind, 'runbook');
  const rec = ctx.$.doc('knowledge_tasks', 'agent-task-1:knowledge-book');
  assert.equal(rec.status, 'completed');
  assert.equal(rec.record_id, 'rt-1');
  assert.equal(rec.command_type, 'knowledge.book.ingest');
  assert.deepEqual(rec.result.item_ids, ['drone/bearings/skillbook/main']);
  assert.deepEqual(rec.result.runbook_ids, ['drone/bearings/runbook/last']);
  assert.equal(ctx.$.transactions.length, 1);
  assert.equal(ctx.$.transactions[0].length, 3);
});

test('Doppelzustellung desselben Befehls ohne Wirkung', async () => {
  const ctx = world();
  await ingest(command(), ctx);
  const before = JSON.stringify([ctx.$.all('knowledge_items'), ctx.$.all('knowledge_runbooks'), ctx.$.all('knowledge_tasks')]);
  const writes = ctx.$.writes.length;
  const again = await ingest(command(), ctx);
  assert.equal(again.duplicate, true);
  assert.equal(ctx.$.writes.length, writes);
  assert.equal(JSON.stringify([ctx.$.all('knowledge_items'), ctx.$.all('knowledge_runbooks'), ctx.$.all('knowledge_tasks')]), before);
});

test('ungültiges oder fehlendes Book → nichts angewendet, knowledge_tasks failed (invalid_book)', async () => {
  for (const [files, pattern] of [
    [{ [REF]: bookJson({ schema_version: 'ctox.research.knowledge-refresh.v1' }) }, /schema_version/],
    [{ [REF]: bookJson({ knowledge_domain: 'andere/domain' }) }, /knowledge_domain/],
    [{ [REF]: bookJson({ elements: [{ id: 'x', kind: 'tabelle', title: 'X' }] }) }, /known kind/],
    [{ [REF]: '{kaputt' }, /JSON/],
    [{}, /not_found/],
  ]) {
    const ctx = world(files);
    const out = await ingest(command(), ctx);
    assert.equal(out.status, 'failed');
    assert.equal(ctx.$.doc('knowledge_items', 'drone/bearings/skillbook/main').title, 'Alt');
    assert.equal(ctx.$.all('knowledge_runbooks').length, 0);
    const rec = ctx.$.doc('knowledge_tasks', 'agent-task-1:knowledge-book');
    assert.equal(rec.status, 'failed');
    assert.equal(rec.error.code, 'invalid_book');
    assert.match(rec.error.message, pattern);
  }
});

test('ohne book.ref → invalid; files nicht erreichbar → Fehler an die Laufzeit, nichts geschrieben', async () => {
  const ctx = world();
  await assert.rejects(ingest(command({ book: {} }), ctx), (e) => e.code === 'invalid');
  const down = world();
  down.files.get = async () => { throw { code: 'unavailable', message: 'offline' }; };
  await assert.rejects(ingest(command(), down), (e) => e.code === 'unavailable');
  assert.equal(down.$.writes.length, 0);
});
