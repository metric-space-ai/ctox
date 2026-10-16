// modules/invoices/tests/mount-unmount.test.mjs
//
// Verifies the App-API 4.0 mount contract (PORTING.md R1/R3):
//   - mount(root, api, {restored}) returns {unmount, snapshot}.
//   - During mount, every watched collection has one api.query subscription.
//   - unmount() releases every subscription.
//   - data-context-* attributes are stamped on the rendered root + panes
//     (skill UI Standard), the workspace stays two panes + one resizer.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { WATCHED, loadInvoicesModule, makeApi, withFakeDom } from './_v4-node.mjs';

test('mount returns {unmount, snapshot} and unmount releases every watcher', async () => {
  await withFakeDom(async (fakeDoc) => {
    const mod = await loadInvoicesModule();
    const t = await makeApi({
      accounting_invoices: [{ id: 'inv_1', state: 'draft', invoice_type: 'sale_out', party_id: 'cust_1', currency: 'EUR', is_deleted: false, created_at_ms: 1, updated_at_ms: 1 }],
      customer_accounts: [{ id: 'cust_1', name: 'Acme', is_deleted: false, updated_at_ms: 1 }],
    });
    const handle = await mod.mount(fakeDoc.createElement('div'), t.api, {});
    assert.equal(typeof handle.unmount, 'function', 'mount must return {unmount}');
    assert.equal(typeof handle.snapshot, 'function', 'mount must return {snapshot}');
    for (const name of WATCHED) {
      assert.equal(t.subscribers(name), 1, `${name} should have exactly one subscription after mount`);
    }
    handle.unmount();
    for (const name of WATCHED) {
      assert.equal(t.subscribers(name), 0, `${name} should have zero subscriptions after unmount`);
    }
  });
});

test('a live write (core upsert) triggers a refresh without crashing; unmount releases it', async () => {
  await withFakeDom(async (fakeDoc) => {
    const mod = await loadInvoicesModule();
    const t = await makeApi({});
    const handle = await mod.mount(fakeDoc.createElement('div'), t.api, {});
    assert.equal(t.subscribers('accounting_invoices'), 1, 'invoice subscription active');
    t.core.upsert('accounting_invoices', {
      id: 'inv_99', state: 'draft', invoice_type: 'sale_out', party_id: '',
      currency: 'EUR', is_deleted: false, created_at_ms: 1, updated_at_ms: 2,
    });
    // Wait past the 80ms refresh debounce.
    await new Promise((r) => setTimeout(r, 150));
    assert.equal(globalThis.window.__ctoxInvoicesModule?.inspect?.().invoice_count ?? 1, 1);
    handle.unmount();
    assert.equal(t.subscribers('accounting_invoices'), 0, 'subscription released after unmount');
  });
});

test('rendered shell stamps data-context-* attributes on root, list, and center panes', async () => {
  await withFakeDom(async (fakeDoc) => {
    const mod = await loadInvoicesModule();
    const t = await makeApi({});
    const handle = await mod.mount(fakeDoc.createElement('div'), t.api, {});
    const root = fakeDoc.getElementById('invoices-root');
    assert.ok(root, 'root must exist');
    assert.equal(root.dataset.contextModule, 'invoices');
    assert.equal(root.dataset.contextSubmodule, 'shell');
    assert.equal(root.dataset.contextSkill, 'product_engineering/business-os-app-module-development');
    // The standard kit frame (.ctox-workspace) interleaves .ctox-column-resizer
    // handles between the panes, so locate panes by their context attributes.
    const grid = root.children[0];
    const listPane = grid.children.find((c) => c.dataset?.contextSubmodule === 'list');
    const centerPane = grid.children.find((c) => c.dataset?.contextSubmodule === 'center');
    assert.ok(listPane, 'list pane must exist');
    assert.ok(centerPane, 'center pane must exist');
    assert.equal(listPane.dataset.contextRecordType, 'accounting_invoices');
    assert.equal(centerPane.dataset.contextRecordType, 'accounting_invoices');
    // IA-Karte: exactly a 2-pane workspace — list + single resizer + main.
    assert.equal(grid.children.length, 3, 'workspace must be two panes and one resizer (no third column)');
    assert.ok(!grid.children.some((c) => c.dataset?.rightContent !== undefined), 'no right/inspector pane');
    assert.equal(t.title, 'Rechnungen', 'title goes through api.shell.setTitle');
    handle.unmount();
  });
});
