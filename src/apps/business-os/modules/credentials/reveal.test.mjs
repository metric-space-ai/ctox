import test from 'node:test';
import assert from 'node:assert/strict';
import { credentialFields, mountCredentialReveal } from './reveal.mjs';
import { assertNativeRequestPrivacy, CREDENTIAL_REVEAL_METHOD } from '../../shared/native-request-privacy.mjs';

// Actual controller with DOM/clipboard doubles. Real browser/native acceptance
// is separate; all fixture values here are synthetic, never provider secrets.
class Element {
  constructor(tag = 'div') { this.tag = tag; this.children = []; this.dataset = {}; this.style = {}; this.listeners = new Map(); this._text = ''; }
  set textContent(text) { this._text = String(text); this.children = []; }
  get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this._text = ''; this.children = nodes; }
  setAttribute() {}
  contains(target) { return this === target || this.children.some(child => child.contains(target)); }
  closest(selector) { return selector === '[data-reveal-action]' && this.dataset.revealAction ? this : null; }
  addEventListener(key, fn) { this.listeners.set(key, fn); }
  removeEventListener(key) { this.listeners.delete(key); }
  descendants() { return this.children.flatMap(child => [child, ...child.descendants()]); }
}
const canary = 'synthetic-value-<script>not-markup</script> ';
function fixture({ value = canary, allowed = true, response, clipboardError = false } = {}) {
  const host = new Element(), windowTarget = new Element(), documentTarget = new Element();
  documentTarget.createElement = tag => new Element(tag);
  const requests = [], copied = [], timers = new Map();
  const dispose = mountCredentialReveal({ host, name: 'TEST_LOGIN', allowed,
    windowTarget, documentTarget, t: key => key,
    request: async (...args) => {
      requests.push(args);
      return response ? response(...args) : { schema: 'ctox.credential-reveal.v1', name: 'TEST_LOGIN', value };
    },
    clipboard: { writeText: async text => {
      if (clipboardError) throw Error(canary);
      copied.push(text);
    } },
    schedule: (fn, ms) => { assert.equal(ms, 30000); timers.set(1, fn); return 1; },
    cancel: id => timers.delete(id),
  });
  return { host, requests, copied, timers, dispose, windowTarget, documentTarget,
    async click(action, index) {
      const target = host.descendants().find(node => node.dataset.revealAction === action
        && (index === undefined || node.dataset.revealIndex === String(index)));
      assert.ok(target, `missing ${action} button`);
      // Invoke even disabled buttons: permission must be enforced in handler.
      await host.listeners.get('click')({ target });
    },
  };
}

test('masked by default; explicit Show uses transient request; Hide removes value', async () => {
  const f = fixture();
  assert.equal(f.requests.length, 0);
  assert.ok(!f.host.textContent.includes(canary));
  await f.click('show');
  assert.deepEqual(f.requests, [[CREDENTIAL_REVEAL_METHOD, { name: 'TEST_LOGIN' }, { collection: 'business_commands', timeoutMs: 10000 }]]);
  assert.ok(f.host.textContent.includes(canary));
  assert.equal(f.host.descendants().filter(node => node.tag === 'script').length, 0);
  assert.equal(f.host.descendants().find(node => node.tag === 'pre').textContent, canary);
  await f.click('hide');
  assert.ok(!f.host.textContent.includes(canary));
  assert.equal(f.timers.size, 0);
  f.dispose();
});

test('bundle username/password can be copied individually without trimming', async () => {
  const f = fixture({ value: JSON.stringify({ username: 'crew@example.invalid', password: canary }) });
  await f.click('show');
  await f.click('copy', 0);
  await f.click('copy', 1);
  assert.deepEqual(f.copied, ['crew@example.invalid', canary]);
  assert.equal(f.requests.length, 1);
  f.dispose();
});

