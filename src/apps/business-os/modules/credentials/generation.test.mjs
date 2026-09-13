import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

// Execute the real module and event handlers, without bundling or browser deps.
// Host/DOM doubles below prove command wiring, not real browser acceptance.
const source = await readFile(new URL('./index.js', import.meta.url), 'utf8');
const tick = () => new Promise(resolve => setImmediate(resolve));
const plain = value => JSON.parse(JSON.stringify(value));
const receipt = name => ({ ok: true, name, created: true,
  secret_ref: { scope: 'credentials', name, secret_id: 'secret:test' },
  secret_value_revealed: false });

function element() {
  return { value: '', dataset: {}, listeners: {}, attrs: {},
    querySelector: () => null, querySelectorAll: () => [], contains: () => true,
    setAttribute(k, v) { this.attrs[k] = v; }, getAttribute(k) { return this.attrs[k]; },
    removeAttribute(k) { delete this.attrs[k]; },
    addEventListener(k, fn) { this.listeners[k] = fn; },
    removeEventListener(k) { delete this.listeners[k]; },
    replaceChildren() {}, focus() {}, classList: { toggle() {} } };
}

async function fixture({ allowed = true, generate = async p => receipt(p.name), existing = [] } = {}) {
  const root = element(), host = element(), nodes = new Map();
  for (const selector of ['.credentials-rail','[data-cred-list]','[data-cred-detail]',
    '[data-cred-form]','[data-cred-key]','[data-cred-value]','[data-cred-submit]',
    '[data-cred-generate]','[data-cred-gate]','[data-cred-title]','[data-cred-mode]',
    '[data-cred-view-toggle]']) nodes.set(selector, element());
  root.querySelector = s => nodes.get(s) || null;
  root.querySelectorAll = () => [...nodes.values()];
  host.querySelector = () => root;
  const style = element(), commands = [], notifications = [];
  let subscriptionSelector;
  const context = vm.createContext({ URL, console, document: {
    documentElement: { lang: 'en' }, querySelector: () => style,
    createElement: element, head: { append() {} } },
    fetch: async () => ({ text: async () => '<main></main>' }),
    DOMParser: class { parseFromString() { return { body: { innerHTML: '<main></main>' }, querySelectorAll: () => [] }; } },
  });
  const module = new vm.SourceTextModule(source, { context,
    initializeImportMeta(meta) { meta.url = new URL('./index.js', import.meta.url).href; } });
  await module.link(async specifier => {
    if (specifier === './reveal.mjs') {
      return new vm.SyntheticModule(['mountCredentialReveal'], function() {
        this.setExport('mountCredentialReveal', () => () => {});
      }, { context });
    }
    const exports = specifier.includes('i18n')
      ? { loadModuleMessages: async (_url, locale, labels) => labels[locale] }
      : { canUseBusinessPermission: () => allowed, BusinessOsPermissions: { SecretsManage: 'secrets.manage' } };
    return new vm.SyntheticModule(Object.keys(exports), function() {
      for (const [key, value] of Object.entries(exports)) this.setExport(key, value);
    }, { context });
  });
  await module.evaluate();
  const dispose = await module.namespace.mount({ host, locale: 'en',
    notifications: { show: notification => notifications.push(notification) },
    db: { collection: () => ({ find: query => {
      subscriptionSelector = plain(query);
      return { $: { subscribe: () => ({ unsubscribe() {} }) } };
    } }) },
    commandBus: { dispatch: async doc => {
      commands.push(plain(doc));
      return { result: doc.command_type === 'ctox.secret.list'
        ? { catalog: existing, extra: [] } : await generate(doc.payload) };
    } },
  });
  await tick();
  return { api: module.namespace, commands, notifications, nodes, dispose,
    get subscriptionSelector() { return subscriptionSelector; },
    clickGenerate(name = 'BRIGHTDATA_CREW_PASSWORD') {
      nodes.get('[data-cred-key]').value = name;
      const button = { dataset: { action: 'generate' } };
      root.listeners.click({ target: { closest: () => button } });
    } };
}

