// modules/invoices/tests/mount-unmount.test.mjs
//
// Verifies the v5 mount contract from the
// `business-os-app-module-development` skill:
//   - mount(ctx) returns a cleanup function.
//   - During mount, every watched collection's `.$` is subscribed to.
//   - When the cleanup runs, every subscription is `.unsubscribe()`-ed.
//   - data-context-* attributes are stamped on the rendered root + panes
//     (skill UI Standard).

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { build } from 'esbuild';
import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';

const SOURCE = new URL('../index.js', import.meta.url);

function makeSubject() {
  const subscribers = new Set();
  return {
    subscribe(fn) {
      subscribers.add(fn);
      return {
        unsubscribe() {
          subscribers.delete(fn);
        },
      };
    },
    emit(payload) {
      for (const fn of subscribers) fn(payload);
    },
    get size() {
      return subscribers.size;
    },
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
    async upsert(doc) {
      docs = docs.filter((d) => d.id !== doc.id).concat([doc]);
      subject.emit({ op: 'update', id: doc.id });
    },
    async remove(id) {
      docs = docs.filter((d) => d.id !== id);
      subject.emit({ op: 'remove', id });
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
        removeChild(c) { this.children = this.children.filter((x) => x !== c); },
        addEventListener() {},
        setAttribute(k, v) { attrs[k] = v; },
      };
      Object.defineProperty(node, 'disabled', { get() { return !!attrs.disabled; }, set(v) { attrs.disabled = v; } });
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
  const timers = new Set();
  return {
    setTimeout(fn, ms) {
      const id = { cleared: false };
      const handle = setTimeout(() => {
        timers.delete(id);
        if (!id.cleared) fn();
      }, ms || 0);
      id.handle = handle;
      timers.add(id);
      return id;
    },
    clearTimeout(id) {
      if (id && id.handle) clearTimeout(id.handle);
      if (id) id.cleared = true;
      timers.delete(id);
    },
  };
}

async function buildInvoicesModule() {
  const result = await build({
    entryPoints: [fileURLToPath(SOURCE)],
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

function fileURLToPath(url) {
  return url.pathname;
}

test('mount returns a cleanup function and the cleanup unsubscribes all watchers', async () => {
  const globalRefs = globalThis;
  const invoicesCol = makeCollection([
    { id: 'inv_1', state: 'draft', invoice_type: 'sale_out', party_id: 'cust_1', currency: 'EUR', is_deleted: false, search_text: '', created_at_ms: 1, updated_at_ms: 1 },
  ]);
  const customerCol = makeCollection([
    { id: 'cust_1', name: 'Acme', is_deleted: false, search_text: '', updated_at_ms: 1 },
  ]);
  // Watched collections, in the order the module requests them.
  const allCollections = {
    accounting_invoices: invoicesCol,
    accounting_invoice_lines: makeCollection(),
    accounting_payments: makeCollection(),
    accounting_payment_allocations: makeCollection(),
    accounting_dunning_runs: makeCollection(),
    accounting_dunning_letters: makeCollection(),
    accounting_journal_entries: makeCollection(),
    accounting_journal_entry_lines: makeCollection(),
    customer_accounts: customerCol,
  };

  const fakeDoc = makeFakeDocument();
  const fakeWin = makeFakeWindow();
  const originalWindow = globalRefs.window;
  const originalDocument = globalRefs.document;
  globalRefs.window = fakeWin;
  globalRefs.document = fakeDoc;

  try {
    const mod = await buildInvoicesModule();
    const ctx = {
      db: allCollections,
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
    assert.equal(typeof unmount, 'function', 'mount must return a cleanup function');

    // The mount established one subscription per watched collection.
    for (const [name, c] of Object.entries(allCollections)) {
      assert.ok(c.$.size >= 1, `${name} should have at least one active subscription after mount`);
    }

    // When we unmount, every subscription must be released.
    unmount();
    for (const [name, c] of Object.entries(allCollections)) {
      assert.equal(c.$.size, 0, `${name} should have zero subscriptions after unmount`);
    }
  } finally {
    if (originalWindow === undefined) delete globalRefs.window; else globalRefs.window = originalWindow;
    if (originalDocument === undefined) delete globalRefs.document; else globalRefs.document = originalDocument;
  }
});

test('reactive subscription triggers a refresh after unmount cleanup is not called', async () => {
  const invoicesCol = makeCollection([]);
  const watched = {
    accounting_invoices: invoicesCol,
    accounting_invoice_lines: makeCollection(),
    accounting_payments: makeCollection(),
    accounting_payment_allocations: makeCollection(),
    accounting_dunning_runs: makeCollection(),
    accounting_dunning_letters: makeCollection(),
    accounting_journal_entries: makeCollection(),
    accounting_journal_entry_lines: makeCollection(),
    customer_accounts: makeCollection(),
  };
  const fakeDoc = makeFakeDocument();
  const fakeWin = makeFakeWindow();
  const originalWindow = globalThis.window;
  const originalDocument = globalThis.document;
  globalThis.window = fakeWin;
  globalThis.document = fakeDoc;
  try {
    const mod = await buildInvoicesModule();
    const ctx = {
      db: watched,
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
    assert.ok(invoicesCol.$.size >= 1, 'invoice subscription active');
    // Simulate a remote write: upsert + emit. The reactive watcher should
    // call scheduleRefresh(), which sets a timer. We don't need to wait for
    // the timer to fire — we just need the subscription to have fired and
    // not have crashed.
    await invoicesCol.upsert({
      id: 'inv_99', state: 'draft', invoice_type: 'sale_out', party_id: '',
      currency: 'EUR', is_deleted: false, search_text: '', created_at_ms: 1, updated_at_ms: 2,
    });
    // Wait past the 80ms refresh debounce.
    await new Promise((r) => setTimeout(r, 150));
    unmount();
    assert.equal(invoicesCol.$.size, 0, 'subscription released after unmount');
  } finally {
    if (originalWindow === undefined) delete globalThis.window; else globalThis.window = originalWindow;
    if (originalDocument === undefined) delete globalThis.document; else globalThis.document = originalDocument;
  }
});

test('rendered shell stamps data-context-* attributes on root, list, and center panes', async () => {
  const watched = {
    accounting_invoices: makeCollection([]),
    accounting_invoice_lines: makeCollection(),
    accounting_payments: makeCollection(),
    accounting_payment_allocations: makeCollection(),
    accounting_dunning_runs: makeCollection(),
    accounting_dunning_letters: makeCollection(),
    accounting_journal_entries: makeCollection(),
    accounting_journal_entry_lines: makeCollection(),
    customer_accounts: makeCollection(),
  };
  const fakeDoc = makeFakeDocument();
  const fakeWin = makeFakeWindow();
  const originalWindow = globalThis.window;
  const originalDocument = globalThis.document;
  globalThis.window = fakeWin;
  globalThis.document = fakeDoc;
  try {
    const mod = await buildInvoicesModule();
    const ctx = {
      db: watched,
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
    // Walk the rendered tree and find elements with data-context-* attrs.
    const root = fakeDoc.getElementById('invoices-root');
    assert.ok(root, 'root must exist');
    assert.equal(root.dataset.contextModule, 'invoices');
    assert.equal(root.dataset.contextSubmodule, 'shell');
    assert.equal(root.dataset.contextSkill, 'product_engineering/business-os-app-module-development');
    // First-level pane children carry context too.
    const grid = root.children[0];
    const listPane = grid.children[0];
    const centerPane = grid.children[1];
    assert.equal(listPane.dataset.contextSubmodule, 'list');
    assert.equal(listPane.dataset.contextRecordType, 'accounting_invoices');
    assert.equal(centerPane.dataset.contextSubmodule, 'center');
    unmount();
  } finally {
    if (originalWindow === undefined) delete globalThis.window; else globalThis.window = originalWindow;
    if (originalDocument === undefined) delete globalThis.document; else globalThis.document = originalDocument;
  }
});
