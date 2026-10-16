// Node scaffolding for the header-less invoices UI tests on App-API 4.0.
// - 'ctox:shared/x.js' resolves onto the shell's shared/ directory, as the Service Worker
//   rewrites it (PORTING.md R14 / §4 Falle 2) — no esbuild bundle needed any more.
// - The app API is the synchronous in-memory attrappe createTestApi (v4/tools/port/test-api.mjs);
//   the browser test tests/v4-api.test.mjs runs against the frozen kernel stub.
// - A minimal fake document/window (no querySelector) exercises the programmatic frame fallback.
import { registerHooks } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, resolve } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const SHARED = resolve(HERE, '../../../shared');
const PORT_TOOLS = resolve(HERE, '../../../v4/tools/port');

let hooked = false;
export function hookCtoxShared() {
  if (hooked) return;
  hooked = true;
  registerHooks({
    resolve(specifier, context, nextResolve) {
      if (specifier.startsWith('ctox:shared/')) {
        return nextResolve(pathToFileURL(resolve(SHARED, specifier.slice('ctox:shared/'.length))).href, context);
      }
      return nextResolve(specifier, context);
    },
  });
}

let loads = 0;
/** Fresh module instance per call (module-wide state must not leak between tests). */
export async function loadInvoicesModule() {
  hookCtoxShared();
  loads += 1;
  return import(`../index.js?case=${loads}`);
}

export async function makeApi(data = {}) {
  const { createTestApi } = await import(pathToFileURL(resolve(PORT_TOOLS, 'test-api.mjs')).href);
  const t = createTestApi({ appId: 'invoices', appVer: '0.1.0', data });
  // createTestApi freezes `shell` but its setTitle writes shell._title → TypeError (V4-PORT.md OFFEN-6).
  // Copy the frozen objects and record the title here instead.
  const titles = [];
  const shell = { ...t.api.shell, setTitle(title) { titles.push(String(title)); } };
  const api = { ...t.api, shell };
  return { ...t, api, titles, get title() { return titles.at(-1); } };
}

export const WATCHED = [
  'accounting_invoices',
  'accounting_invoice_lines',
  'accounting_payments',
  'accounting_payment_allocations',
  'accounting_dunning_runs',
  'accounting_dunning_letters',
  'accounting_journal_entries',
  'accounting_journal_entry_lines',
  'customer_accounts',
];

export function makeFakeDocument() {
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
          toggle(c, on) { if (on ?? !this._set.has(c)) this._set.add(c); else this._set.delete(c); },
        },
        appendChild(c) { this.children.push(c); return c; },
        replaceChildren() { this.children = []; },
        removeChild(c) { this.children = this.children.filter((x) => x !== c); },
        addEventListener() {},
        removeEventListener() {},
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

export function makeFakeWindow() {
  return {
    setTimeout(fn, ms) {
      const id = { cleared: false };
      id.handle = setTimeout(() => { if (!id.cleared) fn(); }, ms || 0);
      return id;
    },
    clearTimeout(id) {
      if (id && id.handle) clearTimeout(id.handle);
      if (id) id.cleared = true;
    },
  };
}

/** Runs fn with fake globals installed; restores them afterwards. */
export async function withFakeDom(fn) {
  const fakeDoc = makeFakeDocument();
  const originalWindow = globalThis.window;
  const originalDocument = globalThis.document;
  globalThis.window = makeFakeWindow();
  globalThis.document = fakeDoc;
  const originalWarn = console.warn;
  console.warn = () => {}; // node fetch() cannot read file: URLs → expected markup fallback
  try {
    return await fn(fakeDoc);
  } finally {
    console.warn = originalWarn;
    if (originalWindow === undefined) delete globalThis.window; else globalThis.window = originalWindow;
    if (originalDocument === undefined) delete globalThis.document; else globalThis.document = originalDocument;
  }
}
