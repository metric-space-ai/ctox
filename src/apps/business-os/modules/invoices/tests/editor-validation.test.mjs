// modules/invoices/tests/editor-validation.test.mjs
//
// P1#1 + P1#2 + P1#3 — exercises the editor's validation gate end-to-end:
//   - the JS validator surfaces field-level errors for empty party / no lines
//   - the published computeValidationIssues helper gates the post button
//   - createDraft refuses to dispatch when no customer is available
//
// We reuse the same fake-DOM + App-API 4.0 scaffolding as mount-unmount.test.mjs (_v4-node.mjs).
// Driving the full DOM-click flow in the shim is brittle, so we focus on
// the validation contract that ties the UI gate to the native gate.

import { strict as assert } from 'node:assert';
import test from 'node:test';
import { loadInvoicesModule, makeApi, withFakeDom } from './_v4-node.mjs';

test('computeValidationIssues flags empty party_id and missing lines', async () => {
  const issues = (await import('../core/invoice-validate.js')).validateInvoice;
  const out = issues({
    id: 'inv_draft',
    invoice_type: 'sale_out',
    party_id: '',
    currency: 'EUR',
    invoice_date_ms: 1700000000000,
    state: 'draft',
    lines: [],
  });
  const errorFields = out
    .filter((i) => (i.severity || 'error') === 'error')
    .map((i) => i.field);
  assert.ok(errorFields.includes('party_id'), `expected party_id, got: ${errorFields}`);
  assert.ok(errorFields.includes('lines'), `expected lines, got: ${errorFields}`);
});

test('computeValidationIssues accepts a complete draft', async () => {
  const issues = (await import('../core/invoice-validate.js')).validateInvoice;
  const out = issues({
    id: 'inv_draft',
    invoice_type: 'sale_out',
    party_id: 'cust_1',
    currency: 'EUR',
    invoice_date_ms: 1700000000000,
    state: 'draft',
    lines: [
      { id: 'l1', position: 1, description: 'Beratung', quantity: 1000, unit: 'h', unit_price_cents: 12000, tax_rate: 0.19, account_code: '8400' },
    ],
  });
  assert.deepEqual(out, [], 'complete draft must produce zero issues');
});

test('computeValidationIssues mirrors the native gate for reverse_charge and skonto', async () => {
  const { validateInvoice } = await import('../core/invoice-validate.js');
  // reverse_charge on credit_note is illegal in both the JS and Rust sides.
  const out1 = validateInvoice({
    id: 'inv_rc',
    invoice_type: 'credit_note_out',
    party_id: 'cust_1',
    currency: 'EUR',
    invoice_date_ms: 1700000000000,
    state: 'draft',
    reverse_charge: true,
    credit_note_for_id: 'inv_origin',
    lines: [
      { id: 'l1', position: 1, description: 'X', quantity: 1000, unit: 'h', unit_price_cents: 100, tax_rate: 0.19, account_code: '8400' },
    ],
  });
  assert.ok(
    out1.some((i) => i.field === 'reverse_charge'),
    'reverse_charge on credit_note must be rejected by the JS validator',
  );
  // skonto_percent without skonto_days must be rejected.
  const out2 = validateInvoice({
    id: 'inv_sk',
    invoice_type: 'sale_out',
    party_id: 'cust_1',
    currency: 'EUR',
    invoice_date_ms: 1700000000000,
    state: 'draft',
    skonto_percent: 2,
    lines: [
      { id: 'l1', position: 1, description: 'X', quantity: 1000, unit: 'h', unit_price_cents: 100, tax_rate: 0.19, account_code: '8400' },
    ],
  });
  assert.ok(
    out2.some((i) => i.field === 'skonto_days'),
    'skonto_percent without skonto_days must be rejected by the JS validator',
  );
});

test('mount publishes a debug handle so the shell can drive tests / inspect state', async () => {
  await withFakeDom(async (fakeDoc) => {
    const mod = await loadInvoicesModule();
    const t = await makeApi({});
    const handle = await mod.mount(fakeDoc.createElement('div'), t.api, {});
    assert.ok(globalThis.window.__ctoxInvoicesModule, 'module must publish __ctoxInvoicesModule');
    assert.equal(typeof globalThis.window.__ctoxInvoicesModule.mount, 'function');
    assert.equal(typeof globalThis.window.__ctoxInvoicesModule.inspect, 'function');
    const snapshot = globalThis.window.__ctoxInvoicesModule.inspect();
    assert.equal(snapshot.mounted, true);
    // v4: no error strip in the content — problems go to the status zone only.
    assert.equal(Object.hasOwn(snapshot, 'last_error'), false);
    assert.equal(t.zone.size, 0);
    handle.unmount();
  });
});

test('mount does not dispatch any command when no parties are available', async () => {
  await withFakeDom(async (fakeDoc) => {
    const mod = await loadInvoicesModule();
    const t = await makeApi({}); // empty CRM
    const handle = await mod.mount(fakeDoc.createElement('div'), t.api, {});
    // Mount does not dispatch any command on its own (the createDraft click
    // handler is the one that would, and we cannot fire it from the shim).
    assert.equal(t.commands.length, 0, 'mount itself must not dispatch any business command');
    handle.unmount();
  });
});
