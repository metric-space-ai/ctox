// node --test src/apps/business-os/modules/knowledge/handlers/tests/knowledge.view.window.test.mjs
// Trigger-Handler E11 gegen die In-Memory-ctx-Attrappe mit simulierter Fähigkeit cap.dataframe.window.
import { test } from 'node:test';
import assert from 'node:assert/strict';

import handle, { rowId, MAX_LIMIT } from '../knowledge.view.window.js';
import { createCtx, triggerCommand, validateDoc } from './_ctx.mjs';
import { createDataframeSim, measuredRows } from './_dataframe-sim.mjs';

const CATALOG = {
  id: 'table:kdt-measured', kind: 'dataframe', title: 'Measured Load Points', updated_at_ms: 1,
  projection_version: 2, rows_source: 'rxdb.rows.fetch', row_count: 300, rows_complete: true,
};

function view(overrides = {}) {
  return {
    id: 'kv:michael:kdt-measured', table: 'kdt-measured', filter: {}, sort: [], skip: 0, limit: 120,
    actor_id: 'michael', updated_at_ms: 10, ...overrides,
  };
}

function setup({ rows = measuredRows(300), catalog = CATALOG, views = [view()] } = {}) {
  const dataframe = createDataframeSim({ 'kdt-measured': rows });
  const ctx = createCtx({ seed: { knowledge_tables: catalog ? [catalog] : [], knowledge_views: views }, dataframe });
  return { ctx, dataframe };
}

const rowsOf = (ctx, viewId) => ctx.$.all('knowledge_table_rows')
  .filter((doc) => doc.view_id === viewId)
  .sort((a, b) => a.row_index - b.row_index);

test('insert: Fenster wird über cap.dataframe.window gelesen und als Zeilen-Dokumente projiziert', async () => {
  const { ctx, dataframe } = setup();
  const result = await handle(triggerCommand(view(), { op: 'insert', seq: 1 }), ctx);
  assert.equal(result.status, 'projected');
  assert.equal(result.rows, 120);
  assert.equal(result.row_count, 300);
  assert.deepEqual(dataframe.calls, [{ table: 'kdt-measured', skip: 0, limit: 120 }]);
  const rows = rowsOf(ctx, view().id);
  assert.equal(rows.length, 120);
  assert.equal(rows[0].id, rowId(view().id, 0));
  assert.equal(rows[0].row_index, 0);
  assert.equal(rows[0].values.row_id, 'MLP-0001');
  assert.equal(rows[0].actor_id, 'michael', 'Zeilenfilter = eigener Nutzer');
  assert.equal(rows[0].table_id, 'kdt-measured');
  assert.equal(rows[119].row_index, 119);
  for (const doc of rows) validateDoc('knowledge_table_rows', doc);
  assert.equal(ctx.$.transactions.length, 1, 'Ersetzen in genau einer Schreibtransaktion');
  assert.deepEqual(ctx.$.cursor(), { seq: 1 });
});

test('update (Blättern): Zeilen des Views werden ersetzt, alte verschwinden', async () => {
  const { ctx } = setup();
  await handle(triggerCommand(view(), { op: 'insert', seq: 1 }), ctx);
  const paged = view({ skip: 240, updated_at_ms: 11 });
  ctx.$.seed('knowledge_views', paged);
  const result = await handle(triggerCommand(paged, { seq: 2 }), ctx);
  assert.equal(result.status, 'projected');
  assert.equal(result.rows, 60, 'letzte Seite: 300 - 240');
  assert.equal(result.removed, 120);
  const rows = rowsOf(ctx, paged.id);
  assert.equal(rows.length, 60);
  assert.equal(rows[0].row_index, 240);
  assert.equal(rows[0].values.row_id, 'MLP-0241');
});

test('Filter und Sortierung im Handler über seitenweise gelesene Tabelle, row_count ist die gefilterte Zahl', async () => {
  const filtered = view({ filter: { text: '  Peak ' }, sort: [{ rpm: 'desc' }] });
  const { ctx, dataframe } = setup({ views: [filtered] });
  const result = await handle(triggerCommand(filtered, { seq: 3 }), ctx);
  // Kernform: filter/sort nur leer → der Handler lädt die Tabelle seitenweise und filtert/sortiert selbst.
  assert.deepEqual(dataframe.calls[0], { table: 'kdt-measured', skip: 0, limit: 1000 });
  assert.ok(dataframe.calls.every((c) => c.filter === undefined && c.sort === undefined));
  assert.equal(result.row_count, 43); // jede 7. Zeile von 300
  const rows = rowsOf(ctx, filtered.id);
  assert.equal(rows.length, 43);
  assert.ok(rows.every((doc) => doc.values.note === 'peak load'));
  assert.ok(rows[0].values.rpm > rows[1].values.rpm);
  assert.ok(rows.every((doc) => doc.row_count === 43));
});