test('native bundle aliases and raw API-key/unknown formats preserve exact values', () => {
  for (const username of ['username', 'email', 'login', 'login_hint']) {
    for (const password of ['password', 'credential', 'secret']) {
      assert.deepEqual(credentialFields(JSON.stringify({ [username]: 'u', [password]: canary })), [
        { label: 'username', value: 'u' }, { label: 'password', value: canary },
      ]);
    }
  }
  for (const value of [canary, '{"api_key":"synthetic"}', '[]', 'null', '"text"', '']) {
    assert.deepEqual(credentialFields(value), [{ label: 'valueLabel', value }]);
  }
});

test('Copy while masked never renders plaintext and does not rotate', async () => {
  const f = fixture();
  await f.click('copy-raw');
  assert.deepEqual(f.copied, [canary]);
  assert.ok(!f.host.textContent.includes(canary));
  assert.equal(f.timers.size, 0);
  assert.equal(f.requests.length, 1);
  assert.equal(f.requests[0][0], CREDENTIAL_REVEAL_METHOD);
  f.dispose();
});

test('unprivileged UI cannot request or copy a value', async () => {
  const f = fixture({ allowed: false });
  await f.click('show'); await f.click('copy-raw');
  assert.equal(f.requests.length, 0);
  assert.equal(f.copied.length, 0);
  f.dispose();
});

test('late response after hide, blur, pagehide, visibility change or disposal is discarded', async () => {
  for (const action of ['hide', 'blur', 'pagehide', 'visibilitychange', 'dispose']) {
    let finish;
    const f = fixture({ response: () => new Promise(resolve => { finish = resolve; }) });
    const inflight = f.click('show');
    if (action === 'hide') await f.click('hide');
    else if (action === 'dispose') f.dispose();
    else if (action === 'visibilitychange') { f.documentTarget.hidden = true; f.documentTarget.listeners.get(action)(); }
    else f.windowTarget.listeners.get(action)();
    finish({ schema: 'ctox.credential-reveal.v1', name: 'TEST_LOGIN', value: canary });
    await inflight;
    assert.ok(!f.host.textContent.includes(canary), action);
    assert.equal(f.copied.length, 0);
    assert.equal(f.timers.size, 0);
    f.dispose();
  }
});

test('timeout clears rendered value; disposal removes listeners and timer', async () => {
  const f = fixture();
  await f.click('show');
  f.timers.get(1)();
  assert.ok(!f.host.textContent.includes(canary));
  await f.click('show'); f.dispose();
  assert.equal(f.host.textContent, '');
  assert.equal(f.host.listeners.size + f.windowTarget.listeners.size + f.documentTarget.listeners.size, 0);
  assert.equal(f.timers.size, 0);
});

test('wrong subject, malformed response, denial and clipboard errors never render raw diagnostics', async () => {
  for (const response of [
    async () => ({ schema: 'ctox.credential-reveal.v1', name: 'OTHER', value: canary }),
    async () => ({ name: 'TEST_LOGIN', value: canary }),
    async () => { throw Error(canary); },
  ]) {
    const f = fixture({ response }); await f.click('show');
    assert.ok(!f.host.textContent.includes(canary));
    assert.ok(f.host.textContent.includes('reveal_failed'));
    f.dispose();
  }
  const f = fixture({ clipboardError: true });
  await f.click('copy-raw');
  assert.ok(!f.host.textContent.includes(canary));
  assert.ok(f.host.textContent.includes('copy_failed'));
  f.dispose();
});

test('private method is denied in follower/proxy paths, including older-tab relay requests', () => {
  for (const options of [{ isLeader: false }, { relayed: true }]) {
    assert.throws(() => assertNativeRequestPrivacy(CREDENTIAL_REVEAL_METHOD, options), { code: 'credential_reveal_direct_tab_required' });
    assert.doesNotThrow(() => assertNativeRequestPrivacy('ctox.browser.live', options));
  }
  assert.doesNotThrow(() => assertNativeRequestPrivacy(CREDENTIAL_REVEAL_METHOD, { isLeader: true }));
});
