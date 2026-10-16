import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.view.save';
const uiPayload = {
  view: { name: 'Kunden · Aktiv', object_type: 'account', view_type: 'table', visibility: 'private', visible_columns: ['name'] },
  filters: [{ field_name: 'customer_stage', operator: 'eq', value: 'active' }],
  sorts: [{ field_name: 'name', direction: 'asc' }],
};

test('speichert Ansicht mit Filtern und Sortierungen (UI-Payload index.js:3491)', async () => {
  const ctx = createCtx();
  const out = await run(ctx, T, uiPayload, { id: 'cv' });
  const viewId = 'view_' + stableDigest('cv');
  assert.equal(out.view.id, viewId);
  const filters = ctx.$.all('customer_view_filters');
  assert.equal(filters.length, 1);
  const expectedFilterRecord = { field_name: 'customer_stage', operator: 'eq', value: 'active', view_id: viewId, position: 0, is_deleted: false, created_at_ms: NOW };
  assert.equal(filters[0].id, 'filter_' + stableDigest(viewId + ':customer_view_filters:0:' + JSON.stringify(expectedFilterRecord)));
  assert.equal(ctx.$.all('customer_view_sorts')[0].id.startsWith('sort_'), true);
  assert.equal(out.activity.properties.filters.length, 1);
  assert.equal(out.activity.linked_record_type, 'view');
});

test('ohne view-Schlüssel gilt der Payload selbst; object_type geprüft', async () => {
  const ctx = createCtx();
  const out = await run(ctx, T, { view_id: 'v1', name: 'Kontakte', object_type: 'contact' });
  assert.equal(out.view.view_type, 'table');
  assert.equal(out.view.visibility, 'private');
  await assert.rejects(run(ctx, T, { view: { name: 'x', object_type: 'task' } }), { code: 'invalid', message: 'view object_type `task` is not supported' });
  await assert.rejects(run(ctx, T, { view: { object_type: 'account' } }), { code: 'invalid', message: 'name is required' });
});

test('erneutes Speichern derselben view_id überschreibt (upsert)', async () => {
  const ctx = createCtx();
  await run(ctx, T, { view_id: 'v1', name: 'A', object_type: 'account' }, { id: 'a' });
  await run(ctx, T, { view_id: 'v1', name: 'B', object_type: 'account' }, { id: 'b' });
  assert.equal(ctx.$.doc('customer_views', 'v1').name, 'B');
});