test('erneute Zustellung (Cursor) und unveränderte Fenster schreiben nichts', async () => {
  const { ctx, dataframe } = setup();
  await handle(triggerCommand(view(), { op: 'insert', seq: 5 }), ctx);
  const writes = ctx.$.writes.length;
  assert.deepEqual(await handle(triggerCommand(view(), { seq: 5 }), ctx), { status: 'skipped', reason: 'seen' });
  assert.equal(dataframe.calls.length, 1);
  const same = await handle(triggerCommand(view(), { seq: 6 }), ctx);
  assert.equal(same.status, 'unchanged');
  assert.equal(ctx.$.writes.length, writes);
  assert.deepEqual(ctx.$.cursor(), { seq: 6 });
});

test('überholte Änderung: nur der aktuelle View-Stand wird projiziert', async () => {
  const { ctx, dataframe } = setup({ views: [view({ skip: 120 })] });
  const result = await handle(triggerCommand(view({ skip: 0 }), { seq: 7 }), ctx);
  assert.deepEqual(result, { status: 'skipped', reason: 'superseded' });
  assert.equal(dataframe.calls.length, 0);
  assert.equal(ctx.$.all('knowledge_table_rows').length, 0);
});

test('unbekannte Tabelle bzw. not_found der Fähigkeit leert das Fenster', async () => {
  const { ctx, dataframe } = setup();
  await handle(triggerCommand(view(), { op: 'insert', seq: 1 }), ctx);
  dataframe.failNext({ code: 'not_found', message: 'archived' });
  const next = view({ skip: 120 });
  ctx.$.seed('knowledge_views', next);
  const result = await handle(triggerCommand(next, { seq: 2 }), ctx);
  assert.equal(result.status, 'not_found');
  assert.equal(result.removed, 120);
  assert.equal(rowsOf(ctx, next.id).length, 0);

  const missing = setup({ catalog: null });
  const r2 = await handle(triggerCommand(view(), { op: 'insert', seq: 1 }), missing.ctx);
  assert.equal(r2.status, 'not_found');
  assert.equal(missing.dataframe.calls.length, 0, 'ohne Katalogeintrag kein Fähigkeitsaufruf');
});

test('wiederholbarer Fehler der Fähigkeit: Fehler geht an die Laufzeit, lokaler Stand bleibt', async () => {
  const { ctx, dataframe } = setup();
  await handle(triggerCommand(view(), { op: 'insert', seq: 1 }), ctx);
  dataframe.failNext({ code: 'unavailable', message: 'parquet busy' });
  const next = view({ skip: 120 });
  ctx.$.seed('knowledge_views', next);
  await assert.rejects(handle(triggerCommand(next, { seq: 2 }), ctx), { code: 'unavailable' });
  assert.equal(rowsOf(ctx, next.id).length, 120);
  assert.equal(rowsOf(ctx, next.id)[0].row_index, 0);
  assert.deepEqual(ctx.$.cursor(), { seq: 1 }, 'Cursor rückt bei Fehler nicht vor');
});

test('ungültige Views und fremde Änderungen', async () => {
  const { ctx } = setup();
  await assert.rejects(handle(triggerCommand(view({ limit: MAX_LIMIT + 1 })), ctx), { code: 'invalid' });
  await assert.rejects(handle(triggerCommand(view({ skip: -1 })), ctx), { code: 'invalid' });
  await assert.rejects(handle(triggerCommand(view({ sort: [{ rpm: 'up' }] })), ctx), { code: 'invalid' });
  await assert.rejects(handle(triggerCommand(view({ table: '' })), ctx), { code: 'invalid' });
  await assert.rejects(handle({ type: 'knowledge.view.window', payload: {} }, ctx), { code: 'invalid' });
  assert.deepEqual(await handle({ type: 'trigger', trigger_id: 't', change: { collection: 'knowledge_items', op: 'update', doc: {} } }, ctx),
    { status: 'skipped', reason: 'collection' });
  assert.deepEqual(await handle(triggerCommand(view(), { op: 'delete' }), ctx), { status: 'skipped', reason: 'op' });
  assert.equal(ctx.$.writes.length, 0);
});

test('View-Dokument der App erfüllt collections.schema.json', () => {
  validateDoc('knowledge_views', view());
  assert.throws(() => validateDoc('knowledge_views', { ...view(), skip: 1.5 }), { code: 'invalid' });
  assert.throws(() => validateDoc('knowledge_views', { id: 'x', table: 't' }), { code: 'invalid' });
});