test('generation request contains only a validated selector and fixed length', async () => {
  const f = await fixture();
  assert.deepEqual(plain(f.api.buildGenerationPayload(' ACCOUNT_PASSWORD ')), { name: 'ACCOUNT_PASSWORD', length: 24 });
  for (const bad of ['', 'lowercase', '1BAD', 'HAS-DASH', 'X'.repeat(65)]) {
    assert.throws(() => f.api.buildGenerationPayload(bad));
  }
  f.dispose();
});

test('receipt validation rejects missing, wrong-subject and revealed outcomes', async () => {
  const f = await fixture();
  assert.equal(f.api.generationReceiptState(receipt('PASSWORD'), 'PASSWORD'), 'generated');
  assert.equal(f.api.generationReceiptState({ ...receipt('PASSWORD'), created: false }, 'PASSWORD'), 'already_exists');
  for (const value of [null, {}, { ...receipt('PASSWORD'), ok: false },
    receipt('OTHER'), { ...receipt('PASSWORD'), created: 'true' },
    { ...receipt('PASSWORD'), secret_value_revealed: true },
    { ...receipt('PASSWORD'), secret_ref: { scope: 'other', name: 'PASSWORD', secret_id: 'x' } },
    { ...receipt('PASSWORD'), secret_ref: { scope: 'credentials', name: 'OTHER', secret_id: 'x' } }]) {
    assert.equal(f.api.generationReceiptState(value, 'PASSWORD'), null);
  }
  f.dispose();
});

test('real click handler dispatches native generation and refreshes metadata', async () => {
  const f = await fixture();
  f.clickGenerate(); await tick();
  const command = f.commands.find(c => c.command_type === 'ctox.secret.generate');
  assert.equal(command.module, 'credentials');
  assert.deepEqual(command.payload, { name: 'BRIGHTDATA_CREW_PASSWORD', length: 24 });
  assert.equal(f.commands.filter(c => c.command_type === 'ctox.secret.list').length, 2);
  assert.equal(f.notifications.at(-1).type, 'success');
  assert.equal(f.nodes.get('[data-cred-value]').value, '');
  assert.deepEqual(f.subscriptionSelector.selector.command_type.$in,
    ['ctox.secret.put', 'ctox.secret.delete', 'ctox.secret.generate']);
  f.dispose();
});

test('concurrent clicks create one request; UI distinguishes existing receipt', async () => {
  let finish;
  const f = await fixture({ generate: p => new Promise(resolve => { finish = () => resolve({ ...receipt(p.name), created: false }); }) });
  f.clickGenerate(); f.clickGenerate(); await tick();
  assert.equal(f.commands.filter(c => c.command_type === 'ctox.secret.generate').length, 1);
  assert.equal(f.nodes.get('[data-cred-generate]').disabled, true);
  finish(); await tick();
  assert.match(f.notifications.at(-1).message, /already exists and was not changed/);
  f.dispose();
});

test('permission denial, invalid key and existing metadata issue no generation', async () => {
  for (const options of [{ allowed: false }, { existing: [{ name: 'BRIGHTDATA_CREW_PASSWORD', is_set: true }] }, {}]) {
    const f = await fixture(options);
    f.clickGenerate(Object.keys(options).length ? undefined : 'bad-key'); await tick();
    assert.equal(f.commands.filter(c => c.command_type === 'ctox.secret.generate').length, 0);
    f.dispose();
  }
});

test('failed or unconfirmed generation does not reveal raw errors or claim success', async () => {
  const canary = 'synthetic-secret-not-for-rendering';
  for (const generate of [async () => { throw new Error(canary); },
    async () => ({ ok: true, value: canary }), async () => ({ ...receipt('WRONG'), error: canary })]) {
    const f = await fixture({ generate });
    f.clickGenerate(); await tick();
    assert.equal(f.notifications.at(-1).type, 'error');
    assert.doesNotMatch(JSON.stringify(f.notifications), new RegExp(canary));
    assert.equal(f.commands.filter(c => c.command_type === 'ctox.secret.list').length, 2);
    f.dispose();
  }
});

test('generator button is a non-submit action with explicit create-only help', async () => {
  const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
  assert.match(html, /type="button"[^>]*data-action="generate"[^>]*data-cred-generate/);
  assert.match(html, /data-i18n="generate_hint"/);
});
