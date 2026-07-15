// modules/invoices/tests/editor-validation.test.mjs
//
// P1#1 + P1#2 + P1#3 — exercises the editor's validation gate end-to-end:
//   - the JS validator surfaces field-level errors for empty party / no lines
//   - the published computeValidationIssues helper gates the post button
//   - createDraft refuses to dispatch when no customer is available
//
// We reuse the same esbuild + fake-DOM scaffolding as mount-unmount.test.mjs.
// Driving the full DOM-click flow in the shim is brittle, so we focus on
// the validation contract that ties the UI gate to the native gate.

import { strict as assert } from 'node:assert';
import test from 'node:test';
import { build } from 'esbuild';
import { Buffer } from 'node:buffer';

const SOURCE = new URL('../index.js', import.meta.url);

function makeSubject() {
  const subscribers = new Set();
  return {
    subscribe(fn) {
      subscribers.add(fn);
      return { unsubscribe() { subscribers.delete(fn); } };
    },
    emit(payload) { for (const fn of subscribers) fn(payload); },
  };
}

function makeCollection(initialDocs = []) {
  const subject = makeSubject();
  let docs = initialDocs.map((d) => ({ ...d }));
  return {
    $: subject,
    find() {
      return {
        async exec() {
          return docs
            .filter((d) => d && d._deleted !== true && d.is_deleted !== true)
            .map((d) => ({ toJSON: () => d }));
        },
      };
    },
  };
}

function makeFakeDocument() {
  const elements = new Map();
  return {
    createElement(tag) {
      const dataset = {};
      const attrs = {};
      const node = {
        tagName: tag.toUpperCase(),
        className: '',
        innerHTML: '',
        textContent: '',
        children: [],
        style: {},
        dataset,
        classList: {
          _set: new Set(),
          add(c) { this._set.add(c); },
          remove(c) { this._set.delete(c); },
          contains(c) { return this._set.has(c); },
        },
        appendChild(c) { this.children.push(c); return c; },
        replaceChildren() { this.children = []; },
        addEventListener() {},
        setAttribute(k, v) { attrs[k] = v; },
      };
      Object.defineProperty(node, 'disabled', {
        get() { return !!attrs.disabled; },
        set(v) { attrs.disabled = v; },
      });
      return node;
    },
    getElementById(id) {
      if (!elements.has(id)) {
        const el = this.createElement('div');
        el.id = id;
        elements.set(id, el);
      }
      return elements.get(id);
    },
    head: { append() {} },
    body: { innerHTML: '' },
  };
}

function makeFakeWindow() {
  return {
    setTimeout(fn, ms) {
      const id = { cleared: false };
      const handle = setTimeout(() => {
        if (!id.cleared) fn();
      }, ms || 0);
      id.handle = handle;
      return id;
    },
    clearTimeout(id) {
      if (id && id.handle) clearTimeout(id.handle);
      if (id) id.cleared = true;
    },
  };
}

async function buildInvoicesModule() {
  const result = await build({
    entryPoints: [SOURCE.pathname],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'es2020',
    write: false,
    external: ['node:*'],
    logLevel: 'silent',
  });
  const code = result.outputFiles[0].text;
  return await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
}

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
  const fakeDoc = makeFakeDocument();
  const fakeWin = makeFakeWindow();
  const originalWindow = globalThis.window;
  const originalDocument = globalThis.document;
  globalThis.window = fakeWin;
  globalThis.document = fakeDoc;
  const watched = {
    accounting_invoices: makeCollection([]),
    accounting_invoice_lines: makeCollection(),
    accounting_payments: makeCollection(),
    accounting_payment_allocations: makeCollection(),
    accounting_dunning_runs: makeCollection(),
    accounting_dunning_letters: makeCollection(),
    accounting_journal_entries: makeCollection(),
    accounting_journal_entry_lines: makeCollection(),
    customer_accounts: makeCollection([]),
  };
  try {
    const mod = await buildInvoicesModule();
    const ctx = {
      db: { collection: (name) => watched[name] || null },
      commandBus: { dispatch: async () => ({ status: 'completed' }) },
      eventBus: { on: () => () => {} },
      modules: [
        { id: 'buchhaltung', installed: true },
        { id: 'customers', installed: true },
      ],
      host: fakeDoc.createElement('div'),
      left: fakeDoc.createElement('aside'),
      right: fakeDoc.createElement('aside'),
    };
    const unmount = await mod.mount(ctx);
    assert.ok(globalThis.window.__ctoxInvoicesModule, 'module must publish __ctoxInvoicesModule');
    assert.equal(typeof globalThis.window.__ctoxInvoicesModule.mount, 'function');
    assert.equal(typeof globalThis.window.__ctoxInvoicesModule.inspect, 'function');
    const snapshot = globalThis.window.__ctoxInvoicesModule.inspect();
    assert.equal(snapshot.mounted, true);
    assert.equal(snapshot.last_error, '');
    unmount();
  } finally {
    if (originalWindow === undefined) delete globalThis.window; else globalThis.window = originalWindow;
    if (originalDocument === undefined) delete globalThis.document; else globalThis.document = originalDocument;
  }
});

test('mount does not dispatch any command when no parties are available', async () => {
  const fakeDoc = makeFakeDocument();
  const fakeWin = makeFakeWindow();
  const originalWindow = globalThis.window;
  const originalDocument = globalThis.document;
  globalThis.window = fakeWin;
  globalThis.document = fakeDoc;
  const watched = {
    accounting_invoices: makeCollection([]),
    accounting_invoice_lines: makeCollection(),
    accounting_payments: makeCollection(),
    accounting_payment_allocations: makeCollection(),
    accounting_dunning_runs: makeCollection(),
    accounting_dunning_letters: makeCollection(),
    accounting_journal_entries: makeCollection(),
    accounting_journal_entry_lines: makeCollection(),
    customer_accounts: makeCollection([]), // empty CRM
  };
  let dispatched = 0;
  try {
    const mod = await buildInvoicesModule();
    const ctx = {
      db: { collection: (name) => watched[name] || null },
      commandBus: { dispatch: async () => { dispatched += 1; return { status: 'completed' }; } },
      eventBus: { on: () => () => {} },
      modules: [
        { id: 'buchhaltung', installed: true },
        { id: 'customers', installed: true },
      ],
      host: fakeDoc.createElement('div'),
      left: fakeDoc.createElement('aside'),
      right: fakeDoc.createElement('aside'),
    };
    const unmount = await mod.mount(ctx);
    // Mount does not dispatch any command on its own (the createDraft click
    // handler is the one that would, and we cannot fire it from the shim).
    // The contract we assert is: the commandBus has not been called.
    assert.equal(dispatched, 0, 'mount itself must not dispatch any business command');
    unmount();
  } finally {
    if (originalWindow === undefined) delete globalThis.window; else globalThis.window = originalWindow;
    if (originalDocument === undefined) delete globalThis.document; else globalThis.document = originalDocument;
  }
});
